//! Writes the JLCPCB upload package for a board.
//!
//! ```sh
//! cargo run -p netsmith-fab --example fab -- board.kicad_pcb --bom parts.csv --out fab
//! cargo run -p netsmith-fab --example fab -- board.kicad_pcb --lcsc-property "LCSC Part #" --out fab
//! ```
//!
//! The BOM comes from `--bom`, a CSV with the columns `Comment,Designator,Footprint,LCSC
//! Part #` (the file this program writes), or else from a property on each footprint,
//! `LCSC Part #` unless `--lcsc-property` names another. Add `--allow-drc-violations` to
//! package a board that fails netsmith's design rules check, and `--allow-missing-lcsc` to
//! list parts without a part number as unassembled. KiCad is not needed.

use netsmith_fab::{Board, Options, bom, package};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut pcb = None;
    let mut bom_file = None;
    let mut property = "LCSC Part #".to_string();
    let mut out = None;
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);
    let usage = "usage: fab <board.kicad_pcb> --out <dir> [--bom <bom.csv> | --lcsc-property <name>] [--allow-drc-violations] [--allow-missing-lcsc]";
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| args.next().ok_or(format!("{flag} needs a value\n{usage}"));
        match arg.as_str() {
            "--bom" => bom_file = Some(PathBuf::from(value("--bom")?)),
            "--lcsc-property" => property = value("--lcsc-property")?,
            "--out" => out = Some(PathBuf::from(value("--out")?)),
            "--allow-drc-violations" => options.allow_drc_violations = true,
            "--allow-missing-lcsc" => options.allow_missing_lcsc = true,
            flag if flag.starts_with("--") => {
                return Err(format!("unknown option {flag}\n{usage}"));
            }
            _ => pcb = Some(PathBuf::from(arg)),
        }
    }
    let pcb = pcb.ok_or(usage)?;
    let out = out.ok_or(usage)?;

    let entries = match bom_file {
        Some(file) => {
            let text = std::fs::read_to_string(&file)
                .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
            bom::parse_csv(&text).map_err(|e| e.to_string())?
        }
        None => {
            let text = std::fs::read_to_string(&pcb)
                .map_err(|e| format!("cannot read {}: {e}", pcb.display()))?;
            bom::from_board(&Board::parse(&text).map_err(|e| e.to_string())?, &property)
        }
    };
    let package = package(&pcb, &entries, &options).map_err(|e| e.to_string())?;
    let written = package.write(&out).map_err(|e| e.to_string())?;
    print!("{}", package.summary());
    for path in written {
        println!("wrote {}", path.display());
    }
    Ok(())
}
