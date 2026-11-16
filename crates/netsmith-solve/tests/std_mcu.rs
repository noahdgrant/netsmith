//! The standard library's microcontroller, `std::parts::mcu::Attiny13a`, whose functions,
//! logic levels and absolute maximums are the numbers of Microchip's data sheet DS40002307A
//! (ADR 0046). The guide shows the messages; these tests pin the numbers.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const HEAD: &str = "import std::parts::mcu::Attiny13a\nimport std::parts::logic::Sn74lvc1g04\n\
    import std::sources::Rail\nimport std::parts::connector::Header1x2\n";

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
fn the_part_is_picked_from_its_catalog_row_and_builds_on_a_3v3_supply() {
    let s = run(&format!(
        "{}    .u: Attiny13a {{ .package &= \"SOIC-8\" }}\n    .j: Header1x2\n    .v ~ .u.vcc\n    .j.p1 ~ .u.pb0.line\n    .u.pb0.line.function &= GPIO\n",
        rail("v", "3.3V +/- 5%")
    ));
    assert_eq!(messages(&s), Vec::<String>::new());
    let pick = s
        .picks
        .iter()
        .find(|p| s.model.path_of(p.part) == "u")
        .expect("the microcontroller is picked");
    assert_eq!(pick.catalog, "attiny13a");
    assert_eq!(pick.lcsc.as_deref(), Some("C14075"));
    assert_eq!(pick.mpn.as_deref(), Some("ATTINY13A-SSU"));
}

#[test]
fn a_supply_below_the_row_the_part_states_is_refused_with_the_numbers() {
    // The datasheet's rows for 1.8 V to 2.4 V have other levels, which the part does not state.
    let s = run(&format!(
        "{}    .u: Attiny13a\n    .v ~ .u.vcc\n",
        rail("v", "1.8V +/- 5%")
    ));
    let m = messages(&s);
    assert!(
        m.iter()
            .any(|m| m.contains("u.vcc.voltage") && m.contains("1.71V..1.89V") && m.contains("2.4V")),
        "{m:?}"
    );
}

#[test]
fn a_supply_above_5v5_is_refused() {
    let s = run(&format!(
        "{}    .u: Attiny13a\n    .v ~ .u.vcc\n",
        rail("v", "6V +/- 1%")
    ));
    let m = messages(&s);
    assert!(
        m.iter().any(|m| m.contains("u.vcc.voltage") && m.contains("5.5V")),
        "{m:?}"
    );
}

/// An inverter at 3.3 V drives a high of at least 3.035 V; the microcontroller at 3.3 V reads a
/// high from 0.6 VCC, 2.079 V at most. At 5 V it needs up to 3.15 V, which the inverter does not give.
#[test]
fn an_inverter_output_is_read_as_a_high_at_3v3_and_not_at_5v() {
    let board = |mcu: &str| {
        format!(
            "{}{}    .i: Sn74lvc1g04\n    .u: Attiny13a\n    .j: Header1x2\n    .a ~ .i.vcc\n    .b ~ .u.vcc\n    .j.p1 ~ .i.a.line\n    .i.y.line ~ .u.pb2.line\n    .u.pb2.line.function &= GPIO\n",
            rail("a", "3.3V +/- 5%"),
            rail("b", mcu),
        )
    };
    assert_eq!(messages(&run(&board("3.3V +/- 5%"))), Vec::<String>::new());
    let m = messages(&run(&board("5V +/- 5%")));
    assert!(
        m.iter().any(|m| m.contains("u.pb2.vih") && m.contains("i.y.voh")),
        "{m:?}"
    );
}

#[test]
fn a_connected_pin_with_several_functions_needs_one_chosen() {
    let src = format!(
        "{HEAD}\nboard B {{\n{}    .u: Attiny13a\n    .j: Header1x2\n    .v ~ .u.vcc\n    .j.p1 ~ .u.pb0.line\n}}\n",
        rail("v", "3.3V +/- 5%")
    );
    let checked = check("test.nsm", &src, &Loader::std());
    let text = if checked.is_ok() {
        let board = checked.standalone_boards().next().expect("one board");
        messages(&solve(board, &Catalogs::jlcpcb())).join("\n")
    } else {
        checked.render()
    };
    assert!(
        text.contains("u.pb0.line") && text.contains("several functions") && text.contains("MOSI"),
        "{text}"
    );
}
