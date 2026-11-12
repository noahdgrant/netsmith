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
#[command(
    after_long_help = "`dxf import` reads the board outline of an enclosure from an ASCII DXF file and prints, to paste into the `case { }` section of the board's physical file, the `import` lines, an `.outline: Outline`, a `Cutout` with its `.x` and `.y` for each loop inside it, and a `MountingHole` for each circle on the holes layer with the requirement on the centre of its body (`.hole1.place.x &= 5mm`). It writes nothing without `--write`.\n\nThe reader takes `LINE`, `ARC`, `CIRCLE`, `LWPOLYLINE` (closed flag, bulges), `POLYLINE` with `VERTEX`, `SPLINE` and `ELLIPSE` (as chords within 0.005 mm), and `INSERT` of blocks (translate, rotate, scale, mirror); text, dimensions and the like are not read and are counted in a warning. Binary DXF is refused. Units come from `$INSUNITS` (millimetre, centimetre, metre, inch); a file without them needs `--units`. Edges whose ends are within 0.001 mm join into closed loops; a gap is listed with its size and place.\n\nThe one loop that is not inside another is the outline and the loops inside it are cutouts; two outer loops, a loop that crosses itself, a loop that crosses a cutout or only touches the outline, or a loop inside a cutout is an error. A loop that crosses the outline (a window in a case wall that is partly beyond the board) is an opening: the import leaves it out and prints a note with its layer, size and place on the board, and imports the rest (provisional, issue #90). A circle on the outline layer is a round `Cutout`; on the holes layer it is a hole, which must be a `MountingHole` package, `M2` (2.2 mm) or `M3` (3.2 mm); a hole of another diameter is refused with the nearest package and the difference (issue #89). The board's frame is the outline's top left corner with y down, and the output says which point of the drawing that is. The outline must be a rectangle with equal rounded corners and each cutout a rounded rectangle, a slot or a circle: any other shape is refused with its size and edge count (ADR 0045 step C8)."
)]
#[command(args_override_self = true)]
pub struct ImportArgs {
    /// The enclosure's ASCII DXF file.
    #[arg(value_name = "FILE.DXF")]
    pub file: PathBuf,
    /// The layer of the board edge (repeatable); with none, the one layer that has lines.
    ///
    /// With neither `--layer` nor `--holes-layer`, the one layer that has lines or arcs is used,
    /// and if there are several the error lists them with their entity counts. A layer that the
    /// file's layer table declares is accepted when nothing is drawn on it; a name that is nowhere
    /// in the file is an error that lists the layers the file has.
    #[arg(long = "layer", value_name = "NAME")]
    pub layers: Vec<String>,
    /// The layer of the mounting holes (repeatable).
    ///
    /// A case with no holes can still pass it, when the layer table declares the layer.
    #[arg(long = "holes-layer", value_name = "NAME")]
    pub holes_layers: Vec<String>,
    /// The drawing's unit, for a file without `$INSUNITS`: mm, cm, m or inch.
    #[arg(long, value_name = "UNIT", value_parser = parse_units)]
    pub units: Option<netsmith_dxf::Units>,
    /// The board, when the file `--write` names has several.
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// Put the import into the physical file beside this source file, and print the diff (ADR 0048).
    ///
    /// It edits `<stem>.physical.nsm` beside that file: it makes the file, the unit or the `case`
    /// section if they are not there, replaces the members of an earlier import (`outline`,
    /// `cutout1`, `hole1`...) and the lines that state their places, keeps every other line and
    /// every comment, and prints a diff. The circuit source and the layout file are never written.
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
