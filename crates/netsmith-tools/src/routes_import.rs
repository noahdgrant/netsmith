//! The import of a KiCad board's copper into a routes file (`netsmith routes import`, issue
//! #599): its tracks, arcs, vias and zones (issue #724) by the design's net names, and where its
//! parts stand, for the layout file. The routes file itself is `netsmith_fabricate::routes`.

use std::collections::BTreeMap;

use crate::copper::{Kind, ZoneKind};
use netsmith_pcb::{
    Board, BoardRoutes, LayoutFace as Face, NetRoutes, PartPlace, RouteTrack, RouteVia, RouteZone,
};

/// What [`import`] read from a KiCad board for one board of the design.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Import {
    /// Each net's copper, by the design's net name, with no pad hash yet ([`Import::finish`]).
    pub nets: BTreeMap<String, NetRoutes>,
    /// Where each part of the design the KiCad board has stands, by its path, for the layout file.
    pub places: BTreeMap<String, PartPlace>,
    /// The KiCad net each net of the design was taken from, and how many of the design's pads
    /// vouched for it.
    pub mapped: Vec<(String, String, usize)>,
    /// The parts placed otherwise than the KiCad board's footprint is, so that their pads stand
    /// where the KiCad board has the pads of their nets ([`fit`]), each a sentence that names
    /// the part by its path and says how and why.
    pub refitted: Vec<String>,
    /// What could not be taken, each a sentence: a net no pad maps, a net whose pads map to two of
    /// the design's, a shape on an inner layer, a part the design does not have, a rule area, a
    /// teardrop, a hatched zone.
    pub skipped: Vec<String>,
}

impl Import {
    /// The table for the routes file, with each net's pad hash taken from `board`, which is the
    /// design's board placed with the imported places.
    pub fn finish(&self, board: &Board) -> BoardRoutes {
        let mut nets = BTreeMap::new();
        for (net, record) in &self.nets {
            let Some(pads) = netsmith_pcb::pad_hash(board, net) else {
                continue;
            };
            nets.insert(
                net.clone(),
                NetRoutes {
                    pads,
                    ..record.clone()
                },
            );
        }
        BoardRoutes { nets }
    }

    /// How many zones were taken.
    pub fn zones(&self) -> usize {
        self.nets.values().map(|n| n.zones.len()).sum()
    }

    /// How many tracks, arcs and vias were taken.
    pub fn counts(&self) -> (usize, usize, usize) {
        let tracks = self.nets.values().flat_map(|n| &n.tracks);
        let arcs = tracks.clone().filter(|t| t.mid.is_some()).count();
        let vias = self.nets.values().map(|n| n.vias.len()).sum();
        (tracks.count() - arcs, arcs, vias)
    }
}

/// The votes of the design's pads for each KiCad net: how many pads of each of the design's
/// nets a KiCad net is on, where the KiCad board's parts are the design's by `ours_of` (the
/// design's reference of each KiCad reference). A KiCad part in `crossed` has its two pads
/// numbered the other way round from the design's footprint ([`crossed_parts`]): its pad 1 is
/// the design's pad 2.
fn votes(
    board: &Board,
    kicad: &crate::reference_table::Board,
    ours_of: &BTreeMap<String, String>,
    crossed: &BTreeMap<String, (String, String)>,
) -> BTreeMap<String, BTreeMap<String, usize>> {
    let mut net_of_pad: BTreeMap<(&str, &str), &str> = BTreeMap::new();
    for net in &board.nets {
        for pad in &net.pads {
            net_of_pad.insert(
                (pad.refdes.as_str(), pad.number.as_str()),
                net.name.as_str(),
            );
        }
    }
    let mut votes: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for part in &kicad.parts {
        let Some(ours) = ours_of.get(&part.reference) else {
            continue;
        };
        for pad in &part.pads {
            let Some(theirs) = &pad.net else { continue };
            let number = match crossed.get(&part.reference) {
                Some((a, b)) if &pad.number == a => b,
                Some((a, b)) if &pad.number == b => a,
                _ => &pad.number,
            };
            if let Some(net) = net_of_pad.get(&(ours.as_str(), number.as_str())) {
                *votes
                    .entry(theirs.clone())
                    .or_default()
                    .entry(net.to_string())
                    .or_default() += 1;
            }
        }
    }
    votes
}

/// The KiCad parts with two pads whose numbers are the other way round from the design's: the
/// copper of the KiCad board reaches its pad 1 on the net that the design has on pad 2, and the
/// other way round, as a resistor drawn turned by half a turn in one schematic is. Each is
/// mapped to its two pad numbers. It is so when the pads of every other part vouch more for the
/// crossed pairing (KiCad's pad 1 net with the design's pad 2 net, and pad 2 with pad 1) than for
/// the straight one, and the design's footprint is the same turned by half a turn (its two pads
/// mirrored through its origin, of one size), so that the import can place it so and the copper
/// ends on the same places.
fn crossed_parts(
    board: &Board,
    kicad: &crate::reference_table::Board,
    ours_of: &BTreeMap<String, String>,
    votes: &BTreeMap<String, BTreeMap<String, usize>>,
) -> BTreeMap<String, (String, String)> {
    let mut net_of_pad: BTreeMap<(&str, &str), &str> = BTreeMap::new();
    for net in &board.nets {
        for pad in &net.pads {
            net_of_pad.insert(
                (pad.refdes.as_str(), pad.number.as_str()),
                net.name.as_str(),
            );
        }
    }
    let mut crossed = BTreeMap::new();
    for part in &kicad.parts {
        let Some(ours) = ours_of
            .get(&part.reference)
            .and_then(|r| board.parts.iter().find(|p| &p.refdes == r))
        else {
            continue;
        };
        let pads = &ours.footprint.pads;
        let [p, q] = pads.as_slice() else { continue };
        let mirrored = (p.at.x + q.at.x).abs() < 1e-6
            && (p.at.y + q.at.y).abs() < 1e-6
            && (p.width - q.width).abs() < 1e-6
            && (p.height - q.height).abs() < 1e-6
            && p.number != q.number;
        if !mirrored {
            continue;
        }
        let theirs = |number: &str| {
            part.pads
                .iter()
                .find(|pad| pad.number == number)
                .and_then(|pad| pad.net.clone())
        };
        let (Some(t1), Some(t2)) = (theirs(&p.number), theirs(&q.number)) else {
            continue;
        };
        let (Some(o1), Some(o2)) = (
            net_of_pad.get(&(ours.refdes.as_str(), p.number.as_str())),
            net_of_pad.get(&(ours.refdes.as_str(), q.number.as_str())),
        ) else {
            continue;
        };
        if t1 == t2 || o1 == o2 {
            continue;
        }
        // The votes of the other parts' pads: this part's own pads vote for the straight
        // pairing, once each.
        let support = |t: &str, o: &str, own: bool| {
            votes
                .get(t)
                .and_then(|v| v.get(o))
                .copied()
                .unwrap_or(0)
                .saturating_sub(usize::from(own))
        };
        let straight = support(&t1, o1, true) + support(&t2, o2, true);
        let across = support(&t1, o2, false) + support(&t2, o1, false);
        if across > straight {
            crossed.insert(part.reference.clone(), (p.number.clone(), q.number.clone()));
        }
    }
    crossed
}

/// The design's net of each KiCad net: the net its pads are on in the design, by the most pads
/// ([`votes`]). A KiCad net no pad maps keeps its name when the design has a net of that name
/// (a leading `/` and the case aside). Returns the map and what was not mapped.
fn map_nets(
    board: &Board,
    kicad: &crate::reference_table::Board,
    votes: &BTreeMap<String, BTreeMap<String, usize>>,
) -> (BTreeMap<String, (String, usize)>, Vec<String>) {
    let plain = |n: &str| n.trim_start_matches('/').to_lowercase();
    let mut map = BTreeMap::new();
    let mut said = Vec::new();
    let mut theirs: Vec<String> = kicad
        .parts
        .iter()
        .flat_map(|p| p.pads.iter().filter_map(|pad| pad.net.clone()))
        .collect();
    theirs.sort();
    theirs.dedup();
    for name in theirs {
        match votes.get(&name) {
            Some(v) => {
                let mut v: Vec<(&String, &usize)> = v.iter().collect();
                v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
                if v.len() > 1 {
                    said.push(format!(
                        "KiCad net `{name}` is on pads of {} of the design's nets ({}): its copper is taken as `{}`'s",
                        v.len(),
                        v.iter()
                            .map(|(n, c)| format!("`{n}` {c}"))
                            .collect::<Vec<_>>()
                            .join(", "),
                        v[0].0
                    ));
                }
                map.insert(name, (v[0].0.clone(), *v[0].1));
            }
            None => {
                if let Some(net) = board.nets.iter().find(|n| plain(&n.name) == plain(&name)) {
                    map.insert(name, (net.name.clone(), 0));
                }
            }
        }
    }
    (map, said)
}

/// Where the import places a part, from the outline's top left corner, and how it is turned.
struct Fitted {
    at: [f64; 2],
    rotation: f64,
    /// The part's two pads are numbered the other way round from the KiCad board's.
    crossed: bool,
    /// How far the place is from the KiCad footprint's origin, in mm.
    moved: [f64; 2],
}

impl Fitted {
    /// What the import says of a part placed otherwise than the KiCad board's footprint
    /// (turned `kicad` degrees), or `None` when it is placed as that footprint is.
    fn why(&self, kicad: f64) -> Option<String> {
        let turn = (self.rotation - kicad).rem_euclid(360.0).round();
        let moved = self.moved[0].hypot(self.moved[1]) > 1e-6;
        if (turn == 0.0 || turn == 360.0) && !moved {
            return None;
        }
        let mut how = Vec::new();
        if turn != 0.0 && turn != 360.0 {
            how.push(format!("turned by {turn} degrees"));
        }
        if moved {
            how.push(format!(
                "moved by ({}, {}) mm",
                (self.moved[0] * 1e6).round() / 1e6,
                (self.moved[1] * 1e6).round() / 1e6
            ));
        }
        Some(format!(
            "is placed {} from the KiCad board's footprint, so that its pads stand where the KiCad board has them: {}",
            how.join(" and "),
            if self.crossed {
                "KiCad numbers its two pads the other way round"
            } else {
                "netsmith's footprint is drawn in another frame"
            }
        ))
    }
}

/// The place and turn of the design's part `ours` that put its pads where the KiCad board's part
/// `theirs` has its pads of the same number (`crossed`: the two pad numbers that are the other
/// way round, [`crossed_parts`]). Of the KiCad turn and the three quarter turns from it, the one
/// whose pads are nearest the KiCad pads is taken (the KiCad turn on a tie); then, if moving
/// the part by the mean of the pads' offsets puts every pad within 0.01 mm of the KiCad one
/// where it was not, the part is moved so (its footprint's origin is elsewhere in the part). A
/// footprint whose pads differ in size or spacing from the KiCad one keeps the KiCad origin.
/// `origin` is the KiCad board's outline corner. Places are from the outline's top left corner.
fn fit(
    board: &Board,
    ours: &netsmith_pcb::Placed,
    theirs: &crate::reference_table::Part,
    origin: (f64, f64),
    crossed: Option<&(String, String)>,
) -> Fitted {
    const NEAR: f64 = 0.01;
    let at = [theirs.x - origin.0, theirs.y - origin.1];
    let kicad = Fitted {
        at,
        rotation: theirs.rotation,
        crossed: crossed.is_some(),
        moved: [0.0, 0.0],
    };
    let corner = board.outline.min;
    // Each KiCad pad, by the design's number, from the outline's corner.
    let pads: Vec<(&str, [f64; 2])> = theirs
        .pads
        .iter()
        .map(|pad| {
            let number = match crossed {
                Some((a, b)) if &pad.number == a => b.as_str(),
                Some((a, b)) if &pad.number == b => a.as_str(),
                _ => pad.number.as_str(),
            };
            let (x, y) = theirs.board_point(pad.x, pad.y);
            (number, [x - origin.0, y - origin.1])
        })
        .collect();
    let mut part = ours.clone();
    part.side = if theirs.layer == "B.Cu" {
        netsmith_pcb::Side::Back
    } else {
        netsmith_pcb::Side::Front
    };
    // For each KiCad pad, the offset to the nearest pad of its number of `part` placed at `at`
    // turned `rotation`: KiCad's pad less ours.
    let mut offsets = |at: [f64; 2], rotation: f64| {
        part.at = corner + netsmith_pcb::Pt::new(at[0], at[1]);
        part.rotation = rotation;
        let mut found = Vec::new();
        for (number, want) in &pads {
            let nearest = part
                .footprint
                .pads_numbered(number)
                .map(|p| {
                    let c = part.pad_centre(p) - corner;
                    [want[0] - c.x, want[1] - c.y]
                })
                .min_by(|a, b| a[0].hypot(a[1]).total_cmp(&b[0].hypot(b[1])));
            if let Some(d) = nearest {
                found.push(d);
            }
        }
        found
    };
    let size = |d: &[f64; 2]| d[0].hypot(d[1]);
    let mut best: Option<(f64, f64, Vec<[f64; 2]>)> = None;
    for quarter in 0..4 {
        let r = (theirs.rotation + 90.0 * f64::from(quarter)).rem_euclid(360.0);
        let rotation = if r > 180.0 { r - 360.0 } else { r };
        let found = offsets(at, rotation);
        if found.is_empty() {
            return kicad;
        }
        let score: f64 = found.iter().map(|d| size(d).powi(2)).sum();
        if best.as_ref().is_none_or(|(s, _, _)| score < s - 1e-9) {
            best = Some((score, rotation, found));
        }
    }
    let Some((_, rotation, found)) = best else {
        return kicad;
    };
    let worst = |found: &[[f64; 2]]| found.iter().map(size).fold(0.0, f64::max);
    let n = found.len() as f64;
    let mean = [
        found.iter().map(|d| d[0]).sum::<f64>() / n,
        found.iter().map(|d| d[1]).sum::<f64>() / n,
    ];
    let shifted: Vec<[f64; 2]> = found
        .iter()
        .map(|d| [d[0] - mean[0], d[1] - mean[1]])
        .collect();
    let moved = if worst(&found) > NEAR && worst(&shifted) <= NEAR && size(&mean) > 1e-6 {
        mean
    } else {
        [0.0, 0.0]
    };
    Fitted {
        at: [at[0] + moved[0], at[1] + moved[1]],
        rotation,
        crossed: crossed.is_some(),
        moved,
    }
}

/// The routes and the places of a KiCad board `pcb`, for the design's placed `board` (its parts,
/// nets and paths; not where it placed them). `parts` is a port's parts file, the KiCad reference
/// of each of the design's member paths; without one a part is the KiCad footprint of its own
/// reference designator. Places are in mm from the outline's top left corner, which for the KiCad
/// board is the top left of its Edge.Cuts' box, as the parity checker reads it. `lock` locks every
/// net imported. Copper on an inner layer, and copper of a net that maps to none of the design's,
/// is left out and said. Each zone of copper is taken whole (its layers, outline, holes and
/// settings, [`RouteZone`]); a rule area, a teardrop and a hatched zone are left out and said.
pub fn import(
    board: &Board,
    pcb: &str,
    parts: Option<&crate::port::PortParts>,
    lock: bool,
) -> Result<Import, String> {
    let kicad = crate::reference_table::read_board(pcb, "the KiCad board")?;
    // The design's reference of each KiCad reference.
    let ours_of: BTreeMap<String, String> = board
        .parts
        .iter()
        .filter_map(|p| {
            let theirs = match parts {
                Some(parts) => parts.reference_of(&p.path)?.to_string(),
                None => p.refdes.clone(),
            };
            Some((theirs, p.refdes.clone()))
        })
        .collect();
    let mut out = Import::default();
    let straight = votes(board, &kicad, &ours_of, &BTreeMap::new());
    let crossed = crossed_parts(board, &kicad, &ours_of, &straight);
    let (map, said) = map_nets(board, &kicad, &votes(board, &kicad, &ours_of, &crossed));
    out.skipped.extend(said);
    let origin = (kicad.outline.min_x, kicad.outline.min_y);
    // To 1 nm, so a place reads as the KiCad board wrote it (11.6, not 11.600000000000001).
    let nm = |v: f64| (v * 1e6).round() / 1e6;
    // Where the parts stand.
    for part in &kicad.parts {
        let Some(ours) = ours_of
            .get(&part.reference)
            .and_then(|r| board.parts.iter().find(|p| &p.refdes == r))
        else {
            out.skipped.push(format!(
                "part {} of the KiCad board is none of the design's: not placed",
                part.reference
            ));
            continue;
        };
        let fitted = fit(board, ours, part, origin, crossed.get(&part.reference));
        if let Some(why) = fitted.why(part.rotation) {
            out.refitted.push(format!("part {} {why}", ours.path));
        }
        out.places.insert(
            ours.path.clone(),
            PartPlace {
                at: [nm(fitted.at[0]), nm(fitted.at[1])],
                rotation: fitted.rotation,
                side: if part.layer == "B.Cu" {
                    Face::Bottom
                } else {
                    Face::Top
                },
                // The reference text above the part, whatever side the placer gave it before:
                // the import's places do not depend on an earlier arrangement.
                text: Default::default(),
                lock: true,
                source: Some(netsmith_pcb::IMPORT_SOURCE.to_string()),
            },
        );
    }
    let (shapes, unread) = crate::copper::read(pcb, "the KiCad board")?;
    out.skipped.extend(unread);
    let mut lost: BTreeMap<String, usize> = BTreeMap::new();
    for shape in shapes {
        let theirs = &shape.net;
        let Some((ours, _)) = map.get(theirs) else {
            *lost.entry(theirs.clone()).or_default() += 1;
            continue;
        };
        let record = out.nets.entry(ours.clone()).or_insert_with(|| NetRoutes {
            lock,
            ..NetRoutes::default()
        });
        if shape.kind == Kind::Via {
            let layers: Vec<&str> = shape.layer.split('-').collect();
            if !(layers.contains(&"F.Cu") && layers.contains(&"B.Cu")) {
                out.skipped.push(format!(
                    "a via of net `{theirs}` that is not a through via of F.Cu and B.Cu: left out"
                ));
                continue;
            }
            record.vias.push(RouteVia {
                at: shape.from,
                diameter: shape.width,
                drill: shape.drill.unwrap_or_default(),
                lock: false,
            });
            continue;
        }
        let kind = if shape.kind == Kind::Arc {
            "arc"
        } else {
            "segment"
        };
        let layer = match shape.layer.as_str() {
            "F.Cu" => Face::Top,
            "B.Cu" => Face::Bottom,
            other => {
                out.skipped.push(format!(
                    "a {kind} of net `{theirs}` on {}: netsmith draws tracks on the outer layers only; left out",
                    if other.is_empty() { "no layer" } else { other }
                ));
                continue;
            }
        };
        record.tracks.push(RouteTrack {
            layer,
            width: shape.width,
            from: shape.from,
            mid: shape.mid,
            to: shape.to,
            lock: false,
        });
    }
    let (zones, unread) = crate::copper::read_zones(pcb, "the KiCad board")?;
    out.skipped.extend(unread);
    let (mut rule_areas, mut teardrops) = (0, 0);
    for zone in zones {
        match zone.kind {
            ZoneKind::RuleArea => {
                rule_areas += 1;
                continue;
            }
            ZoneKind::Teardrop => {
                teardrops += 1;
                continue;
            }
            ZoneKind::Copper => {}
        }
        let theirs = &zone.net;
        let Some((ours, _)) = map.get(theirs) else {
            *lost.entry(theirs.clone()).or_default() += 1;
            continue;
        };
        if zone.hatched {
            out.skipped.push(format!(
                "a zone of net `{theirs}` filled with hatching: netsmith fills solid zones only; left out"
            ));
            continue;
        }
        let words: Option<Vec<String>> = zone
            .layers
            .iter()
            .map(|l| netsmith_pcb::routes_layer_word(l))
            .collect();
        let Some(layers) = words.filter(|w| !w.is_empty()) else {
            out.skipped.push(format!(
                "a zone of net `{theirs}` on {}: not a copper layer; left out",
                zone.layers.join(", ")
            ));
            continue;
        };
        if zone.smoothed {
            out.skipped.push(format!(
                "a zone of net `{theirs}` has smoothed corners: taken with its outline's corners as they are"
            ));
        }
        let record = out.nets.entry(ours.clone()).or_insert_with(|| NetRoutes {
            lock,
            ..NetRoutes::default()
        });
        record.zones.push(RouteZone {
            layers,
            priority: zone.priority,
            clearance: zone.clearance,
            min_width: zone.min_width,
            pads: zone.pads,
            thermal_gap: zone.thermal_gap,
            spoke_width: zone.spoke_width,
            islands: zone.islands,
            island_min_area: zone.island_min_area,
            outline: zone.outline,
            holes: zone.holes,
            lock: false,
        });
    }
    if rule_areas > 0 {
        out.skipped.push(format!(
            "{rule_areas} rule area(s) of the KiCad board: left out (the design's keepouts are its own)"
        ));
    }
    if teardrops > 0 {
        out.skipped.push(format!(
            "{teardrops} teardrop(s) of the KiCad board: left out (`fab` draws its own)"
        ));
    }
    for (net, count) in lost {
        out.skipped.push(format!(
            "{count} shape(s) of KiCad net `{net}`, which is on no pad of a part the design has: left out"
        ));
    }
    out.mapped = map
        .into_iter()
        .filter(|(theirs, (ours, _))| out.nets.contains_key(ours) && !theirs.is_empty())
        .map(|(theirs, (ours, votes))| (ours, theirs, votes))
        .collect();
    Ok(out)
}
