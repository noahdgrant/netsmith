//! A `TrackPath` (issue #1275) in the design: the chain of nets through the parts in series,
//! the target each net takes, the budget, the corner and the fence, and each fault's message.

use netsmith_check::{Loader, check};
use netsmith_core::TrackCorner;
use netsmith_design::{Designed, design_board};
use netsmith_solve::{Catalogs, solve};

/// A radio's pin, a DC block, a 0 ohm link and a connector, with a shunt capacitor to ground
/// on the middle net, and the path's statements `path`.
fn designed(path: &str) -> Result<Designed, Vec<String>> {
    let source = format!(
        r#"import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::passive::{{Capacitor, Resistor}}
import std::sources::{{Rail, TrackCorner, TrackImpedance, TrackPath}}

board B {{
    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}
    .radio: Header1x2
    .j: Header1x2
    .supply: Rail {{ .voltage &= 3.3V }}
    .supply.neg ~ .radio.p2
    .supply.pos ~ .j.p2
    .block: Capacitor {{ .capacitance &= 100pF +/- 5% }}
    .link: Resistor {{ .resistance &= 0ohm }}
    .shunt: Capacitor {{ .capacitance &= 1pF +/- 5% }}
    .radio.p1 ~ .block.a
    .block.b ~ .link.a
    .link.b ~ .j.p1
    .shunt.a ~ .block.b
    .shunt.b ~ .radio.p2
    .radio.p1.net &= "ANT"
    .block.b.net &= "MID"
    .j.p1.net &= "FEED"
{path}
}}
"#
    );
    let c = check("t.nsm", &source, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    design_board(&c.boards[0], &s)
}

const PATH: &str = "    .rf: TrackPath { .impedance &= 50ohm }\n    .rf.first ~ .radio.p1\n    .rf.last ~ .j.p1";

#[test]
fn each_net_of_the_chain_takes_the_target_and_the_shunt_to_ground_is_not_in_it() {
    let d = designed(PATH).unwrap_or_else(|e| panic!("{e:#?}"));
    let f = &d.design.features;
    let p = &f.track_paths[0];
    assert_eq!(p.nets, ["ANT", "MID", "FEED"]);
    assert_eq!(p.parts, ["block", "link"]);
    assert_eq!(p.ohms, 50.0);
    assert_eq!((p.budget_length, p.budget_loss, p.fence), (None, None, None));
    assert_eq!(p.corner, TrackCorner::Rounded);
    let mut targets: Vec<(&str, f64)> = f
        .impedance_targets
        .iter()
        .map(|t| (t.nets[0].as_str(), t.ohms))
        .collect();
    targets.sort_by(|a, b| a.0.cmp(b.0));
    assert_eq!(targets, [("ANT", 50.0), ("FEED", 50.0), ("MID", 50.0)]);
    assert_eq!(f.rf_nets(), ["ANT", "FEED", "MID"]);
}

#[test]
fn the_budget_the_corner_and_the_fence_are_read() {
    let d = designed(&format!(
        "{PATH}\n    .rf.budget_length &= 0mm..25mm\n    .rf.budget_loss &= 0..0.5\n    .rf.corner &= TrackCorner::Mitered\n    .rf.fence &= 1.5mm"
    ))
    .unwrap_or_else(|e| panic!("{e:#?}"));
    let p = &d.design.features.track_paths[0];
    assert_eq!(p.budget_length, Some(25.0));
    assert_eq!(p.budget_loss, Some(0.5));
    assert_eq!(p.corner, TrackCorner::Mitered);
    assert_eq!(p.fence, Some(1.5));
}

#[test]
fn a_path_whose_ends_no_chain_joins_is_refused() {
    let e = designed("    .k: Header1x2\n    .rf: TrackPath { .impedance &= 50ohm }\n    .rf.first ~ .radio.p1\n    .rf.last ~ .k.p1")
        .err()
        .unwrap();
    assert!(
        e.iter().any(|m| m.contains("`rf` runs from the net `ANT`")
            && m.contains("no chain of parts in series")),
        "{e:#?}"
    );
    let e = designed("    .rf: TrackPath { .impedance &= 50ohm }\n    .rf.first ~ .radio.p1\n    .rf.last ~ .j.p2")
        .err()
        .unwrap();
    assert!(
        e.iter().any(|m| m.contains("`rf.last` is on `") && m.contains("a rail's net")),
        "{e:#?}"
    );
}

#[test]
fn a_path_with_two_chains_of_one_length_is_refused() {
    let e = designed(&format!(
        "{PATH}\n    .twin: Resistor {{ .resistance &= 0ohm }}\n    .twin.a ~ .block.b\n    .twin.b ~ .j.p1"
    ))
    .err()
    .unwrap();
    assert!(
        e.iter()
            .any(|m| m.contains("by 2 chains of parts in series") && m.contains("`twin`")),
        "{e:#?}"
    );
}

#[test]
fn a_net_of_a_path_another_target_states_at_another_impedance_is_refused() {
    let e = designed(&format!(
        "{PATH}\n    .other: TrackImpedance {{ .impedance &= 75ohm }}\n    .other.line ~ .j.p1"
    ))
    .err()
    .unwrap();
    assert!(
        e.iter().any(|m| m.contains("the net `FEED` is on the path `rf`, of 50 ohm, and `other` states 75 ohm")),
        "{e:#?}"
    );
}

#[test]
fn an_unstated_impedance_and_a_budget_with_no_top_are_refused() {
    let e = designed(
        "    .rf: TrackPath { .budget_length &= 1mm.. }\n    .rf.first ~ .radio.p1\n    .rf.last ~ .j.p1",
    )
    .err()
    .unwrap();
    assert!(
        e.iter().any(|m| m.contains("`rf.impedance` is not stated")),
        "{e:#?}"
    );
    assert!(
        e.iter().any(|m| m.contains("`rf.budget_length` is") && m.contains("give it a top above 0")),
        "{e:#?}"
    );
}
