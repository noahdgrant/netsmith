//! How far a sheet moves when the design changes a little: the measurement of
//! `netsmith_view::stability` on the boards of the corpus (run with `-- --nocapture` to see the
//! table; `schematic_metrics --stability` does the same for a project), and what must hold.
//!
//! What must hold is in two parts. A drawing is a function of the *set* of parts and nets, not
//! of the order they are listed in (so a part inserted anywhere reorders nothing), and an edit
//! that changes nothing of a sheet changes nothing of its drawing. And the edits that do need
//! room (a part added or taken out) leave most of a sheet where it was; the numbers asserted
//! below are what the engine reaches today, not the ideal: a part added beside a pin makes
//! room, and what is beside it may shift.

mod common;

use common::corpus::{Case, corpus};
use netsmith_view::stability::{self, Board, Kind, Row};
use netsmith_view::{Group, layout_sheets_marked};

fn board(c: &Case) -> Board {
    Board {
        design: c.design.clone(),
        groups: c.groups.clone(),
        rails: c.rails.clone(),
        marks: Default::default(),
    }
}

/// The boards the measurement runs on in a test: the corpus without its slowest.
const BOARDS: &[&str] = &[
    "LedExample",
    "RegulatorExample",
    "Supplies3",
    "UartSheets",
    "Mcu30",
];

fn measured(names: &[&str]) -> Vec<Row> {
    let mut rows = Vec::new();
    for c in corpus()
        .into_iter()
        .filter(|c| names.contains(&c.name.as_str()))
    {
        rows.extend(stability::measure(
            &c.name,
            &board(&c),
            &c.mapping,
            &c.lib,
            &[],
        ));
    }
    rows
}

#[test]
fn the_edits_are_measured_on_the_corpus() {
    let rows = measured(BOARDS);
    println!("{}", stability::table(&rows, 8));
    assert!(!rows.is_empty());
    for r in &rows {
        for f in [&r.movement.anchor, &r.movement.raw] {
            for v in [
                f.unmoved_fraction(),
                f.segments_fraction(),
                f.tags_fraction(),
            ] {
                assert!((0.0..=1.0).contains(&v), "{}: {:?}", r.label, f);
            }
            assert!(f.unmoved <= f.parts && f.segments_same <= f.segments);
        }
    }
    // Every kind of edit found a place on some board.
    for kind in Kind::ALL {
        assert!(rows.iter().any(|r| r.kind == kind), "no board has {kind:?}");
    }
}

#[test]
fn a_comparison_of_a_drawing_with_itself_finds_nothing_moved() {
    let c = corpus().remove(2);
    let b = board(&c);
    let l = b.draw(&c.mapping, &c.lib);
    let m = stability::compare(
        (&l, &b.design),
        (&l, &b.design),
        "board",
        &[],
        &Default::default(),
    )
    .unwrap();
    assert!(m.anchor.is_identical() && m.raw.is_identical());
    assert_eq!(m.anchor.max_move, 0.0);
    assert!(m.anchor.parts > 0 && m.anchor.segments > 0);
}

#[test]
fn a_value_or_a_net_name_changed_moves_nothing() {
    // (a) rename a signal, (h) rename a rail, (e) change a value for one of the same length.
    let rows = measured(BOARDS);
    let mut checked = 0;
    for r in rows.iter().filter(|r| {
        matches!(
            r.kind,
            Kind::RenameNet | Kind::RenameRail | Kind::ChangeValue
        )
    }) {
        assert!(
            r.movement.anchor.is_identical() && r.movement.raw.is_identical(),
            "{}/{}: {} moved {:?}",
            r.board,
            r.movement.sheet,
            r.label,
            r.movement.anchor
        );
        checked += 1;
    }
    assert!(checked >= 10, "{checked}");
}

#[test]
fn a_part_added_to_another_module_leaves_this_sheet_as_it_was() {
    // (f): in the sheet's own frame, which is what the page shows, nothing differs.
    let rows = measured(BOARDS);
    let mut checked = 0;
    for r in rows.iter().filter(|r| r.kind == Kind::AddToOtherModule) {
        assert!(
            r.movement.anchor.is_identical(),
            "{}/{}: {} moved {:?}",
            r.board,
            r.movement.sheet,
            r.label,
            r.movement.anchor
        );
        assert_eq!(r.movement.rows.0, r.movement.rows.1);
        checked += 1;
    }
    assert!(checked >= 3, "{checked}");
}

/// The sheets of `min_parts` parts or more whose rows did not change, for an edit of `kind`.
fn steady(rows: &[Row], kind: Kind, min_parts: usize) -> Vec<&Row> {
    rows.iter()
        .filter(|r| r.kind == kind && r.movement.size >= min_parts && !r.movement.rows_changed())
        .collect()
}

fn mean(rows: &[&Row], f: impl Fn(&Row) -> f64) -> f64 {
    rows.iter().map(|r| f(r)).sum::<f64>() / rows.len().max(1) as f64
}

#[test]
fn a_part_added_on_no_net_moves_almost_nothing() {
    // (g): the part goes last; the sheet keeps its place unless the new part needs a row.
    let rows = measured(BOARDS);
    let s = steady(&rows, Kind::AddUnrelatedPart, 1);
    assert!(!s.is_empty());
    for r in &s {
        assert!(
            r.movement.anchor.unmoved_fraction() >= 0.9,
            "{}/{}: {:?}",
            r.board,
            r.movement.sheet,
            r.movement.anchor
        );
    }
}

#[test]
fn a_part_added_or_taken_out_moves_a_minority_of_a_sheet() {
    // (b), (c), (d) on the sheets of ten parts: a part beside a pin makes room, so the parts
    // beside it may shift, and the numbers say how many do (means over the corpus sheets; the
    // baseline before the engine was made steady was 0.40, 0.80 and 0.65, and a pull-up moves
    // the hops from ground of the net it is on, which turns the part: floor lowered to match).
    let rows = measured(BOARDS);
    for (kind, floor_mean, floor_segments) in [
        (Kind::AddDecouplingCap, 0.85, 0.85),
        (Kind::AddPullUp, 0.3, 0.3),
        (Kind::DeletePart, 0.55, 0.65),
    ] {
        let s = steady(&rows, kind, 8);
        assert!(!s.is_empty(), "{kind:?}");
        let parts = mean(&s, |r| r.movement.anchor.unmoved_fraction());
        let segs = mean(&s, |r| r.movement.anchor.segments_fraction());
        assert!(
            parts >= floor_mean && segs >= floor_segments,
            "{kind:?}: parts unmoved {parts:.2}, segments kept {segs:.2}"
        );
    }
}

/// The design with its parts and nets listed the other way round (the parts and the pins of
/// each net reversed, the groups' parts reindexed and reversed), which is the same board.
fn reversed(c: &Case) -> Board {
    let n = c.design.parts.len();
    let mut design = c.design.clone();
    design.parts.reverse();
    design.nets.reverse();
    for net in &mut design.nets {
        net.pins.reverse();
    }
    let groups: Vec<Group> = c
        .groups
        .iter()
        .map(|g| Group {
            parts: g.parts.iter().rev().map(|&p| n - 1 - p).collect(),
            ..g.clone()
        })
        .collect();
    Board {
        design,
        groups,
        rails: c.rails.clone(),
        marks: Default::default(),
    }
}

#[test]
fn a_drawing_does_not_depend_on_the_order_of_parts_and_nets() {
    for c in corpus() {
        // The biggest boards take long for what this checks.
        if matches!(c.name.as_str(), "Bench" | "Supplies7") {
            continue;
        }
        let a = board(&c);
        let b = reversed(&c);
        let (la, lb) = (a.draw(&c.mapping, &c.lib), b.draw(&c.mapping, &c.lib));
        assert_eq!(la.sheets.len(), lb.sheets.len(), "{}", c.name);
        for sheet in stability::sheet_names(&la) {
            let m = stability::compare(
                (&la, &a.design),
                (&lb, &b.design),
                &sheet,
                &[],
                &Default::default(),
            )
            .unwrap_or_else(|| panic!("{}: no sheet {sheet}", c.name));
            assert!(
                m.anchor.is_identical() && m.raw.is_identical(),
                "{}/{sheet}: drawn differently when listed the other way round: {:?} {:?}",
                c.name,
                m.raw,
                m.moves
            );
            assert_eq!(m.frames[0], m.frames[1], "{}/{sheet}", c.name);
            assert_eq!(m.rows.0, m.rows.1, "{}/{sheet}", c.name);
        }
        assert_eq!(la.wires.len(), lb.wires.len(), "{}", c.name);
        assert_eq!(la.bounds, lb.bounds, "{}", c.name);
        assert_eq!(la.notes.len(), lb.notes.len(), "{}", c.name);
    }
}

#[test]
fn drawing_twice_gives_the_same_bytes() {
    for c in corpus().into_iter().take(6) {
        let a = layout_sheets_marked(
            &c.design,
            &c.groups,
            &c.rails,
            &Default::default(),
            &c.mapping,
            &c.lib,
        )
        .unwrap();
        let b = layout_sheets_marked(
            &c.design,
            &c.groups,
            &c.rails,
            &Default::default(),
            &c.mapping,
            &c.lib,
        )
        .unwrap();
        assert_eq!(a, b, "{}", c.name);
    }
}

#[test]
fn a_drawing_has_its_largest_part_at_the_origin() {
    // The sheet's coordinates start at its reference part, whatever grows round it.
    for name in ["RegulatorExample", "Mcu30", "LedExample"] {
        let c = corpus().into_iter().find(|c| c.name == name).unwrap();
        let l = board(&c).draw(&c.mapping, &c.lib);
        let most = l.parts.iter().map(|p| p.pins.len()).max().unwrap();
        assert!(
            l.parts
                .iter()
                .any(|p| p.pins.len() == most && p.at.x.abs() < 1e-6 && p.at.y.abs() < 1e-6),
            "{name}: no largest part at the origin"
        );
    }
}
