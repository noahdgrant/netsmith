//! The picks file that netsmith no longer reads: `<stem>.picks.toml` (ADR 0122).
//!
//! A hand pick is a narrowing in the source (`.c1.lcsc &= "C307331"`), so the file that once held
//! them beside a source file is refused, and the message says what to write in its place: one
//! narrowing for each entry, in the body of its board, before the file is deleted. Ignoring the
//! file would let its parts move to other rows with no word.

use std::path::{Path, PathBuf};

/// The picks file of a source file: `led.nsm` had `led.picks.toml`.
pub fn picks_path(source: &Path) -> PathBuf {
    source.with_extension("picks.toml")
}

/// An error when the source at `source` still has a picks file beside it, saying how to move its
/// entries into the source. `Ok` when there is none.
pub fn refuse(source: &Path) -> Result<(), String> {
    let file = picks_path(source);
    match std::fs::read_to_string(&file) {
        Ok(text) => Err(refusal(&text, &file.display().to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("error: {}: {e}", file.display())),
    }
}

/// The message for a picks file named `name` with the text `text`: the narrowings to write for
/// the entries it could read, by board, in the order board, part path.
pub fn refusal(text: &str, name: &str) -> String {
    let mut message = format!(
        "error: {name}: netsmith no longer reads a picks file (ADR 0122): a hand pick is a narrowing in the source. Write each entry as a narrowing in the body of its board, then delete the file"
    );
    let boards = moved(text);
    if boards.is_empty() {
        message.push_str(
            ": an entry `[Board.parts.\"c1\"]` with `lcsc = \"C307331\"` is `.c1.lcsc &= \"C307331\"` in `board Board { ... }`, and one with `mpn` is `.c1.mpn &= ...`",
        );
        return message;
    }
    message.push(':');
    for (board, lines) in boards {
        message.push_str(&format!("\n  in `board {board}`:"));
        for line in lines {
            message.push_str(&format!("\n    {line}"));
        }
    }
    message
}

/// The narrowings that replace the entries of the file, by board: `.boot1_cap.lcsc &=
/// "C307331"`. An entry that names no number, or text that is not TOML, gives none.
fn moved(text: &str) -> Vec<(String, Vec<String>)> {
    let Ok(table) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let mut boards = Vec::new();
    for (board, value) in &table {
        let Some(parts) = value
            .as_table()
            .and_then(|t| t.get("parts"))
            .and_then(toml::Value::as_table)
        else {
            continue;
        };
        let mut lines = Vec::new();
        for (path, entry) in parts {
            for key in ["lcsc", "mpn"] {
                if let Some(number) = entry.get(key).and_then(toml::Value::as_str) {
                    lines.push(format!(".{path}.{key} &= \"{number}\""));
                }
            }
        }
        if !lines.is_empty() {
            boards.push((board.clone(), lines));
        }
    }
    boards
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_entry_becomes_a_narrowing_of_its_board() {
        let said = refusal(
            "version = 1\n[B.parts.\"c5\"]\ncatalog = \"capacitor\"\nlcsc = \"C307331\"\n[B.parts.\"stage.c1\"]\ncatalog = \"mine\"\nmpn = \"X-1\"\n",
            "a.picks.toml",
        );
        assert_eq!(
            said,
            "error: a.picks.toml: netsmith no longer reads a picks file (ADR 0122): a hand pick is a narrowing in the source. Write each entry as a narrowing in the body of its board, then delete the file:\n  in `board B`:\n    .c5.lcsc &= \"C307331\"\n    .stage.c1.mpn &= \"X-1\""
        );
    }

    #[test]
    fn a_file_it_cannot_read_still_says_how() {
        let said = refusal("not toml [", "a.picks.toml");
        assert!(
            said.contains("no longer reads a picks file")
                && said.contains("is `.c1.lcsc &= \"C307331\"` in `board Board { ... }`"),
            "{said}"
        );
    }

    #[test]
    fn no_file_is_no_error() {
        let dir = netsmith_testkit::TempDir::new("picks-none");
        assert_eq!(refuse(&dir.join("main.nsm")), Ok(()));
        std::fs::write(dir.join("main.picks.toml"), "version = 1\n").unwrap();
        let said = refuse(&dir.join("main.nsm")).unwrap_err();
        assert!(
            said.contains("main.picks.toml: netsmith no longer reads"),
            "{said}"
        );
    }
}
