use netsmith::project::Target;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith::findings::Format;

use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`parity` compares a board that `fab` wrote with the original of a port (see the skill `netsmith-port-a-kicad-board`), part by part: the place, the rotation, the face and the pads of each part the table names.\n\nThe exit code is 1 when a part the table marks `must_match` fails, 0 when none does and 2 when a file cannot be read."
)]
pub struct Args {
    /// The board `fab` wrote.
    #[arg(value_name = "BOARD.KICAD_PCB")]
    pub board: PathBuf,
    /// The table of the original's parts (default `reference/placements.toml`).
    #[arg(long, value_name = "PLACEMENTS.TOML")]
    pub table: Option<PathBuf>,
    /// The original's designator of each member path (default `reference/parts.toml`).
    ///
    /// Each part is paired with the footprint of the fab board that holds its member path
    /// (`netsmith_path`, #456).
    #[arg(long, value_name = "PARTS.TOML")]
    pub parts: Option<PathBuf>,
    /// Write an overlay of the two boards.
    #[arg(long, value_name = "OVERLAY.SVG")]
    pub svg: Option<PathBuf>,
    /// Text for a person, or one JSON document on stdout.
    ///
    /// `json` holds each item with its figures.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

/// Compares the board part by part with the original's.
pub fn run(args: Args) -> ExitCode {
    run_parity(
        &args.board,
        args.table.as_deref(),
        args.parts.as_deref(),
        args.svg.as_deref(),
        args.format.into(),
    )
}

/// `netsmith parity`: compare a board that `fab` wrote with the board of a port's original
/// (#455; it was the example `parity`). The table and the parts default to those of the project's
/// `reference/` folder, or of the current folder's when there is no project. The report goes to
/// stdout; the exit code is 1 when a must-match item fails, 0 when none does, 2 when a file
/// cannot be read.
fn run_parity(
    board: &Path,
    table: Option<&Path>,
    parts: Option<&Path>,
    svg: Option<&Path>,
    format: Format,
) -> ExitCode {
    use netsmith::parity::{DesignatorMap, Tolerances, check_text};
    use netsmith::port::PortParts;
    let reference = Target::resolve(None)
        .ok()
        .and_then(|t| t.project.map(|p| p.root))
        .unwrap_or_else(|| Path::new(".").to_path_buf())
        .join("reference");
    let read =
        |path: &Path| fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()));
    let run = || -> Result<netsmith::parity::ParityReport, String> {
        let table_path = table.map_or_else(|| reference.join("placements.toml"), Path::to_path_buf);
        let parts_path = parts.map_or_else(|| reference.join("parts.toml"), Path::to_path_buf);
        let parts = PortParts::read(&parts_path)?;
        let pcb = read(board)?;
        let map = DesignatorMap::from_parts(&parts, &pcb)
            .map_err(|e| format!("{}: {e}", board.display()))?;
        check_text(&read(&table_path)?, &pcb, &map, &Tolerances::default())
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

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{kind, parse};
    use crate::commands::{Command, common, parity};
    use clap::error::ErrorKind;

    #[test]
    fn parity_takes_the_board_and_its_files() {
        assert!(matches!(
            parse(&["parity", "b.kicad_pcb", "--format", "json"]),
            Ok(Command::Parity(parity::Args {
                table: None,
                parts: None,
                svg: None,
                format: common::Format::Json,
                ..
            }))
        ));
        assert_eq!(kind(&["parity"]), ErrorKind::MissingRequiredArgument);
        assert!(matches!(
            parse(&["parity", "b.kicad_pcb", "--parts", "p.toml"]),
            Ok(Command::Parity(parity::Args { parts: Some(_), .. }))
        ));
        // `--map` (the designators file) is gone with the file (#456).
        assert_eq!(
            kind(&["parity", "b.kicad_pcb", "--map", "d.toml"]),
            ErrorKind::UnknownArgument
        );
    }
}
