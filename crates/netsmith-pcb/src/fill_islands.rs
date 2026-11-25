//! The islands of a board's fills (issue #1655): for each pour, plane or zone on each copper
//! layer, each separate region of its fill with its area, the pads and vias of its net it joins,
//! whether it reaches the net's main region, and, of a region cut off from it, the tracks, vias,
//! pads and rule areas that bound it, so that the track that closed a pocket can be found.
//! `netsmith board fill --islands` prints it.
//!
//! The fill is [`FillReport`](crate::FillReport)'s (`netsmith_geom::pad_joins`, the one the Gerbers hold and the
//! design rules check judges), and the regions are its pieces; how a region is joined to its net
//! and what bounds it are `netsmith_geom::fill_regions`'s. The net's main region is the copper
//! of the net, joined across every layer by its tracks, vias and pads with holes, that holds the
//! most of its pads (then the most fill): a region outside it is cut off, though a pad in it may
//! be joined to the rest by a track of its own. What bounds a cut-off region is listed with how
//! far it is from the region and whether it comes as near a region of the main one on the layer
//! (`parts_from_main`): what lies between the two parts them, the track that closed the pocket
//! among it. The places are in the frame of the board file, y down, as [`FillReport`](crate::FillReport)'s are.

use serde::{Deserialize, Serialize};

use crate::CopperLayer;
use crate::board::Board;
use crate::copper::{CopperError, artwork};
use crate::fill_report::{FillPolygon, FillSource, fill_source, pad_name};
use crate::geometry::{Pt, Rect};

/// How far a point of a region's outline may move when it is simplified, in mm, as
/// [`FillReport`](crate::FillReport)'s outlines are.
const SIMPLIFY: f64 = 0.002;

/// A pad of the region's net the region joins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionPad {
    pub part: String,
    pub pad: String,
    pub at: Pt,
}

/// A via of the region's net the region joins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionVia {
    pub at: Pt,
}

/// What bounds a cut-off region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionBoundKind {
    Track,
    Via,
    Pad,
    /// A rule area that forbids pours on the layer.
    RuleArea,
}

impl RegionBoundKind {
    pub fn word(self) -> &'static str {
        match self {
            RegionBoundKind::Track => "track",
            RegionBoundKind::Via => "via",
            RegionBoundKind::Pad => "pad",
            RegionBoundKind::RuleArea => "rule area",
        }
    }
}

/// A track, via, pad or rule area that bounds a cut-off region: the fill stops short of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionBound {
    pub kind: RegionBoundKind,
    /// Its net (none of a rule area).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<String>,
    /// A track's start, a via's or a pad's centre, a rule area's first corner.
    pub at: Pt,
    /// A track's end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<Pt>,
    /// A track's width, in mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    /// A pad's part and number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pad: Option<String>,
    /// A rule area's name, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// How far its copper (a rule area's outline) is from the region, in mm.
    pub gap: f64,
    /// Whether it comes as near a region of the net's main region on the layer: it lies between
    /// the two, and is among what parts them.
    pub parts_from_main: bool,
}

/// One separate region of a fill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillRegion {
    /// A corner of it (the place the design rules check gives an island) and its box.
    pub at: Pt,
    pub bounds: Rect,
    /// Its area, in mm².
    pub area: f64,
    /// Its copper, with its holes, simplified within 2 µm.
    pub outline: FillPolygon,
    /// The pads and vias of its net it joins.
    pub pads: Vec<RegionPad>,
    pub vias: Vec<RegionVia>,
    /// Whether it is in the main region of its net.
    pub main: bool,
    /// Of a region cut off from the main one: what bounds it, what parts it from the main region
    /// first, then the nearest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bounded_by: Vec<RegionBound>,
}

/// The regions of the fill of one pour, plane or zone on one layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillRegions {
    /// The routes file's word for the layer and KiCad's name.
    pub layer: String,
    pub layer_name: String,
    pub net: String,
    pub source: FillSource,
    /// Its regions, in the fill's order.
    pub regions: Vec<FillRegion>,
}

/// The islands of a board's fills ([module](self)).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IslandsReport {
    pub board: String,
    /// The outline's top left corner in the frame of the board file.
    pub outline_at: Pt,
    /// Each pour, plane or zone of a net, as [`FillReport::fills`](crate::FillReport::fills) lists them.
    pub fills: Vec<FillRegions>,
    /// How many regions of every fill are cut off from the main region of their net.
    pub cut_off: u32,
}

impl IslandsReport {
    /// The report of `board` as it is, with whatever copper is on it.
    pub fn of(board: &Board) -> Result<IslandsReport, CopperError> {
        let art = artwork(board)?;
        let rules = netsmith_geom::plot_rules_of(Some(&crate::export::kicad_pro(board)));
        let edge = netsmith_geom::edge_outline(&art).map_err(CopperError)?;
        let joins = netsmith_geom::pad_joins(&art, &edge, &rules).map_err(CopperError)?;
        let regions = netsmith_geom::fill_regions(&art, &rules, &joins);
        let pt = |p: netsmith_geom::Pt| Pt::new(p.x, p.y);
        let mut fills = Vec::new();
        for (layer, zones) in regions {
            let word = match CopperLayer::of_name(&layer) {
                Some(c) => c.word().to_string(),
                None => layer.clone(),
            };
            for z in zones {
                let zone = &art.zones[z.zone];
                let net = zone.net.clone().unwrap_or_default();
                let source = fill_source(board, &layer, &net, zone.priority);
                let regions = z
                    .regions
                    .into_iter()
                    .filter_map(|r| {
                        let size = r.piece.size();
                        let piece = crate::copper::area(r.piece);
                        let ring = |r: &[Pt]| crate::simplify::simplify_ring(r, SIMPLIFY);
                        let outer = ring(&piece.outer);
                        let at = *piece.outer.first()?;
                        let bounds = Rect::around(piece.outer.iter().copied())?;
                        Some(FillRegion {
                            at,
                            bounds,
                            area: size,
                            outline: FillPolygon {
                                outer,
                                holes: piece
                                    .holes
                                    .iter()
                                    .map(|h| ring(h))
                                    .filter(|h| h.len() >= 3)
                                    .collect(),
                            },
                            pads: r
                                .pads
                                .iter()
                                .map(|&i| {
                                    let p = &art.pads[i];
                                    RegionPad {
                                        part: p.part.clone(),
                                        pad: p.number.clone(),
                                        at: pt(p.at),
                                    }
                                })
                                .collect(),
                            vias: r
                                .vias
                                .iter()
                                .map(|&i| RegionVia {
                                    at: pt(art.vias[i].at),
                                })
                                .collect(),
                            main: r.main,
                            bounded_by: r.bounds.iter().map(|b| bound(&art, b)).collect(),
                        })
                    })
                    .collect();
                fills.push(FillRegions {
                    layer: word.clone(),
                    layer_name: layer.clone(),
                    net,
                    source,
                    regions,
                });
            }
        }
        let cut_off = fills
            .iter()
            .flat_map(|f| &f.regions)
            .filter(|r| !r.main)
            .count() as u32;
        Ok(IslandsReport {
            board: board.name.clone(),
            outline_at: board.outline.min,
            fills,
            cut_off,
        })
    }

    /// The report as text for a person: each fill with its regions.
    pub fn text(&self) -> String {
        self.text_moved(Pt::new(0.0, 0.0), "board file")
    }

    /// [`IslandsReport::text`] with its places moved by `by`, mm, and the frame they are then in
    /// named `frame`, as [`FillReport::text_moved`](crate::FillReport::text_moved) moves its own.
    pub fn text_moved(&self, by: Pt, frame: &str) -> String {
        let at = |p: Pt| format!("({:.3}, {:.3})", p.x + by.x, p.y + by.y);
        let mut out = format!(
            "{}: the regions of the fill of {} pour(s), plane(s) and zone(s), as the Gerbers hold it (places in mm in the {frame}'s frame; the outline's top left corner is at {})\n",
            self.board,
            self.fills.len(),
            at(self.outline_at)
        );
        for f in &self.fills {
            let head = format!("{} {} on {}", f.layer, f.source.word(), f.net);
            match f.regions.as_slice() {
                [] => out.push_str(&format!("{head}: no copper\n")),
                [r] if r.main => {
                    out.push_str(&format!(
                        "{head}: one region, {:.2} mm2, in the net's main region{}\n",
                        r.area,
                        joins(r)
                    ));
                }
                regions => {
                    let cut = regions.iter().filter(|r| !r.main).count();
                    out.push_str(&format!(
                        "{head}: {} region(s), {cut} cut off from the net's main region\n",
                        regions.len()
                    ));
                    for (k, r) in regions.iter().enumerate() {
                        let state = if r.main {
                            "in the net's main region"
                        } else {
                            "cut off from the net's main region"
                        };
                        out.push_str(&format!(
                            "  region {} at {} (box {} to {}): {:.3} mm2, {state}{}\n",
                            k + 1,
                            at(r.at),
                            at(r.bounds.min),
                            at(r.bounds.max),
                            r.area,
                            joins(r)
                        ));
                        if !r.main {
                            if r.bounded_by.is_empty() {
                                out.push_str(
                                    "    bounded by the fill's outline or the board's edge alone\n",
                                );
                            }
                            for b in &r.bounded_by {
                                out.push_str(&format!("    bounded by {}\n", bound_text(b, &at)));
                            }
                        }
                    }
                }
            }
        }
        out.push_str(&format!(
            "regions cut off from their net's main region: {}\n",
            self.cut_off
        ));
        out
    }
}

/// What a region joins, after its state: `; joins D1.1, R2.2 and 1 via`.
fn joins(r: &FillRegion) -> String {
    let mut parts: Vec<String> = r.pads.iter().map(|p| pad_name(&p.part, &p.pad)).collect();
    match r.vias.len() {
        0 => {}
        1 => parts.push("1 via".into()),
        n => parts.push(format!("{n} vias")),
    }
    if parts.is_empty() {
        "; joins no pad and no via".into()
    } else {
        format!("; joins {}", parts.join(", "))
    }
}

fn bound_text(b: &RegionBound, at: &dyn Fn(Pt) -> String) -> String {
    let net = |n: &Option<String>| match n {
        Some(n) => format!(" [{n}]"),
        None => " [no net]".into(),
    };
    let what = match b.kind {
        RegionBoundKind::Track => format!(
            "track{} {} to {}{}",
            net(&b.net),
            at(b.at),
            b.to.map(at).unwrap_or_default(),
            b.width
                .map(|w| format!(", {w:.3} mm wide"))
                .unwrap_or_default()
        ),
        RegionBoundKind::Via => format!("via{} at {}", net(&b.net), at(b.at)),
        RegionBoundKind::Pad => format!(
            "pad {}{} at {}",
            pad_name(
                b.part.as_deref().unwrap_or_default(),
                b.pad.as_deref().unwrap_or_default()
            ),
            net(&b.net),
            at(b.at)
        ),
        RegionBoundKind::RuleArea => match &b.name {
            Some(name) => format!("rule area `{name}` at {}", at(b.at)),
            None => format!("rule area at {}", at(b.at)),
        },
    };
    let parts = if b.parts_from_main {
        ", and as near the net's main region: it parts the two"
    } else {
        ""
    };
    format!("{what}, {:.3} mm off{parts}", b.gap)
}

fn bound(art: &netsmith_geom::Artwork, b: &netsmith_geom::RegionBound) -> RegionBound {
    let pt = |p: netsmith_geom::Pt| Pt::new(p.x, p.y);
    let mut out = RegionBound {
        kind: RegionBoundKind::Track,
        net: None,
        at: Pt::new(0.0, 0.0),
        to: None,
        width: None,
        part: None,
        pad: None,
        name: None,
        gap: b.gap,
        parts_from_main: b.parts_from_main,
    };
    match b.what {
        netsmith_geom::RegionBounding::Track(i) => {
            let t = &art.tracks[i];
            let (from, to) = match &t.shape {
                netsmith_geom::TrackShape::Line(a, b) => (*a, *b),
                netsmith_geom::TrackShape::Arc(arc) => {
                    let p = arc.points();
                    (p[0], p[p.len() - 1])
                }
            };
            out.net = t.net.clone();
            out.at = pt(from);
            out.to = Some(pt(to));
            out.width = Some(t.width);
        }
        netsmith_geom::RegionBounding::Via(i) => {
            let v = &art.vias[i];
            out.kind = RegionBoundKind::Via;
            out.net = v.net.clone();
            out.at = pt(v.at);
        }
        netsmith_geom::RegionBounding::Pad(i) => {
            let p = &art.pads[i];
            out.kind = RegionBoundKind::Pad;
            out.net = p.net.clone();
            out.at = pt(p.at);
            out.part = Some(p.part.clone());
            out.pad = Some(p.number.clone());
        }
        netsmith_geom::RegionBounding::RuleArea(i) => {
            let z = &art.zones[i];
            out.kind = RegionBoundKind::RuleArea;
            out.at = z
                .outlines
                .first()
                .and_then(|o| o.first())
                .map(|p| pt(*p))
                .unwrap_or(out.at);
            out.name = (!z.name.is_empty()).then(|| z.name.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{BoardPour, CopperLayer, Track};

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

    fn top_gnd(report: &IslandsReport) -> &FillRegions {
        report
            .fills
            .iter()
            .find(|f| f.layer == "top" && f.net == "GND")
            .unwrap_or_else(|| panic!("{}", report.text()))
    }

    #[test]
    fn a_pour_with_one_region_says_so() {
        let report = IslandsReport::of(&poured_top()).unwrap();
        let gnd = top_gnd(&report);
        assert_eq!(gnd.regions.len(), 1, "{}", report.text());
        assert!(gnd.regions[0].main);
        let mut pads: Vec<String> = gnd.regions[0]
            .pads
            .iter()
            .map(|p| pad_name(&p.part, &p.pad))
            .collect();
        pads.sort();
        assert_eq!(pads, ["D1.1", "J1.2"], "{}", report.text());
        assert_eq!(report.cut_off, 0);
        let text = report.text();
        assert!(
            text.contains("top pour on GND: one region, ")
                && text.contains("in the net's main region; joins "),
            "{text}"
        );
        assert!(
            text.contains("regions cut off from their net's main region: 0"),
            "{text}"
        );
    }

    #[test]
    fn a_track_that_cuts_a_pocket_out_of_a_pour_is_named_with_the_pad_left_in_it() {
        let mut board = poured_top();
        // A loop of VCC track round D1 (its pads at about (31.8, 24.2) and (33.4, 24.2)): the
        // pour inside it holds D1.1 and none of the rest of GND.
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
        let report = IslandsReport::of(&board).unwrap();
        let text = report.text();
        let gnd = top_gnd(&report);
        let pocket: Vec<&FillRegion> = gnd.regions.iter().filter(|r| !r.main).collect();
        assert_eq!(pocket.len(), 1, "{text}");
        let pocket = pocket[0];
        assert!(
            pocket
                .pads
                .iter()
                .any(|p| (p.part.as_str(), p.pad.as_str()) == ("D1", "1")),
            "{text}"
        );
        assert!(
            !pocket.pads.iter().any(|p| p.part == "J1"),
            "J1.2 is outside the loop: {text}"
        );
        assert!(gnd.regions.iter().any(|r| {
            r.main
                && r.pads
                    .iter()
                    .any(|p| (p.part.as_str(), p.pad.as_str()) == ("J1", "2"))
        }));
        // The loop's tracks bound the pocket, and part it from the main region.
        let cutting: Vec<&RegionBound> = pocket
            .bounded_by
            .iter()
            .filter(|b| b.kind == RegionBoundKind::Track && b.net.as_deref() == Some("VCC"))
            .collect();
        assert_eq!(cutting.len(), 4, "{text}");
        assert!(cutting.iter().all(|b| b.parts_from_main), "{text}");
        assert!(pocket.bounded_by[0].parts_from_main, "{text}");
        assert_eq!(report.cut_off, 1, "{text}");
        assert!(
            text.contains("cut off from the net's main region; joins D1.1"),
            "{text}"
        );
        assert!(
            text.contains(
                "bounded by track [VCC] (30.600, 22.200) to (34.400, 22.200), 0.200 mm wide, "
            ) && text.contains("it parts the two"),
            "{text}"
        );
        // The JSON has the same.
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["cut_off"], 1);
    }
}
