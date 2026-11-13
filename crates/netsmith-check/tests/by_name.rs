//! `.left ~ .right by name`: the members of two ports joined name by name (ADR 0117). The parts
//! are invented for these tests.

use std::collections::BTreeSet;

use netsmith_check::*;

const PARTS: &str = r#"
import std::sources::Rail

/// A header: four plain pins.
part Header {
    .designator &= "J"
    pins {
        .io4  passive "IO4" "1"
        .io5  passive "IO5" "2"
        .en   passive "EN"  "3"
        .gnd  passive "GND" "4"
    }
}

/// A header whose pins are named as the board's silkscreen, not as the chip's.
part Silkscreen {
    .designator &= "J"
    pins {
        .gpio4 passive "GPIO4" "1"
        .gpio5 passive "GPIO5" "2"
        .en    passive "EN"    "3"
    }
}

/// A chip with a supply rail and three signals.
part Chip {
    .designator &= "U"
    .vdd: Rail
    pins {
        .vdd.pos power         "VDD" "1"
        .vdd.neg power         "VSS" "2"
        .io4     bidirectional "IO4" "3"
        .io5     bidirectional "IO5" "4"
        .en      bidirectional "EN"  "5"
    }
}

/// A chip whose `.vdd` is a plain pin.
part PinSupply {
    .designator &= "U"
    pins {
        .vdd passive "VDD" "1"
        .io4 passive "IO4" "2"
    }
}

interface TwoWire {
    .sda: Pin
    .scl: Pin
}

interface Pads {
    .sda: Pin
    .scl: Pin
}

part Sensor {
    .designator &= "U"
    .bus: TwoWire
    pins {
        .bus.sda bidirectional "SDA" "1"
        .bus.scl bidirectional "SCL" "2"
    }
}

part Probe {
    .designator &= "J"
    .bus: Pads
    pins {
        .bus.sda passive "SDA" "1"
        .bus.scl passive "SCL" "2"
    }
}

/// A module with a supply rail and a signal.
module Left {
    .vdd: Rail
    .io4: Pin
    .u: PinSupply
    .u.io4 ~ .io4
    .u.vdd ~ .vdd.pos
}

module Right {
    .vdd: Rail
    .io4: Pin
    .u: Chip
    .u.io4 ~ .io4
    .vdd ~ .u.vdd
}
"#;

const SUPPLY: &str = "    .supply: Rail {\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }\n";

fn run(body: &str) -> Checked {
    check(
        "test.nsm",
        &format!("{PARTS}\nboard B {{\n{SUPPLY}{body}}}\n"),
        &Loader::std(),
    )
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// The board's nets as sets of part terminals, without their names.
fn nets(c: &Checked) -> BTreeSet<BTreeSet<String>> {
    let m = &c.board("B").expect("the board is there").model;
    m.nets()
        .iter()
        .map(|n| n.terminals.iter().map(|&t| m.path_of(t)).collect())
        .collect()
}

/// The errors of a board, which must have some.
fn errors(body: &str) -> Vec<String> {
    let c = run(body);
    assert!(!c.is_ok(), "expected errors:\n{}", c.render());
    messages(&c)
}

/// Both boards check, and they have the same nets.
fn same_nets(by_name: &str, by_hand: &str) {
    let a = run(by_name);
    assert!(a.is_ok(), "{}", a.render());
    let b = run(by_hand);
    assert!(b.is_ok(), "{}", b.render());
    assert_eq!(nets(&a), nets(&b));
}

const PLACED: &str = "    .h: Header\n    .u: Chip\n    .supply ~ .u.vdd\n    .h.gnd ~ .supply.neg\n";

#[test]
fn members_with_the_same_name_are_joined() {
    same_nets(
        &format!("{PLACED}    .h ~ .u by name except {{ gnd vdd }}\n"),
        &format!("{PLACED}    .h.io4 ~ .u.io4\n    .h.io5 ~ .u.io5\n    .h.en ~ .u.en\n"),
    );
}

#[test]
fn a_rename_joins_members_whose_names_differ() {
    let placed = "    .h: Silkscreen\n    .u: Chip\n    .supply ~ .u.vdd\n";
    same_nets(
        &format!("{placed}    .h ~ .u by name {{ gpio4 = io4; gpio5 = io5 }} except {{ vdd }}\n"),
        &format!("{placed}    .h.gpio4 ~ .u.io4\n    .h.gpio5 ~ .u.io5\n    .h.en ~ .u.en\n"),
    );
}

#[test]
fn a_rename_map_may_span_lines() {
    let placed = "    .h: Silkscreen\n    .u: Chip\n    .supply ~ .u.vdd\n";
    same_nets(
        &format!("{placed}    .h ~ .u by name {{\n        gpio4 = io4\n        gpio5 = io5\n    }} except {{ vdd }}\n"),
        &format!("{placed}    .h.gpio4 ~ .u.io4\n    .h.gpio5 ~ .u.io5\n    .h.en ~ .u.en\n"),
    );
}

#[test]
fn nested_ports_and_interface_members_are_joined() {
    // The ports are interfaces inside parts, of two kinds with the same member names.
    let placed = "    .s: Sensor\n    .p: Probe\n";
    same_nets(
        &format!("{placed}    .s.bus ~ .p.bus by name\n"),
        &format!("{placed}    .s.bus.sda ~ .p.bus.sda\n    .s.bus.scl ~ .p.bus.scl\n"),
    );
    // A rail on each side is joined whole, as `~` joins two rails.
    let placed = "    .l: Left\n    .r: Right\n    .supply ~ .r.vdd\n";
    same_nets(
        &format!("{placed}    .l ~ .r by name\n"),
        &format!("{placed}    .l.vdd ~ .r.vdd\n    .l.io4 ~ .r.io4\n"),
    );
}

#[test]
fn a_member_with_no_partner_is_named() {
    assert_eq!(
        errors(&format!("{PLACED}    .h ~ .u by name\n")),
        [
            "these members have no member of the same name on the other side: `.h.gnd`, `.u.vdd`; give each a partner with a rename, `by name { a = b }`, or leave it out with `except { a }`"
        ]
    );
}

#[test]
fn a_member_with_no_partner_gets_a_near_name() {
    let placed = "    .h: Silkscreen\n    .u: Chip\n    .supply ~ .u.vdd\n";
    assert_eq!(
        errors(&format!("{placed}    .h ~ .u by name {{ gpio4 = io4 }} except {{ vdd }}\n")),
        [
            "these members have no member of the same name on the other side: `.h.gpio5` (did you mean `gpio5 = io5`?), `.u.io5`; give each a partner with a rename, `by name { a = b }`, or leave it out with `except { a }`"
        ]
    );
}

#[test]
fn a_rail_against_a_pin_is_refused() {
    let placed = "    .r: Right\n    .p: PinSupply\n    .supply ~ .r.vdd\n";
    assert_eq!(
        errors(&format!("{placed}    .p ~ .r by name\n")),
        [
            "`.p.vdd` is a pin and `.r.vdd` is a `Rail`; `by name` joins a pin to a pin and an interface to an interface of the same kind, so rename one or leave it out with `except`"
        ]
    );
}

#[test]
fn a_rename_to_a_member_that_does_not_exist_is_refused() {
    let placed = "    .h: Silkscreen\n    .u: Chip\n    .supply ~ .u.vdd\n";
    assert_eq!(
        errors(&format!(
            "{placed}    .h ~ .u by name {{ gpio4 = io44; gpio5 = io5 }} except {{ vdd }}\n"
        )),
        [
            "the rename's right side is a member of `.u`, and `io44` is not a pin or an interface of it; did you mean `io4`?"
        ]
    );
    assert_eq!(
        errors(&format!(
            "{placed}    .h ~ .u by name {{ gpio4 = io4; gpio5 = designator }} except {{ vdd }}\n"
        )),
        [
            "the rename's right side is a member of `.u`, and `designator` is not a pin or an interface, and `by name` joins only those"
        ]
    );
}

#[test]
fn an_except_of_a_member_that_does_not_exist_is_refused() {
    assert_eq!(
        errors(&format!("{PLACED}    .h ~ .u by name except {{ gnd vd }}\n")),
        [
            "`except` leaves out `vd`, but neither `.h` nor `.u` has a pin or an interface of that name; did you mean `vdd`?"
        ]
    );
}

#[test]
fn a_member_has_one_partner() {
    let placed = "    .h: Silkscreen\n    .u: Chip\n    .supply ~ .u.vdd\n";
    assert_eq!(
        errors(&format!(
            "{placed}    .h ~ .u by name {{ gpio4 = io4; gpio5 = io4 }} except {{ vdd io5 }}\n"
        )),
        ["`io4` of `.u` is the partner of two members; a member has one partner"]
    );
    assert_eq!(
        errors(&format!(
            "{placed}    .h ~ .u by name {{ gpio4 = io4; en = en; gpio5 = io5 }} except {{ vdd }}\n"
        )),
        [
            "`en = en` renames nothing: members with the same name are joined without it; leave it out"
        ]
    );
    assert_eq!(
        errors(&format!(
            "{placed}    .h ~ .u by name {{ gpio4 = io4; gpio5 = io5 }} except {{ vdd io5 }}\n"
        )),
        ["`io5` is both renamed and left out with `except`; keep one"]
    );
}

#[test]
fn two_interfaces_of_one_kind_are_joined_with_plain_tilde() {
    let placed = "    .l: Left\n    .r: Right\n    .supply ~ .r.vdd\n";
    assert_eq!(
        errors(&format!("{placed}    .l.vdd ~ .r.vdd by name\n")),
        [
            "`.l.vdd` and `.r.vdd` are both `Rail`, and `~` already joins two interfaces of the same kind member by member; write `.l.vdd ~ .r.vdd`"
        ]
    );
}

#[test]
fn a_port_is_a_part_a_module_or_an_interface() {
    assert_eq!(
        errors(&format!("{PLACED}    .h.io4 ~ .u by name\n")),
        [
            "`.h.io4` is a pin; `by name` joins the members of two parts, modules or interfaces"
        ]
    );
}

#[test]
fn a_module_shows_only_its_interface() {
    // `.u` inside `Right` is a part, not a member `by name` joins, so nothing asks for a
    // partner for it.
    let placed = "    .r: Right\n    .p: PinSupply\n    .supply ~ .r.vdd\n    .p.vdd ~ .supply.pos\n";
    same_nets(
        &format!("{placed}    .p ~ .r by name except {{ vdd }}\n"),
        &format!("{placed}    .p.io4 ~ .r.io4\n"),
    );
}
