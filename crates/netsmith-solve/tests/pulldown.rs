//! `std::bus::PullDown`: a resistor from a line to a ground that states what it carries, so
//! the checks on the resistor run. The numbers and the part are invented.

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

/// An invented gate with a supply that draws 100 uA to 1 mA and two lines, `a` and `b`, that it
/// may drive (pins the DC analysis does not model, so what a pull-down on one carries stays the
/// pull-down's own statement).
const GATE: &str = "\
import std::bus::PullDown
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
/// on it, and `circuit` for the lines `a` and `b` of the gate.
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

fn pulldown(extra: &str) -> String {
    board(&format!(
        "    .pull: PullDown {{ .resistance &= 100kohm +/- 5%\n {extra} }}\n    \
         .pull.reference ~ .load\n    .pull.line ~ .gate.a\n    .gate.b ~ .load.pos\n"
    ))
}

const PLAIN: &str = "    .pull: Resistor { .resistance &= 100kohm +/- 5% }\n    \
                     .gate.a ~> .pull ~> .load.neg\n    .gate.b ~ .load.pos\n";

#[test]
fn a_plain_resistor_has_no_current_and_a_pulldown_has() {
    // The fault `PullDown` fixes: nothing says what a bare resistor to ground carries.
    let plain = run(&board(PLAIN));
    assert_eq!(
        plain.model.domain_at("pull.current"),
        Some(&Domain::Unset),
        "a bare resistor's current is not known"
    );
    let s = run(&pulldown(""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let i = range(&s, "pull.resistor.current");
    assert!(i.lo().abs() < 1e-12 && i.hi() > 0.0, "{i}");
}

#[test]
fn the_current_is_zero_up_to_the_line_over_the_resistance() {
    let s = run(&pulldown(""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:?}", not_evaluated(&s));
    // 3.3 V + 5 % over the resistor, at the bottom of its tolerance.
    let hi = 3.3 * 1.05 / range(&s, "pull.resistor.resistance").lo();
    let i = range(&s, "pull.resistor.current");
    assert!(
        i.lo().abs() < 1e-12 && (i.hi() - hi).abs() < 1e-6 * hi,
        "{i} against 0 to {hi}"
    );
}

#[test]
fn the_module_draws_nothing_from_its_reference_so_the_supply_net_stays_solved() {
    // The reference rail carries nothing: the regulator's current is the load's and the gate's,
    // exactly as it is without the pull-down, and nothing waits.
    let with = run(&pulldown(""));
    let without = run(&board("    .gate.b ~ .load.pos\n    .gate.a ~ .load.pos\n"));
    assert!(messages(&with).is_empty(), "{:?}", messages(&with));
    assert_eq!(
        range(&with, "pull.reference.current"),
        Quantity::point(netsmith_quantity::Dimension::CURRENT, 0.0).unwrap()
    );
    assert_eq!(
        range(&with, "ldo.vout.current"),
        range(&without, "ldo.vout.current")
    );
    assert!(
        not_evaluated(&with).is_empty(),
        "{:?}",
        not_evaluated(&with)
    );
}

#[test]
fn a_board_that_knows_how_long_its_line_is_high_narrows_the_current() {
    let s = run(&pulldown(".line_high &= 0..0.1"));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let whole = 3.3 * 1.05 / range(&s, "pull.resistor.resistance").lo();
    let i = range(&s, "pull.resistor.current");
    assert!((i.hi() - 0.1 * whole).abs() < 1e-6 * whole, "{i}");
}

#[test]
fn a_line_held_above_the_reference_carries_what_the_board_states() {
    // A 5.25 V driver on a line judged against 3.3 V: up to 5.25 V over the lowest resistor.
    let s = run(&pulldown("        .line_top &= 0V..5.25V\n"));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let r_lo = range(&s, "pull.resistor.resistance").lo();
    let i = range(&s, "pull.resistor.current");
    assert!((i.hi() - 5.25 / r_lo).abs() < 1e-9, "{i} at {r_lo}");
}

#[test]
fn a_statement_on_the_resistance_picks_the_resistor() {
    let s = run(&pulldown(""));
    assert_eq!(picked(&s, "pull.resistor"), "100k");
    let r = range(&s, "pull.resistance");
    let inside = range(&s, "pull.resistor.resistance");
    assert!(r.lo() >= 95e3 - 1e-6 && r.hi() <= 105e3 + 1e-6, "{r}");
    assert!(inside.lo() >= r.lo() && inside.hi() <= r.hi(), "{inside}");
}

#[test]
fn two_pulldowns_on_one_rail_are_each_their_own() {
    let s = run(&board(
        "    .one: PullDown { .resistance &= 100kohm +/- 5% }\n    \
         .two: PullDown { .resistance &= 10kohm +/- 5% }\n    \
         .one.reference ~ .load\n    .two.reference ~ .load\n    \
         .one.line ~ .gate.a\n    .two.line ~ .gate.b\n",
    ));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:?}", not_evaluated(&s));
    let (one, two) = (
        range(&s, "one.resistor.current"),
        range(&s, "two.resistor.current"),
    );
    assert!(one.hi() > 30e-6 && one.hi() < 40e-6, "{one}");
    assert!(two.hi() > 0.3e-3 && two.hi() < 0.4e-3, "{two}");
}

#[test]
fn a_pulldown_whose_resistance_nobody_states_is_a_finding_that_names_the_member() {
    let s = run(&board(
        "    .pull: PullDown\n    .pull.reference ~ .load\n    .pull.line ~ .gate.a\n    \
         .gate.b ~ .load.pos\n",
    ));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let found: Vec<(String, String)> = s
        .unconstrained
        .iter()
        .map(|u| {
            (
                s.model.path_of(u.part),
                s.model
                    .path_of_field(u.held_by.as_ref().expect("held to the member").member),
            )
        })
        .collect();
    assert_eq!(
        found,
        [("pull.resistor".to_string(), "pull.resistance".to_string())]
    );
    let s = run(&pulldown(""));
    assert!(s.unconstrained.is_empty(), "{:?}", s.unconstrained);
}

#[test]
fn a_resistor_over_its_rating_is_refused() {
    // 50 ohm across 5.25 V is 0.55 W, far over what any row of the catalog rates: the power
    // of the resistor is judged against the current the pull-down states, and no row passes.
    let s = run(&board(
        "    .pull: PullDown { .resistance &= 50ohm +/- 1%\n .line_top &= 0V..5.25V }\n    \
         .pull.reference ~ .load\n    .pull.line ~ .gate.a\n    .gate.b ~ .load.pos\n",
    ));
    let m = messages(&s);
    assert!(
        m.iter()
            .any(|m| m.contains("pull.resistor") && m.contains("max_power")),
        "no message naming the pull-down's resistor: {m:#?}"
    );
}

// ---- the package of the resistor inside (owner decision of 2026-11-10, item PK1) ----

fn package_of(s: &Solution, path: &str) -> String {
    match s.model.domain_at(path) {
        Some(Domain::Text(t)) => t.clone(),
        other => panic!("`{path}` is {other:?}"),
    }
}

#[test]
fn a_board_states_the_package_of_the_resistor_inside_a_pulldown() {
    let s = run(&pulldown(".resistor.package &= \"0402\""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(package_of(&s, "pull.resistor.package"), "0402");
}

#[test]
fn a_pulldown_whose_package_nobody_states_is_picked_as_before() {
    let s = run(&pulldown(""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(package_of(&s, "pull.resistor.package"), "0402");
    assert!(
        !not_evaluated(&s).iter().any(|r| r.contains("package")),
        "an unstated package is no open relation: {:?}",
        not_evaluated(&s)
    );
}

#[test]
fn a_pulldown_package_that_no_row_has_is_refused_with_its_name() {
    let s = run(&pulldown(".resistor.package &= \"0201\""));
    let all = messages(&s).join("\n");
    assert!(
        all.contains("has nothing for `pull.resistor`") && all.contains("\"0201\""),
        "the usual message names the part and the package: {all}"
    );
}
