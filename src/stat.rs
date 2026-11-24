use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use clap::Args;
use serde::Serialize;
use serde_json::{Value, json};
use sysinfo::{Pid, Process, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::error::SightError;

#[derive(Args)]
pub struct StatArgs {
    /// Root process id to sample
    #[arg(long)]
    pub pid: u32,
    /// Sampling duration (ms)
    #[arg(long)]
    pub duration: u64,
    /// Time between samples (ms)
    #[arg(long, default_value_t = 500)]
    pub interval: u64,
    /// Sample only the root process, not its descendants
    #[arg(long)]
    pub no_tree: bool,
}

/// What `mem_mb` measures. sysinfo fills `Process::virtual_memory()` on Windows
/// from `PROCESS_MEMORY_COUNTERS_EX::PrivateUsage` (private bytes / commit),
/// unlike `memory()`, which is the working set and double-counts shared pages.
const MEM_MEASURE: &str = "private_bytes: PROCESS_MEMORY_COUNTERS_EX.PrivateUsage (sysinfo virtual_memory), summed over the tree";

/// Identity of a process incarnation: (pid, start time), since pids get reused.
type Key = (u32, u64);

#[derive(Serialize)]
struct Tracked {
    pid: u32,
    exe: String,
    start_unix_s: u64,
    exited: bool,
}

#[derive(Default)]
struct Series(Vec<f64>);

impl Series {
    fn summary(&self) -> Value {
        if self.0.is_empty() {
            return json!({"avg": 0.0, "p95": 0.0, "max": 0.0});
        }
        let mut v = self.0.clone();
        v.sort_by(f64::total_cmp);
        let avg = v.iter().sum::<f64>() / v.len() as f64;
        // Nearest-rank percentile.
        let rank = ((0.95 * v.len() as f64).ceil() as usize).clamp(1, v.len());
        json!({"avg": r2(avg), "p95": r2(v[rank - 1]), "max": r2(v[v.len() - 1])})
    }
}

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn exe_name(p: &Process) -> String {
    p.exe()
        .and_then(|e| e.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.name().to_string_lossy().into_owned())
}

/// Root plus descendants as of now. A child counts only if its parent started
/// no later than it did, which rejects reused parent PIDs.
fn scan_tree(sys: &System, root: Pid, no_tree: bool) -> Option<Vec<Pid>> {
    sys.process(root)?;
    let mut tree = vec![root];
    if no_tree {
        return Some(tree);
    }
    let mut children: HashMap<Pid, Vec<&Process>> = HashMap::new();
    for p in sys.processes().values() {
        if let Some(pp) = p.parent() {
            children.entry(pp).or_default().push(p);
        }
    }
    let mut i = 0;
    while i < tree.len() {
        let parent = tree[i];
        let pstart = sys.process(parent).map_or(0, Process::start_time);
        for c in children.get(&parent).into_iter().flatten() {
            if pstart <= c.start_time() && !tree.contains(&c.pid()) {
                tree.push(c.pid());
            }
        }
        i += 1;
    }
    Some(tree)
}

/// Refresh the process list, re-scan the tree, then refresh CPU/memory/exe for
/// just the tree. Returns the keyed members (root first), or None if the root is gone.
fn observe(sys: &mut System, root: Pid, no_tree: bool) -> Option<Vec<(Key, Pid)>> {
    let detail = ProcessRefreshKind::nothing()
        .with_cpu()
        .with_memory()
        .with_exe(UpdateKind::OnlyIfNotSet);
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let tree = scan_tree(sys, root, no_tree)?;
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&tree), true, detail);
    Some(
        tree.into_iter()
            .filter_map(|p| sys.process(p).map(|pr| ((p.as_u32(), pr.start_time()), p)))
            .collect(),
    )
}

pub fn run(args: &StatArgs) -> Result<(Value, Vec<String>), SightError> {
    if args.duration == 0 || args.interval == 0 {
        return Err(SightError::InvalidArgs(
            "--duration and --interval must be > 0".into(),
        ));
    }
    let root = Pid::from_u32(args.pid);
    let mut sys =
        System::new().map_err(|e| SightError::Internal(format!("sysinfo init failed: {e}")))?;
    let logical_cores = std::thread::available_parallelism().map_or(1, |n| n.get());

    let t0 = Instant::now();
    let Some(members) = observe(&mut sys, root, args.no_tree) else {
        return Err(SightError::TargetNotFound(format!(
            "no process with pid {}",
            args.pid
        )));
    };
    let root_key = members[0].0;
    let root_exe = sys.process(root).map(exe_name).unwrap_or_default();
    // No process handle => sysinfo reports zeros for memory and CPU time.
    let unreadable = |p: &Process| p.virtual_memory() == 0 && p.accumulated_cpu_time() == 0;
    if sys.process(root).is_some_and(unreadable) {
        return Err(SightError::Permission(format!(
            "cannot read process {}",
            args.pid
        )));
    }

    let mut warnings: Vec<String> = Vec::new();
    let mut tracked: BTreeMap<Key, Tracked> = BTreeMap::new();
    let mut last_cpu: HashMap<Key, u64> = HashMap::new();
    let mut denied: BTreeMap<u32, String> = BTreeMap::new();
    let mut lost_cpu = false;

    let mut record =
        |sys: &System, members: &[(Key, Pid)], tracked: &mut BTreeMap<Key, Tracked>| {
            for (key, pid) in members {
                let p = sys.process(*pid).expect("just observed");
                tracked.entry(*key).or_insert_with(|| Tracked {
                    pid: key.0,
                    exe: exe_name(p),
                    start_unix_s: key.1,
                    exited: false,
                });
                if *key != root_key && unreadable(p) {
                    denied.entry(key.0).or_insert_with(|| exe_name(p));
                }
            }
        };
    record(&sys, &members, &mut tracked);
    // Baseline: CPU burned before we started watching is not ours to count.
    for (key, pid) in &members {
        last_cpu.insert(
            *key,
            sys.process(*pid).map_or(0, Process::accumulated_cpu_time),
        );
    }

    let (mut cpu_core, mut cpu_total, mut mem) =
        (Series::default(), Series::default(), Series::default());
    let end = t0 + Duration::from_millis(args.duration);
    let step = Duration::from_millis(args.interval);
    let mut next = t0;
    let mut prev_t = Instant::now();
    let mut target_exited = false;
    let mut exited_keys: Vec<Key> = Vec::new();

    loop {
        next = (next + step).min(end);
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
        let now = Instant::now();
        let members = match observe(&mut sys, root, args.no_tree) {
            // A different start time on the root pid means the pid was reused.
            Some(m) if m[0].0 == root_key => m,
            _ => {
                target_exited = true;
                break;
            }
        };
        record(&sys, &members, &mut tracked);

        let live: Vec<Key> = members.iter().map(|m| m.0).collect();
        let mut delta_ms = 0u64;
        let mut mem_bytes = 0u64;
        for (key, pid) in &members {
            let p = sys.process(*pid).expect("just observed");
            let acc = p.accumulated_cpu_time();
            // A process first seen now was spawned during the run: all its CPU counts.
            delta_ms += acc.saturating_sub(last_cpu.insert(*key, acc).unwrap_or(0));
            mem_bytes += p.virtual_memory();
        }
        for key in tracked.keys() {
            if !live.contains(key) && !exited_keys.contains(key) {
                exited_keys.push(*key);
                lost_cpu = true;
            }
        }
        let dt_ms = now.duration_since(prev_t).as_secs_f64() * 1000.0;
        prev_t = now;
        let core_pct = delta_ms as f64 / dt_ms * 100.0;
        cpu_core.0.push(core_pct);
        cpu_total.0.push(core_pct / logical_cores as f64);
        mem.0.push(mem_bytes as f64 / (1024.0 * 1024.0));
        if now >= end {
            break;
        }
    }
    let duration_ms = t0.elapsed().as_millis() as u64;
    if target_exited {
        // The root and everything we were tracking is gone or unreachable.
        warnings.push("target_exited".into());
        for key in tracked.keys() {
            if !exited_keys.contains(key) {
                // Still alive children survive their parent; only mark the ones we can't see.
                let alive = sys
                    .process(Pid::from_u32(key.0))
                    .is_some_and(|p| p.start_time() == key.1);
                if !alive || *key == root_key {
                    exited_keys.push(*key);
                }
            }
        }
        lost_cpu = true;
    }
    if lost_cpu {
        // Each process in `processes` carries its own `exited` flag.
        warnings.push("processes_exited_during_sampling".into());
    }
    if !denied.is_empty() {
        warnings.push("access_denied_for_some_processes".into());
    }
    let processes: Vec<Tracked> = tracked
        .into_iter()
        .map(|(k, mut t)| {
            t.exited = exited_keys.contains(&k);
            t
        })
        .collect();

    let data = json!({
        "pid": args.pid,
        "exe": root_exe,
        "duration_ms": duration_ms,
        "samples": cpu_core.0.len(),
        "logical_cores": logical_cores,
        "cpu_core_pct": cpu_core.summary(),
        "cpu_total_pct": cpu_total.summary(),
        "mem_mb": mem.summary(),
        "mem_measure": MEM_MEASURE,
        "processes": processes,
    });
    let mut data = data;
    if !denied.is_empty() {
        data["access_denied"] = denied
            .iter()
            .map(|(p, e)| json!({"pid": p, "exe": e}))
            .collect();
    }
    Ok((data, warnings))
}

#[cfg(test)]
mod tests {
    use super::r2;

    #[test]
    fn r2_rounds_to_two_decimals() {
        assert_eq!(r2(1.005_1), 1.01);
        assert_eq!(r2(52.874), 52.87);
        assert_eq!(r2(100.0), 100.0);
    }
}
