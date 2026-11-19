//! The floor of a rail (issue #1301): a board lets a regulator's output fall below its rail's
//! range, to `.voltage_floor`, while the regulator is in dropout. The dropout check then needs an
//! input of the floor plus the dropout, and every supply range on the rail must hold the window
//! from the floor up. The rows and curves are invented, as in `dropout.rs`; the numbers are
//! worked out by hand in the comments.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

/// One invented 3.3 V +/- 2 % regulator (3.234 V to 3.366 V) with a dropout of 0.15 V at
/// 100 mA at 25 degC.
const LDO: &str = "mpn,vout.voltage,dropout,max_current,max_power,package,source
ZZL-33,3.3V +/- 2%,0.6V,0.5A,2W,SOT-23,https://example.invalid/zzl
";

const LDO_CURVE: &str = "temperature,output_current,dropout
25degC,0A,0.05V
25degC,100mA,0.15V
25degC,500mA,0.5V
";

fn catalogs() -> Catalogs {
    let mut c = Catalogs::empty();
    c.add_file("ldo", "catalogs/ldo.csv", LDO).unwrap();
    c.add_curve("ldo", "ZZL-33", "catalogs/curves/ldo/ZZL-33.csv", LDO_CURVE)
        .unwrap();
    c
}

/// A falling cell of 3.0 V to 4.2 V into the regulator, whose output feeds a load of up to
/// 100 mA, with `rest` after them.
fn board(rest: &str) -> String {
    format!(
        "import std::parts::regulator::LinearRegulator
import std::sources::Rail
import std::env::Environment

board B {{
    .cell: Rail {{ .voltage &= 3.0V..4.2V; .pos.net &= \"VBAT\" }}
    .u: LinearRegulator
    .cell ~ .u.vin
    .load: Rail {{ .current &= 0mA..100mA; .pos.net &= \"V33\" }}
    .u.vout ~ .load
    .environment: Environment {{ .temperature &= 25degC }}
{rest}
}}
"
    )
}

fn errors(src: &str) -> Vec<String> {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s: Solution = solve(&c.boards[0], &catalogs());
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// Without a floor the output must reach 3.234 V, which needs 3.384 V in: the cell's 3.0 V is
/// too low. With the output allowed down to 2.7 V it needs 2.85 V, which the cell holds.
#[test]
fn a_floor_lets_the_regulator_run_deeper_in_dropout() {
    let all = errors(&board("")).join("\n");
    assert!(all.contains("min(u.vout.voltage) + u.dropout"), "{all}");
    let with = errors(&board("    .load.voltage_floor &= 2.7V"));
    assert!(with.is_empty(), "{with:?}");
}

/// The floor is shared through `~`: stated on the regulator's own output it holds the board's
/// rail too.
#[test]
fn the_floor_is_shared_through_a_join() {
    let with = errors(&board("    .u.vout.voltage_floor &= 2.7V"));
    assert!(with.is_empty(), "{with:?}");
}

/// A supply range on the rail is judged over the whole window: a floor of 2.8 V holds a load that needs 2.75 V,
/// and a floor of 2.7 V does not hold one that needs 2.8 V: the error names the floor.
#[test]
fn a_load_must_hold_the_whole_window() {
    let ok = errors(&board("    .load.voltage_floor &= 2.8V\n    .load.voltage >= 2.75V"));
    assert!(ok.is_empty(), "{ok:?}");
    let all = errors(&board("    .load.voltage_floor &= 2.7V\n    .load.voltage >= 2.8V")).join("\n");
    // The message the guide quotes (chapter 28, _An output below its rail's range_).
    assert!(
        all.contains(
            "load.voltage must always be at least 2.8V, but 2.7V..3.366V is not always above 2.8V (with `load.voltage` over its window 2.7V..3.366V, from the rail's `.voltage_floor`: the least voltage it may fall to in dropout)"
        ),
        "{all}"
    );
}

/// A floor that is not below the rail's range is a mistake: the rail's own relation refuses it.
#[test]
fn a_floor_above_the_rails_least_voltage_is_refused() {
    let all = errors(&board(
        "    .cell.voltage &= 3.5V..4.2V\n    .load.voltage_floor &= 3.25V",
    ))
    .join("\n");
    assert!(all.contains("voltage_floor must be at most min("), "{all}");
}

/// A floor stated on another rail changes nothing here: the check still needs 3.384 V.
#[test]
fn a_floor_on_another_rail_changes_nothing() {
    let all = errors(&board(
        "    .other: Rail { .voltage &= 5V; .voltage_floor &= 2V; .pos.net &= \"V5\" }",
    ))
    .join("\n");
    assert!(all.contains("min(u.vout.voltage) + u.dropout"), "{all}");
}
