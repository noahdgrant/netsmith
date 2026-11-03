//! A `system` of boards: what it may contain, what it may reach, and what it leaves alone.

use netsmith_check::*;

const IMPORTS: &str = "\
import std::parts::passive::{Resistor, Led}
import std::parts::connector::Header1x2
import std::sources::Rail
import std::logic::Logic
";

const BOARDS: &str = "\
module Stage {
    .power: Rail
    .r: Resistor
}

board Power {
    .output: Rail {
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
    }
    .signal: Logic
    .stage: Stage
    .r: Resistor
    .j: Header1x2
    .output.pos ~ .j.p1
    .output.neg ~ .j.p2
}

board Io {
    .input: Rail {
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }
    .signal: Logic
    .r: Resistor
    .input.pos ~> .r ~> .input.neg
}
";

fn run(system: &str) -> Checked {
    check(
        "test.nsm",
        &format!("{IMPORTS}\n{BOARDS}\n{system}"),
        &Loader::std(),
    )
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn one_error(system: &str, needle: &str) {
    let c = run(system);
    let msgs = messages(&c);
    assert_eq!(
        msgs.len(),
        1,
        "expected one error about `{needle}`: {msgs:#?}"
    );
    assert!(msgs[0].contains(needle), "`{}` lacks `{needle}`", msgs[0]);
}

#[test]
fn a_system_holds_boards_and_joins_their_ports() {
    let c = run("system S {\n    .main: Power\n    .io: Io\n    .main.output ~ .io.input\n}\n");
    assert!(c.is_ok(), "{}", c.render());
    let s = c.system("S").expect("the system is elaborated");
    let boards: Vec<(&str, &str)> = s
        .boards
        .iter()
        .map(|b| (b.name.as_str(), b.type_name.as_str()))
        .collect();
    assert_eq!(boards, [("main", "Power"), ("io", "Io")]);
    assert_eq!(s.model.joins().len(), 1);
}

#[test]
fn a_board_a_system_places_is_not_made_on_its_own() {
    let c = run("system S {\n    .main: Power\n    .io: Io\n}\n");
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(c.boards.len(), 2, "both are still checked");
    assert_eq!(c.standalone_boards().count(), 0);
    let alone = run("");
    assert_eq!(alone.standalone_boards().count(), 2);
}

#[test]
fn a_join_equates_the_ports_values_and_leaves_their_nets_apart() {
    let c = run("system S {\n    .main: Power\n    .io: Io\n    .main.output ~ .io.input\n}\n");
    let m = &c.system("S").unwrap().model;
    assert!(!m.equalities().is_empty(), "the voltages are one value");
    // Each board keeps its own nets: `GND` is on both boards and is two nets, and the
    // joined `3V3` and `VCC` are not one.
    let names: Vec<&str> = m.nets().iter().map(|n| n.name.as_str()).collect();
    assert_eq!(
        names.iter().filter(|n| **n == "GND").count(),
        2,
        "{names:?}"
    );
    for net in m.nets() {
        let boards: std::collections::HashSet<_> =
            net.pins.iter().map(|&p| m.board_of(p)).collect();
        assert_eq!(boards.len(), 1, "net {} spans boards", net.name);
    }
    assert!(
        names.contains(&"3V3") && names.contains(&"VCC"),
        "{names:?}"
    );
}

#[test]
fn a_board_seen_from_the_system_is_the_board_as_checked_alone() {
    let c = run("system S {\n    .main: Power\n    .io: Io\n    .main.output ~ .io.input\n}\n");
    let s = c.system("S").unwrap();
    let alone = &c.board("Power").unwrap().model;
    let scoped = s.model.scoped_to(s.boards[0].inst);
    let describe = |m: &Model| {
        let mut parts: Vec<String> = m
            .instances()
            .filter(|(_, i)| i.kind == InstKind::Part)
            .map(|(id, _)| m.path_of(id))
            .collect();
        parts.sort();
        let mut nets: Vec<(String, usize)> = m
            .nets()
            .iter()
            .map(|n| (n.name.clone(), n.terminals.len()))
            .collect();
        nets.sort();
        (parts, nets)
    };
    assert_eq!(describe(&scoped), describe(alone));
}

#[test]
fn only_the_ports_a_board_declares_directly_can_be_reached() {
    one_error(
        "system S {\n    .main: Power\n    .io: Io\n    .main.stage.power ~ .io.input\n}\n",
        "`stage` is inside board `Power`; a system can reach only the ports a board declares directly",
    );
    one_error(
        "system S {\n    .main: Power\n    .io: Io\n    .main.r.a ~ .io.input\n}\n",
        "`r` is inside board `Power`",
    );
    // Nor can a value be set on something inside.
    one_error(
        "system S {\n    .main: Power {\n        .r.resistance &= 1kohm\n    }\n}\n",
        "`r` is inside board `Power`",
    );
}

#[test]
fn a_system_narrows_a_board_only_through_its_ports() {
    let c = run(
        "system S {\n    .main: Power\n    .main.output.voltage &= 3.3V +/- 5%\n    .io: Io\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.system("S").unwrap().model;
    assert!(matches!(
        m.domain_at("main.output.voltage"),
        Some(Domain::Quantity(_))
    ));
}

#[test]
fn a_system_does_not_name_nets() {
    one_error(
        "system S {\n    .main: Power\n    .main.output.pos.net &= \"X\"\n}\n",
        "a system does not: each board names its own nets",
    );
}

#[test]
fn ports_of_different_kinds_cannot_be_joined() {
    one_error(
        "system S {\n    .main: Power\n    .io: Io\n    .main.output ~ .io.signal\n}\n",
        "cannot connect a `Rail` to a `Logic`",
    );
}

#[test]
fn only_ports_are_joined() {
    one_error(
        "system S {\n    .main: Power\n    .io: Io\n    .main.output.pos ~ .io.input.pos\n}\n",
        "a system joins two ports of the same kind; a pin is not a port",
    );
    one_error(
        "system S {\n    .main: Power\n    .io: Io\n    .main.output.pos ~> .io.input.pos\n}\n",
        "a system joins ports with `~`",
    );
    let c = run("system S {\n    .main: Power\n    .io: Io\n    .main ~ .io\n}\n");
    assert!(
        messages(&c)
            .iter()
            .all(|m| m.contains("connect a pin or an interface of this")),
        "{:?}",
        messages(&c)
    );
    assert!(!c.is_ok());
    one_error(
        "system S {\n    .main: Power\n    .main.output ~ .main.signal\n}\n",
        "are on the same board",
    );
}

#[test]
fn a_system_holds_nothing_but_boards() {
    one_error(
        "system S {\n    .main: Power\n    .r: Resistor\n}\n",
        "a system holds boards, but `Resistor` is a part; put it on a board",
    );
    one_error(
        "system S {\n    .main: Power\n    .v: Voltage\n}\n",
        "a system holds boards, but `Voltage` is not one",
    );
}

#[test]
fn a_board_is_placed_only_in_a_system() {
    one_error(
        "board Outer {\n    .inner: Io\n}\n",
        "a board cannot be placed inside a board; boards are placed only in a system",
    );
    one_error(
        "module M {\n    .inner: Io\n}\n",
        "a board cannot be placed inside a module",
    );
    one_error(
        "system S {\n    .main: Power\n}\nsystem T {\n    .s: S\n}\n",
        "a system holds boards, but `S` is a system",
    );
}

#[test]
fn a_system_may_place_the_same_board_twice() {
    let c = run(
        "system S {\n    .main: Power\n    .a: Io\n    .b: Io\n    .main.output ~ .a.input\n    .main.output ~ .b.input\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.system("S").unwrap().model;
    // One balance for the three ports on the join.
    let join = m
        .balances()
        .iter()
        .find(|b| m.instance(b.scope).kind == InstKind::System)
        .expect("a balance at the system");
    assert_eq!(join.draws.len(), 3);
    assert_eq!(join.net, "a.input ~ b.input ~ main.output");
}

#[test]
fn the_order_of_a_systems_statements_does_not_change_it() {
    let one = run("system S {\n    .main: Power\n    .io: Io\n    .main.output ~ .io.input\n}\n");
    let other = run("system S {\n    .main.output ~ .io.input\n    .io: Io\n    .main: Power\n}\n");
    assert!(other.is_ok(), "{}", other.render());
    let key = |c: &Checked| {
        let m = &c.system("S").unwrap().model;
        let mut nets: Vec<String> = m.nets().iter().map(|n| n.name.clone()).collect();
        nets.sort();
        let balances: Vec<String> = m.balances().iter().map(|b| b.net.clone()).collect();
        (nets, balances)
    };
    assert_eq!(key(&one), key(&other));
}
