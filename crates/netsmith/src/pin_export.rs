//! The pin map for the firmware author: what `build` writes of the pin requests of a board
//! (ADR 0101), as a C header and a JSON map beside the schematic.
//!
//! Each signal is named by its request: `.status: .u.GPIO` is the signal `status`. A board with
//! no request has no pin map, and `build` writes neither file. The text is the same for the
//! same pins every time: signals in the byte order of their request names, and no date.

use std::collections::BTreeMap;

use netsmith_check::PinsOutcome;
use netsmith_core::Design;
use serde::Serialize;

/// The two files of a board's pin map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinMap {
    /// `<board>.pins.h`.
    pub header: String,
    /// `<board>.pins.json`.
    pub json: String,
}

/// The name of the header beside the schematic of the board called `board`.
pub fn header_name(board: &str) -> String {
    format!("{board}.pins.h")
}

/// The name of the JSON map beside the schematic of the board called `board`.
pub fn json_name(board: &str) -> String {
    format!("{board}.pins.json")
}

/// One request and the pin it got, with what the part says of the pin.
struct Signal<'a> {
    /// The request's name: its path in the board (`status`, `hub.scl`).
    request: &'a str,
    /// The part's path in the board.
    part: &'a str,
    /// The pin's name in the datasheet (`P0.02`), or its path when the part has no such pin.
    pin: String,
    /// The pads of the pin, joined by a comma, when the part has a pin table row for it.
    pad: Option<String>,
    function: &'a str,
    locked: bool,
}

#[derive(Serialize)]
struct JsonMap<'a> {
    board: &'a str,
    signals: BTreeMap<&'a str, JsonSignal<'a>>,
    source: &'a str,
}

#[derive(Serialize)]
struct JsonSignal<'a> {
    function: &'a str,
    locked: bool,
    pad: Option<&'a str>,
    part: &'a str,
    pin: &'a str,
}

/// The pin map of a stand-alone board: `design` is the board's design, `definition` its
/// definition's name (the key of its table in the pin file), `pins` what the check decided about
/// the pin file and `source` the name of the source file as a message names it. `None` when the
/// board has no request. An error when two requests are one signal in C.
pub fn render(
    design: &Design,
    definition: &str,
    pins: &PinsOutcome,
    source: &str,
) -> Result<Option<PinMap>, String> {
    let mut signals: Vec<Signal> = Vec::new();
    for space in pins.spaces.get(definition).into_iter().flatten() {
        let part = design.parts.iter().find(|p| p.path == space.path);
        for r in &space.requests {
            let row = part.and_then(|p| p.pins.iter().find(|pin| pin.name == r.pin));
            signals.push(Signal {
                request: &r.key,
                part: &space.path,
                pin: row.map_or_else(|| r.pin.clone(), |p| p.datasheet.clone()),
                pad: row
                    .map(|p| p.pads.join(", "))
                    .filter(|pads| !pads.is_empty()),
                function: &r.capability,
                locked: r.lock,
            });
        }
    }
    if signals.is_empty() {
        return Ok(None);
    }
    signals.sort_by(|a, b| a.request.cmp(b.request));

    let board = &design.name;
    let mut taken: BTreeMap<String, &str> = BTreeMap::new();
    for s in &signals {
        if let Some(other) = taken.insert(identifier(s.request), s.request) {
            return Err(format!(
                "error: {board}: the pin requests `{other}` and `{}` are the same signal `{}` in the C header (upper case, anything but a letter or a digit is `_`); rename one",
                s.request,
                identifier(s.request)
            ));
        }
    }
    Ok(Some(PinMap {
        header: header(board, source, &signals),
        json: json(board, source, &signals),
    }))
}

/// `ARM-board.1` as `ARM_BOARD_1`: upper case, and `_` for anything that is not an ASCII letter
/// or digit, so that it is a C identifier part.
fn identifier(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// A C string literal of `text`.
fn c_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\x{:02x}\"\"", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `text` for the inside of a `//` comment: one line, nothing that ends it.
fn comment(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// The pin's number, when its name is letters and underscores and then one number: `GPIO7` is 7
/// and `PA2` is 2. A name with the number in two places (`P0.13`) or with something after it
/// has none: which of the numbers firmware counts by is not for netsmith to guess.
fn number(pin: &str) -> Option<u64> {
    let digits = pin.len() - pin.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (head, tail) = pin.split_at(pin.len() - digits);
    if tail.is_empty()
        || tail.len() > 9
        || head.is_empty()
        || !head.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
    {
        return None;
    }
    tail.parse().ok()
}

fn header(board: &str, source: &str, signals: &[Signal]) -> String {
    let name = identifier(board);
    let guard = format!("{name}_PINS_H");
    let mut out = format!(
        "// Generated by netsmith from {}. Do not edit: run `netsmith build` again.\n// The pins of the requests of board {}.\n#ifndef {guard}\n#define {guard}\n",
        comment(source),
        comment(board)
    );
    for s in signals {
        let macro_name = format!("{name}_PIN_{}", identifier(s.request));
        let pad = s
            .pad
            .as_ref()
            .map(|p| format!(", pad {}", comment(p)))
            .unwrap_or_default();
        out.push_str(&format!(
            "\n// {}: pin {}{pad} of {}, function {}{}\n",
            comment(s.request),
            comment(&s.pin),
            comment(s.part),
            comment(s.function),
            if s.locked { ", locked" } else { "" }
        ));
        out.push_str(&format!("#define {macro_name}_NAME {}\n", c_string(&s.pin)));
        out.push_str(&format!(
            "#define {macro_name}_FUNCTION {}\n",
            c_string(s.function)
        ));
        if let Some(n) = number(&s.pin) {
            out.push_str(&format!("#define {macro_name}_NUMBER {n}\n"));
        }
    }
    out.push_str(&format!("\n#endif // {guard}\n"));
    out
}

fn json(board: &str, source: &str, signals: &[Signal]) -> String {
    let map = JsonMap {
        board,
        signals: signals
            .iter()
            .map(|s| {
                (
                    s.request,
                    JsonSignal {
                        function: s.function,
                        locked: s.locked,
                        pad: s.pad.as_deref(),
                        part: s.part,
                        pin: &s.pin,
                    },
                )
            })
            .collect(),
        source,
    };
    let mut text = serde_json::to_string_pretty(&map).expect("a map of strings is JSON");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pin_has_a_number_only_when_its_name_ends_in_one_number() {
        assert_eq!(number("GPIO7"), Some(7));
        assert_eq!(number("p02"), Some(2));
        assert_eq!(number("PA10"), Some(10));
        assert_eq!(number("P0.13"), None);
        assert_eq!(number("GPIO7_B"), None);
        assert_eq!(number("VDD"), None);
        assert_eq!(number("7"), None);
        assert_eq!(number("P0_13"), None);
    }

    #[test]
    fn a_name_becomes_a_c_identifier_and_a_string_is_escaped() {
        assert_eq!(identifier("hub.scl-2"), "HUB_SCL_2");
        assert_eq!(c_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }
}
