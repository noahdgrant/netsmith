//! Prints what the KiCad lookup of `netsmith_kicad::Search` finds on this machine: the
//! `kicad-cli`, and the symbol and footprint folders with how each was found. CI runs it to
//! show the Linux lookup on a real install (issue #6); it exits with an error if a folder is
//! not found.
//!
//! `cargo run -p netsmith-kicad --example locate`

use netsmith_kicad::{InstallKind, Search, locate_install};

fn main() {
    match Search::machine().kicad_cli() {
        Some(cli) => println!("kicad-cli: {}", cli.display()),
        None => println!("kicad-cli: not found"),
    }
    let mut missing = false;
    for kind in [InstallKind::Symbols, InstallKind::Footprints] {
        match locate_install(kind) {
            Ok(found) => println!(
                "{} folder: {} (found: {:?})",
                kind.variable(),
                found.dir.display(),
                found.source
            ),
            Err(not_found) => {
                println!("{not_found}");
                missing = true;
            }
        }
    }
    if missing {
        std::process::exit(1);
    }
}
