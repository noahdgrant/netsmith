//! A pour or plane of the process that a zone of the routes file replaces (`pour_replaced`,
//! issue #1612): a note, which fails nothing.
//!
//! A zone of the net of the process's pour (or plane) on its layer restates it, and the design's
//! pour is then not drawn there (issue #1314, `netsmith_pcb::Board::replaced`): the zone has only
//! the area it covers. The board file holds the zone and no pour, so the check cannot see that a
//! pour went; the caller that made the board says it ([`ReplacedPour`], in
//! [`crate::Options::replaced_pours`]). The note names the layer, the net and the zones, and a
//! `via_dangling` finding of a via of that net that spans the layer says the pour is replaced
//! there, since that is often why the via joins nothing on it.

use netsmith_geom::{Artwork, DrcFinding, DrcItem, DrcRule, DrcSeverity, Pt};

/// A pour or plane of the process, and the zones of the routes file that replace it on its layer.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplacedPour {
    /// The layer, in KiCad's name (`F.Cu`, `In1.Cu`, `B.Cu`).
    pub layer: String,
    pub net: String,
    /// `pour` (an outer layer the process pours) or `plane` (an inner layer that is one sheet).
    pub kind: &'static str,
    /// Each replacing zone's box as `X,Y,W,H` in mm from the outline's top left corner, as
    /// `routes add-zone --rect` takes it.
    pub zones: Vec<String>,
    /// A place in the first zone, in the board's frame, where the note stands.
    pub at: Pt,
}

impl ReplacedPour {
    fn zones(&self) -> String {
        format!(
            "{} {} (X,Y,W,H in mm from the outline's top left corner, as `routes add-zone --rect` takes it)",
            if self.zones.len() == 1 {
                "zone at"
            } else {
                "zones at"
            },
            self.zones.join("; ")
        )
    }

    /// The text of the note.
    pub fn note(&self) -> String {
        format!(
            "the process's {} of net {} on {} is replaced there by the routes file's {}; the {} is not drawn, and net {} has on {} only the copper of the zone, so a via or pad of the net outside it joins nothing on {}",
            self.kind,
            self.net,
            self.layer,
            self.zones(),
            self.kind,
            self.net,
            self.layer,
            self.layer,
        )
    }
}

/// The `pour_replaced` notes of `replaced`: one finding for each pour or plane, on its layer.
pub fn findings(replaced: &[ReplacedPour]) -> Vec<DrcFinding> {
    replaced
        .iter()
        .map(|r| DrcFinding {
            rule: DrcRule::PourReplaced,
            severity: DrcSeverity::Note,
            message: r.note(),
            at: r.at,
            layer: Some(r.layer.clone()),
            items: vec![DrcItem {
                what: format!("{} of {} on {}", capitalised(r.kind), r.net, r.layer),
                at: r.at,
                part: None,
                pad: None,
                net: Some(r.net.clone()),
            }],
            carrying: None,
            source: None,
            distance: None,
        })
        .collect()
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Adds to each `via_dangling` finding of a via of the net of a replaced pour, a via that spans
/// the pour's layer, that the pour is replaced there.
pub fn name_in_dangling(findings: &mut [DrcFinding], art: &Artwork, replaced: &[ReplacedPour]) {
    if replaced.is_empty() {
        return;
    }
    for f in findings
        .iter_mut()
        .filter(|f| f.rule == DrcRule::ViaDangling)
    {
        let Some(at) = f.items.first().map(|i| i.at) else {
            continue;
        };
        let Some(via) = art.vias.iter().find(|v| {
            (v.at.x - at.x).abs() < 1e-6 && (v.at.y - at.y).abs() < 1e-6 && v.net.is_some()
        }) else {
            continue;
        };
        let causes: Vec<String> = replaced
            .iter()
            .filter(|r| via.net.as_deref() == Some(r.net.as_str()))
            .filter(|r| {
                art.copper
                    .iter()
                    .position(|l| *l == r.layer)
                    .is_some_and(|i| via.on(i))
            })
            .map(|r| {
                format!(
                    "the process's {} of net {} on {} is replaced by the routes file's {}, which does not join it",
                    r.kind,
                    r.net,
                    r.layer,
                    r.zones(),
                )
            })
            .collect();
        if !causes.is_empty() {
            f.message = format!("{}; {}", f.message, causes.join("; "));
        }
    }
}
