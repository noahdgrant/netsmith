//! `unused .processor.usb` (ADR 0083): what the solver does not say about an interface the
//! board states is not used. The limits the part states for it are not listed as not evaluated,
//! its absolute maximums are not read against a net, and a board that uses the interface gets
//! every check it got before.
//!
//! `Zproc` and `Zport` are invented for these tests.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"
import std::bus::Usb2
import std::logic::Logic
import std::sources::Rail

/// Zproc, an invented processor: its datasheet limits VBUS to 4.35 V to 5.5 V on a USB port that
/// has no supply of its own.
part Zproc {
    .vdd: Rail
    .usb: Usb2
    .usb.vbus.neg ~ .vdd.neg
    .usb.vbus.voltage >= 4.35V
    .usb.vbus.voltage <= 5.5V

    pins {
        .vdd.pos      power         "VDD"  "1"
        .vdd.neg      power         "VSS"  "2"
        .usb.vbus.pos power         "VBUS" "3" optional
        .usb.dp       bidirectional "DP"   "4"
        .usb.dm       bidirectional "DM"   "5"
    }
}

/// Zport, an invented part with a logic input that survives 3.6 V, which a datasheet may state
/// for a pad whether or not a board uses it.
part Zport {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.line.abs_max &= 3.6V
    pins {
        .vdd.pos  power "VDD" "1"
        .vdd.neg  power "GND" "2"
        .in.line  input "IN"  "3"
    }
}

/// Zpad, an invented test pad.
part Zpad {
    pins {
        .p passive "P" "1"
    }
}
"#;

fn solved(body: &str) -> Solution {
    let c = check(
        "test.nsm",
        &format!(
            "{PARTS}\nboard B {{\n    .supply: Rail {{\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }}\n{body}}}\n"
        ),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn faults(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// The checks the solver left open, as the build prints them.
fn open(s: &Solution) -> Vec<String> {
    let m = &s.model;
    let mut out: Vec<String> = s.not_evaluated.iter().map(|n| n.message.clone()).collect();
    out.extend(s.deferred.iter().map(|&i| {
        let r = &m.relations()[i];
        format!(
            "not evaluated: {} {} {}",
            m.show_expr(&r.lhs),
            r.op.symbol(),
            m.show_expr(&r.rhs)
        )
    }));
    out
}

const PROC: &str = "    .proc: Zproc\n    .supply ~ .proc.vdd\n";

#[test]
fn the_limits_of_an_unused_port_are_listed_without_the_statement_and_not_with_it() {
    let s = solved(PROC);
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(
        open(&s),
        [
            "not evaluated: proc.usb.vbus.voltage >= 4.35V",
            "not evaluated: proc.usb.vbus.voltage <= 5.5V"
        ]
    );
    let s = solved(&format!("{PROC}    unused .proc.usb\n"));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(open(&s), Vec::<String>::new());
}

#[test]
fn a_board_that_uses_the_port_still_gets_its_limits_checked() {
    // VBUS held to 3.3 V: below the 4.35 V the part needs.
    let c = check(
        "test.nsm",
        &format!(
            "{PARTS}\nboard B {{\n    .supply: Rail {{\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }}\n{PROC}    .vbus: Rail {{\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"VBUS\"\n        .neg.net &= \"GND\"\n    }}\n    .vbus ~ .proc.usb.vbus\n}}\n"
        ),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(
        faults(&s)
            .iter()
            .any(|m| m.contains("proc.usb.vbus.voltage") && m.contains("4.35V")),
        "{:?}",
        faults(&s)
    );
}

#[test]
fn an_absolute_maximum_of_an_unused_pad_is_not_read_against_its_net() {
    let body = "    .z: Zport\n    .supply ~ .z.vdd\n    .tp: Zpad\n    .tp.p ~ .z.in.line\n";
    let s = solved(body);
    assert!(
        open(&s)
            .iter()
            .any(|n| n.contains("the absolute maximum of `z.in.line` against net")),
        "{:?}",
        open(&s)
    );
    let s = solved(&format!("{body}    unused .z.in\n"));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert!(
        open(&s).iter().all(|n| !n.contains("z.in.line")),
        "{:?}",
        open(&s)
    );
}
