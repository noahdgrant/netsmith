//! The solver works with a default (`?=`) as with the narrowing it became (ADR 0051): used
//! when nothing states the field, gone when a `&=` does, and judged by the relations like any
//! other value, also when the value reaches the field through a join.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

/// A source with a defaulted voltage, behind a series resistor. `{supply}` is more of the
/// supply's block.
fn board(supply: &str, extra: &str) -> String {
    format!(
        r#"import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board B {{
    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}
    .source: Rail {{
        .voltage ?= 5V +/- 2%
        .pos.net &= "VIN"
        .neg.net &= "GND"
{supply}    }}
    .r1: Resistor {{ .resistance &= 1kohm +/- 1% }}
    .load: Rail {{
        .pos.net &= "VLOAD"
        .neg.net &= "GND"
        .current &= 2mA..3mA
    }}
    .source.neg ~ .load.neg
    .source.pos ~> .r1 ~> .load.pos
    .load.voltage == .source.voltage - .r1.drop
{extra}}}
"#
    )
}

fn solved(src: &str) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> (f64, f64) {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) => (q.lo(), q.hi()),
        other => panic!("`{path}` is {other:?}"),
    }
}

#[test]
fn a_default_gives_the_supply_its_voltage_and_the_rest_follows() {
    let s = solved(&board("", ""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(range(&s, "source.voltage"), (4.9, 5.1));
    // 5 V less 2 mA to 3 mA through 1 kohm (1 %): a little under 3 V to a little over 3 V.
    let (lo, hi) = range(&s, "load.voltage");
    assert!(lo > 1.8 && hi < 3.2, "{lo}..{hi}");
}

#[test]
fn a_statement_replaces_the_default_and_the_solver_never_sees_the_old_value() {
    let s = solved(&board("        .voltage &= 9V..16V\n", ""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(range(&s, "source.voltage"), (9.0, 16.0));
    let (lo, _) = range(&s, "load.voltage");
    assert!(lo > 5.9, "{lo}");
}

#[test]
fn a_value_that_reaches_the_field_through_a_join_does_not_replace_the_default() {
    // The supply states 12 V and is joined to the module's output: the default stays on the
    // output, so the solver finds two different voltages on one net's joined rails.
    let src = board(
        "",
        "    .bench: Rail { .voltage &= 12V }\n    .bench ~ .source\n",
    );
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    let m = messages(&s);
    assert!(
        !m.is_empty(),
        "the default and the joined 12 V cannot both hold"
    );
    assert!(m.iter().any(|m| m.contains("source.voltage")), "{m:#?}");
}
