//! `netsmith part from-symbol <lib.kicad_sym | .kicad_sch> <symbol>` (#453): a draft part file
//! from a KiCad symbol's pins.
//!
//! A port's original schematic already carries each pin's name, number and electrical type in its
//! symbols. This prints them as the `pins` table of a part, for review: one row for each pin name
//! of a unit (the pads of a name that the symbol stacks, such as four `GND` pads, are one row, and
//! a name in several units is a row of each, its path ending in the unit's letter), with a path
//! made from the name, the name the symbol prints, and the kind taken from the symbol's type.
//! It is a draft and says so in its header: KiCad's types are often looser than the datasheet (a
//! GPIO is `bidirectional`, a comparator's open-drain output is `output`), so the datasheet check
//! stays a person's or an agent's step. A pin whose kind the type does not decide (`unspecified`,
//! `free`, `open_emitter`) or whose stacked pads disagree is written `TODO_KIND`, which does not
//! parse, and listed in the header, so the file does not build until each is decided.

use std::collections::BTreeMap;

use netsmith_kicad::{PinType, Symbol};

/// The kind a symbol's electrical type gives a pin, or `None` when the type does not decide it.
pub fn kind_of(kind: PinType) -> Option<&'static str> {
    Some(match kind {
        PinType::PowerIn | PinType::PowerOut => "power",
        PinType::Input => "input",
        PinType::Output => "output",
        PinType::Bidirectional => "bidirectional",
        PinType::Passive => "passive",
        PinType::TriState => "tri_state",
        PinType::OpenCollector => "open_drain",
        PinType::NoConnect => "no_connect",
        PinType::Free | PinType::Unspecified | PinType::OpenEmitter => return None,
    })
}

/// The text a symbol prints for a pin: `~{RESET}` (an overbar) is `RESET` and makes the path
/// end in `_n`; an unnamed pin (`~`) is named for its pad.
fn printed(name: &str, pad: &str) -> (String, bool) {
    if name.is_empty() || name == "~" {
        return (format!("PIN{pad}"), false);
    }
    let low = name.contains("~{");
    (name.replace("~{", "").replace('}', ""), low)
}

/// A member path from a pin's printed name: lower case, words joined by `_`, never starting with
/// a digit (`3V3` is `p3v3`), and a trailing `+` or `-` is `_pos` or `_neg` (`INA-` is `ina_neg`).
fn path_of(name: &str, low: bool) -> String {
    // The two inputs of a comparator or an amplifier (`INA+`, `INA-`) keep their sign.
    let (name, sign) = match name.strip_suffix('+') {
        Some(rest) => (rest, "_pos"),
        None => match name.strip_suffix('-') {
            Some(rest) => (rest, "_neg"),
            None => (name, ""),
        },
    };
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    let mut out = out.trim_end_matches('_').to_string();
    if out.is_empty() {
        out = "pin".to_string();
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, 'p');
    }
    out.push_str(sign);
    if low {
        out.push_str("_n");
    }
    out
}

/// A part name from a symbol's name: `STM32G071C8T6` is `Stm32g071c8t6`, `LP2985-3.3` is
/// `Lp2985_3_3`.
fn part_name(symbol: &str) -> String {
    let base = symbol.rsplit_once(':').map_or(symbol, |(_, n)| n);
    let mut out = String::new();
    let mut upper = true;
    for c in base.chars() {
        if c.is_ascii_alphanumeric() {
            if upper {
                out.push(c.to_ascii_uppercase());
                upper = false;
            } else {
                out.push(c.to_ascii_lowercase());
            }
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    let out = out.trim_end_matches('_').to_string();
    if out.starts_with(|c: char| c.is_ascii_digit()) || out.is_empty() {
        format!("P{out}")
    } else {
        out
    }
}

/// A pad number for ordering: numbers by value, then the rest (`A1`, `EP`) by text.
fn pad_order(pad: &str) -> (u64, String) {
    (pad.parse().unwrap_or(u64::MAX), pad.to_string())
}

/// A pad of a pin name: its number, its type, whether it is hidden (stacked) and whether the
/// name is drawn with an overbar.
type StackedPad<'a> = (&'a str, PinType, bool, bool);

/// One row of the draft: a pin name and its pads.
struct Row {
    path: String,
    name: String,
    kind: Option<&'static str>,
    pads: Vec<String>,
    /// Why the kind is not decided, when it is not.
    why: Option<String>,
}

/// The draft part file of `symbol`, read from `file` as `asked`.
pub fn draft(symbol: &Symbol, file: &str, asked: &str) -> String {
    // The pads of one printed name in one unit are one row; a hidden stacked pad takes the kind of
    // the pad it is stacked on. A name in several units (the `-` input of each half of a dual
    // amplifier) is a pin of each, and its path ends in the unit's letter. Unit 0 is every unit's.
    let mut units_of: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for pin in symbol.pins() {
        let units = units_of
            .entry(printed(&pin.name, &pin.number).0)
            .or_default();
        if pin.unit != 0 && !units.contains(&pin.unit) {
            units.push(pin.unit);
        }
    }
    let mut by_name: BTreeMap<(String, u32), Vec<StackedPad>> = BTreeMap::new();
    for pin in symbol.pins() {
        let (name, low) = printed(&pin.name, &pin.number);
        let unit = if units_of[&name].len() > 1 {
            pin.unit
        } else {
            0
        };
        by_name.entry((name, unit)).or_default().push((
            pin.number.as_str(),
            pin.kind,
            pin.hidden,
            low,
        ));
    }
    let mut rows: Vec<Row> = Vec::new();
    for ((name, unit), pads) in &by_name {
        let shown: Vec<PinType> = pads.iter().filter(|p| !p.2).map(|p| p.1).collect();
        let types: Vec<PinType> = if shown.is_empty() {
            pads.iter().map(|p| p.1).collect()
        } else {
            shown
        };
        let mut distinct: Vec<PinType> = Vec::new();
        for t in types {
            if !distinct.contains(&t) {
                distinct.push(t);
            }
        }
        let (kind, why) = match distinct.as_slice() {
            [one] => match kind_of(*one) {
                Some(k) => (Some(k), None),
                None => (None, Some(format!("the symbol types it `{}`", one.name()))),
            },
            many => (
                None,
                Some(format!(
                    "its pads are typed {}",
                    many.iter()
                        .map(|t| format!("`{}`", t.name()))
                        .collect::<Vec<_>>()
                        .join(" and ")
                )),
            ),
        };
        let mut numbers: Vec<String> = pads.iter().map(|p| p.0.to_string()).collect();
        numbers.sort_by_key(|p| pad_order(p));
        numbers.dedup();
        let mut path = path_of(name, pads.iter().any(|p| p.3));
        if *unit > 0 {
            // Unit 1 is `a`, as KiCad letters the units of a symbol.
            path.push('_');
            path.push(char::from(b'a' + ((unit - 1) % 26) as u8));
        }
        rows.push(Row {
            path,
            name: name.clone(),
            kind,
            pads: numbers,
            why,
        });
    }
    rows.sort_by_key(|r| pad_order(&r.pads[0]));
    // Two names that make one path (`A-B` and `A_B`) get a number.
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for row in &mut rows {
        let n = seen.entry(row.path.clone()).or_default();
        *n += 1;
        if *n > 1 {
            row.path = format!("{}_{n}", row.path);
        }
    }
    let designator = symbol
        .definition()
        .find_all("property")
        .find(|p| p.args().first().and_then(|a| a.text()) == Some("Reference"))
        .and_then(|p| p.args().get(1).and_then(|a| a.text()).map(str::to_string))
        .map(|r| {
            r.trim_end_matches(|c: char| c.is_ascii_digit() || c == '?')
                .to_string()
        })
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| "U".to_string());
    let undecided: Vec<String> = rows
        .iter()
        .filter_map(|r| {
            r.why
                .as_ref()
                .map(|w| format!("`.{}` (pad {}): {w}", r.path, r.pads.join(", ")))
        })
        .collect();

    let mut out = String::new();
    out.push_str(&format!(
        "// DRAFT written by `netsmith part from-symbol` from {file}, symbol `{asked}`.\n"
    ));
    out.push_str("// It is the symbol's pin table, not the datasheet's. Check every row against the datasheet's pin\n");
    out.push_str("// table (names, numbers, kinds) and say so with the page; KiCad's types are often looser than the\n");
    out.push_str("// datasheet (a GPIO is `bidirectional`, a comparator's open-drain output is `output`). Group the\n");
    out.push_str("// supply pins into rails, give a pin its functions (`fn`), and give the part its catalog.\n");
    if undecided.is_empty() {
        out.push_str("// The symbol's types decided the kind of every pin.\n");
    } else {
        out.push_str("// The symbol does not decide the kind of these pins, which are written TODO_KIND and stop the\n// file from building until each is decided from the datasheet:\n");
        for line in &undecided {
            out.push_str(&format!("//   {line}\n"));
        }
    }
    out.push_str(&format!("part {} {{\n", part_name(asked)));
    out.push_str(&format!("    .designator &= \"{designator}\"\n\n"));
    out.push_str("    pins {\n");
    let width = rows.iter().map(|r| r.path.len()).max().unwrap_or(0) + 1;
    let kind_width = rows
        .iter()
        .map(|r| r.kind.unwrap_or("TODO_KIND").len())
        .max()
        .unwrap_or(0);
    out.push_str(&format!(
        "        //{:<w$} {:<k$} name pads\n",
        "pin",
        "kind",
        w = width - 1,
        k = kind_width
    ));
    for row in &rows {
        let pads: Vec<String> = row.pads.iter().map(|p| format!("\"{p}\"")).collect();
        out.push_str(&format!(
            "        {:<w$} {:<k$} \"{}\" {}\n",
            format!(".{}", row.path),
            row.kind.unwrap_or("TODO_KIND"),
            row.name.replace('"', "'"),
            pads.join(" "),
            w = width,
            k = kind_width
        ));
    }
    out.push_str("    }\n}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_the_printed_name_in_lower_case_and_an_overbar_ends_in_n() {
        assert_eq!(path_of("PA9", false), "pa9");
        assert_eq!(path_of("VDD/VDDA", false), "vdd_vdda");
        assert_eq!(path_of("3V3", false), "p3v3");
        assert_eq!(path_of("RESET", true), "reset_n");
        assert_eq!(path_of("INA+", false), "ina_pos");
        assert_eq!(path_of("INA-", false), "ina_neg");
        assert_eq!(printed("~{RESET}", "7"), ("RESET".to_string(), true));
        assert_eq!(printed("~", "7"), ("PIN7".to_string(), false));
    }

    #[test]
    fn a_part_name_is_the_symbols_without_its_library() {
        assert_eq!(part_name("Local Library:STM32G071C8T6"), "Stm32g071c8t6");
        assert_eq!(part_name("LP2985-3.3"), "Lp2985_3_3");
    }

    #[test]
    fn the_kinds_are_the_languages_and_three_types_decide_none() {
        assert_eq!(kind_of(PinType::PowerIn), Some("power"));
        assert_eq!(kind_of(PinType::OpenCollector), Some("open_drain"));
        for undecided in [PinType::Free, PinType::Unspecified, PinType::OpenEmitter] {
            assert_eq!(kind_of(undecided), None);
        }
    }
}
