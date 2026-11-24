//! The `track drop` guideline (issue #652): a power net whose routed tracks drop more than
//! [`DEFAULT_DROP_BUDGET`] of its rail's voltage at the net's worst-case current. `fab` runs it
//! once the board is routed, since the drop depends on the length the router draws, and `board
//! nets` and `board drc` measure the copper a board has with the same [`drops`] (issue #1540);
//! how the drop is measured is in `netsmith_pcb`'s `track_drop.rs`.
//!
//! **A guideline, as the track width against current is (ADR 0036, ADR 0030).** It warns: a
//! `warning:` line from `fab` and a line in `fab-notes.txt`. With `guidelines = "error"` or
//! `--strict` it is an error and `fab` stops before it writes the package. The budget is fixed
//! for now (decided by the agent under the owner's delegation, issue #652): a way for a design
//! to state its own is a language choice left to the owner. A net whose rail has no known
//! voltage, or whose current is not known, is not checked.

use netsmith_pcb::{Board, DEFAULT_DROP_BUDGET, DropPiece, NEAR_DROP_BUDGET, TrackDrop};

/// The rule's name, as a guideline's message names it: `Board: track drop guideline: ...`.
pub const RULE: &str = "track drop";

/// The drop of one power net's routed tracks against its budget (issue #1540): what `board
/// nets` lists and `board drc` notes, and what `fab`'s finding says.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct NetDrop {
    pub net: String,
    /// The rail's voltage, in volts.
    pub volts: f64,
    /// What the path drops, in volts.
    pub drop: f64,
    /// What the budget allows, in volts: [`DEFAULT_DROP_BUDGET`] of `volts`.
    pub allowed: f64,
    /// `drop` over `allowed`: more than 1 is over the budget.
    pub share: f64,
    /// Whether the drop is over the budget, the guideline's finding.
    pub over: bool,
    /// The most current a run of the path carries, in amperes.
    pub current: f64,
    /// Whether every run of the path carries that current, the net's whole current.
    pub whole_current: bool,
    /// The two pads of the path, as `J1.1`: the source and the load.
    pub from: String,
    pub to: String,
    /// The length of track on the path, in mm, its narrowest track, and the copper's thickness.
    pub length: f64,
    pub narrowest: f64,
    pub thickness: f64,
    /// The least `.min_track` that brings the path within the budget, in mm, or `None` when no
    /// width does. Left out of the JSON for a net within it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width_for: Option<f64>,
    /// The pieces of the path, the one that drops most first (issue #1610), for a net at
    /// [`NEAR_DROP_BUDGET`] of its budget or over: each track and via with its share of the drop
    /// and the width it needs ([`TrackDrop::pieces`]). Left out of the JSON when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<DropPiece>,
    #[serde(skip)]
    measured: TrackDrop,
}

/// The drop of each net of the routed `board` whose rail has a voltage in `voltages` (by net
/// name; `netsmith_design::guidelines::net_voltages`), the largest share of its budget first:
/// the one measure `fab`, `board nets` and `board drc` read (issue #1540).
pub fn drops(board: &Board, voltages: &[(String, f64)]) -> Vec<NetDrop> {
    let mut drops: Vec<NetDrop> = board
        .track_drops()
        .into_iter()
        .filter_map(|d| {
            let &(_, volts) = voltages.iter().find(|(n, _)| *n == d.net)?;
            net_drop(d, volts, DEFAULT_DROP_BUDGET)
        })
        .collect();
    drops.sort_by(|a, b| b.share.total_cmp(&a.share).then(a.net.cmp(&b.net)));
    drops
}

/// [`drops`] of the routed `board` of a design whose checked model is `model`, its rails'
/// voltages read from it: what `fab`, `board nets` and `board drc` call.
pub fn drops_of(board: &Board, model: &netsmith_check::Model) -> Vec<NetDrop> {
    drops(board, &netsmith_design::guidelines::net_voltages(model))
}

/// The drop of `d` against `budget` of `volts`, or `None` for a rail of no voltage.
pub fn net_drop(d: TrackDrop, volts: f64, budget: f64) -> Option<NetDrop> {
    if volts <= 0.0 {
        return None;
    }
    let allowed = volts * budget;
    let drop = d.drop();
    let over = drop > allowed * (1.0 + 1e-9);
    Some(NetDrop {
        net: d.net.clone(),
        volts,
        drop,
        allowed,
        share: drop / allowed,
        over,
        current: d.current,
        whole_current: d.whole_current(),
        from: d.from.to_string(),
        to: d.to.to_string(),
        length: d.length,
        narrowest: d.narrowest,
        thickness: d.thickness,
        width_for: if over { d.width_for(allowed) } else { None },
        path: if drop >= allowed * NEAR_DROP_BUDGET {
            DropPiece::worst_first(d.pieces(allowed))
        } else {
            Vec::new()
        },
        measured: d,
    })
}

impl NetDrop {
    /// The line of `board nets`' `track drop` section: the drop in mV, the share of the budget
    /// and the path.
    pub fn line(&self) -> String {
        format!(
            "{}: {} mV of {} mV ({} of the budget){}: {} to {}, {} mm of track, {} mm at its narrowest, up to {}",
            self.net,
            sig(self.drop * 1e3),
            sig(self.allowed * 1e3),
            percent(self.share),
            if self.over { ", over" } else { "" },
            self.from,
            self.to,
            number(self.length),
            number(self.narrowest),
            amps(self.current),
        )
    }

    /// The lines of the path's pieces (issue #1610), the one that drops most first: a track's
    /// place, layer, width, length, current, its drop in mV and its share of the net's, and the
    /// width it needs; a via's place. None for a net far within its budget.
    pub fn path_lines(&self) -> Vec<String> {
        self.path
            .iter()
            .map(|p| {
                let share = if self.drop > 0.0 { p.drop / self.drop } else { 0.0 };
                match p.via {
                    Some(at) => format!(
                        "#{} via at ({}, {}), {} mV",
                        p.step + 1,
                        number(at.x),
                        number(at.y),
                        sig(p.drop * 1e3)
                    ),
                    None => format!(
                        "#{} track ({}, {}) to ({}, {}) on {}, {} mm wide, {} mm long, {}: {} mV ({} of the drop){}",
                        p.step + 1,
                        number(p.from.x),
                        number(p.from.y),
                        number(p.to.x),
                        number(p.to.y),
                        p.layer.map_or("?", |l| l.name()),
                        number(p.width),
                        number(p.length),
                        amps(p.current),
                        sig(p.drop * 1e3),
                        percent(share),
                        match (p.needs, p.unreachable) {
                            (Some(w), _) => format!(", needs {} mm", number(w)),
                            (None, true) => ", no width is enough".to_string(),
                            _ => String::new(),
                        },
                    ),
                }
            })
            .collect()
    }

    /// The guideline's message when the drop is over the budget, as `fab` reports it.
    pub fn finding(&self) -> Option<String> {
        self.over
            .then(|| finding(&self.measured, self.volts, self.allowed / self.volts))
            .flatten()
    }
}

/// The text of `board nets`' `track drop` section (issue #1540): a line for each net, worst
/// first, or nothing when no net is measured.
pub fn text(drops: &[NetDrop]) -> String {
    if drops.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "track drop (the budget is {} of the rail's voltage):\n",
        percent(DEFAULT_DROP_BUDGET)
    );
    for d in drops {
        out.push_str(&format!("  {}\n", d.line()));
        for line in d.path_lines() {
            out.push_str(&format!("    {line}\n"));
        }
    }
    out
}

/// A finding for each net of the routed `board` whose tracks drop more than the budget of its
/// rail's voltage, from `voltages` (by net name; `netsmith_design::guidelines::net_voltages`).
pub fn findings(board: &Board, voltages: &[(String, f64)]) -> Vec<String> {
    drops(board, voltages)
        .iter()
        .filter_map(NetDrop::finding)
        .collect()
}

/// The message for a net whose path drops more than `budget` of `volts`, or `None`.
pub fn finding(d: &TrackDrop, volts: f64, budget: f64) -> Option<String> {
    let allowed = volts * budget;
    if volts <= 0.0 || d.drop() <= allowed * (1.0 + 1e-9) {
        return None;
    }
    let fix = match d.width_for(allowed) {
        Some(w) => format!(
            "give a rail on the net `.min_track &= {}mm..` (that path at that width drops no more), or give the net an outer pour",
            number(w)
        ),
        None => "no width brings that path within it: give the net an outer pour or a plane, or put the parts nearer".to_string(),
    };
    let current = if d.whole_current() {
        "with the whole current between the two pads with the most resistance between them"
            .to_string()
    } else {
        format!(
            "each run at what the pins past it draw, up to {}",
            amps(d.current)
        )
    };
    Some(format!(
        "net `{}` drops up to {} V on its tracks at {} ({} of its {} V; the budget is {}): from {} to {} through {} mm of track, {} mm at its narrowest, in {} mm copper, {current}; {fix}",
        d.net,
        sig(d.drop()),
        amps(d.current),
        percent(d.drop() / volts),
        sig(volts),
        percent(budget),
        d.from,
        d.to,
        number(d.length),
        number(d.narrowest),
        number(d.thickness),
    ))
}

/// A number with at most three decimals and no trailing zeros: `50`, `0.98`, `0.035`.
fn number(x: f64) -> String {
    let text = format!("{x:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A number to three significant digits, no trailing zeros: `0.0501`, `5`, `3.3`.
fn sig(x: f64) -> String {
    if x == 0.0 {
        return "0".to_string();
    }
    let decimals = (2 - x.abs().log10().floor() as i32).max(0) as usize;
    let text = format!("{x:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    }
}

/// A current to three significant digits: `500 mA`, `2 A`.
fn amps(x: f64) -> String {
    if x < 1.0 {
        format!("{} mA", sig(x * 1e3))
    } else {
        format!("{} A", sig(x))
    }
}

/// A fraction as a percentage to two significant digits at most: `1%`, `1.3%`, `0.5%`.
fn percent(x: f64) -> String {
    let p = x * 100.0;
    let text = if p >= 10.0 {
        format!("{p:.0}")
    } else {
        format!("{p:.1}")
    };
    format!(
        "{}%",
        if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.')
        } else {
            &text
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use netsmith_pcb::PadRef;

    fn drop_of(current: f64, length: f64, width: f64) -> TrackDrop {
        let thickness = 0.035;
        TrackDrop {
            net: "5V".into(),
            current,
            resistance: netsmith_pcb::COPPER_RESISTIVITY * length / (width * thickness),
            from: PadRef {
                refdes: "J1".into(),
                number: "1".into(),
            },
            to: PadRef {
                refdes: "J2".into(),
                number: "1".into(),
            },
            length,
            narrowest: width,
            runs: vec![netsmith_pcb::DropRun {
                length,
                width,
                current,
                from: netsmith_pcb::Pt::new(0.0, 0.0),
                to: netsmith_pcb::Pt::new(length, 0.0),
                track: Some(0),
                layer: Some(netsmith_pcb::CopperLayer::Front),
                drawn: width,
                via_before: None,
            }],
            thickness,
        }
    }

    #[test]
    fn a_net_over_the_budget_is_named_with_the_width_that_brings_it_within() {
        // 2 A through 100 mm of 0.98 mm in 1 oz: 0.0501 ohm, 0.1 V, 2% of 5 V.
        let d = drop_of(2.0, 100.0, 0.98);
        assert_eq!(
            finding(&d, 5.0, 0.01).unwrap(),
            "net `5V` drops up to 0.1 V on its tracks at 2 A (2% of its 5 V; the budget is 1%): from J1.1 to J2.1 through 100 mm of track, 0.98 mm at its narrowest, in 0.035 mm copper, with the whole current between the two pads with the most resistance between them; give a rail on the net `.min_track &= 1.97mm..` (that path at that width drops no more), or give the net an outer pour"
        );
    }

    #[test]
    fn a_net_within_the_budget_has_no_finding() {
        // 2 A through 50 mm of 0.98 mm: 0.05 V, 1% of 5 V and no more.
        let d = drop_of(2.0, 50.0, 0.98);
        assert!(d.drop() < 0.0502 && d.drop() > 0.05, "{}", d.drop());
        assert_eq!(finding(&d, 5.0, 0.0101), None);
        assert!(finding(&d, 5.0, 0.0099).is_some());
    }

    #[test]
    fn a_net_over_its_budget_lists_its_pieces_worst_first_in_text_and_json() {
        use netsmith_pcb::{CopperLayer, NetPad, Pt, Track, track_drop_of};
        // 30 mm at 0.4 mm, 10 mm at 0.09 mm, 30 mm at 0.4 mm, at 0.3 A on a 3 V rail: about
        // 38 mV against 30 mV (issue #1610).
        let tracks = [(0.0, 30.0, 0.4), (30.0, 40.0, 0.09), (40.0, 70.0, 0.4)].map(|(a, b, w)| {
            Track::straight("P", CopperLayer::Front, Pt::new(a, 0.0), Pt::new(b, 0.0), w)
        });
        let refs: Vec<&Track> = tracks.iter().collect();
        let pad = |r: &str, x: f64| NetPad {
            pad: PadRef {
                refdes: r.into(),
                number: "1".into(),
            },
            min: Pt::new(x - 0.5, -0.5),
            max: Pt::new(x + 0.5, 0.5),
            draws: None,
        };
        let d = track_drop_of(
            "P",
            0.3,
            &refs,
            &[],
            &[pad("J1", 0.0), pad("J2", 70.0)],
            0.035,
        )
        .unwrap();
        let n = net_drop(d, 3.0, 0.01).unwrap();
        assert!(n.over);
        let lines = n.path_lines();
        assert_eq!(lines.len(), 3);
        assert_eq!(
            lines[0],
            "#2 track (30, 0) to (40, 0) on F.Cu, 0.09 mm wide, 10 mm long, 300 mA: 16.4 mV (43% of the drop), needs 0.19 mm"
        );
        assert!(!lines[1].contains("needs"), "{}", lines[1]);
        let json = serde_json::to_value(&n).unwrap();
        let path = json["path"].as_array().unwrap();
        assert_eq!(path[0]["track"], 1);
        assert_eq!(path[0]["layer"], "F.Cu");
        assert_eq!(path[0]["needs"], 0.19);
        assert!(path[1].get("needs").is_none());
        // Far within its budget, a net lists no path.
        let d = track_drop_of(
            "P",
            0.01,
            &refs,
            &[],
            &[pad("J1", 0.0), pad("J2", 70.0)],
            0.035,
        )
        .unwrap();
        assert!(net_drop(d, 3.0, 0.01).unwrap().path.is_empty());
    }

    #[test]
    fn a_path_no_width_brings_within_says_so() {
        let d = drop_of(20.0, 5000.0, 0.2);
        assert!(
            finding(&d, 3.3, 0.01)
                .unwrap()
                .contains("no width brings that path within it")
        );
    }
}
