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
