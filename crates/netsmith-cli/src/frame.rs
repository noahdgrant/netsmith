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
    /// are in. The guide's chapter on the commands lists each command's own frame. Only with
    /// `--format json`.
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
];

/// The keys whose places are in a part's own frame, which no frame of the board moves.
pub const LOCAL_KEYS: &[&str] = &["local", "local_courtyards"];

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

    /// Refuses `--frame` with text output, which says its own frame (exit 2 with the reason).
    pub fn for_json(&self, json: bool) -> Result<(), String> {
        match (self.frame, json) {
            (Some(_), false) => Err(
                "`--frame` sets the frame of the places of `--format json`; the text says the frame of its places"
                    .to_string(),
            ),
            _ => Ok(()),
        }
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
        let mut value = serde_json::to_value(doc).expect("a document is plain data");
        let to = self.of(own);
        if to != own {
            let Some((cx, cy)) = corner else {
                return Err(format!(
                    "`--frame {}`: this board's outline corner is not known, so its places are in the frame `{}` only",
                    to.name(),
                    own.name()
                ));
            };
            let sign = if to == Frame::BoardFile { 1.0 } else { -1.0 };
            shift(&mut value, sign * cx, sign * cy, false);
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
        let text = if self.of(own) == own {
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
        assert!(other.for_json(false).is_err());
        assert!(Framing::default().for_json(false).is_ok());
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
