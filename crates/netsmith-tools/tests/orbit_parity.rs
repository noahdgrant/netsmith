//! The PCB that `netsmith fab` writes for ORBIT (`examples/orbit/`) against the original's, as
//! the parity checker (`netsmith_tools::parity`) judges it: the gate of the port's board stage. The
//! checker itself is tested in `rp2350_parity.rs`, on the RP2350A Minimal; this file only builds
//! ORBIT and gates on the must-match items of `reference/must_match.toml`.

use std::fs;
use std::path::PathBuf;

use netsmith_tools::parity::{DesignatorMap, ParityReport, Tolerances, Verdict, check_text};
use netsmith_tools::reference_table::Board;

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/orbit")
}

fn reference_file() -> (String, String) {
    let dir = project().join("reference");
    (
        fs::read_to_string(dir.join("placements.toml")).unwrap(),
        fs::read_to_string(dir.join("ORBIT.kicad_pcb")).unwrap(),
    )
}

fn check(table: &str, pcb: &str) -> ParityReport {
    let map = DesignatorMap::identity(&Board::from_table(table).unwrap());
    check_text(table, pcb, &map, &Tolerances::default()).unwrap_or_else(|e| panic!("{e}"))
}

/// ORBIT's own board file, a real KiCad 7 file whose footprints state their references as
/// `fp_text`, passes the checker against its table: the outline, the four plated holes (read
/// from `MountingHole_*_Pad_Via` as holes, by place and drill) and the twelve must-match parts.
#[test]
fn orbits_board_passes_against_itself() {
    let (table, pcb) = reference_file();
    let report = check(&table, &pcb);
    assert!(report.passed(), "{}", report.text());
    let (ok, failed) = report.must_match_counts();
    assert_eq!(failed, 0);
    assert!(ok > 125, "{ok} items\n{}", report.text());
    for id in [
        "hole H1",
        "hole H3",
        "part USB1 position",
        "part J2 pad 8 net",
    ] {
        let found = report.items.iter().find(|i| i.id == id).unwrap_or_else(|| {
            panic!("no item {id}:\n{}", report.text());
        });
        assert_eq!(found.verdict, Verdict::Ok, "{id}");
    }
}

/// A plated mounting hole is judged by its place and its drill. Its ring of vias is not required
/// (the owner's list), and so a board that has the drilled pad alone, as `fab`'s does, passes; a
/// hole that moved by a tenth of a millimetre, or has another drill, fails as a hole.
#[test]
fn a_plated_hole_is_judged_by_place_and_drill_and_not_by_its_ring_of_vias() {
    let (table, pcb) = reference_file();

    let without_vias: String = pcb
        .lines()
        .filter(|l| {
            !(l.contains("(size 0.8 0.8) (drill 0.5)") || l.contains("(size 0.7 0.7) (drill 0.4)"))
        })
        .map(|l| format!("{l}\n"))
        .collect();
    assert!(without_vias.len() < pcb.len(), "the vias were not found");
    let report = check(&table, &without_vias);
    assert!(report.passed(), "{}", report.text());

    let moved = pcb.replacen(
        "(at 170 74)\n    (descr \"Mounting Hole 2.5mm\")",
        "(at 170.1 74)\n    (descr \"Mounting Hole 2.5mm\")",
        1,
    );
    assert_ne!(moved, pcb);
    let report = check(&table, &moved);
    assert!(!report.passed());
    let hole = report.items.iter().find(|i| i.id == "hole H1").unwrap();
    assert_eq!(hole.verdict, Verdict::Fail, "{}", report.text());

    let wider = without_vias.replacen("(size 5 5) (drill 2.5)", "(size 5 5) (drill 2.6)", 1);
    assert_ne!(wider, without_vias);
    let report = check(&table, &wider);
    assert!(!report.passed());
    assert!(
        report
            .items
            .iter()
            .any(|i| i.verdict == Verdict::Fail && i.id.starts_with("hole H")),
        "{}",
        report.text()
    );
}

/// ORBIT's board file with the four plated mounting holes drawn as the standard non-plated
/// `MountingHole` footprints draw them: one `np_thru_hole` pad of the drill's size, and no ring
/// of vias.
fn with_standard_holes(pcb: &str) -> String {
    let mut out = String::new();
    for line in pcb.lines() {
        if line.contains("(size 0.8 0.8) (drill 0.5)") {
            continue;
        }
        let line = if line.contains("(pad \"1\" thru_hole circle (at 0 0) (size 5 5) (drill 2.5)") {
            "    (pad \"\" np_thru_hole circle (at 0 0) (size 2.5 2.5) (drill 2.5) (layers \"*.Cu\" \"*.Mask\"))"
        } else if line.contains("(pad \"1\" thru_hole circle (at 0 0) (size 4.4 4.4) (drill 2.2)") {
            "    (pad \"\" np_thru_hole circle (at 0 0) (size 2.2 2.2) (drill 2.2) (layers \"*.Cu\" \"*.Mask\"))"
        } else {
            line
        };
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// The original's holes are plated pads of twice the drill. A board whose holes are the standard
/// non-plated `MountingHole` ones (same places, same drills) is not the same board, and the
/// checker says so on each of the four holes.
#[test]
fn standard_non_plated_mounting_holes_fail_where_the_original_has_plated_ones() {
    let (table, pcb) = reference_file();
    let standard = with_standard_holes(&pcb);
    assert_ne!(standard, pcb);
    // The mutated file reads as four holes of the same places and drills.
    let board = netsmith_tools::reference_table::read_board(&standard, "standard").unwrap();
    let holes: Vec<_> = board
        .holes
        .iter()
        .filter(|h| h.reference.starts_with('H'))
        .collect();
    assert_eq!(holes.len(), 4);
    assert!(holes.iter().all(|h| !h.plated));
    let report = check(&table, &standard);
    assert!(!report.passed());
    for hole in ["H1", "H2", "H3", "H4"] {
        let item = report
            .items
            .iter()
            .find(|i| i.id == format!("hole {hole}"))
            .unwrap();
        assert_eq!(item.verdict, Verdict::Fail, "{hole}\n{}", report.text());
        assert!(item.note.contains("plated"), "{}", report.text());
    }
    // The control: the original's own file passes, so the four failures are the holes' doing.
    assert!(check(&table, &pcb).passed());
}

/// The size of a hole's pad is compared: H1's 2.5 mm drill with a 2.6 mm pad, where the original's
/// is 5 mm, fails, and the drill, the place and the plating being the original's.
#[test]
fn a_hole_with_the_right_drill_and_a_pad_of_the_wrong_size_fails() {
    let (table, pcb) = reference_file();
    let at = pcb.find("(at 170 74)").expect("H1");
    let from = at + pcb[at..].find("(size 5 5) (drill 2.5)").expect("H1's pad");
    let mut narrow = pcb.clone();
    narrow.replace_range(from..from + "(size 5 5)".len(), "(size 2.6 2.6)");
    let report = check(&table, &narrow);
    assert!(!report.passed());
    let h1 = report.items.iter().find(|i| i.id == "hole H1").unwrap();
    assert_eq!(h1.verdict, Verdict::Fail, "{}", report.text());
    assert!(h1.note.contains("pad"), "{}", report.text());
    // Nothing but H1's hole failed.
    let failed: Vec<&str> = report.failures().map(|i| i.id.as_str()).collect();
    assert_eq!(failed, ["hole H1"], "{}", report.text());
}
