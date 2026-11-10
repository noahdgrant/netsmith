//! Print the reference table of a KiCad board (outline, mounting holes, every footprint and its
//! pads) as TOML, for the RP2350A Minimal example's parity checker.
//!
//! `cargo run -p netsmith --example reference_table -- [--must-match <list.toml>] <file.kicad_pcb> > placements.toml`
//!
//! The footprints that must match are the RP2350A Minimal's unless a list is given with
//! `--must-match` (`references` and `prefixes`, see `netsmith::reference_table::MustMatch`).
//!
//! The reading is in `netsmith::reference_table`, where a test runs it.

use std::{env, fs, path::Path, process::ExitCode};

use netsmith::reference_table::{MustMatch, placements_toml_with};

fn main() -> ExitCode {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let mut must_match = None;
    if let Some(i) = args.iter().position(|a| a == "--must-match") {
        if i + 1 >= args.len() {
            eprintln!("--must-match needs the file of the list");
            return ExitCode::from(2);
        }
        let list = args.remove(i + 1);
        args.remove(i);
        let text = match fs::read_to_string(&list) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{list}: {e}");
                return ExitCode::from(2);
            }
        };
        match MustMatch::from_toml(&text) {
            Ok(m) => must_match = Some(m),
            Err(e) => {
                eprintln!("{list}: {e}");
                return ExitCode::from(2);
            }
        }
    }
    let [file] = &args[..] else {
        eprintln!(
            "usage: reference_table [--must-match <list.toml>] <file.kicad_pcb>   (the table goes to standard output)"
        );
        return ExitCode::from(2);
    };
    let text = match fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{file}: {e}");
            return ExitCode::from(2);
        }
    };
    let name = Path::new(file)
        .file_name()
        .map_or_else(|| file.clone(), |n| n.to_string_lossy().into_owned());
    match placements_toml_with(&text, &name, &must_match.unwrap_or_default()) {
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
