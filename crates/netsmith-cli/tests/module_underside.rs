//! A module's underside of several depths (issue #1726): `layout move`, `layout fit` and
//! `board fab` refuse a part under the module where it stands taller than the clearance there
//! (the depth, plus the solder allowance, less the margin) and accept it where it does not.
//!
//! The board is invented: a 40 by 20 mm outline, a "module" (a resistor that stands for one)
//! at its centre whose body covers 20 by 10 mm, with a pocket 1.5 mm deep on its left half and a
//! bare area 0.3 mm deep on its right half, lifted 0.1 mm by its solder and with 0.2 mm kept
//! clear; and `low`, 1.2 mm tall, which fits in the pocket and not under the bare area.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use netsmith_testkit::TempDir;
use serde_json::Value;

const BOARD: &str = "\
import std::parts::passive::Resistor
import std::pcb::UndersideKeepout
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Carrier {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .rail: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }
    .module: Resistor {
        .resistance &= 15kohm +/- 1%
        .underside_keepout: UndersideKeepout {
            .x &= -10mm
            .y &= -5mm
            .width &= 10mm
            .height &= 10mm
            .max_height &= 1.5mm
            .solder_allowance &= 0.1mm
            .margin &= 0.2mm
        }
        .underside_keepout_bare: UndersideKeepout {
            .x &= 0mm
            .y &= -5mm
            .width &= 10mm
            .height &= 10mm
            .max_height &= 0.3mm
            .solder_allowance &= 0.1mm
            .margin &= 0.2mm
        }
    }
    .low: Resistor { .resistance &= 15kohm +/- 1% }
    .rail.pos ~> .module ~> .rail.neg
    .rail.pos ~> .low ~> .rail.neg
    .module.height &= 4mm
    .low.height &= 1.2mm
}
";

const PHYSICAL: &str = "\
import std::pcb::Outline

physical for Carrier {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 20mm
        }
    }
    .low.place.side &= \"top\"
}
";

/// The module at the centre, `low` right of its body.
const LAYOUT: &str = "\
version = 1

[Carrier]
seed = 0

[Carrier.parts.module]
at = [20.0, 10.0]
rotation = 0.0
side = \"top\"
lock = true
source = \"hand\"

[Carrier.parts.low]
at = [35.0, 10.0]
rotation = 0.0
side = \"top\"
lock = true
source = \"hand\"
";

/// `fab`'s sentence for `low` under the bare area.
const TOO_TALL: &str = "stands 1.20 mm tall on the top face, and the underside keepout `.underside_keepout_bare` of `module` allows 0.20 mm there (1.00 mm too tall)";

fn project(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"carrier\"\ntop_level = [\"main.nsm\"]\n",
    )
    .unwrap();
    fs::write(dir.join("main.nsm"), BOARD).unwrap();
    fs::write(dir.join("main.physical.nsm"), PHYSICAL).unwrap();
    fs::write(dir.join("main.pcb.toml"), LAYOUT).unwrap();
    dir
}

fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn said(run: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    )
}

fn json(dir: &Path, args: &[&str]) -> Value {
    let run = netsmith(dir, args);
    serde_json::from_slice(&run.stdout).unwrap_or_else(|e| panic!("{args:?}: {e}: {}", said(&run)))
}

#[test]
fn a_move_under_the_shallow_area_is_refused_and_one_into_the_pocket_is_not() {
    let dir = project("underside-move");
    let before = fs::read_to_string(dir.join("main.pcb.toml")).unwrap();
    // Left by 9 mm: x = 26, under the bare area.
    let run = netsmith(&dir, &["layout", "move", "low", "-9,0"]);
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    assert!(said(&run).contains(TOO_TALL), "{}", said(&run));
    assert_eq!(fs::read_to_string(dir.join("main.pcb.toml")).unwrap(), before);
    // Left by 21 mm: x = 14, in the pocket, where 1.4 mm is left.
    let run = netsmith(&dir, &["layout", "move", "low", "-21,0"]);
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    let doc = json(&dir, &["layout", "check", "--format", "json"]);
    let about: Vec<&Value> = doc["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["path"] == "low" && f["severity"] != "note")
        .collect();
    assert!(about.is_empty(), "{about:?}");
}

#[test]
fn fit_offers_the_pocket_and_no_place_under_the_shallow_area_and_says_why() {
    let dir = project("underside-fit");
    // Near a point under the bare area: the nearest place offered is not under it, and the
    // places refused nearer say why in `fab`'s words.
    let doc = json(
        &dir,
        &["layout", "fit", "low", "--near", "25,10", "--as-laid-out", "--format", "json"],
    );
    let offered: Vec<&Value> = doc["poses"].as_array().unwrap().iter().filter(|p| p["fits"] == true).collect();
    assert!(!offered.is_empty(), "{doc}");
    for pose in &offered {
        let x = pose["at"][0].as_f64().unwrap();
        assert!(!(20.0..30.0).contains(&x) || pose["at"][1].as_f64().unwrap() > 15.0 || pose["at"][1].as_f64().unwrap() < 5.0, "{pose}");
        let tall = pose["refusals"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["kind"] == "too_tall")
            .unwrap_or_else(|| panic!("{pose}"));
        assert!(tall["text"].as_str().unwrap().contains(TOO_TALL), "{pose}");
    }
    // Near a point in the pocket: a place there fits.
    let doc = json(
        &dir,
        &["layout", "fit", "low", "--near", "15,10", "--as-laid-out", "--format", "json"],
    );
    let nearest = doc["poses"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["fits"] == true)
        .map(|p| p["distance"].as_f64().unwrap())
        .fold(f64::INFINITY, f64::min);
    assert!(nearest < 1.0, "{doc}");
}

#[test]
fn fab_refuses_the_part_under_the_shallow_area_in_the_same_words() {
    let dir = project("underside-fab");
    let set = netsmith(&dir, &["layout", "set", "low", "--at", "26,10", "--lock"]);
    assert!(set.status.success(), "{}", said(&set));
    let fab = netsmith(&dir, &["board", "fab", "main.nsm", "--out", "fab"]);
    assert_ne!(fab.status.code(), Some(0), "{}", said(&fab));
    assert!(said(&fab).contains(TOO_TALL), "{}", said(&fab));
    // In the pocket, `fab` finds nothing too tall.
    let set = netsmith(&dir, &["layout", "set", "low", "--at", "14,10", "--lock"]);
    assert!(set.status.success(), "{}", said(&set));
    let fab = netsmith(&dir, &["board", "fab", "main.nsm", "--out", "fab"]);
    assert!(!said(&fab).contains("too tall"), "{}", said(&fab));
}
