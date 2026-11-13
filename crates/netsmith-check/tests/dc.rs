//! The `dc` statement and the test current of a level (ADR 0076, amendment 1): the checker
//! validates both and records them, and nothing else about a build changes.

use netsmith_check::*;

fn run(src: &str) -> Checked {
    check("test.nsm", src, &Loader::std())
}

fn run_with(src: &str, files: &[(&str, &str)]) -> Checked {
    let mut loader = Loader::std();
    for (module, text) in files {
        loader.insert(module, &format!("{}.nsm", module.replace("::", "/")), text);
    }
    check("test.nsm", src, &loader)
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// The one message of a run that must fail with exactly one.
fn only(c: &Checked) -> String {
    let m = messages(c);
    assert_eq!(m.len(), 1, "{}", c.render());
    m[0].clone()
}

/// A part `P` with two pins and a resistance, whose body is `body`, on a board.
fn part(body: &str) -> String {
    format!(
        "part P {{\n    pins {{\n        .a passive \"A\" \"1\"\n        .b passive \"B\" \"2\"\n    }}\n    .resistance: Resistance\n    .forward_voltage: Voltage\n    {body}\n}}\nboard B {{\n    .p: P\n}}\n"
    )
}

/// The `dc` element of the instance at `path`, with the pins and values as dotted names.
fn element(c: &Checked, path: &str) -> Option<(DcKind, Vec<String>, Vec<String>)> {
    let model = &c.board("B").unwrap().model;
    let inst = model.inst_at(path)?;
    let dc = model.instance(inst).dc.as_ref()?;
    Some((
        dc.kind,
        dc.pins.iter().map(|p| model.path_below(*p, inst)).collect(),
        dc.values
            .iter()
            .map(|f| model.field(*f).name.clone())
            .collect(),
    ))
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

// ---- the standard library ---------------------------------------------------------------

#[test]
fn the_standard_parts_state_their_dc_element() {
    // The import, the generic, its element, the pins and the values it names.
    type Case = (
        &'static str,
        &'static str,
        DcKind,
        &'static [&'static str],
        &'static [&'static str],
    );
    let cases: [Case; 9] = [
        (
            "passive::Resistor",
            "Resistor",
            DcKind::Resistor,
            &["a", "b"],
            &["resistance"],
        ),
        (
            "passive::Fuse",
            "Fuse",
            DcKind::Resistor,
            &["a", "b"],
            &["resistance"],
        ),
        (
            "passive::FerriteBead",
            "FerriteBead",
            DcKind::Resistor,
            &["a", "b"],
            &["dc_resistance"],
        ),
        (
            "passive::Inductor",
            "Inductor",
            DcKind::Resistor,
            &["a", "b"],
            &["dc_resistance"],
        ),
        (
            "diode::Schottky",
            "Schottky",
            DcKind::Diode,
            &["anode", "cathode"],
            &["forward_voltage"],
        ),
        (
            "diode::Diode",
            "Diode",
            DcKind::Diode,
            &["anode", "cathode"],
            &["forward_voltage"],
        ),
        (
            "transistor::NMosfet",
            "NMosfet",
            DcKind::Nmos,
            &["gate", "drain", "source"],
            &["gate_threshold", "on_resistance"],
        ),
        (
            "transistor::PMosfet",
            "PMosfet",
            DcKind::Pmos,
            &["gate", "drain", "source"],
            &["gate_threshold", "on_resistance"],
        ),
        (
            "regulator::LinearRegulator",
            "LinearRegulator",
            DcKind::LinearRegulator,
            &["vin", "vout"],
            &["quiescent_current"],
        ),
    ];
    for (import, name, kind, pins, values) in cases {
        let src = format!("import std::parts::{import}\nboard B {{\n    .x: {name}\n}}\n");
        let c = run(&src);
        let found = element(&c, "x").unwrap_or_else(|| panic!("{name} has no dc element"));
        assert_eq!(found, (kind, names(pins), names(values)), "{name}");
    }
}

#[test]
fn the_other_standard_parts_with_a_bridge_state_none_yet() {
    // A part with a `bridge` and no `dc` is a boundary (ADR 0076): these are left to a
    // decision of their own, and nothing here changes how they are solved.
    for (import, name) in [
        ("passive::Capacitor", "Capacitor"),
        ("passive::Led", "Led"),
        ("passive::Switch", "Switch"),
        ("diode::Zener", "Zener"),
        ("diode::Tvs", "Tvs"),
    ] {
        let src = format!("import std::parts::{import}\nboard B {{\n    .x: {name}\n}}\n");
        let c = run(&src);
        assert!(element(&c, "x").is_none(), "{name}");
    }
}

#[test]
fn a_provider_of_a_generic_has_its_dc_element_on_its_own_pins_and_values() {
    let acme = "\
import std::parts::passive::Fuse

part SlowBlow provides Fuse {
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
}
";
    let src = "\
import std::parts::passive::Fuse
import src::acme::SlowBlow
module M {
    .f: SlowBlow
    .g: Fuse
}
board B {
    .m: M
}
";
    let c = run_with(src, &[("src::acme", acme)]);
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.board("B").unwrap().model;
    // A module has no element of its own: the parts in it do.
    assert!(model.instance(model.inst_at("m").unwrap()).dc.is_none());
    for path in ["m.f", "m.g"] {
        let inst = model.inst_at(path).unwrap();
        let dc = model.instance(inst).dc.as_ref().unwrap();
        assert_eq!(dc.kind, DcKind::Resistor, "{path}");
        // The element is on this instance: its pins and value belong to it.
        for pin in &dc.pins {
            assert_eq!(model.owner_of(*pin), Some(inst), "{path}");
        }
        assert_eq!(model.field(dc.values[0]).owner, inst, "{path}");
    }
    assert_eq!(model.dc_elements().count(), 2);
}

#[test]
fn a_provider_that_states_its_own_dc_is_refused_with_both_places() {
    let acme = "\
import std::parts::passive::Fuse

part SlowBlow provides Fuse {
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
    dc resistor .a .b .resistance
}
";
    let src = "\
import src::acme::SlowBlow
board B {
    .f: SlowBlow
}
";
    let c = run_with(src, &[("src::acme", acme)]);
    let m = only(&c);
    assert!(
        m.starts_with("this part already has a `dc` statement (at std/parts/passive.nsm:"),
        "{m}"
    );
    assert!(m.ends_with("a part is one element at DC"), "{m}");
}

#[test]
fn a_build_without_dc_statements_has_no_elements_and_a_part_of_its_own_may_state_one() {
    let c = run(&part(""));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(c.board("B").unwrap().model.dc_elements().count(), 0);
    let c = run(&part("dc resistor .a .b .resistance"));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        element(&c, "p"),
        Some((DcKind::Resistor, names(&["a", "b"]), names(&["resistance"])))
    );
}

#[test]
fn statement_order_does_not_matter_for_dc() {
    let before = "\
part P {
    dc resistor .a .b .resistance
    bridge .a ~> .b
    .resistance: Resistance
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
}
board B {
    .p: P
}
";
    let c = run(before);
    assert!(c.is_ok(), "{}", c.render());
    assert!(element(&c, "p").is_some());
    // A bridge the other way round is the same two pins.
    let c = run(&before.replace("bridge .a ~> .b", "bridge .b ~> .a"));
    assert!(c.is_ok(), "{}", c.render());
}

// ---- what a `dc` statement is refused for -------------------------------------------------

#[test]
fn an_unknown_element_lists_the_known_ones() {
    let m = only(&run(&part("dc coil .a .b .resistance")));
    assert_eq!(
        m,
        "`dc` does not know the element `coil`; the elements are resistor, diode, nmos, pmos, linear_regulator, switching_regulator and source"
    );
}

#[test]
fn the_wrong_number_of_operands_shows_the_statement() {
    let m = only(&run(&part("dc resistor .a .resistance")));
    assert_eq!(
        m,
        "`dc resistor` takes 2 pins and then 1 value, as in `dc resistor .a .b .resistance`; this has 2 operands"
    );
    let m = only(&run(&part("dc diode .a .b .forward_voltage .resistance")));
    assert!(m.contains("takes 2 pins and then 1 value,"), "{m}");
    assert!(m.ends_with("this has 4 operands"), "{m}");
    let m = only(&run(&part("dc nmos .a")));
    assert!(m.contains("takes 3 pins and then 2 values,"), "{m}");
    assert!(m.ends_with("this has 1 operand"), "{m}");
}

#[test]
fn a_pin_operand_must_be_a_pin_of_the_part() {
    let m = only(&run(&part("dc resistor .resistance .b .resistance")));
    assert_eq!(
        m,
        "`dc resistor` takes the terminal here, a pin, and `.resistance` is not one; the statement is `dc resistor .a .b .resistance`"
    );
    let m = only(&run(&part("dc resistor .a .c .resistance")));
    assert_eq!(m, "`c` is not a member of this P");
}

#[test]
fn a_pin_of_another_part_is_refused() {
    let src = "\
part Q {
    pins {
        .x passive \"X\" \"1\"
    }
}
part P {
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
    .resistance: Resistance
    support {
        .q: Q
    }
    dc resistor .a .q.x .resistance
}
board B {
    .p: P
}
";
    let m = only(&run(src));
    assert_eq!(
        m,
        "`.q.x` is not a pin of this part, and `dc resistor` names the pins of the part it is in (the terminal)"
    );
}

#[test]
fn a_value_operand_must_be_a_field_of_the_right_quantity() {
    let m = only(&run(&part("dc resistor .a .b .forward_voltage")));
    assert_eq!(
        m,
        "`dc resistor` takes the resistance here, a resistance, and `.forward_voltage` is a voltage"
    );
    let m = only(&run(&part("dc diode .a .b .resistance")));
    assert_eq!(
        m,
        "`dc diode` takes the forward voltage here, a voltage, and `.resistance` is a resistance"
    );
    let m = only(&run(&part("dc resistor .a .b .a")));
    assert_eq!(
        m,
        "`dc resistor` takes the resistance here, a resistance, and `.a` is not a value; the statement is `dc resistor .a .b .resistance`"
    );
}

#[test]
fn a_transistor_names_gate_drain_source_threshold_and_resistance() {
    let src = "\
part T {
    pins {
        .g passive \"G\" \"1\"
        .s passive \"S\" \"2\"
        .d passive \"D\" \"3\"
    }
    .vth: Voltage
    .ron: Resistance
    .rds: Resistance
    BODY
}
board B {
    .t: T
}
";
    let ok = run(&src.replace("BODY", "dc nmos .g .d .s .vth .ron"));
    assert!(ok.is_ok(), "{}", ok.render());
    assert_eq!(
        element(&ok, "t"),
        Some((
            DcKind::Nmos,
            names(&["g", "d", "s"]),
            names(&["vth", "ron"])
        ))
    );
    // The threshold is a voltage and the on resistance a resistance: swapped, both are named.
    let c = run(&src.replace("BODY", "dc pmos .g .d .s .ron .vth"));
    let m = messages(&c);
    assert_eq!(m.len(), 2, "{m:?}");
    assert!(m[0].contains("takes the gate threshold here, a voltage, and `.ron` is a resistance"));
    assert!(m[1].contains("takes the on resistance here, a resistance, and `.vth` is a voltage"));
}

/// A part with two rails and an efficiency, whose body is `body`.
fn converter(body: &str) -> String {
    format!(
        "import std::sources::Rail\npart C {{\n    .vin: Rail\n    .vout: Rail\n    .eff: Number\n    .vref: Voltage\n    .iq: Current\n    {body}\n}}\nboard B {{\n    .c: C\n}}\n"
    )
}

#[test]
fn a_regulator_names_its_two_rails_and_a_switching_one_its_efficiency() {
    let c = run(&converter("dc switching_regulator .vin .vout .eff"));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        element(&c, "c"),
        Some((
            DcKind::SwitchingRegulator,
            names(&["vin", "vout"]),
            names(&["eff"])
        ))
    );
    let c = run(&converter("dc linear_regulator .vin .vout"));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        element(&c, "c"),
        Some((DcKind::LinearRegulator, names(&["vin", "vout"]), vec![]))
    );
}

/// The quiescent current is an optional operand of `dc linear_regulator` (ADR 0076, amendment 9):
/// with it the element has one value, without it none.
#[test]
fn a_linear_regulator_may_name_its_quiescent_current() {
    let c = run(&converter("dc linear_regulator .vin .vout .iq"));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        element(&c, "c"),
        Some((
            DcKind::LinearRegulator,
            names(&["vin", "vout"]),
            names(&["iq"])
        ))
    );
    // It is a current of the part, and no other quantity or part's field.
    let m = only(&run(&converter("dc linear_regulator .vin .vout .vref")));
    assert_eq!(
        m,
        "`dc linear_regulator` takes the quiescent current here, a current, and `.vref` is a voltage"
    );
    // One operand more than the statement has is refused with the form that has the optional one.
    let m = only(&run(&converter("dc linear_regulator .vin .vout .iq .iq")));
    assert_eq!(
        m,
        "`dc linear_regulator` takes 2 pins and then 0 values and, if the part has one, the quiescent current, as in `dc linear_regulator .vin .vout .quiescent_current`; this has 4 operands"
    );
}

/// A switching regulator has the same optional operand after its efficiency (ADR 0096).
#[test]
fn a_switching_regulator_may_name_its_quiescent_current_after_the_efficiency() {
    let c = run(&converter("dc switching_regulator .vin .vout .eff .iq"));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        element(&c, "c"),
        Some((
            DcKind::SwitchingRegulator,
            names(&["vin", "vout"]),
            names(&["eff", "iq"])
        ))
    );
    // Without it the statement is what it was.
    let c = run(&converter("dc switching_regulator .vin .vout .eff"));
    assert!(c.is_ok(), "{}", c.render());
    // One operand more is refused with the form that has the optional one.
    let m = only(&run(&converter(
        "dc switching_regulator .vin .vout .eff .iq .iq",
    )));
    assert!(
        m.contains("takes 2 pins and then 1 value and, if the part has one, the quiescent current, as in `dc switching_regulator .vin .sw .efficiency .quiescent_current`"),
        "{m}"
    );
}

#[test]
fn a_regulator_operand_must_be_a_rail_of_the_part_and_the_efficiency_a_number() {
    // A pin is not a rail.
    let m = only(&run(&converter("dc linear_regulator .vin.pos .vout")));
    assert_eq!(
        m,
        "`dc linear_regulator` takes the input rail here, a rail, and `.vin.pos` is not one; the statement is `dc linear_regulator .vin .vout`"
    );
    // Neither is a field.
    let m = only(&run(&converter("dc linear_regulator .vin .vref")));
    assert!(
        m.contains("takes the output rail here, a rail, and `.vref` is not one"),
        "{m}"
    );
    // The same rail twice.
    let m = only(&run(&converter("dc linear_regulator .vin .vin")));
    assert_eq!(
        m,
        "`dc linear_regulator` names the same rail twice; its rails are different ones, as in `dc linear_regulator .vin .vout`"
    );
    // The efficiency is a number: a voltage is named for what it is.
    let m = only(&run(&converter("dc switching_regulator .vin .vout .vref")));
    assert!(
        m.contains("takes the efficiency here, a number, and `.vref` is a voltage"),
        "{m}"
    );
}

/// A part with a supply rail, whose body is `body`.
fn supply_part(body: &str) -> String {
    format!(
        "import std::sources::Rail\npart S {{\n    .vbatt: Rail\n    .other: Rail\n    .vref: Voltage\n    {body}\n}}\nboard B {{\n    .s: S\n}}\n"
    )
}

#[test]
fn a_part_that_gives_a_supply_names_its_rail() {
    let c = run(&supply_part("dc source .vbatt"));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        element(&c, "s"),
        Some((DcKind::Source, names(&["vbatt"]), vec![]))
    );
}

#[test]
fn the_connectors_that_give_a_supply_state_a_source() {
    for (import, name, rail) in [
        ("connector::Obd2M", "Obd2M", "vbatt"),
        ("connector::UsbCPower", "UsbCPower", "vbus"),
        (
            "connector::UsbCSinkReceptacle",
            "UsbCSinkReceptacle",
            "vbus",
        ),
    ] {
        let src = format!("import std::parts::{import}\nboard B {{\n    .c: {name}\n}}\n");
        let c = run(&src);
        assert!(
            !messages(&c).iter().any(|m| m.contains("dc")),
            "{}",
            c.render()
        );
        assert_eq!(
            element(&c, "c"),
            Some((DcKind::Source, names(&[rail]), vec![])),
            "{name}"
        );
    }
}

/// A board with an invented receptacle that provides the generic `name`, declared as `.c` with
/// `body` in its braces.
fn receptacle(name: &str, body: &str) -> String {
    format!(
        "import std::parts::connector::{name}\nimport std::sources::Rail\npart R provides {name} {{\n    pins {{\n        .vbus.pos power \"VBUS\" \"A4\"\n        .vbus.neg power \"GND\" \"A1\"\n        .dp bidirectional \"D+\" \"A6\"\n        .dm bidirectional \"D-\" \"A7\"\n        .cc1 passive \"CC1\" \"A5\"\n        .cc2 passive \"CC2\" \"B5\"\n        .sbu1 passive \"SBU1\" \"A8\"\n        .sbu2 passive \"SBU2\" \"B8\"\n        .shell passive \"SHELL\" \"S1\"\n    }}\n}}\nboard B {{\n    .vbus: Rail\n    .c: {name} {{ {body} }}\n    .vbus ~ .c.vbus\n}}\n"
    )
}

#[test]
fn the_source_and_dual_role_receptacles_state_no_source() {
    // A source port's VBUS is driven by the board, and a dual-role port's goes either way: neither
    // is a supply of the board (#463).
    for name in ["UsbCSourceReceptacle", "UsbCDualRoleReceptacle"] {
        let c = run(&receptacle(name, ""));
        assert!(c.is_ok(), "{}", c.render());
        assert_eq!(element(&c, "c"), None, "{name}");
    }
}

#[test]
fn a_source_receptacle_takes_the_current_the_board_allows() {
    let c = run(&receptacle(
        "UsbCSourceReceptacle",
        ".vbus.current &= 0A..3A",
    ));
    assert!(c.is_ok(), "{}", c.render());
    // A negative current, a port that would supply the board, is the solver's to refuse
    // (`netsmith-solve`, `usb_receptacles.rs`).
}

#[test]
fn a_dual_role_receptacle_takes_a_signed_current() {
    let c = run(&receptacle(
        "UsbCDualRoleReceptacle",
        ".vbus.current &= -3A..1.5A",
    ));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_source_operand_must_be_a_rail_of_the_part() {
    // A pin, and a field, are not a rail.
    let m = only(&run(&supply_part("dc source .vbatt.pos")));
    assert_eq!(
        m,
        "`dc source` takes the supply rail here, a rail, and `.vbatt.pos` is not one; the statement is `dc source .vbatt`"
    );
    let m = only(&run(&supply_part("dc source .vref")));
    assert!(
        m.contains("takes the supply rail here, a rail, and `.vref` is not one"),
        "{m}"
    );
    // More operands than the one rail.
    let m = only(&run(&supply_part("dc source .vbatt .other")));
    assert!(
        m.contains("`dc source` takes 1 pins and then 0 values, as in `dc source .vbatt`; this has 2 operands"),
        "{m}"
    );
    // One `dc` statement per part.
    let m = only(&run(&supply_part("dc source .vbatt\n    dc source .other")));
    assert!(m.contains("this part already has a `dc` statement"), "{m}");
}

#[test]
fn a_rail_inside_an_interface_of_the_part_is_a_rail_of_the_part() {
    // A part may name the rail of one of its interfaces, through it.
    let src = "import std::sources::Rail\ninterface Out {\n    .sw: Rail\n    .efficiency: Number\n}\npart C {\n    .vin: Rail\n    .out: Out\n    dc switching_regulator .vin .out.sw .out.efficiency\n}\nboard B {\n    .c: C\n}\n";
    let c = run(src);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        element(&c, "c"),
        Some((
            DcKind::SwitchingRegulator,
            names(&["vin", "out.sw"]),
            names(&["efficiency"])
        ))
    );
}

#[test]
fn the_same_pin_twice_is_refused() {
    let m = only(&run(&part("dc resistor .a .a .resistance")));
    assert_eq!(
        m,
        "`dc resistor` names the same pin twice; its pins are different ones, as in `dc resistor .a .b .resistance`"
    );
}

#[test]
fn a_second_dc_statement_names_the_first() {
    let m = only(&run(&part(
        "dc resistor .a .b .resistance\n    dc diode .a .b .forward_voltage",
    )));
    assert_eq!(
        m,
        "this part already has a `dc` statement (at test.nsm:8); a part is one element at DC"
    );
}

#[test]
fn only_a_part_has_a_dc_element() {
    let src = "\
module M {
    dc resistor .a .b .r
}
board B {
    .m: M
}
";
    let m = only(&run(src));
    assert_eq!(
        m,
        "`dc` can only appear inside a part, and this is a module: a module's DC network is the elements of the parts inside it"
    );
    let m = only(&run("board B {\n    dc resistor .a .b .r\n}\n"));
    assert!(
        m.starts_with("`dc` can only appear inside a part, and this is a board"),
        "{m}"
    );
}

#[test]
fn the_element_runs_between_the_pins_the_bridge_joins() {
    let src = "\
part P {
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
        .c passive \"C\" \"3\"
    }
    .resistance: Resistance
    bridge .a ~> .b
    dc resistor .a .c .resistance
}
board B {
    .p: P
}
";
    let m = only(&run(src));
    assert_eq!(
        m,
        "`dc resistor` runs between .a and .c, and this part's `bridge` joins .a and .b: a `~>` chain passes through the element the part is at DC, so the two must be the same pins"
    );
    // A transistor's channel is its drain and source, whatever the gate does.
    let fet = "\
part T {
    pins {
        .g passive \"G\" \"1\"
        .s passive \"S\" \"2\"
        .d passive \"D\" \"3\"
    }
    .vth: Voltage
    .ron: Resistance
    bridge .d ~> .g
    dc nmos .g .d .s .vth .ron
}
board B {
    .t: T
}
";
    let m = only(&run(fet));
    assert!(
        m.starts_with("`dc nmos` runs between .d and .s, and this part's `bridge` joins .d and .g"),
        "{m}"
    );
}

#[test]
fn dc_is_not_a_physical_statement() {
    let board = "\
part P {
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
    .resistance: Resistance
}
board B {
    .p: P
}
";
    let phys = "physical for B {\n    dc resistor .p.a .p.b .p.resistance\n}\n";
    let loader = Loader::std().with_physical("test.nsm", "test.physical.nsm", phys);
    let c = check("test.nsm", board, &loader);
    let m = messages(&c);
    assert!(
        m.iter()
            .any(|m| m.contains("a `dc` statement changes the circuit")),
        "{m:?}"
    );
}

// ---- a level with its test current -------------------------------------------------------

/// A part with an output `Logic`, a supply, and `body` in it.
fn driver(body: &str) -> String {
    format!(
        "import std::logic::Logic\nimport std::sources::Rail\n\npart Driver {{\n    .vdd: Rail\n    .out: Logic\n    .out.reference ~ .vdd\n    {body}\n    pins {{\n        .vdd.pos  power  \"VDD\" \"1\"\n        .vdd.neg  power  \"GND\" \"2\"\n        .out.line output \"OUT\" \"3\"\n    }}\n}}\n\nboard B {{\n    .supply: Rail {{\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }}\n    .u: Driver\n    .supply ~ .u.vdd\n}}\n"
    )
}

#[test]
fn a_level_with_a_test_current_means_what_it_means_without_it() {
    let with = run(&driver(
        ".out.voh &= .vdd.voltage - 0.4V at 2mA\n    .out.vol &= 0.4V at 2mA",
    ));
    let without = run(&driver(
        ".out.voh &= .vdd.voltage - 0.4V\n    .out.vol &= 0.4V",
    ));
    assert!(with.is_ok(), "{}", with.render());
    assert!(without.is_ok(), "{}", without.render());
    let (a, b) = (
        &with.board("B").unwrap().model,
        &without.board("B").unwrap().model,
    );
    // Every field has the same domain, and the relations read the same.
    let show = |m: &Model| -> Vec<String> {
        m.fields()
            .map(|(id, f)| format!("{} {}", m.path_of_field(id), f.domain))
            .collect()
    };
    assert_eq!(show(a), show(b));
    let rels = |m: &Model| -> Vec<String> {
        m.relations()
            .iter()
            .map(|r| {
                format!(
                    "{} {} {}",
                    m.show_expr(&r.lhs),
                    r.op.symbol(),
                    m.show_expr(&r.rhs)
                )
            })
            .collect()
    };
    assert_eq!(rels(a), rels(b));
    assert!(b.level_tests().is_empty());
}

#[test]
fn the_test_current_of_a_level_is_recorded_with_the_level() {
    let c = run(&driver(
        ".out.voh &= .vdd.voltage - 0.4V at 2mA\n    .out.vol &= 0.1V..0.4V at 8mA",
    ));
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    let found: Vec<(String, String, String)> = m
        .level_tests()
        .iter()
        .map(|t| {
            (
                m.path_of_field(t.level),
                t.op.symbol().to_string(),
                t.current.to_string(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            ("u.out.voh".to_string(), "&=".to_string(), "2mA".to_string()),
            ("u.out.vol".to_string(), "&=".to_string(), "8mA".to_string()),
        ]
    );
    let voh = m.lookup("u.out.voh");
    let Some(Member::Field(f)) = voh else {
        panic!()
    };
    assert_eq!(m.level_test(f).unwrap().current.lo(), 0.002);
}

#[test]
fn a_bus_levels_member_takes_a_test_current_too() {
    let src = "\
import std::logic::Levels
import std::sources::Rail

part Chip {
    .vdd: Rail
    .levels: Levels
    .levels.reference ~ .vdd
    .levels.voh &= .vdd.voltage - 0.4V at 2mA
    pins {
        .vdd.pos power \"VDD\" \"1\"
        .vdd.neg power \"GND\" \"2\"
    }
}
board B {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
    }
    .u: Chip
    .supply ~ .u.vdd
}
";
    let c = run(src);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(c.board("B").unwrap().model.level_tests().len(), 1);
}

#[test]
fn an_input_level_has_no_test_current() {
    let m = only(&run(&driver(".out.vih &= 2V at 2mA")));
    assert_eq!(
        m,
        "`at <current>` states the load of an output level; `.out.vih` is a level that a datasheet states without a load: only an output level, `voh` or `vol`, is measured at a current"
    );
}

#[test]
fn a_value_that_is_not_a_level_has_no_test_current() {
    let src = "\
module M {
    .v: Voltage
    .v &= 3V at 2mA
}
board B {
    .m: M
}
";
    let m = only(&run(src));
    assert_eq!(
        m,
        "`at <current>` states the load of an output level; `.v` is not an output level (`voh` or `vol` of a `Logic` or of `Levels`)"
    );
}

#[test]
fn a_test_current_is_a_current_a_number_and_above_zero() {
    let m = only(&run(&driver(".out.voh &= 2.4V at 2V")));
    assert_eq!(
        m,
        "`at` takes the current the level is measured at, such as `2mA`, not a voltage"
    );
    let m = only(&run(&driver(".out.voh &= 2.4V at \"2mA\"")));
    assert!(m.contains("such as `2mA`, not text"), "{m}");
    let m = only(&run(&driver(".out.voh &= 2.4V at 1mA..2mA")));
    assert_eq!(m, "the test current is one number, not the range 1mA..2mA");
    let m = only(&run(&driver(".out.voh &= 2.4V at 0mA")));
    assert_eq!(m, "the test current must be above zero, not 0A");
    let m = only(&run(&driver(".out.voh &= 2.4V at .vdd.current")));
    assert_eq!(
        m,
        "the test current is the number the datasheet prints (`at 2mA`), not a value that depends on another field"
    );
}

#[test]
fn a_level_is_stated_at_one_load() {
    let m = only(&run(&driver(
        ".out.voh &= 2.4V at 2mA\n    .out.voh >= 2.0V at 8mA",
    )));
    assert!(
        m.starts_with("`.out.voh` is already stated at 2mA (at test.nsm:"),
        "{m}"
    );
    assert!(
        m.ends_with("a level is one value at one load, so keep one `at`"),
        "{m}"
    );
}

#[test]
fn at_needs_a_level_on_the_left_of_the_statement() {
    let m = only(&run(&driver(".out.voh + 1V >= 2.4V at 2mA")));
    assert_eq!(
        m,
        "`at` needs a level on the left of the statement, such as `.voh &= .vdd.voltage - 0.4V at 2mA`"
    );
}
