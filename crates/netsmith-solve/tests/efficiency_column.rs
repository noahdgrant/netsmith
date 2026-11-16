//! A switching regulator's efficiency can be a column of its catalog (`.efficiency optional`,
//! ADR 0078): the row that is picked gives the field its datasheet value when the design states
//! none, so the DC analysis models the converter and its input net is sized for the current it
//! draws. A row that leaves the cell empty gives the field nothing, and the converter stays
//! unmodelled, as it is for a part that states no efficiency at all. Every part and number
//! here is invented, except where a test reads the standard catalogs.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

/// A converter whose catalog has the efficiency as an optional column, and a link before it.
const PARTS: &str = r#"import std::sources::Rail

part Link {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    .rated_current: Current
    #[role(current)]
    .current: Current
    .drop: Voltage
    .drop == .current * .resistance
    .current <= .rated_current
    bridge .a ~> .b
    dc resistor .a .b .resistance
}

part Swc {
    .vin: Rail
    .vout: Rail
    .efficiency: Number
    .vin.neg ~ .vout.neg
    pins "SOT-23-5" {
        .vin.pos  passive "IN"   "1"
        .vin.neg  passive "GND"  "2"
        .vout.pos passive "OUT"  "3"
    }
    .package: Package
    catalog swc { .vout.voltage, .package, .efficiency optional }
    dc switching_regulator .vin .vout .efficiency
}
"#;

fn board(regulator: &str) -> String {
    format!(
        "{PARTS}\nboard B {{\n    .supply: Rail {{ .voltage &= 12V; .pos.net &= \"VIN\"; .neg.net &= \"GND\" }}\n    \
         .link: Link {{ .resistance &= 0.5ohm; .rated_current &= 2A }}\n    \
         .reg: Swc {{ .vout.voltage &= 5V{regulator} }}\n    \
         .out: Rail {{ .pos.net &= \"V5\"; .neg.net &= \"GND\"; .current &= 1A }}\n    \
         .supply.pos ~> .link ~> .reg.vin.pos\n    .supply.neg ~ .reg.vin.neg\n    .reg.vout ~ .out\n}}\n"
    )
}

fn solved(src: &str, file: &str) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let mut catalogs = Catalogs::empty();
    catalogs.add_file("swc", "catalogs/swc.csv", file).unwrap();
    solve(&c.boards[0], &catalogs)
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> Option<(f64, f64)> {
    match s.model.domain_at(path).unwrap_or_else(|| panic!("no field {path}")) {
        Domain::Quantity(q) => Some((q.lo(), q.hi())),
        _ => None,
    }
}

const WITH: &str = "name,package,vout.voltage,efficiency\nrated,SOT-23-5,5V,0.85..0.95\n";
const WITHOUT: &str = "name,package,vout.voltage,efficiency\nunrated,SOT-23-5,5V,\n";
const NO_COLUMN: &str = "name,package,vout.voltage\nolder,SOT-23-5,5V\n";

#[test]
fn the_picked_row_gives_the_efficiency_and_the_input_current_follows() {
    let s = solved(&board(""), WITH);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(range(&s, "reg.efficiency"), Some((0.85, 0.95)));
    assert_eq!(s.dc.regulators, 1, "{:?}", s.dc);
    // 5 W out; the input current is the largest at the least efficiency: `0.85 I (12 - 0.5 I) = 5`.
    let at = |eff: f64| {
        let mut i = 0.0;
        for _ in 0..200 {
            i = 5.0 / (eff * (12.0 - 0.5 * i));
        }
        i
    };
    let (lo, hi) = range(&s, "reg.vin.current").unwrap();
    assert!((lo - at(0.95)).abs() < 1e-5 && (hi - at(0.85)).abs() < 1e-5, "{lo} {hi}");
}

#[test]
fn a_range_the_design_states_is_narrowed_to_the_row_and_a_range_the_row_is_outside_refuses_it() {
    // The row's range lies inside the design's: the row is offered and the field is the row's.
    let s = solved(&board("; .efficiency &= 0.8..0.96"), WITH);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(range(&s, "reg.efficiency"), Some((0.85, 0.95)));
    assert_eq!(s.dc.regulators, 1, "{:?}", s.dc);
    // The design wants at least 0.9 and the row says 0.85 at the least: no row is offered.
    let s = solved(&board("; .efficiency &= 0.9..0.96"), WITH);
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(m[0].contains("the `swc` catalog has nothing for `reg`"), "{}", m[0]);
}

#[test]
fn a_row_with_an_empty_cell_leaves_the_efficiency_unsettled() {
    let s = solved(&board(""), WITHOUT);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(range(&s, "reg.efficiency"), None);
    assert_eq!(s.dc.regulators, 0, "{:?}", s.dc);
}

#[test]
fn a_file_without_the_column_is_read_and_leaves_the_efficiency_unsettled() {
    let s = solved(&board(""), NO_COLUMN);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(range(&s, "reg.efficiency"), None);
}

#[test]
fn the_standard_buck_ics_state_their_efficiency_in_their_rows() {
    // The standard parts no longer state it themselves: the picked row's range is the field.
    for (ic, want) in [
        ("Lmr16006y", (0.75, 0.90)),
        ("Rt8059", (0.75, 0.97)),
        ("Tps54202", (0.75, 0.95)),
    ] {
        let src = format!(
            "import std::parts::regulator::{{Buck, {ic}}}\nimport std::sources::Rail\n\nboard B {{\n    \
             .vin: Rail {{ .voltage &= 5V; .pos.net &= \"VIN\"; .neg.net &= \"GND\" }}\n    \
             .out: Rail {{ .pos.net &= \"V3\"; .neg.net &= \"GND\"; .current &= 0mA..300mA }}\n    \
             .reg: Buck<{ic}> {{ .vout.voltage &= 3.3V +/- 5% }}\n    \
             .vin ~ .reg.vin\n    .reg.vout ~ .out\n}}\n"
        );
        let c = check("test.nsm", &src, &Loader::std());
        assert!(c.is_ok(), "{ic}: {}", c.render());
        let s = solve(&c.boards[0], &Catalogs::jlcpcb());
        let got = range(&s, "reg.ic.efficiency");
        assert_eq!(got, Some(want), "{ic}: {:?}", messages(&s));
    }
}
