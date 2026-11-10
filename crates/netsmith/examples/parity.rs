//! Compare a board written by `netsmith fab` with Raspberry Pi's RP2350A Minimal board.
//!
//! `cargo run -p netsmith --example parity -- <netsmith.kicad_pcb> [--table <placements.toml>]
//! [--map <designators.toml>] [--svg <overlay.svg>]`
//!
//! The report goes to standard output and the exit status is 1 when a must-match item fails
//! (0 when none does, 2 when a file cannot be read). By default the table and the map are those
//! of `examples/rp2350_minimal/reference/`. The checking is in `netsmith::parity`.

use std::{env, fs, path::PathBuf, process::ExitCode};

use netsmith::parity::{DesignatorMap, Tolerances, check_text};

const USAGE: &str = "usage: parity <netsmith.kicad_pcb> [--table <placements.toml>] \
                     [--map <designators.toml>] [--svg <overlay.svg>]";

fn read(path: &PathBuf) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn run() -> Result<bool, String> {
    let reference =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/rp2350_minimal/reference");
    let (mut pcb, mut table, mut map, mut svg) = (None, None, None, None);
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--table" | "--map" | "--svg" => {
                let value = args.next().ok_or(format!("{arg} takes a file\n{USAGE}"))?;
                let slot = match arg.as_str() {
                    "--table" => &mut table,
                    "--map" => &mut map,
                    _ => &mut svg,
                };
                *slot = Some(PathBuf::from(value));
            }
            "-h" | "--help" => return Err(USAGE.to_string()),
            flag if flag.starts_with("--") => return Err(format!("{flag} is not a flag\n{USAGE}")),
            _ if pcb.is_none() => pcb = Some(PathBuf::from(arg)),
            _ => return Err(format!("one board file only\n{USAGE}")),
        }
    }
    let pcb = pcb.ok_or(USAGE)?;
    let table = read(&table.unwrap_or_else(|| reference.join("placements.toml")))?;
    let map = DesignatorMap::parse(&read(
        &map.unwrap_or_else(|| reference.join("designators.toml")),
    )?)?;
    let report = check_text(&table, &read(&pcb)?, &map, &Tolerances::default())?;
    print!("{}", report.text());
    if let Some(svg) = svg {
        fs::write(&svg, report.overlay_svg()).map_err(|e| format!("{}: {e}", svg.display()))?;
    }
    Ok(report.passed())
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}
