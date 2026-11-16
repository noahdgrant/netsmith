//! A default (`?=`) through the whole build (ADR 0051): a board that leaves it, a board that
//! replaces it, the parts the solver picks from each, and the messages. Every design here is
//! invented.

use netsmith_pipeline::{Libraries, build_with};
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

fn build(src: &str) -> Result<Vec<netsmith_pipeline::Built>, String> {
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

fn report_of(built: &[netsmith_pipeline::Built], board: &str) -> String {
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

/// Who assembles each part of the built board, by reference designator (issue #847).
fn assembly_of(built: &[netsmith_pipeline::Built]) -> Vec<(String, String)> {
    built[0]
        .sourcing
        .iter()
        .map(|s| (s.refdes.clone(), format!("{:?}", s.assembly)))
        .collect()
}

#[test]
fn a_line_for_assembly_reaches_the_design_stage_and_gives_way_to_a_statement() {
    let board = |defaults: &str, lamp: &str| {
        format!(
            "import std::parts::passive::{{Resistor, Led}}\nimport std::sources::Rail\n\nboard B {{\n{defaults}    .input: Rail {{\n        .voltage &= 5V +/- 5%\n        .pos.net &= \"VCC\"\n        .neg.net &= \"GND\"\n    }}\n    .led: Led {{ .current &= 5mA..15mA {lamp} }}\n    .r1: Resistor\n    .led.current == (.input.voltage - .led.forward_voltage) / .r1.resistance\n    .input.pos ~> .r1 ~> .led ~> .input.neg\n}}\n"
        )
    };
    let built = |src: String| build(&src).unwrap_or_else(|e| panic!("{e}"));
    let pair = |a: &str, b: &str| {
        vec![
            ("D1".to_string(), a.to_string()),
            ("R1".to_string(), b.to_string()),
        ]
    };
    // Nothing said: the board house assembles both.
    assert_eq!(assembly_of(&built(board("", ""))), pair("Fab", "Fab"));
    // A line for the kind: the board's resistors are soldered by hand, its LED is not.
    let lined = board(
        "    defaults {\n        Resistor.assembly ?= AssemblyMode::Hand\n    }\n",
        "",
    );
    assert_eq!(assembly_of(&built(lined)), pair("Fab", "Hand"));
    // The line is an offer: a statement on the part replaces it.
    let stated = board(
        "    defaults {\n        Resistor.assembly ?= AssemblyMode::Hand\n        Led.assembly ?= AssemblyMode::Hand\n    }\n    .r1.assembly &= AssemblyMode::Fab\n",
        "",
    );
    assert_eq!(assembly_of(&built(stated)), pair("Hand", "Fab"));
    let own = board(
        "    defaults {\n        Led.assembly ?= AssemblyMode::Hand\n    }\n",
        "; .assembly &= AssemblyMode::Consigned",
    );
    assert_eq!(assembly_of(&built(own)), pair("Consigned", "Fab"));
}
