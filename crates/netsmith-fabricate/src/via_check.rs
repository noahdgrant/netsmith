//! `netsmith routes check-via` (issue #1643): whether a via fits at one place of the board, and
//! when it does not, what is in its way and by how much.
//!
//! **What judges it.** The design rules check that judges `routes add-via` ([`crate::routes`])
//! and `routes sites` ([`crate::via_sites`]): the check of the board near the place with the via,
//! against the same check without it, on the placed board with the routes file's copper. A pour,
//! a plane or a zone is left out, as `routes sites` leaves it: its fill is made round a via. The
//! via is the process's through via, or the one `--layers` names, with `--diameter` and
//! `--drill` as `routes add-via` takes them, of `--net` (its own pads and copper are then not in
//! its way) or of a net no pad is on.
//!
//! **The numbers.** The check's findings of the rules that ask for a space carry the space asked
//! for and the space measured ([`netsmith_geom::DrcDistance`]), the figures of their messages:
//! nothing here measures again. Each error the via adds is a rule it breaks, with the thing in
//! its way (a pad, a track, a via, a hole, the outline, a keep-out, the mask, or the via's own
//! size against the process), that thing's net, and the shortfall. The check is run with a margin
//! ([`netsmith_geom::DrcExtras::margin`], the board's largest space), so that the spaces the via
//! keeps are reported too: the smallest margins, the room to spare against each neighbour.

use netsmith_geom::{DrcFinding, DrcRule, DrcSeverity};
use netsmith_pcb::{Board, CopperLayer, Pt, Rect, Routes, Via};

use crate::routes::{Refusal, near, via_offer, with_routes};

/// The net of the probe via when no `--net` is asked for: a name no design gives a net.
const PROBE: &str = "(via check)";

/// How many margins the text lists (the JSON has them all).
pub const TEXT_MARGINS: usize = 8;

/// One rule the via breaks, or keeps with room to spare.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Clash {
    /// The rule's id (`clearance`, `hole_clearance`, `copper_edge_clearance`, ...).
    pub rule: String,
    /// What is in the way: `pad`, `track`, `via`, `hole`, `zone`, `outline`, `keep-out`,
    /// `mask`, `part` or `process` (the via's own size against the process's limits).
    pub item: String,
    /// The thing as the check names it (`SMD pad 3 [GND] of U1`), empty for the process.
    pub what: String,
    /// Its net, when it has one.
    pub net: Option<String>,
    /// The space the rule asks for and the space measured, mm, for a rule of a space.
    pub required: Option<f64>,
    pub measured: Option<f64>,
    /// `required - measured`, mm: above 0 for a rule broken; a margin's room is its negative.
    pub shortfall: Option<f64>,
    /// The check's message.
    pub message: String,
}

/// The via judged.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Probe {
    pub diameter: f64,
    pub drill: f64,
    /// The two layers of a blind or a buried via; empty for a through via.
    pub layers: Vec<String>,
}

/// What `routes check-via` found ([module](self)).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ViaCheck {
    pub board: String,
    /// In the routes file's frame (mm from the outline's top left corner, y down).
    pub at: [f64; 2],
    /// The net the via is of, when one was asked for.
    pub net: Option<String>,
    pub via: Probe,
    /// No error of the check is new with the via.
    pub legal: bool,
    /// The errors the via adds, the largest shortfall first.
    pub breaks: Vec<Clash>,
    /// The spaces the via keeps within the margin, the least room first, one for each rule and
    /// thing.
    pub margins: Vec<Clash>,
    /// The warnings and notes the via adds, as the check says them.
    pub notes: Vec<String>,
    /// The margin asked of the check, mm.
    pub margin: f64,
    /// The new islands the via would cut a pour, a plane or a zone into, on a layer it passes
    /// ([`crate::via_pockets`]): a note, which does not make the place illegal.
    pub pockets: Vec<crate::via_pockets::Pocket>,
    /// The design rules checks made.
    pub checks: usize,
    /// The fills of the box round the via made to find the pockets.
    pub fills: usize,
}

impl ViaCheck {
    /// The text for a person.
    pub fn text(&self) -> String {
        self.text_moved([0.0, 0.0], "mm from the outline's top left corner, y down")
    }

    /// [`ViaCheck::text`] with the place moved by `by`, mm, into another frame than the routes
    /// file's, which `frame` says (after `places in`).
    pub fn text_moved(&self, by: [f64; 2], frame: &str) -> String {
        let mut out = format!(
            "a {:.3} mm via ({:.3} mm drill{}){} at {:.3},{:.3} [{}]: {}\n",
            self.via.diameter,
            self.via.drill,
            match self.via.layers.as_slice() {
                [a, b] => format!(", {a} to {b}"),
                _ => String::new(),
            },
            match &self.net {
                Some(n) => format!(" of {n}"),
                None => String::new(),
            },
            self.at[0] + by[0],
            self.at[1] + by[1],
            self.board,
            if self.legal {
                "legal".to_string()
            } else {
                format!("refused, {} rule(s) broken", self.breaks.len())
            }
        );
        let line = |c: &Clash| {
            let thing = match (&c.what[..], &c.net) {
                ("", _) => c.item.clone(),
                (w, Some(n)) if !w.contains(&format!("[{n}]")) => format!("{} {w} [{n}]", c.item),
                (w, _) => format!("{} {w}", c.item),
            };
            match (c.required, c.measured, c.shortfall) {
                (Some(r), Some(m), Some(s)) if s > 0.0 => format!(
                    "  {} {thing}: needs {r:.4} mm, has {m:.4} mm, short by {s:.4} mm\n",
                    c.rule
                ),
                (Some(r), Some(m), Some(s)) => format!(
                    "  {} {thing}: needs {r:.4} mm, has {m:.4} mm, {:.4} mm to spare\n",
                    c.rule, -s
                ),
                _ => format!("  {} {thing}: {}\n", c.rule, c.message),
            }
        };
        for c in &self.breaks {
            out.push_str(&line(c));
        }
        if !self.margins.is_empty() {
            out.push_str(&format!(
                "smallest margins (within {:.3} mm of each rule):\n",
                self.margin
            ));
            for c in self.margins.iter().take(TEXT_MARGINS) {
                out.push_str(&line(c));
            }
            if self.margins.len() > TEXT_MARGINS {
                out.push_str(&format!(
                    "  and {} more (`--format json` lists them)\n",
                    self.margins.len() - TEXT_MARGINS
                ));
            }
        } else if self.legal {
            out.push_str(&format!(
                "no rule's space is within {:.3} mm of its limit\n",
                self.margin
            ));
        }
        for n in &self.notes {
            out.push_str(&format!("  also: {n}\n"));
        }
        for p in &self.pockets {
            out.push_str(&format!("  note: the via {}\n", p.text_moved(by)));
        }
        out.push_str(&format!(
            "places in {frame} ({} design rules check(s), {} fill(s))\n",
            self.checks, self.fills
        ));
        out
    }
}

/// Whether a via of `net` (or of no net of the board) fits at `at` (the routes file's frame) on
/// `board` (placed, with no routes on it) with the copper of `routes`, and what is in its way,
/// as the [module](self) says. Refused when the board has no such net or the process makes no
/// such via.
pub fn check_via(
    board: &Board,
    routes: &Routes,
    at: [f64; 2],
    net: Option<&str>,
    diameter: Option<f64>,
    drill: Option<f64>,
    layers: Option<&[CopperLayer; 2]>,
) -> Result<ViaCheck, Refusal> {
    if let Some(n) = net
        && !board.nets.iter().any(|b| b.name == n)
    {
        return Err(Refusal::Refused(format!(
            "the board {} has no net `{n}`",
            board.name
        )));
    }
    let offer = via_offer(board, layers)?;
    let probe = Via {
        net: net.unwrap_or(PROBE).to_string(),
        at: Pt::new(at[0], at[1]) + board.outline.min,
        diameter: diameter.unwrap_or(offer.diameter),
        drill: drill.unwrap_or(offer.drill),
        span: offer.span,
    };
    let (routed, _) = with_routes(board, routes)?;
    let mut pockets = crate::via_pockets::Pockets::new();
    let cut_into = pockets.of_via(&routed, &probe)?;
    let reach = near::reach(board).max(near::reach(&routed));
    let margin = reach;
    let hood = Rect::new(probe.at, probe.at).expanded(probe.diameter / 2.0 + 2.0 * reach + margin);
    let mut cut = near::local(&routed, &[], &[hood]);
    cut.zones.clear();
    if net.is_none() {
        cut.nets.push(netsmith_pcb::BoardNet {
            name: PROBE.to_string(),
            pads: Vec::new(),
        });
    }
    let project = netsmith_pcb::kicad_pro(board);
    let extras = netsmith_geom::DrcExtras {
        margin,
        ..crate::fab::drc_extras(board)
    };
    let findings = |b: &Board| -> Result<Vec<DrcFinding>, String> {
        let report = netsmith_geom::drc_check(&netsmith_pcb::kicad_pcb(b), Some(&project), &extras)
            .map_err(|e| format!("the design rules check: {e}"))?;
        Ok(report
            .findings
            .into_iter()
            .filter(|f| {
                !matches!(
                    f.rule,
                    DrcRule::UnconnectedItems | DrcRule::TrackDangling | DrcRule::ViaDangling
                )
            })
            .collect())
    };
    let before = findings(&cut)?;
    let mut with = cut.clone();
    with.vias.push(probe.clone());
    let after = findings(&with)?;
    // The findings the via adds: those of the check with it that the check without it has not.
    let mut old: Vec<String> = before.iter().map(ToString::to_string).collect();
    let mut added: Vec<DrcFinding> = Vec::new();
    for f in after {
        match old.iter().position(|o| *o == f.to_string()) {
            Some(k) => {
                old.swap_remove(k);
            }
            None => added.push(f),
        }
    }
    let is_probe = |i: &netsmith_geom::DrcItem| is_via_at(i, probe.at);
    // A margin is a note of a rule whose own findings are not notes: one the margin brought in.
    let own_note = |r: DrcRule| r.default_severity() == Some(DrcSeverity::Note);
    let mut breaks = Vec::new();
    let mut margins: Vec<Clash> = Vec::new();
    let mut notes = Vec::new();
    for f in &added {
        let named = f.items.iter().any(is_probe);
        match f.severity {
            DrcSeverity::Error => breaks.push(clash(f, probe.at)),
            DrcSeverity::Note if named && f.distance.is_some() && !own_note(f.rule) => {
                let c = clash(f, probe.at);
                if !margins
                    .iter()
                    .any(|m| m.rule == c.rule && m.what == c.what && m.shortfall >= c.shortfall)
                {
                    margins.retain(|m| !(m.rule == c.rule && m.what == c.what));
                    margins.push(c);
                }
            }
            _ => notes.push(f.with_items()),
        }
    }
    breaks.dedup();
    let by_short = |a: &Clash, b: &Clash| {
        b.shortfall
            .unwrap_or(f64::INFINITY)
            .total_cmp(&a.shortfall.unwrap_or(f64::INFINITY))
            .then(a.rule.cmp(&b.rule))
            .then(a.what.cmp(&b.what))
    };
    breaks.sort_by(by_short);
    margins.sort_by(by_short);
    Ok(ViaCheck {
        board: board.name.clone(),
        at,
        net: net.map(str::to_string),
        via: Probe {
            diameter: probe.diameter,
            drill: probe.drill,
            layers: offer
                .span
                .map(|s| vec![s.from.word().to_string(), s.to.word().to_string()])
                .unwrap_or_default(),
        },
        legal: breaks.is_empty(),
        breaks,
        margins,
        notes,
        margin: round(margin),
        pockets: cut_into,
        checks: 2,
        fills: pockets.fills,
    })
}

/// Whether `i` is the via at `at` (the board file's frame).
fn is_via_at(i: &netsmith_geom::DrcItem, at: Pt) -> bool {
    i.what.starts_with("Via") && (i.at.x - at.x).abs() < 1e-6 && (i.at.y - at.y).abs() < 1e-6
}

/// The rule a finding of the via at `probe` (the board file's frame) says it breaks, with the
/// thing in its way and the figures of the finding: what `routes check-via` and `routes sites`
/// list.
pub(crate) fn clash(f: &DrcFinding, probe: Pt) -> Clash {
    let other = f.items.iter().find(|i| !is_via_at(i, probe));
    let item = match (f.rule, other) {
        (DrcRule::ItemsNotAllowed | DrcRule::NearRuleArea, _) => "keep-out",
        (DrcRule::CopperEdgeClearance | DrcRule::HoleToEdge | DrcRule::VCutEdge, _) => "outline",
        (DrcRule::SolderMaskBridge | DrcRule::MaskToTrack, _) => "mask",
        (DrcRule::HoleToHole | DrcRule::PadHoleToHole, _) => "hole",
        (_, None) => "process",
        (_, Some(o)) => kind(&o.what),
    };
    let d = f.distance;
    Clash {
        rule: f.rule.id().to_string(),
        item: item.to_string(),
        what: other.map(|o| o.what.clone()).unwrap_or_default(),
        net: other.and_then(|o| o.net.clone()),
        required: d.map(|d| round(d.required)),
        measured: d.map(|d| round(d.measured)),
        shortfall: d.map(|d| round(d.shortfall())),
        message: f.message.clone(),
    }
}

/// The kind of a thing the check names, by the start of its name.
fn kind(what: &str) -> &'static str {
    if what.ends_with("Edge.Cuts") {
        "outline"
    } else if what.contains(" pad ") || what.starts_with("Pad") {
        "pad"
    } else if what.starts_with("Track") || what.starts_with("Arc") {
        "track"
    } else if what.starts_with("Via") {
        "via"
    } else if what.starts_with("Zone") || what.starts_with("Teardrop") {
        "zone"
    } else if what.starts_with("Footprint") {
        "part"
    } else {
        "hole"
    }
}

/// To 0.0001 mm.
fn round(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}
