//! The editor writes the pin file (ADR 0101), the `lock`, `pin` and `source` lines of one entry at
//! a time (and its `function` line, when a move puts a request for a class on a pin whose row
//! names another channel of it, #787) and no other line: a padlock click locks or unlocks a
//! request's pin, and a move puts a request on another pin that offers its function. The owner decided that the editor may write
//! these lines and not the pick lock (ED2 of the pin epic).
//!
//! A choice is judged from the analysis of the files on disk, with the rules the check applies
//! to a pin file it reads (a pin that does not offer the function, a pin two requests have, a
//! pin the source holds, a `shared` limit), so a pin written here is one the next reading keeps.
//! A choice that breaks one is refused with the reason in words, and nothing is written. The
//! lines are changed in place by [`netsmith_check::edit_entry`]: comments, order and spacing of
//! every other line survive. The write is not a source edit; the project is analysed again
//! because a watched file changed, as for a save.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use netsmith_check::{EntrySource, PartSpace, PinEntry, PinFile, PinsOutcome, SpaceRequest};
use netsmith_editor::ApiError;
use netsmith_editor::api::PinWritten;

use super::analysis::BoardData;
use super::{Workspace, files};

/// What a person asked to do to one request.
pub(super) enum Change {
    /// Lock (`true`) or unlock (`false`) the pin the request has.
    Lock(bool),
    /// Put the request on the pin with this path in the part.
    Move(String),
}

/// A choice the pin file's rules do not allow. `422`: the request is well formed, the project
/// is what refuses it.
fn refused(message: impl Into<String>) -> ApiError {
    ApiError::unprocessable(message, "pin_refused", &[])
}

/// The entry the file should have after `change` of `request`, or why not. `space` is the part's
/// pins the optimizer knows, `pins` the pin names the part has, `held` the entry the analysis
/// has for the request.
fn decide(
    space: &PartSpace,
    pins: &[&str],
    request: &SpaceRequest,
    held: Option<&PinEntry>,
    change: &Change,
) -> Result<PinEntry, ApiError> {
    let key = &request.key;
    // A line of a request for the end of a bus (#788) is one pin of an entry that holds the
    // whole peripheral; the editor writes entries of one pin only.
    if let Some(bus) = &request.bus {
        return Err(refused(format!(
            "`{key}` is the line `{}` of `{}`, a request for the end of a bus, which has `{}` and the pin of each of its lines in one entry: edit that entry of the pin file",
            bus.line, bus.request, bus.peripheral
        )));
    }
    match change {
        Change::Lock(locked) => {
            if request.lock == *locked {
                return Err(refused(if *locked {
                    format!("`{key}` is locked already")
                } else {
                    format!("`{key}` is not locked")
                }));
            }
            Ok(PinEntry {
                pin: request.pin.clone(),
                function: request.function_on(&request.pin).to_string(),
                lock: *locked,
                // Who chose the pin does not change with a padlock: a pin the tool picked stays
                // the tool's, and a pin a person put there stays theirs.
                source: held.map_or(EntrySource::Auto, |e| e.source),
                at: None,
            })
        }
        Change::Move(pin) => {
            let (function, part) = (&request.capability, &space.path);
            if request.lock {
                return Err(refused(format!(
                    "`{key}` is locked: unlock it before moving it"
                )));
            }
            if *pin == request.pin {
                return Err(refused(format!("`{key}` is on {pin} already")));
            }
            if !pins.contains(&pin.as_str()) {
                return Err(refused(format!("`{part}` has no pin {pin}")));
            }
            if space.taken.contains(pin) {
                return Err(refused(format!(
                    "{pin} is used by the source (it is connected there, or its function is chosen): `{key}` cannot have it"
                )));
            }
            if let Some(other) = space.requests.iter().find(|r| r.pin == *pin) {
                return Err(refused(format!(
                    "{pin} is held by `{}`{}: move that request first",
                    other.key,
                    if other.lock { ", which is locked" } else { "" }
                )));
            }
            if !request.offers.contains(pin) {
                return Err(refused(format!(
                    "{pin} does not offer {function}, which `{key}` asks `{part}` for"
                )));
            }
            let picks: Vec<(&str, &str)> = space
                .requests
                .iter()
                .map(|r| {
                    let at = if r.key == *key { pin } else { &r.pin };
                    (r.key.as_str(), at.as_str())
                })
                .collect();
            if !space.allows(&picks) {
                return Err(refused(format!(
                    "{pin} would use more of a `shared` resource of `{part}` than its limit allows (`{key}` asks for {function})"
                )));
            }
            Ok(PinEntry {
                pin: pin.clone(),
                // The channel of a class on the new pin, when its row names one (#787).
                function: request.function_on(pin).to_string(),
                lock: false,
                // A person chose this pin.
                source: EntrySource::Hand,
                at: None,
            })
        }
    }
}

impl Workspace {
    /// Change one request's entry in the pin file beside the source of `board`.
    pub(super) fn write_pin(
        &self,
        board: &str,
        request: &str,
        change: &Change,
    ) -> Result<PinWritten, ApiError> {
        let _one_at_a_time = self.editing.lock().unwrap_or_else(|e| e.into_inner());
        let (data, pins, stale) = self.pin_site(board)?;
        let definition = &data.built.design.name;
        // A request the check gave no pin says why, whether or not the board is the last good one.
        let unserved = pins
            .unserved
            .get(definition)
            .into_iter()
            .flatten()
            .find(|u| u.key == request);
        if let Some(u) = unserved {
            return Err(refused(if u.message.is_empty() {
                format!("`{request}` has no pin yet: the problems list says why")
            } else {
                format!("`{request}` has no pin yet: {}", u.message)
            }));
        }
        if stale {
            return Err(refused(
                "the source does not build, so the pin map is the last good one: fix the problems first",
            ));
        }
        let (space, asked) = pins
            .spaces
            .get(definition)
            .into_iter()
            .flatten()
            .find_map(|s| s.requests.iter().find(|r| r.key == request).map(|r| (s, r)))
            .ok_or_else(|| {
                ApiError::not_found(format!("`{board}` has no pin request `{request}`"))
            })?;
        let part = data
            .built
            .design
            .parts
            .iter()
            .find(|p| p.path == space.path)
            .ok_or_else(|| {
                ApiError::internal(format!("the part `{}` is not in the design", space.path))
            })?;
        let names: Vec<&str> = part.pins.iter().map(|p| p.name.as_str()).collect();
        let held = pins.file.board(definition).and_then(|b| b.get(request));
        let to = decide(space, &names, asked, held, change)?;

        let (_, path, shown, text) = self.pin_file_text(&data, &pins)?;
        let out = netsmith_check::edit_entry(&text, definition, request, &to).map_err(refused)?;
        files::write_atomic(&path, &out)
            .map_err(|e| ApiError::internal(format!("{shown}: {e}")))?;
        Ok(PinWritten { file: shown })
    }
}

/// What [`Workspace::apply_pins`] wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinsApplied {
    /// The pin file, as the project names it (`src/main.pins.toml`).
    pub file: String,
    /// The requests moved, one line each (`x: p00 to p01`). Empty when the file already had
    /// every pin of the proposal, and nothing was written.
    pub moved: Vec<String>,
}

impl Workspace {
    /// The board `board` as the analysis has it, with the pin map of its source file, and
    /// whether the source does not build now (so that the board is the last good one).
    fn pin_site(&self, board: &str) -> Result<(Arc<BoardData>, Arc<PinsOutcome>, bool), ApiError> {
        let state = self.fresh();
        let (data, stale) = state
            .snapshot
            .boards
            .iter()
            .find(|(b, _)| b.built.label() == board)
            .map(|(b, stale)| (Arc::clone(b), *stale))
            .ok_or_else(|| ApiError::not_found(format!("there is no board `{board}`")))?;
        let pins = state
            .snapshot
            .pins
            .get(&data.file)
            .cloned()
            .ok_or_else(|| refused(format!("the check made no pin map for `{}`", data.file)))?;
        Ok((data, pins, stale))
    }

    /// The source of `data`, the pin file beside it, its name as a message has it, and its text
    /// (empty when there is none). A file that another program changed since the analysis read it
    /// is a conflict: a write would cover what the reader never saw.
    fn pin_file_text(
        &self,
        data: &BoardData,
        pins: &PinsOutcome,
    ) -> Result<(PathBuf, PathBuf, String, String), ApiError> {
        let source = self
            .target()
            .files
            .iter()
            .find(|f| f.name == data.file)
            .map(|f| f.path.clone())
            .ok_or_else(|| {
                ApiError::internal(format!("{} is not a file of the project", data.file))
            })?;
        let path = netsmith_fabricate::pins::pins_path(&source);
        let shown = netsmith_fabricate::pins::file_name(&data.file);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(ApiError::internal(format!("{shown}: {e}"))),
        };
        // The analysis read the file as it was; if another program changed it since, this
        // would write over what the page never showed.
        let read = pins.existing.clone().unwrap_or_default();
        if PinFile::parse(&text).is_ok_and(|now| now != read) {
            return Err(ApiError::conflict(format!(
                "{shown} changed on disk since the pin map was read: read it again"
            )));
        }
        Ok((source, path, shown, text))
    }

    /// Take the pins of a proposal (`pins.proposed.toml`, which `netsmith board fab --pins
    /// locked` writes) for `board`: each request whose pin the proposal changes is written as the
    /// proposal has its entry, in place as [`netsmith_check::edit_entry`] does, so the comments
    /// and the order of the file survive. Only the entries of `board` are read. Refused, with
    /// nothing written, when the proposal does not read, names a request the board lacks, moves a
    /// locked request, or gives a pin map that the check does not keep as it is (a pin that does
    /// not offer the function, one that two requests have, a `shared` limit): the rules of a move,
    /// applied to the whole map, so that a swap of two pins is not refused for the order it is
    /// written in.
    pub fn apply_pins(&self, board: &str, proposal: &str) -> Result<PinsApplied, ApiError> {
        let _one_at_a_time = self.editing.lock().unwrap_or_else(|e| e.into_inner());
        let (data, pins, stale) = self.pin_site(board)?;
        if stale {
            return Err(refused(
                "the source does not build, so the pin map is the last good one: fix the problems first",
            ));
        }
        let definition = &data.built.design.name;
        let proposed = PinFile::parse(proposal).map_err(|errors| {
            let first = errors.first().map_or("", |e| e.message.as_str());
            refused(format!("the proposal does not read ({first})"))
        })?;
        let (source, path, shown, text) = self.pin_file_text(&data, &pins)?;
        let spaces = pins
            .spaces
            .get(definition)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut out = text.clone();
        let mut moved = Vec::new();
        let mut keys = Vec::new();
        for (key, entry) in proposed.board(definition).into_iter().flatten() {
            let Some(request) = spaces
                .iter()
                .find_map(|s| s.requests.iter().find(|r| r.key == *key))
            else {
                return Err(refused(format!(
                    "the proposal has an entry for `{key}`, which is not a pin request of `{board}`"
                )));
            };
            if request.bus.is_some() {
                return Err(refused(format!(
                    "`{key}` is a line of a request for the end of a bus: edit that entry of the pin file"
                )));
            }
            if request.pin == entry.pin {
                continue;
            }
            if request.lock {
                return Err(refused(format!(
                    "the proposal moves `{key}` from {} to {}, and `{key}` is locked: unlock it first",
                    request.pin, entry.pin
                )));
            }
            moved.push(format!("{key}: {} to {}", request.pin, entry.pin));
            keys.push((key.as_str(), entry));
            out = netsmith_check::edit_entry(&out, definition, key, entry).map_err(refused)?;
        }
        if moved.is_empty() {
            return Ok(PinsApplied { file: shown, moved });
        }
        // The map the file would have, read as a run reads it: every moved request must keep
        // its pin, and no entry may be newly stale.
        let checked = {
            let unsaved = HashMap::from([(path.clone(), out.clone())]);
            let loader = self
                .target()
                .loader_with(&unsaved)
                .map_err(ApiError::internal)?;
            let source_text = fs::read_to_string(&source)
                .map_err(|e| ApiError::internal(format!("{}: {e}", data.file)))?;
            netsmith_check::check(&data.file, &source_text, &loader)
        };
        let mut why: Vec<String> = checked
            .pins
            .findings
            .iter()
            .filter(|f| !pins.findings.contains(f))
            .cloned()
            .collect();
        for (key, entry) in &keys {
            let holds = checked
                .pins
                .file
                .board(definition)
                .and_then(|b| b.get(*key))
                .is_some_and(|e| e.pin == entry.pin);
            if !holds {
                why.push(format!("`{key}` does not keep {}", entry.pin));
            }
        }
        if !why.is_empty() || !checked.is_ok() {
            let mut message = String::from("the proposal is not a pin map the check keeps:");
            for line in &why {
                message.push_str(&format!("\n  {line}"));
            }
            return Err(refused(message));
        }
        files::write_atomic(&path, &out)
            .map_err(|e| ApiError::internal(format!("{shown}: {e}")))?;
        Ok(PinsApplied { file: shown, moved })
    }
}
