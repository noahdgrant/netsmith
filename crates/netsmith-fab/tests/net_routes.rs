//! The nets a board's routing made long and the nets with the most vias (issue #1194), measured
//! on boards drawn by hand: what the scorecard, the design rules check's notes and
//! `fab-notes.txt` list.

use crate::common;

use common::*;
use netsmith_fab::{BomEntry, LONG_RATIO, NetRoutes, Options, Scorecard};
use netsmith_geom::Artwork;

/// A crystal's two lines: XIN straight from (10, 10) to (14, 10), 4 mm; XOUT from (10, 20) to
/// (14, 20), 4 mm apart, routed with a detour of 10 mm down and back when `detour`, through two
/// vias and the back. GND is poured and has vias of its own; SW has three vias and its tracks
/// run straight.
fn board(detour: bool) -> Pcb {
    let mut b = Pcb::new(2);
    b.wire("XIN", "F.Cu", (10.0, 10.0), (14.0, 10.0), 0.2);
    b.stub("XOUT", 10.0, 20.0);
    b.stub("XOUT", 14.0, 20.0);
    if detour {
        b.track("XOUT", "F.Cu", (10.0, 20.0), (10.0, 25.0), 0.2);
        b.via("XOUT", (10.0, 25.0), 0.5, 0.2);
        b.track("XOUT", "B.Cu", (10.0, 25.0), (14.0, 25.0), 0.2);
        b.via("XOUT", (14.0, 25.0), 0.5, 0.2);
        b.track("XOUT", "F.Cu", (14.0, 25.0), (14.0, 20.0), 0.2);
    } else {
        b.track("XOUT", "F.Cu", (10.0, 20.0), (14.0, 20.0), 0.2);
    }
    b.stub("SW", 25.0, 5.0);
    b.stub("SW", 35.0, 5.0);
    b.track("SW", "F.Cu", (25.0, 5.0), (28.0, 5.0), 0.2);
    b.via("SW", (28.0, 5.0), 0.5, 0.2);
    b.track("SW", "B.Cu", (28.0, 5.0), (30.0, 5.0), 0.2);
    b.via("SW", (30.0, 5.0), 0.5, 0.2);
    b.track("SW", "F.Cu", (30.0, 5.0), (32.0, 5.0), 0.2);
    b.via("SW", (32.0, 5.0), 0.5, 0.2);
    b.track("SW", "B.Cu", (32.0, 5.0), (35.0, 5.0), 0.2);
    b.stub("GND", 25.0, 20.0);
    for k in 0..6 {
        b.via("GND", (22.0 + k as f64 * 2.0, 27.0), 0.5, 0.2);
    }
    b.zone("GND", "B.Cu", (20.0, 15.0, 39.0, 29.0));
    b
}

fn routes(b: &mut Pcb) -> NetRoutes {
    NetRoutes::of(&Artwork::parse(&b.pcb()).unwrap())
}

#[test]
fn a_net_routed_far_over_its_straight_length_is_listed_with_its_numbers() {
    let r = routes(&mut board(true));
    assert_eq!(r.long.len(), 1, "{r:?}");
    let x = &r.long[0];
    assert_eq!(x.net, "XOUT");
    assert!((x.length - 14.0).abs() < 1e-9, "{x:?}");
    assert!((x.straight - 4.0).abs() < 1e-9, "{x:?}");
    assert!((x.ratio - 3.5).abs() < 1e-9 && x.ratio > LONG_RATIO, "{x:?}");
    assert_eq!(x.vias, 2);
    let lines = r.lines();
    assert!(
        lines[0].starts_with("routed long: 1 net routed more than 2x the straight length between its pads and 2 mm over it"),
        "{lines:?}"
    );
    assert_eq!(lines[1], "  XOUT: 14 mm routed, straight 4 mm (3.5x), 2 vias");
}

#[test]
fn the_nets_with_the_most_vias_leave_out_a_poured_net_and_a_net_of_one_via() {
    let r = routes(&mut board(true));
    let most: Vec<(&str, usize)> = r.most_vias.iter().map(|n| (n.net.as_str(), n.vias)).collect();
    // GND has six vias, but a pour's vias are stitching, put there on purpose.
    assert_eq!(most, [("SW", 3), ("XOUT", 2)]);
    let line = r.lines().pop().unwrap();
    assert_eq!(
        line,
        "most vias (nets no zone pours, 2 or more): SW 3 vias (10 mm routed, straight 10 mm); XOUT 2 vias (14 mm routed, straight 4 mm)"
    );
}

#[test]
fn a_board_routed_straight_has_no_long_net() {
    let r = routes(&mut board(false));
    assert!(r.long.is_empty(), "{r:?}");
    assert_eq!(r.most_vias.len(), 1, "SW alone: {r:?}");
    assert!(!r.lines().iter().any(|l| l.starts_with("routed long")));
}

#[test]
fn the_scorecard_and_the_check_list_the_same_nets() {
    let mut b = board(true);
    let (pcb, pro) = (b.pcb(), b.pro());
    let want = NetRoutes::of(&Artwork::parse(&pcb).unwrap());
    let card = Scorecard::of(&pcb, Some(&pro)).unwrap();
    assert_eq!(card.nets, want);
    let text = card.text();
    assert!(
        text.contains("\n  XOUT: 14 mm routed, straight 4 mm (3.5x), 2 vias\n"),
        "{text}"
    );
    let json = serde_json::to_value(&card).unwrap();
    assert_eq!(json["nets"]["long"][0]["net"], "XOUT");
    assert_eq!(json["nets"]["long"][0]["vias"], 2);
    assert_eq!(json["nets"]["most_vias"][0]["net"], "SW");
    let checked = netsmith_fab::check(&pcb, Some(&pro), &Options::default()).unwrap();
    assert_eq!(checked.routes, want);
}

#[test]
fn the_notes_of_the_package_list_them() {
    let mut b = board(true);
    let dir = netsmith_testkit::TempDir::new("net-routes-package");
    let pcb = dir.join("crystal.kicad_pcb");
    std::fs::write(&pcb, b.pcb()).unwrap();
    std::fs::write(pcb.with_extension("kicad_pro"), b.pro()).unwrap();
    let parts = netsmith_fab::Board::parse(&std::fs::read_to_string(&pcb).unwrap()).unwrap();
    let entries: Vec<BomEntry> = parts
        .parts
        .iter()
        .map(|p| BomEntry {
            designators: vec![p.designator.clone()],
            value: "pad".into(),
            footprint: "Test".into(),
            lcsc: None,
        })
        .collect();
    let options = Options {
        allow_missing_lcsc: true,
        // The hand-drawn GND pad joins its pour on the other layer by nothing: not this test's.
        allow_drc_violations: true,
        ..Options::default()
    };
    let pkg = netsmith_fab::package(&pcb, &entries, &options).unwrap();
    let notes = &pkg.notes;
    assert!(notes.contains("\nrouted long: 1 net routed more than 2x"), "{notes}");
    assert!(
        notes.contains("\n  XOUT: 14 mm routed, straight 4 mm (3.5x), 2 vias\n"),
        "{notes}"
    );
    assert!(notes.contains("\nmost vias (nets no zone pours, 2 or more): SW 3 vias"), "{notes}");
    assert_eq!(pkg.routes, routes(&mut board(true)));
}
