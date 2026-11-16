//! The `layout` block of a part (ADR 0030, issue #829): a mandatory `source`, `near` statements
//! that name the part's own pins, and what the model keeps of them. The part `Ld9` and its
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
            assert_eq!(l.within, None, "the datasheet gives no distance");
            assert!(l.site.contains("regulator.nsm:"), "{}", l.site);
            let select = match l.select {
                NearSelect::Bridging(a, b) => format!("bridging {} {}", pin(a), pin(b)),
                NearSelect::On { pin: p, kind } => {
                    format!("on {} {}", pin(p), kind.map_or("any", |k| k.word()))
                }
            };
            (l.name.clone(), select, pin(l.to))
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
    assert!((near.within.unwrap() - 2.5).abs() < 1e-9, "{near:?}");
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
