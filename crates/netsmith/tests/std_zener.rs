//! The standard `Zener` over temperature (issue #199): the test current, the temperature
//! coefficient and the leakage are optional catalog columns, and a board that states how far
//! the temperature goes from 25 C, `.temperature_shift`, has the voltage at the hot and the
//! cold corner from the row that was picked. A board that states no shift is unaffected.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, solve};

/// A Zener with no temperature figure, as a project's row that predates the columns: the file
/// leaves them out of its first line (ADR 0078).
const OLD_ROWS: &str = "lcsc,mpn,name,label,zener_voltage,max_power,package,library,source\n\
C900001,ZZ15,ZZ15 15V Zener,ZZ15,13.8V..15.6V,0.5W,SOD-123,Extended,https://example.invalid/z\n";

/// The messages of the checker, or, if it is clean, of the solver; and the names of the rows
/// it picked (`pick path name`) at the end.
fn run(src: &str, catalogs: &Catalogs) -> (Vec<String>, Vec<String>) {
    let checked = check("test.nsm", src, &Loader::std());
    if !checked.is_ok() {
        let m = checked.diagnostics.iter().map(|d| d.message.clone());
        return (m.collect(), Vec::new());
    }
    let mut messages = Vec::new();
    let mut picks = Vec::new();
    for board in checked.standalone_boards() {
        let s = solve(board, catalogs);
        messages.extend(s.diagnostics.iter().map(|d| d.message.clone()));
        picks.extend(
            s.picks
                .iter()
                .map(|p| format!("{} {}", s.model.path_of(p.part), p.candidate)),
        );
    }
    (messages, picks)
}

/// A 12 V rail with a Zener across it, and `body` after it.
fn board(zener: &str, body: &str) -> String {
    on_rail("12V +/- 5%", zener, body)
}

/// A rail of `voltage` with a Zener across it, and `body` after it.
fn on_rail(voltage: &str, zener: &str, body: &str) -> String {
    format!(
        "import std::parts::diode::Zener\nimport std::sources::Rail\nboard B {{\n    \
         .supply: Rail {{\n        .voltage &= {voltage}\n        .pos.net &= \"VIN\"\n        \
         .neg.net &= \"GND\"\n    }}\n    .z: Zener {{ {zener} }}\n{body}\n    \
         .supply.pos ~> .z ~> .supply.neg\n}}\n"
    )
}

const FIFTEEN: &str = ".zener_voltage &= 13V..16V";
const OVER_TEMPERATURE: &str = ".temperature_shift &= -65K..100K";

fn standard() -> Catalogs {
    Catalogs::jlcpcb()
}

#[test]
fn a_row_with_the_figures_gives_the_voltage_at_the_hot_and_the_cold_corner() {
    // BZT52C15: 13.8 V to 15.6 V at 5 mA, 9.2 to 13.0 mV/C. Over -40..125 C (-65 K and
    // +100 K from 25 C) the hot corner is 15.6 V + 13.0 mV * 100 = 16.9 V and the cold one
    // 13.8 V + 13.0 mV * (-65) = 12.955 V.
    let src = |limit: &str| {
        board(
            &format!("{FIFTEEN}\n {OVER_TEMPERATURE}"),
            &format!("    .z.voltage_over_temperature <= {limit}"),
        )
    };
    let (m, picks) = run(&src("18V"), &standard());
    assert!(m.is_empty(), "{m:#?}");
    assert_eq!(picks, ["z BZT52C15 15V 500mW Zener SOD-123 C2104"]);

    // The same row against a limit between the typical and the worst case is refused, and
    // the message names the hot corner of the row.
    let (m, _) = run(&src("16V"), &standard());
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("z.voltage_over_temperature must always be at most 16V")
            && m[0].contains("12.95V..16.9V"),
        "{}",
        m[0]
    );
}

#[test]
fn the_cold_corner_is_checked_with_the_lower_end_of_the_range() {
    // The rail's 12.6 V is under the 12.955 V of the cold corner: the clamp does not conduct.
    let src = |rail: &str| {
        on_rail(
            rail,
            &format!("{FIFTEEN}\n {OVER_TEMPERATURE}"),
            "    .z.bias <= .z.voltage_over_temperature",
        )
    };
    let (m, _) = run(&src("12V +/- 5%"), &standard());
    assert!(m.is_empty(), "{m:#?}");
    // 13.5 V is under the typical 13.8 V and over the cold corner's 12.955 V.
    let (m, _) = run(&src("13.5V"), &standard());
    assert!(!m.is_empty(), "{m:#?}");
    assert!(
        m.iter().any(
            |x| x.contains("z.bias must always be at most z.voltage_over_temperature")
                && x.contains("13.5V")
                && x.contains("12.95V..16.9V")
        ),
        "{m:#?}"
    );
}

#[test]
fn a_design_that_states_no_shift_is_unaffected() {
    // No relation over temperature, no note for it, and the pick is the one it was before the
    // columns existed: the same row, with or without the figures in the catalog.
    let src = board(FIFTEEN, "");
    let (with_figures, picks) = run(&src, &standard());
    let old = Catalogs::empty();
    let mut old = old;
    old.add_file("zener", "zener.csv", OLD_ROWS).unwrap();
    let (without_figures, old_picks) = run(&src, &old);
    assert!(with_figures.is_empty(), "{with_figures:#?}");
    assert!(without_figures.is_empty(), "{without_figures:#?}");
    assert_eq!(picks, ["z BZT52C15 15V 500mW Zener SOD-123 C2104"]);
    assert_eq!(old_picks, ["z ZZ15 15V Zener"]);
}

/// How many relations the solver could not evaluate on the board `src`.
fn deferred(src: &str) -> usize {
    let checked = check("test.nsm", src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    checked
        .standalone_boards()
        .map(|b| solve(b, &standard()).deferred.len())
        .sum()
}

#[test]
fn a_bound_on_the_voltage_without_a_shift_is_not_made_and_says_so() {
    // The library's relation is skipped (nothing states the shift), so the bound has no value
    // to check: it is one more relation listed as not evaluated, and is no error. The
    // relation itself is not listed: the board that states none has the same list as before.
    let plain = board(FIFTEEN, "");
    let bounded = board(FIFTEEN, "    .z.voltage_over_temperature <= 18V");
    let (m, picks) = run(&bounded, &standard());
    assert!(m.is_empty(), "{m:#?}");
    assert_eq!(picks.len(), 1);
    assert_eq!(deferred(&bounded), deferred(&plain) + 1);
    // With the shift stated the value exists and the bound is made, so nothing is deferred
    // for it.
    let shifted = board(
        &format!("{FIFTEEN}\n {OVER_TEMPERATURE}"),
        "    .z.voltage_over_temperature <= 18V",
    );
    assert_eq!(deferred(&shifted), deferred(&plain));
}

#[test]
fn a_shift_asks_for_a_row_that_states_the_coefficient() {
    // A row that has no figure is not offered to a design that needs one: the message names
    // the column and the row is not picked silently with wrong numbers.
    let mut old = Catalogs::empty();
    old.add_file("zener", "zener.csv", OLD_ROWS).unwrap();
    let src = board(
        &format!("{FIFTEEN}\n {OVER_TEMPERATURE}"),
        "    .z.voltage_over_temperature <= 18V",
    );
    let (m, picks) = run(&src, &old);
    assert!(picks.is_empty(), "{picks:#?}");
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(m[0].contains("temperature_coefficient"), "{}", m[0]);
}

#[test]
fn a_design_that_names_the_leakage_is_offered_only_rows_that_state_it() {
    let src = board(
        &format!("{FIFTEEN}\n .leakage_current <= 1uA\n .test_current >= 1mA"),
        "",
    );
    let (m, picks) = run(&src, &standard());
    assert!(m.is_empty(), "{m:#?}");
    assert_eq!(picks, ["z BZT52C15 15V 500mW Zener SOD-123 C2104"]);
    let mut old = Catalogs::empty();
    old.add_file("zener", "zener.csv", OLD_ROWS).unwrap();
    let (m, picks) = run(&src, &old);
    assert!(picks.is_empty());
    assert_eq!(m.len(), 1, "{m:#?}");
}
