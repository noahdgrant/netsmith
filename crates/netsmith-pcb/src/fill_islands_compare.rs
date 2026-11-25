//! The islands of a board's fills compared with an earlier state (issue #1705): the regions that
//! are new, gone or changed, so that the run of copper that pocketed a plane is found without
//! reading the whole of two [`IslandsReport`]s. `netsmith board fill --islands --compare FILE`
//! prints it.
//!
//! Regions are matched within one fill, the same layer, net and source (a pour, plane or zone):
//! each pair of a region before and one after whose boxes overlap, the largest overlap first,
//! each region in one pair at most. A region left with no pair is new (after only) or gone
//! (before only); a fill that only one state has is all new or all gone. A matched pair is
//! changed when its area moves by more than [`AREA`], it joins other pads or vias, it moves in or
//! out of the net's main region, or other tracks, vias, pads or rule areas bound it. Places are
//! compared to the micrometre. The nets of the boundary items that are new (those of a new region,
//! and those new to a changed one) are listed, the run that closed a pocket among them.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::fill_islands::{bound_text, joins};
use crate::fill_report::{FillPolygon, FillSource, pad_name};
use crate::geometry::{Pt, Rect};
use crate::{FillRegion, IslandsReport, RegionBound, RegionPad, RegionVia};

/// How much a matched region's area may move, in mm², and be the same: what the fill's
/// simplification and a JSON's rounding move it by is far less.
const AREA: f64 = 0.01;

/// A region that only one state has, with its fill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionOfFill {
    pub layer: String,
    pub net: String,
    pub source: FillSource,
    pub at: Pt,
    pub bounds: Rect,
    pub area: f64,
    pub pads: Vec<RegionPad>,
    pub vias: Vec<RegionVia>,
    pub main: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bounded_by: Vec<RegionBound>,
}

/// A region both states have that is not the same, its place and box the later state's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionChanged {
    pub layer: String,
    pub net: String,
    pub source: FillSource,
    pub at: Pt,
    pub bounds: Rect,
    pub area_before: f64,
    pub area_after: f64,
    pub main_before: bool,
    pub main_after: bool,
    /// The pads and vias it joins now and did not, and those it joined and does not.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pads_added: Vec<RegionPad>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pads_removed: Vec<RegionPad>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vias_added: Vec<RegionVia>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vias_removed: Vec<RegionVia>,
    /// What bounds it now and did not, and what bounded it and does not.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bounds_added: Vec<RegionBound>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bounds_removed: Vec<RegionBound>,
}

/// The regions of two states of a board's fills compared ([module](self)).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IslandsComparison {
    pub board: String,
    /// What the earlier state is (the file it came from).
    pub against: String,
    /// The outline's top left corner in the frame of the board file.
    pub outline_at: Pt,
    pub new: Vec<RegionOfFill>,
    pub gone: Vec<RegionOfFill>,
    pub changed: Vec<RegionChanged>,
    /// The nets of the boundary items that are new, by name.
    pub nets: Vec<String>,
    /// How many regions are the same in both states.
    pub unchanged: u32,
    /// The regions cut off from their net's main region, before and after.
    pub cut_off_before: u32,
    pub cut_off_after: u32,
}

/// A place to the micrometre.
fn um(p: Pt) -> (i64, i64) {
    ((p.x * 1e3).round() as i64, (p.y * 1e3).round() as i64)
}

fn pad_key(p: &RegionPad) -> (String, String) {
    (p.part.clone(), p.pad.clone())
}

fn bound_key(b: &RegionBound) -> String {
    format!(
        "{:?} {:?} {:?} {:?} {:?} {:?} {:?}",
        b.kind,
        b.net,
        um(b.at),
        b.to.map(um),
        b.part,
        b.pad,
        b.name
    )
}

/// The items of `a` whose key `b` lacks.
fn less<T: Clone, K: Ord>(a: &[T], b: &[T], key: impl Fn(&T) -> K) -> Vec<T> {
    let theirs: BTreeSet<K> = b.iter().map(&key).collect();
    a.iter()
        .filter(|x| !theirs.contains(&key(x)))
        .cloned()
        .collect()
}

/// A fill: its layer, net and source.
type Key = (String, String, FillSource);

/// The regions of each fill of `r`, those of one fill listed twice together.
fn keyed(r: &IslandsReport) -> Vec<(Key, Vec<FillRegion>)> {
    let mut keyed: Vec<(Key, Vec<FillRegion>)> = Vec::new();
    for f in &r.fills {
        let key = (f.layer.clone(), f.net.clone(), f.source);
        match keyed.iter_mut().find(|(k, _)| *k == key) {
            Some((_, all)) => all.extend(f.regions.iter().cloned()),
            None => keyed.push((key, f.regions.clone())),
        }
    }
    keyed
}

fn regions_of(all: &[(Key, Vec<FillRegion>)], key: &Key) -> Vec<FillRegion> {
    all.iter()
        .find(|(k, _)| k == key)
        .map(|(_, r)| r.clone())
        .unwrap_or_default()
}

fn overlap(a: &Rect, b: &Rect) -> f64 {
    let w = a.max.x.min(b.max.x) - a.min.x.max(b.min.x);
    let h = a.max.y.min(b.max.y) - a.min.y.max(b.min.y);
    if w < 0.0 || h < 0.0 {
        return -1.0;
    }
    w * h
}

impl IslandsComparison {
    /// `after` compared with `before`, an earlier state of the same board, named `against`.
    pub fn of(before: &IslandsReport, after: &IslandsReport, against: &str) -> IslandsComparison {
        let mut out = IslandsComparison {
            board: after.board.clone(),
            against: against.to_string(),
            outline_at: after.outline_at,
            new: Vec::new(),
            gone: Vec::new(),
            changed: Vec::new(),
            nets: Vec::new(),
            unchanged: 0,
            cut_off_before: before.cut_off,
            cut_off_after: after.cut_off,
        };
        let (was, now) = (keyed(before), keyed(after));
        let mut keys: Vec<Key> = now.iter().map(|(k, _)| k.clone()).collect();
        for (k, _) in &was {
            if !keys.contains(k) {
                keys.push(k.clone());
            }
        }
        for key in keys {
            let (was, now) = (regions_of(&was, &key), regions_of(&now, &key));
            // Every pair whose boxes meet, the largest overlap first (then the nearer area).
            let mut pairs: Vec<(usize, usize, f64)> = Vec::new();
            for (i, a) in was.iter().enumerate() {
                for (j, b) in now.iter().enumerate() {
                    let o = overlap(&a.bounds, &b.bounds);
                    if o >= 0.0 {
                        pairs.push((i, j, o));
                    }
                }
            }
            pairs.sort_by(|x, y| {
                y.2.total_cmp(&x.2).then(
                    (was[x.0].area - now[x.1].area)
                        .abs()
                        .total_cmp(&(was[y.0].area - now[y.1].area).abs()),
                )
            });
            let (mut was_paired, mut now_paired) = (vec![false; was.len()], vec![false; now.len()]);
            let (layer, net, source) = (key.0.as_str(), key.1.as_str(), key.2);
            let alone = |r: &FillRegion| RegionOfFill {
                layer: layer.to_string(),
                net: net.to_string(),
                source,
                at: r.at,
                bounds: r.bounds,
                area: r.area,
                pads: r.pads.clone(),
                vias: r.vias.clone(),
                main: r.main,
                bounded_by: r.bounded_by.clone(),
            };
            let mut changed = Vec::new();
            for (i, j, _) in pairs {
                if was_paired[i] || now_paired[j] {
                    continue;
                }
                (was_paired[i], now_paired[j]) = (true, true);
                let (a, b) = (&was[i], &now[j]);
                let c = RegionChanged {
                    layer: layer.to_string(),
                    net: net.to_string(),
                    source,
                    at: b.at,
                    bounds: b.bounds,
                    area_before: a.area,
                    area_after: b.area,
                    main_before: a.main,
                    main_after: b.main,
                    pads_added: less(&b.pads, &a.pads, pad_key),
                    pads_removed: less(&a.pads, &b.pads, pad_key),
                    vias_added: less(&b.vias, &a.vias, |v| um(v.at)),
                    vias_removed: less(&a.vias, &b.vias, |v| um(v.at)),
                    bounds_added: less(&b.bounded_by, &a.bounded_by, bound_key),
                    bounds_removed: less(&a.bounded_by, &b.bounded_by, bound_key),
                };
                let same = (c.area_after - c.area_before).abs() <= AREA
                    && c.main_before == c.main_after
                    && c.pads_added.is_empty()
                    && c.pads_removed.is_empty()
                    && c.vias_added.is_empty()
                    && c.vias_removed.is_empty()
                    && c.bounds_added.is_empty()
                    && c.bounds_removed.is_empty();
                if same {
                    out.unchanged += 1;
                } else {
                    changed.push((j, c));
                }
            }
            // In the later state's order, the gone in the earlier's.
            changed.sort_by_key(|(j, _)| *j);
            out.changed.extend(changed.into_iter().map(|(_, c)| c));
            out.new.extend(
                now.iter()
                    .zip(&now_paired)
                    .filter(|(_, p)| !**p)
                    .map(|(r, _)| alone(r)),
            );
            out.gone.extend(
                was.iter()
                    .zip(&was_paired)
                    .filter(|(_, p)| !**p)
                    .map(|(r, _)| alone(r)),
            );
        }
        let nets: BTreeSet<String> = out
            .new
            .iter()
            .flat_map(|r| &r.bounded_by)
            .chain(out.changed.iter().flat_map(|c| &c.bounds_added))
            .filter_map(|b| b.net.clone())
            .collect();
        out.nets = nets.into_iter().collect();
        out
    }

    /// Whether the two states have the same regions.
    pub fn is_unchanged(&self) -> bool {
        self.new.is_empty() && self.gone.is_empty() && self.changed.is_empty()
    }

    /// The comparison as text for a person.
    pub fn text(&self) -> String {
        self.text_moved(Pt::new(0.0, 0.0), "board file")
    }

    /// [`IslandsComparison::text`] with its places moved by `by`, mm, and the frame they are then
    /// in named `frame`, as [`IslandsReport::text_moved`] moves its own.
    pub fn text_moved(&self, by: Pt, frame: &str) -> String {
        let at = |p: Pt| format!("({:.3}, {:.3})", p.x + by.x, p.y + by.y);
        let mut out = format!(
            "{}: the regions of the fills compared with {} (matched within each fill, its layer, net and source, by the overlap of their boxes, the largest first; places in mm in the {frame}'s frame; the outline's top left corner is at {})\n",
            self.board,
            self.against,
            at(self.outline_at)
        );
        if self.is_unchanged() {
            out.push_str(&format!(
                "nothing changed: the {} region(s) are as in {}, {} cut off from their net's main region\n",
                self.unchanged, self.against, self.cut_off_after
            ));
            return out;
        }
        let state = |main: bool| {
            if main {
                "in the net's main region"
            } else {
                "cut off from the net's main region"
            }
        };
        let head = |layer: &str, source: FillSource, net: &str| {
            format!("{layer} {} on {net}", source.word())
        };
        let region = |r: &RegionOfFill| {
            let fill = FillRegion {
                at: r.at,
                bounds: r.bounds,
                area: r.area,
                outline: FillPolygon {
                    outer: Vec::new(),
                    holes: Vec::new(),
                },
                pads: r.pads.clone(),
                vias: r.vias.clone(),
                main: r.main,
                bounded_by: Vec::new(),
            };
            format!(
                "{} region at {} (box {} to {}): {:.3} mm2, {}{}",
                head(&r.layer, r.source, &r.net),
                at(r.at),
                at(r.bounds.min),
                at(r.bounds.max),
                r.area,
                state(r.main),
                joins(&fill)
            )
        };
        for r in &self.new {
            out.push_str(&format!("new: {}\n", region(r)));
            for b in &r.bounded_by {
                out.push_str(&format!("    bounded by {}\n", bound_text(b, &at)));
            }
        }
        for r in &self.gone {
            out.push_str(&format!("gone: {}\n", region(r)));
        }
        for c in &self.changed {
            let mut what = vec![format!(
                "{:.3} to {:.3} mm2 ({:+.3})",
                c.area_before,
                c.area_after,
                c.area_after - c.area_before
            )];
            if c.main_before != c.main_after {
                what.push(format!("now {}", state(c.main_after)));
            }
            let pads = |p: &[RegionPad]| {
                p.iter()
                    .map(|p| pad_name(&p.part, &p.pad))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            if !c.pads_added.is_empty() {
                what.push(format!("now joins {}", pads(&c.pads_added)));
            }
            if !c.pads_removed.is_empty() {
                what.push(format!("no longer joins {}", pads(&c.pads_removed)));
            }
            if !c.vias_added.is_empty() {
                what.push(format!("{} via(s) more", c.vias_added.len()));
            }
            if !c.vias_removed.is_empty() {
                what.push(format!("{} via(s) fewer", c.vias_removed.len()));
            }
            out.push_str(&format!(
                "changed: {} region at {} (box {} to {}): {}\n",
                head(&c.layer, c.source, &c.net),
                at(c.at),
                at(c.bounds.min),
                at(c.bounds.max),
                what.join("; ")
            ));
            for b in &c.bounds_added {
                out.push_str(&format!("    now bounded by {}\n", bound_text(b, &at)));
            }
            for b in &c.bounds_removed {
                out.push_str(&format!(
                    "    no longer bounded by {}\n",
                    bound_text(b, &at)
                ));
            }
        }
        out.push_str(&format!(
            "{} new, {} gone, {} changed, {} unchanged; regions cut off from their net's main region: {} before, {} after\n",
            self.new.len(),
            self.gone.len(),
            self.changed.len(),
            self.unchanged,
            self.cut_off_before,
            self.cut_off_after
        ));
        let any_new = self.new.iter().any(|r| !r.bounded_by.is_empty())
            || self.changed.iter().any(|c| !c.bounds_added.is_empty());
        if any_new && self.nets.is_empty() {
            out.push_str("the new boundary items are of no net\n");
        } else if any_new {
            out.push_str(&format!(
                "the nets of the new boundary items: {}\n",
                self.nets.join(", ")
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RegionBoundKind;
    use crate::board::{Board, BoardPour, CopperLayer, Track};

    /// The LED board with a GND pour on the top, which joins both GND pads (D1.1 and J1.2).
    fn poured_top() -> Board {
        let mut board = crate::board::tests::led_board();
        board.pours.push(BoardPour {
            layer: CopperLayer::Front,
            net: "GND".into(),
            stitch_pitch: None,
            thermal_gap: None,
            spoke_width: None,
            clearance: None,
            min_width: None,
            pads: None,
        });
        board
    }

    /// A loop of VCC track round D1: the pour inside it is cut off.
    fn pocketed() -> Board {
        let mut board = poured_top();
        let (x0, x1, y0, y1) = (30.6, 34.4, 22.2, 26.2);
        let corners = [
            Pt::new(x0, y0),
            Pt::new(x1, y0),
            Pt::new(x1, y1),
            Pt::new(x0, y1),
        ];
        for k in 0..4 {
            board.tracks.push(Track::straight(
                "VCC",
                CopperLayer::Front,
                corners[k],
                corners[(k + 1) % 4],
                0.2,
            ));
        }
        board
    }

    #[test]
    fn an_unchanged_board_says_nothing_changed() {
        let report = IslandsReport::of(&pocketed()).unwrap();
        // Through JSON, as `--compare FILE.json` reads it.
        let read: IslandsReport =
            serde_json::from_value(serde_json::to_value(&report).unwrap()).unwrap();
        let c = IslandsComparison::of(&read, &report, "before.json");
        assert!(c.is_unchanged(), "{}", c.text());
        assert!(c.nets.is_empty());
        assert_eq!(
            c.unchanged as usize,
            report.fills.iter().map(|f| f.regions.len()).sum::<usize>()
        );
        assert!(c.text().contains("nothing changed: the "), "{}", c.text());
    }

    #[test]
    fn a_track_that_cuts_a_pocket_is_a_new_region_bounded_by_the_track() {
        let before = IslandsReport::of(&poured_top()).unwrap();
        let after = IslandsReport::of(&pocketed()).unwrap();
        let c = IslandsComparison::of(&before, &after, "before.routes.toml");
        let text = c.text();
        assert_eq!(c.new.len(), 1, "{text}");
        assert!(c.gone.is_empty(), "{text}");
        let pocket = &c.new[0];
        assert_eq!((pocket.layer.as_str(), pocket.net.as_str()), ("top", "GND"));
        assert!(!pocket.main, "{text}");
        assert!(
            pocket
                .pads
                .iter()
                .any(|p| (p.part.as_str(), p.pad.as_str()) == ("D1", "1"))
        );
        assert!(
            pocket
                .bounded_by
                .iter()
                .any(|b| b.kind == RegionBoundKind::Track
                    && b.net.as_deref() == Some("VCC")
                    && b.parts_from_main),
            "{text}"
        );
        assert!(c.nets.iter().any(|n| n == "VCC"), "{text}");
        // The main region lost the pad the pocket holds, and some area.
        let main = c
            .changed
            .iter()
            .find(|r| r.layer == "top" && r.net == "GND")
            .unwrap_or_else(|| panic!("{text}"));
        assert!(main.main_before && main.main_after, "{text}");
        assert!(main.area_after < main.area_before, "{text}");
        assert!(main.pads_removed.iter().any(|p| p.part == "D1"), "{text}");
        assert!(text.contains("new: top pour on GND region at "), "{text}");
        assert!(
            text.contains("    bounded by track [VCC] (30.600, 22.200) to (34.400, 22.200)"),
            "{text}"
        );
        assert!(
            text.contains("the nets of the new boundary items: VCC"),
            "{text}"
        );
        assert!(text.contains("no longer joins D1.1"), "{text}");
        assert_eq!((c.cut_off_before, c.cut_off_after), (0, 1));
        // And the other way round: the pocket is gone.
        let back = IslandsComparison::of(&after, &before, "after.json");
        assert_eq!((back.new.len(), back.gone.len()), (0, 1), "{}", back.text());
        assert!(back.nets.is_empty(), "{}", back.text());
    }
}
