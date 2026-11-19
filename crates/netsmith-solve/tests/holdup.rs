//! The hold-up of a rail by a storage element (issue #1255), `std::checks::HoldUp`: a
//! supercapacitor charged to 5 V behind a Schottky diode and a 3.3 V linear regulator, and a
//! load on the regulator's output. Every row and curve here is invented: no real part has a
//! recorded source for these figures yet (`data/jlcpcb/README.md`). The numbers are worked out by
//! hand in the comments, not read from the solver.
//!
//! The figures of the board: the storage starts at 4.9 V (5 V - 2 %) and keeps 0.8 F (1 F - 20 %);
//! the rail's least is 3.234 V (3.3 V - 2 %). At a peak of 0.5 A the regulator drops its 0.6 V,
//! the diode its 0.5 V and the ESR of 100 mohm 50 mV, so the storage must keep 4.384 V: it has
//! 0.516 V to give, 0.4128 C at 0.8 F. A load of 0.3 W takes 92.77 mA at 3.234 V, and the storage
//! leaks 5 uA more: 0.18555 C over 2 s, 0.4639 C over 5 s.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const SUPERCAP: &str = "mpn,capacitance,esr,dc_esr,leakage_current,max_voltage,cells,soldering,package,capacitance_at_end_of_life,esr_growth,library,source
ZZS-1F,1F +/- 20%,60mohm,100mohm,5uA,5.5V,2,Hand,Radial_D16.0mm_P7.50mm,0.7,2,Extended,https://example.invalid/zzs-1f.pdf
";

const SCHOTTKY: &str = "mpn,forward_voltage,max_reverse_voltage,max_forward_current,package,library,source
ZZD-1,0.5V,40V,2A,SOD-323,Extended,https://example.invalid/zzd-1.pdf
";

/// Its forward voltage over the current at two temperatures.
const SCHOTTKY_CURVE: &str = "temperature,current,forward_voltage
-20degC,0A,0.2V
-20degC,1A,0.7V
25degC,0A,0.1V
25degC,1A,0.5V
85degC,0A,0.05V
85degC,1A,0.45V
";

const LDO: &str = "mpn,vout.voltage,dropout,max_current,max_power,package,source
ZZL-33,3.3V +/- 2%,0.6V,1A,3W,SOT-223,https://example.invalid/zzl
";

fn catalogs(curve: bool) -> Catalogs {
    let mut c = Catalogs::empty();
    c.add_file("supercap", "catalogs/supercap.csv", SUPERCAP)
        .unwrap();
    c.add_file("schottky", "catalogs/schottky.csv", SCHOTTKY)
        .unwrap();
    c.add_file("ldo", "catalogs/ldo.csv", LDO).unwrap();
    if curve {
        c.add_curve(
            "schottky",
            "ZZD-1",
            "catalogs/curves/schottky/ZZD-1.csv",
            SCHOTTKY_CURVE,
        )
        .unwrap();
    }
    c
}

/// The board, with the hold-up's load `load` and `rest` after it.
fn board(load: &str, rest: &str) -> String {
    format!(
        "import std::checks::HoldUp
import std::env::{{Corner, Environment, Life}}
import std::parts::diode::Schottky
import std::parts::passive::Supercap
import std::parts::regulator::LinearRegulator
import std::sources::Rail

board B {{
    .environment: Environment {{ .temperature &= -20degC..85degC }}
    .backup: Rail {{ .voltage &= 5V +/- 2%; .pos.net &= \"VB\" }}
    .sc: Supercap
    .backup.pos ~> .sc ~> .backup.neg
    .vin: Rail {{ .voltage &= 4.2V..4.6V; .pos.net &= \"VIN\" }}
    .vin.neg ~ .backup.neg
    .d: Schottky {{ .current &= 0A..0.5A }}
    .backup.pos ~> .d ~> .vin.pos
    .u: LinearRegulator
    .vin ~ .u.vin
    .out: Rail {{ .current &= 0mA..100mA; .pos.net &= \"V33\" }}
    .u.vout ~ .out
    .hold: HoldUp {{ {load} }}
    .hold.storage ~ .sc.p
    .hold.rail ~ .out.pos
{rest}
}}
"
    )
}

const TWO_SECONDS: &str = ".average_power &= 0.3W; .peak_current &= 0.5A; .duration &= 2s";
const FIVE_SECONDS: &str = ".average_power &= 0.3W; .peak_current &= 0.5A; .duration &= 5s";
const THREE_SECONDS: &str = ".average_power &= 0.3W; .peak_current &= 0.5A; .duration &= 3s";
const EOL: &str = "    .eol: Corner { .temperature &= 25degC; .life &= Life::End }";

fn solved(src: &str, catalogs: &Catalogs) -> Solution {
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn errors(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn notes(s: &Solution) -> Vec<String> {
    s.not_evaluated.iter().map(|n| n.message.clone()).collect()
}

fn holds(s: &Solution) -> Vec<String> {
    errors(s)
        .into_iter()
        .filter(|e| e.contains("does not hold"))
        .collect()
}

#[test]
fn a_storage_that_holds_the_rail_up_passes() {
    let s = solved(&board(TWO_SECONDS, ""), &catalogs(false));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert!(
        !notes(&s).iter().any(|n| n.contains("hold-up")),
        "{:?}",
        notes(&s)
    );
}

/// Over 5 s the chain takes 0.4639 C and the storage has 0.4128 C to give: it takes
/// 0.4639 / 0.516 = 0.899 F.
#[test]
fn a_storage_too_small_for_the_time_is_refused_with_the_capacitance_it_takes() {
    let s = solved(&board(FIVE_SECONDS, ""), &catalogs(false));
    let found = holds(&s);
    assert_eq!(found.len(), 1, "{:?}", errors(&s));
    let e = &found[0];
    for part in [
        "`hold` does not hold `out` up:",
        "300mW for 5s",
        "the storage gives the chain 92.76mA (and leaks 5uA), 2.153J over the time",
        "gives only 1.916J",
        "at its least 3.234V",
        "between its starting 4.9V and 4.384V",
        "a peak of 500mA",
        "600mV of dropout in `u`",
        "500mV across `d`",
        "50mV across the storage's ESR of 100mΩ",
        "800mF of effective capacitance",
        "898.9mF",
    ] {
        assert!(e.contains(part), "{part}: {e}");
    }
}

/// At the end of life the storage keeps 0.56 F and its ESR doubles to 200 mohm: the cut-off is
/// 4.434 V, 0.466 V below the start, 0.261 C, under the 0.2783 C of 3 s. The parts' own figures
/// (0.4128 C) hold it.
#[test]
fn a_storage_that_fails_at_a_corner_names_the_corner() {
    let s = solved(&board(THREE_SECONDS, EOL), &catalogs(false));
    let found = holds(&s);
    assert_eq!(found.len(), 1, "{:?}", errors(&s));
    let e = &found[0];
    assert!(e.contains("at the corner `eol`"), "{e}");
    assert!(e.contains("at the end of life"), "{e}");
    assert!(e.contains("4.434V"), "{e}");
    assert!(e.contains("560mF"), "{e}");
    // Without the corner, the board passes.
    let s = solved(&board(THREE_SECONDS, ""), &catalogs(false));
    assert!(holds(&s).is_empty(), "{:?}", errors(&s));
}

/// A cold corner reads the storage's ESR off a curve the row does not have: the corner is not
/// judged, and a note names it.
#[test]
fn a_corner_that_cannot_read_the_esr_is_named_in_a_note() {
    let cold = "    .cold: Corner { .temperature &= -20degC; .life &= Life::New }";
    let s = solved(&board(TWO_SECONDS, cold), &catalogs(false));
    assert!(holds(&s).is_empty(), "{:?}", errors(&s));
    let n = notes(&s);
    assert!(
        n.iter().any(|n| n.contains("the hold-up `hold` is not judged at the corner `cold`")
            && n.contains("sc")),
        "{n:?}"
    );
}

/// With the diode's curve, the drop is read off it at the peak: 1 A over the board's -20 C to
/// 85 C is up to 0.7 V, so with 0.6 V across the regulator and 0.1 V across the ESR the storage
/// must keep 4.634 V and has 0.266 V to give, 0.2128 C, under the 0.2319 C of 2.5 s. The row's
/// figure, 0.5 V, leaves 0.466 V, 0.3728 C. A corner at 25 C reads 0.5 V off the curve, and holds.
#[test]
fn the_diodes_drop_is_read_off_its_curve_at_the_peak_and_the_corners_temperature() {
    let load = ".average_power &= 0.3W; .peak_current &= 1A; .duration &= 2.5s";
    let s = solved(&board(load, ""), &catalogs(false));
    assert!(holds(&s).is_empty(), "{:?}", errors(&s));
    let warm = "    .warm: Corner { .temperature &= 25degC; .life &= Life::New }";
    let s = solved(&board(load, warm), &catalogs(true));
    let found = holds(&s);
    assert_eq!(found.len(), 1, "{:?}", errors(&s));
    let e = &found[0];
    assert!(!e.contains("at the corner"), "{e}");
    assert!(e.contains("700mV across `d`"), "{e}");
    assert!(e.contains("4.634V"), "{e}");
}

/// A peak that the chain cannot carry even from the starting voltage: 6 A across the ESR of
/// 100 mohm is 0.6 V, so the storage must keep 3.234 V + 0.6 V + 0.5 V + 0.6 V = 4.934 V, more
/// than it starts at.
#[test]
fn a_peak_that_pulls_the_rail_below_its_minimum_at_the_start_is_refused() {
    let load = ".average_power &= 0.3W; .peak_current &= 6A; .duration &= 2s";
    let s = solved(&board(load, ""), &catalogs(false));
    let found = holds(&s);
    assert_eq!(found.len(), 1, "{:?}", errors(&s));
    assert!(found[0].contains("not below its starting voltage, 4.9V"), "{}", found[0]);
}

#[test]
fn a_hold_up_joined_to_nothing_is_refused() {
    let src = board(TWO_SECONDS, "").replace("    .hold.rail ~ .out.pos\n", "");
    let s = solved(&src, &catalogs(false));
    let all = errors(&s).join("\n");
    assert!(all.contains("`hold.rail` is joined to nothing"), "{all}");
}

#[test]
fn a_hold_up_with_no_chain_to_its_rail_is_noted() {
    let src = board(TWO_SECONDS, "").replace(
        "    .backup.pos ~> .d ~> .vin.pos\n",
        "    .d.bias &= 0V\n",
    );
    let s = solved(&src, &catalogs(false));
    let n = notes(&s);
    assert!(
        n.iter().any(|n| n.contains("the hold-up `hold` is not judged: no chain")),
        "{n:?}"
    );
}

#[test]
fn a_hold_up_that_states_no_load_is_noted() {
    let s = solved(&board(".average_power &= 0.3W", ""), &catalogs(false));
    let n = notes(&s);
    assert!(
        n.iter()
            .any(|n| n.contains("states no `.peak_current`, `.duration`")),
        "{n:?}"
    );
}

/// The rail itself is the storage's net: no chain, and the rail's floor is its minimum. The floor
/// of 2.7 V and 50 mV across the ESR at the peak leave 0.484 V of the 3.234 V the storage starts
/// at, 0.3872 C; 0.3 W at 2.7 V is 111.1 mA, 0.2222 C over 2 s and 0.5556 C over 5 s.
#[test]
fn a_storage_on_the_rail_itself_is_held_down_to_the_rails_floor() {
    let src = |load: &str| {
        board(load, "    .out.voltage_floor &= 2.7V")
            .replace("    .backup.pos ~> .sc ~> .backup.neg\n", "    .out.pos ~> .sc ~> .out.neg\n")
    };
    let s = solved(&src(TWO_SECONDS), &catalogs(false));
    assert!(holds(&s).is_empty(), "{:?}", errors(&s));
    let s = solved(&src(FIVE_SECONDS), &catalogs(false));
    let found = holds(&s);
    assert_eq!(found.len(), 1, "{:?}", errors(&s));
    let e = &found[0];
    assert!(e.contains("at its least 2.7V"), "{e}");
    assert!(e.contains("between its starting 3.234V and 2.75V"), "{e}");
}

/// A buck converter behind the storage: at the peak of 1 A its inductor drops 85 mV, so its
/// output needs 1.831 V, and its IC needs its least input, 3 V, where it draws
/// 1.831 W / (0.85 * 3 V) = 0.718 A, 71.8 mV across the ESR: the storage keeps 3.0718 V and has
/// 1.8282 V to give, 1.4626 C. The load of 0.5 W at 1.746 V draws 286.4 mA, which the IC draws
/// at 3 V as 198.8 mA: 1.3917 C over 7 s and 1.5905 C over 8 s.
#[test]
fn a_switching_regulator_draws_its_outputs_power_at_its_least_input() {
    let s = solved(&buck_board("7s"), &buck_catalogs());
    assert!(holds(&s).is_empty(), "{:?}", errors(&s));
    let s = solved(&buck_board("8s"), &buck_catalogs());
    let found = holds(&s);
    assert_eq!(found.len(), 1, "{:?} {:?}", errors(&s), notes(&s));
    let e = &found[0];
    assert!(e.contains("needs 3V at its input"), "{e}");
    assert!(e.contains("85mV across"), "{e}");
    assert!(e.contains("between its starting 4.9V and 3.072V"), "{e}");
}

fn buck_board(duration: &str) -> String {
    format!(
        r#"import std::checks::HoldUp
import std::parts::passive::Supercap
import std::parts::regulator::{{Buck, BuckIc}}
import std::sources::Rail

part Zbk2 provides BuckIc {{
    .vin.voltage >= 3V
    .vin.voltage <= 65V
    .vref &= 0.8V +/- 1.5%
    .min_input_voltage &= 3V
    .max_input_voltage &= 65V
    .max_output_current &= 3A
    .efficiency &= 0.85..0.95
    .min_input_capacitance &= 2.2uF
    .min_output_capacitance &= 10uF
    .fsw &= 1MHz +/- 10%
    .min_inductance &= 1.5uH
    .vin.current == -.sw.current * .sw.voltage / (.vin.voltage * .efficiency)
    dc switching_regulator .vin .sw .efficiency
    pins {{
        .vin.pos       power   "VIN"  "1"
        .vin.neg       power   "GND"  "2"
        .sw.pos        power   "SW"   "3"
        .fb            input   "FB"   "4"
    }}
    .package: Package
    catalog zbk2 {{ .mpn, .package }}
}}

board Converter {{
    .input: Rail {{ .voltage &= 5V +/- 2%; .pos.net &= "VB"; .neg.net &= "GND" }}
    .sc: Supercap
    .input.pos ~> .sc ~> .input.neg
    .output: Rail {{ .pos.net &= "1V8"; .neg.net &= "GND"; .current &= 0mA..1A }}
    .stage: Buck<Zbk2> {{ .vout.voltage &= 1.8V +/- 3% }}
    .input ~ .stage.vin
    .stage.vout ~ .output
    .hold: HoldUp {{ .average_power &= 0.5W; .peak_current &= 1A; .duration &= {duration} }}
    .hold.storage ~ .sc.p
    .hold.rail ~ .output.pos
}}
"#
    )
}

fn buck_catalogs() -> Catalogs {
    let mut c = Catalogs::empty();
    for (name, text) in [
        ("zbk2", "mpn,name,label,package,source\nZBK2-A,ZBK2-A invented buck IC,ZBK2,SOT-23-6,invented for a test\n"),
        ("resistor", "mpn,resistance,max_power,package,source\nR20,20kohm +/- 0.1%,100mW,0402,invented\nR25,25kohm +/- 0.1%,100mW,0402,invented\nR100,100kohm +/- 0.1%,100mW,0402,invented\n"),
        ("inductor", "mpn,inductance,saturation_current,dc_resistance,package,source\nL22,2.2uH +/- 20%,3.6A,0.085ohm,1210,invented\n"),
        ("capacitor", "mpn,capacitance,effective_capacitance,max_voltage,package,dielectric,source\nC47U6,47uF +/- 20%,derate capacitance by bias: 0:1 3:0.8 6.3:0.5,6.3V,0805,X5R,invented\nC4U35,4.7uF +/- 10%,derate capacitance by bias: 0:1 10:0.8 35:0.3,35V,1206,X7R,invented\n"),
        ("supercap", SUPERCAP),
    ] {
        c.add_file(name, &format!("catalogs/{name}.csv"), text).unwrap();
    }
    c
}
