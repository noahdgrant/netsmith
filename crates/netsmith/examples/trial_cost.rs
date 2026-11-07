//! `cargo run --release -p netsmith --example trial_cost [parts]`
//!
//! Times the pieces of one provider trial (ADR 0068): a whole elaboration (`check`), a solve
//! and a lowering of an invented board of about 60 parts, so that the cost of the trial loop
//! can be put against a number. Nothing in the board is a design.

use std::time::Instant;

use netsmith::outcome_with;
use netsmith_check::{Loader, check};
use netsmith_lower::lower;
use netsmith_solve::{Catalogs, solve};

/// An invented board: LED branches, decoupling capacitors and three LDO stages.
fn board(branches: usize, caps: usize, stages: usize) -> String {
    let mut s = String::from(
        "import std::parts::connector::Header1x2\n\
         import std::parts::passive::{Resistor, Led, Capacitor}\n\
         import std::parts::regulator::LinearRegulator\n\
         import std::sources::Rail\n\
         import std::mfg::jlcpcb::TwoLayer\n\n\
         module LdoStage {\n\
         \x20   .vin: Rail\n    .vout: Rail\n    .regulator: LinearRegulator\n\
         \x20   .input_cap: Capacitor { .effective_capacitance >= 1uF }\n\
         \x20   .output_cap: Capacitor { .effective_capacitance >= 4.7uF }\n\
         \x20   .vin ~ .regulator.vin\n    .regulator.vout ~ .vout\n\
         \x20   .input_cap.max_voltage >= .vin.voltage * 1.5\n\
         \x20   .output_cap.max_voltage >= .vout.voltage * 1.5\n\
         \x20   .vin.pos ~> .input_cap ~> .vin.neg\n    .vout.pos ~> .output_cap ~> .vout.neg\n}\n\n\
         board Big {\n    .process: TwoLayer { .stackup.thickness &= 1.6mm }\n\
         \x20   .supply: Rail { .voltage &= 5V +/- 5%\n        .pos.net &= \"VCC\"\n        .neg.net &= \"GND\" }\n\
         \x20   .power_in: Header1x2\n    .supply.pos ~ .power_in.p1\n    .supply.neg ~ .power_in.p2\n",
    );
    for i in 0..branches {
        s += &format!(
            "    .led{i}: Led {{ .current &= 5mA..15mA }}\n    .r{i}: Resistor {{}}\n\
             \x20   .led{i}.current == (.supply.voltage - .led{i}.forward_voltage) / .r{i}.resistance\n\
             \x20   .supply.pos ~> .r{i} ~> .led{i} ~> .supply.neg\n"
        );
    }
    for i in 0..caps {
        s += &format!(
            "    .c{i}: Capacitor {{ .effective_capacitance >= 100nF\n        .max_voltage >= 10V }}\n\
             \x20   .supply.pos ~> .c{i} ~> .supply.neg\n"
        );
    }
    for i in 0..stages {
        s += &format!(
            "    .out{i}: Rail {{ .pos.net &= \"3V3_{i}\"\n        .neg.net &= \"GND\"\n        .current &= 0mA..50mA }}\n\
             \x20   .stage{i}: LdoStage {{ .vout.voltage &= 3.3V +/- 5% }}\n\
             \x20   .supply ~ .stage{i}.vin\n    .stage{i}.vout ~ .out{i}\n"
        );
    }
    s += "}\n";
    s
}

fn main() {
    let branches = 14;
    let caps = 20;
    let stages = 3;
    let text = board(branches, caps, stages);
    let loader = Loader::std();
    let catalogs = Catalogs::jlcpcb();
    for round in 0..3 {
        let t = Instant::now();
        let checked = check("big.nsm", &text, &loader);
        let t_check = t.elapsed();
        assert!(checked.is_ok(), "{}", checked.render());
        let b = &checked.boards[0];
        let t = Instant::now();
        let solution = solve(b, &catalogs);
        let t_solve = t.elapsed();
        assert!(
            solution.diagnostics.is_empty(),
            "{:?}",
            solution.diagnostics
        );
        let t = Instant::now();
        let lowered = lower(b, &solution).expect("lowers");
        let t_lower = t.elapsed();
        let t = Instant::now();
        let outcome = outcome_with("big.nsm", &text, &loader, &catalogs, None).expect("builds");
        let t_all = t.elapsed();
        println!(
            "round {round}: parts {}  check {:?}  solve {:?}  lower {:?}  outcome_with {:?}",
            lowered.design.parts.len(),
            t_check,
            t_solve,
            t_lower,
            t_all
        );
        let _ = outcome;
    }
}
