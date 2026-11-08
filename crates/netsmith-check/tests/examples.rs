//! The checker on the two example boards.

use netsmith_check::*;

const LED: &str = include_str!("../../../examples/led.nsm");
const STATUS: &str = include_str!("../../../examples/status_indicator.nsm");

fn range(model: &Model, path: &str) -> (f64, f64) {
    match model.domain_at(path) {
        Some(Domain::Quantity(q)) => (q.lo(), q.hi()),
        other => panic!("`{path}` is {other:?}"),
    }
}

fn close(a: (f64, f64), b: (f64, f64)) -> bool {
    let near = |x: f64, y: f64| (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1e-30);
    near(a.0, b.0) && near(a.1, b.1)
}

fn net_of<'a>(model: &'a Model, pin: &str) -> &'a Net {
    let id = model
        .inst_at(pin)
        .unwrap_or_else(|| panic!("no pin `{pin}`"));
    model
        .nets()
        .iter()
        .find(|n| n.pins.contains(&id))
        .unwrap_or_else(|| panic!("`{pin}` is on no net"))
}

fn has_pin(model: &Model, net: &Net, pin: &str) -> bool {
    net.pins.contains(&model.inst_at(pin).unwrap())
}

#[test]
fn led_board_checks_clean() {
    let c = check("examples/led.nsm", LED, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(c.boards.len(), 1);
    assert_eq!(c.boards[0].name, "LedExample");
}

#[test]
fn led_board_narrows_what_the_source_states() {
    let c = check("examples/led.nsm", LED, &Loader::std());
    let m = &c.board("LedExample").unwrap().model;

    assert!(close(range(m, "supply.voltage"), (4.75, 5.25)));
    assert!(close(range(m, "led.current"), (0.005, 0.015)));
    assert_eq!(
        m.domain_at("supply.pos.net"),
        Some(&Domain::Text("VCC".into()))
    );
    assert_eq!(
        m.domain_at("supply.neg.net"),
        Some(&Domain::Text("GND".into()))
    );
    // Left for the solver to choose, with the LED's forward voltage from its catalog.
    assert_eq!(m.domain_at("led.forward_voltage"), Some(&Domain::Unset));
    assert_eq!(m.domain_at("r1.resistance"), Some(&Domain::Unset));
}

#[test]
fn led_board_has_three_nets_and_one_chain() {
    let c = check("examples/led.nsm", LED, &Loader::std());
    let m = &c.board("LedExample").unwrap().model;

    assert_eq!(m.nets().len(), 3);
    let vcc = net_of(m, "supply.pos");
    assert_eq!(vcc.name, "VCC");
    assert!(has_pin(m, vcc, "r1.a"));

    let mid = net_of(m, "r1.b");
    assert_eq!(mid.name, "net-led.anode", "named for its smallest terminal");
    assert!(!mid.explicit);
    assert!(has_pin(m, mid, "led.anode"));

    let gnd = net_of(m, "supply.neg");
    assert_eq!(gnd.name, "GND");
    assert!(has_pin(m, gnd, "led.cathode"));

    assert_eq!(m.chains().len(), 1);
    let chain = &m.chains()[0];
    assert_eq!(chain.start, m.inst_at("supply.pos").unwrap());
    assert_eq!(chain.end, m.inst_at("supply.neg").unwrap());
    let parts: Vec<_> = chain.parts.iter().map(|&p| m.path_of(p)).collect();
    assert_eq!(parts, ["r1", "led"]);
}

#[test]
fn led_board_leaves_five_relations_for_the_solver() {
    let c = check("examples/led.nsm", LED, &Loader::std());
    let m = &c.board("LedExample").unwrap().model;
    // The board's own equation, Ohm's law and the power rating in the resistor, the LED's drop
    // and the LED's current, which cannot be negative.
    assert_eq!(m.relations().len(), 5);
    let board_equation = m
        .relations()
        .iter()
        .find(|r| {
            matches!(&r.lhs, RExpr::Field(f) if m.path_of_field(*f) == "led.current")
                && matches!(r.rhs, RExpr::Bin(..))
        })
        .expect("the board's equation");
    assert!(matches!(board_equation.rhs, RExpr::Bin(..)));
}

#[test]
fn parts_know_their_bridge_and_catalog() {
    let c = check("examples/led.nsm", LED, &Loader::std());
    let m = &c.board("LedExample").unwrap().model;
    let r1 = m.instance(m.inst_at("r1").unwrap());
    let (from, to) = r1.bridge.expect("a resistor bridges");
    assert_eq!(
        (m.path_of(from).as_str(), m.path_of(to).as_str()),
        ("r1.a", "r1.b")
    );
    let catalog = r1.catalog.as_ref().expect("a resistor has a catalog");
    assert_eq!(catalog.name, "resistor");
    let fields: Vec<_> = catalog.fields.iter().map(|&f| m.path_of_field(f)).collect();
    assert_eq!(
        fields,
        [
            "r1.resistance",
            "r1.max_power",
            "r1.package",
            "r1.temperature_coefficient"
        ]
    );
    assert_eq!(m.instance(m.inst_at("r1.a").unwrap()).kind, InstKind::Pin);
    // The pad is in the pin table, not a field of the pin (ADR 0016).
    assert!(m.domain_at("r1.a.pad").is_none());
    let row = m
        .pin_row(m.inst_at("r1.a").unwrap())
        .expect("a resistor has a pin table");
    assert_eq!(
        (row.kind, row.name.as_str(), row.pads.as_slice()),
        (PinKind::Passive, "A", ["1".to_string()].as_slice())
    );
}

#[test]
fn status_example_checks_clean() {
    let c = check("examples/status_indicator.nsm", STATUS, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(c.boards[0].name, "IndicatorExample");
}

#[test]
fn status_example_values_and_wiring() {
    let c = check("examples/status_indicator.nsm", STATUS, &Loader::std());
    let m = &c.board("IndicatorExample").unwrap().model;

    assert!(close(range(m, "power_3v3.voltage"), (3.135, 3.465)));
    assert!(close(
        range(m, "status_indicator.current_limit.resistance"),
        (990.0, 1010.0)
    ));
    // The bypass capacitor is a marked value; its effective capacitance is never asked for,
    // so nothing is worked out for it. Its bias comes from the rail it sits across.
    assert!(close(
        range(m, "status_indicator.bypass.capacitance"),
        (80e-9, 120e-9)
    ));
    assert_eq!(
        m.domain_at("status_indicator.bypass.effective_capacitance"),
        Some(&Domain::Unset)
    );
    assert_eq!(
        m.domain_at("status_indicator.led.color"),
        Some(&Domain::Text("GREEN".into()))
    );

    // One rail joins the source, the logic reference, and the module's power input.
    let positive = net_of(m, "power_3v3.pos");
    for pin in [
        "status_enable.reference.pos",
        "status_indicator.power.pos",
        "status_indicator.enable.reference.pos",
        "status_indicator.bypass.a",
    ] {
        assert!(
            has_pin(m, positive, pin),
            "{pin} should be on the positive rail"
        );
    }
    let ground = net_of(m, "power_3v3.neg");
    assert!(has_pin(m, ground, "status_indicator.led.cathode"));
    assert!(has_pin(m, ground, "status_indicator.bypass.b"));

    // The logic input drives the LED branch through the current-limiting resistor.
    let enable = net_of(m, "status_enable.line");
    assert!(has_pin(m, enable, "status_indicator.enable.line"));
    assert!(has_pin(m, enable, "status_indicator.current_limit.a"));

    assert_eq!(m.chains().len(), 2);
}

#[test]
fn connected_interfaces_tie_their_values_together() {
    let c = check("examples/status_indicator.nsm", STATUS, &Loader::std());
    let m = &c.board("IndicatorExample").unwrap().model;
    let pairs: Vec<_> = m
        .equalities()
        .iter()
        .map(|&(a, b)| (m.path_of_field(a), m.path_of_field(b)))
        .collect();
    // Four joins of rails, each tying a voltage, a peak voltage and a track width, the
    // one current a series chain passes, and the swing of the one joined logic line (ADR 0081).
    assert_eq!(pairs.len(), 14, "{pairs:?}");
    assert!(pairs.contains(&(
        "power_3v3.voltage".to_string(),
        "status_indicator.power.voltage".to_string()
    )));
    assert!(pairs.contains(&(
        "status_indicator.current_limit.current".to_string(),
        "status_indicator.led.current".to_string()
    )));
}

fn terminal_paths(m: &Model, net: &Net) -> Vec<String> {
    net.terminals.iter().map(|&p| m.path_of(p)).collect()
}

#[test]
fn a_rail_the_board_is_given_reaches_the_header_that_supplies_it() {
    let c = check("examples/led.nsm", LED, &Loader::std());
    let m = &c.board("LedExample").unwrap().model;
    // The rail's own pins are ports. The terminals are the header and the parts.
    assert_eq!(sorted_terminals(m, "supply.pos"), ["power_in.p1", "r1.a"]);
    assert_eq!(
        sorted_terminals(m, "supply.neg"),
        ["led.cathode", "power_in.p2"]
    );
    assert_eq!(sorted_terminals(m, "r1.b"), ["led.anode", "r1.b"]);
    assert!(has_pin(m, net_of(m, "supply.pos"), "supply.pos"));
}

#[test]
fn a_board_input_reaches_the_header_it_comes_in_through() {
    let c = check("examples/status_indicator.nsm", STATUS, &Loader::std());
    let m = &c.board("IndicatorExample").unwrap().model;
    assert_eq!(
        sorted_terminals(m, "status_enable.line"),
        ["enable_in.p1", "status_indicator.current_limit.a"]
    );
    assert!(has_pin(
        m,
        net_of(m, "status_enable.line"),
        "status_enable.line"
    ));
}

#[test]
fn rail_pins_inside_a_module_are_ports_not_terminals() {
    let c = check("examples/status_indicator.nsm", STATUS, &Loader::std());
    let m = &c.board("IndicatorExample").unwrap().model;
    let positive = net_of(m, "power_3v3.pos");
    // The board's rail and the module's `power` input both only forward the rail to the
    // capacitor inside the module, so only the header and the capacitor are terminals.
    assert!(has_pin(m, positive, "power_3v3.pos"));
    assert!(has_pin(m, positive, "status_indicator.power.pos"));
    let mut terminals = terminal_paths(m, positive);
    terminals.sort();
    assert_eq!(terminals, ["power_in.p1", "status_indicator.bypass.a"]);
}

const REGULATOR: &str = include_str!("../../../examples/linear_regulator.nsm");

fn sorted_terminals(m: &Model, pin: &str) -> Vec<String> {
    let mut paths = terminal_paths(m, net_of(m, pin));
    paths.sort();
    paths
}

#[test]
fn regulator_board_checks_clean() {
    let c = check("examples/linear_regulator.nsm", REGULATOR, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(c.boards.len(), 1);
    assert_eq!(c.boards[0].name, "RegulatorExample");
}

#[test]
fn regulator_board_has_an_input_a_ground_and_an_output_net() {
    let c = check("examples/linear_regulator.nsm", REGULATOR, &Loader::std());
    let m = &c.board("RegulatorExample").unwrap().model;
    assert_eq!(m.nets().len(), 3);

    let vin = net_of(m, "input.pos");
    assert_eq!(vin.name, "VIN");
    assert_eq!(
        sorted_terminals(m, "input.pos"),
        [
            "power_in.p1",
            "stage.input_cap.a",
            "stage.regulator.vin.pos"
        ]
    );

    let out = net_of(m, "output.pos");
    assert_eq!(out.name, "3V3");
    assert_eq!(
        sorted_terminals(m, "output.pos"),
        [
            "power_out.p1",
            "stage.output_cap.a",
            "stage.regulator.vout.pos"
        ]
    );

    // The input and output rails are both named GND, and the regulator joins them.
    let gnd = net_of(m, "input.neg");
    assert_eq!(gnd.name, "GND");
    assert!(has_pin(m, gnd, "output.neg"));
    assert_eq!(
        sorted_terminals(m, "input.neg"),
        [
            "power_in.p2",
            "power_out.p2",
            "stage.input_cap.b",
            "stage.output_cap.b",
            "stage.regulator.vin.neg",
            "stage.regulator.vout.neg"
        ]
    );
}

#[test]
fn regulator_board_leaves_the_part_choice_and_the_headroom_check_to_the_solver() {
    let c = check("examples/linear_regulator.nsm", REGULATOR, &Loader::std());
    let m = &c.board("RegulatorExample").unwrap().model;

    // The board sets the stage's output rail. The regulator inside learns it from the
    // equality that the connection records, which the solver propagates.
    assert!(close(range(m, "stage.vout.voltage"), (3.135, 3.465)));
    assert_eq!(
        m.domain_at("stage.regulator.vout.voltage"),
        Some(&Domain::Unset)
    );
    assert_eq!(m.domain_at("stage.regulator.dropout"), Some(&Domain::Unset));

    let regulator = m.instance(m.inst_at("stage.regulator").unwrap());
    let catalog = regulator
        .catalog
        .as_ref()
        .expect("a regulator has a catalog");
    assert_eq!(catalog.name, "ldo");
    let columns: Vec<_> = catalog.fields.iter().map(|&f| m.path_of_field(f)).collect();
    assert_eq!(
        columns,
        [
            "stage.regulator.vout.voltage",
            "stage.regulator.dropout",
            "stage.regulator.max_current",
            "stage.regulator.max_power",
            "stage.regulator.package"
        ]
    );

    let headroom = m
        .relations()
        .iter()
        .find(|r| r.op == netsmith_lang::ast::CmpOp::Ge)
        .expect("the dropout requirement");
    assert!(
        matches!(&headroom.lhs, RExpr::Field(f) if m.path_of_field(*f) == "stage.regulator.vin.voltage")
    );
    assert!(matches!(headroom.rhs, RExpr::Bin(..)));

    assert_eq!(m.chains().len(), 2, "one decoupling capacitor on each side");
}

#[test]
fn rails_connected_through_the_stage_share_their_voltage() {
    let c = check("examples/linear_regulator.nsm", REGULATOR, &Loader::std());
    let m = &c.board("RegulatorExample").unwrap().model;
    let pairs: Vec<_> = m
        .equalities()
        .iter()
        .map(|&(a, b)| (m.path_of_field(a), m.path_of_field(b)))
        .collect();
    // Four joins of rails, each tying a voltage, a peak voltage and a track width.
    assert_eq!(pairs.len(), 12, "{pairs:?}");
    assert!(pairs.contains(&("input.voltage".to_string(), "stage.vin.voltage".to_string())));
    assert!(pairs.contains(&(
        "stage.regulator.vout.voltage".to_string(),
        "stage.vout.voltage".to_string()
    )));
}

/// The signed terms of a balance, as `path` or `-path`.
fn terms(m: &Model, scope: &str, net: &str) -> Vec<String> {
    let b = m
        .balances()
        .iter()
        .find(|b| m.path_of(b.scope) == scope && b.net == net)
        .unwrap_or_else(|| panic!("no balance for `{net}` in `{scope}`"));
    b.draws
        .iter()
        .map(|d| {
            let sign = if d.flipped { "-" } else { "" };
            format!("{sign}{}", m.path_of_field(d.field))
        })
        .collect()
}

#[test]
fn a_module_port_counts_flipped_inside_the_module_and_as_it_is_outside() {
    let c = check("examples/linear_regulator.nsm", REGULATOR, &Loader::std());
    let m = &c.board("RegulatorExample").unwrap().model;

    // On the board, the input and output rails and the stage's ports are all draws.
    assert_eq!(
        terms(m, "", "3V3"),
        ["output.current", "stage.vout.current"]
    );
    assert_eq!(terms(m, "", "VIN"), ["input.current", "stage.vin.current"]);

    // Inside the stage, the ports are the supply of what is inside, so they are flipped,
    // and the regulator's rails count as they are. The capacitors are not rails.
    assert_eq!(
        terms(m, "stage", "VIN"),
        ["stage.regulator.vin.current", "-stage.vin.current"]
    );
    assert_eq!(
        terms(m, "stage", "3V3"),
        ["stage.regulator.vout.current", "-stage.vout.current"]
    );
}

#[test]
fn connecting_rails_shares_their_voltage_but_not_their_current() {
    let c = check("examples/linear_regulator.nsm", REGULATOR, &Loader::std());
    let m = &c.board("RegulatorExample").unwrap().model;
    let equated = |name: &str| {
        m.equalities()
            .iter()
            .any(|&(a, b)| m.path_of_field(a) == name || m.path_of_field(b) == name)
    };
    assert!(equated("input.voltage"));
    assert!(!equated("input.current"));
    assert!(!equated("stage.vin.current"));
}

#[test]
fn a_rail_nested_in_an_interface_belongs_to_the_board_or_module_around_it() {
    let c = check("examples/status_indicator.nsm", STATUS, &Loader::std());
    let m = &c.board("IndicatorExample").unwrap().model;
    // `status_enable.reference` is a rail of the board, like `power_3v3`.
    let on_board = terms(m, "", &net_of(m, "power_3v3.pos").name);
    assert!(on_board.contains(&"power_3v3.current".to_string()));
    assert!(on_board.contains(&"status_enable.reference.current".to_string()));
    assert!(on_board.contains(&"status_indicator.power.current".to_string()));
}
