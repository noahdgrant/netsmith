//! `std::bus::PullUp`: a resistor from a rail to one line that states what it draws, so the net
//! it is on has every current it needs. The numbers and the part are invented.

use netsmith_check::{Domain, Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Catalogs, Solution, solve};

/// Check and solve the only board of `src` with the JLCPCB catalogs.
fn run(src: &str) -> Solution {
    let checked = check("test.nsm", src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let board = checked.standalone_boards().next().expect("one board");
    solve(board, &Catalogs::jlcpcb())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> Quantity {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) => *q,
        other => panic!("`{path}` is {other:?}"),
    }
}

fn picked(s: &Solution, path: &str) -> String {
    s.picks
        .iter()
        .find(|p| s.model.path_of(p.part) == path)
        .unwrap_or_else(|| panic!("`{path}` was not picked"))
        .label
        .clone()
}

/// The relations the solver could not evaluate, as text.
fn not_evaluated(s: &Solution) -> Vec<String> {
    s.deferred
        .iter()
        .map(|&i| {
            let r = &s.model.relations()[i];
            format!(
                "{} {}",
                s.model.show_expr(&r.lhs),
                s.model.show_expr(&r.rhs)
            )
        })
        .collect()
}

/// An invented gate with a supply that draws 100 uA to 1 mA and two lines, `a` and `b`, that it may
/// drive (pins the DC analysis does not model: an undriven input on a net behind a regulator draws
/// nothing, which the analysis solves, and a pull-up on it would then draw nothing).
const GATE: &str = "\
import std::bus::PullUp
import std::parts::passive::Resistor
import std::parts::regulator::LinearRegulator
import std::sources::Rail

part Gate {
    .designator &= \"U\"
    .vcc: Rail
    .vcc.current &= 100uA..1mA
    pins {
        .vcc.pos power \"VCC\" \"1\"
        .vcc.neg power \"GND\" \"2\"
        .a bidirectional \"A\" \"3\"
        .b bidirectional \"B\" \"4\"
    }
}
";

/// A 5 V input, a regulator that makes the 3.3 V net, a load of 0 to 20 mA on that net, the gate
/// on it, and `circuit` for the lines `a` and `b` of the gate. `load` is the `.load` rail's own
/// statement of its current.
fn board(circuit: &str) -> String {
    format!(
        "{GATE}\nboard Lines {{\n    .input: Rail {{\n        .voltage &= 5V +/- 5%\n        \
         .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }}\n    \
         .ldo: LinearRegulator {{ .vout.voltage &= 3.3V +/- 5% }}\n    \
         .load: Rail {{\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n        \
         .current &= 0mA..20mA\n    }}\n    .input ~ .ldo.vin\n    .ldo.vout ~ .load\n    \
         .gate: Gate\n    .gate.vcc ~ .load\n{circuit}}}\n"
    )
}

const PLAIN: &str = "    .pull: Resistor { .resistance &= 10kohm +/- 5% }\n    \
                     .load.pos ~> .pull ~> .gate.a\n    .gate.b ~ .load.pos\n";

fn pullup(extra: &str) -> String {
    board(&format!(
        "    .pull: PullUp {{ .resistor.resistance &= 10kohm +/- 5%\n {extra} }}\n    \
         .pull.supply ~ .load\n    .pull.line ~ .gate.a\n    .gate.b ~ .load.pos\n"
    ))
}

#[test]
fn a_plain_resistor_leaves_the_net_unsolved() {
    // The fault `PullUp` fixes: the resistor is the start of a chain on the 3.3 V net and
    // nothing says what it carries, so the net has two unknowns and the checks on it (the
    // regulator's current and its power) wait. The resistor's own power is made all the same:
    // Ohm's law over the net's range bounds its current (the cheap bound, pass-only), though
    // its current stays unknown and the regulator's checks, which need the real one, wait.
    let s = run(&board(PLAIN));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let open = not_evaluated(&s);
    assert!(
        !open.iter().any(|r| r.contains("pull.")),
        "the resistor's checks are proven by the bound: {open:#?}"
    );
    assert!(
        open.iter().any(|r| r.contains("ldo.vout.current")),
        "the regulator's checks were evaluated: {open:#?}"
    );
}

#[test]
fn a_pullup_states_what_it_draws_so_the_net_is_solved_and_the_checks_run() {
    let s = run(&pullup(""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let open = not_evaluated(&s);
    assert!(open.is_empty(), "not evaluated: {open:#?}");
    // The regulator supplies the load, the gate and the pull-up: all of it at most, the
    // gate's 100 uA at least.
    let supply = range(&s, "pull.supply.current");
    let ldo = range(&s, "ldo.vout.current");
    let all = 20e-3 + 1e-3 + supply.hi();
    assert!(
        ldo.lo() <= -all + 1e-12 && ldo.lo() > -all - 1e-9 && (ldo.hi() + 100e-6).abs() < 1e-12,
        "{ldo} against the draws {all}"
    );
}

#[test]
fn the_draw_is_zero_up_to_the_supply_over_the_resistance() {
    let s = run(&pullup(""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    // 3.3 V + 5 % over the resistor, at the bottom of its tolerance.
    let hi = 3.3 * 1.05 / range(&s, "pull.resistor.resistance").lo();
    let i = range(&s, "pull.resistor.current");
    assert!(
        i.lo().abs() < 1e-12 && (i.hi() - hi).abs() < 1e-6 * hi,
        "{i} against 0 to {hi}"
    );
    let supply = range(&s, "pull.supply.current");
    assert!(
        supply.lo().abs() < 1e-12 && (supply.hi() - hi).abs() < 1e-6 * hi,
        "{supply} against 0 to {hi}"
    );
}

#[test]
fn a_board_that_knows_how_long_its_line_is_low_narrows_the_draw() {
    let s = run(&pullup(".line_low &= 0..0.1"));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let whole = 3.3 * 1.05 / range(&s, "pull.resistor.resistance").lo();
    let supply = range(&s, "pull.supply.current");
    assert!((supply.hi() - 0.1 * whole).abs() < 1e-6 * whole, "{supply}");
}

#[test]
fn a_statement_on_the_resistance_picks_the_resistor() {
    let s = run(&pullup(""));
    assert_eq!(picked(&s, "pull.resistor"), "10k");
    let r = range(&s, "pull.resistor.resistance");
    assert!(r.lo() >= 9.5e3 - 1e-6 && r.hi() <= 10.5e3 + 1e-6, "{r}");
    // A different statement picks a different resistor.
    let s = run(&board(
        "    .pull: PullUp { .resistor.resistance &= 2.2kohm +/- 1% }\n    \
         .pull.supply ~ .load\n    .pull.line ~ .gate.a\n    .gate.b ~ .load.pos\n",
    ));
    assert_eq!(picked(&s, "pull.resistor"), "2.2k");
}

#[test]
fn a_budget_on_the_supply_current_is_kept() {
    // 3.465 V over 9.9 kohm is 0.35 mA at most: 1 mA holds it.
    let s = run(&pullup(".supply.current &= 0mA..1mA"));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:?}", not_evaluated(&s));
    // 1 mA to 2 mA cannot be drawn by a 10 kohm pull-up: 0.35 mA is the most it can carry.
    let s = run(&pullup(".supply.current &= 1mA..2mA"));
    let m = messages(&s);
    assert!(
        m.iter().any(|m| m.contains("pull.supply.current")),
        "no message naming the pull-up's supply current: {m:#?}"
    );
}

#[test]
fn two_pullups_on_one_rail_add() {
    let s = run(&board(
        "    .one: PullUp { .resistor.resistance &= 10kohm +/- 5% }\n    \
         .two: PullUp { .resistor.resistance &= 4.7kohm +/- 5% }\n    \
         .one.supply ~ .load\n    .two.supply ~ .load\n    \
         .one.line ~ .gate.a\n    .two.line ~ .gate.b\n",
    ));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:?}", not_evaluated(&s));
    let (one, two) = (
        range(&s, "one.supply.current"),
        range(&s, "two.supply.current"),
    );
    assert!(one.hi() > 0.3e-3 && one.hi() < 0.4e-3, "{one}");
    assert!(two.hi() > 0.7e-3 && two.hi() < 0.8e-3, "{two}");
    // The regulator gives the load's 20 mA, the gate's 1 mA and the sum of both pull-ups.
    let ldo = range(&s, "ldo.vout.current");
    let all = 20e-3 + 1e-3 + one.hi() + two.hi();
    assert!(
        ldo.lo() <= -all + 1e-12 && ldo.lo() > -all - 1e-9,
        "{ldo} against the draws {all}"
    );
}

#[test]
fn a_pullup_is_a_pull_up_for_the_i2c_rule() {
    // The checker refuses an I2C line with no resistor to a rail's positive pin; the resistor
    // inside a `PullUp` is one.
    let src = "\
import std::bus::{I2c, PullUp}
import std::sources::Rail

part Node {
    .designator &= \"U\"
    .vdd: Rail
    .i2c: I2c
    pins {
        .vdd.pos power \"VDD\" \"1\"
        .vdd.neg power \"GND\" \"2\"
        .i2c.sda bidirectional \"SDA\" \"3\"
        .i2c.scl bidirectional \"SCL\" \"4\"
    }
}

board Bus {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
    }
    .a: Node
    .b: Node
    .supply ~ .a.vdd
    .supply ~ .b.vdd
    .a.i2c ~ .b.i2c
    .sda: PullUp { .resistor.resistance &= 4.7kohm +/- 5% }
    .scl: PullUp { .resistor.resistance &= 4.7kohm +/- 5% }
    .sda.supply ~ .supply
    .scl.supply ~ .supply
    .sda.line ~ .a.i2c.sda
    .scl.line ~ .a.i2c.scl
}
";
    let checked = check("test.nsm", src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
}

#[test]
fn a_pullup_whose_resistance_nobody_states_is_a_finding_that_names_the_resistor() {
    // A board that forgets the value builds, and the solver takes the first row of the
    // resistor catalog. The unconstrained pick finding says so, and names the resistor (ADR 0065).
    let s = run(&board(
        "    .pull: PullUp\n    .pull.supply ~ .load\n    .pull.line ~ .gate.a\n    \
         .gate.b ~ .load.pos\n",
    ));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(picked(&s, "pull.resistor"), "330");
    // The resistor is a part inside the module and nothing holds its resistance to a member of
    // the module, so the finding names the resistor and the board states
    // `.pull.resistor.resistance` (ADR 0116).
    let found: Vec<(String, bool)> = s
        .unconstrained
        .iter()
        .map(|u| (s.model.path_of(u.part), u.held_by.is_none()))
        .collect();
    assert_eq!(found, [("pull.resistor".to_string(), true)]);
    // Stated, the finding is gone.
    let s = run(&pullup(""));
    assert!(s.unconstrained.is_empty(), "{:?}", s.unconstrained);
}

/// A module around a `PullUp` that forwards the resistance through a member of its own, on the
/// board of `board`.
fn wrapped(board_statement: &str) -> String {
    format!(
        "{GATE}
module Reset {{
    .supply: Rail
    .line: Pin
    .resistance: Resistance
    .pull: PullUp
    .pull.resistor.resistance &= .resistance
    .supply ~ .pull.supply
    .line ~ .pull.line
}}

board Wrapped {{
    .supply: Rail {{
        .voltage &= 3.3V +/- 5%
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
    }}
    .gate: Gate
    .gate.vcc ~ .supply
    .reset: Reset
    .reset.supply ~ .supply
    .reset.line ~ .gate.a
    .gate.b ~ .supply.pos
{board_statement}}}
"
    )
}

#[test]
fn a_module_forwards_the_resistance_of_its_pullup_through_its_own_member() {
    let s = run(&wrapped("    .reset.resistance &= 10kohm +/- 5%\n"));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(picked(&s, "reset.pull.resistor"), "10k");
    assert!(s.unconstrained.is_empty(), "{:?}", s.unconstrained);
    // A board that forgets is told about the member it can state, and only that one: the
    // wrapper's, not the `PullUp`'s inside it.
    let s = run(&wrapped(""));
    let found: Vec<String> = s
        .unconstrained
        .iter()
        .map(|u| {
            s.model
                .path_of_field(u.held_by.as_ref().expect("held to a member").member)
        })
        .collect();
    assert_eq!(found, ["reset.resistance"]);
}

#[test]
fn the_defaults_state_a_line_that_stays_under_the_supply_at_the_steady_voltage() {
    // Nothing stated: the draw is the one the module always had, and the two offers are what
    // they say they are.
    let s = run(&pullup(""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:?}", not_evaluated(&s));
    assert_eq!(range(&s, "pull.overdrive").lo(), 0.0);
    assert_eq!(range(&s, "pull.overdrive").hi(), 0.0);
    assert_eq!(
        range(&s, "pull.supply_top"),
        range(&s, "pull.supply.voltage")
    );
    let draw = range(&s, "pull.resistor.current");
    assert!(
        draw.lo() == 0.0 && (draw.hi() - 350e-6).abs() < 1e-6,
        "{draw}"
    );
}

#[test]
fn a_line_a_driver_holds_above_the_supply_gives_a_negative_draw() {
    // A 5 V driver on a line pulled to 3.3 V: up to 1.7 V over the supply pushes current back
    // into the rail, 1.7 V over the lowest the resistor can be.
    let s = run(&pullup("        .overdrive &= 0V..1.7V\n"));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let draw = range(&s, "pull.resistor.current");
    let r_lo = range(&s, "pull.resistor.resistance").lo();
    assert!(
        (draw.lo() + 1.7 / r_lo).abs() < 1e-9 && (draw.hi() - 350e-6).abs() < 1e-6,
        "{draw} at {r_lo}"
    );
    // What the regulator behind the rail supplies is then not always a supply: it may have to
    // take the current back, so its upper end moves above where the gate alone puts it.
    let ldo = range(&s, "ldo.vout.current");
    assert!(ldo.hi() > -100e-6, "{ldo}");
    // Without the statement the draw never goes below nothing.
    let plain = run(&pullup(""));
    assert!(range(&plain, "pull.resistor.current").lo() >= 0.0);
}

#[test]
fn a_board_that_states_its_rails_peak_can_have_the_draw_worked_out_at_it() {
    let extra = "        .supply_top &= .supply.peak\n";
    let board = board(&format!(
        "    .pull: PullUp {{ .resistor.resistance &= 10kohm +/- 5%\n{extra} }}\n    \
         .pull.supply ~ .load\n    .pull.line ~ .gate.a\n    .gate.b ~ .load.pos\n    \
         .load.peak &= 3.465V..6V\n"
    ));
    let s = run(&board);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let draw = range(&s, "pull.resistor.current");
    let r_lo = range(&s, "pull.resistor.resistance").lo();
    assert!((draw.hi() - 6.0 / r_lo).abs() < 1e-9, "{draw} at {r_lo}");
    // And the steady draw is what it was when the board states no peak.
    let plain = run(&pullup(""));
    assert!(range(&plain, "pull.resistor.current").hi() < draw.hi());
}

// ---- the package of the resistor inside (owner decision of 2026-11-10, item PK1) ----

fn package_of(s: &Solution, path: &str) -> String {
    match s.model.domain_at(path) {
        Some(Domain::Text(t)) => t.clone(),
        other => panic!("`{path}` is {other:?}"),
    }
}

#[test]
fn a_board_states_the_package_of_the_resistor_inside_a_pullup() {
    let s = run(&pullup(".resistor.package &= \"0402\""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(package_of(&s, "pull.resistor.package"), "0402");
    assert_eq!(picked(&s, "pull.resistor"), "10k");
}

#[test]
fn a_pullup_whose_package_nobody_states_is_picked_as_before() {
    let s = run(&pullup(""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    // The solver chooses, and does not state it: no `.resistor.package` was written by the board.
    assert_eq!(package_of(&s, "pull.resistor.package"), "0603");
    assert!(
        !not_evaluated(&s).iter().any(|r| r.contains("package")),
        "an unstated package is no open relation: {:?}",
        not_evaluated(&s)
    );
}

#[test]
fn a_package_that_no_row_has_is_refused_with_its_name() {
    let s = run(&pullup(".resistor.package &= \"0201\""));
    let all = messages(&s).join("\n");
    // Refused at the statement before any row is tried, with the nearest package (ADR 0118).
    assert!(
        all.contains("`pull.resistor.package` is \"0201\", and no row of the `resistor` catalog is in that package; its packages are ")
            && all.ends_with("; did you mean \"0402\"?"),
        "the message names the field, the package and the nearest: {all}"
    );
}
