use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args as ClapArgs, ValueEnum};
use netsmith_fabricate::schematic::{self, Inputs};
use netsmith_pipeline::Built;

use super::common::{libraries, no_boards, target};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "`view schematic` draws the schematic the editor shows (the schematic layout file beside the source, `<stem>.sch.toml`, is honoured) and writes it with netsmith's own writers, with no KiCad and no browser: the same source gives the same bytes, and the bytes the editor's export and `board fab` write. A PDF has a page for each sheet (a module's box of a board of more than 16 parts; one sheet for a smaller board), each page sized to its sheet, with the sheet's name, the board's, the project's and the release of netsmith in its title block; its text can be searched. A system's PDF has the system's own sheet (its boards and their joins), then each member's sheets. The PNG is drawn by netsmith's own rasterizer at 10 pixels a millimetre (fewer for a drawing more than 8192 pixels long); the SVG is the editor's. The path written is printed."
)]
pub struct Args {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The board, when the file has several (`Name`, or `System/member`), or a system (`System`).
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// What to write: a PDF with a page for each sheet, the SVG, or a PNG.
    #[arg(long, value_enum, default_value = "pdf")]
    pub format: SchematicFormat,
    /// Only this sheet (a module's box, as its title names it: `power`, `stage (2/3)`).
    ///
    /// A board of 16 parts or fewer is one sheet, named for the board. A name the board has no
    /// sheet of exits 2 with the list of its sheets.
    #[arg(long, value_name = "NAME")]
    pub sheet: Option<String>,
    /// The file to write (default `<board>-schematic.<pdf|svg|png>`, with the sheet's name after
    /// `schematic` when `--sheet` is given).
    #[arg(long, value_name = "FILE")]
    pub picture: Option<PathBuf>,
}

/// `view schematic --format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SchematicFormat {
    Pdf,
    Svg,
    Png,
}

impl SchematicFormat {
    fn export(self) -> schematic::Format {
        match self {
            SchematicFormat::Pdf => schematic::Format::Pdf,
            SchematicFormat::Svg => schematic::Format::Svg,
            SchematicFormat::Png => schematic::Format::Png,
        }
    }
}

/// What `--board` picked: a board, or a system and its members.
enum Picked<'a> {
    Board(&'a Built),
    System(&'a netsmith_pipeline::SolvedSystem, Vec<&'a Built>),
}

/// Draws one board's (or system's) schematic and writes it as a PDF, an SVG or a PNG. Exit 0
/// when it is written, 1 when the board does not build, 2 when the command cannot start, the
/// board or the sheet is not there, or the file cannot be written.
pub fn run(args: Args) -> ExitCode {
    let (target, loader, catalogs) = match target(args.file.as_deref(), false) {
        Ok(t) => t,
        Err(code) => return code,
    };
    if no_boards(&target) {
        return ExitCode::from(2);
    }
    let libraries = match libraries(&target) {
        Ok(l) => l,
        Err(code) => return code,
    };
    let project = target.project.as_ref().map(|p| p.manifest.name.clone());
    let mut found: Vec<String> = Vec::new();
    for source in &target.files {
        let text = match fs::read_to_string(&source.path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{}: {e}", source.name);
                return ExitCode::from(2);
            }
        };
        // The pick lock holds the parts it names to their rows, as for `fab`; it is not written.
        let held = match netsmith_pipeline::pick_lock::open(
            &source.path,
            &catalogs,
            false,
            target.locks(source),
        ) {
            Ok((held, _)) => held,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
        };
        // The drawing needs no KiCad schematic: the libraries are read for the symbols only,
        // as the editor reads them.
        let outcome = match netsmith_pipeline::outcome_with(
            &source.name,
            &text,
            &loader,
            held.as_ref().unwrap_or(&catalogs),
            None,
        ) {
            Ok(o) => o,
            Err(problems) => {
                eprint!("{}", netsmith_pipeline::render_problems(&problems));
                return ExitCode::FAILURE;
            }
        };
        let names: Vec<String> = outcome
            .boards
            .iter()
            .map(Built::label)
            .chain(outcome.systems.iter().map(|s| s.name.clone()))
            .collect();
        let picked = match args.board.as_deref() {
            Some(name) => pick(&outcome, name),
            None if names.len() == 1 => pick(&outcome, &names[0]),
            None if target.files.len() == 1 => {
                eprintln!(
                    "error: {} has {} boards and systems: {}; name one with `--board <name>`",
                    source.name,
                    names.len(),
                    names.join(", ")
                );
                return ExitCode::from(2);
            }
            None => None,
        };
        found.extend(names);
        let Some(picked) = picked else { continue };
        let inputs = Inputs::of(&source.path, project.as_deref());
        return write(&args, &picked, &libraries, &inputs);
    }
    match args.board.as_deref() {
        Some(name) => eprintln!(
            "error: no board or system `{name}`; there are: {}",
            found.join(", ")
        ),
        None => eprintln!(
            "error: the project has {} top-level files; name one, or a board with `--board <name>`",
            target.files.len()
        ),
    }
    ExitCode::from(2)
}

fn pick<'a>(outcome: &'a netsmith_pipeline::Outcome, name: &str) -> Option<Picked<'a>> {
    if let Some(system) = outcome.systems.iter().find(|s| s.name == name) {
        let members = netsmith_fabricate::system3d::members(system, &outcome.boards);
        return Some(Picked::System(system, members));
    }
    outcome
        .boards
        .iter()
        .find(|b| b.label() == name)
        .or_else(|| {
            // A board of a system by its own name, when only one board has it.
            let mut by_name = outcome.boards.iter().filter(|b| b.board == name);
            match (by_name.next(), by_name.next()) {
                (Some(b), None) => Some(b),
                _ => None,
            }
        })
        .map(Picked::Board)
}

fn write(
    args: &Args,
    picked: &Picked,
    libraries: &netsmith_pipeline::Libraries,
    inputs: &Inputs,
) -> ExitCode {
    let format = args.format.export();
    let (hints, why) = schematic::read_hints_file(&inputs.layout);
    if let Some(why) = why {
        eprintln!("warning: {why}");
    }
    let draw = |built: &Built| -> Result<String, String> {
        let (svg, findings) = schematic::draw(built, libraries, hints.as_ref())
            .map_err(|why| format!("the schematic of {} cannot be drawn: {why}", built.label()))?;
        for f in &findings {
            eprintln!("warning: {}", schematic::finding_line(&inputs.layout, f));
        }
        Ok(svg)
    };
    let (label, svgs) = match picked {
        Picked::Board(built) => (built.label(), draw(built).map(|s| vec![s])),
        Picked::System(system, members) => {
            let mut svgs = vec![schematic::system_svg(system)];
            // A PDF has every member's sheets after the system's own; a picture is the
            // system's sheet.
            let all = if format == schematic::Format::Pdf {
                members
                    .iter()
                    .map(|b| draw(b))
                    .collect::<Result<Vec<_>, _>>()
            } else {
                Ok(Vec::new())
            };
            (
                system.name.clone(),
                all.map(|more| {
                    svgs.extend(more);
                    svgs
                }),
            )
        }
    };
    let svgs = match svgs {
        Ok(s) => s,
        Err(why) => {
            eprintln!("error: {why}");
            return ExitCode::FAILURE;
        }
    };
    let sheet = args.sheet.as_deref();
    let bytes = match format {
        schematic::Format::Pdf => {
            let all: Vec<&str> = svgs.iter().map(String::as_str).collect();
            let chosen: Result<Vec<&str>, _> = match sheet {
                None => Ok(all),
                // One sheet: the drawing that has it, cut to it.
                Some(name) => one_sheet(&all, name),
            };
            chosen.and_then(|svgs| match sheet {
                None => schematic::export::pdf(
                    &format!("{label} schematic"),
                    &svgs,
                    &schematic::heading(inputs.project.as_deref()),
                ),
                Some(name) => schematic::export::pdf_sheet(
                    &format!("{label} schematic"),
                    svgs[0],
                    name,
                    &schematic::heading(inputs.project.as_deref()),
                ),
            })
        }
        schematic::Format::Svg => {
            schematic::export::svg_of(&svgs[0], sheet).map(String::into_bytes)
        }
        schematic::Format::Png => schematic::export::png(&svgs[0], sheet),
    };
    let bytes = match bytes {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    let out = args
        .picture
        .clone()
        .unwrap_or_else(|| default_out(&label, sheet, format));
    if let Err(e) = fs::write(&out, bytes) {
        eprintln!("error: {}: {e}", out.display());
        return ExitCode::from(2);
    }
    println!("{}", out.display());
    ExitCode::SUCCESS
}

/// The drawing of `svgs` that has the sheet `name`, or the error that lists every sheet.
fn one_sheet<'a>(
    svgs: &[&'a str],
    name: &str,
) -> Result<Vec<&'a str>, schematic::export::ExportError> {
    let mut sheets = Vec::new();
    for svg in svgs {
        let names = schematic::export::sheets(svg)?;
        if names.iter().any(|s| s.name == name) {
            return Ok(vec![svg]);
        }
        sheets.extend(names.into_iter().map(|s| s.name));
    }
    Err(schematic::export::ExportError::NoSheet {
        asked: name.to_string(),
        sheets,
    })
}

/// `<board>-schematic.<ext>` in the current folder, or `<board>-schematic-<sheet>.<ext>`, with
/// what a file name cannot hold as `-`.
fn default_out(label: &str, sheet: Option<&str>, format: schematic::Format) -> PathBuf {
    let clean = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                    c
                } else {
                    '-'
                }
            })
            .collect()
    };
    let stem = match sheet {
        Some(s) => format!("{}-schematic-{}", clean(label), clean(s)),
        None => format!("{}-schematic", clean(label)),
    };
    Path::new(&format!("{stem}.{}", format.extension())).to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::common::testing::{Leaf, parse};

    #[test]
    fn view_schematic_takes_a_format_a_sheet_and_a_picture() {
        let Ok(Leaf::Schematic(a)) = parse(&["view", "schematic"]) else {
            panic!("view schematic should parse");
        };
        assert_eq!(a.format, SchematicFormat::Pdf);
        assert_eq!((a.sheet, a.picture, a.board), (None, None, None));
        let Ok(Leaf::Schematic(a)) = parse(&[
            "view",
            "schematic",
            "a.nsm",
            "--board",
            "B",
            "--format",
            "png",
            "--sheet",
            "power",
            "--picture",
            "x.png",
        ]) else {
            panic!("view schematic should parse its flags");
        };
        assert_eq!(a.file.as_deref(), Some("a.nsm"));
        assert_eq!(a.format, SchematicFormat::Png);
        assert_eq!(a.sheet.as_deref(), Some("power"));
        assert_eq!(a.picture, Some(PathBuf::from("x.png")));
        assert!(parse(&["view", "schematic", "--format", "jpeg"]).is_err());
        // One file is `--picture`, as `view 3d` names it; `--out` is a folder elsewhere.
        assert!(parse(&["view", "schematic", "--out", "x.pdf"]).is_err());
    }

    #[test]
    fn the_default_file_is_named_for_the_board_and_the_sheet() {
        assert_eq!(
            default_out("Sys/led", None, schematic::Format::Pdf),
            PathBuf::from("Sys-led-schematic.pdf")
        );
        assert_eq!(
            default_out("Led", Some("stage (2/3)"), schematic::Format::Png),
            PathBuf::from("Led-schematic-stage--2-3-.png")
        );
    }
}
