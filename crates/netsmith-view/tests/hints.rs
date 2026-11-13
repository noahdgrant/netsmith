//! The schematic layout file (ADR 0061): reading, writing, and matching it with a design.

use std::path::Path;

use netsmith_core::{Design, Note, NoteKind, Part};
use netsmith_view::hints::parse;
use netsmith_view::{
    FindingKind, Group, HintError, LayoutHints, Located, NoteHint, PartHint, Place, SheetHints,
    Steps, hints, schematic_hints_path,
};

fn part(refdes: &str, path: &str) -> Part {
    Part {
        provides: Vec::new(),
        declared_kind: netsmith_testkit::std_kind("Resistor").and_then(netsmith_core::PartKind::from_word),
        refdes: refdes.into(),
        kind: "Resistor".into(),
        value: "1k".into(),
        package: None,
        path: path.into(),
        pins: Vec::new(),
    }
}

fn note(path: &str, kind: NoteKind, anchors: &[&str]) -> Note {
    Note {
        path: path.into(),
        kind,
        text: "text".into(),
        at: None,
        anchors: anchors.iter().map(|s| s.to_string()).collect(),
        result: None,
        derived: false,
    }
}

/// A board `Trellis` with two parts of its own and two modules: `stage` (with a module
/// `filter` inside it) and `radio`.
fn trellis() -> (Design, Vec<Group>) {
    let design = Design {
        modules: Vec::new(),
        name: "Trellis".into(),
        parts: vec![
            part("J1", "j1"),
            part("R1", "r1"),
            part("R2", "stage.output_cap"),
            part("L1", "stage.filter.l1"),
            part("C1", "stage.filter.c1"),
            part("U1", "radio.u1"),
            part("U2", "radio.Pé"),
        ],
        nets: Vec::new(),
        process: None,
        features: Default::default(),
        interfaces: Vec::new(),
        notes: vec![
            note("stage.output_cap", NoteKind::Doc, &["stage.output_cap"]),
            note("stage", NoteKind::Relation, &["stage.filter.l1"]),
            note("stage", NoteKind::Relation, &["stage.filter.c1"]),
            note("", NoteKind::Doc, &[]),
        ],
    };
    let groups = vec![
        Group {
            name: "stage".into(),
            detail: "Stage".into(),
            parts: vec![2, 3, 4],
        },
        Group {
            name: "radio".into(),
            detail: "Radio".into(),
            parts: vec![5, 6],
        },
    ];
    (design, groups)
}

const EXAMPLE: &str = r#"version = 1

# the board's own parts
[Trellis.parts.j1]
dx = 2.54
dy = -1.27

[Trellis.sheets.stage.parts."output_cap"]
rotate = 90
mirror = true

[Trellis.sheets.stage.parts."filter.l1"]
pin = [12.7, 0]

[Trellis.sheets.stage.notes."output_cap#doc"]
dx = 5.08
dy = 0

[Trellis.sheets.radio.parts.gone]
dx = 1.27
dy = 0

[Trellis.sheets.old.parts.c1]
dx = 1.27
dy = 0
"#;

fn errors(text: &str) -> Vec<HintError> {
    parse(text).unwrap_err()
}

fn messages(text: &str) -> Vec<String> {
    errors(text).iter().map(|e| e.to_string()).collect()
}

#[test]
fn the_file_is_beside_the_source_and_named_by_its_stem() {
    assert_eq!(
        schematic_hints_path(Path::new("src/main.nsm")),
        Path::new("src/main.sch.toml")
    );
    assert_eq!(
        schematic_hints_path(Path::new("led.nsm")),
        Path::new("led.sch.toml")
    );
    assert_eq!(
        schematic_hints_path(Path::new("a.b.nsm")),
        Path::new("a.b.sch.toml")
    );
}

#[test]
fn the_example_is_read() {
    let h = parse(EXAMPLE).unwrap();
    let board = &h.boards["Trellis"];
    assert_eq!(
        board.board.parts["j1"].value,
        PartHint {
            place: Some(Place::Offset(Steps::new(2, -1))),
            ..PartHint::default()
        }
    );
    let stage = &board.sheets["stage"];
    assert_eq!(
        stage.parts["output_cap"].value,
        PartHint {
            place: None,
            rotate: Some(90),
            mirror: Some(true),
        }
    );
    assert_eq!(
        stage.parts["filter.l1"].value.place,
        Some(Place::Pinned(Steps::new(10, 0)))
    );
    assert_eq!(
        stage.notes["output_cap#doc"].value,
        NoteHint {
            by: Steps::new(4, 0)
        }
    );
    // The lines are where the entries' tables start.
    assert_eq!(board.board.parts["j1"].line, 4);
    assert_eq!(stage.parts["output_cap"].line, 8);
    assert_eq!(stage.notes["output_cap#doc"].line, 15);
    assert_eq!(board.sheets["old"].line, 23);
}

#[test]
fn what_is_written_is_read_back_to_the_same_hints() {
    let h = parse(EXAMPLE).unwrap();
    let text = hints::to_toml(&h);
    assert_eq!(parse(&text).unwrap(), h, "{text}");
    // Writing is a fixed point: the text of the text is the text.
    assert_eq!(hints::to_toml(&parse(&text).unwrap()), text);
}

#[test]
fn the_text_is_in_the_order_of_board_sheet_and_path_however_the_hints_were_made() {
    let off = |x, y| PartHint {
        place: Some(Place::Offset(Steps::new(x, y))),
        ..PartHint::default()
    };
    let sheet = |parts: &[(&str, PartHint)]| {
        Located::new(SheetHints {
            parts: parts
                .iter()
                .map(|(k, v)| (k.to_string(), Located::new(*v)))
                .collect(),
            notes: Default::default(),
        })
    };
    let mut h = LayoutHints::default();
    // Inserted in the reverse of the order they are written in.
    let mut zed = netsmith_view::BoardHints::default();
    zed.sheets.insert(
        "radio".into(),
        sheet(&[("u1", off(1, 0)), ("c2", off(2, 0))]),
    );
    zed.sheets.insert(
        "analog".into(),
        sheet(&[("b", off(3, 0)), ("a", off(4, 0))]),
    );
    zed.board = sheet(&[("j1", off(0, 1))]).value;
    h.boards.insert("Zed".into(), Located::new(zed));
    let alpha = netsmith_view::BoardHints {
        board: sheet(&[("r1", off(0, -2))]).value,
        ..Default::default()
    };
    h.boards.insert("Alpha".into(), Located::new(alpha));

    assert_eq!(
        hints::to_toml(&h),
        "\
version = 1

[Alpha.parts.r1]
dx = 0
dy = -2.54

[Zed.parts.j1]
dx = 0
dy = 1.27

[Zed.sheets.analog.parts.a]
dx = 5.08
dy = 0

[Zed.sheets.analog.parts.b]
dx = 3.81
dy = 0

[Zed.sheets.radio.parts.c2]
dx = 2.54
dy = 0

[Zed.sheets.radio.parts.u1]
dx = 1.27
dy = 0
"
    );
}

#[test]
fn empty_hints_are_a_version_line_and_a_file_with_no_hints_is_empty_hints() {
    let text = hints::to_toml(&LayoutHints::default());
    assert_eq!(text, "version = 1\n");
    assert_eq!(parse(&text).unwrap(), LayoutHints::default());
    for blank in ["", "\n", "   \n\n", "# nothing moved yet\n"] {
        let h = parse(blank).unwrap_or_else(|e| panic!("{blank:?}: {e:?}"));
        assert!(h.is_empty() && h == LayoutHints::default(), "{blank:?}");
    }
    // A board table with nothing in it is no hints either, and is not written back.
    let h = parse("version = 1\n[Trellis]\n[Trellis.sheets.stage]\n").unwrap();
    assert!(h.is_empty());
    assert_eq!(hints::to_toml(&h), "version = 1\n");
}

#[test]
fn comments_are_allowed_and_not_kept() {
    let text = "\
# moved by hand
version = 1 # the format

[Trellis.parts.j1] # the connector
dx = 1.27   # right one step
dy = 0
";
    let h = parse(text).unwrap();
    assert_eq!(h.boards["Trellis"].board.parts["j1"].line, 4);
    assert!(!hints::to_toml(&h).contains('#'));
}

#[test]
fn every_grid_step_is_written_and_read_back_exactly() {
    for steps in -4000..=4000 {
        let mut h = LayoutHints::default();
        let mut b = netsmith_view::BoardHints::default();
        b.board.parts.insert(
            "p".into(),
            Located::new(PartHint {
                place: Some(Place::Pinned(Steps::new(steps, -steps))),
                ..PartHint::default()
            }),
        );
        h.boards.insert("B".into(), Located::new(b));
        let text = hints::to_toml(&h);
        assert_eq!(parse(&text).unwrap(), h, "{steps}: {text}");
    }
}

#[test]
fn a_number_is_read_as_millimetres_whatever_it_looks_like() {
    let read = |value: &str| -> Result<i32, String> {
        let text = format!("version = 1\n[B.parts.p]\ndx = {value}\n");
        parse(&text)
            .map(|h| match h.boards["B"].board.parts["p"].place {
                Some(Place::Offset(s)) => s.x,
                other => panic!("{other:?}"),
            })
            .map_err(|e| e[0].message.clone())
    };
    assert_eq!(read("2.54"), Ok(2));
    assert_eq!(read("-3.81"), Ok(-3));
    assert_eq!(read("0"), Ok(0));
    assert_eq!(read("0.0"), Ok(0));
    assert_eq!(read("1.27"), Ok(1));
    assert_eq!(read("1.2700"), Ok(1));
    assert_eq!(read("1.270000001"), Ok(1));
    assert_eq!(read("2.54e0"), Ok(2));
}

#[test]
fn bad_toml_is_one_error_with_its_line() {
    let e = errors("version = 1\n\n[Trellis.parts.j1\ndx = 1\n");
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].line, 3, "{e:?}");
    let e = errors("version = = 1\n");
    assert_eq!(e[0].line, 1);
    assert!(e[0].to_string().starts_with("line 1: "), "{}", e[0]);
}

#[test]
fn the_version_is_required_and_must_be_one() {
    assert_eq!(
        messages("version = 2\n"),
        ["line 1: `version` is 2, and this reads only version 1"]
    );
    assert_eq!(
        messages("version = \"one\"\n"),
        ["line 1: `version` is \"one\", and this reads only version 1"]
    );
    assert_eq!(
        messages("\n[Trellis.parts.j1]\ndx = 1.27\n"),
        ["line 2: it has no `version`; write `version = 1`"]
    );
}

#[test]
fn an_unknown_key_is_an_error_that_names_it_and_the_line() {
    let text = "\
version = 1
[Trellis.parts.j1]
dx = 1.27
spin = 3
[Trellis.sheets.stage.parts.c1]
colour = \"red\"
[Trellis.sheets.stage.notes.\"c1#doc\"]
rotate = 90
[Trellis.sheets.stage]
extra = 1
[Trellis]
frobnicate = 1
";
    assert_eq!(
        messages(text),
        [
            "line 4: unknown key `spin` in [Trellis.parts.j1] (a part has: dx, dy, pin, rotate, mirror)",
            "line 6: unknown key `colour` in [Trellis.sheets.stage.parts.c1] (a part has: dx, dy, pin, rotate, mirror)",
            "line 8: unknown key `rotate` in [Trellis.sheets.stage.notes.\"c1#doc\"] (a note has: dx, dy)",
            "line 10: unknown key `extra` in [Trellis.sheets.stage] (it has: parts, notes)",
            "line 12: unknown key `frobnicate` in [Trellis] (it has: parts, notes, sheets)",
        ]
    );
    // A value that is not a table where a table belongs.
    assert_eq!(
        messages("version = 1\nTrellis = 3\n"),
        ["line 2: [Trellis] must be a table, not 3"]
    );
}

#[test]
fn a_value_off_the_grid_is_refused_with_the_nearest_places() {
    let text = "version = 1\n[Trellis.parts.j1]\ndx = 1.0\ndy = 1.27\n";
    assert_eq!(
        messages(text),
        [
            "line 3: [Trellis.parts.j1]: `dx` = 1.0 is not on the 1.27 mm grid (the nearest places are 0 and 1.27)"
        ]
    );
    assert_eq!(
        messages("version = 1\n[B.parts.p]\ndx = 1.28\n"),
        [
            "line 3: [B.parts.p]: `dx` = 1.28 is not on the 1.27 mm grid (the nearest places are 1.27 and 2.54)"
        ]
    );
    assert_eq!(
        messages("version = 1\n[B.parts.p]\npin = [2.54, -1.0]\n"),
        [
            "line 3: [B.parts.p]: `pin` = -1.0 is not on the 1.27 mm grid (the nearest places are -1.27 and 0)"
        ]
    );
    assert_eq!(
        messages("version = 1\n[B.notes.\"p#doc\"]\ndx = 0.5\n"),
        [
            "line 3: [B.notes.\"p#doc\"]: `dx` = 0.5 is not on the 1.27 mm grid (the nearest places are 0 and 1.27)"
        ]
    );
    // Too far, a number of the wrong kind, a pin that is not a pair.
    let far = messages("version = 1\n[B.parts.p]\ndx = 5081.27\n");
    assert!(far[0].contains("further than 5080 mm"), "{far:?}");
    assert_eq!(
        messages("version = 1\n[B.parts.p]\ndx = \"left\"\n"),
        ["line 3: [B.parts.p]: `dx` is \"left\", and must be a number of millimetres"]
    );
    assert_eq!(
        messages("version = 1\n[B.parts.p]\npin = [1.27]\n"),
        [
            "line 3: [B.parts.p]: `pin` is an array, and must be two numbers of millimetres, `[x, y]`"
        ]
    );
    assert!(parse("version = 1\n[B.parts.p]\ndx = nan\n").is_err());
    assert!(parse("version = 1\n[B.parts.p]\ndx = inf\n").is_err());
}

#[test]
fn a_turn_must_be_a_multiple_of_ninety_degrees() {
    let read = |v: &str| {
        parse(&format!("version = 1\n[B.parts.p]\nrotate = {v}\n"))
            .map(|h| h.boards["B"].board.parts["p"].rotate)
            .map_err(|e| e[0].message.clone())
    };
    assert_eq!(read("0"), Ok(Some(0)));
    assert_eq!(read("90"), Ok(Some(90)));
    assert_eq!(read("270"), Ok(Some(270)));
    assert_eq!(read("-90"), Ok(Some(270)));
    assert_eq!(read("360"), Ok(Some(0)));
    assert_eq!(read("180.0"), Ok(Some(180)));
    assert_eq!(
        read("45"),
        Err(
            "[B.parts.p]: `rotate` is 45, and must be a multiple of 90 degrees (0, 90, 180 or 270)"
                .into()
        )
    );
    assert!(read("91.5").is_err());
    assert!(read("\"left\"").is_err());
    assert_eq!(
        messages("version = 1\n[B.parts.p]\nmirror = \"yes\"\n"),
        ["line 3: [B.parts.p]: `mirror` is \"yes\", and must be true or false"]
    );
}

#[test]
fn a_pin_and_a_move_cannot_be_given_together() {
    assert_eq!(
        messages("version = 1\n[B.parts.p]\npin = [0, 0]\ndx = 1.27\n"),
        [
            "line 3: [B.parts.p]: `pin` (a place from the sheet's reference part) and `dx`, `dy` (a move from the automatic place) cannot be given together"
        ]
    );
}

#[test]
fn a_key_that_names_nothing_is_an_error() {
    assert!(
        messages("version = 1\n[B.parts.\"a..b\"]\ndx = 0\n")[0]
            .contains("names joined by single dots")
    );
    assert!(
        messages("version = 1\n[B.parts.\"\"]\ndx = 0\n")[0]
            .contains("names joined by single dots")
    );
    assert!(
        messages("version = 1\n[B.sheets.\"a.b\".parts.c]\ndx = 0\n")[0]
            .contains("a name with no dot")
    );
    for bad in [
        "x",
        "x#",
        "x#note",
        "x#relation1",
        "x#relation0",
        "x#relation02",
        "#",
        "a..b#doc",
    ] {
        let text = format!("version = 1\n[B.notes.\"{bad}\"]\ndx = 0\n");
        let e = messages(&text);
        // `#` alone is a sheet's own note with a missing kind: also refused.
        assert!(
            e.len() == 1 && e[0].contains("a note is keyed `<path>#doc` or `<path>#relation`"),
            "{bad}: {e:?}"
        );
    }
    for good in [
        "x#doc",
        "#doc",
        "a.b#relation",
        "a.b#relation2",
        "a#relation10",
    ] {
        let text = format!("version = 1\n[B.notes.\"{good}\"]\ndx = 0\n");
        assert!(parse(&text).is_ok(), "{good}");
    }
}

#[test]
fn every_fault_is_returned_in_the_order_of_the_file() {
    let text = "\
version = 1
[B.parts.a]
dx = 1.0
[B.parts.b]
rotate = 7
[B.parts.c]
dx = 1.27
[B.parts.d]
mirror = 3
";
    let e = errors(text);
    assert_eq!(e.iter().map(|e| e.line).collect::<Vec<_>>(), [3, 5, 9]);
}

#[test]
fn a_part_with_a_non_ascii_name_is_written_quoted_and_read_back() {
    let text = "version = 1\n[Trellis.sheets.radio.parts.\"Pé\"]\ndx = 1.27\ndy = 0\n";
    let h = parse(text).unwrap();
    assert!(h.boards["Trellis"].sheets["radio"].parts.contains_key("Pé"));
    let out = hints::to_toml(&h);
    assert!(out.contains("parts.\"Pé\"]"), "{out}");
    assert_eq!(parse(&out).unwrap(), h);
    // Quotes, backslashes and control characters in a key survive a round trip.
    let mut h = LayoutHints::default();
    let mut b = netsmith_view::BoardHints::default();
    for key in [
        "a\"b",
        "a\\b",
        "tab\there",
        "line\nbreak",
        "ünï.cödé",
        "日本",
        "x y",
    ] {
        b.board
            .parts
            .insert(key.into(), Located::new(PartHint::default()));
    }
    h.boards.insert("Bé".into(), Located::new(b));
    let out = hints::to_toml(&h);
    assert_eq!(parse(&out).unwrap_or_else(|e| panic!("{e:?}\n{out}")), h);
}

#[test]
fn a_part_with_a_non_ascii_name_is_matched_and_a_stale_one_is_found_without_a_panic() {
    let (design, groups) = trellis();
    let text = "\
version = 1
[Trellis.sheets.radio.parts.\"Pé\"]
dx = 1.27
dy = 0
[Trellis.sheets.radio.parts.\"Pè\"]
dx = 1.27
dy = 0
[Trellis.sheets.\"módulo\".parts.x]
dx = 1.27
dy = 0
";
    let (resolved, findings) = parse(text).unwrap().resolve(&design, &groups);
    assert_eq!(resolved.parts.keys().copied().collect::<Vec<_>>(), [6]);
    assert_eq!(resolved.parts[&6].path, "radio.Pé");
    assert_eq!(
        findings
            .iter()
            .map(|f| f.message.as_str())
            .collect::<Vec<_>>(),
        [
            "[Trellis.sheets.radio.parts.\"Pè\"]: the design has no part `Pè` on this sheet; the entry is ignored",
            "[Trellis.sheets.\"módulo\"]: the board has no sheet `módulo` (its sheets: `radio`, `stage`); its 1 entry is ignored",
        ]
    );
}

#[test]
fn the_entries_of_a_design_are_found_by_path_from_their_sheet() {
    let (design, groups) = trellis();
    let h = parse(EXAMPLE).unwrap();
    let (resolved, findings) = h.resolve(&design, &groups);

    // The board's own part, a module's part, a part of a module inside a module.
    let at = |i: usize| &resolved.parts[&i];
    assert_eq!(at(0).path, "j1");
    assert_eq!(at(0).sheet, "Trellis");
    assert_eq!(at(0).hint.place, Some(Place::Offset(Steps::new(2, -1))));
    assert_eq!(at(0).line, 4);
    assert_eq!(
        (at(2).sheet.as_str(), at(2).path.as_str()),
        ("stage", "stage.output_cap")
    );
    assert_eq!(at(2).hint.rotate, Some(90));
    assert_eq!(at(2).hint.mirror, Some(true));
    assert_eq!(at(3).path, "stage.filter.l1");
    assert_eq!(at(3).hint.place, Some(Place::Pinned(Steps::new(10, 0))));
    assert_eq!(resolved.parts.len(), 3, "{:?}", resolved.parts.keys());

    // The note on the part is the design's first.
    assert_eq!(resolved.notes.len(), 1);
    assert_eq!(resolved.notes[&0].key, "output_cap#doc");
    assert_eq!(resolved.notes[&0].sheet, "stage");
    assert_eq!(resolved.notes[&0].hint.by, Steps::new(4, 0));

    // And two entries are found stale, each at its line, in the order of the file.
    assert_eq!(
        findings
            .iter()
            .map(|f| (f.kind, f.line, f.message.as_str()))
            .collect::<Vec<_>>(),
        [
            (
                FindingKind::StaleEntry,
                19,
                "[Trellis.sheets.radio.parts.gone]: the design has no part `gone` on this sheet; the entry is ignored"
            ),
            (
                FindingKind::StaleSheet,
                23,
                "[Trellis.sheets.old]: the board has no sheet `old` (its sheets: `radio`, `stage`); its 1 entry is ignored"
            ),
        ]
    );
    assert_eq!(
        findings[0].render("src/main.sch.toml"),
        "src/main.sch.toml:19: [Trellis.sheets.radio.parts.gone]: the design has no part `gone` on this sheet; the entry is ignored"
    );
}

#[test]
fn a_module_renamed_keeps_its_entries_when_the_table_is_renamed_and_nothing_else() {
    // The same entries under the sheet's new name: the keys inside are the same.
    let (mut design, mut groups) = trellis();
    for p in &mut design.parts {
        if let Some(rest) = p.path.strip_prefix("stage.") {
            p.path = format!("power.{rest}");
        }
    }
    for n in &mut design.notes {
        if n.path == "stage" || n.path.starts_with("stage.") {
            n.path = n.path.replacen("stage", "power", 1);
        }
        for a in &mut n.anchors {
            *a = a.replacen("stage", "power", 1);
        }
    }
    groups[0].name = "power".into();
    let before = parse(EXAMPLE).unwrap();
    let (kept, _) = before.resolve(&design, &groups);
    // The stage entries are all stale: the file says `stage`.
    assert_eq!(kept.parts.keys().copied().collect::<Vec<_>>(), [0]);
    let renamed = EXAMPLE.replace("sheets.stage", "sheets.power");
    let (kept, findings) = parse(&renamed).unwrap().resolve(&design, &groups);
    assert_eq!(kept.parts.keys().copied().collect::<Vec<_>>(), [0, 2, 3]);
    assert_eq!(kept.notes.keys().copied().collect::<Vec<_>>(), [0]);
    assert_eq!(findings.len(), 2, "{findings:?}");
}

#[test]
fn a_part_under_the_wrong_table_is_named_with_the_sheet_it_is_drawn_on() {
    let (design, groups) = trellis();
    // A module's part under the board's own table, and a board's part under a module's.
    let text = "\
version = 1
[Trellis.parts.\"stage.output_cap\"]
dx = 0
dy = 0
[Trellis.sheets.stage.parts.j1]
dx = 0
dy = 0
[Trellis.sheets.radio.parts.\"filter.l1\"]
dx = 0
dy = 0
";
    let (resolved, findings) = parse(text).unwrap().resolve(&design, &groups);
    assert!(resolved.is_empty());
    let kinds: Vec<_> = findings.iter().map(|f| f.kind).collect();
    assert_eq!(
        kinds,
        [
            FindingKind::WrongSheet,
            FindingKind::StaleEntry,
            FindingKind::StaleEntry
        ]
    );
    assert_eq!(
        findings[0].message,
        "[Trellis.parts.\"stage.output_cap\"]: the part `stage.output_cap` is drawn on the sheet `stage`, not on this one; its entry belongs in [Trellis.sheets.stage.parts.output_cap], and is ignored here"
    );
    // `j1` under `stage` is `stage.j1`, which the design does not have: stale, not misplaced.
    assert!(
        findings[1].message.contains("no part `j1` on this sheet"),
        "{}",
        findings[1].message
    );
}

#[test]
fn a_pinned_part_is_resolved_as_pinned_with_its_turn() {
    let (design, groups) = trellis();
    let text =
        "version = 1\n[Trellis.parts.r1]\npin = [-12.7, 6.35]\nrotate = 180\nmirror = false\n";
    let (resolved, findings) = parse(text).unwrap().resolve(&design, &groups);
    assert!(findings.is_empty());
    assert_eq!(
        resolved.parts[&1].hint,
        PartHint {
            place: Some(Place::Pinned(Steps::new(-10, 5))),
            rotate: Some(180),
            mirror: Some(false),
        }
    );
    assert_eq!(
        resolved.parts[&1].hint.place.map(|p| match p {
            Place::Pinned(s) => s.mm(),
            Place::Offset(s) => s.mm(),
        }),
        Some((-12.7, 6.35))
    );
}

#[test]
fn notes_are_found_by_path_kind_and_order() {
    let (design, groups) = trellis();
    let text = "\
version = 1
[Trellis.sheets.stage.notes.\"#relation\"]
dx = 1.27
dy = 0
[Trellis.sheets.stage.notes.\"#relation2\"]
dx = 2.54
dy = 0
[Trellis.sheets.stage.notes.\"#relation3\"]
dx = 3.81
dy = 0
[Trellis.sheets.stage.notes.\"#doc\"]
dx = 0
dy = 1.27
[Trellis.notes.\"#doc\"]
dx = 0
dy = -1.27
";
    let (resolved, findings) = parse(text).unwrap().resolve(&design, &groups);
    // The two relations of the module `stage` are the design's second and third notes; its
    // board's own writing is the fourth. A third relation and a doc comment of the module do
    // not exist.
    assert_eq!(resolved.notes[&1].hint.by, Steps::new(1, 0));
    assert_eq!(resolved.notes[&2].hint.by, Steps::new(2, 0));
    assert_eq!(resolved.notes[&3].hint.by, Steps::new(0, -1));
    assert_eq!(resolved.notes.len(), 3);
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(
        findings[0]
            .message
            .contains("no note `#relation3` on this sheet")
    );
    assert!(findings[1].message.contains("no note `#doc` on this sheet"));
}

#[test]
fn a_board_with_no_table_and_empty_hints_resolve_to_nothing() {
    let (design, groups) = trellis();
    let (resolved, findings) = LayoutHints::default().resolve(&design, &groups);
    assert!(resolved.is_empty() && findings.is_empty());
    assert_eq!(resolved, Default::default());
    let other = parse("version = 1\n[Other.parts.r1]\ndx = 1.27\ndy = 0\n").unwrap();
    let (resolved, findings) = other.resolve(&design, &groups);
    assert!(resolved.is_empty() && findings.is_empty());
    // The table of a board the source does not have is found by the caller, with the boards.
    let found = other.unknown_boards(&["Trellis"]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, FindingKind::UnknownBoard);
    assert_eq!(found[0].line, 2);
    assert_eq!(
        found[0].message,
        "[Other]: the source has no board `Other` (its boards: `Trellis`); the table is ignored"
    );
    assert!(other.unknown_boards(&["Other", "Trellis"]).is_empty());
}

#[test]
fn a_design_without_paths_or_groups_has_nothing_to_match() {
    let mut design = Design {
        modules: Vec::new(),
        name: "Bare".into(),
        parts: vec![part("R1", ""), part("R2", "")],
        nets: Vec::new(),
        process: None,
        features: Default::default(),
        interfaces: Vec::new(),
        notes: Vec::new(),
    };
    let h = parse("version = 1\n[Bare.parts.r1]\ndx = 1.27\ndy = 0\n").unwrap();
    let (resolved, findings) = h.resolve(&design, &[]);
    assert!(resolved.is_empty());
    assert_eq!(findings.len(), 1);
    design.parts.clear();
    let (resolved, findings) = h.resolve(&design, &[]);
    assert!(resolved.is_empty());
    assert_eq!(findings.len(), 1);
}

#[test]
fn a_group_named_like_the_board_is_still_a_sheet_of_its_own() {
    let (mut design, mut groups) = trellis();
    design.name = "stage".into();
    groups[0].name = "stage".into();
    let text = "version = 1\n[stage.parts.j1]\ndx = 1.27\ndy = 0\n[stage.sheets.stage.parts.output_cap]\ndx = 0\ndy = 0\n";
    let (resolved, findings) = parse(text).unwrap().resolve(&design, &groups);
    assert!(findings.is_empty(), "{findings:?}");
    assert_eq!(resolved.parts.keys().copied().collect::<Vec<_>>(), [0, 2]);
}

#[test]
fn a_finding_that_the_drawing_makes_reads_like_the_others() {
    let f = netsmith_view::HintFinding::unusable(
        "[Trellis.sheets.stage.parts.output_cap]".into(),
        8,
        "its block would lie on the body of `stage.filter.l1`",
    );
    assert_eq!(f.kind, FindingKind::Unusable);
    assert_eq!(
        f.render("main.sch.toml"),
        "main.sch.toml:8: [Trellis.sheets.stage.parts.output_cap]: its block would lie on the body of `stage.filter.l1`; it is ignored"
    );
}
