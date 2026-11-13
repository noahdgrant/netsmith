//! Trimmed pin protrusions (ADR 0045 section 15; owner decisions of 2026-11-04): a
//! flat part on the back over the protruding pins of a 1x10 socket row on the front.
//!
//! The board is invented: 60 x 40 mm, a socket row (ten plated holes, 2.54 mm apart, its
//! courtyard drawn on its own face only), and a flat 3.4 mm part 30 x 26 mm with a pad at
//! each end, on the back under a lid zone that allows 3.6 mm there. The requirements pin both
//! over each other. The tests give the failure modes of the note's section 1.8: F1 (too
//! tall), F2 (no height), F3 (the pins are not stated), F5 (assembled by the board house),
//! F6 (no height limit there), and the unchanged board without a protrusion.

use crate::common;

use netsmith_core::{
    Design, Features, Keepout, Outline, Part, PartHeight, PinProtrusion, PlaceBound, PlaceField,
    Placement,
};
use netsmith_pcb::{Board, BoardError, FootprintLibrary, PlaceOptions, PlacementProblem};
use netsmith_testkit::TempDir;

use common::process;

/// The socket row: ten plated holes along y, 2.54 mm apart, a courtyard on `F.CrtYd` only.
fn socket() -> String {
    let pads: String = (0..10)
        .map(|i| {
            format!(
                "(pad \"{}\" thru_hole circle (at 0 {:.2}) (size 1.7 1.7) (drill 1) (layers \"*.Cu\" \"*.Mask\")) ",
                i + 1,
                f64::from(i) * 2.54 - 11.43
            )
        })
        .collect();
    format!(
        "(footprint \"Row10\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
         (property \"Reference\" \"REF**\" (at 0 -14 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (property \"Value\" \"Row10\" (at 0 14 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (attr through_hole) {pads} \
         (fp_rect (start -1.3 -12.7) (end 1.3 12.7) (stroke (width 0.1) (type solid)) (fill none) (layer \"F.Fab\")) \
         (fp_rect (start -1.8 -13) (end 1.8 13) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n"
    )
}

/// The flat part: 30 x 26 mm of courtyard, a pad at each end of its long side.
fn flat() -> String {
    "(footprint \"Flat\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
     (property \"Reference\" \"REF**\" (at 0 -14 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
     (property \"Value\" \"Flat\" (at 0 14 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
     (attr smd) \
     (pad \"1\" smd rect (at -12 0) (size 2 2) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
     (pad \"2\" smd rect (at 12 0) (size 2 2) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
     (fp_rect (start -14.5 -12.5) (end 14.5 12.5) (stroke (width 0.1) (type solid)) (fill none) (layer \"F.Fab\")) \
     (fp_rect (start -15 -13) (end 15 13) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n"
        .to_string()
}

/// What the invented board states; the default is the working case of the note.
#[derive(Clone)]
struct Case {
    /// The protrusion of the socket row, if the design states it.
    protrusion: Option<f64>,
    /// The cell's height.
    height: Option<f64>,
    /// Whether the cell is fitted by hand.
    hand: bool,
    /// The lid zone over the back, if any.
    zone: Option<f64>,
}

impl Default for Case {
    fn default() -> Self {
        Case {
            protrusion: Some(0.2),
            height: Some(3.4),
            hand: true,
            zone: Some(3.6),
        }
    }
}

fn bound(field: PlaceField, lo: f64, hi: f64) -> PlaceBound {
    PlaceBound {
        field,
        lo,
        hi,
        source: String::new(),
    }
}

fn design(case: &Case) -> Design {
    let part = |refdes: &str, kind: &str, package: &str, path: &str| Part {
        // What the design stage records for a standard connector (ADR 0068).
        provides: Vec::new(),
        class: netsmith_testkit::std_class(kind).and_then(netsmith_core::PartClass::from_word),
        refdes: refdes.into(),
        kind: kind.into(),
        value: kind.into(),
        package: Some(package.into()),
        path: path.into(),
        pins: Vec::new(),
    };
    let pinned = |path: &str, side: &str| Placement {
        path: path.into(),
        module: false,
        bounds: vec![
            bound(PlaceField::X, 29.5, 30.5),
            bound(PlaceField::Y, 19.5, 20.5),
            bound(PlaceField::Rotation, 0.0, 0.0),
        ],
        side: Some(side.into()),
        side_source: String::new(),
    };
    Design {
        modules: Vec::new(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        name: "Protrusion".into(),
        process: Some(process()),
        features: Features {
            outline: Some(Outline {
                width: (60.0, 60.0),
                height: (40.0, 40.0),
                corner_radius: 0.0,
                polygon: None,
            }),
            keepouts: case
                .zone
                .map(|limit| Keepout {
                    name: "back_room".into(),
                    part: None,
                    at: Some((0.0, 0.0)),
                    size: (60.0, 40.0),
                    layers: vec!["bottom".into()],
                    tracks: false,
                    vias: false,
                    copper: false,
                    parts: false,
                    max_height: Some(limit),
                })
                .into_iter()
                .collect(),
            heights: case
                .height
                .map(|height| PartHeight {
                    path: "cell".into(),
                    height,
                })
                .into_iter()
                .collect(),
            protrusions: case
                .protrusion
                .map(|length| PinProtrusion {
                    path: "row".into(),
                    length,
                    source: "widget.physical.nsm:12".into(),
                })
                .into_iter()
                .collect(),
            hand_fitted: if case.hand {
                vec!["cell".into(), "row".into()]
            } else {
                vec!["row".into()]
            },
            constraints: vec![pinned("row", "top"), pinned("cell", "bottom")],
            ..Features::default()
        },
        parts: vec![
            part("J1", "Socket", "Row10", "row"),
            part("U1", "FlatCell", "FlatCell", "cell"),
        ],
        nets: Vec::new(),
    }
}

fn board(case: &Case) -> Result<Board, BoardError> {
    let folder = TempDir::new("protrusions");
    std::fs::write(folder.join("Row10.kicad_mod"), socket()).unwrap();
    std::fs::write(folder.join("Flat.kicad_mod"), flat()).unwrap();
    let mapping = "[[part]]\nkind = \"Socket\"\npackage = \"Row10\"\nsymbol = \"t:R\"\nfootprint = \"t:Row10\"\n\
                   [[part]]\nkind = \"FlatCell\"\npackage = \"FlatCell\"\nsymbol = \"t:R\"\nfootprint = \"t:Flat\"\n";
    let mapping = netsmith_kicad::Mapping::parse(mapping, "protrusions", std::path::Path::new("."))
        .unwrap_or_else(|e| panic!("{e}"));
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let library = FootprintLibrary::at(netsmith_testkit::sample_footprints())
        .with_library("t", folder.path());
    netsmith_placer::board_from_design_placed(
        &design(case),
        &mapping,
        &library,
        &PlaceOptions::default(),
    )
}

/// The lines the placement check finds, as the person reads them.
fn findings(board: &Board) -> Vec<String> {
    board
        .check_placement()
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn a_flat_part_stands_over_protruding_pins_when_the_stack_fits_the_zone() {
    let board = board(&Case::default()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(findings(&board), Vec::<String>::new(), "a clean placement");
    // It really lies over the pins: the cell's courtyard is over the socket's pads.
    let (row, cell) = (board.part("J1").unwrap(), board.part("U1").unwrap());
    assert!(
        cell.courtyard()
            .overlaps(&row.pad_bounds(&row.footprint.pads[4]))
    );
    // The note lists the cut, the length, the covering part and the line.
    let trims = board.trims();
    assert_eq!(trims.len(), 1);
    let note = trims[0].note();
    for expected in [
        "J1 (row)",
        "0.20 mm",
        "protrude 0.20 mm from the bottom face",
        "before U1 (cell) is fitted",
        "widget.physical.nsm:12",
    ] {
        assert!(note.contains(expected), "missing {expected:?} in {note}");
    }
    assert_eq!(
        board.excused_courtyards(),
        vec![("J1".to_string(), "U1".to_string())]
    );
    // The zone is where the stack was compared with something: nothing is unchecked.
    assert!(trims[0].unchecked().is_empty());
}

#[test]
fn without_a_protrusion_the_same_board_is_refused_as_before_and_the_hint_names_the_member() {
    let case = Case {
        protrusion: None,
        ..Case::default()
    };
    // The requirements pin both over each other, so the placer cannot move either: the
    // overlap of today's rule (F3), with how to state the protrusion.
    let found = match board(&case) {
        Ok(board) => findings(&board),
        Err(e) => vec![e.to_string()],
    };
    let text = found.join("\n");
    let overlap =
        text.contains("the courtyards of `row` (J1) and ") && text.contains("(U1) overlap");
    assert!(overlap || text.contains("put on top"), "{text}");
    if overlap {
        assert!(
            text.contains("`.row.pin_protrusion &= 0.2mm`"),
            "the hint names the member: {text}"
        );
    }
}

#[test]
fn f1_a_protrusion_that_leaves_too_little_room_is_named_with_the_numbers() {
    let case = Case {
        protrusion: Some(0.4),
        ..Case::default()
    };
    let text = match board(&case) {
        Ok(board) => findings(&board).join("\n"),
        Err(e) => e.to_string(),
    };
    for expected in [
        "U1",
        "`cell`",
        "J1",
        "widget.physical.nsm:12",
        "0.40 mm",
        "3.80 mm",
        "back_room",
        "3.60 mm",
        "0.20 mm too much",
        "cut the pins to protrude 0.20 mm or less",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in\n{text}");
    }
}

#[test]
fn f2_a_covering_part_with_no_height_is_an_error_naming_the_member_to_state() {
    let case = Case {
        height: None,
        ..Case::default()
    };
    let text = match board(&case) {
        Ok(board) => findings(&board).join("\n"),
        Err(e) => e.to_string(),
    };
    assert!(
        text.contains("states no height") && text.contains(".cell.height"),
        "{text}"
    );
}

#[test]
fn f5_a_covering_part_the_board_house_assembles_is_an_error() {
    let case = Case {
        hand: false,
        ..Case::default()
    };
    let text = match board(&case) {
        Ok(board) => findings(&board).join("\n"),
        Err(e) => e.to_string(),
    };
    assert!(
        text.contains("assembled by the board house") && text.contains("`hand`"),
        "{text}"
    );
}

#[test]
fn f6_with_no_height_limit_there_the_overlap_is_excused_and_the_note_says_nothing_was_compared() {
    let case = Case {
        zone: None,
        ..Case::default()
    };
    let board = board(&case).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(findings(&board), Vec::<String>::new());
    let unchecked = board.trims()[0].unchecked();
    assert_eq!(unchecked.len(), 1, "{unchecked:?}");
    assert!(unchecked[0].contains("no height limit covers `cell` (U1)"));
}

#[test]
fn the_placer_moves_a_free_cover_off_pins_it_may_not_stand_over() {
    // Not pinned: the cell may go anywhere on the back, and with the trim refused (the
    // board house fits it) it is put clear of the socket's pads instead of over them.
    let mut d = design(&Case {
        hand: false,
        ..Case::default()
    });
    d.features
        .constraints
        .retain(|c| c.path != "cell" || c.side.is_some());
    d.features.constraints[1].bounds.clear();
    let folder = TempDir::new("protrusions-free");
    std::fs::write(folder.join("Row10.kicad_mod"), socket()).unwrap();
    std::fs::write(folder.join("Flat.kicad_mod"), flat()).unwrap();
    let mapping = "[[part]]\nkind = \"Socket\"\npackage = \"Row10\"\nsymbol = \"t:R\"\nfootprint = \"t:Row10\"\n\
                   [[part]]\nkind = \"FlatCell\"\npackage = \"FlatCell\"\nsymbol = \"t:R\"\nfootprint = \"t:Flat\"\n";
    let mapping = netsmith_kicad::Mapping::parse(mapping, "protrusions", std::path::Path::new("."))
        .unwrap_or_else(|e| panic!("{e}"));
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let library = FootprintLibrary::at(netsmith_testkit::sample_footprints())
        .with_library("t", folder.path());
    let placed =
        netsmith_placer::board_from_design_placed(&d, &mapping, &library, &PlaceOptions::default());
    // The cell is 30 mm wide and the board 60: it fits beside the row, on the back.
    if let Ok(board) = placed {
        assert!(
            !findings(&board)
                .iter()
                .any(|f| f.contains("lies over the pins")),
            "{:?}",
            findings(&board)
        );
        assert!(
            !board
                .check_placement()
                .iter()
                .any(|p| matches!(p, PlacementProblem::Overlap { .. }))
        );
    }
}

/// What the lines are proved to conflict in, as a person reads it: the placer refuses a
/// board whose pinned parts cannot hold (the proof comes before any placing), and a board it
/// does place is explained afterwards (ADR 0045 sections 13 and 14).
fn conflicts_of(case: &Case) -> String {
    match board(case) {
        Err(e) => e.to_string(),
        Ok(board) => board
            .explain_requirements(&design(case), &PlaceOptions::default())
            .map(|r| r.conflicts.join("\n"))
            .unwrap_or_default(),
    }
}

#[test]
fn the_conflict_explanation_proves_a_cover_pinned_over_protruding_pins_that_the_rule_refuses() {
    let too_tall = conflicts_of(&Case {
        protrusion: Some(0.4),
        ..Case::default()
    });
    for expected in [
        "cannot both hold",
        "U1",
        "J1",
        "pins protruding 0.4 mm",
        "wherever it is put",
        "3.80 mm",
        "back_room",
        "widget.physical.nsm:12",
    ] {
        assert!(
            too_tall.contains(expected),
            "missing {expected:?} in {too_tall}"
        );
    }
    let house = conflicts_of(&Case {
        hand: false,
        ..Case::default()
    });
    assert!(house.contains("assembled by the board house"), "{house}");
    let flat_out = conflicts_of(&Case {
        height: None,
        ..Case::default()
    });
    assert!(flat_out.contains("states no height"), "{flat_out}");
    // When the stack fits, there is nothing to prove.
    assert_eq!(conflicts_of(&Case::default()), "");
}

/// The board of `case` made small (34 x 30 mm) and with only the faces stated, so that the
/// placer has to find the places: the cell's courtyard leaves no room beside the row.
fn small(case: &Case) -> Result<Board, BoardError> {
    let mut d = design(case);
    d.features.outline = Some(Outline {
        width: (34.0, 34.0),
        height: (30.0, 30.0),
        corner_radius: 0.0,
        polygon: None,
    });
    d.features.keepouts[0].size = (34.0, 30.0);
    for c in &mut d.features.constraints {
        c.bounds.clear();
    }
    let folder = TempDir::new("protrusions-small");
    std::fs::write(folder.join("Row10.kicad_mod"), socket()).unwrap();
    std::fs::write(folder.join("Flat.kicad_mod"), flat()).unwrap();
    let mapping = "[[part]]\nkind = \"Socket\"\npackage = \"Row10\"\nsymbol = \"t:R\"\nfootprint = \"t:Row10\"\n\
                   [[part]]\nkind = \"FlatCell\"\npackage = \"FlatCell\"\nsymbol = \"t:R\"\nfootprint = \"t:Flat\"\n";
    let mapping = netsmith_kicad::Mapping::parse(mapping, "protrusions", std::path::Path::new("."))
        .unwrap_or_else(|e| panic!("{e}"));
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let library = FootprintLibrary::at(netsmith_testkit::sample_footprints())
        .with_library("t", folder.path());
    netsmith_placer::board_from_design_placed(&d, &mapping, &library, &PlaceOptions::default())
}

#[test]
fn the_placer_puts_the_cover_over_the_protruding_pins_when_nothing_else_fits() {
    let with = small(&Case::default()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(findings(&with), Vec::<String>::new());
    assert_eq!(
        with.trims()[0].covers.len(),
        1,
        "the cell lies over the row"
    );
    // Without the protrusion stated the same board does not place: nothing is room for both.
    let without = small(&Case {
        protrusion: None,
        ..Case::default()
    });
    let text = match without {
        Ok(b) => findings(&b).join("\n"),
        Err(e) => e.to_string(),
    };
    assert!(!text.is_empty(), "the board placed without the protrusion");
}
