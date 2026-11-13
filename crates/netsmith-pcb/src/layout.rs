//! The layout file: where a person or a placer put things, kept apart from the source
//! (ADR 0020 item 7, ADR 0030 section f).
//!
//! `<stem>.pcb.toml` sits beside the top-level `.nsm` file. It is optional, never needed to
//! build, and written by the editor or the placer, not by the language: the source holds no
//! geometry. One table for each board of the file, keyed by the board's name; inside it,
//! the parts are keyed by their path in the design (`stage.output_cap`), which a renumbered
//! reference designator does not change.
//!
//! ```toml
//! version = 1
//!
//! [RegulatorExample.parts."stage.output_cap"]
//! at = [31.5, 22.0]        # mm from the outline's top left corner, y down
//! rotation = 90            # degrees, counterclockwise on screen
//! side = "top"             # "top" or "bottom"
//! lock = true              # the placer leaves it where it is
//! source = "hand"          # "hand", "auto" or "regen": who wrote it
//!
//! [RegulatorExample.keepouts.antenna]   # a keepout or cutout that states no `.x`, `.y`
//! at = [40.0, 0.0]
//!
//! [RegulatorExample.cutouts.slot]
//! at = [5.0, 4.0]
//!
//! [RegulatorExample.keepouts.drawn1]    # a keepout the editor drew: it has a `size`
//! at = [2.0, 2.0]
//! size = [10.0, 6.0]
//! layers = "top,bottom"    # "all" (the default), or `top`, `bottom`, `inner1`... separated by commas
//! tracks = "forbid"        # "forbid" (the default) or "allow", likewise `vias`, `copper`, `parts`
//! vias = "allow"
//! ```
//!
//! A keepout entry that gives only `at` places a keepout whose source states its size and not
//! its place; one that gives a `size` is a keepout of its own, drawn in the editor, whose
//! name the source must not use (ADR 0029).
//!
//! **What the file holds, and what it does not** (ADR 0030). The file is tier 3 of three: the
//! decisions of this design (where a person put a part, a lock, a keepout the editor drew).
//! What the circuit implies is derived by the placer (tier 1), and a datasheet's fact about a
//! part belongs in the part with its source (tier 2, the `layout` block, not built). A
//! requirement the case states goes to the physical file (ADR 0048), not here. Waivers,
//! overrides of a constraint, per-module seeds and a routes cache (`<stem>.routes.toml`, one
//! record per net with its segments, vias and a hash of its pad positions) were sketched in
//! ADR 0030 and are not read: an unknown key is an error. `place_subset` (in `place.rs`) is what regenerating one module
//! would use.
//!
//! [`apply_layout`] is the one place a layout reaches a board. A part with no entry stays
//! where the placer put it. An entry for a part, keepout or cutout that the board does not
//! have is an error that names it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use netsmith_core::Design;

use crate::board::{Board, BoardKeepout, ReferenceSide, Side};
use crate::geometry::{Pt, Rect};
use crate::placement::{CarriedZone, Fixed, HeightZone, ORIGIN, PlaceOptions};

/// The version of the file format this reads and writes.
pub const VERSION: u32 = 1;

/// Which face of the board a part is on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Face {
    #[default]
    Top,
    Bottom,
}

/// Which side of a part its reference text is on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextSide {
    #[default]
    Above,
    Below,
    Left,
    Right,
}

impl From<TextSide> for ReferenceSide {
    fn from(t: TextSide) -> Self {
        match t {
            TextSide::Above => ReferenceSide::Above,
            TextSide::Below => ReferenceSide::Below,
            TextSide::Left => ReferenceSide::Left,
            TextSide::Right => ReferenceSide::Right,
        }
    }
}

impl From<ReferenceSide> for TextSide {
    fn from(t: ReferenceSide) -> Self {
        match t {
            ReferenceSide::Above => TextSide::Above,
            ReferenceSide::Below => TextSide::Below,
            ReferenceSide::Left => TextSide::Left,
            ReferenceSide::Right => TextSide::Right,
        }
    }
}

impl TextSide {
    fn is_above(&self) -> bool {
        *self == TextSide::Above
    }
}

/// Where one part is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartPlace {
    /// The footprint's origin, in mm from the outline's top left corner, y down.
    pub at: [f64; 2],
    /// Degrees, counterclockwise on screen.
    #[serde(default)]
    pub rotation: f64,
    #[serde(default)]
    pub side: Face,
    /// Which side of the part its reference designator is written on.
    #[serde(default, skip_serializing_if = "TextSide::is_above")]
    pub text: TextSide,
    /// The placer leaves a locked part where it is.
    #[serde(default)]
    pub lock: bool,
    /// Who wrote the entry: `hand`, `auto` or `regen`. Kept as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Where a keepout or a cutout is, when its source does not say.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapePlace {
    /// The corner nearest the outline's top left corner, in mm from it.
    pub at: [f64; 2],
}

/// A keepout's entry: the place of one the source leaves to the file, or all of one drawn in
/// the editor (the entry has a `size`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeepoutPlace {
    /// The corner nearest the outline's top left corner, in mm from it.
    pub at: [f64; 2],
    /// Width and height; only a drawn keepout has them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<[f64; 2]>,
    /// `all` or layer names separated by commas; only a drawn keepout has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layers: Option<String>,
    /// What it forbids, `forbid` or `allow`; a field left out forbids. Only a drawn keepout
    /// has them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracks: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copper: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<String>,
}

impl KeepoutPlace {
    /// The entry that only places a keepout the source sizes.
    pub fn at(x: f64, y: f64) -> Self {
        Self {
            at: [x, y],
            size: None,
            layers: None,
            tracks: None,
            vias: None,
            copper: None,
            parts: None,
        }
    }

    /// Whether this is a keepout drawn in the editor, which the source does not have.
    pub fn is_drawn(&self) -> bool {
        self.size.is_some()
    }
}

/// What the file holds for one board.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardLayout {
    /// Which arrangement the placer finds for the parts the file does not place: the same
    /// seed always gives the same one. Absent is 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parts: BTreeMap<String, PartPlace>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub keepouts: BTreeMap<String, KeepoutPlace>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cutouts: BTreeMap<String, ShapePlace>,
}

/// A layout file: a table for each board.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    pub boards: BTreeMap<String, BoardLayout>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LayoutError {
    /// The file is not TOML of this shape.
    Parse { file: String, message: String },
    /// The file is of a version this does not read.
    Version { file: String, found: String },
    /// Entries for things the board does not have.
    Stale {
        file: String,
        board: String,
        /// `part`, `keepout`, `cutout` or `board`.
        what: &'static str,
        names: Vec<String>,
        /// What the board has, for the message.
        known: Vec<String>,
    },
    /// A position that is not a finite number.
    NotFinite { part: String },
    /// A keepout entry that is wrong in itself: what it says and why.
    BadKeepout {
        file: String,
        name: String,
        why: String,
    },
    /// A keepout or cutout whose source states `.x` and `.y` and whose place the file gives
    /// as well: a thing has one place.
    Twice {
        file: String,
        what: &'static str,
        name: String,
    },
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse { file, message } => write!(f, "{file}: {message}"),
            Self::Version { file, found } => write!(
                f,
                "{file}: `version` is {found}, and this reads only version {VERSION}"
            ),
            Self::Stale {
                file,
                board,
                what,
                names,
                known,
            } => {
                let list = |v: &[String]| {
                    v.iter()
                        .map(|n| format!("`{n}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                if *what == "board" {
                    write!(
                        f,
                        "{file}: has a table for the board {}, which this source does not have (its boards: {}); remove it or rename it",
                        list(names),
                        list(known)
                    )
                } else {
                    write!(
                        f,
                        "{file}: the board `{board}` has no {what} {} (its {what}s: {}); the layout is out of date, remove the entr{} or rename {}",
                        list(names),
                        if known.is_empty() {
                            "none".to_string()
                        } else {
                            list(known)
                        },
                        if names.len() == 1 { "y" } else { "ies" },
                        if names.len() == 1 { "it" } else { "them" },
                    )
                }
            }
            Self::Twice { file, what, name } => write!(
                f,
                "{file}: the {what} `{name}` states `.x` and `.y` in the source, and the file places it as well: say it in one place, so remove the entry or the two statements"
            ),
            Self::BadKeepout { file, name, why } => {
                write!(f, "{file}: the keepout `{name}` {why}")
            }
            Self::NotFinite { part } => write!(
                f,
                "the layout gives `{part}` a position that is not a number"
            ),
        }
    }
}

impl std::error::Error for LayoutError {}

impl Layout {
    /// Read a layout file's text. `file` names it in messages.
    pub fn parse(text: &str, file: &str) -> Result<Layout, LayoutError> {
        let parse_error = |message: String| LayoutError::Parse {
            file: file.to_string(),
            message,
        };
        let mut table: toml::Table = text
            .parse()
            .map_err(|e: toml::de::Error| parse_error(e.message().to_string()))?;
        match table.remove("version") {
            Some(toml::Value::Integer(v)) if v == i64::from(VERSION) => {}
            Some(other) => {
                return Err(LayoutError::Version {
                    file: file.to_string(),
                    found: other.to_string(),
                });
            }
            None => {
                return Err(parse_error(
                    "it has no `version`; write `version = 1`".into(),
                ));
            }
        }
        let mut boards = BTreeMap::new();
        for (name, value) in table {
            let bad_seed = value.get("seed").filter(
                |seed| !matches!(seed, toml::Value::Integer(n) if u32::try_from(*n).is_ok()),
            );
            if let Some(seed) = bad_seed {
                return Err(parse_error(format!(
                    "[{name}]: `seed` is {seed}, and must be a whole number from 0 to {}",
                    u32::MAX
                )));
            }
            let board: BoardLayout = value
                .try_into()
                .map_err(|e: toml::de::Error| parse_error(format!("[{name}]: {}", e.message())))?;
            boards.insert(name, board);
        }
        Ok(Layout { boards })
    }

    /// The file's text: `version`, then a table for each board, in name order, so the same
    /// layout is always the same text.
    pub fn to_text(&self) -> String {
        let mut out = format!("version = {VERSION}\n");
        for (name, board) in &self.boards {
            let mut wrapper = BTreeMap::new();
            wrapper.insert(name.clone(), board);
            out.push('\n');
            out.push_str(&toml::to_string(&wrapper).expect("a layout is plain tables"));
        }
        out
    }

    /// An error for every table of the file that is not a board of the source, so that a
    /// board renamed or removed is found.
    pub fn check_boards(&self, file: &str, boards: &[&str]) -> Result<(), LayoutError> {
        let stale: Vec<String> = self
            .boards
            .keys()
            .filter(|k| !boards.contains(&k.as_str()))
            .cloned()
            .collect();
        if stale.is_empty() {
            return Ok(());
        }
        Err(LayoutError::Stale {
            file: file.to_string(),
            board: String::new(),
            what: "board",
            names: stale,
            known: boards.iter().map(ToString::to_string).collect(),
        })
    }

    /// Where every part of a board is now, as a layout that [`apply_layout`] puts back. The
    /// entries are written by a placer (`source = "auto"`); a part already in `keep` stays as
    /// it is there (its lock, its source), with the position the board has.
    pub fn capture(board: &Board, keep: Option<&BoardLayout>) -> BoardLayout {
        let mut out = BoardLayout {
            seed: keep.and_then(|k| k.seed),
            ..BoardLayout::default()
        };
        for part in &board.parts {
            let at = part.at - board.outline.min;
            let old = keep.and_then(|k| k.parts.get(&part.path));
            out.parts.insert(
                part.path.clone(),
                PartPlace {
                    at: [round(at.x), round(at.y)],
                    rotation: part.rotation,
                    side: if part.side == Side::Back {
                        Face::Bottom
                    } else {
                        Face::Top
                    },
                    text: part.reference_side.into(),
                    lock: old.is_some_and(|o| o.lock),
                    source: old
                        .and_then(|o| o.source.clone())
                        .or_else(|| Some("auto".to_string())),
                },
            );
        }
        for k in board.keepouts.iter().filter(|k| k.part.is_none()) {
            if let Some(at) = k.at {
                let at = [round(at.x), round(at.y)];
                let entry = if k.drawn {
                    let all = copper_layers(board);
                    let word = |forbidden: bool| (!forbidden).then(|| "allow".to_string());
                    KeepoutPlace {
                        at,
                        size: Some([k.size.0, k.size.1]),
                        layers: (k.layers != all).then(|| k.layers.join(",")),
                        tracks: word(k.tracks),
                        vias: word(k.vias),
                        copper: word(k.copper),
                        parts: word(k.parts),
                    }
                } else {
                    KeepoutPlace::at(at[0], at[1])
                };
                out.keepouts.insert(k.name.clone(), entry);
            }
        }
        for c in &board.cutouts {
            if let Some(at) = c.at {
                out.cutouts.insert(
                    c.name.clone(),
                    ShapePlace {
                        at: [round(at.x), round(at.y)],
                    },
                );
            }
        }
        out
    }
}

/// The boxes the placer keeps the rows of parts clear of: every cutout and every keepout that
/// forbids parts, wherever the source or the layout file puts it (the source wins; a layout
/// entry for one the source places is an error that [`apply_layout`] reports). A keepout a
/// part carries moves with the part and is left out.
pub fn clear_of(design: &Design, layout: Option<&Layout>) -> Vec<Rect> {
    let entries = layout.and_then(|l| l.boards.get(&design.name));
    let rect = |at: (f64, f64), size: (f64, f64)| {
        let min = ORIGIN + Pt::new(at.0, at.1);
        Rect::new(min, Pt::new(min.x + size.0, min.y + size.1))
    };
    let cutouts = cutouts_of(design, layout);
    let keepouts = design
        .features
        .keepouts
        .iter()
        .filter(|k| k.part.is_none() && k.parts)
        .filter_map(|k| {
            let at = k.at.or_else(|| {
                let p = entries?.keepouts.get(&k.name)?;
                Some((p.at[0], p.at[1]))
            })?;
            Some(rect(at, k.size))
        });
    // A keepout drawn in the editor keeps parts out too, if it forbids them.
    let drawn = entries.into_iter().flat_map(|e| {
        e.keepouts
            .values()
            .filter(|k| k.is_drawn() && forbids(k.parts.as_deref()) == Ok(true))
            .filter_map(|k| {
                let size = k.size?;
                Some(rect((k.at[0], k.at[1]), (size[0], size[1])))
            })
    });
    cutouts.into_iter().chain(keepouts).chain(drawn).collect()
}

/// The areas that limit how tall a part may stand (a keepout with a `max_height`), wherever
/// the source or the layout file puts them, in the placer's coordinates. A keepout a part
/// carries moves with the part and is left out, as in [`clear_of`]: the placement check
/// still measures it.
pub fn height_zones(design: &Design, layout: Option<&Layout>) -> Vec<HeightZone> {
    let entries = layout.and_then(|l| l.boards.get(&design.name));
    design
        .features
        .keepouts
        .iter()
        .filter(|k| k.part.is_none())
        .filter_map(|k| {
            let limit = k.max_height?;
            let at = k.at.or_else(|| {
                let p = entries?.keepouts.get(&k.name)?;
                Some((p.at[0], p.at[1]))
            })?;
            let min = ORIGIN + Pt::new(at.0, at.1);
            let on = |layer: &str| k.layers.iter().any(|l| l == layer);
            Some(HeightZone {
                name: k.name.clone(),
                rect: Rect::new(min, Pt::new(min.x + k.size.0, min.y + k.size.1)),
                front: on("top"),
                back: on("bottom"),
                limit,
            })
        })
        .collect()
}

/// The keepouts that parts carry, each in its part's own frame: they move and turn with the
/// part, so the placer keeps the other parts out of them where the part is (ADR 0045
/// section 11). One that has no `at` has no place and is left out (the check names it).
pub fn carried_zones(design: &Design) -> Vec<CarriedZone> {
    design
        .features
        .keepouts
        .iter()
        .filter_map(|k| {
            let part = k.part.clone()?;
            let at = k.at?;
            let on = |layer: &str| k.layers.iter().any(|l| l == layer);
            Some(CarriedZone {
                name: k.name.clone(),
                part,
                local: Rect::new(
                    Pt::new(at.0, at.1),
                    Pt::new(at.0 + k.size.0, at.1 + k.size.1),
                ),
                parts: k.parts,
                front: on("top"),
                back: on("bottom"),
                limit: k.max_height,
            })
        })
        .collect()
}

/// The boxes of the cutouts that have a place, wherever the source or the layout file puts
/// them: the part of [`clear_of`] to which a pad keeps the process's distance from copper to
/// an edge as well as the courtyards keeping out. The part of a polygon outline's box that
/// is outside the polygon (issue #57) is among them: board edge, as a cutout's is.
pub fn cutouts_of(design: &Design, layout: Option<&Layout>) -> Vec<Rect> {
    let entries = layout.and_then(|l| l.boards.get(&design.name));
    let outside = design
        .features
        .outline
        .as_ref()
        .and_then(|o| crate::polygon::of_outline(o).ok().flatten())
        .map(|p| p.translated(ORIGIN).outside_boxes())
        .unwrap_or_default();
    design
        .features
        .cutouts
        .iter()
        .filter_map(|c| {
            let at = c.at.or_else(|| {
                let p = entries?.cutouts.get(&c.name)?;
                Some((p.at[0], p.at[1]))
            })?;
            let min = ORIGIN + Pt::new(at.0, at.1);
            Some(Rect::new(min, Pt::new(min.x + c.size.0, min.y + c.size.1)))
        })
        .chain(outside)
        .collect()
}

/// Whether a `tracks`, `vias`, `copper` or `parts` field forbids: it does unless it says
/// `allow`.
fn forbids(word: Option<&str>) -> Result<bool, String> {
    match word {
        None | Some("forbid") => Ok(true),
        Some("allow") => Ok(false),
        Some(other) => Err(format!("is \"{other}\"; say \"forbid\" or \"allow\"")),
    }
}

/// What the placer is told by the design and the layout file: the face of every part the
/// layout puts on one, the parts it locks (placed round) and its seed, and the boxes to keep clear of. The size comes from the design's
/// outline, which the board reads itself. The layout's positions are put on the board
/// afterwards by [`apply_layout`].
pub fn place_options(design: &Design, layout: Option<&Layout>) -> PlaceOptions {
    let mut options = PlaceOptions {
        keepouts: clear_of(design, layout),
        cutouts: cutouts_of(design, layout),
        height_zones: height_zones(design, layout),
        carried: carried_zones(design),
        ..PlaceOptions::default()
    };
    if let Some(entries) = layout.and_then(|l| l.boards.get(&design.name)) {
        options.seed = u64::from(entries.seed.unwrap_or(0));
        for part in &design.parts {
            if let Some(place) = entries.parts.get(&part.path) {
                let side = match place.side {
                    Face::Top => Side::Front,
                    Face::Bottom => Side::Back,
                };
                options.sides.insert(part.refdes.clone(), side);
                // A part moved by hand is put where it was moved afterwards, and the
                // findings say what it overlaps; only a locked part is placed round.
                if place.lock {
                    options.fixed.insert(
                        part.refdes.clone(),
                        Fixed {
                            at: Pt::new(place.at[0], place.at[1]),
                            rotation: place.rotation,
                            side,
                            text: place.text.into(),
                        },
                    );
                }
            }
        }
    }
    options
}

/// A position to a hundredth of a millimetre, so the file holds short numbers.
fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The copper layers of a board by name, top first: `top`, `inner1`..., `bottom`.
pub fn copper_layers(board: &Board) -> Vec<String> {
    let mut layers = vec!["top".to_string()];
    layers.extend((1..=board.inner_layers().len()).map(|n| format!("inner{n}")));
    layers.push("bottom".to_string());
    layers
}

/// The keepout a drawn entry makes, or why the entry is wrong. `layers` are the board's.
fn drawn_keepout(
    name: &str,
    place: &KeepoutPlace,
    layers: &[String],
) -> Result<BoardKeepout, String> {
    let size = place.size.expect("a drawn keepout has a size");
    if !place.at.iter().chain(&size).all(|v| v.is_finite()) {
        return Err("has a position or size that is not a number".to_string());
    }
    if size[0] <= 0.0 || size[1] <= 0.0 {
        return Err(format!(
            "has the size {} by {} mm; a keepout is wider and higher than nothing",
            size[0], size[1]
        ));
    }
    let word = |field: &str, value: &Option<String>| {
        forbids(value.as_deref()).map_err(|why| format!("has `{field}` that {why}"))
    };
    let (tracks, vias, copper, parts) = (
        word("tracks", &place.tracks)?,
        word("vias", &place.vias)?,
        word("copper", &place.copper)?,
        word("parts", &place.parts)?,
    );
    if !(tracks || vias || copper || parts) {
        return Err(
            "forbids nothing: forbid at least one of tracks, vias, copper and parts".into(),
        );
    }
    let listed = place.layers.as_deref().unwrap_or("all");
    let chosen: Vec<String> = if listed.trim() == "all" {
        layers.to_vec()
    } else {
        let mut chosen = Vec::new();
        for word in listed.split(',').map(str::trim) {
            if !layers.iter().any(|l| l == word) {
                return Err(format!(
                    "names the layer \"{word}\", and this {}-layer board has {}",
                    layers.len(),
                    layers
                        .iter()
                        .map(|l| format!("\"{l}\""))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !chosen.iter().any(|c| c == word) {
                chosen.push(word.to_string());
            }
        }
        chosen.sort_by_key(|c| layers.iter().position(|l| l == c));
        chosen
    };
    Ok(BoardKeepout {
        name: name.to_string(),
        part: None,
        at: Some(Pt::new(place.at[0], place.at[1])),
        size: (size[0], size[1]),
        layers: chosen,
        tracks,
        vias,
        copper,
        parts,
        max_height: None,
        drawn: true,
    })
}

/// Put a layout on a board: each part with an entry takes its place, and each keepout and
/// cutout that the source gives no position takes the file's. The board's own table is
/// `layout.boards[board.name]`; a layout with none changes nothing.
///
/// Nothing is changed if an entry is wrong: a part, keepout or cutout that the board does not
/// have, or a position that is not a number. `file` names the layout
/// file in the messages.
pub fn apply_layout(board: &mut Board, layout: &Layout, file: &str) -> Result<(), LayoutError> {
    let Some(entries) = layout.boards.get(&board.name) else {
        return Ok(());
    };
    let bad_keepout = |name: &str, why: &str| LayoutError::BadKeepout {
        file: file.to_string(),
        name: name.to_string(),
        why: why.to_string(),
    };
    let stale = |what: &'static str, names: Vec<String>, known: Vec<String>| {
        (!names.is_empty()).then(|| LayoutError::Stale {
            file: file.to_string(),
            board: board.name.clone(),
            what,
            names,
            known,
        })
    };
    let missing = |keys: Vec<&String>, have: Vec<String>| -> Vec<String> {
        keys.into_iter()
            .filter(|k| !have.contains(k))
            .cloned()
            .collect()
    };
    let parts: Vec<String> = board.parts.iter().map(|p| p.path.clone()).collect();
    let keepouts: Vec<String> = board
        .keepouts
        .iter()
        .filter(|k| k.part.is_none())
        .map(|k| k.name.clone())
        .collect();
    let cutouts: Vec<String> = board.cutouts.iter().map(|c| c.name.clone()).collect();
    // An entry with a `size` is a keepout of its own, drawn in the editor; the others place
    // a keepout of the source.
    let placed_keepouts: Vec<&String> = entries
        .keepouts
        .iter()
        .filter(|(_, k)| !k.is_drawn())
        .map(|(name, _)| name)
        .collect();
    let errors = [
        stale(
            "part",
            missing(entries.parts.keys().collect(), parts.clone()),
            parts,
        ),
        stale(
            "keepout",
            missing(placed_keepouts.clone(), keepouts.clone()),
            keepouts,
        ),
        stale(
            "cutout",
            missing(entries.cutouts.keys().collect(), cutouts.clone()),
            cutouts,
        ),
    ];
    if let Some(error) = errors.into_iter().flatten().next() {
        return Err(error);
    }
    for (what, names, stated) in [
        (
            "keepout",
            placed_keepouts,
            board
                .keepouts
                .iter()
                .filter(|k| k.part.is_none() && k.at.is_some())
                .map(|k| &k.name)
                .collect::<Vec<_>>(),
        ),
        (
            "cutout",
            entries.cutouts.keys().collect(),
            board
                .cutouts
                .iter()
                .filter(|c| c.at.is_some())
                .map(|c| &c.name)
                .collect(),
        ),
    ] {
        if let Some(name) = names.into_iter().find(|n| stated.contains(n)) {
            return Err(LayoutError::Twice {
                file: file.to_string(),
                what,
                name: name.clone(),
            });
        }
    }
    for (path, place) in &entries.parts {
        if !place
            .at
            .iter()
            .chain([&place.rotation])
            .all(|v| v.is_finite())
        {
            return Err(LayoutError::NotFinite { part: path.clone() });
        }
    }

    let layers = copper_layers(board);
    let mut drawn = Vec::new();
    for (name, place) in &entries.keepouts {
        if place.is_drawn() {
            if board.keepouts.iter().any(|k| &k.name == name) {
                return Err(bad_keepout(
                    name,
                    "is drawn in the layout file, and the source has a keepout of that name: rename one of them",
                ));
            }
            drawn.push(drawn_keepout(name, place, &layers).map_err(|why| bad_keepout(name, &why))?);
        } else if let Some(field) = [
            ("layers", &place.layers),
            ("tracks", &place.tracks),
            ("vias", &place.vias),
            ("copper", &place.copper),
            ("parts", &place.parts),
        ]
        .into_iter()
        .find_map(|(field, value)| value.as_ref().map(|_| field))
        {
            return Err(bad_keepout(
                name,
                &format!(
                    "gives `{field}` and no `size`: only a keepout drawn in the file, which has a `size`, says what it forbids"
                ),
            ));
        }
    }

    let origin = board.outline.min;
    for part in &mut board.parts {
        if let Some(place) = entries.parts.get(&part.path) {
            part.at = origin + Pt::new(place.at[0], place.at[1]);
            part.rotation = place.rotation;
            part.reference_side = place.text.into();
            part.side = match place.side {
                Face::Top => Side::Front,
                Face::Bottom => Side::Back,
            };
        }
    }
    for k in board.keepouts.iter_mut().filter(|k| k.part.is_none()) {
        if let Some(place) = entries.keepouts.get(&k.name) {
            k.at = Some(Pt::new(place.at[0], place.at[1]));
        }
    }
    board.keepouts.extend(drawn);
    for c in &mut board.cutouts {
        if let Some(place) = entries.cutouts.get(&c.name) {
            c.at = Some(Pt::new(place.at[0], place.at[1]));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"version = 1

[Demo.parts."stage.output_cap"]
at = [31.5, 22.0]
rotation = 90.0
side = "top"
lock = true
source = "hand"

[Demo.keepouts.antenna]
at = [40.0, 0.0]

[Demo.cutouts.slot]
at = [5.0, 4.0]
"#;

    #[test]
    fn a_keepout_a_part_carries_goes_to_the_placer_in_the_parts_frame_and_not_as_a_fixed_box() {
        // ADR 0045 section 11 (issue 28): a carried keepout moves with its part, so it is not
        // one of the boxes the placer keeps clear of, nor a height zone; it is a carried zone.
        let keepout =
            |name: &str, part: Option<&str>, parts: bool, max_height| netsmith_core::Keepout {
                name: name.into(),
                part: part.map(Into::into),
                at: Some((2.0, -5.0)),
                size: (8.0, 10.0),
                layers: vec!["top".into()],
                tracks: false,
                vias: false,
                copper: false,
                parts,
                max_height,
            };
        let mut design = Design {
            modules: Vec::new(),
            name: "Demo".into(),
            process: None,
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts: Vec::new(),
            nets: Vec::new(),
        };
        design.features.keepouts = vec![
            keepout("radio.clear", Some("radio"), true, None),
            keepout("radio.lid", Some("radio"), false, Some(3.0)),
            keepout("lid", None, false, Some(3.0)),
        ];
        let options = place_options(&design, None);
        assert!(options.keepouts.is_empty());
        assert_eq!(options.height_zones.len(), 1, "only the board's own zone");
        assert_eq!(options.carried.len(), 2);
        let (clear, lid) = (&options.carried[0], &options.carried[1]);
        assert_eq!(
            (clear.name.as_str(), clear.part.as_str()),
            ("radio.clear", "radio")
        );
        assert_eq!(
            clear.local,
            Rect::new(Pt::new(2.0, -5.0), Pt::new(10.0, 5.0))
        );
        assert!(clear.parts && clear.limit.is_none() && clear.front && !clear.back);
        assert!(!lid.parts && lid.limit == Some(3.0));
    }

    #[test]
    fn a_layout_is_read_and_written_back_the_same() {
        let layout = Layout::parse(FILE, "demo.pcb.toml").unwrap();
        let board = &layout.boards["Demo"];
        let place = &board.parts["stage.output_cap"];
        assert_eq!(place.at, [31.5, 22.0]);
        assert_eq!(place.rotation, 90.0);
        assert!(place.lock);
        assert_eq!(place.source.as_deref(), Some("hand"));
        assert_eq!(board.keepouts["antenna"].at, [40.0, 0.0]);
        assert_eq!(board.cutouts["slot"].at, [5.0, 4.0]);
        let text = layout.to_text();
        assert_eq!(Layout::parse(&text, "x").unwrap(), layout);
        assert!(text.starts_with("version = 1\n"), "{text}");
    }

    #[test]
    fn what_is_left_out_has_a_default() {
        let layout = Layout::parse(
            "version = 1\n[Demo.parts.r1]\nat = [1, 2]\n",
            "demo.pcb.toml",
        )
        .unwrap();
        let place = &layout.boards["Demo"].parts["r1"];
        assert_eq!(
            (place.rotation, place.side, place.lock, place.source.clone()),
            (0.0, Face::Top, false, None)
        );
    }

    #[test]
    fn a_wrong_version_or_a_missing_one_or_an_unknown_key_is_refused_by_name() {
        let e = |text: &str| {
            Layout::parse(text, "demo.pcb.toml")
                .unwrap_err()
                .to_string()
        };
        assert_eq!(
            e("version = 2\n"),
            "demo.pcb.toml: `version` is 2, and this reads only version 1"
        );
        assert!(e("[Demo.parts.r1]\nat = [1, 2]\n").contains("it has no `version`"));
        let unknown = e("version = 1\n[Demo.parts.r1]\nat = [1, 2]\nspin = 3\n");
        assert!(
            unknown.contains("demo.pcb.toml: [Demo]") && unknown.contains("unknown field `spin`"),
            "{unknown}"
        );
        assert!(e("version = 1\n[Demo.parts.r1]\nrotation = 3\n").contains("missing field `at`"));
        assert!(e("not toml [").starts_with("demo.pcb.toml: "));
    }

    #[test]
    fn a_table_for_a_board_the_source_lacks_is_found() {
        let layout =
            Layout::parse("version = 1\n[Old.parts.r1]\nat = [1, 2]\n", "d.pcb.toml").unwrap();
        let err = layout
            .check_boards("d.pcb.toml", &["Demo", "Other"])
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "d.pcb.toml: has a table for the board `Old`, which this source does not have (its boards: `Demo`, `Other`); remove it or rename it"
        );
        assert!(layout.check_boards("d.pcb.toml", &["Old"]).is_ok());
    }

    fn led_with_paths() -> Design {
        let mut design = crate::board::tests::led_design();
        for p in &mut design.parts {
            p.path = p.refdes.to_lowercase();
        }
        design
    }

    const SEEDED: &str = r#"version = 1

[LedExample]
seed = 7

[LedExample.parts.r1]
at = [5.0, 4.0]
rotation = 90.0
lock = true

[LedExample.parts.d1]
at = [12.0, 6.0]
side = "bottom"
"#;

    #[test]
    fn a_seed_is_read_written_back_and_kept_when_the_layout_is_captured() {
        let layout = Layout::parse(SEEDED, "led.pcb.toml").unwrap();
        assert_eq!(layout.boards["LedExample"].seed, Some(7));
        let text = layout.to_text();
        assert!(text.contains("seed = 7"), "{text}");
        assert_eq!(Layout::parse(&text, "x").unwrap(), layout);
        let design = led_with_paths();
        let board = crate::board::tests::placed_with(&design, &layout);
        let captured = Layout::capture(&board, layout.boards.get("LedExample"));
        assert_eq!(captured.seed, Some(7));
        assert_eq!(Layout::capture(&board, None).seed, None);
    }

    #[test]
    fn a_seed_that_is_not_a_whole_number_is_refused_by_name() {
        let e = |text: &str| Layout::parse(text, "led.pcb.toml").unwrap_err().to_string();
        let message = e("version = 1\n[LedExample]\nseed = \"seven\"\n");
        assert_eq!(
            message,
            "led.pcb.toml: [LedExample]: `seed` is \"seven\", and must be a whole number from 0 to 4294967295"
        );
        let negative = e("version = 1\n[LedExample]\nseed = -1\n");
        assert!(
            negative.contains("`seed` is -1, and must be a whole number"),
            "{negative}"
        );
    }

    #[test]
    fn the_placer_is_told_the_seed_the_faces_and_the_parts_the_layout_locks() {
        let layout = Layout::parse(SEEDED, "led.pcb.toml").unwrap();
        let design = led_with_paths();
        let options = place_options(&design, Some(&layout));
        assert_eq!(options.seed, 7);
        assert_eq!(
            options.fixed["R1"],
            Fixed {
                at: Pt::new(5.0, 4.0),
                rotation: 90.0,
                side: Side::Front,
                text: ReferenceSide::Above,
            }
        );
        // D1 is on the back but not locked: the placer is told its face and nothing else.
        assert_eq!(options.sides["D1"], Side::Back);
        assert!(!options.fixed.contains_key("D1") && !options.fixed.contains_key("J1"));
        // The board has them there, and the part the layout does not name is placed round
        // them without overlapping.
        let board = crate::board::tests::placed_with(&design, &layout);
        let r1 = board.part("R1").unwrap();
        assert_eq!((r1.at, r1.rotation), (ORIGIN + Pt::new(5.0, 4.0), 90.0));
        assert_eq!(board.part("D1").unwrap().side, Side::Back);
        assert_eq!(board.check_placement(), vec![]);
        // With no layout there is nothing fixed and the seed is 0.
        let none = place_options(&design, None);
        assert_eq!((none.seed, none.fixed.len()), (0, 0));
    }

    #[test]
    fn the_seed_and_the_text_side_survive_a_round_trip_through_the_file() {
        // The editor rewrites the file with `to_text`: what it does not edit must stay.
        let text = "version = 1\n[Demo]\nseed = 9\n[Demo.parts.r1]\nat = [1.0, 2.0]\ntext = \"left\"\nlock = true\n";
        let layout = Layout::parse(text, "demo.pcb.toml").unwrap();
        assert_eq!(layout.boards["Demo"].seed, Some(9));
        assert_eq!(layout.boards["Demo"].parts["r1"].text, TextSide::Left);
        let again = Layout::parse(&layout.to_text(), "demo.pcb.toml").unwrap();
        assert_eq!(again, layout);
        assert!(
            layout.to_text().contains("seed = 9"),
            "{}",
            layout.to_text()
        );
        assert!(
            layout.to_text().contains("text = \"left\""),
            "{}",
            layout.to_text()
        );
    }
}
