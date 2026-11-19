//! A footprint taken from a KiCad board or footprint file (issue #1424, `import_footprint`).

use netsmith_kicad::{FootprintSource, Sexp, import_footprint};

/// The footprint a library holds, drawn from the top.
const LIBRARY: &str = r#"(footprint "Chip" (version 20241229) (generator "pcbnew") (layer "F.Cu")
  (property "Reference" "REF**" (at 0 -2 0) (layer "F.SilkS") (effects (font (size 1 1))))
  (property "Value" "Chip" (at 0 2 0) (layer "F.Fab") (effects (font (size 1 1))))
  (property "Datasheet" "" (at 0 0 0) (layer "F.Fab") (hide yes) (effects (font (size 1 1))))
  (attr smd)
  (fp_rect (start -1.5 -1) (end 1.5 1) (stroke (width 0.05) (type solid)) (fill none) (layer "F.CrtYd"))
  (fp_line (start -1 -0.8) (end 1 -0.8) (stroke (width 0.1) (type solid)) (layer "F.Fab"))
  (pad "1" smd rect (at -1 -0.5) (size 0.6 0.4) (layers "F.Cu" "F.Mask" "F.Paste"))
  (pad "2" smd rect (at 1 0.5 90) (size 0.6 0.4) (layers "F.Cu" "F.Mask" "F.Paste"))
  (model "${KIPRJMOD}/chip.step" (offset (xyz 0 0.5 0)) (scale (xyz 1 1 1)) (rotate (xyz 0 0 90)))
)"#;

/// The same footprint as KiCad stores it on a board: on the bottom face, turned by 90 degrees,
/// with the board's nets, ids and the symbol's fields. Every y of its frame is negated, each layer
/// is on the other face, a pad's or a text's angle is the board's 90 less its own, and the texts
/// are mirrored.
const BOARD: &str = r#"(kicad_pcb (version 20260206) (generator "pcbnew")
  (footprint "Vendor:R_0402" (layer "F.Cu") (uuid "a0") (at 1 1)
    (property "Reference" "R1" (at 0 -1 0) (layer "F.SilkS"))
    (pad "1" smd rect (at -0.5 0) (size 0.5 0.5) (layers "F.Cu"))
    (pad "2" smd rect (at 0.5 0) (size 0.5 0.5) (layers "F.Cu"))
  )
  (footprint "Vendor:SOT-X" (layer "B.Cu") (uuid "a1") (at 10 20 90)
    (property "Reference" "U3" (at 0 2 90) (layer "B.SilkS") (uuid "a2")
      (effects (font (size 1 1)) (justify mirror)))
    (property "Value" "LM1" (at 0 -2 90) (layer "B.Fab") (effects (font (size 1 1)) (justify mirror)))
    (property "Datasheet" "" (at 0 0 90) (layer "B.Fab") (hide yes) (effects (font (size 1 1)) (justify mirror)))
    (property "Footprint" "Vendor:SOT-X" (at 0 0 90) (layer "B.Fab") (hide yes))
    (property "MPN" "LM1-X" (at 0 0 90) (layer "B.Fab") (hide yes))
    (path "/0001") (sheetname "/") (sheetfile "eval.kicad_sch")
    (attr smd)
    (fp_rect (start -1.5 1) (end 1.5 -1) (stroke (width 0.05) (type solid)) (fill none) (layer "B.CrtYd") (uuid "a3"))
    (fp_line (start -1 0.8) (end 1 0.8) (stroke (width 0.1) (type solid)) (layer "B.Fab") (uuid "a4"))
    (pad "1" smd rect (at -1 0.5 90) (size 0.6 0.4) (layers "B.Cu" "B.Mask" "B.Paste")
      (net 1 "GND") (pinfunction "A") (pintype "passive") (uuid "a5"))
    (pad "2" smd rect (at 1 -0.5) (size 0.6 0.4) (layers "B.Cu" "B.Mask" "B.Paste")
      (net "VCC") (uuid "a6"))
    (model "${KIPRJMOD}/chip.step" (offset (xyz 0 0.5 0)) (scale (xyz 1 1 1)) (rotate (xyz 0 0 90)))
  )
)"#;

fn parse(text: &str) -> Sexp {
    Sexp::parse(text).expect("the footprint written parses")
}

fn children<'a>(fp: &'a Sexp, head: &'a str) -> Vec<&'a Sexp> {
    fp.find_all(head).collect()
}

/// Every list of the tree, depth first: to look for a field anywhere.
fn all_heads(node: &Sexp, out: &mut Vec<String>) {
    if let Some(h) = node.head() {
        out.push(h.to_string());
    }
    for item in node.args() {
        all_heads(item, out);
    }
}

#[test]
fn a_part_of_a_board_on_the_bottom_turned_comes_back_as_the_library_draws_it() {
    let got = import_footprint(
        BOARD,
        FootprintSource::Board { reference: "U3" },
        Some("Chip"),
    )
    .expect("U3 is on the board");
    assert_eq!(got.source_name, "SOT-X");
    assert_eq!(got.name, "Chip");
    assert!(got.from_bottom);
    assert_eq!(got.board_angle, 90.0);

    let out = parse(&got.text);
    let library = parse(LIBRARY);
    assert_eq!(out.head(), Some("footprint"));
    assert_eq!(out.args()[0], Sexp::string("Chip"));
    // The board's version, so KiCad reads the fields it wrote.
    assert_eq!(out.find("version"), Some(&parse("(version 20260206)")));
    assert_eq!(out.find("layer"), library.find("layer"));
    for head in ["fp_rect", "fp_line", "attr", "model"] {
        let strip = |fp: &Sexp| -> Vec<Sexp> {
            children(fp, head)
                .into_iter()
                .map(|c| {
                    Sexp::List(
                        c.items()
                            .iter()
                            .filter(|i| i.head() != Some("uuid"))
                            .cloned()
                            .collect(),
                    )
                })
                .collect()
        };
        assert_eq!(strip(&out), strip(&library), "{head}:\n{}", got.text);
    }
    // The pads are where the library has them, at its angles, with nothing of the board's.
    assert_eq!(children(&out, "pad"), children(&library, "pad"), "{}", got.text);

    let field = |name: &str| {
        out.find_all("property")
            .find(|p| p.args().first().and_then(Sexp::text) == Some(name))
            .cloned()
    };
    let reference = field("Reference").expect("the reference stays");
    assert_eq!(reference.args()[1], Sexp::string("REF**"));
    assert_eq!(reference.find("at"), Some(&parse("(at 0 -2)")));
    assert_eq!(reference.find("layer"), Some(&parse("(layer \"F.SilkS\")")));
    assert_eq!(
        reference.find("effects"),
        Some(&parse("(effects (font (size 1 1)))")),
        "the text is not mirrored on the top"
    );
    assert_eq!(field("Value").unwrap().args()[1], Sexp::string("Chip"));
    assert_eq!(field("Datasheet").unwrap().args()[1], Sexp::string(""));
    assert!(field("MPN").is_none(), "{}", got.text);
    assert!(field("Footprint").is_none(), "{}", got.text);

    let mut heads = Vec::new();
    all_heads(&out, &mut heads);
    for gone in [
        "uuid",
        "net",
        "pinfunction",
        "pintype",
        "path",
        "sheetname",
        "sheetfile",
    ] {
        assert!(!heads.iter().any(|h| h == gone), "{gone}:\n{}", got.text);
    }
}

#[test]
fn a_footprint_file_keeps_what_it_draws_under_its_new_name() {
    let got = import_footprint(LIBRARY, FootprintSource::File, Some("Driver")).unwrap();
    assert_eq!(got.source_name, "Chip");
    assert!(!got.from_bottom);
    let out = parse(&got.text);
    let library = parse(LIBRARY);
    assert_eq!(out.args()[0], Sexp::string("Driver"));
    assert_eq!(out.find("version"), library.find("version"));
    for head in ["pad", "fp_rect", "fp_line", "attr", "model"] {
        assert_eq!(children(&out, head), children(&library, head), "{head}");
    }
    let value = out
        .find_all("property")
        .find(|p| p.args()[0] == Sexp::string("Value"))
        .unwrap();
    assert_eq!(value.args()[1], Sexp::string("Driver"));

    // With no name it keeps its own.
    let kept = import_footprint(LIBRARY, FootprintSource::File, None).unwrap();
    assert_eq!(kept.name, "Chip");
}

#[test]
fn a_reference_the_board_lacks_is_refused_with_the_ones_it_has() {
    let err = import_footprint(BOARD, FootprintSource::Board { reference: "U9" }, None)
        .expect_err("no U9");
    assert_eq!(err, "the board has no part `U9`; its parts are R1, U3");

    let twice = BOARD.replace("\"R1\"", "\"U3\"");
    let err = import_footprint(&twice, FootprintSource::Board { reference: "U3" }, None)
        .expect_err("two U3");
    assert_eq!(err, "the board has 2 parts `U3`; a reference must name one");
}

#[test]
fn a_file_of_the_other_kind_is_refused() {
    let err = import_footprint(LIBRARY, FootprintSource::Board { reference: "U3" }, None)
        .expect_err("a footprint is no board");
    assert!(err.contains("not a KiCad board file"), "{err}");
    let err = import_footprint(BOARD, FootprintSource::File, None).expect_err("a board");
    assert!(err.contains("not a KiCad footprint file"), "{err}");
}
