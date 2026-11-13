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

// A Zener across a rail (issue #293, item 4, ADR 0098). The part states
// `min(.bias) <= max(.zener_voltage)`: a rail whose least voltage is above the greatest breakdown
// voltage puts the Zener in breakdown all the time, and is an error. A rail range that
// straddles the breakdown may conduct and is accepted: the rule refuses only the sure case.

/// The messages the checker and the solver print for a rail of `voltage` with a Zener across
/// it, and the rows picked.
fn across(voltage: &str, zener: &str, body: &str) -> (Vec<String>, Vec<String>) {
    run(&on_rail(voltage, zener, body), &standard())
}

#[test]
fn a_zener_that_conducts_on_its_rail_is_refused() {
    // 12 V +/- 5% (11.4 V at the least) across a Zener of 3.1 V to 3.5 V: it is in breakdown
    // all the time and nothing limits its current.
    let (m, picks) = across("12V +/- 5%", ".zener_voltage &= 3.1V..3.5V", "");
    assert!(picks.is_empty(), "{picks:#?}");
    // The pick says which row came closest, and the relation says it again as any failed
    // relation of a part does. Both name the part, the rail's least voltage (11.4 V) and the
    // greatest breakdown (3.5 V), and the relation says what to change.
    assert_eq!(m.len(), 2, "{m:#?}");
    assert!(
        m[0].starts_with("no `zener` candidate satisfies `z`"),
        "{}",
        m[0]
    );
    for message in &m {
        assert!(
            message.contains(
                "min(z.bias) must be at most max(z.zener_voltage), but 11.4V is above 3.5V"
            ) && message.contains("so change one of them"),
            "{message}"
        );
    }
}

#[test]
fn a_zener_across_a_rail_below_its_breakdown_is_accepted() {
    // 3.3 V +/- 5% across a 12 V Zener: it never conducts.
    let (m, picks) = across("3.3V +/- 5%", ".zener_voltage &= 11V..13V", "");
    assert!(m.is_empty(), "{m:#?}");
    assert_eq!(picks.len(), 1, "{picks:#?}");
}

#[test]
fn a_rail_range_that_straddles_the_breakdown_is_accepted() {
    // 4 V to 6 V against 4.7 V to 5.5 V: it may conduct, and the sure case is all the rule
    // refuses.
    let (m, picks) = across("4V..6V", ".zener_voltage &= 4.7V..5.5V", "");
    assert!(m.is_empty(), "{m:#?}");
    assert_eq!(picks.len(), 1, "{picks:#?}");
    // The edge: the row that is picked (BZT52C5V1, 4.8 V to 5.4 V) is met by a rail whose
    // least voltage is its greatest, 5.4 V, and not by one 0.1 V above it.
    let (m, picks) = across("5.4V..9V", ".zener_voltage &= 4.7V..5.5V", "");
    assert!(m.is_empty(), "{m:#?}");
    assert_eq!(picks.len(), 1, "{picks:#?}");
    let (m, picks) = across("5.5V..9V", ".zener_voltage &= 4.7V..5.5V", "");
    assert!(picks.is_empty(), "{picks:#?}");
    assert!(m.iter().any(|x| x.contains("5.5V is above 5.4V")), "{m:#?}");
}

#[test]
fn a_shunt_regulator_behind_a_series_resistor_is_accepted() {
    // The Zener's ends are not both on the rail: its voltage is the node behind the resistor,
    // which is not a rail, so it has no `.bias` of the rail's, and the rule waits.
    let src = "import std::parts::diode::Zener\nimport std::parts::passive::Resistor\n\
               import std::sources::Rail\nboard B {\n    .supply: Rail {\n        \
               .voltage &= 12V +/- 5%\n        .pos.net &= \"VIN\"\n        \
               .neg.net &= \"GND\"\n    }\n    .r: Resistor { .resistance &= 1kohm +/- 1% }\n    \
               .z: Zener { .zener_voltage &= 4.7V..5.5V }\n    \
               .supply.pos ~> .r ~> .z ~> .supply.neg\n}\n";
    let (m, picks) = run(src, &standard());
    assert!(m.is_empty(), "{m:#?}");
    assert_eq!(picks.len(), 2, "{picks:#?}");
}

#[test]
fn a_zener_that_states_no_voltage_across_12v_is_given_a_row_that_reaches_the_rail() {
    // The report of issue #293. The rule also chooses among the rows, as any relation of a
    // part does, so a design that states no breakdown is not refused: the first row of the
    // catalog whose greatest breakdown voltage reaches the rail's least is picked (a 3.3 V
    // row was before the rule). Only a design that states a breakdown that cannot reach the
    // rail gets the error above.
    let (m, picks) = across("12V +/- 5%", "", "");
    assert!(m.is_empty(), "{m:#?}");
    assert_eq!(picks.len(), 1, "{picks:#?}");
    assert!(picks[0].contains("BZT52C12 12V"), "{picks:#?}");
}

// The shift from the board's environment (ENV2-Z, ADR 0099): a board that states an environment
// and bounds `.voltage_over_temperature` is given `.temperature_shift` as the environment's
// temperature less 25 C; a shift the board states overrides it.

/// A board with `.environment: Environment { .temperature &= <range> }`, and the Zener's
/// own statements `zener` and `body` after it.
fn in_environment(range: &str, zener: &str, body: &str) -> String {
    let src = board(zener, body);
    src.replacen(
        "import std::sources::Rail\n",
        "import std::sources::Rail\nimport std::env::Environment\n",
        1,
    )
    .replacen(
        "    .z: Zener",
        &format!("    .environment: Environment {{ .temperature &= {range} }}\n    .z: Zener"),
        1,
    )
}

const BOUND: &str = "    .z.voltage_over_temperature <= 18V";

#[test]
fn the_shift_is_derived_from_the_environment_and_gives_the_worst_case_band() {
    // -40 degC to 125 degC is -65 K..100 K from 25 degC: the band of the BZT52C15 is the
    // one that the same board states by hand (12.955 V..16.9 V).
    let derived = in_environment("-40degC..125degC", FIFTEEN, BOUND);
    let by_hand = board(&format!("{FIFTEEN}\n {OVER_TEMPERATURE}"), BOUND);
    assert_eq!(run(&derived, &standard()), run(&by_hand, &standard()));
    let (m, picks) = run(&derived, &standard());
    assert!(m.is_empty(), "{m:#?}");
    assert_eq!(picks, ["z BZT52C15 15V 500mW Zener SOD-123 C2104"]);

    // The bound is made over that band: a limit between the typical and the worst case is
    // refused with the derived hot and cold corners, as a stated shift gives.
    let tight = in_environment(
        "-40degC..125degC",
        FIFTEEN,
        "    .z.voltage_over_temperature <= 16V",
    );
    let (m, _) = run(&tight, &standard());
    assert!(
        m.iter().any(
            |x| x.contains("z.voltage_over_temperature must always be at most 16V")
                && x.contains("12.95V..16.9V")
        ),
        "{m:#?}"
    );
}

#[test]
fn a_milder_environment_gives_a_narrower_band() {
    // 0 degC to 70 degC is -25 K..45 K: 15.6 V + 13 mV * 45 = 16.185 V, so a limit of
    // 16.5 V that the -40..125 degC board fails is met.
    let src =
        |range: &str| in_environment(range, FIFTEEN, "    .z.voltage_over_temperature <= 16.5V");
    let (m, picks) = run(&src("0degC..70degC"), &standard());
    assert!(m.is_empty(), "{m:#?}");
    assert_eq!(picks.len(), 1);
    let (m, _) = run(&src("-40degC..125degC"), &standard());
    assert!(!m.is_empty());
}

#[test]
fn a_stated_shift_overrides_the_environment() {
    // The board states 0 K..10 K (25 degC to 35 degC) in an environment of -40 degC to 125
    // degC: the stated shift is the one used, so the 16 V limit that the environment's own
    // shift (16.9 V at the hot corner) breaks is met by 15.6 V + 13 mV * 10 = 15.73 V.
    let stated = in_environment(
        "-40degC..125degC",
        &format!("{FIFTEEN}\n .temperature_shift &= 0K..10K"),
        "    .z.voltage_over_temperature <= 16V",
    );
    let (m, picks) = run(&stated, &standard());
    assert!(m.is_empty(), "{m:#?}");
    assert_eq!(picks.len(), 1);
    let derived = in_environment(
        "-40degC..125degC",
        FIFTEEN,
        "    .z.voltage_over_temperature <= 16V",
    );
    assert!(!run(&derived, &standard()).0.is_empty());
}

#[test]
fn no_environment_leaves_the_shift_unstated() {
    // No environment: the bound is listed as not evaluated, as before, and nothing is derived.
    let plain = board(FIFTEEN, BOUND);
    let (m, picks) = run(&plain, &standard());
    assert!(m.is_empty(), "{m:#?}");
    assert_eq!(picks.len(), 1);
    assert_eq!(deferred(&plain), deferred(&board(FIFTEEN, "")) + 1);
}

#[test]
fn an_environment_changes_nothing_for_a_board_that_does_not_bound_the_voltage() {
    // The environment is stated for another reason, and the Zener is picked from a project
    // catalog whose row states no coefficient: the row is offered and picked as it is with no
    // environment, and nothing is deferred beyond what the plain board defers.
    let mut old = Catalogs::empty();
    old.add_file("zener", "zener.csv", OLD_ROWS).unwrap();
    let plain = board(FIFTEEN, "");
    let with_environment = in_environment("-40degC..125degC", FIFTEEN, "");
    let (m, picks) = run(&with_environment, &old);
    assert!(m.is_empty(), "{m:#?}");
    assert_eq!(picks, ["z ZZ15 15V Zener"]);
    assert_eq!(run(&plain, &old), (m, picks));
    assert_eq!(deferred(&with_environment), deferred(&plain));
}

#[test]
fn a_row_with_no_coefficient_is_not_offered_to_a_bound_over_the_environment() {
    // The design asks for the voltage over the environment's range and the row cannot give
    // it: it is not picked with a wrong number, and the message names the column.
    let mut old = Catalogs::empty();
    old.add_file("zener", "zener.csv", OLD_ROWS).unwrap();
    let src = in_environment("-40degC..125degC", FIFTEEN, BOUND);
    let (m, picks) = run(&src, &old);
    assert!(picks.is_empty(), "{picks:#?}");
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(m[0].contains("temperature_coefficient"), "{}", m[0]);
}

#[test]
fn an_environment_that_does_not_hold_25_degc_gives_the_band_over_its_own_range() {
    // 50 degC to 80 degC is 25 K..55 K: the voltage is over that range, not widened to the
    // 25 degC the datasheet states. 13.8 V + 9.2 mV * 25 = 14.03 V at the least, and
    // 15.6 V + 13 mV * 55 = 16.315 V at the most.
    let src = in_environment(
        "50degC..80degC",
        FIFTEEN,
        "    .z.voltage_over_temperature >= 14V",
    );
    let (m, picks) = run(&src, &standard());
    assert!(m.is_empty(), "{m:#?}");
    assert_eq!(picks.len(), 1);
    let src = in_environment(
        "50degC..80degC",
        FIFTEEN,
        "    .z.voltage_over_temperature >= 14.1V",
    );
    assert!(!run(&src, &standard()).0.is_empty());
}
