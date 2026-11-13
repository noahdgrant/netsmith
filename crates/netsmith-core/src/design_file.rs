//! A design saved as JSON: the form between the design stage and the placer (issue #609).
//!
//! `netsmith build --emit-design <file>` writes the designs of a run, and each example has its
//! own beside its source (`<stem>.design.json`, checked by `scripts/check-example-designs.sh`), so
//! a test of the placer, the router or fab starts from the file instead of checking, solving and
//! designing the board again. The file is a JSON array of [`Design`]s in the order the run built
//! them, pretty-printed, with each object's keys in the order of the struct's fields, so the same
//! design is always the same bytes.
//!
//! A number is written as JSON writes it, except an end of a range that is open (an infinite
//! `f64`, which JSON has no number for): that is the string `"inf"` or `"-inf"`. A path on this
//! machine (a model's `resolved` file) is not written.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Design;

/// The text of a design file holding `designs`, ending with a newline.
pub fn to_json(designs: &[Design]) -> String {
    let mut text =
        serde_json::to_string_pretty(designs).expect("a design always serialises to JSON");
    text.push('\n');
    text
}

/// The designs of a design file's text.
pub fn from_json(text: &str) -> Result<Vec<Design>, serde_json::Error> {
    serde_json::from_str(text)
}

/// The designs of the design file at `path`. The error names the file.
pub fn read(path: impl AsRef<Path>) -> Result<Vec<Design>, String> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    from_json(&text).map_err(|e| format!("{}: {e}", path.display()))
}

impl Design {
    /// The one design of the design file at `path` (an example's `<stem>.design.json`). It is an
    /// error when the file holds several boards; [`read`] gives them all.
    pub fn from_json_file(path: impl AsRef<Path>) -> Result<Design, String> {
        let path = path.as_ref();
        let mut designs = read(path)?;
        if designs.len() != 1 {
            let names: Vec<&str> = designs.iter().map(|d| d.name.as_str()).collect();
            return Err(format!(
                "{}: holds {} designs ({}), not one; read them all with `design_file::read`",
                path.display(),
                designs.len(),
                names.join(", ")
            ));
        }
        Ok(designs.remove(0))
    }
}

/// One end of a range as the file has it: a number, or a word for an end JSON has no number for.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum End {
    Number(f64),
    Word(String),
}

impl End {
    fn of(value: f64) -> End {
        if value.is_finite() {
            End::Number(value)
        } else if value.is_nan() {
            End::Word("nan".into())
        } else if value > 0.0 {
            End::Word("inf".into())
        } else {
            End::Word("-inf".into())
        }
    }

    fn value<E: serde::de::Error>(self) -> Result<f64, E> {
        match self {
            End::Number(n) => Ok(n),
            End::Word(w) => match w.as_str() {
                "inf" => Ok(f64::INFINITY),
                "-inf" => Ok(f64::NEG_INFINITY),
                "nan" => Ok(f64::NAN),
                _ => Err(E::custom(format!(
                    "`{w}` is not a number, `inf`, `-inf` or `nan`"
                ))),
            },
        }
    }
}

/// `#[serde(with)]` for an `f64` that may be infinite: the end of a range.
pub(crate) mod end {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::End;

    pub fn serialize<S: Serializer>(value: &f64, s: S) -> Result<S::Ok, S::Error> {
        End::of(*value).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        End::deserialize(d)?.value()
    }
}

/// `#[serde(with)]` for a range `(lo, hi)` whose ends may be infinite.
pub(crate) mod range {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::End;

    pub fn serialize<S: Serializer>(value: &(f64, f64), s: S) -> Result<S::Ok, S::Error> {
        (End::of(value.0), End::of(value.1)).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<(f64, f64), D::Error> {
        let (lo, hi) = <(End, End)>::deserialize(d)?;
        Ok((lo.value()?, hi.value()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Features, Outline, PlaceBound, PlaceField, Placement};

    fn design(name: &str) -> Design {
        Design {
            name: name.into(),
            parts: Vec::new(),
            nets: Vec::new(),
            process: None,
            features: Features::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            modules: Vec::new(),
        }
    }

    #[test]
    fn an_open_range_survives_the_file() {
        let mut d = design("Open");
        d.features.outline = Some(Outline {
            width: (0.0, f64::INFINITY),
            height: (20.0, 20.0),
            corner_radius: 0.0,
            polygon: None,
        });
        d.features.constraints.push(Placement {
            path: "usb".into(),
            module: false,
            bounds: vec![PlaceBound {
                field: PlaceField::Left,
                lo: f64::NEG_INFINITY,
                hi: 3.5,
                source: String::new(),
            }],
            side: None,
            side_source: String::new(),
        });
        let text = to_json(std::slice::from_ref(&d));
        assert!(
            text.contains("\"inf\"") && text.contains("\"-inf\""),
            "{text}"
        );
        assert_eq!(from_json(&text).unwrap(), vec![d]);
    }

    #[test]
    fn a_word_that_is_no_end_is_an_error() {
        let text = to_json(&[design("A")]).replace("\"features\": {", "\"features\": {\"outline\": {\"width\": [0.0, \"wide\"], \"height\": [0.0, 1.0], \"corner_radius\": 0.0, \"polygon\": null},");
        let e = from_json(&text).unwrap_err().to_string();
        assert!(e.contains("`wide` is not a number"), "{e}");
    }
}
