//! A catalog row's `max_voltage` in steps over temperature (issue #1257): the column holds the
//! greatest step, a row whose steps do not hold the voltage across the part at a temperature the
//! board reaches is not offered, and a cell that is not steps of one voltage to one temperature
//! is refused with its file and line.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const HEAD: &str = "mpn,capacitance,effective_capacitance,max_voltage,package,library,source\n";

/// Two rows of 1 F: the first rated 5.5 V up to 70 degC and 4.2 V up to 85 degC, the second
/// 5.5 V up to 85 degC.
fn catalogs() -> Catalogs {
    let text = format!(
        "{HEAD}ZZS-A,1F +/- 20%,derate capacitance by bias:,\"5.5V to 70degC, 4.2V to 85degC\",1210,Basic,https://example.invalid/zzs-a.pdf\n\
ZZS-B,1F +/- 20%,derate capacitance by bias:,5.5V to 85degC,1210,Basic,https://example.invalid/zzs-b.pdf\n"
    );
    let mut c = Catalogs::empty();
    c.add_file("capacitor", "catalogs/capacitor.csv", &text)
        .unwrap();
    c
}

fn solved(environment: &str, rail: &str, catalogs: &Catalogs) -> Solution {
    let src = format!(
        "import std::env::Environment\nimport std::parts::passive::Capacitor\nimport std::sources::Rail\nboard B {{\n    .environment: Environment {{ .temperature &= {environment} }}\n    .supply: Rail {{ .voltage &= {rail}; .pos.net &= \"VBAK\" }}\n    .hold: Capacitor {{ .capacitance &= 1F +/- 20% }}\n    .supply.pos ~> .hold ~> .supply.neg\n}}\n"
    );
    let c = check("t.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], catalogs)
}

#[test]
fn a_row_whose_steps_hold_the_rail_is_picked_with_its_steps() {
    let s = solved("-20degC..85degC", "3.6V +/- 5%", &catalogs());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    assert_eq!(s.picks[0].candidate, "ZZS-A");
    let steps = s.picks[0].max_voltage_steps.as_ref().expect("the row's steps");
    assert_eq!(steps.steps, [(5.5, 343.15), (4.2, 358.15)]);
    // The column holds the greatest step.
    match s.model.domain_at("hold.max_voltage") {
        Some(netsmith_check::Domain::Quantity(q)) => assert_eq!((q.lo(), q.hi()), (5.5, 5.5)),
        other => panic!("the rating is {other:?}"),
    }
}

#[test]
fn a_row_whose_steps_break_the_rail_where_the_board_reaches_is_passed_over() {
    let s = solved("-20degC..85degC", "5V +/- 5%", &catalogs());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    assert_eq!(s.picks[0].candidate, "ZZS-B");
    // A board that stays at or under 70 degC reaches the first step alone, which holds 5 V.
    let s = solved("-20degC..70degC", "5V +/- 5%", &catalogs());
    assert_eq!(s.picks[0].candidate, "ZZS-A");
}

#[test]
fn a_cell_of_steps_is_one_voltage_to_one_temperature_rising() {
    for (cell, why) in [
        (
            "\"5.5V to 85degC, 4.2V to 70degC\"",
            "`70degC` is not above the temperature of the step before it",
        ),
        ("5.5V to 70C", "`70C` in the steps is not one temperature"),
        ("5V..6V to 70degC", "`5V..6V` in the steps is not one voltage"),
    ] {
        let text = format!(
            "{HEAD}ZZS-C,1F +/- 20%,derate capacitance by bias:,{cell},1210,Basic,https://example.invalid/zzs-c.pdf\n"
        );
        let mut c = Catalogs::empty();
        c.add_file("capacitor", "catalogs/capacitor.csv", &text)
            .unwrap();
        let s = solved("-20degC..85degC", "3.3V +/- 5%", &c);
        let messages: Vec<&str> = s.diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(
            messages
                .iter()
                .any(|m| m.contains("catalogs/capacitor.csv:2: `max_voltage`") && m.contains(why)),
            "{cell}: {messages:#?}"
        );
    }
}
