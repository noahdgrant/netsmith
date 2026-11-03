//! The built-in `.place` of parts and modules (ADR 0045).

use netsmith_check::*;

const HEAD: &str = "\
import std::parts::connector::Header1x2
import std::parts::passive::Resistor

module Block {
    .j: Header1x2
}

";

fn run(body: &str) -> Checked {
    check("t.nsm", &format!("{HEAD}{body}"), &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn every_part_and_module_has_a_place_with_a_pose() {
    let c = run(
        "board B {\n    .j: Header1x2\n    .b: Block\n    .j.place.x &= 2mm +/- 0.2mm\n    .j.place.rotation &= 90\n    .j.place.side &= \"bottom\"\n    .b.place.left &= 1mm\n}\n",
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
        "board B {\n    .j: Header1x2\n    .j.place.x &= 5\n    .j.place.rotation &= 90mm\n}\n",
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
fn a_module_has_a_box_and_no_turn_or_face() {
    let c = run("board B {\n    .b: Block\n    .b.place.rotation &= 90\n}\n");
    assert_eq!(messages(&c), ["`Place` has no member `rotation`"]);
    let c = run("board B {\n    .b: Block\n    .b.place.side &= \"top\"\n}\n");
    assert_eq!(messages(&c), ["`Place` has no member `side`"]);
}

#[test]
fn a_board_cannot_place_a_part_inside_a_module() {
    let c = run("board B {\n    .b: Block\n    .b.j.place.x &= 2mm\n}\n");
    assert_eq!(
        messages(&c),
        [
            "`j` is inside `Block` and is not part of its interface; set it through a rail, a pin, or a value that `Block` declares"
        ]
    );
}

#[test]
fn place_is_already_declared() {
    let c = run("module M {\n    .place: Header1x2\n}\nboard B {\n    .m: M\n}\n");
    assert_eq!(messages(&c), ["`place` is already declared in this block"]);
}
