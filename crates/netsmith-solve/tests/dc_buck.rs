//! A buck stage in the DC analysis, with no window written for its input (ADR 0076, amendment 6):
//! the `Inductor` is its winding at DC, so the node past it is the switch node less what the load
//! makes the winding drop; the IC states `dc switching_regulator`; and a regulator whose load a
//! pick decides is enclosed by the part's own bound before the picks, so the pick of an input
//! capacitor that reads the bias waits for no seed. Every part and value here is invented.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const BOARD: &str = r#"import std::parts::passive::{Capacitor, Resistor, FerriteBead}
import std::parts::regulator::{Buck, BuckStage}
import std::sources::Rail

part Zbk2 {
    .vin: Rail
    .vin.voltage >= 3V
    .vin.voltage <= 65V
    .buck: Buck {
        .vref &= 0.8V +/- 1.5%
        .min_input_voltage &= 3V
        .max_input_voltage &= 65V
        .max_output_current &= 3A
        .efficiency &= 0.85..0.95
        .min_input_capacitance &= 2.2uF
        .min_output_capacitance &= 10uF
        .fsw &= 1MHz +/- 10%
        .min_inductance &= 2uH
    }
    .buck.sw.neg ~ .vin.neg
    .vin.current == -.buck.sw.current * .buck.sw.voltage / (.vin.voltage * .buck.efficiency)
    dc switching_regulator .vin .buck.sw .buck.efficiency
    pins {
        .vin.pos       power   "VIN"  "1"
        .vin.neg       power   "GND"  "2"
        .buck.sw.pos   power   "SW"   "3"
        .buck.fb       input   "FB"   "4"
    }
    .mpn: String
    .package: Package
    catalog zbk2 { .mpn, .package }
}

module Zbk2Buck {
    .vin: Rail
    .vout: Rail
    .controller: Zbk2
    .stage: BuckStage
    .controller.vin ~ .vin
    .stage.vin ~ .vin
    .stage.vout ~ .vout
    .stage.buck ~ .controller.buck
}

board Converter {
    .supply: Rail {
        .voltage &= 9V..16V
        .peak &= 30V
        .pos.net &= "VBAT"
        .neg.net &= "GND"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .neg.net &= "GND"
        .current &= 0mA..1A
        .peak &= 1.9V
    }
    .filter: FerriteBead
    .reg: Zbk2Buck {
        .vin.pos.net &= "VIN"
        .vout.voltage &= 1.8V +/- 3%
    }
    .supply.pos ~> .filter ~> .reg.vin.pos
    .supply.neg ~ .reg.vin.neg
    .reg.vin.peak == .supply.peak
    .reg.vout ~ .output
}
"#;

const IC: &str = "mpn,name,label,package,source
ZBK2-A,ZBK2-A invented buck IC,ZBK2,SOT-23-6,invented for a test
";

const RESISTOR: &str = "mpn,resistance,max_power,package,source
R20,20kohm +/- 0.1%,100mW,0402,invented
R25,25kohm +/- 0.1%,100mW,0402,invented
R100,100kohm +/- 0.1%,100mW,0402,invented
";

const FERRITE: &str = "mpn,impedance,impedance_frequency,rated_current,dc_resistance,package,source
FB1,120ohm +/- 25%,100MHz,2A,0.05ohm,0603,invented
FB2,220ohm +/- 25%,100MHz,1.4A,0.1ohm,0603,invented
";

const INDUCTOR: &str = "mpn,inductance,saturation_current,dc_resistance,package,source
L22,2.2uH +/- 20%,3.6A,0.085ohm,1210,invented
L47,4.7uH +/- 20%,3A,0.1ohm,1210,invented
";

const CAPACITOR: &str =
    "mpn,capacitance,effective_capacitance,max_voltage,package,dielectric,source
C1U25,1uF +/- 10%,derate capacitance by bias: 0:1 5:0.9 25:0.6,25V,0402,X5R,invented
C22U6,22uF +/- 20%,derate capacitance by bias: 0:1 3:0.8 6.3:0.5,6.3V,0603,X5R,invented
C47U6,47uF +/- 20%,derate capacitance by bias: 0:1 3:0.8 6.3:0.5,6.3V,0805,X5R,invented
C47U10,47uF +/- 20%,derate capacitance by bias: 0:1 5:0.8 10:0.5,10V,1206,X5R,invented
C2U50,2.2uF +/- 10%,derate capacitance by bias: 0:1 10:0.9 50:0.3,50V,0805,X7R,invented
C4U35,4.7uF +/- 10%,derate capacitance by bias: 0:1 10:0.8 35:0.3,35V,1206,X7R,invented
";

fn catalogs() -> Catalogs {
    let mut c = Catalogs::empty();
    for (name, text) in [
        ("zbk2", IC),
        ("resistor", RESISTOR),
        ("ferrite_bead", FERRITE),
        ("inductor", INDUCTOR),
        ("capacitor", CAPACITOR),
    ] {
        c.add_file(name, &format!("catalogs/{name}.csv"), text)
            .unwrap();
    }
    c
}

fn run(src: &str) -> Solution {
    let c = check("buck.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &catalogs())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> Option<(f64, f64)> {
    match s
        .model
        .domain_at(path)
        .unwrap_or_else(|| panic!("no field {path}"))
    {
        Domain::Quantity(q) => Some((q.lo(), q.hi())),
        _ => None,
    }
}

/// A probe on the stage's output net that states nothing: it takes the voltage of the node past
/// the inductor.
const PROBE: &str = "    .probe: Rail {
        .pos.net &= \"1V8\"
        .neg.net &= \"GND\"
        .current &= 0A
    }
";

fn near(got: f64, want: f64) {
    assert!(
        (got - want).abs() <= 1e-5 * want.abs() + 1e-9,
        "got {got}, want {want}"
    );
}

/// The winding is a resistor at DC, so the stage's inductor carries the load and drops what
/// its resistance makes; the regulator's input follows from the load and the supply through the
/// filter, and the capacitor across it is picked on the voltage the circuit gives, with no
/// window written.
#[test]
fn a_buck_stage_builds_with_no_window_for_its_input() {
    let s = run(&BOARD.replace(
        "    .supply.pos ~> .filter",
        &format!("{PROBE}    .supply.pos ~> .filter"),
    ));
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    assert_eq!(s.dc.regulators, 1, "{:?}", s.dc);
    let vin = range(&s, "reg.vin.voltage").unwrap();
    // The supply is 9 V to 16 V; the filter's drop is what the input current makes.
    assert!(
        vin.0 < 9.0 && vin.0 > 8.9 && (vin.1 - 16.0).abs() < 1e-3,
        "{vin:?}"
    );
    // The load is 0 A to 1 A, through the inductor.
    let current = range(&s, "reg.stage.inductor.current").unwrap();
    assert!(
        current.0 < 1e-3 && (current.1 - 1.0).abs() < 1e-6,
        "{current:?}"
    );
    let dcr = range(&s, "reg.stage.inductor.dc_resistance").unwrap().0;
    let drop = range(&s, "reg.stage.inductor.drop").unwrap();
    near(drop.1, current.1 * dcr);
    // The input draws `Pout / (efficiency * Vin)` at its worst: 1.854 V at 1 A, 85 %, the
    // lowest input.
    let i_in = range(&s, "reg.vin.current").unwrap();
    near(i_in.1, 1.854 / (0.85 * vin.0));
    // The node past the inductor is the switch node (1.746 V to 1.854 V, what the output
    // asks) less the winding's drop at the load: a regulator holds the node its part names, and
    // the probe on the net past the inductor reads the winding's drop.
    let node = range(&s, "probe.voltage").unwrap();
    near(node.0, 1.746 - drop.1);
    near(node.1, 1.854 - drop.0);
    // The capacitor's bias is the input's voltage, and was known when it was picked.
    assert_eq!(range(&s, "reg.stage.input_cap.bias"), Some(vin));
}

/// A load that nothing states and no pick decides (the output rail has no current) is enclosed
/// by what the IC allows its switch node to supply (`-.sw.current <= .max_output_current`), so
/// the network in front of the regulator is solved before the picks and the capacitor across
/// its input is picked with no window. The input is the supply less the filter's drop at the
/// largest current the IC allows, `3 A` at `1.854 V` over `85 %`: `v = 9 - 0.1 * 5.562 / (0.85 v)`.
#[test]
fn a_load_nothing_states_is_enclosed_by_the_parts_own_limit() {
    let s = run(&BOARD.replace("        .current &= 0mA..1A\n", ""));
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    let vin = range(&s, "reg.vin.voltage").unwrap();
    near(
        vin.0,
        (9.0 + (81.0f64 - 4.0 * 0.1 * 5.562 / 0.85).sqrt()) / 2.0,
    );
    assert!((vin.1 - 16.0).abs() < 1e-3, "{vin:?}");
    assert_eq!(range(&s, "reg.stage.input_cap.bias"), Some(vin));
}
