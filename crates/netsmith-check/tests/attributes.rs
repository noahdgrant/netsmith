//! Attributes on a generic, a part and a member (ADR 0125): what a project's own definition
//! gets from them, and the refusals of an unknown or misplaced one. The parts are invented
//! for these tests.

use netsmith_check::*;
use netsmith_core::PartKind;

const HEAD: &str = "import std::sources::Rail\n";

fn checked(src: &str) -> Checked {
    check("t.nsm", &format!("{HEAD}{src}"), &Loader::std())
}

fn ok(src: &str) -> Checked {
    let c = checked(src);
    assert!(c.is_ok(), "{}", c.render());
    c
}

/// The messages of a source the checker refuses.
fn refused(src: &str) -> String {
    let c = checked(src);
    assert!(!c.is_ok(), "the checker took it");
    c.render()
}

/// A project's own two-ended part, `body` added to its definition.
fn part(attrs: &str, body: &str) -> String {
    format!(
        "{attrs}\npart Thing {{\n    .designator &= \"X\"\n    pins {{\n        .a passive \"A\" \"1\"\n        .b passive \"B\" \"2\"\n    }}\n    bridge .a ~> .b\n{body}}}\n"
    )
}

fn instance(c: &Checked, path: &str) -> InstId {
    c.boards[0].model.inst_at(path).expect("the part is placed")
}

#[test]
fn a_project_generic_declared_bank_is_a_bank_with_its_count_role() {
    let src = "#[bank]\ngeneric Cell {\n    .designator &= \"C\"\n    pins {\n        .a passive \"A\" \"1\"\n        .b passive \"B\" \"2\"\n    }\n    #[role(count)]\n    .cells: Number\n}\nboard B {\n    .c: Cell { .cells &= 1..4 }\n}\n";
    let c = ok(src);
    let m = &c.boards[0].model;
    let cell = instance(&c, "c");
    assert!(m.bank(cell).is_some());
    let count = m.bank_count(cell).expect("the bank has its count");
    assert_eq!(m.path_of_field(count), "c.cells");
}

#[test]
fn a_part_that_provides_a_bank_generic_is_a_bank_too() {
    let src = "#[bank]\ngeneric Cell {\n    #[role(count)]\n    .count: Number\n}\npart AcmeCell provides Cell {\n    .designator &= \"C\"\n    pins {\n        .a passive \"A\" \"1\"\n        .b passive \"B\" \"2\"\n    }\n}\nboard B {\n    .c: AcmeCell\n}\n";
    let c = ok(src);
    let m = &c.boards[0].model;
    assert!(m.bank(instance(&c, "c")).is_some());
    assert!(m.bank_count(instance(&c, "c")).is_some());
}

#[test]
fn a_bank_names_the_kind_its_parts_are() {
    let c = ok(
        "import std::parts::passive::{Capacitor, CapacitorBank}\nboard B {\n    .bulk: CapacitorBank\n}\n",
    );
    let m = &c.boards[0].model;
    let bank = m.bank(instance(&c, "bulk")).expect("the standard bank is one");
    assert_eq!((bank.generic.as_str(), bank.member.as_str()), ("CapacitorBank", "Capacitor"));
    assert_eq!(m.path_of_field(bank.count), "bulk.count");
}

#[test]
fn without_bank_a_count_role_makes_no_bank() {
    let src = format!(
        "{}board B {{\n    .t: Thing\n}}\n",
        part("", "    #[role(count)]\n    .count: Number\n")
    );
    let c = ok(&src);
    let m = &c.boards[0].model;
    assert!(m.bank(instance(&c, "t")).is_none());
    assert!(m.bank_count(instance(&c, "t")).is_none());
}

#[test]
fn a_role_names_the_member_whatever_it_is_called() {
    // The bias of a part across a rail goes to the member with `#[role(bias)]`, here `.across`.
    let src = format!(
        "{}board B {{\n    .rail: Rail\n    .t: Thing\n    .rail.pos ~> .t ~> .rail.neg\n}}\n",
        part("", "    #[role(bias)]\n    .across: Voltage\n")
    );
    let c = ok(&src);
    let m = &c.boards[0].model;
    let relations: Vec<String> = m
        .relations()
        .iter()
        .map(|r| format!("{} = {}", m.show_expr(&r.lhs), m.show_expr(&r.rhs)))
        .collect();
    assert!(
        relations.iter().any(|r| r == "t.across = rail.voltage"),
        "{relations:?}"
    );
}

#[test]
fn a_member_named_like_a_role_without_the_attribute_is_a_plain_member() {
    let src = format!(
        "{}board B {{\n    .rail: Rail\n    .t: Thing\n    .rail.pos ~> .t ~> .rail.neg\n}}\n",
        part("", "    .bias: Voltage\n")
    );
    let c = ok(&src);
    let m = &c.boards[0].model;
    assert!(m.bias_of(instance(&c, "t")).is_none());
    assert!(
        !m.relations()
            .iter()
            .any(|r| m.show_expr(&r.lhs).contains("bias"))
    );
}

#[test]
fn a_kind_and_a_value_reach_the_instance_and_its_providers() {
    let src = "#[kind(fuse)]\n#[value(\"PTC\")]\ngeneric Guard {\n    .rated: Current\n}\npart AcmeGuard provides Guard {\n    .designator &= \"F\"\n    pins {\n        .a passive \"A\" \"1\"\n        .b passive \"B\" \"2\"\n    }\n}\nboard B {\n    .g: AcmeGuard\n}\n";
    let c = ok(src);
    let marks = &c.boards[0].model.instance(instance(&c, "g")).marks;
    assert_eq!(marks.declared_kind, Some(PartKind::Fuse));
    assert_eq!(marks.value.as_deref(), Some("PTC"));
}

#[test]
fn a_series_is_kept_as_written() {
    let src = "#[series(E12, E24)]\ngeneric Thermistor {\n    .designator &= \"RT\"\n    pins {\n        .a passive \"A\" \"1\"\n        .b passive \"B\" \"2\"\n    }\n    .resistance: Resistance\n    catalog thermistor { .resistance }\n}\nboard B {\n    .t: Thermistor\n}\n";
    let c = ok(src);
    let marks = &c.boards[0].model.instance(instance(&c, "t")).marks;
    assert_eq!(marks.series, ["E12", "E24"]);
}

#[test]
fn the_standard_generics_state_their_kinds_and_series() {
    let c = ok(
        "import std::parts::passive::{Capacitor, Led, Resistor}\nimport std::parts::connector::Header1x2\nboard B {\n    .r: Resistor\n    .c: Capacitor\n    .d: Led\n    .j: Header1x2\n}\n",
    );
    let m = &c.boards[0].model;
    let marks = |p: &str| m.instance(instance(&c, p)).marks.clone();
    assert_eq!(marks("r").declared_kind, Some(PartKind::Resistor));
    assert_eq!(marks("r").series, ["E12", "E24", "E96"]);
    assert_eq!(marks("c").series, ["E12", "E24"]);
    assert_eq!(marks("d").value.as_deref(), Some("LED"));
    assert_eq!(marks("j").declared_kind, Some(PartKind::Connector));
    assert_eq!(marks("j").assembly.as_deref(), Some("hand"));
}

#[test]
fn an_unknown_attribute_is_refused_with_did_you_mean() {
    let message = refused(&part("#[bnak]", ""));
    assert!(
        message.contains("there is no attribute `#[bnak]` (did you mean `bank`?)"),
        "{message}"
    );
    let message = refused(&part("", "    #[rol(bias)]\n    .v: Voltage\n"));
    assert!(
        message.contains("there is no attribute `#[rol]` for a member (did you mean `role`?)"),
        "{message}"
    );
}

#[test]
fn an_unknown_role_is_refused_with_did_you_mean() {
    let message = refused(&part("", "    #[role(bais)]\n    .v: Voltage\n"));
    assert!(
        message.contains("there is no role `bais` (did you mean `bias`?)"),
        "{message}"
    );
    assert!(message.contains("`supply_fault_current`"), "{message}");
}

#[test]
fn an_unknown_series_or_kind_is_refused_with_did_you_mean() {
    let message = refused(
        "#[series(E25)]\ngeneric G {\n    .r: Resistance\n    catalog g { .r }\n}\n",
    );
    assert!(
        message.contains("there is no E-series `E25` (did you mean `E24`?)"),
        "{message}"
    );
    let message = refused(&part("#[kind(resistr)]", ""));
    assert!(
        message.contains("there is no kind `resistr` (did you mean `resistor`?)"),
        "{message}"
    );
    let message = refused(&part("#[assembly(hnd)]", ""));
    assert!(
        message.contains("there is no assembly `hnd` (did you mean `hand`?)"),
        "{message}"
    );
}

#[test]
fn an_attribute_in_the_wrong_place_is_refused() {
    let message = refused("#[bank]\nmodule M {\n}\n");
    assert!(
        message.contains("`#[bank]` belongs on a generic or a part; `M` is a module"),
        "{message}"
    );
    let message = refused("#[role(bias)]\ngeneric G {\n}\n");
    assert!(
        message.contains("`#[role(...)]` belongs on a member of a generic or a part"),
        "{message}"
    );
    let message = refused(&part("", "    #[bank]\n    .v: Voltage\n"));
    assert!(
        message.contains("`#[bank]` belongs on a generic or a part, above its definition, not on a member"),
        "{message}"
    );
    let message = refused("module M {\n    #[role(bias)]\n    .v: Voltage\n}\n");
    assert!(
        message.contains("`#[role(...)]` belongs on a member of a generic or a part; `M` is a module"),
        "{message}"
    );
    let message = refused("generic G {\n    .r: Resistance\n}\n#[series(E24)]\ngeneric H {\n}\n");
    assert!(
        message.contains("`#[series(...)]` gives values to the parts the solver makes for a catalog, and `H` has no `catalog` statement"),
        "{message}"
    );
}

#[test]
fn a_role_on_a_member_of_another_type_is_refused() {
    let src = format!(
        "{}board B {{\n    .t: Thing\n}}\n",
        part("", "    #[role(bias)]\n    .v: Current\n")
    );
    let message = refused(&src);
    assert!(
        message.contains("`#[role(bias)]` is for a member of type `Voltage`, and `.v` is not one"),
        "{message}"
    );
}

#[test]
fn an_attribute_written_twice_or_with_wrong_arguments_is_refused() {
    let message = refused(&part("#[kind(fuse)]\n#[kind(fuse)]", ""));
    assert!(
        message.contains("`#[kind]` is written twice on `Thing`; write it once"),
        "{message}"
    );
    let message = refused(&part("#[bank(3)]", ""));
    assert!(message.contains("expected a name or a text"), "{message}");
    let message = refused(&part("#[bank(\"x\")]", ""));
    assert!(
        message.contains("`#[bank]` takes no argument, or the name of the generic or part"),
        "{message}"
    );
    let message = refused(&part("#[bank(Nothing)]", ""));
    assert!(
        message.contains("`#[bank(Nothing)]` names the kind each part of the bank is, and `Nothing` is not a generic or a part this file defines or imports"),
        "{message}"
    );
    let message = refused(&part("#[value(LED)]", ""));
    assert!(message.contains("`#[value(...)]` takes one text"), "{message}");
}
