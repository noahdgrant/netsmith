//! What the fill of a board's pours and planes does with its pads (issue #1187): for each pour,
//! plane or zone of the routes file on each copper layer, how it joins each pad of its net
//! (solid copper, a thermal relief with the spokes that reach the fill, or not at all), the pads
//! of other nets it keeps clear of, the pads it starves (KiCad's `starved_thermal`: fewer than
//! two spokes, with the tracks of the pad's net onto the fill), and the copper islands it keeps
//! and the ones it took out, with their places and areas. `netsmith board fill` prints it and
//! the editor's PCB summary tab shows it, both from [`FillReport::of`], so the two cannot
//! differ.
//!
//! The fill is the one the Gerbers hold and the design rules check judges
//! (`netsmith_geom::pad_joins`, from the board file in memory as [`crate::copper_of`] fills it),
//! so the starved pads here are the `starved_thermal` findings of `board drc` on the same board,
//! and no KiCad refill is needed to learn which pads hang on one spoke. The places are in the
//! frame of the board file, y down, as the design rules check's are; [`FillReport::outline_at`]
//! is the outline's top left corner there, so a place less it is in the frame of the layout and
//! routes files.
//!
//! The kinds counted ([`JoinCounts`]): a thermal relief by how many of its spokes reach the fill
//! (none, one, two or more; the tracks onto the fill are counted apart and make a relief with one
//! spoke enough), solid, and none. The data types are meant to be read again: the 2D view's
//! markers and the zones `layout export` writes take the same report.

use serde::{Deserialize, Serialize};

use crate::CopperLayer;
use crate::board::Board;
use crate::copper::{CopperError, artwork};
use crate::geometry::Pt;

/// What the copper area on a layer is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FillSource {
    /// An inner layer that is one sheet on a net.
    Plane,
    /// An outer layer poured on a net by the process.
    Pour,
    /// A zone of the routes file: the layer takes its zones from the file alone.
    Zone,
}

impl FillSource {
    /// The word of the text and of JSON.
    pub fn word(self) -> &'static str {
        match self {
            FillSource::Plane => "plane",
            FillSource::Pour => "pour",
            FillSource::Zone => "zone",
        }
    }
}

/// How the fill joins a pad of its net.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinKind {
    /// Solid copper up to the pad.
    Solid,
    /// A thermal relief: a gap round the pad, crossed by spokes.
    Thermal,
    /// Not at all: the pad connection is `none`, and the fill keeps its clearance round it.
    None,
}

impl JoinKind {
    pub fn word(self) -> &'static str {
        match self {
            JoinKind::Solid => "solid",
            JoinKind::Thermal => "thermal",
            JoinKind::None => "none",
        }
    }
}

/// A pad of the fill's net, and how the fill joins it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillPad {
    /// The designator of its part and its number.
    pub part: String,
    pub pad: String,
    /// Its centre, in the frame of the board file.
    pub at: Pt,
    pub join: JoinKind,
    /// Of a thermal relief: how many spokes it has (none where the pad is narrower than the
    /// fill's minimum width), and how many of them reach the fill. `None` otherwise.
    pub spokes: Option<u32>,
    pub joined_spokes: Option<u32>,
    /// How many tracks of the pad's net on the layer run from it onto the fill (of a thermal
    /// relief; 0 otherwise): each joins it as a spoke does.
    pub tracks: u32,
    /// Whether the fill reaches the pad at all: by a spoke or a track, or solid copper that
    /// touches it.
    pub joined: bool,
    /// Whether the fill starves it (KiCad's `starved_thermal`, the design rules check's): a thermal
    /// relief that fewer than two spokes and tracks join to the fill.
    pub starved: bool,
}

/// A pad of another net (or of none) in the fill's outline, which the fill keeps clear of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClearedPad {
    pub part: String,
    pub pad: String,
    pub net: Option<String>,
    pub at: Pt,
}

/// A copper island of the fill: a piece that touches no copper of its net on the layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillIsland {
    /// A corner of it, in the frame of the board file (the place the design rules check gives
    /// an `isolated_copper` finding).
    pub at: Pt,
    /// Its area, in mm².
    pub area: f64,
}

/// How many pads the fill joins in each way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinCounts {
    /// Thermal reliefs by how many of their spokes reach the fill.
    pub no_spokes: u32,
    pub one_spoke: u32,
    pub two_or_more_spokes: u32,
    pub solid: u32,
    pub none: u32,
    /// The starved thermal reliefs among them ([`FillPad::starved`]).
    pub starved: u32,
}

impl JoinCounts {
    fn add(&mut self, pad: &FillPad) {
        match (pad.join, pad.joined_spokes) {
            (JoinKind::Solid, _) => self.solid += 1,
            (JoinKind::None, _) => self.none += 1,
            (JoinKind::Thermal, Some(0) | None) => self.no_spokes += 1,
            (JoinKind::Thermal, Some(1)) => self.one_spoke += 1,
            (JoinKind::Thermal, Some(_)) => self.two_or_more_spokes += 1,
        }
        self.starved += u32::from(pad.starved);
    }

    fn sum(&mut self, other: &JoinCounts) {
        self.no_spokes += other.no_spokes;
        self.one_spoke += other.one_spoke;
        self.two_or_more_spokes += other.two_or_more_spokes;
        self.solid += other.solid;
        self.none += other.none;
        self.starved += other.starved;
    }

    /// How many pads in all.
    pub fn pads(&self) -> u32 {
        self.no_spokes + self.one_spoke + self.two_or_more_spokes + self.solid + self.none
    }

    fn text(&self) -> String {
        format!(
            "{} pad(s): {} by 2 or more spokes, {} by 1 spoke, {} by no spoke, {} solid, {} none; {} starved",
            self.pads(),
            self.two_or_more_spokes,
            self.one_spoke,
            self.no_spokes,
            self.solid,
            self.none,
            self.starved
        )
    }
}

/// The fill of one pour, plane or zone on one layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillArea {
    /// The routes file's word for the layer (`top`, `inner1`, `bottom`) and KiCad's name.
    pub layer: String,
    pub layer_name: String,
    pub net: String,
    pub source: FillSource,
    /// Whether the fill is the one the board file holds rather than one worked out by netsmith
    /// (a board that came from KiCad with its zones filled).
    pub held: bool,
    /// The fill's area, in mm².
    pub area: f64,
    /// Each pad of its net in its outline, in the board's order of pads.
    pub pads: Vec<FillPad>,
    pub counts: JoinCounts,
    /// The pads of other nets in its outline.
    pub cleared: Vec<ClearedPad>,
    /// The copper islands the fill keeps (its island setting keeps them), and the ones it took
    /// out.
    pub islands: Vec<FillIsland>,
    pub removed_islands: Vec<FillIsland>,
}

/// A starved thermal relief, as the design rules check names it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StarvedPad {
    pub layer: String,
    pub net: String,
    pub part: String,
    pub pad: String,
    pub at: Pt,
    pub spokes: u32,
    pub joined_spokes: u32,
    pub tracks: u32,
}

/// What the fill of a board's pours does with its pads ([module](self)).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillReport {
    pub board: String,
    /// The outline's top left corner in the frame of the board file.
    pub outline_at: Pt,
    /// Each pour, plane or zone of a net, by layer from the top, then in the board file's order.
    pub fills: Vec<FillArea>,
    /// The counts of every fill together.
    pub counts: JoinCounts,
    /// Every starved thermal relief.
    pub starved: Vec<StarvedPad>,
}

impl FillReport {
    /// The report of `board` as it is, with whatever copper is on it: the fill of every zone of
    /// a net of its board file.
    pub fn of(board: &Board) -> Result<FillReport, CopperError> {
        let art = artwork(board)?;
        let rules = netsmith_geom::plot_rules_of(Some(&crate::export::kicad_pro(board)));
        let edge = netsmith_geom::edge_outline(&art).map_err(CopperError)?;
        let pt = |p: netsmith_geom::Pt| Pt::new(p.x, p.y);
        let island = |(at, area): (netsmith_geom::Pt, f64)| FillIsland { at: pt(at), area };
        let mut fills = Vec::new();
        for layer in &art.copper {
            let copper = CopperLayer::of_name(layer);
            let word: &str = match copper {
                Some(c) => c.word(),
                None => layer,
            };
            let source = if board.zoned(layer) {
                FillSource::Zone
            } else if matches!(copper, Some(CopperLayer::Inner(_))) {
                FillSource::Plane
            } else {
                FillSource::Pour
            };
            for z in netsmith_geom::pad_joins(&art, layer, &edge, &rules).map_err(CopperError)? {
                let mut counts = JoinCounts::default();
                let pads: Vec<FillPad> = z
                    .pads
                    .into_iter()
                    .map(|(p, join)| {
                        let pad = fill_pad(p.part, p.number, pt(p.at), join);
                        counts.add(&pad);
                        pad
                    })
                    .collect();
                fills.push(FillArea {
                    layer: word.to_string(),
                    layer_name: layer.clone(),
                    net: art.zones[z.zone].net.clone().unwrap_or_default(),
                    source,
                    held: z.held,
                    area: z.fill.iter().map(|a| a.size()).sum(),
                    pads,
                    counts,
                    cleared: z
                        .cleared
                        .into_iter()
                        .map(|p| ClearedPad {
                            part: p.part,
                            pad: p.number,
                            net: p.net,
                            at: pt(p.at),
                        })
                        .collect(),
                    islands: z.islands.into_iter().map(island).collect(),
                    removed_islands: z.removed.into_iter().map(island).collect(),
                });
            }
        }
        let mut counts = JoinCounts::default();
        let mut starved = Vec::new();
        for f in &fills {
            counts.sum(&f.counts);
            starved.extend(f.pads.iter().filter(|p| p.starved).map(|p| StarvedPad {
                layer: f.layer.clone(),
                net: f.net.clone(),
                part: p.part.clone(),
                pad: p.pad.clone(),
                at: p.at,
                spokes: p.spokes.unwrap_or(0),
                joined_spokes: p.joined_spokes.unwrap_or(0),
                tracks: p.tracks,
            }));
        }
        Ok(FillReport {
            board: board.name.clone(),
            outline_at: board.outline.min,
            fills,
            counts,
            starved,
        })
    }

    /// The report as text for a person: each fill with its counts, its pads, the pads it clears
    /// and its islands, then the counts of the board and the starved reliefs.
    pub fn text(&self) -> String {
        let at = |p: Pt| format!("({:.3}, {:.3})", p.x, p.y);
        let mut out = format!(
            "{}: the fill of {} pour(s), plane(s) and zone(s), as the Gerbers hold it (places in mm in the board file's frame; the outline's top left corner is at {})\n",
            self.board,
            self.fills.len(),
            at(self.outline_at)
        );
        for f in &self.fills {
            out.push_str(&format!(
                "{} {} on {}{}: {:.2} mm2, {}\n",
                f.layer,
                f.source.word(),
                f.net,
                if f.held {
                    " (the fill the board file holds)"
                } else {
                    ""
                },
                f.area,
                f.counts.text()
            ));
            for p in &f.pads {
                let how = match (p.join, p.spokes, p.joined_spokes) {
                    (JoinKind::Thermal, Some(0), _) => {
                        "thermal, no spokes (the pad is narrower than the minimum width)".into()
                    }
                    (JoinKind::Thermal, Some(n), Some(r)) => format!("thermal, {r} of {n} spokes"),
                    (JoinKind::Solid, _, _) if !p.joined => {
                        "solid, the fill does not reach it".into()
                    }
                    (kind, _, _) => kind.word().to_string(),
                };
                let tracks = match p.tracks {
                    0 => String::new(),
                    1 => " and 1 track".into(),
                    n => format!(" and {n} tracks"),
                };
                out.push_str(&format!(
                    "  {} at {}: {how}{tracks}{}\n",
                    pad_name(&p.part, &p.pad),
                    at(p.at),
                    if p.starved { " (starved)" } else { "" }
                ));
            }
            if !f.cleared.is_empty() {
                let names: Vec<String> = f
                    .cleared
                    .iter()
                    .map(|p| {
                        format!(
                            "{} ({})",
                            pad_name(&p.part, &p.pad),
                            p.net.as_deref().unwrap_or("no net")
                        )
                    })
                    .collect();
                out.push_str(&format!(
                    "  keeps clear of {} pad(s) of other nets: {}\n",
                    f.cleared.len(),
                    names.join(", ")
                ));
            }
            for (what, islands) in [
                ("kept", &f.islands),
                ("removed by the fill", &f.removed_islands),
            ] {
                for i in islands.iter() {
                    out.push_str(&format!(
                        "  copper island {what} at {}: {:.3} mm2\n",
                        at(i.at),
                        i.area
                    ));
                }
            }
        }
        out.push_str(&format!("in all: {}\n", self.counts.text()));
        if self.starved.is_empty() {
            out.push_str("starved thermal reliefs: none\n");
        } else {
            out.push_str(&format!(
                "starved thermal reliefs (fewer than {} spokes and tracks join the pad to the fill): {}\n",
                netsmith_geom::MIN_SPOKES,
                self.starved.len()
            ));
            for s in &self.starved {
                out.push_str(&format!(
                    "  {} on {} ({}) at {}: {} of {} spokes, {} track(s)\n",
                    pad_name(&s.part, &s.pad),
                    s.layer,
                    s.net,
                    at(s.at),
                    s.joined_spokes,
                    s.spokes,
                    s.tracks
                ));
            }
        }
        out
    }
}

/// A pad by its part and number, `U1.2`, or the part alone for a pad with no number (a
/// fiducial's).
fn pad_name(part: &str, pad: &str) -> String {
    if pad.is_empty() {
        part.to_string()
    } else {
        format!("{part}.{pad}")
    }
}

fn fill_pad(part: String, pad: String, at: Pt, join: netsmith_geom::PadJoin) -> FillPad {
    let starved = join.starved();
    match join {
        netsmith_geom::PadJoin::Solid { touched } => FillPad {
            part,
            pad,
            at,
            join: JoinKind::Solid,
            spokes: None,
            joined_spokes: None,
            tracks: 0,
            joined: touched,
            starved,
        },
        netsmith_geom::PadJoin::Thermal {
            spokes,
            reach,
            tracks,
        } => FillPad {
            part,
            pad,
            at,
            join: JoinKind::Thermal,
            spokes: Some(spokes as u32),
            joined_spokes: Some(reach as u32),
            tracks: tracks as u32,
            joined: reach + tracks > 0,
            starved,
        },
        netsmith_geom::PadJoin::Unjoined => FillPad {
            part,
            pad,
            at,
            join: JoinKind::None,
            spokes: None,
            joined_spokes: None,
            tracks: 0,
            joined: false,
            starved,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{BoardPour, CopperLayer};

    /// The four-layer board (planes inside) with a GND pour on the top whose 1.5 mm clearance
    /// round D1's anode pad cuts the spokes of the GND pad beside it.
    fn starving() -> Board {
        let mut board = crate::board::tests::four_layer_board();
        board.pours.push(BoardPour {
            layer: CopperLayer::Front,
            net: "GND".into(),
            stitch_pitch: None,
            thermal_gap: None,
            spoke_width: None,
            clearance: Some(1.5),
            min_width: None,
            pads: None,
        });
        board
    }

    #[test]
    fn the_starved_pads_are_the_checks_and_the_fill_is_the_gerbers() {
        let board = starving();
        let report = FillReport::of(&board).unwrap();
        let checked = netsmith_geom::drc_check(
            &crate::kicad_pcb(&board),
            Some(&crate::kicad_pro(&board)),
            &Default::default(),
        )
        .unwrap();
        let mut found: Vec<(String, (i64, i64))> = checked
            .findings
            .iter()
            .filter(|f| f.rule.id() == "starved_thermal")
            .map(|f| {
                let pad = f.items.iter().find_map(|i| i.part.clone()).unwrap();
                (
                    pad,
                    ((f.at.x * 1e3).round() as i64, (f.at.y * 1e3).round() as i64),
                )
            })
            .collect();
        found.sort();
        let mut ours: Vec<(String, (i64, i64))> = report
            .starved
            .iter()
            .map(|s| {
                (
                    s.part.clone(),
                    ((s.at.x * 1e3).round() as i64, (s.at.y * 1e3).round() as i64),
                )
            })
            .collect();
        ours.sort();
        assert_eq!(ours.len(), 1, "{}", report.text());
        assert_eq!(ours, found, "{}", report.text());
        assert_eq!(report.counts.starved, 1);

        // Each layer's fill has the area of the Gerbers' fill of it.
        let copper = crate::copper_of(&board).unwrap();
        for l in &copper.layers {
            let gerber: f64 = l.pours.iter().map(|p| p.area.size()).sum();
            let reported: f64 = report
                .fills
                .iter()
                .filter(|f| f.layer_name == l.layer)
                .map(|f| f.area)
                .sum();
            assert!(
                (gerber - reported).abs() < 1e-6,
                "{}: {gerber} and {reported}",
                l.layer
            );
        }
        let top: Vec<&FillArea> = report.fills.iter().filter(|f| f.layer == "top").collect();
        assert_eq!(top.len(), 1);
        assert_eq!(
            (top[0].source, top[0].net.as_str()),
            (FillSource::Pour, "GND")
        );
        assert!(
            !top[0].cleared.is_empty(),
            "the pour clears the other nets' pads"
        );
        assert!(
            report
                .fills
                .iter()
                .any(|f| f.source == FillSource::Plane && f.layer == "inner1"),
            "{}",
            report.text()
        );
        // The counts are of the pads listed.
        let mut sum = JoinCounts::default();
        for f in &report.fills {
            assert_eq!(f.counts.pads() as usize, f.pads.len());
            sum.sum(&f.counts);
        }
        assert_eq!(sum, report.counts);
        let text = report.text();
        assert!(text.contains("(starved)"), "{text}");
        assert!(
            text.contains("starved thermal reliefs (fewer than 2"),
            "{text}"
        );
    }

    #[test]
    fn a_pad_is_counted_by_the_spokes_that_reach_the_fill() {
        let pad = |join| fill_pad("U1".into(), "2".into(), Pt::new(0.0, 0.0), join);
        let mut c = JoinCounts::default();
        for join in [
            netsmith_geom::PadJoin::Thermal {
                spokes: 4,
                reach: 1,
                tracks: 0,
            },
            netsmith_geom::PadJoin::Thermal {
                spokes: 4,
                reach: 1,
                tracks: 1,
            },
            netsmith_geom::PadJoin::Thermal {
                spokes: 4,
                reach: 3,
                tracks: 0,
            },
            netsmith_geom::PadJoin::Thermal {
                spokes: 0,
                reach: 0,
                tracks: 0,
            },
            netsmith_geom::PadJoin::Solid { touched: true },
            netsmith_geom::PadJoin::Unjoined,
        ] {
            c.add(&pad(join));
        }
        assert_eq!(
            c,
            JoinCounts {
                no_spokes: 1,
                one_spoke: 2,
                two_or_more_spokes: 1,
                solid: 1,
                none: 1,
                starved: 2,
            }
        );
    }
}
