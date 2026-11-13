//! A `Capacitor` is one part and has no `.count`; several are a `CapacitorBank` or `N x
//! Capacitor` (ADR 0122, ADR 0119). A whole number written as the `.count` of a board's own
//! bank is refused and points at `N x Capacitor`; a count the solver chooses and one a module
//! passes through its interface are banks and stay.

use netsmith_check::*;

const HEAD: &str = "\
import std::parts::passive::{Capacitor, CapacitorBank}
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
fn a_capacitor_has_no_count_and_the_message_points_at_the_bank_and_the_group() {
    for body in [
        "    .bulk: Capacitor { .count &= 1..8 }\n",
        "    .bulk: Capacitor\n    .bulk.count &= 3\n",
    ] {
        let m = messages(&board(body));
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(
            m[0].contains("no member `count`") || m[0].contains("`count` is not a member"),
            "{}",
            m[0]
        );
        assert!(
            m[0].contains("a `CapacitorBank`") && m[0].contains("`3 x Capacitor`"),
            "{}",
            m[0]
        );
    }
}

#[test]
fn a_fixed_count_in_a_banks_block_is_refused_with_the_pointer_to_n_x() {
    let m = messages(&board("    .bulk: CapacitorBank { .count &= 3 }\n"));
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].contains("`.count` fixes the count of a bank the board itself declares")
            && m[0].contains("write `3 x Capacitor` instead")
            && m[0].contains("`.bulk[0]` to `.bulk[2]`"),
        "{}",
        m[0]
    );
}

#[test]
fn a_fixed_count_stated_outside_the_block_is_refused_too() {
    let m = messages(&board("    .bulk: CapacitorBank\n    .bulk.count &= 4\n"));
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].contains("`.bulk.count` fixes the count") && m[0].contains("write `4 x Capacitor`"),
        "{}",
        m[0]
    );
}

#[test]
fn a_count_of_one_is_a_plain_capacitor() {
    let m = messages(&board("    .bulk: CapacitorBank { .count &= 1 }\n"));
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(m[0].contains("write `.bulk: Capacitor`"), "{}", m[0]);
}

#[test]
fn a_count_the_solver_chooses_is_a_bank_and_stays() {
    let source = board(
        "    .bulk: CapacitorBank { .count &= 1..8 }\n    .bulk.count * .bulk.effective_capacitance >= 20uF\n",
    );
    let c = check("t.nsm", &source, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.boards[0].model;
    let bank = model.bank(model.inst_at("bulk").unwrap()).unwrap();
    assert_eq!(bank.member, "Capacitor");
    assert_eq!(model.field(bank.count).name, "count");
}

#[test]
fn a_default_range_and_a_bank_with_no_count_stay() {
    let source = board(
        "    .bulk: CapacitorBank { .count ?= 1..4 }\n    .bulk.count * .bulk.effective_capacitance >= 20uF\n",
    );
    let c = check("t.nsm", &source, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let c = check("t.nsm", &board("    .bulk: CapacitorBank\n"), &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_count_a_board_narrows_inside_a_module_stays() {
    // `Buck` has a bank on each side; a board states its count by the path (ADR 0116).
    let source = "\
import std::parts::regulator::{Buck, Lt8641}
import std::sources::Rail

board B {
    .vin: Rail { .voltage &= 12V +/- 5%; .pos.net &= \"VIN\"; .neg.net &= \"GND\" }
    .vout: Rail { .pos.net &= \"5V\"; .neg.net &= \"GND\" }
    .reg: Buck<Lt8641>
    .reg.output_cap.count &= 2
    .reg.vin ~ .vin
    .reg.vout ~ .vout
}
";
    let c = check("t.nsm", source, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_bank_a_module_declares_is_not_the_boards_own() {
    let module = "\
import std::parts::passive::CapacitorBank

module Bypass {
    .c: CapacitorBank { .count &= 2 }
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
