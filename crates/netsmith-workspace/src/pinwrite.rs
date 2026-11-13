//! The editor writes the pin file (ADR 0101), the `lock`, `pin` and `source` lines of one entry at
//! a time and no other line: a padlock click locks or unlocks a request's pin, and a move puts a
//! request on another pin that offers its function. The owner decided that the editor may write
//! these lines and not the pick lock (ED2 of the pin epic).
//!
//! A choice is judged from the analysis of the files on disk, with the rules the check applies
//! to a pin file it reads (a pin that does not offer the function, a pin two requests have, a
//! pin the source holds, a `shared` limit), so a pin written here is one the next reading keeps.
//! A choice that breaks one is refused with the reason in words, and nothing is written. The
//! lines are changed in place by [`netsmith_check::edit_entry`]: comments, order and spacing of
//! every other line survive. The write is not a source edit; the project is analysed again
//! because a watched file changed, as for a save.

use std::fs;
use std::sync::Arc;

use netsmith_check::{EntrySource, PartSpace, PinEntry, PinFile, PinsOutcome, SpaceRequest};
use netsmith_editor::ApiError;
use netsmith_editor::api::PinWritten;

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
                function: request.capability.clone(),
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
                function: function.clone(),
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
        let (data, pins, stale): (_, Arc<PinsOutcome>, bool) = {
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
            (data, pins, stale)
        };
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
                "{shown} changed on disk since the pin map was read: the page reads it again"
            )));
        }
        let out = netsmith_check::edit_entry(&text, definition, request, &to).map_err(refused)?;
        files::write_atomic(&path, &out)
            .map_err(|e| ApiError::internal(format!("{shown}: {e}")))?;
        Ok(PinWritten { file: shown })
    }
}
