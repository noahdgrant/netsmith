//! A part longer than the outline's short side is turned to fit, and when no turn fits the
//! refusal says so with the part's own numbers (ADR 0033, ADR 0045 section 10). The parts are
//! invented: a 60 x 5 mm header with a pad at each end, and four small two-pad parts, on an
//! outline of 40 x 80 mm.

use crate::common;

use netsmith_kicad::testlib::TempDir;

use netsmith_core::{
    Design, Features, Net, Outline, Part, PinRef, PlaceBound, PlaceField, Placement,
};
use netsmith_pcb::{
    Board, BoardError, Fixed, FootprintLibrary, PlaceOptions, Pt, ReferenceSide, Side,
};

use common::process;

/// A footprint with a courtyard of `w` x `h` mm and a pad at each end of the long side.
fn footprint(name: &str, w: f64, h: f64) -> String {
    if name == "LONG" && std::env::var("THT").is_ok() {
        return through(w, h);
    }
    let (x, y) = (w / 2.0, h / 2.0);
    format!(
        "(footprint \"{name}\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
         (property \"Reference\" \"REF**\" (at 0 {ry} 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (property \"Value\" \"{name}\" (at 0 {vy} 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (attr smd) \
         (pad \"1\" smd rect (at {px} 0) (size 1 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
         (pad \"2\" smd rect (at {nx} 0) (size 1 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
         (fp_rect (start {nx2} {ny}) (end {x} {y}) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n",
        ry = -y - 1.0,
        vy = y + 1.0,
        px = -x + 1.0,
        nx = x - 1.0,
        nx2 = -x,
        ny = -y,
    )
}

/// The board: header `J1` of `header` mm (courtyard, width x height) and four small parts,
/// on an outline of `size` mm, placed as `options` say. The header is joined to each small
/// part so the placer has a reason to keep them together.
fn board(
    header: (f64, f64),
    size: (f64, f64),
    options: &PlaceOptions,
) -> Result<Board, BoardError> {
    board_with(header, size, 4, options)
}

/// [`board`] with `count` small parts.
fn board_with(
    header: (f64, f64),
    size: (f64, f64),
    count: usize,
    options: &PlaceOptions,
) -> Result<Board, BoardError> {
    board_of_kind("Header", header, size, count, Vec::new(), options)
}

/// [`board_with`] with the long part of the kind `kind` ("Header" is a connector, which the
/// placer treats apart; "Module" is an ordinary part).
fn board_of_kind(
    kind: &str,
    header: (f64, f64),
    size: (f64, f64),
    count: usize,
    constraints: Vec<Placement>,
    options: &PlaceOptions,
) -> Result<Board, BoardError> {
    let folder = TempDir::new("turn-to-fit");
    std::fs::write(
        folder.join("LONG.kicad_mod"),
        footprint("LONG", header.0, header.1),
    )
    .unwrap();
    std::fs::write(folder.join("SMALL.kicad_mod"), footprint("SMALL", 4.0, 2.5)).unwrap();
    let part = |refdes: &str, kind: &str, package: &str| Part {
        // What the design stage records for a standard connector (ADR 0068).
        provides: if kind.starts_with("Header") || kind.starts_with("Socket") {
            vec![netsmith_core::CONNECTOR_GENERIC.to_string()]
        } else {
            Vec::new()
        },
        refdes: refdes.into(),
        kind: kind.into(),
        value: package.into(),
        package: Some(package.into()),
        path: if refdes == "J1" {
            "long".into()
        } else {
            String::new()
        },
        pins: Vec::new(),
    };
    let pin = |refdes: &str, pin: &str| PinRef {
        refdes: refdes.into(),
        pin: pin.into(),
    };
    let mut parts = vec![part("J1", kind, "LONG")];
    let mut nets = Vec::new();
    for i in 1..=count {
        let refdes = format!("R{i}");
        nets.push(Net {
            name: format!("N{i}"),
            pins: vec![pin("J1", &i.to_string()), pin(&refdes, "1")],
        });
        parts.push(part(&refdes, "Resistor", "SMALL"));
    }
    // The header has two pads; the nets of the other small parts use their second pads.
    for (i, net) in nets.iter_mut().enumerate().skip(2) {
        net.pins[0] = pin(&format!("R{}", i + 1), "2");
    }
    let design = Design {
        interfaces: Vec::new(),
        notes: Vec::new(),
        name: "Turn".into(),
        process: Some(process()),
        features: Features {
            outline: Some(Outline {
                width: (size.0, size.0),
                height: (size.1, size.1),
                corner_radius: 0.0,
            }),
            constraints,
            ..Features::default()
        },
        parts,
        nets,
    };
    let mapping = netsmith_kicad::Mapping::parse(
        "[[part]]\nkind = \"Header\"\npackage = \"LONG\"\nsymbol = \"t:R\"\nfootprint = \"t:LONG\"\n\
         [[part]]\nkind = \"Module\"\npackage = \"LONG\"\nsymbol = \"t:R\"\nfootprint = \"t:LONG\"\n\
         [[part]]\nkind = \"Resistor\"\npackage = \"SMALL\"\nsymbol = \"t:R\"\nfootprint = \"t:SMALL\"\n",
        "turn",
        std::path::Path::new("."),
    )
    .unwrap();
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let library = FootprintLibrary::at(netsmith_kicad::testlib::sample_footprints())
        .with_library("t", folder.path());
    netsmith_place_route::board_from_design_placed(&design, &mapping, &library, options)
}

#[test]
fn a_header_longer_than_the_short_side_is_turned_to_fit() {
    // 60 mm of courtyard on a 40 x 80 mm outline: it only fits along the long side.
    let b = board((60.0, 5.0), (40.0, 80.0), &PlaceOptions::default())
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(b.check_placement(), vec![]);
    let j = b.part("J1").unwrap();
    assert_eq!(j.rotation.rem_euclid(180.0), 90.0, "turned {}", j.rotation);
}

#[test]
fn a_header_longer_than_the_short_side_is_turned_to_fit_with_a_crowd_of_small_parts() {
    // 40 small parts (about 70 % of the room that is left) round the 60 mm header.
    let b = board_with((60.0, 5.0), (40.0, 62.0), 40, &PlaceOptions::default())
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(b.check_placement(), vec![]);
    assert_eq!(b.part("J1").unwrap().rotation.rem_euclid(180.0), 90.0);
}

#[test]
fn a_header_that_fits_the_outline_as_it_is_keeps_its_turn() {
    let b = board((60.0, 5.0), (70.0, 40.0), &PlaceOptions::default())
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(b.check_placement(), vec![]);
    assert_eq!(b.part("J1").unwrap().rotation.rem_euclid(180.0), 0.0);
}

#[test]
fn a_header_that_fits_no_turn_is_refused_with_its_own_numbers() {
    // 60 mm of courtyard does not fit either side of a 40 x 50 mm outline.
    let message = board((60.0, 5.0), (40.0, 50.0), &PlaceOptions::default())
        .expect_err("the header cannot fit")
        .to_string();
    for wanted in [
        "`long` (J1) (Header)",
        "60.0 x 5.0 mm turned 0 degrees",
        "5.0 x 60.0 mm turned 90 degrees",
        "60.0 x 5.0 mm turned 180 degrees",
        "5.0 x 60.0 mm turned 270 degrees",
        "the outline is 40.0 x 50.0 mm",
        "38.5 x 48.5 mm",
    ] {
        assert!(message.contains(wanted), "no `{wanted}` in: {message}");
    }
    // The rows' size says nothing about what the parts need, and is not given.
    assert!(!message.contains("the parts need"), "{message}");
    assert!(!message.contains("mm over in"), "{message}");
}

/// The header locked at 0 degrees, as a layout file does.
fn locked() -> PlaceOptions {
    PlaceOptions {
        fixed: [(
            "J1".to_string(),
            Fixed {
                at: Pt::new(20.0, 10.0),
                rotation: 0.0,
                side: Side::Front,
                text: ReferenceSide::Above,
            },
        )]
        .into(),
        ..PlaceOptions::default()
    }
}

#[test]
fn a_header_whose_turn_is_locked_keeps_it_as_before() {
    // On a 40 x 80 mm outline the header would fit turned 90 degrees, but the layout file
    // locks it at 0 degrees: the stated turn wins, and the placement check says it is outside.
    let b = board((60.0, 5.0), (40.0, 80.0), &locked()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(b.part("J1").unwrap().rotation, 0.0);
    let problems: Vec<String> = b.check_placement().iter().map(|p| p.to_string()).collect();
    assert!(
        problems.iter().any(|p| p.contains("J1")),
        "the placement check finds nothing: {problems:?}"
    );
}

#[test]
fn a_refusal_names_the_locked_header_with_only_the_turn_it_has() {
    // Too many small parts as well: the placer refuses, and the header is named with the
    // one turn tried, not with turns nobody may take.
    let mut options = locked();
    options
        .sides
        .extend((1..=160).map(|i| (format!("R{i}"), Side::Front)));
    let message = board_with((60.0, 5.0), (30.0, 70.0), 160, &options)
        .expect_err("160 parts cannot fit on one face")
        .to_string();
    assert!(message.contains("`long` (J1) (Header)"), "{message}");
    assert!(
        message.contains("60.0 x 5.0 mm turned 0 degrees"),
        "{message}"
    );
    assert!(!message.contains("turned 90 degrees"), "{message}");
    assert!(
        message.contains("only the turns a layout file"),
        "{message}"
    );
}

/// A through-hole header: pads on a 2.54 mm grid along the long side in two rows.
fn through(w: f64, h: f64) -> String {
    let (x, y) = (w / 2.0, h / 2.0);
    let n = ((w - 2.0) / 2.54).floor() as usize;
    let mut pads = String::new();
    for i in 0..n {
        let px = -x + 1.5 + 2.54 * i as f64;
        pads += &format!(
            "(pad \"{}\" thru_hole circle (at {px} -1.27) (size 1.7 1.7) (drill 1) (layers \"*.Cu\" \"*.Mask\")) ",
            2 * i + 1
        );
        pads += &format!(
            "(pad \"{}\" thru_hole circle (at {px} 1.27) (size 1.7 1.7) (drill 1) (layers \"*.Cu\" \"*.Mask\")) ",
            2 * i + 2
        );
    }
    format!(
        "(footprint \"LONG\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
         (property \"Reference\" \"REF**\" (at 0 {ry} 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (property \"Value\" \"LONG\" (at 0 {vy} 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (attr through_hole) {pads} \
         (fp_rect (start {nx} {ny}) (end {x} {y}) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n",
        ry = -y - 1.0,
        vy = y + 1.0,
        nx = -x,
        ny = -y,
    )
}

/// What the design states of the long part (path `long`): its face and some bounds.
fn states(side: Option<&str>, bounds: &[(PlaceField, f64, f64)]) -> Vec<Placement> {
    vec![Placement {
        path: "long".into(),
        module: false,
        bounds: bounds
            .iter()
            .map(|&(field, lo, hi)| PlaceBound {
                field,
                lo,
                hi,
                source: String::new(),
            })
            .collect(),
        side: side.map(str::to_string),
        side_source: String::new(),
    }]
}

/// The long part as a module (not a connector, which the placer sets apart) of 37 x 15 mm
/// on a 35 x 60 mm outline with a dozen small parts: it fits only turned a quarter.
fn long_part(constraints: Vec<Placement>) -> Result<Board, BoardError> {
    board_of_kind(
        "Module",
        (37.0, 15.0),
        (35.0, 60.0),
        12,
        constraints,
        &PlaceOptions::default(),
    )
}

#[test]
fn a_part_that_states_only_a_face_is_turned_to_fit() {
    let b = long_part(states(Some("bottom"), &[])).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(b.check_placement(), vec![]);
    let j = b.part("J1").unwrap();
    assert_eq!(j.side, Side::Back);
    assert_eq!(j.rotation.rem_euclid(180.0), 90.0, "turned {}", j.rotation);
}

#[test]
fn a_part_that_states_only_a_window_on_its_edge_is_turned_to_fit_and_keeps_the_window() {
    // Its left edge 2 to 6 mm from the outline's: a window that leaves the turn free.
    for side in [None, Some("bottom")] {
        let b = long_part(states(side, &[(PlaceField::Left, 2.0, 6.0)]))
            .unwrap_or_else(|e| panic!("{side:?}: {e}"));
        assert_eq!(b.check_placement(), vec![], "{side:?}");
        let j = b.part("J1").unwrap();
        assert_eq!(
            j.rotation.rem_euclid(180.0),
            90.0,
            "{side:?}: {}",
            j.rotation
        );
        let left = j.body().min.x - b.outline.min.x;
        assert!(
            (2.0 - 1e-3..=6.0 + 1e-3).contains(&left),
            "{side:?}: left {left}"
        );
    }
}

#[test]
fn a_part_that_states_a_window_on_its_centre_is_turned_to_fit() {
    let b =
        long_part(states(None, &[(PlaceField::X, 10.0, 25.0)])).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(b.check_placement(), vec![]);
    assert_eq!(b.part("J1").unwrap().rotation.rem_euclid(180.0), 90.0);
}

#[test]
fn a_part_with_a_stated_rotation_keeps_it() {
    // Stated 90: it is the turn that fits, and it is kept.
    let b = long_part(states(
        Some("bottom"),
        &[(PlaceField::Rotation, 90.0, 90.0)],
    ))
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(b.check_placement(), vec![]);
    assert_eq!(b.part("J1").unwrap().rotation, 90.0);
    // Stated 0: 37 mm across a 35 mm board. It is not turned against the requirement, and
    // the refusal names it with the one turn it may take.
    let message = long_part(states(Some("bottom"), &[(PlaceField::Rotation, 0.0, 0.0)]))
        .expect_err("a stated turn that cannot fit")
        .to_string();
    assert!(message.contains("`long` (J1) (Module)"), "{message}");
    assert!(
        message.contains("37.0 x 15.0 mm turned 0 degrees"),
        "{message}"
    );
    assert!(!message.contains("turned 90 degrees"), "{message}");
    assert!(
        message.contains("only the turns a layout file"),
        "{message}"
    );
}

/// A board of `count` small parts forced on the front of a `size` outline: more than one face
/// holds, but not more than two faces would.
fn crowded(count: usize, size: (f64, f64)) -> netsmith_pcb::DoesNotFit {
    let mut options = PlaceOptions::default();
    options
        .sides
        .extend((1..=count).map(|i| (format!("R{i}"), Side::Front)));
    options.sides.insert("J1".into(), Side::Front);
    match board_of_kind("Module", (4.0, 2.5), size, count, Vec::new(), &options) {
        Err(BoardError::DoesNotFit(e)) => e,
        Err(e) => panic!("another refusal: {e}"),
        Ok(_) => panic!("{count} parts fit {size:?} on one face"),
    }
}

/// `crowded(120, (30.0, 40.0))`, refused once for the tests that read the refusal (when they
/// run in one process, as `cargo test` runs them; nextest runs each test in a process of its own).
fn crowded_120() -> &'static netsmith_pcb::DoesNotFit {
    static ERROR: std::sync::OnceLock<netsmith_pcb::DoesNotFit> = std::sync::OnceLock::new();
    ERROR.get_or_init(|| crowded(120, (30.0, 40.0)))
}

#[test]
fn a_refusal_leads_with_every_part_that_does_not_fit_and_how_far_out() {
    let error = crowded_120();
    let message = error.to_string();
    let first = message.lines().next().unwrap();
    // It leads with the parts that are wrong, and says how many.
    let (head, lists) = first
        .split_once(" part(s) do not fit the outline of 30.0 x 40.0 mm: ")
        .unwrap_or_else(|| panic!("{message}"));
    let count: usize = head.parse().unwrap_or_else(|_| panic!("{message}"));
    assert!(
        lists.contains("outside the outline or inside its margin, by mm past"),
        "{message}"
    );
    // Every part that is out or overlapped is in the line, not only the first few.
    let named: std::collections::BTreeSet<&str> = lists
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| {
            (w.starts_with('R') || w.starts_with('J'))
                && w.len() > 1
                && w[1..].chars().all(|c| c.is_ascii_digit())
        })
        .collect();
    assert_eq!(named.len(), count, "{first}");
    assert!(count > 10, "{message}");
    // The rows' size is not the shortfall, and no area makes it certain.
    assert!(!message.contains("the parts need"), "{message}");
    assert!(!message.contains("mm over in"), "{message}");
    assert!(!message.contains("the courtyards add up"), "{message}");
}

#[test]
fn a_refusal_that_is_certain_from_the_courtyards_alone_gives_the_area() {
    // 40 small parts on both faces of 10 x 10 mm: more courtyard than the faces hold.
    let message = crowded_both(40, (10.0, 10.0)).to_string();
    assert!(message.contains("the parts need "), "{message}");
    assert!(message.contains("the courtyards add up to "), "{message}");
    assert!(
        message.contains(" part(s) do not fit the outline of 10.0 x 10.0 mm: "),
        "{message}"
    );
}

/// [`crowded`], with the parts free to be on either face.
fn crowded_both(count: usize, size: (f64, f64)) -> netsmith_pcb::DoesNotFit {
    match board_of_kind(
        "Module",
        (4.0, 2.5),
        size,
        count,
        Vec::new(),
        &PlaceOptions::default(),
    ) {
        Err(BoardError::DoesNotFit(e)) => e,
        Err(e) => panic!("another refusal: {e}"),
        Ok(_) => panic!("{count} parts fit {size:?}"),
    }
}

#[test]
fn the_full_list_has_every_part_with_its_place_and_state() {
    let error = crowded_120();
    let report = error.full_report();
    assert_eq!(error.parts.len(), 121);
    // The message first, then one line for each of the 121 parts.
    assert!(report.starts_with(&error.to_string()), "{report}");
    let lines: Vec<&str> = report.lines().filter(|l| l.contains(" turned ")).collect();
    assert_eq!(lines.len(), 121, "{report}");
    for p in &error.parts {
        let line = lines
            .iter()
            .find(|l| {
                l.starts_with(&format!(
                    "{} (",
                    netsmith_pcb::board::member(&p.path, &p.part)
                ))
            })
            .unwrap_or_else(|| panic!("no line for {}: {report}", p.part));
        assert!(line.contains(" top at ("), "{line}");
        assert!(line.contains(" courtyard "), "{line}");
        assert!(line.ends_with(": ok") || !p.state.is_empty(), "{line}");
    }
    // A part that is out says how far, with the numbers, and one that is fine says ok.
    let state = |name: &str| &error.parts.iter().find(|p| p.part == name).unwrap().state;
    let out = error.parts.iter().find(|p| !p.state.is_empty()).unwrap();
    assert!(
        report.contains(&format!(" mm: {}\n", out.state.join("; "))),
        "{report}"
    );
    assert!(
        state(&out.part).iter().any(|s| s.contains(" mm")),
        "{out:?}"
    );
    assert!(
        error.parts.iter().any(|p| p.state.is_empty()),
        "no part is fine"
    );
}
