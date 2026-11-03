//! Plots a board with netsmith's writers and with KiCad and compares them.
//!
//! ```text
//! cargo run -p netsmith-fab --example crosscheck -- board.kicad_pcb [--out dir]
//! ```
//!
//! With `--out`, writes netsmith's files and a picture of every layer that differs.

use netsmith_fab::crosscheck::{Limits, check};
use netsmith_fab::{KicadCli, KicadCliExe};
use std::path::PathBuf;

fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1);
    let (mut pcb, mut out) = (None, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = args.next().map(PathBuf::from),
            _ => pcb = Some(PathBuf::from(a)),
        }
    }
    let Some(pcb) = pcb else {
        eprintln!("usage: crosscheck <board.kicad_pcb> [--out <dir>]");
        return std::process::ExitCode::from(2);
    };
    let Some(kicad) = KicadCliExe::locate() else {
        eprintln!("error: the cross-check needs KiCad: set KICAD_CLI to its command-line tool");
        return std::process::ExitCode::from(2);
    };
    match check(
        &pcb,
        &kicad as &dyn KicadCli,
        &Limits {
            pictures: out.is_some(),
            ..Limits::default()
        },
    ) {
        Ok(report) => {
            print!("{report}");
            if let Some(dir) = out {
                let _ = std::fs::create_dir_all(&dir);
                for l in &report.layers {
                    if let Some(png) = &l.diff_png {
                        let _ = std::fs::write(dir.join(format!("{}.diff.png", l.file)), png);
                    }
                }
            }
            if report.ok() {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::from(2)
        }
    }
}
