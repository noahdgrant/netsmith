//! `.a ?~ .b`, a join of two pins that a join of the first pin from outside the part replaces
//! (ADR 0130), here as the `Lt8641` uses it for its `BIAS` pin.

use netsmith_check::{Checked, Loader, check};

fn board(lines: &str) -> String {
    format!(
        "import std::parts::regulator::{{Buck, Lt8641}}\nimport std::sources::Rail\n\n\
         board B {{\n    .vin: Rail {{ .voltage &= 12V +/- 5%; .pos.net &= \"VIN\"; .neg.net &= \"GND\" }}\n    \
         .vout: Rail {{ .pos.net &= \"5V\"; .neg.net &= \"GND\" }}\n    .reg: Buck<Lt8641>\n{lines}    .reg.vin ~ .vin\n    .reg.vout ~ .vout\n}}\n"
    )
}

/// The name of the net a pin of the board is on.
fn net_of(c: &Checked, pin: &str) -> String {
    let m = &c.boards[0].model;
    m.nets()
        .iter()
        .find(|n| n.pins.iter().any(|&p| m.path_of(p) == pin))
        .map(|n| n.name.clone())
        .unwrap_or_else(|| panic!("`{pin}` is on no net"))
}

#[test]
fn a_default_join_holds_while_nothing_else_joins_the_pin() {
    let c = check("t.nsm", &board(""), &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(net_of(&c, "reg.ic.bias"), "GND");
}

#[test]
fn a_join_from_the_board_replaces_the_default() {
    let c = check(
        "t.nsm",
        &board("    .reg.ic.bias ~ .vout.pos\n"),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(net_of(&c, "reg.ic.bias"), "5V");
    // Nothing else of the part moved: its ground pin is still on the ground.
    assert_eq!(net_of(&c, "reg.ic.vin.neg"), "GND");
}

#[test]
fn the_order_of_the_statements_does_not_matter() {
    let first = check(
        "t.nsm",
        &board("    .reg.ic.bias ~ .vout.pos\n"),
        &Loader::std(),
    );
    let src = board("    .reg.ic.bias ~ .vout.pos\n").replace(
        "    .reg: Buck<Lt8641>\n    .reg.ic.bias ~ .vout.pos\n",
        "    .reg.ic.bias ~ .vout.pos\n    .reg: Buck<Lt8641>\n",
    );
    let second = check("t.nsm", &src, &Loader::std());
    assert!(first.is_ok() && second.is_ok(), "{}", second.render());
    assert_eq!(net_of(&second, "reg.ic.bias"), net_of(&first, "reg.ic.bias"));
}

#[test]
fn a_board_cannot_write_a_default_join() {
    let c = check(
        "t.nsm",
        &board("    .reg.ic.bias ?~ .vout.pos\n"),
        &Loader::std(),
    );
    assert!(!c.is_ok());
    let text = c.render();
    assert!(
        text.contains("`?~` offers a default join in a part's own definition, and this is not one: a board joins with `~`"),
        "{text}"
    );
}

#[test]
fn a_default_join_of_a_part_holds_in_a_part_placed_alone() {
    let src = "part P {\n    pins {\n        .x input \"X\" \"1\" optional\n        .y input \"Y\" \"2\" optional\n    }\n    .x ?~ .y\n}\n\
        board B {\n    .p: P\n}\n";
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(net_of(&c, "p.x"), net_of(&c, "p.y"));
}

#[test]
fn a_default_join_joined_elsewhere_gives_way_in_a_part_placed_alone() {
    let src = "part P {\n    pins {\n        .x input \"X\" \"1\" optional\n        .y input \"Y\" \"2\" optional\n        .z input \"Z\" \"3\" optional\n    }\n    .x ?~ .y\n}\n\
        board B {\n    .p: P\n    .p.x ~ .p.z\n}\n";
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(net_of(&c, "p.x"), net_of(&c, "p.z"));
    let m = &c.boards[0].model;
    assert!(
        !m.nets()
            .iter()
            .any(|n| n.pins.iter().any(|&p| m.path_of(p) == "p.y")
                && n.pins.iter().any(|&p| m.path_of(p) == "p.x")),
        "y is not on x's net"
    );
}
