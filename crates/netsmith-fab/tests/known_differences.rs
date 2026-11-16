//! The two differences that the cross-check once reported against KiCad 9.0.0 (issue #13),
//! on an invented board (`tests/data/known_differences.kicad_pcb`): round pads with thermal
//! spokes in a plane, and pads that state their own solder mask margin, on both faces; and the
//! copper KiCad's fill trims from a plane (issue #837), on another.

use netsmith_fab::{CrossCheckLimits, cross_check, KicadCli, KicadCliExe};

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
    // degrees, and the copper islands removed as KiCad removes them, it is within a tenth of
    // the default limit.
    let limits = CrossCheckLimits {
        pixels: 0.002,
        pour_beyond_tolerance: 0.001,
        ..CrossCheckLimits::default()
    };
    let report = cross_check(&pcb, &kicad, &limits).unwrap_or_else(|e| panic!("{e}"));
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
    let report = cross_check(&pcb, &kicad, &CrossCheckLimits::default()).unwrap_or_else(|e| panic!("{e}"));
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

const ISLANDS_BOARD: &str = include_str!("data/pour_islands.kicad_pcb");

/// A plane on an invented board (`tests/data/pour_islands.kicad_pcb`) that KiCad's fill trims
/// (issue #837): a ring of another net's tracks round 52 mm² of it (a copper island, 14% of the
/// plane), two tracks 1 mm apart that leave a strip 0.2 mm wide (under the 0.25 mm minimum
/// thickness), and a track whose clearance takes the end of a pad's spoke. netsmith's fill kept
/// all three before, which put the plane far outside the limit.
#[test]
fn a_plane_whose_island_sliver_and_dead_spoke_kicad_removes_agrees_with_kicads() {
    let Some(kicad) = KicadCliExe::locate_runnable() else {
        eprintln!("skipped: no KiCad");
        return;
    };
    let dir = netsmith_testkit::TempDir::new("fab-pour-islands");
    let pcb = dir.join("pour_islands.kicad_pcb");
    std::fs::write(&pcb, ISLANDS_BOARD).unwrap();
    std::fs::write(dir.join("pour_islands.kicad_pro"), PROJECT).unwrap();
    let report =
        cross_check(&pcb, &kicad, &CrossCheckLimits::default()).unwrap_or_else(|e| panic!("{e}"));
    let plane = report
        .layers
        .iter()
        .find(|l| l.file.ends_with("In1_Cu.gbr"))
        .unwrap();
    assert!(plane.ok && plane.kind == "pour", "{report}");
}

/// The same board's plane as netsmith fills it, with no KiCad: the island, the strip and the
/// spoke's stub are gone, and the plane round them is there.
#[test]
fn the_island_sliver_and_dead_spoke_of_the_plane_are_not_in_netsmiths_fill() {
    use netsmith_geom::{Artwork, Polygons, Pt, edge_outline, fill_pour, plot_rules_of};
    let art = Artwork::parse(ISLANDS_BOARD).unwrap();
    let edge = edge_outline(&art).unwrap();
    let mut pen = Polygons::new();
    fill_pour(&mut pen, &art, "In1.Cu", &edge, &plot_rules_of(Some(PROJECT))).unwrap();
    let areas = pen.areas();
    let copper = |x: f64, y: f64| {
        areas.iter().any(|a| {
            netsmith_geom::polygon_contains(&a.outer, Pt::new(x, y))
                && !a.holes.iter().any(|h| netsmith_geom::polygon_contains(h, Pt::new(x, y)))
        })
    };
    assert!(!copper(13.0, 9.0), "the island inside the ring");
    assert!(!copper(12.0, 16.5), "the strip between the two tracks");
    assert!(!copper(5.05, 10.0), "the stub of the spoke into the track's clearance");
    assert!(copper(2.75, 10.0), "the spoke on the other side");
    assert!(copper(12.0, 3.0) && copper(12.0, 18.5), "the plane");
}
