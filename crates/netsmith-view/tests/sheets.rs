//! A board drawn by module: sheets that do not overlap, hold their parts and wires, and tie
//! to the rest of the board by named stubs.

use crate::common;

use netsmith_core::Design;
use netsmith_kicad::{Mapping, SymbolLib, testlib};
use netsmith_view::geom::Rect;
use netsmith_view::layout::{Layout, layout};
use netsmith_view::render_svg_by_module;
use netsmith_view::sheets::{Group, MAX_SHEET_PARTS, layout_sheets};

fn lib() -> SymbolLib {
    SymbolLib::at(testlib::sample_symbols())
}

fn lay(d: &Design, groups: &[Group]) -> Layout {
    layout_sheets(d, groups, &Mapping::standard(), &lib()).unwrap()
}

fn overlap(a: &Rect, b: &Rect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

fn contains(outer: &Rect, inner: &Rect) -> bool {
    let eps = 1e-6;
    outer.min.x - eps <= inner.min.x
        && outer.min.y - eps <= inner.min.y
        && inner.max.x <= outer.max.x + eps
        && inner.max.y <= outer.max.y + eps
}

#[test]
fn seventy_parts_make_seven_sheets_that_do_not_overlap() {
    let (d, groups) = common::supplies(7);
    assert_eq!(d.parts.len(), 70);
    let l = lay(&d, &groups);
    assert_eq!(l.sheets.len(), 7);
    for (i, a) in l.sheets.iter().enumerate() {
        for b in &l.sheets[i + 1..] {
            assert!(!overlap(&a.frame, &b.frame), "{} and {}", a.name, b.name);
        }
    }
    for sheet in &l.sheets {
        assert!(contains(&l.bounds, &sheet.frame), "{}", sheet.name);
        for &p in &sheet.parts {
            // A box after the parts is a further unit of one of them.
            assert!(l.parts[p].part == p || p >= d.parts.len());
            assert!(
                contains(&sheet.frame, &l.parts[p].extent),
                "{} is outside {}",
                d.parts[l.parts[p].part].refdes,
                sheet.name
            );
        }
    }
}

#[test]
fn the_whole_drawing_is_not_a_strip() {
    for modules in [7, 12] {
        let (d, groups) = common::supplies(modules);
        let l = lay(&d, &groups);
        let aspect = l.bounds.width() / l.bounds.height();
        assert!(
            (0.4..=3.0).contains(&aspect),
            "{modules} modules: {:.1} by {:.1} mm",
            l.bounds.width(),
            l.bounds.height()
        );
    }
}

#[test]
fn the_same_board_gives_the_same_drawing() {
    let (d, groups) = common::supplies(7);
    assert_eq!(lay(&d, &groups), lay(&d, &groups));
}

#[test]
fn every_wire_stays_on_the_sheet_it_starts_on() {
    let (d, groups) = common::supplies(7);
    let l = lay(&d, &groups);
    for w in &l.wires {
        let sheet = l
            .sheets
            .iter()
            .find(|s| {
                w.points
                    .iter()
                    .all(|p| contains(&s.frame, &Rect { min: *p, max: *p }))
            })
            .unwrap_or_else(|| panic!("a wire of {} crosses sheets", d.nets[w.net].name));
        assert!(sheet.frame.width() > 0.0);
    }
}

#[test]
fn a_net_between_sheets_is_wired_on_each_and_leaves_through_a_flag() {
    let (d, groups) = common::supplies(7);
    let l = lay(&d, &groups);
    let vin = d.nets.iter().position(|n| n.name == "VIN").unwrap();
    // One flag on each sheet, with the net's name in it, and no label for it.
    assert!(l.labels.iter().all(|t| t.net != vin));
    let flags: Vec<_> = l.ports.iter().filter(|p| p.net == vin).collect();
    assert_eq!(flags.len(), 7, "one flag on each sheet");
    for flag in &flags {
        assert_eq!(flag.label.as_ref().unwrap().text, "VIN");
        assert!(
            l.sheets.iter().any(|s| contains(&s.frame, &flag.extent())),
            "the flag is inside a sheet"
        );
    }
    // The two pins of VIN on a sheet are joined by wires, and the flag is on them.
    for sheet in &l.sheets {
        let at: Vec<_> = d.nets[vin]
            .pins
            .iter()
            .filter(|p| sheet.parts.iter().any(|&i| d.parts[i].refdes == p.refdes))
            .map(|p| pin_at(&l, &d, &p.refdes, &p.pin))
            .collect();
        assert!(common::wired_together(&l, vin, &at), "{}", sheet.name);
        let flag = flags
            .iter()
            .find(|f| contains(&sheet.frame, &f.extent()))
            .unwrap();
        assert!(common::wired_together(&l, vin, &[at[0], flag.at]));
    }
}

#[test]
fn pins_keep_their_nets_in_the_whole_board_numbering() {
    let (d, groups) = common::supplies(3);
    let l = lay(&d, &groups);
    for placed in &l.parts {
        let part = &d.parts[placed.part];
        for pin in &placed.pins {
            let expected = d.nets.iter().position(|n| {
                n.pins
                    .iter()
                    .any(|p| p.refdes == part.refdes && p.pin == pin.id)
            });
            assert_eq!(pin.net, expected, "{}.{}", part.refdes, pin.id);
        }
    }
}

#[test]
fn a_small_board_is_one_drawing_as_before() {
    let d = common::regulator();
    let groups = vec![Group {
        name: "all".into(),
        detail: String::new(),
        parts: (0..d.parts.len()).collect(),
    }];
    let l = lay(&d, &groups);
    assert!(l.sheets.is_empty());
    assert_eq!(
        l,
        layout(&d, &Mapping::standard(), &lib()).unwrap(),
        "same drawing"
    );
}

#[test]
fn parts_in_no_group_are_a_sheet_of_their_own() {
    let (d, mut groups) = common::supplies(4);
    groups.truncate(3);
    let l = lay(&d, &groups);
    assert_eq!(l.sheets.len(), 4);
    assert_eq!(l.sheets[3].name, "Big");
    assert_eq!(l.sheets[3].parts, (30..40).collect::<Vec<_>>());
}

#[test]
fn a_group_that_is_too_big_is_cut_into_numbered_sheets() {
    let (d, _) = common::supplies(7);
    let l = lay(&d, &[]);
    let pieces = 70usize.div_ceil(MAX_SHEET_PARTS);
    assert_eq!(
        l.sheets.len(),
        pieces,
        "70 parts in sheets of at most {MAX_SHEET_PARTS}"
    );
    for (i, s) in l.sheets.iter().enumerate() {
        assert_eq!(s.name, format!("Big ({}/{pieces})", i + 1));
        assert!(s.parts.len() <= MAX_SHEET_PARTS);
    }
    let mut all: Vec<usize> = l.sheets.iter().flat_map(|s| s.parts.clone()).collect();
    all.sort_unstable();
    assert_eq!(all, (0..70).collect::<Vec<_>>());
}

#[test]
fn a_module_that_fits_one_page_is_not_split() {
    // The old drawing put every column of a module in one row, so a module of more than 20
    // parts was a long thin strip and was cut in two; the new one arranges parts in two
    // dimensions, and a module of up to MAX_SHEET_PARTS parts stays whole.
    let (d, groups) = common::supplies(1);
    let mut groups = groups;
    groups[0].parts = (0..10).collect();
    let l = lay(&d, &groups);
    assert!(l.sheets.is_empty() || l.sheets.iter().all(|s| !s.name.contains('/')));
    let (d, _) = common::supplies(4);
    let all = Group {
        name: "supplies".into(),
        detail: "Three".into(),
        parts: (0..30).collect(),
    };
    let l = lay(&d, &[all]);
    assert_eq!(l.sheets.len(), 2, "the module and the rest");
    assert_eq!(
        l.sheets[0].name, "supplies",
        "30 parts of one module are one sheet"
    );
    assert_eq!(l.sheets[0].parts.len(), 30);
    let f = l.sheets[0].frame;
    let aspect = f.width() / f.height();
    assert!((0.3..=4.0).contains(&aspect), "{aspect}");
}

#[test]
fn the_drawing_names_each_module_and_stays_well_formed() {
    let (d, groups) = common::supplies(7);
    let svg = render_svg_by_module(&d, &groups, &Mapping::standard(), &lib()).unwrap();
    for m in 1..=7 {
        assert!(
            svg.contains(&format!("data-module=\"supply{m}\"")),
            "supply{m}"
        );
    }
    assert!(svg.contains("supply1: Supply"));
    common::balanced(&svg).unwrap();
}

fn pin_at(l: &Layout, d: &Design, refdes: &str, pad: &str) -> netsmith_view::geom::Pt {
    let i = d.parts.iter().position(|p| p.refdes == refdes).unwrap();
    l.parts[i].pins.iter().find(|p| p.id == pad).unwrap().at
}
