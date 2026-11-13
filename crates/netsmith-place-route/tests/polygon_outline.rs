//! A board whose outline is a polygon (issue #57): the placer keeps the parts inside it, the
//! placement check measures its edge, and `Edge.Cuts` and the editor's sheet draw it. The
//! router is not tested here: it routes inside the polygon's bounding box (issue #495).

use crate::common;

use netsmith_core::{Design, Features, Net, Outline, OutlineVertex, Part, PinRef};
use netsmith_kicad::testlib::TempDir;
use netsmith_pcb::{Board, BoardError, FootprintLibrary, PlaceOptions, PlacementProblem, Pt, Rect};

use common::process;

/// A 2.2 x 1 mm courtyard with two pads inside it.
const SMALL: &str = "(footprint \"SMALL\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
    (property \"Reference\" \"REF**\" (at 0 -1.6 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
    (property \"Value\" \"SMALL\" (at 0 1.6 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
    (attr smd) \
    (pad \"1\" smd rect (at -0.6 0) (size 0.6 0.8) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
    (pad \"2\" smd rect (at 0.6 0) (size 0.6 0.8) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
    (fp_rect (start -1.1 -0.5) (end 1.1 0.5) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n";

fn corner(x: f64, y: f64) -> OutlineVertex {
    OutlineVertex {
        at: (x, y),
        through: None,
    }
}

/// An L, 30 x 20 mm, with the top right 14 x 12 mm taken away.
fn ell() -> Vec<OutlineVertex> {
    vec![
        corner(0.0, 0.0),
        corner(16.0, 0.0),
        corner(16.0, 12.0),
        corner(30.0, 12.0),
        corner(30.0, 20.0),
        corner(0.0, 20.0),
    ]
}

fn board(count: usize, polygon: Vec<OutlineVertex>, size: (f64, f64)) -> Result<Board, BoardError> {
    let folder = TempDir::new("polygon-outline");
    std::fs::write(folder.join("SMALL.kicad_mod"), SMALL).unwrap();
    let parts: Vec<Part> = (1..=count)
        .map(|i| Part {
            provides: Vec::new(),
            refdes: format!("R{i}"),
            kind: "Resistor".into(),
            value: "10k".into(),
            package: Some("SMALL".into()),
            path: String::new(),
            pins: Vec::new(),
        })
        .collect();
    let design = Design {
        modules: Vec::new(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        name: "Shaped".into(),
        process: Some(process()),
        features: Features {
            outline: Some(Outline {
                width: (size.0, size.0),
                height: (size.1, size.1),
                corner_radius: 0.0,
                polygon: Some(polygon),
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
        "[[part]]\nkind = \"Resistor\"\npackage = \"SMALL\"\nsymbol = \"t:R\"\nfootprint = \"t:SMALL\"\n",
        "shaped",
        std::path::Path::new("."),
    )
    .unwrap();
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let library = FootprintLibrary::at(netsmith_kicad::testlib::sample_footprints())
        .with_library("t", folder.path());
    netsmith_place_route::board_from_design_placed(
        &design,
        &mapping,
        &library,
        &PlaceOptions::default(),
    )
}

/// The notch of the L on the sheet.
fn notch(b: &Board) -> Rect {
    let o = b.outline.min;
    Rect::new(Pt::new(o.x + 16.0, o.y), Pt::new(o.x + 30.0, o.y + 12.0))
}

#[test]
fn the_parts_of_an_l_board_are_placed_inside_the_l_and_none_in_its_notch() {
    let b = board(12, ell(), (30.0, 20.0)).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((b.outline.width(), b.outline.height()), (30.0, 20.0));
    assert!(b.polygon.is_some());
    assert_eq!(b.check_placement(), vec![]);
    let notch = notch(&b);
    for p in &b.parts {
        let c = p.courtyard();
        assert!(!c.overlaps(&notch), "{} is in the notch: {c:?}", p.refdes);
        assert!(b.edge_distance(&c) >= 0.0, "{} is outside the L", p.refdes);
        for pad in &p.footprint.pads {
            let d = b.edge_distance(&p.pad_bounds(pad));
            assert!(
                d >= b.rules.copper_to_edge - 1e-9,
                "{} pad {}: {d}",
                p.refdes,
                pad.number
            );
        }
    }
}

#[test]
fn a_part_moved_into_the_notch_is_outside_the_outline_by_how_far() {
    let mut b = board(1, ell(), (30.0, 20.0)).unwrap_or_else(|e| panic!("{e}"));
    // The courtyard's centre 1 mm above the notch's floor, well inside the box.
    let o = b.outline.min;
    b.parts[0].at = Pt::new(o.x + 20.0, o.y + 11.0);
    b.parts[0].rotation = 0.0;
    let problems = b.check_placement();
    let found = problems
        .iter()
        .find_map(|p| match p {
            PlacementProblem::OutsideOutline { past, cut, .. } => Some((past.clone(), *cut)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{problems:?}"));
    assert!(found.0.is_empty(), "inside the box: {:?}", found.0);
    let cut = found.1.expect("the depth past the polygon's edge");
    assert!((cut - 1.5).abs() < 1e-6, "{cut}");
    let text = problems
        .iter()
        .map(ToString::to_string)
        .find(|t| t.contains("R1"))
        .unwrap();
    assert_eq!(
        text,
        "R1 is not wholly inside the board outline: its courtyard is 1.50 mm past the outline's edge"
    );
}

#[test]
fn the_l_is_six_lines_on_edge_cuts_and_a_path_on_the_sheet() {
    let b = board(2, ell(), (30.0, 20.0)).unwrap_or_else(|e| panic!("{e}"));
    let pcb = netsmith_pcb::kicad_pcb(&b);
    let edge_lines: Vec<&str> = pcb
        .lines()
        .filter(|l| l.contains("Edge.Cuts") && !l.contains("user"))
        .collect();
    let lines = pcb.matches("(gr_line").count();
    assert_eq!(lines, 6, "{edge_lines:?}");
    assert!(!pcb.contains("(gr_rect"), "no box round it");
    let o = b.outline.min;
    // The inner corner of the L is a corner of the edge.
    let inner = format!("{} {}", o.x + 16.0, o.y + 12.0);
    assert!(pcb.contains(&inner), "{inner}");
    let svg = netsmith_pcb::svg::render_view(&b);
    assert!(svg.contains(r#"data-outline="polygon""#), "{svg}");
}

#[test]
fn unequal_corners_are_one_arc_on_edge_cuts() {
    // A 30 x 20 mm rectangle with a 5 mm radius at the bottom right only.
    let s = 5.0 * std::f64::consts::FRAC_1_SQRT_2;
    let polygon = vec![
        corner(0.0, 0.0),
        corner(30.0, 0.0),
        OutlineVertex {
            at: (30.0, 15.0),
            through: Some((25.0 + s, 15.0 + s)),
        },
        corner(25.0, 20.0),
        corner(0.0, 20.0),
    ];
    let b = board(2, polygon, (30.0, 20.0)).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(b.check_placement(), vec![]);
    let pcb = netsmith_pcb::kicad_pcb(&b);
    assert_eq!(pcb.matches("(gr_arc").count(), 1);
    assert_eq!(pcb.matches("(gr_line").count(), 4);
    // The cut corner is outside, the square ones are not.
    let o = b.outline.min;
    let edge = b.edge();
    assert!(!edge.contains(Pt::new(o.x + 29.5, o.y + 19.5)));
    assert!(edge.contains(Pt::new(o.x + 29.9, o.y + 0.1)));
}

#[test]
fn an_outline_that_crosses_itself_is_refused_with_where() {
    let bow = vec![
        corner(0.0, 0.0),
        corner(20.0, 20.0),
        corner(20.0, 0.0),
        corner(0.0, 20.0),
    ];
    let e = board(1, bow, (20.0, 20.0)).unwrap_err();
    assert!(matches!(e, BoardError::Outline(_)), "{e:?}");
    assert!(
        e.to_string()
            .starts_with("Shaped: the outline crosses or touches itself near"),
        "{e}"
    );
}
