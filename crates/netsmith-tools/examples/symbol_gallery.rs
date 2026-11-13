//! Write a page that shows every symbol netsmith draws itself beside the KiCad symbol it
//! replaces (ADR 0097), so that its look can be reviewed.
//!
//! `cargo run -p netsmith-tools --example symbol_gallery -- --out <folder> [--file <name>]`
//!
//! The page is `gallery.html` in the folder, one self-contained file (inline SVG, no script,
//! no network, a light and a dark theme). KiCad's stock symbols are drawn where its symbol
//! library is installed (`NETSMITH_KICAD_SYMBOLS`, or the usual places, as `netsmith_kicad::Search`
//! says); where it is not, each stock side says so. `--no-kicad` leaves them out on purpose.
//! The committed copy is `docs/symbol-gallery.html` (`--out docs --file symbol-gallery.html`).
//!
//! The drawing is in `netsmith_tools::symbol_gallery`, where a test runs it.

use std::{env, fs, path::Path, process::ExitCode};

use netsmith_tools::symbol_gallery::{example_ics, installed_kicad, render};

const USAGE: &str = "usage: symbol_gallery --out <folder> [--file <name.html>] [--no-kicad]";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let (mut out, mut file, mut kicad) = (None, "gallery.html".to_string(), true);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => out = it.next().cloned(),
            "--file" => match it.next() {
                Some(name) => file = name.clone(),
                None => {
                    eprintln!("{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "--no-kicad" => kicad = false,
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(out) = out else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let ics = match example_ics(&examples) {
        Ok(ics) => ics,
        Err(e) => {
            eprintln!("symbol_gallery: {e}");
            return ExitCode::from(1);
        }
    };
    let library = if kicad { installed_kicad() } else { None };
    match &library {
        Some(_) => eprintln!("KiCad's symbol library found: the stock symbols are drawn"),
        None => {
            eprintln!("KiCad's symbol library not found (or --no-kicad): no stock symbol drawn")
        }
    }
    let page = render(library.as_ref(), &ics);
    let path = Path::new(&out).join(&file);
    if let Err(e) = fs::create_dir_all(&out).and_then(|()| fs::write(&path, page)) {
        eprintln!("{}: {e}", path.display());
        return ExitCode::from(1);
    }
    eprintln!("wrote {}", path.display());
    ExitCode::SUCCESS
}
