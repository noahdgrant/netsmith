//! Requirements on where parts and modules go (ADR 0045): `.place` is narrowed by the board,
//! solved, and carried into the design with the numbers that are left.

use netsmith_check::{Loader, check};
use netsmith_core::{PlaceField, Placement};
use netsmith_design::{Designed, design_board};
use netsmith_solve::{Catalogs, solve};

/// `check` of a source that states its board's physical intent inside the board, as the
/// language took it before ADR 0048: split into the circuit source and the physical file
/// that the language takes now (`netsmith_kicad::testlib::split_physical`).
fn check_split(src: &str) -> netsmith_check::Checked {
    let (circuit, physical) = netsmith_kicad::testlib::split_physical(src);
    let mut loader = Loader::std();
    if let Some(physical) = physical {
        loader.attach_physical("t.nsm", "t.physical.nsm", &physical);
    }
    check("t.nsm", &circuit, &loader)
}

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

/// The messages of the solver or the design stage for a board with `body` added.
fn designed(body: &str) -> Result<Designed, Vec<String>> {
    designed_in(".width &= 40mm; .height &= 30mm", body)
}

/// [`designed`] with the outline's members given.
fn designed_in(outline: &str, body: &str) -> Result<Designed, Vec<String>> {
    let src = format!(
        "{HEAD}board B {{\n    .outline: Outline {{ {outline} }}\n    .j: Header1x2\n{body}}}\n"
    );
    let c = check_split(&src);
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    if !s.diagnostics.is_empty() {
        return Err(s.diagnostics.iter().map(|d| d.message.clone()).collect());
    }
    design_board(&c.boards[0], &s)
}

fn placement(l: &Designed, path: &str) -> Placement {
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
    assert!(designed("").unwrap().design.features.constraints.is_empty());
}

#[test]
fn a_fixed_pose_is_carried_in_millimetres_and_degrees() {
    let l = designed(
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
    let l = designed(
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
    let l = designed("    .j.place.left >= -0.5mm\n").unwrap();
    let (lo, hi) = bound(&placement(&l, "j"), PlaceField::Left);
    assert_eq!(lo, -0.5);
    assert_eq!(hi, f64::INFINITY);
}

#[test]
fn a_module_has_a_box_and_offers_a_part_through_a_value() {
    let l =
        designed("    .p: Pair\n    .p.place.left &= 1mm\n    .p.offered_y &= 12mm +/- 0.5mm\n")
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
    let c = check_split(&src);
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
        design_board(&c.boards[0], &s)
            .unwrap()
            .design
            .features
            .constraints
            .is_empty()
    );
}

#[test]
fn bounds_that_leave_no_position_name_both_with_their_numbers() {
    let e = designed("    .j.place.y >= 12mm\n    .j.place.y <= 3mm\n").unwrap_err();
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
    let e = designed("    .j.place.x &= 2mm\n    .j.place.x >= 5mm\n").unwrap_err();
    assert!(
        e[0].contains("at least 5mm") && e[0].contains("at most 2mm"),
        "{}",
        e[0]
    );
}

#[test]
fn a_rotation_that_is_no_multiple_of_90_is_refused() {
    let e = designed("    .j.place.rotation &= 45\n").unwrap_err();
    assert_eq!(
        e,
        ["`j.place.rotation` allows 45 to 45 degrees, and a part is turned by multiples of 90"]
    );
}

#[test]
fn a_side_other_than_top_or_bottom_is_refused() {
    let e = designed("    .j.place.side &= \"front\"\n").unwrap_err();
    assert_eq!(
        e,
        ["`j.place.side` is \"front\", which is not \"top\" or \"bottom\""]
    );
}

#[test]
fn a_requirement_measured_from_an_outline_that_is_not_fixed_is_refused() {
    let e = designed_in(
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
        designed_in(
            ".width &= 40mm; .height &= 25mm..35mm",
            "    .j.place.x >= .outline.width / 2\n",
        )
        .is_ok()
    );
    // No size at all.
    let e = designed_in(".width &= 40mm", "    .j.place.y <= .outline.height / 2\n").unwrap_err();
    assert!(
        e[0].contains("`outline.height` does not state one size"),
        "{}",
        e[0]
    );
}

/// [`designed`] with a second header `.k` and the module `Pair` as `.pair` on the board.
fn designed_two(body: &str) -> Result<Designed, Vec<String>> {
    designed(&format!("    .k: Header1x2\n    .pair: Pair\n{body}"))
}

fn offsets(l: &Designed) -> Vec<String> {
    l.design
        .features
        .offsets
        .iter()
        .map(|o| {
            format!(
                "{}.{} - {}.{} in {}..{}",
                o.b.path,
                o.b.field.word(),
                o.a.path,
                o.a.field.word(),
                o.lo,
                o.hi
            )
        })
        .collect()
}

#[test]
fn a_relation_between_two_parts_is_an_offset_in_millimetres() {
    let l = designed_two(
        "    .k.place.x - .j.place.x == 17.5mm +/- 0.05mm\n    .pair.a.place.y == .j.place.y\n",
    )
    .unwrap();
    assert_eq!(
        offsets(&l),
        [
            "k.x - j.x in 17.45..17.55",
            // `a == b` is a window of zero, from the part named on the left.
            "pair.a.y - j.y in 0..0",
        ]
    );
    // Each is stated at its own statement, for the check to name its line.
    assert_eq!(l.offset_sites.len(), 2);
}

#[test]
fn a_bound_between_two_parts_is_a_one_sided_window() {
    let l = designed_two(
        "    .k.place.left >= .j.place.right + 2mm\n    .k.place.top <= .j.place.top + 1mm\n",
    )
    .unwrap();
    assert_eq!(
        offsets(&l),
        ["k.left - j.right in 2..inf", "k.top - j.top in -inf..1"]
    );
}

#[test]
fn the_sides_of_a_relation_may_be_swapped_and_the_constants_moved() {
    let l = designed_two("    .j.place.x + 17.5mm == .k.place.x\n").unwrap();
    // The same relation, with the places in the order they came.
    assert_eq!(offsets(&l), ["j.x - k.x in -17.5..-17.5"]);
}

#[test]
fn a_relation_with_an_outline_is_evaluated_for_its_constants() {
    let l = designed_two("    .k.place.x - .j.place.x == .outline.width / 2\n").unwrap();
    assert_eq!(offsets(&l), ["k.x - j.x in 20..20"]);
}

#[test]
fn a_relation_that_is_not_two_places_along_one_axis_is_not_an_offset() {
    // A sum, two axes, a module, a turn and a product are kept as relations, not evaluated.
    let l = designed_two(
        "    .k.place.x + .j.place.x == 30mm\n    .k.place.x - .j.place.y == 3mm\n    .pair.place.left - .j.place.left == 3mm\n    .k.place.rotation == .j.place.rotation\n    .k.place.x - .j.place.x == 2 * .k.place.x\n",
    );
    assert!(offsets(&l.unwrap()).is_empty());
}

#[test]
fn a_relation_of_one_part_with_itself_is_not_an_offset() {
    let l = designed_two("    .j.place.right - .j.place.left == 5mm\n").unwrap();
    assert!(offsets(&l).is_empty());
}

#[test]
fn a_relation_between_parts_is_no_bound_on_either_and_no_note() {
    let l = designed_two(
        "    .j.place.x &= 10mm +/- 1mm\n    .k.place.x >= .j.place.x + 5mm\n    .k.place.x <= .j.place.x + 5.2mm\n",
    )
    .unwrap();
    let j = placement(&l, "j");
    assert_eq!(bound(&j, PlaceField::X), (9.0, 11.0));
    // Nothing narrows `k` by a relation to `j`: the relation is the offsets', not a window.
    assert!(
        l.design.features.constraints.iter().all(|p| p.path != "k"),
        "{:?}",
        l.design.features.constraints
    );
    assert_eq!(
        offsets(&l),
        ["k.x - j.x in 5..inf", "k.x - j.x in -inf..5.2"]
    );
}

#[test]
fn a_relation_that_can_never_be_held_names_both_places_and_the_numbers() {
    // The sum of the places has to lie in 2.5 to 1.5 mm: the left side is wider than the right.
    let e =
        designed_two("    .k.place.x - .j.place.x + (0mm..2mm) == 3mm +/- 0.5mm\n").unwrap_err();
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(
        e[0].starts_with("`k.place.x` minus `j.place.x` cannot be held:")
            && e[0].contains("leaves it at least 2.5 mm and at most 1.5 mm"),
        "{}",
        e[0]
    );
}

#[test]
fn two_relations_that_disagree_are_the_placers_to_report() {
    let l =
        designed_two("    .k.place.x >= .j.place.x + 5mm\n    .k.place.x <= .j.place.x + 4mm\n")
            .unwrap();
    assert_eq!(offsets(&l).len(), 2);
}

#[test]
fn the_solver_lists_no_relation_between_parts_as_not_evaluated() {
    let src = format!(
        "{HEAD}board B {{\n    .outline: Outline {{ .width &= 40mm; .height &= 30mm }}\n    .j: Header1x2\n    .k: Header1x2\n    .k.place.x - .j.place.x == 17.5mm +/- 0.05mm\n    .k.place.x * 2 == .j.place.x\n}}\n"
    );
    let c = check_split(&src);
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    let shown: Vec<String> = s
        .deferred
        .iter()
        .map(|&i| s.model.show_expr(&s.model.relations()[i].lhs))
        .collect();
    assert_eq!(shown.len(), 1, "{shown:?}");
}
