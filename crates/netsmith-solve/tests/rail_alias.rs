//! A module's rail and the rail of the part inside it are one current (ADR 0077): the part's
//! stated supply current reaches the module's rail, and the sheet that adds up its modules'
//! draws has a value for each. Every part and module here is invented.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

/// Two wrapper modules on one rail of a sheet, which states what it draws as the sum of theirs.
/// The sensor's logic line names the sensor's supply as its reference and does not say that the
/// reference draws nothing, so the balance of the wrapper has a second current it does not know.
fn board(sensor_body: &str, sheet_equation: &str) -> String {
    format!(
        r#"import std::parts::passive::Capacitor
import std::sources::Rail
import std::logic::Logic

part Sensor {{
    .designator &= "U"
    .vs: Rail
    .alert: Logic
    .alert.reference ~ .vs
    .vs.current &= 0A..1.1mA
{sensor_body}
}}

part Flash {{
    .designator &= "U"
    .vcc: Rail
    .vcc.current &= 0A..10mA
}}

module Monitor {{
    .vdd: Rail
    .sensor: Sensor
    .bypass: Capacitor
    .vdd ~ .sensor.vs
    .vdd.pos ~> .bypass ~> .vdd.neg
}}

module Storage {{
    .vcc: Rail
    .flash: Flash
    .vcc ~ .flash.vcc
}}

module Sheet {{
    .vdd: Rail
    .monitor: Monitor
    .storage: Storage
    .vdd ~ .monitor.vdd
    .vdd ~ .storage.vcc
    {sheet_equation}
}}

board B {{
    .supply: Rail {{
        .voltage &= 3.3V +/- 3%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }}
    .sheet: Sheet
    .supply ~ .sheet.vdd
}}
"#
    )
}

const SUM: &str = ".vdd.current == .monitor.vdd.current + .storage.vcc.current";

fn solved(src: &str) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn current(s: &Solution, path: &str) -> String {
    let field = s
        .model
        .fields()
        .map(|(id, _)| id)
        .find(|&id| s.model.path_of_field(id) == path)
        .unwrap_or_else(|| panic!("no field `{path}`"));
    match s.model.field(field).domain {
        Domain::Quantity(q) => format!("{q}"),
        _ => "unset".to_string(),
    }
}

#[test]
fn the_part_s_stated_draw_reaches_the_module_s_rail_and_the_sheet_s_sum() {
    let s = solved(&board("", SUM));
    assert_eq!(current(&s, "sheet.monitor.vdd.current"), "0A..1.1mA");
    assert_eq!(current(&s, "sheet.storage.vcc.current"), "0A..10mA");
    assert_eq!(current(&s, "sheet.vdd.current"), "0A..11.1mA");
    assert_eq!(current(&s, "supply.current"), "-11.1mA..0A");
    assert!(s.circular.is_empty(), "{:?}", s.circular);
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
}

#[test]
fn a_budget_stated_at_the_module_s_rail_is_a_budget_for_the_part_s_claim() {
    // The part claims up to 1.1 mA, and the rail it is behind is allowed 2 mA: the rail takes
    // what the part draws, and the part's claim stays as its datasheet gives it.
    let src = board("", &format!("{SUM}\n    .monitor.vdd.current &= 0A..2mA"));
    let s = solved(&src);
    assert_eq!(current(&s, "sheet.monitor.vdd.current"), "0A..1.1mA");
    assert_eq!(current(&s, "sheet.monitor.sensor.vs.current"), "0A..1.1mA");
    assert_eq!(current(&s, "supply.current"), "-11.1mA..0A");
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
}

#[test]
fn a_rail_allowed_less_than_the_part_claims_is_refused_as_without_the_module() {
    // 0.5 mA is less than the part says it may draw (ADR 0066, ADR 0038): the part's claim is not
    // narrowed to fit the budget, and the net says so.
    let src = board("", &format!("{SUM}\n    .monitor.vdd.current &= 0A..500uA"));
    let s = solved(&src);
    let said: Vec<&str> = s.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert!(
        said.iter().any(
            |m| m.contains("`-sheet.monitor.vdd.current` must stay within -500uA..0A")
                && m.contains("-1.1mA..0A")
        ),
        "{said:?}"
    );
    assert_eq!(current(&s, "sheet.monitor.sensor.vs.current"), "0A..1.1mA");
}

/// The guide's `Stage` twice, on one input rail.
const TWO_STAGES: &str = r#"import std::parts::regulator::LinearRegulator
import std::sources::Rail

module Stage {
    .vin: Rail
    .vout: Rail
    .regulator: LinearRegulator
    .vin ~ .regulator.vin
    .regulator.vout ~ .vout
}

board TwoStages {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .a: Stage { .vout.voltage &= 3.3V +/- 5% }
    .b: Stage { .vout.voltage &= 3.3V +/- 5% }
    .out_a: Rail {
        .pos.net &= "A3V3"
        .neg.net &= "GND"
        .current &= 0mA..20mA
    }
    .out_b: Rail {
        .pos.net &= "B3V3"
        .neg.net &= "GND"
        .current &= 0mA..30mA
    }
    .input ~ .a.vin
    .input ~ .b.vin
    .a.vout ~ .out_a
    .b.vout ~ .out_b
}
"#;

#[test]
fn two_stages_that_wrap_a_regulator_add_up_on_the_input_rail() {
    let s = solved(TWO_STAGES);
    assert_eq!(current(&s, "a.vin.current"), "0A..20mA");
    assert_eq!(current(&s, "b.vin.current"), "0A..30mA");
    assert_eq!(current(&s, "input.current"), "-50mA..0A");
    assert!(s.circular.is_empty(), "{:?}", s.circular);
}
