//! The standard library is ordinary source, so it must parse and round-trip.

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const PASSIVE: &str = include_str!("../../../std/parts/passive.nsm");
const SOURCES: &str = include_str!("../../../std/sources.nsm");
const LOGIC: &str = include_str!("../../../std/logic.nsm");
const REGULATOR: &str = include_str!("../../../std/parts/regulator.nsm");
const STACKUP: &str = include_str!("../../../std/mfg/stackup.nsm");
const JLCPCB: &str = include_str!("../../../std/mfg/jlcpcb.nsm");
const ACME: &str = include_str!("../../../std/mfg/acme.nsm");
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
    // Ten parts, the enum `Dielectric` of the capacitor (ADR 0055) and the enum `Color` of the
    // LED (ADR 0118). Nine of the parts are generics that are their own first provider (ADR 0068
    // stage 2), `CapacitorBank` (ADR 0123) and `NtcThermistor` (#473) among them; `Switch` has
    // no catalog and stays a `part`.
    assert_eq!(file.definitions.len(), 12);
    assert_eq!(
        file.definitions
            .iter()
            .filter(|d| matches!(d.kind, DefKind::Part | DefKind::Generic))
            .count(),
        10
    );
    assert_eq!(
        file.definitions
            .iter()
            .filter(|d| d.kind == DefKind::Generic)
            .count(),
        9
    );
    let dielectric = def(&file, "Dielectric");
    assert_eq!(dielectric.kind, DefKind::Enum);
    let variants: Vec<&str> = dielectric
        .variants
        .iter()
        .map(|v| v.name.name.as_str())
        .collect();
    assert_eq!(variants, ["C0G", "X5R", "X7R"]);
    let color = def(&file, "Color");
    assert_eq!(color.kind, DefKind::Enum);
    let variants: Vec<&str> = color
        .variants
        .iter()
        .map(|v| v.name.name.as_str())
        .collect();
    assert_eq!(variants, ["Red", "Green", "Blue", "White", "Yellow"]);

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
    assert_eq!(led_fields, [".color", ".forward_voltage", ".package", ".max_forward_current", ".test_current"]);

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
        12,
        "pos, neg, the voltage between them, the current drawn through pos, the peak voltage, the reverse peak, the peak current, the most the source gives at an instant and the check of the one against the other, the narrowest track, the default ground, and the isolated mark"
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
            (".swing".to_string(), "Voltage".to_string()),
            (".vih".to_string(), "Voltage".to_string()),
            (".vil".to_string(), "Voltage".to_string()),
            (".voh".to_string(), "Voltage".to_string()),
            (".vol".to_string(), "Voltage".to_string()),
            (".ii".to_string(), "Current".to_string()),
            (".roh".to_string(), "Resistance".to_string()),
            (".rol".to_string(), "Resistance".to_string()),
        ]
    );
}

#[test]
fn the_fictional_fabricator_has_an_eight_and_a_ten_layer_process() {
    let file = parse(ACME).unwrap();
    let names: Vec<&str> = file
        .definitions
        .iter()
        .map(|d| d.name.name.as_str())
        .collect();
    assert_eq!(names, ["EightLayer", "TenLayer"]);
    assert!(
        file.definitions
            .iter()
            .all(|d| d.kind == DefKind::Interface)
    );
}

#[test]
fn library_files_round_trip_through_the_printer() {
    for src in [
        PASSIVE, SOURCES, LOGIC, REGULATOR, STACKUP, JLCPCB, ACME, DIODE, TRANSISTOR, MECHANICAL,
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
    // `TwoLayer` is a generic whose providers are interfaces, one per copper weight (ADR 0113).
    let two_layer = def(&file, "TwoLayer");
    assert_eq!(two_layer.kind, DefKind::Generic);
    for weight in ["JlcTwoLayer1oz", "JlcTwoLayer2oz"] {
        let provider = def(&file, weight);
        assert_eq!(provider.kind, DefKind::Interface);
        assert_eq!(provider.provides[0].name, "TwoLayer");
    }
    // `FourLayer` and `SixLayer` are generics too, one provider for each build-up of
    // JLCPCB's impedance page (ADR 0113).
    for process in ["FourLayer", "SixLayer"] {
        assert_eq!(def(&file, process).kind, DefKind::Generic);
    }
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

/// The providers of a multilayer process, from the text of `std/mfg/jlcpcb.nsm`: each one's
/// name and its `&=` lines (field, value), in order.
fn providers_of(process: &str) -> Vec<(String, Vec<(String, String)>)> {
    let header = format!(" provides {process} {{");
    let mut found = Vec::new();
    let mut lines = JLCPCB.lines();
    while let Some(line) = lines.next() {
        let Some(name) = line
            .strip_prefix("interface ")
            .and_then(|l| l.strip_suffix(header.as_str()))
        else {
            continue;
        };
        let mut fields = Vec::new();
        for line in lines.by_ref().take_while(|l| *l != "}") {
            let (field, value) = line
                .trim()
                .split_once(" &= ")
                .unwrap_or_else(|| panic!("{name}: `{line}` is not a `&=` line"));
            fields.push((field.to_string(), value.to_string()));
        }
        found.push((name.to_string(), fields));
    }
    found
}

fn mm(value: &str) -> f64 {
    value
        .strip_suffix("mm")
        .unwrap_or_else(|| panic!("{value} is not a length"))
        .parse()
        .unwrap()
}

/// Every provider of `FourLayer` and `SixLayer` states a whole build-up from JLCPCB's impedance
/// page: a name that says the layer count and the thickness, an inner copper, one dielectric
/// layer between each two copper layers (a core, then a prepreg, from the outer prepreg
/// inwards), the permittivity of the page for its glass, and layers that add up to within the
/// fabricator's tolerance of the thickness (`netsmith_pcb::thickness_tolerance`: 10% from 1 mm,
/// else 0.1 mm). The first provider is the 1.6 mm board that a board saying nothing gets, and
/// each thickness the process offers has one.
#[test]
fn every_multilayer_provider_states_a_whole_build_up_that_adds_up() {
    const EPSILON: [(&str, &str); 4] = [
        ("7628", "4.4"),
        ("3313", "4.1"),
        ("1080", "3.91"),
        ("2116", "4.16"),
    ];
    for (process, layers, first) in [
        ("FourLayer", 4, "JlcFourLayerAA04161H7628"),
        ("SixLayer", 6, "JlcSixLayerAA06161H3313"),
    ] {
        let mut providers = providers_of(process);
        providers.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(providers[0].0, first, "{process}: the first provider");
        let mut thicknesses: Vec<String> = Vec::new();
        for (name, fields) in &providers {
            let get = |field: &str| -> &str {
                fields
                    .iter()
                    .find(|(f, _)| f == field)
                    .unwrap_or_else(|| panic!("{name} states no {field}"))
                    .1
                    .as_str()
            };
            let thickness = get(".stackup.thickness");
            let build = get(".stackup.build").trim_matches('"');
            // `JLC04121H-3313`: four layers, 1.2 mm, 1 oz outside and half an ounce inside.
            let code = build.strip_prefix("JLC").expect("a JLCPCB build-up");
            let (head, glass) = code.split_once('-').expect("a build-up has a glass");
            assert_eq!(head[..2], format!("{layers:02}"), "{name}: layers");
            assert_eq!(&head[4..], "1H", "{name}: 1 oz outside, 0.5 oz inside");
            let nominal = format!("{:.1}", mm(thickness));
            assert_eq!(head[2..4], nominal.replace('.', ""), "{name}: thickness");
            // `JlcFourLayerDB04121H3313`: the order of the names is the order the providers
            // are tried in (ADR 0068), so two letters put the 1.6 mm default first, and the
            // page's default of each thickness first among its own.
            let group = match thickness {
                "1.6mm" => 'A',
                "0.8mm" => 'B',
                "1.0mm" => 'C',
                "1.2mm" => 'D',
                "2.0mm" => 'E',
                other => panic!("{name}: {other} is not a thickness the page offers"),
            };
            let word = &process[..process.len() - 5];
            assert!(
                name.starts_with(&format!("Jlc{word}Layer{group}"))
                    && name.ends_with(&format!("{head}{glass}")),
                "{name}: not named for {build} at {thickness}"
            );
            let rank = name.as_bytes()[format!("Jlc{word}Layer{group}").len()] as char;
            assert!(rank == 'A' || rank == 'B', "{name}: rank {rank}");
            if !thicknesses.contains(&thickness.to_string()) {
                thicknesses.push(thickness.to_string());
            }
            let inner = mm(get(".stackup.inner_copper"));
            assert_eq!(inner, 0.0152, "{name}: inner copper");
            let mut sum = 2.0 * 0.035 + (layers - 2) as f64 * inner;
            for n in 1..layers {
                let kind = get(&format!(".stackup.dielectric{n}.kind"));
                // Prepreg outside, a core and a prepreg between each two inner layer pairs.
                let want = if n % 2 == 1 { "\"prepreg\"" } else { "\"core\"" };
                assert_eq!(kind, want, "{name}: dielectric{n}");
                let epsilon = get(&format!(".stackup.dielectric{n}.epsilon"));
                if n % 2 == 1 {
                    let glass = get(&format!(".stackup.dielectric{n}.glass")).trim_matches('"');
                    let (_, e) = EPSILON
                        .iter()
                        .find(|(g, _)| *g == glass)
                        .unwrap_or_else(|| panic!("{name}: no permittivity for {glass}"));
                    assert_eq!(epsilon, *e, "{name}: dielectric{n}");
                } else {
                    assert_eq!(epsilon, "4.6", "{name}: dielectric{n}");
                }
                sum += mm(get(&format!(".stackup.dielectric{n}.thickness")));
            }
            assert!(
                fields
                    .iter()
                    .all(|(f, _)| !f.starts_with(&format!(".stackup.dielectric{layers}"))),
                "{name}: a {layers}-layer board has {} dielectric layers",
                layers - 1
            );
            let nominal = mm(thickness);
            let tolerance = if nominal >= 1.0 { 0.1 * nominal } else { 0.1 };
            assert!(
                (sum - nominal).abs() <= tolerance,
                "{name}: the layers add up to {sum} mm, more than {tolerance} mm from {nominal} mm"
            );
        }
        assert_eq!(
            thicknesses,
            ["1.6mm", "0.8mm", "1.0mm", "1.2mm", "2.0mm"],
            "{process}: every thickness the process offers has a provider, in name order"
        );
        // The first of each thickness is the page's default, and it is the only one.
        for group in ["A", "B", "C", "D", "E"] {
            let prefix = format!("Jlc{}Layer{group}", &process[..process.len() - 5]);
            let ranks: Vec<char> = providers
                .iter()
                .filter(|(n, _)| n.starts_with(&prefix))
                .map(|(n, _)| n.as_bytes()[prefix.len()] as char)
                .collect();
            assert_eq!(ranks[0], 'A', "{process}: the default of {group} is first");
            assert_eq!(ranks.iter().filter(|r| **r == 'A').count(), 1, "{process} {group}");
        }
    }
}
