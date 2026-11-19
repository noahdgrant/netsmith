//! A distance between kinds of parts that a placed board does not keep (issue 1277):
//! [`Board::kept_apart_broken`].
//!
//! `keep 10mm between BuckIc and Crystal` in a board's physical file asks every part of the one
//! kind to stand at least 10 mm from every part of the other, courtyard to courtyard, in the
//! plane of the board whatever faces they are on. It is a requirement on where parts go: the
//! placer keeps it as hard as an overlap ([`PartApart`] on each part), and `fab` and
//! `layout check` stop where the parts are nearer. The distance between two courtyards is the
//! least between any of their shapes: along one axis where the shapes face each other, and
//! corner to corner otherwise; courtyards that overlap are 0 apart.

use std::fmt;

use serde::Serialize;

use crate::board::{Board, Placed, member};
use crate::miss::RequirementMiss;

/// One other part a part keeps a distance from, as a `keep` statement asks (issue 1277).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PartApart {
    /// The other part's path.
    pub path: String,
    /// In mm, the least distance between the two courtyards.
    pub distance: f64,
    /// The statement (`keep 10mm between BuckIc and Crystal`).
    pub rule: String,
    /// Its line (`widget.physical.nsm:14`); empty when it is not known.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

/// The other parts the part at `path` keeps a distance from, by the `keep` statements of
/// `features`: the largest distance where two statements reach the same pair.
pub(crate) fn apart_of(features: &netsmith_core::Features, path: &str) -> Vec<PartApart> {
    let mut found: Vec<PartApart> = Vec::new();
    for rule in &features.keep_apart {
        for (mine, theirs) in [(0, 1), (1, 0)] {
            if !rule.parts[mine].iter().any(|p| p == path) {
                continue;
            }
            for other in &rule.parts[theirs] {
                if other == path {
                    continue;
                }
                let wanted = PartApart {
                    path: other.clone(),
                    distance: rule.distance,
                    rule: rule.rule.clone(),
                    source: rule.source.clone(),
                };
                match found.iter_mut().find(|a| a.path == *other) {
                    Some(a) if a.distance < rule.distance => *a = wanted,
                    Some(_) => {}
                    None => found.push(wanted),
                }
            }
        }
    }
    found
}

impl Placed {
    /// The distance between this part's courtyard and `other`'s in the plane of the board, in
    /// mm, whatever faces they are on: the least between any two of their shapes, 0 where they
    /// overlap or touch.
    pub fn courtyard_distance(&self, other: &Placed) -> f64 {
        let mut least = f64::INFINITY;
        for a in self.courtyards() {
            for b in other.courtyards() {
                let dx = (a.min.x - b.max.x).max(b.min.x - a.max.x).max(0.0);
                let dy = (a.min.y - b.max.y).max(b.min.y - a.max.y).max(0.0);
                least = least.min((dx * dx + dy * dy).sqrt());
            }
        }
        least
    }
}

/// Two parts nearer each other than a `keep` statement asks.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ApartBroken {
    pub refdes: String,
    pub path: String,
    pub other: String,
    pub other_path: String,
    /// From courtyard to courtyard, and the distance asked, in mm.
    pub gap: f64,
    pub distance: f64,
    /// The statement and its line (empty when it is not known).
    pub rule: String,
    pub source: String,
}

impl fmt::Display for ApartBroken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "requirement `{}`", self.rule)?;
        if !self.source.is_empty() {
            write!(f, " ({})", self.source)?;
        }
        write!(
            f,
            " is violated: {} is {} mm from {}, courtyard to courtyard, and it asks for {} mm ({} mm short); move them apart, or change the rule",
            member(&self.path, &self.refdes),
            crate::mm_text(self.gap),
            member(&self.other_path, &self.other),
            crate::mm_text(self.distance),
            crate::mm_text(self.distance - self.gap),
        )
    }
}

impl ApartBroken {
    /// The same as data, for `--format json` (`measure` `distance`, the least distance as `min`).
    pub fn miss(&self) -> RequirementMiss {
        RequirementMiss::numeric(
            &self.rule,
            &self.source,
            vec![self.refdes.clone(), self.other.clone()],
            "distance",
            "mm",
            Some(self.gap),
            (self.distance, f64::INFINITY),
            self.distance - self.gap,
            self.to_string(),
        )
    }
}

impl Board {
    /// The pairs of parts nearer each other than a `keep` statement asks, where the parts are
    /// now, each pair once, in the order of the parts. Reads the placement only.
    pub fn kept_apart_broken(&self) -> Vec<ApartBroken> {
        let mut found = Vec::new();
        for (i, part) in self.parts.iter().enumerate() {
            for apart in &part.apart {
                let Some((j, other)) = self
                    .parts
                    .iter()
                    .enumerate()
                    .find(|(_, p)| p.path == apart.path)
                else {
                    continue;
                };
                if j <= i {
                    continue;
                }
                let gap = part.courtyard_distance(other);
                if gap < apart.distance - 1e-6 {
                    found.push(ApartBroken {
                        refdes: part.refdes.clone(),
                        path: part.path.clone(),
                        other: other.refdes.clone(),
                        other_path: other.path.clone(),
                        gap,
                        distance: apart.distance,
                        rule: apart.rule.clone(),
                        source: apart.source.clone(),
                    });
                }
            }
        }
        found
    }
}
