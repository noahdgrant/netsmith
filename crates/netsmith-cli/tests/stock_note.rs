//! A note for a part the solver picked although a file says it is out of stock (issue #176).
//! The solver does not read stock (ADR 0047), so the pick is the same either way; the build
//! says so in its notes, and the editor shows the same line. The board and the parts are
//! invented, except that the one LCSC number in `data/jlcpcb/considered.csv` is a real one.

use netsmith_kicad::testlib::TempDir;
use std::{fs, process::Command};

const BOARD: &str = r#"
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

module Limit {
    .supply: Rail
    .limit: Resistor { .resistance &= 45.8kohm..46kohm }
    .supply.pos ~> .limit ~> .supply.neg
}

board Invented {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .charge: Limit
    .supply ~ .charge.supply
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
}
"#;

/// The project's own resistor row: a value the standard catalog has no row for.
fn rows(lcsc: &str, stock: &str) -> String {
    format!(
        "lcsc,mpn,resistance,max_power,package,library,stock,source\n\
         {lcsc},ZZR-459,45.9kohm +/- 0.1%,150mW,0603,Extended,{stock},https://example.invalid/zzr-459\n"
    )
}

#[test]
fn netsmith_build_prints_the_note_and_still_builds() {
    let root = TempDir::new("stock-note-build");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("catalogs")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"invented\"\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), BOARD).unwrap();
    fs::write(root.join("catalogs/resistor.csv"), rows("C900001", "0")).unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["build"])
        .current_dir(&*root)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env(
            "NETSMITH_KICAD_SYMBOLS",
            netsmith_kicad::testlib::sample_symbols(),
        )
        .env(
            "NETSMITH_KICAD_FOOTPRINTS",
            netsmith_kicad::testlib::sample_footprints(),
        )
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{err}");
    let line = err
        .lines()
        .find(|l| l.starts_with("note: R1 (`charge.limit`) is C900001"))
        .unwrap_or_else(|| panic!("no stock note:\n{err}"));
    assert!(line.contains("states a stock of 0"), "{line}");
    assert!(
        !err.lines()
            .any(|l| l.starts_with("warning:") && l.contains("stock")),
        "a note is not a warning:\n{err}"
    );
}
