//! Thermal spokes counted as KiCad's refill leaves them (issue #1115): a GND pad between two
//! other nets' pads whose clearances leave only a strip of fill beside it, narrower than the
//! pour's minimum thickness, gets no spokes to the sides, as KiCad's filler makes none there.
//! netsmith's design rules check then reports the same `starved_thermal` pads as KiCad's does
//! after KiCad refills the zones (when KiCad is installed; the comparison is skipped where it
//! is not, and runs in the `kicad` job).

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
