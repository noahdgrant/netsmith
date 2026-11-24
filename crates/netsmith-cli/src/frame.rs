//! `--frame` (issue #1588): the frame of the places a command prints as JSON, and the `frame`
//! key that names it.
//!
//! A board has two frames in 2D, both in mm with y down: the outline's (from the outline's top
//! left corner, as the layout file and the routes file have their places) and the board file's
//! (KiCad's sheet, where netsmith puts the outline's corner at [`netsmith_pcb::ORIGIN`], as the
//! design rules check gives its places). Each command keeps the frame it has always printed in;
//! `--frame` asks for the other, and every document says which it is in. `dev board3d` prints
//! in the 3D scene's frame (y up, from the outline's bottom left corner) and says so in prose;
//! `--frame` puts its plan places in one of the two.
//!
//! The places are moved in the document, by key: [`PLACE_KEYS`] are the keys whose value is a
//! place or holds places, at any depth, and a box with `x_min`, `x_max`, `y_min` and `y_max` is
//! one wherever it is. What is in a part's own frame ([`LOCAL_KEYS`]) is left as it is.

use clap::{Args as ClapArgs, ValueEnum};
use serde::Serialize;
use serde_json::{Map, Value};

/// `--frame`, of every command that prints places as JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ClapArgs)]
pub struct Framing {
    /// The frame of the places in the JSON: `outline` or `board-file`; with none, the command's own.
    ///
    /// `outline` is mm from the outline's top left corner, y down, as the layout file and the
    /// routes file have their places; `board-file` is the KiCad board file's, with the outline's
    /// top left corner at (20, 20) mm. The document's `frame` key says which frame its places
    /// are in; the text says the frame of its places. The guide's chapter on the commands lists
    /// each command's own frame.
    #[arg(long, value_enum, value_name = "FRAME")]
    pub frame: Option<Frame>,
}

/// A frame of a board's places in 2D.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Frame {
    /// From the outline's top left corner, y down: the layout file's and the routes file's.
    Outline,
    /// The KiCad board file's: the outline's top left corner at (20, 20) mm, y down.
    BoardFile,
}

impl Frame {
    /// Its name, as `--frame` and the `frame` key say it.
    pub fn name(self) -> &'static str {
        match self {
            Frame::Outline => "outline",
            Frame::BoardFile => "board-file",
        }
    }

    /// What a text says of its places in this frame, after `mm`.
    pub fn says(self) -> String {
        match self {
            Frame::Outline => "mm from the outline's top left corner, y down".to_string(),
            Frame::BoardFile => {
                let (x, y) = design_corner();
                format!(
                    "mm in the board file's frame, y down, the outline's top left corner at ({x}, {y})"
                )
            }
        }
    }

    /// Its name as prose says it: `outline`, `board file`.
    pub fn words(self) -> &'static str {
        match self {
            Frame::Outline => "outline",
            Frame::BoardFile => "board file",
        }
    }
}

/// The keys whose value is a place (`[x, y]`, `[x, y, z]`, `{"x", "y"}`) or holds places (a box
/// `{min, max}`, a ring, a list of them).
pub const PLACE_KEYS: &[&str] = &[
    "at",
    "from",
    "to",
    "mid",
    "via",
    "near",
    "place",
    "outline_at",
    "outline",
    "outlines",
    "holes",
    "outer",
    "polygons",
    "min",
    "max",
    "corners",
    "bounds",
    "body",
    "courtyard",
    "courtyards",
    "largest_free",
    "origin",
    "body_centre",
    "region",
];

/// The keys whose value holds lists of objects with an `x` and a `y` of their own, which are
/// places: the pairs of courtyards that overlap and the parts' places of a refusal because the
/// parts do not fit (`does_not_fit`), listed by the key of the list.
pub const PLACE_LISTS: &[(&str, &[&str])] = &[("does_not_fit", &["overlapping", "parts"])];

/// The keys whose value, a string or a list of them, is a sentence of the placer or a line of the
/// design rules check, which say a place inside the sentence: the `message` and `text` of the
/// findings, the `said` and `why` of the changes of the routes commands, their `refused`, the
/// placer's `placement_refused`, and the `appeared` and `gone` of `layout compare` and
/// `regenerate`. [`Framing::document`] moves the places inside them (`netsmith_pcb::prose`).
pub const PROSE_KEYS: &[&str] = &[
    "message",
    "text",
    "said",
    "why",
    "refused",
    "placement_refused",
    "appeared",
    "gone",
];

/// The keys whose places are in a part's own frame, which no frame of the board moves.
///
/// A designator's `at` is the offset of its text from the part's origin: it moves with the part.
pub const LOCAL_KEYS: &[&str] = &["local", "local_courtyards", "designator"];

/// The outline's top left corner in the board file's frame, for a design's board.
pub fn design_corner() -> (f64, f64) {
    (netsmith_pcb::ORIGIN.x, netsmith_pcb::ORIGIN.y)
}

/// Says `e` and ends with 2: `--frame` could not be had.
pub fn refused(e: &str) -> std::process::ExitCode {
    eprintln!("error: {e}");
    std::process::ExitCode::from(2)
}

impl Framing {
    /// The frame `doc` is printed in: the asked one, or `own`.
    pub fn of(&self, own: Frame) -> Frame {
        self.frame.unwrap_or(own)
    }

    /// The frame the places of a text are in and how far to move them from `own`, `(dx, dy)` in
    /// mm: the asked frame, or `own` (no move). `corner` is the outline's top left corner in the
    /// board file's frame (`None` where it is not known, a KiCad board's: then only `own` may be
    /// asked for).
    pub fn moved(
        &self,
        own: Frame,
        corner: Option<(f64, f64)>,
    ) -> Result<(Frame, f64, f64), String> {
        let to = self.of(own);
        if to == own {
            return Ok((to, 0.0, 0.0));
        }
        let Some((cx, cy)) = corner else {
            return Err(format!(
                "`--frame {}`: this board's outline corner is not known, so its places are in the frame `{}` only",
                to.name(),
                own.name()
            ));
        };
        let sign = if to == Frame::BoardFile { 1.0 } else { -1.0 };
        Ok((to, sign * cx, sign * cy))
    }

    /// How far (`[dx, dy]`, mm) and in which words the places a text says in the outline's
    /// frame move to the frame asked for: nothing without `--frame`, and `says` is what the text
    /// says of the frame after `mm`.
    pub fn text_move(&self) -> ([f64; 2], String) {
        let (cx, cy) = design_corner();
        match self.frame {
            Some(Frame::BoardFile) => ([cx, cy], Frame::BoardFile.says()),
            Some(Frame::Outline) => ([0.0, 0.0], Frame::Outline.says()),
            None => ([0.0, 0.0], Frame::Outline.says()),
        }
    }

    /// The same for the lines of the design rules check, which say their places in the board
    /// file's frame: nothing without `--frame` or with `--frame board-file`.
    pub fn board_file_move(&self) -> [f64; 2] {
        let (cx, cy) = design_corner();
        match self.frame {
            Some(Frame::Outline) => [-cx, -cy],
            _ => [0.0, 0.0],
        }
    }

    /// `text`, a placer's sentences (a courtyard "from the board's top left corner") with their
    /// places in the frame asked for.
    pub fn placer(&self, text: &str) -> String {
        let (by, says) = self.text_move();
        netsmith_pcb::prose::placer_moved(text, by, &says)
    }

    /// `text`, lines of the design rules check, with their places in the frame asked for.
    pub fn drc(&self, text: &str) -> String {
        netsmith_pcb::prose::drc_moved(text, self.board_file_move())
    }

    /// A sentence of the placer or a line of the design rules check, its places in the frame
    /// asked for. The placer's are in the outline's frame; the check's are in `drc_in`'s.
    fn sentence(&self, text: &str, drc_in: Frame) -> String {
        let placed = self.placer(text);
        match drc_in {
            Frame::Outline => self.routes_drc(&placed),
            Frame::BoardFile => self.drc(&placed),
        }
    }

    /// `value`, a document in the outline's frame, with the places of its sentences
    /// ([`PROSE_KEYS`], and the `vias` of the `walls` of `routes route`) in the frame asked for.
    /// The lines of the design rules check in them are in `drc_in`'s frame: the routes
    /// commands' in the outline's, `fab`'s in the board file's.
    pub fn prose(&self, value: &mut Value, drc_in: Frame) {
        if self.frame.is_none() {
            return;
        }
        let (by, _) = self.text_move();
        match value {
            Value::Array(items) => {
                for item in items {
                    self.prose(item, drc_in);
                }
            }
            Value::Object(map) => {
                for (key, child) in map.iter_mut() {
                    if PROSE_KEYS.contains(&key.as_str()) {
                        match child {
                            Value::String(text) => *text = self.sentence(text, drc_in),
                            Value::Array(items) => {
                                for item in items {
                                    if let Value::String(text) = item {
                                        *text = self.sentence(text, drc_in);
                                    }
                                }
                            }
                            _ => {}
                        }
                    } else if key == "walls" {
                        for wall in each(child) {
                            for via in each(&mut wall["vias"]) {
                                if let Value::String(text) = via {
                                    *text = netsmith_pcb::prose::via_moved(text, by);
                                }
                            }
                        }
                    }
                    self.prose(child, drc_in);
                }
            }
            _ => {}
        }
    }

    /// `text`, lines of the design rules check that a routes command made (the check of an edit
    /// of the routes file, `netsmith_fabricate::routes::drc_errors`), with their places in the
    /// frame asked for: those lines say their places in the routes file's frame, the outline's.
    pub fn routes_drc(&self, text: &str) -> String {
        netsmith_pcb::prose::drc_moved(text, self.text_move().0)
    }

    /// `doc` with a `frame` key, its places in the asked frame. Its places are in `own`, with
    /// the outline's top left corner at `corner` in the board file's frame (`None` where it is
    /// not known, a KiCad board's: then only `own` may be asked for).
    pub fn document<T: Serialize>(
        &self,
        doc: &T,
        own: Frame,
        corner: Option<(f64, f64)>,
    ) -> Result<Value, String> {
        self.document_with(doc, own, corner, Frame::Outline)
    }

    /// [`Framing::document`] where the lines of the design rules check in its sentences are in
    /// `drc_in`'s frame.
    pub fn document_with<T: Serialize>(
        &self,
        doc: &T,
        own: Frame,
        corner: Option<(f64, f64)>,
        drc_in: Frame,
    ) -> Result<Value, String> {
        let mut value = serde_json::to_value(doc).expect("a document is plain data");
        let (to, dx, dy) = self.moved(own, corner)?;
        if to != own {
            shift(&mut value, dx, dy, false);
        }
        if own == Frame::Outline {
            self.prose(&mut value, drc_in);
        }
        if let Value::Object(map) = &mut value {
            map.insert("frame".into(), Value::String(to.name().into()));
        }
        Ok(value)
    }

    /// Prints `doc` as [`Framing::document`] has it, pretty or on one line, with its keys in
    /// their order when it is in its own frame. Exit 2 with the reason when it cannot be.
    pub fn print<T: Serialize>(
        &self,
        doc: &T,
        own: Frame,
        corner: Option<(f64, f64)>,
        pretty: bool,
    ) -> Result<(), String> {
        // A document in the outline's frame with `--frame` goes through its value, which moves
        // the places its sentences say (the design rules check's are in the board file's frame,
        // so `--frame outline` moves them too).
        let by_value = self.frame.is_some() && own == Frame::Outline;
        let text = if self.of(own) == own && !by_value {
            #[derive(Serialize)]
            struct Framed<'a, T> {
                #[serde(flatten)]
                doc: &'a T,
                frame: &'static str,
            }
            let framed = Framed {
                doc,
                frame: own.name(),
            };
            if pretty {
                serde_json::to_string_pretty(&framed)
            } else {
                serde_json::to_string(&framed)
            }
        } else {
            let value = self.document(doc, own, corner)?;
            if pretty {
                serde_json::to_string_pretty(&value)
            } else {
                serde_json::to_string(&value)
            }
        };
        println!("{}", text.expect("a document is plain data"));
        Ok(())
    }

    /// The report of `board check`, `build` and `fab` as `--format json` prints it: its places
    /// are in the outline's frame, and in the asked one with `--frame`.
    pub fn report_json(&self, report: &netsmith_tools::findings::Report) -> Result<String, String> {
        if self.frame.is_none() {
            return Ok(report.to_json());
        }
        if self.of(Frame::Outline) == Frame::Outline {
            // The document as it is (its keys in their order), the board file's lines of the
            // design rules check in its messages moved to the outline's frame.
            let mut report = report.clone();
            for finding in &mut report.findings {
                finding.message = self.drc(&self.placer(&finding.message));
            }
            return Ok(report.to_json());
        }
        // The sentences in the messages say their places in the document's frame.
        let value = self.document_with(
            report,
            Frame::Outline,
            Some(design_corner()),
            Frame::BoardFile,
        )?;
        let mut text = serde_json::to_string_pretty(&value).expect("a document is plain data");
        text.push('\n');
        Ok(text)
    }

    /// `dev board3d`'s report put in the asked frame: its plan places (in the scene's frame, y
    /// up from the outline's bottom left corner) in the outline's or the board file's, y down,
    /// and its `frame` the frame's name. With no `--frame` it is as it was, its `frame` the
    /// scene's in prose. The case models' `rotation` and `offset` (a motion) and every z stay
    /// the scene's.
    pub fn scene_report(&self, mut report: Value, corner: (f64, f64)) -> Value {
        let Some(to) = self.frame else {
            return report;
        };
        let height = report["board"]["outline"]["height"]
            .as_f64()
            .unwrap_or_default();
        let flip = |v: &mut Value| flip_y(v, height);
        for (list, keys) in [
            ("cutouts", &["min", "max"][..]),
            ("holes", &["from", "to"][..]),
        ] {
            for item in each(&mut report["board"][list]) {
                for key in keys {
                    flip(&mut item[*key]);
                }
                swap_box(item);
            }
        }
        for part in each(&mut report["parts"]) {
            for key in ["min", "max", "corners"] {
                flip(&mut part["body"][key]);
            }
            swap_box(&mut part["body"]);
            for c in each(&mut part["courtyards"]) {
                flip(&mut c["min"]);
                flip(&mut c["max"]);
                swap_box(c);
            }
            if part["pins"].is_object() {
                flip(&mut part["pins"]["min"]);
                flip(&mut part["pins"]["max"]);
                swap_box(&mut part["pins"]);
            }
        }
        for limit in each(&mut report["height_limits"]) {
            flip(&mut limit["min"]);
            flip(&mut limit["max"]);
            swap_box(limit);
        }
        for f in each(&mut report["case_findings"]) {
            flip(&mut f["at"]);
        }
        for model in each(&mut report["case_models"]) {
            for s in each(&mut model["solids"]) {
                flip(&mut s["min"]);
                flip(&mut s["max"]);
                swap_box(s);
            }
        }
        // Now in the outline's frame, as the layout places and the placement's boxes are.
        if to == Frame::BoardFile {
            for part in each(&mut report["parts"]) {
                if let Some(layout) = part.get_mut("layout") {
                    move_xy(layout, corner.0, corner.1);
                }
            }
            shift(&mut report, corner.0, corner.1, false);
        }
        // The placement check's sentences say their places in the frame asked for too.
        self.prose(&mut report, Frame::Outline);
        report["frame"] = Value::String(to.name().into());
        report
    }
}

/// The items of a list (none for anything else).
fn each(v: &mut Value) -> impl Iterator<Item = &mut Value> {
    v.as_array_mut().into_iter().flatten()
}

/// Moves every place of `v` by `(dx, dy)`: the values of [`PLACE_KEYS`] (`in_place` once under
/// one) and every box of `x_min`..`y_max`, but nothing under [`LOCAL_KEYS`].
fn shift(v: &mut Value, dx: f64, dy: f64, in_place: bool) {
    match v {
        Value::Array(items) => {
            if in_place && is_point(items) {
                add(&mut items[0], dx);
                add(&mut items[1], dy);
                return;
            }
            for item in items {
                shift(item, dx, dy, in_place);
            }
        }
        Value::Object(map) => {
            move_span(map, dx, dy);
            if in_place {
                move_xy_map(map, dx, dy);
            }
            for (key, child) in map.iter_mut() {
                if LOCAL_KEYS.contains(&key.as_str()) {
                    continue;
                }
                if let Some((_, lists)) = PLACE_LISTS.iter().find(|(parent, _)| parent == key) {
                    for list in *lists {
                        if let Some(items) = child.get_mut(*list).and_then(Value::as_array_mut) {
                            for item in items {
                                move_xy(item, dx, dy);
                            }
                        }
                    }
                }
                shift(
                    child,
                    dx,
                    dy,
                    in_place || PLACE_KEYS.contains(&key.as_str()),
                );
            }
        }
        _ => {}
    }
}

fn is_point(items: &[Value]) -> bool {
    (items.len() == 2 || items.len() == 3) && items.iter().all(Value::is_number)
}

fn add(v: &mut Value, d: f64) {
    if let Some(n) = v.as_f64() {
        *v = Value::from(round(n + d));
    }
}

/// To a micrometre, so a place moved and moved back is the place.
fn round(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    if r == 0.0 { 0.0 } else { r }
}

fn move_xy(v: &mut Value, dx: f64, dy: f64) {
    if let Value::Object(map) = v {
        move_xy_map(map, dx, dy);
    }
}

fn move_xy_map(map: &mut Map<String, Value>, dx: f64, dy: f64) {
    if map.get("x").is_some_and(Value::is_number) && map.get("y").is_some_and(Value::is_number) {
        add(map.get_mut("x").expect("x"), dx);
        add(map.get_mut("y").expect("y"), dy);
    }
}

fn move_span(map: &mut Map<String, Value>, dx: f64, dy: f64) {
    let keys = ["x_min", "x_max", "y_min", "y_max"];
    if keys
        .iter()
        .all(|k| map.get(*k).is_some_and(Value::is_number))
    {
        for (k, d) in keys.iter().zip([dx, dx, dy, dy]) {
            add(map.get_mut(*k).expect("a span key"), d);
        }
    }
}

/// A place of the scene `[x, y(, z)]`, or a list of them, with y measured down from the top
/// edge instead of up from the bottom one.
fn flip_y(v: &mut Value, height: f64) {
    if let Value::Array(items) = v {
        if is_point(items) {
            if let Some(y) = items[1].as_f64() {
                items[1] = Value::from(round(height - y));
            }
        } else {
            for item in items {
                flip_y(item, height);
            }
        }
    }
}

/// A box `{min, max}` whose y were flipped: its least y is the flipped largest.
fn swap_box(b: &mut Value) {
    let (Some(lo), Some(hi)) = (b["min"][1].as_f64(), b["max"][1].as_f64()) else {
        return;
    };
    if lo > hi {
        b["min"][1] = Value::from(hi);
        b["max"][1] = Value::from(lo);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_place_moves_by_the_corner_and_a_local_one_does_not() {
        let framing = Framing {
            frame: Some(Frame::BoardFile),
        };
        let doc = json!({
            "board": "B",
            "width": 10.0,
            "parts": [{
                "frame": {"at": [1.0, 2.0], "turn": 90.0},
                "courtyard": {"min": [0.5, 1.5], "max": [1.5, 2.5]},
                "local_courtyards": [{"min": [-0.5, -0.5], "max": [0.5, 0.5]}],
                "pads": [{"at": [1.0, 2.5], "local": [0.0, 0.5], "width": 0.4}],
                "shared": {"x_min": 0.0, "x_max": 1.0, "y_min": 0.0, "y_max": 1.0},
            }],
            "outline_at": {"x": 0.0, "y": 0.0},
            "gaps": [0.1, 0.2],
        });
        let moved = framing
            .document(&doc, Frame::Outline, Some((20.0, 20.0)))
            .unwrap();
        let part = &moved["parts"][0];
        assert_eq!(part["frame"]["at"], json!([21.0, 22.0]));
        assert_eq!(part["frame"]["turn"], 90.0);
        assert_eq!(part["courtyard"]["max"], json!([21.5, 22.5]));
        assert_eq!(part["local_courtyards"][0]["min"], json!([-0.5, -0.5]));
        assert_eq!(part["pads"][0]["at"], json!([21.0, 22.5]));
        assert_eq!(part["pads"][0]["local"], json!([0.0, 0.5]));
        assert_eq!(part["pads"][0]["width"], 0.4);
        assert_eq!(part["shared"]["y_max"], 21.0);
        assert_eq!(moved["outline_at"], json!({"x": 20.0, "y": 20.0}));
        assert_eq!(moved["gaps"], json!([0.1, 0.2]));
        assert_eq!(moved["width"], 10.0);
        assert_eq!(moved["frame"], "board-file");
        // And back.
        let back = Framing {
            frame: Some(Frame::Outline),
        }
        .document(&moved, Frame::BoardFile, Some((20.0, 20.0)))
        .unwrap();
        assert_eq!(back["parts"], doc["parts"]);
        assert_eq!(back["frame"], "outline");
    }

    #[test]
    fn the_places_of_a_refusal_origins_and_a_designator_are_moved_as_they_are() {
        let framing = Framing {
            frame: Some(Frame::BoardFile),
        };
        let doc = json!({
            "findings": [{
                "does_not_fit": {
                    "outline": {"width": 10.0, "height": 8.0},
                    "overlapping": [{"a": {"refdes": "R1"}, "width": 1.0, "height": 1.0, "x": 2.0, "y": 3.0}],
                    "outside": [{"refdes": "R2", "worst": 0.5}],
                    "parts": [{"refdes": "R1", "x": 4.0, "y": 5.0, "rotation": 90.0,
                               "courtyard": {"width": 1.0, "height": 2.0}}],
                },
            }],
            "boards": [{"parts": [{
                "origin": [1.0, 2.0], "body_centre": [1.5, 2.5], "offset": [-0.5, -0.5],
            }]}],
            "entry": {"at": [1.0, 2.0], "designator": {"at": [0.0, -1.0], "turn": 90.0}},
        });
        let moved = framing
            .document(&doc, Frame::Outline, Some((20.0, 20.0)))
            .unwrap();
        let fit = &moved["findings"][0]["does_not_fit"];
        assert_eq!(fit["overlapping"][0]["x"], 22.0);
        assert_eq!(fit["overlapping"][0]["y"], 23.0);
        assert_eq!(fit["overlapping"][0]["width"], 1.0);
        assert_eq!(fit["parts"][0]["x"], 24.0);
        assert_eq!(fit["parts"][0]["y"], 25.0);
        assert_eq!(fit["parts"][0]["rotation"], 90.0);
        assert_eq!(fit["outline"]["width"], 10.0);
        let part = &moved["boards"][0]["parts"][0];
        assert_eq!(part["origin"], json!([21.0, 22.0]));
        assert_eq!(part["body_centre"], json!([21.5, 22.5]));
        assert_eq!(part["offset"], json!([-0.5, -0.5]));
        assert_eq!(moved["entry"]["at"], json!([21.0, 22.0]));
        assert_eq!(moved["entry"]["designator"]["at"], json!([0.0, -1.0]));
    }

    #[test]
    fn the_places_inside_the_sentences_of_a_document_are_moved_by_the_frame_asked_for() {
        let placer =
            "R1 stands 2.00 mm tall, around (10.5, 12.0) mm from the board's top left corner";
        let drc = "error [clearance] a gap at (25.000, 30.500) on F.Cu: track";
        let doc = json!({
            "findings": [{"message": placer, "at": [10.5, 12.0]}],
            "shapes": [{"said": drc, "why": [placer], "appeared": [placer], "refused": 1}],
            "walls": [{"vias": ["net GND at (5, 0.4)"], "nets": ["at (1, 2)"]}],
        });
        let file = Framing {
            frame: Some(Frame::BoardFile),
        };
        let moved = file
            .document(&doc, Frame::Outline, Some((20.0, 20.0)))
            .unwrap();
        assert!(
            moved["findings"][0]["message"]
                .as_str()
                .unwrap()
                .contains("around (30.5, 32.0) mm in the board file's frame"),
            "{moved:#}"
        );
        assert_eq!(moved["findings"][0]["at"], json!([30.5, 32.0]));
        // A routes command's check lines are in the outline's frame, `fab`'s in the board file's.
        assert_eq!(
            moved["shapes"][0]["said"],
            "error [clearance] a gap at (45.000, 50.500) on F.Cu: track"
        );
        assert!(
            moved["shapes"][0]["why"][0]
                .as_str()
                .unwrap()
                .contains("(30.5, 32.0)")
        );
        assert!(
            moved["shapes"][0]["appeared"][0]
                .as_str()
                .unwrap()
                .contains("(30.5, 32.0)")
        );
        assert_eq!(moved["shapes"][0]["refused"], 1);
        assert_eq!(moved["walls"][0]["vias"][0], "net GND at (25, 20.4)");
        assert_eq!(moved["walls"][0]["nets"][0], "at (1, 2)");
        let by_fab = file
            .document_with(&doc, Frame::Outline, Some((20.0, 20.0)), Frame::BoardFile)
            .unwrap();
        assert_eq!(by_fab["shapes"][0]["said"], drc);
        // `--frame outline` moves the lines of the check that are in the board file's frame.
        let outline = Framing {
            frame: Some(Frame::Outline),
        };
        let own = outline
            .document_with(&doc, Frame::Outline, Some((20.0, 20.0)), Frame::BoardFile)
            .unwrap();
        assert_eq!(
            own["shapes"][0]["said"],
            "error [clearance] a gap at (5.000, 10.500) on F.Cu: track"
        );
        assert_eq!(own["walls"][0]["vias"][0], "net GND at (5, 0.4)");
        // With no flag nothing moves.
        let none = Framing::default()
            .document(&doc, Frame::Outline, Some((20.0, 20.0)))
            .unwrap();
        assert_eq!(none["shapes"][0]["said"], drc);
        assert_eq!(none["findings"][0]["message"], placer);
    }

    #[test]
    fn the_own_frame_is_named_and_an_unknown_corner_refuses_the_other() {
        let doc = json!({"at": [1.0, 2.0]});
        let own = Framing::default()
            .document(&doc, Frame::BoardFile, None)
            .unwrap();
        assert_eq!(own, json!({"at": [1.0, 2.0], "frame": "board-file"}));
        let other = Framing {
            frame: Some(Frame::Outline),
        };
        assert!(other.document(&doc, Frame::BoardFile, None).is_err());
        assert!(other.moved(Frame::BoardFile, None).is_err());
        assert_eq!(
            other.moved(Frame::BoardFile, Some((20.0, 20.0))).unwrap(),
            (Frame::Outline, -20.0, -20.0)
        );
        assert_eq!(
            Framing::default().moved(Frame::BoardFile, None).unwrap(),
            (Frame::BoardFile, 0.0, 0.0)
        );
    }

    #[test]
    fn the_scene_turns_to_y_down() {
        let report = json!({
            "frame": "millimetres, right-handed, z up; ...",
            "board": {"outline": {"width": 10.0, "height": 8.0}, "cutouts": [],
                      "holes": [{"from": [2.0, 5.0], "to": [2.0, 5.0]}]},
            "parts": [{"layout": {"x": 2.0, "y": 3.0, "rotation": 0.0},
                       "body": {"min": [1.0, 4.0], "max": [3.0, 6.0], "corners": [[1.0, 4.0]]},
                       "courtyards": [], "pins": null}],
            "placement": [{"parts": [{"courtyard": {"x_min": 1.0, "x_max": 3.0, "y_min": 2.0, "y_max": 4.0}}]}],
        });
        assert_eq!(
            Framing::default().scene_report(report.clone(), (20.0, 20.0)),
            report
        );
        let outline = Framing {
            frame: Some(Frame::Outline),
        }
        .scene_report(report.clone(), (20.0, 20.0));
        assert_eq!(outline["frame"], "outline");
        assert_eq!(outline["board"]["holes"][0]["from"], json!([2.0, 3.0]));
        assert_eq!(outline["parts"][0]["body"]["min"], json!([1.0, 2.0]));
        assert_eq!(outline["parts"][0]["body"]["max"], json!([3.0, 4.0]));
        assert_eq!(outline["parts"][0]["layout"]["y"], 3.0);
        let file = Framing {
            frame: Some(Frame::BoardFile),
        }
        .scene_report(report, (20.0, 20.0));
        assert_eq!(file["board"]["holes"][0]["from"], json!([22.0, 23.0]));
        assert_eq!(file["parts"][0]["layout"]["x"], 22.0);
        assert_eq!(file["placement"][0]["parts"][0]["courtyard"]["y_min"], 22.0);
    }
}
