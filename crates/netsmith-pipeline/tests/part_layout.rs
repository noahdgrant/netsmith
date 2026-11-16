//! The `layout` block of a part reaches the design (ADR 0030, issues #829 and #994): the
//! guidelines of the `Lmr16006y` datasheet, for the IC that `Buck<Lmr16006y>` places, with the
//! pins named as the part's pin table names them, which is how the placer finds their pads; a
//! `keepout` as a keepout the part carries, a `route` width as the least width of its net, and
//! a block for one package only on a part in that package.

use netsmith_check::Loader;
use netsmith_core::{LayoutRule, NearSelect, PartKind};
use netsmith_pipeline::outcome_with;
use netsmith_solve::Catalogs;

/// The converter of issue #744: a buck with a 5 mA to 50 mA load.
const BUCK: &str = r#"import std::parts::regulator::{Buck, Lmr16006y}
import std::sources::Rail

board Repro {
    .input: Rail {
        .voltage &= 9V..16V
        .peak &= 24V
        .pos.net &= "VIN"
    }
    .reg: Buck<Lmr16006y> {
        .vout.pos.net &= "V3V3"
        .vout.voltage &= 3.3V +/- 5%
    }
    .input ~ .reg.vin
    .load: Rail { .current &= 5mA..50mA }
    .reg.vout ~ .load
}
"#;

#[test]
fn the_design_carries_the_datasheet_guidelines_of_the_ic() {
    // No library is read: the design is what the placer is handed.
    let built = outcome_with("buck.nsm", BUCK, &Loader::std(), &Catalogs::jlcpcb(), None)
        .unwrap_or_else(|e| panic!("{e:?}"))
        .boards
        .remove(0);
    let layout = &built.design.features.layout;
    let seen: Vec<(&str, &str, &NearSelect, &str)> = layout
        .iter()
        .map(|l| {
            assert_eq!(l.source, "SNVSA24, 11.1 Layout Guidelines, p.17");
            let LayoutRule::Near { select, to, within } = &l.rule else {
                panic!("{l:?}")
            };
            assert_eq!(*within, None);
            (l.part.as_str(), l.name.as_str(), select, to.as_str())
        })
        .collect();
    let on = |pin: &str, kind| NearSelect::On {
        pin: pin.into(),
        kind: Some(kind),
    };
    assert_eq!(
        seen,
        [
            ("reg.ic", "feedback", &on("fb", PartKind::Resistor), "fb"),
            (
                "reg.ic",
                "inductor",
                &on("sw.pos", PartKind::Inductor),
                "sw.pos"
            ),
            (
                "reg.ic",
                "input_cap",
                &NearSelect::Bridging {
                    a: "vin.pos".into(),
                    b: "vin.neg".into()
                },
                "vin.pos"
            ),
        ]
    );
    // Each pin it names has pads in the IC's pin table.
    let ic = built
        .design
        .parts
        .iter()
        .find(|p| p.path == "reg.ic")
        .unwrap();
    for pin in ["fb", "sw.pos", "vin.pos", "vin.neg"] {
        assert!(
            ic.pins.iter().any(|p| p.name == pin && !p.pads.is_empty()),
            "{pin}: {:?}",
            ic.pins
        );
    }
}

/// An invented regulator with two packages, in SOT-223, with a block for every package and one
/// for each of the two.
const LD7: &str = r#"import std::sources::Rail

part Ld7 {
    .designator &= "U"
    .vin: Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    .vin.current == -.vout.current
    pins "SOT-23" "SOT-223" {
        .vin.pos  power "IN"  "3"
        .vin.neg  power "GND" "1"
        .vout.pos power "OUT" "2"
    }
    .package: Package
    .package &= "SOT-223"
    layout {
        source "LD7, 10 Layout, p.14"
        route out .vout.pos width 0.8mm length 20mm
        keepout under at -1mm, -1.5mm size 2mm, 3mm forbid vias copper
    }
    layout "SOT-223" {
        source "LD7, 10.2 SOT-223, p.15"
        vias tab .vin.neg 3
    }
    layout "SOT-23" {
        source "LD7, 10.3 SOT-23, p.15"
        edge out_edge .vout.pos within 1mm
    }
}

board B {
    .input: Rail {
        .voltage &= 5V
        .pos.net &= "VIN"
    }
    .u: Ld7
    .input ~ .u.vin
    .load: Rail { .current &= 10mA }
    .u.vout.voltage &= 3.3V
    .u.vout.pos.net &= "VOUT"
    .u.vout ~ .load
}
"#;

#[test]
fn the_other_statements_reach_the_design_as_what_uses_them() {
    let built = outcome_with("ld7.nsm", LD7, &Loader::std(), &Catalogs::jlcpcb(), None)
        .unwrap_or_else(|e| panic!("{e:?}"))
        .boards
        .remove(0);
    let features = &built.design.features;
    let rules: Vec<(&str, &LayoutRule)> = features
        .layout
        .iter()
        .map(|l| (l.name.as_str(), &l.rule))
        .collect();
    assert_eq!(
        rules,
        [
            (
                "out",
                &LayoutRule::Route {
                    pin: "vout.pos".into(),
                    width: Some(0.8),
                    length: Some(20.0),
                }
            ),
            (
                "tab",
                &LayoutRule::Vias {
                    pin: "vin.neg".into(),
                    count: 3,
                }
            ),
        ],
        "the block for SOT-23 is left out of a part in SOT-223, and a keepout is no guideline"
    );
    let keepout = features
        .keepouts
        .iter()
        .find(|k| k.name == "u.under")
        .unwrap_or_else(|| panic!("{:?}", features.keepouts));
    assert_eq!(keepout.part.as_deref(), Some("u"));
    assert_eq!(keepout.at, Some((-1.0, -1.5)));
    assert_eq!(keepout.size, (2.0, 3.0));
    assert_eq!(
        (keepout.tracks, keepout.vias, keepout.copper, keepout.parts),
        (false, true, true, false)
    );
    assert!(keepout.layers.contains(&"top".to_string()));
    let width = features
        .net_widths
        .iter()
        .find(|(n, _)| n == "VOUT")
        .map(|(_, w)| *w);
    assert!(width.is_some_and(|w| w >= 0.8 - 1e-9), "{:?}", features.net_widths);
}
