//! A requirement on where a part goes that a board does not meet, as data (`--format json`,
//! issue #816): the requirement, what was measured, the window it allows and how far out it is.
//! The sentence a person reads says the same; this is for a program that reads it.
//!
//! Two places make one: the report of where the placer ended
//! ([`RequirementReport::misses`](crate::RequirementReport::misses)) and the check of the board
//! file after routing ([`RequirementMiss::of_outcome`], from a measured
//! [`Outcome`](netsmith_geom::ConstraintOutcome)). The figures are in mm, or in degrees for a
//! turn, to four decimals, in the frame of the requirements: from the outline's top left
//! corner, y down.

use netsmith_geom::ConstraintOutcome;
use serde::Serialize;

/// One requirement a board does not meet.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RequirementMiss {
    /// What the design calls it: `power_in.place.left`, or `b.place.x - a.place.x` for a
    /// requirement on two parts together.
    pub id: String,
    /// The line of the physical file that states it (`main.physical.nsm:13`; several, joined
    /// by `, `, where lines state one requirement), when the design knows it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The reference designators of the parts it is about; for one on two parts, the second
    /// term first, as the id reads.
    pub parts: Vec<String>,
    /// What is measured: `x` or `y` (the centre of the body), `left`, `right`, `top`,
    /// `bottom` (an edge of the body), `rotation`, `side` (the face), or `relation_x` or
    /// `relation_y` (the distance between two parts along that axis).
    pub measure: &'static str,
    /// `mm` or `degrees`; none for a face.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<&'static str>,
    /// The value measured; none for a face, and where the part is not on the board.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measured: Option<f64>,
    /// The window it allows; an end the requirement leaves open is left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// How far out of the window the value is; none for a face, and where nothing was measured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub over: Option<f64>,
    /// The face the part is on (`top` or `bottom`) and the face the requirement allows, for
    /// `measure` `side`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_side: Option<String>,
    /// The sentence a person reads for it. Not in the JSON: the finding carries the message.
    #[serde(skip)]
    pub text: String,
}

/// A figure to four decimals, with no `-0`.
fn round(v: f64) -> f64 {
    let r = (v * 1e4).round() / 1e4;
    if r == 0.0 { 0.0 } else { r }
}

/// A figure that is there only where it is a number: none for infinity (an open end of a
/// window, a miss that cannot be measured).
pub(crate) fn finite(v: f64) -> Option<f64> {
    v.is_finite().then(|| round(v))
}

impl RequirementMiss {
    /// A requirement on one field of a part, or on two parts together, which `measure` names.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn numeric(
        id: &str,
        source: &str,
        parts: Vec<String>,
        measure: &'static str,
        unit: &'static str,
        value: Option<f64>,
        window: (f64, f64),
        over: f64,
        text: String,
    ) -> RequirementMiss {
        RequirementMiss {
            id: id.to_string(),
            source: (!source.is_empty()).then(|| source.to_string()),
            parts,
            measure,
            unit: Some(unit),
            measured: value.and_then(finite),
            min: finite(window.0),
            max: finite(window.1),
            over: finite(over),
            side: None,
            allowed_side: None,
            text,
        }
    }

    /// A requirement on the face a part is on.
    pub(crate) fn face(
        id: &str,
        source: &str,
        part: &str,
        is: &str,
        allowed: &str,
        text: String,
    ) -> RequirementMiss {
        RequirementMiss {
            id: id.to_string(),
            source: (!source.is_empty()).then(|| source.to_string()),
            parts: vec![part.to_string()],
            measure: "side",
            unit: None,
            measured: None,
            min: None,
            max: None,
            over: None,
            side: Some(is.to_string()),
            allowed_side: Some(allowed.to_string()),
            text,
        }
    }

    /// What the check of the board file measured, for a requirement it found not met.
    pub fn of_outcome(o: &ConstraintOutcome) -> RequirementMiss {
        if let Some((is, allowed)) = &o.face {
            return RequirementMiss::face(
                &o.id,
                &o.source,
                o.parts.first().map_or("", String::as_str),
                is,
                allowed,
                o.message.clone(),
            );
        }
        let unit = if o.measure == "rotation" {
            "degrees"
        } else {
            "mm"
        };
        RequirementMiss::numeric(
            &o.id,
            &o.source,
            o.parts.clone(),
            o.measure,
            unit,
            o.measured,
            o.allowed,
            o.over,
            o.message.clone(),
        )
    }
}
