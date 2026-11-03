//! The layout file beside a source file (ADR 0020 item 7, ADR 0032): `<stem>.pcb.toml`.
//!
//! It is optional. `fab` and the editor's PCB view read it when it exists. It tells the placer
//! which face each part is on and what to keep clear of
//! ([`netsmith_pcb::place_options`]), and then puts what it holds on the placed board with
//! [`netsmith_pcb::apply_layout`].

use std::path::{Path, PathBuf};

use netsmith_pcb::Layout;

/// The layout file of a source file: `led.nsm` has `led.pcb.toml`.
pub fn layout_path(source: &Path) -> PathBuf {
    source.with_extension("pcb.toml")
}

/// The layout in the file at `file`, with the name to quote it by, or `None` if there is no
/// such file. `boards` are the names of all the boards of the source file, so that a table
/// for a board that is gone is found. A file that cannot be read or has a wrong table is an
/// error naming it.
pub fn read(file: &Path, boards: &[String]) -> Result<Option<(Layout, String)>, String> {
    let name = file.display().to_string();
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{name}: {e}")),
    };
    let layout = Layout::parse(&text, &name).map_err(|e| e.to_string())?;
    let known: Vec<&str> = boards.iter().map(String::as_str).collect();
    layout
        .check_boards(&name, &known)
        .map_err(|e| e.to_string())?;
    Ok(Some((layout, name)))
}
