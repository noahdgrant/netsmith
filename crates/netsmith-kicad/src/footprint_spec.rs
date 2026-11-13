//! The `gen:` footprint of a project's `kicad.toml`: a generator's name and its parameters
//! in place of a `Library:Name` id (ADR 0104).
//!
//! ```toml
//! footprint = "gen:QFN(pins=48, pitch=0.5mm, body=7x7mm, ep=5.2x5.2mm, lead_width=0.2-0.3mm, lead_length=0.35-0.45mm)"
//! ```
//!
//! This module reads the text and spells it as an id; it does not know the generators. What a
//! generator calls its parameters, which it needs and which ranges it takes is the generators'
//! (`netsmith_pcb::generated_footprint`), which check a [`FootprintSpec`] when the mapping is used
//! (`Libraries::for_project`).
//!
//! The id is what the rest of netsmith passes round where it passes `Library:Name`: the
//! library is `gen` and the name is the generator, then each parameter sorted by key, as
//! `key` followed by its value, joined by `_`: `gen:QFN_body7x7mm_ep5.2x5.2mm_lead_length0.35-0.45mm_..._pins48`.
//! Two spellings of one set of numbers (`0.50mm`, `.5mm`, a different order, other spaces) are one id,
//! and the id reads back to the same spec ([`FootprintSpec::from_id`]). A value starts with a digit, a
//! key has none, so the pieces of the name are told apart without a table.

use std::collections::BTreeMap;

/// The library of a generated footprint's id, and the word that starts the spelling in a
/// `kicad.toml` row (`gen:`).
pub const LIBRARY: &str = "gen";

/// A generator and its parameters, as the mapping wrote them, with the numbers spelled
/// as [`normalise`] spells them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootprintSpec {
    pub generator: String,
    /// By key, so that the order a row wrote them in does not matter.
    pub params: BTreeMap<String, String>,
}

/// Whether `text` is written as a generated footprint: it starts with `gen:`.
pub(crate) fn is_spelling(text: &str) -> bool {
    text.starts_with("gen:")
}

/// Whether `id` is the id of a generated footprint, `gen:Name_...`.
#[cfg(test)]
pub(crate) fn is_id(id: &str) -> bool {
    id.split_once(':').is_some_and(|(l, _)| l == LIBRARY)
}

/// Spell the numbers in `value` the way ids do: the shortest text of each number (`0.50` is
/// `0.5`, `.5` is `0.5`, `7.0` is `7`). Everything else (`x`, `-`, `mm`) stays as written.
fn normalise(value: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = value;
    while !rest.is_empty() {
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        if end == 0 {
            let c = rest.chars().next().expect("the text is not empty");
            out.push(c);
            rest = &rest[c.len_utf8()..];
            continue;
        }
        let number = &rest[..end];
        let v: f64 = number
            .parse()
            .map_err(|_| format!("`{number}` in {value:?} is not a number"))?;
        out.push_str(&format!("{v}"));
        rest = &rest[end..];
    }
    Ok(out)
}

impl FootprintSpec {
    /// Read `gen:Name(key=value, ...)`. The parentheses may be left off when there are no
    /// parameters. The message says what is wrong with the text, and not where the text is
    /// (the caller names the row and the line).
    pub fn parse(text: &str) -> Result<FootprintSpec, String> {
        let body = text
            .strip_prefix("gen:")
            .ok_or_else(|| format!("{text:?} does not start with `gen:`"))?
            .trim();
        let (generator, args) = match body.split_once('(') {
            Some((generator, rest)) => {
                let args = rest.trim_end().strip_suffix(')').ok_or_else(|| {
                    format!("`gen:{body}` is missing the `)` that closes the parameters")
                })?;
                (generator.trim(), args)
            }
            None => (body, ""),
        };
        let mut chars = generator.chars();
        if !chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            || !chars.all(|c| c.is_ascii_alphanumeric())
        {
            return Err(format!(
                "the generator name {generator:?} must be letters and digits, starting with a letter, as in `gen:QFN(pins=16, ...)`"
            ));
        }
        let mut params = BTreeMap::new();
        for item in args.split(',') {
            let item = item.trim();
            if item.is_empty() {
                // A trailing comma and an empty list are both fine.
                continue;
            }
            let (key, value) = item
                .split_once('=')
                .ok_or_else(|| format!("`{item}` is not `key=value`"))?;
            let (key, value) = (key.trim(), value.trim());
            if key.is_empty()
                || !key.starts_with(|c: char| c.is_ascii_lowercase())
                || !key.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                || key.ends_with('_')
                || key.contains("__")
            {
                return Err(format!(
                    "the parameter name {key:?} must be lowercase letters and single `_`, as in `lead_width`"
                ));
            }
            if value.is_empty() {
                return Err(format!("`{key}` has no value"));
            }
            if !value.starts_with(|c: char| c.is_ascii_digit() || c == '.')
                || !value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
            {
                return Err(format!(
                    "the value {value:?} of `{key}` must be a number, a pair `7x7`, a range `0.2-0.3`, \
                     and may end in `mm`: digits, letters, `.` and `-`, starting with a digit"
                ));
            }
            let value = normalise(value).map_err(|e| format!("`{key}`: {e}"))?;
            if params.insert(key.to_string(), value).is_some() {
                return Err(format!("`{key}` is given twice"));
            }
        }
        Ok(FootprintSpec {
            generator: generator.to_string(),
            params,
        })
    }

    /// The id the rest of netsmith uses for this footprint: `gen:QFN_body7x7mm_..._pins48`.
    pub fn id(&self) -> String {
        let mut id = format!("{LIBRARY}:{}", self.generator);
        for (key, value) in &self.params {
            id.push('_');
            id.push_str(key);
            id.push_str(value);
        }
        id
    }

    /// The spec an id of [`FootprintSpec::id`] was made from, or `None` for another id.
    pub fn from_id(id: &str) -> Option<FootprintSpec> {
        let name = id.strip_prefix("gen:")?;
        let mut pieces = name.split('_');
        let generator = pieces.next().filter(|g| !g.is_empty())?;
        let mut params = BTreeMap::new();
        let mut key = String::new();
        for piece in pieces {
            match piece.find(|c: char| c.is_ascii_digit() || c == '.') {
                // A word of a key such as `lead_width`.
                None => {
                    key.push_str(piece);
                    key.push('_');
                }
                Some(at) => {
                    key.push_str(&piece[..at]);
                    if key.is_empty() || key.ends_with('_') {
                        return None;
                    }
                    if params
                        .insert(std::mem::take(&mut key), piece[at..].to_string())
                        .is_some()
                    {
                        return None;
                    }
                }
            }
        }
        key.is_empty().then_some(FootprintSpec {
            generator: generator.to_string(),
            params,
        })
    }

    /// The spelling a `kicad.toml` row uses, with the parameters in the id's order:
    /// `gen:QFN(body=7x7mm, ep=5.2x5.2mm, ...)`.
    pub fn spelling(&self) -> String {
        let args: Vec<String> = self
            .params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        format!("gen:{}({})", self.generator, args.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spelling_reads_and_the_id_reads_back_to_the_same_spec() {
        let spec = FootprintSpec::parse(
            "gen:QFN(pins=48, pitch=0.50mm, body=7x7mm, ep=5.2x5.2mm, lead_width=0.2-0.3mm)",
        )
        .unwrap();
        assert_eq!(spec.generator, "QFN");
        assert_eq!(spec.params["pitch"], "0.5mm");
        assert_eq!(spec.params["lead_width"], "0.2-0.3mm");
        let id = spec.id();
        assert_eq!(
            id,
            "gen:QFN_body7x7mm_ep5.2x5.2mm_lead_width0.2-0.3mm_pins48_pitch0.5mm"
        );
        assert!(is_id(&id) && !is_id("Device:R"));
        assert_eq!(FootprintSpec::from_id(&id).unwrap(), spec);
        assert_eq!(
            FootprintSpec::parse(&spec.spelling()).unwrap(),
            spec,
            "{}",
            spec.spelling()
        );
    }

    #[test]
    fn one_set_of_numbers_has_one_id_whatever_the_order_the_spaces_and_the_zeros() {
        let a = FootprintSpec::parse("gen:SO(pins=8,pitch=1.27mm,body=3.9x4.9mm)").unwrap();
        let b =
            FootprintSpec::parse("gen:SO( body=3.90x4.90mm , pitch=1.270mm,  pins=08, )").unwrap();
        assert_eq!(a.id(), b.id());
        assert_eq!(
            FootprintSpec::parse("gen:BGA(pitch=.8mm)").unwrap().id(),
            "gen:BGA_pitch0.8mm"
        );
    }

    #[test]
    fn a_generator_with_digits_and_no_parameters_has_an_id_that_reads_back() {
        for text in ["gen:SOT23", "gen:SOT23()"] {
            let spec = FootprintSpec::parse(text).unwrap();
            assert_eq!(spec.id(), "gen:SOT23");
            assert_eq!(FootprintSpec::from_id("gen:SOT23").unwrap(), spec);
        }
    }

    #[test]
    fn text_that_is_not_a_spelling_says_what_is_wrong() {
        for (text, want) in [
            ("QFN(pins=4)", "does not start with `gen:`"),
            ("gen:QFN(pins=4", "missing the `)`"),
            ("gen:(pins=4)", "generator name"),
            ("gen:Q-FN(pins=4)", "generator name"),
            ("gen:QFN(pins)", "is not `key=value`"),
            ("gen:QFN(Pins=4)", "parameter name"),
            ("gen:QFN(lead__width=4)", "parameter name"),
            ("gen:QFN(pins=)", "no value"),
            ("gen:QFN(pins=four)", "must be a number"),
            ("gen:QFN(pins=1.2.3)", "not a number"),
            ("gen:QFN(pins=4, pins=5)", "given twice"),
        ] {
            let e = FootprintSpec::parse(text).unwrap_err();
            assert!(e.contains(want), "{text}: {e}");
        }
    }

    #[test]
    fn an_id_that_is_not_made_by_id_is_not_a_spec() {
        for id in [
            "Device:R",
            "gen:",
            "gen:QFN_pins",
            "gen:QFN_4",
            "gen:QFN_pins4_pins5",
        ] {
            assert_eq!(FootprintSpec::from_id(id), None, "{id}");
        }
    }
}
