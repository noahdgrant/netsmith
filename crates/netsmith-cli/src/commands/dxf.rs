use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args as ClapArgs, Subcommand};

#[derive(Debug, PartialEq, ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum Command {
    /// Print an enclosure's outline, cutouts and mounting holes from an ASCII DXF as source to paste (ADR 0045).
    Import(ImportArgs),
}

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
pub struct ImportArgs {
    /// The enclosure's ASCII DXF file.
    #[arg(value_name = "FILE.DXF")]
    pub file: PathBuf,
    /// The layer of the board edge (repeatable); with none, the one layer that has lines.
    #[arg(long = "layer", value_name = "NAME")]
    pub layers: Vec<String>,
    /// The layer of the mounting holes (repeatable).
    #[arg(long = "holes-layer", value_name = "NAME")]
    pub holes_layers: Vec<String>,
    /// The drawing's unit, for a file without `$INSUNITS`: mm, cm, m or inch.
    #[arg(long, value_name = "UNIT", value_parser = parse_units)]
    pub units: Option<netsmith_dxf::Units>,
    /// The board, when the file `--write` names has several.
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// Put the import into the physical file beside this source file, and print the diff (ADR 0048).
    #[arg(long, value_name = "FILE.NSM")]
    pub write: Option<PathBuf>,
}

pub fn run(args: Args) -> ExitCode {
    match args.command {
        Command::Import(a) => run_dxf_import(
            &a.file,
            &netsmith::dxf::Request {
                layers: a.layers,
                holes_layers: a.holes_layers,
                units: a.units,
            },
            a.board.as_deref(),
            a.write.as_deref(),
        ),
    }
}

/// `netsmith dxf import`: the source to paste goes to stdout, the report and warnings to
/// stderr. With `--write`, the places go into the layout file and the diff is printed.
fn run_dxf_import(
    file: &Path,
    request: &netsmith::dxf::Request,
    board: Option<&str>,
    write: Option<&Path>,
) -> ExitCode {
    use netsmith::dxf;
    let fail = |e: String| {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    };
    let bytes = match fs::read(file) {
        Ok(b) => b,
        Err(e) => return fail(format!("{}: {e}", file.display())),
    };
    // Old files are often in a Windows code page: the bytes that are not UTF-8 are only in
    // text that is not read.
    let text = String::from_utf8_lossy(&bytes);
    let name = file.display().to_string();
    let imported = match dxf::import(&text, &name, request) {
        Ok(i) => i,
        Err(e) => return fail(e),
    };
    for w in &imported.warnings {
        eprintln!("warning: {w}");
    }
    let mut written = None;
    if let Some(source) = write {
        match dxf::write_physical(source, board, &imported) {
            Ok((b, diff)) => written = Some((b, diff)),
            Err(e) => return fail(e),
        }
    }
    let board_name = written.as_ref().map(|(b, _)| b.as_str()).or(board);
    print!(
        "{}",
        dxf::render(&imported, board_name, &name, written.is_some())
    );
    if let Some((_, diff)) = written {
        println!("\n// physical file written; what changed:");
        print!("{diff}");
    }
    ExitCode::SUCCESS
}

fn parse_units(given: &str) -> Result<netsmith_dxf::Units, String> {
    netsmith_dxf::Units::parse(given)
        .ok_or_else(|| format!("needs mm, cm, m or inch, not `{given}`"))
}
