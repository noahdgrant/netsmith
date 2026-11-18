//! Thermal spokes counted as KiCad's refill leaves them (issue #1115): a GND pad between two
//! other nets' pads whose clearances leave only a strip of fill beside it, narrower than the
//! pour's minimum thickness, gets no spokes to the sides, as KiCad's filler makes none there.
//! netsmith's design rules check then reports the same `starved_thermal` pads as KiCad's does
//! after KiCad refills the zones (when KiCad is installed; the comparison is skipped where it
//! is not, and runs in the `kicad` job). The report of each pad's join to its pour (issue #1187,
//! `netsmith_geom::pad_joins`, which `board fill` prints) counts the same pads of the same fill.

use crate::common;

use common::*;
use netsmith_geom::{DrcExtras, DrcRule, Pt, drc_check};

/// The GND pads of the board, by name and centre.
const PADS: [(&str, (f64, f64)); 3] = [("A", (8.0, 15.0)), ("B", (20.0, 8.0)), ("C", (8.0, 6.0))];

/// How far the other nets' pads of a row are from its GND pad, in mm: their clearances (0.3 mm
/// round a 0.6 mm wide pad) start 0.95 mm from the GND pad's centre, and its relief (0.5 mm round
/// it) ends 0.8 mm from it, so the fill between is 0.15 mm wide, under the minimum of 0.25 mm.
const PITCH: f64 = 1.55;

/// The board of the issue's check: a 30 x 20 mm board with a GND pour on F.Cu (gap and spokes
/// 0.5 mm, clearance 0.3 mm, minimum thickness 0.25 mm, netsmith's defaults) whose lower edge
/// runs 0.85 mm below pad A, a GND via that joins the pour to something besides the pads, and
/// three 0.6 x 1 mm GND pads:
///
/// - A, between a SIG1 and a SIG2 pad (each net a pad alone), with the pour's edge below it: one spoke (up) reaches the
///   fill, a starved thermal relief;
/// - B, between a SIG3 and a SIG4 pad in the open pour: two spokes (up and down), enough;
/// - C, alone: four.
fn board() -> Pcb {
    let mut b = Pcb::new(2);
    b.outline = (0.0, 0.0, 30.0, 20.0);
    let none = Draw {
        courtyard: None,
        silk: None,
        back: false,
    };
    let row = |b: &mut Pcb, refdes: &str, at: (f64, f64), [left, right]: [&'static str; 2]| {
        b.part(
            refdes,
            at,
            0.0,
            &[
                smd("1", -PITCH, 0.0, 0.6, 1.0, left),
                smd("2", 0.0, 0.0, 0.6, 1.0, "GND"),
                smd("3", PITCH, 0.0, 0.6, 1.0, right),
            ],
            none,
        );
    };
    // The via first, so that GND is net 1, which the zone names.
    b.via("GND", (25.0, 3.0), 0.6, 0.3);
    row(&mut b, "U1", PADS[0].1, ["SIG1", "SIG2"]);
    row(&mut b, "U2", PADS[1].1, ["SIG3", "SIG4"]);
    b.part(
        "U3",
        PADS[2].1,
        0.0,
        &[smd("1", 0.0, 0.0, 0.6, 1.0, "GND")],
        none,
    );
    let bottom = PADS[0].1.1 + 0.85;
    b.raw(&format!(
        "(zone (net 1) (net_name \"GND\") (layer \"F.Cu\") (hatch edge 0.5) (connect_pads (clearance 0.3)) (min_thickness 0.25) (filled_areas_thickness no) (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5)) (polygon (pts (xy 1 1) (xy 29 1) (xy 29 {bottom}) (xy 1 {bottom}))))"
    ));
    b
}

/// The name of the GND pad at `at`, if one is there.
fn pad_at(at: Pt) -> Option<&'static str> {
    PADS.iter()
        .find(|(_, (x, y))| at.dist(Pt::new(*x, *y)) < 0.05)
        .map(|(name, _)| *name)
}

#[test]
fn a_pad_whose_fill_beside_it_is_thinner_than_the_minimum_is_starved_as_kicad_finds_it() {
    let mut b = board();
    let (pcb, pro) = (b.pcb(), b.pro());
    let ours = drc_check(&pcb, Some(&pro), &DrcExtras::default()).unwrap();
    let starved: Vec<_> = ours
        .findings
        .iter()
        .filter(|f| f.rule == DrcRule::StarvedThermal)
        .collect();
    let mut names: Vec<&str> = starved
        .iter()
        .filter_map(|f| f.items.iter().find_map(|i| pad_at(i.at)))
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["A"], "{ours}");
    assert!(
        starved[0]
            .message
            .starts_with("thermal relief joins the pour with 1 of its 4 spokes; the least is 2"),
        "{}",
        starved[0].message
    );

    let Some(kicad) = kicad_findings("thermal_spokes", &pcb, &pro) else {
        eprintln!("thermal_spokes: KiCad is not installed; netsmith's count alone is checked");
        return;
    };
    let mut theirs: Vec<&str> = kicad
        .iter()
        .filter(|k| k.kind == "starved_thermal")
        .filter_map(|k| k.items.iter().find_map(|(_, at)| pad_at(*at)))
        .collect();
    theirs.sort_unstable();
    let said: Vec<String> = kicad
        .iter()
        .filter(|k| k.kind == "starved_thermal")
        .map(|k| format!("{} {:?}", k.description, k.items))
        .collect();
    assert_eq!(theirs, names, "KiCad's starved thermals: {said:?}");
}

/// The report of each pad's join to its pour (issue #1187), of the same board: pad A with the one
/// spoke that reaches the fill, B with two, C with four, the other nets' pads beside A and B
/// cleared, and counts that agree with the design rules check's `starved_thermal` findings and
/// with the fill the Gerbers hold.
#[test]
fn the_report_of_the_pads_of_a_pour_counts_what_the_check_and_the_gerbers_have() {
    use netsmith_geom::{Artwork, PadJoin, Polygons, edge_outline, fill_pour, pad_joins, plot_rules_of};

    let mut b = board();
    let (pcb, pro) = (b.pcb(), b.pro());
    let art = Artwork::parse(&pcb).unwrap();
    let edge = edge_outline(&art).unwrap();
    let rules = plot_rules_of(Some(&pro));
    let joins = pad_joins(&art, "F.Cu", &edge, &rules).unwrap();
    assert_eq!(joins.len(), 1, "one GND zone");
    let zone = &joins[0];
    let mut joined: Vec<(&str, usize, usize)> = zone
        .pads
        .iter()
        .map(|(p, join)| {
            let name = pad_at(p.at).unwrap_or_else(|| panic!("a GND pad of the board: {p:?}"));
            match *join {
                PadJoin::Thermal { spokes, reach, .. } => (name, spokes, reach),
                other => panic!("{name} is joined by a thermal relief: {other:?}"),
            }
        })
        .collect();
    joined.sort_unstable();
    assert_eq!(joined, [("A", 4, 1), ("B", 4, 2), ("C", 4, 4)]);
    let mut cleared: Vec<String> = zone
        .cleared
        .iter()
        .map(|p| format!("{}.{} {}", p.part, p.number, p.net.as_deref().unwrap_or("")))
        .collect();
    cleared.sort_unstable();
    assert_eq!(cleared, ["U1.1 SIG1", "U1.3 SIG2", "U2.1 SIG3", "U2.3 SIG4"]);
    assert!(zone.islands.is_empty() && !zone.held);

    // The starved pads are the check's `starved_thermal` findings.
    let starved: Vec<&str> = zone
        .pads
        .iter()
        .filter(|(_, j)| j.starved())
        .filter_map(|(p, _)| pad_at(p.at))
        .collect();
    let ours = drc_check(&pcb, Some(&pro), &DrcExtras::default()).unwrap();
    let mut found: Vec<&str> = ours
        .findings
        .iter()
        .filter(|f| f.rule == DrcRule::StarvedThermal)
        .filter_map(|f| f.items.iter().find_map(|i| pad_at(i.at)))
        .collect();
    found.sort_unstable();
    assert_eq!(starved, found);
    assert_eq!(starved, ["A"]);

    // The fill is the one the Gerbers hold: the same pieces, the same area.
    let mut pen = Polygons::new();
    fill_pour(&mut pen, &art, "F.Cu", &edge, &rules).unwrap();
    let gerber = pen.areas();
    let area = |all: &[netsmith_geom::PolygonArea]| all.iter().map(|a| a.size()).sum::<f64>();
    assert_eq!(gerber.len(), zone.fill.len());
    assert!(
        (area(&gerber) - area(&zone.fill)).abs() < 1e-6,
        "{} and {}",
        area(&gerber),
        area(&zone.fill)
    );
}

/// The islands the fill takes out are reported with their places and areas (issue #1187): a
/// strip of the pour that a track of another net cuts off from the rest, which touches no GND
/// copper, is one more than the board without the track has.
#[test]
fn the_report_says_which_copper_islands_the_fill_took_out() {
    use netsmith_geom::{Artwork, edge_outline, pad_joins, plot_rules_of};

    let removed = |b: &mut Pcb| {
        let (pcb, pro) = (b.pcb(), b.pro());
        let art = Artwork::parse(&pcb).unwrap();
        let edge = edge_outline(&art).unwrap();
        let joins = pad_joins(&art, "F.Cu", &edge, &plot_rules_of(Some(&pro))).unwrap();
        assert!(joins[0].islands.is_empty(), "the zone removes its islands");
        joins[0].removed.clone()
    };
    let before = removed(&mut board());
    // A SIG track down the whole pour 2 mm from its left edge: the strip of fill left of it
    // touches no GND copper (the via and the pads are further right).
    let x = 3.0;
    let mut b = board();
    b.track("SIG1", "F.Cu", (x, 0.5), (x, 19.5), 0.25);
    let after = removed(&mut b);
    let left = |all: &[(Pt, f64)]| all.iter().filter(|(at, _)| at.x < x).count();
    assert_eq!(left(&before), 0, "{before:?}");
    assert_eq!(left(&after), 1, "the strip left of the track is an island: {after:?}");
    assert_eq!(after.len(), before.len() + 1, "{before:?} {after:?}");
    let (_, area) = after.iter().find(|(at, _)| at.x < x).unwrap();
    // About 1.5 mm wide (from the pour's edge at 1 mm to the track's clearance at 2.575 mm) and
    // 14.85 mm long, less its rounded corners.
    assert!((15.0..25.0).contains(area), "{area}");
}
