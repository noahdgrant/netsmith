//! A part in a `~>` chain from a rail has the rail's `.peak` as its `.supply_peak` (ADR 0086).

use netsmith_check::*;

const HEAD: &str = "import std::parts::passive::{Fuse, Resistor}\nimport std::sources::Rail\n";

/// Every relation that gives a supply peak, as `field = rail peak`.
fn peaks(src: &str, board: &str) -> Vec<String> {
    let c = check("t.nsm", &format!("{HEAD}{src}"), &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board(board).unwrap().model;
    m.relations()
        .iter()
        .map(|r| format!("{} = {}", m.show_expr(&r.lhs), m.show_expr(&r.rhs)))
        .filter(|s| s.contains("supply_peak") && s.contains(" = "))
        .filter(|s| s.contains(".peak"))
        .collect()
}

#[test]
fn a_fuse_in_a_chain_from_a_rail_takes_its_peak() {
    let p = peaks(
        "board B {\n    .input: Rail\n    .load: Rail\n    .fuse: Fuse\n    .input.pos ~> .fuse ~> .load.pos\n}\n",
        "B",
    );
    assert_eq!(p, ["fuse.supply_peak = input.peak"]);
}

#[test]
fn a_part_further_along_the_chain_gets_the_start_peak_too() {
    let p = peaks(
        "board B {\n    .input: Rail\n    .load: Rail\n    .r: Resistor\n    .fuse: Fuse\n    .input.pos ~> .r ~> .fuse ~> .load.pos\n}\n",
        "B",
    );
    assert_eq!(p, ["fuse.supply_peak = input.peak"]);
}

#[test]
fn the_direction_of_the_chain_picks_the_start_net() {
    // Written from the load, the chain starts on the load's net, and that is the rail whose
    // peak it carries (a rail on the net).
    let p = peaks(
        "board B {\n    .input: Rail\n    .load: Rail\n    .fuse: Fuse\n    .load.pos ~> .fuse ~> .input.pos\n}\n",
        "B",
    );
    assert_eq!(p, ["fuse.supply_peak = load.peak"]);
}

#[test]
fn rails_the_scope_declares_come_before_the_rails_of_what_it_holds() {
    let p = peaks(
        "module Stage {\n    .out: Rail\n}\nboard B {\n    .a_stage: Stage\n    .input: Rail\n    .fuse: Fuse\n    .a_stage.out ~ .input\n    .input.pos ~> .fuse ~> .a_stage.out.pos\n}\n",
        "B",
    );
    assert_eq!(p, ["fuse.supply_peak = input.peak"]);
}

#[test]
fn a_fuse_in_a_module_takes_the_peak_of_the_modules_port() {
    let p = peaks(
        "module Guard {\n    .vin: Rail\n    .vout: Rail\n    .fuse: Fuse\n    .vin.pos ~> .fuse ~> .vout.pos\n}\nboard B {\n    .input: Rail\n    .g: Guard\n    .input ~ .g.vin\n}\n",
        "B",
    );
    // The board's rail is out of the module's reach: its peak arrives at the port by the `~`.
    assert_eq!(p, ["g.fuse.supply_peak = g.vin.peak"]);
}

#[test]
fn a_chain_that_starts_on_no_rail_gives_nothing() {
    for body in [
        // Starts on a part's pin.
        "    .load: Rail\n    .r: Resistor\n    .fuse: Fuse\n    .r.b ~> .fuse ~> .load.pos\n",
        // Starts on a rail's negative pin.
        "    .input: Rail\n    .fuse: Fuse\n    .input.neg ~> .fuse ~> .r.a\n    .r: Resistor\n",
        // Not in a chain.
        "    .input: Rail\n    .fuse: Fuse\n    .input.pos ~ .fuse.a\n",
    ] {
        let p = peaks(&format!("board B {{\n{body}}}\n"), "B");
        assert!(p.is_empty(), "{body}\n{p:?}");
    }
}

#[test]
fn a_part_without_the_field_is_left_alone() {
    let p = peaks(
        "board B {\n    .input: Rail\n    .load: Rail\n    .r: Resistor\n    .input.pos ~> .r ~> .load.pos\n}\n",
        "B",
    );
    assert!(p.is_empty(), "{p:?}");
}
