//! The `layout` block of a part reaches the design (ADR 0030, issue #829): the guidelines of the
//! `Lmr16006y` datasheet, for the IC that `Buck<Lmr16006y>` places, with the pins named as the
//! part's pin table names them, which is how the placer finds their pads.

use netsmith_check::Loader;
use netsmith_core::{NearSelect, PartKind};
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
            assert_eq!(l.within, None);
            (l.part.as_str(), l.name.as_str(), &l.select, l.to.as_str())
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
