//! A pad keeps the process's distance from the edge of a cutout: the placer and the placement
//! check measure it the same way. The parts are an invented two-pad part whose courtyard is
//! drawn tight round its pads, as a library author may, so the courtyard keeping out of the
//! cutout is not enough to keep the copper 0.2 mm away.

mod common;

use netsmith_core::{Cutout, Design, Features, Net, Outline, Part, PinRef};
use netsmith_pcb::{Board, FootprintLibrary, PlaceOptions, Rect};

use common::process;

const TIGHT: &str = "(footprint \"TIGHT\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
    (property \"Reference\" \"REF**\" (at 0 -1.6 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
    (property \"Value\" \"TIGHT\" (at 0 1.6 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
    (attr smd) \
    (pad \"1\" smd rect (at -0.6 0) (size 0.8 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
    (pad \"2\" smd rect (at 0.6 0) (size 0.8 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
    (fp_rect (start -1 -0.5) (end 1 0.5) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n";

/// `count` tight parts, in a chain of nets, on a board of `size` mm with a cutout.
fn tight_board(count: usize, size: (f64, f64), cutout: Cutout) -> Board {
    let folder = std::env::temp_dir().join(format!("netsmith-cutout-pads-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("TIGHT.kicad_mod"), TIGHT).unwrap();
    let parts: Vec<Part> = (1..=count)
        .map(|i| Part {
            refdes: format!("R{i}"),
            kind: "Resistor".into(),
            value: "10k".into(),
            package: Some("TIGHT".into()),
            path: String::new(),
            pins: Vec::new(),
        })
        .collect();
    let nets: Vec<Net> = (1..count)
        .map(|i| Net {
            name: format!("N{i}"),
            pins: vec![
                PinRef {
                    refdes: format!("R{i}"),
                    pin: "2".into(),
                },
                PinRef {
                    refdes: format!("R{}", i + 1),
                    pin: "1".into(),
                },
            ],
        })
        .collect();
    let design = Design {
        name: "Tight".into(),
        process: Some(process()),
        features: Features {
            outline: Some(Outline {
                width: (size.0, size.0),
                height: (size.1, size.1),
                corner_radius: 0.0,
            }),
            cutouts: vec![cutout],
            ..Features::default()
        },
        parts,
        nets,
    };
    let mapping = netsmith_kicad::Mapping::parse(
        "[[part]]\nkind = \"Resistor\"\npackage = \"TIGHT\"\nsymbol = \"t:R\"\nfootprint = \"t:TIGHT\"\n",
        "tight",
        std::path::Path::new("."),
    )
    .unwrap();
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let library = FootprintLibrary::at(netsmith_kicad::testlib::sample_footprints())
        .with_library("t", folder);
    Board::from_design_placed(&design, &mapping, &library, &PlaceOptions::default())
        .unwrap_or_else(|e| panic!("{e}"))
}

/// The gap between two boxes, zero if they touch.
fn distance(a: &Rect, b: &Rect) -> f64 {
    let dx = (a.min.x - b.max.x).max(b.min.x - a.max.x).max(0.0);
    let dy = (a.min.y - b.max.y).max(b.min.y - a.max.y).max(0.0);
    dx.hypot(dy)
}

#[test]
fn pads_keep_the_edge_distance_from_a_cutout_even_when_the_courtyard_is_tight() {
    // A 24 by 14 mm board with a 12 by 4 mm slot, and 60 parts with no room to spare, so the
    // parts line the slot with only their courtyards touching it.
    let cutout = Cutout {
        name: "slot".into(),
        at: Some((6.0, 5.0)),
        size: (12.0, 4.0),
        corner_radius: 0.0,
    };
    let board = tight_board(60, (24.0, 14.0), cutout);
    assert_eq!(board.check_placement(), vec![]);
    let hole = board.cutout_shapes()[0].1.rect;
    let want = board.rules.copper_to_edge;
    for part in &board.parts {
        for pad in &part.footprint.pads {
            let gap = distance(&part.pad_bounds(pad), &hole);
            assert!(
                gap >= want - 1e-9,
                "a pad of {} is {gap:.3} mm from the cutout; the rule is {want}",
                part.refdes
            );
        }
    }
}
