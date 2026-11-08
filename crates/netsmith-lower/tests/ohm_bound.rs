//! A current that is not known but has an upper bound from Ohm's law, and the tracks of its
//! net (ADR 0036, amendment 4). The bound is in `netsmith-solve`'s tests; here is only what
//! lowering does with it: the width is the one it would be without the bound, and the note
//! that the tracks are a lower bound goes only when the width drawn carries the known current
//! and every bound together. Every part and value here is invented.

use netsmith_check::{Loader, check};
use netsmith_lower::{Lowered, lower};
use netsmith_solve::{Catalogs, solve};

const PARTS: &str = r#"
import std::logic::Logic
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::sources::Rail

part Driver {
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    pins {
        .vdd.pos   power  "VDD" "1"
        .vdd.neg   power  "GND" "2"
        .out.line  output "OUT" "3"
    }
}

/// A pin that drives a line and says nothing of how far.
part Opaque {
    pins {
        .out output "OUT" "1"
    }
}
"#;

fn lowered(body: &str) -> Lowered {
    let src = format!(
        "{PARTS}\nboard B {{\n    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}\n    .s3: Rail {{\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n        .current &= 0mA..1mA\n    }}\n{body}}}\n"
    );
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    lower(&c.boards[0], &s).unwrap()
}

/// The notes that start by naming `part` as the one whose current is not known.
fn notes_naming(l: &Lowered, part: &str) -> Vec<String> {
    l.notes
        .iter()
        .filter(|n| n.starts_with(&format!("the current of part `{part}` is not known")))
        .cloned()
        .collect()
}

/// The notes that name `part` anywhere among the currents that are not known.
fn notes_naming_part(l: &Lowered, part: &str) -> Vec<String> {
    l.notes
        .iter()
        .filter(|n| {
            n.starts_with("the current of ")
                && n.split(" is not known")
                    .next()
                    .is_some_and(|who| who.contains(&format!("part `{part}`")))
        })
        .cloned()
        .collect()
}

#[test]
fn a_pull_down_whose_bound_fits_the_least_track_has_no_note_and_the_same_width() {
    let l = lowered(
        "    .a: Driver\n    .s3 ~ .a.vdd\n    .r: Resistor { .resistance &= 10kohm +/- 1% }\n    .r.a ~ .a.out.line\n    .r.b ~ .s3.neg\n",
    );
    assert_eq!(notes_naming(&l, "r"), Vec::<String>::new(), "{:?}", l.notes);
    // No track is widened: the bound says the least width is enough.
    assert!(
        l.design.features.net_widths.is_empty(),
        "{:?}",
        l.design.features.net_widths
    );
}

#[test]
fn a_resistor_whose_bound_needs_more_than_the_track_keeps_the_note() {
    // Two 1 ohm resistors across the supply, so that neither current is worked out: the bound
    // of each is 3.5 A, which the least track does not carry.
    let l = lowered(
        "    .r: Resistor { .resistance &= 1ohm +/- 1% }\n    .q: Resistor { .resistance &= 1ohm +/- 1% }\n    .r.a ~ .s3.pos\n    .r.b ~ .s3.neg\n    .q.a ~ .s3.pos\n    .q.b ~ .s3.neg\n",
    );
    for part in ["r", "q"] {
        let said = notes_naming_part(&l, part);
        assert!(!said.is_empty(), "{part}: {:?}", l.notes);
        assert!(said.iter().all(|n| n.contains("a lower bound")), "{said:?}");
    }
}

#[test]
fn a_part_with_no_bound_keeps_the_note() {
    // Two resistors from a pin that nothing bounds to the ground: no bound for either, so the
    // tracks are still a lower bound.
    let l = lowered(
        "    .o: Opaque\n    .r: Resistor { .resistance &= 10kohm +/- 1% }\n    .q: Resistor { .resistance &= 10kohm +/- 1% }\n    .r.a ~ .o.out\n    .r.b ~ .s3.neg\n    .q.a ~ .o.out\n    .q.b ~ .s3.neg\n",
    );
    for part in ["r", "q"] {
        assert!(
            !notes_naming_part(&l, part).is_empty(),
            "{part}: {:?}",
            l.notes
        );
    }
}
