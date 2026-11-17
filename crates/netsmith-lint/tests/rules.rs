//! Each rule of the linter, on a source that has its fault and on one that does not (issue #1104).

use std::collections::BTreeSet;

use netsmith_check::Loader;
use netsmith_lint::{Lint, Project, Root, Rule, lint};
use netsmith_solve::Catalogs;

/// The findings of one file, as a project of that file alone.
fn findings_with(text: &str, loader: &Loader) -> Vec<Lint> {
    let catalogs = Catalogs::jlcpcb();
    let project = Project {
        loader,
        roots: vec![Root {
            name: "main.nsm".into(),
            text: text.into(),
            assignment: Default::default(),
        }],
        whole: true,
        catalogs: Some(&catalogs),
        off: BTreeSet::new(),
        named: Vec::new(),
    };
    lint(&project).unwrap_or_else(|e| panic!("{e}"))
}

fn findings(text: &str) -> Vec<Lint> {
    findings_with(text, &Loader::std())
}

/// The findings of one rule, rendered.
fn of(rule: Rule, text: &str) -> Vec<String> {
    findings(text)
        .into_iter()
        .filter(|l| l.rule == rule)
        .map(|l| l.render())
        .collect()
}

/// A board with nothing to say against it: the base the faults are added to.
const CLEAN: &str = r#"import std::parts::passive::Resistor
import std::sources::Rail

module Pulldown {
    .input: Rail
    .r: Resistor { .resistance &= 10kohm +/- 1% }
    .input.pos ~> .r ~> .input.neg
}

board Probe {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .hold: Pulldown
    .supply ~ .hold.input
}
"#;

#[test]
fn a_clean_board_has_no_finding() {
    assert_eq!(findings(CLEAN), []);
}

#[test]
fn an_import_the_file_never_uses_is_found() {
    let text = CLEAN.replace(
        "import std::parts::passive::Resistor",
        "import std::parts::passive::{Resistor, Capacitor}",
    );
    assert_eq!(
        of(Rule::UnusedImport, &text),
        ["main.nsm:1:40: lint.unused_import: `Capacitor` is imported from `std::parts::passive` and never used in this file; delete it from the import"]
    );
    assert_eq!(of(Rule::UnusedImport, CLEAN), Vec::<String>::new());
}

#[test]
fn a_module_no_board_reaches_is_found_and_one_a_board_uses_is_not() {
    let text = format!("{CLEAN}\nmodule Spare {{\n    .r: Resistor {{ .resistance &= 1kohm +/- 1% }}\n}}\n");
    assert_eq!(
        of(Rule::UnusedDefinition, &text),
        ["main.nsm:19:8: lint.unused_definition: the module `Spare` is never used: no board of the project reaches it; use it, or delete it"]
    );
    assert_eq!(of(Rule::UnusedDefinition, CLEAN), Vec::<String>::new());
}

#[test]
fn a_definition_of_a_project_with_no_board_is_not_judged() {
    let text = "import std::parts::passive::Resistor\n\nmodule Spare {\n    .r: Resistor\n}\n";
    assert_eq!(of(Rule::UnusedDefinition, text), Vec::<String>::new());
}

#[test]
fn a_member_never_joined_and_a_value_never_used_are_found() {
    let text = CLEAN.replace(
        "    .input.pos ~> .r ~> .input.neg\n",
        "    .input.pos ~> .r ~> .input.neg\n    .spare: Resistor { .resistance &= 1kohm +/- 1% }\n    .gain: Voltage\n",
    );
    assert_eq!(
        of(Rule::UnusedDefinition, &text),
        [
            "main.nsm:8:5: lint.unused_definition: `.spare` is declared and never joined: no pin of it is on a net; join it, or delete it",
            "main.nsm:9:5: lint.unused_definition: `.gain` is declared and never used: no statement states or reads it; state it, read it, or delete it",
        ]
    );
    // A value that a relation reads is used.
    let read = text.replace(".gain: Voltage", ".gain: Voltage\n    .gain == .input.voltage");
    assert_eq!(
        of(Rule::UnusedDefinition, &read),
        ["main.nsm:8:5: lint.unused_definition: `.spare` is declared and never joined: no pin of it is on a net; join it, or delete it"]
    );
}

#[test]
fn a_narrowing_a_board_default_already_gives_is_found_and_another_value_is_not() {
    let text = CLEAN.replace(
        "    .hold: Pulldown\n",
        "    defaults { Resistor.package ?= \"0402\" }\n    .hold: Pulldown { .r.package &= \"0402\" }\n",
    );
    assert_eq!(
        of(Rule::Redundant, &text),
        ["main.nsm:16:23: lint.redundant: `.r.package &= \"0402\"` says what the default at main.nsm:15 already gives; delete it"]
    );
    let other = text.replace(".r.package &= \"0402\"", ".r.package &= \"0603\"");
    assert_eq!(of(Rule::Redundant, &other), Vec::<String>::new());
}

#[test]
fn a_default_the_kind_s_own_default_gives_is_found() {
    let loader = Loader::std();
    let module = "/// A part with a default.\npart Pad {\n    .designator &= \"TP\"\n    .size: Length\n    .size ?= 1mm\n    pins { .p passive \"P\" \"1\" }\n}\n";
    let text = format!("{module}\nboard B {{\n    .a: Pad {{ .size &= 1mm }}\n    .b: Pad\n    .a.p ~ .b.p\n}}\n");
    let found: Vec<String> = findings_with(&text, &loader)
        .into_iter()
        .filter(|l| l.rule == Rule::Redundant)
        .map(|l| l.render())
        .collect();
    assert_eq!(
        found,
        ["main.nsm:10:15: lint.redundant: `.size &= 1mm` says what the default at main.nsm:5 already gives; delete it"]
    );
}

#[test]
fn names_against_the_guide_are_found() {
    let text = CLEAN
        .replace("board Probe", "board probe_board")
        .replace(".hold: Pulldown", ".Hold: Pulldown")
        .replace(".supply ~ .hold.input", ".supply ~ .Hold.input");
    assert_eq!(
        of(Rule::Naming, &text),
        [
            "main.nsm:10:7: lint.naming: `probe_board` is a board, whose name is `CamelCase` in the guide; call it `ProbeBoard`",
            "main.nsm:15:6: lint.naming: `.Hold` is a member, whose name is `snake_case` in the guide; call it `.hold`",
        ]
    );
    assert_eq!(of(Rule::Naming, CLEAN), Vec::<String>::new());
}

#[test]
fn a_comment_turns_a_rule_off_for_one_line() {
    let text = CLEAN.replace("board Probe {", "board probe { // lint: off naming");
    assert_eq!(of(Rule::Naming, &text), Vec::<String>::new());
    let above = CLEAN.replace("board Probe {", "// lint: off naming\nboard probe {");
    assert_eq!(of(Rule::Naming, &above), Vec::<String>::new());
}

#[test]
fn a_rule_turned_off_says_nothing() {
    let text = CLEAN.replace("board Probe", "board probe");
    let loader = Loader::std();
    let project = Project {
        loader: &loader,
        roots: vec![Root {
            name: "main.nsm".into(),
            text,
            assignment: Default::default(),
        }],
        whole: true,
        catalogs: None,
        off: BTreeSet::from([Rule::Naming]),
        named: Vec::new(),
    };
    assert_eq!(lint(&project).unwrap(), []);
}

#[test]
fn a_source_that_does_not_check_is_refused_and_a_misspelt_rule_too() {
    let loader = Loader::std();
    let project = |text: &str| Project {
        loader: &loader,
        roots: vec![Root {
            name: "main.nsm".into(),
            text: text.into(),
            assignment: Default::default(),
        }],
        whole: true,
        catalogs: None,
        off: BTreeSet::new(),
        named: Vec::new(),
    };
    let e = lint(&project("board B {\n    .r: Nothing\n}\n")).unwrap_err();
    assert!(e.0.starts_with("main.nsm does not check (1 error)"), "{e}");
    let e = lint(&project(&CLEAN.replace("board Probe {", "board Probe { // lint: off nameing")))
        .unwrap_err();
    assert!(e.0.starts_with("main.nsm:10: `// lint: off nameing` names no rule"), "{e}");
}
