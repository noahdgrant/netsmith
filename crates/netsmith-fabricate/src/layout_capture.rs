//! `netsmith layout capture` (issue #828): the placement as the layout file, with its seed.
//!
//! [`capture`] places the board as `fab` places it (the layout file and the physical file read)
//! and gives the layout file that says where every part is: an entry for each part, the seed
//! that placed the board, and every other table of the file as it was. A part the file already
//! has an entry for keeps its place, its `lock` and its `source` (a build puts a part with an
//! entry where the entry says); a part the placer placed gets an entry with `source = "auto"`.
//! A build of the file then has every part where it was captured, whatever the placer, a
//! catalog or the circuit around it does later: that is what freezes a result. [`accept`]
//! writes it over the file, keeping the file's comments.
//!
//! The places are rounded to 0.01 mm, as every entry of the file is, so a part the placer placed
//! is up to 0.005 mm from where it was in each coordinate once the file is read.

use std::path::Path;

use netsmith_core::Design;
use netsmith_pcb::Layout;
use serde::Serialize;

/// What a capture changes in the layout file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Captured {
    pub board: String,
    /// The seed that placed the board, written as the file's seed.
    pub seed: u32,
    /// The seed the file stated before (`None`: it stated none).
    pub seed_was: Option<u32>,
    /// How many parts the board has: each has an entry after the capture.
    pub parts: usize,
    /// The paths of the parts that had no entry, the ones the placer placed.
    pub added: Vec<String>,
    /// How many parts had an entry already, which is kept as it is.
    pub kept: usize,
    /// How many keepouts and cutouts the layout file places (each keeps its entry).
    pub shapes: usize,
    /// Why the seed is not the file's, when it is not: the file's seed no longer places the board.
    pub seed_note: Option<String>,
}

impl Captured {
    /// What the capture is, for a person: the board, its seed, and the parts.
    pub fn text(&self) -> String {
        let seed = match self.seed_was {
            Some(was) if was == self.seed => format!("seed {} (as the file states it)", self.seed),
            Some(was) => format!("seed {} (the file stated {was})", self.seed),
            None => format!("seed {} (the file stated none)", self.seed),
        };
        let mut out = format!(
            "{}: {} part(s) with {seed}: {} new, {} already had an entry\n",
            self.board,
            self.parts,
            self.added.len(),
            self.kept,
        );
        if let Some(note) = &self.seed_note {
            out.push_str(&format!("note: {note}\n"));
        }
        for path in &self.added {
            out.push_str(&format!("  new: {path}\n"));
        }
        out
    }
}

/// What capturing `design` says, placed as `fab` places it with the layout file at `layout_file`
/// (if it exists), and the layout it gives. Nothing is written. A board that no seed places is
/// refused with the placer's report: a capture holds a board that places.
pub fn capture(
    design: &Design,
    layout_file: &Path,
    libraries: &netsmith_pipeline::Libraries,
    lines_warn: bool,
    seeds: crate::layout::Seeding,
) -> Result<(Captured, Layout), String> {
    let name = layout_file.display().to_string();
    let layout = match std::fs::read_to_string(layout_file) {
        Ok(text) => Some((
            Layout::parse(&text, &name).map_err(|e| e.to_string())?,
            name.clone(),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("{name}: {e}")),
    };
    let placing = crate::layout::place(design, layout.as_ref(), libraries, lines_warn, seeds)
        .map_err(|e| e.message())?;
    if !placing.seeds.placed {
        return Err(placing.seeds.report());
    }
    let seed = u32::try_from(placing.seeds.used)
        .map_err(|_| format!("{name}: seed {} is too big", placing.seeds.used))?;
    let board = &placing.board;
    let layout = layout.map(|(l, _)| l).unwrap_or_default();
    let table = layout.boards.get(&board.name);

    let mut captured = Layout::capture(board, table);
    captured.seed = Some(seed);
    // A keepout or cutout the source places is not the file's to place, and an entry for one is
    // an error. The ones the file places have `at` in the board and none in the design.
    captured.keepouts.retain(|k, _| {
        design
            .features
            .keepouts
            .iter()
            .any(|d| &d.name == k && d.at.is_none())
    });
    captured.cutouts.retain(|k, _| {
        design
            .features
            .cutouts
            .iter()
            .any(|d| &d.name == k && d.at.is_none())
    });

    let had = |path: &String| table.is_some_and(|t| t.parts.contains_key(path));
    let added: Vec<String> = captured.parts.keys().filter(|p| !had(p)).cloned().collect();
    let report = Captured {
        board: board.name.clone(),
        seed,
        seed_was: table.and_then(|t| t.seed),
        parts: captured.parts.len(),
        kept: captured.parts.len() - added.len(),
        added,
        shapes: captured.keepouts.len() + captured.cutouts.len(),
        seed_note: placing.seeds.note(&name),
    };
    let mut next = layout;
    next.boards.insert(board.name.clone(), captured);
    Ok((report, next))
}

/// Write `layout` over the layout file at `file`, keeping its comments.
pub fn accept(file: &Path, layout: &Layout) -> Result<(), String> {
    crate::regenerate::accept(file, layout)
}
