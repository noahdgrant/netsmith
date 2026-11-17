//! A sheet a schematic layout file places (`at`, ADR 0139): its box is where the file says, the
//! sheets never moved are set out by their connections round it, and a place the drawing
//! cannot honour is a finding.

use crate::common;

use netsmith_core::Design;
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_view::geom::Rect;
use netsmith_view::hints::parse;
use netsmith_view::layout::Layout;
use netsmith_view::sheets::{Group, layout_sheets};
use netsmith_view::{
    FindingKind, HintFinding, Marks, Rails, ResolvedHints, layout_sheets_hinted_findings,
};

fn lib() -> SymbolLib {
    SymbolLib::at(netsmith_testkit::sample_symbols())
}

/// The drawing with the file `text` (a table for the board `Big`), and what it says about it.
fn draw(d: &Design, groups: &[Group], text: &str) -> (Layout, Vec<HintFinding>) {
    let (resolved, mut found) = parse(text).unwrap().resolve(d, groups);
    let (layout, drawn) = layout_sheets_hinted_findings(
        d,
        groups,
        &Rails::default(),
        &Marks::default(),
        &resolved,
        &Mapping::standard(),
        &lib(),
    )
    .unwrap();
    found.extend(drawn);
    (layout, found)
}

fn frame<'l>(l: &'l Layout, name: &str) -> &'l Rect {
    &l.sheets.iter().find(|s| s.name == name).unwrap().frame
}

fn apart(a: &Rect, b: &Rect, gap: f64) -> bool {
    a.max.x + gap <= b.min.x + 1e-6
        || b.max.x + gap <= a.min.x + 1e-6
        || a.max.y + gap <= b.min.y + 1e-6
        || b.max.y + gap <= a.min.y + 1e-6
}

/// The sheets' names row by row, each row from the left (as the editor's cards are).
fn set_out(l: &Layout) -> Vec<String> {
    let mut sheets: Vec<_> = l.sheets.iter().collect();
    sheets.sort_by(|a, b| {
        let (p, q) = (a.frame.min, b.frame.min);
        if (p.y - q.y).abs() < 5.0 {
            p.x.total_cmp(&q.x)
        } else {
            p.y.total_cmp(&q.y)
        }
    });
    sheets.iter().map(|s| s.name.clone()).collect()
}

#[test]
fn a_placed_sheet_is_where_the_file_says_and_the_others_are_set_out_round_it() {
    let (d, groups) = common::supplies(7);
    let before = layout_sheets(&d, &groups, &Mapping::standard(), &lib()).unwrap();
    // supply4 goes where supply1 is: the top-left corner of the page.
    let (l, found) = draw(&d, &groups, "version = 1\n[Big.sheets.supply4]\nat = [0, 0]\n");
    assert!(found.is_empty(), "{found:?}");
    let placed = frame(&l, "supply4");
    assert!(
        placed.min.x.abs() < 0.64 && placed.min.y.abs() < 0.64,
        "supply4 at {:?}",
        placed.min
    );
    // No two boxes overlap, and none comes nearer the placed one than the gap between sheets.
    for (i, a) in l.sheets.iter().enumerate() {
        for b in &l.sheets[i + 1..] {
            assert!(apart(&a.frame, &b.frame, 0.0), "{} and {}", a.name, b.name);
        }
        if a.name != "supply4" {
            assert!(apart(&a.frame, placed, 12.7 - 1e-6), "{} is too near", a.name);
        }
    }
    // The card of supply4 comes first; the others keep the order of their connections.
    let order = |l: &Layout| -> Vec<String> {
        set_out(l)
            .into_iter()
            .filter(|n| n != "supply4")
            .collect()
    };
    assert_eq!(set_out(&l)[0], "supply4");
    assert_eq!(order(&l), order(&before));
    // Every part is still in its sheet's box.
    for sheet in &l.sheets {
        for &p in &sheet.parts {
            let e = l.parts[p].extent;
            assert!(
                sheet.frame.min.x <= e.min.x + 1e-6 && e.max.x <= sheet.frame.max.x + 1e-6,
                "{} leaves {}",
                d.parts[l.parts[p].part].refdes,
                sheet.name
            );
        }
    }
}

#[test]
fn a_placed_sheet_moves_its_drawing_as_a_whole() {
    // The place moves the box and everything in it by one translation, on the grid: the parts
    // of the sheet keep their places from each other.
    let (d, groups) = common::supplies(7);
    let (plain, _) = draw(&d, &groups, "version = 1\n");
    let (l, found) = draw(&d, &groups, "version = 1\n[Big.sheets.supply2]\nat = [508, 254]\n");
    assert!(found.is_empty(), "{found:?}");
    let sheet = l.sheets.iter().find(|s| s.name == "supply2").unwrap();
    let by_x = l.parts[sheet.parts[0]].at.x - plain.parts[sheet.parts[0]].at.x;
    let by_y = l.parts[sheet.parts[0]].at.y - plain.parts[sheet.parts[0]].at.y;
    for k in [by_x, by_y] {
        assert!(((k / 1.27).round() * 1.27 - k).abs() < 1e-6, "{k} is off the grid");
    }
    for &p in &sheet.parts {
        assert!((l.parts[p].at.x - plain.parts[p].at.x - by_x).abs() < 1e-6);
        assert!((l.parts[p].at.y - plain.parts[p].at.y - by_y).abs() < 1e-6);
    }
    let f = frame(&l, "supply2");
    assert!((f.min.x - 508.0).abs() < 0.64 && (f.min.y - 254.0).abs() < 0.64);
}

#[test]
fn no_place_is_the_drawing_there_is_without_a_file() {
    let (d, groups) = common::supplies(7);
    let plain = layout_sheets(&d, &groups, &Mapping::standard(), &lib()).unwrap();
    let (l, _) = draw(&d, &groups, "version = 1\n[Big.sheets.supply2.parts.x]\nrotate = 90\n");
    assert_eq!(l, plain);
    let (l, found) = layout_sheets_hinted_findings(
        &d,
        &groups,
        &Rails::default(),
        &Marks::default(),
        &ResolvedHints::default(),
        &Mapping::standard(),
        &lib(),
    )
    .unwrap();
    assert_eq!(l, plain);
    assert!(found.is_empty());
}

#[test]
fn a_sheet_placed_on_another_placed_sheet_is_a_finding_and_is_set_out_with_the_rest() {
    let (d, groups) = common::supplies(7);
    let text = "version = 1\n[Big.sheets.supply3]\nat = [0, 0]\n\n[Big.sheets.supply5]\nat = [12.7, 12.7]\n";
    let (l, found) = draw(&d, &groups, text);
    assert_eq!(
        found
            .iter()
            .map(|f| (f.kind, f.line, f.message.as_str()))
            .collect::<Vec<_>>(),
        [(
            FindingKind::Unusable,
            6,
            "[Big.sheets.supply5]: its box would lie on the box of the sheet `supply3`; it is ignored"
        )]
    );
    for (i, a) in l.sheets.iter().enumerate() {
        for b in &l.sheets[i + 1..] {
            assert!(apart(&a.frame, &b.frame, 0.0), "{} and {}", a.name, b.name);
        }
    }
}

#[test]
fn a_sheet_drawn_as_several_boxes_has_no_one_place() {
    // With no groups the 70 parts are the board's own sheet, cut in two.
    let (d, _) = common::supplies(7);
    let (l, found) = draw(&d, &[], "version = 1\n[Big]\nat = [254, 0]\n");
    assert_eq!(l.sheets.len(), 2);
    assert_eq!(
        found.iter().map(|f| f.message.as_str()).collect::<Vec<_>>(),
        ["[Big]: the sheet `Big` is drawn as 2 boxes, so it has no one place on the page; it is ignored"]
    );
    assert_eq!(l, layout_sheets(&d, &[], &Mapping::standard(), &lib()).unwrap());
}

#[test]
fn a_board_drawn_as_one_sheet_places_no_sheet() {
    let (d, groups) = common::supplies(1);
    let (l, found) = draw(&d, &groups, "version = 1\n[Big.sheets.supply1]\nat = [254, 0]\n");
    assert!(l.sheets.is_empty());
    assert_eq!(
        found.iter().map(|f| f.message.as_str()).collect::<Vec<_>>(),
        ["[Big.sheets.supply1]: the board is drawn as one sheet, so no sheet has a place of its own on the page; it is ignored"]
    );
}
