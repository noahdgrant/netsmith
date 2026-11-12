//! Requirements on where parts go (ADR 0045), from the source to the package: the placer
//! keeps to them, the layout file keeps the solution, the check measures them on the board
//! file with every number, and a violation stops `fab` unless the project says `warn`.
//! The numbers are an invented case drawing.

use netsmith_kicad::testlib::TempDir;
use std::fs;

use netsmith_fab::KicadCliExe;

/// A 40 by 24 mm board whose supply header must sit on the left edge, turned, centred along
/// it, and whose LED must stay inside the right of the board.
const BOARD: &str = "\
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Widget {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }
    .led: Led {
        .current &= 5mA..15mA
    }
    .r1: Resistor {}
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
";

/// The case drawing of `BOARD`, in its physical file (ADR 0048).
const PHYSICAL: &str = "\
import std::pcb::Outline

physical for Widget {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
    }

    // Case drawing, slot A: the header's body is 1.5 mm in from the left edge, centred.
    .power_in.place.left &= 1.5mm +/- 0.1mm
    .power_in.place.y >= .outline.height / 2 - 0.1mm
    .power_in.place.y <= .outline.height / 2 + 0.1mm
    .power_in.place.rotation &= 90
    .led.place.right <= 38mm
    .led.place.top >= 3mm
}
";

fn out(name: &str) -> TempDir {
    TempDir::unused(&format!("constraints-test-{name}"))
}

fn have_libraries() -> bool {
    KicadCliExe::locate_runnable().is_some()
}

/// A layout file that moves the header by hand, without locking it.
const MOVED: &str = "version = 1\n[Widget.parts.power_in]\nat = [30.0, 5.0]\n";

/// A project whose manifest says `manifest` (a line, or none) and whose layout file moves the
/// header out of its window, run with `args`.
fn run_fab(name: &str, manifest: &str, args: &[&str]) -> std::process::Output {
    let root = out(name);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"widget\"\ntop_level = [\"src/main.nsm\"]\n{manifest}\n"),
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), BOARD).unwrap();
    fs::write(root.join("src/main.physical.nsm"), PHYSICAL).unwrap();
    fs::write(root.join("src/main.pcb.toml"), MOVED).unwrap();
    std::process::Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .arg("fab")
        .args(args)
        .args(["--out", "fab"])
        .current_dir(&root)
        .output()
        .unwrap()
}

#[test]
fn the_command_line_over_rules_the_manifest_both_ways() {
    if !have_libraries() {
        return;
    }
    let said = |run: &std::process::Output| String::from_utf8_lossy(&run.stderr).to_string();
    // The manifest says nothing: an error, as by default.
    let run = run_fab("cli-default", "", &[]);
    assert!(!run.status.success(), "{}", said(&run));
    assert!(said(&run).contains("error [constraint] constraint power_in.place.left"));
    // `--constraints warn` is a warning for one run.
    let run = run_fab("cli-warn", "", &["--constraints", "warn"]);
    assert!(run.status.success(), "{}", said(&run));
    assert!(said(&run).contains("warning [constraint] constraint power_in.place.left"));
    // The manifest says warn: a warning; `--constraints error` makes it an error again.
    let warn = "constraints = \"warn\"";
    let run = run_fab("manifest-warn", warn, &[]);
    assert!(run.status.success(), "{}", said(&run));
    let run = run_fab("manifest-warn-forced", warn, &["--constraints", "error"]);
    assert!(!run.status.success(), "{}", said(&run));
    assert!(said(&run).contains("error [constraint] constraint power_in.place.left"));
    // The manifest says error: `--constraints warn` wins.
    let run = run_fab(
        "manifest-error",
        "constraints = \"error\"",
        &["--constraints", "warn"],
    );
    assert!(run.status.success(), "{}", said(&run));
    // `--allow-drc-violations` does not help.
    let run = run_fab("cli-allow", "", &["--allow-drc-violations"]);
    assert!(!run.status.success(), "{}", said(&run));
    assert!(said(&run).contains("does not waive them"));
}
