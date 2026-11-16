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
}

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "`case place` turns a pose given as a matrix (a STEP assembly's transform, a CAD program's matrix) into the lines of a `CaseModel` (`std::pcb::CaseModel`) that pose the model the same way, to paste into the case model's block in the `case { }` section of the board's physical file: `.rx`, `.ry` and `.rz` (degrees, about x, then y, then z of the 3D scene's frame) and `.x`, `.y` and `.z` (mm).\n\nThe matrix is 3 x 4, row major: each row is three numbers of the rotation and then the offset in mm, and it acts on (x, y, z, 1) of the model's file. Sixteen numbers are a 4 x 4 matrix whose last row must be 0 0 0 1. The numbers may be separated by spaces, commas, brackets or parentheses, in one argument or many, so a matrix printed as `[[1, 0, 0, 0], [0, -1, 0, 0], ...]` can be pasted in quotes.\n\nA rotation has two sets of angles in general: the one printed has `.ry` from -90 to 90, and `.rz` 0 when `.ry` is a quarter turn. The command checks its angles by turning them back into a matrix, and says on stderr by how much that matrix differs from the one given. A matrix that is not a rotation is refused (exit 1): a row whose length is not 1 (a scale, as a model in inches would need, or a shear), two rows that are not square to one another (a shear), or a mirror (a determinant of -1, which no angles draw), each within 0.0001 so that a matrix typed with four decimals is read."
)]
#[command(args_override_self = true)]
pub struct PlaceArgs {
    /// The matrix's 12 numbers (3 x 4, row major, the offset in mm last in each row), or 16 for a 4 x 4 one.
    #[arg(value_name = "NUMBER", required = true, num_args = 1.., allow_hyphen_values = true)]
    pub matrix: Vec<String>,
}

pub fn run(args: Args) -> ExitCode {
    match args.command {
        Command::Place(a) => run_place(&a.matrix),
    }
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
