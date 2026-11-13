//! A bank that no row can serve is named at once, though the parts it is searched with decide
//! other values it reads (ADR 0057). The board is the shape of a switching regulator stage with
//! a frequency resistor, an input filter and two more input capacitors; every part, value and
//! name here is invented.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const BOARD: &str = r#"import std::parts::passive::{Capacitor, Resistor, FerriteBead}
import std::parts::regulator::{Buck, BuckIc}
import std::sources::Rail

part Zbk2 provides BuckIc {
    .vin.voltage >= 3V
    .vin.voltage <= 65V
    .vin_abs_max: Voltage
    .vin_abs_max &= 80V
    .vin.peak <= .vin_abs_max
    .vref &= 0.8V +/- 1.5%
    .min_input_voltage &= 3V
    .max_input_voltage &= 65V
    .max_output_current &= 3A
    .efficiency &= 0.85..0.95
    .min_input_capacitance &= 2.2uF
    .min_output_capacitance &= 10uF
    .rt_resistance: Resistance
    .fsw_nominal: Frequency
    .fsw_nominal == 46.5MHz * 1kohm / (.rt_resistance + 5.2kohm)
    .fsw_tolerance: Number
    .fsw_tolerance &= 0.925..1.075
    .fsw ?= 0.8MHz..1.2MHz
    .fsw == .fsw_nominal * .fsw_tolerance
    .min_inductance == (2 * .sw.voltage - .vin.voltage) / (2.5 * .fsw * 1A)
    .vin.current == -.sw.current * .sw.voltage / (.vin.voltage * .efficiency)
    pins {
        .vin.pos       power   "VIN"  "1"
        .vin.neg       power   "GND"  "2"
        .sw.pos        power   "SW"   "3"
        .fb            input   "FB"   "4"
        .en            input   "EN"   "5"
        .rt            passive "RT"   "6"
    }
    .mpn: String
    .package: Package
    catalog zbk2 { .mpn, .package }

    support {
        .en ~ .vin.pos
        .frequency_resistor: Resistor
        .rt ~> .frequency_resistor ~> .vin.neg
        .rt_resistance == .frequency_resistor.resistance
    }
}

module Zbk2Buck {
    .vin: Rail
    .vout: Rail
    .stage: Buck<Zbk2>
    .vin1_cap: Capacitor { .capacitance &= 1uF +/- 20% }
    .vin2_cap: Capacitor { .capacitance &= 1uF +/- 20% }

    .stage.vin ~ .vin
    .stage.vout ~ .vout
    .vin.pos ~> .vin1_cap ~> .vin.neg
    .vin.pos ~> .vin2_cap ~> .vin.neg
    .vin1_cap.max_voltage >= .vin.voltage * 2
    .vin2_cap.max_voltage >= .vin.voltage * 2
}

board Converter {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .peak &= 75V
        .pos.net &= "VBAT"
        .neg.net &= "GND"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .neg.net &= "GND"
        .current &= 0mA..1A
        .voltage &= 1.8V +/- 3%
        .peak &= 1.9V
    }
    .filter: FerriteBead
    .reg: Zbk2Buck {
        .vin.pos.net &= "VIN"
        .vin.voltage &= 4.5V..5.3V
    }
    .supply.pos ~> .filter ~> .reg.vin.pos
    .supply.neg ~ .reg.vin.neg
    .reg.vin.voltage == .supply.voltage - .filter.drop
    .reg.vin.peak == .supply.peak
    .filter.current == .reg.vin.current
    .reg.vout ~ .output
}
"#;

const IC: &str = "mpn,name,label,package,source
ZBK2-A,ZBK2-A invented buck IC,ZBK2,SOT-23-6,invented for a test
";

const RESISTOR: &str = "mpn,resistance,max_power,package,source
R39,39kohm +/- 1%,100mW,0402,invented
R43,43kohm +/- 1%,100mW,0402,invented
R47,47kohm +/- 1%,100mW,0402,invented
R51,51kohm +/- 1%,100mW,0402,invented
R100,100kohm +/- 1%,100mW,0402,invented
R120,120kohm +/- 1%,100mW,0402,invented
R150,150kohm +/- 1%,100mW,0402,invented
R180,180kohm +/- 1%,100mW,0402,invented
";

const FERRITE: &str = "mpn,impedance,impedance_frequency,rated_current,dc_resistance,package,source
FB1,120ohm +/- 25%,100MHz,2A,0.05ohm,0603,invented
FB2,220ohm +/- 25%,100MHz,1.4A,0.1ohm,0603,invented
";

const INDUCTOR: &str = "mpn,inductance,saturation_current,dc_resistance,package,source
L22,2.2uH +/- 20%,3.6A,0.085ohm,1210,invented
L47,4.7uH +/- 20%,3A,0.1ohm,1210,invented
";

/// None is rated above 50 V, so no row is rated for a 75 V peak.
const CAPACITOR: &str =
    "mpn,capacitance,effective_capacitance,max_voltage,package,dielectric,source
C1U25,1uF +/- 10%,derate capacitance by bias: 0:1 5:0.9 25:0.6,25V,0402,X5R,invented
C1U16,1uF +/- 20%,derate capacitance by bias: 0:1 5:0.85 16:0.5,16V,0402,X5R,invented
C22U6,22uF +/- 20%,derate capacitance by bias: 0:1 3:0.8 6.3:0.5,6.3V,0603,X5R,invented
C47U6,47uF +/- 20%,derate capacitance by bias: 0:1 3:0.8 6.3:0.5,6.3V,0805,X5R,invented
C2U50,2.2uF +/- 10%,derate capacitance by bias: 0:1 10:0.9 50:0.3,50V,0805,X7R,invented
C4U35,4.7uF +/- 10%,derate capacitance by bias: 0:1 10:0.8 35:0.3,35V,1206,X7R,invented
C10N50,10nF +/- 10%,derate capacitance by bias: 0:1 25:0.7 50:0.3,50V,0402,X7R,invented
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
    let c = check("bank.nsm", src, &Loader::std());
    assert!(
        c.is_ok(),
        "the source should check clean:
{}",
        c.render()
    );
    solve(&c.boards[0], &catalogs())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn the_board_with_a_rail_peak_every_input_capacitor_row_carries_builds() {
    let s = run(&BOARD.replace(".peak &= 75V", ".peak &= 30V"));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
}

#[test]
fn an_input_capacitor_bank_no_row_is_rated_for_is_named_though_it_is_searched_with_six_parts() {
    // The stage's input bank, the two input capacitors, the IC, its frequency
    // resistor, the inductor and the filter read values that picks decide, so they are one
    // group. The bank has no row rated for 75 V at any count: that is the cause, and it is
    // said before the combinations of the other parts are tried.
    let s = run(BOARD);
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].starts_with(
            "no `capacitor` candidate satisfies `reg.stage.input_cap`: the closest of the "
        ),
        "{}",
        m[0]
    );
    assert!(
        m[0].contains("reg.stage.input_cap.max_voltage must always be at least (reg.stage.vin.peak * reg.stage.voltage_margin)")
            && m[0].contains("not always above 75V"),
        "{}",
        m[0]
    );
    assert!(!m[0].contains("gave up"), "{}", m[0]);
    assert!(s.trials < 200, "{} trials", s.trials);
}

#[test]
fn a_bank_refused_for_a_value_another_part_decides_is_not_blamed_alone() {
    // 5 mF at the input is more than eight of any row gives, but what a row gives depends on
    // the voltage across it, and the filter's pick decides that voltage: the bank is not the
    // cause by itself, so the parts are named together, as before.
    let s = run(&BOARD.replace(".peak &= 75V", ".peak &= 30V").replace(
        ".min_input_capacitance &= 2.2uF",
        ".min_input_capacitance &= 5mF",
    ));
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].starts_with("no combination of candidates satisfies `filter` and "),
        "{}",
        m[0]
    );
}
