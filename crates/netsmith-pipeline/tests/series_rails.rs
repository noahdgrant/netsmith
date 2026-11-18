//! A pull-up fed from the rail behind a series bead (issue #1268): the build said `3V3F has no
//! source`, `has 2 loads with a current the solver has not settled` and `the DC analysis left
//! the network of `ldo` alone`, and fed from the rail in front of the bead it said nothing. Both
//! now build with no note. The part is invented; the solve-stage tests are in
//! `crates/netsmith-solve/tests/dc_series_rails.rs`.

use netsmith_check::Loader;
use netsmith_solve::Catalogs;

const SRC: &str = r#"
import std::bus::PullUp
import std::parts::passive::FerriteBead
import std::parts::regulator::LinearRegulator
import std::mfg::jlcpcb::TwoLayer
import std::sources::Rail

/// Zgate, an invented gate with an open-drain line that states no levels.
part Zgate {
    .vcc: Rail
    .vcc.current &= 100uA..1mA
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
        .a input "A" "3"
        .b open_drain "B" "4"
    }
}

board Bench {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .ldo: LinearRegulator { .vout.voltage &= 3.3V +/- 5% }
    .v33: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0A
    }
    .v33f: Rail {
        .pos.net &= "3V3F"
        .neg.net &= "GND"
        .current &= 0A
    }
    .bead: FerriteBead { .impedance &= 600ohm +/- 25%; .impedance_frequency &= 100MHz }
    .input ~ .ldo.vin
    .ldo.vout ~ .v33
    .v33.pos ~> .bead ~> .v33f.pos
    .near: Zgate
    .near.vcc ~ .v33
    .far: Zgate
    .far.vcc ~ .v33f
    .pull: PullUp { .resistor.resistance &= 10kohm +/- 5% }
    .pull.supply ~ FROM
    .pull.line ~ .far.b
    .far.a ~ .far.b
    .near.a ~ .far.b
}
"#;

/// The power notes and the solver's notes of the board with the pull-up fed from `from`.
fn notes(from: &str) -> (Vec<String>, Vec<String>) {
    let src = SRC.replace("FROM", from);
    let outcome = netsmith_pipeline::outcome_with(
        "bench.nsm",
        &src,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        None,
    )
    .unwrap_or_else(|p| panic!("{}", netsmith_pipeline::render_problems(&p)));
    let board = &outcome.boards[0];
    (board.power_notes(), board.notes.clone())
}

#[test]
fn a_pullup_fed_in_front_of_a_bead_has_no_note() {
    let (power, solver) = notes(".v33");
    assert!(power.is_empty(), "{power:#?}");
    assert!(solver.is_empty(), "{solver:#?}");
}

#[test]
fn a_pullup_fed_behind_a_bead_has_no_note() {
    let (power, solver) = notes(".v33f");
    assert!(power.is_empty(), "{power:#?}");
    assert!(solver.is_empty(), "{solver:#?}");
}
