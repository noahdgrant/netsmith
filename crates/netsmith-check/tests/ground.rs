//! A rail is on `GND` unless a statement says otherwise: `Rail.neg.net ?= "GND"` in std, which
//! takes effect where a rail is on a board (ADR 0118).

use netsmith_check::*;

const IMPORTS: &str = "\
import std::sources::Rail
import std::parts::passive::Resistor
";

fn run(body: &str) -> Checked {
    check("test.nsm", &format!("{IMPORTS}\n{body}"), &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// The names of a board's nets, sorted, with whether each was named by a statement.
fn nets(c: &Checked, board: &str) -> Vec<(String, bool)> {
    let m = &c.board(board).unwrap().model;
    let mut nets: Vec<(String, bool)> = m
        .nets()
        .iter()
        .map(|n| (n.name.clone(), n.explicit))
        .collect();
    nets.sort();
    nets
}

fn net_names(c: &Checked, board: &str) -> Vec<String> {
    nets(c, board).into_iter().map(|(n, _)| n).collect()
}

#[test]
fn a_rail_with_no_ground_statement_is_on_gnd() {
    let c = run("board B {\n    .supply: Rail\n    .r: Resistor\n    .supply.pos ~> .r ~> .supply.neg\n}\n");
    assert!(c.is_ok(), "{}", c.render());
    let nets = nets(&c, "B");
    assert!(nets.contains(&("GND".to_string(), true)), "{nets:?}");
}

#[test]
fn two_rails_with_no_ground_statement_share_gnd() {
    let c = run(
        "board B {\n    .a: Rail\n    .b: Rail\n    .r1: Resistor\n    .r2: Resistor\n    .a.pos ~> .r1 ~> .a.neg\n    .b.pos ~> .r2 ~> .b.neg\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    let gnd: Vec<_> = m.nets().iter().filter(|n| n.name == "GND").collect();
    assert_eq!(gnd.len(), 1, "one ground net: {:?}", net_names(&c, "B"));
    assert_eq!(gnd[0].terminals.len(), 2);
}

#[test]
fn a_rail_states_its_own_ground_and_it_is_a_second_net() {
    let c = run(
        "board B {\n    .a: Rail\n    .analog: Rail { .neg.net &= \"AGND\" }\n    .r1: Resistor\n    .r2: Resistor\n    .a.pos ~> .r1 ~> .a.neg\n    .analog.pos ~> .r2 ~> .analog.neg\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let names = net_names(&c, "B");
    assert!(
        names.contains(&"GND".to_string()) && names.contains(&"AGND".to_string()),
        "{names:?}"
    );
}

#[test]
fn a_statement_of_gnd_still_means_gnd() {
    let c = run("board B {\n    .a: Rail { .neg.net &= \"GND\" }\n    .b: Rail\n}\n");
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(net_names(&c, "B").iter().filter(|n| *n == "GND").count(), 1);
}

#[test]
fn a_module_rail_with_no_statement_is_on_the_boards_gnd() {
    let c = run(
        "module M {\n    .out: Rail\n    .r: Resistor\n    .out.pos ~> .r ~> .out.neg\n}\n\
         board B {\n    .m: M\n    .supply: Rail\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    let gnd: Vec<_> = m.nets().iter().filter(|n| n.name == "GND").collect();
    assert_eq!(gnd.len(), 1, "{:?}", net_names(&c, "B"));
    // The module's resistor and the board's rail are on the one net.
    assert!(!gnd[0].terminals.is_empty());
}

#[test]
fn a_module_rail_joined_to_a_rail_that_states_agnd_keeps_the_join() {
    let c = run(
        "module M {\n    .out: Rail\n    .r: Resistor\n    .out.pos ~> .r ~> .out.neg\n}\n\
         board B {\n    .m: M\n    .analog: Rail { .neg.net &= \"AGND\" }\n    .m.out ~ .analog\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let names = net_names(&c, "B");
    assert!(names.contains(&"AGND".to_string()), "{names:?}");
    assert!(
        !names.contains(&"GND".to_string()),
        "the joined ground is one net, and it is AGND: {names:?}"
    );
}

#[test]
fn a_default_gives_way_through_a_chain_of_joins() {
    // `a` joins `b` joins `c`, and only `c` states a name: all three are on it.
    let c = run(
        "board B {\n    .a: Rail\n    .b: Rail\n    .c: Rail { .neg.net &= \"AGND\" }\n    .r: Resistor\n    .a.pos ~> .r ~> .a.neg\n    .a.neg ~ .b.neg\n    .b.neg ~ .c.neg\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let names = net_names(&c, "B");
    assert!(names.contains(&"AGND".to_string()), "{names:?}");
    assert!(!names.contains(&"GND".to_string()), "{names:?}");
}

#[test]
fn joining_a_default_rail_to_gnd_by_a_pin_that_states_another_name_follows_the_pin() {
    let c = run(
        "board B {\n    .a: Rail\n    .r: Resistor\n    .r.b.net &= \"RET\"\n    .a.neg ~ .r.b\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let names = net_names(&c, "B");
    assert!(names.contains(&"RET".to_string()), "{names:?}");
    assert!(!names.contains(&"GND".to_string()), "{names:?}");
}

#[test]
fn two_names_stated_on_joined_rails_are_still_a_contradiction() {
    let c = run(
        "board B {\n    .a: Rail { .neg.net &= \"GND\" }\n    .b: Rail { .neg.net &= \"AGND\" }\n    .a.neg ~ .b.neg\n}\n",
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("connected but name their net differently"),
        "{}",
        m[0]
    );
}

#[test]
fn a_default_and_a_statement_on_the_same_rail_end_up_in_agreement_with_the_statement() {
    // The default is dropped for a rail that states its ground, with no overlap needed.
    let c = run("board B {\n    .a: Rail { .neg.net &= \"AGND\" }\n}\n");
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(net_names(&c, "B"), ["AGND"]);
}

#[test]
fn two_boards_of_a_system_each_have_their_own_gnd() {
    let c = run(
        "board P {\n    .out: Rail\n    .r: Resistor\n    .out.pos ~> .r ~> .out.neg\n}\nboard Q {\n    .input: Rail\n    .r: Resistor\n    .input.pos ~> .r ~> .input.neg\n}\n\
         system S {\n    .p: P\n    .q: Q\n    .p.out ~ .q.input\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.system("S").unwrap().model;
    let gnd: Vec<_> = m.nets().iter().filter(|n| n.name == "GND").collect();
    assert_eq!(gnd.len(), 2, "one `GND` for each board");
    for net in &gnd {
        let boards: std::collections::HashSet<_> =
            net.pins.iter().map(|&p| m.board_of(p)).collect();
        assert_eq!(boards.len(), 1, "a ground spans boards");
    }
}

#[test]
fn two_boards_of_a_system_may_differ_in_ground() {
    let c = run(
        "board P {\n    .out: Rail { .neg.net &= \"AGND\" }\n    .r: Resistor\n    .out.pos ~> .r ~> .out.neg\n}\nboard Q {\n    .input: Rail\n    .r: Resistor\n    .input.pos ~> .r ~> .input.neg\n}\n\
         system S {\n    .p: P\n    .q: Q\n    .p.out ~ .q.input\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let names: Vec<String> = c
        .system("S")
        .unwrap()
        .model
        .nets()
        .iter()
        .map(|n| n.name.clone())
        .collect();
    assert!(names.contains(&"AGND".to_string()), "{names:?}");
    assert!(names.contains(&"GND".to_string()), "{names:?}");
}

#[test]
fn a_module_checked_on_its_own_names_no_net() {
    // No board is above the rail, so the default has nothing to name a net on.
    let c = run("module Solo {\n    .out: Rail\n    .r: Resistor\n    .out.pos ~> .r ~> .out.neg\n}\n");
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_rail_with_no_part_on_it_names_no_net() {
    // The ground of a rail nothing is connected to is no net: a bus port's reference rail, for
    // one, is on no net until a part joins it, and must not look connected to ground.
    let c = run("board B {\n    .a: Rail\n}\n");
    assert!(c.is_ok(), "{}", c.render());
    assert!(net_names(&c, "B").is_empty(), "{:?}", net_names(&c, "B"));
    let c = run("import std::logic::Logic\nboard B {\n    .x: Logic\n    .y: Logic\n    .x.line ~ .y.line\n}\n");
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(net_names(&c, "B"), ["net-x.line"]);
}

// ---- who may offer a net name --------------------------------------------------

#[test]
fn a_module_cannot_offer_a_net_name() {
    let c = run("module M {\n    .out: Rail\n    .out.pos.net ?= \"VCC\"\n}\n");
    let m = messages(&c);
    assert!(!m.is_empty(), "{m:#?}");
    assert!(
        m.iter()
            .all(|m| m.contains("`out.pos.net` names a net by default, and only a board names nets")),
        "{m:#?}"
    );
}

#[test]
fn a_part_cannot_offer_a_net_name() {
    let c = run("part P {\n    .a: Pin\n    .a.net ?= \"X\"\n}\n");
    let m = messages(&c);
    assert!(!m.is_empty(), "{m:#?}");
    assert!(
        m.iter()
            .all(|m| m.contains("`a.net` names a net by default, and only a board names nets")),
        "{m:#?}"
    );
}

#[test]
fn a_system_cannot_offer_a_net_name() {
    let c = run(
        "board P {\n    .out: Rail\n}\nsystem S {\n    .p: P\n    .p.out.pos.net ?= \"X\"\n}\n",
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(m[0].contains("a system does not"), "{}", m[0]);
}

#[test]
fn a_board_may_offer_a_net_name_and_replace_it() {
    let c = run(
        "board B {\n    .a: Rail\n    .r: Resistor\n    .a.pos ~> .r ~> .a.neg\n    .a.pos.net ?= \"VCC\"\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    assert!(net_names(&c, "B").contains(&"VCC".to_string()));
    let c = run(
        "board B {\n    .a: Rail\n    .r: Resistor\n    .a.pos ~> .r ~> .a.neg\n    .a.neg.net ?= \"RET\"\n}\n",
    );
    // Two defaults for one field: the rail's own and the board's.
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(m[0].contains("has 2 defaults"), "{}", m[0]);
}

#[test]
fn an_interface_may_offer_a_net_name_for_its_pins() {
    let c = run(
        "interface Clk {\n    .line: Pin\n    .line.net ?= \"CLK\"\n}\nboard B {\n    .c: Clk\n    .r: Resistor\n    .c.line ~ .r.a\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    assert!(net_names(&c, "B").contains(&"CLK".to_string()));
}
