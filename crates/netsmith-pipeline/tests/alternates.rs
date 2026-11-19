//! A board with `alternate` parts (issue 1312): the board is checked, solved and designed again
//! with each alternate fitted, a failure is said under the alternate that fails, and the
//! alternates that pass are kept for the bill of materials. Every part here is invented.

use netsmith_check::Loader;
use netsmith_pipeline::{Outcome, Problem, outcome_with};
use netsmith_solve::Catalogs;

const PARTS: &str = r#"import std::parts::connector::Header1x2
import std::sources::Rail

part ModuleA {
    .vcc: Rail
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
    }
}

part ModuleB {
    .vcc: Rail
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
    }
}

/// Its supply range ends below the board's rail.
part ModuleLow {
    .vcc: Rail
    .vcc.voltage <= 2V
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
    }
}

/// The same pins on the other pads.
part ModuleTurned {
    .vcc: Rail
    pins {
        .vcc.pos power "VCC" "2"
        .vcc.neg power "GND" "1"
    }
}
"#;

fn source(alternates: &str) -> String {
    format!(
        r#"{PARTS}
board Tracker {{
    .input: Rail {{ .voltage &= 3.3V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }}
    .j: Header1x2
    .input.pos ~ .j.p1
    .input.neg ~ .j.p2
    .u: ModuleA {{ .assembly &= AssemblyMode::Hand }}
{alternates}
    .input ~ .u.vcc
}}
"#
    )
}

fn outcome(alternates: &str) -> Result<Outcome, Vec<Problem>> {
    outcome_with(
        "main.nsm",
        &source(alternates),
        &Loader::std(),
        &Catalogs::jlcpcb(),
        None,
    )
}

fn failure(alternates: &str) -> String {
    match outcome(alternates) {
        Ok(_) => panic!("{alternates} was accepted"),
        Err(problems) => problems
            .iter()
            .map(|p| p.rendered.clone())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

#[test]
fn an_alternate_that_passes_is_kept_with_its_sourcing() {
    let built = outcome("    alternate .u: ModuleB { .assembly &= AssemblyMode::Hand }")
        .unwrap_or_else(|p| panic!("{}", p[0].rendered))
        .boards
        .remove(0);
    let [alternate] = built.alternates.as_slice() else {
        panic!("one alternate, not {:?}", built.alternates)
    };
    assert_eq!(alternate.path, "u");
    assert_eq!((alternate.kind.as_str(), alternate.primary.as_str()), ("ModuleB", "ModuleA"));
    assert_eq!(alternate.sourcing.kind, "ModuleB");
    let u = built.design.parts.iter().find(|p| p.path == "u").unwrap();
    assert_eq!(alternate.refdes, u.refdes);
    // The board is drawn with the member as written.
    assert_eq!(u.kind, "ModuleA");
    assert_eq!(alternate.at.line, 45);
}

#[test]
fn a_board_with_no_alternate_keeps_none() {
    let built = outcome("").unwrap_or_else(|p| panic!("{}", p[0].rendered));
    assert!(built.boards[0].alternates.is_empty());
}

#[test]
fn the_alternate_that_fails_a_check_is_named() {
    let e = failure(
        "    alternate .u: ModuleB { .assembly &= AssemblyMode::Hand }\n    alternate .u: ModuleLow { .assembly &= AssemblyMode::Hand }",
    );
    assert!(
        e.contains("with `alternate .u: ModuleLow` (main.nsm:46) fitted in the place of `u`"),
        "{e}"
    );
    assert!(!e.contains("ModuleB`"), "only the alternate that fails is named: {e}");
}

#[test]
fn an_alternate_on_other_pads_is_refused() {
    let e = failure("    alternate .u: ModuleTurned { .assembly &= AssemblyMode::Hand }");
    assert!(
        e.contains("with `alternate .u: ModuleTurned` (main.nsm:45) fitted in the place of `u`: pad 1 of `u` is joined to `j.1` with `ModuleA` fitted and to `j.2`"),
        "{e}"
    );
    assert!(e.contains("with `ModuleA` fitted and to"), "{e}");
}
