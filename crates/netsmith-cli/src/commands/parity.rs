use netsmith::project::Target;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith::findings::Format;

use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "For a port: the place, rotation, face and pads of each part the table names. The exit code is 1 when a part the table marks `must_match` fails, 0 when none does, and 2 when a file cannot be read."
)]
#[command(args_override_self = true)]
pub struct Args {
    /// The board `fab` wrote.
    #[arg(value_name = "BOARD.KICAD_PCB")]
    pub board: PathBuf,
    /// The table of the original's parts (default `reference/placements.toml`).
    #[arg(long, value_name = "PLACEMENTS.TOML")]
    pub table: Option<PathBuf>,
    /// The original's designators to the fab board's (default `reference/designators.toml`).
    #[arg(long, value_name = "DESIGNATORS.TOML")]
    pub map: Option<PathBuf>,
    /// Write an overlay of the two boards.
    #[arg(long, value_name = "OVERLAY.SVG")]
    pub svg: Option<PathBuf>,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

/// Compares the board part by part with the original's.
pub fn run(args: Args) -> ExitCode {
    run_parity(
        &args.board,
        args.table.as_deref(),
        args.map.as_deref(),
        args.svg.as_deref(),
        args.format.into(),
    )
}

/// `netsmith parity`: compare a board that `fab` wrote with the board of a port's original
/// (#455; it was the example `parity`). The table and the map default to those of the project's
/// `reference/` folder, or of the current folder's when there is no project. The report goes to
/// stdout; the exit code is 1 when a must-match item fails, 0 when none does, 2 when a file
/// cannot be read.
fn run_parity(
    board: &Path,
    table: Option<&Path>,
    map: Option<&Path>,
    svg: Option<&Path>,
    format: Format,
) -> ExitCode {
    use netsmith::parity::{DesignatorMap, Tolerances, check_text};
    let reference = Target::resolve(None)
        .ok()
        .and_then(|t| t.project.map(|p| p.root))
        .unwrap_or_else(|| Path::new(".").to_path_buf())
        .join("reference");
    let read =
        |path: &Path| fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()));
    let run = || -> Result<netsmith::parity::ParityReport, String> {
        let table_path = table.map_or_else(|| reference.join("placements.toml"), Path::to_path_buf);
        let map_path = map.map_or_else(|| reference.join("designators.toml"), Path::to_path_buf);
        let map = DesignatorMap::parse(&read(&map_path)?)
            .map_err(|e| format!("{}: {e}", map_path.display()))?;
        check_text(
            &read(&table_path)?,
            &read(board)?,
            &map,
            &Tolerances::default(),
        )
    };
    let report = match run() {
        Ok(report) => report,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    match format {
        Format::Json => print!("{}", report.json()),
        Format::Text => print!("{}", report.text()),
    }
    if let Some(svg) = svg
        && let Err(e) = fs::write(svg, report.overlay_svg())
    {
        eprintln!("error: {}: {e}", svg.display());
        return ExitCode::from(2);
    }
    if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
