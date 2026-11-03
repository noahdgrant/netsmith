//! `cargo run -p netsmith-lang --example parse -- examples/led.nsm`
//! Prints the canonical form of a file, or the first error.

use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: parse <file.nsm>");
        return ExitCode::from(2);
    };
    let src = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    };
    match netsmith_lang::parse(&src) {
        Ok(file) => {
            print!("{}", file.to_source());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprint!("{}", e.render(&path, &src));
            ExitCode::FAILURE
        }
    }
}
