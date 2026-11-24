//! `netsmith layout move` (issue #1627): one part's entry in the layout file moved by an offset,
//! refused when the move makes a fault of the placement check `fab` runs (courtyards that
//! overlap, a part past the edge, in a keepout or a cutout), with what the move does to the
//! copper of the routes file: the track ends and vias that ended in a pad of the part and miss it
//! now, and those still on it.
//!
//! Nothing is placed. The board is made twice as the layout file has it
//! ([`Board::laid_out_entries`], as `layout check` makes it), once with the entry where it is and
//! once moved, and a fault of the second board about the part that the first does not have
//! refuses the move (a fault the part had already does not: a move out of an overlap in two
//! steps is allowed). An end is on a pad as the ratsnest joins copper to a pad
//! ([`netsmith_pcb::ratsnest_on_pad`]): its copper (half the track's width round the end, the
//! via's radius) runs into the pad's box on the track's layer.
//!
//! The places are in the layout file's frame: mm from the outline's top left corner, y down.

use std::path::Path;

use netsmith_core::Design;
use netsmith_pcb::{Board, Layout, PlacementFault, Pt, Routes};
use serde::Serialize;

use crate::layout_edit::{Finding, Severity, no_such_part, number, part_paths, read};

/// A track's end or a via of the routes file that ended in a pad of the moved part.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PadEnd {
    /// The net of the record.
    pub net: String,
    /// `track 2` or `via 1`, numbered from 1 in the net's record, as `netsmith routes` numbers
    /// them.
    pub shape: String,
    /// The track's end or the via, in the routes file's places (it does not move).
    pub at: [f64; 2],
    /// The pad it ended in before the move, `R1.1`.
    pub pad: String,
}

impl PadEnd {
    fn text_moved(&self, by: [f64; 2]) -> String {
        let what = if self.shape.starts_with("track") {
            format!("{}'s end", self.shape)
        } else {
            self.shape.clone()
        };
        format!(
            "net `{}`: {what} at {}, {}",
            self.net,
            number(self.at[0] + by[0]),
            number(self.at[1] + by[1])
        )
    }
}

/// What [`propose`] says of a move.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Moved {
    pub board: String,
    pub file: String,
    pub path: String,
    pub refdes: String,
    /// The offset asked for, `[dx, dy]` in mm (a distance: no frame moves it).
    pub by: [f64; 2],
    /// The entry's place before and after, the footprint's origin.
    pub from: [f64; 2],
    pub to: [f64; 2],
    /// Whether the move makes no fault; when it does not, nothing is written.
    pub ok: bool,
    /// The faults the move makes, as `layout check` has its `placement` findings.
    pub findings: Vec<Finding>,
    /// The routes file read, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routes: Option<String>,
    /// The ends that ended in a pad of the part and miss it after the move.
    pub missed: Vec<PadEnd>,
    /// The ends that ended in a pad of the part and still run into it.
    pub still_on: Vec<PadEnd>,
}

impl Moved {
    /// For a person, with the places moved by `by`, mm, into the frame `says` names.
    pub fn text_moved(&self, by: [f64; 2], says: &str) -> String {
        let place = |p: [f64; 2]| format!("{}, {}", number(p[0] + by[0]), number(p[1] + by[1]));
        let mut out = format!(
            "{} `{}` ({}) by {}, {} mm: from {} to {} ({})\n",
            if self.ok { "move" } else { "refused: move" },
            self.path,
            self.refdes,
            number(self.by[0]),
            number(self.by[1]),
            place(self.from),
            place(self.to),
            says,
        );
        if !self.ok {
            out.push_str(&format!(
                "the move makes {} fault(s) of the placement check, and {} is as it was:\n",
                self.findings.len(),
                self.file
            ));
            for f in &self.findings {
                out.push_str(&format!("  {}\n", f.message));
            }
        }
        match &self.routes {
            None => out.push_str("no routes file: no copper to miss its pad\n"),
            Some(file) if self.missed.is_empty() && self.still_on.is_empty() => {
                out.push_str(&format!(
                    "{file}: no track or via ends in a pad of {}\n",
                    self.refdes
                ));
            }
            Some(file) => {
                if self.missed.is_empty() {
                    out.push_str(&format!(
                        "{file}: no track or via misses its pad of {}\n",
                        self.refdes
                    ));
                } else {
                    out.push_str(&format!(
                        "{file}: the ends that miss their pad of {} after the move:\n",
                        self.refdes
                    ));
                    for end in &self.missed {
                        out.push_str(&format!("  {} misses {}\n", end.text_moved(by), end.pad));
                    }
                }
                if !self.still_on.is_empty() {
                    out.push_str("still on their pad:\n");
                    for end in &self.still_on {
                        out.push_str(&format!("  {} on {}\n", end.text_moved(by), end.pad));
                    }
                }
            }
        }
        out
    }
}

/// A fault's identity, to tell a fault the move makes from one there before: its kind and the
/// paths of its parts.
fn identity(fault: &PlacementFault) -> (&'static str, Vec<String>) {
    let mut parts: Vec<String> = fault.parts.iter().map(|p| p.part.path.clone()).collect();
    parts.sort();
    (fault.kind, parts)
}

/// The entry of the part at `path` (its path in the design or its designator) in the layout
/// file at `file`, moved by `by` (mm), as a proposal: what it makes of the placement check and
/// of the copper of the routes file `routes` (the one beside the layout file), with the file's
/// layout moved, to write when the proposal is `ok`. Refused: a part the board has no part at
/// (the nearest names), a part with no entry (there is no place to move from), an offset that
/// is not a number.
pub fn propose(
    design: &Design,
    file: &Path,
    libraries: &netsmith_pipeline::Libraries,
    routes: Option<(&Routes, String)>,
    path: &str,
    by: [f64; 2],
) -> Result<(Moved, Layout), String> {
    if !by.iter().all(|v| v.is_finite()) {
        return Err("the offset is not a number".to_string());
    }
    let part = design
        .parts
        .iter()
        .find(|p| p.path == path)
        .or_else(|| design.parts.iter().find(|p| p.refdes == path))
        .ok_or_else(|| no_such_part(design, path, &part_paths(design)))?;
    let name = file.display().to_string();
    let no_entry = || {
        format!(
            "`{}` has no entry in {name}, so there is no place to move it from: give it one with `netsmith layout set {} --at X,Y`, or write every part's with `netsmith layout capture`",
            part.path, part.path
        )
    };
    let (_, layout) = read(file)?.ok_or_else(no_entry)?;
    let old = layout
        .boards
        .get(&design.name)
        .and_then(|t| t.parts.get(&part.path))
        .ok_or_else(no_entry)?
        .clone();
    let round = |v: f64| (v * 100.0).round() / 100.0;
    let to = [round(old.at[0] + by[0]), round(old.at[1] + by[1])];
    let mut next = layout.clone();
    next.boards
        .get_mut(&design.name)
        .and_then(|t| t.parts.get_mut(&part.path))
        .expect("the entry read above")
        .at = to;

    let options = netsmith_pcb::place_options(design, Some(&layout));
    let board_of = |layout: &Layout| {
        Board::laid_out_entries(
            design,
            &libraries.mapping,
            &libraries.footprints,
            &options,
            Some(&(layout.clone(), name.clone())),
        )
    };
    let before = board_of(&layout)?;
    let after = board_of(&next)?;
    let about = |f: &PlacementFault| f.parts.iter().any(|p| p.part.path == part.path);
    let had: Vec<_> = before
        .placement_faults()
        .iter()
        .filter(|f| about(f))
        .map(identity)
        .collect();
    let lines = std::fs::read_to_string(file)
        .map(|text| Layout::part_lines(&text, &design.name))
        .unwrap_or_default();
    let findings: Vec<Finding> = after
        .placement_faults()
        .into_iter()
        .filter(|f| about(f) && !had.contains(&identity(f)))
        .map(|fault| Finding {
            severity: Severity::Error,
            kind: "placement",
            path: Some(part.path.clone()),
            line: lines.get(&part.path).copied(),
            message: fault.text.clone(),
            fault: Some(fault),
        })
        .collect();

    let (missed, still_on) = match &routes {
        Some((routes, _)) => pad_ends(&before, &after, routes, &part.refdes),
        None => (Vec::new(), Vec::new()),
    };
    Ok((
        Moved {
            board: design.name.clone(),
            file: name,
            path: part.path.clone(),
            refdes: part.refdes.clone(),
            by,
            from: old.at,
            to,
            ok: findings.is_empty(),
            findings,
            routes: routes.map(|(_, name)| name),
            missed,
            still_on,
        },
        next,
    ))
}

/// The ends of the tracks and the vias of `routes` that run into a pad of the part `refdes` on
/// `before`: those that miss it on `after`, and those still on it.
fn pad_ends(
    before: &Board,
    after: &Board,
    routes: &Routes,
    refdes: &str,
) -> (Vec<PadEnd>, Vec<PadEnd>) {
    let (mut missed, mut still_on) = (Vec::new(), Vec::new());
    let Some(table) = routes.boards.get(&before.name) else {
        return (missed, still_on);
    };
    let pads = |board: &Board, net: &str| -> Vec<netsmith_pcb::RatNode> {
        board
            .nets
            .iter()
            .find(|n| n.name == net)
            .map(|n| {
                netsmith_pcb::ratsnest_nodes_of(&board.parts, n)
                    .into_iter()
                    .filter(|node| node.pad.refdes == refdes)
                    .collect()
            })
            .unwrap_or_default()
    };
    for (net, record) in &table.nets {
        let (was, now) = (pads(before, net), pads(after, net));
        if was.is_empty() {
            continue;
        }
        let corner = before.outline.min;
        let mut ends: Vec<(String, [f64; 2], f64, Option<netsmith_pcb::CopperLayer>)> = Vec::new();
        for (k, t) in record.tracks.iter().enumerate() {
            for end in [t.from, t.to] {
                ends.push((
                    format!("track {}", k + 1),
                    end,
                    t.width / 2.0,
                    Some(t.layer),
                ));
            }
        }
        for (k, v) in record.vias.iter().enumerate() {
            ends.push((format!("via {}", k + 1), v.at, v.diameter / 2.0, None));
        }
        for (shape, at, reach, layer) in ends {
            let point = corner + Pt::new(at[0], at[1]);
            let Some(i) = was
                .iter()
                .position(|n| netsmith_pcb::ratsnest_on_pad(n, point, reach, layer))
            else {
                continue;
            };
            let end = PadEnd {
                net: net.clone(),
                shape,
                at,
                pad: was[i].pad.to_string(),
            };
            let point = after.outline.min + Pt::new(at[0], at[1]);
            let on = now
                .iter()
                .filter(|n| n.pad == was[i].pad)
                .any(|n| netsmith_pcb::ratsnest_on_pad(n, point, reach, layer));
            if on {
                still_on.push(end);
            } else {
                missed.push(end);
            }
        }
    }
    (missed, still_on)
}
