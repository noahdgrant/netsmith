//! The standard library is ordinary source, so it must parse and round-trip.

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const PASSIVE: &str = include_str!("../../../std/parts/passive.nsm");
const SOURCES: &str = include_str!("../../../std/sources.nsm");
const LOGIC: &str = include_str!("../../../std/logic.nsm");
const REGULATOR: &str = include_str!("../../../std/parts/regulator.nsm");
const STACKUP: &str = include_str!("../../../std/mfg/stackup.nsm");
const JLCPCB: &str = include_str!("../../../std/mfg/jlcpcb.nsm");
const DIODE: &str = include_str!("../../../std/parts/diode.nsm");
const TRANSISTOR: &str = include_str!("../../../std/parts/transistor.nsm");
const MECHANICAL: &str = include_str!("../../../std/parts/mechanical.nsm");
const CONNECTOR: &str = include_str!("../../../std/parts/connector.nsm");
const BUS: &str = include_str!("../../../std/bus.nsm");

fn def<'a>(file: &'a File, name: &str) -> &'a Definition {
    file.definitions
        .iter()
        .find(|d| d.name.name == name)
        .unwrap_or_else(|| panic!("no {name}"))
}

fn message(src: &str) -> String {
    parse(src).unwrap_err().message
}

#[test]
fn passive_parts_parse() {
    let file = parse(PASSIVE).unwrap();
    // Eight parts and the enum `Dielectric` of the capacitor (ADR 0055). Seven of the parts
    // are generics that are their own first provider (ADR 0059 stage 2); `Switch` has no
    // catalog and stays a `part`.
    assert_eq!(file.definitions.len(), 9);
    assert_eq!(
        file.definitions
            .iter()
            .filter(|d| matches!(d.kind, DefKind::Part | DefKind::Generic))
            .count(),
        8
    );
    assert_eq!(
        file.definitions
            .iter()
            .filter(|d| d.kind == DefKind::Generic)
            .count(),
        7
    );
    let dielectric = def(&file, "Dielectric");
    assert_eq!(dielectric.kind, DefKind::Enum);
    let variants: Vec<&str> = dielectric
        .variants
        .iter()
        .map(|v| v.name.name.as_str())
        .collect();
    assert_eq!(variants, ["C0G", "X5R", "X7R"]);

    let r = def(&file, "Resistor");
    let bridges: Vec<_> = r
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Bridge(b) => Some(b),
            _ => None,
        })
        .collect();
    assert_eq!(bridges.len(), 1);
    assert_eq!(
        (
            bridges[0].from.to_source().as_str(),
            bridges[0].to.to_source().as_str()
        ),
        (".a", ".b")
    );

    let catalog = r.items.iter().find_map(|i| match i {
        Item::Catalog(c) => Some(c),
        _ => None,
    });
    let catalog = catalog.expect("resistor has a catalog");
    assert_eq!(catalog.name.name, "resistor");
    assert_eq!(
        catalog
            .fields
            .iter()
            .map(Path::to_source)
            .collect::<Vec<_>>(),
        [
            ".resistance",
            ".max_power",
            ".package",
            ".temperature_coefficient"
        ]
    );

    let led = def(&file, "Led");
    assert!(
        led.items
            .iter()
            .any(|i| matches!(i, Item::Bridge(b) if b.from.to_source() == ".anode"))
    );
    let led_catalog = led.items.iter().find_map(|i| match i {
        Item::Catalog(c) => Some(c),
        _ => None,
    });
    let led_fields: Vec<_> = led_catalog
        .expect("an LED has a catalog")
        .fields
        .iter()
        .map(Path::to_source)
        .collect();
    assert_eq!(led_fields, [".color", ".forward_voltage", ".package"]);

    let cap = def(&file, "Capacitor");
    assert!(cap.items.iter().any(|i| matches!(i, Item::Bridge(_))));
}

#[test]
fn sources_parse() {
    let file = parse(SOURCES).unwrap();
    assert_eq!(file.definitions.len(), 1);
    let rail = def(&file, "Rail");
    assert_eq!(rail.kind, DefKind::Interface);
    assert_eq!(
        rail.items.len(),
        6,
        "pos, neg, the voltage between them, the current drawn through pos, the peak voltage, and the narrowest track"
    );
}

#[test]
fn logic_is_an_interface_that_carries_its_reference_rail() {
    let file = parse(LOGIC).unwrap();
    assert_eq!(file.imports.len(), 1);
    let logic = def(&file, "Logic");
    assert_eq!(logic.kind, DefKind::Interface);
    let fields: Vec<_> = logic
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Decl(d) => Some((d.name.to_source(), d.ty.segments[0].name.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        fields,
        [
            (".line".to_string(), "Pin".to_string()),
            (".reference".to_string(), "Rail".to_string()),
            (".vih".to_string(), "Voltage".to_string()),
            (".vil".to_string(), "Voltage".to_string()),
            (".voh".to_string(), "Voltage".to_string()),
            (".vol".to_string(), "Voltage".to_string()),
        ]
    );
}

#[test]
fn library_files_round_trip_through_the_printer() {
    for src in [
        PASSIVE, SOURCES, LOGIC, REGULATOR, STACKUP, JLCPCB, DIODE, TRANSISTOR, MECHANICAL,
        CONNECTOR, BUS,
    ] {
        let once = parse(src).unwrap().to_source();
        assert_eq!(parse(&once).unwrap().to_source(), once);
    }
}

#[test]
fn every_definition_kind_parses() {
    let src =
        "part P {}\ninterface Power {\n    .hv: Pin\n}\nmodule C {}\nboard D {}\nsystem E {}\n";
    let file = parse(src).unwrap();
    let kinds: Vec<_> = file.definitions.iter().map(|d| d.kind).collect();
    assert_eq!(
        kinds,
        [
            DefKind::Part,
            DefKind::Interface,
            DefKind::Module,
            DefKind::Board,
            DefKind::System
        ]
    );
    assert_eq!(
        file.to_source(),
        "part P {}\n\ninterface Power {\n    .hv: Pin\n}\n\nmodule C {}\n\nboard D {}\n\nsystem E {}\n"
    );
}

#[test]
fn bridge_needs_two_terminals_joined_by_an_arrow() {
    assert!(message("part P {\n    bridge .a .b\n}\n").contains("expected `~>`"));
    assert!(message("part P {\n    bridge .a ~> \n}\n").contains("expected a name"));
}

#[test]
fn catalog_lists_fields_with_commas() {
    assert!(message("part P {\n    catalog c { .a .b }\n}\n").contains("expected `,` or `}`"));
    let ok = parse("part P {\n    catalog c {\n        .a,\n        .b,\n    }\n}\n").unwrap();
    let Item::Catalog(c) = &ok.definitions[0].items[0] else {
        panic!()
    };
    assert_eq!(c.fields.len(), 2);
}

#[test]
fn bridge_and_catalog_are_only_keywords_before_a_name() {
    // A leading dot makes a field of the same spelling an ordinary name.
    let file = parse("part P {\n    .bridge: Pin\n    .catalog: Pin\n}\n").unwrap();
    assert!(
        file.definitions[0]
            .items
            .iter()
            .all(|i| matches!(i, Item::Decl(_)))
    );
}

#[test]
fn a_regulator_has_two_rails_a_headroom_requirement_and_a_catalog() {
    let file = parse(REGULATOR).unwrap();
    let reg = def(&file, "LinearRegulator");
    let rails: Vec<_> = reg
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Decl(d) if d.ty.segments[0].name == "Rail" => Some(d.name.to_source()),
            _ => None,
        })
        .collect();
    assert_eq!(rails, [".vin", ".vout"]);
    assert!(
        reg.items
            .iter()
            .any(|i| matches!(i, Item::Constraint(c) if c.op == CmpOp::Ge))
    );
    assert!(reg.items.iter().any(|i| matches!(i, Item::Connect(_))));
    assert!(
        reg.items
            .iter()
            .any(|i| matches!(i, Item::Catalog(c) if c.name.name == "ldo"))
    );
}

#[test]
fn a_manufacturing_process_is_an_interface_of_quantities_with_no_pins() {
    let file = parse(JLCPCB).unwrap();
    let two_layer = def(&file, "TwoLayer");
    assert_eq!(two_layer.kind, DefKind::Interface);
    let pins = two_layer.items.iter().any(|i| match i {
        Item::Decl(d) => d.ty.segments[0].name == "Pin",
        _ => false,
    });
    assert!(!pins, "a process has no pins");
    assert_eq!(
        def(&parse(STACKUP).unwrap(), "Stackup").kind,
        DefKind::Interface
    );
}
