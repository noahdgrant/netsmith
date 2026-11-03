//! Requirements on where parts and modules go (ADR 0045): `.place` is narrowed by the board,
//! solved, and carried into the design with the numbers that are left.

use netsmith_check::{Loader, check};
use netsmith_core::{PlaceField, Placement};
use netsmith_lower::{Lowered, lower};
use netsmith_solve::{Catalogs, solve};

const HEAD: &str = "\
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::pcb::Outline

module Pair {
    .offered_y: Length
    .a: Resistor
    .a.place.y &= .offered_y
}

";

/// The messages of the solver or the lowering for a board with `body` added.
fn lowered(body: &str) -> Result<Lowered, Vec<String>> {
    lowered_in(".width &= 40mm; .height &= 30mm", body)
}

/// [`lowered`] with the outline's members given.
fn lowered_in(outline: &str, body: &str) -> Result<Lowered, Vec<String>> {
    let src = format!(
        "{HEAD}board B {{\n    .outline: Outline {{ {outline} }}\n    .j: Header1x2\n{body}}}\n"
    );
    let c = check("t.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    if !s.diagnostics.is_empty() {
        return Err(s.diagnostics.iter().map(|d| d.message.clone()).collect());
    }
    lower(&c.boards[0], &s)
}

fn placement(l: &Lowered, path: &str) -> Placement {
    l.design
        .features
        .constraints
        .iter()
        .find(|p| p.path == path)
        .unwrap_or_else(|| {
            panic!(
                "no requirement on `{path}`: {:?}",
                l.design.features.constraints
            )
        })
        .clone()
}

fn bound(p: &Placement, f: PlaceField) -> (f64, f64) {
    let b = p.bound(f).unwrap_or_else(|| panic!("no {f:?} in {p:?}"));
    (b.lo, b.hi)
}

#[test]
fn a_board_without_requirements_has_none() {
    assert!(lowered("").unwrap().design.features.constraints.is_empty());
}

#[test]
fn a_fixed_pose_is_carried_in_millimetres_and_degrees() {
    let l = lowered(
        "    .j.place.x &= 2mm\n    .j.place.y &= 15mm\n    .j.place.rotation &= 270\n    .j.place.side &= \"top\"\n",
    )
    .unwrap();
    let p = placement(&l, "j");
    assert!(!p.module);
    assert_eq!(bound(&p, PlaceField::X), (2.0, 2.0));
    assert_eq!(bound(&p, PlaceField::Y), (15.0, 15.0));
    assert_eq!(bound(&p, PlaceField::Rotation), (270.0, 270.0));
    assert_eq!(p.side.as_deref(), Some("top"));
    assert!(p.bound(PlaceField::Left).is_none());
}

#[test]
fn a_tolerance_is_a_window_and_two_relations_are_one_too() {
    let l = lowered(
        "    .j.place.x &= 2mm +/- 0.2mm\n    .j.place.y >= .outline.height / 2 - 0.1mm\n    .j.place.y <= .outline.height / 2 + 0.1mm\n",
    )
    .unwrap();
    let p = placement(&l, "j");
    let (lo, hi) = bound(&p, PlaceField::X);
    assert!(
        (lo - 1.8).abs() < 1e-9 && (hi - 2.2).abs() < 1e-9,
        "{lo} {hi}"
    );
    let (lo, hi) = bound(&p, PlaceField::Y);
    assert!(
        (lo - 14.9).abs() < 1e-9 && (hi - 15.1).abs() < 1e-9,
        "{lo} {hi}"
    );
}

#[test]
fn a_bound_with_one_side_open_has_an_infinite_end() {
    let l = lowered("    .j.place.left >= -0.5mm\n").unwrap();
    let (lo, hi) = bound(&placement(&l, "j"), PlaceField::Left);
    assert_eq!(lo, -0.5);
    assert_eq!(hi, f64::INFINITY);
}

#[test]
fn a_module_has_a_box_and_offers_a_part_through_a_value() {
    let l = lowered("    .p: Pair\n    .p.place.left &= 1mm\n    .p.offered_y &= 12mm +/- 0.5mm\n")
        .unwrap();
    let module = placement(&l, "p");
    assert!(module.module);
    assert_eq!(bound(&module, PlaceField::Left), (1.0, 1.0));
    let part = placement(&l, "p.a");
    let (lo, hi) = bound(&part, PlaceField::Y);
    assert!((lo - 11.5).abs() < 1e-9 && (hi - 12.5).abs() < 1e-9);
    // Constraints come in the order of the paths.
    let paths: Vec<_> = l
        .design
        .features
        .constraints
        .iter()
        .map(|p| p.path.as_str())
        .collect();
    assert_eq!(paths, ["p", "p.a"]);
}

#[test]
fn an_offer_the_board_leaves_alone_is_no_requirement_and_no_note() {
    let src = format!("{HEAD}board B {{\n    .p: Pair\n}}\n");
    let c = check("t.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty());
    let notes: Vec<_> = s
        .deferred
        .iter()
        .map(|&i| {
            c.boards[0]
                .model
                .show_expr(&c.boards[0].model.relations()[i].lhs)
        })
        .collect();
    assert!(!notes.iter().any(|n| n.contains("place")), "{notes:?}");
    assert!(
        lower(&c.boards[0], &s)
            .unwrap()
            .design
            .features
            .constraints
            .is_empty()
    );
}

#[test]
fn bounds_that_leave_no_position_name_both_with_their_numbers() {
    let e = lowered("    .j.place.y >= 12mm\n    .j.place.y <= 3mm\n").unwrap_err();
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(
        e[0].contains("the requirements on `j.place.y` cannot all hold")
            && e[0].contains("at least 12mm (`j.place.y >= 12mm`)")
            && e[0].contains("at most 3mm (`j.place.y <= 3mm`)"),
        "{}",
        e[0]
    );
}

#[test]
fn a_window_that_does_not_meet_a_stated_value_is_a_contradiction() {
    let e = lowered("    .j.place.x &= 2mm\n    .j.place.x >= 5mm\n").unwrap_err();
    assert!(
        e[0].contains("at least 5mm") && e[0].contains("at most 2mm"),
        "{}",
        e[0]
    );
}

#[test]
fn a_rotation_that_is_no_multiple_of_90_is_refused() {
    let e = lowered("    .j.place.rotation &= 45\n").unwrap_err();
    assert_eq!(
        e,
        ["`j.place.rotation` allows 45 to 45 degrees, and a part is turned by multiples of 90"]
    );
}

#[test]
fn a_side_other_than_top_or_bottom_is_refused() {
    let e = lowered("    .j.place.side &= \"front\"\n").unwrap_err();
    assert_eq!(
        e,
        ["`j.place.side` is \"front\", which is not \"top\" or \"bottom\""]
    );
}

#[test]
fn a_requirement_measured_from_an_outline_that_is_not_fixed_is_refused() {
    let e = lowered_in(
        ".width &= 40mm; .height &= 25mm..35mm",
        "    .j.place.y >= .outline.height / 2 - 0.1mm\n",
    )
    .unwrap_err();
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(
        e[0].contains("`j.place.y` is measured from the outline's top left corner")
            && e[0].contains("`outline.height` does not state one size"),
        "{}",
        e[0]
    );
    // Width alone is fine for a requirement that does not use the height.
    assert!(
        lowered_in(
            ".width &= 40mm; .height &= 25mm..35mm",
            "    .j.place.x >= .outline.width / 2\n",
        )
        .is_ok()
    );
    // No size at all.
    let e = lowered_in(".width &= 40mm", "    .j.place.y <= .outline.height / 2\n").unwrap_err();
    assert!(
        e[0].contains("`outline.height` does not state one size"),
        "{}",
        e[0]
    );
}
