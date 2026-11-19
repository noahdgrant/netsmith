//! A part that gives several rails states a `dc` for each (issue 1271): a `dc source` or a
//! regulator, one to a rail. The DC analysis holds each rail, a stated `.current` of each is a
//! budget of its own, and the regulators of one part that take the same input rail draw the sum
//! of what their outputs ask through it. Every part and number is invented.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"import std::sources::Rail

/// A module that gives the board two rails from outside it, each with its own limit.
part Radio {
    .vio: Rail
    .vsim: Rail
    .vio.neg ~ .vsim.neg
    .vio.voltage &= 1.8V
    .vio.current &= -50mA..0A
    .vsim.voltage &= 3V
    .vsim.current &= -20mA..0A
    dc source .vio
    dc source .vsim
}

/// Two linear regulators in one part, on one input rail.
part DualLdo {
    .vin: Rail
    .va: Rail
    .vb: Rail
    .vin.neg ~ .va.neg
    .vin.neg ~ .vb.neg
    .va.voltage &= 3.3V
    .vb.voltage &= 1.8V
    dc linear_regulator .vin .va
    dc linear_regulator .vin .vb
}

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

fn load(name: &str, net: &str, current: &str) -> String {
    format!(
        "    .{name}: Rail {{\n        .pos.net &= \"{net}\"\n        .neg.net &= \"GND\"\n        .current &= {current}\n    }}\n"
    )
}

fn range(s: &Solution, path: &str) -> (f64, f64) {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) => (q.lo(), q.hi()),
        other => panic!("{path}: {other:?}"),
    }
}

fn near(got: (f64, f64), want: (f64, f64)) {
    let ok = |a: f64, b: f64| (a - b).abs() <= 2e-6 * b.abs() + 1e-9;
    assert!(
        ok(got.0, want.0) && ok(got.1, want.1),
        "got {got:?}, want {want:?}"
    );
}

/// The radio's two rails, each with a resistor across it: `vio_r` and `vsim_r` ohms.
fn radio(vio_r: &str, vsim_r: &str) -> String {
    format!(
        "    .radio: Radio\n    .rio: Link {{ .resistance &= {vio_r} }}\n    .rsim: Link {{ .resistance &= {vsim_r} }}\n    .radio.vio.pos ~> .rio ~> .radio.vio.neg\n    .radio.vsim.pos ~> .rsim ~> .radio.vsim.neg\n"
    )
}

#[test]
fn each_rail_a_part_gives_holds_its_net() {
    // 1.8 V across 180 ohm and 3 V across 600 ohm: 10 mA and 5 mA.
    let s = solved(&radio("180ohm", "600ohm"));
    assert_eq!(messages(&s), Vec::<String>::new());
    near(range(&s, "rio.current"), (0.01, 0.01));
    near(range(&s, "rsim.current"), (0.005, 0.005));
}

/// The SIM rail gives at most 20 mA and 3 V across 100 ohm draws 30 mA: the finding names that
/// rail, and the I/O rail, whose 10 mA is within its 50 mA, has none.
#[test]
fn each_rail_a_part_gives_has_its_own_budget() {
    let m = messages(&solved(&radio("180ohm", "100ohm")));
    assert!(
        m.iter().any(|m| m.starts_with(
            "`radio.vsim.current` is stated -20mA..0A, so the supply `radio.vsim` gives at most 20mA"
        )),
        "{m:?}"
    );
    assert!(!m.iter().any(|m| m.contains("radio.vio")), "{m:?}");
    // 1.8 V across 30 ohm is 60 mA, over the 50 mA of the I/O rail.
    let m = messages(&solved(&radio("30ohm", "600ohm")));
    assert!(
        m.iter().any(|m| m.starts_with("`radio.vio.current` is stated")),
        "{m:?}"
    );
    assert!(!m.iter().any(|m| m.contains("radio.vsim")), "{m:?}");
}

/// Two regulators of one part on one input: 100 mA at 3.3 V and 200 mA at 1.8 V are 300 mA in,
/// through the link in front of the part, and both regulators are modelled.
#[test]
fn two_regulators_of_one_part_draw_the_sum_through_their_input() {
    let body = format!(
        "    .supply: Rail {{\n        .voltage &= 5V\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }}\n    .link: Link {{ .resistance &= 0.1ohm }}\n    .ldo: DualLdo\n{}{}    .supply.pos ~> .link ~> .ldo.vin.pos\n    .supply.neg ~ .ldo.vin.neg\n    .ldo.va ~ .a\n    .ldo.vb ~ .b\n",
        load("a", "V3V3", "100mA"),
        load("b", "V1V8", "200mA")
    );
    let s = solved(&body);
    assert_eq!(messages(&s), Vec::<String>::new());
    assert_eq!(s.dc.regulators, 2, "{:?}", s.dc);
    near(range(&s, "link.current"), (0.3, 0.3));
    near(range(&s, "ldo.vin.current"), (0.3, 0.3));
    near(range(&s, "ldo.vin.voltage"), (4.97, 4.97));
}
