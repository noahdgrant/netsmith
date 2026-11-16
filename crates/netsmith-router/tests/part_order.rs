//! The router gives a board the same copper whatever order its parts come in (issue #718): a
//! board that routed only because of the order the design stage happened to list its parts in
//! is a board the router is fragile on. The board here has a USB pair that must cross to reach
//! its connector, beside a fine-pitch package's decoupling capacitors whose ground pads are near
//! enough to share one via to the pour, which is what the order of the parts used to decide.

use crate::common;
use crate::guard;

use netsmith_core::{Design, Net, Part, PartPin, PinKind, PinRef};
use netsmith_pcb::{Board, BoardPour, CopperLayer, Pt, Rect};
use netsmith_router::{RouteOptions, route_with};
use netsmith_testkit::TempDir;

/// A QFN of 16 pads, 3 by 3 mm on a 0.5 mm pitch (enough for escape lanes): pads 1 to 4 down
/// the left side, 5 to 8 along the bottom, 9 to 12 up the right side and 13 to 16 along the top.
fn qfn16() -> String {
    let (pitch, reach, long, short) = (0.5, 1.45, 0.7, 0.25);
    let mut pads = String::new();
    for k in 0..16 {
        let along = (k % 4) as f64 * pitch - 0.75;
        let (x, y, w, h) = match k / 4 {
            0 => (-reach, along, long, short),
            1 => (along, reach, short, long),
            2 => (reach, -along, long, short),
            _ => (-along, -reach, short, long),
        };
        pads += &format!(
            "(pad \"{}\" smd rect (at {x:.3} {y:.3}) (size {w} {h}) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) ",
            k + 1
        );
    }
    let court = 2.0;
    let lines = [
        ((-court, -court), (court, -court)),
        ((court, -court), (court, court)),
        ((court, court), (-court, court)),
        ((-court, court), (-court, -court)),
    ]
    .map(|(a, b)| {
        format!(
            "(fp_line (start {} {}) (end {} {}) (stroke (width 0.05) (type solid)) (layer \"F.CrtYd\")) ",
            a.0, a.1, b.0, b.1
        )
    })
    .concat();
    format!(
        "(footprint \"QFN16\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
         (property \"Reference\" \"REF**\" (at 0 -2.7 0) (layer \"F.SilkS\") \
         (effects (font (size 1 1) (thickness 0.15)))) \
         (property \"Value\" \"QFN16\" (at 0 2.7 0) (layer \"F.Fab\") \
         (effects (font (size 1 1) (thickness 0.15)))) \
         (attr smd) {lines}{pads})\n"
    )
}

fn part(refdes: &str, kind: &str, package: &str, pins: Vec<PartPin>) -> Part {
    Part {
        provides: if netsmith_testkit::std_kind(kind) == Some("connector") {
            vec!["Connector".to_string()]
        } else {
            Vec::new()
        },
        declared_kind: netsmith_testkit::std_kind(kind).and_then(netsmith_core::PartKind::from_word),
        refdes: refdes.into(),
        kind: kind.into(),
        value: "1".into(),
        package: Some(package.into()),
        path: format!("usb.{refdes}"),
        pins,
    }
}

fn power(name: &str, pad: &str) -> PartPin {
    PartPin {
        name: name.into(),
        datasheet: name.to_uppercase(),
        kind: PinKind::Power,
        pads: vec![pad.into()],
        function: None,
        power_flow: None,
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

/// The board, placed by hand, with GND poured on the back. U1's USB pins run `dp` above `dm`
/// down its left side, and the header J1 has them the other way round, so the pair crosses. C1
/// and C2 decouple U1's supply pin (pad 4) and ground pin (pad 5, round the corner) side by side
/// below it, their ground pads 1.3 mm apart and joined to the pin on the front: each needs a via
/// to the pour of its own unless the other's is near, so which of them is tried first is what
/// gets the via (the router took them in the parts' order before issue #718).
fn usb_board() -> Board {
    let design = Design {
        modules: Vec::new(),
        name: "UsbPair".into(),
        process: Some(common::process_with(&[])),
        features: Default::default(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        parts: vec![
            part(
                "U1",
                "Qfn",
                "QFN16",
                vec![power("gnd", "5"), power("vdd", "4")],
            ),
            part("J1", "Header1x4", "2.54mm", Vec::new()),
            part("C1", "Capacitor", "0402", Vec::new()),
            part("C2", "Capacitor", "0402", Vec::new()),
            part("R1", "Resistor", "0402", Vec::new()),
        ],
        nets: vec![
            net("GND", &[("U1", "5"), ("C1", "2"), ("C2", "2")]),
            net(
                "VDD",
                &[("U1", "4"), ("C1", "1"), ("C2", "1"), ("J1", "1"), ("R1", "1")],
            ),
            net("usb.dp", &[("U1", "2"), ("J1", "3")]),
            net("usb.dm", &[("U1", "3"), ("J1", "2")]),
            net("net-pullup", &[("U1", "12"), ("R1", "2")]),
        ],
    };
    let folder = TempDir::new("part-order");
    std::fs::write(folder.join("QFN16.kicad_mod"), qfn16()).unwrap();
    let mapping = netsmith_kicad::Mapping::parse(
        "[[part]]\nkind = \"Qfn\"\npackage = \"QFN16\"\nsymbol = \"synth:Q\"\nfootprint = \"synth:QFN16\"\n",
        "synthetic",
        std::path::Path::new("."),
    )
    .unwrap();
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let mut board = netsmith_placer::board_from_design(
        &design,
        &mapping,
        &common::fixtures().with_library("synth", folder.path()),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let o = board.outline.min;
    board.outline = Rect::new(o, Pt::new(o.x + 20.0, o.y + 14.0));
    let place = |board: &mut Board, refdes: &str, x: f64, y: f64, rotation: f64| {
        let p = board.parts.iter_mut().find(|p| p.refdes == refdes).unwrap();
        p.at = Pt::new(o.x + x, o.y + y);
        p.rotation = rotation;
    };
    place(&mut board, "U1", 11.0, 6.0, 0.0);
    place(&mut board, "J1", 3.0, 3.0, 0.0);
    place(&mut board, "C1", 9.6, 9.6, 90.0);
    place(&mut board, "C2", 10.9, 9.6, 90.0);
    place(&mut board, "R1", 15.0, 9.0, 90.0);
    board.pours.push(BoardPour {
        layer: CopperLayer::Back,
        net: "GND".into(),
        stitch_pitch: None,
    });
    assert_eq!(board.check_placement(), vec![]);
    board
}

/// The board's copper as text, in an order of its own: what two routings are compared by.
fn copper(board: &Board) -> Vec<String> {
    let mut all: Vec<String> = board
        .tracks
        .iter()
        .map(|t| format!("{t:?}"))
        .chain(board.vias.iter().map(|v| format!("{v:?}")))
        .collect();
    all.sort();
    all
}

#[test]
fn a_usb_pair_gets_the_same_copper_whatever_order_the_parts_come_in() {
    let board = usb_board();
    let count = board.parts.len();
    // The design's order, reversed, and each turn of it.
    let mut orders: Vec<Vec<usize>> = vec![(0..count).collect(), (0..count).rev().collect()];
    orders.extend((1..count).map(|k| (0..count).map(|i| (i + k) % count).collect()));
    let mut first: Option<Vec<String>> = None;
    for order in orders {
        let mut b = board.clone();
        b.parts = order.iter().map(|&i| board.parts[i].clone()).collect();
        let names: Vec<String> = b.parts.iter().map(|p| p.refdes.clone()).collect();
        let routing = route_with(
            &mut b,
            RouteOptions {
                time_limit: Some(guard::GUARD),
                ..RouteOptions::default()
            },
        );
        guard::not_hung(&routing);
        assert!(
            routing.left.is_empty(),
            "parts in the order {names:?} leave {:?}",
            routing.left.lines
        );
        // Both nets of the pair are routed, and matched in length as a pair.
        assert_eq!(routing.pairs.len(), 1, "{:?}", routing.pairs);
        for net in ["usb.dp", "usb.dm"] {
            assert!(b.tracks.iter().any(|t| t.net == net), "{net} has no track");
        }
        // A capacitor's ground via (and so the order they were tried in) is part of the copper.
        assert!(b.vias.iter().any(|v| v.net == "GND"), "{:?}", b.vias);
        let mine = copper(&b);
        match &first {
            None => first = Some(mine),
            Some(f) => assert!(
                *f == mine,
                "parts in the order {names:?} give other copper than the design's order"
            ),
        }
    }
}
