//! The comments of a layout file that a program writes again (issue #827).
//!
//! [`crate::Layout::to_text`] writes the same text for the same layout, which has no place for a
//! comment. A person's file has comments, so a writer that changes the file asks
//! [`crate::Layout::to_text_over`], which writes that text and then [`carry`]s the comments of
//! the file as it was into it.
//!
//! A comment belongs to a line that holds a key or a table header, an **item**, named by the
//! path of its key (`Demo`, `parts`, `"stage.cap"`, `at`; the deepest key that starts on the
//! line). A comment on a line of its own belongs to the next item, and stays above it; a
//! comment after a value or a header belongs to that line and stays after it. So a comment
//! follows its entry when the entries move (the text writes them in name order) and goes with
//! it when the entry goes. A comment of a key that is no longer written goes above its table's
//! header, where the table is still written; a comment of a table that has no header of its
//! own any more (the placer's text writes no `[Board]` line for a board with no seed) goes above
//! the first line of what is left of it. A comment at the end of the file stays at the end.
//!
//! Blank lines and the spacing of a line are the text's own, not the file's.

use std::collections::{BTreeMap, HashMap};

use toml::de::DeTable;

type Path = Vec<String>;

/// A line that holds a key or the header of a table.
struct Item {
    line: usize,
    path: Path,
    /// Whether the key holds a table (the line is a header, or a key of an inline table).
    table: bool,
}

/// What a file says around one item.
#[derive(Default)]
struct Notes {
    /// The comment lines above the item, as written, without their indentation.
    above: Vec<String>,
    /// The comment after the item on its line, with the spaces before it.
    after: Option<String>,
}

/// The text of `new` with the comments of `old` carried into it.
///
/// `new` and `old` are layout files. When `old` is not TOML there is nothing to carry.
pub fn carry(old: &str, new: &str) -> String {
    let (Some(old_items), Some(new_items)) = (items(old), items(new)) else {
        return new.to_string();
    };
    let (mut notes, tail) = gather(old, &old_items);
    let index: HashMap<&Path, usize> = new_items
        .iter()
        .enumerate()
        .map(|(i, item)| (&item.path, i))
        .collect();
    let mut into: Vec<Notes> = new_items.iter().map(|_| Notes::default()).collect();
    for item in &old_items {
        let Some(n) = notes.remove(&item.path) else {
            continue;
        };
        if let Some(&at) = index.get(&item.path) {
            into[at].above.extend(n.above);
            into[at].after = n.after;
            continue;
        }
        // The item is not written any more.
        let target = if item.table {
            new_items
                .iter()
                .position(|n| n.path.starts_with(&item.path))
        } else {
            item.path
                .split_last()
                .and_then(|(_, parent)| index.get(&parent.to_vec()).copied())
        };
        if let Some(at) = target {
            into[at].above.extend(n.above);
            into[at]
                .above
                .extend(n.after.map(|c| c.trim_start().to_string()));
        }
    }
    let at_line: HashMap<usize, usize> = new_items
        .iter()
        .enumerate()
        .map(|(i, item)| (item.line, i))
        .collect();
    let mut out = String::with_capacity(new.len());
    for (line, text) in new.split_inclusive('\n').enumerate() {
        let Some(&at) = at_line.get(&line) else {
            out.push_str(text);
            continue;
        };
        for above in &into[at].above {
            out.push_str(above);
            out.push('\n');
        }
        out.push_str(text.trim_end_matches('\n'));
        if let Some(after) = &into[at].after {
            out.push_str(after);
        }
        out.push('\n');
    }
    if !tail.is_empty() {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
        for line in tail {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

/// The items of `text`, in the order of their lines; `None` for a text that is not TOML.
fn items(text: &str) -> Option<Vec<Item>> {
    let document = DeTable::parse(text).ok()?;
    let starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let mut deepest: BTreeMap<usize, Item> = BTreeMap::new();
    walk(document.get_ref(), &mut Vec::new(), &starts, &mut deepest);
    Some(deepest.into_values().collect())
}

fn walk(table: &DeTable<'_>, prefix: &mut Path, starts: &[usize], out: &mut BTreeMap<usize, Item>) {
    for (key, value) in table.iter() {
        prefix.push(key.get_ref().to_string());
        let line = starts.partition_point(|&s| s <= key.span().start) - 1;
        let inner = value.get_ref().as_table();
        if out.get(&line).is_none_or(|i| i.path.len() < prefix.len()) {
            out.insert(
                line,
                Item {
                    line,
                    path: prefix.clone(),
                    table: inner.is_some(),
                },
            );
        }
        if let Some(inner) = inner {
            walk(inner, prefix, starts, out);
        }
        prefix.pop();
    }
}

/// The notes of each item of `text`, and the comment lines after the last item.
fn gather(text: &str, items: &[Item]) -> (HashMap<Path, Notes>, Vec<String>) {
    let mut notes: HashMap<Path, Notes> = HashMap::new();
    let mut tail = Vec::new();
    for hash in comment_starts(text) {
        let start = text[..hash].rfind('\n').map_or(0, |i| i + 1);
        let end = text[hash..].find('\n').map_or(text.len(), |i| hash + i);
        let line = text[..start].matches('\n').count();
        let comment = text[hash..end].trim_end();
        let before = &text[start..hash];
        let own = items.iter().find(|i| i.line == line);
        if let (Some(item), false) = (own, before.trim().is_empty()) {
            let gap = &before[before.trim_end().len()..];
            notes.entry(item.path.clone()).or_default().after = Some(format!("{gap}{comment}"));
            continue;
        }
        // On a line of its own, or after the rest of a value that runs over lines: above the
        // next item.
        match items.iter().find(|i| i.line > line) {
            Some(next) => notes
                .entry(next.path.clone())
                .or_default()
                .above
                .push(comment.to_string()),
            None => tail.push(comment.to_string()),
        }
    }
    (notes, tail)
}

/// The offset of each `#` that starts a comment: not one inside a string.
fn comment_starts(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut starts = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'#' => {
                starts.push(i);
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            quote @ (b'"' | b'\'') => i = after_string(text, i, quote),
            _ => i += 1,
        }
    }
    starts
}

/// The offset after the string that starts at `from` with `quote`.
fn after_string(text: &str, from: usize, quote: u8) -> usize {
    let bytes = text.as_bytes();
    let triple = [quote; 3];
    let long = bytes[from..].starts_with(&triple);
    let escapes = quote == b'"';
    let mut i = from + if long { 3 } else { 1 };
    while i < bytes.len() {
        if escapes && bytes[i] == b'\\' {
            i += 2;
        } else if long && bytes[i..].starts_with(&triple) {
            return i + 3;
        } else if bytes[i] == quote && !long {
            return i + 1;
        } else if bytes[i] == b'\n' && !long {
            return i;
        } else {
            i += 1;
        }
    }
    bytes.len()
}
