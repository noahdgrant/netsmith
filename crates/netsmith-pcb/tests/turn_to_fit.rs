//! A part longer than the outline's short side is turned to fit, and when no turn fits the
//! refusal says so with the part's own numbers (ADR 0033, ADR 0045 section 10). The parts are
//! invented: a 60 x 5 mm header with a pad at each end, and four small two-pad parts, on an
//! outline of 40 x 80 mm.

mod common;

use netsmith_kicad::testlib::TempDir;

use netsmith_core::{Design, Features, Net, Outline, Part, PinRef};
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
    let folder = TempDir::new("turn-to-fit");
    std::fs::write(
        folder.join("LONG.kicad_mod"),
        footprint("LONG", header.0, header.1),
    )
    .unwrap();
    std::fs::write(folder.join("SMALL.kicad_mod"), footprint("SMALL", 4.0, 2.5)).unwrap();
    let part = |refdes: &str, kind: &str, package: &str| Part {
        refdes: refdes.into(),
        kind: kind.into(),
        value: package.into(),
        package: Some(package.into()),
        path: String::new(),
        pins: Vec::new(),
    };
    let pin = |refdes: &str, pin: &str| PinRef {
        refdes: refdes.into(),
        pin: pin.into(),
    };
    let mut parts = vec![part("J1", "Header", "LONG")];
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
        name: "Turn".into(),
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
        nets,
    };
    let mapping = netsmith_kicad::Mapping::parse(
        "[[part]]\nkind = \"Header\"\npackage = \"LONG\"\nsymbol = \"t:R\"\nfootprint = \"t:LONG\"\n\
         [[part]]\nkind = \"Resistor\"\npackage = \"SMALL\"\nsymbol = \"t:R\"\nfootprint = \"t:SMALL\"\n",
        "turn",
        std::path::Path::new("."),
    )
    .unwrap();
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let library = FootprintLibrary::at(netsmith_kicad::testlib::sample_footprints())
        .with_library("t", folder.path());
    Board::from_design_placed(&design, &mapping, &library, options)
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
        "J1 (Header)",
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
    assert!(message.contains("J1 (Header)"), "{message}");
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
