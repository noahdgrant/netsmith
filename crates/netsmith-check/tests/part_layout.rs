//! The `layout` block of a part (ADR 0030, issues #829 and #994): a mandatory `source`,
//! statements that name the part's own pins, and what the model keeps of them. The part `Ld9` and its
//! datasheet are invented; `Lmr16006y` is the library's.

use netsmith_check::*;

fn part(layout: &str) -> String {
    format!(
        "\
import std::sources::Rail

part Ld9 {{
    .designator &= \"U\"
    .vin: Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    pins {{
        .vin.pos  power \"IN\"  \"3\"
        .vin.neg  power \"GND\" \"1\"
        .vout.pos power \"OUT\" \"2\"
    }}
    layout {{
{layout}
    }}
}}

board B {{
    .u: Ld9
}}
"
    )
}

fn messages(src: &str) -> Vec<String> {
    let c = check("test.nsm", src, &Loader::std());
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// The one message that names `needle`, of the check of `src`.
fn message(src: &str, needle: &str) -> String {
    let all = messages(src);
    let found: Vec<&String> = all.iter().filter(|m| m.contains(needle)).collect();
    assert_eq!(found.len(), 1, "{all:#?}");
    found[0].clone()
}

#[test]
fn the_library_buck_ic_states_its_datasheet_guidelines() {
    let board = "\
import std::parts::regulator::{Buck, Lmr16006y}
import std::sources::Rail

board B {
    .input: Rail { .voltage &= 12V +/- 5% }
    .output: Rail { .current &= 0mA..300mA }
    .reg: Buck<Lmr16006y> { .vout.voltage &= 3.3V +/- 3% }
    .input ~ .reg.vin
    .reg.vout ~ .output
}
";
    let c = check("test.nsm", board, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.board("B").unwrap().model;
    let ic = model.inst_at("reg.ic").unwrap();
    let pin = |p: InstId| model.path_below(p, ic);
    let mut seen: Vec<(String, String, String)> = model
        .layout
        .iter()
        .map(|l| {
            assert_eq!(l.part, ic);
            assert_eq!(l.source, "SNVSA24, 11.1 Layout Guidelines, p.17");
            assert!(l.site.contains("regulator.nsm:"), "{}", l.site);
            assert_eq!(l.package, None);
            let GuidelineRule::Near { select, to, within } = l.rule else {
                panic!("{:?}", l.rule)
            };
            assert_eq!(within, None, "the datasheet gives no distance");
            let select = match select {
                NearSelect::Bridging(a, b) => format!("bridging {} {}", pin(a), pin(b)),
                NearSelect::On { pin: p, kind } => {
                    format!("on {} {}", pin(p), kind.map_or("any", |k| k.word()))
                }
            };
            (l.name.clone(), select, pin(to))
        })
        .collect();
    seen.sort();
    let want = [
        ("feedback", "on fb resistor", "fb"),
        ("inductor", "on sw.pos inductor", "sw.pos"),
        ("input_cap", "bridging vin.pos vin.neg", "vin.pos"),
    ];
    let want: Vec<(String, String, String)> = want
        .iter()
        .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
        .collect();
    assert_eq!(seen, want);
}

#[test]
fn a_within_is_kept_in_millimetres() {
    let src = part(
        "        source \"LD9, 9.1 Layout, p.12\"\n        near input_cap bridging .vin.pos .vin.neg to .vin.pos within 2.5mm",
    );
    let c = check("test.nsm", &src, &Loader::std());
    let model = &c.board("B").unwrap().model;
    let [near] = model.layout.as_slice() else {
        panic!("{:?}\n{}", model.layout, c.render())
    };
    let GuidelineRule::Near { within, .. } = near.rule else {
        panic!("{near:?}")
    };
    assert!((within.unwrap() - 2.5).abs() < 1e-9, "{near:?}");
    assert_eq!(near.source, "LD9, 9.1 Layout, p.12");
}

#[test]
fn a_block_without_a_source_is_refused() {
    let m = message(
        &part("        near input_cap bridging .vin.pos .vin.neg to .vin.pos"),
        "states no source",
    );
    assert!(m.contains("add `source \"...\"` with the document and the section"), "{m}");
}

#[test]
fn an_empty_source_is_refused() {
    let m = message(
        &part("        source \"  \"\n        near c bridging .vin.pos .vin.neg to .vin.pos"),
        "is empty",
    );
    assert!(m.contains("name the document and the section"), "{m}");
}

#[test]
fn a_block_has_one_source() {
    let m = message(
        &part("        source \"LD9, p.12\"\n        source \"AN-1149\"\n        near c bridging .vin.pos .vin.neg to .vin.pos"),
        "one `source`",
    );
    assert!(m.contains("a `layout` block of their own"), "{m}");
}

#[test]
fn a_kind_that_does_not_exist_is_named() {
    let m = message(
        &part("        source \"LD9, p.12\"\n        near c on .vout.pos kind capacitr to .vout.pos"),
        "there is no kind",
    );
    assert!(m.contains("`capacitr`") && m.contains("capacitor"), "{m}");
}

#[test]
fn a_statement_names_pins_of_the_part() {
    let m = message(
        &part("        source \"LD9, p.12\"\n        near c on .vout to .vout.pos"),
        "not a pin",
    );
    assert!(m.contains("`.vout`"), "{m}");
}

#[test]
fn within_is_a_length() {
    let m = message(
        &part("        source \"LD9, p.12\"\n        near c on .vout.pos to .vout.pos within 2V"),
        "`within`",
    );
    assert!(m.contains("one length above zero") && m.contains("`2V`"), "{m}");
}

#[test]
fn a_name_is_stated_once_in_a_part() {
    let m = message(
        &part("        source \"LD9, p.12\"\n        near c on .vout.pos to .vout.pos\n        near c on .vin.pos to .vin.pos"),
        "already states",
    );
    assert!(m.contains("`c`"), "{m}");
}

#[test]
fn only_a_part_holds_a_block() {
    let src = "\
module M {
    layout {
        source \"LD9, p.12\"
    }
}

board B {
    .m: M
}
";
    let m = message(src, "`layout` can only appear");
    assert!(m.contains("directly inside a part"), "{m}");
}

/// The guidelines of the check of `src`, by name, as the model keeps them.
fn rules(src: &str) -> Vec<(String, GuidelineRule)> {
    let c = check("test.nsm", src, &Loader::std());
    let model = &c.board("B").unwrap().model;
    model
        .layout
        .iter()
        .map(|l| (l.name.clone(), l.rule.clone()))
        .collect()
}

#[test]
fn the_other_statements_keep_their_numbers_in_millimetres() {
    let src = part(
        "        source \"LD9, 9.1 Layout, p.12\"
        away coil on .vout.pos kind inductor from .vin.pos beyond 4mm
        loop hot bridging .vin.pos .vin.neg within 15mm
        keepout under at -1mm, -2.5mm size 2mm, 5mm forbid tracks vias
        keepout all at 0mm, 0mm size 1mm, 1mm
        edge out .vout.pos within 1.5mm
        vias gnd .vin.neg 4
        route out_track .vout.pos width 0.5mm length 10mm",
    );
    let rules = rules(&src);
    let names: Vec<&str> = rules.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["coil", "hot", "under", "all", "out", "gnd", "out_track"]);
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let GuidelineRule::Away { beyond, select: NearSelect::On { kind, .. }, .. } = rules[0].1 else {
        panic!("{:?}", rules[0])
    };
    assert!(close(beyond, 4.0) && kind == Some(netsmith_core::PartKind::Inductor));
    let GuidelineRule::Loop { within, .. } = rules[1].1 else {
        panic!("{:?}", rules[1])
    };
    assert!(close(within.unwrap(), 15.0));
    let GuidelineRule::Keepout { at, size, tracks, vias, copper, parts } = rules[2].1 else {
        panic!("{:?}", rules[2])
    };
    assert!(close(at.0, -1.0) && close(at.1, -2.5) && close(size.0, 2.0) && close(size.1, 5.0));
    assert_eq!((tracks, vias, copper, parts), (true, true, false, false));
    let GuidelineRule::Keepout { tracks, vias, copper, parts, .. } = rules[3].1 else {
        panic!("{:?}", rules[3])
    };
    assert_eq!((tracks, vias, copper, parts), (true, true, true, true), "none named is all four");
    let GuidelineRule::Edge { within, .. } = rules[4].1 else {
        panic!("{:?}", rules[4])
    };
    assert!(close(within.unwrap(), 1.5));
    assert!(matches!(rules[5].1, GuidelineRule::Vias { count: 4, .. }));
    let GuidelineRule::Route { width, length, .. } = rules[6].1 else {
        panic!("{:?}", rules[6])
    };
    assert!(close(width.unwrap(), 0.5) && close(length.unwrap(), 10.0));
}

#[test]
fn beyond_is_a_length_above_zero() {
    let m = message(
        &part("        source \"LD9, p.12\"\n        away c on .vout.pos from .vin.pos beyond 0mm"),
        "`beyond`",
    );
    assert!(m.contains("`away c`") && m.contains("one length above zero"), "{m}");
}

#[test]
fn a_loop_names_two_pins() {
    let m = message(
        &part("        source \"LD9, p.12\"\n        loop hot bridging .vin.pos .vin.pos"),
        "two different pins",
    );
    assert!(m.contains("the parts of the loop"), "{m}");
}

#[test]
fn a_keepout_forbids_what_the_interface_names() {
    let m = message(
        &part("        source \"LD9, p.12\"\n        keepout k at 0mm, 0mm size 1mm, 1mm forbid traces"),
        "a keepout forbids",
    );
    assert!(m.contains("`traces`"), "{m}");
}

#[test]
fn the_size_of_a_keepout_is_above_zero() {
    let m = message(
        &part("        source \"LD9, p.12\"\n        keepout k at 0mm, 0mm size 0mm, 1mm"),
        "`size`",
    );
    assert!(m.contains("one length above zero"), "{m}");
}

#[test]
fn a_count_of_vias_is_a_whole_number() {
    let m = message(
        &part("        source \"LD9, p.12\"\n        vias gnd .vin.neg 2.5"),
        "the count of `vias gnd`",
    );
    assert!(m.contains("a whole number of one or more") && m.contains("`2.5`"), "{m}");
}

/// A part with a pin table for each of two packages and a block for one of them.
fn packaged(package: &str) -> String {
    format!(
        "\
import std::sources::Rail

part Ld7 {{
    .designator &= \"U\"
    .vin: Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    pins \"SOT-23\" \"SOT-223\" {{
        .vin.pos  power \"IN\"  \"3\"
        .vin.neg  power \"GND\" \"1\"
        .vout.pos power \"OUT\" \"2\"
    }}
    .package: Package
    .package &= \"SOT-223\"
    layout \"{package}\" {{
        source \"LD7, 10 Layout, p.14\"
        vias tab .vin.neg 3
    }}
}}

board B {{
    .u: Ld7
}}
"
    )
}

#[test]
fn a_block_for_a_package_keeps_the_package() {
    let c = check("test.nsm", &packaged("SOT-223"), &Loader::std());
    let model = &c.board("B").unwrap().model;
    let [g] = model.layout.as_slice() else {
        panic!("{:?}", model.layout)
    };
    assert_eq!(g.package.as_deref(), Some("SOT-223"));
}

#[test]
fn a_block_for_a_package_the_part_has_no_table_for_is_refused() {
    let m = message(&packaged("SOT-89"), "no pin table for the package");
    assert!(m.contains("\"SOT-89\"") && m.contains("\"SOT-23\", \"SOT-223\""), "{m}");
}

#[test]
fn a_block_for_a_catalog_row_belongs_in_the_library_layouts_file() {
    let src = "\
import std::parts::regulator::LinearRegulator

layout \"XC6206P332MR-G\" for LinearRegulator {
    source \"XC6206 series, 9 Layout, p.9\"
    vias ground .vin.neg 2
}

board B {
    .reg: LinearRegulator
}
";
    let m = message(src, "`layout \"MPN\" for ...` belongs in the standard library's `std/layouts.nsm`");
    assert!(m.contains("a part of a project states its own in a `layout { }` block"), "{m}");
}

#[test]
fn the_layouts_file_gives_a_generic_the_blocks_of_its_rows() {
    let layouts = "\
layout \"XC6206P332MR-G\" for LinearRegulator {
    source \"XC invented, 9 Layout, p.9\"
    near input_cap bridging .vin.pos .vin.neg to .vin.pos within 2mm
    vias ground .vin.neg 2
}
layout \"OTHER-ROW\" for LinearRegulator {
    source \"OTHER invented, 3 Layout, p.4\"
    near input_cap bridging .vin.pos .vin.neg to .vin.pos
}
layout \"NOT-A-REGULATOR\" for Schottky {
    source \"x\"
    vias ground .cathode 2
}
";
    let board = "\
import std::parts::regulator::LinearRegulator

board B {
    .reg: LinearRegulator
}
";
    let mut loader = Loader::std();
    loader.insert("std::layouts", "std/layouts.nsm", layouts);
    let c = check("test.nsm", board, &loader);
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.board("B").unwrap().model;
    let mut seen: Vec<(String, Option<String>, String)> = model
        .layout
        .iter()
        .map(|l| (l.name.clone(), l.mpn.clone(), l.source.clone()))
        .collect();
    seen.sort();
    assert_eq!(
        seen,
        [
            (
                "ground".to_string(),
                Some("XC6206P332MR-G".to_string()),
                "XC invented, 9 Layout, p.9".to_string()
            ),
            (
                "input_cap".to_string(),
                Some("OTHER-ROW".to_string()),
                "OTHER invented, 3 Layout, p.4".to_string()
            ),
            (
                "input_cap".to_string(),
                Some("XC6206P332MR-G".to_string()),
                "XC invented, 9 Layout, p.9".to_string()
            ),
        ],
        "one name may be used in the blocks of two rows, and a block of another generic is not read"
    );
}

#[test]
fn a_mistake_in_a_row_block_is_reported_at_the_layouts_file() {
    let layouts = "\
layout \"XC6206P332MR-G\" for LinearRegulator {
    source \"XC invented, 9 Layout, p.9\"
    vias ground .nope 2
}
";
    let board = "\
import std::parts::regulator::LinearRegulator

board B {
    .reg: LinearRegulator
}
";
    let mut loader = Loader::std();
    loader.insert("std::layouts", "std/layouts.nsm", layouts);
    let c = check("test.nsm", board, &loader);
    let text = c.render();
    assert!(text.contains("std/layouts.nsm:3"), "{text}");
    assert!(text.contains("nope"), "{text}");
}

#[test]
fn a_kelvin_statement_keeps_its_selector_and_its_sense_pin() {
    let src = part(
        "        source \"LD9, 9.1 Layout, p.12\"
        kelvin sense bridging .vout.pos .vin.neg to .vout.pos",
    );
    let rules = rules(&src);
    let [(name, GuidelineRule::Kelvin { select: NearSelect::Bridging(a, b), to })] =
        rules.as_slice()
    else {
        panic!("{rules:?}")
    };
    assert_eq!(name, "sense");
    assert!(a == to && b != to, "{rules:?}");
}

#[test]
fn the_sense_pin_of_a_kelvin_statement_is_a_pin_of_its_selector() {
    let m = message(
        &part("        source \"LD9, p.12\"\n        kelvin sense on .vout.pos to .vin.pos"),
        "sense pin",
    );
    assert!(
        m.contains("`kelvin sense`") && m.contains("is a pin of its selector"),
        "{m}"
    );
}
