//! The `dc` statement and the `at` of a level (ADR 0076, amendment 1): what the parser reads,
//! what it prints back, and what it refuses.

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const PART: &str = "\
part Fuse1 {
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
    .resistance: Resistance
    bridge .a ~> .b
    dc resistor .a .b .resistance
}
";

fn err(src: &str) -> (String, usize, usize) {
    let e = parse(src).unwrap_err();
    let (line, col) = e.line_col(src);
    (e.message, line, col)
}

fn dcs(src: &str) -> Vec<Dc> {
    let file = parse(src).unwrap();
    file.definitions[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Dc(d) => Some(d.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_dc_statement_names_an_element_and_its_operands() {
    let found = dcs(PART);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind.name, "resistor");
    let operands: Vec<String> = found[0].operands.iter().map(Path::to_source).collect();
    assert_eq!(operands, [".a", ".b", ".resistance"]);
}

#[test]
fn every_element_kind_parses_with_its_own_number_of_operands() {
    for (text, n) in [
        ("dc diode .anode .cathode .forward_voltage", 3),
        (
            "dc nmos .gate .drain .source .gate_threshold .on_resistance",
            5,
        ),
        (
            "dc pmos .gate .drain .source .gate_threshold .on_resistance",
            5,
        ),
    ] {
        let found = dcs(&format!("part P {{\n    {text}\n}}\n"));
        assert_eq!(found[0].operands.len(), n, "{text}");
    }
}

#[test]
fn the_parser_leaves_the_kind_and_the_count_to_the_checker() {
    // The checker knows each element; the parser reads a word and the paths after it.
    let found = dcs("part P {\n    dc coil .a\n    dc resistor\n}\n");
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].operands.len(), 1);
    assert!(found[1].operands.is_empty());
}

#[test]
fn a_dc_statement_prints_back_as_written() {
    let once = parse(PART).unwrap().to_source();
    assert!(
        once.contains("    dc resistor .a .b .resistance\n"),
        "{once}"
    );
    assert_eq!(parse(&once).unwrap().to_source(), once);
}

#[test]
fn a_member_may_still_be_called_dc() {
    let file = parse("module M {\n    .dc: Current\n    .dc &= 1mA\n    .dc <= 2mA\n}\n").unwrap();
    let def = &file.definitions[0];
    assert!(def.items.iter().all(|i| !matches!(i, Item::Dc(_))));
}

#[test]
fn a_dc_statement_takes_no_doc_comment() {
    let (message, line, _) = err("part P {\n    /// An element.\n    dc resistor .a .b .r\n}\n");
    assert_eq!(message, "doc comments can only be attached to declarations");
    assert_eq!(line, 2);
}

#[test]
fn dc_needs_an_element_after_it() {
    // `dc` is a keyword only before a word; before anything else it is a name, as it was.
    let (message, _, _) = err("part P {\n    dc 3\n}\n");
    assert!(message.contains("found `3`"), "{message}");
}

fn constraint(src: &str) -> Constraint {
    let file = parse(&format!("part P {{\n    {src}\n}}\n")).unwrap();
    let def = &file.definitions[0];
    let Item::Constraint(c) = &def.items[0] else {
        panic!()
    };
    c.clone()
}

#[test]
fn a_level_may_state_the_current_it_is_measured_at() {
    let c = constraint(".voh &= .vdd.voltage - 0.4V at 2mA");
    assert_eq!(c.rhs.to_source(), ".vdd.voltage - 0.4V");
    assert_eq!(c.at.as_ref().unwrap().to_source(), "2mA");
}

#[test]
fn a_statement_without_at_has_none() {
    assert!(constraint(".voh &= 2.4V").at.is_none());
}

#[test]
fn at_is_read_after_any_comparison_and_a_range() {
    for op in ["&=", "?=", "==", ">=", "<="] {
        let c = constraint(&format!(".vol {op} 0.1V..0.4V at 8mA"));
        assert_eq!(c.at.as_ref().unwrap().to_source(), "8mA", "{op}");
        assert_eq!(c.rhs.to_source(), "0.1V..0.4V");
    }
}

#[test]
fn at_prints_back_after_the_value() {
    let src = "part P {\n    .out.voh &= .vdd.voltage - 0.4V at 2mA\n}\n";
    assert_eq!(parse(src).unwrap().to_source(), src);
}

#[test]
fn a_member_may_still_be_called_at() {
    let c = constraint(".x &= .at");
    assert!(c.at.is_none());
    assert_eq!(c.rhs.to_source(), ".at");
}

#[test]
fn at_needs_a_value_after_it() {
    let (message, _, _) = err("part P {\n    .voh &= 2.4V at\n}\n");
    assert!(message.contains("a value"), "{message}");
}
