//! Thermal spokes counted as KiCad's refill leaves them (issue #1115): a GND pad between two
//! other nets' pads whose clearances leave only a strip of fill beside it, narrower than the
//! pour's minimum thickness, gets no spokes to the sides, as KiCad's filler makes none there.
//! netsmith's design rules check then reports the same `starved_thermal` pads as KiCad's does
//! after KiCad refills the zones (when KiCad is installed; the comparison is skipped where it
//! is not, and runs in the `kicad` job). The report of each pad's join to its pour (issue #1187,
//! `netsmith_geom::pad_joins`, which `board fill` prints) counts the same pads of the same fill.
//!
//! Issue #1222: the fill makes the spokes KiCad's refill makes on a board of pads at the spacing of
//! a real one, spoke for spoke, at several spoke widths, gaps and minimum thicknesses; a narrower
//! spoke or a smaller gap never joins fewer pads by two spokes (a spoke as wide as the minimum
//! thickness was once trimmed away whole); and KiCad's check of the fill netsmith writes into the
//! zones finds the starved thermal reliefs, isolated copper and copper slivers netsmith's check
//! finds (two zones of a net that touch only each other, and the corner where two zones of a net
//! meet, once left copper KiCad called isolated and a sliver).

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
    board_with(0.5, 0.5, 0.25)
}

/// [`board`] with its pour's spoke width, thermal gap and minimum thickness set.
fn board_with(spoke: f64, gap: f64, min: f64) -> Pcb {
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
        "(zone (net 1) (net_name \"GND\") (layer \"F.Cu\") (hatch edge 0.5) (connect_pads (clearance 0.3)) (min_thickness {min}) (filled_areas_thickness no) (fill yes (thermal_gap {gap}) (thermal_bridge_width {spoke})) (polygon (pts (xy 1 1) (xy 29 1) (xy 29 {bottom}) (xy 1 {bottom}))))"
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
    use netsmith_geom::{Artwork, PadJoin, Polygons, edge_outline, fill_pour, plot_rules_of};

    let mut b = board();
    let (pcb, pro) = (b.pcb(), b.pro());
    let art = Artwork::parse(&pcb).unwrap();
    let edge = edge_outline(&art).unwrap();
    let rules = plot_rules_of(Some(&pro));
    let joins = front_joins(&art, &edge, &rules);
    assert_eq!(joins.len(), 1, "one GND zone");
    let zone = &joins[0];
    let mut joined: Vec<(&str, usize, usize)> = zone
        .pads
        .iter()
        .map(|(p, join)| {
            let name = pad_at(p.at).unwrap_or_else(|| panic!("a GND pad of the board: {p:?}"));
            match join {
                PadJoin::Thermal { spokes, reach, .. } => (name, *spokes, *reach),
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
    use netsmith_geom::{Artwork, edge_outline, plot_rules_of};

    let removed = |b: &mut Pcb| {
        let (pcb, pro) = (b.pcb(), b.pro());
        let art = Artwork::parse(&pcb).unwrap();
        let edge = edge_outline(&art).unwrap();
        let joins = front_joins(&art, &edge, &plot_rules_of(Some(&pro)));
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


/// No part draws anything but its pads.
const BARE: Draw = Draw {
    courtyard: None,
    silk: None,
    back: false,
};

/// The board of the sweep (issue #1222): [`board_with`] and pads at the spacing of a real board's
/// (four layers, 122 parts, whose ground pours left most of the starved pads): an 0402 part's GND
/// pad beside its other pad (0.6 x 0.5 mm pads, 1 mm apart), a GND pad between two other nets'
/// pads at a 0.65 mm pitch (0.3 x 0.8 mm pads), and an 0402 part's GND pad beside a track of
/// another net.
fn swept(spoke: f64, gap: f64, min: f64) -> Pcb {
    let mut b = board_with(spoke, gap, min);
    let two = |net: &'static str| {
        [
            smd("1", -0.5, 0.0, 0.6, 0.5, net),
            smd("2", 0.5, 0.0, 0.6, 0.5, "GND"),
        ]
    };
    b.part("C1", (14.0, 4.0), 0.0, &two("SIG5"), BARE);
    b.part(
        "U4",
        (14.0, 10.0),
        0.0,
        &[
            smd("1", -0.65, 0.0, 0.3, 0.8, "SIG6"),
            smd("2", 0.0, 0.0, 0.3, 0.8, "GND"),
            smd("3", 0.65, 0.0, 0.3, 0.8, "SIG7"),
        ],
        BARE,
    );
    b.part("C2", (14.0, 14.0), 90.0, &two("SIG8"), BARE);
    b.track("SIG9", "F.Cu", (14.6, 12.0), (14.6, 16.5), 0.2);
    b
}

/// Each GND pad of a board's F.Cu pour by `part.number`, with how many spokes reach the fill, as
/// `pad_joins` counts them: of the fill the file holds where it holds one (a board KiCad refilled
/// and saved), else of netsmith's.
fn reach_of(pcb: &str, pro: &str) -> Vec<(String, usize)> {
    use netsmith_geom::{Artwork, PadJoin, edge_outline, plot_rules_of};
    let art = Artwork::parse(pcb).unwrap();
    let edge = edge_outline(&art).unwrap();
    let rules = plot_rules_of(Some(pro));
    let mut out: Vec<(String, usize)> = front_joins(&art, &edge, &rules)
        .into_iter()
        .flat_map(|z| z.pads)
        .filter_map(|(p, j)| match j {
            PadJoin::Thermal { reach, .. } => Some((format!("{}.{}", p.part, p.number), reach)),
            _ => None,
        })
        .collect();
    out.sort();
    out
}

/// The spoke widths, thermal gaps and minimum thickness of the sweep: the spokes from netsmith's
/// default down to the minimum thickness, the gaps from the default down to 0.2 mm.
const SPOKES: [f64; 4] = [0.5, 0.4, 0.3, 0.25];
const GAPS: [f64; 5] = [0.5, 0.4, 0.3, 0.25, 0.2];
const MIN: f64 = 0.25;

#[test]
fn a_narrower_spoke_or_a_smaller_gap_never_joins_fewer_pads_by_two_spokes() {
    // How many GND pads fewer than two spokes join, at each spoke width (rows) and gap (columns).
    let fewer = |spoke: f64, gap: f64| {
        let mut b = swept(spoke, gap, MIN);
        let (pcb, pro) = (b.pcb(), b.pro());
        reach_of(&pcb, &pro).iter().filter(|(_, r)| *r < 2).count()
    };
    let table: Vec<Vec<usize>> = SPOKES
        .iter()
        .map(|&s| GAPS.iter().map(|&g| fewer(s, g)).collect())
        .collect();
    eprintln!("pads joined by fewer than two spokes, spokes {SPOKES:?} by gaps {GAPS:?}: {table:?}");
    for (i, row) in table.iter().enumerate() {
        for (k, &n) in row.iter().enumerate() {
            if k > 0 {
                assert!(n <= row[k - 1], "gap {} gave {n}, more than gap {}: {table:?}", GAPS[k], GAPS[k - 1]);
            }
            if i > 0 {
                let wider = table[i - 1][k];
                assert!(n <= wider, "spoke {} gave {n}, more than spoke {}: {table:?}", SPOKES[i], SPOKES[i - 1]);
            }
        }
    }
    // A spoke as wide as the minimum thickness is made: the pads it joins are not all starved.
    let pads = {
        let mut b = swept(MIN, 0.5, MIN);
        let (pcb, pro) = (b.pcb(), b.pro());
        reach_of(&pcb, &pro).len()
    };
    assert!(table[SPOKES.len() - 1][0] < pads, "{table:?}");
}

#[test]
fn the_fill_makes_the_spokes_kicads_refill_makes() {
    for (spoke, gap, min) in [(0.5, 0.5, 0.25), (0.25, 0.25, 0.25), (0.3, 0.2, 0.25), (0.2, 0.2, 0.2)] {
        let mut b = swept(spoke, gap, min);
        let (pcb, pro) = (b.pcb(), b.pro());
        let ours = reach_of(&pcb, &pro);
        assert!(ours.iter().any(|(_, r)| *r > 0), "{ours:?}");
        // No dot in the name: the file's extension is what follows the last one.
        let name = format!("sweep-{spoke}-{gap}-{min}").replace('.', "_");
        let Some(filled) = kicad_refilled(&name, &pcb, &pro) else {
            eprintln!("thermal_spokes: KiCad is not installed; netsmith's spokes alone are counted");
            return;
        };
        let theirs = reach_of(&filled, &pro);
        assert_eq!(ours, theirs, "spoke {spoke}, gap {gap}, minimum {min}: netsmith's, then KiCad's");
    }
}

/// `pcb` with netsmith's fill written into its zones, as `fab` writes it (each piece one
/// `filled_polygon`, its holes joined to its outline): the zones of the board are its lines that
/// start a zone, in the order netsmith reads them.
fn with_netsmith_fill(pcb: &str, pro: &str) -> String {
    use netsmith_geom::{Artwork, edge_outline, plot_rules_of, zone_fills};
    let art = Artwork::parse(pcb).unwrap();
    let edge = edge_outline(&art).unwrap();
    let rules = plot_rules_of(Some(pro));
    let mut fills: Vec<String> = vec![String::new(); art.zones.len()];
    for layer in &art.copper {
        for (i, areas) in zone_fills(&art, layer, &edge, &rules).unwrap() {
            for a in areas {
                let pts: String = a
                    .fractured()
                    .iter()
                    .map(|p| format!("(xy {:.6} {:.6}) ", p.x, p.y))
                    .collect();
                fills[i].push_str(&format!(" (filled_polygon (layer \"{layer}\") (pts {pts}))"));
            }
        }
    }
    let mut zones = fills.into_iter();
    pcb.lines()
        .map(|line| match line.strip_suffix(')') {
            Some(open) if line.starts_with("\t(zone") => {
                format!("{open}{})", zones.next().unwrap_or_default())
            }
            _ => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A zone of GND of priority 1 over a rectangle, on F.Cu, with the pour's settings.
fn higher(b: &mut Pcb, (x0, y0, x1, y1): (f64, f64, f64, f64)) {
    b.raw(&format!(
        "(zone (net 1) (net_name \"GND\") (layer \"F.Cu\") (priority 1) (hatch edge 0.5) (connect_pads (clearance 0.3)) (min_thickness 0.25) (filled_areas_thickness no) (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5)) (polygon (pts (xy {x0} {y0}) (xy {x1} {y0}) (xy {x1} {y1}) (xy {x0} {y1}))))"
    ));
}

/// The board of the check of the fill (issue #1222): a GND pour over a 30 x 20 mm board, and three
/// places where zones of GND meet:
///
/// - D, a GND pad whose relief ends 0.18 mm from a GND zone of a higher priority, SIG tracks over
///   the ends of its other spokes: KiCad takes the higher zone's area out of the lower zone's fill
///   last, so the strip between joins the spoke to the right, and the pad is starved (one spoke);
/// - E, inside a ring of SIG track, a GND zone of a higher priority over half the inside: its
///   piece and the lower zone's beside it touch only each other, both islands;
/// - F, a SIG pad across the lower edge of a GND zone of a higher priority: where the pad's
///   clearance meets that edge, each zone is trimmed as if the other's area were its own, so no
///   notch is left between them with a spike of copper beside it.
fn meeting() -> Pcb {
    let mut b = Pcb::new(2);
    b.outline = (0.0, 0.0, 30.0, 20.0);
    b.via("GND", (2.0, 2.0), 0.6, 0.3);
    b.raw(
        "(zone (net 1) (net_name \"GND\") (layer \"F.Cu\") (hatch edge 0.5) (connect_pads (clearance 0.3)) (min_thickness 0.25) (filled_areas_thickness no) (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5)) (polygon (pts (xy 1 1) (xy 29 1) (xy 29 19) (xy 1 19))))",
    );
    // D.
    b.part("U5", (5.0, 10.0), 0.0, &[smd("1", 0.0, 0.0, 1.0, 1.0, "GND")], BARE);
    higher(&mut b, (6.18, 7.0, 9.0, 13.0));
    b.via("GND", (7.5, 10.0), 0.6, 0.3);
    b.track("SIG1", "F.Cu", (3.7, 7.0), (3.7, 13.0), 0.2);
    b.track("SIG1", "F.Cu", (3.0, 8.7), (5.6, 8.7), 0.2);
    b.track("SIG1", "F.Cu", (3.0, 11.3), (5.6, 11.3), 0.2);
    // E.
    let ring = [(14.0, 6.0), (20.0, 6.0), (20.0, 12.0), (14.0, 12.0)];
    for k in 0..4 {
        b.track("SIG2", "F.Cu", ring[k], ring[(k + 1) % 4], 0.2);
    }
    higher(&mut b, (15.0, 7.0, 17.0, 11.0));
    // F.
    higher(&mut b, (22.0, 5.0, 27.0, 9.0));
    b.via("GND", (26.0, 6.0), 0.6, 0.3);
    b.part("U6", (23.5, 9.0), 0.0, &[smd("1", 0.0, 0.0, 1.0, 1.0, "SIG3")], BARE);
    b
}

#[test]
fn kicads_check_of_the_fill_netsmith_writes_finds_what_netsmiths_check_finds() {
    use netsmith_geom::{Artwork, edge_outline, plot_rules_of};
    let mut b = meeting();
    let (pcb, pro) = (b.pcb(), b.pro());
    // D has the spoke to the right, into the strip beside the higher zone, and no other.
    assert_eq!(reach_of(&pcb, &pro), [("U5.1".to_string(), 1)]);
    let ours = drc_check(&pcb, Some(&pro), &DrcExtras::default()).unwrap();
    let count = |rule: DrcRule| ours.findings.iter().filter(|f| f.rule == rule).count();
    let starved: Vec<String> = ours
        .findings
        .iter()
        .filter(|f| f.rule == DrcRule::StarvedThermal)
        .filter_map(|f| f.items.iter().find_map(|i| i.part.clone()))
        .collect();
    assert_eq!(starved, ["U5"], "{ours}");
    assert_eq!(count(DrcRule::IsolatedCopper), 0, "{ours}");
    assert_eq!(count(DrcRule::CopperSliver), 0, "{ours}");
    // E's two pieces are taken out, as islands.
    let art = Artwork::parse(&pcb).unwrap();
    let edge = edge_outline(&art).unwrap();
    let joins = front_joins(&art, &edge, &plot_rules_of(Some(&pro)));
    let inside = |p: &Pt| (14.0..20.0).contains(&p.x) && (6.0..12.0).contains(&p.y);
    let removed = joins
        .iter()
        .flat_map(|z| &z.removed)
        .filter(|(at, _)| inside(at))
        .count();
    assert_eq!(removed, 2, "{joins:?}");

    let filled = with_netsmith_fill(&pcb, &pro);
    let Some(kicad) = kicad_findings_as_filled("meeting", &filled, &pro) else {
        eprintln!("thermal_spokes: KiCad is not installed; netsmith's findings alone are checked");
        return;
    };
    let of = |kind: &str| -> Vec<String> {
        kicad
            .iter()
            .filter(|k| k.kind == kind)
            .map(|k| format!("{} {:?}", k.description, k.items))
            .collect()
    };
    let theirs_starved = of("starved_thermal");
    assert_eq!(theirs_starved.len(), 1, "{theirs_starved:?}");
    assert!(
        kicad
            .iter()
            .filter(|k| k.kind == "starved_thermal")
            .all(|k| k.items.iter().any(|(_, at)| at.dist(Pt::new(5.0, 10.0)) < 0.01)),
        "{theirs_starved:?}"
    );
    assert_eq!(of("isolated_copper"), Vec::<String>::new());
    assert_eq!(of("copper_sliver"), Vec::<String>::new());
    assert_eq!(of("clearance"), Vec::<String>::new());
}

/// What the fill of F.Cu does with the pads in it (`netsmith_geom::pad_joins` of that layer).
fn front_joins(
    art: &netsmith_geom::Artwork,
    edge: &netsmith_geom::EdgeOutline,
    rules: &netsmith_geom::PlotRules,
) -> Vec<netsmith_geom::ZoneJoins> {
    netsmith_geom::pad_joins(art, edge, rules)
        .unwrap()
        .into_iter()
        .find(|(l, _)| l == "F.Cu")
        .map(|(_, j)| j)
        .unwrap_or_default()
}
