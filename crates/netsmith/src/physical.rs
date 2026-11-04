//! The physical file beside a source file (ADR 0048): `<stem>.physical.nsm`.
//!
//! It is optional. `build`, `fab`, the orders and the editor give it to the checker with the
//! source, which puts each unit `physical for Board` with that board, as the layout file
//! (`layout.rs`) is found by stem.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use netsmith_check::{Loader, physical_file_name};

/// The physical file of a source file: `led.nsm` has `led.physical.nsm`.
pub fn physical_path(source: &Path) -> PathBuf {
    source.with_extension("physical.nsm")
}

/// Whether a file name is a physical file's: `led.physical.nsm`. The project's module scan
/// passes over such files, as they are not importable modules.
pub fn is_physical_file(file_name: &str) -> bool {
    file_name.ends_with(".physical.nsm")
}

/// Pair the physical file beside `path` with the source called `name` in `loader`, if the
/// file exists. `unsaved` holds the text an editor has not saved yet. A file that exists and
/// cannot be read is an error naming it.
pub fn attach(
    loader: &mut Loader,
    name: &str,
    path: &Path,
    unsaved: &HashMap<PathBuf, String>,
) -> Result<(), String> {
    let file = physical_path(path);
    let text = match unsaved.get(&file) {
        Some(text) => text.clone(),
        None => match fs::read_to_string(&file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("{}: {e}", file.display())),
        },
    };
    loader.attach_physical(name, &physical_file_name(name), &text);
    Ok(())
}

// ---- writing the file ------------------------------------------------------------------

/// Where in a unit `physical for Board` an edit puts its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// The `case { }` section: the outline, cutouts, holes, fiducials and labels.
    Case,
    /// The unit itself, outside `case`: a keepout, a relation.
    Unit,
}

/// One edit of a board's physical file (ADR 0048, ADR 0045 section 4): the statements about
/// some members are taken out and new text is put in. The edit is made on the text, at the
/// spans of the syntax tree, so every comment and every line it does not name stays as it
/// was.
#[derive(Debug, Clone)]
pub struct Edit<'a> {
    pub board: &'a str,
    pub section: Section,
    /// Members whose declaration, and every statement that starts with the member's name
    /// (`.hole1.place.x`), is taken out first. The `///` lines above a declaration go with it.
    pub remove: &'a [String],
    /// The statements to add at the end of the section, at column 0; they are indented to
    /// fit. Empty to only remove.
    pub text: &'a str,
    /// `(module, name)` pairs to import if the file does not import the name yet
    /// (`("std::pcb", "Outline")`).
    pub imports: &'a [(&'a str, &'a str)],
}

/// The physical file after `edit`, from the file as it is (`None` if there is none). A file
/// that does not parse is an error naming the problem, and nothing is made of it.
pub fn edit(old: Option<&str>, edit: &Edit) -> Result<String, String> {
    let (case_text, body_text) = match edit.section {
        Section::Case => (edit.text, ""),
        Section::Unit => ("", edit.text),
    };
    let Some(old) = old else {
        let imports = missing_imports(edit, &[]);
        let sep = if imports.is_empty() { "" } else { "\n" };
        return Ok(format!(
            "//! The physical intent of {}.\n\n{imports}{sep}{}",
            edit.board,
            unit_text(edit.board, case_text, body_text)
        ));
    };
    let parse =
        |text: &str| netsmith_lang::parse(text).map_err(|e| e.render("the physical file", text));
    // First the old statements go, then the new ones come, then the imports they need. Each
    // step reads the text the step before made, so a blank line is decided by the lines round it.
    let text = remove_members(old, &parse(old)?, edit);
    let text = add_text(&text, &parse(&text)?, edit, case_text, body_text);
    let text = add_imports(&text, &parse(&text)?, edit);
    parse(&text)
        .map_err(|e| format!("the edit of the physical file does not parse (a bug): {e}"))?;
    Ok(text)
}

/// `import` lines for the names of `edit.imports` that `have` does not hold.
fn missing_imports(edit: &Edit, have: &[String]) -> String {
    edit.imports
        .iter()
        .filter(|(_, name)| !have.iter().any(|h| h == name))
        .map(|(module, name)| format!("import {module}::{name}\n"))
        .collect()
}

fn indented(text: &str, by: &str) -> String {
    text.lines()
        .map(|l| {
            if l.is_empty() {
                "\n".to_string()
            } else {
                format!("{by}{l}\n")
            }
        })
        .collect()
}

fn unit_text(board: &str, case_text: &str, body: &str) -> String {
    let mut unit = format!("physical for {board} {{\n");
    if !case_text.is_empty() {
        unit.push_str(&format!(
            "    case {{\n{}    }}\n",
            indented(case_text, "        ")
        ));
    }
    unit.push_str(&indented(body, "    "));
    unit.push_str("}\n");
    unit
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |i| i + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |i| at + i + 1)
}

/// The spaces a line starts with, for the line `at` is on.
fn indent_at(text: &str, at: usize) -> String {
    text[line_start(text, at)..]
        .chars()
        .take_while(|c| *c == ' ')
        .collect()
}

fn remove_members(old: &str, file: &netsmith_lang::ast::File, edit: &Edit) -> String {
    use netsmith_lang::ast::{Expr, Item};
    let Some(unit) = file.physicals.iter().find(|u| u.board.name == edit.board) else {
        return old.to_string();
    };
    let named = |name: &str| edit.remove.iter().any(|r| r == name);
    // A statement's range: whole lines, the `///` lines right above it, and a blank line
    // after it if one stands on both sides (so that removing leaves one, not two).
    let range_of = |start: usize, end: usize| -> (usize, usize) {
        let mut from = line_start(old, start);
        while from > 0 {
            let before = line_start(old, from - 1);
            if old[before..from].trim_start().starts_with("///") {
                from = before;
            } else {
                break;
            }
        }
        let mut to = line_end(old, end.saturating_sub(1).max(start));
        let blank_before = from == 0 || old[line_start(old, from - 1)..from].trim().is_empty();
        let blank_after = old[to..]
            .lines()
            .next()
            .is_some_and(|l| l.trim().is_empty());
        if blank_before && blank_after {
            to = line_end(old, to);
        }
        (from, to)
    };
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut visit = |items: &[Item]| {
        for item in items {
            match item {
                Item::Decl(d)
                    if d.name.leading_dot
                        && d.name.segments.len() == 1
                        && named(&d.name.segments[0].name) =>
                {
                    ranges.push(range_of(d.span.start, d.span.end));
                }
                Item::Constraint(c) => {
                    let first = match &c.lhs {
                        Expr::Path(p) if p.leading_dot => p.segments.first(),
                        _ => None,
                    };
                    if first.is_some_and(|s| named(&s.name)) {
                        ranges.push(range_of(c.span.start, c.span.end));
                    }
                }
                _ => {}
            }
        }
    };
    visit(&unit.items);
    for item in &unit.items {
        if let Item::Case(case) = item {
            visit(&case.items);
        }
    }
    ranges.sort_by_key(|r| std::cmp::Reverse(*r));
    let mut text = old.to_string();
    for (a, b) in ranges {
        text.replace_range(a..b, "");
    }
    text
}

fn add_text(
    old: &str,
    file: &netsmith_lang::ast::File,
    edit: &Edit,
    case_text: &str,
    body_text: &str,
) -> String {
    use netsmith_lang::ast::Item;
    let mut text = old.to_string();
    if case_text.is_empty() && body_text.is_empty() {
        return text;
    }
    // A blank line between the lines before and the new ones, unless one is there or the
    // section has just opened.
    let separated = |at: usize, new: String| -> String {
        let before = old[..at].lines().last().unwrap_or("");
        if before.trim().is_empty() || before.trim_end().ends_with('{') {
            new
        } else {
            format!("\n{new}")
        }
    };
    let Some(unit) = file.physicals.iter().find(|u| u.board.name == edit.board) else {
        let sep = if old.ends_with('\n') || old.is_empty() {
            "\n"
        } else {
            "\n\n"
        };
        text.push_str(&format!(
            "{sep}{}",
            unit_text(edit.board, case_text, body_text)
        ));
        return text;
    };
    let case = unit.items.iter().find_map(|i| match i {
        Item::Case(c) => Some(c),
        _ => None,
    });
    let unit_indent = indent_at(old, unit.span.start);
    let (at, new) = match (edit.section, case) {
        (Section::Case, Some(case)) => {
            let at = line_start(old, case.span.end - 1);
            let indent = indent_at(old, case.span.start);
            (at, indented(case_text, &format!("{indent}    ")))
        }
        (Section::Case, None) => {
            let at = line_start(old, unit.span.end - 1);
            (
                at,
                format!(
                    "{unit_indent}    case {{\n{}{unit_indent}    }}\n",
                    indented(case_text, &format!("{unit_indent}        "))
                ),
            )
        }
        (Section::Unit, _) => {
            let at = line_start(old, unit.span.end - 1);
            (at, indented(body_text, &format!("{unit_indent}    ")))
        }
    };
    text.insert_str(at, &separated(at, new));
    text
}

fn add_imports(old: &str, file: &netsmith_lang::ast::File, edit: &Edit) -> String {
    let have: Vec<String> = file
        .imports
        .iter()
        .flat_map(|i| match &i.names {
            Some(names) => names.iter().map(|n| n.name.clone()).collect::<Vec<_>>(),
            None => i.path.last().map(|n| n.name.clone()).into_iter().collect(),
        })
        .collect();
    let imports = missing_imports(edit, &have);
    if imports.is_empty() {
        return old.to_string();
    }
    // After the last import, or after the leading `//!` lines.
    let (at, lead, trail) = match file.imports.last() {
        Some(last) => (
            line_end(old, last.span.end.saturating_sub(1).max(last.span.start)),
            "",
            "",
        ),
        None => {
            let mut at = 0;
            for line in old.split_inclusive('\n') {
                if line.trim_start().starts_with("//!") {
                    at += line.len();
                } else {
                    break;
                }
            }
            (at, if at > 0 { "\n" } else { "" }, "\n")
        }
    };
    let mut text = old.to_string();
    text.insert_str(at, &format!("{lead}{imports}{trail}"));
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(old: Option<&str>, section: Section, remove: &[&str], text: &str) -> String {
        let remove: Vec<String> = remove.iter().map(|s| s.to_string()).collect();
        edit(
            old,
            &Edit {
                board: "B",
                section,
                remove: &remove,
                text,
                imports: &[("std::pcb", "Outline"), ("std::pcb", "Keepout")],
            },
        )
        .unwrap()
    }

    #[test]
    fn a_new_file_has_its_imports_and_its_unit() {
        let text = run(
            None,
            Section::Case,
            &[],
            ".outline: Outline {\n    .width &= 4mm\n}\n",
        );
        assert_eq!(
            text,
            "//! The physical intent of B.\n\nimport std::pcb::Outline\nimport std::pcb::Keepout\n\nphysical for B {\n    case {\n        .outline: Outline {\n            .width &= 4mm\n        }\n    }\n}\n"
        );
        netsmith_lang::parse(&text).unwrap();
    }

    #[test]
    fn comments_and_other_lines_survive_and_the_named_members_are_replaced() {
        let old = "//! Case TC-12.\nimport std::pcb::Outline\n\n// the unit\nphysical for B {\n    // the case\n    case {\n        /// the edge\n        .outline: Outline {\n            .width &= 4mm\n        }\n        // a hole, kept\n        .other: Outline { .width &= 1mm }\n    }\n    // radio row\n    .radio.a.place.left &= 8mm   // from sheet 1\n    .hole1.place.x &= 1mm\n}\n";
        let text = run(
            Some(old),
            Section::Case,
            &["outline", "hole1"],
            ".outline: Outline {\n    .width &= 5mm\n}\n",
        );
        assert_eq!(
            text,
            "//! Case TC-12.\nimport std::pcb::Outline\nimport std::pcb::Keepout\n\n// the unit\nphysical for B {\n    // the case\n    case {\n        // a hole, kept\n        .other: Outline { .width &= 1mm }\n\n        .outline: Outline {\n            .width &= 5mm\n        }\n    }\n    // radio row\n    .radio.a.place.left &= 8mm   // from sheet 1\n}\n"
        );
        // The same edit again changes nothing more.
        let again = run(
            Some(&text),
            Section::Case,
            &["outline", "hole1"],
            ".outline: Outline {\n    .width &= 5mm\n}\n",
        );
        assert_eq!(again, text);
    }

    #[test]
    fn a_unit_without_a_case_gets_one_and_a_file_without_the_unit_gets_the_unit() {
        let old = "import std::pcb::Outline\n\nphysical for B {\n    .a.place.x &= 1mm\n}\n";
        let text = run(
            Some(old),
            Section::Case,
            &[],
            ".outline: Outline { .width &= 4mm }\n",
        );
        assert!(
            text.contains("    case {\n        .outline: Outline { .width &= 4mm }\n    }\n}\n"),
            "{text}"
        );
        let other = "physical for A {\n}\n";
        let text = run(
            Some(other),
            Section::Unit,
            &[],
            ".k: Keepout { .width &= 1mm }\n",
        );
        assert_eq!(
            text,
            "import std::pcb::Outline\nimport std::pcb::Keepout\n\nphysical for A {\n}\n\nphysical for B {\n    .k: Keepout { .width &= 1mm }\n}\n"
        );
    }

    #[test]
    fn a_file_that_does_not_parse_is_an_error_and_not_touched() {
        let e = edit(
            Some("physical for B {\n    .a ?? \n}\n"),
            &Edit {
                board: "B",
                section: Section::Case,
                remove: &[],
                text: ".x: Outline\n",
                imports: &[],
            },
        )
        .unwrap_err();
        assert!(e.contains("physical file"), "{e}");
    }
}
