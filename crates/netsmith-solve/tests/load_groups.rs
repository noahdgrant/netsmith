//! A group of loads, at most N of them on at once (issue #806): `shared NAME [limit N] { .load
//! ... }` in a board or a module. The members of a group on one net add up to the worst N, for
//! the balance of the net (the current a supply gives, and so a regulator's rating), a stated
//! budget, and the DC analysis. Without the statement every load is on at once, as before.
//! Every part and number is invented.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"import std::sources::Rail

/// A linear regulator rated 100 mA.
part Ldo {
    .vin: Rail
    .vout: Rail
    .max_current: Current
    .max_current &= 100mA
    .vin.neg ~ .vout.neg
    .vin.current == -.vout.current
    -.vout.current <= .max_current
}

/// A link with a resistance, which the DC analysis models.
part Link {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    #[role(current)]
    .current: Current
    .drop: Voltage
    .drop == .current * .resistance
    bridge .a ~> .b
    dc resistor .a .b .resistance
}

/// A switching regulator: 5 V out at 90 %.
part Sw {
    .vin: Rail
    .vout: Rail
    .efficiency: Number
    .vin.neg ~ .vout.neg
    dc switching_regulator .vin .vout .efficiency
}

/// A heater on a rail of its own, for a module port in a group.
part Heater {
    .vdd: Rail
    .vdd.current &= 0mA..60mA
}

module Zone {
    .vdd: Rail
    .heater: Heater
    .vdd ~ .heater.vdd
}
"#;

fn solved(body: &str) -> Solution {
    let c = check(
        "test.nsm",
        &format!("{PARTS}\nboard B {{\n{body}}}\n"),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> (f64, f64) {
    let f = s.model.field_at(path).expect("the field is there");
    match &s.model.field(f).domain {
        Domain::Quantity(q) => (q.lo(), q.hi()),
        other => panic!("`{path}` is {other}"),
    }
}

/// A 3.3 V regulator rated 100 mA with three loads of up to 60 mA on its output, and `group`.
fn three_loads(group: &str) -> String {
    format!(
        "    .input: Rail {{\n        .voltage &= 5V\n        .pos.net &= \"VIN\"\n    }}\n    .reg: Ldo\n    .input ~ .reg.vin\n    .a: Rail {{ .current &= 0mA..60mA }}\n    .b: Rail {{ .current &= 0mA..60mA }}\n    .c: Rail {{ .current &= 0mA..60mA }}\n    .reg.vout ~ .a\n    .reg.vout ~ .b\n    .reg.vout ~ .c\n    .a.pos.net &= \"V3\"\n{group}"
    )
}

#[test]
fn without_a_group_every_load_is_on_and_the_regulator_is_refused() {
    let s = solved(&three_loads(""));
    let all = messages(&s).join("\n");
    assert!(
        all.contains("0A..180mA is not always below 100mA"),
        "{all}"
    );
    assert!(!all.contains("on at once"), "{all}");
}

#[test]
fn one_of_a_group_on_at_once_draws_the_largest_load_only() {
    let s = solved(&three_loads("    shared LOADS { .a .b .c }\n"));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    let (lo, hi) = range(&s, "reg.vout.current");
    assert!(lo.abs() > 59e-3 && lo.abs() < 61e-3, "{lo}");
    assert!(hi.abs() < 1e-12, "{hi}");
}

#[test]
fn a_refused_supply_names_the_group() {
    let s = solved(&three_loads("    shared LOADS limit 2 { .a .b .c }\n"));
    let all = messages(&s).join("\n");
    assert!(
        all.contains("0A..120mA is not always below 100mA"),
        "{all}"
    );
    assert!(all.contains("with at most 2 of `LOADS` on at once"), "{all}");
}

#[test]
fn a_load_outside_the_group_is_always_on() {
    let s = solved(&three_loads("    shared LOADS { .a .b }\n"));
    // `c` and the larger of `a` and `b`: 120 mA.
    let all = messages(&s).join("\n");
    assert!(
        all.contains("0A..120mA is not always below 100mA"),
        "{all}"
    );
}

/// A group of the low ends and the largest spans: a load that is off draws the low end of its
/// range.
#[test]
fn a_load_that_is_off_draws_the_low_end_of_its_range() {
    let body = three_loads("    shared LOADS { .a .b .c }\n")
        .replace(".a: Rail { .current &= 0mA..60mA }", ".a: Rail { .current &= 10mA..30mA }");
    let s = solved(&body);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    // Off, `a` draws 10 mA; `b` or `c` on adds 60 mA: 10 mA..70 mA.
    let (lo, hi) = range(&s, "reg.vout.current");
    assert!((lo + 70e-3).abs() < 1e-6, "{lo}");
    assert!((hi + 10e-3).abs() < 1e-6, "{hi}");
}

/// A module's port in a group stands for what the module draws.
#[test]
fn a_module_port_is_a_member() {
    let body = "    .input: Rail {\n        .voltage &= 5V\n        .pos.net &= \"VIN\"\n    }\n    .reg: Ldo\n    .input ~ .reg.vin\n    .z1: Zone\n    .z2: Zone\n    .reg.vout ~ .z1.vdd\n    .reg.vout ~ .z2.vdd\n    .z1.vdd.pos.net &= \"V3\"\n";
    let without = solved(body);
    let (lo, _) = range(&without, "reg.vout.current");
    assert!((lo + 120e-3).abs() < 1e-6, "{lo}");
    let with = solved(&format!("{body}    shared ZONES {{ .z1.vdd .z2.vdd }}\n"));
    assert!(with.diagnostics.is_empty(), "{:?}", messages(&with));
    let (lo, _) = range(&with, "reg.vout.current");
    assert!((lo + 60e-3).abs() < 1e-6, "{lo}");
}

/// The DC analysis leaves out the corners with more loads of a group on than it allows: the
/// current through the link in front of a switching regulator is what one 2 A load asks.
fn dc_tree(group: &str) -> String {
    format!(
        "    .supply: Rail {{\n        .voltage &= 9V..16V\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }}\n    .link: Link {{ .resistance &= 0.1ohm }}\n    .reg: Sw {{ .efficiency &= 0.9; .vout.voltage &= 5V }}\n    .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .x: Rail {{ .current &= 0A..2A }}\n    .y: Rail {{ .current &= 0A..2A }}\n    .reg.vout ~ .x\n    .reg.vout ~ .y\n    .x.pos.net &= \"V5\"\n{group}"
    )
}

#[test]
fn the_dc_analysis_takes_the_worst_of_a_group() {
    let without = solved(&dc_tree(""));
    assert!(without.diagnostics.is_empty(), "{:?}", messages(&without));
    let with = solved(&dc_tree("    shared OUTS { .x .y }\n"));
    assert!(with.diagnostics.is_empty(), "{:?}", messages(&with));
    let (_, all) = range(&without, "link.current");
    let (_, one) = range(&with, "link.current");
    // 4 A at 5 V from 9 V at 90 % is about 2.5 A; 2 A is about half of it.
    assert!(all > 2.4 && all < 2.6, "{all}");
    assert!(one > 1.2 && one < 1.3, "{one}");
    assert!(with.dc.solved >= 1, "{:?}", with.dc);
}

/// Two loads of a group behind one link are one island: its corners with both on are left out.
fn behind_a_link(group: &str) -> String {
    format!(
        "    .supply: Rail {{\n        .voltage &= 5V\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }}\n    .link: Link {{ .resistance &= 0.1ohm }}\n    .x: Rail {{ .current &= 0A..2A }}\n    .y: Rail {{ .current &= 0A..1A }}\n    .supply.pos ~> .link ~> .x.pos\n    .x ~ .y\n    .x.neg ~ .supply.neg\n{group}"
    )
}

#[test]
fn an_island_leaves_out_the_corners_with_too_many_loads_on() {
    let without = solved(&behind_a_link(""));
    assert!(without.diagnostics.is_empty(), "{:?}", messages(&without));
    let with = solved(&behind_a_link("    shared OUTS { .x .y }\n"));
    assert!(with.diagnostics.is_empty(), "{:?}", messages(&with));
    let (_, all) = range(&without, "link.current");
    let (_, one) = range(&with, "link.current");
    assert!((all - 3.0).abs() < 1e-6, "{all}");
    assert!((one - 2.0).abs() < 1e-6, "{one}");
}
