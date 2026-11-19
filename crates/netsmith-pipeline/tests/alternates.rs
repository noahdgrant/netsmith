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

/// A board that a system places, whose input comes from another board of the system: what the
/// alternate's checks read is what the system gives it (issue 1414).
fn system(alternates: &str) -> String {
    format!(
        r#"{PARTS}
board Feed {{
    .output: Rail {{ .voltage &= 3.3V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }}
    .j: Header1x2
    .output.pos ~ .j.p1
    .output.neg ~ .j.p2
}}

board Tracker {{
    .input: Rail {{ .pos.net &= "VCC"; .neg.net &= "GND" }}
    .j: Header1x2
    .input.pos ~ .j.p1
    .input.neg ~ .j.p2
    .u: ModuleA {{ .assembly &= AssemblyMode::Hand }}
{alternates}
    .input ~ .u.vcc
}}

system Kit {{
    .feed: Feed
    .main: Tracker
    .feed.output ~ .main.input
}}
"#
    )
}

fn system_outcome(alternates: &str) -> Result<Outcome, Vec<Problem>> {
    outcome_with(
        "main.nsm",
        &system(alternates),
        &Loader::std(),
        &Catalogs::jlcpcb(),
        None,
    )
}

#[test]
fn an_alternate_of_a_board_in_a_system_is_kept() {
    let outcome = system_outcome("    alternate .u: ModuleB { .assembly &= AssemblyMode::Hand }")
        .unwrap_or_else(|p| panic!("{}", p[0].rendered));
    let main = outcome
        .boards
        .iter()
        .find(|b| b.board == "main")
        .expect("the board of the system is built");
    let [alternate] = main.alternates.as_slice() else {
        panic!("one alternate, not {:?}", main.alternates)
    };
    assert_eq!(alternate.path, "u");
    assert_eq!(alternate.sourcing.kind, "ModuleB");
    let u = main.design.parts.iter().find(|p| p.path == "u").unwrap();
    assert_eq!(alternate.refdes, u.refdes);
    let feed = outcome.boards.iter().find(|b| b.board == "feed").unwrap();
    assert!(feed.alternates.is_empty());
}

#[test]
fn an_alternate_of_a_board_in_a_system_is_checked_with_what_the_system_gives() {
    let problems = system_outcome("    alternate .u: ModuleLow { .assembly &= AssemblyMode::Hand }")
        .err()
        .expect("the alternate's supply range ends below what the system gives");
    let e = problems
        .iter()
        .map(|p| p.rendered.clone())
        .collect::<String>();
    assert!(
        e.contains("with `alternate .u: ModuleLow` (main.nsm:52) fitted in the place of `u`: "),
        "{e}"
    );
    assert!(e.contains("2V"), "{e}");
    assert!(
        problems.iter().all(|p| p.board.as_deref() == Some("Kit/main")),
        "{problems:?}"
    );
}

/// The board with a resistor, whose alternate is a capacitor that nothing limits.
fn passive(alternates: &str) -> String {
    format!(
        r#"import std::parts::connector::Header1x2
import std::parts::passive::Capacitor
import std::parts::passive::Resistor

board Filter {{
    .j: Header1x2
    .r: Resistor {{ .resistance &= 10kohm +/- 5%; .package &= "0402" }}
{alternates}
    .j.p1 ~ .r.a
    .j.p2 ~ .r.b
}}
"#
    )
}

#[test]
fn a_guideline_finding_of_the_board_with_an_alternate_fitted_is_named_under_it() {
    let source = passive("    alternate .r: Capacitor { .package &= \"0402\" }");
    let outcome = outcome_with(
        "main.nsm",
        &source,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        None,
    )
    .unwrap_or_else(|p| panic!("{}", p[0].rendered));
    let board = &outcome.boards[0];
    let found: Vec<&str> = board
        .guidelines
        .iter()
        .filter(|g| g.rule == "unconstrained pick")
        .map(|g| g.message.as_str())
        .collect();
    let [finding] = found.as_slice() else {
        panic!("one unconstrained pick, not {:?}", board.guidelines)
    };
    assert!(
        finding.starts_with("with `alternate .r: Capacitor` (main.nsm:8) fitted in the place of `r`: `r` (Capacitor) was chosen with nothing limiting"),
        "{finding}"
    );
    let r = board.trace.model.inst_at("r");
    assert!(
        board
            .guidelines
            .iter()
            .any(|g| g.rule == "unconstrained pick" && g.at == r && r.is_some()),
        "the finding is about `r` of the board as written"
    );

    // With guidelines made errors, it stops the build under the alternate.
    let strict = outcome_with(
        "main.nsm",
        &source,
        &Loader::std(),
        &Catalogs::jlcpcb().with_strict_guidelines(true),
        None,
    );
    let e = strict
        .err()
        .expect("the finding is an error")
        .iter()
        .map(|p| p.rendered.clone())
        .collect::<String>();
    assert!(
        e.contains("with `alternate .r: Capacitor` (main.nsm:8) fitted in the place of `r`: Filter: unconstrained pick guideline: `r` (Capacitor)"),
        "{e}"
    );
}

#[test]
fn a_finding_the_board_as_written_has_is_not_said_again_for_its_alternate() {
    let source = r#"import std::parts::connector::Header1x2
import std::parts::passive::Resistor
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

board Tracker {
    .input: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }
    .j: Header1x2
    .input.pos ~ .j.p1
    .input.neg ~ .j.p2
    .u: ModuleA { .assembly &= AssemblyMode::Hand }
    alternate .u: ModuleB { .assembly &= AssemblyMode::Hand }
    .input ~ .u.vcc
    .pull: Resistor
    .input.pos ~> .pull ~> .input.neg
}
"#;
    let outcome = outcome_with(
        "main.nsm",
        source,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        None,
    )
    .unwrap_or_else(|p| panic!("{}", p[0].rendered));
    let picks: Vec<&str> = outcome.boards[0]
        .guidelines
        .iter()
        .filter(|g| g.rule == "unconstrained pick")
        .map(|g| g.message.as_str())
        .collect();
    assert_eq!(picks.len(), 1, "{picks:?}");
    assert!(picks[0].starts_with("`pull` (Resistor)"), "{picks:?}");
}
