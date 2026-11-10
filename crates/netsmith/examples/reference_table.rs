//! Print the reference table of a KiCad board (outline, mounting holes, every footprint and its
//! pads) as TOML, for the RP2350A Minimal example's parity checker.
//!
//! `cargo run -p netsmith --example reference_table -- <file.kicad_pcb> > placements.toml`
//!
//! The reading is in `netsmith::reference_table`, where a test runs it.

use std::{env, fs, path::Path, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let [file] = &args[..] else {
        eprintln!("usage: reference_table <file.kicad_pcb>   (the table goes to standard output)");
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
    match netsmith::reference_table::placements_toml(&text, &name) {
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
