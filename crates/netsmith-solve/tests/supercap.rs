//! The supercapacitor (issue #1261): `std::parts::passive::Supercap`, one part of one cell or
//! two, with an optional balance tap `.mid`. Every row here is invented: no real part has a
//! recorded source yet (`data/jlcpcb/README.md`), so the standard catalog `supercap` is empty.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const HEAD: &str = "mpn,capacitance,esr,dc_esr,leakage_current,max_voltage,cells,soldering,package,height_growth,source\n";

/// One cell, 2.7 V, in a two-lead package.
const SINGLE: &str = "ZZS-1F,1F +/- 20%,60mohm,90mohm,4uA,2.7V,1,Reflow,Radial_D8.0mm_P3.50mm,0.5mm,https://example.invalid/zzs-1f.pdf\n";
/// Two cells with the tap, 5.5 V to 65 degC and 4.2 V to 85 degC.
const TAPPED: &str = "ZZS-0F5-T,0.5F +/- 20%,120mohm,200mohm,6uA,\"5.5V to 65degC, 4.2V to 85degC\",2,Hand,Radial_D16.0mm_P7.50mm_Tap,1mm,https://example.invalid/zzs-0f5-t.pdf\n";
/// Two cells with no tap, 5.5 V at every temperature.
const DUAL: &str = "ZZS-0F5,0.5F +/- 20%,120mohm,200mohm,6uA,5.5V,2,Hand,Radial_D16.0mm_P7.50mm,1mm,https://example.invalid/zzs-0f5.pdf\n";

fn catalogs(rows: &[&str]) -> Catalogs {
    let mut c = Catalogs::empty();
    crate::offered::add(&mut c, 
        "supercap",
        "catalogs/supercap.csv",
        &format!("{HEAD}{}", rows.concat()),
    )
    .unwrap();
    c
}

fn board(volts: &str, lines: &str) -> String {
    format!(
        "import std::parts::passive::Supercap
import std::sources::Rail

board B {{
    .supply: Rail {{ .voltage &= {volts}; .pos.net &= \"V\" }}
    .sc: Supercap
    .supply.pos ~> .sc ~> .supply.neg
{lines}
}}
"
    )
}

fn solved(src: &str, catalogs: &Catalogs) -> Solution {
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn errors(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn the_first_row_rated_for_the_rail_is_picked() {
    let s = solved(&board("5V", ""), &catalogs(&[SINGLE, DUAL]));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert_eq!(s.picks[0].candidate, "ZZS-0F5");
}

#[test]
fn a_board_that_connects_the_tap_is_given_a_row_that_has_one() {
    let lines = "    .balance: Rail { .voltage &= 2.5V; .pos.net &= \"MID\" }\n    .sc.mid ~ .balance.pos";
    let s = solved(&board("5V", lines), &catalogs(&[SINGLE, DUAL, TAPPED]));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert_eq!(s.picks[0].candidate, "ZZS-0F5-T");
}

#[test]
fn the_voltage_of_a_cell_is_the_bias_over_the_cells() {
    let lines = "    .each: Voltage\n    .each == .sc.cell_voltage\n    .each <= 2.6V";
    let s = solved(&board("5V", lines), &catalogs(&[DUAL]));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    let lines = "    .each: Voltage\n    .each == .sc.cell_voltage\n    .each <= 2.4V";
    let s = solved(&board("5V", lines), &catalogs(&[DUAL]));
    assert!(!errors(&s).is_empty(), "2.5 V a cell is over 2.4 V");
}

#[test]
fn a_supercapacitor_wired_the_wrong_way_round_is_refused() {
    let src = "import std::parts::passive::Supercap
import std::sources::Rail

board B {
    .supply: Rail { .voltage &= 5V; .pos.net &= \"V\" }
    .sc: Supercap
    .supply.neg ~> .sc ~> .supply.pos
}
";
    let s = solved(src, &catalogs(&[DUAL]));
    let errors = errors(&s);
    assert!(!errors.is_empty(), "a reversed supercapacitor builds");
    assert!(
        errors
            .iter()
            .any(|e| e.contains("sc.bias must always be at least 0V, but -5V is not always above 0V")),
        "{errors:?}"
    );
}

#[test]
fn a_rail_over_every_rating_finds_no_row() {
    let s = solved(&board("12V", ""), &catalogs(&[SINGLE, DUAL]));
    let errors = errors(&s);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("sc.bias must always be at most sc.max_voltage, but 12V is not always below 5.5V"),
        "{}",
        errors[0]
    );
}

#[test]
fn a_row_whose_steps_do_not_hold_the_rail_where_the_board_reaches_is_passed_over() {
    let src = |hot: &str| {
        board(
            "5V",
            &format!("    .environment: Environment {{ .temperature &= -20degC..{hot} }}"),
        )
        .replace("import std::sources::Rail", "import std::env::Environment\nimport std::sources::Rail")
    };
    // Up to 60 degC the first step, 5.5 V, holds 5 V.
    let s = solved(&src("60degC"), &catalogs(&[TAPPED]));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    // At 85 degC the second, 4.2 V, does not.
    let s = solved(&src("85degC"), &catalogs(&[TAPPED]));
    let errors = errors(&s);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("1 row was left out because its maximum voltage, stated in steps over temperature, is under the 5 V across `sc`"),
        "{}",
        errors[0]
    );
}

#[test]
fn a_supercapacitor_across_a_rail_holds_a_burst_the_source_cannot_give() {
    // 50 mA more than the source gives, for 3 s, is 150 mC: 0.4 F (0.5 F less its 20 %) falls
    // 0.375 V from 3.201 V, so the radio's 2.8 V holds and 3 V does not.
    let src = |least: &str| {
        format!(
            "import std::parts::passive::Supercap
import std::sources::Rail

board B {{
    .supply: Rail {{ .voltage &= 3.3V +/- 3%; .pos.net &= \"3V3\"; .max_peak_current &= 600mA }}
    .mcu: Rail {{ .current &= 5mA..150mA }}
    .radio: Rail {{ .current &= 10mA..40mA; .peak_current &= 500mA; .burst &= 3s }}
    .supply ~ .mcu
    .supply ~ .radio
    .radio.voltage >= {least}
    .hold: Supercap
    .supply.pos ~> .hold ~> .supply.neg
}}
"
        )
    };
    let s = solved(&src("2.8V"), &catalogs(&[DUAL]));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    let s = solved(&src("3V"), &catalogs(&[DUAL]));
    let errors = errors(&s);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("below the 3V that `radio` needs"), "{}", errors[0]);
}
