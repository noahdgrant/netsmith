//! Copper that touches end to edge (issue #1334): a track whose end lies on a pad's edge, or whose
//! round end runs into it, is joined to the pad, as KiCad's connectivity joins a track's end to
//! what lies nearer than half its width. A round end that only touches the edge is not joined, in
//! KiCad (9.0) as here, nor is one that misses by a hair, and the findings say so. The test that
//! runs KiCad skips itself where it is not installed.

use crate::common;

use common::*;
use netsmith_fab::compare_kicad_drc;
use netsmith_geom::{DrcExtras, DrcReport, DrcRule, drc_check};

/// Two pads of net A, 0.6 mm square at (10, 10) and (20, 10), and a track of width 0.2 from the
/// first's centre to `beyond` mm short of the second's left edge (at x = 19.7): 0 ends on the
/// edge, 0.1 (half the width) has the round end only touch it, and less runs into the pad.
fn board(beyond: f64) -> Pcb {
    let mut b = Pcb::new(2);
    b.stub("A", 10.0, 10.0);
    b.stub("A", 20.0, 10.0);
    b.track("A", "F.Cu", (10.0, 10.0), (19.7 - beyond, 10.0), 0.2);
    b
}

fn check(b: &mut Pcb) -> DrcReport {
    let (pcb, pro) = (b.pcb(), b.pro());
    drc_check(&pcb, Some(&pro), &DrcExtras::default()).unwrap_or_else(|e| panic!("{e}"))
}

const JOINED: [f64; 3] = [-0.1, 0.0, 0.05];

#[test]
fn a_track_whose_end_is_on_a_pads_edge_is_joined_and_does_not_dangle() {
    for beyond in JOINED {
        let r = check(&mut board(beyond));
        assert_eq!(r.count(DrcRule::UnconnectedItems), 0, "{beyond} mm: {r}");
        assert_eq!(r.count(DrcRule::TrackDangling), 0, "{beyond} mm: {r}");
    }
}

#[test]
fn a_track_whose_copper_only_touches_or_just_misses_a_pad_is_not_joined_and_the_findings_say_so() {
    for (beyond, says) in [(0.1, "only touches"), (0.105, "by 0.0050 mm")] {
        let r = check(&mut board(beyond));
        assert_eq!(r.count(DrcRule::UnconnectedItems), 1, "{beyond} mm: {r}");
        assert_eq!(r.count(DrcRule::TrackDangling), 1, "{beyond} mm: {r}");
        for f in &r.findings {
            if matches!(f.rule, DrcRule::UnconnectedItems | DrcRule::TrackDangling) {
                assert!(f.message.contains(says), "{beyond} mm: {}", f.message);
            }
        }
    }
    // Far off, nothing is said of a miss.
    let r = check(&mut board(1.0));
    assert!(
        r.findings
            .iter()
            .all(|f| !f.message.contains("misses") && !f.message.contains("only touches")),
        "{r}"
    );
}

#[test]
fn kicad_joins_and_parts_the_same_ends() {
    let mut runs = 0;
    for beyond in JOINED.into_iter().chain([0.1, 0.105]) {
        let mut b = board(beyond);
        let (pcb, pro) = (b.pcb(), b.pro());
        let Some(theirs) = kicad_findings("touching", &pcb, &pro) else {
            eprintln!("skipped: KiCad is not installed");
            return;
        };
        runs += 1;
        let ours = check(&mut b);
        let c = compare_kicad_drc(&ours, &theirs, 0.1);
        assert!(c.agrees(), "{beyond} mm short of the edge: {c}\nnetsmith:\n{ours}");
    }
    assert_eq!(runs, 5);
}
