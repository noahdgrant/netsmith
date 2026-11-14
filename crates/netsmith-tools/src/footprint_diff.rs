//! The footprints of a board `fab` wrote against those of a port's original KiCad board, part by
//! part (issue #728): whether each part of the reproduction stands, turns and is drawn as the
//! original's does.
//!
//! It is the parity checker ([`crate::parity`]) with every part held to match and the tolerances
//! at the rounding of the files (5 µm, [`EXACT`]): each part's place, rotation and face, and
//! each pad's number, centre and size in the footprint's frame, shape, angle, drill, type, copper
//! layers and net, and each hole. To that it adds the courtyard: the box the footprint's
//! courtyard drawing spans in its own frame ([`crate::reference_table::Part::courtyard`]), which
//! is what netsmith's placement check measures two parts' overlap by. Both boards are read by
//! [`crate::reference_table::read_board`], and the parts are paired by the port's parts file and
//! the member path `fab` writes on each footprint ([`crate::parity::DesignatorMap::from_parts`]).
//! A footprint of the original that the parts file names no member for (a logo) is not a part of
//! the design: it is listed apart and not compared.

use std::collections::BTreeMap;
use std::fmt;

use crate::parity::{self, DesignatorMap, Tolerances, Verdict};
use crate::port::PortParts;
use crate::reference_table::{Board, read_board};

/// How far two figures of the same footprint may be apart, in mm (and in degrees for a turn):
/// 5 µm, the rounding of the files and of the generators that wrote them (KiCad's footprint
/// generator and netsmith's put the paste pads of one QFN 3 µm apart, and a port's footprint
/// written from a board's figures may carry 4 µm of its rounding). Nothing that a board house
/// makes or a placer measures is that small.
const EXACT: f64 = 0.005;

/// What differs between the two boards' footprints.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FootprintDiff {
    /// Each original part (or hole, or `outline`) that differs, by the original's reference, with
    /// its member path when it has one and what differs, one sentence each.
    pub parts: BTreeMap<String, (Option<String>, Vec<String>)>,
    /// The original's footprints that no member of the design stands for (not compared).
    pub not_in_design: Vec<String>,
}

impl FootprintDiff {
    /// No part differs.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// The original's references of the parts that differ, in order.
    pub fn references(&self) -> Vec<&str> {
        self.parts.keys().map(String::as_str).collect()
    }
}

impl fmt::Display for FootprintDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (reference, (path, what)) in &self.parts {
            match path {
                Some(path) => writeln!(f, "{reference} ({path}):")?,
                None => writeln!(f, "{reference}:")?,
            }
            for line in what {
                writeln!(f, "    {line}")?;
            }
        }
        if !self.not_in_design.is_empty() {
            writeln!(
                f,
                "not parts of the design, not compared: {}",
                self.not_in_design.join(", ")
            )?;
        }
        Ok(())
    }
}

/// Compare the footprints of `board` (the `.kicad_pcb` `fab` wrote) with those of `original`
/// (the original's `.kicad_pcb`), the parts paired through `parts`.
pub fn compare(original: &str, board: &str, parts: &PortParts) -> Result<FootprintDiff, String> {
    let mut reference = read_board(original, "the original")?;
    let actual = read_board(board, "the board")?;
    let map = DesignatorMap::from_parts(parts, board)?;
    let mut diff = FootprintDiff::default();
    // A hole is compared as a hole (by place, drill and pad), whether or not the parts file
    // names the footprint that draws it.
    let holes: Vec<String> = reference
        .holes
        .iter()
        .map(|h| h.reference.clone())
        .collect();
    reference.parts.retain(|p| {
        let ours = parts.path_of(&p.reference).is_some();
        if !ours && !holes.contains(&p.reference) {
            diff.not_in_design.push(p.reference.clone());
        }
        ours
    });
    for part in &mut reference.parts {
        part.must_match = true;
    }
    let tolerances = Tolerances {
        outline: EXACT,
        hole: EXACT,
        hole_drill: EXACT,
        position: EXACT,
        pad_position: EXACT,
        pad_size: EXACT,
        rotation: EXACT,
    };
    let report = parity::check(&reference, &actual, &map, &tolerances);
    let mut add = |subject: &str, line: String| {
        let path = parts.path_of(subject).map(str::to_string);
        diff.parts
            .entry(subject.to_string())
            .or_insert_with(|| (path, Vec::new()))
            .1
            .push(line);
    };
    for item in report.items.iter().filter(|i| i.verdict == Verdict::Fail) {
        let what = item
            .id
            .strip_prefix(&format!("part {} ", item.subject))
            .unwrap_or(&item.id);
        let mut line = what.to_string();
        if let (Some(dx), Some(dy)) = (item.dx, item.dy)
            && dx.hypot(dy) > EXACT
        {
            line.push_str(&format!(" moved by ({dx:+.3}, {dy:+.3}) mm"));
        }
        if !item.note.is_empty() {
            line.push_str(": ");
            line.push_str(&item.note);
        }
        add(&item.subject, line);
    }
    // Each original part's courtyard against that of the board's part it is mapped to.
    for theirs in &reference.parts {
        let Some(ours) = map
            .get(&theirs.reference)
            .and_then(|to| actual.parts.iter().find(|a| a.reference == to))
        else {
            continue;
        };
        let same = match (theirs.courtyard, ours.courtyard) {
            (Some(t), Some(o)) => {
                theirs.courtyard_layer == ours.courtyard_layer
                    && t.iter().zip(&o).all(|(a, b)| (a - b).abs() <= EXACT)
            }
            (t, o) => t == o,
        };
        if !same {
            add(
                &theirs.reference,
                format!(
                    "courtyard: the board's {}, the original's {}",
                    boxed(ours.courtyard, ours.courtyard_layer.as_deref()),
                    boxed(theirs.courtyard, theirs.courtyard_layer.as_deref())
                ),
            );
        }
    }
    Ok(diff)
}

/// The pairs of parts of a board whose courtyards overlap, by reference and in order, with the
/// area they share in mm²: each courtyard the box its drawing spans
/// ([`crate::reference_table::Part::courtyard`]), turned with the part onto the board, and two
/// parts compared when their courtyards are drawn on the same layer. A part that draws no
/// courtyard is in no pair (KiCad's check passes it over too). It measures an original board by
/// the rule netsmith's design rules check measures a reproduction by, so that a test can tell
/// the overlaps the original has itself from those a footprint of netsmith's would add.
pub fn courtyard_overlaps(board: &Board) -> Vec<(String, String, f64)> {
    let boxes: Vec<(&str, &str, [f64; 4])> = board
        .parts
        .iter()
        .filter_map(|p| {
            let [x0, y0, x1, y1] = p.courtyard?;
            let layer = p.courtyard_layer.as_deref()?;
            let corners =
                [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|(x, y)| p.board_point(x, y));
            let xs = corners.map(|c| c.0);
            let ys = corners.map(|c| c.1);
            let min = |v: [f64; 4]| v.into_iter().fold(f64::INFINITY, f64::min);
            let max = |v: [f64; 4]| v.into_iter().fold(f64::NEG_INFINITY, f64::max);
            Some((
                p.reference.as_str(),
                layer,
                [min(xs), min(ys), max(xs), max(ys)],
            ))
        })
        .collect();
    let mut pairs = Vec::new();
    for (i, (a, face_a, p)) in boxes.iter().enumerate() {
        for (b, face_b, q) in &boxes[i + 1..] {
            if face_a != face_b {
                continue;
            }
            let w = p[2].min(q[2]) - p[0].max(q[0]);
            let h = p[3].min(q[3]) - p[1].max(q[1]);
            if w > EXACT && h > EXACT {
                pairs.push((a.to_string(), b.to_string(), w * h));
            }
        }
    }
    pairs
}

/// A courtyard box and its layer, for a message.
fn boxed(b: Option<[f64; 4]>, layer: Option<&str>) -> String {
    match b {
        None => "none".to_string(),
        Some([x0, y0, x1, y1]) => format!(
            "{:.3} x {:.3} at ({x0:.3}, {y0:.3})..({x1:.3}, {y1:.3}) on {}",
            x1 - x0,
            y1 - y0,
            layer.unwrap_or("?")
        ),
    }
}
