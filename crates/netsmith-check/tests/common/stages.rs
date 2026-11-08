//! A synthetic board for the tests of cancellation (ADR 0062): `stages` regulated rails, each
//! with a regulator and two capacitors, an indicator and a bank of resistors, all in one file.
//! The solver's search over their catalogs is what takes the time, as it does on a real board:
//! about a second for thirty-two stages in a dev build (the solver crates are optimised in the
//! dev profile), six seconds for ninety-six and half a minute, with several gigabytes of memory,
//! for two hundred. The other crates' tests take this file with `#[path]`.
#![allow(dead_code)]

/// The source of the board, with `stages` rails.
pub fn source(stages: usize) -> String {
    let mut text = String::from(
        "\
import std::parts::passive::{Resistor, Led, Capacitor}
import std::parts::regulator::LinearRegulator
import std::parts::connector::Header1x2
import std::sources::Rail

module LdoStage {
    .vin:  Rail
    .vout: Rail
    .regulator:  LinearRegulator
    .input_cap:  Capacitor { .effective_capacitance >= 1uF }
    .output_cap: Capacitor { .effective_capacitance >= 4.7uF }
    .vin ~ .regulator.vin
    .regulator.vout ~ .vout
    .input_cap.max_voltage  >= .vin.voltage * 1.5
    .output_cap.max_voltage >= .vout.voltage * 1.5
    .vin.pos  ~> .input_cap  ~> .vin.neg
    .vout.pos ~> .output_cap ~> .vout.neg
}

module Indicator {
    .power: Rail
    .power.current &= 5mA..15mA
    .limit: Resistor { .resistance &= 1kohm +/- 1% }
    .led: Led {
        .color &= \"GREEN\"
        .forward_voltage &= 1.8V..2.5V
    }
    .bypass: Capacitor { .capacitance &= 100nF +/- 20% }
    .power.pos ~> .limit  ~> .led ~> .power.neg
    .power.pos ~> .bypass ~> .power.neg
}

module PullBank {
    .power: Rail
    .power.current &= 0mA..2mA
    .a: Resistor { .resistance &= 10kohm +/- 1% }
    .b: Resistor { .resistance &= 10kohm +/- 1% }
    .c: Resistor { .resistance &= 4.7kohm +/- 1% }
    .d: Resistor { .resistance &= 4.7kohm +/- 1% }
    .filter: Capacitor { .capacitance &= 100nF +/- 20% }
    .power.pos ~> .a ~> .b ~> .power.neg
    .power.pos ~> .c ~> .d ~> .power.neg
    .power.pos ~> .filter ~> .power.neg
}

board Synthetic {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }
    .power_in: Header1x2
    .input.pos ~ .power_in.p1
    .input.neg ~ .power_in.p2
",
    );
    for i in 1..=stages {
        let load = [5, 10, 15, 20, 30, 40, 50, 60][(i - 1) % 8];
        text.push_str(&format!(
            "
    .stage{i}: LdoStage {{ .vout.voltage &= 3.3V +/- 5% }}
    .out{i}: Rail {{
        .pos.net &= \"RAIL{i}\"
        .neg.net &= \"GND\"
        .current &= 0mA..{load}mA
    }}
    .light{i}: Indicator
    .pulls{i}: PullBank
    .input ~ .stage{i}.vin
    .stage{i}.vout ~ .out{i}
    .stage{i}.vout ~ .light{i}.power
    .stage{i}.vout ~ .pulls{i}.power
"
        ));
    }
    text.push_str("}\n");
    text
}
