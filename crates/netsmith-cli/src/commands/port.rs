//! `netsmith port skeleton` (#452): the first source of a port of a KiCad board, and its parts
//! file. The logic is `netsmith::port_skeleton`.

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
    /// Write the first source of a port of a KiCad board, and its `reference/parts.toml`.
    #[command(
        after_long_help = "One member for each footprint, named by its designator in lower case, and one join for each net, after a comment with the original's net name; a net the original names itself gets `.<member>.<pin>.net &= \"<name>\"` on one pin. A resistor, capacitor, inductor or LED whose footprint and value say so plainly (KiCad's own footprint name, `R_0402_1005Metric`, and a value of one word, `4k7`, `100nF`, `GREEN`) is the standard part; an R, C or L has a tolerance of 1 %, 10 % or 20 % on a line marked `TODO tolerance` (the original states none), and the command lists each such line to check against the original's bill of materials. Every other footprint is a part named after its value, marked GAP when its file `src/<name>.nsm` is not there, so the source does not build until each has one. The joins name a gap's pins by the original board's pin names, made into paths as `part from-symbol` makes them, and a pad with no name by its number (`.u1.'3'`); a part file that is there names them from its pin table. The parts file goes in `reference/` of the project (the folder above the source's). The board file is read as it is, with no KiCad, and nothing is overwritten: the exit code is 2 when either file is there already or the board cannot be read, and 0 otherwise."
    )]
    Skeleton(SkeletonArgs),
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct SkeletonArgs {
    /// The original's KiCad board.
    #[arg(value_name = "ORIGINAL.KICAD_PCB")]
    pub original: PathBuf,
    /// The source to write (default `src/main.nsm`); it must not be there yet.
    #[arg(long, value_name = "FILE.NSM")]
    pub out: Option<PathBuf>,
}

pub fn run(args: Args) -> ExitCode {
    match args.command {
        Command::Skeleton(a) => skeleton(&a),
    }
}

/// Write the two files and print the counts, the lines marked `TODO tolerance` and the gaps.
fn skeleton(args: &SkeletonArgs) -> ExitCode {
    match netsmith::port_skeleton::write(&args.original, args.out.as_deref()) {
        Ok((skeleton, parts)) => {
            let out = args.out.as_deref().unwrap_or(Path::new("src/main.nsm"));
            println!(
                "wrote {} ({} members: {} standard parts, {} gaps; {} joins, {} net names) and {}",
                out.display(),
                skeleton.members,
                skeleton.standard,
                skeleton.gaps.len(),
                skeleton.joins,
                skeleton.named,
                parts.display()
            );
            if !skeleton.tolerances.is_empty() {
                println!(
                    "the skeleton's tolerance, marked TODO tolerance (the original states none; \
                     check each against its bill of materials):"
                );
                for (line, refdes) in &skeleton.tolerances {
                    println!("  {}:{line}: {refdes}", out.display());
                }
            }
            if !skeleton.gaps.is_empty() {
                println!(
                    "gaps, each needs a part file (marked GAP in the source): {}",
                    skeleton.gaps.join(", ")
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{kind, parse};
    use crate::commands::{Command, port};
    use clap::error::ErrorKind;
    use std::path::PathBuf;

    #[test]
    fn port_skeleton_takes_a_board_and_an_optional_out() {
        assert_eq!(
            parse(&["port", "skeleton", "a.kicad_pcb", "--out", "src/b.nsm"]),
            Ok(Command::Port(port::Args {
                command: port::Command::Skeleton(port::SkeletonArgs {
                    original: PathBuf::from("a.kicad_pcb"),
                    out: Some(PathBuf::from("src/b.nsm")),
                })
            }))
        );
        assert!(matches!(
            parse(&["port", "skeleton", "a.kicad_pcb"]),
            Ok(Command::Port(port::Args {
                command: port::Command::Skeleton(port::SkeletonArgs { out: None, .. })
            }))
        ));
        assert_eq!(
            kind(&["port", "skeleton"]),
            ErrorKind::MissingRequiredArgument
        );
        assert_eq!(
            kind(&["port", "skeleton", "a.kicad_pcb", "--force"]),
            ErrorKind::UnknownArgument
        );
    }
}
