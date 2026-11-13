//! Print the reference table of a KiCad board (outline, mounting holes, every footprint and its
//! pads) as TOML, for an example's parity checker (the RP2350A Minimal, ORBIT).
//!
//! `cargo run -p netsmith-tools --example reference_table -- <file.kicad_pcb> > placements.toml`
//!
//! The parts marked `must_match = true` are the RP2350A Minimal board's unless the board has
//! its own list: `--must-match <file.toml>` (a TOML file with `parts = ["J1", ...]`, see
//! `netsmith_tools::reference_table::MustMatch`).
//!
//! The reading is in `netsmith_tools::reference_table`, where a test runs it.

use std::{env, fs, path::Path, process::ExitCode};

use netsmith_tools::reference_table::{MustMatch, placements_toml_with};

const USAGE: &str = "usage: reference_table [--must-match <must_match.toml>] <file.kicad_pcb>   \
                     (the table goes to standard output)";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let (list, file) = match &args[..] {
        [file] => (None, file),
        [flag, list, file] if flag == "--must-match" => (Some(list), file),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let read = |path: &String| match fs::read_to_string(path) {
        Ok(t) => Ok(t),
        Err(e) => {
            eprintln!("{path}: {e}");
            Err(ExitCode::from(2))
        }
    };
    let must = match list {
        None => MustMatch::default(),
        Some(path) => {
            let text = match read(path) {
                Ok(t) => t,
                Err(code) => return code,
            };
            match MustMatch::from_toml(&text) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("{path}: {e}");
                    return ExitCode::from(2);
                }
            }
        }
    };
    let text = match read(file) {
        Ok(t) => t,
        Err(code) => return code,
    };
    let name = Path::new(file)
        .file_name()
        .map_or_else(|| file.clone(), |n| n.to_string_lossy().into_owned());
    match placements_toml_with(&text, &name, &must) {
        Ok(table) => {
            print!("{table}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}
