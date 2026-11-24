//! What the fill of a board's pours and planes does with its pads (issue #1187): for each pour,
//! plane or zone of the routes file on each copper layer, how it joins each pad of its net
//! (solid copper, a thermal relief with the spokes that reach the fill, or not at all), the pads
//! of other nets it keeps clear of, the pads it starves (KiCad's `starved_thermal`: one or more
//! spokes but fewer than two, with the tracks of the pad's net onto the fill), the pads it joins
//! by nothing and why (`unjoinable_thermal`, issue #1222), and the copper islands it keeps
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

/// How far a point of a removed island's outline may move when it is simplified, in mm: the 2
/// µm the 2D views simplify a pour within.
const ISLAND_SIMPLIFY: f64 = 0.002;

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
    /// relief that one or more spokes, but fewer than two spokes and tracks, join to the fill.
    pub starved: bool,
    /// Of a thermal relief that no spoke and no track joins to the fill (the design rules
    /// check's `unjoinable_thermal`, issue #1222): why the fill makes none of its spokes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unjoinable: Option<String>,
    /// Of a thermal relief that no spoke and no track joins to this fill: whether another fill
    /// of its net joins the pad (a zone of the net of a higher priority over it, or a fill on
    /// another layer of a pad with a hole), which makes it not unjoinable (issue #1292).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub joined_elsewhere: bool,
    /// The pin whose own setting joins the pad (issue #1276), by its path in the design: its
    /// `.pour_connection`, `.spoke_width` or both, which the fill takes over the pour's. `None`
    /// for a pad joined as the pour says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stated_by: Option<String>,
    /// The width of the spokes the pad's pin states, in mm (`.spoke_width`), if it does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spoke_width: Option<f64>,
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
    /// Of an island the fill took out: its rings, the outline first and then its holes, each
    /// simplified within 2 µm, in the frame of the board file. `view pcb --islands` and the
    /// editor's 2D view outline it there (issue #1188). Empty for an island kept: it is copper of
    /// the fill and drawn with it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outline: Vec<Vec<Pt>>,
}

/// One piece of a fill: a polygon and its holes (issue #1293).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillPolygon {
    /// Its outline, simplified within 2 µm, in the frame of the board file.
    pub outer: Vec<Pt>,
    /// Its holes (round the pads, vias and tracks of other nets, the gaps of thermal reliefs),
    /// simplified so.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Vec<Pt>>,
}

impl FillPolygon {
    /// Whether `p` is on the copper of the piece: in its outline and in none of its holes.
    pub fn contains(&self, p: Pt) -> bool {
        in_ring(&self.outer, p) && !self.holes.iter().any(|h| in_ring(h, p))
    }

    /// The least distance from `p` to an edge of the piece, its outline's or a hole's.
    pub fn distance_to_edge(&self, p: Pt) -> f64 {
        std::iter::once(&self.outer)
            .chain(&self.holes)
            .flat_map(|ring| {
                ring.iter()
                    .zip(ring.iter().cycle().skip(1))
                    .map(move |(a, b)| segment_distance(p, *a, *b))
            })
            .fold(f64::INFINITY, f64::min)
    }
}

/// Whether `p` is inside the ring `ring` (even and odd crossings).
fn in_ring(ring: &[Pt], p: Pt) -> bool {
    let mut inside = false;
    for (a, b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}

/// The distance from `p` to the segment from `a` to `b`.
fn segment_distance(p: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(Pt::new(a.x + t * dx, a.y + t * dy))
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
    /// The thermal reliefs among them that the fill joins by nothing ([`FillPad::unjoinable`]).
    #[serde(default)]
    pub unjoinable: u32,
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
        self.unjoinable += u32::from(pad.unjoinable.is_some());
    }

    fn sum(&mut self, other: &JoinCounts) {
        self.no_spokes += other.no_spokes;
        self.one_spoke += other.one_spoke;
        self.two_or_more_spokes += other.two_or_more_spokes;
        self.solid += other.solid;
        self.none += other.none;
        self.starved += other.starved;
        self.unjoinable += other.unjoinable;
    }

    /// How many pads in all.
    pub fn pads(&self) -> u32 {
        self.no_spokes + self.one_spoke + self.two_or_more_spokes + self.solid + self.none
    }

    fn text(&self) -> String {
        format!(
            "{} pad(s): {} by 2 or more spokes, {} by 1 spoke, {} by no spoke, {} solid, {} none; {} starved, {} unjoinable",
            self.pads(),
            self.two_or_more_spokes,
            self.one_spoke,
            self.no_spokes,
            self.solid,
            self.none,
            self.starved,
            self.unjoinable
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
    /// The fill's copper, piece by piece, each with its holes, as the Gerbers hold it (issue
    /// #1293): where a track from a pad reaches the fill.
    #[serde(default)]
    pub outline: Vec<FillPolygon>,
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

/// A thermal relief the fill joins by nothing, as the design rules check's `unjoinable_thermal`
/// names it, and why (issue #1222).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnjoinablePad {
    pub layer: String,
    pub net: String,
    pub part: String,
    pub pad: String,
    pub at: Pt,
    pub spokes: u32,
    pub why: String,
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
    /// Every thermal relief the fill joins by nothing.
    #[serde(default)]
    pub unjoinable: Vec<UnjoinablePad>,
}

impl FillReport {
    /// The report of `board` as it is, with whatever copper is on it: the fill of every zone of
    /// a net of its board file.
    pub fn of(board: &Board) -> Result<FillReport, CopperError> {
        let art = artwork(board)?;
        let rules = netsmith_geom::plot_rules_of(Some(&crate::export::kicad_pro(board)));
        let edge = netsmith_geom::edge_outline(&art).map_err(CopperError)?;
        let pt = |p: netsmith_geom::Pt| Pt::new(p.x, p.y);
        let island = |(at, area): (netsmith_geom::Pt, f64)| FillIsland {
            at: pt(at),
            area,
            outline: Vec::new(),
        };
        let removed = |(at, area): (netsmith_geom::Pt, f64), piece: netsmith_geom::PolygonArea| {
            let piece = crate::copper::area(piece);
            FillIsland {
                outline: std::iter::once(&piece.outer)
                    .chain(&piece.holes)
                    .map(|ring| crate::simplify::simplify_ring(ring, ISLAND_SIMPLIFY))
                    .filter(|ring| ring.len() >= 3)
                    .collect(),
                ..island((at, area))
            }
        };
        let polygon = |piece: netsmith_geom::PolygonArea| {
            let piece = crate::copper::area(piece);
            let ring = |r: &[Pt]| crate::simplify::simplify_ring(r, ISLAND_SIMPLIFY);
            let outer = ring(&piece.outer);
            (outer.len() >= 3).then(|| FillPolygon {
                outer,
                holes: piece
                    .holes
                    .iter()
                    .map(|h| ring(h))
                    .filter(|h| h.len() >= 3)
                    .collect(),
            })
        };
        let mut fills = Vec::new();
        // Every layer at once, as the design rules check judges them (issue #1292).
        for (layer, joins) in netsmith_geom::pad_joins(&art, &edge, &rules).map_err(CopperError)? {
            let layer = &layer;
            let copper = CopperLayer::of_name(layer);
            let word: &str = match copper {
                Some(c) => c.word(),
                None => layer,
            };
            for z in joins {
                let net = art.zones[z.zone].net.clone().unwrap_or_default();
                // A zone of the routes file restates a pour or a plane of its net; one of another
                // net leaves the pour drawn round it (issue #1314), and so does a patch of its own
                // net (issue #1608), which the board file raises above the pour, written at
                // priority 0 ([`Board::zone_lift`]): a fill above 0 is the patch's.
                let source = if board.replaced(layer, &net)
                    || art.zones[z.zone].priority > 0
                    || !board
                        .pours
                        .iter()
                        .any(|p| p.layer.name() == layer && p.net == net)
                        && !board
                            .planes
                            .iter()
                            .any(|p| p.layer == *layer && p.net == net)
                {
                    FillSource::Zone
                } else if matches!(copper, Some(CopperLayer::Inner(_))) {
                    FillSource::Plane
                } else {
                    FillSource::Pour
                };
                let mut counts = JoinCounts::default();
                let pads: Vec<FillPad> = z
                    .pads
                    .into_iter()
                    .map(|(p, join)| {
                        let own = board.pad_pour(&p.part, &p.number);
                        let mut pad = fill_pad(p.part, p.number, pt(p.at), join);
                        pad.stated_by = own.map(|o| o.path.clone());
                        pad.spoke_width = own.and_then(|o| o.spoke_width);
                        counts.add(&pad);
                        pad
                    })
                    .collect();
                fills.push(FillArea {
                    layer: word.to_string(),
                    layer_name: layer.clone(),
                    net,
                    source,
                    held: z.held,
                    area: z.fill.iter().map(|a| a.size()).sum(),
                    outline: z.fill.into_iter().filter_map(polygon).collect(),
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
                    removed_islands: z
                        .removed
                        .into_iter()
                        .zip(z.removed_fill)
                        .map(|(at, piece)| removed(at, piece))
                        .collect(),
                });
            }
        }
        let mut counts = JoinCounts::default();
        let mut starved = Vec::new();
        let mut unjoinable = Vec::new();
        for f in &fills {
            counts.sum(&f.counts);
            unjoinable.extend(f.pads.iter().filter_map(|p| {
                Some(UnjoinablePad {
                    layer: f.layer.clone(),
                    net: f.net.clone(),
                    part: p.part.clone(),
                    pad: p.pad.clone(),
                    at: p.at,
                    spokes: p.spokes.unwrap_or(0),
                    why: p.unjoinable.clone()?,
                })
            }));
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
            unjoinable,
        })
    }

    /// The report as text for a person: each fill with its counts, its pads, the pads it clears
    /// and its islands, then the counts of the board and the starved reliefs.
    pub fn text(&self) -> String {
        self.text_moved(Pt::new(0.0, 0.0), "board file")
    }

    /// [`FillReport::text`] with its places moved by `by`, mm, and the frame they are then in
    /// named `frame` (`outline`): the report's are in the board file's, and minus the outline's
    /// corner [`FillReport::outline_at`] is the outline's.
    pub fn text_moved(&self, by: Pt, frame: &str) -> String {
        let at = |p: Pt| format!("({:.3}, {:.3})", p.x + by.x, p.y + by.y);
        let mut out = format!(
            "{}: the fill of {} pour(s), plane(s) and zone(s), as the Gerbers hold it (places in mm in the {frame}'s frame; the outline's top left corner is at {})\n",
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
                    (JoinKind::Thermal, Some(0), _) => "thermal, no spokes".into(),
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
                let flag = match &p.unjoinable {
                    Some(why) => format!(" (unjoinable: {why})"),
                    None if p.starved => " (starved)".into(),
                    None if p.joined_elsewhere => " (joined by another fill of its net)".into(),
                    None => String::new(),
                };
                // A pad its pin joins otherwise than the pour (issue #1276).
                let own = match (&p.stated_by, p.spoke_width) {
                    (Some(pin), Some(w)) => format!(" (as its pin `{pin}` states, {w} mm spokes)"),
                    (Some(pin), None) => format!(" (as its pin `{pin}` states)"),
                    _ => String::new(),
                };
                out.push_str(&format!(
                    "  {} at {}: {how}{tracks}{own}{flag}\n",
                    pad_name(&p.part, &p.pad),
                    at(p.at),
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
                "starved thermal reliefs (one or more but fewer than {} spokes and tracks join the pad to the fill): {}\n",
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
        if self.unjoinable.is_empty() {
            out.push_str("thermal reliefs the fill joins by nothing: none\n");
        } else {
            out.push_str(&format!(
                "thermal reliefs the fill joins by nothing (no spoke and no track; the pad's own via or track joins it): {}\n",
                self.unjoinable.len()
            ));
            for u in &self.unjoinable {
                out.push_str(&format!(
                    "  {} on {} ({}) at {}: {}\n",
                    pad_name(&u.part, &u.pad),
                    u.layer,
                    u.net,
                    at(u.at),
                    u.why
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
    let unjoinable = join.unjoinable().map(ToString::to_string);
    let joined_elsewhere = matches!(
        join,
        netsmith_geom::PadJoin::Thermal {
            reach: 0,
            tracks: 0,
            elsewhere: true,
            ..
        }
    );
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
            unjoinable,
            joined_elsewhere,
            stated_by: None,
            spoke_width: None,
        },
        netsmith_geom::PadJoin::Thermal {
            spokes,
            reach,
            tracks,
            ..
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
            unjoinable,
            joined_elsewhere,
            stated_by: None,
            spoke_width: None,
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
            unjoinable,
            joined_elsewhere,
            stated_by: None,
            spoke_width: None,
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
            text.contains("starved thermal reliefs (one or more but fewer than 2"),
            "{text}"
        );
    }

    /// A zone of the routes file on F.Cu of `net` over `area`, with the pad connection `pads`
    /// and the priority `priority`.
    fn zone(
        net: &str,
        area: crate::geometry::Rect,
        pads: crate::board::ZonePads,
        priority: u32,
    ) -> crate::board::BoardZone {
        crate::board::BoardZone {
            patch: false,
            net: net.into(),
            layers: vec!["F.Cu".into()],
            outline: area.corners().to_vec(),
            holes: Vec::new(),
            priority,
            clearance: 0.2,
            min_width: 0.2,
            pads,
            thermal_gap: 0.3,
            spoke_width: 0.3,
            islands: crate::board::ZoneIslands::Remove,
            island_min_area: None,
        }
    }

    fn unjoinable_findings(board: &Board) -> usize {
        netsmith_geom::drc_check(
            &crate::kicad_pcb(board),
            Some(&crate::kicad_pro(board)),
            &Default::default(),
        )
        .unwrap()
        .findings
        .iter()
        .filter(|f| f.rule.id() == "unjoinable_thermal")
        .count()
    }

    /// The LED board with a GND pour on the top, thermal reliefs at its pads.
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

    /// The joins of the pour's fill on the top, but of `but`: each pad with how it is joined.
    fn pour_joins(report: &FillReport, but: (&str, &str)) -> Vec<(String, String, JoinKind, bool)> {
        let mut joins: Vec<_> = report
            .fills
            .iter()
            .filter(|f| f.layer == "top" && f.source == FillSource::Pour)
            .flat_map(|f| f.pads.iter())
            .filter(|p| (p.part.as_str(), p.pad.as_str()) != but)
            .map(|p| (p.part.clone(), p.pad.clone(), p.join, p.joined))
            .collect();
        joins.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        joins
    }

    #[test]
    fn a_patch_zone_joins_a_pad_solid_and_keeps_the_rest_of_the_pour() {
        // Issue #1608: a solid GND zone marked `patch` over one GND pad of a thermal GND pour
        // joins that pad solid, and the pour stays, with its other pads as they were; the same
        // zone without the mark replaces the pour on the layer (issue #1314).
        use crate::board::ZonePads;
        let mut board = poured_top();
        let before = FillReport::of(&board).unwrap();
        let pad = before
            .fills
            .iter()
            .filter(|f| f.layer == "top" && f.source == FillSource::Pour)
            .flat_map(|f| f.pads.iter())
            .find(|p| p.join == JoinKind::Thermal && p.joined)
            .unwrap_or_else(|| panic!("a GND pad the pour joins: {}", before.text()))
            .clone();
        let around = crate::geometry::Rect::new(
            Pt::new(pad.at.x - 1.2, pad.at.y - 1.2),
            Pt::new(pad.at.x + 1.2, pad.at.y + 1.2),
        );
        let mut patch = zone("GND", around, ZonePads::Solid, 0);
        patch.patch = true;
        board.zones.push(patch.clone());
        assert!(!board.replaced("F.Cu", "GND"));
        assert_eq!(board.zone_lift(), 1, "the patch is written above the pour");
        let report = FillReport::of(&board).unwrap();
        let joins: Vec<(FillSource, JoinKind, bool)> = report
            .fills
            .iter()
            .filter(|f| f.layer == "top")
            .flat_map(|f| f.pads.iter().map(move |p| (f.source, p)))
            .filter(|(_, p)| (p.part.as_str(), p.pad.as_str()) == (&pad.part, &pad.pad))
            .map(|(source, p)| (source, p.join, p.joined))
            .collect();
        assert!(
            joins.contains(&(FillSource::Zone, JoinKind::Solid, true)),
            "{joins:?}\n{}",
            report.text()
        );
        let but = (pad.part.as_str(), pad.pad.as_str());
        assert!(!pour_joins(&before, but).is_empty(), "{}", before.text());
        assert_eq!(
            pour_joins(&report, but),
            pour_joins(&before, but),
            "{}",
            report.text()
        );
        assert_eq!(report.counts.unjoinable, 0, "{}", report.text());

        // Without the mark the zone replaces the pour: no fill of the pour is left on the top.
        patch.patch = false;
        board.zones = vec![patch];
        assert!(board.replaced("F.Cu", "GND"));
        let replaced = FillReport::of(&board).unwrap();
        assert!(
            !replaced
                .fills
                .iter()
                .any(|f| f.layer == "top" && f.source == FillSource::Pour),
            "{}",
            replaced.text()
        );
    }

    #[test]
    fn a_pad_a_solid_zone_of_its_net_joins_is_not_unjoinable_in_the_zone_under_it() {
        // Issue #1292: a thermal GND zone over the board, and a solid GND zone of a higher
        // priority over one of its pads, which the lower zone gives its area: none of the lower
        // zone's spokes is on its fill, but the higher zone's fill joins the pad.
        use crate::board::ZonePads;
        let mut board = crate::board::tests::led_board();
        board.zones.push(zone(
            "GND",
            board.outline.expanded(-0.3),
            ZonePads::Thermal,
            0,
        ));
        let before = FillReport::of(&board).unwrap();
        let pad = before.fills[0]
            .pads
            .iter()
            .find(|p| p.join == JoinKind::Thermal && p.joined)
            .unwrap_or_else(|| panic!("a GND pad the zone joins: {}", before.text()))
            .clone();
        let around = crate::geometry::Rect::new(
            Pt::new(pad.at.x - 1.5, pad.at.y - 1.5),
            Pt::new(pad.at.x + 1.5, pad.at.y + 1.5),
        );
        board.zones.push(zone("GND", around, ZonePads::Solid, 1));
        let report = FillReport::of(&board).unwrap();
        let fills_of_the_pad: Vec<(&FillArea, &FillPad)> = report
            .fills
            .iter()
            .flat_map(|f| f.pads.iter().map(move |p| (f, p)))
            .filter(|(_, p)| (p.part.as_str(), p.pad.as_str()) == (&pad.part, &pad.pad))
            .collect();
        // The higher zone joins it solid; the lower one's relief reaches nothing but is not
        // unjoinable.
        assert!(
            fills_of_the_pad
                .iter()
                .any(|(_, p)| p.join == JoinKind::Solid && p.joined),
            "{}",
            report.text()
        );
        assert!(
            fills_of_the_pad
                .iter()
                .any(|(_, p)| p.join == JoinKind::Thermal && p.joined_spokes == Some(0)),
            "{}",
            report.text()
        );
        assert!(report.unjoinable.is_empty(), "{}", report.text());
        assert!(
            fills_of_the_pad.iter().any(|(_, p)| p.joined_elsewhere),
            "{}",
            report.text()
        );
        assert!(
            report
                .text()
                .contains("(joined by another fill of its net)"),
            "{}",
            report.text()
        );
        assert_eq!(report.counts.unjoinable, 0, "{}", report.text());
        // `board drc` (and `fab`, which runs it) judge the pad by the same join.
        assert_eq!(unjoinable_findings(&board), 0);
    }

    #[test]
    fn a_pad_is_counted_by_the_spokes_that_reach_the_fill() {
        let pad = |join| fill_pad("U1".into(), "2".into(), Pt::new(0.0, 0.0), join);
        let thermal = |spokes, reach, tracks| netsmith_geom::PadJoin::Thermal {
            spokes,
            reach,
            lone: 0,
            tracks,
            unjoinable: (reach == 0).then(|| netsmith_geom::Unjoinable::Blocked(Vec::new())),
            elsewhere: false,
        };
        let mut c = JoinCounts::default();
        for join in [
            thermal(4, 1, 0),
            thermal(4, 1, 1),
            thermal(4, 3, 0),
            thermal(0, 0, 0),
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
                starved: 1,
                unjoinable: 1,
            }
        );
    }
}
