//! A range whose ends are values (ADR 0066): `.vcc.current &= .min_draw .. .max_draw` is read
//! when both ends have a value, as the range of its low end's lowest and its high end's highest
//! value, and is not a claim while an end has none. Every part and number here is invented.

use netsmith_check::{Domain, Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Catalogs, Solution, solve};

/// A gate whose datasheet gives the least and the most its supply pin draws.
const GATE: &str = "\
import std::parts::regulator::LinearRegulator
import std::sources::Rail

part Gate {
    .designator &= \"U\"
    .vcc: Rail
    .min_draw: Current
    .max_draw: Current
    .vcc.current &= .min_draw .. .max_draw
    pins {
        .vcc.pos power \"VCC\" \"1\"
        .vcc.neg power \"GND\" \"2\"
    }
}
";

/// A 5 V input, a regulator that makes the 3.3 V net, a load of 0 to 20 mA on it and the gate,
/// with `gate` as the gate's own statements and `extra` as more lines of the board.
fn board(gate: &str, extra: &str) -> String {
    format!(
        "{GATE}\nboard Counted {{\n    .input: Rail {{\n        .voltage &= 5V +/- 5%\n        \
         .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }}\n    \
         .ldo: LinearRegulator {{ .vout.voltage &= 3.3V +/- 5% }}\n    \
         .load: Rail {{\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n        \
         .current &= 0mA..20mA\n    }}\n    .input ~ .ldo.vin\n    .ldo.vout ~ .load\n    \
         .gate: Gate {{\n{gate}    }}\n    .gate.vcc ~ .load\n{extra}}}\n"
    )
}

fn run(src: &str) -> Solution {
    let checked = check("test.nsm", src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let board = checked.standalone_boards().next().expect("one board");
    solve(board, &Catalogs::jlcpcb())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn domain(s: &Solution, path: &str) -> Domain {
    s.model
        .domain_at(path)
        .unwrap_or_else(|| panic!("no `{path}`"))
        .clone()
}

fn range(s: &Solution, path: &str) -> Quantity {
    match domain(s, path) {
        Domain::Quantity(q) => q,
        other => panic!("`{path}` is {other:?}"),
    }
}

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

#[test]
fn an_end_that_is_a_member_is_read_once_it_has_a_value() {
    let s = run(&board(
        "        .min_draw &= 100nA\n        .max_draw &= 750nA\n",
        "",
    ));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:?}", not_evaluated(&s));
    let draw = range(&s, "gate.vcc.current");
    assert!(
        (draw.lo() - 100e-9).abs() < 1e-15 && (draw.hi() - 750e-9).abs() < 1e-15,
        "{draw}"
    );
}

#[test]
fn an_end_that_is_itself_a_range_gives_the_widest_range_it_allows() {
    // The low end is 80 nA to 120 nA and the high 700 nA to 800 nA: the range holds the lowest
    // of the first and the highest of the second, as the written `lo..hi` does.
    let s = run(&board(
        "        .min_draw &= 80nA..120nA\n        .max_draw &= 700nA..800nA\n",
        "",
    ));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let draw = range(&s, "gate.vcc.current");
    assert!(
        (draw.lo() - 80e-9).abs() < 1e-15 && (draw.hi() - 800e-9).abs() < 1e-15,
        "{draw}"
    );
}

#[test]
fn an_end_with_no_value_is_not_a_claim_and_is_named() {
    // The high end is never stated: nothing narrows the current, nothing is refused, and the
    // statement is listed as not evaluated, with its ends.
    let s = run(&board("        .min_draw &= 100nA\n", ""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(domain(&s, "gate.vcc.current"), Domain::Unset);
    let open = not_evaluated(&s);
    assert!(
        open.contains(&"gate.vcc.current (gate.min_draw..gate.max_draw)".to_string()),
        "{open:#?}"
    );
}

#[test]
fn a_low_end_above_the_high_end_is_refused_with_the_numbers() {
    let s = run(&board(
        "        .min_draw &= 2uA\n        .max_draw &= 750nA\n",
        "",
    ));
    let m = messages(&s).join("\n");
    assert!(
        m.contains(
            "(gate.min_draw..gate.max_draw): the low end of the range is above the high end: 2uA against 750nA"
        ),
        "{m}"
    );
}

#[test]
fn the_range_reaches_the_rail_and_the_regulator_that_supplies_it() {
    let s = run(&board(
        "        .min_draw &= 100nA\n        .max_draw &= 750nA\n",
        "",
    ));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    // The regulator supplies the load, 0 to 20 mA, and the gate, 100 nA to 750 nA.
    let ldo = range(&s, "ldo.vout.current");
    assert!(
        (ldo.lo() + (20e-3 + 750e-9)).abs() < 1e-12 && (ldo.hi() + 100e-9).abs() < 1e-12,
        "{ldo}"
    );
}

/// A supply that is allowed `budget` and the gate on it, nothing else.
fn alone(budget: &str) -> String {
    format!(
        "{GATE}\nboard Alone {{\n    .supply: Rail {{\n        .voltage &= 3.3V +/- 5%\n        \
         .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n        .current &= {budget}\n    }}\n    \
         .gate: Gate {{\n        .min_draw &= 100nA\n        .max_draw &= 750nA\n    }}\n    \
         .supply ~ .gate.vcc\n}}\n"
    )
}

#[test]
fn a_budget_the_gate_cannot_keep_is_refused_with_the_numbers() {
    // The gate draws 100 nA to 750 nA, so a supply that may give 1 uA is enough and one that
    // may give 500 nA is not. A supply gives, so its current is the draws negated.
    let s = run(&alone("-1uA..0A"));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:?}", not_evaluated(&s));
    let s = run(&alone("-500nA..0A"));
    let m = messages(&s).join("\n");
    assert!(
        m.contains("do not add up") && m.contains("gate.vcc.current") && m.contains("750nA"),
        "{m}"
    );
}

#[test]
fn the_order_of_the_statements_does_not_matter() {
    let gate = "        .min_draw &= 100nA\n        .max_draw &= 750nA\n";
    let reversed = "        .max_draw &= 750nA\n        .min_draw &= 100nA\n";
    let forward = run(&board(gate, ""));
    // The board's statements the other way round, and the part's range written before the
    // members it reads are declared.
    let line = "    .vcc.current &= .min_draw .. .max_draw\n";
    let src = board(reversed, "").replace(line, "");
    let src = src.replace("    .vcc: Rail\n", &format!("{line}    .vcc: Rail\n"));
    assert_ne!(src, board(reversed, ""));
    let backward = run(&src);
    assert!(messages(&backward).is_empty(), "{:?}", messages(&backward));
    assert_eq!(
        range(&forward, "gate.vcc.current"),
        range(&backward, "gate.vcc.current")
    );
    assert_eq!(
        range(&forward, "ldo.vout.current"),
        range(&backward, "ldo.vout.current")
    );
}
