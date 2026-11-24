use std::time::Instant;

use clap::Args;
use serde_json::{Value, json};

use crate::error::SightError;
use crate::shot::{self, CaptureArgs, ShotFail};

#[derive(Args)]
pub struct WaitArgs {
    #[command(flatten)]
    pub cap: CaptureArgs,
    /// Take the first frame as a baseline and wait until pixels differ from it
    /// (then apply --settle). Use right after an external click or keypress
    #[arg(long)]
    pub change: bool,
}

/// Same waiting logic as `shot`, but nothing is saved: tells an agent when the
/// UI is ready for its next action.
pub fn run(args: &WaitArgs) -> Result<(Value, Vec<String>), Box<ShotFail>> {
    let start = Instant::now();
    let mut warnings = Vec::new();
    let cap = shot::acquire(&args.cap, args.change, start, &mut warnings)?;
    let data = json!({
        "changed": cap.changed,
        "change_ms": cap.change_ms,
        "settled": cap.settled,
        "elapsed_ms": start.elapsed().as_millis() as u64,
    });
    if cap.settled {
        Ok((data, warnings))
    } else {
        Err(Box::new(ShotFail {
            err: SightError::Timeout(shot::timeout_message(
                args.change,
                cap.changed,
                args.cap.timeout,
                "no image saved",
            )),
            data: Some(data),
            warnings,
        }))
    }
}
