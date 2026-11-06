//! `generic` definitions and `provides` (ADR 0059).

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const COAX: &str = "\
/// A coaxial receptacle.
generic CoaxReceptacle {
    .designator &= \"J\"

    pins {
        .signal passive required
        .shield passive
        .detect input optional
    }

    .impedance: Resistance
    .impedance &= 50ohm
    .max_frequency: Frequency
    .mpn: String
    .max_frequency >= 1GHz
}
";

fn only(src: &str) -> Definition {
    parse(src).unwrap().definitions.remove(0)
}

fn err(src: &str) -> String {
    parse(src).unwrap_err().message
}

#[test]
fn a_generic_is_a_definition_of_its_own_kind() {
    let def = only(COAX);
    assert_eq!(def.kind, DefKind::Generic);
    assert_eq!(def.kind.keyword(), "generic");
    assert_eq!(def.name.name, "CoaxReceptacle");
    assert!(def.provides.is_empty());
    assert_eq!(def.docs, ["A coaxial receptacle."]);
}

#[test]
fn a_generic_holds_every_statement_of_a_part() {
    let def = only(COAX);
    let kinds: Vec<&str> = def
        .items
        .iter()
        .map(|i| match i {
            Item::Constraint(_) => "constraint",
            Item::Decl(_) => "decl",
            Item::Pins(_) => "pins",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "constraint",
            "pins",
            "decl",
            "constraint",
            "decl",
            "decl",
            "constraint"
        ]
    );
}

#[test]
fn a_generic_pin_row_has_a_kind_and_no_name_and_no_pads() {
    let def = only(COAX);
    let Some(Item::Pins(pins)) = def.items.iter().find(|i| matches!(i, Item::Pins(_))) else {
        panic!("no pin table");
    };
    assert_eq!(pins.rows.len(), 3);
    assert!(pins.rows.iter().all(|r| r.is_contract()));
    assert_eq!(pins.rows[0].pin.to_source(), ".signal");
    assert_eq!(pins.rows[0].kind, PinKind::Passive);
    assert_eq!(
        pins.rows[0].requirement.map(|r| r.0),
        Some(Requirement::Required)
    );
    assert_eq!(pins.rows[1].requirement, None);
    assert_eq!(pins.rows[2].kind, PinKind::Input);
    assert_eq!(
        pins.rows[2].requirement.map(|r| r.0),
        Some(Requirement::Optional)
    );
}

#[test]
fn a_generic_may_also_have_rows_with_a_name_and_pads() {
    let def = only("generic G {\n pins {\n  .a passive \"A\" \"1\"\n  .b passive\n }\n}\n");
    let Item::Pins(pins) = &def.items[0] else {
        panic!()
    };
    assert!(!pins.rows[0].is_contract());
    assert_eq!(pins.rows[0].name, "A");
    assert!(pins.rows[1].is_contract());
}

#[test]
fn a_part_still_needs_a_name_and_pads_in_every_row() {
    let message = err("part P {\n pins {\n  .a passive\n }\n}\n");
    assert!(message.contains("datasheet name of `.a`"), "{message}");
}

#[test]
fn a_part_provides_one_generic_or_several() {
    let one = only("part AcmeMicroCoax provides CoaxReceptacle {}\n");
    assert_eq!(one.kind, DefKind::Part);
    assert_eq!(one.provides.len(), 1);
    assert_eq!(one.provides[0].name, "CoaxReceptacle");

    let two = only("part AcmeMicroCoax provides CoaxReceptacle, RfConnector {\n .x: Pin\n}\n");
    let names: Vec<&str> = two.provides.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["CoaxReceptacle", "RfConnector"]);
    assert_eq!(two.items.len(), 1);
}

#[test]
fn a_module_provides_a_generic() {
    let def = only("module Zbk9Buck provides BuckConverter {\n .controller: Zbk9\n}\n");
    assert_eq!(def.kind, DefKind::Module);
    assert_eq!(def.provides[0].name, "BuckConverter");
}

#[test]
fn the_list_of_generics_may_wrap_after_a_comma() {
    let def = only("part P provides A,\n  B,\n  C {}\n");
    assert_eq!(def.provides.len(), 3);
}

#[test]
fn only_a_part_or_a_module_provides() {
    for kind in ["board", "system", "panel", "interface", "enum", "generic"] {
        let message = err(&format!("{kind} X provides G {{}}\n"));
        assert!(
            message.contains("only a part or a module provides a generic"),
            "{kind}: {message}"
        );
        assert!(message.contains(&format!("a {kind} does not")), "{message}");
    }
}

#[test]
fn provides_wants_a_name() {
    let message = err("part P provides {}\n");
    assert!(
        message.contains("the name of a generic after `provides`"),
        "{message}"
    );
}

#[test]
fn a_block_after_provides_is_asked_for_by_name() {
    let message = err("part P provides G\n");
    assert!(
        message.contains("after the generics the part provides"),
        "{message}"
    );
}

#[test]
fn the_error_for_a_stray_word_at_the_top_lists_generic() {
    let message = err("trait X {}\n");
    assert!(message.contains("`generic`"), "{message}");
}

#[test]
fn generic_and_provides_are_ordinary_names_inside_a_body() {
    let file = parse(
        "part P {\n .generic: Number\n .provides: Number\n .generic &= 1\n .provides == .generic\n}\n",
    )
    .unwrap();
    assert_eq!(file.definitions[0].items.len(), 4);
    // And as the name of a definition.
    let file = parse("module generic {}\nmodule provides {}\n").unwrap();
    assert_eq!(file.definitions.len(), 2);
}

#[test]
fn a_generic_and_its_providers_print_back_to_what_they_were() {
    let src = format!(
        "{COAX}\npart AcmeMicroCoax provides CoaxReceptacle, RfConnector {{\n    .max_frequency &= 6GHz\n}}\n\nmodule Zbk9Buck provides BuckConverter {{\n    .vin_abs_max &= 65V\n}}\n"
    );
    let file = parse(&src).unwrap();
    let printed = file.to_source();
    let again = parse(&printed).unwrap();
    assert_eq!(again.to_source(), printed);
    assert!(printed.contains("generic CoaxReceptacle {"), "{printed}");
    assert!(printed.contains(".signal passive required\n"), "{printed}");
    assert!(printed.contains(".shield passive\n"), "{printed}");
    assert!(
        printed.contains("part AcmeMicroCoax provides CoaxReceptacle, RfConnector {"),
        "{printed}"
    );
    assert!(
        printed.contains("module Zbk9Buck provides BuckConverter {"),
        "{printed}"
    );
}
