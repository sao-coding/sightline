mod baseline;
mod capture;
mod error;
mod list;
mod output;
mod rec;
mod shot;
mod stat;
mod wait;
mod win;

use clap::{Parser, Subcommand, error::ErrorKind};
use error::SightError;
use serde_json::{Value, json};

#[derive(Parser)]
#[command(name = "sight", version, about = "Observe and measure apps on Windows")]
struct Cli {
    /// Pretty-print JSON output
    #[arg(long, global = true)]
    pretty: bool,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List capturable windows
    List(list::ListArgs),
    /// Take a screenshot
    Shot(shot::ShotArgs),
    /// Wait for the UI to change and/or settle, without saving an image
    Wait(wait::WaitArgs),
    /// Record a window or region to an animated GIF
    Rec(rec::RecArgs),
    /// Sample process stats
    Stat(stat::StatArgs),
    /// Check the environment
    Doctor,
}

fn doctor() -> Result<Value, SightError> {
    let monitors: Vec<Value> = win::monitors()
        .into_iter()
        .map(|m| json!({"index": m.index, "rect": m.rect, "dpi_scale": m.dpi_scale, "primary": m.primary}))
        .collect();
    if monitors.is_empty() {
        return Err(SightError::Internal("no monitors found".into()));
    }
    Ok(json!({ "monitors": monitors }))
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        output::failure(&SightError::Internal(format!("panic: {info}")), false);
        std::process::exit(6);
    }));
    win::init_dpi_awareness();
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) => {
            e.exit()
        }
        Err(e) => {
            let pretty = std::env::args().any(|a| a == "--pretty");
            output::failure(
                &SightError::InvalidArgs(e.to_string().trim().to_string()),
                pretty,
            );
            std::process::exit(1);
        }
    };
    let result = match &cli.command {
        Cmd::List(a) => list::run(a).map_err(Into::into),
        Cmd::Shot(a) => shot::run(a),
        Cmd::Wait(a) => wait::run(a),
        Cmd::Rec(a) => rec::run(a).map_err(Into::into),
        Cmd::Stat(a) => stat::run(a).map_err(Into::into),
        Cmd::Doctor => doctor().map(|d| (d, Vec::new())).map_err(Into::into),
    };
    match result {
        Ok((data, warnings)) => output::success(&data, &warnings, cli.pretty),
        Err(f) => {
            output::failure_with(&f.err, f.data.as_ref(), &f.warnings, cli.pretty);
            std::process::exit(f.err.exit_code());
        }
    }
}
