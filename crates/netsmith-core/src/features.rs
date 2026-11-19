//! The current of each net and the optional features of a design.

use crate::{
    CaseModel, Cutout, Keepout, Label, Logo, Offset, Outline, PartHeight, PartHeightGrowth,
    PartKind, PartRoutingMargin, PartSilk, PartSoftMargin, PartStandoff, PinProtrusion, PinRef,
    Placement, Pour, StepModel,
};

/// The current a net carries, and the least track that carries it (issue #431).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NetCurrent {
    pub net: String,
    /// The worst case, in amperes: every load on at once (ADR 0036). A lower bound where a
    /// current on the net is not known.
    pub current: f64,
    /// The narrowest track that carries it, in mm, before the process's least track.
    pub least_width: f64,
    /// The narrowest track on an inner signal layer that carries it, in mm, before the
    /// process's least track: sized with IPC-2221's inner constant for the inner copper (issue
    /// #833). `None` when the board has no inner signal layer; such a net keeps to the outer
    /// layers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_least_width: Option<f64>,
    /// How many of the process's vias, side by side, carry it where a track changes layer.
    pub vias: usize,
    /// Where the current comes from, in words (issue 1373): the loads on the net that add up
    /// to it, as the track width check says them (`the sum of the currents on it with every
    /// load on at once: rail `a` 2 A + rail `b` 1 A`), or what the DC analysis found. Empty
    /// where a design file written before it says none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

/// A branch of a net that carries a current of its own (issue #1113): the copper from the pads
/// of a part's pin that states `.current` to the rest of its net. A bias pin, a feedback
/// divider's top or a sense tap on a rail of amps carries a few milliamps, and its copper is
/// drawn, and checked, for that and not for the net's current.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BranchCurrent {
    pub net: String,
    /// The pin's path in the design (`ldo.bias`).
    pub path: String,
    /// The pads of the pin: one for each pad, and for each part of a bank.
    pub pins: Vec<PinRef>,
    /// The most the branch carries, in amperes.
    pub current: f64,
    /// The width its tracks are drawn at on the outer layers, in mm: what its current needs,
    /// and never less than the process's least track, a rail's `.min_track` on the net or a
    /// part's `route` statement for it.
    pub width: f64,
    /// The narrowest a track of it may be, in mm, before the process's least track.
    pub least_width: f64,
    /// The same on an inner signal layer; `None` when the board has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_least_width: Option<f64>,
    /// How many of the process's vias, side by side, carry it.
    pub vias: usize,
}

/// A pin on a net with a known current whose own current the model knows and is less than the
/// net's (issue #1333): the sum of what the rails and the parts that pass a current through it
/// carry at their largest, as a feedback divider's top resistor carries microamps from a rail
/// of amps. The design rules check sizes the copper that only feeds such pins (and pins that
/// state their current, [`BranchCurrent`]) for what flows to them; the router does not read it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PinCurrent {
    pub net: String,
    /// The pin's path in the design (`reg.r_top.p1`).
    pub path: String,
    /// The pads of the pin: one for each pad, and for each part of a bank.
    pub pins: Vec<PinRef>,
    /// The most that flows through the pin, in amperes.
    pub current: f64,
}

/// The impedance a design states for the tracks of one net or of a pair of nets (issue #1007):
/// a `TrackImpedance` or a `PairImpedance` of the design (`std::pcb`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImpedanceTarget {
    /// The member's path in the design.
    pub path: String,
    /// The net of a single-ended target, or the two nets of a pair, the positive one first.
    pub nets: Vec<String>,
    /// The impedance in ohms: from the track to the reference layer, or between the pair's
    /// two tracks.
    pub ohms: f64,
    /// The space between the two tracks of a pair, edge to edge, in mm; none for a single net.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing: Option<f64>,
    /// The most a pair's two track lengths may differ by, in mm: the top of the pair's `.skew`
    /// (issue #1234); none for a single net.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skew: Option<f64>,
}

/// Two nets the design states are a diff pair with no impedance (issue #1193): the two pins of
/// a `std::sources::DiffPair`, or the two lines of a std bus whose lines are a pair
/// (`std::bus::Can`). The router routes them as it routes the nets of a `PairImpedance` and two
/// nets whose names say they are a pair (`netsmith_geom::PAIR_SUFFIXES`), with no width or
/// spacing of their own.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NetPair {
    /// The path of the member that states it (`can_pair`, `xcvr.can`): the first in the order
    /// of the paths, when several state the same pair.
    pub path: String,
    /// The positive net (a `DiffPair`'s `.p`, a bus's `canh`).
    pub p: String,
    /// The negative net.
    pub n: String,
    /// The most the two nets' track lengths may differ by, in mm: the top of the member's
    /// `.skew` (issue #1234). None when the member states none, and the router's target holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skew: Option<f64>,
}

/// Nets whose track lengths are matched to each other (issue #1234), a parallel bus or a byte
/// lane: the router lengthens the shorter nets towards the longest until no two differ by more
/// than `tolerance`, and `netsmith board nets` lists each group's lengths and spread.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LengthGroup {
    /// The path of the member that states it.
    pub path: String,
    /// The nets, in the order of the member's lines, each once.
    pub nets: Vec<String>,
    /// The most the longest and the shortest net may differ by, in mm.
    pub tolerance: f64,
}

/// What a design says about the board beyond its parts and nets (ADR 0020). Everything is
/// optional; a design that says none of it is laid out as before.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Features {
    /// The narrowest track of the nets that have a rail with a `min_track`, in mm, by net
    /// name. A net takes the largest bottom of the ranges of the rails with a pin on it, so a
    /// ground that two rails return through takes the wider (ADR 0020; the reason for a field
    /// of the rail is in `std/sources.nsm`). The nets no rail gives a width are sized from
    /// their current (ADR 0036, see `netsmith_design::guidelines`) and put here too.
    pub net_widths: Vec<(String, f64)>,
    /// The least space between copper of the nets that have a rail with a `min_clearance` and
    /// copper of any other net, in mm, by net name: the largest bottom of the ranges of the
    /// rails with a pin on it, as for [`Features::net_widths`] (issue #758). Sorted by net
    /// name. Left out of a design file when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub net_clearances: Vec<(String, f64)>,
    /// The nets a `Rail` of the design has a pin on, `pos` or `neg`, sorted by name (issue
    /// #1006): the supplies and the grounds of the design, which a chip's power pins find on a
    /// board too, and a rail joined to passive parts alone does not. Left out of a design file
    /// when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rail_nets: Vec<String>,
    /// The impedance the design states for the tracks of some nets (issue #1007), in the order
    /// of their paths. Left out of a design file when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub impedance_targets: Vec<ImpedanceTarget>,
    /// The pairs of nets the design states are diff pairs with no impedance (issue #1193), each
    /// once, in the order of their paths. Left out of a design file when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pairs: Vec<NetPair>,
    /// The groups of nets the design matches in length (issue #1234), in the order of their
    /// paths. Left out of a design file when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub length_groups: Vec<LengthGroup>,
    /// The current each net carries, where it is known, and the least width that carries it
    /// (IPC-2221 at the width tolerance, ADR 0036): what the router may not narrow a track of
    /// the net below, away from its pads, and what the design rules check holds the routed
    /// copper to (issue #431). Sorted by net name.
    pub net_currents: Vec<NetCurrent>,
    /// The branches whose pins state their current (issue #1113), by the pin's path: the
    /// router draws the line to such a pad, and its vias, for the branch's current, and the
    /// design rules check holds that copper to it. Left out of a design file when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branch_currents: Vec<BranchCurrent>,
    /// The pins whose current the model knows, below their net's (issue #1333), by the pin's
    /// path: the design rules check holds the copper that only feeds them to what they draw.
    /// Left out of a design file when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pin_currents: Vec<PinCurrent>,
    pub outline: Option<Outline>,
    pub cutouts: Vec<Cutout>,
    pub pours: Vec<Pour>,
    pub keepouts: Vec<Keepout>,
    /// The heights the design states for parts, by path (ADR 0045, "Height limits").
    pub heights: Vec<PartHeight>,
    /// How high the underside of the body of a part stands above the board, as the design
    /// states it (`.standoff`, issue #775), by path. Left out of a design file when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub standoffs: Vec<PartStandoff>,
    /// How much the bodies of parts may grow in height over their life, as the design states it
    /// (`.height_growth`, issue #1263), by path. Left out of a design file when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub height_growths: Vec<PartHeightGrowth>,
    /// The room for routing each part keeps round its courtyard (`.routing_margin`, issue
    /// #1313), its own or the board's, by path; a part with none is not listed. Left out of a
    /// design file when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routing_margins: Vec<PartRoutingMargin>,
    /// The distance other parts are asked to keep from each part (`soft margin 3mm round .u1`,
    /// issue 1310), by path; a part with none is not listed. Left out of a design file when
    /// empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub soft_margins: Vec<PartSoftMargin>,
    /// How far the pins of through-hole parts protrude, as the design states it, by path
    /// (ADR 0045 section 15).
    pub protrusions: Vec<PinProtrusion>,
    /// What the silkscreen prints for the parts that do not print their designator, by path:
    /// nothing, or a text of the physical file's (`.silk`, `.silk_text`). A part not listed
    /// prints its designator.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub silks: Vec<PartSilk>,
    /// The STEP models parts are drawn from, by path (ADR 0108). A design file holds only the
    /// ones the design states: a model the project's model table gave
    /// ([`StepModel::footprint`]) is left out of it.
    #[serde(serialize_with = "stated_models")]
    pub models: Vec<StepModel>,
    /// The STEP files of the board's case, by member name (ADR 0109).
    pub case_models: Vec<CaseModel>,
    /// The paths of the parts the board house does not assemble (`.assembly` is `hand` or
    /// `none`), so fitted by hand or not at all. Filled by the caller that knows who
    /// assembles each part; empty otherwise, which counts every part as assembled by the
    /// board house.
    pub hand_fitted: Vec<String>,
    pub labels: Vec<Label>,
    /// The vector drawings printed or etched on the board (`Logo` parts), by path. Left out of a
    /// design file when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub logos: Vec<Logo>,
    /// Requirements on where parts and modules go (ADR 0045), by path.
    pub constraints: Vec<Placement>,
    /// Relations between the places of two parts (ADR 0045, section 3).
    pub offsets: Vec<Offset>,
    /// The layout guidelines that the parts' datasheets state (`layout` blocks, ADR 0030, issues
    /// #829 and #994), one for each part of the board whose definition states one, and whose
    /// package is the one a `layout "PACKAGE"` block names. A `keepout` statement is not here:
    /// it is a keepout the part carries, in [`Features::keepouts`]. Left out of a design file
    /// when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layout: Vec<LayoutGuideline>,
    /// The text of each line of the physical file that states a requirement, by the
    /// `file:line` the `source` fields name (`widget.physical.nsm:14`), so that a message can
    /// quote the line. Filled by the caller that knows the source files; empty otherwise.
    pub line_texts: std::collections::BTreeMap<String, String>,
}

/// A guideline of a part's `layout` block (ADR 0030, issues #829 and #994), for one part of the
/// board. A guideline and not a rule: the placer and the router work towards it, and `fab` warns
/// when a number is not met (ADR 0036), unless the layout file waives it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LayoutGuideline {
    /// The path of the part that states it: `reg.ic`.
    pub part: String,
    /// The guideline's name in the part's `layout` block: `input_cap`.
    pub name: String,
    /// The document and section the block cites: `SNVSA24, 11.1 Layout Guidelines, p.17`.
    pub source: String,
    /// Where the part states it: `std/parts/regulator.nsm:1100`.
    pub site: String,
    #[serde(flatten)]
    pub rule: LayoutRule,
}

/// What a [`LayoutGuideline`] says, with the part's pins named as its pin table names them
/// (`vin.pos`) and the lengths in mm. Distances are pad centre to pad centre.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "statement", rename_all = "snake_case")]
pub enum LayoutRule {
    /// The parts that [`NearSelect`] finds go next to the pads of the pin `to`: from a pad of a
    /// selected part on the net of `to` (any of its pads, when none is on it) to a pad of `to`,
    /// within `within` when the datasheet gives a number. `None` for a datasheet that says
    /// "close to": the placer pulls, and nothing is checked.
    Near {
        select: NearSelect,
        to: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        within: Option<f64>,
    },
    /// The parts that [`NearSelect`] finds stay at least `beyond` from every pad of the pin
    /// `from`, measured from their nearest pad.
    Away {
        select: NearSelect,
        from: String,
        beyond: f64,
    },
    /// Each other part with two pads, one on the net of `a` and one on the net of `b`, closes
    /// a current loop with the part: from a pad of `a` to the part's pad on that net, across
    /// the part, from its other pad to a pad of `b`, and back from `b` to `a`. The loop's
    /// length is at most `within` when the datasheet gives a number.
    Loop {
        a: String,
        b: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        within: Option<f64>,
    },
    /// The pads of `pin` go at the board's edge: the nearest pad within `within` of the
    /// outline when the datasheet gives a number.
    Edge {
        pin: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        within: Option<f64>,
    },
    /// The pads of `pin` take at least `count` vias of their net, each with its centre on one
    /// of the pads.
    Vias { pin: String, count: usize },
    /// The tracks of the net of `pin` are at least `width` wide (the design adds it to the
    /// net's least width) and at most `length` long in all.
    Route {
        pin: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        length: Option<f64>,
    },
    /// `to` is a sense pin, a Kelvin connection (issue #1148): it senses the pad of the part
    /// that [`NearSelect`] finds on the net of `to`, and the router routes `to` from that pad,
    /// alone: no other copper of the net joins the sense track on the way. `to` is a pin of the
    /// selector.
    Kelvin { select: NearSelect, to: String },
}

impl LayoutRule {
    /// The statement's word in the language: `near`, `away`, `loop`, `edge`, `vias`, `route`,
    /// `kelvin`.
    pub fn word(&self) -> &'static str {
        match self {
            LayoutRule::Near { .. } => "near",
            LayoutRule::Away { .. } => "away",
            LayoutRule::Loop { .. } => "loop",
            LayoutRule::Edge { .. } => "edge",
            LayoutRule::Vias { .. } => "vias",
            LayoutRule::Route { .. } => "route",
            LayoutRule::Kelvin { .. } => "kelvin",
        }
    }
}

/// Which parts of the board a `near`, an `away` or a `kelvin` [`LayoutRule`] is about, found from the nets of the part's pins
/// (named as its pin table names them).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NearSelect {
    /// Every other part with two pads, one on the net of each pin.
    Bridging { a: String, b: String },
    /// Every other part with a pad on the net of the pin, of the kind (ADR 0125) when one is
    /// given.
    On {
        pin: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<PartKind>,
    },
}

/// The models of [`Features::models`] that the design states, as a design file holds them.
fn stated_models<S: serde::Serializer>(models: &[StepModel], s: S) -> Result<S::Ok, S::Error> {
    s.collect_seq(models.iter().filter(|m| m.footprint.is_none()))
}
