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
//! ```
//!
//! A keepout entry gives only `at`: it places a keepout whose physical file states its size and
//! not its place. A keepout of its own, which an earlier editor drew here whole (a `size`, and
//! `layers`, `tracks`, `vias`, `copper`, `parts`), is a requirement and goes in the physical
//! file (ADR 0048 decision 6; the editor writes the keepouts it draws there, issue #809). Such
//! an entry is an error, with no grace period (ADR 0001, issue #812), whose message gives the
//! `Keepout` to write there ([`keepout_declaration`]).
//!
//! **What the file holds, and what it does not** (ADR 0030). The file is tier 3 of three: the
//! decisions of this design (where a person put a part, a lock, a keepout the editor drew).
//! What the circuit implies is derived by the placer (tier 1), and a datasheet's fact about a
//! part belongs in the part with its source (tier 2, the `layout` block, not built). A
//! requirement the case states goes to the physical file (ADR 0048), not here. Waivers,
//! overrides of a constraint and per-module seeds were sketched in ADR 0030 and are not read:
//! an unknown key is an error. The routes ADR 0030 sketched are a file of their own beside this
//! one, the routes file `<stem>.routes.toml` ([`crate::Routes`], issue #599), which whatever
//! reads this file reads too. Regenerating one module (`netsmith layout regenerate`, issue #825)
//! writes the entries of the parts it placed again with `source = "regen"`.
//!
//! [`apply_layout`] is the one place a layout reaches a board. A part with no entry stays
//! where the placer put it. An entry for a part, keepout or cutout that the board does not
//! have is an error that names it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use netsmith_core::Design;

use crate::board::{Board, BoardKeepout, ReferenceSide, Side};
use crate::geometry::{Pt, Rect};
use crate::placement::{CarriedZone, FaceKeepout, Fixed, HeightZone, ORIGIN, PlaceOptions};

/// The version of the file format this reads and writes.
pub(crate) const VERSION: u32 = 1;

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
    /// Who wrote the entry: `hand`, `auto`, `regen` or `import` ([`IMPORT_SOURCE`]: two
    /// parts placed by an import may share courtyard area). Kept as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// The `source` of an entry that `netsmith routes import` wrote: the part stands where a KiCad
/// board has it. Two such parts may share courtyard area ([`crate::Board::imported`]).
pub const IMPORT_SOURCE: &str = "import";

impl PartPlace {
    /// Whether the entry puts the part where an imported KiCad board has it.
    pub fn is_imported(&self) -> bool {
        self.source.as_deref() == Some(IMPORT_SOURCE)
    }
}

/// Where a keepout or a cutout is, when its source does not say.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapePlace {
    /// The corner nearest the outline's top left corner, in mm from it.
    pub at: [f64; 2],
}

/// A keepout's entry: the place of one the physical file leaves to the layout file. The other
/// fields are of a keepout stated whole, which the layout file no longer holds: they are read
/// so that [`apply_layout`] can say where the keepout goes, and the editor describes a keepout
/// it draws in the physical file with them ([`keepout_declaration`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeepoutPlace {
    /// The corner nearest the outline's top left corner, in mm from it.
    pub at: [f64; 2],
    /// Width and height; only a keepout stated whole has them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<[f64; 2]>,
    /// `all` or layer names separated by commas; only a keepout stated whole has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layers: Option<String>,
    /// What it forbids, `forbid` or `allow`; a field left out forbids. Only a keepout stated
    /// whole has them.
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

    /// The fields beyond `at` that the entry gives, which only a keepout stated whole has.
    fn whole_fields(&self) -> Vec<&'static str> {
        [
            ("size", self.size.is_some()),
            ("layers", self.layers.is_some()),
            ("tracks", self.tracks.is_some()),
            ("vias", self.vias.is_some()),
            ("copper", self.copper.is_some()),
            ("parts", self.parts.is_some()),
        ]
        .into_iter()
        .filter_map(|(field, given)| given.then_some(field))
        .collect()
    }
}

/// A length as a declaration of the physical file states it: `2mm`, `2.5mm`, `-1mm`.
fn length(v: f64) -> String {
    let v = (v * 100.0).round() / 100.0;
    // No `-0mm`.
    let v = if v == 0.0 { 0.0 } else { v };
    format!("{v}mm")
}

/// The `Keepout` member of a board's physical file that states the keepout `place` describes
/// whole, at column 0, one field to a line: no field says what a keepout says by default.
/// `place` has a `size`.
pub fn keepout_declaration(name: &str, place: &KeepoutPlace) -> String {
    let [width, height] = place.size.expect("a keepout stated whole has a size");
    let mut text = format!(
        ".{name}: Keepout {{\n    .x &= {}\n    .y &= {}\n    .width &= {}\n    .height &= {}\n",
        length(place.at[0]),
        length(place.at[1]),
        length(width),
        length(height)
    );
    if let Some(layers) = &place.layers {
        text.push_str(&format!("    .layers &= \"{layers}\"\n"));
    }
    for (field, word) in [
        ("tracks", &place.tracks),
        ("vias", &place.vias),
        ("copper", &place.copper),
        ("parts", &place.parts),
    ] {
        if let Some(word) = word {
            text.push_str(&format!("    .{field} &= \"{word}\"\n"));
        }
    }
    text.push_str("}\n");
    text
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

/// The table of `table` that is named `name`.
fn child<'a, 'i>(
    table: &'a toml::de::DeTable<'i>,
    name: &str,
) -> Option<&'a toml::de::DeTable<'i>> {
    table
        .iter()
        .find(|(k, _)| k.get_ref().as_ref() == name)
        .and_then(|(_, v)| v.get_ref().as_table())
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

/// What to say of an entry that names a capacitor of a bank in the old way (`bulk#2`, the second
/// capacitor; `bulk`, the first). A bank's capacitors are `bulk[0]`, `bulk[1]`, counted from 0 as
/// the members of `N x Kind` are (issue #685). Empty when no name is of the old form.
fn bank_renames(names: &[String], known: &[String]) -> String {
    let now: Vec<String> = names
        .iter()
        .filter_map(|name| {
            let new = match name.rsplit_once('#') {
                Some((base, n)) if !base.is_empty() && !n.is_empty() => {
                    let k: usize = n.parse().ok().filter(|k| *k >= 1)?;
                    format!("{base}[{}]", k - 1)
                }
                _ => format!("{name}[0]"),
            };
            known
                .contains(&new)
                .then(|| format!("`{name}` is now `{new}`"))
        })
        .collect();
    if now.is_empty() {
        String::new()
    } else {
        format!(
            " (the capacitors of a bank are `name[0]`, `name[1]`, ...: {})",
            now.join(", ")
        )
    }
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
                        "{file}: the board `{board}` has no {what} {} (its {what}s: {}); the layout is out of date, remove the entr{} or rename {}{}",
                        list(names),
                        if known.is_empty() {
                            "none".to_string()
                        } else {
                            list(known)
                        },
                        if names.len() == 1 { "y" } else { "ies" },
                        if names.len() == 1 { "it" } else { "them" },
                        if *what == "part" {
                            bank_renames(names, known)
                        } else {
                            String::new()
                        },
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

    /// The file's text as [`Self::to_text`] has it, with the comments of `old`, the file as it
    /// is now, carried into it (issue #827; [`crate::layout_notes`] says where each goes). A
    /// file that already says this layout is returned as it is, byte for byte, and one that
    /// is not TOML is written as `to_text` has it.
    pub fn to_text_over(&self, old: &str) -> String {
        if matches!(Layout::parse(old, ""), Ok(read) if read == *self) {
            return old.to_string();
        }
        crate::layout_notes::carry(old, &self.to_text())
    }

    /// The line, counted from 1, where each part entry of the board `board` is written in the
    /// layout file's `text`, by the part's path: the line of its table's header, or of its key
    /// where the table is written in another way. A text that is not TOML gives none.
    pub fn part_lines(text: &str, board: &str) -> BTreeMap<String, usize> {
        let Ok(document) = toml::de::DeTable::parse(text) else {
            return BTreeMap::new();
        };
        let line_of = |offset: usize| text[..offset.min(text.len())].matches('\n').count() + 1;
        let Some(parts) = child(document.get_ref(), board).and_then(|board| child(board, "parts"))
        else {
            return BTreeMap::new();
        };
        parts
            .iter()
            .map(|(k, _)| (k.get_ref().to_string(), line_of(k.span().start)))
            .collect()
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
                out.keepouts
                    .insert(k.name.clone(), KeepoutPlace::at(round(at.x), round(at.y)));
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

/// The faces of the board whose parts a keepout keeps out, from its `.layers` (`all`, or
/// layer names separated by commas): `(front, back)`. A part stands on a face, so a keepout
/// whose layers are all inner ones keeps out no part.
fn faces_of(layers: &[String]) -> (bool, bool) {
    let on = |name: &str| layers.iter().any(|l| l == name);
    (on("top"), on("bottom"))
}

/// The keepouts that forbid parts, wherever the source or the layout file puts them, with the
/// faces whose parts each keeps out: its name, its box and `(front, back)`. A keepout a part
/// carries moves with the part and is left out; so is one that keeps out no face.
fn forbidding(design: &Design, layout: Option<&Layout>) -> Vec<(String, Rect, (bool, bool))> {
    let entries = layout.and_then(|l| l.boards.get(&design.name));
    let rect = |at: (f64, f64), size: (f64, f64)| {
        let min = ORIGIN + Pt::new(at.0, at.1);
        Rect::new(min, Pt::new(min.x + size.0, min.y + size.1))
    };
    let mut found: Vec<(String, Rect, (bool, bool))> = design
        .features
        .keepouts
        .iter()
        .filter(|k| k.part.is_none() && k.parts)
        .filter_map(|k| {
            let at = k.at.or_else(|| {
                let p = entries?.keepouts.get(&k.name)?;
                Some((p.at[0], p.at[1]))
            })?;
            Some((k.name.clone(), rect(at, k.size), faces_of(&k.layers)))
        })
        .collect();
    found.retain(|(_, _, (front, back))| *front || *back);
    found
}

/// The boxes the placer keeps the rows of parts clear of: every cutout and every keepout that
/// forbids parts on both faces, wherever the source or the layout file puts it (the source
/// wins; a layout entry for one the source places is an error that [`apply_layout`]
/// reports). A keepout a part carries moves with the part and is left out. One that forbids
/// parts on a single face is in [`face_keepouts`].
pub(crate) fn clear_of(design: &Design, layout: Option<&Layout>) -> Vec<Rect> {
    let both = forbidding(design, layout)
        .into_iter()
        .filter(|(_, _, (front, back))| *front && *back)
        .map(|(_, rect, _)| rect);
    cutouts_of(design, layout).into_iter().chain(both).collect()
}

/// The keepouts that forbid parts on one face of the board only (`.layers &= "bottom"`,
/// issue #767), wherever the source or the layout file puts them, for the placer.
pub(crate) fn face_keepouts(design: &Design, layout: Option<&Layout>) -> Vec<FaceKeepout> {
    forbidding(design, layout)
        .into_iter()
        .filter(|(_, _, (front, back))| front != back)
        .map(|(name, rect, (front, back))| FaceKeepout {
            name,
            rect,
            front,
            back,
        })
        .collect()
}

/// The areas that limit how tall a part may stand (a keepout with a `max_height`), wherever
/// the source or the layout file puts them, in the placer's coordinates. A keepout a part
/// carries moves with the part and is left out, as in [`clear_of`]: the placement check
/// still measures it.
pub(crate) fn height_zones(design: &Design, layout: Option<&Layout>) -> Vec<HeightZone> {
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
pub(crate) fn carried_zones(design: &Design) -> Vec<CarriedZone> {
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
                on_carrier_face: false,
            })
        })
        .collect()
}

/// The part of a polygon outline's box that is outside the polygon, as boxes with the sides of
/// the box each piece of it touches (`PlaceOptions::outside`).
fn outside_of(design: &Design) -> Vec<(Rect, crate::Edges)> {
    design
        .features
        .outline
        .as_ref()
        .and_then(|o| crate::polygon::of_outline(o).ok().flatten())
        .map(|p| p.translated(ORIGIN).outside_pieces())
        .unwrap_or_default()
        .into_iter()
        .flat_map(|(boxes, sides)| boxes.into_iter().map(move |b| (b, sides)))
        .collect()
}

/// The boxes of the cutouts that have a place, wherever the source or the layout file puts
/// them: the part of [`clear_of`] to which a pad keeps the process's distance from copper to
/// an edge as well as the courtyards keeping out. The part of a polygon outline's box that
/// is outside the polygon (issue #57) is among them: board edge, as a cutout's is.
pub(crate) fn cutouts_of(design: &Design, layout: Option<&Layout>) -> Vec<Rect> {
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
        face_keepouts: face_keepouts(design, layout),
        cutouts: cutouts_of(design, layout),
        outside: outside_of(design),
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
                if place.lock && place.is_imported() {
                    options.imported.insert(part.refdes.clone());
                }
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

/// Why a keepout the editor draws is wrong, if it is, in the words of a mistake after its name
/// (``the keepout `k` has the size 0 by 3 mm; ...``). The editor checks a keepout it draws with
/// this before it writes it to the physical file. `layers` are the board's copper layers.
pub fn check_drawn_keepout(
    name: &str,
    place: &KeepoutPlace,
    layers: &[String],
) -> Result<(), String> {
    if place.size.is_none() {
        return Err(format!("the keepout `{name}` has no size"));
    }
    drawn_keepout(name, place, layers)
        .map(|_| ())
        .map_err(|why| format!("the keepout `{name}` {why}"))
}

/// The keepout a drawn one makes, or why it is wrong. `layers` are the board's.
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
    // A keepout stated whole is a requirement, and the layout file holds none (ADR 0048
    // decision 6, issue #812): the entry is refused with the member to write in the physical
    // file, before what it names is looked for.
    for (name, place) in &entries.keepouts {
        let fields = place.whole_fields();
        if fields.is_empty() {
            continue;
        }
        let physical = file.strip_suffix(".pcb.toml").map_or_else(
            || "the board's physical file".to_string(),
            |stem| format!("`{stem}.physical.nsm`"),
        );
        let why = if place.size.is_some() {
            // On one line, its fields separated by `;`.
            let declaration = keepout_declaration(name, place);
            let lines: Vec<&str> = declaration.trim_end().lines().collect();
            let fields: Vec<&str> = lines[1..lines.len() - 1].iter().map(|l| l.trim()).collect();
            let declaration = format!("{} {} }}", lines[0], fields.join("; "));
            format!(
                "is stated whole here (it gives `size`): a keepout of its own is a requirement, and it goes in the physical file (ADR 0048). Remove the entry and write `{declaration}` in `physical for {} {{ }}` of {physical}",
                board.name
            )
        } else {
            format!(
                "gives `{}`: what a keepout forbids and its layers are stated in {physical}, and its entry here gives only `at`",
                fields.join("`, `")
            )
        };
        return Err(bad_keepout(name, &why));
    }
    let placed_keepouts: Vec<&String> = entries.keepouts.keys().collect();
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

    let origin = board.outline.min;
    board.imported.clear();
    for part in &mut board.parts {
        if let Some(place) = entries.parts.get(&part.path) {
            if place.is_imported() {
                board.imported.insert(part.refdes.clone());
            }
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
    fn the_line_of_each_part_entry_is_found() {
        let text = "version = 1\n\n[Demo]\nseed = 1\n\n[Demo.parts.\"bulk[1]\"]\nat = [1.0, 2.0]\n\n[Demo.parts.led]\nat = [3.0, 4.0]\n\n[Other.parts.led]\nat = [5.0, 6.0]\n";
        let lines = Layout::part_lines(text, "Demo");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines["bulk[1]"], 6);
        assert_eq!(lines["led"], 9);
        assert_eq!(Layout::part_lines(text, "Other")["led"], 12);
        assert!(Layout::part_lines(text, "Missing").is_empty());
        assert!(Layout::part_lines("not toml [", "Demo").is_empty());
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

    /// A layout file a person has commented.
    const COMMENTED: &str = "# The demo board's layout: kept by hand.\nversion = 1\n\n# the board\n[Demo]\nseed = 3   # tried a few\n\n# the bulk capacitor, near the connector\n[Demo.parts.bulk]\nat = [1.0, 2.0]   # mm from the corner\nlock = true   # do not move\n\n# the LED stays\n[Demo.parts.led]\nat = [3.0, 4.0]\nsource = \"hand#1\"   # a # in a string is no comment\n\n# last words\n";

    fn moved(text: &str, part: &str, at: [f64; 2]) -> String {
        let mut layout = Layout::parse(text, "demo.pcb.toml").unwrap();
        layout
            .boards
            .get_mut("Demo")
            .unwrap()
            .parts
            .get_mut(part)
            .unwrap()
            .at = at;
        layout.to_text_over(text)
    }

    #[test]
    fn the_comments_of_a_file_stay_when_a_part_is_moved() {
        let out = moved(COMMENTED, "led", [9.0, 8.0]);
        for kept in [
            "# The demo board's layout: kept by hand.\nversion = 1",
            "# the board\n[Demo]\nseed = 3   # tried a few",
            "# the bulk capacitor, near the connector\n[Demo.parts.bulk]\nat = [1.0, 2.0]   # mm from the corner",
            "lock = true   # do not move",
            "# the LED stays\n[Demo.parts.led]\nat = [9.0, 8.0]",
            "source = \"hand#1\"   # a # in a string is no comment",
        ] {
            assert!(out.contains(kept), "{kept:?} is gone from\n{out}");
        }
        assert!(out.trim_end().ends_with("# last words"), "{out}");
        let again = Layout::parse(&out, "demo.pcb.toml").unwrap();
        assert_eq!(again.boards["Demo"].parts["led"].at, [9.0, 8.0]);
        assert_eq!(again.boards["Demo"].parts["bulk"].at, [1.0, 2.0]);
        // A second write of the same layout changes nothing.
        assert_eq!(again.to_text_over(&out), out);
    }

    #[test]
    fn a_file_that_says_the_layout_is_returned_as_it_is() {
        let spaced = "version=1\n\n\n[Demo.parts.led]   # hand spacing\nat = [3,   4]\n";
        let layout = Layout::parse(spaced, "demo.pcb.toml").unwrap();
        assert_eq!(layout.to_text_over(spaced), spaced);
    }

    #[test]
    fn the_comments_of_an_entry_that_is_removed_go_with_it() {
        let mut layout = Layout::parse(COMMENTED, "demo.pcb.toml").unwrap();
        layout.boards.get_mut("Demo").unwrap().parts.remove("bulk");
        let out = layout.to_text_over(COMMENTED);
        assert!(!out.contains("bulk"), "{out}");
        assert!(!out.contains("do not move"), "{out}");
        assert!(out.contains("# the LED stays\n[Demo.parts.led]"), "{out}");
        assert!(out.contains("# the board\n[Demo]"), "{out}");
    }

    #[test]
    fn the_comment_of_a_key_that_is_not_written_any_more_goes_above_its_table() {
        let mut layout = Layout::parse(COMMENTED, "demo.pcb.toml").unwrap();
        layout
            .boards
            .get_mut("Demo")
            .unwrap()
            .parts
            .get_mut("led")
            .unwrap()
            .source = None;
        let out = layout.to_text_over(COMMENTED);
        assert!(
            out.contains("# the LED stays\n# a # in a string is no comment\n[Demo.parts.led]"),
            "{out}"
        );
    }

    #[test]
    fn the_comment_of_a_board_header_the_text_does_not_write_goes_above_what_is_left() {
        let text =
            "version = 1\n\n# the demo\n[Demo]\nseed = 5\n\n[Demo.parts.led]\nat = [3.0, 4.0]\n";
        let mut layout = Layout::parse(text, "demo.pcb.toml").unwrap();
        layout.boards.get_mut("Demo").unwrap().seed = None;
        let out = layout.to_text_over(text);
        assert!(out.contains("# the demo\n[Demo.parts.led]"), "{out}");
        assert!(!out.contains("seed"), "{out}");
    }

    #[test]
    fn a_file_that_is_not_toml_is_written_as_to_text_has_it() {
        let layout = Layout::parse(COMMENTED, "demo.pcb.toml").unwrap();
        assert_eq!(layout.to_text_over("not toml ["), layout.to_text());
    }

    #[test]
    fn a_banks_capacitors_are_keyed_by_their_index_and_the_old_form_is_refused_with_the_new_one() {
        // The capacitors of a bank of two or more are `bulk[0]`, `bulk[1]` (issue #685); TOML
        // quotes the key.
        let layout = Layout::parse(
            "version = 1\n[Demo.parts.\"bulk[1]\"]\nat = [1, 2]\n",
            "demo.pcb.toml",
        )
        .unwrap();
        assert!(layout.boards["Demo"].parts.contains_key("bulk[1]"));
        assert!(layout.to_text().contains("[Demo.parts.\"bulk[1]\"]"));
        let stale = |names: &[&str]| {
            LayoutError::Stale {
                file: "demo.pcb.toml".into(),
                board: "Demo".into(),
                what: "part",
                names: names.iter().map(ToString::to_string).collect(),
                known: vec!["bulk[0]".into(), "bulk[1]".into(), "r1".into()],
            }
            .to_string()
        };
        let text = stale(&["bulk#2", "bulk"]);
        assert!(
            text.contains("`bulk#2` is now `bulk[1]`") && text.contains("`bulk` is now `bulk[0]`"),
            "{text}"
        );
        assert!(!stale(&["r2"]).contains("is now"));
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
        let board = crate::board::tests::led_board();
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
        // (The placer puts them there: `netsmith-placer`'s `board::tests`.)
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
