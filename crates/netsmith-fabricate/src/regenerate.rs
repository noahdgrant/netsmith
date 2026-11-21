//! Regenerate one module (ADR 0030, issue #825): the proposal that `netsmith layout regenerate`
//! prints and the editor previews, and the layout file it writes once accepted.
//!
//! [`propose`] takes a board as the layout file places it, holds every part outside the module
//! and every locked part of it where it is, and places the module's other parts again with
//! `netsmith_placer::regenerate` (its header has the scope, the region and the seed). The
//! proposal says which parts moved and where to, which nets lose their copper (the nets of the
//! parts placed again, which `fab` and the editor route again), which records of the routes file
//! no longer match their pads (kept, open where their copper misses a moved pad, as any record
//! whose pads moved is, issue #1328), and the placement's findings and totals before and after
//! (`netsmith_pcb::PlacementComparison`). It writes nothing.
//!
//! The layout it gives has an entry for each part placed again, unlocked and with `source =
//! "regen"`, and every other table as it was; [`accept`] writes it over the file, keeping the
//! file's comments. A build of the file then puts the parts where the proposal has them: an
//! unlocked entry is where the placer starts from and the part is put where the entry says.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use netsmith_pcb::{Board, Layout, LayoutFace, PartPlace, PlacementComparison, Routes, Side};
use serde::Serialize;

/// How far round a module's parts a regeneration may place them, in mm, unless asked otherwise.
pub const DEFAULT_MARGIN: f64 = netsmith_placer::DEFAULT_MARGIN;

/// The `source` of an entry a regeneration wrote.
pub const SOURCE: &str = "regen";

/// What to regenerate.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// The module's path in the design (`stage`, `channels[1]`).
    pub module: String,
    pub seed: u64,
    /// How far round the module's parts they may go, in mm.
    pub margin: f64,
}

/// A part a regeneration placed again: its entry before (none when the placer placed it) and
/// after.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Moved {
    pub path: String,
    pub refdes: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<PartPlace>,
    /// Where the board had it before, as an entry would say it.
    pub was: PartPlace,
    pub after: PartPlace,
}

/// The region the parts were placed in, in mm from the outline's top left corner, y down.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Region {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// What [`propose`] found: the module's parts placed again, and what that changes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Proposal {
    pub board: String,
    pub module: String,
    pub seed: u64,
    pub margin: f64,
    pub region: Region,
    /// The parts placed again, in the board's order.
    pub parts: Vec<Moved>,
    /// The parts of the module the layout file locks or the requirements pin, by path: left
    /// where they were.
    pub held: Vec<String>,
    /// The nets with a pad on a part placed again: their copper is routed again.
    pub nets: Vec<String>,
    /// The nets whose record in the routes file no longer matches its pads once the parts
    /// moved: its copper is kept, open where it misses a moved pad, and the router routes what it
    /// leaves (issue #1328).
    pub routes_dropped: Vec<String>,
    /// The placement's findings and totals before and after.
    pub comparison: PlacementComparison,
    /// What of the router's copper kept on disk (`crate::kept_routes`, issue #1019) a route
    /// after the regeneration keeps; `None` when there is no kept file (or the editor asked,
    /// which keeps its routed boards itself).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copper: Option<KeptCopper>,
}

/// What of the router's copper kept on disk a route of the regenerated board keeps
/// (`netsmith_router::carry::carry_copper`, as the editor and `board fab --reuse-routes` route it).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KeptCopper {
    /// The kept file.
    pub file: String,
    /// The nets whose kept copper is carried.
    pub kept: Vec<String>,
    /// The nets whose kept copper is dropped and routed again.
    pub again: Vec<String>,
    /// Why the file is not used at all, when it is not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unused: Option<String>,
}

impl Proposal {
    /// The text `netsmith layout regenerate` prints.
    pub fn text(&self) -> String {
        let mut out = format!(
            "regenerated `{}` of {} with seed {} in {:.2},{:.2} {:.2} x {:.2} mm: {} part(s) placed again",
            self.module,
            self.board,
            self.seed,
            self.region.x,
            self.region.y,
            self.region.width,
            self.region.height,
            self.parts.len()
        );
        if !self.held.is_empty() {
            out.push_str(&format!(", held: {}", self.held.join(", ")));
        }
        out.push('\n');
        for m in &self.parts {
            out.push_str(&format!(
                "  {} ({}): {} -> {}\n",
                m.path,
                m.refdes,
                place_text(&m.was),
                place_text(&m.after)
            ));
        }
        if !self.nets.is_empty() {
            out.push_str(&format!("routed again: {}\n", self.nets.join(", ")));
        }
        if !self.routes_dropped.is_empty() {
            out.push_str(&format!(
                "routes file records that no longer match their pads (their copper kept, open where it misses a moved pad): {}\n",
                self.routes_dropped.join(", ")
            ));
        }
        match &self.copper {
            Some(KeptCopper {
                file,
                unused: Some(why),
                ..
            }) => out.push_str(&format!("kept copper ({file}): {why}\n")),
            Some(k) => out.push_str(&format!(
                "kept copper ({}): the router's copper of {} net(s) kept, {} routed again\n",
                k.file,
                k.kept.len(),
                k.again.len()
            )),
            None => {}
        }
        let c = &self.comparison;
        for f in &c.appeared {
            out.push_str(&format!("finding appeared: {f}\n"));
        }
        for f in &c.gone {
            out.push_str(&format!("finding gone: {f}\n"));
        }
        out.push_str(&format!(
            "hpwl {:.2} -> {:.2} mm, ratsnest {:.2} -> {:.2} mm, crossings {} -> {}\n",
            c.before.hpwl,
            c.after.hpwl,
            c.before.ratsnest,
            c.after.ratsnest,
            c.before.crossings,
            c.after.crossings
        ));
        out
    }
}

fn place_text(p: &PartPlace) -> String {
    format!(
        "{:.2},{:.2} {}° {}",
        p.at[0],
        p.at[1],
        p.rotation,
        match p.side {
            LayoutFace::Top => "top",
            LayoutFace::Bottom => "bottom",
        }
    )
}

/// Where `part` of `board` is, as an entry of the layout file says it.
fn entry_of(
    board: &Board,
    part: &netsmith_pcb::Placed,
    lock: bool,
    source: Option<String>,
) -> PartPlace {
    let at = part.at - board.outline.min;
    let round = |v: f64| (v * 100.0).round() / 100.0;
    PartPlace {
        at: [round(at.x) + 0.0, round(at.y) + 0.0],
        rotation: part.rotation,
        side: if part.side == Side::Back {
            LayoutFace::Bottom
        } else {
            LayoutFace::Top
        },
        text: part.reference_side.into(),
        designator: part.designator.map(Into::into),
        lock,
        source,
    }
}

/// The proposal of regenerating `request.module` of `board` (placed as the layout file `layout`
/// says, with no copper), the board after it, and the layout with the new entries. `routes` is
/// the routes file beside the layout file, if any. `say` is told each step as it begins (the
/// editor passes it on to the page as progress). Nothing is written.
pub fn propose_on(
    board: &Board,
    layout: Option<&Layout>,
    routes: Option<&Routes>,
    request: &Request,
    say: &dyn Fn(&str),
) -> Result<(Proposal, Board, Layout), String> {
    let table = layout.and_then(|l| l.boards.get(&board.name));
    let held: BTreeSet<String> = board
        .parts
        .iter()
        .filter(|p| {
            table
                .and_then(|t| t.parts.get(&p.path))
                .is_some_and(|e| e.lock)
        })
        .map(|p| p.refdes.clone())
        .collect();
    let mut after = board.clone();
    say(&format!("placing the parts of {} again", request.module));
    let done = netsmith_placer::regenerate(
        &mut after,
        &request.module,
        &held,
        request.seed,
        request.margin,
    )
    .map_err(|e| e.to_string())?;
    let mut next = layout.cloned().unwrap_or_default();
    let next_table = next.boards.entry(board.name.clone()).or_default();
    let mut parts = Vec::new();
    for p in after
        .parts
        .iter()
        .filter(|p| done.parts.contains(&p.refdes))
    {
        let old = board.part(&p.refdes).expect("the same parts");
        let before = table.and_then(|t| t.parts.get(&p.path)).cloned();
        let entry = entry_of(&after, p, false, Some(SOURCE.to_string()));
        next_table.parts.insert(p.path.clone(), entry.clone());
        parts.push(Moved {
            path: p.path.clone(),
            refdes: p.refdes.clone(),
            was: entry_of(
                board,
                old,
                false,
                before.as_ref().and_then(|b| b.source.clone()),
            ),
            before,
            after: entry,
        });
    }
    let path_of = |r: &str| {
        board
            .part(r)
            .map_or_else(|| r.to_string(), |p| p.path.clone())
    };
    let records: BTreeMap<&String, &netsmith_pcb::NetRoutes> = routes
        .and_then(|r| r.boards.get(&board.name))
        .map(|t| t.nets.iter().collect())
        .unwrap_or_default();
    let routes_dropped = done
        .nets
        .iter()
        .filter(|net| {
            records.get(net).is_some_and(|record| {
                netsmith_pcb::pad_hash(&after, net).is_some_and(|h| h != record.pads)
                    && netsmith_pcb::pad_hash(board, net).is_some_and(|h| h == record.pads)
            })
        })
        .cloned()
        .collect();
    say("measuring the placement before and after");
    let comparison = PlacementComparison::between(board, &after);
    let o = after.outline.min;
    let proposal = Proposal {
        board: board.name.clone(),
        module: request.module.clone(),
        seed: request.seed,
        margin: request.margin,
        region: Region {
            x: done.region.min.x - o.x,
            y: done.region.min.y - o.y,
            width: done.region.width(),
            height: done.region.height(),
        },
        parts,
        held: done.held.iter().map(|r| path_of(r)).collect(),
        nets: done.nets.clone(),
        routes_dropped,
        comparison,
        copper: None,
    };
    Ok((proposal, after, next))
}

/// [`propose_on`] for the design `design`, placed as `fab` places it with the layout file at
/// `layout_file` (if it exists) and the routes file beside it.
pub fn propose(
    design: &netsmith_core::Design,
    layout_file: &Path,
    libraries: &netsmith_pipeline::Libraries,
    lines_warn: bool,
    seeds: crate::layout::Seeding,
    request: &Request,
) -> Result<(Proposal, Layout), String> {
    let layout = std::fs::read_to_string(layout_file)
        .ok()
        .map(|text| {
            let name = layout_file.display().to_string();
            Layout::parse(&text, &name)
                .map(|l| (l, name))
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    let placing = crate::layout::place(design, layout.as_ref(), libraries, lines_warn, seeds)
        .map_err(|e| e.message())?;
    let routes = crate::routes::load(&crate::routes::beside_layout(layout_file))?;
    let (mut proposal, after, next) = propose_on(
        &placing.board,
        layout.as_ref().map(|(l, _)| l),
        Some(&routes),
        request,
        &|_| {},
    )?;
    proposal.copper = kept_copper(layout_file, &after, &routes, &proposal.nets)?;
    Ok((proposal, next))
}

/// What of the router's copper kept on disk for the source of `layout_file` a route of `after`
/// (the regenerated board, with no copper) keeps: every net but `nets` (the regenerated parts')
/// and those the placement changed is carried, as the editor and `board fab --reuse-routes` carry it.
/// `None` when there is no kept file.
pub fn kept_copper(
    layout_file: &Path,
    after: &Board,
    routes: &Routes,
    nets: &[String],
) -> Result<Option<KeptCopper>, String> {
    let kept = crate::kept_routes::KeptRoutes::beside_layout(layout_file);
    let file = kept.file(&after.name).display().to_string();
    let mut board = after.clone();
    netsmith_pcb::apply_routes(&mut board, routes, "the routes file").map_err(|e| e.to_string())?;
    let start = match kept.start_from(&board) {
        Ok(None) => return Ok(None),
        Ok(Some(start)) => start,
        Err(why) => {
            return Ok(Some(KeptCopper {
                file,
                kept: Vec::new(),
                again: Vec::new(),
                unused: Some(why),
            }));
        }
    };
    let rip: BTreeSet<String> = nets.iter().cloned().chain(start.rip).collect();
    let carried = netsmith_router::carry::carry_copper(&start.before, &mut board, &rip);
    Ok(Some(KeptCopper {
        file,
        kept: carried.kept,
        again: carried.ripped,
        unused: None,
    }))
}

/// Write `layout` over the layout file at `file`, keeping its comments.
pub fn accept(file: &Path, layout: &Layout) -> Result<(), String> {
    std::fs::write(file, crate::layout::text_over(file, layout))
        .map_err(|e| format!("{}: {e}", file.display()))
}
