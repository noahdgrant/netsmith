//! What the editor shows of the two files a run of netsmith keeps beside a source file, read
//! only: the pin file (`<stem>.pins.toml`, ADR 0101) as the pin map of a part, and the pick
//! lock (`<stem>.lock.toml`, ADR 0100) as what `netsmith lock --check` says of it.
//!
//! The pin map is what the check assigned in memory, as it does on every analysis; the lock is
//! solved again with no holding and compared with the file, as `lock --check` does, which needs
//! no KiCad. The lock is never written here. The pin file is written by `pinwrite.rs`, the
//! `lock`, `pin` and `source` lines of one entry at a time (the owner's decision on the editor's
//! writes, ED2 of the pin epic).

use std::path::Path;

use netsmith_check::{PinsOutcome, SpaceRequest};
use netsmith_editor::api::{PickLockInfo, PinOffer, PinRequestInfo};

use netsmith_fabricate::project::Target;

/// The pin requests that ask the part at `part_path` (of the board defined as `board`) for a
/// pin: the ones that got a pin first in the byte order of their paths, merged with the ones
/// that got none, so the list is in that order. `part` is the part of the design, for the pad.
pub(super) fn pin_requests(
    pins: &PinsOutcome,
    board: &str,
    part_path: &str,
    part: &netsmith_core::Part,
) -> Vec<PinRequestInfo> {
    let entries = pins.file.board(board);
    let before = pins.existing.as_ref().and_then(|f| f.board(board));
    let mut out: Vec<PinRequestInfo> = Vec::new();
    let served: &[SpaceRequest] = pins
        .spaces
        .get(board)
        .and_then(|spaces| spaces.iter().find(|s| s.path == part_path))
        .map_or(&[], |s| s.requests.as_slice());
    for r in served {
        let entry = entries.and_then(|e| e.get(&r.key));
        let pad_of = |pin: &str| {
            part.pins
                .iter()
                .find(|p| p.name == pin)
                .map(|p| p.pads.join(", "))
                .filter(|pads| !pads.is_empty())
        };
        let offers = r
            .offers
            .iter()
            .map(|pin| PinOffer {
                pin: pin.clone(),
                pad: pad_of(pin),
                held_by: served
                    .iter()
                    .find(|o| o.pin == *pin && o.key != r.key)
                    .map(|o| o.key.clone()),
            })
            .collect();
        let held = entry.is_some_and(|e| before.and_then(|b| b.get(&r.key)) == Some(e));
        out.push(PinRequestInfo {
            request: r.key.clone(),
            function: r.capability.clone(),
            pin: Some(r.pin.clone()),
            pad: pad_of(&r.pin),
            locked: r.lock,
            entry: entry.map(|e| e.source.word().to_string()),
            from: Some(if held { "file" } else { "run" }.to_string()),
            message: None,
            offers,
        });
    }
    for u in pins
        .unserved
        .get(board)
        .into_iter()
        .flatten()
        .filter(|u| u.part == part_path)
    {
        out.push(PinRequestInfo {
            request: u.key.clone(),
            function: u.capability.clone(),
            pin: None,
            pad: None,
            locked: false,
            entry: None,
            from: None,
            offers: Vec::new(),
            message: Some(if u.message.is_empty() {
                "the check gave this request no pin; the problems list says why".to_string()
            } else {
                u.message.clone()
            }),
        });
    }
    out.sort_by(|a, b| a.request.cmp(&b.request));
    out
}

/// What `netsmith lock --check` finds for the source file called `name`, as the editor shows it:
/// `None` when there is nothing to compare, because the source cannot be read or does not
/// build (the problems list says why), and an `error` when the lock file is not one this
/// netsmith reads.
pub(super) fn lock_info(target: &Target, name: &str) -> Option<PickLockInfo> {
    let shown = Path::new(name)
        .with_extension("lock.toml")
        .display()
        .to_string();
    let source = target.files.iter().find(|f| f.name == name)?;
    if let Err(error) =
        netsmith_pipeline::pick_lock::read(&netsmith_pipeline::pick_lock::lock_path(&source.path))
    {
        return Some(PickLockInfo {
            file: shown,
            exists: true,
            current: false,
            diff: Vec::new(),
            reported: String::new(),
            error: Some(error.trim().trim_start_matches("error: ").to_string()),
        });
    }
    let text = std::fs::read_to_string(&source.path).ok()?;
    let (loader, catalogs) = (target.loader().ok()?, target.catalogs().ok()?);
    let found =
        netsmith_pipeline::pick_lock::refresh(&source.path, name, &text, &loader, &catalogs)
            .ok()?;
    Some(PickLockInfo {
        file: shown,
        exists: found.existed,
        current: found.is_current(),
        // `lock --check` prints no lines for a lock that does not exist: only that it does
        // not, which the page says.
        diff: if found.existed {
            found.diff
        } else {
            Vec::new()
        },
        reported: found.reported.trim_end().to_string(),
        error: None,
    })
}
