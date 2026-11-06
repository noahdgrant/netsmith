//! A default (`?=`) through the whole build (ADR 0051): a board that leaves it, a board that
//! replaces it, the parts the solver picks from each, and the messages. Every design here is
//! invented.

use netsmith::{Libraries, build_with};
use netsmith_check::Loader;
use netsmith_solve::Catalogs;

const LAMP: &str = r#"import std::parts::passive::{Resistor, Led}
import std::sources::Rail

module Lamp {
    .supply: Rail
    .current: Current
    .current ?= 2mA..3mA
    .led: Led
    .r1: Resistor
    .led.current &= .current
    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}

board Dim {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .lamp: Lamp
    .input ~ .lamp.supply
}

board Bright {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .lamp: Lamp { .current &= 6mA..9mA }
    .input ~ .lamp.supply
}
"#;

fn build(src: &str) -> Result<Vec<netsmith::Built>, String> {
    build_with(
        "main.nsm",
        src,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
    )
}

fn error_of(src: &str) -> String {
    match build(src) {
        Ok(_) => panic!("this should not build"),
        Err(message) => message,
    }
}

fn report_of(built: &[netsmith::Built], board: &str) -> String {
    built
        .iter()
        .map(|b| b.report.clone())
        .find(|r| r.starts_with(&format!("board {board}\n")))
        .unwrap_or_else(|| panic!("no report for {board}"))
}

#[test]
fn a_board_that_leaves_the_default_and_one_that_replaces_it_get_different_parts() {
    let built = build(LAMP).unwrap_or_else(|e| panic!("{e}"));
    let dim = report_of(&built, "Dim");
    let bright = report_of(&built, "Bright");
    assert!(dim.contains("R1   Resistor        1.2k"), "{dim}");
    assert!(bright.contains("R1   Resistor        390 "), "{bright}");
}

#[test]
fn a_default_on_a_catalog_column_narrows_the_candidates_until_a_statement_replaces_it() {
    let board = |resistor: &str| {
        format!(
            "import std::parts::passive::Resistor
import std::sources::Rail

board B {{
    .supply: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }}
    .r1: Resistor {{ {resistor} }}
    .supply.pos ~> .r1 ~> .supply.neg
}}
"
        )
    };
    let default = build(&board(".resistance ?= 10kohm +/- 1%")).unwrap_or_else(|e| panic!("{e}"));
    assert!(report_of(&default, "B").contains("Resistor        10k"));
    let replaced = build(&board(
        ".resistance ?= 10kohm +/- 1%; .resistance &= 22kohm +/- 1%",
    ))
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(report_of(&replaced, "B").contains("Resistor        22k"));
}

#[test]
fn two_defaults_are_an_error_that_names_both_lines() {
    let src = LAMP.replace(
        "    .lamp: Lamp\n",
        "    .lamp: Lamp\n    .lamp.current ?= 4mA..5mA\n",
    );
    let message = error_of(&src);
    assert!(
        message.contains("`lamp.current` has 2 defaults, at main.nsm:7 and main.nsm:22"),
        "{message}"
    );
}

#[test]
fn a_lone_question_mark_is_not_an_operator() {
    let message = error_of("board B {\n    .a ? 5V\n}\n");
    assert!(message.contains("unexpected character `?`"), "{message}");
}
