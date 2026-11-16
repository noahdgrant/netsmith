//! A requirement of the design that the board file does not meet, as data for
//! `--format json` (issue #816): what the check of the board file measured
//! ([`netsmith_geom::ConstraintOutcome`]) as a [`RequirementMiss`], for each kind of
//! requirement. The figures are those of the sentence.

use netsmith_geom::{ConstraintOutcome, Pt};
use netsmith_pcb::RequirementMiss;

fn outcome(measure: &'static str, measured: Option<f64>, allowed: (f64, f64), over: f64) -> ConstraintOutcome {
    ConstraintOutcome {
        id: "j.place.left".into(),
        source: "main.physical.nsm:13".into(),
        satisfied: false,
        parts: vec!["J1".into()],
        at: Pt::default(),
        measured,
        allowed,
        over,
        margin: 0.0,
        body: "F.Fab".into(),
        message: "the sentence".into(),
        measure,
        face: None,
    }
}

fn json(o: &ConstraintOutcome) -> serde_json::Value {
    serde_json::to_value(RequirementMiss::of_outcome(o)).unwrap()
}

#[test]
fn a_field_of_a_part_has_the_value_the_window_and_the_miss() {
    assert_eq!(
        json(&outcome("left", Some(28.730000001), (1.4, 1.6), 27.13)),
        serde_json::json!({
            "id": "j.place.left",
            "source": "main.physical.nsm:13",
            "parts": ["J1"],
            "measure": "left",
            "unit": "mm",
            "measured": 28.73,
            "min": 1.4,
            "max": 1.6,
            "over": 27.13,
        })
    );
    assert_eq!(RequirementMiss::of_outcome(&outcome("left", Some(1.0), (1.4, 1.6), 0.4)).text, "the sentence");
}

#[test]
fn a_window_open_at_one_end_leaves_that_end_out() {
    let j = json(&outcome("right", Some(60.0), (f64::NEG_INFINITY, 59.0), 1.0));
    assert!(j.get("min").is_none(), "{j}");
    assert_eq!(j["max"], 59.0);
}

#[test]
fn a_turn_is_in_degrees() {
    let j = json(&outcome("rotation", Some(270.0), (0.0, 0.0), 90.0));
    assert_eq!(j["unit"], "degrees");
    assert_eq!(j["measure"], "rotation");
}

#[test]
fn a_relation_has_both_parts_and_its_axis() {
    let mut o = outcome("relation_x", Some(16.0), (17.45, 17.55), 1.45);
    o.id = "radio.row_b.place.x - radio.row_a.place.x".into();
    o.parts = vec!["J2".into(), "J1".into()];
    let j = json(&o);
    assert_eq!(j["parts"], serde_json::json!(["J2", "J1"]));
    assert_eq!(j["measure"], "relation_x");
    assert_eq!(j["over"], 1.45);
}

#[test]
fn a_face_has_both_faces_and_no_figures() {
    let mut o = outcome("side", None, (0.0, 0.0), f64::INFINITY);
    o.face = Some(("top".into(), "bottom".into()));
    assert_eq!(
        json(&o),
        serde_json::json!({
            "id": "j.place.left",
            "source": "main.physical.nsm:13",
            "parts": ["J1"],
            "measure": "side",
            "side": "top",
            "allowed_side": "bottom",
        })
    );
}

#[test]
fn a_part_that_is_not_on_the_board_has_nothing_measured() {
    let j = json(&outcome("left", None, (1.4, 1.6), f64::INFINITY));
    assert!(j.get("measured").is_none(), "{j}");
    assert!(j.get("over").is_none(), "{j}");
    assert_eq!(j["min"], 1.4);
}
