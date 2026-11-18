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
    let dir = TempDir::new("footprint-keepout");
    let pretty = dir.path().join("keep.pretty");
    std::fs::create_dir_all(&pretty).unwrap();
    std::fs::write(pretty.join("Keep.kicad_mod"), KEEP).unwrap();
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
