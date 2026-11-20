//! Named corners of a board (issue #1256): a bound that reads a capacitor's effective
//! capacitance or ESR is judged again at each corner the board states, with the row's
//! `capacitance_at_end_of_life` and `esr_growth` at the end of life and its curve file over
//! temperature at the corner's temperature. Every row and curve here is invented: no real part's
//! end-of-life figures have a recorded source yet (`data/jlcpcb/README.md`).

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const HEAD: &str = "mpn,capacitance,effective_capacitance,max_voltage,package,esr,capacitance_at_end_of_life,esr_growth,source\n";

/// Keeps half of its capacitance and triples its ESR by the end of its life.
const LOSSY: &str = "ZZC-LOSSY,100uF +/- 20%,derate capacitance by bias: 0:1 16:1,16V,1210,20mohm,0.5,3,https://example.invalid/zzc-lossy.pdf\n";
/// Keeps 90 % and grows its ESR by half.
const STEADY: &str = "ZZC-STEADY,100uF +/- 20%,derate capacitance by bias: 0:1 16:1,16V,1210,20mohm,0.9,1.5,https://example.invalid/zzc-steady.pdf\n";
/// States nothing of its life.
const BARE: &str = "ZZC-BARE,100uF +/- 20%,derate capacitance by bias: 0:1 16:1,16V,1210,20mohm,,,https://example.invalid/zzc-bare.pdf\n";

fn catalogs(rows: &[&str]) -> Catalogs {
    let mut c = Catalogs::empty();
    crate::offered::add(&mut c, "capacitor", "catalogs/capacitor.csv", &format!("{HEAD}{}", rows.concat()))
        .unwrap();
    c
}

/// A board with one capacitor of 100 uF across 5 V, its effective capacitance and ESR named on
/// the board and bounded, and the corners `corners`.
fn board(corners: &str, bounds: &str) -> String {
    format!(
        "import std::env::{{Corner, Environment, Life}}
import std::parts::passive::Capacitor
import std::sources::Rail

board B {{
    .environment: Environment {{ .temperature &= -20degC..85degC }}
{corners}
    .supply: Rail {{ .voltage &= 5V +/- 5%; .pos.net &= \"V\" }}
    .c: Capacitor {{ .capacitance &= 100uF +/- 20% }}
    .supply.pos ~> .c ~> .supply.neg
    .kept: Capacitance
    .kept == .c.effective_capacitance
    .series: Resistance
    .series == .c.esr
{bounds}
}}
"
    )
}

const EOL: &str = "    .eol: Corner { .temperature &= 25degC; .life &= Life::End }";
const COLD: &str = "    .cold: Corner { .temperature &= -20degC; .life &= Life::New }";
const NEW: &str = "    .new: Corner { .temperature &= 25degC; .life &= Life::New }";

fn solved(src: &str, catalogs: &Catalogs) -> Solution {
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn errors(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn notes(s: &Solution) -> Vec<String> {
    s.not_evaluated.iter().map(|n| n.message.clone()).collect()
}

#[test]
fn a_board_with_no_corner_is_judged_on_the_rows_figures_alone() {
    let s = solved(&board("", "    .kept >= 66uF"), &catalogs(&[LOSSY, STEADY]));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert_eq!(s.picks[0].candidate, "ZZC-LOSSY");
}

#[test]
fn the_pick_takes_the_first_row_that_holds_at_every_corner() {
    // At the end of life the lossy row keeps 40 uF..60 uF, under 66 uF; the steady one 72 uF.
    let s = solved(&board(EOL, "    .kept >= 66uF"), &catalogs(&[LOSSY, STEADY]));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert_eq!(s.picks[0].candidate, "ZZC-STEADY");
    assert!(notes(&s).is_empty(), "{:?}", notes(&s));
}

#[test]
fn a_bound_that_fails_at_a_corner_names_the_corner_and_the_values() {
    let s = solved(&board(EOL, "    .series <= 50mohm"), &catalogs(&[LOSSY]));
    let errors = errors(&s);
    assert_eq!(errors.len(), 1, "{errors:?}");
    let e = &errors[0];
    assert!(
        e.contains("at the corner `eol` (298.15 K (25degC), at the end of life)"),
        "{e}"
    );
    assert!(e.contains("series must always be at most 50mΩ"), "{e}");
    // 20 mohm, three times.
    assert!(e.contains("`c.esr` is 60mΩ"), "{e}");
}

#[test]
fn a_corner_that_lacks_a_value_is_not_judged_and_is_named() {
    let s = solved(&board(EOL, "    .kept >= 66uF"), &catalogs(&[BARE]));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    let notes = notes(&s);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("at the corner `eol`"), "{}", notes[0]);
    assert!(notes[0].contains("`kept >= 66uF` is not evaluated"), "{}", notes[0]);
    assert!(
        notes[0].contains("`c` states no `capacitance_at_end_of_life`"),
        "{}",
        notes[0]
    );
}

#[test]
fn a_single_esr_figure_stands_at_25_degc_only() {
    let rows = catalogs(&[BARE]);
    let s = solved(&board(NEW, "    .series <= 50mohm"), &rows);
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert!(notes(&s).is_empty(), "{:?}", notes(&s));
    let s = solved(&board(COLD, "    .series <= 50mohm"), &rows);
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    let notes = notes(&s);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("no curve file over temperature"), "{}", notes[0]);
}

/// The ESR over temperature, falling as it warms.
const ESR_CURVE: &str = "temperature,esr
-40degC,200mohm
25degC,20mohm
85degC,15mohm
";

#[test]
fn the_esr_is_read_off_its_curve_at_the_corners_temperature() {
    let mut rows = catalogs(&[LOSSY]);
    rows.add_curve(
        "capacitor",
        "ZZC-LOSSY",
        "catalogs/curves/capacitor/ZZC-LOSSY.csv",
        ESR_CURVE,
    )
    .unwrap();
    // The environment's cold end, 0 degC here, gives 89.2 mohm and so the bound holds for the
    // row's own figures; at the end of life at 0 degC the ESR is three times that.
    let src = board(
        "    .eol_cold: Corner { .temperature &= 0degC; .life &= Life::End }",
        "    .series <= 100mohm",
    )
    .replace("-20degC..85degC", "0degC..85degC");
    let s = solved(&src, &rows);
    let errors = errors(&s);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("at the corner `eol_cold`"), "{}", errors[0]);
    // 200 - 180 * 40 / 65 = 89.23 mohm, times 3.
    assert!(errors[0].contains("`c.esr` is 267.7mΩ"), "{}", errors[0]);
}

#[test]
fn a_corner_is_one_temperature_inside_the_environment_with_a_life() {
    for (corner, want) in [
        (
            "    .hot: Corner { .temperature &= 100degC; .life &= Life::New }",
            "outside the board's environment",
        ),
        (
            "    .wide: Corner { .temperature &= 0degC..25degC; .life &= Life::New }",
            "a corner is one temperature",
        ),
        (
            "    .when: Corner { .temperature &= 25degC }",
            "states no life",
        ),
    ] {
        let s = solved(&board(corner, ""), &catalogs(&[LOSSY]));
        let errors = errors(&s);
        assert!(errors.iter().any(|e| e.contains(want)), "{corner}: {errors:?}");
    }
}
