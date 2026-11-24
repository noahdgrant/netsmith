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
    /// How many zones of the KiCad board are the process's own planes and pours: of the same
    /// net, layer, outline and settings (issue #1116), or with the uuid `fab` gives them
    /// ([`netsmith_pcb::process_zone_uuid`], issue #1609). Each is left out and said in `skipped`
    /// and [`Import::zones_left_out`], or with [`Options::process_zones`] taken as a hand zone.
    pub process_zones: usize,
    /// The vias of the KiCad board left out because each is a stitching via of a pour that `fab`
    /// lays again (on the pour's grid, of the rules' size, with no track ending on it,
    /// [`netsmith_pcb::is_stitching_via`], issue #1483), each its net and its place from the
    /// board's corner as the routes file writes places; their count is said once in `skipped`,
    /// and `routes show` counts them as the vias `fab` lays (issue #1583). A locked import leaves
    /// none out.
    pub stitching_vias: Vec<(String, [f64; 2])>,
    /// Each zone of the KiCad board the import did not take, with why (issue #1609): the
    /// process's own planes and pours, rule areas, teardrops, hatched zones, zones of no copper
    /// layer or of a net the design does not have, and zones that could not be read.
    pub zones_left_out: Vec<ZoneLeftOut>,
}

/// A zone of the KiCad board that [`import`] left out of the routes file.
#[derive(Debug, Clone, PartialEq)]
pub struct ZoneLeftOut {
    /// The KiCad board's net of it (`""` for none).
    pub net: String,
    /// KiCad's names of its layers.
    pub layers: Vec<String>,
    pub reason: ZoneReason,
    /// The sentence that says it.
    pub said: String,
}

/// Why a zone of the KiCad board was left out of the routes file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ZoneReason {
    /// The process's own plane or pour, as `fab` writes it: the process draws it.
    Process,
    /// A zone `fab` wrote for the process's plane or pour (its uuid says so), changed since.
    ProcessChanged,
    /// A rule area (keepout): the routes file holds copper only.
    RuleArea,
    /// A teardrop: `fab` draws its own.
    Teardrop,
    /// Filled with hatching: netsmith fills solid zones only.
    Hatched,
    /// On no copper layer netsmith draws.
    NotCopper,
    /// Of a KiCad net on no pad of a part the design has.
    NoNet,
    /// Without an outline or a setting KiCad writes into every zone.
    Unreadable,
}

impl ZoneReason {
    /// The word of the JSON output.
    pub fn word(self) -> &'static str {
        match self {
            Self::Process => "process",
            Self::ProcessChanged => "process_changed",
            Self::RuleArea => "rule_area",
            Self::Teardrop => "teardrop",
            Self::Hatched => "hatched",
            Self::NotCopper => "not_copper",
            Self::NoNet => "no_net",
            Self::Unreadable => "unreadable",
        }
    }

    /// What the text output calls the zones left out for this reason.
    pub fn phrase(self) -> &'static str {
        match self {
            Self::Process => "of the process's own planes and pours",
            Self::ProcessChanged => {
                "of the process's planes and pours as fab wrote them, changed since"
            }
            Self::RuleArea => "rule area(s)",
            Self::Teardrop => "teardrop(s)",
            Self::Hatched => "hatched",
            Self::NotCopper => "on no copper layer",
            Self::NoNet => "of no net of the design",
            Self::Unreadable => "unreadable",
        }
    }
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
/// settings, [`RouteZone`]); a rule area, a teardrop and a hatched zone are left out and said, and
/// so is a zone that is the process's own plane or pour (same net, layer, outline and settings as
/// the board's process states them, [`netsmith_pcb::process_zones`], issue #1116, or with the uuid
/// `fab` gives that zone, [`netsmith_pcb::process_zone_uuid`], issue #1609): any other zone is a
/// hand zone. Each zone left out is in [`Import::zones_left_out`] with its reason.
pub fn import(
    board: &Board,
    pcb: &str,
    parts: Option<&crate::port::PortParts>,
    lock: bool,
) -> Result<Import, String> {
    import_with(
        board,
        pcb,
        parts,
        Options {
            lock,
            ..Options::default()
        },
    )
}

/// What the import does beyond [`import`]'s defaults.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// Lock every net imported.
    pub lock: bool,
    /// Take the zones that are the process's own planes and pours (known by their settings or
    /// by the uuid `fab` gives them) as hand zones too, instead of leaving them out
    /// (`routes import --process-zones`).
    pub process_zones: bool,
}

/// [`import`] with its [`Options`].
pub fn import_with(
    board: &Board,
    pcb: &str,
    parts: Option<&crate::port::PortParts>,
    options: Options,
) -> Result<Import, String> {
    let lock = options.lock;
    let kicad = crate::reference_table::read_board(pcb, "the KiCad board")?;
    let signal = board.signal_layers();
    let copper_layers = board.copper_count();
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
                designator: None,
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
            // A through via joins F.Cu and B.Cu; a blind or a buried via (KiCad's `blind` or
            // `micro`) is kept with the two layers it ends on, the upper first (issue #1015),
            // and the design rules check says whether the board's process makes it.
            let ends: Vec<netsmith_pcb::CopperLayer> = shape
                .layer
                .split('-')
                .filter_map(netsmith_pcb::CopperLayer::of_name)
                .collect();
            let layers = match ends.as_slice() {
                [a, b] if a.is_outer() && b.is_outer() && a != b => Vec::new(),
                [a, b] if a != b => {
                    let span = netsmith_pcb::ViaSpan::between(*a, *b, copper_layers);
                    vec![span.from.word().to_string(), span.to.word().to_string()]
                }
                _ => {
                    out.skipped.push(format!(
                        "a via of net `{theirs}` on `{}`, which names no two copper layers: left out",
                        shape.layer
                    ));
                    continue;
                }
            };
            record.vias.push(RouteVia {
                at: shape.from,
                diameter: shape.width,
                drill: shape.drill.unwrap_or_default(),
                layers,
                lock: false,
            });
            continue;
        }
        let kind = if shape.kind == Kind::Arc {
            "arc"
        } else {
            "segment"
        };
        // A track is on an outer layer or on one of the board's inner signal layers (issue
        // #833); an inner plane holds none.
        let layer = match netsmith_pcb::CopperLayer::of_name(&shape.layer) {
            Some(l) if l.is_outer() || signal.contains(&l) => l,
            _ => {
                let other = shape.layer.as_str();
                out.skipped.push(format!(
                    "a {kind} of net `{theirs}` on {}: netsmith draws tracks on the outer layers and the process's inner signal layers only; left out",
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
    // The stitching vias of the pours, as `fab` laid them: `fab` lays them again on the same
    // grid, so the routes file keeps none (issue #1483). A locked net gets no copper from `fab`,
    // so its stitching vias are kept.
    let mut stitching: Vec<(String, [f64; 2])> = Vec::new();
    let mut kept = 0;
    // The pads stand where the KiCad board has them, for the check that a via is in none.
    let mut placed = board.clone();
    for part in &mut placed.parts {
        if let Some(place) = out.places.get(&part.path) {
            part.at = board.outline.min + netsmith_pcb::Pt::new(place.at[0], place.at[1]);
            part.rotation = place.rotation;
            part.side = match place.side {
                Face::Bottom => netsmith_pcb::Side::Back,
                Face::Top => netsmith_pcb::Side::Front,
            };
        }
    }
    for (net, record) in out.nets.iter_mut() {
        let ends: Vec<[f64; 2]> = record.tracks.iter().flat_map(|t| [t.from, t.to]).collect();
        let mut found = 0;
        record.vias.retain(|v| {
            let via = netsmith_pcb::Via {
                net: net.clone(),
                at: board.outline.min + netsmith_pcb::Pt::new(v.at[0], v.at[1]),
                diameter: v.diameter,
                drill: v.drill,
                span: None,
            };
            let joined = ends
                .iter()
                .any(|e| (e[0] - v.at[0]).hypot(e[1] - v.at[1]) < 1e-3);
            let is =
                v.layers.is_empty() && !joined && netsmith_pcb::is_stitching_via(&placed, &via);
            if is {
                found += 1;
                if !lock {
                    stitching.push((net.clone(), v.at));
                }
            }
            !is || lock
        });
        if lock {
            kept += found;
        }
    }
    if !stitching.is_empty() {
        out.skipped.push(format!(
            "{} stitching via(s) of the pours: left out (`fab` lays them again on the pour's grid, and `routes show` counts them as its stitching vias; `--format json` names each in `stitching_vias_left_out`)",
            stitching.len()
        ));
    }
    out.stitching_vias = stitching;
    if kept > 0 {
        out.skipped.push(format!(
            "{kept} stitching via(s) of the pours kept, as the import locks their nets and `fab` lays no copper on a locked net"
        ));
    }
    let (zones, unread) = crate::copper::read_zones(pcb, "the KiCad board")?;
    for u in unread {
        out.skipped.push(u.said.clone());
        out.zones_left_out.push(ZoneLeftOut {
            net: u.net,
            layers: u.layers,
            reason: ZoneReason::Unreadable,
            said: u.said,
        });
    }
    let mut process = netsmith_pcb::process_zones(board);
    // The layers of the process's own zones the KiCad board has, for the lift below.
    let mut found: Vec<String> = Vec::new();
    // The process's own zones carried as hand zones (`process_zones`), added after the lift.
    let mut carried: Vec<(String, RouteZone)> = Vec::new();
    let (mut rule_areas, mut teardrops) = (0, 0);
    for zone in zones {
        let theirs = &zone.net;
        let mut leave = |reason: ZoneReason, said: String| {
            out.zones_left_out.push(ZoneLeftOut {
                net: zone.net.clone(),
                layers: zone.layers.clone(),
                reason,
                said,
            });
        };
        match zone.kind {
            ZoneKind::RuleArea => {
                rule_areas += 1;
                leave(
                    ZoneReason::RuleArea,
                    "a rule area: left out (the design's keepouts are its own)".into(),
                );
                continue;
            }
            ZoneKind::Teardrop => {
                teardrops += 1;
                leave(
                    ZoneReason::Teardrop,
                    format!("a teardrop of net `{theirs}`: left out (`fab` draws its own)"),
                );
                continue;
            }
            ZoneKind::Copper => {}
        }
        let Some((ours, _)) = map.get(theirs) else {
            *lost.entry(theirs.clone()).or_default() += 1;
            leave(
                ZoneReason::NoNet,
                format!(
                    "a zone of KiCad net `{theirs}`, which is on no pad of a part the design has: left out"
                ),
            );
            continue;
        };
        if zone.hatched {
            let said = format!(
                "a zone of net `{theirs}` filled with hatching: netsmith fills solid zones only; left out"
            );
            out.skipped.push(said.clone());
            leave(ZoneReason::Hatched, said);
            continue;
        }
        let words: Option<Vec<String>> = zone
            .layers
            .iter()
            .map(|l| netsmith_pcb::routes_layer_word(l))
            .collect();
        let Some(layers) = words.filter(|w| !w.is_empty()) else {
            let said = format!(
                "a zone of net `{theirs}` on {}: not a copper layer; left out",
                zone.layers.join(", ")
            );
            out.skipped.push(said.clone());
            leave(ZoneReason::NotCopper, said);
            continue;
        };
        let taken = RouteZone {
            layers,
            priority: zone.priority,
            clearance: zone.clearance,
            min_width: zone.min_width,
            pads: zone.pads,
            thermal_gap: zone.thermal_gap,
            spoke_width: zone.spoke_width,
            islands: zone.islands,
            island_min_area: zone.island_min_area,
            outline: zone.outline.clone(),
            holes: zone.holes.clone(),
            lock: false,
        };
        // The process's own plane or pour, as `fab` wrote it, is the design's already: taken, it
        // would replace the process's with a hand zone that has the settings frozen as of now.
        // It is known by its settings (the net, layer, outline and settings the process states)
        // or by the uuid `fab` gives it, which KiCad keeps: a zone `fab` wrote for the process
        // and changed since, in KiCad or by a process that states other settings now, is still
        // the process's and not a hand zone (issue #1609).
        let same = process
            .iter()
            .position(|(net, own)| net == ours && own.same_copper(&taken));
        let written = zone.uuid.as_deref().is_some_and(|u| {
            zone.layers
                .iter()
                .any(|l| u == netsmith_pcb::process_zone_uuid(board, l))
        });
        if same.is_some() || written {
            found.extend(taken.layers.iter().cloned());
            if let Some(i) = same {
                process.remove(i);
            }
            out.process_zones += 1;
            let on = taken.layers.join(", ");
            if options.process_zones {
                out.skipped.push(format!(
                    "a zone of net `{ours}` on {on} is the process's own plane or pour: taken as a hand zone (`--process-zones`)"
                ));
                carried.push((ours.clone(), taken));
                continue;
            }
            let (reason, said) = if same.is_some() {
                (
                    ZoneReason::Process,
                    format!(
                        "a zone of net `{ours}` on {on} is the process's own plane or pour: left out (the process draws it)"
                    ),
                )
            } else {
                (
                    ZoneReason::ProcessChanged,
                    format!(
                        "a zone of net `{ours}` on {on} is the process's own plane or pour as `fab` wrote it, changed since (in KiCad, or the process states other settings now): left out (the process draws it as it states it; `--process-zones` takes it as a hand zone)"
                    ),
                )
            };
            out.skipped.push(said.clone());
            leave(reason, said);
            continue;
        }
        if zone.smoothed {
            out.skipped.push(format!(
                "a zone of net `{theirs}` has smoothed corners: taken with its outline's corners as they are"
            ));
        }
        let record = out.nets.entry(ours.clone()).or_insert_with(|| NetRoutes {
            lock,
            ..NetRoutes::default()
        });
        record.zones.push(taken);
    }
    unlift(&mut out.nets, &found);
    for (net, zone) in carried {
        out.nets
            .entry(net)
            .or_insert_with(|| NetRoutes {
                lock,
                ..NetRoutes::default()
            })
            .zones
            .push(zone);
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

/// Takes the 1 off each zone's priority that `fab` adds where the process's pour or plane is
/// drawn under the zones of the routes file (`Board::zone_lift`, issue #1314), so that `fab` and
/// then `routes import` of its board gives the priorities back as they were: done when the
/// KiCad board has the process's own zone on a layer (`found`, routes file words) that a zone
/// taken is on, and every zone taken is of priority 1 or more (the lift leaves none at 0). The
/// order of the zones among themselves is kept either way.
fn unlift(nets: &mut BTreeMap<String, NetRoutes>, found: &[String]) {
    let zones = || nets.values().flat_map(|r| r.zones.iter());
    let under = zones().any(|z| z.layers.iter().any(|l| found.contains(l)));
    if !under || zones().any(|z| z.priority == 0) {
        return;
    }
    for z in nets.values_mut().flat_map(|r| r.zones.iter_mut()) {
        z.priority -= 1;
    }
}
