//! Notes: the doc comments and `==` relations of the source, drawn as framed text boxes on the
//! sheet (ADR 0060, `layout/notes.rs`). A note joins nothing, lies on nothing, wraps its text
//! inside its frame, stands near the part it is about, and a sheet without notes is drawn as it
//! was.

use crate::common;

use netsmith_core::{Design, Note, NoteAt, NoteKind};
use netsmith_kicad::{Mapping, SymbolLib, testlib};
use netsmith_view::check::check;
use netsmith_view::geom::{Pt, Rect};
use netsmith_view::layout::{Layout, layout_rails};
use netsmith_view::metrics::measure;
use netsmith_view::{Group, Rails, layout_sheets_rails, render_svg_with};

fn lib() -> (Mapping, SymbolLib) {
    (
        Mapping::standard(),
        SymbolLib::at(testlib::sample_symbols()),
    )
}

fn rails() -> Rails {
    let mut r = Rails::default();
    r.ground.insert("GND".into());
    r.positive.insert("VCC".into());
    r
}

fn doc(path: &str, anchors: &[&str], text: &str, line: usize) -> Note {
    Note {
        path: path.into(),
        kind: NoteKind::Doc,
        text: text.into(),
        at: Some(NoteAt {
            file: "led.nsm".into(),
            line,
        }),
        anchors: anchors.iter().map(|s| s.to_string()).collect(),
        result: None,
    }
}

fn relation(anchors: &[&str], text: &str, result: Option<&str>) -> Note {
    Note {
        kind: NoteKind::Relation,
        result: result.map(str::to_string),
        ..doc("", anchors, text, 30)
    }
}

/// The LED board with paths on its parts and three notes: the board's doc comment, a doc
/// comment on the resistor and a relation across the LED and the resistor.
fn board() -> Design {
    let mut d = common::led();
    for p in &mut d.parts {
        p.path = p.refdes.to_lowercase();
    }
    d.notes = vec![
        doc(
            "",
            &[],
            "A small board that lights an LED from a 5 V supply through a resistor.",
            1,
        ),
        doc(
            "r1",
            &["r1"],
            "Sets the LED current: the 300 ohm resistor is the value the solver picked from the catalog, \
             the nearest one that keeps the LED inside its rated current over the whole range of the supply.",
            12,
        ),
        relation(
            &["d1", "r1"],
            ".led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance",
            Some("led.current = 8.7mA"),
        ),
    ];
    d
}

fn lay(d: &Design) -> Layout {
    let (m, l) = lib();
    layout_rails(d, &rails(), &m, &l).unwrap()
}

fn svg(d: &Design) -> String {
    let (m, l) = lib();
    render_svg_with(d, &rails(), &m, &l).unwrap()
}

fn part_index(d: &Design, refdes: &str) -> usize {
    d.parts.iter().position(|p| p.refdes == refdes).unwrap()
}

/// The distance from `a` to `b`, 0 when they touch or overlap.
fn gap(a: Rect, b: Rect) -> f64 {
    let dx = (a.min.x - b.max.x).max(b.min.x - a.max.x).max(0.0);
    let dy = (a.min.y - b.max.y).max(b.min.y - a.max.y).max(0.0);
    dx.hypot(dy)
}

fn contains(outer: Rect, inner: Rect) -> bool {
    inner.min.x >= outer.min.x - 1e-9
        && inner.min.y >= outer.min.y - 1e-9
        && inner.max.x <= outer.max.x + 1e-9
        && inner.max.y <= outer.max.y + 1e-9
}

#[test]
fn the_notes_of_a_design_are_drawn_and_break_no_rule() {
    let d = board();
    let l = lay(&d);
    assert_eq!(
        l.notes.len(),
        3,
        "a doc on the board, one on a part, a relation"
    );
    let found = check(&d, &l);
    assert!(found.is_empty(), "{found:?}");
    for n in &l.notes {
        assert!(
            contains(l.bounds, n.frame),
            "note {} is outside the sheet: {:?} in {:?}",
            n.note,
            n.frame,
            l.bounds
        );
    }
    let m = measure(&l);
    assert_eq!(m.total.notes, 3);
    assert!(m.total.note_area > 0.0);
    assert_eq!(m.total.note_overlaps, 0);
}

#[test]
fn the_text_of_a_note_is_wrapped_inside_its_frame() {
    let d = board();
    let l = lay(&d);
    for n in &l.notes {
        assert!(!n.lines.is_empty());
        for line in &n.lines {
            let w = netsmith_view::layout::line_width(&line.text, n.size, n.mono);
            assert!(
                line.at.x + w <= n.frame.max.x - 0.3,
                "`{}` is {w:.1} mm wide at x {:.1} and the frame ends at {:.1}",
                line.text,
                line.at.x,
                n.frame.max.x
            );
            assert!(line.at.y < n.frame.max.y && line.at.y > n.frame.min.y);
        }
        assert!(
            n.frame.width() <= 90.0,
            "a note is about 60 to 90 mm wide at most"
        );
    }
    // The doc comment of the resistor is long enough to need two lines or more.
    let long = l.notes.iter().find(|n| n.path == "r1").unwrap();
    assert!(long.lines.len() >= 2, "{:?}", long.lines);
    // Words are not cut: every line breaks at a space.
    let joined = long
        .lines
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(joined, d.notes[1].text);
}

#[test]
fn a_relation_is_monospace_with_a_heading_and_keeps_its_dots_and_operators() {
    let d = board();
    let l = lay(&d);
    let r = l
        .notes
        .iter()
        .find(|n| n.kind == NoteKind::Relation)
        .unwrap();
    assert!(r.mono);
    assert_eq!(r.heading.as_ref().unwrap().text, "Relation");
    let text = r
        .lines
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        text.contains(".led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance"),
        "{text}"
    );
    assert!(text.contains("→ led.current = 8.7mA"), "{text}");
    let doc = l.notes.iter().find(|n| n.path == "r1").unwrap();
    assert!(!doc.mono && doc.heading.is_none(), "a doc has no heading");
}

#[test]
fn a_note_of_a_part_stands_near_that_part_and_the_sheets_note_is_at_the_top_left() {
    let d = board();
    let l = lay(&d);
    let r1 = &l.parts[part_index(&d, "R1")];
    let note = l.notes.iter().find(|n| n.path == "r1").unwrap();
    assert_eq!(note.anchor, Some(part_index(&d, "R1")));
    assert!(
        gap(note.frame, r1.extent) < 60.0,
        "the note of R1 is {:.1} mm from it",
        gap(note.frame, r1.extent)
    );
    let header = l
        .notes
        .iter()
        .find(|n| n.path.is_empty() && n.kind == NoteKind::Doc)
        .unwrap();
    assert_eq!(header.anchor, None);
    let top = l
        .parts
        .iter()
        .map(|p| p.extent.min.y)
        .fold(f64::MAX, f64::min);
    let left = l
        .parts
        .iter()
        .map(|p| p.extent.min.x)
        .fold(f64::MAX, f64::min);
    assert!(
        header.frame.max.y <= top,
        "the sheet's note is above the drawing"
    );
    assert!(
        (header.frame.min.x - left).abs() < 12.0,
        "and at its left: {:.1} against {:.1}",
        header.frame.min.x,
        left
    );
}

#[test]
fn a_note_never_moves_a_wire_or_a_part() {
    let with = board();
    let mut without = with.clone();
    without.notes.clear();
    let (a, b) = (lay(&with), lay(&without));
    assert_eq!(a.parts, b.parts);
    assert_eq!(a.wires, b.wires);
    assert_eq!(a.junctions, b.junctions);
    assert_eq!(a.labels, b.labels);
    assert_eq!(a.symbols, b.symbols);
    assert_eq!(a.ports, b.ports);
    assert_eq!(a.buses, b.buses);
}

#[test]
fn a_design_without_notes_is_drawn_with_no_note_at_all() {
    for d in common::all() {
        let l = lay(&d);
        assert!(l.notes.is_empty());
        let doc = svg(&d);
        assert!(!doc.contains("class=\"notes\""), "{}", d.name);
        assert!(!doc.contains("data-note"), "{}", d.name);
    }
}

#[test]
fn the_drawing_of_a_design_with_notes_is_deterministic_and_in_source_order() {
    let d = board();
    assert_eq!(lay(&d), lay(&d));
    assert_eq!(svg(&d), svg(&d));
    let l = lay(&d);
    let order: Vec<usize> = l.notes.iter().map(|n| n.note).collect();
    assert_eq!(order, [0, 1, 2]);
}

#[test]
fn a_long_note_is_cut_with_an_ellipsis_and_the_tooltip_has_all_of_it() {
    let mut d = board();
    let long: String = (0..140).map(|i| format!("word{i} ")).collect::<String>();
    assert!(long.chars().count() > 600);
    d.notes[1].text = long.trim_end().to_string();
    let l = lay(&d);
    let n = l.notes.iter().find(|n| n.path == "r1").unwrap();
    let shown: String = n
        .lines
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(shown.ends_with('…'), "{shown}");
    assert!(shown.chars().count() <= 600, "{}", shown.chars().count());
    assert_eq!(n.full.as_deref(), Some(d.notes[1].text.as_str()));
    let doc = svg(&d);
    assert!(
        doc.contains(&format!("<title>{}</title>", d.notes[1].text)),
        "the whole text is in the tooltip"
    );
    assert!(check(&d, &l).is_empty());
}

#[test]
fn the_svg_of_a_note_has_its_classes_and_references() {
    let d = board();
    let doc = svg(&d);
    common::balanced(&doc).unwrap();
    assert_eq!(doc.matches("class=\"note note-doc\"").count(), 2);
    assert_eq!(doc.matches("class=\"note note-relation\"").count(), 1);
    assert!(doc.contains("data-note=\"r1\""));
    assert!(doc.contains("data-ref=\"R1\""));
    assert!(doc.contains("data-at=\"led.nsm:12\""));
    assert!(doc.contains("class=\"note-frame\""));
    assert!(doc.contains("class=\"note-heading\""));
    // Selectable text: one <text> with a <tspan> for each line.
    assert!(doc.contains("<tspan "));
    assert!(doc.contains("(.supply.voltage - .led.forward_voltage)"));
    // The sheet is as large as its notes.
    let bounds = lay(&d).bounds;
    assert!(
        doc.contains(&format!("data-bounds=\"{}", bounds.min.x)),
        "{doc:.300}"
    );
}

/// The attribute `data-anchors` lists the designators of every part a note is about, in the
/// order the design's note names them, so the page need not read the words of a relation.
fn anchors_of(doc: &str, n: usize) -> String {
    let at = doc
        .find(&format!(" data-n=\"{n}\""))
        .unwrap_or_else(|| panic!("no note {n} in {doc:.200}"));
    let tag = &doc[doc[..at].rfind("<g ").unwrap()..];
    let tag = &tag[..tag.find('>').unwrap()];
    let key = " data-anchors=\"";
    let from = tag
        .find(key)
        .unwrap_or_else(|| panic!("note {n} has no data-anchors: {tag}"))
        + key.len();
    tag[from..from + tag[from..].find('"').unwrap()].to_string()
}

#[test]
fn a_doc_note_of_a_part_names_that_part() {
    let doc = svg(&board());
    assert_eq!(anchors_of(&doc, 1), "R1");
}

#[test]
fn a_relation_names_every_part_it_is_about_in_the_order_of_the_note() {
    let doc = svg(&board());
    assert_eq!(anchors_of(&doc, 2), "D1 R1");
    // The first of them is the one `data-ref` names.
    assert!(doc.contains("data-n=\"2\" data-ref=\"D1\""), "{doc:.200}");
}

#[test]
fn a_relation_of_three_parts_names_all_three_not_only_the_first() {
    let mut d = board();
    d.notes.push(relation(
        &["r1", "d1", "d1"],
        ".r1.resistance == .led.current * 3",
        None,
    ));
    // Every part once, in the order the note has them.
    assert_eq!(anchors_of(&svg(&d), 3), "R1 D1");
}

#[test]
fn the_note_of_a_sheet_names_no_part_but_says_so() {
    let doc = svg(&board());
    assert_eq!(anchors_of(&doc, 0), "");
    assert!(!doc.contains("data-ref=\"\""));
}

#[test]
fn a_relation_across_two_sheets_names_the_parts_of_both() {
    let (mut d, groups): (Design, Vec<Group>) = common::supplies(3);
    for g in &groups {
        for &p in &g.parts {
            d.parts[p].path = format!("{}.{}", g.name, d.parts[p].refdes.to_lowercase());
        }
    }
    let (a, b) = (
        d.parts[groups[0].parts[0]].clone(),
        d.parts[groups[1].parts[0]].clone(),
    );
    d.notes = vec![relation(
        &[a.path.as_str(), b.path.as_str()],
        ".a.resistance == .b.resistance",
        None,
    )];
    let (m, lib) = lib();
    let l = layout_sheets_rails(&d, &groups, &Rails::default(), &m, &lib).unwrap();
    let doc = netsmith_view::render_layout(&d, &l);
    // The note is on the sheet of its first part, and names the part of the other sheet too.
    assert_eq!(
        anchors_of(&doc, 0),
        format!("{} {}", a.refdes, b.refdes),
        "{doc:.300}"
    );
}

#[test]
fn a_design_with_no_note_has_no_data_anchors() {
    let mut d = board();
    d.notes.clear();
    assert!(!svg(&d).contains("data-anchors"));
}

#[test]
fn more_than_eight_notes_go_in_one_column_at_the_right_margin() {
    let mut d = board();
    d.notes = (0..10)
        .map(|k| {
            doc(
                "r1",
                &["r1"],
                &format!("Note number {k} about the resistor."),
                k + 1,
            )
        })
        .collect();
    let l = lay(&d);
    assert_eq!(l.notes.len(), 10);
    let right = l
        .parts
        .iter()
        .map(|p| p.extent.max.x)
        .fold(f64::MIN, f64::max);
    let x = l.notes[0].frame.min.x;
    assert!(x > right, "the column is right of the drawing");
    for n in &l.notes {
        assert_eq!(n.frame.min.x, x, "one column");
    }
    for pair in l.notes.windows(2) {
        assert!(
            pair[0].frame.max.y < pair[1].frame.min.y,
            "stacked, in order"
        );
    }
    assert!(check(&d, &l).is_empty());
}

#[test]
fn a_note_that_finds_no_room_near_its_part_goes_to_the_margin() {
    // Seven notes about one connector: the room beside it within reach runs out, and the
    // rest go to a margin, outside the drawing, in a column or a strip; none lies on another.
    let mut d = board();
    d.notes = (0..7)
        .map(|k| {
            doc(
                "j1",
                &["j1"],
                &format!("Note {k}: the connector takes the supply, polarity as marked."),
                k + 1,
            )
        })
        .collect();
    let l = lay(&d);
    assert_eq!(l.notes.len(), 7);
    let found = check(&d, &l);
    assert!(found.is_empty(), "{found:?}");
    // On a sheet this small there is no room inside the drawing: the notes are in a margin, level
    // with the connector as far as they can be, one under the other and none on another.
    let j1 = l.parts[part_index(&d, "J1")].extent;
    let first = &l.notes[0];
    assert!(
        gap(first.frame, j1) < 40.0,
        "the first note is {:.1} mm from the connector",
        gap(first.frame, j1)
    );
    for pair in l.notes.windows(2) {
        assert!(
            pair[0].frame.max.y < pair[1].frame.min.y,
            "stacked in order"
        );
    }
    for n in &l.notes {
        assert!(contains(l.bounds, n.frame));
    }
}

#[test]
fn a_note_on_a_part_is_a_violation_and_so_is_text_out_of_its_frame() {
    let d = board();
    let l = lay(&d);
    // Move a note onto a part.
    let mut on_part = l.clone();
    let part = on_part.parts[part_index(&d, "R1")].extent;
    let n = &mut on_part.notes[1];
    let (w, h) = (n.frame.width(), n.frame.height());
    let by = Pt::new(part.min.x - n.frame.min.x, part.min.y - n.frame.min.y);
    n.frame = Rect {
        min: Pt::new(part.min.x, part.min.y),
        max: Pt::new(part.min.x + w, part.min.y + h),
    };
    for t in n.lines.iter_mut().chain(n.heading.iter_mut()) {
        t.at = Pt::new(t.at.x + by.x, t.at.y + by.y);
    }
    let found = check(&d, &on_part);
    assert!(
        found.iter().any(|v| v.kind() == "note-overlap"),
        "{found:?}"
    );
    // Two notes on each other.
    let mut two = l.clone();
    two.notes[1].frame = two.notes[0].frame;
    let found = check(&d, &two);
    assert!(
        found.iter().any(|v| v.kind() == "note-overlap"),
        "{found:?}"
    );
    // A note on a wire.
    let mut wired = l.clone();
    let wire = wired.wires[0].points[0];
    let n = &mut wired.notes[1];
    let (w, h) = (n.frame.width(), n.frame.height());
    n.frame = Rect {
        min: Pt::new(wire.x - w / 2.0, wire.y - h / 2.0),
        max: Pt::new(wire.x + w / 2.0, wire.y + h / 2.0),
    };
    assert!(
        check(&d, &wired).iter().any(|v| v.kind() == "note-overlap"),
        "a note over a wire is found"
    );
    // The text escapes its frame: the frame is cut short.
    let mut narrow = l.clone();
    narrow.notes[1].frame.max.x = narrow.notes[1].frame.min.x + 10.0;
    let found = check(&d, &narrow);
    let overflow: Vec<_> = found
        .iter()
        .filter(|v| v.kind() == "note-text-overflow")
        .collect();
    assert!(!overflow.is_empty(), "{found:?}");
    assert!(
        overflow[0].to_string().contains("mm wide"),
        "the message names the numbers: {}",
        overflow[0]
    );
}

#[test]
fn a_note_is_no_conductor() {
    // The netlist the check rebuilds does not read notes: dropping every note leaves the
    // same violations (none) and the same nets.
    let d = board();
    let l = lay(&d);
    assert!(check(&d, &l).is_empty());
    let mut bare = l.clone();
    bare.notes.clear();
    assert!(check(&d, &bare).is_empty());
}

/// Three supplies, each a sheet of its own, with a doc comment on each module (the sheet's
/// note), one on a part and a relation across two parts of one module.
#[test]
fn each_note_is_on_the_sheet_of_its_module_and_the_boxes_are_inside_the_sheets() {
    let (mut d, groups): (Design, Vec<Group>) = common::supplies(3);
    for g in &groups {
        for &p in &g.parts {
            d.parts[p].path = format!("{}.{}", g.name, d.parts[p].refdes.to_lowercase());
        }
    }
    d.notes = vec![
        doc("", &[], "The board: three supplies from one input.", 1),
        doc("supply1", &[], "The first supply.", 3),
        doc("supply2", &[], "The second supply, from the first.", 20),
        doc("supply2.u2", &["supply2.u2"], "Its regulator.", 22),
        relation(
            &["supply3.r3", "supply3.ra3"],
            ".r3.resistance == .ra3.resistance / 10",
            None,
        ),
    ];
    let (m, lib) = lib();
    let l = layout_sheets_rails(&d, &groups, &Rails::default(), &m, &lib).unwrap();
    assert_eq!(l.notes.len(), 5);
    let found = check(&d, &l);
    assert!(found.is_empty(), "{found:?}");
    let sheet_of = |note: usize| {
        let n = l.notes.iter().find(|n| n.note == note).unwrap();
        l.sheets
            .iter()
            .position(|s| contains(s.frame, n.frame))
            .unwrap_or_else(|| panic!("note {note} is in no sheet's frame"))
    };
    let names: Vec<&str> = l.sheets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names[sheet_of(1)], "supply1");
    assert_eq!(names[sheet_of(2)], "supply2");
    assert_eq!(names[sheet_of(3)], "supply2");
    assert_eq!(names[sheet_of(4)], "supply3");
    // The board's own note is on the sheet that holds the parts of no module, else the first.
    assert!(sheet_of(0) < l.sheets.len());
    // Anchors point at design parts.
    let n3 = l.notes.iter().find(|n| n.note == 3).unwrap();
    assert_eq!(d.parts[n3.anchor.unwrap()].path, "supply2.u2");
    // Bytes are the same for the same design.
    assert_eq!(
        layout_sheets_rails(&d, &groups, &Rails::default(), &m, &lib).unwrap(),
        l
    );
}

#[test]
fn a_board_of_sheets_without_notes_has_no_note_in_it() {
    let (d, groups) = common::supplies(3);
    let (m, lib) = lib();
    let l = layout_sheets_rails(&d, &groups, &Rails::default(), &m, &lib).unwrap();
    assert!(l.notes.is_empty());
    assert_eq!(measure(&l).total.notes, 0);
    assert!(!measure(&l).table().contains("noteArea"));
}
