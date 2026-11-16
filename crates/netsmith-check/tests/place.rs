//! The built-in `.place` of parts and modules (ADR 0045), stated in a board's physical
//! file (ADR 0048).

use netsmith_check::*;

const HEAD: &str = "\
import std::parts::connector::Header1x2
import std::parts::passive::Resistor

module Block {
    .j: Header1x2
}

";

/// A board `B` whose circuit source is `body` and whose physical file holds `physical`.
fn run(body: &str, physical: &str) -> Checked {
    let loader = Loader::std().with_physical("t.nsm", "t.physical.nsm", physical);
    check("t.nsm", &format!("{HEAD}{body}"), &loader)
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn every_part_and_module_has_a_place_with_a_pose() {
    let c = run(
        "board B {\n    .j: Header1x2\n    .b: Block\n}\n",
        "physical for B {\n    .j.place.x &= 2mm +/- 0.2mm\n    .j.place.rotation &= 90\n    .j.place.side &= \"bottom\"\n    .b.place.left &= 1mm\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    assert!(m.field_at("j.place.left").is_some());
    assert!(m.field_at("j.place.rotation").is_some());
    assert!(m.field_at("b.place.bottom").is_some());
    assert_eq!(
        m.domain_at("j.place.side"),
        Some(&Domain::Text("bottom".into()))
    );
}

#[test]
fn a_position_holds_a_length_and_a_rotation_a_plain_number() {
    let c = run(
        "board B {\n    .j: Header1x2\n}\n",
        "physical for B {\n    .j.place.x &= 5\n    .j.place.rotation &= 90mm\n}\n",
    );
    assert_eq!(
        messages(&c),
        [
            "`j.place.x` holds m, but this value is a plain number",
            "`j.place.rotation` holds a plain number, but this value is m"
        ]
    );
}

#[test]
fn a_module_has_a_box_and_a_face_but_no_turn() {
    let c = run(
        "board B {\n    .b: Block\n}\n",
        "physical for B {\n    .b.place.rotation &= 90\n}\n",
    );
    assert_eq!(messages(&c), ["`Place` has no member `rotation`"]);
    let c = run(
        "board B {\n    .b: Block\n}\n",
        "physical for B {\n    .b.place.side &= \"top\"\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_face_on_a_module_is_a_requirement_on_every_part_of_it() {
    let c = run(
        "module Deep {\n    .inner: Block\n    .r: Resistor {}\n}\nboard B {\n    .d: Deep\n    .j: Header1x2\n}\n",
        "physical for B {\n    .d.place.side &= \"bottom\"\n    .j.place.side &= \"top\"\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    let bottom = Some(&Domain::Text("bottom".into()));
    assert_eq!(m.domain_at("d.r.place.side"), bottom);
    assert_eq!(m.domain_at("d.inner.j.place.side"), bottom);
    assert_eq!(
        m.domain_at("j.place.side"),
        Some(&Domain::Text("top".into()))
    );
}

#[test]
fn a_part_on_the_other_face_conflicts_with_its_module_and_the_message_says_so() {
    let c = run(
        "board B {\n    .b: Block\n}\n",
        "physical for B {\n    .b.j.place.side &= \"top\"\n    .b.place.side &= \"bottom\"\n}\n",
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].contains("`b` is on the bottom face, which is a requirement on every part of it")
            && m[0].contains("`b.j.place.side` is already \"top\" (stated at t.physical.nsm:2)")
            && m[0].contains("state the faces of its parts one by one"),
        "{}",
        m[0]
    );
    // The order of the two lines does not matter: the part's line names the module's.
    let c = run(
        "board B {\n    .b: Block\n}\n",
        "physical for B {\n    .b.place.side &= \"bottom\"\n    .b.j.place.side &= \"top\"\n}\n",
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{}", c.render());
    assert!(
        m[0].contains("`b` is on the bottom face, which is a requirement on every part of it (stated at t.physical.nsm:2)")
            && m[0].contains("but `b.j.place.side` is \"top\"")
            && m[0].contains("state the faces of its parts one by one"),
        "{}",
        m[0]
    );
}

#[test]
fn place_is_already_declared() {
    let c = run(
        "module M {\n    .place: Header1x2\n}\nboard B {\n    .m: M\n}\n",
        "",
    );
    assert_eq!(messages(&c), ["`place` is already declared in this block"]);
}

#[test]
fn a_face_on_the_board_is_a_requirement_on_every_part_of_it() {
    let c = run(
        "module Deep {\n    .inner: Block\n    .r: Resistor {}\n}\nboard B {\n    .d: Deep\n    .j: Header1x2\n}\n",
        "physical for B {\n    .place.side &= \"top\"\n    .j.place.side &= \"top\"\n    .d.place.side &= \"top\"\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    let top = Some(&Domain::Text("top".into()));
    assert_eq!(m.domain_at("d.r.place.side"), top);
    assert_eq!(m.domain_at("d.inner.j.place.side"), top);
    assert_eq!(m.domain_at("j.place.side"), top);
}

#[test]
fn a_board_has_a_face_but_no_box_and_no_turn() {
    let c = run(
        "board B {\n    .j: Header1x2\n}\n",
        "physical for B {\n    .place.x &= 2mm\n    .place.rotation &= 90\n}\n",
    );
    assert_eq!(
        messages(&c),
        [
            "`Place` has no member `x`",
            "`Place` has no member `rotation`"
        ]
    );
}

#[test]
fn a_part_on_the_other_face_conflicts_with_the_board_and_the_message_says_so() {
    // The part's line first, then the board's.
    let c = run(
        "board B {\n    .j: Header1x2\n    .r: Resistor {}\n}\n",
        "physical for B {\n    .j.place.side &= \"bottom\"\n    .place.side &= \"top\"\n}\n",
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].contains("the board `B` is on the top face, which is a requirement on every part of it")
            && m[0].contains("`j.place.side` is already \"bottom\" (stated at t.physical.nsm:2)")
            && m[0].contains("the board `B` is assembled on one face only"),
        "{}",
        m[0]
    );
    // The board's line first, then the part's.
    let c = run(
        "board B {\n    .j: Header1x2\n}\n",
        "physical for B {\n    .place.side &= \"top\"\n    .j.place.side &= \"bottom\"\n}\n",
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{}", c.render());
    assert!(
        m[0].contains("the board `B` is on the top face, which is a requirement on every part of it (stated at t.physical.nsm:2)")
            && m[0].contains("but `j.place.side` is \"bottom\"")
            && m[0].contains("take `.place.side` off the board"),
        "{}",
        m[0]
    );
}

#[test]
fn a_module_on_the_other_face_conflicts_with_the_board_in_either_order() {
    for physical in [
        "physical for B {\n    .place.side &= \"top\"\n    .b.place.side &= \"bottom\"\n}\n",
        "physical for B {\n    .b.place.side &= \"bottom\"\n    .place.side &= \"top\"\n}\n",
    ] {
        let c = run("board B {\n    .b: Block\n}\n", physical);
        let m = messages(&c);
        assert_eq!(m.len(), 1, "{}", c.render());
        assert!(
            m[0].contains("the board `B`")
                && m[0].contains("`b")
                && m[0].contains("(stated at t.physical.nsm:")
                && m[0].contains("assembled on one face only"),
            "{}",
            m[0]
        );
    }
}

#[test]
fn a_face_on_the_board_belongs_in_its_physical_file() {
    let c = run(
        "board B {\n    .j: Header1x2\n    .place.side &= \"top\"\n}\n",
        "",
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(m[0].contains("physical intent"), "{}", m[0]);
}
