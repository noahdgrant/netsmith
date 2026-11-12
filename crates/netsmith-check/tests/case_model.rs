//! A board's case model (ADR 0109): a `CaseModel` of the physical file's `case { }` section,
//! with the fields of a part's `StepModel`, and the places it may not stand.

use netsmith_check::*;

const BOARD: &str = "\
import std::parts::connector::Header1x2

board B {
    .j: Header1x2
}
";

fn run(circuit: &str, physical: &str) -> Checked {
    let loader = Loader::std().with_physical("t.nsm", "t.physical.nsm", physical);
    check("t.nsm", circuit, &loader)
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn a_case_model_is_declared_in_the_case_section() {
    let c = run(
        BOARD,
        "import std::pcb::CaseModel\n\nphysical for B {\n    case {\n        .shell: CaseModel {\n            .file &= \"case/box.step\"\n            .z &= -1.6mm\n            .rx &= 180\n        }\n        .lid: CaseModel {\n            .file &= \"case/lid.step\"\n        }\n    }\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    let shell = m
        .inst_at("shell")
        .expect("the case model is a member of the board");
    assert_eq!(m.instance(shell).type_name, "CaseModel");
    for field in ["file", "x", "y", "z", "rx", "ry", "rz"] {
        assert!(m.field_at(&format!("shell.{field}")).is_some(), "{field}");
    }
    assert_eq!(
        m.domain_at("shell.file"),
        Some(&Domain::Text("case/box.step".into()))
    );
    assert!(matches!(m.domain_at("shell.rx"), Some(Domain::Quantity(q)) if q.lo() == 180.0));
    assert!(m.inst_at("lid").is_some());
}

#[test]
fn the_unit_may_state_a_case_models_fields_outside_the_section() {
    let c = run(
        BOARD,
        "import std::pcb::CaseModel\n\nphysical for B {\n    case {\n        .shell: CaseModel {}\n    }\n    .shell.file &= \"case/box.step\"\n    .shell.y &= 2mm\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_case_model_outside_the_case_section_is_refused() {
    let c = run(
        BOARD,
        "import std::pcb::CaseModel\n\nphysical for B {\n    .shell: CaseModel {\n        .file &= \"case/box.step\"\n    }\n}\n",
    );
    assert_eq!(
        messages(&c),
        [
            "a case model belongs in the `case { }` section of `physical for B`: wrap `.shell: CaseModel` in `case { ... }`"
        ]
    );
}

#[test]
fn a_case_model_in_the_circuit_source_is_refused() {
    let c = run(
        "import std::pcb::CaseModel\n\nboard B {\n    .shell: CaseModel {\n        .file &= \"case/box.step\"\n    }\n}\n",
        "",
    );
    assert_eq!(
        messages(&c),
        [
            "`.shell: CaseModel` is a case model, which is physical intent, and a board's circuit source does not hold it (ADR 0048); declare it in the `case { }` section of `physical for B` in `t.physical.nsm`"
        ]
    );
}
