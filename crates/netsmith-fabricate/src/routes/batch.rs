//! Many shapes added to the routes file in one call (issue #1472): `netsmith routes add SHAPES`
//! and its tool `routes_add` of `netsmith mcp`. The document holds `[[track]]`, `[[via]]` and
//! `[[zone]]` tables (TOML) or the arrays `track`, `via` and `zone` (JSON), each with the fields
//! of the flags of `routes add-track`, `add-via` and `add-zone` ([`Document`]). Each shape is
//! checked as that verb checks it, one after another in the order tracks, vias, zones (each in
//! the document's order), on the board with the shapes made before it: a shape refused is
//! skipped and said, with the findings of the design rules check that refused it, and the rest
//! go on (or, with `stop_on_refusal`, the batch stops there and keeps what was made before it).
//! The routes file is written once, by the caller, with every shape made.
//!
//! Each check is made where its shapes are ([`super::edit_all`], issue #1471), several shapes in
//! a row at once ([`add`], [`GROUP`]), and the board with the routes file's copper on it is
//! carried from one check to the next, so a batch costs its shapes' neighbourhoods, not a check
//! of the whole board for each shape. The check of a few items is mostly its own setting up
//! (about 10 ms of a release build, of the board's rules and fills), so checking a group at
//! once is what makes 2,000 shapes take seconds rather than minutes (`routes_edit::
//! measured_batch`: about 20 s on a board of 154 parts and four layers).

use serde::{Deserialize, Serialize};

use netsmith_pcb::{Board, CopperLayer, Routes, ZonePads};

use super::{Checking, Edit, EditCounts, Refusal};

/// The document of a batch. A field left out is the flag's default.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    #[serde(default)]
    pub track: Vec<TrackShape>,
    #[serde(default)]
    pub via: Vec<ViaShape>,
    #[serde(default)]
    pub zone: Vec<ZoneShape>,
}

/// A track, as `routes add-track` takes it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackShape {
    pub net: String,
    /// `top` (the default), `bottom` or an inner signal layer, `inner2`.
    #[serde(default)]
    pub layer: Option<String>,
    /// In mm; none: the net's own width on the layer.
    #[serde(default)]
    pub width: Option<f64>,
    pub from: [f64; 2],
    pub to: [f64; 2],
    /// For an arc, a point on it halfway from `from` to `to`.
    #[serde(default)]
    pub mid: Option<[f64; 2]>,
    #[serde(default)]
    pub lock: bool,
}

/// A via, or a group of vias side by side, as `routes add-via` takes it: `at` one place, or a
/// list of places (a group, checked together), or one place with `count` and `pitch`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViaShape {
    pub net: String,
    pub at: Places,
    #[serde(default)]
    pub count: Option<usize>,
    #[serde(default)]
    pub pitch: Option<f64>,
    #[serde(default)]
    pub diameter: Option<f64>,
    #[serde(default)]
    pub drill: Option<f64>,
    /// A blind or buried via's two layers, `["top", "inner1"]`; none: a through via.
    #[serde(default)]
    pub layers: Option<[String; 2]>,
    #[serde(default)]
    pub lock: bool,
}

/// One place or several.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Places {
    One([f64; 2]),
    Many(Vec<[f64; 2]>),
}

/// A zone, as `routes add-zone` takes it: one of `outline`, `rect` and `board_outline`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZoneShape {
    pub net: String,
    /// A layer, or a list of them (`top` by default).
    #[serde(default)]
    pub layer: Option<Layers>,
    #[serde(default)]
    pub outline: Option<Vec<[f64; 2]>>,
    /// `[x, y, w, h]`: the top left corner and the size.
    #[serde(default)]
    pub rect: Option<[f64; 4]>,
    #[serde(default)]
    pub board_outline: bool,
    #[serde(default)]
    pub clearance: Option<f64>,
    #[serde(default)]
    pub min_width: Option<f64>,
    #[serde(default)]
    pub thermal_gap: Option<f64>,
    #[serde(default)]
    pub spoke_width: Option<f64>,
    #[serde(default)]
    pub priority: u32,
    /// `solid`, `thermal` (the default), `none` or `thru_hole_only` (`thru-hole-only` too, as
    /// the flag spells it).
    #[serde(default)]
    pub pads: Option<String>,
    /// A patch of the pour or plane of its net (issue #1608): `routes add-zone --patch`.
    #[serde(default)]
    pub patch: bool,
    #[serde(default)]
    pub lock: bool,
}

/// One layer or several.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Layers {
    One(String),
    Many(Vec<String>),
}

impl Document {
    /// The document in `text`, JSON when `json`, TOML otherwise; `name` names it in an error.
    pub fn parse(text: &str, name: &str, json: bool) -> Result<Document, String> {
        let doc: Document = if json {
            serde_json::from_str(text).map_err(|e| format!("{name}: {e}"))?
        } else {
            toml::from_str(text).map_err(|e| format!("{name}: {e}"))?
        };
        if doc.shapes() == 0 {
            return Err(format!(
                "{name}: no shape: give `[[track]]`, `[[via]]` or `[[zone]]` tables (or the arrays `track`, `via` and `zone` in JSON)"
            ));
        }
        Ok(doc)
    }

    /// Whether the document of a file is JSON: by its extension, `.json`; for stdin (`-`) and a
    /// file of another extension, when its text starts with `{`.
    pub fn is_json(path: &str, text: &str) -> bool {
        match std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
        {
            Some(e) if e.eq_ignore_ascii_case("json") => true,
            Some(e) if e.eq_ignore_ascii_case("toml") => false,
            _ => text.trim_start().starts_with('{'),
        }
    }

    /// How many entries the document has (a group of vias is one).
    pub fn shapes(&self) -> usize {
        self.track.len() + self.via.len() + self.zone.len()
    }

    /// Each entry as the edits it makes, in the order the batch checks them, or why it cannot
    /// be one (a word that is not a layer, a zone with no outline).
    fn edits(&self, board: &Board) -> Vec<(Which, Result<Vec<Edit>, String>)> {
        let mut out = Vec::new();
        for (n, t) in self.track.iter().enumerate() {
            out.push((Which::new("track", n, &t.net), track(t).map(|e| vec![e])));
        }
        for (n, v) in self.via.iter().enumerate() {
            out.push((Which::new("via", n, &v.net), via(v)));
        }
        for (n, z) in self.zone.iter().enumerate() {
            out.push((
                Which::new("zone", n, &z.net),
                zone(z, board).map(|e| vec![e]),
            ));
        }
        out
    }
}

fn layer(word: &str) -> Result<CopperLayer, String> {
    CopperLayer::of_word(word).ok_or_else(|| {
        format!("`{word}` is not a copper layer: `top`, `bottom` or `inner1`, `inner2`, ...")
    })
}

fn track(t: &TrackShape) -> Result<Edit, String> {
    Ok(Edit::AddTrack {
        net: t.net.clone(),
        layer: layer(t.layer.as_deref().unwrap_or("top"))?,
        width: t.width,
        from: t.from,
        mid: t.mid,
        to: t.to,
        lock: t.lock,
    })
}

fn via(v: &ViaShape) -> Result<Vec<Edit>, String> {
    let layers = match &v.layers {
        Some([a, b]) => {
            let (a, b) = (layer(a)?, layer(b)?);
            if a == b {
                return Err(format!(
                    "`layers` names {} twice: a via joins two layers",
                    a.word()
                ));
            }
            Some([a, b])
        }
        None => None,
    };
    let places = match (&v.at, v.count, v.pitch) {
        (Places::One(at), Some(count), Some(pitch)) => {
            if count == 0 || !(pitch.is_finite() && pitch > 0.0) {
                return Err("`count` is 1 or more and `pitch` above 0 mm".to_string());
            }
            super::via_row(*at, count, pitch).map_err(|e| e.to_string())?
        }
        (_, None, None) => match &v.at {
            Places::One(at) => vec![*at],
            Places::Many(at) if at.is_empty() => {
                return Err("`at` has no place".to_string());
            }
            Places::Many(at) => at.clone(),
        },
        (Places::Many(_), _, _) => {
            return Err(
                "`count` and `pitch` go round one place `at`; for several places give `at` a list and no `count`"
                    .to_string(),
            );
        }
        _ => return Err("`count` and `pitch` go together".to_string()),
    };
    if places.len() > super::MOST_VIAS_IN_A_GROUP {
        return Err(format!(
            "a group has at most {} vias",
            super::MOST_VIAS_IN_A_GROUP
        ));
    }
    Ok(places
        .into_iter()
        .map(|at| Edit::AddVia {
            net: v.net.clone(),
            at,
            diameter: v.diameter,
            drill: v.drill,
            layers,
            lock: v.lock,
        })
        .collect())
}

fn zone(z: &ZoneShape, board: &Board) -> Result<Edit, String> {
    let outline = match (&z.outline, z.rect, z.board_outline) {
        (Some(o), None, false) => o.clone(),
        (None, Some([x, y, w, h]), false) => {
            if !(w > 0.0 && h > 0.0) {
                return Err("`rect`: the width and height must be above 0".to_string());
            }
            vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]]
        }
        (None, None, true) => super::board_outline(board),
        (None, None, false) => {
            return Err(
                "a zone needs its outline: `outline`, `rect` or `board_outline = true`".to_string(),
            );
        }
        _ => return Err("give one of `outline`, `rect` and `board_outline`".to_string()),
    };
    let pads = match z.pads.as_deref().map(|p| p.replace('-', "_")).as_deref() {
        None | Some("thermal") => ZonePads::Thermal,
        Some("solid") => ZonePads::Solid,
        Some("none") => ZonePads::None,
        Some("thru_hole_only") => ZonePads::ThruHoleOnly,
        Some(other) => {
            return Err(format!(
                "`pads = \"{other}\"`: `solid`, `thermal`, `none` or `thru_hole_only`"
            ));
        }
    };
    Ok(Edit::AddZone {
        net: z.net.clone(),
        layers: match &z.layer {
            None => vec!["top".to_string()],
            Some(Layers::One(l)) => vec![l.clone()],
            Some(Layers::Many(l)) => l.clone(),
        },
        outline,
        clearance: z.clearance,
        min_width: z.min_width,
        thermal_gap: z.thermal_gap,
        spoke_width: z.spoke_width,
        priority: z.priority,
        pads,
        patch: z.patch,
        lock: z.lock,
    })
}

/// Which entry of the document: its kind, its number among its kind's (from 1) and its net.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct Which {
    kind: &'static str,
    index: usize,
    net: String,
}

impl Which {
    fn new(kind: &'static str, n: usize, net: &str) -> Which {
        Which {
            kind,
            index: n + 1,
            net: net.to_string(),
        }
    }
}

/// One finding of the design rules check that refused a shape, in the routes file's frame.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Found {
    /// The rule, as `board drc` names it: `clearance`, `shorting_items`, `tracks_crossing`.
    pub rule: String,
    pub message: String,
    /// Where, in mm from the outline's top left corner, y down.
    pub at: [f64; 2],
    pub layer: Option<String>,
    /// The copper, pads and holes it names: `Track [GND] on F.Cu`, `Pad 1 [VIN] of C1`.
    pub shapes: Vec<String>,
}

impl Found {
    /// A finding of the check in the routes file's frame (as [`super::edit_all`] has them).
    pub fn of(f: &netsmith_geom::DrcFinding) -> Found {
        let mm = |v: f64| (v * 1e4).round() / 1e4;
        Found {
            rule: f.rule.id().to_string(),
            message: f.message.clone(),
            at: [mm(f.at.x), mm(f.at.y)],
            layer: f.layer.clone(),
            shapes: f.items.iter().map(|i| i.what.clone()).collect(),
        }
    }
}

/// What the batch did with one entry of the document.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShapeResult {
    /// `track`, `via` or `zone`.
    pub kind: &'static str,
    /// Its number among the document's entries of its kind, from 1.
    pub index: usize,
    pub net: String,
    /// `ok`, `refused`, or `not_tried` (after a refusal, with `stop_on_refusal`).
    pub result: &'static str,
    /// What was made, or why it was refused, in words.
    pub said: String,
    /// The new errors of the design rules check that refused it, each with its rule and place.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<Found>,
}

/// What a batch did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Batch {
    pub board: String,
    pub shapes: Vec<ShapeResult>,
    pub made: usize,
    pub refused: usize,
    pub not_tried: usize,
    /// Whether it stopped at a refusal (`stop_on_refusal`).
    pub stopped: bool,
    /// The design rules checks the batch made and the items they looked at (a shape's check
    /// looks at its neighbourhood, so these grow with the shapes, not with the board).
    pub checks: usize,
    pub items_checked: usize,
}

impl Batch {
    /// The text for a person: a line for each refused shape and a line of totals.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for s in &self.shapes {
            if s.result == "refused" {
                out.push_str(&format!(
                    "{} {} (net {}): {}\n",
                    s.kind, s.index, s.net, s.said
                ));
            }
        }
        out.push_str(&format!(
            "board {}: {} shape(s) made, {} refused{}; {} check(s) of {} item(s)\n",
            self.board,
            self.made,
            self.refused,
            if self.stopped {
                format!(
                    ", stopped at the first refusal ({} not tried)",
                    self.not_tried
                )
            } else {
                String::new()
            },
            self.checks,
            self.items_checked
        ));
        out
    }
}

/// The most entries checked together ([`add`]): a group the check takes is one check of its
/// shapes' neighbourhoods, and a group it refuses is halved, so a refused shape costs a check
/// for each halving, at most 4 more for a group of 16.
pub const GROUP: usize = 16;

/// Add the shapes of `doc` to `routes` for `board` (placed, with no routes on it), each checked
/// as its verb checks it, on the board with the shapes made before it; a refused shape is
/// skipped (or, with `stop_on_refusal`, the rest are not tried). `routes` has every shape made.
/// An error is a board or file that cannot be checked at all.
///
/// The entries are checked in groups of up to [`GROUP`] in a row, all of a group at once where
/// they are (one check costs about as much as one for a shape alone, as the check of a few
/// items is mostly its own setting up): a group the check finds no new error with is made
/// whole, and a group it does is halved, the first half checked and then the second on the
/// board with what the first made, until each shape refused is found alone, on the board with
/// every shape made before it. A shape is so refused exactly when it is refused alone, except
/// that the error it brings is not counted when a later shape of its group takes it away.
pub fn add(
    board: &Board,
    routes: &mut Routes,
    doc: &Document,
    stop_on_refusal: bool,
) -> Result<Batch, String> {
    let mut counts = EditCounts::default();
    let mut checking = Checking {
        counts: &mut counts,
        before: None,
        found: Vec::new(),
    };
    let entries = doc.edits(board);
    let mut shapes: Vec<ShapeResult> = entries
        .iter()
        .map(|(which, _)| ShapeResult {
            kind: which.kind,
            index: which.index,
            net: which.net.clone(),
            result: "not_tried",
            said: String::new(),
            findings: Vec::new(),
        })
        .collect();
    let mut stopped = false;
    let mut i = 0;
    while i < entries.len() && !stopped {
        if let Err(e) = &entries[i].1 {
            shapes[i].result = "refused";
            shapes[i].said = e.clone();
            stopped = stop_on_refusal;
            i += 1;
            continue;
        }
        let mut j = i;
        while j < entries.len() && j - i < GROUP && entries[j].1.is_ok() {
            j += 1;
        }
        let mut group = Group {
            board,
            routes,
            checking: &mut checking,
            entries: &entries,
            shapes: &mut shapes,
            stop_on_refusal,
            stopped: false,
        };
        group.check(i, j)?;
        stopped = group.stopped;
        i = j;
    }
    let count = |r: &str| shapes.iter().filter(|s| s.result == r).count();
    Ok(Batch {
        board: board.name.clone(),
        made: count("ok"),
        refused: count("refused"),
        not_tried: count("not_tried"),
        stopped,
        checks: counts.checks,
        items_checked: counts.items,
        shapes,
    })
}

/// The entries `[from, to)` of a batch checked together ([`add`]).
struct Group<'a, 'c> {
    board: &'a Board,
    routes: &'a mut Routes,
    checking: &'a mut Checking<'c>,
    entries: &'a [(Which, Result<Vec<Edit>, String>)],
    shapes: &'a mut [ShapeResult],
    stop_on_refusal: bool,
    stopped: bool,
}

impl Group<'_, '_> {
    fn check(&mut self, from: usize, to: usize) -> Result<(), String> {
        if self.stopped || from == to {
            return Ok(());
        }
        let edits: Vec<Edit> = self.entries[from..to]
            .iter()
            .flat_map(|(_, e)| e.as_ref().expect("a group's entries are edits").clone())
            .collect();
        match super::edit_all_with(self.board, self.routes, &edits, self.checking) {
            Ok(said) => {
                let mut said = said.into_iter();
                for k in from..to {
                    let n = self.entries[k].1.as_ref().map_or(0, Vec::len);
                    let mine: Vec<String> = said.by_ref().take(n).collect();
                    self.shapes[k].result = "ok";
                    self.shapes[k].said = super::summary(&mine);
                }
                Ok(())
            }
            Err(Refusal::Refused(e)) if to - from == 1 => {
                self.shapes[from].result = "refused";
                self.shapes[from].said = e;
                self.shapes[from].findings = self.checking.found.iter().map(Found::of).collect();
                self.stopped = self.stop_on_refusal;
                Ok(())
            }
            Err(Refusal::Refused(_)) => {
                let middle = from + (to - from) / 2;
                self.check(from, middle)?;
                self.check(middle, to)
            }
            Err(Refusal::Failed(e)) => Err(e),
        }
    }
}
