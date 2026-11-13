//! The pin file beside a source file (ADR 0101): `<stem>.pins.toml`.
//!
//! It is optional. It holds the pins that the pin requests of the source (`.status: .u.GPIO`)
//! were given. `build`, `fab` and the editor give it to the checker with the source
//! ([`attach`]); `build` and `fab` also write it when a run picks a pin ([`sync`]), and
//! `--locked` makes a run that would change it stop instead, as `cargo --locked` does for
//! `Cargo.lock`. The editor assigns in memory; the only writes it makes are the `lock`, `pin` and
//! `source` lines of one entry (`editor/pinwrite.rs`).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use netsmith_check::{Loader, check};

/// The pin file of a source file: `main.nsm` has `main.pins.toml`.
pub fn pins_path(source: &Path) -> PathBuf {
    source.with_extension("pins.toml")
}

/// Pair the pin file beside `path` with the source called `name` in `loader`, if the file
/// exists. `unsaved` holds the text an editor has not saved yet. A file that exists and cannot
/// be read is an error naming it.
pub fn attach(
    loader: &mut Loader,
    name: &str,
    path: &Path,
    unsaved: &HashMap<PathBuf, String>,
) -> Result<(), String> {
    let file = pins_path(path);
    let text = match unsaved.get(&file) {
        Some(text) => text.clone(),
        None => match fs::read_to_string(&file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("{}: {e}", file.display())),
        },
    };
    loader.attach_pins(name, &file_name(name), &text);
    Ok(())
}

/// How the pin file of the source called `name` is named in a message: `src/main.pins.toml`.
pub fn file_name(name: &str) -> String {
    Path::new(name)
        .with_extension("pins.toml")
        .display()
        .to_string()
}

/// What a run did about the pin file of one source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Synced {
    /// What the run says about the file, one line each: what it wrote, and the entries that
    /// were stale.
    pub notes: Vec<String>,
}

/// Make the pin file of the source at `path` (called `name`, with text `text`) say what the
/// requests of the source got. Writes it when a run picked a pin, moved one or dropped a stale
/// entry; with `locked` it writes nothing and returns the changes as the error instead.
///
/// A source that does not check is left alone: the build reports its errors.
pub fn sync(
    name: &str,
    path: &Path,
    text: &str,
    loader: &Loader,
    locked: bool,
) -> Result<Synced, String> {
    let checked = check(name, text, loader);
    if !checked.is_ok() {
        return Ok(Synced::default());
    }
    let pins = &checked.pins;
    let file = pins_path(path);
    let shown = file_name(name);
    let mut notes: Vec<String> = pins.findings.clone();
    if !pins.changed() {
        return Ok(Synced { notes });
    }
    if locked {
        let mut message = format!(
            "error: the pin file `{shown}` is out of date, and --locked does not change it:\n"
        );
        for line in &pins.changes {
            message.push_str(&format!("  {line}\n"));
        }
        for line in &pins.findings {
            message.push_str(&format!("  {line}\n"));
        }
        message.push_str(
            "run `netsmith build` without --locked, read the change and commit the file\n",
        );
        return Err(message);
    }
    fs::write(&file, pins.file.to_text()).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut wrote = vec![format!("wrote {shown}")];
    wrote.extend(pins.changes.iter().map(|c| format!("  {c}")));
    notes.splice(0..0, wrote);
    Ok(Synced { notes })
}
