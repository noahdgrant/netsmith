//! Runs netsmith's design rules check on a `.kicad_pcb` (with the `.kicad_pro` and the
//! `.kicad_dru` beside it), and with `--kicad` runs KiCad's too and compares the two.
//!
//! `cargo run -p netsmith-fab --example drc -- board.kicad_pcb [--kicad]`

use netsmith_fab::{KicadCli, KicadCliExe};
use netsmith_fab::{compare_kicad_drc, compare_kicad_drc_with_rules_file, parse_kicad_drc};
use netsmith_geom::{DrcExtras, drc_check, drc_project_of, drc_rules_file_of};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("usage: drc <board.kicad_pcb> [--kicad]");
        std::process::exit(2);
    };
    let path = Path::new(path);
    let src = std::fs::read_to_string(path).expect("cannot read the board");
    let started = std::time::Instant::now();
    let rules_file = drc_rules_file_of(path);
    let extras = match &rules_file {
        Some(text) => DrcExtras::default().with_rules_file(text),
        None => DrcExtras::default(),
    };
    let report =
        drc_check(&src, drc_project_of(path).as_deref(), &extras).unwrap_or_else(|e| panic!("{e}"));
    eprintln!(
        "netsmith: {} finding(s) in {:?}",
        report.findings.len(),
        started.elapsed()
    );
    print!("{report}");
    if args.iter().any(|a| a == "--kicad") {
        let cli = KicadCliExe::locate().expect("no kicad-cli");
        let json = cli.drc_json(path).unwrap_or_else(|e| panic!("{e}"));
        let theirs = parse_kicad_drc(&json).unwrap();
        let comparison = if rules_file.is_some() {
            compare_kicad_drc_with_rules_file(&report, &theirs, 0.1)
        } else {
            compare_kicad_drc(&report, &theirs, 0.1)
        };
        print!("{comparison}");
    }
}
