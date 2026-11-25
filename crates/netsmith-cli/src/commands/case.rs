use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args as ClapArgs, Subcommand};

#[derive(Debug, PartialEq, ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum Command {
    /// Print a case model's `.rx`, `.ry`, `.rz` and `.x`, `.y`, `.z` for a pose given as a matrix, and check them.
    Place(PlaceArgs),
    /// Print the outline, cutouts and mounting holes of a case's STEP model as source to paste (ADR 0109).
    Model(ModelArgs),
}

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "`import case place` turns a pose given as a matrix (a STEP assembly's transform, a CAD program's matrix) into the lines of a `CaseModel` (`std::pcb::CaseModel`) that pose the model the same way, to paste into the case model's block in the `case { }` section of the board's physical file: `.rx`, `.ry` and `.rz` (degrees, about x, then y, then z of the 3D scene's frame) and `.x`, `.y` and `.z` (mm).\n\nThe matrix is 3 x 4, row major: each row is three numbers of the rotation and then the offset in mm, and it acts on (x, y, z, 1) of the model's file. Sixteen numbers are a 4 x 4 matrix whose last row must be 0 0 0 1. The numbers may be separated by spaces, commas, brackets or parentheses, in one argument or many, so a matrix printed as `[[1, 0, 0, 0], [0, -1, 0, 0], ...]` can be pasted in quotes.\n\nA rotation has two sets of angles in general: the one printed has `.ry` from -90 to 90, and `.rz` 0 when `.ry` is a quarter turn. The command checks its angles by turning them back into a matrix, and says on stderr by how much that matrix differs from the one given. A matrix that is not a rotation is refused (exit 1): a row whose length is not 1 (a scale, as a model in inches would need, or a shear), two rows that are not square to one another (a shear), or a mirror (a determinant of -1, which no angles draw), each within 0.0001 so that a matrix typed with four decimals is read."
)]
#[command(args_override_self = true)]
pub struct PlaceArgs {
    /// The matrix's 12 numbers (3 x 4, row major, the offset in mm last in each row), or 16 for a 4 x 4 one.
    #[arg(value_name = "NUMBER", required = true, num_args = 1.., allow_hyphen_values = true)]
    pub matrix: Vec<String>,
}

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "`import case model` cuts the STEP model of a case by a plane parallel to the board and prints what the cut says of the board, to paste into the `case { }` section of the board's physical file, as `import dxf` prints a drawing's (the output is reviewed, and never derived at build time, ADR 0109). It writes nothing without `--write`.\n\nThe model is posed as a `CaseModel` poses it: `--rx`, `--ry`, `--rz` (degrees, about x, then y, then z) and then `--x`, `--y`, `--z` (mm); `import case place` gives them for a matrix. The plane is at `--at` mm in that frame, where the board's top face is z = 0 and its bottom face z = -1.6 (the default, -0.8, is the middle of a 1.6 mm board). The cut draws closed curves on the model's solids, and how many of the others enclose each is its depth:\n\n- depth 0, the outside of a wall: not used;\n- depth 1, a pocket in the wall: the board's outline (the largest pocket, if there are several; the others are said and left out). A rib, a notch or a rounded corner of the wall is the outline's, so it is printed as `import dxf` prints one: `.width`, `.height` and `.corner_radius` for a rectangle with equal rounded corners, and any other as a `polygon` of the corners, each arc `arc through` its middle point;\n- depth 2, inside the pocket: a post, a pin or a block standing where the board is, a `Cutout` (a rounded rectangle, a slot or a circle; another shape is refused, as `import dxf` refuses it);\n- deeper: not used.\n\nWith `--holes-at` (a standoff's height, below the board) a second cut gives the mounting holes: each circle at depth 3, a hole in a post inside the pocket, is a `MountingHole` at its centre, stated by its diameter (`.diameter`, as `import dxf` states a hole); a drill outside what a mounting hole's footprint can be drawn for refuses the import, as it does there.\n\nThe cut is of the model's triangles (0.01 mm of chord): the arcs are found again within 2 micrometres and the collinear corners joined; a shape that is not lines and circular arcs stays chords. The board's frame is the outline's top left corner with y down, and the output says which point of the posed model that is, and the `.x` and `.y` that put the model's pocket where the board's frame has it. With `--limits` the cut is not the only one: what the case holds over and under the board are the **height limits**, each a `Keepout` with `.max_height`, named `limit1`... (the top face's from the lowest, then the bottom face's). The case's undersides above the board's top face (a lid, a rib or a boss hanging from it) limit the parts on the top face (`.layers &= \"top\"`, `.max_height` the underside's height from the top face); what the case holds below the board's bottom face (z = -`--thickness`, 1.6 mm unless said: a floor, a rib or a boss standing under it) limits the parts on the bottom face (`.layers &= \"bottom\"`, `.max_height` from the bottom face down to it). A limit is cut to the outline, and one that lies in another of the same height or a lower one is left out. What stands through the board or from its face to the case (a post or a block, a cutout) is not a limit. A region that is not a rectangle is covered by rectangles taken outward to 0.05 mm: an L is two, a round underside slabs of at most `--limit-step` mm (0.5 by default), so it is limited a little more than it covers; a sloped underside (a dome, a slope, a chamfer: a face that leans more than about 10 degrees from upright) is followed in steps of `--limit-step` mm, each step limited to the height of its lower end, so the underside is no lower over the step and up to a step higher, and a feature thinner than a step is not seen (a smaller step gives more, finer limits). A plane at which a solid is not closed is a warning and its limits are left out.\n\nA solid that is not closed at the plane is an error that says where."
)]
#[command(args_override_self = true)]
pub struct ModelArgs {
    /// The case's STEP file.
    #[arg(value_name = "FILE.STEP")]
    pub file: PathBuf,
    /// The model's rotation about x, in degrees (`.rx`).
    #[arg(
        long,
        value_name = "DEG",
        default_value_t = 0.0,
        allow_hyphen_values = true
    )]
    pub rx: f64,
    /// The model's rotation about y, in degrees (`.ry`).
    #[arg(
        long,
        value_name = "DEG",
        default_value_t = 0.0,
        allow_hyphen_values = true
    )]
    pub ry: f64,
    /// The model's rotation about z, in degrees (`.rz`).
    #[arg(
        long,
        value_name = "DEG",
        default_value_t = 0.0,
        allow_hyphen_values = true
    )]
    pub rz: f64,
    /// The model's offset in x, in mm (`.x`).
    #[arg(
        long,
        value_name = "MM",
        default_value_t = 0.0,
        allow_hyphen_values = true
    )]
    pub x: f64,
    /// The model's offset in y, in mm (`.y`).
    #[arg(
        long,
        value_name = "MM",
        default_value_t = 0.0,
        allow_hyphen_values = true
    )]
    pub y: f64,
    /// The model's offset in z, in mm (`.z`).
    #[arg(
        long,
        value_name = "MM",
        default_value_t = 0.0,
        allow_hyphen_values = true
    )]
    pub z: f64,
    /// The height of the plane that cuts the outline and the cutouts, in mm (the board's top face is 0).
    #[arg(long, value_name = "MM", default_value_t = -0.8, allow_hyphen_values = true)]
    pub at: f64,
    /// The height of a second plane that cuts the standoffs for the mounting holes, in mm.
    #[arg(long, value_name = "MM", allow_hyphen_values = true)]
    pub holes_at: Option<f64>,
    /// Also print the height limits the case puts over and under the board, as `Keepout`s with `.max_height`.
    #[arg(long)]
    pub limits: bool,
    /// The board's thickness, in mm: its bottom face is at z = -THICKNESS (for the limits of the bottom face).
    #[arg(long, value_name = "MM", default_value_t = 1.6)]
    pub thickness: f64,
    /// Step in mm by which `--limits` follows a sloped or curved underside.
    ///
    /// It is the height of a step of a slope and of a slab of a curve.
    #[arg(long, value_name = "MM", default_value_t = 0.5)]
    pub limit_step: f64,
    /// The board, when the file `--write` names has several.
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// Put the import into the physical file beside this source file, and print the diff (ADR 0048).
    ///
    /// It edits `<stem>.physical.nsm` beside that file as `import dxf --write` does: the members
    /// of an earlier import (`outline`, `cutout1`, `hole1`, `limit1`...) and the lines that
    /// state their places are replaced, every other line and comment stays. Every numbered
    /// member of a kind the import prints is replaced, also the ones above the new count
    /// (`limit7` of an import that now finds 5; the holes only with `--holes-at`, the limits only
    /// with `--limits`). The circuit source and the layout file are never written.
    #[arg(long, value_name = "FILE.NSM")]
    pub write: Option<PathBuf>,
}

pub fn run(args: Args) -> ExitCode {
    match args.command {
        Command::Place(a) => run_place(&a.matrix),
        Command::Model(a) => run_import(&a),
    }
}

/// `netsmith import case model`: the source to paste goes to stdout, the report and warnings to
/// stderr (as `import dxf` does).
fn run_import(a: &ModelArgs) -> ExitCode {
    use netsmith_tools::{case_import, dxf};
    let fail = |e: String| {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    };
    let bytes = match fs::read(&a.file) {
        Ok(b) => b,
        Err(e) => return fail(format!("{}: {e}", a.file.display())),
    };
    let name = a.file.display().to_string();
    let imported = match case_import::import(
        &bytes,
        &name,
        &case_import::Request {
            rotation: [a.rx, a.ry, a.rz],
            offset: [a.x, a.y, a.z],
            at: a.at,
            holes_at: a.holes_at,
            limits: a.limits,
            thickness: a.thickness,
            limit_step: a.limit_step,
        },
    ) {
        Ok(i) => i,
        Err(e) => return fail(e),
    };
    for w in &imported.warnings {
        eprintln!("warning: {w}");
    }
    let written = match a.write.as_deref() {
        Some(source) => match dxf::write_physical(source, a.board.as_deref(), &imported) {
            Ok(w) => Some(w),
            Err(e) => return fail(e),
        },
        None => None,
    };
    let board_name = written
        .as_ref()
        .map(|(b, _)| b.as_str())
        .or(a.board.as_deref());
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

fn run_place(words: &[String]) -> ExitCode {
    let matrix = match parse_matrix(words) {
        Ok(m) => m,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::from(2);
        }
    };
    match netsmith_3d::case_pose_of_matrix(matrix) {
        Ok(pose) => {
            print!("{}", lines(&pose));
            eprintln!(
                "checked: these angles turn back into the matrix given to within {} (the largest difference of an entry of the rotation)",
                number(pose.residual.max(0.0), 9)
            );
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("error: the matrix is not a pose of a case model: {why}");
            ExitCode::from(1)
        }
    }
}

/// The lines of a `CaseModel` that state `pose`, each one value.
fn lines(pose: &netsmith_3d::CasePose) -> String {
    let mut out = String::new();
    for (name, v) in ["rx", "ry", "rz"].iter().zip(pose.rotation) {
        out.push_str(&format!(".{name} &= {}\n", number(v, 6)));
    }
    for (name, v) in ["x", "y", "z"].iter().zip(pose.offset) {
        out.push_str(&format!(".{name} &= {}mm\n", number(v, 6)));
    }
    out
}

/// `v` to `places` decimals, with no trailing zeros and no negative zero.
fn number(v: f64, places: usize) -> String {
    let s = format!("{v:.places$}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    };
    if s == "-0" { "0".to_string() } else { s }
}

/// The 3 x 4 matrix the words say: 12 numbers, or 16 whose last four are 0 0 0 1.
fn parse_matrix(words: &[String]) -> Result<[[f64; 4]; 3], String> {
    let mut numbers = Vec::new();
    for word in words {
        for piece in word
            .split(|c: char| c.is_whitespace() || matches!(c, ',' | '[' | ']' | '(' | ')' | ';'))
            .filter(|p| !p.is_empty())
        {
            let n: f64 = piece
                .parse()
                .map_err(|_| format!("`{piece}` is not a number"))?;
            numbers.push(n);
        }
    }
    match numbers.len() {
        12 => {}
        16 => {
            if numbers[12..] != [0.0, 0.0, 0.0, 1.0] {
                return Err(format!(
                    "the last row of a 4 x 4 matrix is 0 0 0 1, and this one's is {}",
                    numbers[12..]
                        .iter()
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
        }
        n => {
            return Err(format!(
                "a pose is a 3 x 4 matrix of 12 numbers (or 4 x 4, 16), and {n} were given"
            ));
        }
    }
    Ok([0, 1, 2].map(|r| [0, 1, 2, 3].map(|c| numbers[r * 4 + c])))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &[&str]) -> Vec<String> {
        s.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn a_matrix_is_read_from_words_brackets_and_commas() {
        let want = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, -1.0, 0.0, 2.5],
            [0.0, 0.0, -1.0, -1.6],
        ];
        assert_eq!(
            parse_matrix(&words(&["1 0 0 0", "0 -1 0 2.5", "0", "0", "-1", "-1.6"])),
            Ok(want)
        );
        assert_eq!(
            parse_matrix(&words(&[
                "[[1, 0, 0, 0], [0, -1, 0, 2.5], [0, 0, -1, -1.6], [0, 0, 0, 1]]"
            ])),
            Ok(want)
        );
        assert!(
            parse_matrix(&words(&["1 0 0"]))
                .unwrap_err()
                .contains("3 were given")
        );
        assert!(
            parse_matrix(&words(&["1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 2"]))
                .unwrap_err()
                .contains("0 0 0 1")
        );
        assert!(
            parse_matrix(&words(&["1 0 0 0 0 1 0 0 0 0 1 x"]))
                .unwrap_err()
                .contains("`x`")
        );
    }

    #[test]
    fn a_number_has_no_trailing_zeros_and_no_negative_zero() {
        assert_eq!(number(180.0, 6), "180");
        assert_eq!(number(-1.6, 6), "-1.6");
        assert_eq!(number(-0.0000001, 6), "0");
        assert_eq!(number(12.3456789, 6), "12.345679");
    }
}
