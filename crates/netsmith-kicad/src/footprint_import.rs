//! A footprint taken from a KiCad board or footprint file into a project's library (issue #1424,
//! `netsmith import footprint`).
//!
//! A vendor's reference board (`.kicad_pcb`) holds the land pattern the vendor laid out; a
//! distributor's KiCad export holds a `.kicad_mod`. Either is read with the S-expression reader
//! and written back as a `.kicad_mod` of its own: there is no new reader of footprints, and the
//! file keeps everything it draws (pads, courtyard, silkscreen, fabrication layer, 3D model).
//!
//! What a board adds to a footprint is taken away, so that the file is the footprint as a library
//! holds it, seen from the top with its origin and its angle:
//!
//! - the footprint's place on the board: its `(at x y angle)`. The drawings of a footprint on a
//!   board are already in its own frame, but the angle of each pad and text is the board's (the
//!   footprint's angle added), so the footprint's angle is taken off each.
//! - the bottom face: a footprint on `B.Cu` is stored mirrored (every y of its frame negated,
//!   each layer on the other face, a pad's own angle negated, its texts mirrored). Each is undone,
//!   so the file is drawn from the top as every footprint of a library is (_A footprint on the
//!   bottom face_ in the guide).
//! - what belongs to the board and not the footprint: the nets of the pads and the pins of the
//!   schematic they name, the links to the schematic (`path`, `sheetname`, `sheetfile`), the
//!   locks, the ids (`uuid`, `tstamp`), the symbol's fields other than the reference, the value,
//!   the datasheet and the description, and the `Footprint` field, which names the old id.
//!
//! The reference becomes `REF**` and the value the footprint's new name, as KiCad's libraries
//! have them.

use crate::sexp::Sexp;

/// Where the footprint comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FootprintSource<'a> {
    /// A `.kicad_mod` file: the one footprint it holds.
    File,
    /// A `.kicad_pcb` board: the footprint of the part with this reference designator.
    Board { reference: &'a str },
}

/// A footprint taken out of a file, ready to be written as a `.kicad_mod`.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedFootprint {
    /// The name the source gives the footprint, without its library (`SOIC-8_3.9x4.9mm`).
    pub source_name: String,
    /// The name it is written under: the name asked for, or [`Self::source_name`].
    pub name: String,
    /// Whether the board had it on the bottom face (and it was turned back to the top).
    pub from_bottom: bool,
    /// The footprint's angle on the board, in degrees, taken off its pads and texts.
    pub board_angle: f64,
    /// The `.kicad_mod` text.
    pub text: String,
    /// What the import changed or left out that the person should check, one sentence each
    /// (an Eagle package's drawings on a layer KiCad has none for); none from a KiCad file.
    pub notes: Vec<String>,
}

/// The fields of a footprint that a board adds and a library footprint does not have.
const BOARD_ONLY: &[&str] = &[
    "uuid",
    "tstamp",
    "path",
    "sheetname",
    "sheetfile",
    "units",
    "locked",
    "placed",
    "component_classes",
    "tedit",
    "version",
    "generator",
    "generator_version",
];

/// The fields of a pad that a board adds.
const PAD_BOARD_ONLY: &[&str] = &["net", "pinfunction", "pintype", "uuid", "tstamp"];

/// The symbol fields a library footprint keeps; the others are the board's.
const KEPT_PROPERTIES: &[&str] = &["Reference", "Value", "Datasheet", "Description"];

/// The footprint of `text` (a board or a footprint file, as `source` says), named `name` (or the
/// source's own name when `None`), as a `.kicad_mod`. The error says what the file lacks: no
/// footprint, no part of that reference (with the references the board has), two parts of it.
pub fn import_footprint(
    text: &str,
    source: FootprintSource,
    name: Option<&str>,
) -> Result<ImportedFootprint, String> {
    let root = Sexp::parse(text).map_err(|e| format!("not an S-expression file: {e}"))?;
    let (footprint, version) = match source {
        FootprintSource::File => {
            if !matches!(root.head(), Some("footprint" | "module")) {
                return Err(format!(
                    "not a KiCad footprint file: it starts with `({}`, not `(footprint`",
                    root.head().unwrap_or("")
                ));
            }
            (&root, root.find("version").cloned())
        }
        FootprintSource::Board { reference } => {
            if root.head() != Some("kicad_pcb") {
                return Err(format!(
                    "not a KiCad board file: it starts with `({}`, not `(kicad_pcb`",
                    root.head().unwrap_or("")
                ));
            }
            (
                board_footprint(&root, reference)?,
                root.find("version").cloned(),
            )
        }
    };
    let id = footprint
        .args()
        .first()
        .and_then(Sexp::text)
        .unwrap_or_default();
    let source_name = id.rsplit(':').next().unwrap_or(id).to_string();
    let name = name.map_or_else(|| source_name.clone(), str::to_string);
    if name.is_empty() {
        return Err("the footprint has no name: give one with `--name`".into());
    }
    let (board_angle, from_bottom) = match source {
        FootprintSource::File => (0.0, false),
        FootprintSource::Board { .. } => {
            let angle = footprint
                .find("at")
                .and_then(|at| at.args().get(2))
                .and_then(number)
                .unwrap_or(0.0);
            let bottom = footprint
                .find("layer")
                .and_then(|l| l.args().first())
                .and_then(Sexp::text)
                == Some("B.Cu");
            (angle, bottom)
        }
    };
    let turn = Turn {
        angle: board_angle,
        mirror: from_bottom,
    };
    let mut items = vec![Sexp::atom("footprint"), Sexp::string(name.clone())];
    if let Some(v) = version {
        items.push(v);
    }
    items.push(Sexp::node("generator", vec![Sexp::string("netsmith")]));
    for child in footprint.args().iter().skip(1) {
        let Some(head) = child.head() else {
            continue;
        };
        if BOARD_ONLY.contains(&head) || head == "at" {
            continue;
        }
        let child = match head {
            "property" => {
                let field = child.args().first().and_then(Sexp::text).unwrap_or("");
                if !KEPT_PROPERTIES.contains(&field) {
                    continue;
                }
                with_value(child, field, &name)
            }
            "fp_text" => {
                let field = child.args().first().and_then(Sexp::text).unwrap_or("");
                with_value(child, field, &name)
            }
            _ => child.clone(),
        };
        items.push(turn.apply(&child, head));
    }
    let out = Sexp::List(items);
    Ok(ImportedFootprint {
        source_name,
        name,
        from_bottom,
        board_angle,
        text: out.to_kicad_string(),
        notes: Vec::new(),
    })
}

/// The footprint of the board whose reference is `reference`.
fn board_footprint<'a>(board: &'a Sexp, reference: &str) -> Result<&'a Sexp, String> {
    let footprints: Vec<&Sexp> = board
        .args()
        .iter()
        .filter(|c| matches!(c.head(), Some("footprint" | "module")))
        .collect();
    let found: Vec<&&Sexp> = footprints
        .iter()
        .filter(|f| reference_of(f) == Some(reference))
        .collect();
    match found.as_slice() {
        [one] => Ok(one),
        [] => {
            let mut refs: Vec<&str> = footprints.iter().filter_map(|f| reference_of(f)).collect();
            refs.sort_by_key(|r| natural(r));
            refs.dedup();
            if refs.is_empty() {
                return Err("the board has no footprints".into());
            }
            let shown = if refs.len() > 40 {
                format!("{}, ... ({} in all)", refs[..40].join(", "), refs.len())
            } else {
                refs.join(", ")
            };
            Err(format!(
                "the board has no part `{reference}`; its parts are {shown}"
            ))
        }
        many => Err(format!(
            "the board has {} parts `{reference}`; a reference must name one",
            many.len()
        )),
    }
}

/// The reference designator of a board footprint: its `Reference` field (KiCad 8 and later) or
/// its `fp_text reference` (before).
fn reference_of(footprint: &Sexp) -> Option<&str> {
    footprint
        .args()
        .iter()
        .find(|c| {
            matches!(c.head(), Some("property" | "fp_text"))
                && c.args().first().and_then(Sexp::text).map(str::to_lowercase)
                    == Some("reference".into())
        })
        .and_then(|c| c.args().get(1))
        .and_then(Sexp::text)
}

/// `R10` after `R9`: the letters, then the number.
pub(crate) fn natural(reference: &str) -> (String, u64, String) {
    let letters: String = reference
        .chars()
        .take_while(|c| !c.is_ascii_digit())
        .collect();
    let rest = &reference[letters.len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let number = digits.parse().unwrap_or(0);
    (letters, number, rest[digits.len()..].to_string())
}

/// The field `(property "Reference" "U3" ...)` or `(fp_text reference "U3" ...)` with the value
/// a library gives it: `REF**` for the reference, the footprint's name for the value.
fn with_value(field: &Sexp, which: &str, name: &str) -> Sexp {
    let value = match which.to_lowercase().as_str() {
        "reference" => "REF**",
        "value" => name,
        _ => return field.clone(),
    };
    let mut items = field.items().to_vec();
    if items.len() > 2 {
        items[2] = Sexp::string(value);
    }
    Sexp::List(items)
}

fn number(s: &Sexp) -> Option<f64> {
    s.text()?.parse().ok()
}

/// What turns a board footprint back into its library form: the board's angle taken off the
/// pads and texts, and, for one on the bottom, the mirror undone.
struct Turn {
    angle: f64,
    mirror: bool,
}

impl Turn {
    /// `node`, a child of the footprint (or of one of its children) headed `head`.
    fn apply(&self, node: &Sexp, head: &str) -> Sexp {
        let Sexp::List(items) = node else {
            return node.clone();
        };
        match head {
            // A model's offset and rotation are the model's own, in its frame, and a board
            // leaves them as the library has them.
            "model" => return node.clone(),
            "layer" | "layers" if self.mirror => {
                return Sexp::List(items.iter().map(other_face).collect());
            }
            "start" | "end" | "center" | "mid" | "xy" | "offset" | "rect_delta" if self.mirror => {
                return Sexp::List(negate_y(items));
            }
            // An arc of the old form, a start, an end and a sweep: mirrored, it sweeps the
            // other way.
            "angle" if self.mirror => {
                return Sexp::List(
                    items
                        .iter()
                        .enumerate()
                        .map(|(i, s)| match (i, number(s)) {
                            (1, Some(a)) => Sexp::num(-a),
                            _ => s.clone(),
                        })
                        .collect(),
                );
            }
            _ => {}
        }
        let turned = matches!(head, "pad" | "property" | "fp_text" | "fp_text_box");
        let out = items
            .iter()
            .filter_map(|child| match child.head() {
                Some("at") if turned => Some(self.at(child)),
                Some("at") if self.mirror => Some(Sexp::List(negate_y(child.items()))),
                Some(h) if head == "pad" && PAD_BOARD_ONLY.contains(&h) => None,
                Some("uuid" | "tstamp") => None,
                // A text on the bottom is mirrored, and on the top it is not.
                Some("justify") if self.mirror => {
                    let kept: Vec<Sexp> = child
                        .items()
                        .iter()
                        .filter(|i| i.text() != Some("mirror"))
                        .cloned()
                        .collect();
                    (kept.len() > 1).then_some(Sexp::List(kept))
                }
                Some(h) => Some(self.apply(child, h)),
                None => Some(child.clone()),
            })
            .collect();
        Sexp::List(out)
    }

    /// The `(at x y angle)` of a pad or a text: the angle less the board's, negated on the
    /// bottom (with the y), written as KiCad writes it (none when it is 0, from 0 to 360).
    fn at(&self, at: &Sexp) -> Sexp {
        let items = at.items();
        let x = items.get(1).cloned().unwrap_or(Sexp::num(0.0));
        let y = items.get(2).and_then(number).unwrap_or(0.0);
        let mut angle = items.get(3).and_then(number).unwrap_or(0.0) - self.angle;
        let y = if self.mirror { -y } else { y };
        if self.mirror {
            angle = -angle;
        }
        let angle = angle.rem_euclid(360.0);
        let angle = if (angle - 360.0).abs() < 1e-9 {
            0.0
        } else {
            angle
        };
        let mut out = vec![Sexp::atom("at"), x, Sexp::num(y)];
        if angle.abs() > 1e-9 {
            out.push(Sexp::num(angle));
        }
        // `unlocked` and the like after the angle stay.
        out.extend(items.iter().skip(4).cloned());
        Sexp::List(out)
    }
}

/// `(xy 1 2)` as `(xy 1 -2)`: the second number of a point negated.
fn negate_y(items: &[Sexp]) -> Vec<Sexp> {
    items
        .iter()
        .enumerate()
        .map(|(i, s)| match (i, number(s)) {
            (2, Some(y)) => Sexp::num(-y),
            _ => s.clone(),
        })
        .collect()
}

/// `B.Cu` as `F.Cu`, `F.SilkS` as `B.SilkS`: a layer on the other face.
fn other_face(s: &Sexp) -> Sexp {
    match s {
        Sexp::Str(l) | Sexp::Atom(l) if l.starts_with("B.") || l.starts_with("F.") => {
            let flipped = if let Some(rest) = l.strip_prefix("B.") {
                format!("F.{rest}")
            } else {
                format!("B.{}", &l[2..])
            };
            match s {
                Sexp::Str(_) => Sexp::string(flipped),
                _ => Sexp::atom(flipped),
            }
        }
        other => other.clone(),
    }
}
