//! A footprint taken from an Altium library or board (issue #1431, `import_altium_footprint`).
//!
//! There is no file saved by Altium here: each file is written record by record by
//! `altium_fixture`, at the offsets the format is documented with, and the footprints are
//! invented.

use netsmith_kicad::altium_fixture::{
    self as fx, Component, NO_COMPONENT, Pad, Shape, layer, pcbdoc, pcblib,
};
use netsmith_kicad::{FootprintSource, Sexp, import_altium_footprint};

/// A six-pin sensor: two rows of round-rectangle pads 0.95 mm apart, a silkscreen outline and
/// pin-1 dot, an assembly outline on mechanical 13, the courtyard as a region on mechanical 15,
/// its designator text and a keep-out line, in the library's frame (mm, y up).
fn sensor(component: u16, place: &dyn Fn((f64, f64)) -> (f64, f64), angle: f64, bottom: bool) -> Vec<Vec<u8>> {
    let (top, overlay) = if bottom {
        (layer::BOTTOM, 34)
    } else {
        (layer::TOP, layer::TOP_OVERLAY)
    };
    let pad_angle = |own: f64| if bottom { angle - own } else { angle + own };
    let mut out = Vec::new();
    for (i, (x, y)) in [
        (-1.3, 0.95),
        (-1.3, 0.0),
        (-1.3, -0.95),
        (1.3, -0.95),
        (1.3, 0.0),
        (1.3, 0.95),
    ]
    .into_iter()
    .enumerate()
    {
        let mut pad = Pad::smd(&(i + 1).to_string(), place((x, y)), (1.0, 0.6), Shape::RoundRect(50));
        pad.layer = top;
        pad.component = component;
        pad.angle = pad_angle(0.0);
        out.push(pad.record());
    }
    // In the order a board's streams give them: tracks, arcs, regions (and the texts apart).
    for (a, b) in [((-0.8, 1.5), (0.8, 1.5)), ((-0.8, -1.5), (0.8, -1.5))] {
        out.push(fx::track(overlay, component, place(a), place(b), 0.12));
    }
    out.push(fx::track(
        layer::mechanical(13),
        component,
        place((-0.8, -1.45)),
        place((-0.8, 1.45)),
        0.1,
    ));
    out.push(fx::track(
        layer::KEEP_OUT,
        component,
        place((0.0, -1.0)),
        place((0.0, 1.0)),
        0.1,
    ));
    out.push(fx::arc(
        overlay,
        component,
        place((-2.0, 1.4)),
        0.1,
        (0.0, 0.0),
        0.2,
    ));
    out.push(fx::region(
        layer::mechanical(15),
        component,
        &[(-2.1, -1.7), (2.1, -1.7), (2.1, 1.7), (-2.1, 1.7)].map(place),
    ));
    out.push(fx::text(overlay, component, ".Designator"));
    out
}

fn same((x, y): (f64, f64)) -> (f64, f64) {
    (x, y)
}

/// An expression on one line.
fn compact(s: &Sexp) -> String {
    match s {
        Sexp::Atom(a) => a.clone(),
        Sexp::Str(t) => format!("{t:?}"),
        Sexp::List(items) => format!(
            "({})",
            items.iter().map(compact).collect::<Vec<_>>().join(" ")
        ),
    }
}

/// The pads of a footprint on one line each, without their layers.
fn pads(text: &str) -> Vec<String> {
    let root = Sexp::parse(text).unwrap();
    root.find_all("pad")
        .map(|p| {
            let short: Vec<Sexp> = p
                .items()
                .iter()
                .filter(|i| !matches!(i.head(), Some("layers")))
                .cloned()
                .collect();
            compact(&Sexp::List(short))
        })
        .collect()
}

#[test]
fn a_library_footprint_becomes_a_kicad_footprint_seen_from_the_top() {
    let file = pcblib(&[("SENSOR-6", sensor(NO_COMPONENT, &same, 0.0, false))]);
    let got = import_altium_footprint(&file, FootprintSource::File, None, None).unwrap();
    assert_eq!(got.source_name, "SENSOR-6");
    assert_eq!(got.name, "SENSOR-6");
    assert!(!got.from_bottom);
    let text = &got.text;
    assert_eq!(
        pads(text),
        [
            "(pad \"1\" smd roundrect (at -1.3 -0.95) (size 1 0.6) (roundrect_rratio 0.25))",
            "(pad \"2\" smd roundrect (at -1.3 0) (size 1 0.6) (roundrect_rratio 0.25))",
            "(pad \"3\" smd roundrect (at -1.3 0.95) (size 1 0.6) (roundrect_rratio 0.25))",
            "(pad \"4\" smd roundrect (at 1.3 0.95) (size 1 0.6) (roundrect_rratio 0.25))",
            "(pad \"5\" smd roundrect (at 1.3 0) (size 1 0.6) (roundrect_rratio 0.25))",
            "(pad \"6\" smd roundrect (at 1.3 -0.95) (size 1 0.6) (roundrect_rratio 0.25))",
        ]
    );
    let flat = compact(&Sexp::parse(text).unwrap());
    for want in [
        "(footprint \"SENSOR-6\" (version 20241229) (generator \"netsmith\") (layer \"F.Cu\")",
        "(property \"Reference\" \"REF**\"",
        "(property \"Value\" \"SENSOR-6\"",
        "(attr smd)",
        "(layers \"F.Cu\" \"F.Paste\" \"F.Mask\")",
        "(fp_line (start -0.8 -1.5) (end 0.8 -1.5) (stroke (width 0.12) (type solid)) (layer \"F.SilkS\"))",
        "(fp_circle (center -2 -1.4) (end -1.9 -1.4)",
        "(fp_line (start -0.8 1.45) (end -0.8 -1.45) (stroke (width 0.1) (type solid)) (layer \"F.Fab\"))",
        "(fp_poly (pts (xy -2.1 1.7) (xy 2.1 1.7) (xy 2.1 -1.7) (xy -2.1 -1.7)) (stroke (width 0.05) (type solid)) (fill no) (layer \"F.CrtYd\"))",
    ] {
        assert!(flat.contains(want), "{want}\n{text}");
    }
    assert!(!flat.contains("Designator"), "{text}");
    assert_eq!(
        got.notes,
        [
            "1 graphic(s) on a layer not taken (keep-out, or a mechanical layer other than 13 and 15) left out",
            "1 text(s) left out: the reference is `REF**` and the value the footprint's name",
        ]
    );
}

#[test]
fn through_hole_pads_keep_their_holes_and_shapes() {
    let mut slot = Pad::through("3", (5.0, 0.0), (1.2, 2.4), Shape::Round, 0.8);
    slot.slot = Some((2.0, 90.0));
    let mut free = Pad::through("", (0.0, -3.0), (3.0, 3.0), Shape::Round, 3.0);
    free.plated = false;
    let mut turned = Pad::through("4", (7.5, 0.0), (1.7, 1.7), Shape::Octagon, 1.0);
    turned.angle = 90.0;
    let records = vec![
        Pad::through("1", (0.0, 0.0), (1.7, 1.7), Shape::Rect, 1.0).record(),
        Pad::through("2", (2.5, 0.0), (1.7, 1.7), Shape::Round, 1.0).record(),
        slot.record(),
        turned.record(),
        free.record(),
    ];
    let file = pcblib(&[("Terminal_1x4", records)]);
    let got = import_altium_footprint(&file, FootprintSource::File, None, Some("Terminal")).unwrap();
    assert_eq!(got.name, "Terminal");
    assert_eq!(got.source_name, "Terminal_1x4");
    assert_eq!(
        pads(&got.text),
        [
            "(pad \"1\" thru_hole rect (at 0 0) (size 1.7 1.7) (drill 1))",
            "(pad \"2\" thru_hole circle (at 2.5 0) (size 1.7 1.7) (drill 1))",
            "(pad \"3\" thru_hole oval (at 5 0) (size 1.2 2.4) (drill oval 0.8 2))",
            "(pad \"4\" thru_hole roundrect (at 7.5 0 90) (size 1.7 1.7) (drill 1) (roundrect_rratio 0) (chamfer_ratio 0.25) (chamfer top_left top_right bottom_left bottom_right))",
            "(pad \"\" np_thru_hole circle (at 0 3) (size 3 3) (drill 3))",
        ]
    );
    assert!(got.text.contains("(attr through_hole)"), "{}", got.text);
    assert!(got.text.contains("(layers \"*.Cu\" \"*.Mask\")"), "{}", got.text);
    assert!(
        got.notes
            .contains(&"no courtyard: nothing is drawn on mechanical layer 15; draw one on F.CrtYd".to_string()),
        "{:?}",
        got.notes
    );
}

/// The board's place of a point of the library's frame: mirrored for the bottom (x negated),
/// turned by `angle`, moved to `origin`.
fn placed(origin: (f64, f64), angle: f64, bottom: bool) -> impl Fn((f64, f64)) -> (f64, f64) {
    move |(x, y)| {
        let x = if bottom { -x } else { x };
        let (s, c) = angle.to_radians().sin_cos();
        (origin.0 + x * c - y * s, origin.1 + x * s + y * c)
    }
}

#[test]
fn a_part_of_a_board_comes_back_as_the_library_draws_it() {
    let lib = pcblib(&[("SENSOR-6", sensor(NO_COMPONENT, &same, 0.0, false))]);
    let want = import_altium_footprint(&lib, FootprintSource::File, None, None).unwrap();
    for (angle, bottom) in [(0.0, false), (90.0, false), (90.0, true), (210.0, true)] {
        let place = placed((20.0, 30.0), angle, bottom);
        // `R1` at index 0 is another part; the sensor is `U3` at index 1.
        let other = Pad::smd("1", (5.0, 5.0), (0.5, 0.5), Shape::Rect);
        let mut other = other;
        other.component = 0;
        let (mut pads, mut rest) = (vec![other.record()], Vec::new());
        for r in sensor(1, &place, angle, bottom) {
            if r[0] == 2 {
                pads.push(r);
            } else {
                rest.push(r);
            }
        }
        let tracks: Vec<Vec<u8>> = rest.iter().filter(|r| r[0] == 4).cloned().collect();
        let arcs: Vec<Vec<u8>> = rest.iter().filter(|r| r[0] == 1).cloned().collect();
        let regions: Vec<Vec<u8>> = rest.iter().filter(|r| r[0] == 11).cloned().collect();
        let texts: Vec<Vec<u8>> = rest.iter().filter(|r| r[0] == 5).cloned().collect();
        let board = pcbdoc(
            &[
                Component {
                    designator: "R1",
                    pattern: "R0402",
                    at: (5.0, 5.0),
                    angle: 0.0,
                    bottom: false,
                },
                Component {
                    designator: "U3",
                    pattern: "SENSOR-6",
                    at: (20.0, 30.0),
                    angle,
                    bottom,
                },
            ],
            &[
                ("Pads6", pads),
                ("Tracks6", tracks),
                ("Arcs6", arcs),
                ("Regions6", regions),
                ("Texts6", texts),
            ],
        );
        let got = import_altium_footprint(&board, FootprintSource::Board { reference: "U3" }, None, None)
            .unwrap();
        assert_eq!(got.from_bottom, bottom);
        assert_eq!(got.board_angle, angle);
        assert_eq!(got.source_name, "SENSOR-6");
        assert_eq!(got.text, want.text, "turned {angle}, bottom {bottom}");
        assert_eq!(got.notes, want.notes);
    }
}

#[test]
fn what_cannot_be_imported_says_why() {
    let two = pcblib(&[
        ("B-PART", vec![Pad::smd("1", (0.0, 0.0), (1.0, 1.0), Shape::Rect).record()]),
        ("A-PART", vec![Pad::smd("1", (0.0, 0.0), (1.0, 1.0), Shape::Rect).record()]),
    ]);
    let board = pcbdoc(
        &[Component {
            designator: "R1",
            pattern: "R0402",
            at: (0.0, 0.0),
            angle: 0.0,
            bottom: false,
        }],
        &[],
    );
    let mut cut = pcblib(&[("X", vec![Pad::smd("1", (0.0, 0.0), (1.0, 1.0), Shape::Rect).record()])]);
    // A pad record whose length runs past the end of its stream.
    let short = {
        let mut r = Pad::smd("1", (0.0, 0.0), (1.0, 1.0), Shape::Rect).record();
        r.truncate(40);
        pcblib(&[("X", vec![r])])
    };
    cut.truncate(1024);
    let cases: [(&[u8], FootprintSource, &str); 6] = [
        (
            &two,
            FootprintSource::File,
            "the library holds 2 footprints (A-PART, B-PART): name the one to import with `--footprint NAME`",
        ),
        (
            &board,
            FootprintSource::Board { reference: "U9" },
            "the board has no part `U9`; its parts are R1",
        ),
        (
            b"(footprint \"X\")",
            FootprintSource::File,
            "not an Altium library or board: not a compound (OLE) file",
        ),
        (
            b"|RECORD=Board|KIND=Protel_Advanced_PCB|",
            FootprintSource::Board { reference: "U1" },
            "an Altium file saved as ASCII: save it in Altium as binary to import it",
        ),
        (
            &short,
            FootprintSource::File,
            "footprint `X`: a record of ",
        ),
        (
            &two,
            FootprintSource::Board { reference: "U1" },
            "not an Altium board: it has no `Components6` stream",
        ),
    ];
    for (bytes, source, want) in cases {
        let got = import_altium_footprint(bytes, source, None, None).unwrap_err();
        assert!(got.starts_with(want), "{got}\n{want}");
    }
    // A file cut short is an error, not a panic.
    assert!(import_altium_footprint(&cut, FootprintSource::File, None, None).is_err());
}

#[test]
fn a_library_of_several_footprints_gives_the_one_named() {
    let pad = |n: &str, x: f64| Pad::smd(n, (x, 0.0), (1.0, 1.0), Shape::Rect).record();
    let lib = pcblib(&[
        ("B-PART", vec![pad("1", 0.0), pad("2", 1.0)]),
        ("A-PART", vec![pad("1", 0.0)]),
    ]);
    let pick = |name| import_altium_footprint(&lib, FootprintSource::File, Some(name), None);
    let a = pick("A-PART").unwrap();
    assert_eq!(a.source_name, "A-PART");
    assert_eq!(a.text.matches("(pad ").count(), 1);
    let b = pick("B-PART").unwrap();
    assert_eq!(b.source_name, "B-PART");
    assert_eq!(b.text.matches("(pad ").count(), 2);
    assert_eq!(
        pick("C-PART").unwrap_err(),
        "the library has no footprint `C-PART`; its footprints are A-PART, B-PART"
    );
    // A library of one is asked by name too.
    let one = pcblib(&[("A-PART", vec![pad("1", 0.0)])]);
    let err = import_altium_footprint(&one, FootprintSource::File, Some("B-PART"), None).unwrap_err();
    assert_eq!(
        err,
        "the library has no footprint `B-PART`; its footprints are A-PART"
    );
}

#[test]
fn a_footprint_larger_than_the_mini_stream_reads_from_whole_sectors() {
    // Forty pads make a `Data` stream past the 4096 bytes under which a stream is in the mini
    // stream.
    let records: Vec<Vec<u8>> = (0..40)
        .map(|i| Pad::smd(&(i + 1).to_string(), (f64::from(i) * 0.5, 0.0), (0.3, 1.0), Shape::Rect).record())
        .collect();
    assert!(records.concat().len() > 4096);
    let file = pcblib(&[("QFN-40", records)]);
    let got = import_altium_footprint(&file, FootprintSource::File, None, None).unwrap();
    let pads = pads(&got.text);
    assert_eq!(pads.len(), 40);
    assert_eq!(pads[39], "(pad \"40\" smd rect (at 19.5 0) (size 0.3 1))");
}
