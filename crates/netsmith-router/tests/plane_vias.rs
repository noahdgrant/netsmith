//! Fewer vias to the planes (issue #834): two pads of a plane net near each other share one via,
//! and a pad that an outer pour of its net joins to the plane gives up its own. Each board here
//! routes completely with fewer vias than with the pass off, and passes netsmith's own design
//! rules check; `kicad_drc.rs` has KiCad's unconnected check on the same boards.

use netsmith_core::{Design, Net, Part, PinRef};
use netsmith_pcb::{Board, Pt, Ratsnest, Rect, kicad_pcb};
use netsmith_router::{RouteOptions, route_with};

use crate::common;

/// A four-layer board (GND and 3V3 planes, and the top poured on GND if `poured`) of four 0402
/// resistors in two columns: the left pads of the left two are on GND and the right pads of the
/// right two on 3V3, each pair 1.2 mm apart, and two signals run across between them.
pub(crate) fn board(poured: bool) -> Board {
    let part = |refdes: &str| Part {
        provides: Vec::new(),
        declared_kind: netsmith_testkit::std_kind("Resistor")
            .and_then(netsmith_core::PartKind::from_word),
        refdes: refdes.into(),
        kind: "Resistor".into(),
        value: "1k".into(),
        package: Some("0402".into()),
        path: String::new(),
        pins: Vec::new(),
    };
    let net = |name: &str, pads: &[(&str, &str)]| Net {
        name: name.into(),
        pins: pads
            .iter()
            .map(|(r, p)| PinRef {
                refdes: (*r).into(),
                pin: (*p).into(),
            })
            .collect(),
    };
    let places = [
        ("R1", 8.0, 6.0),
        ("R2", 8.0, 7.2),
        ("R3", 14.0, 6.0),
        ("R4", 14.0, 7.2),
    ];
    let design = Design {
        modules: Vec::new(),
        name: "PlaneVias".into(),
        process: Some(common::four_layer_process()),
        features: Default::default(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        parts: places.iter().map(|(r, ..)| part(r)).collect(),
        nets: vec![
            net("GND", &[("R1", "1"), ("R2", "1")]),
            net("3V3", &[("R3", "2"), ("R4", "2")]),
            net("S1", &[("R1", "2"), ("R3", "1")]),
            net("S2", &[("R2", "2"), ("R4", "1")]),
        ],
    };
    let mut board = netsmith_placer::board_from_design(
        &design,
        &netsmith_kicad::Mapping::standard(),
        &common::fixtures(),
    )
    .unwrap();
    for (placed, (_, x, y)) in board.parts.iter_mut().zip(places) {
        placed.at = Pt::new(x, y);
        placed.rotation = 0.0;
        // The text sides the placer chose are for the outline it made, not this one.
        placed.reference_side = netsmith_pcb::ReferenceSide::default();
    }
    board.outline = Rect::new(Pt::new(3.0, 3.0), Pt::new(19.0, 11.0));
    if poured {
        board.pours.push(netsmith_pcb::BoardPour {
            layer: netsmith_pcb::CopperLayer::Front,
            net: "GND".into(),
            stitch_pitch: Some(4.0),
            thermal_gap: None,
            spoke_width: None,
            clearance: None,
            min_width: None,
            pads: None,
        });
    }
    board
}

/// The board routed with the options, and its vias of the plane nets.
pub(crate) fn routed(poured: bool, options: RouteOptions) -> (Board, usize) {
    let mut b = board(poured);
    assert_eq!(b.planes.len(), 2);
    assert_eq!(b.pours.len(), usize::from(poured));
    let routing = route_with(&mut b, options);
    assert!(routing.left.is_empty(), "{:?}", routing.left);
    assert_eq!(Ratsnest::of(&b).len(), 0);
    let vias = b
        .vias
        .iter()
        .filter(|v| v.net == "GND" || v.net == "3V3")
        .count();
    (b, vias)
}

/// Both passes off: every pad of a plane net keeps a via of its own.
pub(crate) fn off() -> RouteOptions {
    RouteOptions {
        share_vias: false,
        pour_pads: false,
        ..RouteOptions::default()
    }
}

/// The errors of netsmith's own design rules check (ADR 0037), with no KiCad.
fn own_findings(board: &Board) -> Vec<String> {
    let report = netsmith_geom::drc_check(
        &kicad_pcb(board),
        Some(&netsmith_pcb::kicad_pro(board)),
        &Default::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    report.errors().map(|f| f.to_string()).collect()
}

#[test]
fn pads_of_a_plane_net_near_each_other_share_a_via() {
    let (_, before) = routed(false, off());
    let (board, after) = routed(false, RouteOptions::default());
    println!("{before} vias to the planes, {after} with shared vias");
    // Each pad had its own via; each pair now shares one.
    assert_eq!((before, after), (4, 2));
    assert_eq!(own_findings(&board), Vec::<String>::new());
}

#[test]
fn a_pad_in_a_pour_of_its_net_gives_up_its_via() {
    let only_pours = RouteOptions {
        share_vias: false,
        ..RouteOptions::default()
    };
    let (before_board, before) = routed(true, off());
    let (board, after) = routed(true, only_pours);
    let gnd = |b: &Board| b.vias.iter().filter(|v| v.net == "GND").count();
    println!(
        "{before} vias of the plane nets ({} GND), {after} with the pour ({} GND)",
        gnd(&before_board),
        gnd(&board)
    );
    // The two GND pads in the pour gave up their vias; the 3V3 pads, under no pour of theirs,
    // kept theirs.
    assert_eq!(before - after, 2);
    assert_eq!(
        board.vias.iter().filter(|v| v.net == "3V3").count(),
        2,
        "{:?}",
        board.vias
    );
    assert_eq!(own_findings(&board), Vec::<String>::new());
}

#[test]
fn the_ratsnest_counts_a_pad_joined_to_its_plane_by_the_pour_and_a_via_it_covers() {
    // The pads' own vias and tracks taken off a board routed with both passes off: the pour
    // joins the pads to its stitching vias, and the ratsnest says nothing is left.
    let (mut board, _) = routed(true, off());
    let touches = |t: &netsmith_pcb::Track, at: Pt| {
        t.from.distance(at) < 1e-6 || t.to.distance(at) < 1e-6
    };
    let pad_vias: Vec<Pt> = board
        .vias
        .iter()
        .filter(|v| v.net == "GND" && board.tracks.iter().any(|t| touches(t, v.at)))
        .map(|v| v.at)
        .collect();
    assert_eq!(pad_vias.len(), 2, "{:?}", board.vias);
    assert!(
        board.vias.iter().filter(|v| v.net == "GND").count() > 2,
        "the pour is stitched"
    );
    board
        .tracks
        .retain(|t| t.net != "GND" || !pad_vias.iter().any(|&at| touches(t, at)));
    board.vias.retain(|v| !pad_vias.contains(&v.at));
    board.teardrops.retain(|t| t.net != "GND");
    assert_eq!(Ratsnest::without_pour_fills(&board).len(), 2);
    assert_eq!(Ratsnest::of(&board).len(), 0);
    // With the stitching gone too, nothing joins the pour to the plane.
    board.vias.retain(|v| v.net != "GND");
    assert_eq!(Ratsnest::of(&board).len(), 2);
}

/// The poured board with R1 in its top left corner (issue #1115), routed with the pour pass
/// alone, and the same board with R1's GND pad's own track and via taken off. The
/// board's edge takes the ends of that pad's spokes to the left and up, and its S1 pad's
/// clearance the one to the right, so the pour joins the pad by the spoke down alone.
pub(crate) fn corner() -> (Board, Board) {
    let mut b = board(true);
    b.parts[0].at = Pt::new(4.3, 3.8);
    let options = RouteOptions {
        share_vias: false,
        ..RouteOptions::default()
    };
    let routing = route_with(&mut b, options);
    assert!(routing.left.is_empty(), "{:?}", routing.left);
    assert_eq!(Ratsnest::of(&b).len(), 0);
    let r1 = gnd_pad(&b, "R1");
    let mut bare = b.clone();
    let touches = |t: &netsmith_pcb::Track| t.from.distance(r1) < 0.01 || t.to.distance(r1) < 0.01;
    let ends: Vec<Pt> = b
        .tracks
        .iter()
        .filter(|t| t.net == "GND" && touches(t))
        .flat_map(|t| [t.from, t.to])
        .collect();
    bare.tracks.retain(|t| t.net != "GND" || !touches(t));
    bare.vias
        .retain(|v| v.net != "GND" || !ends.iter().any(|e| e.distance(v.at) < 0.01));
    (b, bare)
}

/// The centre of the GND pad of a resistor of [`board`].
fn gnd_pad(b: &Board, refdes: &str) -> Pt {
    let art = netsmith_pcb::copper_artwork(b).unwrap();
    let p = art
        .pads
        .iter()
        .find(|p| p.part == refdes && p.number == "1")
        .unwrap();
    Pt::new(p.at.x, p.at.y)
}

#[test]
fn a_pad_the_pour_joins_with_one_spoke_keeps_its_via() {
    // KiCad's check calls R1's GND pad a starved thermal relief without its via: it keeps it, and
    // R2's, in the open pour, still goes.
    let (b, bare) = corner();
    let own_via = |at: Pt| {
        b.tracks.iter().any(|t| {
            t.net == "GND"
                && [(t.from, t.to), (t.to, t.from)].iter().any(|(a, z)| {
                    a.distance(at) < 0.01 && b.vias.iter().any(|v| v.at.distance(*z) < 0.01)
                })
        })
    };
    assert!(own_via(gnd_pad(&b, "R1")), "R1 keeps its via: {:?}", b.vias);
    assert!(!own_via(gnd_pad(&b, "R2")), "R2 gives its via up: {:?}", b.vias);
    let starved = |b: &Board| -> Vec<String> {
        netsmith_pcb::starved_pour_pads(b)
            .unwrap()
            .into_iter()
            .map(|(r, n, _)| format!("{r}.{n}"))
            .collect()
    };
    assert_eq!(starved(&b), Vec::<String>::new());
    // Without its track and via the pad is starved: the track joins it as a second spoke would.
    assert_eq!(starved(&bare), ["R1.1"]);
    assert_eq!(own_findings(&b), Vec::<String>::new());
}
