//! The import of a KiCad board's copper into a routes file (`netsmith routes import`, issue
//! #599): its tracks, arcs and vias by the design's net names, and where its parts stand, for the
//! layout file. The routes file itself is `netsmith_fabricate::routes`.

use std::collections::BTreeMap;

use netsmith_pcb::{
    Board, BoardRoutes, LayoutFace as Face, NetRoutes, PartPlace, RouteTrack, RouteVia,
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
    /// What could not be taken, each a sentence: a net no pad maps, a net whose pads map to two of
    /// the design's, a shape on an inner layer, a part the design does not have.
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

    /// How many tracks, arcs and vias were taken.
    pub fn counts(&self) -> (usize, usize, usize) {
        let tracks = self.nets.values().flat_map(|n| &n.tracks);
        let arcs = tracks.clone().filter(|t| t.mid.is_some()).count();
        let vias = self.nets.values().map(|n| n.vias.len()).sum();
        (tracks.count() - arcs, arcs, vias)
    }
}

/// The design's net of each KiCad net: the net its pads are on in the design, by the most pads,
/// where the KiCad board's parts are the design's by `designators` (the KiCad reference of each of
/// the design's). A KiCad net no pad maps keeps its name when the design has a net of that name
/// (a leading `/` and the case aside). Returns the map and what was not mapped.
fn map_nets(
    board: &Board,
    kicad: &crate::reference_table::Board,
    ours_of: &BTreeMap<String, String>,
) -> (BTreeMap<String, (String, usize)>, Vec<String>) {
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
            if let Some(net) = net_of_pad.get(&(ours.as_str(), pad.number.as_str())) {
                *votes
                    .entry(theirs.clone())
                    .or_default()
                    .entry(net.to_string())
                    .or_default() += 1;
            }
        }
    }
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

/// The routes and the places of a KiCad board `pcb`, for the design's placed `board` (its parts,
/// nets and paths; not where it placed them). `parts` is a port's parts file, the KiCad reference
/// of each of the design's member paths; without one a part is the KiCad footprint of its own
/// reference designator. Places are in mm from the outline's top left corner, which for the KiCad
/// board is the top left of its Edge.Cuts' box, as the parity checker reads it. `lock` locks every
/// net imported. Copper on an inner layer, and copper of a net that maps to none of the design's,
/// is left out and said.
pub fn import(
    board: &Board,
    pcb: &str,
    parts: Option<&crate::port::PortParts>,
    lock: bool,
) -> Result<Import, String> {
    use netsmith_kicad::Sexp;
    let kicad = crate::reference_table::read_board(pcb, "the KiCad board")?;
    let root = Sexp::parse(pcb).map_err(|e| format!("the KiCad board: {e}"))?;
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
    let (map, said) = map_nets(board, &kicad, &ours_of);
    out.skipped.extend(said);
    let origin = (kicad.outline.min_x, kicad.outline.min_y);
    // To 1 nm, so a place reads as the KiCad board wrote it (11.6, not 11.600000000000001).
    let nm = |v: f64| (v * 1e6).round() / 1e6;
    let at = |x: f64, y: f64| [nm(x - origin.0), nm(y - origin.1)];
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
        out.places.insert(
            ours.path.clone(),
            PartPlace {
                at: at(part.x, part.y),
                rotation: part.rotation,
                side: if part.layer == "B.Cu" {
                    Face::Bottom
                } else {
                    Face::Top
                },
                text: ours.reference_side.into(),
                lock: true,
                source: Some("import".to_string()),
            },
        );
    }
    // The KiCad 9 net table, number to name; KiCad 10 names the net on each item.
    let numbered: BTreeMap<String, String> = root
        .find_all("net")
        .filter_map(|n| {
            let args = n.args();
            Some((
                args.first()?.text()?.to_string(),
                args.get(1)?.text()?.to_string(),
            ))
        })
        .collect();
    let net_name = |item: &Sexp| -> Option<String> {
        let arg = item.find("net")?.args().first()?;
        match arg {
            Sexp::Str(name) => Some(name.clone()),
            other => numbered.get(other.text()?).cloned(),
        }
    };
    let point = |item: &Sexp, key: &str| -> Option<[f64; 2]> {
        let a = item.find(key)?.args();
        let n = |k: usize| a.get(k)?.text()?.parse::<f64>().ok();
        Some(at(n(0)?, n(1)?))
    };
    let number = |item: &Sexp, key: &str| -> Option<f64> {
        item.find(key)?.args().first()?.text()?.parse().ok()
    };
    let mut lost: BTreeMap<String, usize> = BTreeMap::new();
    for item in root.args() {
        let kind = match item.head() {
            Some(k @ ("segment" | "arc" | "via")) => k,
            _ => continue,
        };
        let theirs = net_name(item).unwrap_or_default();
        let Some((ours, _)) = map.get(&theirs) else {
            *lost.entry(theirs).or_default() += 1;
            continue;
        };
        let record = out.nets.entry(ours.clone()).or_insert_with(|| NetRoutes {
            lock,
            ..NetRoutes::default()
        });
        if kind == "via" {
            let layers: Vec<&str> = item
                .find("layers")
                .map(|l| l.args().iter().filter_map(Sexp::text).collect())
                .unwrap_or_default();
            let through = layers.contains(&"F.Cu") && layers.contains(&"B.Cu");
            let (Some(at), Some(diameter), Some(drill), true) = (
                point(item, "at"),
                number(item, "size"),
                number(item, "drill"),
                through,
            ) else {
                out.skipped.push(format!(
                    "a via of net `{theirs}` that is not a through via of F.Cu and B.Cu: left out"
                ));
                continue;
            };
            record.vias.push(RouteVia {
                at,
                diameter,
                drill,
                lock: false,
            });
            continue;
        }
        let layer = match item.find("layer").and_then(|l| l.args().first()?.text()) {
            Some("F.Cu") => Face::Top,
            Some("B.Cu") => Face::Bottom,
            other => {
                out.skipped.push(format!(
                    "a {kind} of net `{theirs}` on {}: netsmith draws tracks on the outer layers only; left out",
                    other.unwrap_or("no layer")
                ));
                continue;
            }
        };
        let (Some(from), Some(to), Some(width)) = (
            point(item, "start"),
            point(item, "end"),
            number(item, "width"),
        ) else {
            out.skipped.push(format!(
                "a {kind} of net `{theirs}` with no start, end or width: left out"
            ));
            continue;
        };
        record.tracks.push(RouteTrack {
            layer,
            width,
            from,
            mid: if kind == "arc" {
                point(item, "mid")
            } else {
                None
            },
            to,
            lock: false,
        });
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
