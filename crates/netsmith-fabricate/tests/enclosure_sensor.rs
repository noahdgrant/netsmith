//! `examples/enclosure_sensor/`: an invented sensor board that states all its physical intent
//! in its physical file (outline with a cutout, holes and fiducials, a 40 mm plug that hangs
//! over an edge, an antenna and a coax connector on one line, two socket rows a fixed distance
//! apart, a module on the back with one part on the front, a row of test pads on a pitch, a
//! height limit), and goes through `fab` to `design rules check: passed`.
//!
//! The example is a project (it defines three invented parts and maps them to KiCad's stock
//! footprints in its `kicad.toml`), so it is fabricated with the project's own libraries. KiCad's
//! libraries are needed for that, so the test skips itself when KiCad is not installed.

use netsmith_pipeline::Libraries;
use netsmith_fabricate::fab::{FabOptions, fab_with_options};
use netsmith_check::Loader;
use netsmith_fab::KicadCliExe;
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;
use std::fs;
use std::path::PathBuf;

const CIRCUIT: &str = include_str!("../../../examples/enclosure_sensor/enclosure_sensor.nsm");
const PHYSICAL: &str =
    include_str!("../../../examples/enclosure_sensor/enclosure_sensor.physical.nsm");

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/enclosure_sensor")
}

/// The package of the example, made with `physical` as its physical file.
fn fab_example(
    name: &str,
    physical: &str,
) -> (TempDir, Result<Vec<netsmith_fabricate::fab::Fabricated>, String>) {
    let dir = TempDir::unused(&format!("fab-test-{name}"));
    let libraries = Libraries::for_project(Some(&project())).unwrap_or_else(|e| panic!("{e}"));
    let made = fab_with_options(
        "enclosure_sensor.nsm",
        CIRCUIT,
        &Loader::std().with_physical(
            "enclosure_sensor.nsm",
            "enclosure_sensor.physical.nsm",
            physical,
        ),
        &dir.join("fab"),
        &Catalogs::jlcpcb(),
        &libraries,
        None,
        &FabOptions::default(),
    );
    (dir, made)
}

#[test]
fn the_enclosure_sensor_example_meets_every_requirement_and_passes_the_design_rules_check() {
    if KicadCliExe::locate_runnable().is_none() {
        return;
    }
    let (dir, made) = fab_example("enclosure-sensor", PHYSICAL);
    let made = made.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(made.len(), 1);
    let notes = &made[0].notes;
    assert!(notes.contains("design rules check: passed"), "{notes}");
    // Every requirement of the physical file is measured on the board, and all are met.
    let counted = notes
        .lines()
        .find_map(|l| l.trim().strip_prefix("constraints: "))
        .unwrap_or_else(|| panic!("{notes}"));
    let (met, all) = counted
        .strip_suffix(" satisfied")
        .and_then(|c| c.split_once(" of "))
        .unwrap_or_else(|| panic!("{counted}"));
    assert_eq!(met, all, "{notes}");
    assert!(all.parse::<usize>().unwrap() >= 60, "{notes}");
    // The radio's pins are cut to protrude 0.2 mm and the flat cell lies across them: one
    // step for a person for each row, naming the cell that is fitted after.
    for row in ["radio.row_a", "radio.row_b"] {
        let trim = notes
            .lines()
            .find(|l| l.starts_with("trim: ") && l.contains(&format!("({row})")))
            .unwrap_or_else(|| panic!("a trim line for {row}: {notes}"));
        assert!(
            trim.contains("cut the pins to protrude 0.20 mm from the bottom face")
                && trim.contains("before BT1 (cell) is fitted")
                && trim.contains("enclosure_sensor.physical.nsm:"),
            "{trim}"
        );
    }
    // The plug's overhang is stated, so it is a warning and not an error.
    assert!(
        notes.contains("`plug` (J2) hangs over the left edge"),
        "the stated overhang of the sensor plug: {notes}"
    );
    // The parts of the board: about fifty, with the plug and its twelve test pads among them.
    let pcb =
        fs::read_to_string(dir.join("fab/EnclosureSensor/EnclosureSensor.kicad_pcb")).unwrap();
    let footprints = pcb.matches("(footprint \"").count();
    assert!((40..=60).contains(&footprints), "{footprints} footprints");
    assert_eq!(pcb.matches("TestPoint_Pad_D1.5mm").count(), 12);
    // The pads are numbered in natural order: `.pads.tp2` is TP2 and `tp10` follows `tp9`, so
    // the names of the physical file and the labels of the board agree.
    for n in 1..=12 {
        assert!(pcb.contains(&format!("\"TP{n}\"")), "TP{n} is on the board");
    }
    // The back-side module: three resistors and two LEDs are on the back, and its third LED is
    // the exception on the front; the flat cell is on the back too.
    let on_back = pcb
        .split("(footprint \"")
        .skip(1)
        .filter(|f| f.lines().take(3).any(|l| l.contains("(layer \"B.Cu\")")))
        .count();
    assert_eq!(on_back, 6);
}

#[test]
fn a_module_that_states_a_face_and_a_part_on_the_other_is_refused_naming_both_lines() {
    // The exception of intent 6 written the short way: the module's face, then one LED's own.
    let physical = PHYSICAL.replace(
        "    .lights.led_a.place.side &= \"bottom\"\n",
        "    .lights.place.side &= \"bottom\"\n",
    );
    assert_ne!(physical, PHYSICAL);
    let made = fab_example("enclosure-sensor-face", &physical).1;
    let err = made.expect_err("a module cannot state a face and have an exception");
    assert!(
        err.contains("`lights` is on the bottom face, which is a requirement on every part of it (stated at enclosure_sensor.physical.nsm:")
            && err.contains("but `lights.led_c.place.side` is \"top\"")
            && err.contains("state the faces of its parts one by one"),
        "{err}"
    );
}
