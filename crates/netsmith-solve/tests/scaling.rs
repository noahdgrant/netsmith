//! The solver's work grows with the board: `n` copies of a regulator stage and a lamp, each
//! with a few generic picks, on one input rail.
//!
//! The first test checks what the search chooses for a small board, twice, as an invariant
//! of the picks (they are the same each time and each stage got its parts). The second compares
//! the trials the search reports (`Solution::trials`, a count that is the same on every
//! machine) between two sizes. The third, `measured`, prints the time, which is no check
//! (a time is not asserted, a machine's is not the next one's): run it with `cargo test --release
//! -p netsmith-solve --test main scaling::measured -- --ignored --nocapture` and see the time for
//! 10, 20 and 40 stages. Every name and number here is invented.

use std::time::Instant;

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

/// `n` stages and lamps on the input rail.
fn board(n: usize) -> String {
    let mut src = String::from(
        "import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led, Capacitor}
import std::parts::regulator::LinearRegulator
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

module Stage {
    .vin: Rail
    .vout: Rail
    .regulator: LinearRegulator
    .input_cap: Capacitor { .effective_capacitance >= 1uF }
    .output_cap: Capacitor { .effective_capacitance >= 4.7uF }
    .bypass: Capacitor { .capacitance &= 100nF +/- 20% }
    .vin ~ .regulator.vin
    .regulator.vout ~ .vout
    .input_cap.max_voltage >= .vin.voltage * 1.5
    .output_cap.max_voltage >= .vout.voltage * 1.5
    .vin.pos ~> .input_cap ~> .vin.neg
    .vout.pos ~> .output_cap ~> .vout.neg
    .vout.pos ~> .bypass ~> .vout.neg
}

module Lamp {
    .power: Rail
    .limit: Resistor {}
    .led: Led { .current &= 5mA..15mA }
    .led.current == (.power.voltage - .led.forward_voltage) / .limit.resistance
    .power.pos ~> .limit ~> .led ~> .power.neg
}

board Synthetic {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
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
    for i in 1..=n {
        src.push_str(&format!(
            "    .stage{i}: Stage {{ .vout.voltage &= 3.3V +/- 5% }}
    .out{i}: Rail {{ .current &= 0mA..50mA }}
    .lamp{i}: Lamp
    .input ~ .stage{i}.vin
    .stage{i}.vout ~ .out{i}
    .stage{i}.vout ~ .lamp{i}.power
"
        ));
    }
    src.push_str("}\n");
    src
}

fn run(n: usize, catalogs: &Catalogs) -> Solution {
    let c = check("synthetic.nsm", &board(n), &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn picked(s: &Solution) -> Vec<String> {
    s.picks
        .iter()
        .map(|p| {
            format!(
                "{} = {} {:?} (rejected {}, count {})",
                s.model.path_of(p.part),
                p.candidate,
                p.lcsc,
                p.rejected,
                p.count
            )
        })
        .collect()
}

#[test]
fn every_stage_gets_its_parts_and_the_picks_are_the_same_each_time() {
    let catalogs = Catalogs::jlcpcb();
    let first = run(6, &catalogs);
    let second = run(6, &catalogs);
    assert!(
        first.diagnostics.is_empty(),
        "{:?}",
        first
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
    );
    assert!(first.unresolved.is_empty());
    // The header, and a regulator, three capacitors, a resistor and an LED in each of six
    // stages.
    assert_eq!(first.picks.len(), 1 + 6 * 6);
    assert_eq!(picked(&first), picked(&second));
}

/// Twice the stages, about twice the trials: the stages share nothing but the input rail, so the
/// search over them is a search for each and not one search over all of them (which tries
/// combinations, and grows with their product). A count, so a busy runner cannot fail it. The work
/// is printed and not bounded: a trial copies the model and propagates, so what a trial costs
/// grows with the board and the work with the square of it by design (`netsmith_core::job`).
#[test]
fn the_trials_of_the_search_grow_with_the_stages_and_not_with_their_combinations() {
    let catalogs = Catalogs::jlcpcb();
    let (small, big) = (run(10, &catalogs), run(20, &catalogs));
    let trials = big.trials as f64 / small.trials.max(1) as f64;
    eprintln!(
        "10 stages: {} trials, {} work; 20 stages: {} trials, {} work; ratio of the trials {trials:.2}",
        small.trials, small.work, big.trials, big.work
    );
    assert!(
        trials < 2.5,
        "twice the stages took {trials:.2} times the trials ({} and {})",
        small.trials,
        big.trials
    );
}

#[test]
#[ignore = "a measurement, not a check: prints the time of the solver for 10, 20 and 40 stages"]
fn measured() {
    let catalogs = Catalogs::jlcpcb();
    for n in [10, 20, 40] {
        let c = check("synthetic.nsm", &board(n), &Loader::std());
        assert!(c.is_ok());
        let started = Instant::now();
        let s = solve(&c.boards[0], &catalogs);
        println!(
            "{n:3} stages: {:6.2} s, {} picks, {} trials",
            started.elapsed().as_secs_f64(),
            s.picks.len(),
            s.trials
        );
    }
}
