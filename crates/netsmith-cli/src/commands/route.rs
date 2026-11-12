use netsmith::fab::FabOptions;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::Args as ClapArgs;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "A router tool: no checks, no fab package. The exit code is 0 when every connection is routed, 1 when connections are left, and 2 when the file cannot be read (a file saved by another version of netsmith is refused: save it again with `fab --save-placed`)."
)]
#[command(args_override_self = true)]
pub struct Args {
    /// The placed board that `fab --save-placed` wrote.
    #[arg(value_name = "BOARD.PLACED.JSON")]
    pub file: PathBuf,
    /// The folder to write the board files into (default `out/`).
    #[arg(long, value_name = "DIR")]
    pub out: Option<PathBuf>,
    /// Seconds the router may take; 0 is no limit.
    #[arg(long, value_name = "SECS")]
    pub route_timeout: Option<u64>,
    /// The track margin, from 1 (the process's least track) to 4, over the file's.
    #[arg(long, value_name = "X", value_parser = parse_margin)]
    pub margin: Option<f64>,
}

/// Routes the saved board again, alone, the way `fab` does, and writes its board files.
pub fn run(args: Args) -> ExitCode {
    run_route(
        &args.file,
        args.out.as_deref(),
        args.route_timeout,
        args.margin,
    )
}

/// `netsmith route`: route the placed board that `fab --save-placed` wrote, alone, and write
/// its board files into `out` (`out/` when `None`). Exits 1 when connections are left, 2 when
/// the file cannot be read or the files cannot be written.
fn run_route(
    file: &Path,
    out: Option<&Path>,
    route_timeout: Option<u64>,
    margin: Option<f64>,
) -> ExitCode {
    let placed = match fs::read_to_string(file)
        .map_err(|e| e.to_string())
        .and_then(|text| netsmith::placed::from_json(&text))
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}: {e}", file.display());
            return ExitCode::from(2);
        }
    };
    let request = netsmith::placed::RouteRequest {
        route_timeout: match route_timeout {
            Some(0) => None,
            Some(secs) => Some(Duration::from_secs(secs)),
            None => FabOptions::default().route_timeout,
        },
        margin,
        progress: Some(|message| eprintln!("{message}")),
    };
    let out = out.unwrap_or(Path::new("out"));
    let routed = match netsmith::placed::route(placed, out, &request, &|m| eprintln!("{m}")) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    for path in &routed.written {
        println!("wrote {}", path.display());
    }
    for line in &routed.left {
        println!("not routed: {line}");
    }
    println!(
        "{}: {} connections left, {} tracks, {} vias, routed in {:.1} s ({} tries{})",
        routed.board.name,
        routed.left.len(),
        routed.board.tracks.len(),
        routed.board.vias.len(),
        routed.elapsed.as_secs_f64(),
        routed.tries,
        if routed.stopped {
            ", stopped at the route timeout"
        } else {
            ""
        }
    );
    if routed.left.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn parse_margin(given: &str) -> Result<f64, String> {
    given
        .parse::<f64>()
        .ok()
        .filter(|m| (1.0..=4.0).contains(m))
        .ok_or_else(|| {
            format!("takes a number from 1 (the process's least track) to 4, not `{given}`")
        })
}
