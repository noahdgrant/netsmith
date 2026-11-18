//! `netsmith board check` on a board whose transceiver has its mode pins strapped to the supply
//! and to ground (issue #1304): the pins are judged at the rail's voltage and at 0 V, and no note
//! says their levels were not evaluated or names the passives on the ground. `Transceiver` and
//! `Driver` are invented; neither is a real part.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

const SOURCE: &str = r#"
import std::logic::{Levels, Logic}
import std::parts::passive::Resistor
import std::sources::Rail

/// Invented: a radio transceiver whose mode pins are strapped by the board, one table of
/// levels for its I/O pins, and a power-down input with levels of its own.
part Transceiver {
    .vdd: Rail
    .levels: Levels
    .levels.reference ~ .vdd
    .levels.voh &= .vdd.voltage - 0.4V
    .levels.vol &= 0.4V
    .levels.vih &= 0.7 * .vdd.voltage
    .levels.vil &= 0.3 * .vdd.voltage
    .pd: Logic
    .pd.reference ~ .vdd
    .pd.reference.current &= 0A
    .pd.vih &= 0.9 * .vdd.voltage
    pins {
        .vdd.pos  power  "VDD"  "1"
        .vdd.neg  power  "GND"  "2"
        .mode0    input  "M0"   "3" optional
        .mode1    input  "M1"   "4" optional
        .pd.line  input  "PD"   "5" optional
        .irq      output "IRQ"  "6"
    }
}

/// Invented: drives within 0.1 V of its supply.
part Driver {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.voh &= .vdd.voltage - 0.1V
    .out.vol &= 0.1V
    pins {
        .vdd.pos  power  "VDD" "1"
        .vdd.neg  power  "GND" "2"
        .out.line output "OUT" "3"
    }
}

board Radio {
    .s3: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .a: Driver
    .s3 ~ .a.vdd
    .down: Resistor { .resistance &= 10kohm +/- 1% }
    .up: Resistor { .resistance &= 10kohm +/- 1% }
    .a.out.line ~> .down ~> .s3.neg
    .s3.pos ~> .up ~> .a.out.line
    .u: Transceiver
    .s3 ~ .u.vdd
    .u.mode0 ~ .s3.neg
    .u.mode1 ~ .s3.pos
    .u.pd.line ~ .s3.pos
}
"#;

const KICAD: &str = r#"[[part]]
kind = "Driver"
package = "SOT-23"
default = true
footprint = "gen:SOT23(pins=3)"

[[part]]
kind = "Transceiver"
package = "SOT-23-6"
default = true
footprint = "gen:SOT23(pins=6)"
"#;

/// `netsmith board check --notes all` on a project of one source, and what it printed.
fn check(root: &Path, source: &str) -> std::process::Output {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"radio\"\nnetsmith = \"0.4.0\"\n",
    )
    .unwrap();
    fs::write(root.join("kicad.toml"), KICAD).unwrap();
    fs::write(root.join("src/main.nsm"), source).unwrap();
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["board", "check", "--notes", "all"])
        .current_dir(root)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .output()
        .unwrap()
}

fn stderr(run: &std::process::Output) -> String {
    String::from_utf8_lossy(&run.stderr).to_string()
}

#[test]
fn strapped_control_inputs_give_no_level_notes() {
    let root = TempDir::new("strapped-levels");
    let run = check(&root, SOURCE);
    let text = stderr(&run);
    assert!(run.status.success(), "{text}");
    assert!(!text.contains("not evaluated"), "{text}");
    assert!(!text.contains("error"), "{text}");
}

#[test]
fn a_mode_pin_strapped_to_a_rail_too_low_for_a_high_is_an_error_with_the_rail() {
    let root = TempDir::new("strapped-levels-low");
    let source = SOURCE
        .replacen(
            "    .a: Driver\n",
            "    .s18: Rail {\n        .voltage &= 1.8V +/- 5%\n        .pos.net &= \"1V8\"\n        .neg.net &= \"GND\"\n    }\n    .a: Driver\n",
            1,
        )
        .replacen(".u.mode1 ~ .s3.pos", ".u.mode1 ~ .s18.pos", 1);
    assert_ne!(source, SOURCE);
    let run = check(&root, &source);
    let stderr = stderr(&run);
    assert_eq!(run.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("net `1V8`: rail `s18` holds the line at 1.71V..1.89V but `u.levels.vih` needs 2.194V..2.426V"),
        "{stderr}"
    );
}
