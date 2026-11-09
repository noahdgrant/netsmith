//! The parts that read the DC analysis are picked after it (ADR 0076, amendment 7). Every part
//! and number here is invented, except the standard fuse, resistor, capacitor and Zener, whose
//! catalogs are the repository's.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"import std::parts::diode::Zener
import std::parts::passive::{Capacitor, Fuse, Resistor}
import std::sources::Rail

part Plug {
    .supply: Rail
    dc source .supply
    pins {
        .supply.pos power "V" "1"
        .supply.neg power "G" "2"
    }
}
"#;

fn solved(body: &str) -> Solution {
    let src = format!("{PARTS}\nboard B {{\n{body}\n}}\n");
    let c = check("staged.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
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

/// A fuse in front of a bus with a Zener clamp (whose current is a window that reads a column of
/// the part to be picked), a divider with a limit on its draw and a capacitor whose bias is the
/// bus. The network is open until the Zener is picked and unbounded while the divider's
/// resistors are at the hull of their catalog, and no window is written for the bus: the fuse is
/// picked first, the Zener's leakage and the divider's limit enclose what they draw, and the
/// parts that read the bus voltage are picked on it.
const BUS: &str = "    .plug: Plug { .supply.voltage &= 9V..16V; .supply.peak &= 30V; .supply.pos.net &= \"VIN\"; .supply.neg.net &= \"GND\" }
    .fuse: Fuse { .rated_current &= 3A }
    .bus: Rail {
        .pos.net &= \"BUS\"
        .neg.net &= \"GND\"
        .current &= 10mA..100mA
    }
    .plug.supply.pos ~> .fuse ~> .bus.pos
    .plug.supply.neg ~ .bus.neg
    .fuse.supply_peak == .plug.supply.peak
    .clamp: Zener
    .clamp.zener_voltage >= 20V
    .clamp.current &= 0A .. .clamp.leakage_current
    .clamp.leakage_test_voltage >= .bus.voltage
    .bus.pos ~> .clamp ~> .bus.neg
    .top: Resistor
    .bottom: Resistor
    .draw: Current
    .draw == .bus.voltage / (.top.resistance + .bottom.resistance)
    .draw <= 20uA
    .top.current <= 20uA
    .bus.pos ~> .top ~> .bottom ~> .bus.neg
    .hold: Capacitor
    .hold.capacitance >= 1uF
    .hold.effective_capacitance >= 0.5uF
    .bus.pos ~> .hold ~> .bus.neg
";

#[test]
fn parts_that_read_the_bus_are_picked_after_the_fuse_in_front_of_it() {
    let s = solved(BUS);
    assert!(messages(&s).is_empty(), "{:#?}", messages(&s));
    // The bus is the supply less the fuse's drop at the load: nothing else is written.
    let bus = range(&s, "bus.voltage").unwrap();
    assert!(
        bus.0 < 9.0 && bus.0 > 8.0 && bus.1 <= 16.0 && bus.1 > 15.0,
        "{bus:?}"
    );
    // The capacitor and the Zener were judged on it: the capacitor's bias is the bus.
    assert_eq!(range(&s, "hold.bias"), Some(bus));
    // The divider's draw is within its limit at the top of the bus.
    let draw = range(&s, "draw").unwrap();
    assert!(draw.1 <= 20e-6, "{draw:?}");
}
