//! A keepout zone a footprint carries is written to the board in board coordinates, as KiCad
//! saves it (issue #1237): a library has it in the footprint's frame (a `.kicad_mod` puts the
//! footprint at the origin), and a `.kicad_pcb` has the points of the zone where the footprint is,
//! turned and moved with it, and mirrored on the back.
//!
//! A copper pour on the face of a part that carries a keepout over its pads (which forbids a pour)
//! is filled by netsmith, and by KiCad's own filler where KiCad is installed (the test skips
//! without it and runs in the `kicad` job): neither fill has copper inside the keepout where the
//! footprint is, and both reach its edges there. The part is at a position, turned by 0 and 90
//! degrees, on the front and on the back; the keepout is off the footprint's origin, so a frame
//! that is wrong cannot hide behind a symmetry.
//!
//! A keepout drawn with an arc (a circle round a mounting hole is one arc) is moved with the
//! footprint as well, and read as a circle (issue #1299): the same pours, with netsmith's fill and
//! KiCad's, keep out of a round keepout where the footprint is, at a position, turned and on both
//! faces.
//!
//! Copper keeps out of such a keepout where it forbids it (issue #1291): a footprint whose keepout
//! is a wall across the straight way between two pads, under two stitched pours, is routed and
//! stitched with no track or via in it, and netsmith's check and KiCad's find nothing there; the
//! same wall allowing copper takes stitching vias and a track, which both checks flag, at the same
//! places, when the wall forbids them.

use netsmith_core::{Design, Net, Part, PinRef};
use netsmith_fab::KicadCliExe;
use netsmith_kicad::{Mapping, Sexp};
use netsmith_pcb::{
    Board, BoardPour, CopperLayer, FootprintLibrary, Pt, Rect, Side, kicad_pcb, kicad_pcb_filled,
    kicad_pro,
};
use netsmith_testkit::TempDir;

use crate::common;

/// A footprint of two pads with a keepout over them, in the footprint's frame:
/// x from -1 to 3 and y from -2 to 1 mm, which forbids copper pours (and vias) on `F.Cu`.
const KEEP: &str = r#"(footprint "Keep" (version 20241229) (generator "netsmith") (layer "F.Cu")
    (property "Reference" "REF**" (at 0 -3 0) (layer "F.SilkS") (effects (font (size 1 1) (thickness 0.15))))
    (property "Value" "Keep" (at 0 3 0) (layer "F.Fab") (effects (font (size 1 1) (thickness 0.15))))
    (attr smd)
    (pad "1" smd rect (at 0 0) (size 0.8 0.8) (layers "F.Cu" "F.Mask" "F.Paste"))
    (pad "2" smd rect (at 1.5 0.5) (size 0.8 0.8) (layers "F.Cu" "F.Mask" "F.Paste"))
    (fp_rect (start -1.5 -2.5) (end 3.5 1.5) (stroke (width 0.05) (type solid)) (fill no) (layer "F.CrtYd"))
    (zone (net 0) (net_name "") (layers "F.Cu") (hatch full 0.508)
        (connect_pads (clearance 0)) (min_thickness 0.254) (filled_areas_thickness no)
        (keepout (tracks allowed) (vias not_allowed) (pads allowed) (copperpour not_allowed) (footprints allowed))
        (placement (enabled no) (sheetname ""))
        (fill (thermal_gap 0.5) (thermal_bridge_width 0.5))
        (polygon (pts (xy -1 -2) (xy 3 -2) (xy 3 1) (xy -1 1)))))"#;

/// [`KEEP`] with its keepout drawn as one circle of one arc: centre (0.75, 0.25) and radius 1.5 mm
/// in the footprint's frame, over the two pads (issue #1299).
fn round_keep() -> String {
    KEEP.replace(
        "(polygon (pts (xy -1 -2) (xy 3 -2) (xy 3 1) (xy -1 1)))",
        "(polygon (pts (arc (start 2.25 0.25) (mid -0.75 0.25) (end 2.25 0.25))))",
    )
}

/// Where the part is, and the library's keepout in the footprint's frame.
const AT: Pt = Pt { x: 15.0, y: 10.0 };
const LIBRARY: [(f64, f64); 2] = [(-1.0, -2.0), (3.0, 1.0)];

fn part(refdes: &str, kind: &str) -> Part {
    Part {
        provides: Vec::new(),
        declared_kind: netsmith_testkit::std_kind(kind).and_then(netsmith_core::PartKind::from_word),
        refdes: refdes.into(),
        kind: kind.into(),
        value: "1k".into(),
        package: Some(if kind == "Resistor" { "0402" } else { "P" }.into()),
        path: String::new(),
        pins: Vec::new(),
    }
}

fn net(name: &str, pins: &[(&str, &str)]) -> Net {
    Net {
        name: name.into(),
        pins: pins
            .iter()
            .map(|(r, p)| PinRef {
                refdes: (*r).into(),
                pin: (*p).into(),
            })
            .collect(),
    }
}

/// A 30 x 20 mm board, the keeping part K1 at [`AT`] turned by `rotation` on `side`, two
/// resistors beside it that join a GND pour on that side (the pour is on the part's face).
fn board(rotation: f64, side: Side) -> Board {
    board_of(KEEP, rotation, side)
}

/// [`board`] with the keeping part's footprint `footprint`.
fn board_of(footprint: &str, rotation: f64, side: Side) -> Board {
    let dir = TempDir::new("footprint-keepout");
    let pretty = dir.path().join("keep.pretty");
    std::fs::create_dir_all(&pretty).unwrap();
    std::fs::write(pretty.join("Keep.kicad_mod"), footprint).unwrap();
    let rows = "[[part]]\nkind = \"Keep\"\npackage = \"P\"\nsymbol = \"keep:Keep\"\nfootprint = \"keep:Keep\"\n";
    let mapping = Mapping::standard()
        .merged(Mapping::parse(rows, "kicad.toml", dir.path()).unwrap())
        .unwrap();
    let library =
        FootprintLibrary::at(netsmith_testkit::sample_footprints()).with_library("keep", pretty);
    let design = Design {
        modules: Vec::new(),
        name: "Keepout".into(),
        process: Some(common::process_with(&[])),
        features: Default::default(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        parts: vec![
            part("K1", "Keep"),
            part("R1", "Resistor"),
            part("R2", "Resistor"),
        ],
        nets: vec![
            net("GND", &[("R1", "1"), ("R2", "1")]),
            net("SIG", &[("R1", "2"), ("K1", "1"), ("R2", "2")]),
        ],
    };
    let mut board = netsmith_placer::board_from_design(&design, &mapping, &library).unwrap();
    for placed in &mut board.parts {
        let (at, turn) = match placed.refdes.as_str() {
            "K1" => (AT, rotation),
            "R1" => (Pt::new(6.0, 5.0), 0.0),
            _ => (Pt::new(24.0, 15.0), 0.0),
        };
        placed.at = at;
        placed.rotation = turn;
        placed.side = side;
        // The text sides the placer chose are for the outline it made, not this one.
        placed.reference_side = netsmith_pcb::ReferenceSide::default();
    }
    board.outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(30.0, 20.0));
    board.pours.push(BoardPour {
        layer: if side == Side::Back {
            CopperLayer::Back
        } else {
            CopperLayer::Front
        },
        net: "GND".into(),
        stitch_pitch: None,
        thermal_gap: None,
        spoke_width: None,
        clearance: None,
        min_width: None,
        pads: None,
    });
    board
}

/// The keepout where K1 is: the box of its two corners of the library, as the part's own frame
/// puts them on the board (`Placed::to_board`, which its pads follow). At a right angle it is
/// still a box with sides along the axes.
fn keepout_box(board: &Board) -> Rect {
    let k1 = board.parts.iter().find(|p| p.refdes == "K1").unwrap();
    let a = k1.to_board(Pt::new(LIBRARY[0].0, LIBRARY[0].1));
    let b = k1.to_board(Pt::new(LIBRARY[1].0, LIBRARY[1].1));
    Rect::new(
        Pt::new(a.x.min(b.x), a.y.min(b.y)),
        Pt::new(a.x.max(b.x), a.y.max(b.y)),
    )
}

/// The points of every filled polygon of the pour zones of the board file `text`.
fn fill_points(text: &str) -> Vec<(f64, f64)> {
    let root = Sexp::parse(text).unwrap();
    let mut points = Vec::new();
    for zone in root.args().iter().filter(|i| i.head() == Some("zone")) {
        if zone.find("keepout").is_some() {
            continue;
        }
        for fill in zone.find_all("filled_polygon") {
            for xy in fill.find("pts").unwrap().find_all("xy") {
                let n = |i: usize| xy.items()[i].text().unwrap().parse::<f64>().unwrap();
                points.push((n(1), n(2)));
            }
        }
    }
    points
}

/// Checks that a fill keeps out of the keepout `at` and reaches its four sides.
fn assert_pour_keeps_out(who: &str, points: &[(f64, f64)], at: Rect) {
    assert!(!points.is_empty(), "{who}: the pour has no fill");
    let inside: Vec<_> = points
        .iter()
        .filter(|p| {
            p.0 > at.min.x + 0.01 && p.0 < at.max.x - 0.01 && p.1 > at.min.y + 0.01 && p.1 < at.max.y - 0.01
        })
        .collect();
    assert!(
        inside.is_empty(),
        "{who}: the fill has points inside the keepout {at:?}: {inside:?}"
    );
    // The fill reaches each side of the keepout, so that it is the keepout and not a gap that
    // is somewhere else.
    let near = |a: f64, b: f64| (a - b).abs() < 0.01;
    for (side, reached) in [
        ("left", points.iter().any(|p| near(p.0, at.min.x) && p.1 >= at.min.y - 0.01 && p.1 <= at.max.y + 0.01)),
        ("right", points.iter().any(|p| near(p.0, at.max.x) && p.1 >= at.min.y - 0.01 && p.1 <= at.max.y + 0.01)),
        ("top", points.iter().any(|p| near(p.1, at.min.y) && p.0 >= at.min.x - 0.01 && p.0 <= at.max.x + 0.01)),
        ("bottom", points.iter().any(|p| near(p.1, at.max.y) && p.0 >= at.min.x - 0.01 && p.0 <= at.max.x + 0.01)),
    ] {
        assert!(reached, "{who}: the fill does not reach the {side} of the keepout {at:?}");
    }
}

#[test]
fn the_pour_keeps_out_of_a_footprints_keepout_where_the_footprint_is_in_netsmiths_fill_and_in_kicads() {
    let cli = KicadCliExe::locate_runnable();
    if cli.is_none() {
        eprintln!("skipped KiCad's refill: no KiCad");
    }
    for side in [Side::Front, Side::Back] {
        for rotation in [0.0, 90.0] {
            let who = format!("{rotation} degrees on the {side:?}");
            let board = board(rotation, side);
            let at = keepout_box(&board);
            // The board's own reading of the keepout, which the router and the stitching keep
            // out of (issue #1291): the same box, on the part's face, forbidding vias and pours.
            let read = board.footprint_keepouts();
            assert_eq!(read.len(), 1, "{who}: {read:?}");
            let near = |a: Pt, b: Pt| a.distance(b) < 1e-9;
            assert!(
                near(read[0].rect.min, at.min) && near(read[0].rect.max, at.max),
                "{who}: {:?} is not {at:?}",
                read[0].rect
            );
            let face = if side == Side::Back { "bottom" } else { "top" };
            assert_eq!(read[0].layers, [face], "{who}");
            assert!(read[0].vias && read[0].copper && !read[0].tracks, "{who}");
            // The keepout is over the pads: where the board puts each of K1's pads.
            let k1 = board.parts.iter().find(|p| p.refdes == "K1").unwrap();
            for pad in [Pt::new(0.0, 0.0), Pt::new(1.5, 0.5)] {
                let p = k1.to_board(pad);
                assert!(
                    p.x > at.min.x && p.x < at.max.x && p.y > at.min.y && p.y < at.max.y,
                    "{who}: the pad at {p:?} is outside {at:?}"
                );
            }
            // The written zone is that box, as the file's own points.
            let filled = kicad_pcb_filled(&board).unwrap();
            let root = Sexp::parse(&filled.text).unwrap();
            let zone = root
                .find_all("footprint")
                .flat_map(|f| f.find_all("zone"))
                .next()
                .expect("the footprint's zone is written");
            let xs: Vec<f64> = zone
                .find("polygon")
                .unwrap()
                .find("pts")
                .unwrap()
                .find_all("xy")
                .map(|xy| xy.items()[1].text().unwrap().parse().unwrap())
                .collect();
            assert!(
                xs.iter().all(|x| *x >= at.min.x - 1e-3 && *x <= at.max.x + 1e-3),
                "{who}: {xs:?} is not in {at:?}"
            );
            // netsmith's fill, the one the file holds, keeps out of it.
            assert_pour_keeps_out(
                &format!("netsmith, {who}"),
                &fill_points(&filled.text),
                at,
            );
            // KiCad's own refill of the file does too.
            let Some(cli) = &cli else { continue };
            let dir = TempDir::new("footprint-keepout-kicad");
            let pcb = dir.join("keepout.kicad_pcb");
            std::fs::write(&pcb, kicad_pcb(&board)).unwrap();
            std::fs::write(pcb.with_extension("kicad_pro"), kicad_pro(&board)).unwrap();
            let refilled = cli.refilled(&pcb).unwrap_or_else(|e| panic!("{who}: {e}"));
            assert_pour_keeps_out(&format!("KiCad, {who}"), &fill_points(&refilled), at);
        }
    }
}

/// Checks that a fill keeps out of the circle of `centre` and `radius` and reaches round it.
fn assert_pour_keeps_out_of_circle(who: &str, points: &[(f64, f64)], centre: Pt, radius: f64) {
    assert!(!points.is_empty(), "{who}: the pour has no fill");
    let dist = |p: &(f64, f64)| Pt::new(p.0, p.1).distance(centre);
    let inside: Vec<_> = points.iter().filter(|p| dist(p) < radius - 0.02).collect();
    assert!(
        inside.is_empty(),
        "{who}: the fill has points inside the keepout round {centre:?}: {inside:?}"
    );
    // The fill reaches the circle on each side of it, so that the keepout is round that circle
    // and not a gap somewhere else. KiCad's fill has a vertex on the circle about every
    // 0.4 mm (it cuts a curve to 5 µm), so one is near the side, not at it.
    for (side, dx, dy) in [("left", -1.0, 0.0), ("right", 1.0, 0.0), ("top", 0.0, -1.0), ("bottom", 0.0, 1.0)] {
        let edge = Pt::new(centre.x + dx * radius, centre.y + dy * radius);
        assert!(
            points.iter().any(|p| Pt::new(p.0, p.1).distance(edge) < 0.4),
            "{who}: the fill does not reach the {side} of the keepout round {centre:?}"
        );
    }
}

#[test]
fn the_pour_keeps_out_of_a_footprints_round_keepout_drawn_as_an_arc_in_netsmiths_fill_and_in_kicads() {
    let cli = KicadCliExe::locate_runnable();
    if cli.is_none() {
        eprintln!("skipped KiCad's refill: no KiCad");
    }
    for side in [Side::Front, Side::Back] {
        for rotation in [0.0, 90.0] {
            let who = format!("{rotation} degrees on the {side:?}");
            let board = board_of(&round_keep(), rotation, side);
            let k1 = board.parts.iter().find(|p| p.refdes == "K1").unwrap();
            let centre = k1.to_board(Pt::new(0.75, 0.25));
            // The board's own reading: the box round the circle.
            let read = board.footprint_keepouts();
            assert_eq!(read.len(), 1, "{who}: {read:?}");
            // The box of the points no further than `ARC_ERROR` (2 µm) inside the circle.
            let near = |a: Pt, b: Pt| a.distance(b) < 1e-3;
            let close = |a: Pt, b: Pt| a.distance(b) < 0.003;
            assert!(
                close(read[0].rect.min, Pt::new(centre.x - 1.5, centre.y - 1.5))
                    && close(read[0].rect.max, Pt::new(centre.x + 1.5, centre.y + 1.5)),
                "{who}: {:?} is not the box round {centre:?}",
                read[0].rect
            );
            // The written arc is the circle round the footprint's centre, in board coordinates.
            let filled = kicad_pcb_filled(&board).unwrap();
            let root = Sexp::parse(&filled.text).unwrap();
            let arc = root
                .find_all("footprint")
                .flat_map(|f| f.find_all("zone"))
                .next()
                .and_then(|z| z.find("polygon")?.find("pts")?.find("arc").cloned())
                .unwrap_or_else(|| panic!("{who}: the footprint's arc is written"));
            let at = |name: &str| {
                let n = arc.find(name).unwrap();
                let v = |i: usize| n.items()[i].text().unwrap().parse::<f64>().unwrap();
                Pt::new(v(1), v(2))
            };
            assert!(near(at("start"), at("end")), "{who}");
            assert!(near(at("start"), k1.to_board(Pt::new(2.25, 0.25))), "{who}");
            assert!(near(at("mid"), k1.to_board(Pt::new(-0.75, 0.25))), "{who}");
            // netsmith's fill, the one the file holds, keeps out of it.
            assert_pour_keeps_out_of_circle(
                &format!("netsmith, {who}"),
                &fill_points(&filled.text),
                centre,
                1.5,
            );
            // KiCad's own refill of the file does too.
            let Some(cli) = &cli else { continue };
            let dir = TempDir::new("footprint-keepout-round-kicad");
            let pcb = dir.join("keepout.kicad_pcb");
            std::fs::write(&pcb, kicad_pcb(&board)).unwrap();
            std::fs::write(pcb.with_extension("kicad_pro"), kicad_pro(&board)).unwrap();
            let refilled = cli.refilled(&pcb).unwrap_or_else(|e| panic!("{who}: {e}"));
            assert_pour_keeps_out_of_circle(&format!("KiCad, {who}"), &fill_points(&refilled), centre, 1.5);
        }
    }
}

// ---- copper keeps out of a footprint's keepout (issue #1291) -------------------------------------

/// A footprint that carries a wall: a keepout from x -1 to 1 and y -4 to 4 mm on both outer layers
/// that forbids `forbids` (`not_allowed` or `allowed` for tracks, vias and copper pours), and a pad
/// off its end.
fn wall(forbids: &str) -> String {
    format!(
        r#"(footprint "Wall" (version 20241229) (generator "netsmith") (layer "F.Cu")
    (property "Reference" "REF**" (at 0 -6 0) (layer "F.SilkS") (effects (font (size 1 1) (thickness 0.15))))
    (property "Value" "Wall" (at 0 6 0) (layer "F.Fab") (effects (font (size 1 1) (thickness 0.15))))
    (attr smd)
    (pad "1" smd rect (at 0 6) (size 0.8 0.8) (layers "F.Cu" "F.Mask" "F.Paste"))
    (fp_rect (start -1.2 -4.2) (end 1.2 6.6) (stroke (width 0.05) (type solid)) (fill no) (layer "F.CrtYd"))
    (zone (net 0) (net_name "") (layers "F&B.Cu") (hatch full 0.508)
        (connect_pads (clearance 0)) (min_thickness 0.254) (filled_areas_thickness no)
        (keepout (tracks {forbids}) (vias {forbids}) (pads allowed) (copperpour {forbids}) (footprints allowed))
        (placement (enabled no) (sheetname ""))
        (fill (thermal_gap 0.5) (thermal_bridge_width 0.5))
        (polygon (pts (xy -1 -4) (xy 1 -4) (xy 1 4) (xy -1 4)))))"#
    )
}

/// A 30 x 20 mm board with the wall W1 at (15, 10) between R1 at (6, 10) and R2 at (24, 10), which
/// SIG joins across it; GND joins their other pins, poured on both faces and stitched every 2 mm.
fn walled_board(forbids: &str) -> Board {
    let dir = TempDir::new("footprint-keepout-wall");
    let pretty = dir.path().join("keep.pretty");
    std::fs::create_dir_all(&pretty).unwrap();
    std::fs::write(pretty.join("Wall.kicad_mod"), wall(forbids)).unwrap();
    let rows = "[[part]]\nkind = \"Keep\"\npackage = \"P\"\nsymbol = \"keep:Keep\"\nfootprint = \"keep:Wall\"\n";
    let mapping = Mapping::standard()
        .merged(Mapping::parse(rows, "kicad.toml", dir.path()).unwrap())
        .unwrap();
    let library =
        FootprintLibrary::at(netsmith_testkit::sample_footprints()).with_library("keep", pretty);
    let design = Design {
        modules: Vec::new(),
        name: "Wall".into(),
        process: Some(common::process_with(&[])),
        features: Default::default(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        parts: vec![
            part("W1", "Keep"),
            part("R1", "Resistor"),
            part("R2", "Resistor"),
        ],
        nets: vec![
            net("GND", &[("R1", "1"), ("R2", "1")]),
            net("SIG", &[("R1", "2"), ("R2", "2")]),
        ],
    };
    let mut board = netsmith_placer::board_from_design(&design, &mapping, &library).unwrap();
    for placed in &mut board.parts {
        placed.at = match placed.refdes.as_str() {
            "W1" => Pt::new(15.0, 10.0),
            "R1" => Pt::new(6.0, 10.0),
            _ => Pt::new(24.0, 10.0),
        };
        placed.rotation = 0.0;
        placed.side = Side::Front;
        placed.reference_side = netsmith_pcb::ReferenceSide::default();
    }
    board.outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(30.0, 20.0));
    for layer in [CopperLayer::Front, CopperLayer::Back] {
        board.pours.push(BoardPour {
            layer,
            net: "GND".into(),
            stitch_pitch: Some(2.0),
            thermal_gap: None,
            spoke_width: None,
            clearance: None,
            min_width: None,
            pads: None,
        });
    }
    board
}

/// The wall on the board: x 14 to 16, y 6 to 14.
const WALL: Rect = Rect {
    min: Pt { x: 14.0, y: 6.0 },
    max: Pt { x: 16.0, y: 14.0 },
};

/// The tracks and vias of `board` with copper inside the wall.
fn in_wall(board: &Board) -> Vec<String> {
    let mut found = Vec::new();
    for t in &board.tracks {
        let seg = Rect::around([t.from, t.to]).unwrap().expanded(t.width / 2.0 - 0.01);
        if seg.overlaps(&WALL) {
            found.push(format!("track {t:?}"));
        }
    }
    for v in &board.vias {
        if Rect::new(v.at, v.at).expanded(v.diameter / 2.0 - 0.01).overlaps(&WALL) {
            found.push(format!("via {v:?}"));
        }
    }
    found
}

#[test]
fn the_router_and_the_stitching_keep_out_of_a_footprints_keepout_and_both_checks_find_nothing_in_it()
{
    let mut board = walled_board("not_allowed");
    let keepouts = board.footprint_keepouts();
    assert_eq!(keepouts.len(), 1, "{keepouts:?}");
    assert_eq!(keepouts[0].rect, WALL);
    assert_eq!(keepouts[0].layers, ["top", "bottom"]);
    let left = netsmith_router::route(&mut board);
    assert!(left.is_empty(), "{left:?}");
    assert!(
        board.vias.iter().filter(|v| v.net == "GND").count() > 4,
        "the pours are stitched: {:?}",
        board.vias
    );
    let found = in_wall(&board);
    assert!(found.is_empty(), "copper in the wall: {found:#?}");
    // netsmith's check of the board file finds nothing in it, nor does KiCad's.
    let filled = kicad_pcb_filled(&board).unwrap();
    let report = netsmith_geom::drc_check(
        &filled.text,
        Some(&kicad_pro(&board)),
        &netsmith_geom::DrcExtras::default(),
    )
    .unwrap();
    assert_eq!(
        report.count(netsmith_geom::DrcRule::ItemsNotAllowed),
        0,
        "{report}"
    );
    if let Some(kicad) = crate::drc_kit::check("footprint-keepout-wall", &board) {
        assert_eq!(
            kicad.count("items_not_allowed"),
            0,
            "{:?}",
            kicad.violations
        );
    }
}

#[test]
fn a_footprints_keepout_that_allows_copper_takes_the_stitching_and_one_that_forbids_it_is_flagged() {
    // The same wall, allowing everything: the stitching puts vias in it, so the wall above is what
    // keeps them out.
    let mut open = walled_board("allowed");
    assert!(open.footprint_keepouts().is_empty(), "a zone that forbids nothing is no keepout");
    netsmith_router::route(&mut open);
    let found = in_wall(&open);
    assert!(
        found.iter().any(|f| f.starts_with("via")),
        "the stitching reaches the wall where it is allowed: {:?}",
        open.vias
    );
    // That copper, on the board whose wall forbids it, is what both checks flag.
    let mut walled = walled_board("not_allowed");
    walled.tracks = open.tracks.clone();
    walled.vias = open.vias.clone();
    let filled = kicad_pcb_filled(&walled).unwrap();
    let report = netsmith_geom::drc_check(
        &filled.text,
        Some(&kicad_pro(&walled)),
        &netsmith_geom::DrcExtras::default(),
    )
    .unwrap();
    // Each piece of copper whose box is in the wall, less one whose box only is (a track that
    // passes a corner on a slant).
    let flagged = report.count(netsmith_geom::DrcRule::ItemsNotAllowed);
    assert!(flagged > 0 && flagged <= found.len(), "{found:#?}\n{report}");
    // KiCad's check flags the same copper. Only this rule is compared: KiCad refills the pours
    // first, out of the wall, which leaves the vias in it joined to nothing, where netsmith's check
    // of the file joins a via inside a pour's outline to it.
    let Some(cli) = KicadCliExe::locate_runnable() else {
        eprintln!("skipped KiCad's check: no KiCad");
        return;
    };
    let dir = TempDir::new("footprint-keepout-flagged");
    let pcb = dir.join("flagged.kicad_pcb");
    std::fs::write(&pcb, &filled.text).unwrap();
    std::fs::write(pcb.with_extension("kicad_pro"), kicad_pro(&walled)).unwrap();
    let json = netsmith_fab::KicadCli::drc_json(&cli, &pcb).unwrap_or_else(|e| panic!("{e}"));
    let theirs = netsmith_fab::parse_kicad_drc(&json).unwrap();
    let mut kicad: Vec<_> = theirs
        .iter()
        .filter(|f| f.kind == "items_not_allowed")
        .flat_map(|f| f.items.iter().map(|(_, at)| (at.x * 100.0).round() as i64 * 100_000 + (at.y * 100.0).round() as i64))
        .collect();
    let mut ours: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.rule == netsmith_geom::DrcRule::ItemsNotAllowed)
        .map(|f| (f.at.x * 100.0).round() as i64 * 100_000 + (f.at.y * 100.0).round() as i64)
        .collect();
    kicad.sort_unstable();
    ours.sort_unstable();
    assert_eq!(kicad, ours, "KiCad's items_not_allowed: {theirs:?}");
}
