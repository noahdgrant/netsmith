//! Lowering solved boards into the tool-independent design.

use netsmith_check::{Loader, check};
use netsmith_core::{Design, PartPin, PinKind};
use netsmith_lower::{Assembly, Lowered, lower, lower_system_board};
use netsmith_quantity::Quantity;
use netsmith_solve::{Candidate, Catalog, Catalogs, Library, Value, solve, solve_system};

const LED: &str = include_str!("../../../examples/led.nsm");
const STATUS: &str = include_str!("../../../examples/status_indicator.nsm");
const REGULATOR: &str = include_str!("../../../examples/linear_regulator.nsm");
const FOUR_LAYER: &str = include_str!("../../../examples/four_layer.nsm");

fn lowered_with_sourcing(src: &str) -> Result<Lowered, Vec<String>> {
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    lower(&c.boards[0], &s)
}

fn lowered(src: &str) -> Result<Design, Vec<String>> {
    lowered_with_sourcing(src).map(|l| l.design)
}

fn pins(d: &Design, net: &str) -> Vec<String> {
    let n = d
        .nets
        .iter()
        .find(|n| n.name == net)
        .unwrap_or_else(|| panic!("no net {net}"));
    n.pins
        .iter()
        .map(|p| format!("{}.{}", p.refdes, p.pin))
        .collect()
}

#[test]
fn the_led_board_becomes_a_header_two_parts_and_three_nets() {
    let d = lowered(LED).unwrap();
    assert_eq!(d.name, "LedExample");

    let parts: Vec<_> = d
        .parts
        .iter()
        .map(|p| {
            (
                p.refdes.as_str(),
                p.kind.as_str(),
                p.value.as_str(),
                p.package.as_deref(),
            )
        })
        .collect();
    // Numbered in path order: `led`, `power_in`, `r1`. The header's catalog row gives no
    // package, so the exporter will use its default, and its label is the symbol's name.
    assert_eq!(
        parts,
        [
            ("D1", "Led", "KT-0603R", Some("0603")),
            ("J1", "Header1x2", "Conn_01x02", None),
            ("R1", "Resistor", "330", Some("0402"))
        ]
    );

    let nets: Vec<_> = d.nets.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(nets, ["GND", "VCC", "net-led.anode"]);
    assert_eq!(pins(&d, "GND"), ["D1.1", "J1.2"]);
    assert_eq!(pins(&d, "VCC"), ["J1.1", "R1.1"]);
    assert_eq!(pins(&d, "net-led.anode"), ["D1.2", "R1.2"]);
}

#[test]
fn numbering_does_not_depend_on_declaration_order() {
    let reordered = "\
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board LedExample {
    .supply.neg ~ .power_in.p2
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .r1: Resistor {}
    .led: Led {
        .current &= 5mA..15mA
    }
    .supply: Rail {
        .neg.net &= \"GND\"
        .pos.net &= \"VCC\"
        .voltage &= 5V +/- 5%
    }
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
}
";
    // The notes say where in the source they are, and the reordered file has no doc comment:
    // they are compared on their own, in `doc_comments_and_relations_become_notes_in_source_order`.
    let (mut a, mut b) = (lowered(LED).unwrap(), lowered(reordered).unwrap());
    a.notes.clear();
    b.notes.clear();
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}

#[test]
fn the_status_indicator_numbers_each_kind_separately() {
    let d = lowered(STATUS).unwrap();
    let parts: Vec<_> = d
        .parts
        .iter()
        .map(|p| (p.refdes.as_str(), p.kind.as_str(), p.value.as_str()))
        .collect();
    assert_eq!(
        parts,
        [
            ("J1", "Header1x2", "Conn_01x02"),
            ("J2", "Header1x2", "Conn_01x02"),
            ("C1", "Capacitor", "100n"),
            ("R1", "Resistor", "1k"),
            ("D1", "Led", "LTST-C190KGKT")
        ]
    );

    // Four nets: the rail, ground, the enable signal, and the node between R1 and D1.
    assert_eq!(d.nets.len(), 4);
    let has = |pins: &[&str]| {
        d.nets.iter().any(|n| {
            n.pins
                .iter()
                .map(|p| format!("{}.{}", p.refdes, p.pin))
                .collect::<Vec<_>>()
                == pins
        })
    };
    assert!(has(&["C1.1", "J2.1"]), "the positive rail");
    assert!(has(&["C1.2", "D1.1", "J1.2", "J2.2"]), "ground");
    assert!(has(&["J1.1", "R1.1"]), "the enable signal");
    assert!(has(&["D1.2", "R1.2"]), "between the resistor and the LED");
}

#[test]
fn the_regulator_stage_lowers_with_the_pads_its_catalog_entry_supplies() {
    let d = lowered(REGULATOR).unwrap();
    let parts: Vec<_> = d
        .parts
        .iter()
        .map(|p| (p.refdes.as_str(), p.value.as_str(), p.package.as_deref()))
        .collect();
    assert_eq!(
        parts,
        [
            ("J1", "Conn_01x02", None),
            ("J2", "Conn_01x02", None),
            ("C1", "10u", Some("0402")),
            ("C2", "22u", Some("0603")),
            ("U1", "XC6206P332MR-G", Some("SOT-23"))
        ]
    );
    // Pad 1 is the regulator's ground, shared by its input and output, and listed once.
    assert_eq!(pins(&d, "VIN"), ["C1.1", "J1.1", "U1.3"]);
    assert_eq!(pins(&d, "3V3"), ["C2.1", "J2.1", "U1.2"]);
    assert_eq!(pins(&d, "GND"), ["C1.2", "C2.2", "J1.2", "J2.2", "U1.1"]);
}

#[test]
fn a_part_whose_pins_have_no_pads_is_reported_once() {
    let src = "\
part Bare {
    .a: Pin
    .b: Pin
}

board B {
    .p: Bare
    .p.a ~ .p.b
}
";
    let errors = lowered(src).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].contains("`p` is a `Bare`"), "{}", errors[0]);
    assert!(
        errors[0].contains("`a`") && errors[0].contains("`b`"),
        "{}",
        errors[0]
    );
}

#[test]
fn two_pins_of_one_part_on_the_same_pad_are_one_pin() {
    let src = "\
part Shared {
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"1\"
        .c passive \"C\" \"2\"
    }
    .a ~ .b
}

board B {
    .p: Shared
    .p.a ~ .p.c
}
";
    let d = lowered(src).unwrap();
    assert_eq!(d.nets.len(), 1);
    assert_eq!(pins(&d, &d.nets[0].name), ["U1.1", "U1.2"]);
}

#[test]
fn each_part_carries_the_sourcing_the_solver_picked() {
    let l = lowered_with_sourcing(LED).unwrap();
    assert_eq!(l.sourcing.len(), l.design.parts.len());
    let row: Vec<_> = l
        .sourcing
        .iter()
        .map(|s| {
            (
                s.refdes.as_str(),
                s.lcsc.as_deref(),
                s.mpn.as_deref(),
                s.library,
                s.assembled(),
            )
        })
        .collect();
    assert_eq!(
        row,
        [
            (
                "D1",
                Some("C2286"),
                Some("KT-0603R"),
                Some(Library::Basic),
                true
            ),
            // The `header` catalog rates the contacts (ADR 0040) and so picks a row, which a
            // header that is soldered by hand does not need and still carries.
            (
                "J1",
                Some("C42431798"),
                Some("PH2.54-1X2P-H25"),
                Some(Library::Extended),
                false
            ),
            (
                "R1",
                Some("C25104"),
                Some("0402WGF3300TCE"),
                Some(Library::Basic),
                true
            ),
        ]
    );
    // Same order, same words as the design.
    for (s, p) in l.sourcing.iter().zip(&l.design.parts) {
        assert_eq!(
            (&s.refdes, &s.kind, &s.value, &s.package),
            (&p.refdes, &p.kind, &p.value, &p.package)
        );
    }
}

#[test]
fn only_the_headers_are_left_for_hand_soldering() {
    for src in [LED, STATUS, REGULATOR] {
        for s in lowered_with_sourcing(src).unwrap().sourcing {
            assert_eq!(s.assembled(), !s.kind.starts_with("Header"), "{}", s.refdes);
            if s.assembled() {
                assert!(s.lcsc.is_some(), "{} has no LCSC number", s.refdes);
            }
        }
    }
}

/// What each part of a board with a resistor `r`, a header `j` and an LED `d` is assembled
/// by, when `body` is added to the board.
fn assembly_of(body: &str) -> Vec<(String, Assembly, bool)> {
    let src = format!(
        "import std::parts::connector::Header1x2\nimport std::parts::passive::{{Led, Resistor}}\n\
         board B {{\n    .d: Led\n    .j: Header1x2\n    .r: Resistor {{ .resistance &= 330ohm +/- 1% }}\n{body}}}\n"
    );
    lowered_with_sourcing(&src)
        .unwrap()
        .sourcing
        .iter()
        .map(|s| (s.refdes.clone(), s.assembly, s.assembly_stated))
        .collect()
}

#[test]
fn a_part_is_assembled_by_the_fab_unless_the_design_says_otherwise() {
    use Assembly::*;
    // The header is always through hole; the others have a catalog row and a number.
    assert_eq!(
        assembly_of(""),
        [
            ("D1".to_string(), Fab, false),
            ("J1".to_string(), Hand, false),
            ("R1".to_string(), Fab, false)
        ]
    );
    // The board narrows `.assembly` of any part it owns.
    assert_eq!(
        assembly_of(
            "    .r.assembly &= AssemblyMode::Hand\n    .j.assembly &= AssemblyMode::Fab\n    .d.assembly &= AssemblyMode::None\n"
        ),
        [
            ("D1".to_string(), None, true),
            ("J1".to_string(), Fab, true),
            ("R1".to_string(), Hand, true)
        ]
    );
}

#[test]
fn a_part_the_board_marks_consigned_is_placed_by_the_board_house_and_needs_no_number() {
    use Assembly::*;
    let src = "import std::parts::passive::Resistor\nboard B {\n    .r: Resistor { .resistance &= 330ohm +/- 1% }\n    .s: Resistor { .resistance &= 330ohm +/- 1%\n        .assembly &= AssemblyMode::Consigned\n    }\n}\n";
    let lowered = lowered_with_sourcing(src).unwrap();
    let by: Vec<_> = lowered
        .sourcing
        .iter()
        .map(|s| {
            (
                s.refdes.as_str(),
                s.assembly,
                s.assembled(),
                s.needs_part_number(),
            )
        })
        .collect();
    assert_eq!(
        by,
        [("R1", Fab, true, true), ("R2", Consigned, true, false)]
    );
}

#[test]
fn a_module_offers_consigned_through_its_assembly_mode() {
    let src = "import std::parts::passive::Resistor\n\
         module Pad {\n    .assembly_mode: AssemblyMode\n    .r: Resistor { .resistance &= 330ohm +/- 1% }\n    .r.assembly &= .assembly_mode\n}\n\
         board B {\n    .pad: Pad { .assembly_mode &= AssemblyMode::Consigned }\n}\n";
    let lowered = lowered_with_sourcing(src).unwrap();
    assert_eq!(lowered.sourcing[0].assembly, Assembly::Consigned);
    assert!(lowered.sourcing[0].assembly_stated);
}

#[test]
fn the_projects_default_may_be_consigned_and_the_word_is_the_one_of_the_catalog_column() {
    assert_eq!(Assembly::parse("consigned"), Some(Assembly::Consigned));
    assert_eq!(Assembly::Consigned.word(), "consigned");
    assert_eq!(
        Assembly::from_variant("Consigned"),
        Some(Assembly::Consigned)
    );
    assert_eq!(
        assembly_with_default("consigned", ""),
        [
            ("D1".to_string(), Assembly::Consigned, true),
            ("H1".to_string(), Assembly::None, true),
            ("J1".to_string(), Assembly::Consigned, true),
            ("R1".to_string(), Assembly::Consigned, true)
        ]
    );
}

/// [`assembly_of`] with the project's default (ADR 0028) and a mounting hole added (in a
/// module: a board's own hole is declared in its physical file).
fn assembly_with_default(default: &str, body: &str) -> Vec<(String, Assembly, bool)> {
    let src = format!(
        "import std::parts::connector::Header1x2\nimport std::parts::mechanical::MountingHole\n\
         import std::parts::passive::{{Led, Resistor}}\n\
         module Mech {{\n    .h: MountingHole\n}}\n\
         board B {{\n    .d: Led\n    .h: Mech\n    .j: Header1x2\n    .r: Resistor {{ .resistance &= 330ohm +/- 1% }}\n{body}}}\n"
    );
    let c = check("t.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(
        &c.boards[0],
        &Catalogs::jlcpcb().with_default_assembly(Some(default)),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    lower(&c.boards[0], &s)
        .unwrap()
        .sourcing
        .iter()
        .map(|s| (s.refdes.clone(), s.assembly, s.assembly_stated))
        .collect()
}

#[test]
fn the_projects_default_beats_the_built_in_rule_and_loses_to_everything_that_says_more() {
    use Assembly::*;
    // Built in: the header is soldered by hand. With `fab` as the project's default it is
    // the board house's, and the hole keeps what its definition says (`none`).
    assert_eq!(
        assembly_with_default("fab", ""),
        [
            ("D1".to_string(), Fab, true),
            ("H1".to_string(), None, true),
            ("J1".to_string(), Fab, true),
            ("R1".to_string(), Fab, true)
        ]
    );
    assert_eq!(
        assembly_with_default("hand", ""),
        [
            ("D1".to_string(), Hand, true),
            ("H1".to_string(), None, true),
            ("J1".to_string(), Hand, true),
            ("R1".to_string(), Hand, true)
        ]
    );
    // The design's own `.assembly &=` is the last word.
    assert_eq!(
        assembly_with_default("hand", "    .r.assembly &= AssemblyMode::Fab\n"),
        [
            ("D1".to_string(), Hand, true),
            ("H1".to_string(), None, true),
            ("J1".to_string(), Hand, true),
            ("R1".to_string(), Fab, true)
        ]
    );
}

#[test]
fn a_catalog_rows_assembly_beats_the_projects_default() {
    let mut c = Catalogs::empty();
    let ohms = Quantity::literal(330.0, Some("ohm")).unwrap().quantity;
    c.insert(Catalog {
        name: "resistor".into(),
        candidates: vec![Candidate {
            name: "330".into(),
            label: "330".into(),
            columns: vec![
                ("resistance".into(), Value::Quantity(ohms)),
                (
                    "max_power".into(),
                    Value::Quantity(Quantity::literal(62.5, Some("mW")).unwrap().quantity),
                ),
                ("package".into(), Value::Text("0402".into())),
            ],
            lcsc: Some("C1".into()),
            assembly: Some("fab".into()),
            ..Default::default()
        }],
    });
    let src = "import std::parts::passive::Resistor\nboard B {\n    .r: Resistor { .resistance &= 330ohm }\n}\n";
    let checked = check("t.nsm", src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let solved = solve(&checked.boards[0], &c.with_default_assembly(Some("hand")));
    assert!(solved.diagnostics.is_empty(), "{:?}", solved.diagnostics);
    let lowered = lower(&checked.boards[0], &solved).unwrap();
    assert_eq!(lowered.sourcing[0].assembly, Assembly::Fab);
}

#[test]
fn a_catalog_row_may_state_who_assembles_its_part_and_the_design_overrides_it() {
    let row = |assembly: Option<&str>| {
        let mut c = Catalogs::empty();
        let ohms = Quantity::literal(330.0, Some("ohm")).unwrap().quantity;
        let one_percent = Quantity::literal(1.0, Some("%")).unwrap().quantity;
        c.insert(Catalog {
            name: "resistor".into(),
            candidates: vec![Candidate {
                name: "330".into(),
                label: "330".into(),
                columns: vec![
                    (
                        "resistance".into(),
                        Value::Quantity(Quantity::tolerance(&ohms, &one_percent, true).unwrap()),
                    ),
                    (
                        "max_power".into(),
                        Value::Quantity(Quantity::literal(62.5, Some("mW")).unwrap().quantity),
                    ),
                    ("package".into(), Value::Text("0402".into())),
                ],
                lcsc: Some("C1".into()),
                mpn: None,
                library: None,
                assembly: assembly.map(str::to_string),
                height: None,
                generic: None,
            }],
        });
        c
    };
    let lower_with = |assembly: Option<&str>, narrowing: &str| {
        let src = format!(
            "import std::parts::passive::Resistor\nboard B {{\n    .r: Resistor {{ .resistance &= 330ohm +/- 1%; {narrowing} }}\n}}\n"
        );
        let c = check("t.nsm", &src, &Loader::std());
        assert!(c.is_ok(), "{}", c.render());
        let s = solve(&c.boards[0], &row(assembly));
        assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
        let l = lower(&c.boards[0], &s).unwrap();
        (l.sourcing[0].assembly, l.sourcing[0].assembly_stated)
    };
    assert_eq!(lower_with(None, ""), (Assembly::Fab, false));
    assert_eq!(lower_with(Some("hand"), ""), (Assembly::Hand, true));
    assert_eq!(
        lower_with(Some("hand"), ".assembly &= AssemblyMode::Fab"),
        (Assembly::Fab, true)
    );
}

#[test]
fn the_regulator_board_is_sourced_from_the_catalog() {
    let l = lowered_with_sourcing(REGULATOR).unwrap();
    let lcsc: Vec<_> = l
        .sourcing
        .iter()
        .map(|s| (s.refdes.as_str(), s.lcsc.as_deref()))
        .collect();
    assert_eq!(
        lcsc,
        [
            ("J1", Some("C42431798")),
            ("J2", Some("C42431798")),
            ("C1", Some("C315248")),
            ("C2", Some("C59461")),
            ("U1", Some("C5446"))
        ]
    );
}

#[test]
fn a_part_the_sample_catalog_picked_has_no_sourcing() {
    let c = check("t.nsm", REGULATOR, &Loader::std());
    let s = solve(&c.boards[0], &Catalogs::sample());
    let l = lower(&c.boards[0], &s).unwrap();
    let u1 = l.sourcing.iter().find(|s| s.refdes == "U1").unwrap();
    assert_eq!(
        (u1.lcsc.as_deref(), u1.mpn.as_deref(), u1.library),
        (None, None, None)
    );
    assert!(
        u1.assembled(),
        "the fab step refuses it, lowering does not hide it"
    );
}

/// A resistor on its own board, with `process` as given.
fn board_with(process: &str) -> String {
    format!(
        "import std::parts::passive::Resistor\nimport std::mfg::jlcpcb::TwoLayer\nimport std::sources::Rail\nboard B {{\n    {process}\n    .r: Resistor\n}}\n"
    )
}

#[test]
fn the_process_arrives_in_the_design_as_the_library_states_it() {
    let d = lowered(LED).unwrap();
    let p = d.process.expect("the LED board names a process");
    assert_eq!(
        (p.name.as_str(), p.fabricator.as_str()),
        ("TwoLayer", "JLCPCB")
    );
    assert_eq!(p.source, "https://jlcpcb.com/capabilities/pcb-capabilities");
    assert_eq!((p.min_track, p.min_clearance), (0.1, 0.1));
    assert_eq!(
        (p.min_drill, p.min_via_diameter, p.min_annular_ring),
        (0.15, 0.25, 0.18)
    );
    assert_eq!((p.min_hole_to_hole, p.min_edge_clearance), (0.45, 0.2));
    assert_eq!(
        (p.min_silk_width, p.min_text_height, p.min_silk_to_pad),
        (0.15, 1.0, 0.15)
    );
    assert_eq!(p.min_mask_bridge, 0.13);
    assert_eq!(p.stackup.layers, 2);
    assert_eq!(p.stackup.copper_weight, 1.0);
    assert_eq!(p.stackup.chosen_thickness(), Some(1.6));
}

#[test]
fn a_board_that_does_not_choose_a_thickness_leaves_the_whole_range() {
    let d = lowered(&board_with(".process: TwoLayer")).unwrap();
    let s = d.process.unwrap().stackup;
    assert_eq!(s.thickness, (0.4, 2.0));
    assert_eq!(s.chosen_thickness(), None);
}

#[test]
fn narrowing_a_rule_moves_the_bottom_of_its_range() {
    let d = lowered(&board_with(
        ".process: TwoLayer {\n        .min_track &= 0.15mm..0.3mm\n        .min_edge_clearance &= 0.5mm..1m\n    }",
    ))
    .unwrap();
    let p = d.process.unwrap();
    assert_eq!((p.min_track, p.min_edge_clearance), (0.15, 0.5));
    assert_eq!(
        p.min_clearance, 0.1,
        "what was not narrowed is the library's"
    );
}

#[test]
fn a_via_in_a_pad_is_what_the_process_offers_and_the_board_may_refuse_it() {
    // The library's ceiling for vias the fabricator fills and caps.
    let d = lowered(&board_with(".process: TwoLayer")).unwrap();
    assert_eq!(d.process.unwrap().via_in_pad, 0.55);
    // It is an upper bound, so the board narrows it down; `0mm` is no vias in pads.
    let d = lowered(&board_with(
        ".process: TwoLayer {\n        .via_in_pad &= 0mm\n    }",
    ))
    .unwrap();
    assert_eq!(d.process.unwrap().via_in_pad, 0.0);
    let d = lowered(&board_with(
        ".process: TwoLayer {\n        .via_in_pad &= 0mm..0.3mm\n    }",
    ))
    .unwrap();
    assert_eq!(d.process.unwrap().via_in_pad, 0.3);
}

#[test]
fn a_board_with_no_process_has_none() {
    let d = lowered(&board_with("")).unwrap();
    assert_eq!(d.process, None);
}

#[test]
fn a_process_that_is_not_one_is_refused_with_what_it_lacks() {
    let errors = lowered(&board_with(".process: Rail")).unwrap_err();
    let text = errors.join("\n");
    assert!(
        text.contains("`process` is a `Rail` and has no value for `min_track`"),
        "{text}"
    );
}

#[test]
fn a_four_layer_process_arrives_with_its_planes_and_build_up() {
    let p = lowered(FOUR_LAYER).unwrap().process.unwrap();
    assert_eq!(
        (p.name.as_str(), p.fabricator.as_str()),
        ("FourLayer", "JLCPCB")
    );
    // Multilayer numbers, from the capabilities page.
    assert_eq!((p.min_track, p.min_clearance), (0.09, 0.1));
    assert_eq!(p.min_annular_ring, 0.15);
    assert_eq!(p.stackup.layers, 4);
    assert_eq!(p.stackup.chosen_thickness(), Some(1.6));
    let inner = p
        .stackup
        .inner
        .expect("a four-layer board has inner layers");
    assert_eq!(inner.planes, ["GND", "3V3"]);
    // JLC04161H-7628, from the impedance page.
    assert_eq!(
        (inner.copper, inner.prepreg, inner.core),
        (0.0152, 0.2104, 1.065)
    );
    assert_eq!((inner.prepreg_epsilon, inner.core_epsilon), (4.4, 4.6));
    assert_eq!(inner.hole_clearance, 0.3);
}

#[test]
fn a_two_layer_process_has_no_inner_layers() {
    let p = lowered(LED).unwrap().process.unwrap();
    assert_eq!(p.stackup.inner, None);
}

#[test]
fn an_inner_layer_with_no_net_is_refused_and_told_how_to_name_it() {
    let source = FOUR_LAYER.replace("        .inner2.net &= \"3V3\"\n", "");
    let errors = lowered(&source).unwrap_err();
    let text = errors.join("\n");
    assert!(
        text.contains("`process.inner2` is an inner layer of a 4-layer board and has no net")
            && text.contains(".process.inner2.net &= \"GND\""),
        "{text}"
    );
    assert!(!text.contains("inner1"), "the named layer is fine: {text}");
}

#[test]
fn a_plane_on_a_net_the_board_does_not_have_names_it_and_the_nets_there_are() {
    let source = FOUR_LAYER.replace(".inner2.net &= \"3V3\"", ".inner2.net &= \"5V\"");
    let errors = lowered(&source).unwrap_err();
    let text = errors.join("\n");
    assert!(
        text.contains(
            "`process.inner2` is a plane on the net `5V`, but the board has no net of that name"
        ) && text.contains("`3V3`")
            && text.contains("`GND`")
            && text.contains("`VIN`"),
        "{text}"
    );
}

#[test]
fn each_board_of_a_system_is_lowered_as_if_alone() {
    let src = include_str!("../../../examples/two_boards.nsm");
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let system = c.system("TwoBoards").unwrap();
    let s = solve_system(system, &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);

    let designs: Vec<Design> = system
        .boards
        .iter()
        .map(|m| lower_system_board(system, m, &s).unwrap().design)
        .collect();
    let [power, led] = &designs[..] else {
        panic!("two boards")
    };
    // Named after the board, not its member name.
    assert_eq!(
        (power.name.as_str(), led.name.as_str()),
        ("PowerBoard", "LedBoard")
    );
    // Each has only its own parts, numbered from one, and its own nets: both have a `GND`
    // and a `3V3`, which are not the same net.
    assert_eq!(power.parts.len(), 5, "{:?}", power.parts);
    assert_eq!(led.parts.len(), 3, "{:?}", led.parts);
    // The socket comes between the LED and the resistor, in path order, and is `J1`.
    assert_eq!(pins(led, "3V3"), ["J1.1", "R1.1"]);
    assert_eq!(pins(led, "GND"), ["D1.1", "J1.2"]);
    assert_eq!(pins(power, "3V3"), ["C2.1", "J2.1", "U1.2"]);
    assert!(power.nets.iter().all(|n| n.name != "net-led.anode"));
    // The process is the board's own.
    assert!(power.process.is_some() && led.process.is_some());

    // The regulator board alone is the same design: the system added no parts and no nets.
    let alone = lowered(src).unwrap();
    assert_eq!(alone.name, "PowerBoard");
    assert_eq!(alone.parts, power.parts);
    assert_eq!(alone.nets, power.nets);
}

#[test]
fn a_board_of_a_system_can_be_four_layer_and_its_planes_are_checked_against_its_own_nets() {
    let src = include_str!("../../../examples/two_boards.nsm")
        .replace(
            "import std::mfg::jlcpcb::TwoLayer",
            "import std::mfg::jlcpcb::{TwoLayer, FourLayer}",
        )
        .replacen(
            ".process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }",
            ".process: FourLayer {\n        .inner1.net &= \"GND\"\n        .inner2.net &= \"3V3\"\n    }",
            1,
        );
    let c = check("t.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let system = c.system("TwoBoards").unwrap();
    let s = solve_system(system, &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    let designs: Vec<Design> = system
        .boards
        .iter()
        .map(|m| lower_system_board(system, m, &s).unwrap().design)
        .collect();
    let planes = |d: &Design| {
        d.process
            .as_ref()
            .unwrap()
            .stackup
            .inner
            .as_ref()
            .map(|i| i.planes.clone())
    };
    assert_eq!(
        planes(&designs[0]),
        Some(vec!["GND".to_string(), "3V3".to_string()])
    );
    assert_eq!(
        planes(&designs[1]),
        None,
        "the other board is still two-layer"
    );

    // A plane's net must be one of the nets of its own board, not of the system.
    let wrong = src.replace(".inner2.net &= \"3V3\"", ".inner2.net &= \"VCC\"");
    let c = check("t.nsm", &wrong, &Loader::std());
    let system = c.system("TwoBoards").unwrap();
    let s = solve_system(system, &Catalogs::jlcpcb());
    let errors = lower_system_board(system, &system.boards[0], &s).unwrap_err();
    assert!(
        errors.join("\n").contains("a plane on the net `VCC`"),
        "{errors:?}"
    );
}

// ---- pin tables (ADR 0016) -----------------------------------------------------------

/// A part with pins that have several pads, a chosen function and an alias. Invented for
/// these tests.
const MULTI: &str = "\
part Multi {
    .package: Package
    pins \"Q\" {
        .vdd.pos power         \"VDD\" \"1\" \"17\" \"32\"
        .vdd.neg power         \"VSS\" \"8\"
        .pa2     bidirectional \"PA2\" \"12\" fn GPIO UART1_TX
        .nc      no_connect    \"NC\"  \"5\"
    }
    .vdd: Rail
    .vdd.neg ~ .gnd
    .gnd: Pin
}

board B {
    .m: Multi { .package &= \"Q\" }
    .supply: Rail { .pos.net &= \"V\"
 .neg.net &= \"GND\" }
    .supply ~ .m.vdd
    .m.pa2.function &= \"UART1_TX\"
    .m.gnd.net &= \"GND\"
}
";

fn multi() -> Design {
    lowered(&format!("import std::sources::Rail\n{MULTI}")).unwrap()
}

#[test]
fn a_pin_with_several_pads_is_one_pin_ref_for_each_pad() {
    let d = multi();
    assert_eq!(pins(&d, "V"), ["U1.1", "U1.17", "U1.32"]);
}

#[test]
fn a_pin_joined_inside_the_part_to_one_with_pads_adds_no_pin_ref_of_its_own() {
    // `gnd` has no row; the part joins it to `vdd.neg`, which has pad 8.
    let d = multi();
    assert_eq!(pins(&d, "GND"), ["U1.8"]);
}

#[test]
fn the_design_carries_what_each_pin_is() {
    let d = multi();
    let pin = |name: &str| -> &PartPin {
        d.parts[0]
            .pins
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("no pin {name}"))
    };
    assert_eq!(
        d.parts[0]
            .pins
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["vdd.pos", "vdd.neg", "pa2", "nc"]
    );
    let vdd = pin("vdd.pos");
    assert_eq!(
        (
            vdd.datasheet.as_str(),
            vdd.kind,
            vdd.pads.clone(),
            vdd.function.clone()
        ),
        (
            "VDD",
            PinKind::Power,
            vec!["1".into(), "17".into(), "32".into()],
            None
        )
    );
    let pa2 = pin("pa2");
    assert_eq!(
        (pa2.kind, pa2.function.as_deref()),
        (PinKind::Bidirectional, Some("UART1_TX"))
    );
    // A pad on no net still belongs to its pin: the exporter marks it.
    assert_eq!(pin("nc").kind, PinKind::NoConnect);
    assert_eq!(pin("nc").pads, ["5"]);
}

#[test]
fn the_library_parts_carry_their_pin_tables() {
    let d = lowered(REGULATOR).unwrap();
    let u1 = d.parts.iter().find(|p| p.refdes == "U1").unwrap();
    let rows: Vec<_> = u1
        .pins
        .iter()
        .map(|p| (p.name.as_str(), p.datasheet.as_str(), p.pads.clone()))
        .collect();
    assert_eq!(
        rows,
        [
            ("vin.pos", "IN", vec!["3".to_string()]),
            ("vin.neg", "GND", vec!["1".to_string()]),
            ("vout.pos", "OUT", vec!["2".to_string()])
        ]
    );
    let r = lowered(LED).unwrap();
    let led = r.parts.iter().find(|p| p.refdes == "D1").unwrap();
    assert_eq!(
        led.pins
            .iter()
            .map(|p| (p.name.as_str(), p.datasheet.as_str(), p.pads[0].as_str()))
            .collect::<Vec<_>>(),
        [("anode", "A", "2"), ("cathode", "K", "1")]
    );
}

#[test]
fn a_part_whose_package_was_never_chosen_says_which_packages_it_has() {
    let src = MULTI.replace(".m: Multi { .package &= \"Q\" }", ".m: Multi");
    let errors = lowered(&format!("import std::sources::Rail\n{src}")).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(
        errors[0].contains("`m` is a `Multi` and no package is chosen, so its pads are not known; its pin tables are for `Q`"),
        "{}",
        errors[0]
    );
}

/// A module that offers who assembles its header (ADR 0042), and a board that uses it
/// twice with `body` added.
fn exposed_assembly(body: &str) -> String {
    format!(
        "import std::parts::connector::Header1x2\n\
         /// A header, with the choice of who solders it left to the board.\n\
         module HeaderBlock {{\n    .assembly_mode: AssemblyMode\n    .j: Header1x2\n    .j.assembly &= .assembly_mode\n}}\n\
         board B {{\n    .a: HeaderBlock\n    .b: HeaderBlock\n{body}}}\n"
    )
}

/// Who assembles each part of [`exposed_assembly`]'s board, with the project's default.
fn exposed_assembly_of(body: &str, default: Option<&str>) -> Vec<(String, Assembly, bool)> {
    let src = exposed_assembly(body);
    let c = check("t.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(
        &c.boards[0],
        &Catalogs::jlcpcb().with_default_assembly(default),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    assert!(
        s.deferred.is_empty(),
        "an offered value left alone is not a note: {:?}",
        s.deferred
    );
    lower(&c.boards[0], &s)
        .unwrap()
        .sourcing
        .iter()
        .map(|s| (s.refdes.clone(), s.assembly, s.assembly_stated))
        .collect()
}

#[test]
fn a_module_may_offer_who_assembles_its_part_and_the_board_chooses() {
    use Assembly::{Fab, Hand};
    let row = |r: &str, a, stated| (r.to_string(), a, stated);
    // Nothing chosen: the rules of ADR 0024 and 0028 decide, as for a part on the board.
    assert_eq!(
        exposed_assembly_of("", Option::None),
        [row("J1", Hand, false), row("J2", Hand, false)]
    );
    assert_eq!(
        exposed_assembly_of("", Some("fab")),
        [row("J1", Fab, true), row("J2", Fab, true)]
    );
    // The board's choice counts as the design's own `.assembly &=` on the part, so it
    // is stronger than the project's default, and each instance has its own.
    assert_eq!(
        exposed_assembly_of(
            "    .a.assembly_mode &= AssemblyMode::Fab\n    .b.assembly_mode &= AssemblyMode::None\n",
            Some("hand")
        ),
        [row("J1", Fab, true), row("J2", Assembly::None, true)]
    );
    // One instance chosen, the other left to the default.
    assert_eq!(
        exposed_assembly_of("    .b.assembly_mode &= AssemblyMode::Hand\n", Some("fab")),
        [row("J1", Fab, true), row("J2", Hand, true)]
    );
}

#[test]
fn a_module_passes_an_offered_assembly_on_through_another_module() {
    let src = "import std::parts::connector::Header1x2\n\
        module Inner {\n    .assembly_mode: AssemblyMode\n    .j: Header1x2\n    .j.assembly &= .assembly_mode\n}\n\
        module Outer {\n    .assembly_mode: AssemblyMode\n    .inner: Inner\n    .inner.assembly_mode &= .assembly_mode\n}\n\
        board B {\n    .o: Outer\n    .o.assembly_mode &= AssemblyMode::None\n}\n";
    let l = lowered_with_sourcing(src).unwrap();
    assert_eq!(l.sourcing[0].assembly, Assembly::None);
}

#[test]
fn a_board_cannot_reach_the_part_inside_a_module_to_set_who_assembles_it() {
    let src = exposed_assembly("    .a.j.assembly &= AssemblyMode::Hand\n");
    let c = check("t.nsm", &src, &Loader::std());
    assert!(!c.is_ok());
    let text = c.render();
    assert!(
        text.contains(
            "`j` is inside `HeaderBlock` and is not part of its interface; set it through a rail, a pin, or a value that `HeaderBlock` declares"
        ),
        "{text}"
    );
}

#[test]
fn an_offered_assembly_that_a_part_kind_forbids_is_a_contradiction_at_the_part() {
    // `Fiducial` states `.assembly &= AssemblyMode::None`, and an enum field narrows to one
    // variant.
    let src = "import std::parts::mechanical::Fiducial\n\
        module Mark {\n    .assembly_mode: AssemblyMode\n    .f: Fiducial\n    .f.assembly &= .assembly_mode\n}\n\
        board B {\n    .m: Mark\n    .m.assembly_mode &= AssemblyMode::Fab\n}\n";
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    let messages: Vec<_> = s.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "contradiction on `m.f.assembly`: it is already AssemblyMode::None, which does not overlap AssemblyMode::Fab"
        ]
    );
}

/// The designators of the parts of a board with a resistor `r`, a user part `buzzer` that
/// states its own letters, a user part `ic` that states none, and `body` added to the board.
fn designators_with(body: &str) -> Result<Vec<(String, String)>, Vec<String>> {
    let src = format!(
        "import std::parts::passive::Resistor\n\
         part Buzzer {{\n    .designator &= \"BZ\"\n    pins {{ .a passive \"A\" \"1\"; .b passive \"B\" \"2\" }}\n}}\n\
         part Chip {{\n    pins {{ .a passive \"A\" \"1\"; .b passive \"B\" \"2\" }}\n}}\n\
         board B {{\n    .r: Resistor {{ .resistance &= 330ohm +/- 1% }}\n    .buzzer: Buzzer\n    .ic: Chip\n{body}}}\n"
    );
    lowered(&src).map(|d| {
        d.parts
            .iter()
            .map(|p| (p.path.clone(), p.refdes.clone()))
            .collect()
    })
}

fn pair(path: &str, refdes: &str) -> (String, String) {
    (path.to_string(), refdes.to_string())
}

#[test]
fn a_part_takes_its_designator_letters_from_its_kind_and_a_part_with_none_is_u() {
    assert_eq!(
        designators_with("").unwrap(),
        [pair("buzzer", "BZ1"), pair("ic", "U1"), pair("r", "R1")]
    );
}

#[test]
fn the_board_narrows_the_letters_of_a_part_that_states_none() {
    assert_eq!(
        designators_with("    .ic.designator &= \"IC\"\n").unwrap(),
        [pair("buzzer", "BZ1"), pair("ic", "IC1"), pair("r", "R1")]
    );
}

#[test]
fn letters_a_kind_states_cannot_be_changed_by_the_board() {
    // The refusal is the checker's, before lowering: the kind's letters are one value.
    let src = "import std::parts::passive::Resistor\nboard B {\n    .r: Resistor { .designator &= \"Z\" }\n}\n";
    let c = check("t.nsm", src, &Loader::std());
    assert!(!c.is_ok());
    let text = c.render();
    assert!(
        text.contains(
            "contradiction on `r.designator`: it is already \"R\", which does not overlap \"Z\""
        ),
        "{text}"
    );
}

#[test]
fn letters_that_are_not_capital_letters_are_refused_naming_the_part() {
    for word in ["r1", "", "U 1", "7"] {
        let errors = designators_with(&format!("    .ic.designator &= \"{word}\"\n")).unwrap_err();
        assert_eq!(
            errors,
            [format!(
                "`ic` has `.designator` \"{word}\", which is not one or more capital letters such as \"R\" or \"IC\""
            )]
        );
    }
}

#[test]
fn designators_are_numbered_for_each_letter_in_path_order() {
    let d = designators_with("    .ic2: Chip\n    .ic.designator &= \"U\"\n").unwrap();
    assert_eq!(
        d,
        [
            pair("buzzer", "BZ1"),
            pair("ic", "U1"),
            pair("ic2", "U2"),
            pair("r", "R1")
        ]
    );
}

#[test]
fn a_number_at_the_end_of_a_path_segment_sorts_as_a_number() {
    let body: String = [10, 2, 1, 11, 9]
        .iter()
        .map(|n| format!("    .tp{n}: Chip\n"))
        .collect();
    let got = designators_with(&body).unwrap();
    // `.tp2` is `U3` (after `ic`, `tp1`), not `U6`; `tp10` follows `tp9`.
    let of = |path: &str| {
        got.iter()
            .find(|(p, _)| p == path)
            .map(|(_, r)| r.clone())
            .unwrap()
    };
    assert_eq!(
        [
            of("ic"),
            of("tp1"),
            of("tp2"),
            of("tp9"),
            of("tp10"),
            of("tp11")
        ],
        ["U1", "U2", "U3", "U4", "U5", "U6"]
    );
}

#[test]
fn natural_order_compares_segments_in_turn_and_is_total() {
    use netsmith_lower::natural_path_order as order;
    use std::cmp::Ordering::{Equal, Greater, Less};
    assert_eq!(order("pads.tp2", "pads.tp10"), Less);
    assert_eq!(order("pads.tp10", "pads.tp9"), Greater);
    assert_eq!(order("a10.x", "a2.y"), Greater);
    assert_eq!(order("a", "a.b"), Less);
    assert_eq!(order("a", "a1"), Less);
    assert_eq!(order("pads.tp2", "pads.tp2"), Equal);
    // Two spellings of one number are still ordered, so the numbering is deterministic.
    assert_eq!(order("tp02", "tp2"), Greater);
    assert_eq!(order("tp2", "tp02"), Less);
    // Text compares as before.
    assert_eq!(order("led", "r1"), Less);
    assert_eq!(order("b1", "a9"), Greater);
}

// --- Interface instances ----------------------------------------------------------------

/// An SPI master and slave in two modules, joined with `~` on the board, as the language
/// guide does.
const SPI_BOARD: &str = "
import std::bus::Spi
import std::sources::Rail

part Master {
    .vdd: Rail
    .spi: Spi
    pins {
        .vdd.pos       power         \"VDD\"    \"1\"
        .vdd.neg       power         \"VSS\"    \"2\"
        .spi.sclk      output        \"SCK\"    \"3\"
        .spi.mosi      output        \"MOSI\"   \"4\"
        .spi.miso      input         \"MISO\"   \"5\"
        .spi.cs        output        \"CS\"     \"6\"
    }
}

part Slave {
    .vdd: Rail
    .spi: Spi
    pins {
        .vdd.pos       power         \"VDD\"    \"1\"
        .vdd.neg       power         \"VSS\"    \"2\"
        .spi.sclk      input         \"SCK\"    \"3\"
        .spi.mosi      input         \"SI\"     \"4\"
        .spi.miso      output        \"SO\"     \"5\"
        .spi.cs        input         \"CS\"     \"6\"
    }
}

module Controller {
    .vdd: Rail
    .spi: Spi
    .u: Master
    .vdd ~ .u.vdd
    .spi ~ .u.spi
}

module Storage {
    .vdd: Rail
    .spi: Spi
    .u: Slave
    .vdd ~ .u.vdd
    .spi ~ .u.spi
}

board SpiBoard {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
    }
    .ctl: Controller
    .flash: Storage
    .supply ~ .ctl.vdd
    .supply ~ .flash.vdd
    .ctl.spi ~ .flash.spi
}
";

fn spi_board() -> Design {
    lowered(SPI_BOARD).unwrap()
}

#[test]
fn the_design_carries_the_interface_instances_and_the_nets_of_their_members() {
    let d = spi_board();
    let spi: Vec<_> = d
        .interfaces
        .iter()
        .filter(|i| i.type_name == "Spi")
        .map(|i| i.path.as_str())
        .collect();
    // The ports of the modules and the interfaces of their parts: each joins the same nets,
    // and each is listed, by path.
    assert_eq!(spi, ["ctl.spi", "ctl.u.spi", "flash.spi", "flash.u.spi"]);
    let members = |path: &str| -> Vec<(String, String)> {
        d.interfaces
            .iter()
            .find(|i| i.path == path)
            .unwrap_or_else(|| panic!("no interface {path}"))
            .members
            .iter()
            .map(|m| (m.name.clone(), m.net.clone()))
            .collect()
    };
    // Nobody named the nets, so each is called after its smallest terminal path.
    let expected: Vec<(String, String)> = ["sclk", "mosi", "miso", "cs"]
        .iter()
        .map(|m| (m.to_string(), format!("net-ctl.u.spi.{m}")))
        .collect();
    // The two ends of the bus see the same four nets under the same member names.
    for path in ["ctl.spi", "ctl.u.spi", "flash.spi", "flash.u.spi"] {
        assert_eq!(members(path), expected, "{path}");
    }
    // Each member's net is one of the design's nets.
    for i in &d.interfaces {
        for m in &i.members {
            assert!(d.nets.iter().any(|n| n.name == m.net), "{}", m.net);
        }
    }
    // The rails are interface instances of their own: the supply's pins are not members of
    // the SPI interfaces.
    assert!(d.interfaces.iter().any(|i| i.type_name == "Rail"));
    assert!(
        d.interfaces
            .iter()
            .all(|i| i.type_name != "Spi" || i.members.len() == 4)
    );
}

#[test]
fn interface_instances_are_listed_in_path_order() {
    let d = spi_board();
    let paths: Vec<&str> = d.interfaces.iter().map(|i| i.path.as_str()).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted);
}

#[test]
fn a_board_with_no_interface_has_none_in_its_design() {
    let d = lowered(LED).unwrap();
    assert!(
        d.interfaces.iter().all(|i| i.type_name == "Rail"),
        "{:?}",
        d.interfaces
    );
}

// ---- notes (ADR 0060) ------------------------------------------------------------------

const NOTED: &str = r#"
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

/// A small module: an LED and its resistor.
module Light {
    //! It takes its current from the rail it is on.
    .r: Resistor {}
    /// The light itself.
    .d: Led {
        .current &= 5mA..15mA
    }
    .supply: Rail
    // This one is an ordinary comment and is never a note.
    .d.current == (.supply.voltage - .d.forward_voltage) / .r.resistance
    .supply.pos ~> .r ~> .d ~> .supply.neg
}

/// The board of the test.
///
/// Two paragraphs.
board Noted {
    /// Made by JLCPCB's two-layer process.
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    /// Where the supply comes in.
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    /// The supply comes in through this header.
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
    /// Used as a status light.
    .light: Light
    .light.supply ~ .supply
}
"#;

#[test]
fn doc_comments_and_relations_become_notes_in_source_order() {
    use netsmith_core::NoteKind;
    let d = lowered(NOTED).unwrap();
    let docs: Vec<(&str, &str, &[String])> = d
        .notes
        .iter()
        .filter(|n| n.kind == NoteKind::Doc)
        .map(|n| (n.path.as_str(), n.text.as_str(), n.anchors.as_slice()))
        .collect();
    // The board's doc (two paragraphs, a line break between), a part's, the module's (its own
    // `///` and `//!`, then the line above its declaration) and the rail's, anchored on the
    // parts that are on its nets (the light, the resistor and the header: three).
    let find = |path: &str| {
        docs.iter()
            .find(|d| d.0 == path)
            .unwrap_or_else(|| panic!("{path}: {docs:?}"))
    };
    assert_eq!(find("").1, "The board of the test.\nTwo paragraphs.");
    assert!(find("").2.is_empty(), "the board's note is the sheet's");
    assert_eq!(
        find("power_in").1,
        "The supply comes in through this header."
    );
    assert_eq!(find("power_in").2, ["power_in"]);
    assert_eq!(
        find("light").1,
        "A small module: an LED and its resistor. It takes its current from the rail it is on.\nUsed as a status light."
    );
    assert!(find("light").2.is_empty());
    assert_eq!(find("light.d").1, "The light itself.");
    assert_eq!(find("light.d").2, ["light.d"]);
    assert_eq!(find("supply").1, "Where the supply comes in.");
    assert_eq!(find("supply").2, ["light.d", "light.r", "power_in"]);
    // Not an ordinary comment, not the process (nothing drawn is on it), not the library's.
    assert!(docs.iter().all(|d| !d.1.contains("ordinary comment")));
    assert!(docs.iter().all(|d| d.0 != "process"));
    assert!(
        d.notes
            .iter()
            .all(|n| n.at.as_ref().is_some_and(|a| !a.file.starts_with("std/")))
    );
    // The relation of the module, as written, about the parts it names; none of the part
    // models' own relations (`.drop == .current * .resistance` of a resistor).
    let relations: Vec<_> = d
        .notes
        .iter()
        .filter(|n| n.kind == NoteKind::Relation)
        .collect();
    assert_eq!(relations.len(), 1, "{relations:?}");
    assert_eq!(relations[0].path, "light");
    assert_eq!(
        relations[0].text,
        ".d.current == (.supply.voltage - .d.forward_voltage) / .r.resistance"
    );
    assert_eq!(relations[0].anchors, ["light.d", "light.r"]);
    let at = relations[0].at.as_ref().unwrap();
    assert_eq!((at.file.as_str(), at.line), ("t.nsm", 17));
    let result = relations[0].result.as_deref().unwrap();
    assert!(result.starts_with("d.current = "), "{result}");
    // Docs come before relations, and both keep the source's order.
    let kinds: Vec<_> = d.notes.iter().map(|n| n.kind).collect();
    assert!(kinds.windows(2).all(|w| w[0] <= w[1]), "{kinds:?}");
}

#[test]
fn a_board_with_no_doc_comment_and_no_relation_has_no_note() {
    let d = lowered(REGULATOR_NO_DOCS).unwrap();
    assert!(d.notes.is_empty(), "{:?}", d.notes);
}

const REGULATOR_NO_DOCS: &str = r#"
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail

board Plain {
    // Only an ordinary comment here.
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .supply.pos ~> .r1 ~> .supply.neg
}
"#;

// ---- the parts a relation is about (ADR 0060, "Built in the polish") ----------------------

/// Two rails, a resistor between them and a relation of the rails only; `loads` more resistors
/// from the output to ground, so ground has all of them and the output net has each of them.
fn rails_only(loads: usize) -> String {
    let mut src = String::from(
        r#"
import std::parts::passive::Resistor
import std::sources::Rail

board Divided {
    .input: Rail {
        .voltage &= 5V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .output: Rail {
        .pos.net &= "VOUT"
        .neg.net &= "GND"
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .input.pos ~> .r1 ~> .output.pos
    .input.neg ~ .output.neg
    .output.voltage == .input.voltage / 2
"#,
    );
    for i in 2..2 + loads {
        src.push_str(&format!(
            "    .r{i}: Resistor {{ .resistance &= 10kohm +/- 1% }}\n    .output.pos ~> .r{i} ~> .output.neg\n"
        ));
    }
    src.push_str("}\n");
    src
}

fn relation_anchors(d: &Design) -> Vec<Vec<String>> {
    use netsmith_core::NoteKind;
    d.notes
        .iter()
        .filter(|n| n.kind == NoteKind::Relation)
        .map(|n| n.anchors.clone())
        .collect()
}

#[test]
fn a_relation_of_rails_only_is_about_the_parts_on_their_nets() {
    let d = lowered(&rails_only(0)).unwrap();
    assert_eq!(relation_anchors(&d), [["r1"]], "{:?}", d.notes);
}

#[test]
fn the_return_net_of_a_rail_is_left_out_because_every_part_is_on_it() {
    // `rb` is on ground and on the middle of a chain, and on no net a rail supplies.
    let mut src = rails_only(0);
    src.truncate(src.rfind('}').unwrap());
    src.push_str(
        "    .ra: Resistor { .resistance &= 10kohm +/- 1% }\n    .rb: Resistor { .resistance &= 10kohm +/- 1% }\n    .output.pos ~> .ra ~> .rb ~> .output.neg\n}\n",
    );
    let d = lowered(&src).unwrap();
    assert_eq!(relation_anchors(&d), [["r1", "ra"]], "{:?}", d.notes);
}

#[test]
fn a_relation_of_rails_with_more_than_three_parts_on_them_is_the_sheets() {
    let d = lowered(&rails_only(3)).unwrap();
    assert_eq!(
        relation_anchors(&d),
        [Vec::<String>::new()],
        "{:?}",
        d.notes
    );
}

#[test]
fn a_relation_that_names_a_part_is_about_the_parts_it_names_and_not_the_rails_parts() {
    let d = lowered(NOTED).unwrap();
    assert_eq!(relation_anchors(&d), [["light.d", "light.r"]]);
}
