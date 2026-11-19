//! `netsmith board build` on a board with a backup cell (issue #1258): a 5 V input into the
//! standard linear regulator, a cell behind a Schottky on its 3.3 V output, and a monitor that
//! senses the input through a divider. In the scenario with the input removed, the regulator's
//! body diode (the standard `.reverse`, which no part states) feeds the input from behind and
//! the divider puts the sense line between the monitor's levels: the build fails and says so.
//! With the regulator stated as blocking, the build passes and its note lists what the cell
//! reaches. `Monitor` is invented; it is not a real part.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

const SOURCE: &str = r#"import std::logic::Logic
import std::sources::{Rail, ReversePath}
import std::parts::passive::Resistor
import std::parts::diode::Schottky
import std::parts::regulator::LinearRegulator
import std::parts::connector::Header1x2

/// Invented: runs from 2.4 V to 3.6 V and reads a high from 70 % of its supply.
#[kind(ic)]
part Monitor {
    .designator &= "U"
    .vdd: Rail
    .vdd.voltage >= 2.4V
    .vdd.voltage <= 3.6V
    .det: Logic
    .det.reference ~ .vdd
    .det.reference.current &= 0A
    .det.vih &= 0.7 * .vdd.voltage
    .det.vil &= 0.3 * .vdd.voltage
    .package: Package
    .package &= "SOT-23"
    pins "SOT-23" {
        .vdd.pos  power "VDD" "1"
        .vdd.neg  power "GND" "2"
        .det.line input "DET" "3"
    }
}

board Backup {
    .usb: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VBUS" }
    .cell: Rail { .voltage &= 3V..3.3V; .pos.net &= "VBAT" }
    .ldo: LinearRegulator { .vout.voltage &= 3.3V +/- 5% }
    .usb ~ .ldo.vin
    .ldo.vout.pos.net &= "3V3"
    .d: Schottky
    .cell.pos ~> .d ~> .ldo.vout.pos
    .cell.neg ~ .usb.neg
    .mon: Monitor { .vdd.current &= 1mA..5mA }
    .mon.vdd ~ .ldo.vout
    .r1: Resistor { .resistance &= 100kohm +/- 1% }
    .r2: Resistor { .resistance &= 100kohm +/- 1% }
    .usb.pos ~> .r1 ~> .mon.det.line
    .mon.det.line ~> .r2 ~> .usb.neg
    .j: Header1x2
    .j.p1 ~ .usb.pos
    .j.p2 ~ .usb.neg
    .k: Header1x2
    .k.p1 ~ .cell.pos
    .k.p2 ~ .cell.neg

    scenario backup {
        remove .usb
        source .cell
        sense .mon.det.line
    }
}
"#;

const KICAD: &str = r#"[[part]]
kind = "Monitor"
package = "SOT-23"
default = true
footprint = "gen:SOT23(pins=3)"
"#;

fn project(root: &Path, source: &str) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"backup\"\nnetsmith = \"0.5.0\"\n",
    )
    .unwrap();
    fs::write(root.join("kicad.toml"), KICAD).unwrap();
    fs::write(root.join("src/main.nsm"), source).unwrap();
}

fn netsmith(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(root)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .output()
        .unwrap()
}

fn text(run: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    )
}

#[test]
fn a_body_diode_back_feeds_the_input_and_the_sense_line_reads_neither_low_nor_high() {
    let root = TempDir::new("scenario-backfeed");
    project(&root, SOURCE);
    let build = netsmith(&root, &["board", "build", "--out", "out"]);
    let out = text(&build);
    assert!(!build.status.success(), "{out}");
    assert!(
        out.contains("error: in the scenario `backup`, `.mon.det.line` settles at")
            && out.contains("which the pin reads as neither low"),
        "{out}"
    );
    assert!(out.contains("sense .mon.det.line"), "{out}");
}

#[test]
fn a_regulator_that_blocks_keeps_the_input_off_and_the_note_lists_what_the_cell_powers() {
    let root = TempDir::new("scenario-blocked");
    let source = SOURCE.replacen(
        "    .ldo.vout.pos.net &= \"3V3\"\n",
        "    .ldo.vout.pos.net &= \"3V3\"\n    .ldo.reverse.path &= ReversePath::Blocking\n",
        1,
    );
    assert_ne!(source, SOURCE);
    project(&root, &source);
    let build = netsmith(&root, &["board", "build", "--out", "out"]);
    let out = text(&build);
    assert!(build.status.success(), "{out}");
    assert!(
        out.contains("note: scenario `backup`, with `.usb` removed; `.cell` (3V..3.3V) is blocked by `.ldo`; it powers `.mon.vdd` at"),
        "{out}"
    );
    assert!(
        out.contains("`.mon.det.line` settles at 0V, read as low by the pin."),
        "{out}"
    );
}
