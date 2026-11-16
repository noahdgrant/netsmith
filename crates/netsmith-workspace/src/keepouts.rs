//! The keepouts the editor draws, kept in the board's physical file (ADR 0048, decision 6:
//! requirements go to the physical file, solutions to the layout file; issue #809).
//!
//! A keepout the PCB view draws is written as a `Keepout` member of the unit `physical for
//! Board`, each field a plain `&=` of a literal:
//!
//! ```text
//! .keepout1: Keepout {
//!     .x &= 2mm
//!     .y &= 2mm
//!     .width &= 10mm
//!     .height &= 6mm
//!     .copper &= "allow"
//! }
//! ```
//!
//! The editor moves, resizes and deletes a board's keepout of the physical file that has this
//! shape ([`drawn_in`]): its place and its size stated in millimetres, `.layers` and the four
//! words of what it forbids as strings, and nothing else. A change rewrites the declaration
//! where it stands, so its `///` lines and every other line of the file stay; a new keepout
//! goes at the end of the unit, and a removed one is taken out with its `///` lines
//! ([`netsmith_fabricate::physical::edit`]).
//!
//! A keepout of the physical file in another shape is moved and resized by a span edit
//! ([`stated_in`], issue #810) when each of its `.x`, `.y`, `.width` and `.height` is stated by
//! one `&=` whose right side is a length of numbers alone (`18mm`, `2500um`, `10mm + 8mm`), in
//! its body or as `.antenna.x &= 18mm` in the unit: a move or a resize writes the new number in
//! millimetres over the right side of each of those statements whose value changes
//! ([`restate`]), and every other character of the file stays, `.max_height` and comments in
//! the body among them. What such a keepout forbids and its layers are changed in the file, and
//! so is one whose place or size is computed from another member, a range, or stated twice.
//!
//! A keepout drawn before this was written to the layout file (`[Board.keepouts.<name>]` with a
//! `size`); such an entry is now an error whose message gives the member to write here
//! (`netsmith_pcb::keepout_declaration`, issue #812).

use std::collections::HashMap;

use netsmith_fabricate::physical::{Edit, Section};
use netsmith_lang::Span;
use netsmith_lang::ast::{CmpOp, Expr, Item};
use netsmith_pcb::KeepoutPlace;

/// A keepout of the physical file that the editor may change: what it states, as the layout
/// file's entry for a drawn keepout would have it, and the span of its declaration.
#[derive(Debug, Clone)]
pub(crate) struct Drawn {
    pub place: KeepoutPlace,
    span: Span,
}

/// The keepouts of the unit `physical for board` in `text` that the editor may change, by name.
/// A file that does not parse has none.
pub(crate) fn drawn_in(text: &str, board: &str) -> HashMap<String, Drawn> {
    let Ok(file) = netsmith_lang::parse(text) else {
        return HashMap::new();
    };
    let Some(unit) = file.physicals.iter().find(|u| u.board.name == board) else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    let mut visit = |items: &[Item]| {
        for item in items {
            if let Item::Decl(d) = item
                && d.name.leading_dot
                && d.name.segments.len() == 1
                && d.count.is_none()
                && d.optional.is_none()
                && d.attrs.is_empty()
                && d.ty.args.is_empty()
                && !d.ty.of_member
                && d.ty.segments.last().is_some_and(|s| s.name == "Keepout")
                && let Some(place) = d.body.as_deref().and_then(literal_place)
            {
                out.insert(
                    d.name.segments[0].name.clone(),
                    Drawn {
                        place,
                        span: d.span,
                    },
                );
            }
        }
    };
    visit(&unit.items);
    for item in &unit.items {
        if let Item::Case(case) = item {
            visit(&case.items);
        }
    }
    out
}

/// A keepout of the physical file that the editor moves and resizes by a span edit: where the
/// right side of the one statement of each of `.x`, `.y`, `.width` and `.height` is.
#[derive(Debug, Clone)]
pub(crate) struct Stated {
    values: [Span; 4],
}

/// The fields a span edit rewrites, in the order of [`Stated`]'s spans.
const PLACE_AND_SIZE: [&str; 4] = ["x", "y", "width", "height"];

/// The keepouts of the unit `physical for board` in `text` that are not [`drawn_in`] it and that
/// the editor may move and resize by a span edit, by name. A file that does not parse has none.
pub(crate) fn stated_in(text: &str, board: &str) -> HashMap<String, Stated> {
    let Ok(file) = netsmith_lang::parse(text) else {
        return HashMap::new();
    };
    let Some(unit) = file.physicals.iter().find(|u| u.board.name == board) else {
        return HashMap::new();
    };
    let drawn = drawn_in(text, board);
    let mut items: Vec<&Item> = unit.items.iter().collect();
    for item in &unit.items {
        if let Item::Case(case) = item {
            items.extend(&case.items);
        }
    }
    // Every statement of a field of a board's keepout, by keepout and field: those of its body,
    // and those of the unit that name it (`.antenna.x &= 18mm`).
    let mut keepouts: Vec<String> = Vec::new();
    let mut statements: HashMap<(String, String), Vec<&netsmith_lang::ast::Constraint>> =
        HashMap::new();
    for item in &items {
        match item {
            Item::Decl(d)
                if d.name.leading_dot
                    && d.name.segments.len() == 1
                    && d.count.is_none()
                    && d.ty.segments.last().is_some_and(|s| s.name == "Keepout") =>
            {
                let name = d.name.segments[0].name.clone();
                for item in d.body.iter().flatten() {
                    if let Item::Constraint(c) = item
                        && let Expr::Path(p) = &c.lhs
                        && p.leading_dot
                        && p.segments.len() == 1
                    {
                        statements
                            .entry((name.clone(), p.segments[0].name.clone()))
                            .or_default()
                            .push(c);
                    }
                }
                keepouts.push(name);
            }
            Item::Constraint(c) => {
                if let Expr::Path(p) = &c.lhs
                    && p.leading_dot
                    && p.segments.len() == 2
                {
                    statements
                        .entry((p.segments[0].name.clone(), p.segments[1].name.clone()))
                        .or_default()
                        .push(c);
                }
            }
            _ => {}
        }
    }
    let mut out = HashMap::new();
    for name in keepouts {
        if drawn.contains_key(&name) || out.contains_key(&name) {
            continue;
        }
        let value = |field: &str| -> Option<Span> {
            let [c] = statements
                .get(&(name.clone(), field.to_string()))?
                .as_slice()
            else {
                return None;
            };
            (c.op == CmpOp::Narrow && c.at.is_none() && numbers_alone(&c.rhs)).then(|| c.rhs.span())
        };
        let values = PLACE_AND_SIZE.map(value);
        if let [Some(x), Some(y), Some(width), Some(height)] = values {
            out.insert(
                name,
                Stated {
                    values: [x, y, width, height],
                },
            );
        }
    }
    out
}

/// Whether `expr` is made of numbers alone: no member, call, range or tolerance.
fn numbers_alone(expr: &Expr) -> bool {
    match expr {
        Expr::Number(..) => true,
        Expr::Neg(inner, _) | Expr::Paren(inner, _) => numbers_alone(inner),
        Expr::Binary { lhs, rhs, .. } => numbers_alone(lhs) && numbers_alone(rhs),
        _ => false,
    }
}

/// `text` with the place and size of [`Stated`] keepouts changed, each from `now` to `after`
/// (`[x, y, width, height]` in mm): the right side of the statement of each field whose value
/// changes becomes the new value in millimetres, and nothing else changes.
pub(crate) fn restate(text: &str, changes: &[(&Stated, [f64; 4], [f64; 4])]) -> String {
    let mut edits: Vec<(Span, f64)> = changes
        .iter()
        .flat_map(|(stated, now, after)| {
            (0..4)
                .filter(|&i| (now[i] - after[i]).abs() >= 0.006)
                .map(|i| (stated.values[i], after[i]))
        })
        .collect();
    // From the end, so that each span still points at its statement.
    edits.sort_by_key(|(span, _)| std::cmp::Reverse(span.start));
    let mut text = text.to_string();
    for (span, value) in edits {
        text.replace_range(span.start..span.end, &length(value));
    }
    text
}

/// What a keepout's body states, if it is only `&=` of literals the editor writes.
fn literal_place(body: &[Item]) -> Option<KeepoutPlace> {
    let mut lengths: HashMap<&str, f64> = HashMap::new();
    let mut words: HashMap<&str, String> = HashMap::new();
    for item in body {
        let Item::Constraint(c) = item else {
            return None;
        };
        if c.op != CmpOp::Narrow || c.at.is_some() {
            return None;
        }
        let Expr::Path(p) = &c.lhs else {
            return None;
        };
        if !p.leading_dot || p.segments.len() != 1 {
            return None;
        }
        let field = p.segments[0].name.as_str();
        let fresh = match field {
            "x" | "y" | "width" | "height" => lengths.insert(field, millimetres(&c.rhs)?),
            "layers" | "tracks" | "vias" | "copper" | "parts" => match &c.rhs {
                Expr::Str(s, _) => words.insert(field, s.clone()).map(|_| 0.0),
                _ => return None,
            },
            _ => return None,
        };
        // A field stated twice is not what the editor writes.
        if fresh.is_some() {
            return None;
        }
    }
    let length = |f: &str| lengths.get(f).copied();
    let mut place = KeepoutPlace::at(length("x")?, length("y")?);
    place.size = Some([length("width")?, length("height")?]);
    let word = |f: &str| -> Option<Option<String>> {
        match words.get(f).map(String::as_str) {
            None | Some("forbid") => Some(None),
            Some("allow") => Some(Some("allow".to_string())),
            Some(_) => None,
        }
    };
    place.tracks = word("tracks")?;
    place.vias = word("vias")?;
    place.copper = word("copper")?;
    place.parts = word("parts")?;
    place.layers = words
        .get("layers")
        .filter(|l| l.trim() != "all")
        .map(|l| l.split(',').map(str::trim).collect::<Vec<_>>().join(","));
    Some(place)
}

/// A length written in millimetres (`2mm`, `-1.5mm`), or a bare zero.
fn millimetres(expr: &Expr) -> Option<f64> {
    match expr {
        Expr::Number(n, _) => match n.unit.as_deref() {
            Some("mm") => Some(n.value),
            None if n.value == 0.0 => Some(0.0),
            _ => None,
        },
        Expr::Neg(inner, _) => millimetres(inner).map(|v| -v),
        _ => None,
    }
}

/// A length as the declaration states it: `2mm`, `2.5mm`, `-1mm`.
fn length(v: f64) -> String {
    let v = (v * 100.0).round() / 100.0;
    // No `-0mm`.
    let v = if v == 0.0 { 0.0 } else { v };
    format!("{v}mm")
}

/// The physical file after `changes`, each a keepout's name and what it becomes (`None`:
/// removed), from the file as it is (`old`, `None` if there is none). A keepout the file has in
/// the editor's shape is rewritten where it stands; a new one goes at the end of the unit.
pub(crate) fn rewrite(
    old: Option<&str>,
    board: &str,
    changes: &[(String, Option<KeepoutPlace>)],
) -> Result<String, String> {
    let have = old.map_or_else(HashMap::new, |text| drawn_in(text, board));
    let mut text = old.map(str::to_string);
    // In place, from the end of the file, so that each span still points at its declaration.
    let mut in_place: Vec<(&Drawn, String)> = changes
        .iter()
        .filter_map(|(name, after)| {
            Some((
                have.get(name)?,
                netsmith_pcb::keepout_declaration(name, after.as_ref()?),
            ))
        })
        .collect();
    in_place.sort_by_key(|(d, _)| std::cmp::Reverse(d.span.start));
    if let Some(text) = text.as_mut() {
        for (d, new) in in_place {
            let indent: String = text[..d.span.start]
                .rsplit('\n')
                .next()
                .unwrap_or("")
                .chars()
                .take_while(|c| *c == ' ')
                .collect();
            let mut lines = new.trim_end_matches('\n').lines();
            let mut replaced = lines.next().unwrap_or("").to_string();
            for line in lines {
                replaced.push('\n');
                replaced.push_str(&indent);
                replaced.push_str(line);
            }
            text.replace_range(d.span.start..d.span.end, &replaced);
        }
    }
    let remove: Vec<String> = changes
        .iter()
        .filter(|(name, after)| after.is_none() && have.contains_key(name))
        .map(|(name, _)| name.clone())
        .collect();
    let added: String = changes
        .iter()
        .filter(|(name, _)| !have.contains_key(name))
        .filter_map(|(name, after)| Some(netsmith_pcb::keepout_declaration(name, after.as_ref()?)))
        .collect();
    if remove.is_empty() && added.is_empty() {
        return Ok(text.unwrap_or_default());
    }
    let imports: &[(&str, &str)] = if added.is_empty() {
        &[]
    } else {
        &[("std::pcb", "Keepout")]
    };
    netsmith_fabricate::physical::edit(
        text.as_deref(),
        &Edit {
            board,
            section: Section::Unit,
            remove: &remove,
            text: &added,
            imports,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawn(x: f64, y: f64, w: f64, h: f64) -> KeepoutPlace {
        let mut place = KeepoutPlace::at(x, y);
        place.size = Some([w, h]);
        place
    }

    #[test]
    fn a_new_keepout_makes_the_file_and_reads_back_the_same() {
        let mut k = drawn(2.0, 2.5, 10.0, 6.0);
        k.copper = Some("allow".into());
        k.layers = Some("top,bottom".into());
        let text = rewrite(None, "B", &[("keepout1".into(), Some(k.clone()))]).unwrap();
        assert_eq!(
            text,
            "//! The physical intent of B.\n\nimport std::pcb::Keepout\n\nphysical for B {\n    .keepout1: Keepout {\n        .x &= 2mm\n        .y &= 2.5mm\n        .width &= 10mm\n        .height &= 6mm\n        .layers &= \"top,bottom\"\n        .copper &= \"allow\"\n    }\n}\n"
        );
        assert_eq!(drawn_in(&text, "B")["keepout1"].place, k);
    }

    #[test]
    fn a_keepout_added_and_removed_leaves_the_file_as_it_was() {
        let old = "import std::pcb::Keepout\n\nphysical for B {\n    .r1.place.x &= 5mm\n}\n";
        let text = rewrite(
            Some(old),
            "B",
            &[("k".into(), Some(drawn(1.0, 1.0, 2.0, 2.0)))],
        )
        .unwrap();
        assert!(text.contains("5mm\n\n    .k: Keepout {\n"), "{text}");
        assert_eq!(
            rewrite(Some(&text), "B", &[("k".into(), None)]).unwrap(),
            old
        );
    }

    #[test]
    fn a_change_is_made_where_the_keepout_stands_and_keeps_its_comments() {
        let old = "//! Case TC-12.\nimport std::pcb::{Keepout, Outline}\n\nphysical for B {\n    case {\n        .outline: Outline { .width &= 4mm }\n    }\n\n    /// Under the antenna.\n    .k: Keepout {\n        .x &= 1mm\n        .y &= 1mm\n        .width &= 3mm\n        .height &= 3mm\n    }\n    // kept\n    .r1.place.x &= 5mm\n}\n";
        let text = rewrite(
            Some(old),
            "B",
            &[("k".into(), Some(drawn(2.0, 1.0, 4.0, 3.0)))],
        )
        .unwrap();
        assert_eq!(
            text,
            old.replace(".x &= 1mm", ".x &= 2mm")
                .replace(".width &= 3mm", ".width &= 4mm")
        );
        // Removed with its `///` line; the rest stays.
        let text = rewrite(Some(&text), "B", &[("k".into(), None)]).unwrap();
        assert!(!text.contains("antenna") && !text.contains(".k:"), "{text}");
        assert!(text.contains("// kept\n    .r1.place.x &= 5mm\n"), "{text}");
    }

    #[test]
    fn only_a_keepout_the_editor_could_write_back_is_its_to_change() {
        let text = "import std::pcb::Keepout\n\nphysical for B {\n    .a: Keepout {\n        .x &= 1mm\n        .y &= -1mm\n        .width &= 3mm\n        .height &= 3mm\n        .vias &= \"allow\"\n    }\n    .placed: Keepout {\n        .width &= 3mm\n        .height &= 3mm\n    }\n    .tall: Keepout {\n        .x &= 1mm\n        .y &= 1mm\n        .width &= 3mm\n        .height &= 3mm\n        .max_height &= 2mm\n    }\n    .sum: Keepout {\n        .x &= 1mm + 1mm\n        .y &= 1mm\n        .width &= 3mm\n        .height &= 3mm\n    }\n    .inch: Keepout {\n        .x &= 1in\n        .y &= 1mm\n        .width &= 3mm\n        .height &= 3mm\n    }\n}\n";
        let have = drawn_in(text, "B");
        assert_eq!(have.keys().collect::<Vec<_>>(), ["a"]);
        assert_eq!(have["a"].place.at, [1.0, -1.0]);
        assert_eq!(have["a"].place.vias.as_deref(), Some("allow"));
        assert!(drawn_in(text, "Other").is_empty());
        assert!(drawn_in("physical for B {", "B").is_empty());
    }

    const STATED: &str = "import std::pcb::Keepout\n\nphysical for B {\n    .a: Keepout {\n        .x &= 1mm\n        .y &= 1mm\n        .width &= 3mm\n        .height &= 3mm\n    }\n    /// Under the lid.\n    .tall: Keepout {\n        .x &= 1mm\n        .y &= 1mm // from the top edge\n        .width &= 3mm\n        .height &= 3mm\n        .max_height &= 2mm\n    }\n    .sum: Keepout {\n        .x &= 10mm + 8mm\n        .y &= -(1mm)\n        .width &= 25400um\n        .height &= 3mm\n    }\n    .outside: Keepout {\n        .width &= 3mm\n        .height &= 3mm\n    }\n    .outside.x &= 4mm\n    .outside.y &= 5mm\n    .placed: Keepout {\n        .width &= 3mm\n        .height &= 3mm\n    }\n    .computed: Keepout {\n        .x &= .outside.x + 1mm\n        .y &= 1mm\n        .width &= 3mm\n        .height &= 3mm\n    }\n    .range: Keepout {\n        .x &= 1mm..2mm\n        .y &= 1mm\n        .width &= 3mm\n        .height &= 3mm\n    }\n    .twice: Keepout {\n        .x &= 1mm\n        .y &= 1mm\n        .width &= 3mm\n        .height &= 3mm\n    }\n    .twice.x &= 1mm\n}\n";

    #[test]
    fn a_keepout_whose_place_and_size_are_numbers_stated_once_is_moved_by_a_span_edit() {
        let have = stated_in(STATED, "B");
        let mut names: Vec<&String> = have.keys().collect();
        names.sort();
        // `a` is the editor's whole; the others state a place that is not numbers alone, no
        // place, or a field twice.
        assert_eq!(names, ["outside", "sum", "tall"]);
        assert!(stated_in(STATED, "Other").is_empty());
        assert!(stated_in("physical for B {", "B").is_empty());
    }

    #[test]
    fn a_span_edit_rewrites_only_the_numbers_that_change() {
        let have = stated_in(STATED, "B");
        let text = restate(
            STATED,
            &[
                // Moved right: only `.x`, and the comment after `.y` and `.max_height` stay.
                (&have["tall"], [1.0, 1.0, 3.0, 3.0], [2.5, 1.0, 3.0, 3.0]),
                // Resized: the sum stays, the width in micrometres becomes millimetres.
                (
                    &have["sum"],
                    [18.0, -1.0, 25.4, 3.0],
                    [18.0, -1.0, 20.0, 3.0],
                ),
                // Stated outside its body.
                (&have["outside"], [4.0, 5.0, 3.0, 3.0], [4.0, 6.0, 3.0, 3.0]),
            ],
        );
        let expected = STATED
            .replacen(
                "    .tall: Keepout {\n        .x &= 1mm\n",
                "    .tall: Keepout {\n        .x &= 2.5mm\n",
                1,
            )
            .replace(".width &= 25400um", ".width &= 20mm")
            .replace(".outside.y &= 5mm", ".outside.y &= 6mm");
        assert_eq!(text, expected);
        assert!(text.contains(".x &= 10mm + 8mm\n") && text.contains(".max_height &= 2mm"));
        // The file still reads the same keepouts the same way.
        assert_eq!(stated_in(&text, "B").len(), 3);
    }
}
