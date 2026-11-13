//! A note for a part the solver picked although a file says it is out of stock (issue #176).
//! The solver does not read stock (ADR 0047), so the pick is the same either way; the build
//! says so in its notes, and the editor shows the same line. The board and the parts are
//! invented, except that the one LCSC number in `data/jlcpcb/considered.csv` is a real one.

use netsmith_pipeline::{Built, outcome_with};
use netsmith_check::Loader;
use netsmith_solve::Catalogs;

const BOARD: &str = r#"
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

module Limit {
    .supply: Rail
    .limit: Resistor { .resistance &= 45.8kohm..46kohm }
    .supply.pos ~> .limit ~> .supply.neg
}

board Invented {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .charge: Limit
    .supply ~ .charge.supply
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
}
"#;

/// The project's own resistor row: a value the standard catalog has no row for.
fn rows(lcsc: &str, stock: &str) -> String {
    format!(
        "lcsc,mpn,resistance,max_power,package,library,stock,source\n\
         {lcsc},ZZR-459,45.9kohm +/- 0.1%,150mW,0603,Extended,{stock},https://example.invalid/zzr-459\n"
    )
}

fn built(lcsc: &str, stock: &str) -> Built {
    let mut catalogs = Catalogs::jlcpcb();
    catalogs
        .add_file("resistor", "catalogs/resistor.csv", &rows(lcsc, stock))
        .unwrap();
    let outcome = outcome_with("invented.nsm", BOARD, &Loader::std(), &catalogs, None)
        .unwrap_or_else(|e| panic!("{}", netsmith_pipeline::render_problems(&e)));
    outcome.boards.into_iter().next().expect("one board")
}

fn stock_notes(b: &Built) -> Vec<&String> {
    b.notes.iter().filter(|n| n.contains("stock")).collect()
}

#[test]
fn a_row_that_states_a_stock_of_zero_gets_a_note_that_names_the_part_and_what_to_do() {
    let b = built("C900001", "0");
    let notes = stock_notes(&b);
    assert_eq!(notes.len(), 1, "{:#?}", b.notes);
    let note = notes[0];
    for want in [
        "R1 (`charge.limit`)",
        "C900001 (ZZR-459)",
        "catalogs/resistor.csv:2",
        "states a stock of 0",
        "the solver does not read stock",
        "choose another row of the catalog or state a different value",
    ] {
        assert!(note.contains(want), "{want}: {note}");
    }
}

#[test]
fn a_part_that_considered_csv_records_as_out_of_stock_gets_a_note_when_the_row_states_no_stock() {
    let b = built("C1723147", "");
    let notes = stock_notes(&b);
    assert_eq!(notes.len(), 1, "{:#?}", b.notes);
    for want in [
        "R1 (`charge.limit`)",
        "C1723147 (ZZR-459)",
        "data/jlcpcb/considered.csv records it as out of stock",
        "choose another row of the catalog or state a different value",
    ] {
        assert!(notes[0].contains(want), "{want}: {}", notes[0]);
    }
}

#[test]
fn an_empty_or_positive_stock_and_a_part_not_in_considered_csv_get_no_note() {
    for (lcsc, stock) in [("C900001", ""), ("C900001", "40"), ("C1723147", "40")] {
        let b = built(lcsc, stock);
        assert!(stock_notes(&b).is_empty(), "{lcsc} {stock}: {:#?}", b.notes);
    }
}

#[test]
fn the_note_does_not_change_the_pick() {
    // The same part is picked and sourced whatever the row says about its stock.
    let picked = |stock: &str| {
        let b = built("C900001", stock);
        b.sourcing
            .iter()
            .filter(|s| s.kind == "Resistor")
            .map(|s| (s.refdes.clone(), s.lcsc.clone(), s.mpn.clone()))
            .collect::<Vec<_>>()
    };
    let zero = picked("0");
    assert_eq!(zero, picked(""));
    assert_eq!(zero, picked("500"));
    assert_eq!(zero[0].1.as_deref(), Some("C900001"));
}

#[test]
fn the_note_is_a_note_and_not_a_guideline() {
    let b = built("C900001", "0");
    assert!(
        b.guidelines
            .iter()
            .all(|g| !g.message.contains("stock of 0")),
        "{:#?}",
        b.guidelines
    );
}
