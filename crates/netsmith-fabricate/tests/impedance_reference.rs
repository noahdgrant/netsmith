//! The track impedance calculator (issue #1007) against JLCPCB's own impedance calculator: for
//! each row of `data/jlcpcb/impedance_calculator.csv`, a track width that JLCPCB's calculator
//! gave for a target impedance on a build-up of the examples, and the impedance this crate
//! works out for that width.
//!
//! The rows are figures read from https://jlcpcb.com/pcb-impedance-calculator (the README of the
//! folder says how and when). JLCPCB's calculator solves a width for the target to within the
//! tolerance it shows, 0.5 %, so the impedance of its width is its target; this crate's
//! figure differs from it by the models' difference. The tolerance below is what the rows show
//! and a limit on it: the figure is not JLCPCB's, and the test says by how much it is not.

use netsmith_check::Loader;
use netsmith_core::Stackup;
use netsmith_pipeline::{Libraries, build_with};
use netsmith_solve::Catalogs;

const CSV: &str = include_str!("../../../data/jlcpcb/impedance_calculator.csv");
const FOUR_LAYER: &str = include_str!("../../../examples/four_layer.nsm");
const SIX_LAYER: &str = include_str!("../../../examples/six_layer.nsm");

/// The most this crate's impedance may be off JLCPCB's target at JLCPCB's width, as a fraction
/// (JLCPCB's own impedance test passes a board within 10 % of its target). The rows of 2026-11-16
/// show a single track within 4 % and a pair within 7 % (this crate's figure higher in all but
/// one: JLCPCB's calculator has the trace narrower at the top than at the base, a trapezoid,
/// which this crate's rectangle does not, and a mask over the copper's sides); the limits are
/// those with a little room, so a change to the formulas that moves a figure by more than that
/// says so.
const SINGLE_LIMIT: f64 = 0.05;
const PAIR_LIMIT: f64 = 0.08;

const MIL: f64 = 0.0254;

fn stackup(source: &str) -> Stackup {
    build_with(
        "example.nsm",
        source,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0)
    .design
    .process
    .expect("a process")
    .stackup
}

/// KiCad's name of the layer JLCPCB's calculator calls `L<n>`, on a board of `layers` layers.
fn layer_name(n: usize, layers: usize) -> String {
    match n {
        1 => "F.Cu".to_string(),
        n if n == layers => "B.Cu".to_string(),
        n => format!("In{}.Cu", n - 1),
    }
}

#[test]
fn the_impedance_at_jlcpcbs_widths_is_near_its_target() {
    // The four-layer example with its first inner layer a signal layer, so that a track may
    // be on it (the impedance of a layer does not depend on what the planes are).
    let four = stackup(&FOUR_LAYER.replace(".inner1.net &= \"GND\"", ".inner1.use &= \"signal\""));
    let six = stackup(SIX_LAYER);
    let mut worst = 0.0_f64;
    let mut checked = 0;
    for row in CSV.lines().skip(1) {
        let cells: Vec<&str> = row.split(',').collect();
        let [build, kind, ohms, layer, _top, _bottom, spacing, width] = cells[..] else {
            panic!("a row of eight cells: {row}");
        };
        let stackup = match build {
            "JLC04161H-7628" => &four,
            "JLC06161H-3313" => &six,
            other => panic!("no example is built on {other}"),
        };
        assert_eq!(
            stackup.inner.as_ref().map(|i| i.build.as_str()),
            Some(build),
            "the example's build-up"
        );
        let n: usize = layer[1..].parse().unwrap();
        let name = layer_name(n, stackup.layers as usize);
        let target: f64 = ohms.parse().unwrap();
        let width = width.parse::<f64>().unwrap() * MIL;
        let (spacing, limit) = match kind {
            "single" => (None, SINGLE_LIMIT),
            "differential" => (Some(spacing.parse::<f64>().unwrap() * MIL), PAIR_LIMIT),
            other => panic!("a row of a kind {other}"),
        };
        let got = netsmith_pcb::impedance_at(stackup, &name, width, spacing)
            .unwrap_or_else(|e| panic!("{row}: {e}"));
        let off = (got - target) / target;
        println!("{row}: {got:.2} ohm here, {:+.1} %", off * 100.0);
        assert!(
            off.abs() <= limit,
            "{row}: {got:.2} ohm here at JLCPCB's width, {:+.1} % from its target of {target} ohm (limit {} %)",
            off * 100.0,
            limit * 100.0
        );
        worst = worst.max(off.abs());
        checked += 1;
    }
    assert!(checked >= 6, "only {checked} rows");
    println!("the worst: {:.1} %", worst * 100.0);
}
