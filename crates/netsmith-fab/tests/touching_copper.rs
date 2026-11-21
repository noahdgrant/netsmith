//! Copper that touches end to edge (issue #1334): a track whose end lies on a pad's edge, or whose
//! round end runs into it, is joined to the pad, as KiCad's connectivity joins a track's end to
//! what lies nearer than half its width. A round end that only touches the edge is not joined, in
//! KiCad (9.0) as here, nor is one that misses by a hair, and the findings say so.
//!
//! A track that ends on the middle of another of a different width (issue #1542) follows the same
//! rule, whichever is the wider and from whatever side or angle it comes: it is joined while its
//! end lies nearer to the other's centre line than the two half widths together, and not once the
//! round end only touches the other's edge. An arc is the exception: KiCad counts its edge as on
//! it, so an end that only touches an arc's copper is joined, and the distance is the arc's own,
//! not its chords'. The tests that run KiCad skip themselves where it is not installed.

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

/// A run of net A from (10, 10) to (20, 10), `run` wide, and a track `end` wide from a pad of
/// the net 5 mm off, coming at `approach` degrees from the run's normal, whose end lies `d` mm
/// from the run's centre line at its middle, on the side `side` (1 above, -1 below).
fn tee(run: f64, end: f64, approach: f64, side: f64, d: f64) -> Pcb {
    let mut b = Pcb::new(2);
    b.wire("A", "F.Cu", (10.0, 10.0), (20.0, 10.0), run);
    let (sin, cos) = approach.to_radians().sin_cos();
    let to = (15.0, 10.0 + side * d);
    let from = (to.0 + 5.0 * sin, to.1 + side * 5.0 * cos);
    b.stub("A", from.0, from.1);
    b.track("A", "F.Cu", from, to, end);
    b
}

/// The tees of [`tee`], each with whether it is joined and what the findings say if it is not.
fn tees() -> Vec<(String, Pcb, Option<&'static str>)> {
    let mut out = Vec::new();
    for (run, end) in [(0.5, 0.2), (0.2, 0.5)] {
        let reach: f64 = (run + end) / 2.0;
        for (approach, side) in [(0.0, 1.0), (45.0, -1.0)] {
            let cases = [
                (0.0, None),
                (run / 2.0, None),
                (reach - 0.0005, None),
                (reach, Some("only touches")),
                (reach + 0.005, Some("by 0.0050 mm")),
            ];
            for (d, says) in cases {
                out.push((
                    format!("a {end} mm track at {approach} degrees, {d:.4} mm off a {run} mm one"),
                    tee(run, end, approach, side, d),
                    says,
                ));
            }
        }
    }
    out
}

/// An arc of net A from (10, 10) over (15, 15) to (20, 10), `run` wide (its centre (15, 10),
/// radius 5), and a straight track `end` wide coming at it along a radius at `deg` degrees from
/// the top, whose end lies `d` mm outside the arc's centre line (`side` 1) or inside it (-1).
fn arc_tee(run: f64, end: f64, deg: f64, side: f64, d: f64) -> Pcb {
    let mut b = Pcb::new(2);
    b.stub("A", 10.0, 10.0);
    b.stub("A", 20.0, 10.0);
    b.arc("A", "F.Cu", [(10.0, 10.0), (15.0, 15.0), (20.0, 10.0)], run);
    let (sin, cos) = deg.to_radians().sin_cos();
    let at = |r: f64| (15.0 + r * sin, 10.0 + r * cos);
    let from = at(5.0 + side * 3.0);
    b.stub("A", from.0, from.1);
    b.track("A", "F.Cu", from, at(5.0 + side * d), end);
    b
}

/// The arc tees of [`arc_tee`] and whether each is joined. 7 degrees is between two points of
/// the arc's chords, where they lie furthest inside it.
fn arc_tees() -> Vec<(String, Pcb, bool)> {
    let mut out = Vec::new();
    for (run, end) in [(0.5, 0.2), (0.2, 0.5)] {
        let reach: f64 = (run + end) / 2.0;
        for deg in [0.0, 7.0] {
            for side in [1.0, -1.0] {
                for (d, joined) in [(reach - 0.002, true), (reach, true), (reach + 0.0005, false)]
                {
                    out.push((
                        format!(
                            "a {end} mm track {d:.4} mm {} a {run} mm arc at {deg} degrees",
                            if side > 0.0 { "outside" } else { "inside" }
                        ),
                        arc_tee(run, end, deg, side, d),
                        joined,
                    ));
                }
            }
        }
    }
    out
}

#[test]
fn a_tee_of_two_widths_is_joined_while_the_round_end_runs_into_the_other_track() {
    for (name, mut b, says) in tees() {
        let r = check(&mut b);
        let n = usize::from(says.is_some());
        assert_eq!(r.count(DrcRule::UnconnectedItems), n, "{name}: {r}");
        assert_eq!(r.count(DrcRule::TrackDangling), n, "{name}: {r}");
        if let Some(says) = says {
            for f in &r.findings {
                if matches!(f.rule, DrcRule::UnconnectedItems | DrcRule::TrackDangling) {
                    assert!(f.message.contains(says), "{name}: {}", f.message);
                }
            }
        }
    }
}

#[test]
fn a_track_whose_round_end_touches_an_arcs_edge_is_joined_and_one_a_hair_off_is_not() {
    for (name, mut b, joined) in arc_tees() {
        let r = check(&mut b);
        let n = usize::from(!joined);
        assert_eq!(r.count(DrcRule::UnconnectedItems), n, "{name}: {r}");
        assert_eq!(r.count(DrcRule::TrackDangling), n, "{name}: {r}");
    }
}

#[test]
fn kicad_joins_and_parts_the_same_tees() {
    let mut runs = 0;
    let cases = tees();
    let total = cases.len();
    for (name, mut b, _) in cases {
        let (pcb, pro) = (b.pcb(), b.pro());
        let Some(theirs) = kicad_findings("tee", &pcb, &pro) else {
            eprintln!("skipped: KiCad is not installed");
            return;
        };
        runs += 1;
        let ours = check(&mut b);
        let c = compare_kicad_drc(&ours, &theirs, 0.1);
        assert!(c.agrees(), "{name}: {c}\nnetsmith:\n{ours}");
    }
    assert_eq!(runs, total);
}

#[test]
fn kicad_joins_and_parts_the_same_ends_on_an_arc() {
    let mut runs = 0;
    let cases = arc_tees();
    let total = cases.len();
    for (name, mut b, _) in cases {
        let (pcb, pro) = (b.pcb(), b.pro());
        let Some(theirs) = kicad_findings("arc-tee", &pcb, &pro) else {
            eprintln!("skipped: KiCad is not installed");
            return;
        };
        runs += 1;
        let ours = check(&mut b);
        let c = compare_kicad_drc(&ours, &theirs, 0.1);
        assert!(c.agrees(), "{name}: {c}\nnetsmith:\n{ours}");
    }
    assert_eq!(runs, total);
}
