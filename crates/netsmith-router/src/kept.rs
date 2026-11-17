//! The router's copper kept on disk (ADR 0030, issue #1019): what partial routing
//! ([`crate::carry`]) starts from in a later editor session, after `netsmith layout regenerate`,
//! and in `board fab --reuse-routes`.
//!
//! A [`Kept`] is one routed board's router copper (the tracks and vias after the routes file's,
//! [`netsmith_pcb::Board::fixed`]), net by net with each net's [`pad_hash`], the place, turn and
//! face of every part it was routed round, and three keys: the router's version
//! ([`router_version`]), a hash of the copper the routes file put on the board ([`routes_hash`])
//! and a hash of the board's design rules ([`rules_hash`]). netsmith-fabricate writes it to
//! `.netsmith/routes/<stem>.<board>.toml`, which is not committed.
//!
//! [`Kept::before`] gives back the board it was routed on, made from the board placed now: a
//! keep that is stale (another router, other routes-file copper, other rules, another board) is
//! refused whole ([`Stale`]); otherwise the board's parts are put where the routed board had
//! them and its copper added after the routes file's, and the nets whose pads are not where the
//! record says are named, to be ripped up. [`crate::carry::carry_copper`] decides the rest as it
//! does for a board kept in memory.

use std::collections::{BTreeMap, BTreeSet};

use netsmith_pcb::{Board, Side, Track, Via, pad_hash};
use serde::{Deserialize, Serialize};

/// The version of the kept file's format.
pub const FORMAT: u32 = 1;

/// The router's version, as a kept file records it: netsmith's version and the format. Copper
/// another version drew is routed again.
pub fn router_version() -> String {
    format!("{}/{FORMAT}", env!("CARGO_PKG_VERSION"))
}

/// Where a part was when the board was routed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeptPlace {
    pub at: [f64; 2],
    pub turn: f64,
    pub side: Side,
}

/// One net's router copper.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeptNet {
    /// [`pad_hash`] of the net on the routed board.
    pub pads: String,
    #[serde(default)]
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub vias: Vec<Via>,
}

/// One routed board's router copper and what it was routed round (the module's header).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Kept {
    pub format: u32,
    /// [`router_version`] of the router that drew it.
    pub router: String,
    pub board: String,
    /// The source file, as the caller names it, so that two sources of one stem never share
    /// copper.
    pub source: String,
    /// [`routes_hash`] of the board it was routed on.
    pub routes: String,
    /// [`rules_hash`] of the board it was routed on.
    pub rules: String,
    /// The parts, by path in the design.
    pub parts: BTreeMap<String, KeptPlace>,
    pub nets: BTreeMap<String, KeptNet>,
}

/// Why a kept file is not used at all.
#[derive(Debug, Clone, PartialEq)]
pub enum Stale {
    Router { was: String },
    Board { was: String },
    Source { was: String },
    Routes,
    Rules,
}

impl std::fmt::Display for Stale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Stale::Router { was } => write!(
                f,
                "it was routed by netsmith {was}, not {}",
                router_version()
            ),
            Stale::Board { was } => write!(f, "it is of board `{was}`"),
            Stale::Source { was } => write!(f, "it is of the source {was}"),
            Stale::Routes => write!(f, "the routes file's copper changed since it was routed"),
            Stale::Rules => write!(f, "the board's design rules changed since it was routed"),
        }
    }
}

/// A 64-bit FNV-1a hash of `text`, 16 hex digits.
fn hash(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// A hash of the copper the routes file put on `board`: its fixed tracks and vias, the nets it
/// locks and its zones. The router's copper was drawn round it, so other copper there drops the
/// keep.
pub fn routes_hash(board: &Board) -> String {
    let tracks = &board.tracks[..board.fixed.tracks.min(board.tracks.len())];
    let vias = &board.vias[..board.fixed.vias.min(board.vias.len())];
    hash(&format!(
        "{tracks:?}\n{vias:?}\n{:?}\n{:?}",
        board.fixed.locked, board.zones
    ))
}

/// A hash of what the router is held to on `board` apart from its parts and nets: the name, the
/// outline and its cutouts, pours, planes and keepouts, the rules, the process and the nets'
/// own widths, clearances and currents.
pub fn rules_hash(board: &Board) -> String {
    hash(&format!(
        "{}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}",
        board.name,
        board.outline,
        board.corner_radius,
        board.polygon,
        board.cutouts,
        board.pours,
        board.planes,
        board.keepouts,
        board.rules,
        board.process,
        board.net_widths,
        board.net_clearances,
        board.impedance_targets,
        board.net_currents,
    ))
}

impl Kept {
    /// The router's copper of `routed`, routed from `start` (the board as placed, with the routes
    /// file's copper and no other: its fixed copper is what of `routed` is not the router's).
    pub fn of(start: &Board, routed: &Board, source: &str) -> Kept {
        let from_tracks = start.fixed.tracks.min(routed.tracks.len());
        let from_vias = start.fixed.vias.min(routed.vias.len());
        let mut nets: BTreeMap<String, KeptNet> = BTreeMap::new();
        for t in &routed.tracks[from_tracks..] {
            nets.entry(t.net.clone())
                .or_default()
                .tracks
                .push(t.clone());
        }
        for v in &routed.vias[from_vias..] {
            nets.entry(v.net.clone()).or_default().vias.push(v.clone());
        }
        nets.retain(|net, record| match pad_hash(routed, net) {
            Some(pads) => {
                record.pads = pads;
                true
            }
            None => false,
        });
        Kept {
            format: FORMAT,
            router: router_version(),
            board: start.name.clone(),
            source: source.to_string(),
            routes: routes_hash(start),
            rules: rules_hash(start),
            parts: routed
                .parts
                .iter()
                .map(|p| {
                    (
                        p.path.clone(),
                        KeptPlace {
                            at: [p.at.x, p.at.y],
                            turn: p.rotation,
                            side: p.side,
                        },
                    )
                })
                .collect(),
            nets,
        }
    }

    /// How many nets have copper kept.
    pub fn len(&self) -> usize {
        self.nets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nets.is_empty()
    }

    /// The board this was routed on, made from `placed` (the board as placed now, with the
    /// routes file's copper and no other), to carry the copper of
    /// ([`crate::carry::carry_copper`]), and the nets whose pads are not where the record says,
    /// to rip up with the rest. A part this does not have is left out of the board, so that it
    /// counts as moved.
    pub fn before(&self, placed: &Board, source: &str) -> Result<(Board, BTreeSet<String>), Stale> {
        if self.format != FORMAT || self.router != router_version() {
            return Err(Stale::Router {
                was: self.router.clone(),
            });
        }
        if self.board != placed.name {
            return Err(Stale::Board {
                was: self.board.clone(),
            });
        }
        if self.source != source {
            return Err(Stale::Source {
                was: self.source.clone(),
            });
        }
        if self.routes != routes_hash(placed) {
            return Err(Stale::Routes);
        }
        if self.rules != rules_hash(placed) {
            return Err(Stale::Rules);
        }
        let mut before = placed.clone();
        before.tracks.truncate(placed.fixed.tracks);
        before.vias.truncate(placed.fixed.vias);
        before.parts.retain_mut(|p| match self.parts.get(&p.path) {
            Some(place) => {
                p.at = netsmith_pcb::Pt::new(place.at[0], place.at[1]);
                p.rotation = place.turn;
                p.side = place.side;
                true
            }
            None => false,
        });
        let mut stale = BTreeSet::new();
        for (net, record) in &self.nets {
            if pad_hash(&before, net).as_deref() != Some(record.pads.as_str()) {
                stale.insert(net.clone());
            }
            before.tracks.extend(record.tracks.iter().cloned());
            before.vias.extend(record.vias.iter().cloned());
        }
        Ok((before, stale))
    }
}
