//! "Require here" (ADR 0045 C5, issue #815): the PCB view writes where a part is as placement
//! requirements of the board's physical file (ADR 0048, decision 6: requirements go to the
//! physical file, solutions to the layout file).
//!
//! The editor states a part's place in one shape, a statement for each field, each a plain `&=`
//! of a literal in the unit `physical for Board`:
//!
//! ```text
//! .usb.place.x &= 12.5mm
//! .usb.place.y &= 4.25mm
//! .usb.place.rotation &= 90
//! .usb.place.side &= "top"
//! ```
//!
//! `x` and `y` are the centre of the part's body, the box the check measures, to a tenth of a
//! micrometre, so the requirement holds where the layout file puts the part to within the
//! check's half micrometre. Those four fields of a part, stated so, are the editor's to change
//! ([`scan`]): a change writes the new literal over the right side of the statement where it
//! stands, takes out a field it no longer states with its line, and puts a new one at the end of
//! the unit, so every comment and every other line of the file stays ([`rewrite`]).
//!
//! The editor writes nothing about a part whose place the file states in any other shape: a
//! window (`>=`, a tolerance, a range), an edge (`left`, `right`, `top`, `bottom`), a field
//! stated twice, a relation with another part or a `row` statement. Such a requirement came from
//! a drawing and is changed in the file; the scene says why ([`Scan::elsewhere`]), and the page
//! offers nothing. A module's requirement does not stop it: it holds for every part of the
//! module, the editor never writes it, and the check says if the new place breaks it.

use std::collections::HashMap;

use netsmith_editor::{
    ApiError,
    api::{PcbSide, RequiredPlace},
};
use netsmith_fabricate::physical::{Edit, Section};
use netsmith_lang::Span;
use netsmith_lang::ast::{CmpOp, Expr, Item, Path};

/// What the unit states about one part's place.
#[derive(Debug, Clone, Default)]
pub(crate) struct Scan {
    /// The fields stated in the editor's shape.
    pub place: RequiredPlace,
    /// For each of those, the span of its statement and of its right side.
    spans: HashMap<&'static str, (Span, Span)>,
    /// The first statement about the part's place in another shape, with its line:
    /// `` `.usb.place.left &= 1mm +/- 0.1mm` (main.physical.nsm:12) ``.
    pub elsewhere: Option<String>,
}

const FIELDS: [&str; 4] = ["x", "y", "rotation", "side"];

/// What the unit `physical for board` in `text` (the file `file`, for messages) states about the
/// place of each part it names, by the part's path. A file that does not parse states nothing.
pub(crate) fn scan(text: &str, board: &str, file: &str) -> HashMap<String, Scan> {
    let Ok(parsed) = netsmith_lang::parse(text) else {
        return HashMap::new();
    };
    let Some(unit) = parsed.physicals.iter().find(|u| u.board.name == board) else {
        return HashMap::new();
    };
    let mut out: HashMap<String, Scan> = HashMap::new();
    let line = |at: usize| text[..at].matches('\n').count() + 1;
    let quote = |span: Span| {
        format!(
            "`{}` ({file}:{})",
            text[span.start..span.end].trim(),
            line(span.start)
        )
    };
    let mut items: Vec<&Item> = unit.items.iter().collect();
    for item in &unit.items {
        if let Item::Case(case) = item {
            items.extend(case.items.iter());
        }
    }
    for item in items {
        match item {
            Item::Constraint(c) => {
                let mut mentioned = Vec::new();
                for e in [Some(&c.lhs), Some(&c.rhs), c.at.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    places_in(e, &mut mentioned);
                }
                let owned = match &c.lhs {
                    Expr::Path(p) if c.op == CmpOp::Narrow && c.at.is_none() => place_of(p)
                        .and_then(|(part, field)| {
                            let field = FIELDS.into_iter().find(|f| *f == field)?;
                            literal(field, &c.rhs).map(|v| (part, field, v))
                        }),
                    _ => None,
                };
                if let Some((part, field, value)) = owned {
                    let s = out.entry(part).or_default();
                    if s.spans.contains_key(field) {
                        s.elsewhere.get_or_insert_with(|| quote(c.span));
                    } else {
                        s.spans.insert(field, (c.span, c.rhs.span()));
                        value.set(&mut s.place, field);
                    }
                    continue;
                }
                for (part, _) in mentioned {
                    out.entry(part)
                        .or_default()
                        .elsewhere
                        .get_or_insert_with(|| quote(c.span));
                }
            }
            Item::Row(r) => {
                // The run's first and last parts; the ones between are found by the relations
                // the design lowers it to ([`crate::pcb::scene`]).
                for p in [&r.first, &r.last] {
                    out.entry(dotted(p))
                        .or_default()
                        .elsewhere
                        .get_or_insert_with(|| quote(r.span));
                }
            }
            _ => {}
        }
    }
    out
}

/// A value the editor writes.
enum Literal {
    Length(f64),
    Degrees(f64),
    Side(PcbSide),
}

impl Literal {
    fn set(self, place: &mut RequiredPlace, field: &str) {
        match (self, field) {
            (Self::Length(v), "x") => place.x = Some(v),
            (Self::Length(v), "y") => place.y = Some(v),
            (Self::Degrees(v), "rotation") => place.rotation = Some(v),
            (Self::Side(s), "side") => place.side = Some(s),
            _ => {}
        }
    }
}

/// The literal `expr` is, if it is one the editor writes for `field`.
fn literal(field: &str, expr: &Expr) -> Option<Literal> {
    match field {
        "x" | "y" => crate::keepouts::millimetres(expr).map(Literal::Length),
        "rotation" => degrees(expr).map(Literal::Degrees),
        "side" => match expr {
            Expr::Str(s, _) if s == "top" => Some(Literal::Side(PcbSide::Top)),
            Expr::Str(s, _) if s == "bottom" => Some(Literal::Side(PcbSide::Bottom)),
            _ => None,
        },
        _ => None,
    }
}

fn degrees(expr: &Expr) -> Option<f64> {
    match expr {
        Expr::Number(n, _) if n.unit.is_none() => Some(n.value),
        Expr::Neg(inner, _) => degrees(inner).map(|v| -v),
        _ => None,
    }
}

fn dotted(p: &Path) -> String {
    p.segments
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

/// `.a.b.place.x` is the part `a.b` and the field `x`; a path with no `.place` after a part
/// (the board's own `.place.side`, a keepout's field) is none.
fn place_of(p: &Path) -> Option<(String, String)> {
    if !p.leading_dot {
        return None;
    }
    let at = p.segments.iter().position(|s| s.name == "place")?;
    if at == 0 {
        return None;
    }
    let part = p.segments[..at]
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join(".");
    let field = p.segments.get(at + 1).map_or("", |s| s.name.as_str());
    Some((part, field.to_string()))
}

/// Every part whose `.place` the expression names, with the field.
fn places_in(e: &Expr, out: &mut Vec<(String, String)>) {
    match e {
        Expr::Path(p) => out.extend(place_of(p)),
        Expr::Neg(inner, _) | Expr::Paren(inner, _) => places_in(inner, out),
        Expr::Binary { lhs, rhs, .. } => {
            places_in(lhs, out);
            places_in(rhs, out);
        }
        Expr::Range { lo, hi, .. } => {
            for end in [lo, hi].into_iter().flatten() {
                places_in(end, out);
            }
        }
        Expr::Tolerance { center, tol, .. } => {
            places_in(center, out);
            places_in(tol, out);
        }
        Expr::Call { args, .. } => args.iter().for_each(|a| places_in(a, out)),
        Expr::Steps { steps, .. } => steps.iter().for_each(|s| {
            places_in(&s.value, out);
            places_in(&s.to, out);
        }),
        Expr::SupplySteps { rows, .. } => rows.iter().for_each(|r| {
            places_in(&r.value, out);
            places_in(&r.at, out);
        }),
        Expr::Number(..) | Expr::Str(..) | Expr::Variant { .. } => {}
    }
}

/// A length as the editor states it: millimetres to a tenth of a micrometre, `12.5mm`.
fn length(v: f64) -> String {
    let v = (v * 1e4).round() / 1e4;
    let v = if v == 0.0 { 0.0 } else { v };
    format!("{v}mm")
}

/// A turn as the editor states it: whole degrees from 0 to 359 where it is one, `90`.
fn turn(v: f64) -> String {
    let v = ((v * 1e4).round() / 1e4).rem_euclid(360.0);
    let v = if v == 0.0 { 0.0 } else { v };
    format!("{v}")
}

fn value_text(field: &str, place: &RequiredPlace) -> Option<String> {
    match field {
        "x" => place.x.map(length),
        "y" => place.y.map(length),
        "rotation" => place.rotation.map(turn),
        "side" => place.side.map(|s| match s {
            PcbSide::Top => "\"top\"".to_string(),
            PcbSide::Bottom => "\"bottom\"".to_string(),
        }),
        _ => None,
    }
}

/// Whether two places state the same fields with the same values, as the file keeps them.
pub(crate) fn same(a: &RequiredPlace, b: &RequiredPlace) -> bool {
    FIELDS
        .into_iter()
        .all(|f| value_text(f, a) == value_text(f, b))
}

/// The physical file after the part `path` changes from `before` to `after`, from the file as it
/// is (`old`, `None` if there is none; `file` names it in a message). Refused with 409 if the
/// file does not state `before` now, and with 400 if it states the part's place in a shape the
/// editor does not write.
pub(crate) fn rewrite(
    old: Option<&str>,
    board: &str,
    file: &str,
    path: &str,
    before: &RequiredPlace,
    after: &RequiredPlace,
) -> Result<String, ApiError> {
    let now = old
        .map(|t| scan(t, board, file).remove(path).unwrap_or_default())
        .unwrap_or_default();
    if let Some(stated) = &now.elsewhere {
        return Err(ApiError::bad_request(format!(
            "{file} states where `{path}` goes in a shape the editor does not write, {stated}: change it there"
        )));
    }
    if !same(&now.place, before) {
        return Err(ApiError::conflict(format!(
            "the requirements on where `{path}` goes are not what the page last saw (the file changed, or the page is out of date): reload the board"
        )));
    }
    for (field, v) in [("x", after.x), ("y", after.y), ("rotation", after.rotation)] {
        if v.is_some_and(|v| !v.is_finite()) {
            return Err(ApiError::bad_request(format!(
                "the place of `{path}` has a {field} that is not a number"
            )));
        }
    }
    // In place, from the end of the file, so that each span still points at its statement.
    let mut text = old.map(str::to_string);
    let mut spans: Vec<(&str, Span, Span)> = now
        .spans
        .iter()
        .map(|(f, (stmt, rhs))| (*f, *stmt, *rhs))
        .collect();
    spans.sort_by_key(|(_, stmt, _)| std::cmp::Reverse(stmt.start));
    if let Some(text) = text.as_mut() {
        for (field, stmt, rhs) in spans {
            match value_text(field, after) {
                Some(v) => text.replace_range(rhs.start..rhs.end, &v),
                None => {
                    let (from, to) = whole_lines(text, stmt);
                    text.replace_range(from..to, "");
                }
            }
        }
    }
    let added: String = FIELDS
        .into_iter()
        .filter(|f| !now.spans.contains_key(f))
        .filter_map(|f| Some(format!(".{path}.place.{f} &= {}\n", value_text(f, after)?)))
        .collect();
    if added.is_empty() {
        return Ok(text.unwrap_or_default());
    }
    netsmith_fabricate::physical::edit(
        text.as_deref(),
        &Edit {
            board,
            section: Section::Unit,
            remove: &[],
            families: &[],
            text: &added,
            imports: &[],
        },
    )
    .map_err(|e| ApiError::bad_request(format!("{file}: {e}")))
}

/// The lines a statement stands on, with a comment after it on its last line, when nothing else
/// is on them; else the statement alone.
fn whole_lines(text: &str, span: Span) -> (usize, usize) {
    let from = text[..span.start].rfind('\n').map_or(0, |i| i + 1);
    let to = text[span.end..]
        .find('\n')
        .map_or(text.len(), |i| span.end + i + 1);
    let after = text[span.end..to].trim();
    let alone =
        text[from..span.start].trim().is_empty() && (after.is_empty() || after.starts_with("//"));
    if !alone {
        return (span.start, span.end);
    }
    // No blank line left above the `}` that closes the unit, and one, not two, between others.
    let before = text[..from.saturating_sub(1)]
        .rfind('\n')
        .map_or(0, |i| i + 1);
    let blank_before = from > 0 && text[before..from].trim().is_empty();
    let next = text[to..].lines().next().unwrap_or("");
    if blank_before && next.trim_start().starts_with('}') {
        (before, to)
    } else if blank_before && next.trim().is_empty() && to < text.len() {
        (from, (to + next.len() + 1).min(text.len()))
    } else {
        (from, to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn here(x: f64, y: f64, rotation: f64, side: PcbSide) -> RequiredPlace {
        RequiredPlace {
            x: Some(x),
            y: Some(y),
            rotation: Some(rotation),
            side: Some(side),
        }
    }

    const FILE: &str = "main.physical.nsm";

    #[test]
    fn require_here_makes_the_file_and_reads_back_the_same() {
        let after = here(12.345_67, 4.0, 90.0, PcbSide::Top);
        let text = rewrite(None, "B", FILE, "usb", &RequiredPlace::default(), &after).unwrap();
        assert_eq!(
            text,
            "//! The physical intent of B.\n\nphysical for B {\n    .usb.place.x &= 12.3457mm\n    .usb.place.y &= 4mm\n    .usb.place.rotation &= 90\n    .usb.place.side &= \"top\"\n}\n"
        );
        let s = &scan(&text, "B", FILE)["usb"];
        assert!(same(&s.place, &after) && s.elsewhere.is_none());
    }

    #[test]
    fn a_change_rewrites_the_numbers_where_they_stand_and_keeps_the_rest() {
        let old = "import std::pcb::Outline\n\nphysical for B {\n    case {\n        .outline: Outline { .width &= 40mm }\n    }\n\n    // Where the case wants the switch.\n    .sw.place.x &= 3mm // from the drawing\n    .sw.place.y &= 4mm\n    .other.place.left &= 1mm +/- 0.1mm\n    .sw.place.rotation &= 0\n    .sw.place.side &= \"top\"\n}\n";
        let before = here(3.0, 4.0, 0.0, PcbSide::Top);
        let after = here(5.5, 4.0, 270.0, PcbSide::Bottom);
        let text = rewrite(Some(old), "B", FILE, "sw", &before, &after).unwrap();
        assert_eq!(
            text,
            old.replace("x &= 3mm", "x &= 5.5mm")
                .replace("rotation &= 0", "rotation &= 270")
                .replace("side &= \"top\"", "side &= \"bottom\"")
        );
        // Taken out with its line, and the comments and the other part's requirement stay; then
        // put back.
        let none = RequiredPlace::default();
        let gone = rewrite(Some(&text), "B", FILE, "sw", &after, &none).unwrap();
        assert_eq!(
            gone,
            "import std::pcb::Outline\n\nphysical for B {\n    case {\n        .outline: Outline { .width &= 40mm }\n    }\n\n    // Where the case wants the switch.\n    .other.place.left &= 1mm +/- 0.1mm\n}\n"
        );
        let back = rewrite(Some(&gone), "B", FILE, "sw", &none, &before).unwrap();
        assert!(back.ends_with("    .other.place.left &= 1mm +/- 0.1mm\n\n    .sw.place.x &= 3mm\n    .sw.place.y &= 4mm\n    .sw.place.rotation &= 0\n    .sw.place.side &= \"top\"\n}\n"), "{back}");
        // Taken out again, the file is as it was before: no blank line is left above the `}`.
        assert_eq!(
            rewrite(Some(&back), "B", FILE, "sw", &before, &none).unwrap(),
            gone
        );
    }

    #[test]
    fn a_place_stated_in_another_shape_is_not_the_editors() {
        let text = "physical for B {\n    .a.place.left &= 1mm +/- 0.1mm\n    .b.place.x &= 2mm\n    .b.place.x &= 2mm\n    .c.place.x >= 1mm\n    .d.place.x - .e.place.x == 5mm\n    .f.place.x &= .outline.width / 2\n    .g.place.x &= 1mm\n    .g.place.side &= \"bottom\"\n    .m.n.place.y &= 0mm\n    .place.side &= \"top\"\n}\n";
        let have = scan(text, "B", FILE);
        let why = |p: &str| have[p].elsewhere.clone().unwrap_or_default();
        assert_eq!(
            why("a"),
            "`.a.place.left &= 1mm +/- 0.1mm` (main.physical.nsm:2)"
        );
        assert_eq!(why("b"), "`.b.place.x &= 2mm` (main.physical.nsm:4)");
        assert!(why("c").contains(">= 1mm"));
        assert!(why("d").contains("== 5mm") && why("e").contains("== 5mm"));
        assert!(why("f").contains(".outline.width / 2"));
        assert!(have["g"].elsewhere.is_none());
        assert_eq!(have["g"].place.x, Some(1.0));
        assert_eq!(have["g"].place.side, Some(PcbSide::Bottom));
        assert_eq!(have["m.n"].place.y, Some(0.0));
        assert!(!have.contains_key(""), "the board's own face names no part");
        let refused = rewrite(
            Some(text),
            "B",
            FILE,
            "a",
            &RequiredPlace::default(),
            &here(1.0, 1.0, 0.0, PcbSide::Top),
        )
        .unwrap_err();
        assert!(refused.message.contains("change it there"), "{refused:?}");
    }

    #[test]
    fn a_change_from_a_place_the_file_no_longer_states_is_refused() {
        let text = "physical for B {\n    .g.place.x &= 1mm\n}\n";
        let err = rewrite(
            Some(text),
            "B",
            FILE,
            "g",
            &RequiredPlace::default(),
            &here(1.0, 1.0, 0.0, PcbSide::Top),
        )
        .unwrap_err();
        assert!(err.message.contains("reload the board"), "{err:?}");
    }
}
