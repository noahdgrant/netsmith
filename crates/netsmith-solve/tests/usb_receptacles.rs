//! The USB Type-C receptacles of a port that gives power (#463): the device on the cable draws
//! from the board, so a current that would supply the board is refused.
//!
//! The receptacle is invented for these tests.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

fn solved(current: &str) -> Solution {
    let src = format!(
        "import std::parts::connector::UsbCSourceReceptacle\nimport std::sources::Rail\npart R provides UsbCSourceReceptacle {{\n    pins {{\n        .vbus.pos power \"VBUS\" \"A4\"\n        .vbus.neg power \"GND\" \"A1\"\n        .dp bidirectional \"D+\" \"A6\"\n        .dm bidirectional \"D-\" \"A7\"\n        .cc1 passive \"CC1\" \"A5\"\n        .cc2 passive \"CC2\" \"B5\"\n        .sbu1 passive \"SBU1\" \"A8\"\n        .sbu2 passive \"SBU2\" \"B8\"\n        .shell passive \"SHELL\" \"S1\"\n    }}\n}}\nboard B {{\n    .vbus: Rail {{ .voltage &= 5V +/- 5% }}\n    .c: UsbCSourceReceptacle {{ .vbus.current &= {current} }}\n    .vbus ~ .c.vbus\n}}\n"
    );
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn faults(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn a_source_port_that_the_device_draws_from_is_accepted() {
    let s = solved("0A..3A");
    assert!(
        faults(&s).iter().all(|m| !m.contains("vbus.current")),
        "{:#?}",
        faults(&s)
    );
}

#[test]
fn a_source_port_that_would_supply_the_board_is_refused() {
    let s = solved("-1A");
    let f = faults(&s);
    assert_eq!(
        f,
        [
            "c.vbus.current must always be at least 0A, but -1A is not always above 0A (the currents on net `net-c.vbus.pos`: c.vbus.current (-1A) + vbus.current (1A) add up to zero)"
        ]
    );
}
