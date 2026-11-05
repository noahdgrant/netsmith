//! How far a drawing moves when the design changes a little. This reports and does not judge:
//! the engine is known to move a lot when a part is added, and the numbers printed here
//! (run with `-- --nocapture`) are the baseline later stages are compared with. What is
//! asserted is that the harness runs and that its numbers are fractions.

mod common;

use common::corpus::{Case, corpus};
use common::stability::{Edit, Stability, apply, compare};
use netsmith_view::sheets::layout_sheets_rails;

fn case(name: &str) -> Case {
    corpus().into_iter().find(|c| c.name == name).unwrap()
}

fn edit(case: &Case, edit: &Edit) -> Stability {
    let before = case.drawn();
    let edited = apply(&case.design, &case.groups, &case.rails, edit);
    let after = layout_sheets_rails(
        &edited.design,
        &edited.groups,
        &edited.rails,
        &case.mapping,
        &case.lib,
    )
    .unwrap();
    let s = compare(
        (&before, &case.design),
        (&after, &edited.design),
        &edited.touched,
        &edited.renames,
    );
    println!("{} / {}", case.name, s.line(&edit.label()));
    s
}

fn plausible(s: &Stability) {
    for f in [
        s.segments_fraction(),
        s.length_fraction(),
        s.length_fraction_aligned(),
    ] {
        assert!((0.0..=1.0).contains(&f), "{s:?}");
    }
    assert!(s.parts_moved <= s.parts_compared);
}

#[test]
fn nothing_changes_when_nothing_is_edited() {
    // A control for the harness itself: a drawing compared with itself stays whole.
    let c = case("Bench");
    let l = c.drawn();
    let s = compare((&l, &c.design), (&l, &c.design), &[], &Default::default());
    assert_eq!(s.segments_kept, s.segments_before);
    assert_eq!(s.length_kept, s.length_before);
    assert_eq!(s.parts_moved, 0);
    assert_eq!(s.max_displacement, 0.0);
}

#[test]
fn the_edits_are_reported_on_the_bench_board() {
    let c = case("Bench");
    let edits = [
        Edit::RenameNet {
            from: "+3V3".into(),
            to: "VDD33".into(),
        },
        Edit::AddDecouplingCap {
            refdes: "C99".into(),
            beside: "U5".into(),
            supply: "+3V3".into(),
            ground: "GND".into(),
        },
        Edit::DeletePart("R9".into()),
    ];
    for e in &edits {
        plausible(&edit(&c, e));
    }
}

#[test]
fn the_edits_are_reported_on_a_board_of_three_supplies() {
    let c = case("Supplies3");
    let edits = [
        Edit::RenameNet {
            from: "out2".into(),
            to: "vout2".into(),
        },
        Edit::AddDecouplingCap {
            refdes: "C99".into(),
            beside: "U2".into(),
            supply: "out2".into(),
            ground: "GND".into(),
        },
        Edit::DeletePart("RC2".into()),
    ];
    for e in &edits {
        plausible(&edit(&c, e));
    }
}

#[test]
fn an_edit_changes_the_design() {
    let c = case("Supplies3");
    let added = apply(
        &c.design,
        &c.groups,
        &c.rails,
        &Edit::AddDecouplingCap {
            refdes: "C99".into(),
            beside: "U2".into(),
            supply: "out2".into(),
            ground: "GND".into(),
        },
    );
    assert_eq!(added.design.parts.len(), c.design.parts.len() + 1);
    assert!(added.groups[1].parts.contains(&c.design.parts.len()));
    let gone = apply(
        &c.design,
        &c.groups,
        &c.rails,
        &Edit::DeletePart("RC2".into()),
    );
    assert_eq!(gone.design.parts.len(), c.design.parts.len() - 1);
    gone.design.validate().unwrap();
    assert!(
        gone.groups
            .iter()
            .all(|g| g.parts.iter().all(|&p| p < gone.design.parts.len()))
    );
    added.design.validate().unwrap();
}
