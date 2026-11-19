//! The `within` and `route` statements of a physical file (issue #1274) in the design: the
//! parts and pins by path, the net by name, and the RF nets a stated impedance makes.

use netsmith_check::{Loader, check};
use netsmith_design::{Designed, design_board};
use netsmith_solve::{Catalogs, solve};

const SOURCE: &str = r#"import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::TrackImpedance

module Radio {
    .rf: Pin
    .ant: Header1x2
    .ant.p1 ~ .rf
}

board B {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .radio: Radio
    .j: Header1x2
    .k: Header1x2
    .j.p1 ~ .radio.rf
    .radio.rf.net &= "RF"
    .line: TrackImpedance { .impedance &= 50ohm }
    .line.line ~ .radio.rf
    .k.p1 ~ .k.p2
}
"#;

fn designed(physical: &str) -> Result<Designed, Vec<String>> {
    let loader = Loader::std().with_physical(
        "t.nsm",
        "t.physical.nsm",
        &format!("physical for B {{\n{physical}\n}}\n"),
    );
    let c = check("t.nsm", SOURCE, &loader);
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    design_board(&c.boards[0], &s)
}

#[test]
fn a_within_statement_names_the_parts_and_pins_by_path() {
    let d = designed("    .j within 8mm of .radio.ant.p1\n    .k.p2 within 3mm of .radio.ant.p2")
        .unwrap_or_else(|e| panic!("{e:#?}"));
    let found: Vec<(String, Option<String>, String, String, f64)> = d
        .design
        .features
        .pin_distances
        .iter()
        .map(|p| {
            (
                p.part.clone(),
                p.pin.clone(),
                p.to_part.clone(),
                p.to_pin.clone(),
                p.within,
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            ("j".into(), None, "radio.ant".into(), "p1".into(), 8.0),
            ("k".into(), Some("p2".into()), "radio.ant".into(), "p2".into(), 3.0),
        ]
    );
    assert_eq!(d.pin_distance_sites.len(), 2);
    assert_eq!(
        d.design.features.pin_distances[0].id(),
        "j within 8 mm of radio.ant.p1"
    );
}

#[test]
fn a_net_with_a_stated_impedance_is_an_rf_net_on_one_layer_unless_it_says_otherwise() {
    let d = designed("").unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(d.design.features.rf_nets(), ["RF"]);
    assert_eq!(d.design.features.one_layer_nets(), ["RF"]);
    let d = designed("    .radio.rf route any_layer").unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(d.design.features.rf_nets(), ["RF"]);
    assert!(d.design.features.one_layer_nets().is_empty());
    let d = designed("    .k.p1 route one_layer").unwrap_or_else(|e| panic!("{e:#?}"));
    let k = &d.design.features.net_routes[0];
    assert_eq!((k.pin.as_str(), k.one_layer), ("k.p1", true));
    assert_eq!(d.design.features.one_layer_nets().len(), 2);
    assert!(d.design.features.one_layer_nets().contains(&k.net));
}

#[test]
fn two_route_statements_that_disagree_on_a_net_are_an_error_that_names_both() {
    let e = designed("    .radio.rf route any_layer\n    .j.p1 route one_layer").unwrap_err();
    assert!(
        e.iter().any(|m| m
            == "`.j.p1 route one_layer` and `.radio.rf route any_layer` say different things of the net `RF`: keep one"),
        "{e:#?}"
    );
}
