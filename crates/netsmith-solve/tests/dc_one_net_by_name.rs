//! A part whose two ends are on one net only by the name the net takes (issue #745).
//!
//! A part's rail takes the default ground of `Rail` (`.neg.net ?= "GND"`, ADR 0121), so its `neg`
//! is on the board's `GND` whatever it is wired to. A resistor from that `neg` to the supply's
//! ground then has both ends on `GND`: the DC analysis finds no current in it, and an equation that
//! gives it one is refused. The refusal says why: the two ends are one net by its name, which no
//! `~` gives them, and which pin to name. The cell and its numbers are invented.

use netsmith_check::{Loader, check};
use netsmith_solve::{Solution, solve};

fn solved(src: &str) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &netsmith_solve::Catalogs::jlcpcb())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// A three-terminal store: two cells in series with a pin at the midpoint between them, a rail
/// for its main terminals that draws a stated current, a resistor across each cell and one in the
/// return. `extra` is written in the board.
fn store(extra: &str) -> String {
    format!(
        r#"import std::parts::passive::Resistor
import std::sources::Rail

part Cell {{
    .designator &= "U"
    .main: Rail
    .main.current &= 0A..10uA
    pins {{
        .main.neg power   "N"   "1"
        .tap      passive "TAP" "2"
        .main.pos power   "P"   "3"
    }}
    .package: Package
    catalog cell {{ .mpn, .package }}
}}

module Store {{
    .supply: Rail
    .cell: Cell
    .top: Resistor {{ .resistance &= 43kohm +/- 1% }}
    .bottom: Resistor {{ .resistance &= 43kohm +/- 1% }}
    .limit: Resistor {{ .resistance &= 10ohm +/- 1% }}
    .supply.pos ~ .cell.main.pos
    .supply.pos ~> .top ~> .cell.tap
    .cell.tap ~> .bottom ~> .cell.main.neg
    .cell.main.neg ~> .limit ~> .supply.neg
    .limit.current == .supply.voltage / (.top.resistance + .bottom.resistance + .limit.resistance)
}}

board Repro {{
    .supply: Rail {{
        .voltage &= 5.0V +/- 2%
        .pos.net &= "V5"
    }}
    .store: Store
    .supply ~ .store.supply
{extra}}}
"#
    )
}

/// The cell's `neg` takes `GND` by default, so the return resistor carries nothing: the refusal
/// names the resistor, the net, the two pins that ask for the name and the pin to name.
#[test]
fn a_part_on_one_net_by_a_default_ground_says_so() {
    let s = solved(&store(""));
    let m = messages(&s);
    let x = m
        .iter()
        .find(|x| x.contains("`store.limit.current` is inconsistent"))
        .unwrap_or_else(|| panic!("{m:?}"));
    assert!(
        x.contains(
            "both ends of `store.limit` are on `GND` only by that name, which no `~` gives them"
        ),
        "{x}"
    );
    assert!(
        x.contains("`store.cell.main.neg` and `supply.neg` take it from a default (`?=`)"),
        "{x}"
    );
    assert!(
        x.contains("name its net on the board (`.store.cell.main.neg.net &= \"...\"`)"),
        "{x}"
    );
}

/// With the cell's `neg` named, the resistor is in the chain and the equation agrees with the
/// analysis.
#[test]
fn naming_the_parts_ground_puts_it_back_in_the_chain() {
    let s = solved(&store("    .store.cell.main.neg.net &= \"CELL_N\"\n"));
    let m = messages(&s);
    assert!(m.iter().all(|x| !x.contains("inconsistent")), "{m:?}");
}

/// A resistor whose ends a `~` joins to one net carries nothing too, but by the design's own
/// words: the refusal has nothing to add about names.
#[test]
fn a_part_on_one_net_by_a_join_has_no_word_about_names() {
    let src = store("").replace(
        ".cell.main.neg ~> .limit ~> .supply.neg",
        ".cell.main.neg ~> .limit ~> .supply.neg\n    .cell.main.neg ~ .supply.neg",
    );
    let s = solved(&src);
    let m = messages(&s);
    let x = m
        .iter()
        .find(|x| x.contains("`store.limit.current` is inconsistent"))
        .unwrap_or_else(|| panic!("{m:?}"));
    assert!(!x.contains("only by that name"), "{x}");
}
