//! `netsmith port skeleton` (#452): the first source of a port of a KiCad board, and its parts
//! file. The logic is `netsmith_tools::port_skeleton`. `netsmith port tables` (#560): the derived
//! tables of an original too large to commit, held against its `source.toml`; the logic is
//! `netsmith_tools::port_tables`.

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
    /// Write the original's netlist and placements into `reference/`, held against its `source.toml`.
    #[command(
        after_long_help = "For an original too large to commit (a `.kicad_pcb` over 10 MB), the port holds what its tests read from it instead: `netlist.xml`, the original's netlist in KiCad's netlist XML (the nets proof and `netsmith compare` read it), and `placements.toml`, its outline, holes, parts and pads (`netsmith parity` reads it), with `source.toml`, which names the original by its file name, its URL at a pinned commit, its size and its sha256. The board file is read as it is, with no KiCad. With no `source.toml` in the reference folder, `--url` is needed (an https URL with a commit's 40 hex digits in its path, not a branch) and the file is written; with one, the original must have its size and sha256, and `--url`, when given, its URL. The must-match parts of `placements.toml` are those of `must_match.toml` in the reference folder when there is one. The tables are written over the ones there, unless `--check`: then nothing is written and each committed table must be what the original gives. The exit code is 1 when the original is not the file `source.toml` names or, with `--check`, a table is not current, 2 when a file cannot be read or `--url` is missing, and 0 otherwise."
    )]
    Tables(TablesArgs),
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct TablesArgs {
    /// The original's KiCad board, fetched from the URL of `source.toml`.
    #[arg(value_name = "ORIGINAL.KICAD_PCB")]
    pub original: PathBuf,
    /// The original's URL at a pinned commit, for a new `source.toml`.
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,
    /// The reference folder of the port (default `reference`).
    #[arg(long, value_name = "DIR", default_value = "reference")]
    pub reference: PathBuf,
    /// Write nothing; fail when a committed table is not what the original gives.
    #[arg(long)]
    pub check: bool,
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
        Command::Tables(a) => tables(&a),
    }
}

/// Write or check the tables and print what they hold.
fn tables(args: &TablesArgs) -> ExitCode {
    use netsmith_tools::port_tables::{Mode, TablesError, write};
    let mode = if args.check { Mode::Check } else { Mode::Write };
    match write(&args.original, &args.reference, args.url.as_deref(), mode) {
        Ok(w) => {
            let dir = args.reference.display();
            let what = format!(
                "{} components and {} nets of {} ({} bytes, sha256 {})",
                w.tables.components, w.tables.nets, w.source.file, w.source.size, w.source.sha256
            );
            match mode {
                Mode::Check => {
                    println!("{dir}/netlist.xml and {dir}/placements.toml are current: {what}")
                }
                Mode::Write => println!(
                    "wrote {dir}/netlist.xml and {dir}/placements.toml{}: {what}",
                    if w.new_source {
                        format!(" and {dir}/source.toml")
                    } else {
                        String::new()
                    }
                ),
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(match e {
                TablesError::Failed(_) => 1,
                TablesError::Cannot(_) => 2,
            })
        }
    }
}

/// Write the two files and print the counts, the lines marked `TODO tolerance` and the gaps.
fn skeleton(args: &SkeletonArgs) -> ExitCode {
    match netsmith_tools::port_skeleton::write(&args.original, args.out.as_deref()) {
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

    #[test]
    fn port_tables_takes_a_board_a_url_a_folder_and_check() {
        assert_eq!(
            parse(&[
                "port",
                "tables",
                "a.kicad_pcb",
                "--url",
                "https://h/x",
                "--check"
            ]),
            Ok(Command::Port(port::Args {
                command: port::Command::Tables(port::TablesArgs {
                    original: PathBuf::from("a.kicad_pcb"),
                    url: Some("https://h/x".into()),
                    reference: PathBuf::from("reference"),
                    check: true,
                })
            }))
        );
        assert!(matches!(
            parse(&["port", "tables", "a.kicad_pcb", "--reference", "r"]),
            Ok(Command::Port(port::Args {
                command: port::Command::Tables(port::TablesArgs {
                    url: None,
                    check: false,
                    ..
                })
            }))
        ));
        assert_eq!(
            kind(&["port", "tables"]),
            ErrorKind::MissingRequiredArgument
        );
    }
}
