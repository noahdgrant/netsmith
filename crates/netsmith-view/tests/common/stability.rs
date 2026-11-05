//! How much a drawing moves when the design changes a little. A helper for tests that
//! report, not assert: apply an [`Edit`] to a design, draw both, and compare what stayed.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use netsmith_core::{Design, Part, PinRef};
use netsmith_view::layout::Layout;
use netsmith_view::{Group, Rails};

/// A small change to a board.
#[derive(Debug, Clone)]
pub enum Edit {
    /// Give a net another name.
    RenameNet { from: String, to: String },
    /// Add a capacitor `refdes` between the nets `supply` and `ground`, in the module of
    /// the part `beside`.
    AddDecouplingCap {
        refdes: String,
        beside: String,
        supply: String,
        ground: String,
    },
    /// Take a part out, and its pins from their nets.
    DeletePart(String),
}

impl Edit {
    pub fn label(&self) -> String {
        match self {
            Edit::RenameNet { from, to } => format!("rename net {from} to {to}"),
            Edit::AddDecouplingCap {
                refdes,
                beside,
                supply,
                ground,
            } => format!("add {refdes} between {supply} and {ground} beside {beside}"),
            Edit::DeletePart(r) => format!("delete {r}"),
        }
    }
}

/// The board after the edit, and the part the edit added or took out (which no comparison
/// counts) and the net renames to apply to the old board's names.
pub struct Edited {
    pub design: Design,
    pub groups: Vec<Group>,
    pub rails: Rails,
    pub touched: Vec<String>,
    pub renames: BTreeMap<String, String>,
}

pub fn apply(design: &Design, groups: &[Group], rails: &Rails, edit: &Edit) -> Edited {
    let mut design = design.clone();
    let mut groups = groups.to_vec();
    let mut rails = rails.clone();
    let mut touched = Vec::new();
    let mut renames = BTreeMap::new();
    match edit {
        Edit::RenameNet { from, to } => {
            design
                .nets
                .iter_mut()
                .find(|n| &n.name == from)
                .unwrap_or_else(|| panic!("no net {from}"))
                .name = to.clone();
            for set in [&mut rails.ground, &mut rails.positive] {
                if set.remove(from) {
                    set.insert(to.clone());
                }
            }
            renames.insert(from.clone(), to.clone());
        }
        Edit::AddDecouplingCap {
            refdes,
            beside,
            supply,
            ground,
        } => {
            let at = design.parts.len();
            let neighbour = design
                .parts
                .iter()
                .position(|p| &p.refdes == beside)
                .unwrap_or_else(|| panic!("no part {beside}"));
            design.parts.push(Part {
                refdes: refdes.clone(),
                kind: "Capacitor".into(),
                value: "100n".into(),
                package: Some("0402".into()),
                path: String::new(),
                pins: Vec::new(),
            });
            for (name, pad) in [(supply, "1"), (ground, "2")] {
                design
                    .nets
                    .iter_mut()
                    .find(|n| &n.name == name)
                    .unwrap_or_else(|| panic!("no net {name}"))
                    .pins
                    .push(PinRef {
                        refdes: refdes.clone(),
                        pin: pad.into(),
                    });
            }
            if let Some(g) = groups.iter_mut().find(|g| g.parts.contains(&neighbour)) {
                g.parts.push(at);
            }
            touched.push(refdes.clone());
        }
        Edit::DeletePart(refdes) => {
            let gone = design
                .parts
                .iter()
                .position(|p| &p.refdes == refdes)
                .unwrap_or_else(|| panic!("no part {refdes}"));
            design.parts.remove(gone);
            for net in &mut design.nets {
                net.pins.retain(|p| &p.refdes != refdes);
            }
            design.nets.retain(|n| !n.pins.is_empty());
            for g in &mut groups {
                g.parts.retain(|&p| p != gone);
                for p in &mut g.parts {
                    if *p > gone {
                        *p -= 1;
                    }
                }
            }
            groups.retain(|g| !g.parts.is_empty());
            touched.push(refdes.clone());
        }
    }
    Edited {
        design,
        groups,
        rails,
        touched,
        renames,
    }
}

/// What stayed. Lengths are in grid steps of 1.27 mm.
#[derive(Debug, Clone, PartialEq)]
pub struct Stability {
    pub segments_before: usize,
    /// Wire segments of the old drawing that are, point for point, in the new one.
    pub segments_kept: usize,
    /// The old drawing's wire length (grid steps) that lies on the new drawing's wires of
    /// the same net.
    pub length_before: usize,
    pub length_kept: usize,
    /// The same two, once the new drawing is moved back by the shift most parts share (the
    /// drawing may grow to the left or up).
    pub length_kept_aligned: usize,
    pub parts_compared: usize,
    pub parts_moved: usize,
    pub parts_turned: usize,
    /// The greatest distance, in mm, a part in both drawings moved; the same once the
    /// shared shift is taken out.
    pub max_displacement: f64,
    pub max_displacement_aligned: f64,
    /// The shift most parts share, in mm.
    pub shift: (f64, f64),
}

impl Stability {
    pub fn segments_fraction(&self) -> f64 {
        frac(self.segments_kept, self.segments_before)
    }

    pub fn length_fraction(&self) -> f64 {
        frac(self.length_kept, self.length_before)
    }

    pub fn length_fraction_aligned(&self) -> f64 {
        frac(self.length_kept_aligned, self.length_before)
    }

    /// One line for a report.
    pub fn line(&self, label: &str) -> String {
        format!(
            "{label}: segments kept {}/{} ({:.0}%), wire kept {:.0}% (aligned {:.0}%), parts moved {}/{} (turned {}), max move {:.1} mm (aligned {:.1} mm, shift {:.1},{:.1})",
            self.segments_kept,
            self.segments_before,
            100.0 * self.segments_fraction(),
            100.0 * self.length_fraction(),
            100.0 * self.length_fraction_aligned(),
            self.parts_moved,
            self.parts_compared,
            self.parts_turned,
            self.max_displacement,
            self.max_displacement_aligned,
            self.shift.0,
            self.shift.1
        )
    }
}

fn frac(a: usize, b: usize) -> f64 {
    if b == 0 { 1.0 } else { a as f64 / b as f64 }
}

const GRID: f64 = 1.27;

type Edge = (String, i64, i64, i64, i64);

fn cell(v: f64) -> i64 {
    (v / GRID).round() as i64
}

/// The wire of a drawing as unit edges between grid points, by net name.
fn edges(
    l: &Layout,
    d: &Design,
    rename: &BTreeMap<String, String>,
    shift: (f64, f64),
) -> BTreeSet<Edge> {
    let mut out = BTreeSet::new();
    for w in &l.wires {
        let name = &d.nets[w.net].name;
        let name = rename.get(name).unwrap_or(name).clone();
        for s in w.points.windows(2) {
            let (a, b) = (
                (cell(s[0].x - shift.0), cell(s[0].y - shift.1)),
                (cell(s[1].x - shift.0), cell(s[1].y - shift.1)),
            );
            let (dx, dy) = ((b.0 - a.0).signum(), (b.1 - a.1).signum());
            let mut p = a;
            while p != b && (dx == 0 || dy == 0) {
                let q = (p.0 + dx, p.1 + dy);
                let (lo, hi) = if p <= q { (p, q) } else { (q, p) };
                out.insert((name.clone(), lo.0, lo.1, hi.0, hi.1));
                p = q;
            }
        }
    }
    out
}

fn segments(l: &Layout, d: &Design, rename: &BTreeMap<String, String>) -> BTreeSet<Edge> {
    let mut out = BTreeSet::new();
    for w in &l.wires {
        let name = &d.nets[w.net].name;
        let name = rename.get(name).unwrap_or(name).clone();
        for s in w.points.windows(2) {
            let (a, b) = ((cell(s[0].x), cell(s[0].y)), (cell(s[1].x), cell(s[1].y)));
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            out.insert((name.clone(), lo.0, lo.1, hi.0, hi.1));
        }
    }
    out
}

/// Compares the drawing of a board with the drawing after an edit. `touched` are the
/// refdes the edit added or took out; `renames` maps old net names to new ones.
pub fn compare(
    before: (&Layout, &Design),
    after: (&Layout, &Design),
    touched: &[String],
    renames: &BTreeMap<String, String>,
) -> Stability {
    let (bl, bd) = before;
    let (al, ad) = after;
    let none = BTreeMap::new();

    // Parts in both drawings, by refdes.
    let mut common: Vec<(usize, usize)> = Vec::new();
    for (i, p) in bd.parts.iter().enumerate() {
        if touched.contains(&p.refdes) {
            continue;
        }
        if let Some(j) = ad.parts.iter().position(|q| q.refdes == p.refdes) {
            common.push((i, j));
        }
    }
    let moved: Vec<(f64, f64)> = common
        .iter()
        .map(|&(i, j)| {
            (
                al.parts[j].at.x - bl.parts[i].at.x,
                al.parts[j].at.y - bl.parts[i].at.y,
            )
        })
        .collect();
    let mut votes: BTreeMap<(i64, i64), usize> = BTreeMap::new();
    for m in &moved {
        *votes
            .entry(((m.0 * 100.0).round() as i64, (m.1 * 100.0).round() as i64))
            .or_default() += 1;
    }
    let shift = votes
        .iter()
        .max_by_key(|&(k, n)| (*n, std::cmp::Reverse(*k)))
        .map_or((0.0, 0.0), |(k, _)| {
            (k.0 as f64 / 100.0, k.1 as f64 / 100.0)
        });
    let dist = |m: &(f64, f64)| (m.0 * m.0 + m.1 * m.1).sqrt();

    // The old drawing under the new names; the new drawing as it is, and moved back.
    let seg_before = segments(bl, bd, renames);
    let seg_after = segments(al, ad, &none);
    let edges_before = edges(bl, bd, renames, (0.0, 0.0));
    let edges_after = edges(al, ad, &none, (0.0, 0.0));
    let edges_aligned = edges(al, ad, &none, shift);
    Stability {
        segments_before: seg_before.len(),
        segments_kept: seg_before.intersection(&seg_after).count(),
        length_before: edges_before.len(),
        length_kept: edges_before.intersection(&edges_after).count(),
        length_kept_aligned: edges_before.intersection(&edges_aligned).count(),
        parts_compared: common.len(),
        parts_moved: moved.iter().filter(|m| dist(m) > 1e-6).count(),
        parts_turned: common
            .iter()
            .filter(|&&(i, j)| bl.parts[i].orient != al.parts[j].orient)
            .count(),
        max_displacement: moved.iter().map(dist).fold(0.0, f64::max),
        max_displacement_aligned: moved
            .iter()
            .map(|m| dist(&(m.0 - shift.0, m.1 - shift.1)))
            .fold(0.0, f64::max),
        shift,
    }
}
