//! The per-net table of a routed board (issue #1223), measured on boards drawn by hand: each
//! net's lengths, vias, layer changes and layers, and each diff pair's lengths, skew, coupled
//! length, gaps and vias side by side.

use crate::common;

use common::*;
use netsmith_fab::{NetRoutes, NetTable};
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
    let pairs = [("P".to_string(), "N".to_string())];
    NetTable::of(&Artwork::parse(&b.pcb()).unwrap(), &pairs, limit)
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
