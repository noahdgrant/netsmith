//! A range the board states on a value that an equation works out is held to what the
//! equation gives, to the last digit and no further (ADR 0056): a range that ends exactly
//! where the circuit's does is accepted, and one that is a hair short of it is refused.
//! Every value here is invented.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

fn board(window: &str, current: &str) -> String {
    format!(
        r#"import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

part Link {{
    .designator &= "R"
    pins {{
        .a passive "A" "1"
        .b passive "B" "2"
    }}
    .resistance: Resistance
    #[role(current)]
    .current: Current
    .drop: Voltage
    .drop == .current * .resistance
    bridge .a ~> .b
}}

board B {{
    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}
    .supply: Rail {{
        .voltage &= 10V..15V
        .pos.net &= "VBATT"
        .neg.net &= "GND"
    }}
    .link: Link {{
        .resistance &= 47mohm +/- 2%
        .current &= {current}
    }}
    .after: Rail {{
        .pos.net &= "AFTER"
        .neg.net &= "GND"
        .voltage &= {window}
    }}
    .supply.neg ~ .after.neg
    .supply.pos ~> .link ~> .after.pos
    .after.voltage == .supply.voltage - .link.drop
}}
"#
    )
}

fn solved(window: &str, current: &str) -> Solution {
    let src = board(window, current);
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

#[test]
fn a_window_that_ends_at_the_supplys_maximum_is_accepted() {
    // The drop is 0 A * 47 mohm up to 0.9 A * 47 mohm: its lowest value is exactly zero, so
    // the voltage behind it ends at the supply's 15 V.
    let s = solved("9V..15V", "0A..0.9A");
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
}

#[test]
fn a_window_below_the_supplys_maximum_is_refused() {
    let s = solved("9V..14.99V", "0A..0.9A");
    assert!(
        s.diagnostics.iter().any(|d| d.message.contains(
            "`after.voltage` must stay within 9V..14.99V, but (supply.voltage - link.drop) works out to 9.956V..15V"
        )),
        "{:?}",
        s.diagnostics
    );
}

#[test]
fn a_window_a_hair_above_the_lowest_voltage_is_refused() {
    // 10 V less 0.9 A * 47 mohm * 1.02 is 9.95685 V: a window from 9.96 V is not guaranteed.
    let s = solved("9.96V..15V", "0A..0.9A");
    assert!(
        s.diagnostics.iter().any(|d| d
            .message
            .contains("`after.voltage` must stay within 9.96V..15V")),
        "{:?}",
        s.diagnostics
    );
    let s = solved("9.95V..15V", "0A..0.9A");
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
}

#[test]
fn a_current_that_can_run_the_other_way_puts_the_voltage_above_the_supply() {
    // A drop that may be below zero lifts the voltage past 15 V: the window is refused.
    let s = solved("9V..15V", "-0.1A..0.9A");
    assert!(
        s.diagnostics.iter().any(|d| d
            .message
            .contains("`after.voltage` must stay within 9V..15V")),
        "{:?}",
        s.diagnostics
    );
}
