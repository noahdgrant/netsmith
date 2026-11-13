//! A whole number written as the `.count` of a board's own capacitor is refused and points at
//! `N x Capacitor` (ADR 0119, issue #685). A count the solver chooses, one a module passes
//! through its interface, and a capacitor a module declares are banks and stay (ADR 0052).

use netsmith_check::*;

const HEAD: &str = "\
import std::parts::passive::Capacitor
import std::sources::Rail

";

fn board(body: &str) -> String {
    format!(
        "{HEAD}board B {{
    .v: Rail {{
        .voltage &= 3.3V +/- 5%
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
    }}
{body}    .v.pos ~> .bulk ~> .v.neg
}}
"
    )
}

fn messages(source: &str) -> Vec<String> {
    let c = check("t.nsm", source, &Loader::std());
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn a_fixed_count_in_the_capacitors_block_is_refused_with_the_pointer_to_n_x() {
    let m = messages(&board("    .bulk: Capacitor { .count &= 3 }\n"));
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].contains("`.count` fixes the count of a capacitor the board itself declares")
            && m[0].contains("write `3 x Capacitor` instead")
            && m[0].contains("`.bulk[0]` to `.bulk[2]`"),
        "{}",
        m[0]
    );
}

#[test]
fn a_fixed_count_stated_outside_the_block_is_refused_too() {
    let m = messages(&board(
        "    .bulk: Capacitor\n    .bulk.count &= 4\n",
    ));
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].contains("`.bulk.count` fixes the count") && m[0].contains("write `4 x Capacitor`"),
        "{}",
        m[0]
    );
}

#[test]
fn a_count_of_one_is_a_plain_capacitor() {
    let m = messages(&board("    .bulk: Capacitor { .count &= 1 }\n"));
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(m[0].contains("a plain `Capacitor`: leave `.count` out"), "{}", m[0]);
}

#[test]
fn a_count_the_solver_chooses_is_a_bank_and_stays() {
    let source = board(
        "    .bulk: Capacitor { .count &= 1..8 }\n    .bulk.count * .bulk.effective_capacitance >= 20uF\n",
    );
    let c = check("t.nsm", &source, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.boards[0].model;
    assert!(model.bank_count(model.inst_at("bulk").unwrap()).is_some());
}

#[test]
fn a_count_that_reads_another_field_or_a_default_range_stays() {
    // A range default is the solver's choice, and a relation is not a statement of the count.
    let source = board(
        "    .bulk: Capacitor { .count ?= 1..4 }\n    .bulk.count * .bulk.effective_capacitance >= 20uF\n",
    );
    let c = check("t.nsm", &source, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_count_passed_through_a_modules_interface_stays() {
    let source = "\
import std::parts::passive::Capacitor
import std::parts::regulator::{Buck, Lt8641}
import std::sources::Rail

board B {
    .vin: Rail { .voltage &= 12V +/- 5%; .pos.net &= \"VIN\"; .neg.net &= \"GND\" }
    .vout: Rail { .pos.net &= \"5V\"; .neg.net &= \"GND\" }
    .reg: Buck<Lt8641>
    .reg.output_count &= 2
    .reg.vin ~ .vin
    .reg.vout ~ .vout
}
";
    let c = check("t.nsm", source, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_capacitor_a_module_declares_is_not_the_boards_own() {
    let module = "\
import std::parts::passive::Capacitor

module Bypass {
    .c: Capacitor { .count &= 2 }
}
";
    let source = "\
import src::bypass::Bypass

board B {
    .b: Bypass
}
";
    let mut loader = Loader::std();
    loader.insert("src::bypass", "src/bypass.nsm", module);
    let c = check("t.nsm", source, &loader);
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_part_of_the_projects_own_with_a_member_named_count_is_not_a_bank() {
    let source = "\
part Bar {
    pins {
        .a passive \"1\" \"1\"
        .b passive \"2\" \"2\"
    }
    .count: Number
}
board B {
    .bar: Bar { .count &= 8 }
}
";
    let c = check("t.nsm", source, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
}
