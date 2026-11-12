//! The `polygon` statement of an `Outline` (issue #57): the checker keeps its corners in mm,
//! and every refusal names the value and what to write.

use netsmith_check::*;

const HEAD: &str = "\
import std::parts::connector::Header1x2

board B {
    .j: Header1x2
}
";

fn physical(body: &str) -> Checked {
    let text = format!(
        "import std::pcb::{{Cutout, Outline}}\nphysical for B {{\n    case {{\n{body}\n    }}\n}}\n"
    );
    let loader = Loader::std().with_physical("t.nsm", "t.physical.nsm", &text);
    check("t.nsm", HEAD, &loader)
}

fn outline(polygon: &str) -> Checked {
    physical(&format!(
        "        .outline: Outline {{\n            {polygon}\n        }}"
    ))
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn the_corners_are_kept_in_millimetres_with_their_fillets_and_arcs() {
    let c = outline(
        "polygon (0mm, 0mm) (20mm + 5mm, 0mm) round 2mm (25mm, 15mm) (40mm, 15mm)\n                (40mm, 30mm) arc through (20mm, 30mm + 4mm) (0mm, 30mm) round 3mm",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    let outline = match m.member(m.root, "outline") {
        Some(Member::Inst(i)) => i,
        other => panic!("{other:?}"),
    };
    let p = m.polygon_of(outline).expect("the polygon");
    let corners: Vec<PolygonCorner> = p.corners.clone();
    assert_eq!(corners.len(), 6);
    assert_eq!(corners[1].at, (25.0, 0.0));
    assert_eq!(corners[1].round, Some(2.0));
    assert_eq!(corners[4].through, Some((20.0, 34.0)));
    assert_eq!(corners[5].round, Some(3.0));
    assert_eq!(corners[0].round, None);
}

#[test]
fn a_range_a_relation_or_a_value_that_is_not_a_length_is_refused() {
    let c = outline("polygon (0mm, 0mm) (40mm, 0mm..1mm) (40mm, 30mm) (0mm, 30mm)");
    assert_eq!(
        messages(&c),
        [
            "a corner of a `polygon` is one value, and `0mm..1mm` is a range (0mm..1mm): the outline is what the case drawing says, so write the one number"
        ]
    );
    let c = outline("polygon (0mm, 0mm) (.width, 0mm) (40mm, 30mm) (0mm, 30mm)");
    assert_eq!(
        messages(&c),
        [
            "a corner of a `polygon` is a fixed number, and `.width` reads another value: write the number the case drawing gives"
        ]
    );
    let c = outline("polygon (0mm, 0mm) (40mm, 0mm) round 2V (40mm, 30mm) (0mm, 30mm)");
    assert_eq!(
        messages(&c),
        ["a fillet radius of a `polygon` is a length such as `12mm`, and `2V` is not"]
    );
    let c = outline(
        "polygon (0mm, 0mm) (40mm, 0mm) arc through (20mm +/- 1mm, 1mm) (40mm, 30mm) (0mm, 30mm)",
    );
    assert!(
        messages(&c)[0].starts_with("the point an arc passes through of a `polygon` is one value, and `20mm +/- 1mm` is a range"),
        "{:?}",
        messages(&c)
    );
}

#[test]
fn a_fillet_of_no_radius_and_a_run_of_two_corners_are_refused() {
    let c = outline("polygon (0mm, 0mm) round 0mm (40mm, 0mm) (40mm, 30mm)");
    assert_eq!(
        messages(&c),
        [
            "a fillet radius is more than 0 mm, and `round 0mm` is not; leave `round` out for a sharp corner"
        ]
    );
    let c = outline("polygon (0mm, 0mm) (40mm, 0mm)");
    assert_eq!(
        messages(&c),
        ["a `polygon` has at least three corners, and this one has 2"]
    );
}

#[test]
fn a_polygon_outside_the_outline_or_in_the_circuit_source_is_refused() {
    let c = physical(
        "        .slot: Cutout {\n            polygon (0mm, 0mm) (4mm, 0mm) (4mm, 4mm)\n        }",
    );
    let refusal = "a `polygon` is the board edge, and it belongs in the board's `.outline: Outline { ... }` in the `case` section of its physical file (issue #57)";
    assert!(
        messages(&c).iter().any(|m| m == refusal),
        "{:?}",
        messages(&c)
    );
    let c = check(
        "t.nsm",
        "board B {\n    polygon (0mm, 0mm) (4mm, 0mm) (4mm, 4mm)\n}\n",
        &Loader::std(),
    );
    assert_eq!(messages(&c), [refusal]);
}

#[test]
fn an_outline_has_one_polygon() {
    let c = outline(
        "polygon (0mm, 0mm) (4mm, 0mm) (4mm, 4mm)\n            polygon (0mm, 0mm) (5mm, 0mm) (5mm, 5mm)",
    );
    assert_eq!(
        messages(&c),
        ["the outline already has a `polygon`, and a board has one edge"]
    );
}
