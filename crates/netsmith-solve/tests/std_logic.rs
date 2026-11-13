//! The standard library's logic IC, `std::parts::logic::Sn74lvc1g04`, whose levels and
//! absolute maximums are the numbers of TI's datasheet SCES214AF (ADR 0046). The guide shows
//! the messages; these tests pin the numbers.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const HEAD: &str = "import std::parts::logic::Sn74lvc1g04\nimport std::sources::Rail\n\
    import std::parts::connector::Header1x2\n";

fn run(body: &str) -> Solution {
    let src = format!("{HEAD}\nboard B {{\n{body}}}\n");
    let checked = check("test.nsm", &src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let board = checked.standalone_boards().next().expect("one board");
    solve(board, &Catalogs::jlcpcb())
}

fn rail(name: &str, volts: &str) -> String {
    format!(
        "    .{name}: Rail {{\n        .voltage &= {volts}\n        .pos.net &= \"{name}\"\n        .neg.net &= \"GND\"\n    }}\n"
    )
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn two_inverters_on_a_3v3_supply_read_each_other_with_no_fault() {
    let s = run(&format!(
        "{}    .u1: Sn74lvc1g04\n    .u2: Sn74lvc1g04\n    .j: Header1x2\n    .v ~ .u1.vcc\n    .v ~ .u2.vcc\n    .j.p1 ~ .u1.a.line\n    .j.p2 ~ .u2.y.line\n    .u1.y.line ~ .u2.a.line\n",
        rail("v", "3.3V +/- 5%")
    ));
    assert_eq!(messages(&s), Vec::<String>::new());
    // The one input nothing drives on the board says what it could not check.
    let notes: Vec<&str> = s.not_evaluated.iter().map(|n| n.message.as_str()).collect();
    assert_eq!(
        notes,
        [
            "not evaluated: the absolute maximum of `u1.a.line` against net `net-j.p1`: nothing sets its voltage: no rail is on it and no `Logic` line drives it"
        ]
    );
}

#[test]
fn a_supply_outside_the_row_the_part_states_is_refused_with_the_numbers() {
    let s = run(&format!(
        "{}    .u: Sn74lvc1g04\n    .v ~ .u.vcc\n",
        rail("v", "5V +/- 5%")
    ));
    let m = messages(&s);
    assert!(
        m.iter().any(|m| m.contains("u.vcc.voltage")
            && m.contains("4.75V..5.25V")
            && m.contains("3.6V")),
        "{m:?}"
    );
}

#[test]
fn an_input_on_a_5v_line_is_within_its_absolute_maximum_and_a_7v_one_is_not() {
    let driven = |volts: &str| {
        let hi = format!(
            "    .hi: Rail {{\n        .voltage &= {volts}\n        .pos.net &= \"HI\"\n        .neg.net &= \"GND\"\n    }}\n"
        );
        run(&format!(
            "{}{hi}    .u: Sn74lvc1g04\n    .j: Header1x2\n    .v ~ .u.vcc\n    .hi.pos ~ .u.a.line\n    .hi.neg ~ .v.neg\n    .j.p1 ~ .u.y.line\n",
            rail("v", "3.3V +/- 5%"),
        ))
    };
    assert_eq!(messages(&driven("5V +/- 5%")), Vec::<String>::new());
    let m = messages(&driven("7V +/- 1%"));
    assert!(
        m.iter().any(
            |m| m.contains("net `HI` reaches 7.07V above its ground `GND`")
                && m.contains("`u.a.line.abs_max` is 6.5V")
        ),
        "{m:?}"
    );
}
