//! The two differences that the cross-check once reported against KiCad 9.0.0 (issue #13),
//! on an invented board (`tests/data/known_differences.kicad_pcb`): round pads with thermal
//! spokes in a plane, and pads that state their own solder mask margin, on both faces.

use netsmith_fab::crosscheck::{Limits, check};
use netsmith_fab::{KicadCli, KicadCliExe};

const BOARD: &str = include_str!("data/known_differences.kicad_pcb");
const PROJECT: &str = include_str!("data/known_differences.kicad_pro");

fn board_in(name: &str) -> (netsmith_testkit::TempDir, std::path::PathBuf) {
    let dir = netsmith_testkit::TempDir::new(name);
    let pcb = dir.join("known_differences.kicad_pcb");
    std::fs::write(&pcb, BOARD).unwrap();
    std::fs::write(dir.join("known_differences.kicad_pro"), PROJECT).unwrap();
    (dir, pcb)
}

#[test]
fn the_plane_agrees_with_kicads_to_a_tenth_of_what_the_default_limit_allows() {
    let Some(kicad) = KicadCliExe::locate_runnable() else {
        eprintln!("skipped: no KiCad");
        return;
    };
    let (_dir, pcb) = board_in("fab-known-spokes");
    // Spokes along the axes of a round pad, which is what netsmith drew before, make this
    // plane differ by 0.6% of its pixels (0.5% beyond the tolerance); with KiCad's 45
    // degrees the difference is the islands KiCad removes and nothing else.
    let limits = Limits {
        pixels: 0.002,
        pour_beyond_tolerance: 0.001,
        ..Limits::default()
    };
    let report = check(&pcb, &kicad, &limits).unwrap_or_else(|e| panic!("{e}"));
    let plane = report
        .layers
        .iter()
        .find(|l| l.file.ends_with("In1_Cu.gbr"))
        .unwrap();
    assert!(plane.ok && plane.kind == "pour", "{report}");
}

#[test]
fn a_pad_margin_that_kicad_9_0_0_drops_on_the_back_is_named_and_does_not_fail() {
    let Some(kicad) = KicadCliExe::locate_runnable() else {
        eprintln!("skipped: no KiCad");
        return;
    };
    let (_dir, pcb) = board_in("fab-known-mask");
    let report = check(&pcb, &kicad, &Limits::default()).unwrap_or_else(|e| panic!("{e}"));
    assert!(report.ok(), "{report}");
    let back = report
        .layers
        .iter()
        .find(|l| l.file.ends_with("B_Mask.gbr"))
        .unwrap();
    let version = kicad.version().unwrap();
    if version.trim() == "9.0.0" {
        assert!(back.ok && back.pixels < 0.001, "{report}");
        // Both pads that state a margin of their own (a through-hole pad of J1 and a pad
        // of R1 on the back) are named; the pad that takes its footprint's is not.
        assert_eq!(report.notes.len(), 1, "{report}");
        let note = &report.notes[0];
        assert!(
            note.contains("J1.3 (0.15 mm)") && note.contains("R1.1 (0.1 mm)"),
            "{note}"
        );
        assert!(!note.contains("J1.1") && !note.contains("R1.2"), "{note}");
    } else {
        // A release that reads the margin for both faces needs no allowance, and none is made.
        // Its back mask differs by 0.13% of the pixels on 9.0.8 (issue #104), inside the
        // cross-check's own limits, which `back.ok` and `report.ok()` apply.
        assert!(back.ok, "{report}");
        assert!(report.notes.is_empty(), "KiCad {version}: {report}");
    }
}
