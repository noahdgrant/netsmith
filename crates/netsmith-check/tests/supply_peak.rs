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

// ---- ADR 0091: the reverse peak and the fault current ----

const DIODES: &str = "import std::parts::diode::{Schottky, Tvs}\nimport std::parts::passive::Fuse\nimport std::sources::Rail\n";

/// Every relation that a chain gave a part, as `field = value`.
fn gave(src: &str, board: &str, field: &str) -> Vec<String> {
    let c = check("t.nsm", &format!("{DIODES}{src}"), &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board(board).unwrap().model;
    m.relations()
        .iter()
        .map(|r| format!("{} = {}", m.show_expr(&r.lhs), m.show_expr(&r.rhs)))
        .filter(|s| !s.starts_with('-') && s.contains(field) && s.contains(" = "))
        .filter(|s| !s.contains("max_"))
        .collect()
}

#[test]
fn the_first_part_with_a_reverse_peak_takes_the_rails_and_the_ones_behind_it_nothing() {
    let g = gave(
        "board B {\n    .input: Rail\n    .load: Rail\n    .fuse: Fuse\n    .d: Schottky\n    .t: Tvs\n    .input.pos ~> .fuse ~> .d ~> .t ~> .load.pos\n    .input.reverse_peak &= 100V\n}\n",
        "B",
        "supply_reverse_peak",
    );
    assert_eq!(g, ["d.supply_reverse_peak = input.reverse_peak"]);
}

#[test]
fn a_tvs_diode_first_in_its_chain_meets_the_reverse_peak() {
    let g = gave(
        "board B {\n    .input: Rail\n    .t: Tvs\n    .input.pos ~> .t ~> .input.neg\n    .input.reverse_peak &= 100V\n}\n",
        "B",
        "supply_reverse_peak",
    );
    assert_eq!(g, ["t.supply_reverse_peak = input.reverse_peak"]);
}

#[test]
fn a_part_behind_a_fuse_takes_the_fuses_limit_current() {
    let g = gave(
        "board B {\n    .input: Rail\n    .load: Rail\n    .fuse: Fuse\n    .d: Schottky\n    .input.pos ~> .fuse ~> .d ~> .load.pos\n}\n",
        "B",
        "supply_fault_current",
    );
    assert_eq!(g, ["d.supply_fault_current = fuse.limit_current"]);
}

#[test]
fn the_nearest_fuse_in_front_gives_the_limit() {
    let g = gave(
        "board B {\n    .input: Rail\n    .load: Rail\n    .f1: Fuse\n    .f2: Fuse\n    .d: Schottky\n    .input.pos ~> .f1 ~> .f2 ~> .d ~> .load.pos\n}\n",
        "B",
        "supply_fault_current",
    );
    assert_eq!(g, ["d.supply_fault_current = f2.limit_current"]);
}

#[test]
fn a_fuse_behind_the_diode_gives_it_nothing_and_a_chain_from_a_signal_still_has_a_limit() {
    let g = gave(
        "board B {\n    .input: Rail\n    .load: Rail\n    .fuse: Fuse\n    .d: Schottky\n    .input.pos ~> .d ~> .fuse ~> .load.pos\n}\n",
        "B",
        "supply_fault_current",
    );
    assert!(g.is_empty(), "{g:?}");
    // The fault current needs no rail at the start of the chain.
    let g = gave(
        "board B {\n    .load: Rail\n    .r: Schottky\n    .fuse: Fuse\n    .d: Schottky\n    .r.cathode ~> .fuse ~> .d ~> .load.pos\n}\n",
        "B",
        "supply_fault_current",
    );
    assert_eq!(g, ["d.supply_fault_current = fuse.limit_current"]);
}

#[test]
fn a_part_in_no_chain_gets_neither() {
    let g = gave(
        "board B {\n    .input: Rail\n    .d: Schottky\n    .input.pos ~ .d.anode\n}\n",
        "B",
        "supply_",
    );
    assert!(g.is_empty(), "{g:?}");
}
