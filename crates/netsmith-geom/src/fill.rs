//! Filling zones: the copper pours of a board (the planes of an inner layer and the pours of
//! an outer one), as KiCad's zone fill makes them, written into a Gerber layer.
//!
//! Each zone is filled in four steps, as KiCad fills it:
//!
//! 1. **The clearances.** The zone's outline less its holes (and, within the zone's minimum
//!    thickness of it, the outlines of the zones of its net that touch it, KiCad's apron, cut away
//!    again after step 3, so that no corner is rounded where two zones of a net meet: [`apron`],
//!    issue #1222), less everything the pour keeps away from: the outside of the board and a band along its edge, rule areas that forbid pours,
//!    the clearance round the copper of other nets (pads, vias, tracks, teardrops, drawings) and
//!    round holes, and the thermal gaps round the pads of its own net, drawn as the four pieces
//!    between the spokes so that the spokes stay copper. Where zones of one layer overlap, a
//!    zone also gives up what a zone of higher priority has ([`fill_zones`]). Every space but
//!    the thermal gap is 2.5 µm wider than the rule asks ([`MARGIN`], issue #1184), as KiCad's
//!    filler makes it, so that the polygons keep it in full.
//! 2. **Spokes KiCad would not make.** A spoke is kept only where KiCad's filler makes one
//!    (below): the others are taken out, the pad's own copper left.
//! 3. **The minimum thickness.** The copper narrower than the zone's `min_thickness` is taken
//!    out ([`crate::polygons::opened`]: shrunk by a little less than half the width, with square
//!    corners, and grown back, with round ones, as KiCad's filler does it), so a sliver goes and
//!    a neck narrower than it parts the copper in two, a spoke narrowed by a clearance with it.
//!    Copper exactly the minimum thickness wide stays ([`TRIM_SLACK`], issue #1222). Then the
//!    outline of every zone of the net with a higher priority is taken out, last, as KiCad takes
//!    it out (`subtractHigherPriorityZones`).
//! 4. **Copper islands.** A piece of the fill that touches no copper of the zone's net on the layer
//!    (a pad, a via, a track, a teardrop), nor a piece of another zone of the net that does, through
//!    any number of pieces that touch ([`isolated`], issue #1222), is removed, as the zone's
//!    `island_removal_mode` says ([`crate::artwork::Islands`]: always, the default,
//!    never, or below `island_area_min`). A zone of no net keeps every piece, as KiCad's does.
//!
//! Each piece is then written as one dark region with its holes cleared; the caller draws every
//! pad, via and track dark again after it.
//!
//! The polygons are worked out with [`crate::polygons::Polygons`], and the fill is written to
//! any [`Pen`]: the Gerber file, or `Polygons` again. `netsmith_pcb::copper_of` fills a board's
//! pours that way for the 3D view (ADR 0105), so its pours are this fill and there is one filler.
//!
//! The gap and spoke width come from the zone (the width no wider than the pad, below); the
//! spokes' turn is the pad's own `thermal_bridge_angle`, else KiCad's default: 45 degrees from
//! the pad's axes for a round pad or a custom pad with a round anchor, along the axes for every
//! other shape (KiCad's board reader, `parsePAD`, the same in 9.0.0 and 9.0.5; a file older than
//! KiCad 6, version 20211014 or earlier, gave a custom pad with a round anchor 90 degrees, and
//! netsmith does not read the file's version and gives it 45). netsmith once drew every spoke
//! along the axes, so its planes differed from the ones KiCad fills round every round pad (0.64%
//! of the pixels of a small plane, 1.2% with more pads). Neither angle is better for the factory
//! (JLCPCB's page lists no rule about spokes), and matching KiCad costs nothing, so the file is
//! the same as the one the designer sees (`crates/netsmith-fab/tests/known_differences.rs`). A via
//! of the pour's net is joined solid, as KiCad joins it. Fills the file already holds
//! (`filled_polygon`) are used as they are; the fill netsmith writes into the zones of the KiCad
//! board it exports ([`zone_fills`], issue #1184) is not one of them when netsmith reads that
//! file again (`crate::artwork::Zone::filled`), so it is made again here, the same.
//!
//! **Spokes (issue #1115).** netsmith makes the spokes KiCad 9's filler makes
//! (`ZONE_FILLER::buildThermalSpokes` and `fillCopperZone`), so that its design rules check counts
//! the `starved_thermal` pads KiCad's refill leaves, and the router keeps a pad's own via where
//! the pour would join the pad by one spoke or none (`netsmith_pcb::starved_pour_pads`):
//!
//! - A spoke is as wide as the zone's `thermal_bridge_width`, but no wider than the pad's narrower
//!   side; where that is narrower than the zone's minimum thickness the pad gets no spokes and
//!   its whole gap is cleared ([`spoke_width`]).
//! - A spoke runs from the pad's centre to a test point on its axis: where the pad's bounding box
//!   in its own frame, grown by the gap, KiCad's `m_MaxError` (0.005 mm) and half the minimum
//!   thickness, meets the axis (on the circle, for a round pad).
//! - It is made where the test point is on the fill as it is before any spoke is added (every
//!   relief cleared whole, every clearance taken out) and trimmed to the minimum thickness (with
//!   square corners both ways, KiCad's `CHAMFER_ALL_CORNERS` there), or
//!   where it lies in another spoke whose own test point lies in it (two pads of the net that
//!   face each other). So a spoke whose end only a strip of fill thinner than the minimum
//!   thickness reaches is not made: the strip goes with the trimming, and the spoke would be a
//!   stub that joins nothing. netsmith once tried a point 0.01 mm beyond the gap against the
//!   fill with the spokes in it and trimmed, where the spoke's own copper kept that strip, and
//!   counted such stubs (three spokes on a pad KiCad joins by one).
//!
//! **The pads of a pour (issue #1187).** [`pad_joins`] says, of the same fill, how each zone
//! joins each pad of its net in its outline ([`Join`]: solid, a thermal relief with the spokes
//! that reach the fill, or not at all), which pads of other nets it keeps clear of, and which
//! copper islands it keeps and which it took out: `netsmith board fill` prints it
//! (`netsmith_pcb::FillReport`). The spokes are counted as the design rules check counts them
//! for `starved_thermal`, by one code ([`judge`]), so the two cannot differ. They are counted as
//! KiCad 9's check counts them (issue #1222, [`starved`]): a spoke onto a piece of the fill that
//! joins nothing but the pad counts only when nothing else joins it, and a track of the pad's net
//! counts when one end is in the pad grown by half the gap and the other on the fill
//! ([`tracks_onto`]). On a routed board of four layers, KiCad's check of the fill netsmith writes
//! into the zones then finds the starved reliefs `board fill` finds, and no isolated copper, copper
//! sliver or clearance in it.
//!
//! **A pad no spoke joins (issue #1222).** Where the fill makes none of a relief's spokes, the
//! relief says why ([`Unjoinable`]): its spokes are narrower than the minimum thickness, or what
//! keeps the fill off the end of each ([`Blocker`]: another net's clearance by name, a thermal gap
//! of another pad of the net, the edge, a keepout, a zone of another net, or a strip narrower than
//! the minimum thickness). On a routed board of four layers this was most pads of its ground pours
//! that no spoke joins, and KiCad's refill makes none of their spokes either; KiCad's check leaves
//! such a pad to its connection check, where netsmith's check calls it `unjoinable_thermal`.
//! A pad that another fill of its net joins is not one ([`judge_board`], issue #1292): a zone of
//! the net of a higher priority over it, whose area the lower zone gives up after its spokes are
//! made, so that none of them is on the lower zone's fill, or a fill on another layer of a pad
//! with a hole. Both the check and [`pad_joins`] judge every layer at once by that function.
//!
//! **Where it differs from KiCad.** Copper islands are removed once every zone of the layer is
//! filled, and a zone of lower priority keeps away from a higher one's fill as it was before its
//! islands went, so it does not grow into the room an island leaves. A piece is an island by the
//! copper it touches on its layer, where KiCad follows the copper through vias to a pad on any
//! layer: a piece joined only to a via or a track that reaches no pad is kept here. The apron is
//! not given up where zones of other nets with a higher priority enclose the zone of the net it
//! touches (KiCad's wrinkle of its issue 13915). A spoke of a custom pad runs
//! along the pad's axes from its centre, where KiCad uses the spokes the pad's own primitives
//! draw, if it has any, and the centre of its bounding box. The trimming does not drop a piece
//! less than twice the minimum thickness across, nor join nearby pieces before it grows the copper
//! back, as KiCad's does. KiCad cuts the clearance round a curve outside it by up to its
//! `m_MaxError` (5 µm), and netsmith inside it by [`geom::ARC_ERROR`] and [`MARGIN`] beyond, so
//! where a strip of fill or a spoke's end is within a few micrometres of the minimum thickness
//! between round copper, one joins a spoke the other does not: on the routed board above, at each
//! of nine settings of the spoke width, the gap and the minimum thickness, netsmith's fill and
//! KiCad's refill made the same spokes of 139 reliefs but for at most three pads, where netsmith
//! made one more, and one pad beside a keepout of a footprint, which the board file netsmith writes
//! puts elsewhere for KiCad (issue #1237).

use crate::artwork::{Artwork, Connect, Islands, Pad, PadKind, PadShape, Shape, TrackShape, Zone};
use crate::edge::Outline as BoardEdge;
use crate::gbr::{Contour, Pen};
use crate::geom::{self, Arc, Bounds, Pt};
use crate::pads::{self, Outline};
use crate::plot::Rules;
use crate::polygons::{
    Area, Corners, Polygons, difference, grown, intersection, opened, overlap, trimmed, union,
};

/// KiCad's clearance when a zone gives none.
const DEFAULT_CLEARANCE: f64 = 0.2;

mod cut;
pub(crate) use cut::{By, cuts};

/// Whether a via joins a pour with thermal spokes (as a pad does) rather than solid copper:
/// KiCad joins it solid (found by the cross-check, ADR 0034).
const VIA_THERMAL: bool = false;

/// How much more than its clearance the fill keeps from what it keeps clear of (other nets'
/// copper, holes, the board's edge, rule areas that forbid pours, a zone of another net with a
/// higher priority), in mm (issue #1184): the most a curve cut into straight pieces lies inside
/// it ([`geom::ARC_ERROR`]), and the 0.5 µm KiCad 9's filler adds to every clearance (its
/// advanced setting `ExtraClearance`, 0.0005 mm by default). A clearance round a round pad, a via
/// or a track's end is cut as a polygon inside its circle, and trimming to the minimum thickness
/// rounds the corner of a hole (a rule area's) into it, each by up to 2 µm; so the copper keeps
/// every clearance and stays out of every rule area in full, as KiCad's fill, which cuts outside
/// the curve, does. KiCad's check of the fill the board file holds then finds no clearance short
/// by 2 µm, and no solder mask opening that ends where the clearance does (a fiducial's)
/// touching the fill. The fill's own edges, where it meets another zone of its net, stay.
const MARGIN: f64 = geom::ARC_ERROR + 0.0005;

/// KiCad 9's own extra clearance, its advanced setting `ExtraClearance` (0.0005 mm by default), in
/// mm: the part of [`MARGIN`] KiCad's filler adds too.
const KICAD_EXTRA_CLEARANCE: f64 = 0.0005;

/// How much less than half the minimum thickness the fill is shrunk and grown back by when it is
/// trimmed (step 3 and the spokes' test), in mm (issue #1222): the micrometre KiCad's filler takes
/// off (`fillCopperZone`'s `epsilon`, so that copper exactly the minimum thickness wide stays, a
/// spoke as wide as the minimum thickness among it), and half the 2 µm by which [`MARGIN`] cuts a
/// clearance along a straight edge deeper than KiCad's filler does, so that a spoke or a strip one
/// such clearance shaves is kept, as KiCad keeps it. Without it, a spoke as wide as the zone's
/// minimum thickness was trimmed away whole, so a narrower spoke starved more pads than a wider
/// one. Not the whole 2 µm: KiCad cuts round a curve up to 5 µm deeper than along a straight edge
/// (its `m_MaxError`, outside the curve), and with it netsmith kept, between a clearance and the
/// round end of a track, a neck of fill that KiCad trims away.
const TRIM_SLACK: f64 = 0.001 + (MARGIN - KICAD_EXTRA_CLEARANCE) / 2.0;

/// How far the fill of a zone is shrunk and grown back to trim it to its minimum thickness
/// ([`TRIM_SLACK`] under half of it), in mm; none where that is under KiCad's micrometre.
fn trim_by(z: &Zone) -> f64 {
    let by = z.min_thickness / 2.0 - TRIM_SLACK;
    if by > 0.001 { by } else { 0.0 }
}

/// How far a piece of another zone of the net is grown to count as touching an island, in mm:
/// two zones of one net that meet share an edge and no area.
const TOUCH: f64 = 0.001;

/// The fill of every zone of `layer` that holds no fill of its own in the file, by the zone's
/// index in `art.zones`, in the four steps of the module docs: what [`pour`] draws, and what
/// the KiCad export writes into its zones (`filled_polygon`, issue #1184). A zone whose fill
/// is empty is listed with no areas.
pub fn zone_fills(
    art: &Artwork,
    layer: &str,
    edge: &BoardEdge,
    rules: &Rules,
) -> Result<Vec<(usize, Vec<Area>)>, String> {
    let index: Vec<usize> = art
        .zones
        .iter()
        .enumerate()
        .filter(|(_, z)| {
            z.keepout.is_none()
                && z.layers.iter().any(|l| l == layer)
                && !z.filled.iter().any(|(l, _)| l == layer)
        })
        .map(|(i, _)| i)
        .collect();
    if index.is_empty() {
        return Ok(Vec::new());
    }
    let zones: Vec<&Zone> = index.iter().map(|&i| &art.zones[i]).collect();
    Ok(fill_zones(art, layer, edge, rules, &zones)?
        .into_iter()
        .map(|f| (index[f.zone], f.fill))
        .collect())
}

/// Writes the fill of every zone of `layer` that holds no fill of its own in the file.
pub fn pour(
    g: &mut dyn Pen,
    art: &Artwork,
    layer: &str,
    edge: &BoardEdge,
    rules: &Rules,
) -> Result<(), String> {
    let filled = zone_fills(art, layer, edge, rules)?;
    // Each piece of copper as one dark region with its holes cleared, the pieces with the
    // largest box first, so that clearing a hole never takes copper drawn before it (a piece
    // inside a hole is smaller than the piece round it).
    let mut pieces: Vec<(f64, &Zone, &Area)> = Vec::new();
    for (i, areas) in &filled {
        for a in areas {
            let size = Bounds::of(a.outer.iter().copied())
                .map_or(0.0, |b| (b.max.x - b.min.x) * (b.max.y - b.min.y));
            pieces.push((size, &art.zones[*i], a));
        }
    }
    pieces.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, z, a) in pieces {
        g.polarity(true);
        g.region(
            Contour::polygon(&a.outer),
            Some("Conductor"),
            &net_attr(z.net.as_deref()),
        );
        if !a.holes.is_empty() {
            g.polarity(false);
            for h in &a.holes {
                g.region(Contour::polygon(h), None, &[]);
            }
        }
    }
    g.polarity(true);
    Ok(())
}

/// Clears the rule areas of `layer` that forbid pours, each grown by [`MARGIN`].
fn keep_out_of_rule_areas(g: &mut dyn Pen, art: &Artwork, layer: &str) {
    for k in art.zones.iter().filter(|z| {
        z.keepout.as_ref().is_some_and(|k| k.copperpour) && z.layers.iter().any(|l| l == layer)
    }) {
        for o in k.outlines.iter().filter(|o| o.len() >= 3) {
            g.region(Contour::polygon(o), None, &[]);
            let mut ring = o.clone();
            ring.push(o[0]);
            for w in ring.windows(2) {
                stroke(g, w[0], w[1], 2.0 * MARGIN);
            }
        }
    }
}

/// A zone's clearance, KiCad's default when it gives none.
fn clearance_of(z: &Zone) -> f64 {
    if z.clearance > 0.0 {
        z.clearance
    } else {
        DEFAULT_CLEARANCE
    }
}

/// The clearance a zone keeps from copper of other nets: its own, or its net's class's if
/// that is larger (issue #758). Copper of a net whose class keeps more keeps that.
fn zone_clearance(z: &Zone, rules: &Rules) -> f64 {
    clearance_of(z).max(rules.class_clearance(z.net.as_deref()))
}

/// A zone's outline less its holes.
pub(crate) fn outline_area(z: &Zone) -> Vec<Area> {
    let mut p = Polygons::new();
    for o in z.outlines.iter().filter(|o| o.len() >= 3) {
        p.region(Contour::polygon(o), None, &[]);
    }
    p.polarity(false);
    for h in z.holes.iter().filter(|h| h.len() >= 3) {
        p.region(Contour::polygon(h), None, &[]);
    }
    p.areas()
}

/// Whether the outlines of two zones, less their holes, share an area: the overlap that
/// [`fill_zones`] refuses between two zones of different nets with one priority, and that the
/// `.kicad_pcb` writer looks for before it orders zones of one priority (`netsmith_pcb::export`).
pub fn zones_overlap(a: &Zone, b: &Zone) -> bool {
    overlap(&outline_area(a), &outline_area(b))
}

/// The fill of each zone of a layer (by its index in `zones`), in the four steps of the module
/// docs. Where zones overlap, KiCad's rule holds: from the highest priority down, a zone gives
/// up the outline of every zone of its net with a higher priority (that zone's settings hold
/// there), and the fill of every zone of another net with a higher priority, grown by the larger
/// of the two zones' clearances. Of two zones of one net with the same priority, the one first
/// in the file is taken as the higher; two zones of different nets with the same priority whose
/// outlines overlap are refused, since which one has the area would be KiCad's choice by an
/// identifier (the zone's uuid), not the design's.
fn fill_zones(
    art: &Artwork,
    layer: &str,
    edge: &BoardEdge,
    rules: &Rules,
    zones: &[&Zone],
) -> Result<Vec<Filled>, String> {
    let outlines: Vec<Vec<Area>> = zones.iter().map(|z| outline_area(z)).collect();
    for (i, a) in zones.iter().enumerate() {
        for (j, b) in zones.iter().enumerate().skip(i + 1) {
            if a.net != b.net && a.priority == b.priority && overlap(&outlines[i], &outlines[j]) {
                return Err(format!(
                    "two zones on {layer} overlap, are on different nets ({} and {}) and have the same priority ({}); give the one that is to have the area the higher priority",
                    a.net.as_deref().unwrap_or("no net"),
                    b.net.as_deref().unwrap_or("no net"),
                    a.priority
                ));
            }
        }
    }
    // The highest priority first; of equal ones, the one first in the file.
    let mut order: Vec<usize> = (0..zones.len()).collect();
    order.sort_by(|&a, &b| zones[b].priority.cmp(&zones[a].priority).then(a.cmp(&b)));
    let mut filled: Vec<(usize, Vec<Area>)> = Vec::new();
    for &i in &order {
        let z = zones[i];
        // Everything the fill keeps away from, drawn dark, then taken out of the zone's area.
        let mut p = Polygons::new();
        keep_out_of_the_edge(&mut p, art, edge, rules.edge_clearance + MARGIN, &[z]);
        keep_out_of_rule_areas(&mut p, art, layer);
        around_copper(&mut p, art, layer, z, rules)?;
        let mut fill = difference(&apron(art, layer, z, &outlines[i]), &p.areas());
        let mut taken: Vec<Area> = Vec::new();
        // The outlines of the zones of the net with a higher priority, which KiCad takes out of
        // the fill last, after the spokes and the trimming (`subtractHigherPriorityZones`).
        let mut own: Vec<Area> = Vec::new();
        for (j, higher) in &filled {
            let h = zones[*j];
            if h.net == z.net {
                own.extend(outlines[*j].iter().cloned());
            } else {
                taken.extend(grown(
                    higher,
                    zone_clearance(z, rules).max(zone_clearance(h, rules)) + MARGIN,
                ));
            }
        }
        if !taken.is_empty() {
            fill = difference(&fill, &taken);
        }
        let fill = without_dead_spokes(fill, art, layer, z);
        let mut fill = opened(&fill, trim_by(z), Corners::Square, Corners::Round);
        fill = intersection(&fill, &outlines[i]);
        if !own.is_empty() {
            fill = difference(&fill, &own);
        }
        filled.push((i, fill));
    }
    let all: Vec<(&Zone, &[Area])> = filled
        .iter()
        .map(|(i, fill)| (zones[*i], fill.as_slice()))
        .collect();
    let kept: Vec<(Vec<Area>, Vec<Area>)> = filled
        .iter()
        .zip(isolated(&all, art, layer))
        .map(|((i, fill), isolated)| without_islands(fill, zones[*i], isolated))
        .collect();
    Ok(filled
        .into_iter()
        .zip(kept)
        .map(|((zone, _), (fill, removed))| Filled {
            zone,
            fill,
            removed,
        })
        .collect())
}

/// The area a zone is filled in before it is trimmed to its own outline (`outline`): its outline,
/// and within its minimum thickness of it, the outlines of the zones of its net on `layer` that
/// touch it, as KiCad fills a zone (`ZONE::BuildSmoothedPoly`, its apron). So the trimming to the
/// minimum thickness rounds no corner where two zones of a net meet, and leaves no notch between
/// their fills there, with a spike of copper beside it that KiCad's check calls a copper sliver
/// (issue #1222); the fill is cut back to the outline after the trimming.
fn apron(art: &Artwork, layer: &str, z: &Zone, outline: &[Area]) -> Vec<Area> {
    let touching = grown(outline, TOUCH);
    let same: Vec<Area> = art
        .zones
        .iter()
        .filter(|o| {
            !std::ptr::eq(*o, z)
                && o.keepout.is_none()
                && !o.teardrop
                && o.net.is_some()
                && o.net == z.net
                && o.layers.iter().any(|l| l == layer)
        })
        .map(outline_area)
        .filter(|o| overlap(&touching, o))
        .flatten()
        .collect();
    if same.is_empty() || z.min_thickness <= 0.0 {
        return outline.to_vec();
    }
    let mut all = outline.to_vec();
    all.extend(same);
    intersection(&union(&all), &grown(outline, z.min_thickness))
}

/// The fill of one zone ([`fill_zones`]).
struct Filled {
    /// The zone, by its index in the zones given.
    zone: usize,
    /// Its pieces of copper.
    fill: Vec<Area>,
    /// The copper islands the zone's island setting took out of it (step 4).
    removed: Vec<Area>,
}

/// How a pad of the zone's net joins the zone: the pad's own setting, else the zone's, with
/// `thru_hole_only` made thermal or solid by the pad's kind.
fn connection(pad: &Pad, z: &Zone) -> Connect {
    match pad.zone_connect.unwrap_or(z.connect) {
        Connect::ThruHoleOnly if pad.kind == PadKind::Smd => Connect::Solid,
        Connect::ThruHoleOnly => Connect::Thermal,
        other => other,
    }
}

/// Whether a pad is of the zone's net (a zone of no net has no pads of its own).
fn own_net(z: &Zone, net: &Option<String>) -> bool {
    z.net.is_some() && *net == z.net
}

/// The fill less each thermal spoke of the zone that KiCad's filler would not make (step 2),
/// before the fill is trimmed to its minimum thickness, as KiCad decides it
/// (`ZONE_FILLER::fillCopperZone`, KiCad 9): a spoke is made where its test point
/// ([`Spoke::end`]) is on the fill with every relief cleared whole and with the copper
/// narrower than the minimum thickness trimmed off, or where it lies in another spoke whose own
/// test point lies in it (two pads of the net that face each other). The spoke's own copper
/// does not count, so a spoke whose end only a thin strip of fill reaches is not made.
fn without_dead_spokes(fill: Vec<Area>, art: &Artwork, layer: &str, z: &Zone) -> Vec<Area> {
    if fill.is_empty() || z.net.is_none() {
        return fill;
    }
    let Some(area) = zone_bounds(z).map(|b| b.expanded(z.thermal_gap.max(0.0))) else {
        return fill;
    };
    // Each pad's spokes, with the pad's outline.
    let mut reliefs: Vec<(Outline, Vec<Spoke>)> = Vec::new();
    for pad in art.pads.iter().filter(|p| {
        own_net(z, &p.net) && art.pad_copper_on(p, layer) && connection(p, z) == Connect::Thermal
    }) {
        let shape = pads::outline(pad);
        if !shape.bounds().overlaps(&area) {
            continue;
        }
        let spokes = spokes(pad, z);
        if !spokes.is_empty() {
            reliefs.push((shape, spokes));
        }
    }
    if reliefs.is_empty() {
        return fill;
    }
    let all: Vec<&Spoke> = reliefs.iter().flat_map(|(_, s)| s).collect();
    // KiCad's test area: the fill with the reliefs cleared whole (the spokes taken out), trimmed
    // to the minimum thickness with square corners both ways, as KiCad trims it there
    // (`CHAMFER_ALL_CORNERS`, issue #1222). Only the fill near a test point can decide it
    // (trimming keeps a point where the minimum thickness round it fits, its square corners
    // reaching less than one and a half times that), so it is worked out in a square round each
    // test point, wide enough that its sides decide nothing.
    let reach = 1.5 * z.min_thickness.max(0.0) + 0.1;
    let windows: Vec<Area> = all
        .iter()
        .map(|s| {
            Area::of(&[
                Pt::new(s.end.x - reach, s.end.y - reach),
                Pt::new(s.end.x + reach, s.end.y - reach),
                Pt::new(s.end.x + reach, s.end.y + reach),
                Pt::new(s.end.x - reach, s.end.y + reach),
            ])
        })
        .collect();
    let spoke_copper: Vec<Area> = all.iter().map(|s| Area::of(&s.gap)).collect();
    let near = intersection(&fill, &windows);
    let test = opened(
        &difference(&near, &spoke_copper),
        trim_by(z),
        Corners::Square,
        Corners::Square,
    );
    let on_test = |p: Pt| test.iter().any(|a| a.contains(p));
    let lives = |s: &Spoke| {
        on_test(s.end)
            || all.iter().any(|o| {
                !std::ptr::eq(*o, s)
                    && geom::contains(&o.whole, s.end)
                    && geom::contains(&s.whole, o.end)
            })
    };
    let mut dead: Vec<Area> = Vec::new();
    // What a dead spoke's cut must leave: the pads it leaves and the live spokes. A spoke runs
    // from the pad's centre, so a dead spoke can cover the root of a live one beside the pad.
    let mut kept: Vec<Area> = Vec::new();
    for (shape, spokes) in &reliefs {
        let live: Vec<bool> = spokes.iter().map(lives).collect();
        if live.iter().all(|&l| l) {
            continue;
        }
        for (s, &l) in spokes.iter().zip(&live) {
            if l {
                kept.push(Area::of(&s.gap));
            } else {
                dead.push(Area::of(&s.gap));
            }
        }
        if live.contains(&true)
            && let Some(p) = shape.polygon()
        {
            kept.push(Area::of(&p));
        }
    }
    if dead.is_empty() {
        return fill;
    }
    // The dead spokes less the pads they leave and the live spokes, which stay copper.
    let cut = difference(&dead, &kept);
    difference(&fill, &cut)
}

/// The fill less its copper islands (step 4): the pieces that touch no copper of the zone's net on
/// `layer` and none of `others` (the pieces of the other zones of the net on the layer), as the
/// zone's island setting says. The pieces kept, then the islands taken out.
fn without_islands(fill: &[Area], z: &Zone, isolated: Vec<bool>) -> (Vec<Area>, Vec<Area>) {
    if z.islands == Islands::Keep {
        return (fill.to_vec(), Vec::new());
    }
    let (kept, removed): (Vec<_>, Vec<_>) =
        fill.iter().zip(isolated).partition(|(piece, isolated)| {
            !isolated || matches!(z.islands, Islands::RemoveBelow(min) if piece.size() >= min)
        });
    let pieces = |all: Vec<(&Area, bool)>| all.into_iter().map(|(a, _)| a.clone()).collect();
    (pieces(kept), pieces(removed))
}

/// A piece of copper with its box.
type Boxed = (Area, Option<Bounds>);

/// For each fill of `layer` (a zone and its pieces), whether each of its pieces is a copper island:
/// it touches no copper of the zone's net on the layer (a pad, a via, a track, a teardrop), and no
/// piece of another fill of the net that is joined so, however many pieces of fills that touch
/// lie between (issue #1222): two pieces of two zones of a net that touch only each other are both
/// islands, as KiCad's connectivity finds them, where netsmith once kept both. A zone of no net
/// has no islands, as KiCad's has none.
fn isolated(fills: &[(&Zone, &[Area])], art: &Artwork, layer: &str) -> Vec<Vec<bool>> {
    // Each piece of every fill, by its fill and its place in it.
    let pieces: Vec<(usize, &Area, Option<Bounds>)> = fills
        .iter()
        .enumerate()
        .flat_map(|(f, (_, fill))| {
            fill.iter()
                .map(move |a| (f, a, Bounds::of(a.outer.iter().copied())))
        })
        .collect();
    let net_of = |f: usize| fills[f].0.net.as_deref();
    // The copper of each net on the layer, with its boxes.
    let mut copper: Vec<(&str, Vec<Boxed>)> = Vec::new();
    for (z, _) in fills {
        if let Some(net) = z.net.as_deref()
            && !copper.iter().any(|(n, _)| *n == net)
        {
            let areas = net_copper(art, layer, net)
                .into_iter()
                .map(|a| {
                    let b = Bounds::of(a.outer.iter().copied());
                    (a, b)
                })
                .collect();
            copper.push((net, areas));
        }
    }
    let touches = |piece: &Area,
                   b: &Option<Bounds>,
                   others: &mut dyn Iterator<Item = (&Area, &Option<Bounds>)>| {
        let Some(b) = b else { return false };
        let near: Vec<Area> = others
            .filter(|(_, ob)| ob.is_some_and(|ob| ob.expanded(TOUCH).overlaps(b)))
            .map(|(a, _)| a.clone())
            .collect();
        !near.is_empty() && overlap(std::slice::from_ref(piece), &grown(&near, TOUCH))
    };
    // A piece is joined when it touches copper of its net, or a piece of another fill of its net
    // that is joined (issue #1222), as KiCad's connectivity joins the pieces of zones that touch.
    let mut joined: Vec<bool> = pieces
        .iter()
        .map(|(f, piece, b)| match net_of(*f) {
            None => true,
            Some(net) => copper
                .iter()
                .find(|(n, _)| *n == net)
                .is_some_and(|(_, areas)| {
                    let Some(b) = b else { return false };
                    let near: Vec<Area> = areas
                        .iter()
                        .filter(|(_, ab)| ab.is_some_and(|ab| ab.overlaps(b)))
                        .map(|(a, _)| a.clone())
                        .collect();
                    !near.is_empty() && overlap(std::slice::from_ref(*piece), &near)
                }),
        })
        .collect();
    let mut grew = true;
    while grew {
        grew = false;
        for k in 0..pieces.len() {
            if joined[k] {
                continue;
            }
            let (f, piece, b) = &pieces[k];
            let mut others = pieces
                .iter()
                .zip(&joined)
                .filter(|((g, _, _), j)| **j && *g != *f && net_of(*g) == net_of(*f))
                .map(|((_, a, ob), _)| (*a, ob));
            if touches(piece, b, &mut others) {
                joined[k] = true;
                grew = true;
            }
        }
    }
    let mut out: Vec<Vec<bool>> = fills.iter().map(|_| Vec::new()).collect();
    for ((f, _, _), j) in pieces.iter().zip(joined) {
        out[*f].push(!j);
    }
    out
}

/// What the design rules check reads of the fill of one zone on one layer (issues #838, #993):
/// the fill, the thermal reliefs of the pads in it and its copper islands.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Judged {
    /// The zone, by its index in the board's zones.
    pub zone: usize,
    /// Whether the fill is the one the board file holds (`filled_polygon`), rather than one
    /// worked out here.
    pub held: bool,
    /// Each pad of the zone's net that the zone joins with thermal spokes and whose centre is
    /// in the zone's outline.
    pub reliefs: Vec<Relief>,
    /// Each piece of the fill that touches no copper of the zone's net: a corner of it, and
    /// its area in mm². Of a fill worked out here, only the pieces the zone's
    /// `island_removal_mode` keeps (the others are gone); of a fill the file holds, every one,
    /// since that copper is made as it is.
    pub islands: Vec<(Pt, f64)>,
    /// The copper islands the fill took out, as the zone's `island_removal_mode` says: a corner
    /// of each, and its area in mm². None of a fill the file holds.
    pub removed: Vec<(Pt, f64)>,
    /// The pieces of copper of `removed`, in its order: what a picture outlines (issue #1188).
    pub removed_fill: Vec<Area>,
    /// Each copper sliver of the fill ([`slivers`]), when the check asked for them: a corner of
    /// it and its area in mm².
    pub slivers: Vec<(Pt, f64)>,
    /// Each corner of the fill, with the copper of its net on the layer, that comes to a point
    /// sharper than KiCad's copper sliver allows ([`sharp_corners`]), when the check asked for
    /// slivers (issue #1222).
    pub spikes: Vec<Pt>,
    /// The fill itself.
    pub fill: Vec<Area>,
}

/// A piece of copper by a corner of it and its area in mm², as the check places it.
fn piece_at(piece: &Area) -> Option<(Pt, f64)> {
    Some((*piece.outer.first()?, piece.size()))
}

/// A thermal relief of a pad in a zone, as the fill leaves it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Relief {
    /// The pad, by its index in the board's pads.
    pub pad: usize,
    /// How many spokes the relief has, and how many of them reach the fill: the fill has the
    /// spoke's copper across the thermal gap and its outer end.
    pub spokes: usize,
    pub reach: usize,
    /// How many of the spokes that reach the fill reach a piece of it that joins nothing but the
    /// pad, which KiCad's check counts only when nothing else joins the pad ([`starved`]).
    pub lone: usize,
    /// How many tracks of the zone's net on the layer run from the pad onto the fill, which
    /// join the pad to the pour as a spoke does ([`tracks_onto`]).
    pub tracks: usize,
    /// Of a relief no spoke of which reaches the fill: why ([`Unjoinable`]).
    pub unjoinable: Option<Unjoinable>,
    /// Whether another fill of the pad's net joins the pad, on this layer or another
    /// ([`judge_board`], issue #1292): a zone of the net of a higher priority that joins it
    /// solid, or whose spokes reach it. Such a pad is not [`unjoinable`].
    pub elsewhere: bool,
}

impl Relief {
    /// Whether the fill joins the relief by nothing and no other fill of its net joins the pad
    /// ([`unjoinable`]): the design rules check's `unjoinable_thermal`.
    pub(crate) fn unjoined(&self) -> bool {
        unjoinable(self.reach, self.tracks, self.elsewhere)
    }

    /// Whether this fill joins the pad at all, by a spoke or a track.
    fn joins(&self) -> bool {
        self.reach + self.tracks > 0
    }
}

/// The fewest spokes (or tracks of the pad's net onto the fill) that join a thermal relief to
/// its pour: KiCad 9's default for its `min_resolved_spokes` constraint (the implicit rule its
/// design rules engine adds, `drc_engine.cpp`). Fewer is a `starved_thermal`.
pub const MIN_SPOKES: usize = 2;

/// A pad a pour of its net joins with thermal spokes, and how many of them reach the fill
/// (issue #1115), as the design rules check counts them for `starved_thermal`.
#[derive(Debug, Clone, PartialEq)]
pub struct ThermalJoin {
    /// The pad's footprint's designator and the pad's number, and its centre.
    pub part: String,
    pub number: String,
    pub at: Pt,
    /// How many spokes the relief has (none where the pad is too narrow for one), how many of
    /// them reach the fill, and how many tracks of the pad's net run from it onto the fill.
    pub spokes: usize,
    pub reach: usize,
    /// How many of those reach a piece of the fill that joins nothing but the pad.
    pub lone: usize,
    pub tracks: usize,
}

impl ThermalJoin {
    /// Whether the pour joins the pad by fewer than [`MIN_SPOKES`] spokes and tracks: a starved
    /// relief ([`starved`]) or one it joins by nothing ([`unjoinable`]). The router keeps such a
    /// pad's own via and track.
    pub fn starved(&self) -> bool {
        starved(self.reach, self.lone, self.tracks) || unjoinable(self.reach, self.tracks, false)
    }
}

/// Whether a relief with `reach` spokes on the fill (`lone` of them onto a piece of it that joins
/// nothing but the pad) and `tracks` tracks of the pad's net onto it is starved, as KiCad 9's
/// `starved_thermal` counts it (`drc_test_provider_zone_connections.cpp`): one or more spokes,
/// and fewer than [`MIN_SPOKES`] spokes onto copper that joins something else, with the tracks;
/// where neither joins the pad, the lone spokes are counted instead. A relief no spoke of which reaches the fill is not starved but
/// [`unjoinable`] when no track joins it either: KiCad leaves a pad with no spoke to its
/// connection check (`unconnected_items`), and netsmith's connection check joins every pad in a
/// pour's outline, so it is a finding of its own here (issue #1222).
pub(crate) fn starved(reach: usize, lone: usize, tracks: usize) -> bool {
    let joined = reach - lone.min(reach);
    if reach == 0 || joined >= MIN_SPOKES {
        return false;
    }
    let count = match joined + tracks {
        0 => lone,
        n => n,
    };
    count < MIN_SPOKES
}

/// Whether the fill joins a relief by nothing: no spoke reaches the fill and no track of the
/// pad's net runs from the pad onto it (issue #1222), and no other fill of the net joins the
/// pad (`elsewhere`, issue #1292: a zone of the net of a higher priority over the pad, which the
/// lower one gives its area, or a fill on another layer of a pad with a hole). The fill can do
/// nothing about it (each spoke is made where its end is on the fill, [`Unjoinable`] says why
/// none is), so the pad is joined to its net by its own copper, a via or a track, which the
/// router keeps for it.
pub(crate) fn unjoinable(reach: usize, tracks: usize, elsewhere: bool) -> bool {
    reach == 0 && tracks == 0 && !elsewhere
}

/// Why the fill can join a thermal relief by none of its spokes (issue #1222).
#[derive(Debug, Clone, PartialEq)]
pub enum Unjoinable {
    /// The relief has no spokes: KiCad's filler makes no spoke narrower than the zone's minimum
    /// thickness, and the spoke is as wide as the zone's spoke width but no wider than the pad's
    /// narrower side (`by_pad` when the pad's side made it too narrow).
    /// `own` when the spoke width is the pad's own (`thermal_bridge_width`, a pin's
    /// `.spoke_width`, issue #1276) and not the zone's.
    Narrow {
        width: f64,
        min: f64,
        by_pad: bool,
        own: bool,
    },
    /// Each spoke ends where the fill does not reach: what keeps the fill off the end of each, in
    /// the order of the spokes.
    Blocked(Vec<Blocker>),
}

/// What keeps the fill off the end of a spoke (KiCad's test point, [`Spoke::end`]).
#[derive(Debug, Clone, PartialEq)]
pub enum Blocker {
    /// The clearance the fill keeps from copper of another net or from a hole: the thing (`pad
    /// C1.1 [VCC]`, `track [SIG]`, `via [SIG]`, `teardrop [SIG]`, `hole of pad H1.1`) and the
    /// clearance, in mm.
    Clearance { what: String, clearance: f64 },
    /// The thermal gap round another pad of the zone's net (`U1.3`).
    Gap { pad: String },
    /// The board's edge, or the band the fill keeps along it.
    Edge,
    /// The outside of the zone's outline.
    Outline,
    /// A rule area that keeps pours out.
    Keepout,
    /// A zone of another net with a higher priority, and the clearance kept from it.
    Zone { net: String },
    /// The fill reaches the end, but is narrower there than the zone's minimum thickness (in
    /// mm), which trimming takes out: a strip between two clearances.
    Thin { min: f64 },
}

impl std::fmt::Display for Blocker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Blocker::Clearance { what, clearance } => {
                write!(f, "the clearance of {what} ({clearance} mm)")
            }
            Blocker::Gap { pad } => write!(f, "the thermal gap of pad {pad}"),
            Blocker::Edge => f.write_str("the band along the board's edge"),
            Blocker::Outline => f.write_str("the outside of the zone's outline"),
            Blocker::Keepout => f.write_str("a rule area that keeps pours out"),
            Blocker::Zone { net } => write!(f, "the clearance of the zone of {net}"),
            Blocker::Thin { min } => write!(
                f,
                "a strip of fill narrower than the zone's minimum thickness ({min} mm)"
            ),
        }
    }
}

impl std::fmt::Display for Unjoinable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unjoinable::Narrow {
                width,
                min,
                by_pad,
                own,
            } => {
                let what = if *by_pad {
                    "the pad's narrower side"
                } else if *own {
                    "the pad's own spoke width"
                } else {
                    "the zone's spoke width"
                };
                write!(
                    f,
                    "{what}, {width} mm, is under the zone's minimum thickness, {min} mm, so the fill makes no spoke"
                )
            }
            Unjoinable::Blocked(blockers) => {
                // Each thing once, with how many spokes it stops.
                let mut seen: Vec<(String, usize)> = Vec::new();
                for b in blockers {
                    let text = b.to_string();
                    match seen.iter_mut().find(|(t, _)| *t == text) {
                        Some((_, n)) => *n += 1,
                        None => seen.push((text, 1)),
                    }
                }
                let parts: Vec<String> = seen
                    .into_iter()
                    .map(|(t, n)| match n {
                        1 => t,
                        n => format!("{t} ({n} spokes)"),
                    })
                    .collect();
                write!(
                    f,
                    "the end of each spoke is off the fill: {}",
                    parts.join("; ")
                )
            }
        }
    }
}

/// Each pad that a zone of its net on `layer` joins with thermal spokes, and how well
/// ([`ThermalJoin`]): of the fill the file holds where a zone holds one, else of the fill
/// [`pour`] draws.
pub fn thermal_joins(
    art: &Artwork,
    layer: &str,
    edge: &BoardEdge,
    rules: &Rules,
) -> Result<Vec<ThermalJoin>, String> {
    Ok(judge(art, layer, edge, rules, None)?
        .into_iter()
        .flat_map(|j| j.reliefs)
        .map(|r| {
            let pad = &art.pads[r.pad];
            ThermalJoin {
                part: pad.part.clone(),
                number: pad.number.clone(),
                at: pad.at,
                spokes: r.spokes,
                reach: r.reach,
                lone: r.lone,
                tracks: r.tracks,
            }
        })
        .collect())
}

/// How the fill of a zone joins one pad of its net (issue #1187).
#[derive(Debug, Clone, PartialEq)]
pub enum Join {
    /// Solid copper (the pad connection `solid`, or a surface pad of `thru_hole_only`):
    /// `touched` says whether the fill reaches the pad at all.
    Solid { touched: bool },
    /// A thermal relief: how many spokes it has (none where the pad is too narrow for one), how
    /// many of them reach the fill, and how many tracks of the pad's net on the layer run from
    /// the pad onto the fill, which join it as a spoke does ([`ThermalJoin`]); and when no spoke
    /// reaches the fill, why ([`Unjoinable`], issue #1222); and whether another fill of the
    /// pad's net joins the pad, which makes it not unjoinable ([`Relief::elsewhere`], issue
    /// #1292).
    Thermal {
        spokes: usize,
        reach: usize,
        lone: usize,
        tracks: usize,
        unjoinable: Option<Unjoinable>,
        elsewhere: bool,
    },
    /// Not at all: the zone's pad connection is `none`, and the fill keeps its clearance round
    /// the pad.
    Unjoined,
}

impl Join {
    /// Whether the pour starves the pad: a thermal relief that one or more spokes but fewer than
    /// [`MIN_SPOKES`] spokes and tracks join to the fill, as the design rules check's
    /// `starved_thermal` says.
    pub fn starved(&self) -> bool {
        matches!(*self, Join::Thermal { reach, lone, tracks, .. } if starved(reach, lone, tracks))
    }

    /// Why the pour joins the pad by nothing, a thermal relief no spoke and no track of which
    /// reaches the fill (the design rules check's `unjoinable_thermal`, issue #1222); `None`
    /// for a pad it joins, and for one another fill of its net joins (issue #1292).
    pub fn unjoinable(&self) -> Option<&Unjoinable> {
        match self {
            Join::Thermal {
                reach,
                tracks,
                unjoinable: why,
                elsewhere,
                ..
            } if unjoinable(*reach, *tracks, *elsewhere) => why.as_ref(),
            _ => None,
        }
    }
}

/// A pad, by its footprint's designator and number, its net and its centre on the board.
#[derive(Debug, Clone, PartialEq)]
pub struct PadAt {
    pub part: String,
    pub number: String,
    pub net: Option<String>,
    pub at: Pt,
}

/// What the fill of one zone on one layer does with the pads in it (issue #1187), from the
/// fill [`pour`] draws (or the fill the file holds, where the zone holds one), the one the
/// design rules check judges.
#[derive(Debug, Clone, PartialEq)]
pub struct ZoneJoins {
    /// The zone, by its index in the board's zones.
    pub zone: usize,
    /// Whether the fill is the one the board file holds (`filled_polygon`).
    pub held: bool,
    /// Each pad of the zone's net with copper on the layer and its centre in the zone's
    /// outline, and how the fill joins it.
    pub pads: Vec<(PadAt, Join)>,
    /// Each pad of another net (or of none) with copper on the layer and its centre in the
    /// zone's outline: the fill keeps its clearance round it.
    pub cleared: Vec<PadAt>,
    /// The copper islands the fill keeps (the zone's island setting keeps them; of a fill the
    /// file holds, every one), each by a corner and its area in mm².
    pub islands: Vec<(Pt, f64)>,
    /// The copper islands the fill took out, each by a corner and its area in mm².
    pub removed: Vec<(Pt, f64)>,
    /// The pieces of copper of `removed`, in its order, as the fill had them before it took them
    /// out: what the 2D view outlines (issue #1188).
    pub removed_fill: Vec<Area>,
    /// The pieces of the fill, as the Gerbers hold them.
    pub fill: Vec<Area>,
}

/// What the fill of each zone of a net on each copper layer of the board does with the pads in
/// it ([`ZoneJoins`]), by layer from the top and in the order of the board's zones: the same
/// fill, the same count of spokes and the same join ([`judge_board`]) as the design rules
/// check's `starved_thermal`, `unjoinable_thermal` and `isolated_copper`, and the fill [`pour`]
/// writes into the Gerbers.
pub fn pad_joins(
    art: &Artwork,
    edge: &BoardEdge,
    rules: &Rules,
) -> Result<Vec<(String, Vec<ZoneJoins>)>, String> {
    judge_board(art, &art.copper, edge, rules, None)
        .into_iter()
        .map(|(layer, judged)| Ok((layer.to_string(), zone_joins(art, layer, judged?))))
        .collect()
}

/// [`pad_joins`] of one layer, from its fills as [`judge_board`] judges them.
fn zone_joins(art: &Artwork, layer: &str, judged: Vec<Judged>) -> Vec<ZoneJoins> {
    let at = |pad: &Pad| PadAt {
        part: pad.part.clone(),
        number: pad.number.clone(),
        net: pad.net.clone(),
        at: pad.at,
    };
    judged
        .into_iter()
        .map(|j| {
            let z = &art.zones[j.zone];
            let outline = outline_area(z);
            let mut pads = Vec::new();
            let mut cleared = Vec::new();
            for (k, pad) in art.pads.iter().enumerate().filter(|(_, p)| {
                art.pad_copper_on(p, layer) && outline.iter().any(|a| a.contains(p.at))
            }) {
                if !own_net(z, &pad.net) {
                    cleared.push(at(pad));
                    continue;
                }
                let join = match connection(pad, z) {
                    Connect::Thermal => match j.reliefs.iter().find(|r| r.pad == k) {
                        Some(r) => Join::Thermal {
                            spokes: r.spokes,
                            reach: r.reach,
                            lone: r.lone,
                            tracks: r.tracks,
                            unjoinable: r.unjoinable.clone(),
                            elsewhere: r.elsewhere,
                        },
                        None => continue,
                    },
                    Connect::None => Join::Unjoined,
                    _ => Join::Solid {
                        touched: touches(pad, &j.fill),
                    },
                };
                pads.push((at(pad), join));
            }
            ZoneJoins {
                zone: j.zone,
                held: j.held,
                pads,
                cleared,
                islands: j.islands,
                removed: j.removed,
                removed_fill: j.removed_fill,
                fill: j.fill,
            }
        })
        .collect()
}

/// Whether a fill reaches a pad's copper: how a zone that joins the pad solid joins it.
fn touches(pad: &Pad, fill: &[Area]) -> bool {
    pads::outline(pad)
        .polygon()
        .is_some_and(|p| overlap(fill, &[Area::of(&p)]))
}

/// The fills of every layer of `layers` ([`judge`]), with each thermal relief that joins nothing
/// marked when another fill of its net joins the pad ([`Relief::elsewhere`], issue #1292): a zone
/// of the net of a higher priority over the pad, which the lower one gives its area (so none of
/// the lower one's spokes is on its fill, though the pad is in copper of its net), or, of a pad
/// with copper on more than one layer, a fill of another layer. A fill joins a pad when it joins
/// it solid and reaches it, or when a spoke or a track of its relief reaches it. The design rules
/// check and [`pad_joins`] (`board fill`, `fab`'s notes, the editor) judge the pads by this one
/// function, so they cannot differ. A layer whose zones cannot be filled is the error of that
/// layer, and its fills join nothing.
pub(crate) fn judge_board<'a>(
    art: &Artwork,
    layers: &'a [String],
    edge: &BoardEdge,
    rules: &Rules,
    sliver_width: Option<f64>,
) -> Vec<(&'a str, Result<Vec<Judged>, String>)> {
    let mut judged: Vec<(&str, Result<Vec<Judged>, String>)> = layers
        .iter()
        .map(|l| (l.as_str(), judge(art, l, edge, rules, sliver_width)))
        .collect();
    // The fills that join each pad, by the zone's index and the layer.
    let mut joined: std::collections::HashMap<usize, Vec<(usize, &str)>> = Default::default();
    for (layer, j) in &judged {
        for j in j.iter().flatten() {
            let z = &art.zones[j.zone];
            let outline = outline_area(z);
            for (k, pad) in art.pads.iter().enumerate().filter(|(_, p)| {
                own_net(z, &p.net)
                    && art.pad_copper_on(p, layer)
                    && outline.iter().any(|a| a.contains(p.at))
            }) {
                let joins = match connection(pad, z) {
                    Connect::Thermal => j.reliefs.iter().any(|r| r.pad == k && r.joins()),
                    Connect::None => false,
                    _ => touches(pad, &j.fill),
                };
                if joins {
                    joined.entry(k).or_default().push((j.zone, *layer));
                }
            }
        }
    }
    for (layer, j) in &mut judged {
        for j in j.iter_mut().flatten() {
            let zone = j.zone;
            for r in &mut j.reliefs {
                r.elsewhere = joined
                    .get(&r.pad)
                    .is_some_and(|by| by.iter().any(|&(z, l)| (z, l) != (zone, &**layer)));
            }
        }
    }
    judged
}

/// Points along a track, at most 0.05 mm apart, its two ends among them.
fn track_points(shape: &TrackShape) -> Vec<Pt> {
    match shape {
        TrackShape::Line(a, b) => {
            let n = ((a.dist(*b) / 0.05).ceil() as usize).max(1);
            (0..=n)
                .map(|i| *a + (*b - *a) * (i as f64 / n as f64))
                .collect()
        }
        TrackShape::Arc(arc) => arc.points(),
    }
}

/// The fill a zone holds in the board file on `layer`, as areas (each `filled_polygon` read by
/// the even-odd rule, as a Gerber region is, so a polygon KiCad fractures into one ring with
/// slits to its holes is the piece with its holes).
fn held_fill(z: &Zone, layer: &str) -> Vec<Area> {
    let mut p = Polygons::new();
    for (l, poly) in &z.filled {
        if l == layer && poly.len() >= 3 {
            p.region(Contour::polygon(poly), None, &[]);
        }
    }
    p.areas()
}

/// The fill of every zone of `layer` with a net, and what the check reads of it ([`Judged`]):
/// the fill the file holds where a zone holds one, else the fill worked out as [`pour`] draws
/// it. With `sliver_width`, the copper slivers narrower than it too: of every fill the file
/// holds, and of a fill worked out here only when the zone's minimum thickness is under the
/// width (the fill is trimmed to that thickness, which leaves nothing narrower).
pub(crate) fn judge(
    art: &Artwork,
    layer: &str,
    edge: &BoardEdge,
    rules: &Rules,
    sliver_width: Option<f64>,
) -> Result<Vec<Judged>, String> {
    let on_layer = |z: &Zone| z.keepout.is_none() && z.layers.iter().any(|l| l == layer);
    let holds = |z: &Zone| z.filled.iter().any(|(l, _)| l == layer);
    if !art
        .zones
        .iter()
        .any(|z| on_layer(z) && z.net.is_some() && !z.teardrop)
    {
        return Ok(Vec::new());
    }
    let index: Vec<usize> = art
        .zones
        .iter()
        .enumerate()
        .filter(|(_, z)| on_layer(z) && !holds(z))
        .map(|(i, _)| i)
        .collect();
    // Each zone's fill, by its index in the board's zones, whether the file holds it, and the
    // islands the fill took out.
    let mut fills: Vec<(usize, Vec<Area>, bool, Vec<Area>)> = Vec::new();
    if !index.is_empty() {
        let zones: Vec<&Zone> = index.iter().map(|&i| &art.zones[i]).collect();
        for f in fill_zones(art, layer, edge, rules, &zones)? {
            fills.push((index[f.zone], f.fill, false, f.removed));
        }
    }
    for (i, z) in art.zones.iter().enumerate() {
        if on_layer(z) && holds(z) && !z.teardrop {
            fills.push((i, held_fill(z, layer), true, Vec::new()));
        }
    }
    // The islands of every fill of the layer, the pieces of zones of a net that touch joined.
    let orphans = {
        let all: Vec<(&Zone, &[Area])> = fills
            .iter()
            .map(|(i, fill, _, _)| (&art.zones[*i], fill.as_slice()))
            .collect();
        isolated(&all, art, layer)
    };
    let mut out = Vec::new();
    for (n, (i, fill, held, removed)) in fills.iter().enumerate() {
        let z = &art.zones[*i];
        if z.net.is_none() || z.teardrop {
            continue;
        }
        let outline = outline_area(z);
        let on_fill = |p: &Pt| fill.iter().any(|a| a.contains(*p));
        // The pieces of the other zones of the net on the layer.
        let others_of = |i: usize| -> Vec<Area> {
            fills
                .iter()
                .filter(|(j, _, _, _)| *j != i && art.zones[*j].net == art.zones[i].net)
                .flat_map(|(_, f, _, _)| f.iter().cloned())
                .collect()
        };
        let mut reliefs = Vec::new();
        for (k, pad) in art.pads.iter().enumerate().filter(|(_, p)| {
            own_net(z, &p.net)
                && art.pad_copper_on(p, layer)
                && connection(p, z) == Connect::Thermal
                && outline.iter().any(|a| a.contains(p.at))
        }) {
            let shape = pads::outline(pad);
            let spokes = spokes(pad, z);
            // KiCad's check counts the places where the fill crosses the pad grown by half the
            // gap: a spoke whose middle there is on the fill.
            let reach = spokes.iter().filter(|s| on_fill(&s.root)).count();
            // The spokes onto a piece of the fill that joins nothing but the pad (issue #1222).
            let lone = spokes
                .iter()
                .filter_map(|s| fill.iter().position(|a| a.contains(s.root)))
                .filter(|&piece| alone(&fill[piece], art, layer, z, k, &others_of(*i)))
                .count();
            let tracks = tracks_onto(art, layer, z, &shape, &on_fill);
            let unjoinable = (reach == 0).then(|| {
                if spokes.is_empty() {
                    let side = pad.size.0.min(pad.size.1);
                    let bridge = thermal_bridge(pad, z);
                    let by_pad = side < bridge;
                    Unjoinable::Narrow {
                        width: if by_pad { side } else { bridge },
                        min: z.min_thickness,
                        by_pad,
                        own: pad.thermal_bridge.is_some(),
                    }
                } else {
                    let higher: Vec<(&Zone, &[Area])> = fills
                        .iter()
                        .filter(|(j, _, _, _)| j != i)
                        .map(|(j, f, _, _)| (&art.zones[*j], f.as_slice()))
                        .filter(|(h, _)| h.net != z.net && h.priority > z.priority)
                        .collect();
                    let keeps_off = |p: Pt| {
                        if on_fill(&p) {
                            // The end is on the fill but the spoke's middle is not: the fill
                            // the file holds, made another way.
                            Blocker::Thin {
                                min: z.min_thickness,
                            }
                        } else {
                            blocker(art, layer, edge, rules, z, k, &higher, p)
                        }
                    };
                    Unjoinable::Blocked(spokes.iter().map(|s| keeps_off(s.end)).collect())
                }
            });
            reliefs.push(Relief {
                pad: k,
                spokes: spokes.len(),
                reach,
                lone,
                tracks,
                unjoinable,
                elsewhere: false,
            });
        }
        let others = || -> Vec<Area> {
            fills
                .iter()
                .filter(|(j, _, _, _)| j != i && art.zones[*j].net == z.net)
                .flat_map(|(_, f, _, _)| f.iter().cloned())
                .collect()
        };
        let mut islands = Vec::new();
        if *held || z.islands != Islands::Remove {
            for (piece, isolated) in fill.iter().zip(&orphans[n]) {
                if *isolated && let Some(island) = piece_at(piece) {
                    islands.push(island);
                }
            }
        }
        let slivers = match sliver_width {
            Some(width) if width > 0.0 && (*held || z.min_thickness < width) => {
                // The copper of the net round the fill: a spike of the fill against a pad, a track
                // or another zone of the net is part of wider copper and no sliver.
                let mut round = others();
                round.extend(net_copper(art, layer, z.net.as_deref().unwrap_or_default()));
                slivers(fill, &round, width)
            }
            _ => Vec::new(),
        };
        let spikes = if sliver_width.is_some() && !fill.is_empty() {
            let mut all = fill.clone();
            all.extend(grown(&others(), TOUCH));
            all.extend(grown(
                &net_copper(art, layer, z.net.as_deref().unwrap_or_default()),
                TOUCH,
            ));
            let on_fill = |p: &Pt| {
                fill.iter().any(|a| {
                    std::iter::once(&a.outer)
                        .chain(&a.holes)
                        .any(|r| ring_distance(r, *p) <= 2.0 * TOUCH)
                })
            };
            sharp_corners(&union(&all))
                .into_iter()
                .filter(on_fill)
                .collect()
        } else {
            Vec::new()
        };
        out.push(Judged {
            zone: *i,
            held: *held,
            fill: fill.clone(),
            reliefs,
            islands,
            removed: removed.iter().filter_map(piece_at).collect(),
            removed_fill: removed
                .iter()
                .filter(|p| piece_at(p).is_some())
                .cloned()
                .collect(),
            slivers,
            spikes,
        });
    }
    out.sort_by_key(|j| j.zone);
    Ok(out)
}

/// The copper of a fill narrower than `width` that reaches further than `width` from the copper
/// that trimming to `width` keeps (issue #993): a corner, which trimming only rounds, stays
/// within it; a strip or a spike narrower than the width runs further. The fill is trimmed
/// together with `round`, the other copper of its net on the layer, so a part of the fill that
/// lies against that copper is as wide as the two together. Each piece, by a corner of it and
/// its area in mm², pieces under a square micrometre (the curves' rounding) left out.
fn slivers(fill: &[Area], round: &[Area], width: f64) -> Vec<(Pt, f64)> {
    if width <= 0.0 || fill.is_empty() {
        return Vec::new();
    }
    let copper = if round.is_empty() {
        fill.to_vec()
    } else {
        // Copper that touches the fill shares an edge with it and no area: grown a little, it
        // joins.
        let mut all = fill.to_vec();
        all.extend(grown(round, TOUCH));
        union(&all)
    };
    let kept = trimmed(&copper, width);
    let narrow = difference(fill, &kept);
    if narrow.iter().all(|a| a.size() < SLIVER_NOISE) {
        return Vec::new();
    }
    let far = if kept.is_empty() {
        narrow
    } else {
        difference(&narrow, &grown(&kept, width))
    };
    far.iter()
        .filter(|a| a.size() >= SLIVER_NOISE)
        .filter_map(|a| Some((*a.outer.first()?, a.size())))
        .collect()
}

/// KiCad 9's copper sliver (`drc_test_provider_sliver_checker.cpp`, its advanced settings'
/// defaults): a corner of the outline of the copper of a layer (every item and fill together)
/// that points out of the copper at an angle under 20 degrees, the ends of its two sides more
/// than 0.08 mm apart; sides shorter than 0.8 µm are passed over. Each such corner (issue #1222).
pub(crate) fn sharp_corners(copper: &[Area]) -> Vec<Pt> {
    const WIDTH: f64 = 0.08;
    const ANGLE: f64 = 20.0;
    const MIN_LEN: f64 = 0.0008;
    let cos_tol = ANGLE.to_radians().cos();
    let mut out = Vec::new();
    for a in copper {
        let pts = &a.outer;
        let n = pts.len();
        if n <= 5 {
            continue;
        }
        let short = |v: Pt| v.x.abs() < MIN_LEN && v.y.abs() < MIN_LEN;
        for k in 0..n {
            let p = pts[k];
            let Some(prior) = (1..n)
                .map(|i| pts[(k + n - i) % n])
                .find(|q| !short(*q - p))
            else {
                continue;
            };
            let Some(after) = (1..n).map(|i| pts[(k + i) % n]).find(|q| !short(*q - p)) else {
                continue;
            };
            let (u, v) = (prior - p, after - p);
            let (lu, lv) = (
                (u.x * u.x + u.y * u.y).sqrt(),
                (v.x * v.x + v.y * v.y).sqrt(),
            );
            let cos = (u.x * v.x + u.y * v.y) / (lu * lv);
            // A corner of the copper, not of a notch in it: the outer ring runs positive, so the
            // copper is on one side of the turn.
            let turn = u.x * v.y - u.y * v.x;
            if cos > cos_tol && turn > 0.0 && prior.dist(after) > WIDTH {
                out.push(p);
            }
        }
    }
    out
}

/// The least area of a piece of [`slivers`], in mm²: a square micrometre, far above the
/// rounding of a curve cut into straight pieces and far below any copper a fabricator etches.
const SLIVER_NOISE: f64 = 1e-6;

/// The copper of `net` on `layer` that joins a piece of its zone: its pads with copper on the
/// layer, its vias, its tracks on the layer and its teardrops there.
fn net_copper(art: &Artwork, layer: &str, net: &str) -> Vec<Area> {
    let is = |n: &Option<String>| n.as_deref() == Some(net);
    let mut p = Polygons::new();
    for pad in art
        .pads
        .iter()
        .filter(|pad| is(&pad.net) && art.pad_copper_on(pad, layer))
    {
        pads::outline(pad).draw(&mut p, None, &[]);
    }
    for via in art
        .vias
        .iter()
        .filter(|v| is(&v.net) && art.via_on(v, layer))
    {
        Outline::Circle {
            c: via.at,
            r: via.diameter / 2.0,
        }
        .draw(&mut p, None, &[]);
    }
    for t in art.tracks.iter().filter(|t| t.layer == layer && is(&t.net)) {
        match &t.shape {
            TrackShape::Line(a, b) => stroke(&mut p, *a, *b, t.width),
            TrackShape::Arc(arc) => p.arc(t.width, None, arc, &[]),
        }
    }
    for t in art
        .zones
        .iter()
        .filter(|t| t.teardrop && t.keepout.is_none() && is(&t.net))
    {
        for (l, poly) in &t.filled {
            if l == layer && poly.len() >= 3 {
                p.region(Contour::polygon(poly), None, &[]);
            }
        }
    }
    p.areas()
}

fn net_attr(net: Option<&str>) -> Vec<String> {
    vec![format!("N,{}", crate::gbr::escape(net.unwrap_or("N/C")))]
}

fn zone_bounds(z: &Zone) -> Option<Bounds> {
    Bounds::of(z.outlines.iter().flatten().copied())
}

/// Clears the outside of the board, the cutouts, and a band of `2 * clearance` along every
/// edge line.
fn keep_out_of_the_edge(
    g: &mut dyn Pen,
    art: &Artwork,
    edge: &BoardEdge,
    clearance: f64,
    zones: &[&Zone],
) {
    let Some(mut b) = Bounds::of(
        zones
            .iter()
            .filter_map(|z| zone_bounds(z))
            .flat_map(|b| [b.min, b.max]),
    ) else {
        return;
    };
    b = b.expanded(1.0);
    // The outside: a frame with the board's outline cut out of it, joined by a slit.
    let mut outer = edge.outer.clone();
    let frame = [
        Pt::new(b.min.x, b.min.y),
        Pt::new(b.max.x, b.min.y),
        Pt::new(b.max.x, b.max.y),
        Pt::new(b.min.x, b.max.y),
    ];
    // The frame and the outline must run in opposite directions.
    if (geom::area2(&frame) > 0.0) == (geom::area2(&outer) > 0.0) {
        outer.reverse();
    }
    let mut ring: Vec<Pt> = frame.to_vec();
    ring.push(frame[0]);
    ring.push(outer[0]);
    ring.extend(outer.iter().skip(1));
    ring.push(outer[0]);
    ring.push(frame[0]);
    g.region(Contour::polygon(&ring), None, &[]);
    for c in &edge.cutouts {
        g.region(Contour::polygon(c), None, &[]);
    }
    let width = 2.0 * clearance;
    for gr in art.graphics.iter().filter(|gr| gr.layer == "Edge.Cuts") {
        stroke_shape(g, &gr.shape, width);
    }
}

/// A shape's outline stroked with a round pen (in whatever polarity the file is in).
fn stroke_shape(g: &mut dyn Pen, shape: &Shape, width: f64) {
    match shape {
        Shape::Line(a, b) => stroke(g, *a, *b, width),
        Shape::Arc(arc) => g.arc(width, None, arc, &[]),
        Shape::Circle { centre, radius } => {
            let start = Pt::new(centre.x + radius, centre.y);
            g.arc(
                width,
                None,
                &Arc {
                    start,
                    end: start,
                    centre: *centre,
                    radius: *radius,
                    ccw: true,
                },
                &[],
            );
        }
        Shape::Poly(p) => {
            for i in 0..p.len() {
                stroke(g, p[i], p[(i + 1) % p.len()], width);
            }
        }
    }
}

fn stroke(g: &mut dyn Pen, a: Pt, b: Pt, width: f64) {
    if a == b {
        g.flash(crate::gbr::Ap::Circle(width), None, a, &[]);
    } else {
        g.line(width, None, a, b, &[]);
    }
}

/// Clears what one zone keeps away from on `layer`.
fn around_copper(
    g: &mut dyn Pen,
    art: &Artwork,
    layer: &str,
    z: &Zone,
    rules: &Rules,
) -> Result<(), String> {
    let clearance = zone_clearance(z, rules);
    // The clearance from copper of `net`: the zone's, or the class's of `net` if larger, and
    // MARGIN more, as every space this cuts is.
    let from = |net: &Option<String>| clearance.max(rules.class_clearance(net.as_deref())) + MARGIN;
    let reach = clearance.max(rules.widest_class_clearance()) + z.thermal_gap.max(0.0);
    let Some(area) = zone_bounds(z).map(|b| b.expanded(reach)) else {
        return Ok(());
    };
    let own = |net: &Option<String>| own_net(z, net);

    for pad in &art.pads {
        let shape = pads::outline(pad);
        let copper = art.pad_copper_on(pad, layer);
        if !copper {
            // A hole with no copper round it still keeps the clearance.
            if let Some(d) = &pad.drill {
                let by = rules.hole_clearance.unwrap_or(clearance) + MARGIN;
                if hole_outline(pad, d).bounds().expanded(by).overlaps(&area) {
                    draw_grown(g, &hole_outline(pad, d), by, &pad.part, &pad.number)?;
                }
            }
            continue;
        }
        if !shape.bounds().expanded(reach).overlaps(&area) {
            continue;
        }
        let mode = connection(pad, z);
        if own(&pad.net) && mode != Connect::None {
            if mode == Connect::Thermal {
                let width = spoke_width(pad, z);
                thermal(g, &shape, pad.at, pad.angle + pad.spoke_angle(), width, z)?;
            }
        } else {
            let by = pad
                .clearance
                .map_or(from(&pad.net), |c| (c + MARGIN).max(from(&pad.net)));
            draw_grown(g, &shape, by, &pad.part, &pad.number)?;
        }
    }
    // A blind or a buried via has neither copper nor a hole on a layer it does not reach
    // (issue #1015), so the zone needs no room for it there.
    for via in art.vias.iter().filter(|v| art.via_on(v, layer)) {
        let shape = Outline::Circle {
            c: via.at,
            r: via.diameter / 2.0,
        };
        if !shape.bounds().expanded(reach).overlaps(&area) {
            continue;
        }
        if own(&via.net) {
            if VIA_THERMAL && z.connect == Connect::Thermal {
                thermal(g, &shape, via.at, 0.0, Some(z.thermal_bridge), z)?;
            }
        } else {
            draw_grown(g, &shape, from(&via.net), "via", "")?;
        }
    }
    for t in art
        .tracks
        .iter()
        .filter(|t| t.layer == layer && !own(&t.net))
    {
        let width = t.width + 2.0 * from(&t.net);
        match &t.shape {
            TrackShape::Line(a, b) => {
                if Bounds::of([*a, *b]).is_some_and(|bb| bb.expanded(width).overlaps(&area)) {
                    stroke(g, *a, *b, width);
                }
            }
            TrackShape::Arc(arc) => {
                if Bounds::of(arc.points()).is_some_and(|bb| bb.expanded(width).overlaps(&area)) {
                    g.arc(width, None, arc, &[]);
                }
            }
        }
    }
    // A teardrop of another net: its fill grown by the clearance.
    for t in art
        .zones
        .iter()
        .filter(|t| t.teardrop && t.keepout.is_none() && !own(&t.net))
    {
        let clearance = from(&t.net);
        for (l, poly) in &t.filled {
            if l != layer
                || poly.len() < 3
                || !Bounds::of(poly.iter().copied())
                    .is_some_and(|b| b.expanded(clearance).overlaps(&area))
            {
                continue;
            }
            g.region(Contour::polygon(poly), None, &[]);
            let mut ring = poly.clone();
            ring.push(poly[0]);
            for w in ring.windows(2) {
                stroke(g, w[0], w[1], 2.0 * clearance);
            }
        }
    }
    for gr in art.graphics.iter().filter(|gr| gr.layer == layer) {
        let width = gr.width.max(0.0) + 2.0 * (clearance + MARGIN);
        if gr.fill
            && let Shape::Poly(p) = &gr.shape
            && p.len() >= 3
        {
            g.region(Contour::polygon(p), None, &[]);
        }
        if let Shape::Circle { centre, radius } = &gr.shape
            && gr.fill
        {
            g.flash(
                crate::gbr::Ap::Circle(2.0 * radius + width),
                None,
                *centre,
                &[],
            );
            continue;
        }
        stroke_shape(g, &gr.shape, width);
    }
    Ok(())
}

/// How many tracks of the zone's net on `layer` join `pad` to the fill as a spoke does, as KiCad
/// 9's check counts them (`drc_test_provider_zone_connections.cpp`, issue #1222): a track that
/// touches the pad (`shape`), one end of which is in the pad grown by half the thermal gap and the other
/// on the fill (the end, not any point along it: a track that leaves the fill again counts not).
fn tracks_onto(
    art: &Artwork,
    layer: &str,
    z: &Zone,
    shape: &Outline,
    on_fill: &dyn Fn(&Pt) -> bool,
) -> usize {
    let Some(poly) = shape.polygon() else {
        return 0;
    };
    let near = match shape.grow(z.thermal_gap / 2.0) {
        Ok(Some(g)) => g.polygon().unwrap_or_else(|| poly.clone()),
        _ => poly.clone(),
    };
    art.tracks
        .iter()
        .filter(|t| t.layer == layer && t.net == z.net)
        .filter(|t| {
            let (start, end) = match &t.shape {
                TrackShape::Line(a, b) => (*a, *b),
                TrackShape::Arc(arc) => (arc.start, arc.end),
            };
            // The track touches the pad: an end on it or within half the track's width of it.
            let touches =
                |p: Pt| geom::contains(&poly, p) || ring_distance(&poly, p) <= t.width / 2.0;
            if geom::contains(&near, start) && touches(start) {
                on_fill(&end)
            } else if geom::contains(&near, end) && touches(end) {
                on_fill(&start)
            } else {
                false
            }
        })
        .count()
}

/// Whether a piece of the fill of `z` joins nothing but the pad `own` (by its index in the board's
/// pads): no other pad, via or track of the zone's net on `layer` and no piece of another zone of
/// the net (`others`) touches it. KiCad's check does not count a spoke onto such a piece, an
/// island of the pad's own, while anything else joins the pad (issue #1222).
fn alone(piece: &Area, art: &Artwork, layer: &str, z: &Zone, own: usize, others: &[Area]) -> bool {
    let Some(b) = Bounds::of(piece.outer.iter().copied()) else {
        return true;
    };
    let near = |q: &Bounds| q.overlaps(&b);
    let one = std::slice::from_ref(piece);
    let pads = art.pads.iter().enumerate().any(|(k, p)| {
        k != own
            && own_net(z, &p.net)
            && art.pad_copper_on(p, layer)
            && pads::outline(p).polygon().is_some_and(|poly| {
                Bounds::of(poly.iter().copied()).is_some_and(|q| near(&q))
                    && overlap(one, &[Area::of(&poly)])
            })
    });
    if pads {
        return false;
    }
    let ring_near = |p: Pt, by: f64| {
        piece.contains(p)
            || std::iter::once(&piece.outer)
                .chain(&piece.holes)
                .any(|r| ring_distance(r, p) <= by)
    };
    if art
        .vias
        .iter()
        .filter(|v| own_net(z, &v.net) && art.via_on(v, layer))
        .any(|v| ring_near(v.at, v.diameter / 2.0))
    {
        return false;
    }
    if art
        .tracks
        .iter()
        .filter(|t| t.layer == layer && own_net(z, &t.net))
        .any(|t| {
            track_points(&t.shape)
                .into_iter()
                .any(|p| ring_near(p, t.width / 2.0))
        })
    {
        return false;
    }
    !others.iter().any(|o| {
        Bounds::of(o.outer.iter().copied()).is_some_and(|q| q.expanded(TOUCH).overlaps(&b))
            && overlap(one, &grown(std::slice::from_ref(o), TOUCH))
    })
}

/// The distance from `p` to the segment from `a` to `b`.
fn segment_distance(p: Pt, a: Pt, b: Pt) -> f64 {
    let d = b - a;
    let l = d.x * d.x + d.y * d.y;
    let t = if l > 0.0 {
        (((p - a).x * d.x + (p - a).y * d.y) / l).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.dist(a + d * t)
}

/// The distance from `p` to the nearest side of a closed polygon.
fn ring_distance(ring: &[Pt], p: Pt) -> f64 {
    (0..ring.len())
        .map(|i| segment_distance(p, ring[i], ring[(i + 1) % ring.len()]))
        .fold(f64::INFINITY, f64::min)
}

/// What keeps the fill of `z` off the point `p`, the end of a spoke of the pad `own` (by its
/// index in the board's pads) that is not on the fill (issue #1222): the first of the zone's
/// outline, the board's edge, a rule area, the clearance of another net's copper or a hole, the
/// thermal gap of another pad of the net and a zone of another net of a higher priority (in
/// `higher`, with its fill) whose keep-away area holds `p`, as [`fill_zones`] cuts them; else a
/// strip of fill narrower than the minimum thickness, which the trimming took out.
#[allow(clippy::too_many_arguments)]
fn blocker(
    art: &Artwork,
    layer: &str,
    edge: &BoardEdge,
    rules: &Rules,
    z: &Zone,
    own: usize,
    higher: &[(&Zone, &[Area])],
    p: Pt,
) -> Blocker {
    if !outline_area(z).iter().any(|a| a.contains(p)) {
        return Blocker::Outline;
    }
    let band = rules.edge_clearance + MARGIN;
    let on_board =
        geom::contains(&edge.outer, p) && !edge.cutouts.iter().any(|c| geom::contains(c, p));
    if !on_board
        || std::iter::once(&edge.outer)
            .chain(&edge.cutouts)
            .any(|r| ring_distance(r, p) <= band)
    {
        return Blocker::Edge;
    }
    if art.zones.iter().any(|k| {
        k.keepout.as_ref().is_some_and(|k| k.copperpour)
            && k.layers.iter().any(|l| l == layer)
            && k.outlines
                .iter()
                .any(|o| o.len() >= 3 && (geom::contains(o, p) || ring_distance(o, p) <= MARGIN))
    }) {
        return Blocker::Keepout;
    }
    let clearance = zone_clearance(z, rules);
    let from = |net: &Option<String>| clearance.max(rules.class_clearance(net.as_deref()));
    let net = |n: &Option<String>| n.clone().unwrap_or_else(|| "no net".into());
    let within = |shape: &Outline, by: f64| matches!(shape.grow(by), Ok(Some(g)) if g.polygon().is_some_and(|poly| geom::contains(&poly, p)));
    for (k, pad) in art.pads.iter().enumerate() {
        let name = format!("{}.{}", pad.part, pad.number);
        if !art.pad_copper_on(pad, layer) {
            if let Some(d) = &pad.drill {
                let by = rules.hole_clearance.unwrap_or(clearance);
                if within(&hole_outline(pad, d), by + MARGIN) {
                    return Blocker::Clearance {
                        what: format!("hole of pad {name}"),
                        clearance: by,
                    };
                }
            }
            continue;
        }
        let shape = pads::outline(pad);
        let mode = connection(pad, z);
        if own_net(z, &pad.net) && mode != Connect::None {
            if k != own && mode == Connect::Thermal && within(&shape, z.thermal_gap) {
                return Blocker::Gap { pad: name };
            }
            continue;
        }
        let by = pad
            .clearance
            .map_or(from(&pad.net), |c| c.max(from(&pad.net)));
        if within(&shape, by + MARGIN) {
            return Blocker::Clearance {
                what: format!("pad {name} [{}]", net(&pad.net)),
                clearance: by,
            };
        }
    }
    for via in art
        .vias
        .iter()
        .filter(|v| art.via_on(v, layer) && !own_net(z, &v.net))
    {
        let by = from(&via.net);
        if p.dist(via.at) <= via.diameter / 2.0 + by + MARGIN {
            return Blocker::Clearance {
                what: format!("via [{}]", net(&via.net)),
                clearance: by,
            };
        }
    }
    for t in art
        .tracks
        .iter()
        .filter(|t| t.layer == layer && !own_net(z, &t.net))
    {
        let by = from(&t.net);
        let points = match &t.shape {
            TrackShape::Line(a, b) => vec![*a, *b],
            TrackShape::Arc(arc) => arc.points(),
        };
        let near = points
            .windows(2)
            .map(|w| segment_distance(p, w[0], w[1]))
            .fold(f64::INFINITY, f64::min);
        if near <= t.width / 2.0 + by + MARGIN {
            return Blocker::Clearance {
                what: format!("track [{}]", net(&t.net)),
                clearance: by,
            };
        }
    }
    for t in art
        .zones
        .iter()
        .filter(|t| t.teardrop && t.keepout.is_none() && !own_net(z, &t.net))
    {
        let by = from(&t.net);
        if t.filled.iter().any(|(l, poly)| {
            l == layer
                && poly.len() >= 3
                && (geom::contains(poly, p) || ring_distance(poly, p) <= by + MARGIN)
        }) {
            return Blocker::Clearance {
                what: format!("teardrop [{}]", net(&t.net)),
                clearance: by,
            };
        }
    }
    for (h, fill) in higher {
        let by = zone_clearance(z, rules).max(zone_clearance(h, rules)) + MARGIN;
        if fill.iter().any(|a| {
            a.contains(p)
                || std::iter::once(&a.outer)
                    .chain(&a.holes)
                    .any(|r| ring_distance(r, p) <= by)
        }) {
            return Blocker::Zone { net: net(&h.net) };
        }
    }
    Blocker::Thin {
        min: z.min_thickness,
    }
}

/// The shape of a pad's hole.
fn hole_outline(pad: &crate::artwork::Pad, d: &crate::artwork::Drill) -> Outline {
    let c = pad.at - d.offset.rotate(pad.angle);
    if d.is_slot() {
        Outline::Oval {
            c,
            w: d.width,
            h: d.length,
            angle: pad.angle,
        }
    } else {
        Outline::Circle {
            c,
            r: d.width / 2.0,
        }
    }
}

fn draw_grown(
    g: &mut dyn Pen,
    shape: &Outline,
    by: f64,
    part: &str,
    number: &str,
) -> Result<(), String> {
    match shape.grow(by) {
        Ok(Some(s)) => {
            s.draw(g, None, &[]);
            Ok(())
        }
        Ok(None) => Ok(()),
        Err(e) => Err(format!("pad {part}.{number}: {e}")),
    }
}

/// The thermal relief of a pad or via: the gap round it, in the four pieces between the
/// spokes, which run along the axes turned by `angle` (the pad's turn and its spoke angle,
/// [`crate::artwork::Pad::spoke_angle`]) and are `width` wide ([`spoke_width`]). With no width
/// the whole gap is cleared, as KiCad clears it round a pad too narrow for a spoke.
fn thermal(
    g: &mut dyn Pen,
    shape: &Outline,
    centre: Pt,
    angle: f64,
    width: Option<f64>,
    z: &Zone,
) -> Result<(), String> {
    let Ok(Some(grown)) = shape.grow(z.thermal_gap) else {
        return Ok(());
    };
    let Some(poly) = grown.polygon() else {
        return Ok(());
    };
    let Some(width) = width else {
        g.region(Contour::polygon(&poly), None, &[]);
        return Ok(());
    };
    let local: Vec<Pt> = poly.iter().map(|p| (*p - centre).rotate(-angle)).collect();
    let half = width / 2.0;
    // Each piece: the part of the gap in one quadrant beyond the spokes' half width.
    let quadrants = [
        [
            (Pt::new(half, 0.0), Pt::new(0.0, -1.0)),
            (Pt::new(0.0, half), Pt::new(1.0, 0.0)),
        ],
        [
            (Pt::new(-half, 0.0), Pt::new(0.0, 1.0)),
            (Pt::new(0.0, half), Pt::new(1.0, 0.0)),
        ],
        [
            (Pt::new(-half, 0.0), Pt::new(0.0, 1.0)),
            (Pt::new(0.0, -half), Pt::new(-1.0, 0.0)),
        ],
        [
            (Pt::new(half, 0.0), Pt::new(0.0, -1.0)),
            (Pt::new(0.0, -half), Pt::new(-1.0, 0.0)),
        ],
    ];
    for q in quadrants {
        let mut piece = local.clone();
        for (at, dir) in q {
            piece = geom::clip_half_plane(&piece, at, dir);
        }
        if piece.len() >= 3 {
            let world: Vec<Pt> = piece.iter().map(|p| centre + p.rotate(angle)).collect();
            g.region(Contour::polygon(&world), None, &[]);
        }
    }
    Ok(())
}

/// How wide a pad's thermal spokes are in a zone, as KiCad's filler makes them
/// (`buildThermalSpokes`): the zone's spoke width, but no wider than the pad's narrower side;
/// `None` (no spokes, the whole gap cleared) when that is narrower than the zone's minimum
/// thickness, since KiCad makes no spoke the fill's trimming would take out.
fn spoke_width(pad: &Pad, z: &Zone) -> Option<f64> {
    let width = thermal_bridge(pad, z).min(pad.size.0.min(pad.size.1));
    (width > 0.0 && width >= z.min_thickness).then_some(width)
}

/// The spoke width a pad's thermal relief asks for in a zone: the pad's own
/// `thermal_bridge_width` (a pin's `.spoke_width`, issue #1276), else the zone's, as KiCad's
/// rules engine resolves it (the pad's local override first).
fn thermal_bridge(pad: &Pad, z: &Zone) -> f64 {
    pad.thermal_bridge.unwrap_or(z.thermal_bridge)
}

/// How far KiCad's spoke test point lies beyond the thermal gap, besides half the zone's
/// minimum thickness: KiCad's `m_MaxError` (its default, 0.005 mm), which KiCad adds so that a
/// point on the edge of a polygon counts as inside.
const MAX_ERROR: f64 = 0.005;

/// A thermal spoke of a pad, as KiCad's filler builds it (`buildThermalSpokes`, KiCad 9).
struct Spoke {
    /// The spoke's copper from the pad's centre across the gap, to the outline of the pad grown
    /// by the gap: what [`thermal`] leaves of the relief there.
    gap: Vec<Pt>,
    /// KiCad's whole spoke, a rectangle from the pad's centre to `end`, which another spoke's
    /// end may lie in.
    whole: Vec<Pt>,
    /// KiCad's test point, the middle of the spoke's outer end: where the pad's bounding box
    /// (in its own frame), grown by the gap, [`MAX_ERROR`] and half the zone's minimum
    /// thickness, meets the spoke's axis. The spoke is made only where the fill reaches it.
    end: Pt,
    /// The middle of the spoke halfway across the gap: KiCad's check counts a spoke where the
    /// fill crosses the pad grown by half the gap.
    root: Pt,
}

/// The four spokes of a pad's thermal relief in a zone, along the axes [`thermal`] leaves them
/// on; none when the pad is too narrow for one ([`spoke_width`]).
fn spokes(pad: &Pad, z: &Zone) -> Vec<Spoke> {
    let Some(width) = spoke_width(pad, z) else {
        return Vec::new();
    };
    let shape = pads::outline(pad);
    let Ok(Some(grown)) = shape.grow(z.thermal_gap) else {
        return Vec::new();
    };
    let Some(poly) = grown.polygon() else {
        return Vec::new();
    };
    let centre = pad.at;
    // The pad's bounding box in its own frame, as KiCad takes it from the pad unturned.
    let (hx, hy) = shape
        .polygon()
        .and_then(|p| Bounds::of(p.iter().map(|q| (*q - centre).rotate(-pad.angle))))
        .map_or((pad.size.0 / 2.0, pad.size.1 / 2.0), |b| {
            (
                b.max.x.abs().max(b.min.x.abs()),
                b.max.y.abs().max(b.min.y.abs()),
            )
        });
    let beyond = z.thermal_gap + MAX_ERROR + z.min_thickness / 2.0;
    let (hx, hy) = (hx + beyond, hy + beyond);
    // KiCad builds a round pad's spokes along the axes and turns them, so their ends lie on
    // the circle, not on the box's corners.
    let circular = match pad.shape {
        PadShape::Circle => true,
        PadShape::Oval => (pad.size.0 - pad.size.1).abs() < 1e-9,
        _ => false,
    };
    let half = width / 2.0;
    let mut out = Vec::new();
    for k in 0..4 {
        let local_angle = pad.spoke_angle() + 90.0 * f64::from(k);
        // In a frame where the spoke runs along +x.
        let turn = pad.angle + local_angle;
        let local: Vec<Pt> = poly.iter().map(|p| (*p - centre).rotate(-turn)).collect();
        let mut spoke = geom::clip_half_plane(&local, Pt::new(0.0, half), Pt::new(-1.0, 0.0));
        spoke = geom::clip_half_plane(&spoke, Pt::new(0.0, -half), Pt::new(1.0, 0.0));
        spoke = geom::clip_half_plane(&spoke, Pt::new(0.0, 0.0), Pt::new(0.0, -1.0));
        if spoke.len() < 3 {
            continue;
        }
        let reach = spoke.iter().map(|p| p.x).fold(0.0, f64::max);
        // The length to the test point: where the spoke's axis leaves the grown box.
        let length = if circular {
            hx
        } else {
            let d = Pt::new(1.0, 0.0).rotate(local_angle);
            let (dx, dy) = (d.x.abs(), d.y.abs());
            if dx < 1e-9 {
                hy
            } else if dy < 1e-9 {
                hx
            } else {
                (hx / dx).min(hy / dy)
            }
        };
        let world = |p: Pt| centre + p.rotate(turn);
        out.push(Spoke {
            gap: spoke.iter().map(|p| world(*p)).collect(),
            whole: [
                Pt::new(0.0, -half),
                Pt::new(length, -half),
                Pt::new(length, half),
                Pt::new(0.0, half),
            ]
            .into_iter()
            .map(world)
            .collect(),
            end: world(Pt::new(length, 0.0)),
            root: world(Pt::new(reach - z.thermal_gap / 2.0, 0.0)),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gbr::Gerber;

    /// A via of GND in a corner, so that the GND pour touches its net and is no island.
    const GND_VIA: &str = r#"(via (at 2 2) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))"#;

    /// A board with a GND zone on F.Cu of minimum thickness `min` and the fill settings `fill`.
    fn art_with(min: f64, fill: &str, items: &str) -> Artwork {
        Artwork::parse(&format!(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal))
            (net 0 "") (net 1 "GND") (net 2 "SIG")
            (gr_rect (start 0 0) (end 20 20) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))
            (zone (net 1) (net_name "GND") (layer "F.Cu") (connect_pads (clearance 0.3)) (min_thickness {min})
              (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5) {fill})
              (polygon (pts (xy 0.2 0.2) (xy 19.8 0.2) (xy 19.8 19.8) (xy 0.2 19.8))))
            {items})"#
        ))
        .unwrap()
    }

    fn art(items: &str) -> Artwork {
        art_with(0.25, "", items)
    }

    fn rules() -> Rules {
        Rules {
            edge_clearance: 0.2,
            hole_clearance: None,
            classes: Vec::new(),
        }
    }

    fn poured(a: &Artwork) -> String {
        let edge = crate::edge::outline(a).unwrap();
        let mut g = Gerber::new(Pt::default());
        pour(&mut g, a, "F.Cu", &edge, &rules()).unwrap();
        g.text("Copper,L1,Top", false, "B")
    }

    /// The fill of F.Cu of `a` as polygons, by net, with `rules`.
    fn fill_with(a: &Artwork, rules: &Rules) -> Vec<(Option<String>, Vec<Area>)> {
        let edge = crate::edge::outline(a).unwrap();
        let mut p = Polygons::new();
        pour(&mut p, a, "F.Cu", &edge, rules).unwrap();
        p.areas_by_net()
    }

    /// The fill of F.Cu of `a` as polygons, by net.
    fn fill_by_net(a: &Artwork) -> Vec<(Option<String>, Vec<Area>)> {
        fill_with(a, &Rules::default())
    }

    fn size(areas: &[(Option<String>, Vec<Area>)], net: &str) -> f64 {
        areas
            .iter()
            .filter(|(n, _)| n.as_deref() == Some(&format!("N,{net}")))
            .flat_map(|(_, a)| a.iter().map(Area::size))
            .sum()
    }

    /// Whether the fill of `net` is copper at (x, y).
    fn copper(areas: &[(Option<String>, Vec<Area>)], net: &str, x: f64, y: f64) -> bool {
        areas
            .iter()
            .filter(|(n, _)| n.as_deref() == Some(&format!("N,{net}")))
            .any(|(_, a)| a.iter().any(|a| a.contains(Pt::new(x, y))))
    }

    #[test]
    fn a_pour_is_dark_regions_of_its_net_less_the_clearance_round_other_copper() {
        let a = art(&format!(
            r#"{GND_VIA} (segment (start 5 10) (end 15 10) (width 0.2) (layer "F.Cu") (net 2))"#
        ));
        let t = poured(&a);
        assert!(t.contains("G36*") && t.contains("%TO.N,GND*%"), "{t}");
        assert!(t.trim_end().ends_with("M02*"));
        // The track is cleared to its half width and the clearance: 0.1 + 0.3.
        let f = fill_by_net(&a);
        assert!(!copper(&f, "GND", 10.0, 10.35) && copper(&f, "GND", 10.0, 10.45));
    }

    #[test]
    fn a_pour_keeps_the_clearance_of_the_class_of_another_net_when_it_is_larger() {
        let a = art(&format!(
            r#"{GND_VIA} (segment (start 5 10) (end 15 10) (width 0.2) (layer "F.Cu") (net 2))"#
        ));
        let class = |clearance: f64, pattern: &str| crate::drc::Class {
            name: format!("Clearance_{clearance}mm"),
            clearance: Some(clearance),
            priority: 0,
            patterns: vec![pattern.to_string()],
            ..Default::default()
        };
        let rules = Rules {
            edge_clearance: 0.2,
            hole_clearance: None,
            classes: vec![class(0.5, "SIG")],
        };
        // The track is cleared to its half width and its class's clearance: 0.1 + 0.5.
        let f = fill_with(&a, &rules);
        assert!(!copper(&f, "GND", 10.0, 10.55) && copper(&f, "GND", 10.0, 10.65));
        assert_eq!(rules.class_clearance(Some("SIG")), 0.5);
        assert_eq!(rules.class_clearance(Some("GND")), 0.0);
    }

    #[test]
    fn a_pour_keeps_its_clearance_from_a_teardrop_of_another_net() {
        let drop = r#"(zone (net 2) (net_name "SIG") (layer "F.Cu") (attr (teardrop (type padvia)))
              (connect_pads yes (clearance 0)) (polygon (pts (xy 5 9) (xy 6 9.9) (xy 6 10.1) (xy 5 11)))
              (filled_polygon (layer "F.Cu") (pts (xy 5 9) (xy 6 9.9) (xy 6 10.1) (xy 5 11))))"#;
        let f = fill_by_net(&art(&format!("{GND_VIA} {drop}")));
        // Inside it, and 0.2 mm from its edge, less than the clearance.
        assert!(!copper(&f, "GND", 5.5, 10.0) && !copper(&f, "GND", 4.8, 10.0));
        assert!(copper(&f, "GND", 4.6, 10.0));
        // Of the pour's own net, nothing is cleared round it.
        let own = drop.replace("(net 2) (net_name \"SIG\")", "(net 1) (net_name \"GND\")");
        let f = fill_by_net(&art(&format!("{GND_VIA} {own}")));
        assert!(copper(&f, "GND", 5.5, 10.0) && copper(&f, "GND", 4.8, 10.0));
    }

    #[test]
    fn a_track_of_the_pours_own_net_is_not_cleared() {
        let a = art(r#"(segment (start 5 10) (end 15 10) (width 0.2) (layer "F.Cu") (net 1))"#);
        assert!(copper(&fill_by_net(&a), "GND", 10.0, 10.2));
    }

    fn pad(net: &str) -> String {
        format!(
            r#"(footprint "L:R" (layer "F.Cu") (at 10 10)
                (property "Reference" "R1")
                (pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu") (net {net})))"#
        )
    }

    #[test]
    fn a_pad_of_the_pours_net_gets_a_gap_crossed_by_spokes_and_another_nets_pad_a_full_ring() {
        let f = fill_by_net(&art(&pad("1 \"GND\"")));
        // The spokes across the gap (0.5 to 1.0 mm from the centre), and the gap between them.
        for (x, y) in [(10.75, 10.0), (9.25, 10.0), (10.0, 10.75), (10.0, 9.25)] {
            assert!(copper(&f, "GND", x, y), "a spoke at ({x}, {y})");
        }
        assert!(!copper(&f, "GND", 10.7, 10.7), "the gap");
        let f = fill_by_net(&art(&format!("{GND_VIA} {}", pad("2 \"SIG\""))));
        // The pad is cleared by a rounded rectangle of 0.5 + 0.3 from its centre.
        assert!(!copper(&f, "GND", 10.75, 10.0) && copper(&f, "GND", 10.85, 10.0));
    }

    #[test]
    fn copper_narrower_than_the_minimum_thickness_is_removed() {
        // Two tracks of SIG 1 mm apart leave a strip of GND 0.2 mm wide between them.
        let tracks = format!(
            r#"{GND_VIA} (segment (start 5 10) (end 15 10) (width 0.2) (layer "F.Cu") (net 2))
            (segment (start 5 11) (end 15 11) (width 0.2) (layer "F.Cu") (net 2))"#
        );
        assert!(!copper(&fill_by_net(&art(&tracks)), "GND", 10.0, 10.5));
        let thin = art_with(0.1, "", &tracks);
        assert!(copper(&fill_by_net(&thin), "GND", 10.0, 10.5));
        // A zone with no minimum keeps it.
        let none = art_with(0.0, "", &tracks);
        assert!(copper(&fill_by_net(&none), "GND", 10.0, 10.5));
    }

    /// A square ring of SIG round the middle of the board, which leaves GND an island inside.
    fn ring() -> String {
        let side = |(x0, y0): (f64, f64), (x1, y1): (f64, f64)| {
            format!(
                r#"(segment (start {x0} {y0}) (end {x1} {y1}) (width 0.2) (layer "F.Cu") (net 2))"#
            )
        };
        let c = [(5.0, 5.0), (15.0, 5.0), (15.0, 15.0), (5.0, 15.0)];
        (0..4).map(|i| side(c[i], c[(i + 1) % 4])).collect()
    }

    #[test]
    fn an_island_is_removed_as_the_zone_says() {
        let items = format!("{GND_VIA} {}", ring());
        let f = fill_by_net(&art(&items));
        assert!(!copper(&f, "GND", 10.0, 10.0), "the island goes");
        assert!(copper(&f, "GND", 3.0, 3.0), "the pour round the via stays");
        // Joined by a via of its net, it stays.
        let joined = format!(
            r#"{items} (via (at 10 10) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))"#
        );
        assert!(copper(&fill_by_net(&art(&joined)), "GND", 10.0, 10.0));
        // Kept by the zone, or kept because it is not smaller than the zone's least island
        // (the inside of the ring is 9.2 mm square, 84.6 mm²).
        let keep = art_with(0.25, "(island_removal_mode 1)", &items);
        assert!(copper(&fill_by_net(&keep), "GND", 10.0, 10.0));
        let big = art_with(0.25, "(island_removal_mode 2) (island_area_min 50)", &items);
        assert!(copper(&fill_by_net(&big), "GND", 10.0, 10.0));
        let small = art_with(
            0.25,
            "(island_removal_mode 2) (island_area_min 100)",
            &items,
        );
        assert!(!copper(&fill_by_net(&small), "GND", 10.0, 10.0));
        // A zone that touches nothing of its net is all island.
        assert_eq!(size(&fill_by_net(&art(&ring())), "GND"), 0.0);
    }

    #[test]
    fn two_pieces_of_zones_of_a_net_that_touch_only_each_other_are_islands() {
        // Inside the SIG ring, a GND zone of a higher priority over part of the inside: its piece
        // and the lower zone's piece beside it touch each other and nothing else of GND, so KiCad
        // removes both (issue #1222). netsmith once kept both, each joined by the other.
        let higher = r#"(zone (net 1) (net_name "GND") (layer "F.Cu") (priority 1) (connect_pads (clearance 0.3)) (min_thickness 0.25)
              (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5))
              (polygon (pts (xy 7 7) (xy 10 7) (xy 10 13) (xy 7 13))))"#;
        let items = format!("{GND_VIA} {} {higher}", ring());
        let f = fill_by_net(&art(&items));
        assert!(
            !copper(&f, "GND", 8.5, 10.0),
            "the higher zone's island goes"
        );
        assert!(
            !copper(&f, "GND", 12.0, 10.0),
            "the lower zone's island beside it goes"
        );
        assert!(copper(&f, "GND", 3.0, 3.0), "the pour round the via stays");
        // A via of GND in the higher zone joins both.
        let joined = format!(
            r#"{items} (via (at 8.5 10) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))"#
        );
        let f = fill_by_net(&art(&joined));
        assert!(copper(&f, "GND", 9.5, 10.0) && copper(&f, "GND", 12.0, 10.0));
    }

    #[test]
    fn where_two_zones_of_a_net_meet_neither_rounds_its_corner() {
        // A GND zone of a higher priority from y = 5 to 9, and a SIG pad across its lower edge,
        // whose clearance (0.3 mm round the 1 mm pad) runs down across that edge at x = 10.8.
        // KiCad fills each zone as if the other's area within its minimum thickness were its own
        // (issue #1222), so neither zone's corner there is rounded by the trimming: no notch
        // between them, with a spike of copper beside it that KiCad's check calls a sliver.
        let higher = r#"(zone (net 1) (net_name "GND") (layer "F.Cu") (priority 1) (connect_pads (clearance 0.3)) (min_thickness 0.25)
              (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5))
              (polygon (pts (xy 5 5) (xy 15 5) (xy 15 9) (xy 5 9))))"#;
        let sig = r#"(footprint "L:R" (layer "F.Cu") (at 10 9) (property "Reference" "U1")
                (pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu") (net 2 "SIG")))"#;
        let items = format!(
            r#"{GND_VIA} {higher} {sig} (via (at 14 6) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))"#
        );
        let f = fill_by_net(&art(&items));
        // Just right of the clearance, just above and just below the higher zone's edge.
        for y in [8.99, 9.01] {
            assert!(
                copper(&f, "GND", 10.81 + MARGIN, y),
                "the corner at y = {y}"
            );
        }
    }

    #[test]
    fn a_zone_of_no_net_keeps_its_islands() {
        let a = art(&ring());
        let a = Artwork {
            zones: a
                .zones
                .into_iter()
                .map(|mut z| {
                    z.net = None;
                    z
                })
                .collect(),
            ..a
        };
        let f = fill_by_net(&a);
        assert!(
            f.iter()
                .any(|(_, areas)| { areas.iter().any(|a| a.contains(Pt::new(10.0, 10.0))) })
        );
    }

    #[test]
    fn a_spoke_whose_end_is_in_another_nets_clearance_is_removed() {
        // A SIG track at x = 11.3 clears GND from 10.8 to 11.8: the spoke to the right (10.5 to
        // 11.0) ends in it, and would be a stub from 10.5 to 10.8.
        let items = format!(
            r#"{} (segment (start 11.3 5) (end 11.3 15) (width 0.2) (layer "F.Cu") (net 2))"#,
            pad("1 \"GND\"")
        );
        let f = fill_by_net(&art(&items));
        assert!(!copper(&f, "GND", 10.65, 10.0), "the stub is gone");
        for (x, y) in [(9.25, 10.0), (10.0, 10.75), (10.0, 9.25)] {
            assert!(copper(&f, "GND", x, y), "the spoke at ({x}, {y}) stays");
        }
    }

    #[test]
    fn a_dead_spoke_wider_than_its_pad_leaves_the_root_of_a_live_one() {
        // A pad 0.3 mm tall, narrower than the 0.5 mm bridge: the spokes to the left and the
        // right are 0.05 mm taller than the pad on each side, over the root of the spoke up.
        // SIG tracks at x = 8.7 and 11.3 and y = 11.0 put the ends of the spokes to the left,
        // the right and down in their clearance; the spoke up (y 9.85 to 9.35) reaches the fill
        // and has to stay joined to the pad (a QFN pin of the rp2350_minimal port lost its only spoke so).
        let items = format!(
            r#"{GND_VIA}
            (footprint "L:R" (layer "F.Cu") (at 10 10) (property "Reference" "U1")
                (pad "1" smd rect (at 0 0) (size 1 0.3) (layers "F.Cu") (net 1 "GND")))
            (segment (start 8.7 5) (end 8.7 15) (width 0.2) (layer "F.Cu") (net 2))
            (segment (start 11.3 5) (end 11.3 15) (width 0.2) (layer "F.Cu") (net 2))
            (segment (start 5 11.0) (end 15 11.0) (width 0.2) (layer "F.Cu") (net 2))"#
        );
        let f = fill_by_net(&art(&items));
        for (x, y) in [(10.1, 9.8), (10.1, 9.6), (10.1, 9.4)] {
            assert!(copper(&f, "GND", x, y), "the spoke up at ({x}, {y}) stays");
        }
        for (x, y) in [(10.75, 10.0), (9.25, 10.0), (10.0, 10.4)] {
            assert!(
                !copper(&f, "GND", x, y),
                "the dead spoke at ({x}, {y}) goes"
            );
        }
    }

    /// A row of three 0.6 x 1 mm pads at (x, y): `left`, GND and `right`, 1.55 mm apart, so that
    /// the clearances of the outer two (0.3 mm) leave 0.15 mm of fill beside the GND pad's relief.
    fn row(refdes: &str, x: f64, y: f64, left: &str, right: &str) -> String {
        format!(
            r#"(footprint "L:R" (layer "F.Cu") (at {x} {y}) (property "Reference" "{refdes}")
                (pad "1" smd rect (at -1.55 0) (size 0.6 1) (layers "F.Cu") (net {left}))
                (pad "2" smd rect (at 0 0) (size 0.6 1) (layers "F.Cu") (net 1 "GND"))
                (pad "3" smd rect (at 1.55 0) (size 0.6 1) (layers "F.Cu") (net {right})))"#
        )
    }

    /// The spokes of each GND pad that reach the fill, by its part, as the check counts them.
    /// [`pad_joins`] of F.Cu.
    fn layer_joins(a: &Artwork, edge: &BoardEdge) -> Vec<ZoneJoins> {
        pad_joins(a, edge, &rules())
            .unwrap()
            .into_iter()
            .find(|(l, _)| l == "F.Cu")
            .map(|(_, j)| j)
            .unwrap_or_default()
    }

    fn reaching(a: &Artwork) -> Vec<(String, usize, usize)> {
        let edge = crate::edge::outline(a).unwrap();
        let mut out: Vec<(String, usize, usize)> = judge(a, "F.Cu", &edge, &rules(), None)
            .unwrap()
            .into_iter()
            .flat_map(|j| j.reliefs)
            .map(|r| (a.pads[r.pad].part.clone(), r.spokes, r.reach))
            .collect();
        out.sort();
        out
    }

    #[test]
    fn a_spoke_whose_end_only_a_strip_thinner_than_the_minimum_reaches_is_not_made() {
        // The board of issue #1115: U1 between two other nets' pads, the pour's lower edge 0.85
        // mm below it; U2 between two others in the open pour; U3 alone. KiCad's refill gives
        // them one spoke (up), two (up and down) and four; netsmith once counted three, four
        // and four, its spokes to the sides reaching the 0.15 mm strip beside the relief.
        let items = format!(
            r#"{GND_VIA} {} {}
            (footprint "L:R" (layer "F.Cu") (at 10 4) (property "Reference" "U3")
                (pad "1" smd rect (at 0 0) (size 0.6 1) (layers "F.Cu") (net 1 "GND")))"#,
            row("U1", 6.0, 18.95, "2 \"SIG\"", "3 \"SIG2\""),
            row("U2", 12.0, 10.0, "2 \"SIG\"", "3 \"SIG2\""),
        );
        let a = Artwork::parse(&format!(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal))
            (net 0 "") (net 1 "GND") (net 2 "SIG") (net 3 "SIG2")
            (gr_rect (start 0 0) (end 20 20) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))
            (zone (net 1) (net_name "GND") (layer "F.Cu") (connect_pads (clearance 0.3)) (min_thickness 0.25)
              (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5))
              (polygon (pts (xy 0.2 0.2) (xy 19.8 0.2) (xy 19.8 19.8) (xy 0.2 19.8))))
            {items})"#
        ))
        .unwrap();
        assert_eq!(
            reaching(&a),
            [
                ("U1".to_string(), 4, 1),
                ("U2".to_string(), 4, 2),
                ("U3".to_string(), 4, 4)
            ]
        );
        // The spokes to the sides are not drawn: no stub of them is left on the pad.
        let f = fill_by_net(&a);
        for x in [11.4, 12.6] {
            assert!(!copper(&f, "GND", x, 10.0), "a stub at ({x}, 10)");
        }
        assert!(copper(&f, "GND", 12.0, 9.25) && copper(&f, "GND", 12.0, 10.75));
    }

    #[test]
    fn a_spoke_is_no_wider_than_its_pad_and_a_pad_narrower_than_the_minimum_has_none() {
        // A 0.3 mm tall pad: its spokes to the sides are 0.3 mm wide, not the zone's 0.5 mm.
        let pad = |h: f64| {
            format!(
                r#"{GND_VIA} (footprint "L:R" (layer "F.Cu") (at 10 10) (property "Reference" "U1")
                (pad "1" smd rect (at 0 0) (size 1 {h}) (layers "F.Cu") (net 1 "GND")))"#
            )
        };
        let f = fill_by_net(&art(&pad(0.3)));
        assert!(copper(&f, "GND", 10.75, 10.1) && !copper(&f, "GND", 10.75, 10.2));
        assert_eq!(reaching(&art(&pad(0.3))), [("U1".to_string(), 4, 4)]);
        // A 0.2 mm tall pad is under the zone's 0.25 mm minimum: KiCad makes it no spoke, and
        // the whole gap is cleared.
        let f = fill_by_net(&art(&pad(0.2)));
        for (x, y) in [(10.75, 10.0), (10.0, 9.6), (10.0, 10.4)] {
            assert!(!copper(&f, "GND", x, y), "a spoke at ({x}, {y})");
        }
        assert_eq!(reaching(&art(&pad(0.2))), [("U1".to_string(), 0, 0)]);
    }

    #[test]
    fn a_pads_own_spoke_width_and_connection_hold_over_the_zones() {
        // The zone's spokes are 0.5 mm; the pad's own `thermal_bridge_width` (a pin's
        // `.spoke_width`, issue #1276) makes them 0.3 mm, as KiCad's filler does.
        let pad = |own: &str| {
            format!(
                r#"{GND_VIA} (footprint "L:R" (layer "F.Cu") (at 10 10) (property "Reference" "U1")
                (pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu") (net 1 "GND") {own}))"#
            )
        };
        let zone = fill_by_net(&art(&pad("")));
        assert!(copper(&zone, "GND", 10.75, 10.2));
        let own = art(&pad("(thermal_bridge_width 0.3)"));
        assert_eq!(own.pads[0].thermal_bridge, Some(0.3));
        let f = fill_by_net(&own);
        assert!(copper(&f, "GND", 10.75, 10.1) && !copper(&f, "GND", 10.75, 10.2));
        assert_eq!(reaching(&own), [("U1".to_string(), 4, 4)]);
        // Under the zone's minimum thickness the pad has no spoke, and the reason names the
        // pad's own width.
        let narrow = art(&pad("(thermal_bridge_width 0.2)"));
        let edge = crate::edge::outline(&narrow).unwrap();
        let joins = layer_joins(&narrow, &edge);
        assert_eq!(
            joins[0].pads[0].1.unjoinable().unwrap().to_string(),
            "the pad's own spoke width, 0.2 mm, is under the zone's minimum thickness, 0.25 mm, so the fill makes no spoke"
        );
        // A pad's own solid connection fills the gap the relief would leave.
        let solid = fill_by_net(&art(&pad("(zone_connect 2)")));
        assert!(copper(&solid, "GND", 10.75, 10.4));
        assert!(!copper(&zone, "GND", 10.75, 10.4));
    }

    /// `a` with its first zone's spoke width, thermal gap and minimum thickness set.
    fn with_relief(mut a: Artwork, spoke: f64, gap: f64, min: f64) -> Artwork {
        a.zones[0].thermal_bridge = spoke;
        a.zones[0].thermal_gap = gap;
        a.zones[0].min_thickness = min;
        a
    }

    #[test]
    fn a_spoke_as_wide_as_the_minimum_thickness_is_made_and_one_narrower_is_not() {
        // KiCad trims a micrometre less than half the minimum thickness, so a spoke exactly that
        // wide stays (issue #1222): netsmith once trimmed it away whole, and a narrower spoke
        // starved more pads than a wider one.
        let pad = format!(
            r#"{GND_VIA} (footprint "L:R" (layer "F.Cu") (at 10 10) (property "Reference" "U1")
                (pad "1" smd rect (at 0 0) (size 0.6 1) (layers "F.Cu") (net 1 "GND")))"#
        );
        for (spoke, min) in [(0.25, 0.25), (0.2, 0.2), (0.3, 0.25)] {
            let a = with_relief(art(&pad), spoke, 0.25, min);
            assert_eq!(
                reaching(&a),
                [("U1".to_string(), 4, 4)],
                "spoke {spoke}, minimum {min}"
            );
        }
        // A spoke narrower than the minimum thickness is not made, as KiCad makes none, and the
        // report says so.
        let a = with_relief(art(&pad), 0.2, 0.25, 0.25);
        assert_eq!(reaching(&a), [("U1".to_string(), 0, 0)]);
        let edge = crate::edge::outline(&a).unwrap();
        let joins = layer_joins(&a, &edge);
        let join = &joins[0].pads[0].1;
        assert_eq!(
            join.unjoinable(),
            Some(&Unjoinable::Narrow {
                width: 0.2,
                min: 0.25,
                by_pad: false,
                own: false
            })
        );
        assert_eq!(
            join.unjoinable().unwrap().to_string(),
            "the zone's spoke width, 0.2 mm, is under the zone's minimum thickness, 0.25 mm, so the fill makes no spoke"
        );
    }

    #[test]
    fn a_spoke_into_a_strip_beside_a_higher_zone_of_the_net_is_made_as_kicad_makes_it() {
        // A 1 mm GND pad at (10, 10), its relief to x = 11.0, and a GND zone of a higher priority
        // from x = 11.18: the strip of the lower zone between them is 0.18 mm wide, under the
        // minimum thickness, but KiCad takes the higher zone's area out of the lower zone's fill
        // last, after the spokes and the trimming, so the strip is part of the wide fill under
        // the higher zone when the spoke to the right is tried and trimmed, and the spoke is
        // made (issue #1222). SIG tracks keep the fill off the ends of the other three spokes.
        let items = format!(
            r#"{GND_VIA} {}
            (via (at 13 10) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))
            (segment (start 8.7 5) (end 8.7 15) (width 0.2) (layer "F.Cu") (net 2))
            (segment (start 5 8.7) (end 10.6 8.7) (width 0.2) (layer "F.Cu") (net 2))
            (segment (start 5 11.3) (end 10.6 11.3) (width 0.2) (layer "F.Cu") (net 2))
            (zone (net 1) (net_name "GND") (layer "F.Cu") (priority 1) (connect_pads (clearance 0.3)) (min_thickness 0.25)
              (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5))
              (polygon (pts (xy 11.18 5) (xy 15 5) (xy 15 15) (xy 11.18 15))))"#,
            pad("1 \"GND\"")
        );
        let a = art(&items);
        assert_eq!(reaching(&a), [("R1".to_string(), 4, 1)]);
        let f = fill_by_net(&a);
        assert!(copper(&f, "GND", 10.75, 10.0), "the spoke to the right");
        assert!(copper(&f, "GND", 11.1, 10.0), "the strip it joins");
    }

    #[test]
    fn a_pad_no_spoke_reaches_is_unjoinable_and_says_what_keeps_the_fill_off() {
        // The pad of `a_dead_spoke_wider_than_its_pad_leaves_the_root_of_a_live_one` with the
        // spoke up cut too, by the clearance of a SIG2 pad above it: SIG tracks at x = 8.7 and 11.3
        // and y = 11.0 cut the others.
        let items = format!(
            r#"{GND_VIA}
            (footprint "L:R" (layer "F.Cu") (at 10 10) (property "Reference" "U1")
                (pad "1" smd rect (at 0 0) (size 1 0.3) (layers "F.Cu") (net 1 "GND")))
            (footprint "L:R" (layer "F.Cu") (at 10 8.9) (property "Reference" "U2")
                (pad "1" smd rect (at 0 0) (size 1 0.3) (layers "F.Cu") (net 3 "SIG2")))
            (segment (start 8.7 5) (end 8.7 15) (width 0.2) (layer "F.Cu") (net 2))
            (segment (start 11.3 5) (end 11.3 15) (width 0.2) (layer "F.Cu") (net 2))
            (segment (start 5 11.0) (end 15 11.0) (width 0.2) (layer "F.Cu") (net 2))"#
        );
        let a = art(&items);
        assert_eq!(reaching(&a), [("U1".to_string(), 4, 0)]);
        let edge = crate::edge::outline(&a).unwrap();
        let joins = layer_joins(&a, &edge);
        let (_, join) = joins[0].pads.iter().find(|(p, _)| p.part == "U1").unwrap();
        assert!(!join.starved(), "no spoke: not starved");
        let Some(Unjoinable::Blocked(blockers)) = join.unjoinable() else {
            panic!("{join:?}");
        };
        // The spokes to the right, up, left and down, as the pad's axes turn them.
        let mut said: Vec<String> = blockers.iter().map(ToString::to_string).collect();
        said.sort();
        assert_eq!(
            said,
            [
                "the clearance of pad U2.1 [SIG2] (0.3 mm)",
                "the clearance of track [SIG] (0.3 mm)",
                "the clearance of track [SIG] (0.3 mm)",
                "the clearance of track [SIG] (0.3 mm)",
            ]
        );
    }

    #[test]
    fn a_relief_no_spoke_reaches_is_not_unjoinable_where_another_fill_of_its_net_joins_the_pad() {
        // Issue #1292. A 1 mm GND pad at (10, 10) whose four spokes on F.Cu SIG tracks cut.
        let cut = r#"
            (segment (start 8.7 5) (end 8.7 15) (width 0.2) (layer "F.Cu") (net 2))
            (segment (start 11.3 5) (end 11.3 15) (width 0.2) (layer "F.Cu") (net 2))
            (segment (start 9 8.7) (end 11 8.7) (width 0.2) (layer "F.Cu") (net 2))
            (segment (start 9 11.3) (end 11 11.3) (width 0.2) (layer "F.Cu") (net 2))"#;
        let unjoinable = |items: &str| -> Vec<(String, bool)> {
            let a = art(&format!("{GND_VIA} {cut} {items}"));
            let edge = crate::edge::outline(&a).unwrap();
            let mut out: Vec<(String, bool)> = pad_joins(&a, &edge, &rules())
                .unwrap()
                .into_iter()
                .flat_map(|(l, z)| z.into_iter().map(move |z| (l.clone(), z)))
                .flat_map(|(l, z)| {
                    z.pads
                        .into_iter()
                        .filter(|(_, j)| matches!(j, Join::Thermal { reach: 0, .. }))
                        .map(move |(_, j)| (l.clone(), j.unjoinable().is_some()))
                })
                .collect();
            out.sort();
            out
        };
        let smd = r#"(footprint "L:R" (layer "F.Cu") (at 10 10) (property "Reference" "R1")
            (pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu") (net 1 "GND")))"#;
        assert_eq!(unjoinable(smd), [("F.Cu".to_string(), true)]);
        // A solid GND zone of a higher priority over the pad on F.Cu: the pour gives it the
        // area, and its fill joins the pad.
        let solid = r#"(zone (net 1) (net_name "GND") (layer "F.Cu") (priority 1) (connect_pads yes (clearance 0.3)) (min_thickness 0.25)
            (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5))
            (polygon (pts (xy 9.2 9.2) (xy 10.8 9.2) (xy 10.8 10.8) (xy 9.2 10.8))))"#;
        assert_eq!(
            unjoinable(&format!("{smd} {solid}")),
            [("F.Cu".to_string(), false)]
        );
        // A through-hole pad that a solid GND zone on B.Cu joins.
        let tht = r#"(footprint "L:R" (layer "F.Cu") (at 10 10) (property "Reference" "J1")
            (pad "1" thru_hole rect (at 0 0) (size 1 1) (drill 0.5) (layers "*.Cu") (net 1 "GND")))"#;
        assert_eq!(unjoinable(tht), [("F.Cu".to_string(), true)]);
        let back = r#"(zone (net 1) (net_name "GND") (layer "B.Cu") (connect_pads yes (clearance 0.3)) (min_thickness 0.25)
            (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5))
            (polygon (pts (xy 0.2 0.2) (xy 19.8 0.2) (xy 19.8 19.8) (xy 0.2 19.8))))"#;
        assert_eq!(
            unjoinable(&format!("{tht} {back}")),
            [("F.Cu".to_string(), false)]
        );
    }

    #[test]
    fn two_zones_of_different_nets_that_overlap_are_refused() {
        let a = art(
            r#"(zone (net 2) (net_name "SIG") (layer "F.Cu") (connect_pads (clearance 0.3))
              (polygon (pts (xy 5 5) (xy 15 5) (xy 15 15) (xy 5 15))))"#,
        );
        let edge = crate::edge::outline(&a).unwrap();
        let mut g = Gerber::new(Pt::default());
        let err = pour(&mut g, &a, "F.Cu", &edge, &Rules::default()).unwrap_err();
        assert!(
            err.contains("GND") && err.contains("SIG") && err.contains("F.Cu"),
            "{err}"
        );
    }

    #[test]
    fn of_two_zones_of_different_nets_that_overlap_the_higher_priority_has_the_area() {
        // SIG, priority 1, over the middle of the GND zone (priority 0): GND keeps 0.3 mm
        // (the larger clearance) away from SIG's fill. A via of each net joins its zone.
        // No minimum thickness, so that no corner is rounded and the areas are exact.
        let sig = r#"(via (at 10 10) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 2))"#;
        let a = art_with(
            0.0,
            "",
            &format!(
                r#"{GND_VIA} {sig} (zone (net 2) (net_name "SIG") (layer "F.Cu") (priority 1) (connect_pads (clearance 0.2))
              (polygon (pts (xy 5 5) (xy 15 5) (xy 15 15) (xy 5 15))))"#,
            ),
        );
        let by_net = fill_by_net(&a);
        let sig_size = size(&by_net, "SIG");
        let gnd = size(&by_net, "GND");
        assert!((sig_size - 100.0).abs() < 1e-6, "{sig_size}");
        // GND: the board less its edge band (19.6 - 2 * 0.2 on each side, 0.2 mm band from
        // the edge at 0) less SIG's square grown by 0.3 with round corners.
        // The space round SIG's fill is MARGIN wider than its 0.3 mm.
        let space = 0.3 + MARGIN;
        let grown = (10.0 + 2.0 * space).powi(2) - (4.0 - std::f64::consts::PI) * space * space;
        let board = 19.6 * 19.6;
        assert!((gnd - (board - grown)).abs() < 0.01, "{gnd}");
        // The other way round, GND has the middle and SIG nothing.
        let b = art_with(
            0.0,
            "",
            &format!(
                r#"{GND_VIA} {sig} (zone (net 2) (net_name "SIG") (layer "F.Cu") (priority 0) (connect_pads (clearance 0.2))
              (polygon (pts (xy 5 5) (xy 15 5) (xy 15 15) (xy 5 15))))"#,
            ),
        );
        let b = Artwork {
            zones: b
                .zones
                .into_iter()
                .map(|mut z| {
                    if z.net.as_deref() == Some("GND") {
                        z.priority = 2;
                    }
                    z
                })
                .collect(),
            ..b
        };
        // SIG keeps only the copper under its via (radius 0.3), where GND keeps away from it.
        let under = size(&fill_by_net(&b), "SIG");
        assert!(under <= std::f64::consts::PI * 0.09 + 1e-9, "{under}");
    }

    #[test]
    fn a_zones_further_polygons_are_holes() {
        let a = Artwork::parse(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal)) (net 1 "GND")
            (gr_rect (start 0 0) (end 20 20) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))
            (via (at 3 3) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))
            (zone (net 1) (net_name "GND") (layer "F.Cu") (connect_pads (clearance 0.3))
              (polygon (pts (xy 2 2) (xy 18 2) (xy 18 18) (xy 2 18)))
              (polygon (pts (xy 5 5) (xy 10 5) (xy 10 10) (xy 5 10)))))"#,
        )
        .unwrap();
        assert_eq!((a.zones[0].outlines.len(), a.zones[0].holes.len()), (1, 1));
        let gnd = size(&fill_by_net(&a), "GND");
        assert!((gnd - (256.0 - 25.0)).abs() < 1e-6, "{gnd}");
    }

    #[test]
    fn a_fill_the_file_already_holds_is_not_filled_again() {
        let a = Artwork::parse(
            r#"(kicad_pcb (layers (0 "F.Cu" signal)) (net 1 "GND")
            (zone (net 1) (net_name "GND") (layer "F.Cu") (connect_pads (clearance 0.3))
              (polygon (pts (xy 0 0) (xy 5 0) (xy 5 5) (xy 0 5)))
              (filled_polygon (layer "F.Cu") (pts (xy 0 0) (xy 4 0) (xy 4 4) (xy 0 4)))))"#,
        )
        .unwrap();
        let edge = BoardEdge {
            outer: vec![Pt::new(0.0, 0.0), Pt::new(5.0, 0.0), Pt::new(5.0, 5.0)],
            cutouts: Vec::new(),
        };
        let mut g = Gerber::new(Pt::default());
        pour(&mut g, &a, "F.Cu", &edge, &Rules::default()).unwrap();
        assert!(g.is_empty());
    }

    #[test]
    fn a_zones_island_setting_is_read_from_its_fill() {
        assert_eq!(art("").zones[0].islands, Islands::Remove);
        let keep = art_with(0.25, "(island_removal_mode 1)", "");
        assert_eq!(keep.zones[0].islands, Islands::Keep);
        let below = art_with(0.25, "(island_removal_mode 2) (island_area_min 3.5)", "");
        assert_eq!(below.zones[0].islands, Islands::RemoveBelow(3.5));
    }
}
