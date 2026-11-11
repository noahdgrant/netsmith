//! The PCB that `netsmith fab` writes for ORBIT (`examples/orbit/`) against the original's, as
//! the parity checker (`netsmith::parity`) judges it: the gate of the port's board stage. The
//! checker itself is tested in `rp2350_parity.rs`, on the RP2350A Minimal; this file only builds
//! ORBIT and gates on the must-match items of `reference/must_match.toml`.

use std::fs;
use std::path::PathBuf;

use netsmith::parity::{DesignatorMap, ParityReport, Tolerances, Verdict, check_text};
use netsmith::reference_table::Board;

use crate::common;

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/orbit")
}

/// The longest `fab` may route in the test, in seconds (see `rp2350_parity.rs`): the board file is
/// written either way, marked INCOMPLETE when the router is stopped, and the checker reads the
/// places of the parts, which the placer fixes before the router starts.
const ROUTE_SECONDS: &str = "20";

/// Builds ORBIT with `netsmith fab --locked` and no KiCad (the project needs none, ADR 0097) and
/// gates on the checker: the outline, the four plated holes, USB1, BAT1, J1, J2, SW1, SW2, BOOT0
/// and PWR1 must be `ok` on the board it writes, complete or INCOMPLETE; the committed overlay
/// must be the picture of that board.
#[test]
fn orbit_pcb_matches_the_original() {
    common::fab_matches_the_reference(&common::ParityGate {
        project: project(),
        tag: "orbit-parity-fab",
        parts: 48,
        min_ok: 125,
        route_seconds: ROUTE_SECONDS,
        regenerate: "from examples/orbit run `netsmith fab --locked`, then\n  cargo run -p \
                     netsmith --example parity -- fab/Orbit/Orbit.kicad_pcb --table \
                     reference/placements.toml --map reference/designators.toml --svg \
                     reference/parity-overlay.svg",
    });
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
