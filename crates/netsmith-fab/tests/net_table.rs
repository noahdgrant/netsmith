//! The per-net table of a routed board (issue #1223), measured on boards drawn by hand: each
//! net's lengths, vias, layer changes and layers, and each diff pair's lengths, skew, coupled
//! length, gaps and vias side by side, and each group of nets matched in length (issue #1234).

use crate::common;

use common::*;
use netsmith_core::LengthGroup;
use netsmith_fab::{NetRoutes, NetTable, Uncounted};
use netsmith_geom::Artwork;

/// A pair `P`/`N` from x = 10 to x = 30: `P` at y = 10, `N` at y = 10.3 (0.3 mm pitch, 0.1 mm
/// gap with 0.2 mm tracks) on the front from 10 to 20, through a via pair side by side to the
/// back at 20, and on to 30, where `N` jogs out to y = 11 for its last 2 mm and runs 1 mm on
/// (`N` is 1 mm longer, with the jog's diagonal). `GND` is poured; `S` runs alone with no via.
fn board() -> Pcb {
    let mut b = Pcb::new(2);
    b.stub("P", 10.0, 10.0);
    b.stub("P", 30.0, 10.0);
    b.track("P", "F.Cu", (10.0, 10.0), (20.0, 10.0), 0.2);
    b.via("P", (20.0, 10.0), 0.5, 0.2);
    b.track("P", "B.Cu", (20.0, 10.0), (30.0, 10.0), 0.2);
    b.stub("N", 10.0, 10.3);
    b.stub("N", 31.0, 11.0);
    b.track("N", "F.Cu", (10.0, 10.3), (20.0, 10.3), 0.2);
    // The via of N stands below P's, 0.6 mm apart centre to centre: their copper 0.1 mm apart.
    b.via("N", (20.0, 10.6), 0.5, 0.2);
    b.track("N", "F.Cu", (20.0, 10.3), (20.0, 10.6), 0.2);
    b.track("N", "B.Cu", (20.0, 10.6), (20.3, 10.3), 0.2);
    b.track("N", "B.Cu", (20.3, 10.3), (28.0, 10.3), 0.2);
    b.track("N", "B.Cu", (28.0, 10.3), (28.7, 11.0), 0.2);
    b.track("N", "B.Cu", (28.7, 11.0), (31.0, 11.0), 0.2);
    b.stub("S", 5.0, 20.0);
    b.stub("S", 9.0, 23.0);
    b.track("S", "F.Cu", (5.0, 20.0), (9.0, 23.0), 0.2);
    b.stub("GND", 25.0, 20.0);
    b.via("GND", (22.0, 22.0), 0.5, 0.2);
    b.zone("GND", "B.Cu", (20.0, 15.0, 39.0, 29.0));
    b
}

fn table(b: &mut Pcb, limit: f64) -> NetTable {
    let pairs = [("P".to_string(), "N".to_string(), limit)];
    NetTable::of(&Artwork::parse(&b.pcb()).unwrap(), &pairs, &[])
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn each_net_has_its_lengths_vias_layer_changes_and_layers() {
    let t = table(&mut board(), 0.1);
    let names: Vec<&str> = t.nets.iter().map(|n| n.net.as_str()).collect();
    // A poured net is no connection of tracks: left out, as from the list of long nets.
    assert_eq!(names, ["N", "P", "S"]);
    let p = &t.nets[1];
    assert!(close(p.length, 20.0) && close(p.straight, 20.0), "{p:?}");
    assert_eq!((p.vias, p.layer_changes), (1, 1));
    assert_eq!(p.layers, ["F.Cu", "B.Cu"]);
    let s = &t.nets[2];
    assert!(close(s.length, 5.0) && close(s.ratio, 1.0), "{s:?}");
    assert_eq!((s.vias, s.layer_changes), (0, 0));
    assert_eq!(s.layers, ["F.Cu"]);
    // The long nets' list reads the same numbers.
    let routes = NetRoutes::of(&Artwork::parse(&board().pcb()).unwrap());
    assert!(routes.long.is_empty(), "{routes:?}");
}

#[test]
fn a_pair_has_its_skew_coupled_length_gaps_and_vias_side_by_side() {
    let t = table(&mut board(), 0.1);
    assert_eq!(t.pairs.len(), 1);
    let pair = &t.pairs[0];
    let n_length = 10.0 + 0.3 + 0.3f64.hypot(0.3) + 7.7 + 0.7f64.hypot(0.7) + 2.3;
    assert!(close(pair.length_p, 20.0), "{pair:?}");
    assert!(close(pair.length_n, n_length), "{pair:?}");
    assert!(close(pair.skew, n_length - 20.0) && pair.over, "{pair:?}");
    // Coupled on the front from 10 to 20 and on the back from 20.3 to about 28.3, where N
    // jogs out past 0.5 mm: at most 18 mm of P's 20, at least the 10 + 7.7 of the straight runs.
    assert!(
        pair.coupled > 17.5 && pair.coupled < 18.5,
        "{:.2} mm coupled",
        pair.coupled
    );
    assert!(close(pair.share, pair.coupled / 20.0));
    // On the straight runs the gap is the pitch less the widths: 0.1 mm; the jogs open it.
    let (least, most) = pair.gaps.unwrap();
    assert!(close(least, 0.1), "{least}");
    assert!(most > 0.1 && most <= 0.3 + 1e-9, "{most}");
    assert_eq!((pair.vias_p, pair.vias_n, pair.via_pairs), (1, 1, 1));
    let lines = t.over_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("pair skew: P/N: 20 and "), "{lines:?}");
    // A larger limit has the pair within it, and the notes say nothing.
    let t = table(&mut board(), 2.0);
    assert!(!t.pairs[0].over && t.over_lines().is_empty());
    let text = t.text();
    assert!(text.contains("pair P/N: 20.00 and "), "{text}");
    assert!(text.contains("within the 2 mm limit"), "{text}");
    assert!(text.contains("vias 1 and 1, 1 side by side"), "{text}");
}

#[test]
fn vias_of_a_pair_far_apart_are_not_side_by_side_and_the_nets_sort_by_length() {
    let mut b = board();
    b.via("P", (25.0, 10.0), 0.5, 0.2);
    b.via("N", (25.0, 14.0), 0.5, 0.2);
    let mut t = table(&mut b, 0.1);
    let pair = &t.pairs[0];
    assert_eq!((pair.vias_p, pair.vias_n, pair.via_pairs), (2, 2, 1));
    t.by_length();
    let names: Vec<&str> = t.nets.iter().map(|n| n.net.as_str()).collect();
    assert_eq!(names, ["N", "P", "S"]);
}

#[test]
fn a_length_group_has_each_nets_length_and_its_spread_against_its_tolerance() {
    let group = |tolerance: f64| LengthGroup {
        path: "lanes".into(),
        // `GND` is poured and `X` has no track: neither is a net of the group's row.
        nets: vec!["P".into(), "S".into(), "GND".into(), "X".into()],
        tolerance,
    };
    let art = Artwork::parse(&board().pcb()).unwrap();
    let t = NetTable::of(&art, &[], &[group(1.0)]);
    assert_eq!(t.groups.len(), 1);
    let g = &t.groups[0];
    let names: Vec<&str> = g.nets.iter().map(|n| n.net.as_str()).collect();
    assert_eq!(names, ["P", "S"]);
    assert!(close(g.spread, 15.0) && close(g.nets[1].short_by, 15.0), "{g:?}");
    assert!(g.over, "{g:?}");
    let lines = t.over_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("length group: lanes: 2 nets, spread 15 mm (S the shortest), over its tolerance of 1 mm"),
        "{lines:?}"
    );
    assert!(t.text().contains("group lanes: P 20.00, S 5.00 mm, spread 15.00 mm (over the 1 mm tolerance)"), "{}", t.text());
    let t = NetTable::of(&art, &[], &[group(20.0)]);
    assert!(!t.groups[0].over && t.over_lines().is_empty());
    // A group with one net with copper has nothing to match.
    let lone = LengthGroup {
        nets: vec!["P".into(), "X".into()],
        ..group(1.0)
    };
    assert!(NetTable::of(&art, &[], &[lone]).groups.is_empty());
}

#[test]
fn a_track_path_sums_its_nets_lengths_against_its_budget() {
    let path = |budget: Option<f64>| netsmith_core::TrackPath {
        path: "rf".into(),
        // `X` has no track: it counts 0 mm.
        nets: vec!["P".into(), "S".into(), "X".into()],
        parts: vec!["c1".into(), "r1".into()],
        ohms: 50.0,
        budget_length: budget,
        budget_loss: None,
        frequency: None,
        corner: netsmith_core::TrackCorner::Rounded,
        fence: None,
    };
    let art = Artwork::parse(&board().pcb()).unwrap();
    let t = NetTable::of(&art, &[], &[]).with_paths(&[path(Some(20.0))], &[]);
    let p = &t.paths[0];
    assert!(close(p.length, 25.0) && p.over, "{p:?}");
    assert_eq!(p.nets.len(), 3);
    let lines = t.over_lines();
    assert_eq!(
        lines,
        ["track path: rf: 3 nets, 25 mm of track, over its length budget of 20 mm; move the parts of the path nearer each other"]
    );
    assert!(
        t.text().contains("path rf: P 20.00, S 5.00, X 0.00 mm, 25.00 mm in all (over the 20 mm budget)"),
        "{}",
        t.text()
    );
    let t = NetTable::of(&art, &[], &[]).with_paths(&[path(Some(30.0))], &[]);
    assert!(!t.paths[0].over && t.over_lines().is_empty());
    // No budget: the length is said and not judged.
    let t = NetTable::of(&art, &[], &[]).with_paths(&[path(None)], &[]);
    assert!(!t.paths[0].over && t.text().contains("25.00 mm in all\n"));
}

#[test]
fn a_track_paths_loss_is_judged_against_its_loss_budget() {
    let path = netsmith_core::TrackPath {
        path: "rf".into(),
        nets: vec!["P".into(), "S".into()],
        parts: vec!["c1".into()],
        ohms: 50.0,
        budget_length: None,
        budget_loss: Some(0.5),
        frequency: Some(2.4e9),
        corner: netsmith_core::TrackCorner::Rounded,
        fence: None,
    };
    let art = Artwork::parse(&board().pcb()).unwrap();
    let paths = [path.clone()];
    let t = NetTable::of(&art, &[], &[]).with_paths(&paths, &[Some(Ok(0.75))]);
    let p = &t.paths[0];
    assert!(p.loss_over && !p.over && p.loss == Some(0.75), "{p:?}");
    assert_eq!(
        t.over_lines(),
        ["track path: rf: 0.750 dB of track loss at 2.4 GHz, over its loss budget of 0.5 dB; shorten the path or widen its tracks on a thicker dielectric"]
    );
    assert!(
        t.text().contains("; 0.750 dB at 2.4 GHz (over the 0.5 dB budget)"),
        "{}",
        t.text()
    );
    let t = NetTable::of(&art, &[], &[]).with_paths(&paths, &[Some(Ok(0.25))]);
    assert!(!t.paths[0].loss_over && t.over_lines().is_empty());
    assert!(t.text().contains("(within the 0.5 dB budget)"), "{}", t.text());
    // No figure: the reason is said, and nothing is judged.
    let why = "build-up X records no loss tangent".to_string();
    let t = NetTable::of(&art, &[], &[]).with_paths(&paths, &[Some(Err(why))]);
    assert!(!t.paths[0].loss_over && t.over_lines().is_empty());
    assert!(
        t.text().contains("; no loss at 2.4 GHz: build-up X records no loss tangent"),
        "{}",
        t.text()
    );
    // No frequency: the budget asks for one.
    let t = NetTable::of(&art, &[], &[]).with_paths(&paths, &[None]);
    assert!(t.text().contains("its 0.5 dB loss budget needs a `.frequency`"), "{}", t.text());
}

/// The pair of [`board`] with a second way of `P` (issue #1487): from (12, 10) a track runs down
/// to y = 8 on the front, 6 mm along and back up to (18, 10), beside `P`'s own run, as a person's
/// duplicate of one connection does; and a dead stub of `N` of 1.5 mm from (15, 10.3) on the
/// front, which leads to no pad.
fn board_with_a_second_way() -> Pcb {
    let mut b = board();
    b.track("P", "F.Cu", (12.0, 10.0), (12.0, 8.0), 0.2);
    b.track("P", "F.Cu", (12.0, 8.0), (18.0, 8.0), 0.2);
    b.track("P", "F.Cu", (18.0, 8.0), (18.0, 10.0), 0.2);
    b.track("N", "F.Cu", (15.0, 10.3), (15.0, 11.8), 0.2);
    b
}

#[test]
fn a_second_way_and_a_dead_stub_are_not_counted_in_a_nets_length_or_a_pairs_skew() {
    let plain = table(&mut board(), 2.0);
    let t = table(&mut board_with_a_second_way(), 2.0);
    let p = t.nets.iter().find(|n| n.net == "P").unwrap();
    // The connection is the 20 mm from pad to pad; the 10 mm way round is copper not counted.
    assert!(close(p.length, 20.0) && close(p.copper, 30.0), "{p:?}");
    assert_eq!(p.not_counted.len(), 1, "{p:?}");
    let way = &p.not_counted[0];
    assert_eq!((way.kind, way.layer.as_str()), (Uncounted::SecondWay, "F.Cu"));
    assert!(close(way.length, 10.0), "{way:?}");
    // Its pieces are the copper it is (issue #1495): they add up to its length, and one starts
    // where it is said to be.
    let pieces: f64 = way
        .pieces
        .iter()
        .map(|s| ((s.to.0 - s.from.0).powi(2) + (s.to.1 - s.from.1).powi(2)).sqrt())
        .sum();
    assert!(close(pieces, way.length), "{way:?}");
    assert!(way.pieces.iter().any(|s| s.from == way.at) && way.pieces.iter().all(|s| s.width > 0.0));
    assert!(close(p.uncounted(Uncounted::SecondWay), 10.0));
    let n = t.nets.iter().find(|n| n.net == "N").unwrap();
    let plain_n = plain.nets.iter().find(|n| n.net == "N").unwrap();
    assert!(close(n.length, plain_n.length), "{n:?}");
    assert_eq!(n.not_counted.len(), 1, "{n:?}");
    let stub = &n.not_counted[0];
    assert_eq!(stub.kind, Uncounted::Stub);
    assert!(close(stub.length, 1.5) && close(n.uncounted(Uncounted::Stub), 1.5));
    // The pair is as it was: the second way and the stub change neither length, so the skew is
    // that of the board without them, and the second way runs coupled with nothing.
    let (pair, before) = (&t.pairs[0], &plain.pairs[0]);
    assert!(close(pair.length_p, 20.0) && close(pair.skew, before.skew), "{pair:?}");
    assert!(close(pair.coupled, before.coupled), "{pair:?} {before:?}");
    // The text and the JSON say which copper is left out.
    let text = t.text();
    assert!(
        text.contains("net P: 10.00 mm of its 30.00 mm of copper not counted in its length: a second way of 10.00 mm on F.Cu at ("),
        "{text}"
    );
    assert!(text.contains("net N: 1.50 mm of its "), "{text}");
    assert!(text.contains("a dead stub of 1.50 mm on F.Cu"), "{text}");
    assert!(
        text.contains("(the lengths leave out copper of P and N that is not part of its connection: see its net)"),
        "{text}"
    );
    let json = serde_json::to_value(&t).unwrap();
    let p = json["nets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["net"] == "P")
        .unwrap();
    assert_eq!(p["not_counted"][0]["kind"], "second_way");
    assert_eq!(p["copper"], 30.0);
    assert!(p["not_counted"][0]["pieces"][0]["width"].as_f64().unwrap() > 0.0);
    // A net with nothing left out says nothing of it.
    assert!(plain.nets.iter().all(|n| n.not_counted.is_empty()));
    assert!(!plain.text().contains("not counted"), "{}", plain.text());
}

#[test]
fn a_net_of_three_pads_counts_the_branches_its_pads_need_joined_at_a_tee() {
    // Pads at (0, 0), (10, 0) and (5, 6): a trunk from the first to the second on the front,
    // and a branch that tees into the trunk's middle up to the third, through a via to the back.
    let mut b = Pcb::new(2);
    b.stub("T", 0.0, 0.0);
    b.stub("T", 10.0, 0.0);
    b.stub("T", 5.0, 6.0);
    b.track("T", "F.Cu", (0.0, 0.0), (10.0, 0.0), 0.2);
    b.track("T", "F.Cu", (5.0, 0.0), (5.0, 3.0), 0.2);
    b.via("T", (5.0, 3.0), 0.5, 0.2);
    b.track("T", "B.Cu", (5.0, 3.0), (5.0, 4.0), 0.2);
    b.via("T", (5.0, 4.0), 0.5, 0.2);
    b.track("T", "F.Cu", (5.0, 4.0), (5.0, 6.0), 0.2);
    let t = NetTable::of(&Artwork::parse(&b.pcb()).unwrap(), &[], &[]);
    let row = &t.nets[0];
    assert!(close(row.length, 16.0) && close(row.copper, 16.0), "{row:?}");
    assert!(row.not_counted.is_empty(), "{row:?}");
}

#[test]
fn a_track_inside_a_pad_is_counted_once_and_the_runs_add_up_to_the_copper_less_the_length() {
    // Pads at (0, 0) and (6, 6), a track from one to the other, and at each end the short track
    // a hand-routed board draws inside the pad to its centre, from a point the long track's
    // copper touches but its centre line misses (issue #1511): each was listed twice, as a dead
    // stub, and the runs added up to more than the copper less the length. A dead stub of 1 mm
    // tees into the middle.
    let mut b = Pcb::new(2);
    b.stub("L", 0.0, 0.0);
    b.stub("L", 6.0, 6.0);
    b.track("L", "F.Cu", (0.0, 0.0), (6.0, 6.0), 0.15);
    b.track("L", "F.Cu", (0.01, 0.0), (0.0, 0.0), 0.15);
    b.track("L", "F.Cu", (6.0125, 5.95), (6.0, 6.0), 0.15);
    b.track("L", "F.Cu", (3.0, 3.0), (3.0, 4.0), 0.15);
    let t = NetTable::of(&Artwork::parse(&b.pcb()).unwrap(), &[], &[]);
    let row = &t.nets[0];
    let inside = 0.01 + (0.0125f64.powi(2) + 0.05f64.powi(2)).sqrt();
    assert!(close(row.copper, 72f64.sqrt() + inside + 1.0), "{row:?}");
    // The copper inside the pads is the way into them: counted, and no run.
    assert!(close(row.length, 72f64.sqrt() + inside), "{row:?}");
    assert_eq!(row.not_counted.len(), 1, "{row:?}");
    let stub = &row.not_counted[0];
    assert_eq!((stub.kind, stub.pieces.len()), (Uncounted::Stub, 1), "{stub:?}");
    assert!(close(stub.length, 1.0), "{stub:?}");
    let runs: f64 = row.not_counted.iter().map(|r| r.length).sum();
    assert!(close(runs, row.copper - row.length), "{row:?}");
    assert!(t.text().contains("net L: 1.00 mm of its "), "{}", t.text());
}

#[test]
fn a_short_track_that_leads_to_no_pad_is_one_stub_and_the_runs_add_up() {
    // The short track of the test above, at a point no pad covers: it touches the long track's
    // copper, so it is one dead stub, not a copy of itself on the long track.
    let mut b = Pcb::new(2);
    b.stub("L", 0.0, 0.0);
    b.stub("L", 6.0, 0.0);
    b.track("L", "F.Cu", (0.0, 0.0), (6.0, 0.0), 0.15);
    b.track("L", "F.Cu", (3.0, 0.05), (3.0, 2.0), 0.15);
    let t = NetTable::of(&Artwork::parse(&b.pcb()).unwrap(), &[], &[]);
    let row = &t.nets[0];
    assert!(close(row.length, 6.0) && close(row.copper, 7.95), "{row:?}");
    assert_eq!(row.not_counted.len(), 1, "{row:?}");
    assert!(close(row.not_counted[0].length, 1.95), "{row:?}");
    let runs: f64 = row.not_counted.iter().map(|r| r.length).sum();
    assert!(close(runs, row.copper - row.length), "{row:?}");
}

/// A pair `P`/`N` from x = 10 to x = 30, 20 mm for `P`; `N` runs 2 mm below it, straight, or
/// with a detour of 0.5 mm down and back that makes it 21 mm, 1 mm out of skew.
fn two_lines(detour: bool) -> Pcb {
    let mut b = Pcb::new(2);
    b.wire("P", "F.Cu", (10.0, 10.0), (30.0, 10.0), 0.2);
    b.stub("N", 10.0, 12.0);
    b.stub("N", 30.0, 12.0);
    if detour {
        for (a, z) in [
            ((10.0, 12.0), (15.0, 12.0)),
            ((15.0, 12.0), (15.0, 12.5)),
            ((15.0, 12.5), (25.0, 12.5)),
            ((25.0, 12.5), (25.0, 12.0)),
            ((25.0, 12.0), (30.0, 12.0)),
        ] {
            b.track("N", "F.Cu", a, z, 0.2);
        }
    } else {
        b.track("N", "F.Cu", (10.0, 12.0), (30.0, 12.0), 0.2);
    }
    b
}

fn skew_findings(detour: bool, strict: bool) -> (netsmith_fab::Checked, Vec<netsmith_geom::DrcFinding>) {
    let mut b = two_lines(detour);
    let options = netsmith_fab::Options {
        pairs: vec![("P".to_string(), "N".to_string(), 0.1)],
        drc: netsmith_geom::DrcExtras {
            strict_guidelines: strict,
            ..Default::default()
        },
        ..Default::default()
    };
    let checked = netsmith_fab::check(&b.pcb(), Some(&b.pro()), &options).unwrap();
    let found = checked
        .report
        .findings
        .iter()
        .filter(|f| f.rule == netsmith_geom::DrcRule::PairSkew)
        .cloned()
        .collect();
    (checked, found)
}

#[test]
fn a_pair_1_mm_out_of_skew_is_a_pair_skew_finding_and_a_matched_pair_is_none() {
    let (checked, found) = skew_findings(true, false);
    assert_eq!(found.len(), 1, "{:?}", checked.report.findings);
    let f = &found[0];
    assert_eq!(f.severity, netsmith_geom::DrcSeverity::Warning);
    assert!(
        f.message.contains("P/N")
            && f.message.contains("20 and 21 mm")
            && f.message.contains("skew of 1 mm")
            && f.message.contains("limit of 0.1 mm"),
        "{}",
        f.message
    );
    let nets: Vec<_> = f.items.iter().filter_map(|i| i.net.as_deref()).collect();
    assert_eq!(nets, ["P", "N"]);
    // A warning does not fail the check; the pair stays in the table's list too.
    assert!(checked.report.is_clean(), "{:?}", checked.report.findings);
    assert_eq!(checked.skewed.len(), 1);
    assert!(f.to_string().starts_with("warning [pair_skew] diff pair P/N"));

    let (checked, found) = skew_findings(false, false);
    assert!(found.is_empty() && checked.skewed.is_empty(), "{found:?}");
}

#[test]
fn with_strict_guidelines_a_pair_over_its_skew_is_an_error() {
    let (checked, found) = skew_findings(true, true);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].severity, netsmith_geom::DrcSeverity::Error);
    assert!(!checked.report.is_clean());

/// Net B: a pad 0.5 by 0.36 mm at (0, 0), stubs at (5, 0) and (0, 3), a track of 0.2 mm from the
/// pad's centre to (5, 0), and a branch up to (0, 3) that starts at `branch`: at (0, 0.2) it is
/// 0.1 mm off the track's copper and 0.02 mm outside the pad's edge, its round end running into
/// the pad (issue #1582).
fn board_with_a_branch(branch: (f64, f64)) -> Pcb {
    let mut b = Pcb::new(2);
    let d = Draw {
        courtyard: None,
        silk: None,
        back: false,
    };
    b.part("P1", (0.0, 0.0), 0.0, &[smd("1", 0.0, 0.0, 0.5, 0.36, "B")], d);
    b.stub("B", 5.0, 0.0);
    b.stub("B", 0.0, 3.0);
    b.track("B", "F.Cu", (0.0, 0.0), (5.0, 0.0), 0.2);
    b.track("B", "F.Cu", branch, (0.0, 3.0), 0.2);
    b
}

#[test]
fn a_branch_joined_to_the_track_through_a_pads_copper_is_counted_as_the_design_rules_check_joins_it()
 {
    let mut b = board_with_a_branch((0.0, 0.2));
    let (pcb, pro) = (b.pcb(), b.pro());
    // The design rules check finds the branch joined: its round end runs into the pad.
    let r = netsmith_geom::drc_check(&pcb, Some(&pro), &netsmith_geom::DrcExtras::default())
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(r.count(netsmith_geom::DrcRule::UnconnectedItems), 0, "{r}");
    let t = NetTable::of(&Artwork::parse(&pcb).unwrap(), &[], &[]);
    let row = &t.nets[0];
    // So the length counts it all: 5 mm of track and 2.8 mm of branch.
    assert!(close(row.length, 7.8) && close(row.copper, 7.8), "{row:?}");
    assert!(row.not_counted.is_empty(), "{row:?}");
}

#[test]
fn a_branch_joined_to_nothing_is_still_a_dead_stub() {
    // The branch starts at (0, 0.6), clear of the pad and the track: the design rules check
    // finds it apart, and the length leaves it out as a stub.
    let mut b = board_with_a_branch((0.0, 0.6));
    let (pcb, pro) = (b.pcb(), b.pro());
    let r = netsmith_geom::drc_check(&pcb, Some(&pro), &netsmith_geom::DrcExtras::default())
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(r.count(netsmith_geom::DrcRule::UnconnectedItems), 1, "{r}");
    let t = NetTable::of(&Artwork::parse(&pcb).unwrap(), &[], &[]);
    let row = &t.nets[0];
    assert!(close(row.length, 5.0) && close(row.copper, 7.4), "{row:?}");
    assert_eq!(row.not_counted.len(), 1, "{row:?}");
    assert_eq!(row.not_counted[0].kind, Uncounted::Stub);
    assert!(close(row.not_counted[0].length, 2.4), "{row:?}");
}

#[test]
fn a_branch_whose_round_end_reaches_a_track_is_joined_to_it_and_a_stair_is_not_cut_short() {
    // Net C: stubs at (0, 0), (6, 0) and (3, 3); a trunk of 0.2 mm from the first to the second,
    // and a branch to the third that starts at (3, 0.15), off the trunk's copper but with its
    // round end in it (the check's rule): the branch tees into the trunk.
    let mut b = Pcb::new(2);
    b.stub("C", 0.0, 0.0);
    b.stub("C", 6.0, 0.0);
    b.stub("C", 3.0, 3.0);
    b.track("C", "F.Cu", (0.0, 0.0), (6.0, 0.0), 0.2);
    b.track("C", "F.Cu", (3.0, 0.15), (3.0, 3.0), 0.2);
    let t = NetTable::of(&Artwork::parse(&b.pcb()).unwrap(), &[], &[]);
    let row = &t.nets[0];
    assert!(close(row.length, 8.85) && row.not_counted.is_empty(), "{row:?}");
    // A stair of tracks 0.05 mm long, each end within the next but one's round end: it is
    // joined end to end already, so no tee cuts across it and its length is all its copper.
    let mut b = Pcb::new(2);
    b.stub("D", 0.0, 0.0);
    b.stub("D", 2.0, 0.0);
    b.track("D", "F.Cu", (0.0, 0.0), (0.95, 0.0), 0.2);
    for k in 0..2 {
        let x = 0.95 + 0.05 * k as f64;
        b.track("D", "F.Cu", (x, 0.0), (x + 0.05, 0.0), 0.2);
    }
    b.track("D", "F.Cu", (1.05, 0.0), (2.0, 0.0), 0.2);
    let t = NetTable::of(&Artwork::parse(&b.pcb()).unwrap(), &[], &[]);
    let row = &t.nets[0];
    assert!(close(row.length, 2.0) && row.not_counted.is_empty(), "{row:?}");
}
