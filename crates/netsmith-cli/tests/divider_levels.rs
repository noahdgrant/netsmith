//! `netsmith board check` on a board whose host drives a 1.8 V input through a two-resistor
//! divider (issue #1270): the input is judged at the divided levels, with the tolerances of both
//! resistors, and against its absolute maximum. `Host` and `Sensor` are invented; neither is a
//! real part.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

const SOURCE: &str = r#"
import std::logic::Logic
import std::parts::passive::Resistor
import std::sources::Rail

/// Invented: drives within 0.1 V of its supply.
part Host {
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

/// Invented: reads a high from 70 % of its supply and a low up to 30 %, and survives 2.1 V.
part Sensor {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 0.7 * .vdd.voltage
    .in.vil &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos power "VDD" "1"
        .vdd.neg power "GND" "2"
        .in.line input "IN"  "3" abs_max 2.1V
    }
}

board Shifted {
    .s3: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .s18: Rail {
        .voltage &= 1.8V +/- 5%
        .pos.net &= "1V8"
        .neg.net &= "GND"
    }
    .a: Host
    .b: Sensor
    .s3 ~ .a.vdd
    .s18 ~ .b.vdd
    .top: Resistor { .resistance &= 10kohm +/- 1% }
    .bottom: Resistor { .resistance &= 10kohm +/- 1% }
    .a.out.line ~> .top ~> .b.in.line
    .b.in.line ~> .bottom ~> .s3.neg
}
"#;

const KICAD: &str = r#"[[part]]
kind = "Host"
package = "SOT-23"
default = true
footprint = "gen:SOT23(pins=3)"

[[part]]
kind = "Sensor"
package = "SOT-23"
default = true
footprint = "gen:SOT23(pins=3)"
"#;

/// `netsmith board check --notes all` on a project of one source, and what it printed.
fn check(root: &Path, source: &str) -> std::process::Output {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        format!(
            "[project]\nname = \"shifted\"\nnetsmith = \"{}\"\n",
            env!("CARGO_PKG_VERSION")
        ),
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
fn a_divider_that_gives_a_high_and_stays_under_the_limit_passes_with_no_level_note() {
    let root = TempDir::new("divider-levels");
    let run = check(&root, SOURCE);
    let text = stderr(&run);
    assert!(run.status.success(), "{text}");
    assert!(!text.contains("comes through"), "{text}");
    assert!(!text.contains("error"), "{text}");
}

#[test]
fn a_divider_whose_high_is_too_low_is_an_error_with_the_divided_levels() {
    let root = TempDir::new("divider-levels-low");
    let source = SOURCE.replacen(
        ".bottom: Resistor { .resistance &= 10kohm",
        ".bottom: Resistor { .resistance &= 6.8kohm",
        1,
    );
    assert_ne!(source, SOURCE);
    let run = check(&root, &source);
    let stderr = stderr(&run);
    assert_eq!(run.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("net `net-b.in.line`: with `a.out` driving high, the divider of `top` from net `net-a.out.line` and `bottom` to ground `GND` holds the line at 1.213V..")
            && stderr.contains("but `b.in.vih` needs 1.197V..1.323V"),
        "{stderr}"
    );
}

#[test]
fn a_divider_that_lifts_the_pin_past_its_limit_is_an_error_with_the_divider() {
    let root = TempDir::new("divider-levels-max");
    let source = SOURCE.replacen(
        ".bottom: Resistor { .resistance &= 10kohm",
        ".bottom: Resistor { .resistance &= 20kohm",
        1,
    );
    assert_ne!(source, SOURCE);
    let run = check(&root, &source);
    let stderr = stderr(&run);
    assert_eq!(run.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("(the divider of `top` from net `net-a.out.line` and `bottom` to ground `GND`) but `b.in.line.abs_max` is 2.1V"),
        "{stderr}"
    );
}
