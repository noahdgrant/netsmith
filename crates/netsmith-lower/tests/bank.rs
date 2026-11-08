//! A bank of capacitors (ADR 0052) lowers to one part for each, on the same nets.

use netsmith_check::{Loader, check};
use netsmith_lower::{Lowered, lower};
use netsmith_solve::{Catalogs, solve};

fn lowered(bulk: &str, need: &str) -> Result<Lowered, Vec<String>> {
    let src = format!(
        r#"import std::parts::passive::Capacitor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board B {{
    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}
    .supply: Rail {{
        .voltage &= 5V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }}
    .bulk: Capacitor {{ {bulk} }}
    .bulk.count * .bulk.effective_capacitance >= {need}
    .supply.pos ~> .bulk ~> .supply.neg
}}
"#
    );
    let c = check("t.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    lower(&c.boards[0], &s)
}

#[test]
fn a_bank_is_one_part_for_each_capacitor_on_the_same_nets() {
    let l = lowered(".count &= 1..8", "40uF").unwrap();
    let parts = &l.design.parts;
    assert!(parts.len() > 1, "{} parts", parts.len());
    // Numbered one after the other; the first has the path of the instance, the others add
    // `#2`, `#3`, so that a layout file keys each by its own path.
    for (i, p) in parts.iter().enumerate() {
        assert_eq!(p.refdes, format!("C{}", i + 1));
        assert_eq!(
            p.path,
            if i == 0 {
                "bulk".to_string()
            } else {
                format!("bulk#{}", i + 1)
            }
        );
        assert_eq!(p.kind, "Capacitor");
        assert_eq!(p.value, parts[0].value);
        assert_eq!(p.pins.len(), 2);
    }
    // Every capacitor has its pad 1 on one net and its pad 2 on the other.
    for (net, pad) in [("VIN", "1"), ("GND", "2")] {
        let on = l
            .design
            .nets
            .iter()
            .find(|n| n.name == net)
            .unwrap_or_else(|| panic!("no net {net}"));
        let mine: Vec<&str> = on
            .pins
            .iter()
            .filter(|p| p.pin == pad)
            .map(|p| p.refdes.as_str())
            .collect();
        let all: Vec<String> = (1..=parts.len()).map(|i| format!("C{i}")).collect();
        assert_eq!(mine, all.iter().map(String::as_str).collect::<Vec<_>>());
    }
    // The sourcing and the origins have a line for each part, the same row each time.
    assert_eq!(l.sourcing.len(), parts.len());
    assert_eq!(l.origins.len(), parts.len());
    assert!(
        l.sourcing
            .iter()
            .all(|s| s.lcsc == l.sourcing[0].lcsc && s.lcsc.is_some())
    );
    assert!(l.origins.iter().all(|o| *o == l.origins[0]));
}

#[test]
fn a_bank_of_one_is_the_part_it_was() {
    let l = lowered(".count &= 1..8", "4uF").unwrap();
    assert_eq!(l.design.parts.len(), 1);
    assert_eq!(l.design.parts[0].refdes, "C1");
    assert_eq!(l.design.parts[0].path, "bulk");
}

#[test]
fn a_count_that_is_not_a_whole_number_is_refused_naming_the_field() {
    // The solver reports a count with no whole number in it; lowering, which reads the
    // model whatever the solver said, refuses to draw 2.5 parts.
    let src = r#"import std::parts::passive::Capacitor
import std::sources::Rail

board B {
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .bulk: Capacitor { .count &= 2.5 }
    .supply.pos ~> .bulk ~> .supply.neg
}
"#;
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    let errors = lower(&c.boards[0], &s).unwrap_err();
    assert!(
        errors.iter().any(|e| e.contains("`bulk.count` is 2.5")
            && e.contains("not a whole number of parts from 1 to 64")),
        "{errors:#?}"
    );
}

/// A bank (`.bulk`, several capacitors) beside a resistor, and a physical file.
fn lowered_physical(physical: &str) -> Lowered {
    let src = r#"import std::parts::passive::{Capacitor, Resistor}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board B {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .bulk: Capacitor { .count &= 1..8 }
    .bulk.count * .bulk.effective_capacitance >= 40uF
    .supply.pos ~> .r1 ~> .supply.neg
    .supply.pos ~> .bulk ~> .supply.neg
}
"#;
    let mut loader = Loader::std();
    loader.attach_physical("t.nsm", "t.physical.nsm", physical);
    let c = check("t.nsm", src, &loader);
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    lower(&c.boards[0], &s).unwrap_or_else(|e| panic!("{e:#?}"))
}

const OUTLINE: &str = "import std::pcb::Outline\nphysical for B {\n    case { .outline: Outline { .width &= 40mm; .height &= 30mm } }\n";

fn paths_of_bank(l: &Lowered) -> Vec<String> {
    l.design
        .parts
        .iter()
        .filter(|p| p.path == "bulk" || p.path.starts_with("bulk#"))
        .map(|p| p.path.clone())
        .collect()
}

#[test]
fn a_requirement_on_a_banks_part_holds_for_every_capacitor_and_a_note_says_so() {
    let l = lowered_physical(&format!(
        "{OUTLINE}    .bulk.place.top &= 3mm..6mm\n    .bulk.place.side &= \"bottom\"\n    .bulk.height &= 1.2mm\n}}\n"
    ));
    let bank = paths_of_bank(&l);
    assert!(bank.len() >= 2, "{bank:?}");
    let f = &l.design.features;
    // One requirement for each capacitor, with the same bounds and face, and the lines that
    // state them copied with it.
    for path in &bank {
        let p = f
            .constraints
            .iter()
            .find(|p| &p.path == path)
            .unwrap_or_else(|| panic!("no requirement on `{path}`: {:?}", f.constraints));
        assert_eq!(p.side.as_deref(), Some("bottom"), "{path}");
        let top = p.bound(netsmith_core::PlaceField::Top).expect("top");
        assert_eq!((top.lo, top.hi), (3.0, 6.0), "{path}");
        assert!(f.heights.iter().any(|h| &h.path == path), "{path}");
    }
    let index = |path: &str| f.constraints.iter().position(|p| p.path == path).unwrap();
    for path in &bank {
        let i = index(path);
        let fields: Vec<&str> = l
            .constraint_sites
            .iter()
            .filter(|s| s.0 == i)
            .map(|s| s.1.as_str())
            .collect();
        assert!(
            fields.contains(&"top") && fields.contains(&"side"),
            "{path}: {fields:?}"
        );
    }
    assert!(
        l.notes
            .iter()
            .any(|n| n.contains("`bulk` is a bank of") && n.contains("holds for each of them")),
        "{:#?}",
        l.notes
    );
}

#[test]
fn what_a_bank_says_of_what_it_did_has_the_cause_other() {
    let l = lowered_physical(&format!(
        "{OUTLINE}    .bulk.place.x - .r1.place.x == 5.5mm +/- 3.5mm\n}}\n"
    ));
    assert!(!l.notes.is_empty());
    assert_eq!(l.note_causes.len(), l.notes.len());
    assert!(
        l.note_causes
            .iter()
            .all(|&c| c == netsmith_core::Cause::Other),
        "{:?}",
        l.note_causes
    );
}

#[test]
fn a_relation_with_a_banks_part_holds_for_every_capacitor_and_a_note_says_so() {
    let l = lowered_physical(&format!(
        "{OUTLINE}    .bulk.place.x - .r1.place.x == 5.5mm +/- 3.5mm\n}}\n"
    ));
    let bank = paths_of_bank(&l);
    assert!(bank.len() >= 2, "{bank:?}");
    let f = &l.design.features;
    assert_eq!(f.offsets.len(), bank.len(), "{:?}", f.offsets);
    for path in &bank {
        let o = f
            .offsets
            .iter()
            .find(|o| &o.b.path == path)
            .unwrap_or_else(|| panic!("no relation for `{path}`: {:?}", f.offsets));
        assert_eq!(o.a.path, "r1");
        assert_eq!((o.lo, o.hi), (2.0, 9.0));
    }
    // The sites stay in step with the relations.
    assert_eq!(l.offset_sites.len(), f.offsets.len());
    assert!(
        l.notes
            .iter()
            .any(|n| n.contains("the relation holds for each pair of capacitors")),
        "{:#?}",
        l.notes
    );
}

#[test]
fn a_requirement_on_a_part_that_is_not_a_bank_is_left_alone_and_says_nothing() {
    let l = lowered_physical(&format!("{OUTLINE}    .r1.place.top &= 3mm..6mm\n}}\n"));
    let f = &l.design.features;
    assert_eq!(f.constraints.len(), 1);
    assert_eq!(f.constraints[0].path, "r1");
    assert!(
        l.notes.iter().all(|n| !n.contains("bank")),
        "{:#?}",
        l.notes
    );
}

#[test]
fn a_user_part_with_a_count_member_is_one_part_not_a_bank() {
    let src = r#"import std::sources::Rail

part LedBar {
    .designator &= "D"
    .count: Number
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    bridge .a ~> .b
}

board B {
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .bar: LedBar { .count &= 8 }
    .supply.pos ~> .bar ~> .supply.neg
}
"#;
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    let l = lower(&c.boards[0], &s).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(l.design.parts.len(), 1, "{:?}", l.design.parts);
    assert_eq!(l.design.parts[0].path, "bar");
}
