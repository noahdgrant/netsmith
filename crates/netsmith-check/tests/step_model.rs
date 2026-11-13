//! The built-in `.model` of every part (ADR 0108): a `StepModel` that a part's definition, a
//! board or its physical file states, and a contradiction between two statements.

use netsmith_check::*;

const HEAD: &str = "\
import std::parts::connector::Header1x2

part Usb {
    .p1: Pin
    .model.file &= \"models/usb.step\"
    .model.z &= -0.2mm
    .model.rz &= 90
}

";

fn run(board: &str, physical: &str) -> Checked {
    let loader = Loader::std().with_physical("t.nsm", "t.physical.nsm", physical);
    check("t.nsm", &format!("{HEAD}{board}"), &loader)
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn every_part_has_a_model_of_the_preludes_step_model() {
    let c = run("board B {\n    .j: Header1x2\n}\n", "");
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    let model = m.inst_at("j.model").expect("a part has `.model`");
    assert_eq!(m.instance(model).type_name, "StepModel");
    assert!(m.is_step_model(model));
    for field in ["file", "x", "y", "z", "rx", "ry", "rz"] {
        assert!(m.field_at(&format!("j.model.{field}")).is_some(), "{field}");
    }
    assert_eq!(m.domain_at("j.model.file"), Some(&Domain::Unset));
}

#[test]
fn a_part_definition_states_its_model_for_every_use() {
    let c = run("board B {\n    .usb: Usb\n}\n", "");
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    assert_eq!(
        m.domain_at("usb.model.file"),
        Some(&Domain::Text("models/usb.step".into()))
    );
    assert!(matches!(m.domain_at("usb.model.rz"), Some(Domain::Quantity(q)) if q.lo() == 90.0));
}

#[test]
fn a_board_and_its_physical_file_state_the_model_of_one_part() {
    let c = run(
        "board B {\n    .j: Header1x2\n    .j.model.file &= \"models/header.step\"\n}\n",
        "physical for B {\n    .j.model.rx &= -90\n    .j.model.y &= 1.27mm\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    assert!(matches!(m.domain_at("j.model.rx"), Some(Domain::Quantity(q)) if q.lo() == -90.0));
}

#[test]
fn two_files_for_one_part_contradict() {
    let c = run(
        "board B {\n    .usb: Usb\n}\n",
        "physical for B {\n    .usb.model.file &= \"models/other.step\"\n}\n",
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].contains("usb.model.file") && m[0].contains("models/other.step"),
        "{}",
        m[0]
    );
}

#[test]
fn a_model_is_of_a_part_and_not_of_a_module() {
    let c = check(
        "t.nsm",
        "module M {\n}\nboard B {\n    .m: M\n    .m.model.file &= \"x.step\"\n}\n",
        &Loader::std(),
    );
    assert!(!c.is_ok(), "a module has no `.model`");
}
