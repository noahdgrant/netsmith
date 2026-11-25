//! A pad's own connection stated off a part (issue #1674): `.pour_connection` and `.spoke_width`
//! reach the pads of a part's pin, so a port of a module or a pin of a rail, which has no pads, is
//! refused, and the message names the pins of parts on its net inside the module or board.

use netsmith_check::*;

const PARTS: &str = r#"
import std::mfg::stackup::PadConnection
import std::parts::connector::Header1x2
import std::sources::Rail

/// An invented converter with its ground on two pins.
part Conv {
    .designator &= "U"
    .vin: Rail
    pins {
        .vin.pos power   "VIN"  "1"
        .vin.neg power   "GND"  "2"
        .gnd2    power   "GND2" "3"
    }
}

/// An invented stage: the converter, with its second ground joined to the port's.
module Stage {
    .vin: Rail
    .u: Conv
    .vin ~ .u.vin
    .u.gnd2 ~ .vin.neg
}
"#;

fn run(rest: &str) -> Checked {
    check("main.nsm", &format!("{PARTS}\n{rest}"), &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn a_setting_on_a_port_of_a_module_from_the_board_names_the_pins_of_parts_inside_it() {
    let c = run(
        "board B {
    .supply: Header1x2
    .stage: Stage
    .supply.p1 ~ .stage.vin.pos
    .supply.p2 ~ .stage.vin.neg
    .supply.p2.net &= \"GND\"
    .stage.vin.neg.pour_connection &= PadConnection::Solid
}",
    );
    assert_eq!(
        messages(&c),
        [
            "`.stage.vin.neg.pour_connection` reaches no pad: `.stage.vin.neg` is a pin of `.stage.vin`, a `Rail` port of module `Stage`, not a pin of a part, and has no pads; a pour joins the pads of a part's pin as that pin states: state it on the pin of the part you mean, such as `.stage.u.gnd2.pour_connection`; the pins of parts on its net inside `.stage` are `.stage.u.gnd2`, `.stage.u.vin.neg`",
        ],
        "{}",
        c.render()
    );
}

#[test]
fn a_setting_on_its_own_port_in_a_module_body_names_the_pins_from_the_module() {
    let c = run(
        "module Spoked {
    .vin: Rail
    .u: Conv
    .vin ~ .u.vin
    .u.gnd2 ~ .vin.neg
    .vin.neg.spoke_width &= 0.3mm..
}
board B {
    .supply: Header1x2
    .a: Spoked
    .b: Spoked
    .supply.p1 ~ .a.vin.pos
    .supply.p2 ~ .a.vin.neg
    .a.vin ~ .b.vin
    .supply.p2.net &= \"GND\"
}",
    );
    // One message for the line, though two uses of the module read it.
    assert_eq!(
        messages(&c),
        [
            "`.vin.neg.spoke_width` reaches no pad: `.vin.neg` is a pin of `.vin`, a `Rail` port of module `Spoked`, not a pin of a part, and has no pads; a pour joins the pads of a part's pin as that pin states: state it on the pin of the part you mean, such as `.u.gnd2.spoke_width`; the pins of parts on its net are `.u.gnd2`, `.u.vin.neg`",
        ],
        "{}",
        c.render()
    );
}

#[test]
fn a_setting_on_a_rail_of_the_board_names_the_pins_of_parts_on_its_net() {
    let c = run(
        "board B {
    .input: Rail { .pos.net &= \"VIN\"; .neg.net &= \"GND\" }
    .feed: Header1x2
    .u: Conv
    .input ~ .u.vin
    .u.gnd2 ~ .input.neg
    .feed.p1 ~ .input.pos
    .feed.p2 ~ .input.neg
    .input.neg.pour_connection &= PadConnection::Solid
}",
    );
    assert_eq!(
        messages(&c),
        [
            "`.input.neg.pour_connection` reaches no pad: `.input.neg` is a pin of `.input`, a `Rail` interface of the board, not a pin of a part, and has no pads; a pour joins the pads of a part's pin as that pin states: state it on the pin of the part you mean, such as `.feed.p2.pour_connection`; the pins of parts on its net are `.feed.p2`, `.u.gnd2`, `.u.vin.neg`",
        ],
        "{}",
        c.render()
    );
}

#[test]
fn a_setting_on_a_port_with_no_part_on_its_net_says_so() {
    let c = run(
        "module Empty {
    .vin: Rail
    .vin.neg.pour_connection &= PadConnection::Thermal
}
board B {
    .feed: Header1x2
    .e: Empty
    .feed.p1 ~ .e.vin.pos
    .feed.p2 ~ .e.vin.neg
}",
    );
    assert_eq!(
        messages(&c),
        [
            "`.vin.neg.pour_connection` reaches no pad: `.vin.neg` is a pin of `.vin`, a `Rail` port of module `Empty`, not a pin of a part, and has no pads; a pour joins the pads of a part's pin as that pin states, and no pin of a part is on its net: state `.pour_connection` on the pin of the part whose pads it is for",
        ],
        "{}",
        c.render()
    );
}

#[test]
fn a_setting_on_a_pin_of_a_part_is_kept() {
    let c = run(
        "module Solid {
    .vin: Rail
    .u: Conv
    .vin ~ .u.vin
    .u.gnd2 ~ .vin.neg
    .u.vin.neg.pour_connection &= PadConnection::Solid
}
board B {
    .feed: Header1x2
    .s: Solid
    .t: Stage
    .feed.p1 ~ .s.vin.pos
    .feed.p2 ~ .s.vin.neg
    .s.vin ~ .t.vin
    .feed.p2.net &= \"GND\"
    .t.u.gnd2.pour_connection &= PadConnection::Solid
    .t.u.gnd2.spoke_width &= 0.3mm..
    .feed.p1.spoke_width &= 0.3mm..
}",
    );
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.board("B").unwrap().model;
    assert!(matches!(
        model.domain_at("s.u.vin.neg.pour_connection"),
        Some(Domain::Text(v)) if v == "Solid"
    ));
    assert!(matches!(
        model.domain_at("t.u.gnd2.pour_connection"),
        Some(Domain::Text(v)) if v == "Solid"
    ));
}
