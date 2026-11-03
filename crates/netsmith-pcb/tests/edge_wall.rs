//! The space the placer keeps between a courtyard and the board edge comes from the process
//! (`Rules::edge_wall`), not a fixed 2.0 mm, and a pad that sticks out of its courtyard still
//! keeps the process's copper to edge space.

mod common;

use netsmith_core::{Design, Features, Net, Outline, Part, PinRef};
use netsmith_pcb::{Board, FootprintLibrary, PlaceOptions, PlacementProblem, Rect};

use common::process;

/// A part whose two pads stick out of its courtyard by 0.6 mm on each side.
const PADS_OUT: &str = "(footprint \"PADSOUT\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
    (property \"Reference\" \"REF**\" (at 0 -1.6 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
    (property \"Value\" \"PADSOUT\" (at 0 1.6 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
    (attr smd) \
    (pad \"1\" smd rect (at -1.3 0) (size 0.8 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
    (pad \"2\" smd rect (at 1.3 0) (size 0.8 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
    (fp_rect (start -1.1 -0.5) (end 1.1 0.5) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n";

fn board(count: usize, size: (f64, f64)) -> Result<Board, netsmith_pcb::BoardError> {
    let folder = std::env::temp_dir().join(format!("netsmith-edge-wall-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("PADSOUT.kicad_mod"), PADS_OUT).unwrap();
    let parts: Vec<Part> = (1..=count)
        .map(|i| Part {
            refdes: format!("R{i}"),
            kind: "Resistor".into(),
            value: "10k".into(),
            package: Some("PADSOUT".into()),
            path: String::new(),
            pins: Vec::new(),
        })
        .collect();
    let design = Design {
        name: "Wall".into(),
        process: Some(process()),
        features: Features {
            outline: Some(Outline {
                width: (size.0, size.0),
                height: (size.1, size.1),
                corner_radius: 0.0,
            }),
            ..Features::default()
        },
        parts,
        nets: (1..count)
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
            .collect(),
    };
    let mapping = netsmith_kicad::Mapping::parse(
        "[[part]]\nkind = \"Resistor\"\npackage = \"PADSOUT\"\nsymbol = \"t:R\"\nfootprint = \"t:PADSOUT\"\n",
        "wall",
        std::path::Path::new("."),
    )
    .unwrap();
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let library = FootprintLibrary::at(netsmith_kicad::testlib::sample_footprints())
        .with_library("t", folder);
    Board::from_design_placed(&design, &mapping, &library, &PlaceOptions::default())
}

#[test]
fn the_wall_is_the_process_copper_to_edge_space_and_a_track_with_its_space() {
    let b = board(1, (20.0, 20.0)).unwrap();
    let r = &b.rules;
    assert_eq!(
        r.edge_wall(),
        r.copper_to_edge + r.clearance + r.min_track_width
    );
    assert!(r.edge_wall() < 2.0, "{}", r.edge_wall());
}

#[test]
fn parts_stand_at_the_wall_from_the_edge_where_two_millimetres_would_not_fit_them() {
    // The courtyards are 2.2 mm wide and the board 6 mm: with the 2.0 mm wall the old placer
    // had 2.0 mm of width to put them in; the process's wall (0.75 mm here) leaves 4.5 mm.
    let b = board(2, (6.0, 7.0)).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(b.check_placement(), vec![]);
    let wall = b.rules.edge_wall();
    for p in &b.parts {
        let c = p.courtyard();
        let gap = (c.min.x - b.outline.min.x)
            .min(b.outline.max.x - c.max.x)
            .min(c.min.y - b.outline.min.y)
            .min(b.outline.max.y - c.max.y);
        assert!(
            gap >= wall - 1e-9 && gap < 2.0,
            "{} is {gap} mm from the edge",
            p.refdes
        );
    }
}

#[test]
fn a_pad_outside_its_courtyard_still_keeps_the_copper_to_edge_space() {
    // The pads reach 0.6 mm past the courtyard, so the wall alone would leave them nearer
    // the edge than the process allows.
    let b = board(2, (6.0, 7.0)).unwrap_or_else(|e| panic!("{e}"));
    let want = b.rules.copper_to_edge;
    let outline: Rect = b.outline;
    for p in &b.parts {
        for pad in &p.footprint.pads {
            let r = p.pad_bounds(pad);
            let gap = (r.min.x - outline.min.x)
                .min(outline.max.x - r.max.x)
                .min(r.min.y - outline.min.y)
                .min(outline.max.y - r.max.y);
            assert!(
                gap >= want - 1e-9,
                "a pad of {} is {gap} mm from the edge",
                p.refdes
            );
        }
    }
}

#[test]
fn a_board_too_small_even_for_the_thin_wall_says_what_the_wall_is() {
    let e = board(12, (6.0, 7.0)).unwrap_err().to_string();
    assert!(
        e.contains("mm margin, on both faces)") && !e.contains("2.0 mm margin"),
        "{e}"
    );
}

#[test]
fn a_board_the_courtyards_fit_but_the_pads_do_not_is_refused_not_placed_with_pads_too_near() {
    // The courtyard (2.2 mm) and two walls (0.75 mm) need 3.7 mm; the pads reach 0.6 mm past
    // it on each side and keep 0.3 mm from the edge, which needs 4.0 mm.
    // Refused, or placed with the pad named by the placement check: never placed as if fine.
    match board(1, (3.9, 7.0)) {
        Err(e) => assert!(e.to_string().contains("the parts need"), "{e}"),
        Ok(b) => assert!(
            matches!(
                b.check_placement()[..],
                [PlacementProblem::PadNearEdge { .. }]
            ),
            "{:?}",
            b.check_placement()
        ),
    }
    let b = board(1, (4.6, 7.0)).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(b.check_placement(), vec![]);
}
