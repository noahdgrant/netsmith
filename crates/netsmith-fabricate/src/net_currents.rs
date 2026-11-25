//! The one current of each net (issue #1584), as `board nets --currents` lists it: the number the
//! design rules check holds the net's tracks (`track_current`) and its vias (`via_current`) to,
//! and the track-drop guideline measures the net at, with where it comes from. Where a rail on
//! the net states an `.rms_current` (issue #1690), the net has a second figure, its heating
//! current, and the tracks, the vias and the two checks are held to that one; the track drop
//! stays at the peak.
//!
//! The current is the design stage's (`netsmith_design::guidelines::plan_tracks`, ADR 0036): the
//! sum of the rails and parts on the net with every load on at once, or the DC analysis' worst
//! case where it solved the net, or one rail or part on the net that draws more than the
//! analysis finds (a part's stated `.current`). Copper that only feeds the pins of branches is
//! held to what those pins carry instead, never more than this (issue #1113).

use netsmith_core::CurrentFrom;
use netsmith_pcb::Board;

/// One net's current, as `board nets --currents` lists it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct NetCurrentRow {
    pub net: String,
    /// The worst case, in amperes.
    pub current: f64,
    /// Which way it was found: `sum`, `analysis` or `carrier`.
    pub from: CurrentFrom,
    /// Whether a current on the net is not known, so that `current` is a lower bound.
    pub lower_bound: bool,
    /// Where it comes from, in words: the words of the `track_current` and `via_current`
    /// findings where the net has no rms current.
    pub source: String,
    /// The heating (rms) current, in amperes, where a rail on the net states an
    /// `.rms_current` that brings it below `current` (issue #1690): what the widths, the vias
    /// and the two checks are sized by. `current` stays the peak, which the track drop reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rms: Option<f64>,
    /// Where `rms` comes from, in words.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub rms_source: String,
    /// The least width a track of the net may have away from its pads, in mm, on an outer
    /// layer and on an inner signal layer (`None` on a board with none): what `track_current`
    /// holds it to, never less than the process's least track.
    pub least_width: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inner_least_width: Option<f64>,
    /// How many of the process's vias it needs side by side where it changes layer: what
    /// `via_current` holds it to.
    pub vias: usize,
}

/// The current of each net of `board` that has one, by net name.
pub fn of_board(board: &Board) -> Vec<NetCurrentRow> {
    let least = board.rules.min_track_width;
    let mut rows: Vec<NetCurrentRow> = board
        .net_currents
        .iter()
        .map(|c| NetCurrentRow {
            net: c.net.clone(),
            current: c.current,
            from: c.from,
            lower_bound: c.lower_bound,
            source: c.source.clone(),
            rms: c.rms,
            rms_source: c.rms_source.clone(),
            least_width: c.least_width.max(least),
            inner_least_width: c.inner_least_width.map(|w| w.max(least)),
            vias: c.vias,
        })
        .collect();
    rows.sort_by(|a, b| a.net.cmp(&b.net));
    rows
}

/// The `currents` section of `board nets --currents`: a line for each net.
pub fn text(rows: &[NetCurrentRow]) -> String {
    if rows.is_empty() {
        return "currents: no net has a known current\n".to_string();
    }
    let mut out = String::from(
        "currents (the peak the track drop measures each net at, and the rms current its tracks and vias are sized for heat by where one is stated):\n",
    );
    for r in rows {
        let at_least = if r.lower_bound { "at least " } else { "" };
        let inner = r
            .inner_least_width
            .map(|w| format!(", {w:.2} mm inner"))
            .unwrap_or_default();
        let rms = r
            .rms
            .map(|x| format!(", {at_least}{} rms ({})", amps(x), r.rms_source))
            .unwrap_or_default();
        out.push_str(&format!(
            "  {}: {at_least}{} ({}: {}){rms}; tracks {:.2} mm{inner}, {} via(s) side by side\n",
            r.net,
            amps(r.current),
            from_word(r.from),
            r.source,
            r.least_width,
            r.vias,
        ));
    }
    out
}

/// The word for where a current comes from, as the JSON says it.
fn from_word(from: CurrentFrom) -> &'static str {
    match from {
        CurrentFrom::Sum => "sum",
        CurrentFrom::Analysis => "analysis",
        CurrentFrom::Carrier => "carrier",
    }
}

/// A current for a line: `500 mA`, `2.5 A`.
fn amps(x: f64) -> String {
    let text = |v: f64| {
        let t = format!("{v:.3}");
        t.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    if x < 1.0 {
        format!("{} mA", text(x * 1e3))
    } else {
        format!("{} A", text(x))
    }
}
