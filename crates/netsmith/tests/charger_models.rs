//! The models of three parts of the Power Manifold charger module (#473): the TPS55288
//! buck-boost converter at DC (`dc switching_regulator`, its windows, its efficiency and its
//! quiescent current), the TCPP02-M18 protector's gate drive and overcurrent threshold, and the
//! standard `NtcThermistor` that the charger's ANTC1608 thermistor provides. Each is judged on
//! a small board of the test's own (never the project's board), checked and solved with the
//! project's parts and catalogs, and a value outside a datasheet window is refused with its
//! message.

use std::path::PathBuf;

use netsmith::project::Target;
use netsmith_check::{Domain, Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Catalogs, Solution, solve};

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/power_manifold_charger")
}

/// Check and solve the only board of `src` with the charger's own parts and catalogs.
fn run(src: &str) -> Solution {
    let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
    let checked = check("fixture.nsm", src, &target.loader().unwrap());
    assert!(checked.is_ok(), "{}", checked.render());
    let board = checked.standalone_boards().next().expect("one board");
    solve(board, &target.catalogs().unwrap())
}

/// Check and solve with the standard library and catalogs only.
fn run_std(src: &str) -> Solution {
    let checked = check("fixture.nsm", src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let board = checked.standalone_boards().next().expect("one board");
    solve(board, &Catalogs::jlcpcb())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> Quantity {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) => *q,
        other => panic!("`{path}` is {other:?}"),
    }
}

/// The relations the solver could not evaluate, as text.
fn deferred(s: &Solution) -> Vec<String> {
    let relations = s.model.relations();
    s.deferred
        .iter()
        .map(|&i| {
            let r = &relations[i];
            format!(
                "{} {} {}",
                s.model.show_expr(&r.lhs),
                r.op.symbol(),
                s.model.show_expr(&r.rhs)
            )
        })
        .collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1e-12)
}

/// A connector that gives a supply, for the fixtures.
const PLUG: &str = r#"
/// A connector that gives the supply.
part Plug {
    .designator &= "J"
    .supply: Rail
    dc source .supply
    pins {
        .supply.pos power "V" "1"
        .supply.neg power "G" "2"
    }
}
"#;

// ---- TPS55288: a buck-boost converter at DC -----------------------------------------------

/// The converter from a supply of `vin` to a load rail of `vout` drawing `load`.
fn converter(vin: &str, vout: &str, load: &str) -> String {
    format!(
        r#"
import src::tps55288::Tps55288
import std::parts::connector::{{Header1x4, Header1x8}}
import std::sources::Rail
{PLUG}
board ConverterFixture {{
    .plug: Plug {{
        .supply.voltage &= {vin}
        .supply.pos.net &= "VIN"
        .supply.neg.net &= "GND"
    }}
    .load: Rail {{
        .voltage &= {vout}
        .current &= {load}
        .pos.net &= "VOUT"
        .neg.net &= "GND"
    }}
    .u3: Tps55288 {{ .mpn &= "TPS55288RPMR" }}
    .u3.vin ~ .plug.supply
    .u3.vout ~ .load
    .plug.supply.neg ~ .u3.agnd
    .conv: Header1x8
    .u3.en ~ .conv.p1
    .u3.scl ~ .conv.p2
    .u3.sda ~ .conv.p3
    .u3.dith ~ .conv.p4
    .u3.mode ~ .conv.p5
    .u3.comp ~ .conv.p6
    .u3.isp ~ .conv.p7
    .u3.isn ~ .conv.p8
    .sw: Header1x4
    .u3.sw1 ~ .sw.p1
    .u3.sw2 ~ .sw.p2
    .u3.boot1 ~ .sw.p3
    .u3.boot2 ~ .sw.p4
}}
"#
    )
}

// ---- TCPP02-M18: the gate drive and the overcurrent trip -----------------------------------

/// The port switch of the charger: the protector drives the gates of two back-to-back MOSFETs
/// and senses the port's current across a shunt. `extra` is more lines in the board.
fn protector(extra: &str) -> String {
    format!(
        r#"
import src::bsc059n04ls6::Bsc059n04ls6
import src::tcpp02_m18::Tcpp02M18
import std::parts::connector::Header1x8
import std::parts::passive::Resistor
import std::sources::Rail

board ProtectorFixture {{
    .v5: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= "+5V"
        .neg.net &= "GND"
    }}
    .u5: Tcpp02M18 {{ .mpn &= "TCPP02-M18" }}
    .q_a: Bsc059n04ls6 {{ .mpn &= "BSC059N04LS6" }}
    .q_b: Bsc059n04ls6 {{ .mpn &= "BSC059N04LS6" }}
    .shunt: Resistor {{ .resistance &= 7mohm +/- 1%; .package &= "1206" }}
    .u5.vcc ~ .v5
    .q_a.source ~ .q_b.source
    .q_a.source ~ .u5.src
    .q_a.gate ~ .q_b.gate
    .q_a.gate ~ .u5.gate
    .q_b.drain ~ .u5.isense
    .q_b.drain ~ .shunt.b
    .shunt.a ~ .u5.vbusc
    .io: Header1x8
    .q_a.drain ~ .io.p1
    .shunt.a ~ .io.p2
    .u5.en ~ .io.p3
    .u5.i2c_add ~ .io.p4
    .u5.cc1 ~ .io.p5
    .u5.cc1c ~ .io.p6
    .u5.cbias ~ .io.p7

    .q_a.gate_source_voltage == .u5.gate_drive
    .q_b.gate_source_voltage == .u5.gate_drive
    .trip_current: Current
    .trip_current == .u5.ocp_threshold / .shunt.resistance
    .trip_current <= .q_a.max_drain_current
    .trip_current <= .q_b.max_drain_current
    .u5.ocp_threshold * .trip_current <= .shunt.max_power
    {extra}
}}
"#
    )
}

// ---- NtcThermistor: a resistor over the maker's table ------------------------------------

/// The charger's thermistor under a 10 kohm pull-up from a 3.3 V supply. `extra` is more lines
/// in the thermistor's body.
fn thermistor(extra: &str) -> String {
    format!(
        r#"
import src::antc1608_ntc::Antc1608Ntc
import std::parts::passive::Resistor
import std::sources::Rail
{PLUG}
board ThermistorFixture {{
    .plug: Plug {{
        .supply.voltage &= 3.3V +/- 5%
        .supply.pos.net &= "+3.3V"
        .supply.neg.net &= "GND"
    }}
    .pullup: Resistor {{ .resistance &= 10kohm +/- 1%; .package &= "0402" }}
    .th: Antc1608Ntc {{ .mpn &= "ANTC1608-103F3380FB"; {extra} }}
    .plug.supply.pos ~ .pullup.a
    .pullup.b ~ .th.a
    .th.b ~ .plug.supply.neg
}}
"#
    )
}

#[test]
fn the_buck_boost_draws_its_input_from_its_output_over_the_figures_efficiency() {
    // 3 A at 20 V +/- 2 % from 12 V +/- 5 %: the most the input draws is the most power out
    // over the least input and the least efficiency of Figures 6-1 to 6-10 (0.80), with the
    // 860 uA the IC keeps for itself (SLVSF01B 6.5, p.6).
    let s = run(&converter("12V +/- 5%", "20V +/- 2%", "0A..3A"));
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(deferred(&s).is_empty(), "{:#?}", deferred(&s));
    let vin = range(&s, "u3.vin.voltage");
    assert!(close(vin.lo(), 11.4) && close(vin.hi(), 12.6), "{vin}");
    let iin = range(&s, "u3.vin.current");
    assert_eq!(iin.lo(), 0.0, "{iin}");
    let most = 3.0 * 20.4 / (11.4 * 0.80) + 860e-6;
    assert!(close(iin.hi(), most), "{iin}, not {most}");
}

#[test]
fn a_board_states_the_efficiency_it_reads_off_the_figures_for_its_own_point() {
    // The default is a range the board may replace with its own reading (`?=`): 95 % flat.
    let src = converter("12V +/- 5%", "20V +/- 2%", "0A..3A").replace(
        ".mpn &= \"TPS55288RPMR\"",
        ".mpn &= \"TPS55288RPMR\"; .efficiency &= 0.95",
    );
    let s = run(&src);
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let most = 3.0 * 20.4 / (11.4 * 0.95) + 860e-6;
    assert!(close(range(&s, "u3.vin.current").hi(), most));
}

#[test]
fn an_output_or_an_input_outside_the_datasheets_window_is_refused() {
    // SLVSF01B 6.3, p.5: VOUT 0.8 V to 22 V, VIN 2.7 V to 36 V.
    let m = messages(&run(&converter("12V +/- 5%", "24V", "0A..1A")));
    assert!(
        m.iter()
            .any(|m| m
                == "u3.vout.voltage must always be at most 22V, but 24V is not always below 22V"),
        "{m:#?}"
    );
    let m = messages(&run(&converter("40V", "20V", "0A..1A")));
    assert!(
        m.iter()
            .any(|m| m
                == "u3.vin.voltage must always be at most 36V, but 40V is not always below 36V"),
        "{m:#?}"
    );
}

#[test]
fn the_protectors_gate_drive_judges_the_switches_and_its_threshold_the_trip_current() {
    let s = run(&protector(""));
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    // The gate drive is 4.5 V to 5.5 V (DS13787 Rev 4, Table 6, p.6), so the three gate checks
    // of each MOSFET are judged (and hold: 1.3 V to 2.3 V threshold, 4.5 V for the on-resistance,
    // 20 V most) and none is left over.
    let gate = range(&s, "q_a.gate_source_voltage");
    assert!(close(gate.lo(), 4.5) && close(gate.hi(), 5.5), "{gate}");
    let left = deferred(&s);
    assert!(
        !left.iter().any(|r| r.contains("gate_source_voltage")),
        "{left:#?}"
    );
    // 35 mV to 45 mV over 7 mohm +/- 1 %: the trip current, judged against the MOSFETs' 17 A
    // and the shunt's 1 W, which it does not reach.
    let trip = range(&s, "trip_current");
    assert!(
        close(trip.lo(), 0.035 / 0.00707) && close(trip.hi(), 0.045 / 0.00693),
        "{trip}"
    );
    assert!(
        !left.iter().any(|r| r.contains("trip_current")),
        "{left:#?}"
    );
}

#[test]
fn a_gate_drive_outside_the_protectors_table_is_refused() {
    let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
    let checked = check(
        "fixture.nsm",
        &protector(".u5.gate_drive &= 6V"),
        &target.loader().unwrap(),
    );
    assert!(!checked.is_ok());
    let text = checked.render();
    assert!(
        text.contains(
            "contradiction on `u5.gate_drive`: it is already 4.5V..5.5V, which does not overlap 6V"
        ),
        "{text}"
    );
}

#[test]
fn the_thermistor_is_a_resistor_over_the_makers_table_at_dc() {
    // 197.39 kohm at -40 C to 0.532 kohm at 125 C (APV ANTC series, revised 2026-06-21, p.10),
    // under 10 kohm +/- 1 % from 3.3 V +/- 5 %: the divider is solved over the whole range.
    let s = run(&thermistor(""));
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert!(deferred(&s).is_empty(), "{:#?}", deferred(&s));
    let r = range(&s, "th.resistance");
    assert!(close(r.lo(), 532.0) && close(r.hi(), 197_390.0), "{r}");
    let i = range(&s, "th.current");
    assert!(
        close(i.lo(), 3.135 / (10_100.0 + 197_390.0)) && close(i.hi(), 3.465 / (9_900.0 + 532.0)),
        "{i}"
    );
}

#[test]
fn a_board_narrows_the_thermistor_to_its_temperatures_from_the_same_table() {
    // 0 C to 85 C of the same column (p.10): 27.445 kohm and 1.455 kohm.
    let s = run(&thermistor(".resistance &= 1.455kohm..27.445kohm"));
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let i = range(&s, "th.current");
    assert!(close(i.hi(), 3.465 / (9_900.0 + 1_455.0)), "{i}");
}

#[test]
fn a_resistance_outside_the_makers_table_is_refused() {
    let m = messages(&run(&thermistor(".resistance &= 0.1kohm..10kohm")));
    assert!(
        m.iter().any(|m| m.contains(
            "th.resistance must always be at least th.hot_resistance, but 100Ω..10kΩ is not always above 532Ω"
        )),
        "{m:#?}"
    );
}

#[test]
fn the_standard_thermistor_states_its_table_on_a_board_with_no_provider() {
    // The generic with the table's ends stated on the board: a resistor at DC, as above.
    let src = format!(
        r#"
import std::parts::passive::{{NtcThermistor, Resistor}}
import std::sources::Rail
{PLUG}
board Thermistor {{
    .plug: Plug {{
        .supply.voltage &= 3.3V
        .supply.pos.net &= "+3.3V"
        .supply.neg.net &= "GND"
    }}
    .pullup: Resistor {{ .resistance &= 10kohm +/- 1%; .package &= "0402" }}
    .th: NtcThermistor {{
        .resistance_25c &= 10kohm +/- 1%
        .cold_resistance &= 197.39kohm
        .hot_resistance &= 0.532kohm
    }}
    .plug.supply.pos ~ .pullup.a
    .pullup.b ~ .th.a
    .th.b ~ .plug.supply.neg
}}
"#
    );
    let s = run_std(&src);
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let i = range(&s, "th.current");
    assert!(
        close(i.lo(), 3.3 / 207_490.0) && close(i.hi(), 3.3 / 10_432.0),
        "{i}"
    );
}
