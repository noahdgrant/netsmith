//! `netsmith fab` and a board's environment (ADR 0099, ADR 0047): a generic part is judged once
//! `fab` has found the row that stands for it, a row outside the environment stops the run before
//! anything is placed, and `fab-notes.txt` says what the check did and did not do for a board
//! that has an environment, and nothing for one that has none. The boards and rows are invented.

use netsmith::Libraries;
use netsmith::fab::fab;
use netsmith_kicad::testlib::TempDir;
use netsmith_solve::Catalogs;
use std::fs;

/// The board: a header, a supply and `parts`, in `environment`.
fn board(parts: &str, environment: &str) -> String {
    format!(
        r#"
import std::env::Environment
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Invented {{
    .process: TwoLayer {{
        .stackup.thickness &= 1.6mm
    }}
    .supply: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }}
{parts}
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
{environment}}}
"#
    )
}

const WARM: &str = "    .environment: Environment { .temperature &= -40degC..85degC }\n";

/// 31.6 kohm: a row that states -20 C to 70 C. In an environment to 85 C the solver leaves it
/// out and keeps the generic part, and `fab` finds this row for it.
const OUTSIDE_ROW: &str = "lcsc,mpn,resistance,max_power,package,temp_min,temp_max,library,source\n\
    C99001,ACME-R1,31.6kohm +/- 1%,62.5mW,0402,253.15K,343.15K,Basic,https://example.invalid/acme.pdf\n";

/// 31.6 kohm with a range inside -40 C to 85 C, and 71.5 kohm with none.
const KNOWN_ROWS: &str = "lcsc,mpn,resistance,max_power,package,temp_min,temp_max,library,source\n\
    C99002,ACME-IN,31.6kohm +/- 1%,62.5mW,0402,-40degC,125degC,Basic,https://example.invalid/acme.pdf\n\
    C99003,ACME-NONE,71.5kohm +/- 1%,62.5mW,0402,,,Basic,https://example.invalid/acme.pdf\n";

fn run(
    source: &str,
    rows: &str,
    dir: &std::path::Path,
) -> Result<Vec<netsmith::fab::Fabricated>, String> {
    // A project that states an E-series is offered generic parts of it after the rows.
    let mut catalogs = Catalogs::jlcpcb().with_series(Some(netsmith_solve::Series::E96));
    catalogs
        .add_file("resistor", "catalogs/resistor.csv", rows)
        .unwrap();
    fab(
        "invented.nsm",
        source,
        dir,
        &catalogs,
        &Libraries::standard(),
        None,
    )
}

const ONE: &str =
    "    .r1: Resistor { .resistance &= 31.6kohm +/- 1% }\n    .supply.pos ~> .r1 ~> .supply.neg\n";
const TWO: &str = "    .r1: Resistor { .resistance &= 31.6kohm +/- 1% }\n    .r2: Resistor { .resistance &= 71.5kohm +/- 1% }\n    .supply.pos ~> .r1 ~> .r2 ~> .supply.neg\n";

#[test]
fn a_generic_part_whose_row_is_outside_the_environment_stops_fab_before_anything_is_written() {
    let dir = TempDir::unused("fab-environment-outside");
    let e = run(&board(ONE, WARM), OUTSIDE_ROW, &dir).expect_err("the row would be ordered");
    // The row states -20 C to 70 C: both ends of -40 C to 85 C are refused, each as a message.
    for want in [
        "error: Invented: `r1` works down to 253.15 K (-20degC) only (the catalog row ACME-R1 (C99001) states `temp_min`), over the board's environment, which reaches 233.15 K (-40degC)",
        "error: Invented: `r1` works up to 343.15 K (70degC) only (the catalog row ACME-R1 (C99001) states `temp_max`), under the board's environment, which reaches 358.15 K (85degC) (`environment.temperature` 233.15 K to 358.15 K (-40degC to 85degC)): choose a part rated for it or narrow the environment",
    ] {
        assert!(e.contains(want), "{want}\n{e}");
    }
    assert!(!dir.join("Invented").exists(), "files were written");
}

#[test]
fn the_same_generic_part_in_a_board_with_no_environment_is_not_judged() {
    let dir = TempDir::unused("fab-environment-none");
    run(&board(ONE, ""), OUTSIDE_ROW, &dir).unwrap_or_else(|e| panic!("{e}"));
    let notes = fs::read_to_string(dir.join("Invented").join("fab-notes.txt")).unwrap();
    assert!(!notes.contains("environment"), "{notes}");
}

#[test]
fn the_notes_say_what_the_check_did_for_how_many_parts_and_what_it_did_not() {
    let dir = TempDir::unused("fab-environment-notes");
    run(&board(TWO, WARM), KNOWN_ROWS, &dir).unwrap_or_else(|e| panic!("{e}"));
    let notes = fs::read_to_string(dir.join("Invented").join("fab-notes.txt")).unwrap();
    let lines: Vec<&str> = notes
        .lines()
        .filter(|l| l.starts_with("environment"))
        .collect();
    assert_eq!(
        lines,
        [
            "environment: temperature of each fitted part against `environment.temperature` 233.15 K to 358.15 K (-40degC to 85degC): 2 met, 0 NOT MET, 1 UNJUDGED (1 with a range stated at one end only or at neither)",
            "environment not checked: radiation, outgassing, screening and lot traceability (no catalog row states them), and the part's own heating",
        ],
        "{notes}"
    );
}
