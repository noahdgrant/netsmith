//! The numbers (`netsmith_view::metrics`): counted by hand on a drawing made of wires, and
//! sane on every board the tests draw.

mod common;

use common::corpus::corpus;
use netsmith_view::geom::{Pt, Rect};
use netsmith_view::layout::{Layout, Wire};
use netsmith_view::metrics::measure;

fn wire(net: usize, points: &[(f64, f64)]) -> Wire {
    Wire {
        net,
        points: points.iter().map(|&(x, y)| Pt::new(x, y)).collect(),
    }
}

fn only_wires(wires: Vec<Wire>) -> Layout {
    Layout {
        parts: Vec::new(),
        symbols: Vec::new(),
        ports: Vec::new(),
        marks: Vec::new(),
        sheets: Vec::new(),
        wires,
        junctions: Vec::new(),
        labels: Vec::new(),
        buses: Vec::new(),
        spines: Vec::new(),
        notes: Vec::new(),
        rows: 0,
        bounds: Rect {
            min: Pt::new(0.0, 0.0),
            max: Pt::new(60.0, 30.0),
        },
    }
}

#[test]
fn bends_crossings_overlaps_and_contacts_are_counted_by_hand() {
    let l = only_wires(vec![
        // Net 0: one corner, with a straight-through point on the way, which is no bend and
        // must not hide the crossing at (5, 0).
        wire(0, &[(0.0, 0.0), (5.0, 0.0), (10.0, 0.0), (10.0, 10.0)]),
        // Net 1 crosses net 0 square at (5, 0).
        wire(1, &[(5.0, -5.0), (5.0, 5.0)]),
        // Net 4 lies along net 0 for 2 mm.
        wire(4, &[(2.0, 0.0), (4.0, 0.0)]),
        // Net 2 and net 3: net 3 ends on the middle of net 2 (a contact).
        wire(2, &[(20.0, 0.0), (20.0, 10.0)]),
        wire(3, &[(20.0, 5.0), (30.0, 5.0)]),
        // Net 5 has a corner made of the ends of two wires.
        wire(5, &[(40.0, 0.0), (50.0, 0.0)]),
        wire(5, &[(50.0, 0.0), (50.0, 10.0)]),
    ]);
    let m = measure(&l);
    assert_eq!(m.sheets.len(), 1);
    let s = &m.sheets[0];
    assert_eq!(s.wired_nets, 6);
    assert_eq!(s.wires, 7);
    assert_eq!(s.bends, 2);
    assert_eq!(s.max_bends_in_a_net, 1);
    assert_eq!(s.crossings, 1);
    assert_eq!(s.collinear, 1);
    assert_eq!(s.contacts, 1);
    assert_eq!(s.diagonal_segments, 0);
    assert!((s.bends_per_net() - 2.0 / 6.0).abs() < 1e-9);
    // The longest detour is of net 0 and net 5: 20 mm routed over 20 mm straight... a wire
    // of one bend is as long as the distance across (Manhattan), so no detour at all.
    assert!(s.longest_detour.abs() < 1e-9, "{}", s.longest_detour);
    assert_eq!(s.aspect(), 2.0);
    assert_eq!(s.area(), 1800.0);
    assert_eq!(m.total.bends, 2);
}

#[test]
fn a_wire_that_goes_round_has_a_detour() {
    // From (0, 0) to (10, 0) by way of y = 6: 6 + 10 + 6 = 22 mm for 10 mm across.
    let l = only_wires(vec![wire(
        0,
        &[(0.0, 0.0), (0.0, 6.0), (10.0, 6.0), (10.0, 0.0)],
    )]);
    let s = &measure(&l).sheets[0];
    assert!(
        (s.longest_detour - 12.0).abs() < 1e-9,
        "{}",
        s.longest_detour
    );
    assert_eq!(s.bends, 2);
}

#[test]
fn a_crossing_at_a_corner_is_a_contact_and_not_a_crossing() {
    // Net 1 ends on the end of net 0: touching, not crossing.
    let l = only_wires(vec![
        wire(0, &[(0.0, 0.0), (10.0, 0.0)]),
        wire(1, &[(10.0, 0.0), (10.0, 5.0)]),
    ]);
    let s = &measure(&l).sheets[0];
    assert_eq!((s.crossings, s.contacts, s.collinear), (0, 1, 0));
}

#[test]
fn the_same_drawing_gives_the_same_numbers() {
    for case in corpus() {
        let l = case.drawn();
        let (a, b) = (measure(&l), measure(&l));
        assert_eq!(a, b, "{}", case.name);
        assert_eq!(a.table(), b.table());
        assert_eq!(a.plain(), b.plain());
    }
}

#[test]
fn every_board_is_measured_sheet_by_sheet_and_the_sheets_add_up() {
    for case in corpus() {
        for l in [case.drawn(), case.drawn_plain()] {
            let m = measure(&l);
            let sheets = l.sheets.len().max(1);
            assert_eq!(m.sheets.len(), sheets, "{}", case.name);
            // A part whose symbol has several units is a body for each.
            assert_eq!(m.total.parts, l.parts.len(), "{}", case.name);
            assert_eq!(
                m.total.parts,
                case.design.parts.len() + l.parts.len() - case.design.parts.len(),
                "{}",
                case.name
            );
            assert_eq!(
                m.total.units,
                l.parts.iter().filter(|p| p.glyph.unit.is_some()).count(),
                "{}",
                case.name
            );
            let sum = |f: fn(&netsmith_view::metrics::SheetMetrics) -> usize| {
                m.sheets.iter().map(f).sum::<usize>()
            };
            assert_eq!(sum(|s| s.bends), m.total.bends);
            assert_eq!(sum(|s| s.crossings), m.total.crossings);
            assert_eq!(sum(|s| s.wires), l.wires.len(), "{}", case.name);
            assert_eq!(sum(|s| s.flags), l.ports.len());
            assert_eq!(sum(|s| s.power_symbols), l.symbols.len());
            assert_eq!(sum(|s| s.open_pins), l.marks.len());
            for s in &m.sheets {
                assert!(s.width > 0.0 && s.height > 0.0, "{}: {}", case.name, s.name);
                assert_eq!(s.diagonal_segments, 0, "{}: {}", case.name, s.name);
                assert!((0.0..=1.0).contains(&s.side_consistency()));
            }
            // The table has a header, a line per sheet, and the total.
            assert_eq!(m.table().lines().count(), sheets + 2, "{}", case.name);
            assert!(m.plain().contains("total.bends = "));
        }
    }
}

#[test]
fn a_net_wired_pin_to_pin_is_no_longer_than_it_must_be_on_the_small_boards() {
    // On a board with no flags a net's wire joins all its pins, so it is at least the
    // half-perimeter of them.
    for case in corpus().into_iter().take(3) {
        let s = &measure(&case.drawn_plain()).total;
        assert!(s.half_perimeter > 0.0, "{}", case.name);
        assert!(
            s.length_ratio() >= 1.0 - 1e-9,
            "{}: {}",
            case.name,
            s.length_ratio()
        );
    }
}

#[test]
fn a_capacitor_stands_beside_the_pin_it_decouples() {
    // The regulator board: C1 and C2 are the satellites, the regulator's pins are the
    // anchors. The nearest anchor pin is a dozen grid steps away at most today (a later
    // stage should make that smaller, never larger).
    let case = corpus().remove(2);
    let s = &measure(&case.drawn()).total;
    assert_eq!(s.satellites, 2);
    assert!(
        s.satellite_steps_max <= 12.0 + 1e-6,
        "{}",
        s.satellite_steps_max
    );
    assert_eq!(s.power_symbols, case.drawn().symbols.len());
}

#[test]
fn symbols_overlaps_and_texts_are_zero_on_the_drawings_the_tests_already_check() {
    // The tests of the drawing already hold these at zero for the bench board.
    let case = corpus().remove(5);
    assert_eq!(case.name, "Bench");
    for l in [case.drawn(), case.drawn_plain()] {
        let s = &measure(&l).total;
        assert_eq!(s.symbol_overlaps, 0);
        assert_eq!(s.wire_over_text, 0);
    }
}
