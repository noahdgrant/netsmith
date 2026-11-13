//! The derived tables of a port whose original is too large to commit (#560): instead of the
//! original's `.kicad_pcb`, `reference/` holds what the tests read from it, its netlist
//! (`netlist.xml`, KiCad's netlist XML) and its parts and pads (`placements.toml`), with
//! `source.toml`, which names the original: its URL at a pinned commit, its size and its sha256.
//!
//! `netsmith port tables` writes the two tables from the original, or checks the committed ones,
//! and holds the original against `source.toml`: a reviewer who fetches the file proves the
//! tables come from it.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use netsmith_netlist::Netlist;
use sha2::{Digest, Sha256};

use crate::reference_table::{MustMatch, placements_toml_with};

/// The file of the netlist in `reference/`.
pub const NETLIST: &str = "netlist.xml";
/// The file of the parts and pads in `reference/`.
pub const PLACEMENTS: &str = "placements.toml";
/// The file that names the original in `reference/`.
pub const SOURCE: &str = "source.toml";

/// What `source.toml` says of the original.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The original's file name, which the tables' headers give.
    pub file: String,
    /// Where the original is, at a pinned commit.
    pub url: String,
    /// Its size in bytes.
    pub size: u64,
    /// Its SHA-256, in lower-case hex.
    pub sha256: String,
}

/// The SHA-256 of `bytes` in lower-case hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A URL that names a commit: `https://`, with a path segment of 40 or 64 hex digits (a git
/// commit's hash, as GitHub's and GitLab's raw URLs have it), so the file it names cannot change.
pub fn check_url(url: &str) -> Result<(), String> {
    let Some(rest) = url.strip_prefix("https://") else {
        return Err(format!(
            "{url}: the URL of the original must start with https://"
        ));
    };
    let pinned = rest.split(['/', '?', '#']).any(|segment| {
        matches!(segment.len(), 40 | 64) && segment.chars().all(|c| c.is_ascii_hexdigit())
    });
    if pinned {
        Ok(())
    } else {
        Err(format!(
            "{url}: the URL must name the original at a pinned commit (a path segment of a \
             commit's 40 hex digits), not a branch, so that the file it names cannot change"
        ))
    }
}

impl Source {
    /// The source of `bytes`, the original named `file`, at `url`.
    pub fn of(bytes: &[u8], file: &str, url: &str) -> Source {
        Source {
            file: file.to_string(),
            url: url.to_string(),
            size: bytes.len() as u64,
            sha256: sha256_hex(bytes),
        }
    }

    /// `source.toml`: the keys `file`, `url`, `size` and `sha256`, no other. `name` is what a
    /// message calls it.
    pub fn parse(text: &str, name: &str) -> Result<Source, String> {
        let table: toml::Table = text.parse().map_err(|e| format!("{name}: {e}"))?;
        const KEYS: [&str; 4] = ["file", "url", "size", "sha256"];
        if let Some(extra) = table.keys().find(|k| !KEYS.contains(&k.as_str())) {
            return Err(format!(
                "{name}: `{extra}`; the file holds only {}",
                KEYS.join(", ")
            ));
        }
        let text = |key: &str| {
            table
                .get(key)
                .and_then(toml::Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("{name}: it has no `{key}` as text"))
        };
        let size = table
            .get("size")
            .and_then(toml::Value::as_integer)
            .and_then(|s| u64::try_from(s).ok())
            .ok_or_else(|| format!("{name}: it has no `size` in bytes"))?;
        let sha256 = text("sha256")?;
        if sha256.len() != 64 || !sha256.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
            return Err(format!(
                "{name}: `sha256` must be 64 lower-case hex digits, not {sha256:?}"
            ));
        }
        Ok(Source {
            file: text("file")?,
            url: text("url")?,
            size,
            sha256,
        })
    }

    /// The text of `source.toml`.
    pub fn to_toml(&self) -> String {
        let mut table = toml::Table::new();
        table.insert("file".into(), self.file.clone().into());
        table.insert("url".into(), self.url.clone().into());
        table.insert("size".into(), (self.size as i64).into());
        table.insert("sha256".into(), self.sha256.clone().into());
        format!(
            "# The original that netlist.xml and placements.toml are derived from, by\n\
             #   netsmith port tables <{}>\n\
             # (run in the project). The command checks the file against size and sha256.\n\n{}",
            self.file,
            toml::to_string(&table).expect("a table of text and an integer is TOML")
        )
    }
}

/// The two tables of an original.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tables {
    /// `netlist.xml`.
    pub netlist: String,
    /// `placements.toml`.
    pub placements: String,
    /// The components and the nets of the netlist.
    pub components: usize,
    pub nets: usize,
}

/// The tables of `pcb`, the text of the original `file`, with the must-match parts of `must`. The
/// netlist is the one [`Netlist::from_kicad_pcb`] reads, and the XML is read back here: a
/// netlist that the XML does not give back (a value with spaces around it) is an error.
pub fn derive(pcb: &str, file: &str, must: &MustMatch) -> Result<Tables, String> {
    let netlist = Netlist::from_kicad_pcb(pcb).map_err(|e| format!("{file}: {e}"))?;
    let xml = netlist.to_kicad_xml(file);
    let back = Netlist::from_kicad_xml(&xml).map_err(|e| format!("{file}: {e}"))?;
    if back != netlist {
        let differs = netlist
            .components
            .iter()
            .zip(&back.components)
            .find(|(a, b)| a != b)
            .map_or_else(
                || "a net".to_string(),
                |(a, _)| format!("component {}", a.refdes),
            );
        return Err(format!(
            "{file}: the netlist written as XML does not read back the same ({differs}); \
             a value or a footprint with spaces around it cannot be held as XML"
        ));
    }
    let header = format!(
        "<!-- The netlist of {file}, derived by `netsmith port tables` (see source.toml).\n     \
         Do not edit: run the command again. -->\n"
    );
    let netlist_xml = xml.replacen("<export", &format!("{header}<export"), 1);
    Ok(Tables {
        placements: placements_toml_with(pcb, file, must)?,
        netlist: netlist_xml,
        components: netlist.components.len(),
        nets: netlist.nets.len(),
    })
}

/// Why the tables were not written, or not current.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TablesError {
    /// What was checked failed: the original is not the file `source.toml` names, or a committed
    /// table is not what the original gives.
    Failed(String),
    /// The command could not run: a file cannot be read, no `--url` for a new `source.toml`.
    Cannot(String),
}

impl fmt::Display for TablesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Failed(m) | Self::Cannot(m) => write!(f, "{m}"),
        }
    }
}

/// What [`write`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub tables: Tables,
    pub source: Source,
    /// `source.toml` was written (it was not there).
    pub new_source: bool,
}

/// What [`write`] is asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Write the two tables, and `source.toml` when it is not there.
    Write,
    /// Write nothing: the committed tables must be what the original gives.
    Check,
}

/// Derive the tables of `original` into the folder `reference`, or check them (`mode`).
///
/// With `reference/source.toml` there, the original must have its size and sha256, and `url`,
/// when given, its URL; the tables name the original by its `file`. Without it, `url` is
/// needed (a pinned commit, [`check_url`]) and the file is written. The must-match parts are those
/// of `reference/must_match.toml` when there is one, as for `reference_table`.
pub fn write(
    original: &Path,
    reference: &Path,
    url: Option<&str>,
    mode: Mode,
) -> Result<Written, TablesError> {
    let cannot = |m: String| TablesError::Cannot(m);
    let bytes = fs::read(original).map_err(|e| cannot(format!("{}: {e}", original.display())))?;
    let source_path = reference.join(SOURCE);
    let existing = if source_path.exists() {
        let text = fs::read_to_string(&source_path)
            .map_err(|e| cannot(format!("{}: {e}", source_path.display())))?;
        Some(Source::parse(&text, &source_path.display().to_string()).map_err(cannot)?)
    } else {
        None
    };
    let (source, new_source) = match existing {
        Some(source) => {
            let found = Source::of(&bytes, &source.file, &source.url);
            if (found.size, &found.sha256) != (source.size, &source.sha256) {
                return Err(TablesError::Failed(format!(
                    "{} is not the original that {} names: it has {} bytes and sha256 {}, the \
                     original {} bytes and sha256 {} ({})",
                    original.display(),
                    source_path.display(),
                    found.size,
                    found.sha256,
                    source.size,
                    source.sha256,
                    source.url
                )));
            }
            if let Some(url) = url.filter(|u| *u != source.url) {
                return Err(TablesError::Failed(format!(
                    "--url {url} is not the URL of {}, {}; to name another original, remove \
                     the file",
                    source_path.display(),
                    source.url
                )));
            }
            (source, false)
        }
        None if mode == Mode::Check => {
            return Err(TablesError::Failed(format!(
                "{} is not there: the tables name no original",
                source_path.display()
            )));
        }
        None => {
            let url = url.ok_or_else(|| {
                cannot(format!(
                    "{} is not there: give the original's URL at a pinned commit with --url",
                    source_path.display()
                ))
            })?;
            check_url(url).map_err(cannot)?;
            let file = original.file_name().map_or_else(
                || original.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            (Source::of(&bytes, &file, url), true)
        }
    };
    let text = String::from_utf8(bytes)
        .map_err(|_| cannot(format!("{}: not a text file", original.display())))?;
    let must_path = reference.join("must_match.toml");
    let must = if must_path.exists() {
        let list = fs::read_to_string(&must_path)
            .map_err(|e| cannot(format!("{}: {e}", must_path.display())))?;
        MustMatch::from_toml(&list).map_err(|e| cannot(format!("{}: {e}", must_path.display())))?
    } else {
        MustMatch::default()
    };
    let tables = derive(&text, &source.file, &must).map_err(cannot)?;
    let files: [(PathBuf, &str); 2] = [
        (reference.join(NETLIST), &tables.netlist),
        (reference.join(PLACEMENTS), &tables.placements),
    ];
    match mode {
        Mode::Check => {
            let stale: Vec<String> = files
                .iter()
                .filter(|(path, text)| fs::read_to_string(path).ok().as_deref() != Some(*text))
                .map(|(path, _)| path.display().to_string())
                .collect();
            if !stale.is_empty() {
                return Err(TablesError::Failed(format!(
                    "{} not what {} gives: run `netsmith port tables` without --check",
                    match stale.as_slice() {
                        [one] => format!("{one} is"),
                        many => format!("{} are", many.join(" and ")),
                    },
                    original.display()
                )));
            }
        }
        Mode::Write => {
            fs::create_dir_all(reference)
                .map_err(|e| cannot(format!("{}: {e}", reference.display())))?;
            let mut writes: Vec<(PathBuf, String)> = files
                .iter()
                .map(|(p, t)| (p.clone(), t.to_string()))
                .collect();
            if new_source {
                writes.push((source_path, source.to_toml()));
            }
            for (path, text) in writes {
                fs::write(&path, text).map_err(|e| cannot(format!("{}: {e}", path.display())))?;
            }
        }
    }
    Ok(Written {
        tables,
        source,
        new_source,
    })
}

/// The original's netlist that a port's tests read from `reference`: from its board file when
/// the port names one, else from the derived `netlist.xml`.
pub fn reference_netlist(reference: &Path, board: Option<&str>) -> Result<Netlist, String> {
    let path = reference.join(board.unwrap_or(NETLIST));
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let netlist = match board {
        Some(_) => Netlist::from_kicad_pcb(&text),
        None => Netlist::from_kicad_xml(&text),
    };
    netlist.map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_is_written_and_read_back() {
        let source = Source::of(b"abc", "board.kicad_pcb", "https://example.com/x");
        assert_eq!(
            source.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(source.size, 3);
        assert_eq!(
            Source::parse(&source.to_toml(), "source.toml").unwrap(),
            source
        );
        let e = Source::parse(
            "file = \"a\"\nurl = \"b\"\nsize = 1\nsha256 = \"AB\"\n",
            "s",
        )
        .unwrap_err();
        assert!(e.contains("64 lower-case hex digits"), "{e}");
        let e = Source::parse("extra = 1", "s").unwrap_err();
        assert!(e.contains("`extra`"), "{e}");
    }

    #[test]
    fn a_url_must_name_a_commit() {
        let commit = "0123456789abcdef0123456789abcdef01234567";
        assert!(
            check_url(&format!(
                "https://raw.githubusercontent.com/o/r/{commit}/b.kicad_pcb"
            ))
            .is_ok()
        );
        let e = check_url("https://raw.githubusercontent.com/o/r/main/b.kicad_pcb").unwrap_err();
        assert!(e.contains("pinned commit"), "{e}");
        assert!(check_url(&format!("http://h/{commit}/b")).is_err());
    }
}
