//! Change one entry of a pin file's text in place (ADR 0101), for a program that must not
//! rewrite the file: the editor's padlock and its move of a request to another pin.
//!
//! [`PinFile::to_text`] writes the whole file the same way every time, which is right for a run
//! that picks pins and wrong for a person's edit: it would drop their comments and reorder their
//! entries. [`edit_entry`] changes only the `pin`, `function`, `lock` and `source` lines of the
//! entry that differ from what is asked, and adds the entry (after the last of its board) when
//! the file has none. Every other byte of the file stays as it was, comments and spacing
//! included. The result is read again, and refused unless it says exactly the entries the file
//! said, with this one changed: a form of the file it cannot edit safely is never written.

use crate::pinfile::{PinEntry, PinFile, VERSION, toml_key, toml_string};

/// The text of the pin file `text` with the entry of `key` in the table of `board` saying `to`.
///
/// An entry the file has is changed line by line: a `pin`, `function` or `source` line that
/// already says it is left alone, a `lock` line is added when `to.lock` is true and removed
/// when it is false (a run writes `lock` only when it is true). An entry the file does not
/// have is written after the last entry of its board, or at the end, as a run would write it.
/// An empty file, or one of comments, is given `version = 1` first.
///
/// Fails, with a message a person can act on, when the file does not read, or when the entry is
/// written in a form this does not change (not a `[board.pins.key]` table).
pub fn edit_entry(text: &str, board: &str, key: &str, to: &PinEntry) -> Result<String, String> {
    let file = PinFile::parse(text).map_err(|errors| {
        let first = errors.first().map_or("", |e| e.message.as_str());
        format!("the pin file does not read ({first}); mend it by hand first")
    })?;
    let mut want = file.clone();
    want.boards
        .entry(board.to_string())
        .or_default()
        .insert(key.to_string(), to.clone());
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_string).collect();
    match file.board(board).and_then(|b| b.get(key)) {
        Some(old) => change(&mut lines, board, key, old, to, eol)?,
        None => add(&mut lines, &file, board, key, to, eol),
    }
    let out = lines.concat();
    match PinFile::parse(&out) {
        Ok(got) if got == want => Ok(out),
        _ => Err(format!(
            "the pin file has `{key}` in a form the editor cannot change without risk to the other lines; change it in the file"
        )),
    }
}

/// The index of the line that holds the byte `offset`.
fn line_at(lines: &[String], offset: usize) -> usize {
    let mut start = 0;
    for (i, l) in lines.iter().enumerate() {
        start += l.len();
        if offset < start {
            return i;
        }
    }
    lines.len().saturating_sub(1)
}

/// Whether the line opens a table (`[a.b.c]`), the end of the one before it.
fn is_header(line: &str) -> bool {
    line.trim_start().starts_with('[')
}

/// The end (exclusive) of the table whose header is at `header`.
fn table_end(lines: &[String], header: usize) -> usize {
    (header + 1..lines.len())
        .find(|&i| is_header(&lines[i]))
        .unwrap_or(lines.len())
}

/// The key a `key = value` line sets, if it is one.
fn key_of(line: &str) -> Option<&str> {
    let t = line.trim_start();
    if t.starts_with('#') || t.starts_with('[') {
        return None;
    }
    t.split_once('=').map(|(k, _)| k.trim())
}

/// The line of `name` in the table whose body is `from..to`.
fn find(lines: &[String], from: usize, to: usize, name: &str) -> Option<usize> {
    (from..to).find(|&i| key_of(&lines[i]) == Some(name))
}

/// The last line of the body that sets a key: where a new line of the entry goes.
fn last_key(lines: &[String], header: usize) -> usize {
    let end = table_end(lines, header);
    (header + 1..end)
        .rev()
        .find(|&i| key_of(&lines[i]).is_some())
        .unwrap_or(header)
}

/// The length of the value that starts `rest`: a string in quotes, or a word.
fn value_len(rest: &str) -> Option<usize> {
    match rest.chars().next()? {
        '"' => {
            let mut escaped = false;
            for (i, c) in rest.char_indices().skip(1) {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    return Some(i + 1);
                }
            }
            None
        }
        '\'' => rest[1..].find('\'').map(|i| i + 2),
        _ => Some(
            rest.find(|c: char| c.is_whitespace() || c == '#')
                .unwrap_or(rest.len()),
        ),
    }
}

/// The line with the value after its `=` replaced, keeping the indentation, the spacing and a
/// comment after it.
fn with_value(line: &str, value: &str) -> Option<String> {
    let eq = line.find('=')?;
    let after = &line[eq + 1..];
    let start = eq + 1 + (after.len() - after.trim_start_matches([' ', '\t']).len());
    let rest = &line[start..];
    let len = value_len(rest)?;
    Some(format!("{}{value}{}", &line[..start], &rest[len..]))
}

/// Put `new` on its own line after the line at `at`.
fn insert_after(lines: &mut Vec<String>, at: usize, new: String, eol: &str) {
    if !lines[at].ends_with('\n') {
        lines[at].push_str(eol);
    }
    lines.insert(at + 1, new);
}

fn change(
    lines: &mut Vec<String>,
    board: &str,
    key: &str,
    old: &PinEntry,
    to: &PinEntry,
    eol: &str,
) -> Result<(), String> {
    let cannot = || {
        format!(
            "the pin file has `{key}` in a form the editor cannot change: it is not a table `[{board}.pins.{key}]`; change it in the file"
        )
    };
    let header = old
        .at
        .map(|s| line_at(lines, s.start))
        .filter(|&i| is_header(&lines[i]) && !lines[i].trim_start().starts_with("[["))
        .ok_or_else(cannot)?;
    let end = table_end(lines, header);
    let replace = |lines: &mut Vec<String>, name: &str, value: String| -> Result<(), String> {
        let i = find(lines, header + 1, end, name).ok_or_else(cannot)?;
        lines[i] = with_value(&lines[i], &value).ok_or_else(cannot)?;
        Ok(())
    };
    if old.pin != to.pin {
        replace(lines, "pin", toml_string(&to.pin))?;
    }
    if old.function != to.function {
        replace(lines, "function", toml_string(&to.function))?;
    }
    let mut source_missing = false;
    if old.source != to.source {
        match find(lines, header + 1, end, "source") {
            Some(_) => replace(lines, "source", toml_string(to.source.word()))?,
            None => source_missing = true,
        }
    }
    let mut lock_missing = false;
    if old.lock != to.lock {
        match (find(lines, header + 1, end, "lock"), to.lock) {
            (Some(_), true) => replace(lines, "lock", "true".to_string())?,
            (Some(i), false) => {
                lines.remove(i);
            }
            (None, true) => lock_missing = true,
            (None, false) => {}
        }
    }
    // Added last, so that no index above moved under another change. The end is read again.
    if lock_missing {
        let end = table_end(lines, header);
        let after = find(lines, header + 1, end, "function")
            .or_else(|| find(lines, header + 1, end, "pin"))
            .ok_or_else(cannot)?;
        insert_after(lines, after, format!("lock = true{eol}"), eol);
    }
    if source_missing {
        let after = last_key(lines, header);
        insert_after(
            lines,
            after,
            format!("source = {}{eol}", toml_string(to.source.word())),
            eol,
        );
    }
    Ok(())
}

fn add(lines: &mut Vec<String>, file: &PinFile, board: &str, key: &str, to: &PinEntry, eol: &str) {
    let mut block = vec![
        eol.to_string(),
        format!("[{}.pins.{}]{eol}", toml_key(board), toml_key(key)),
        format!("pin = {}{eol}", toml_string(&to.pin)),
        format!("function = {}{eol}", toml_string(&to.function)),
    ];
    if to.lock {
        block.push(format!("lock = true{eol}"));
    }
    block.push(format!("source = {}{eol}", toml_string(to.source.word())));
    let has_content = lines.iter().any(|l| {
        let t = l.trim();
        !t.is_empty() && !t.starts_with('#')
    });
    if !has_content {
        // A comment-only file may end without a line end: the comment keeps its text and gets one.
        if let Some(last) = lines.last_mut()
            && !last.ends_with('\n')
        {
            last.push_str(eol);
        }
        lines.push(format!("version = {VERSION}{eol}"));
    }
    // After the last entry of the board, so that its entries stay together; else at the end.
    let anchor = file
        .board(board)
        .into_iter()
        .flatten()
        .filter_map(|(_, e)| e.at)
        .map(|s| line_at(lines, s.start))
        .filter(|&i| is_header(&lines[i]))
        .max();
    let after = match anchor {
        Some(header) => last_key(lines, header),
        None => lines.len().saturating_sub(1),
    };
    if !lines[after].ends_with('\n') {
        lines[after].push_str(eol);
    }
    for (n, l) in block.into_iter().enumerate() {
        lines.insert(after + 1 + n, l);
    }
}
