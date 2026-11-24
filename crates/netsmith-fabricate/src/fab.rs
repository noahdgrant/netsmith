//! `netsmith board fab`: from source text to the files a board house wants.
//!
//! For each board it builds the design, places and routes a PCB, writes the board and its
//! design rules, and packages Gerbers, a drill file, a bill of materials and a placement
//! file for JLCPCB. It refuses, with the reason, rather than write files that would not
//! make a working board: an unplaceable board, a design rules violation, or an assembled part
//! with no part number. A board the router could not finish is written marked INCOMPLETE
//! (`routing-failure.txt`, a first line of `fab-notes.txt`) so that its routed part can be
//! looked at, and the run still fails. The Gerbers, drill files, placement file and design
//! rules check are netsmith's own (ADR 0034, ADR 0037); KiCad is only the optional
//! cross-check.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use netsmith_check::{Loader, PartSpace, PinFile};
use netsmith_core::{PanelSpec, timings};
use netsmith_design::{Assembly, Sourced};
use netsmith_fab::{
    BomEntry, Fit, KicadCli, Options, Package, PanelBuildSpec, PanelCellFacts, PanelCellInput,
    Side, package,
};
use netsmith_kicad::Mapping;
use netsmith_pcb::{
    Board, Trim, kicad_dru, kicad_pcb, kicad_pcb_filled, kicad_pro, place_options, render_svg,
};
use netsmith_router::{RouteOptions, route_with};
use netsmith_solve::Catalogs;

use crate::order::{BoardFacts, PanelFacts};
use crate::pin_swap::{self, Score};
use netsmith_pipeline::resolve::resolve_generics;

use netsmith_pipeline::{Built, Libraries, outcome_with, render_problems};

/// One board, made.
#[derive(Debug)]
pub struct Fabricated {
    pub board: String,
    /// Where its files were written.
    pub dir: PathBuf,
    /// The Gerber and drill files inside the zip.
    pub gerber_files: Vec<String>,
    /// What was decided, for a person to read.
    pub summary: String,
    /// Things to check before ordering.
    pub notes: String,
    pub files: Vec<PathBuf>,
    /// The numbers an order's summary is made of.
    pub facts: BoardFacts,
    /// Where each part's footprint origin is and how far it is from its body's centre (#457);
    /// none for a panel.
    pub origins: Vec<netsmith_pcb::PartOrigin>,
    /// The requirements on where parts go that the board does not meet and the run lists as
    /// warnings (`constraints = "warn"`), as data (#816); each is also a line of `notes`.
    pub requirements: Vec<netsmith_pcb::RequirementMiss>,
    /// What the pre-check before placing said was only suspicious (lines that cannot all hold,
    /// when constraints are warnings), as data (#998); each is also told as it goes.
    pub place_warnings: Vec<PlaceWarning>,
    /// The nets routed long and the nets with the most vias, as the notes list them (issue
    /// #1194); empty for a panel and a reused package.
    pub routes: netsmith_fab::NetRoutes,
    /// How many stages (`netsmith_core::timings::stage`: resolve, place, route, package...) this
    /// board's run did: the count that shows a reused package, which runs none (issue #841).
    pub stages_run: u64,
    /// The package is one a plain `fab` made, copied (`FabOptions::reuse`), and nothing was
    /// made again. Its `origins` and `requirements` are empty: `fab` reports them, an order does
    /// not read them.
    pub reused: bool,
}

/// A warning of the pre-check before placing, with the sets of lines behind it as data (#998).
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceWarning {
    pub message: String,
    pub conflicts: Vec<netsmith_pcb::Conflict>,
}

impl PrecheckReport {
    /// The warnings with the data behind each.
    pub fn place_warnings(&self) -> Vec<PlaceWarning> {
        self.warnings
            .iter()
            .enumerate()
            .map(|(n, message)| PlaceWarning {
                message: message.clone(),
                conflicts: self.warning_proofs.get(n).cloned().unwrap_or_default(),
            })
            .collect()
    }
}

/// What `netsmith board fab` says when it finds no KiCad, and why there is none: KiCad is switched off
/// (`NETSMITH_NO_KICAD`, which hides an install that may be there) or it was not found. netsmith's
/// own design rules check and its Gerbers need none; KiCad is only the second witness of
/// `--cross-check`, which plots the boards and runs KiCad's design rules check to compare with
/// netsmith's own (ADR 0034, ADR 0037); the first, the viewer comparison of the Gerbers with the
/// board model, needs none (issue #1158). `scripts/check-no-kicad.sh` greps for either lead
/// ("KiCad is switched off", "KiCad is not installed") as the proof that KiCad was hidden.
pub fn no_kicad_note(search: &netsmith_kicad::Search) -> String {
    let why = if search.is_off() {
        format!(
            "KiCad is switched off ({} is set)",
            netsmith_kicad::NO_KICAD_VARIABLE
        )
    } else {
        "KiCad is not installed".to_string()
    };
    format!(
        "note: {why}, so --cross-check runs netsmith's viewer comparison alone (the Gerbers against the board model), not KiCad's plots and design rules check; netsmith's own design rules check still runs"
    )
}

/// Why there is no `kicad-cli` when KiCad is switched off (`NETSMITH_NO_KICAD`), for the message of
/// a command that needs one: the search then finds none whatever `KICAD_CLI` says, so a message
/// that says "set KICAD_CLI" would send a person to a setting that changes nothing. `None` when
/// KiCad is not switched off.
pub fn kicad_switched_off(search: &netsmith_kicad::Search) -> Option<String> {
    search.is_off().then(|| {
        format!(
            "{} is set, which hides every KiCad program and install (kicad-cli too, whatever {} says): unset it to use KiCad",
            netsmith_kicad::NO_KICAD_VARIABLE,
            netsmith_kicad::CLI_VARIABLE
        )
    })
}

/// Build, place, route and package every board in one source file. A system's boards each
/// get a package of their own, in `<out>/<System>/<board>/`. On failure the error is every
/// problem found.
pub fn fab(
    name: &str,
    text: &str,
    out: &Path,
    catalogs: &Catalogs,
    libraries: &Libraries,
    kicad: Option<&dyn KicadCli>,
) -> Result<Vec<Fabricated>, String> {
    fab_with(name, text, &Loader::std(), out, catalogs, libraries, kicad)
}

/// How long the router may work on a board unless told otherwise.
pub const DEFAULT_ROUTE_TIMEOUT: Duration = Duration::from_secs(300);

/// Whether constraints were asked to be strict: `--constraints error` for this run
/// (`flag == Some(false)`, `Some(true)` being `warn`), else `constraints = "error"` in the
/// manifest. Unset is not strict here, though a violated requirement is an error either way
/// (ADR 0045, "Height limits").
pub fn constraints_strict(manifest: Option<&str>, flag: Option<bool>) -> bool {
    match flag {
        Some(warn) => !warn,
        None => manifest == Some("error"),
    }
}

/// How `fab` goes about it.
#[derive(Debug, Clone)]
pub struct FabOptions {
    /// After this long routing a board, stop and report the connections left. `None` is no
    /// limit.
    pub route_timeout: Option<Duration>,
    /// Told which stage `fab` is in and how far the router has got, so that a long run is
    /// not silent. The command line prints these to stderr.
    pub progress: Option<fn(&str)>,
    /// The layout file of the source (ADR 0020 item 7), which is put on each board after it
    /// is placed. Optional: a file that does not exist changes nothing.
    pub layout: Option<PathBuf>,
    /// Package a board although netsmith's design rules check finds errors. The notes say
    /// so. Nothing else needs this: the check runs without KiCad.
    pub allow_drc_violations: bool,
    /// Write the files of a board that fails netsmith's design rules check for a person to look
    /// at, and say so (`board fab --for-review`, issue #1189): the KiCad board and project, the
    /// Gerber zip as `<board>-gerbers-REVIEW.zip`, the SVG, the 3D scene, the STEP file, the
    /// schematic and `fab-notes.txt` with the findings, each marked "FOR REVIEW, NOT FOR ORDER",
    /// and no `bom.csv` and no `cpl.csv`. The run still ends in an error while the check has
    /// errors, and an order never takes such a package (it makes the board again, and the check
    /// stops it). A board that passes is packaged as usual. Not with `allow_drc_violations`.
    ///
    /// It waives the findings of the board's own limits too (issue #1212), which stop a plain
    /// `fab` at placement: a part over a height limit (`too_tall`), a part or a pad in a keepout
    /// (`in_keepout`, `pad_in_keepout`), a part over pins that protrude (`over_pins`), a
    /// collision with a case model, a part of unknown height under a limit when constraints are
    /// strict, and the requirements on where parts go that the placer's own arrangement does not
    /// meet ([`review_waives`]). Each is listed under `FOR REVIEW` in the notes, and a board with
    /// one is packaged for review even when the design rules check passes.
    pub for_review: bool,
    /// With `for_review`, write the pictures of the copper of a package made for review (issue
    /// #1213): `<board>-top-REVIEW.png`, `<board>-inner<n>-REVIEW.png` for each inner signal
    /// layer and `<board>-bottom-REVIEW.png`, the bytes `view pcb --layer <layer> --drc
    /// --islands` draws of the board `fab` routed ([`review_pictures`]). `false` is `board fab
    /// --no-pictures`. A package that is not for review has none, and the ones an earlier run
    /// left are removed.
    pub review_pictures: bool,
    /// After the files are written, compare the copper of each layer of the Gerbers with the
    /// board model's with netsmith's own viewer (`gerber_compare`, issue #1158; needs no KiCad),
    /// and with KiCad installed plot each board with KiCad too and compare the two sets
    /// (ADR 0034), and run KiCad's design rules check and compare its findings with
    /// netsmith's (ADR 0037). Without KiCad the viewer comparison runs alone, and the notes say
    /// so; a difference is an error.
    pub cross_check: bool,
    /// Make only the boards with these labels (`Unit`, `DevKit/carrier`) and skip the rest of
    /// the file. `None` makes every board. An order uses it (ADR 0035).
    pub only: Option<Vec<String>>,
    /// Constraints were asked to be strict (`board fab --constraints error`, or `constraints =
    /// "error"` in the manifest, and not the default): a part under a height limit that states
    /// no height is then an error and not a warning (ADR 0045, "Height limits").
    /// `--allow-drc-violations` does not waive it.
    pub strict_heights: bool,
    /// The pick lock of the source (`pick_lock`): written when it does not exist, compared with
    /// the picks when it does, and with `locked` an error if they differ, before anything is
    /// placed. The solver holds parts to it through the catalogs, which the caller gives. `None`
    /// leaves the lock out of the run.
    pub lock: Option<netsmith_pipeline::pick_lock::Use>,
    /// What `fab` does with the pins that pin requests got when a board does not route (ADR 0101,
    /// `pin_swap`). `None` leaves the pins alone: a board that does not route is INCOMPLETE as
    /// before. The command line gives `Some` unless an order is made.
    pub pins: Option<PinsUse>,
    /// The name of the source file as the pin map for firmware names it (`<board>.pins.h` and
    /// `.pins.json`, which every board with pin requests gets in its output folder): the path
    /// from the project's root. `None` names the file as `fab` was given it.
    pub pin_map_source: Option<String>,
    /// How many times the process's least track the router draws a signal at, where there is
    /// room (`track_margin` in `netsmith.toml`): a board it leaves unfinished at that width is
    /// routed again at the least, so the margin is kept where it fits and given up where it does
    /// not (issue #431). 1 is the least.
    pub track_margin: f64,
    /// The rounds of rip-up and re-route of whole nets the router gives a try that leaves
    /// connections (`rip_rounds` in `netsmith.toml`, `--rip-rounds`; issue #1088): the router's
    /// own, [`netsmith_router::RIP_ROUNDS`], by default, and 0 runs none.
    pub rip_rounds: usize,
    /// The weights of the router's quality objective (`--length-weight`, `--via-weight`,
    /// `--layer-change-weight`; issue #1517); `None`, the default, routes at the router's own
    /// costs with no polish pass.
    pub weights: Option<netsmith_router::Weights>,
    /// Write the board's 3D scene as `<board>.glb` into the board's package folder, beside the
    /// pin map and `fab-notes.txt` and outside the Gerber zip: the scene of the board as routed,
    /// as `dev board3d --glb` writes it (docs/BOARD3D.md). The default is `true`; the command line
    /// gives `false` for `--no-glb` or `glb = false` in `netsmith.toml`. When it is `false`, a
    /// `<board>.glb` that an earlier run left in the folder is removed, as the pin map's files
    /// are, so that the folder never holds a scene of another layout.
    pub glb: bool,
    /// Write the board as placed, just before it is routed, as `<board>.placed.json` into the
    /// board's output folder (`board fab --save-placed`), for `netsmith dev route` to route again with no
    /// placing. The default is `false`.
    pub save_placed: bool,
    /// Write the board as STEP, `<board>.step`, into the board's package folder beside the
    /// `.glb` and outside the Gerber zip, as `dev board3d --step` writes it (ADR 0107,
    /// docs/BOARD3D.md). The default is `true`; the command line gives `false` for `--no-step` or
    /// `step = false` in `netsmith.toml`. When it is `false`, a `<board>.step` that an earlier run
    /// left in the folder is removed, as the `.glb` is.
    pub step: bool,
    /// Write the board's schematic as `<board>-schematic.pdf` into the board's package folder,
    /// outside the Gerber zip: a page for each sheet, as the editor draws it (issue #1091,
    /// `netsmith_view::export`). The default is `true`; the command line gives `false` for
    /// `--no-pdf` or `pdf = false` in `netsmith.toml`. When it is `false`, a
    /// `<board>-schematic.pdf` an earlier run left in the folder is removed, as the `.glb` is.
    pub pdf: bool,
    /// What the schematic PDF is drawn with besides the board: the schematic layout file beside
    /// the source and the project's name. `None` draws it with no layout file and names no
    /// project.
    pub schematic: Option<crate::schematic::Inputs>,
    /// The project's revision (`revision` in `netsmith.toml`, issue #1134): the first line of
    /// `fab-notes.txt` says it. `None` says none.
    pub revision: Option<String>,
    /// Where the files of the target are, to find the STEP models its parts are drawn from
    /// (ADR 0108) and its case models (ADR 0109): a path outside the project stops `fab`, a
    /// file that is not there is a warning (a case model's, an error with `strict_heights`).
    /// `None` finds none, and every part is drawn with its generated shape.
    pub models: Option<crate::models::Places>,
    /// How many seeds the placer tries for a board, from the layout file's (`place_seeds` in
    /// `netsmith.toml`, issue #458; [`crate::layout::DEFAULT_PLACE_SEEDS`] when it does not
    /// say).
    pub place_seeds: u32,
    /// How long the placer may look for a placement of a board before it gives the closest
    /// try (`--place-budget`, `place_budget` in `netsmith.toml`, issue #764). The default is
    /// no limit.
    pub place_budget: netsmith_core::job::Limit,
    /// The weight of the placer's objective for routing (`--routing-objective`,
    /// `routing_objective` in `netsmith.toml`, issue 1544). The default is 0: the objective is
    /// left out, and every board places as it did before.
    pub routing_objective: f64,
    /// Whether a seed that places the board and is not the layout file's may be written to the
    /// file (issue #458): `fab` writes it; an order and the editor's release never change a
    /// choice by themselves, and stop instead.
    pub seed_writes: SeedWrites,
    /// The run was given `--offline`: no supplier was asked, so the stock and price of the parts
    /// were not checked, and `fab-notes.txt` says so (issue #803). The default is `false`.
    pub offline: bool,
    /// What the run does about reusing packages (issue #841): [`Reuse::Off`] by default.
    pub reuse: Reuse,
    /// Start each board's route from the router's copper kept on disk
    /// (`crate::kept_routes`, `board fab --reuse-routes`, issue #1019), keeping the copper of every
    /// net the placement left alone, and keep the copper routed for the next run. Needs
    /// `layout`, which says where the source is. The default is `false`: the board is routed
    /// from nothing and nothing is kept, so that a clean checkout gives the same board.
    pub reuse_routes: bool,
    /// Write each board's KiCad files as a self-contained project (`board fab --portable`, issue
    /// #1198): the symbols and footprints the board uses and their 3D models are copied into the
    /// package's folder, with library tables and `(model ...)` entries relative to it
    /// ([`netsmith_pipeline::portable`]). The default is `false`.
    pub portable: bool,
}

/// What a run does about reusing the package of a board (`crate::reuse`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Reuse {
    /// Makes every board, and leaves no record of how it was made.
    #[default]
    Off,
    /// A plain `fab`: makes every board, and writes beside each package a record of the inputs
    /// it was made from, which an order can reuse it by.
    Record,
    /// An order: takes a board's package from the folder of a plain `fab` (`<fab>`, where
    /// `<fab>/<board>/` is the package) when the record there says the same inputs made it and
    /// no file of it has changed, and makes the board again when not. A board of a system and a
    /// panel are always made again.
    From(PathBuf),
}

/// What a run may do with a seed that places the board and is not the layout file's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SeedWrites {
    /// Write it to the layout file (`fab`).
    #[default]
    Write,
    /// Stop with an error that says the seed: an order (`project order`) is a release.
    Order,
    /// Stop with an error that says the seed: the editor's release is a release.
    EditorRelease,
}

/// Whether `fab` may move pins by itself, and where the pin file is.
#[derive(Debug, Clone)]
pub struct PinsUse {
    pub mode: PinsMode,
    /// The pin file beside the source (`<stem>.pins.toml`), which `auto` rewrites.
    pub file: PathBuf,
    /// The clock the pin optimizer's time budget runs on: the system's, unless a test moves its
    /// own.
    pub clock: pin_swap::Clock,
    /// What made the mode `locked`, for the messages: they name what the person gave and never
    /// advise dropping a flag they did not pass. [`LockedBy::PinsFlag`] unless told.
    pub locked_by: LockedBy,
}

/// What a person gave that makes [`PinsMode::Locked`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LockedBy {
    /// `--pins locked`.
    #[default]
    PinsFlag,
    /// `--locked`, which implies it.
    LockedFlag,
    /// `pins = "locked"` in `netsmith.toml`.
    Manifest,
}

impl LockedBy {
    /// What was given, as the messages say it.
    fn given(self) -> &'static str {
        match self {
            LockedBy::PinsFlag => "--pins locked",
            LockedBy::LockedFlag => "--locked",
            LockedBy::Manifest => "`pins = \"locked\"` in netsmith.toml",
        }
    }

    /// How to let the pins move: drop what was given, or the flag that wins over it.
    fn way_out(self) -> &'static str {
        match self {
            LockedBy::PinsFlag => "run without --pins locked",
            LockedBy::LockedFlag => "run without --locked",
            LockedBy::Manifest => {
                "run with --pins auto (or set `pins = \"auto\"` in netsmith.toml)"
            }
        }
    }
}

impl PinsUse {
    /// The mode and the file, on the system clock.
    pub fn new(mode: PinsMode, file: PathBuf) -> PinsUse {
        PinsUse {
            mode,
            file,
            clock: pin_swap::Clock::real(),
            locked_by: LockedBy::default(),
        }
    }
}

/// `--pins`: what a routing failure may do to the pin map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinsMode {
    /// Swap interchangeable pins, place and route again, and write the pin file.
    Auto,
    /// Change nothing. Write the swaps that would help to `pins.proposed.toml` in the output
    /// folder, and fail as before.
    Locked,
}

impl PinsMode {
    /// `"auto"` or `"locked"`, as `--pins` and the manifest spell it.
    pub fn parse(word: &str) -> Option<PinsMode> {
        match word {
            "auto" => Some(PinsMode::Auto),
            "locked" => Some(PinsMode::Locked),
            _ => None,
        }
    }
}

impl Default for FabOptions {
    fn default() -> Self {
        Self {
            route_timeout: Some(DEFAULT_ROUTE_TIMEOUT),
            progress: None,
            layout: None,
            allow_drc_violations: false,
            for_review: false,
            review_pictures: true,
            cross_check: false,
            only: None,
            strict_heights: false,
            lock: None,
            pins: None,
            pin_map_source: None,
            track_margin: DEFAULT_TRACK_MARGIN,
            rip_rounds: netsmith_router::RIP_ROUNDS,
            weights: None,
            glb: true,
            save_placed: false,
            step: true,
            pdf: true,
            schematic: None,
            revision: None,
            models: None,
            place_seeds: crate::layout::DEFAULT_PLACE_SEEDS,
            place_budget: netsmith_core::job::Limit::default(),
            routing_objective: 0.0,
            seed_writes: SeedWrites::Write,
            offline: false,
            reuse: Reuse::Off,
            reuse_routes: false,
            portable: false,
        }
    }
}

/// The line of `fab-notes.txt` of a run with `--offline`: the stock and price of the parts were
/// not checked (the bill of materials of the package names neither).
pub const OFFLINE_NOTE: &str = "offline: stock and price of the parts were not checked (no supplier was asked); look each part up at the supplier before you order";

/// The rounds of rip-up the router gives a try that leaves connections, as the project says
/// (`rip_rounds` in `netsmith.toml`), or the router's own (issue #1088).
pub fn rip_rounds(project: Option<&crate::project::Project>) -> usize {
    project
        .and_then(|p| p.manifest.rip_rounds)
        .unwrap_or(netsmith_router::RIP_ROUNDS)
}

/// The weights of the router's quality objective the project's keys give (`length_weight`,
/// `via_weight`, `layer_change_weight` of `netsmith.toml`, issue 1517), the router's own for a
/// key not stated; `None` when none is stated, which routes as the router always has. The reason
/// when a weight is out of its range.
pub fn weights_of(
    manifest: &crate::project::Manifest,
) -> Result<Option<netsmith_router::Weights>, String> {
    weights_with(Some(manifest), (None, None, None))
}

/// [`weights_of`] the project, for the editor and the orders, which have no flags: a project
/// that parsed has weights in their ranges, so there is no error to tell here.
pub fn project_weights(
    project: Option<&crate::project::Project>,
) -> Option<netsmith_router::Weights> {
    weights(project, (None, None, None)).ok().flatten()
}

/// The line `fab` tells of the weights it routes with (issue 1536), so a run says which the flags
/// and the project's keys gave.
pub fn weights_line(w: &netsmith_router::Weights) -> String {
    format!(
        "with the weights of the quality objective: length {} of the router's own, via {} mm, layer change {} mm",
        w.length, w.via, w.layer_change
    )
}

/// The weights of a run: each of the three flags (`--length-weight`, `--via-weight`,
/// `--layer-change-weight`) wins over the project's key of that name, and the router's own
/// weight stands for one neither states; `None` when none is stated at all.
pub fn weights(
    project: Option<&crate::project::Project>,
    flags: (Option<f64>, Option<f64>, Option<f64>),
) -> Result<Option<netsmith_router::Weights>, String> {
    weights_with(project.map(|p| &p.manifest), flags)
}

fn weights_with(
    manifest: Option<&crate::project::Manifest>,
    (length, via, layer_change): (Option<f64>, Option<f64>, Option<f64>),
) -> Result<Option<netsmith_router::Weights>, String> {
    let key = |f: fn(&crate::project::Manifest) -> Option<f64>| manifest.and_then(f);
    let length = length.or(key(|m| m.length_weight));
    let via = via.or(key(|m| m.via_weight));
    let layer_change = layer_change.or(key(|m| m.layer_change_weight));
    if length.is_none() && via.is_none() && layer_change.is_none() {
        return Ok(None);
    }
    let own = netsmith_router::Weights::DEFAULT;
    let w = netsmith_router::Weights {
        length: length.unwrap_or(own.length),
        via: via.unwrap_or(own.via),
        layer_change: layer_change.unwrap_or(own.layer_change),
    };
    w.check()?;
    Ok(Some(w))
}

/// How many times the process's least track a signal is drawn at where there is room, when the
/// project does not say (`track_margin` in `netsmith.toml`, issue #431): JLCPCB's page says of its
/// 0.1 mm least "stay well above if possible", and 1.5 times it is 0.15 mm.
pub const DEFAULT_TRACK_MARGIN: f64 = 1.5;

/// [`fab`], with imports resolved by `loader`.
pub fn fab_with(
    name: &str,
    text: &str,
    loader: &Loader,
    out: &Path,
    catalogs: &Catalogs,
    libraries: &Libraries,
    kicad: Option<&dyn KicadCli>,
) -> Result<Vec<Fabricated>, String> {
    fab_with_options(
        name,
        text,
        loader,
        out,
        catalogs,
        libraries,
        kicad,
        &FabOptions::default(),
    )
}

/// [`fab_with`], with a choice of the router's time limit and a way to follow progress.
#[allow(clippy::too_many_arguments)]
pub fn fab_with_options(
    name: &str,
    text: &str,
    loader: &Loader,
    out: &Path,
    catalogs: &Catalogs,
    libraries: &Libraries,
    kicad: Option<&dyn KicadCli>,
    options: &FabOptions,
) -> Result<Vec<Fabricated>, String> {
    fab_detailed(name, text, loader, out, catalogs, libraries, kicad, options).map_err(|f| f.text)
}

/// Why a fab run stopped: the text the command line prints, and the problems of the build when
/// it was the build that stopped it (for `--format json`, #455). A failure of a later step (the
/// placer, the router, the design rules check, a file) has the text alone.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FabFailure {
    pub text: String,
    pub problems: Vec<netsmith_pipeline::Problem>,
    /// The requirements on where parts go that the board does not meet, as data (#816), when
    /// that is why the run stopped: where the placer ended, or on the board file after routing.
    pub requirements: Vec<netsmith_pcb::RequirementMiss>,
    /// The parts that do not fit the outline, as data (#817), when that is why the run
    /// stopped.
    pub does_not_fit: Option<Box<netsmith_pcb::FitReport>>,
    /// The sets of lines and locks that cannot all hold, as data (#779), when that is why the
    /// run stopped: proved before placing, or where the placer ended.
    pub conflicts: Box<[netsmith_pcb::Conflict]>,
    /// The faults of the placement check where the placer ended, as data (#779), when they are
    /// why the run stopped.
    pub placement: Box<[netsmith_pcb::PlacementFault]>,
    /// The package `--for-review` made, when it made one (issue #1212).
    pub review: Option<Box<Review>>,
}

/// A package made for review: its board, and what `--for-review` waived of it (issue #1212).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Review {
    /// The board, as its label (`Unit`, `DevKit/carrier`).
    pub board: String,
    /// Each finding waived, as data, in the order the notes' `FOR REVIEW` heading lists them.
    pub waived: Vec<Waived>,
}

/// A finding `board fab --for-review` waived (issue #1212): a line of the `FOR REVIEW` heading
/// of the notes, and a finding of code `fab` with `waived: "for-review"` in `--format json`.
#[derive(Debug, Clone, PartialEq)]
pub struct Waived {
    /// The rule that found it: a fault of the placement check (`too_tall`, `in_keepout`, ...), a
    /// case model's finding (`case_collision`, ...), `height_unchecked`, or a rule of the design
    /// rules check (`clearance`, `constraint`, ...).
    pub rule: String,
    /// The sentence, with the part, the numbers and the place.
    pub message: String,
    /// The fault of the placement check, with its parts and numbers, when it is one.
    pub placement: Option<netsmith_pcb::PlacementFault>,
    /// The finding of the design rules check, with its place, nets and parts, when it is one.
    pub drc: Option<netsmith_geom::DrcFinding>,
}

impl Waived {
    /// The line of the notes: `error [rule] message`, as the design rules check words its own.
    pub fn line(&self) -> String {
        match &self.drc {
            Some(f) => f.to_string(),
            None => format!("error [{}] {}", self.rule, self.message),
        }
    }

    fn of_fault(fault: netsmith_pcb::PlacementFault) -> Waived {
        // The fault's sentence names its place where it has one (`too_tall`); for the others
        // the box each part's courtyard takes is the place.
        let mut message = fault.text.clone();
        if fault.kind != "too_tall" {
            for part in &fault.parts {
                if let Some(c) = &part.courtyard {
                    message.push_str(&format!(
                        "; the courtyard of {} is ({:.2}, {:.2}) to ({:.2}, {:.2}) mm from the board's top left corner",
                        part.part.refdes, c.x_min, c.y_min, c.x_max, c.y_max
                    ));
                }
            }
        }
        Waived {
            rule: fault.kind.to_string(),
            message,
            placement: Some(fault),
            drc: None,
        }
    }

    fn of_drc(finding: &netsmith_geom::DrcFinding) -> Waived {
        Waived {
            rule: finding.rule.id().to_string(),
            message: finding.with_items(),
            placement: None,
            drc: Some(finding.clone()),
        }
    }
}

/// Whether `--for-review` waives a fault of the placement check of this kind (issue #1212): the
/// limits the board states on what may stand where (a keepout, a height limit, pins that
/// protrude), which a board breaks and can still be made and assembled as drawn. The faults that
/// say the parts do not fit the board itself (courtyards that overlap, a part outside the outline
/// or over a cutout, a pad off or too near an edge, holes too near) or that the board cannot be
/// drawn (a cutout outside the outline, a keepout with no place) still stop `fab`.
pub fn review_waives(kind: &str) -> bool {
    matches!(
        kind,
        "too_tall" | "in_keepout" | "pad_in_keepout" | "over_pins"
    )
}

impl From<String> for FabFailure {
    fn from(text: String) -> Self {
        FabFailure {
            text,
            ..FabFailure::default()
        }
    }
}

/// [`fab_with_options`], with the problems of a build that stops it kept as data.
#[allow(clippy::too_many_arguments)]
pub fn fab_detailed(
    name: &str,
    text: &str,
    loader: &Loader,
    out: &Path,
    catalogs: &Catalogs,
    libraries: &Libraries,
    kicad: Option<&dyn KicadCli>,
    options: &FabOptions,
) -> Result<Vec<Fabricated>, FabFailure> {
    let say = |message: String| {
        if let Some(progress) = options.progress {
            progress(&message);
        }
    };
    let mut made = Vec::new();
    let outcome =
        outcome_with(name, text, loader, catalogs, Some(libraries)).map_err(|problems| {
            FabFailure {
                text: render_problems(&problems),
                problems,
                ..FabFailure::default()
            }
        })?;
    if let Some(using) = &options.lock {
        let settled =
            netsmith_pipeline::pick_lock::settle(using, catalogs.pick_lock(), &outcome.lock)
                .map_err(|e| format!("{e}\n"))?;
        if let Some(file) = settled.wrote {
            say(format!("wrote {}", file.display()));
        }
        for note in settled.notes {
            say(format!("note: {note}"));
        }
    }
    let mut pin_state = PinState::new(&outcome.pins);
    let (mut built_boards, panels, systems) = (outcome.boards, outcome.panels, outcome.systems);
    // The STEP models parts are drawn from and the case models, found on disk (ADR 0108,
    // ADR 0109).
    if let Some(places) = &options.models {
        let mut errors = String::new();
        for built in &mut built_boards {
            let label = built.label();
            let found = crate::models::locate(&mut built.design, places, Some(&libraries.mapping));
            for warning in found.warnings {
                say(format!("warning: {label}: {warning}"));
            }
            for note in found.notes {
                say(format!("note: {label}: {note}"));
            }
            for error in found.errors {
                errors.push_str(&format!("error: {label}: {error}\n"));
            }
            // A case model that is not there leaves the fit unchecked (ADR 0109): an error
            // when constraints are strict, as a part of unknown height under a limit is.
            for unchecked in found.unchecked {
                if options.strict_heights {
                    errors.push_str(&format!(
                        "error: {label}: {unchecked}; this is an error because constraints are strict (`--constraints error` or `constraints = \"error\"`), and `--allow-drc-violations` does not waive it: add the file, or run with `--constraints warn`\n"
                    ));
                } else {
                    say(format!("warning: {label}: {unchecked}"));
                }
            }
        }
        if !errors.is_empty() {
            return Err(errors.into());
        }
    }
    let board_names: Vec<String> = built_boards.iter().map(|b| b.design.name.clone()).collect();
    // The layout file beside the source, if there is one: it says which face each part is on
    // and where things are, and a wrong entry stops fab before anything is placed.
    let layout = match &options.layout {
        Some(file) => {
            crate::layout::read(file, &board_names).map_err(|e| format!("error: {name}: {e}\n"))?
        }
        None => None,
    };
    // The routes file beside it is read now too, so a wrong one stops fab before anything is
    // placed; each board puts its table on after it is placed.
    if let Some(file) = &options.layout {
        crate::routes::read(&crate::routes::beside_layout(file), &board_names)
            .map_err(|e| format!("error: {name}: {e}\n"))?;
    }
    // A board of a system defined in another file of the project reads the layout file beside
    // that file, and the routes file beside it (issue #1327, `crate::layout::board_file`); a
    // wrong one stops fab before anything is placed too. Each such board is made with the
    // options naming its own file, so its seed and its copper go there.
    let mut own: BTreeMap<String, (LayoutFile, FabOptions)> = BTreeMap::new();
    if let Some(file) = &options.layout {
        for built in &built_boards {
            let label = built.label();
            let fail = |e: String| format!("error: {label}: {e}\n");
            let (read, at) =
                crate::layout::read_board(built, loader, file, &board_names).map_err(fail)?;
            if let Some(note) = &at.note {
                say(format!("{label}: note: {note}"));
            }
            if at.own {
                crate::routes::load(&crate::routes::beside_layout(&at.file)).map_err(fail)?;
                let mut theirs = options.clone();
                theirs.layout = Some(at.file);
                own.insert(label, (read, theirs));
            }
        }
    }
    // The area each member of a system lies under or over another member, from the mates: a
    // keepout-by-height carried by its connector, which its placer and placement check keep to
    // (issue 1331, `crate::system_areas`).
    for (label, note) in crate::system_areas::add(
        &mut built_boards,
        &systems,
        loader,
        options.layout.as_deref(),
        libraries,
    ) {
        say(format!("{label}: note: {note}"));
    }
    // Two requests that are one signal in the C header of the pin map are an error, said now
    // with every other check of the source and before any board is placed or written.
    let mut collisions = String::new();
    for built in &built_boards {
        let source = options.pin_map_source.as_deref().unwrap_or(name);
        let spaces = pin_state.spaces_of(built);
        if let Err(message) = crate::pin_export::render(&built.design, spaces, source) {
            collisions.push_str(&format!("{message}\n"));
        }
    }
    if !collisions.is_empty() {
        return Err(collisions.into());
    }
    // Each board as made, routed, by label: a system's scene is made of its members'.
    let mut routed: BTreeMap<String, Board> = BTreeMap::new();
    for built in &built_boards {
        if let Some(only) = &options.only
            && !only.contains(&built.label())
        {
            continue;
        }
        // The key of the package this board's inputs make (`crate::reuse`): a board of a system
        // is made again whatever the options say, its scene is made of its members as routed.
        let label = built.label();
        let key_of = |pin_state: &PinState| {
            if built.system.is_some() || options.reuse == Reuse::Off {
                return None;
            }
            let source = options.pin_map_source.as_deref().unwrap_or(name);
            crate::reuse::key(&crate::reuse::Inputs {
                built,
                libraries,
                catalogs,
                options,
                layout: layout.as_ref().map(|l| &l.0),
                pin_map: format!(
                    "{:?}",
                    crate::pin_export::render(&built.design, pin_state.spaces_of(built), source)
                ),
            })
        };
        if let Reuse::From(root) = &options.reuse
            && built.system.is_none()
        {
            let from = built.dir(root);
            match key_of(&pin_state) {
                None => say(format!(
                    "{label}: made again: the inputs of a package could not be fingerprinted"
                )),
                Some(key) => match crate::reuse::take(&from, &built.dir(out), &key, &label) {
                    Ok(reused) => {
                        say(format!(
                            "{label}: reused the package a plain fab made in {} (no stage run)",
                            from.display()
                        ));
                        made.push(reused);
                        continue;
                    }
                    Err(why) => say(format!("{label}: made again, not reused: {why}")),
                },
            }
        }
        let stages_before = timings::stages_run();
        let (layout, options) = match own.get(&label) {
            Some((layout, options)) => (layout, options),
            None => (&layout, options),
        };
        let mut run = fab_board_pinned(
            &Pass { name, text, loader },
            built,
            &mut pin_state,
            out,
            layout,
            libraries,
            kicad,
            options,
            catalogs,
            &say,
        )?;
        run.fabricated.stages_run = timings::stages_run() - stages_before;
        // A plain `fab` leaves what an order needs to reuse the package, unless pins were moved:
        // the package is then of a pin file that is no longer the one on disk.
        if options.reuse == Reuse::Record
            && pin_changes(&pin_state.original, &pin_state.file).is_empty()
            && let Some(key) = key_of(&pin_state)
            && let Err(e) = crate::reuse::write_record(&run.fabricated, &key)
        {
            say(format!(
                "{label}: warning: no record of the package was written: {e}"
            ));
        }
        routed.insert(label, run.board);
        made.push(run.fabricated);
    }
    // The system scene of each system whose boards were all made (ADR 0110): checked across
    // boards, and written beside them.
    for system in &systems {
        let members = crate::system3d::members(system, &built_boards);
        let boards: Option<Vec<(&Built, &Board)>> = members
            .iter()
            .map(|b| routed.get(&b.label()).map(|board| (*b, board)))
            .collect();
        let Some(boards) = boards else { continue };
        fab_system(
            system,
            &boards,
            out,
            libraries,
            options,
            catalogs.strict_guidelines(),
            &say,
        )?;
    }
    // The panels of the file (ADR 0040): each repeats a board of it, which is made again, in
    // the panel's own folder.
    for panel in &panels {
        if let Some(only) = &options.only
            && !only.contains(&panel.name)
        {
            continue;
        }
        let fail = |what: String| format!("error: panel {}: {what}\n", panel.name);
        let built = built_boards
            .iter()
            .find(|b| b.system.is_none() && b.board == panel.cell)
            .ok_or_else(|| fail(format!("it repeats `{}`, which is not built", panel.cell)))?;
        let dir = out.join(&panel.name);
        say(format!(
            "{}: making {}, its board, first",
            panel.name, panel.cell
        ));
        let run = fab_board_pinned(
            &Pass { name, text, loader },
            built,
            &mut pin_state,
            &dir.join("cell"),
            &layout,
            libraries,
            kicad,
            options,
            catalogs,
            &say,
        )?;
        say(format!(
            "{}: {} x {} cells of {}",
            panel.name, panel.columns, panel.rows, panel.cell
        ));
        made.push(fab_panel(panel, &run, &dir, kicad, options, &say)?);
    }
    Ok(made)
}

/// What the requirements prove about a board before any placing (ADR 0045 section 14): lines
/// that cannot all hold, a courtyard past an edge nobody stated, a part
/// bigger than the outline. It takes milliseconds and starts no placer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrecheckReport {
    /// The board's label.
    pub board: String,
    /// What is proved, each as a message with the numbers and the lines.
    pub errors: Vec<String>,
    /// What is only suspicious.
    pub warnings: Vec<String>,
    /// The lines that cannot all hold among `errors`, and each fault about one part (a courtyard
    /// past an edge nobody stated, a part bigger than the outline), as data (#779).
    pub conflicts: Vec<netsmith_pcb::Conflict>,
    /// For a lock of the layout file that breaks a requirement, the requirements not met on the
    /// board as the lock leaves it, as data (#779); empty for any other refusal.
    pub requirements: Vec<netsmith_pcb::RequirementMiss>,
    /// The faults of the placement check on that board (#779).
    pub placement: Vec<netsmith_pcb::PlacementFault>,
    /// What each of `errors` is about, as data, in the same order (#998): the sets of lines
    /// and the faults about one part (`past_edge`, `too_big`) behind it.
    pub error_proofs: Vec<Vec<netsmith_pcb::Conflict>>,
    /// The same for `warnings`.
    pub warning_proofs: Vec<Vec<netsmith_pcb::Conflict>>,
}

/// The pre-check of one built board with the layout file's locks, if any. A board the
/// libraries cannot give footprints for (no KiCad install, a part with no mapping) has nothing
/// proved here: `build` has never needed them, and `fab` reports that when it places. A lock
/// that breaks a requirement is refused here, with the board as the lock leaves it as data
/// (`requirements`, `placement`, issue #779). A part the placer places that has no room among
/// the parts that stay where they are ([`netsmith_pcb::parts_without_room`], issue #1283) is a
/// warning, an error when `strict` (`--strict`, or `guidelines = "error"`).
pub fn precheck_board(
    built: &Built,
    layout: Option<&(netsmith_pcb::Layout, String)>,
    libraries: &Libraries,
    constraints_warn: bool,
    strict: bool,
) -> PrecheckReport {
    let mut report = PrecheckReport {
        board: built.label(),
        ..PrecheckReport::default()
    };
    let mut placement = place_options(&built.design, layout.map(|l| &l.0));
    placement.lines_warn = constraints_warn;
    match netsmith_placer::precheck(
        &built.design,
        &libraries.mapping,
        &libraries.footprints,
        &placement,
    ) {
        Ok(found) => {
            report.errors = found.errors();
            report.error_proofs = found.error_proofs();
            report.conflicts = found.all_proofs();
            report.warning_proofs = found.warning_proofs();
            report.warnings = found.warnings;
        }
        Err(e @ netsmith_pcb::BoardError::Constraints(_)) => {
            report.errors = vec![e.to_string()];
            report.error_proofs = vec![Vec::new()];
            let failure = crate::layout::PlaceFailure::Board(e);
            if let Some(ended) = crate::layout::where_ended(
                &built.design,
                layout,
                libraries,
                constraints_warn,
                &failure,
            ) {
                let explained = ended.explain(&built.design);
                report.requirements = explained.misses;
                report.error_proofs = vec![explained.proofs.clone()];
                report.conflicts = explained.proofs;
                report.placement = ended.board.placement_faults();
            }
        }
        Err(_) => {}
    }
    // Each part the placer places against the room the parts that stay leave it (issue #1283),
    // when nothing above stops the board: a part bigger than the outline is said there.
    if report.errors.is_empty()
        && let Ok(room) = netsmith_pcb::parts_without_room(
            &built.design,
            &libraries.mapping,
            &libraries.footprints,
            &placement,
        )
    {
        // Each message keeps its own proofs, by its place in the list.
        report.error_proofs.resize(report.errors.len(), Vec::new());
        report
            .warning_proofs
            .resize(report.warnings.len(), Vec::new());
        for part in room.no_room {
            if strict {
                report.errors.push(format!(
                    "{part}; this is an error because the run is strict (`--strict` or `guidelines = \"error\"`)"
                ));
                report.error_proofs.push(Vec::new());
            } else {
                report.warnings.push(part.to_string());
                report.warning_proofs.push(Vec::new());
            }
        }
    }
    report
}

/// The file in the board's fab folder that lists every part's final place and state when
/// the parts do not fit (PROVISIONAL, ADR 0033).
const PLACEMENT_FAILURE: &str = "placement-failure.txt";

/// The failure of a refusal because the parts do not fit: the message, and the refusal as data
/// (issue #817), with the requirements it found not met (issue #816).
fn does_not_fit(text: String, e: &netsmith_pcb::DoesNotFit) -> FabFailure {
    FabFailure {
        text,
        requirements: e
            .requirements
            .iter()
            .flat_map(|r| r.misses.iter().cloned())
            .collect(),
        does_not_fit: Some(Box::new(e.report())),
        conflicts: e
            .requirements
            .iter()
            .flat_map(|r| r.proofs.iter().cloned())
            .collect(),
        ..FabFailure::default()
    }
}

/// The refusal's message, with the full list written to `placement-failure.txt` in `out` and
/// named; if it cannot be written, the message says so and carries the list.
fn placement_failure(e: &netsmith_pcb::DoesNotFit, out: &Path) -> String {
    let file = out.join(PLACEMENT_FAILURE);
    let written =
        std::fs::create_dir_all(out).and_then(|()| std::fs::write(&file, e.full_report()));
    match written {
        Ok(()) => format!(
            "{e}\n  every part's final place and state ({} parts) is in {}",
            e.parts.len(),
            file.display()
        ),
        Err(err) => format!(
            "{e}\n  (the list of every part's final place and state could not be written to {}: {err})",
            file.display()
        ),
    }
}

/// The requirements of the physical file that the arrangement the placer ended on does not
/// meet, as the message of the first error that `fab` prints for it, with the list also
/// written to `placement-failure.txt` in `out` with the faults and every part's place.
fn requirement_failure(
    report: &netsmith_pcb::RequirementReport,
    problems: &[String],
    board: &Board,
    out: &Path,
) -> String {
    let mut text = format!("{report}\n");
    if !problems.is_empty() {
        text.push_str("\nThe faults the placement check found where the placer ended:\n");
        for p in problems {
            text.push_str(&format!("  {p}\n"));
        }
    }
    text.push_str(
        "\nEvery part where the placer ended (the origin of each footprint is in mm from the outline's top left corner, y down):\n",
    );
    for p in &board.parts {
        text.push_str(&format!(
            "{} ({}) {} at ({:.2}, {:.2}) turned {}\n",
            netsmith_pcb::board_member(&p.path, &p.refdes),
            p.kind,
            p.side.word(),
            p.at.x - board.outline.min.x,
            p.at.y - board.outline.min.y,
            p.rotation
        ));
    }
    let file = out.join(PLACEMENT_FAILURE);
    match std::fs::create_dir_all(out).and_then(|()| std::fs::write(&file, text)) {
        Ok(()) => format!(
            "{report}\n  every part's final place{} is in {}",
            if problems.is_empty() {
                ""
            } else {
                " and the faults that follow"
            },
            file.display()
        ),
        Err(err) => format!(
            "{report}\n  (the list could not be written to {}: {err})",
            file.display()
        ),
    }
}

/// What a seed after the layout file's that placed the board does (issue #458): it is said,
/// with the seed it replaces (stale, if the file states it) and what blocked each seed tried,
/// and written to the layout file as the board's seed, so that the next run starts from it.
/// With `--locked` nothing may be written, so a seed that changed is an error, as a pick lock
/// that differs is. A run with no layout file to name (`FabOptions::layout`) only says it.
fn settle_seed(
    (label, board): (&str, &str),
    seeds: &crate::layout::Seeds,
    layout: &LayoutFile,
    options: &FabOptions,
    say: &dyn Fn(String),
) -> Result<(), String> {
    let fail = |what: String| format!("error: {label}: {what}\n");
    let file = options
        .layout
        .as_ref()
        .map(|f| f.display().to_string())
        .or_else(|| layout.as_ref().map(|(_, name)| name.clone()));
    let name = file
        .clone()
        .unwrap_or_else(|| "the layout file".to_string());
    let Some(note) = seeds.note(&name) else {
        return Ok(());
    };
    let locked = options.lock.as_ref().is_some_and(|l| l.locked);
    let release = match options.seed_writes {
        SeedWrites::Write => None,
        SeedWrites::Order => Some("an order"),
        SeedWrites::EditorRelease => Some("a release from the editor"),
    };
    if let (Some(what), Some(_)) = (release, &options.layout) {
        return Err(fail(format!(
            "{note}, and {what} is a release, which never changes a choice by itself: run `netsmith board fab` on the source to write `seed = {}` to {name}, review it and commit it, then make the release again",
            seeds.used
        )));
    }
    match (&options.layout, locked) {
        (Some(_), true) => Err(fail(format!(
            "{note}, and --locked says {name} must not be written: run without --locked to write `seed = {}` to it, and commit it",
            seeds.used
        ))),
        (Some(path), false) => {
            crate::layout::write_seed(path, board, seeds.used).map_err(fail)?;
            say(format!("{label}: note: {note}, and is written to {name}"));
            Ok(())
        }
        (None, _) => {
            say(format!("{label}: note: {note}"));
            Ok(())
        }
    }
}

/// One board, made: placed, routed, written and packaged, with the work's results kept for
/// a panel that repeats it.
struct BoardRun {
    fabricated: Fabricated,
    board: Board,
    package: Package,
    entries: Vec<BomEntry>,
    fit: BTreeMap<String, Fit>,
    pcb: PathBuf,
    options: Options,
}

type LayoutFile = Option<(netsmith_pcb::Layout, String)>;

#[allow(clippy::too_many_arguments)]
fn fab_board(
    built: &Built,
    out: &Path,
    layout: &LayoutFile,
    libraries: &Libraries,
    kicad: Option<&dyn KicadCli>,
    options: &FabOptions,
    catalogs: &Catalogs,
    say: &dyn Fn(String),
    stuck: &mut Option<Stuck>,
    pin_notes: &[String],
    pin_map_of: &PinMapOf,
) -> Result<BoardRun, FabFailure> {
    {
        let fail = |what: String| format!("error: {}: {what}\n", built.label());
        let name = &built.design.name;
        // The pin map for firmware is of the pins THIS pass was made with, which are the pins
        // of the board it writes, so a pass whose files are kept or put back (see
        // `fab_board_pinned`) leaves the map of its own board beside it. A signal name that
        // collides in C is an error here, before anything is placed.
        let pin_map = pin_map_of.render(built)?;

        // A generic part (ADR 0047) becomes the row that stands for it now; one that no row
        // stands for cannot be ordered, and is refused by name with the nearest rows.
        let label = built.label();
        let (sourcing, unresolved) = timings::stage("fab.resolve", &label, || {
            resolve_generics(&built.sourcing, catalogs)
        });
        let unresolved: Vec<String> = unresolved
            .iter()
            .filter(|u| {
                built
                    .sourcing
                    .iter()
                    .any(|s| s.refdes == u.refdes && s.needs_part_number())
            })
            .map(|u| fail(u.line(&built.rechecks, catalogs)))
            .collect();
        // Every part to assemble needs a supplier part number. All that lack one are named
        // together, now and not after the routing, with what to write to say that the board
        // house is not to assemble them.
        let unnumbered = crate::no_supplier::find(built, &sourcing);
        if !unnumbered.is_empty() || !unresolved.is_empty() {
            let mut errors = unresolved.concat();
            if !unnumbered.is_empty() {
                errors.push_str(&fail(crate::no_supplier::message(
                    &built.board,
                    &unnumbered,
                    catalogs.channels(),
                )));
            }
            return Err(errors.into());
        }

        // The generic parts are judged against the board's environment with the rows they were
        // resolved to: a row outside it would have been ordered, so nothing is placed (ADR 0099).
        let environment = crate::fab_environment::judge_resolved(built, &sourcing)
            .map_err(|errors| errors.into_iter().map(fail).collect::<String>())?;

        // What the requirements prove cannot be placed is said now, before the placer starts
        // (it can take a minute on a tight board).
        let precheck = timings::stage("fab.precheck", &label, || {
            precheck_board(
                built,
                layout.as_ref(),
                libraries,
                catalogs.constraints_warn(),
                catalogs.strict_guidelines(),
            )
        });
        for warning in &precheck.warnings {
            say(format!("{}: warning: {warning}", built.label()));
        }
        if !precheck.errors.is_empty() {
            return Err(FabFailure {
                text: precheck.errors.into_iter().map(fail).collect(),
                requirements: precheck.requirements,
                conflicts: precheck.conflicts.into(),
                placement: precheck.placement.into(),
                ..FabFailure::default()
            });
        }
        // The parts the layout file places stand where it says whatever the seed: a fault among
        // them (courtyards that overlap, a part past the edge, in a keepout or a cutout) is
        // found now, as `layout check` finds it, with no placing (issue 1309). The faults
        // `--for-review` waives are left to the check after placing, which lists them.
        if let Some(layout) = layout.as_ref() {
            // A board the entries cannot be measured on is the placer's to refuse.
            if let Ok((faults, conflicts)) =
                crate::layout_edit::entry_check(&built.design, layout, libraries)
            {
                let faults: Vec<_> = faults
                    .into_iter()
                    .filter(|f| !(options.for_review && review_waives(f.kind)))
                    .collect();
                if !faults.is_empty() {
                    return Err(FabFailure {
                        text: faults.iter().map(|f| fail(f.text.clone())).collect(),
                        conflicts: conflicts.into(),
                        placement: faults.into(),
                        ..FabFailure::default()
                    });
                }
            }
        }
        say(format!(
            "{}: placing {} parts",
            built.label(),
            built.design.parts.len()
        ));
        // The placer tells how it goes (issue #764), as the other stages do.
        let sink = crate::layout::place_progress(built.label(), {
            let progress = options.progress;
            move |line: &str| {
                if let Some(progress) = progress {
                    progress(line);
                }
            }
        });
        let placing = timings::stage("fab.place", &label, || {
            netsmith_core::job::reporting(sink, || {
                crate::layout::place(
                    &built.design,
                    layout.as_ref(),
                    libraries,
                    catalogs.constraints_warn(),
                    crate::layout::Seeding {
                        seeds: options.place_seeds,
                        budget: options.place_budget,
                        routing_objective: options.routing_objective,
                        progress: None,
                    },
                )
            })
        })
        .map_err(|e| match e {
            crate::layout::PlaceFailure::Board(netsmith_pcb::BoardError::DoesNotFit(e)) => {
                does_not_fit(fail(placement_failure(&e, out)), &e)
            }
            crate::layout::PlaceFailure::Unplaced { error, seeds } => does_not_fit(
                fail(format!(
                    "{}\n{}",
                    seeds.report(),
                    placement_failure(&error, out)
                )),
                &error,
            ),
            crate::layout::PlaceFailure::Board(e) => fail(e.to_string()).into(),
            crate::layout::PlaceFailure::Layout(e) => fail(e).into(),
        })?;
        let crate::layout::Placing {
            mut board,
            options: placement,
            seeds,
        } = placing;
        settle_seed(
            (label.as_str(), name.as_str()),
            &seeds,
            layout,
            options,
            say,
        )?;
        // `--for-review` waives the faults of the board's own limits (issue #1212): they are
        // listed under FOR REVIEW in the notes, and the rest still stop `fab`.
        let (waived_faults, faults): (Vec<_>, Vec<_>) = board
            .placement_faults()
            .into_iter()
            .partition(|f| options.for_review && review_waives(f.kind));
        let mut waived: Vec<Waived> = waived_faults.into_iter().map(Waived::of_fault).collect();
        let problems: Vec<String> = faults.iter().map(|f| f.text.clone()).collect();
        // When the placer could not meet the requirements of the physical file, the faults
        // above are its symptoms: say first which lines are not met and which cannot all hold,
        // and keep the same list in `placement-failure.txt` (ADR 0045).
        let unmet = board.explain_requirements(&built.design, &placement);
        // No seed placed the board: say so first, with what blocks the closest try, which is
        // the board whose faults follow (issue #458).
        let no_seed = (!seeds.placed).then(|| fail(seeds.report()));
        if !problems.is_empty() {
            let mut errors = no_seed.clone().unwrap_or_default();
            if let Some(report) = &unmet {
                errors.push_str(&fail(requirement_failure(report, &problems, &board, out)));
            }
            errors.extend(problems.iter().map(|p| fail(p.clone())));
            let (requirements, conflicts) = unmet.map(|r| (r.misses, r.proofs)).unwrap_or_default();
            return Err(FabFailure {
                text: errors,
                requirements,
                conflicts: conflicts.into(),
                placement: faults.into(),
                ..FabFailure::default()
            });
        }
        // The placer's own arrangement, with no layout file to move parts after it, that does
        // not meet a requirement would be found by the check after routing; say so now, with
        // the same list, unless constraints are only warnings for this run. (With a layout
        // file the positions are the file's, and the check after routing measures those.)
        // `--for-review` goes on: the check after routing finds them, and they are waived there.
        if layout.is_none()
            && let Some(report) = unmet.filter(|r| !r.unmet.is_empty())
            && !catalogs.constraints_warn()
            && !options.for_review
        {
            return Err(FabFailure {
                text: no_seed.unwrap_or_default()
                    + &fail(requirement_failure(&report, &[], &board, out)),
                requirements: report.misses,
                conflicts: report.proofs.into(),
                ..FabFailure::default()
            });
        }
        for hang in board.hangs() {
            say(format!("{}: warning: {hang}", built.label()));
        }
        // A pad other parts wall in where they are placed (issue #1313), said before a route is
        // tried: the router would report it `walled in` after trying.
        for walled in board.walled_pads() {
            say(format!("{}: warning: {walled}", built.label()));
        }
        // A part nearer a part with a `soft margin` than the margin asks (issue 1310): the placer
        // kept it out where it could. A warning, and an error when the run is strict.
        let broken = board.soft_margins_broken();
        if catalogs.strict_guidelines() && !broken.is_empty() {
            return Err(broken
                .iter()
                .map(|b| {
                    fail(format!(
                        "{b}; this is an error because the run is strict (`--strict` or `guidelines = \"error\"`)"
                    ))
                })
                .collect::<String>()
                .into());
        }
        for b in &broken {
            say(format!("{}: warning: {b}", built.label()));
        }
        // Two parts nearer each other than a `keep` statement asks (issue 1277): a requirement
        // on where parts go, which stops `fab` as one the check after routing finds does, unless
        // constraints are only warnings for this run; `--for-review` lists it under FOR REVIEW.
        let apart = board.kept_apart_broken();
        if !apart.is_empty() && !catalogs.constraints_warn() && !options.for_review {
            return Err(FabFailure {
                text: apart.iter().map(|b| fail(b.to_string())).collect(),
                requirements: apart.iter().map(netsmith_pcb::ApartBroken::miss).collect(),
                ..FabFailure::default()
            });
        }
        for b in apart {
            say(format!("{}: warning: {b}", built.label()));
            if options.for_review {
                waived.push(Waived {
                    rule: "constraint".to_string(),
                    message: b.to_string(),
                    placement: None,
                    drc: None,
                });
            }
        }
        // Two parts where an imported KiCad board has them may share courtyard area: that
        // board was made with its own footprints (`Board::imported`).
        for overlap in board.imported_overlaps() {
            say(format!(
                "{}: warning: {overlap}: both stand where the imported KiCad board has them (`source = \"import\"` in the layout file), which was made with its own footprints",
                built.label()
            ));
        }
        // A part under a height limit that states no height is not a pass (ADR 0045,
        // "Height limits"): it is named, here and in the notes.
        let mut unchecked = board.heights_unchecked();
        if options.strict_heights && options.for_review {
            // Waived for review, and then not also a warning.
            for u in std::mem::take(&mut unchecked) {
                waived.push(Waived {
                    rule: "height_unchecked".to_string(),
                    message: u.to_string(),
                    placement: None,
                    drc: None,
                });
            }
        }
        if options.strict_heights && !unchecked.is_empty() {
            return Err(unchecked
                .iter()
                .map(|u| {
                    fail(format!(
                        "{u}; this is an error because constraints are strict (`--constraints error` or `constraints = \"error\"`), and `--allow-drc-violations` does not waive it: state the height, or run with `--constraints warn`"
                    ))
                })
                .collect::<String>()
                .into());
        }
        for u in &unchecked {
            say(format!("{}: warning: {u}", built.label()));
        }
        // The parts the height check cannot check, under a limit or under another part's body:
        // one warning, so that a pass does not read as "fine" (issue #1117). Always a warning.
        let unknown = netsmith_pcb::heights_unknown_warning(&board.heights_unknown());
        if let Some(w) = &unknown {
            say(format!("{}: warning: {w}", built.label()));
        }
        // A pass that rests on a package standard's height is said to be one (ADR 0106).
        for pass in board.standard_passes() {
            say(format!("{}: note: {pass}", built.label()));
        }
        // The board's fit in its case models (ADR 0109), measured on the placement before
        // routing: a collision stops `fab` whatever the flags say, an unchecked fit does when
        // constraints are strict, the rest are warnings. A file that is not there was said by
        // `models::locate` above; it is in the notes too.
        let mut fit = if built.design.features.case_models.is_empty() {
            netsmith_3d::CaseScene::default()
        } else {
            timings::stage("fab.case", &label, || {
                crate::board3d::scene(built, &board).case
            })
        };
        // `--for-review` waives a collision with the case and a fit it cannot check, not a case
        // model it cannot read; what it waives is not also a warning.
        if options.for_review {
            fit.findings.retain(|f| {
                let waive = f.severity(options.strict_heights) == "error"
                    && !matches!(f.why, netsmith_3d::CaseWhy::Unreadable { .. });
                if waive {
                    waived.push(Waived {
                        rule: f.kind.word().to_string(),
                        message: f.to_string(),
                        placement: None,
                        drc: None,
                    });
                }
                !waive
            });
        }
        let refused: Vec<String> = fit
            .findings
            .iter()
            .filter(|f| f.severity(options.strict_heights) == "error")
            .map(|f| fail(case_refusal(f)))
            .collect();
        if !refused.is_empty() {
            return Err(refused.concat().into());
        }
        for f in fit
            .findings
            .iter()
            .filter(|f| !matches!(f.why, netsmith_3d::CaseWhy::Missing { .. }))
        {
            say(format!(
                "{}: warning: {}: {f}",
                built.label(),
                f.kind.word()
            ));
        }
        // A trim is a step a person does with a cutter. The statement stays a warning for as
        // long as it stands (ADR 0045 section 15).
        for trim in board.trims() {
            say(format!(
                "{}: warning: the design states a trim: {}",
                built.label(),
                trim.note()
            ));
            for u in trim.unchecked() {
                say(format!("{}: warning: {u}", built.label()));
            }
        }
        say(format!(
            "{}: placed on a {} by {} mm board with seed {}",
            built.label(),
            board.outline.width(),
            board.outline.height(),
            seeds.used
        ));
        // The copper the routes file fixes, before the router (ADR 0030); what of it does not
        // match the board any more is said, not used.
        for note in
            crate::routes::apply_beside(&mut board, options.layout.as_deref()).map_err(fail)?
        {
            say(format!("{}: warning: {note}", built.label()));
        }
        // The board as placed, for the pin optimizer if the router leaves connections.
        let placed = options.pins.is_some().then(|| board.clone());
        // `--save-placed`: the board as placed, for `netsmith dev route`; without it, one an earlier
        // run left is removed, so the folder never holds a placed board of another layout.
        if let Some(file) = crate::placed::save_or_remove(
            &built.dir(out),
            &board,
            options.track_margin,
            options.save_placed,
        )
        .map_err(fail)?
        {
            say(format!(
                "{}: saved the placed board to {}",
                built.label(),
                file.display()
            ));
        }
        // The router tells how it goes as the placer does (issue #764): labelled and timed.
        let sink = crate::layout::route_progress(built.label(), {
            let progress = options.progress;
            move |line: &str| {
                if let Some(progress) = progress {
                    progress(line);
                }
            }
        });
        // `--reuse-routes`: the copper kept on disk, for the nets the placement left alone.
        let kept = options
            .reuse_routes
            .then_some(options.layout.as_deref())
            .flatten()
            .map(crate::kept_routes::KeptRoutes::beside_layout);
        let start = kept.as_ref().map(|_| board.clone());
        let from = kept.as_ref().and_then(|k| match k.start_from(&board) {
            Ok(from) => from,
            Err(why) => {
                say(format!(
                    "{}: warning: {why}; routing every net",
                    built.label()
                ));
                None
            }
        });
        if let Some(w) = options.weights {
            say(format!("{}: routing {}", built.label(), weights_line(&w)));
        }
        let routing = timings::stage("fab.route", &label, || {
            netsmith_core::job::reporting(sink, || match &from {
                Some(from) => {
                    let done = reroute_with_margin(
                        &mut board,
                        &from.before,
                        &from.rip,
                        options.route_timeout,
                        RouteOptions {
                            rip_rounds: options.rip_rounds,
                            weights: options.weights,
                            ..RouteOptions::default()
                        },
                        options.track_margin,
                        say,
                    );
                    let mut again = done.carried.ripped.clone();
                    again.extend(done.also.iter().cloned());
                    again.sort();
                    again.dedup();
                    say(format!(
                        "{}: kept the router's copper of {} net(s) from {}, routed {} again",
                        built.label(),
                        done.carried.kept.len(),
                        from.file.display(),
                        again.len()
                    ));
                    done.routing
                }
                None => route_with_margin(
                    &mut board,
                    options.route_timeout,
                    RouteOptions {
                        rip_rounds: options.rip_rounds,
                        weights: options.weights,
                        ..RouteOptions::default()
                    },
                    options.track_margin,
                    say,
                ),
            })
        });
        // Kept for the next run when it ran to its end: a route stopped short is no board to
        // start from.
        if let (Some(kept), Some(start), false) = (&kept, &start, routing.stopped) {
            match kept.write(start, &board) {
                Ok(file) => say(format!(
                    "{}: kept the router's copper in {}",
                    built.label(),
                    file.display()
                )),
                Err(e) => say(format!("{}: warning: {e}", built.label())),
            }
        }
        // A diff pair routed as two separate lines says so (issue #1295).
        if let Some(sep) = &routing.separate {
            for (p, n) in &sep.pairs {
                say(format!(
                    "{}: diff pair {p}/{n} routed as two separate lines: routed as pairs, the board left {} connections, as two lines {}; its lines do not run side by side, and on a board routed completely the meander matches its lengths",
                    built.label(),
                    sep.left_as_pairs,
                    sep.left
                ));
            }
        }
        // A diff pair whose meanders could not match it says the skew it keeps (issue #431, S11).
        for p in &routing.pairs {
            if p.after > p.limit {
                say(format!(
                    "{}: diff pair {}/{} keeps a skew of {:.2} mm (from {:.2} mm): no straight run of its shorter net has room for more meander",
                    built.label(),
                    p.p,
                    p.n,
                    p.after,
                    p.before
                ));
            }
        }
        // A group of nets matched in length that the meanders could not match (issue #1234).
        for g in &routing.groups {
            if g.after > g.tolerance {
                say(format!(
                    "{}: length group {} keeps a spread of {:.2} mm (from {:.2} mm, tolerance {:.2} mm): no straight run of its shorter nets has room for more meander",
                    built.label(),
                    g.path,
                    g.after,
                    g.before,
                    g.tolerance
                ));
            }
        }
        let unrouted = routing.left;
        if let (false, Some(placed)) = (unrouted.is_empty(), placed) {
            *stuck = Some(Stuck {
                left: unrouted.len(),
                crossings: Score::of(&placed).crossings,
                placed,
            });
        }
        // A board with connections left is still written, so that the owner can open the part
        // that is routed in the PCB view, but it is marked INCOMPLETE and the run ends in an
        // error (see `write_incomplete`).
        let incomplete = (!unrouted.is_empty()).then(|| {
            let mut msg = format!("{} connections could not be routed", unrouted.len());
            match (routing.stopped, options.route_timeout) {
                (true, Some(limit)) => msg.push_str(&format!(
                    " (stopped after the route timeout of {} s, on try {} of the router; raise it with --route-timeout)",
                    limit.as_secs(),
                    routing.tries
                )),
                _ => msg.push_str(&format!(
                    " (after {} tries with the lines in different orders, ripping up the nets in the way)",
                    routing.tries
                )),
            }
            msg.push(':');
            for line in &unrouted.lines {
                msg.push_str(&format!("\n  {line}"));
            }
            // A board that refuses vias in pads says what that cost (issue #1118).
            let cost = netsmith_router::via_in_pad::cost_line(&board, &unrouted);
            if let Some(cost) = &cost {
                msg.push_str(&format!("\n{cost}"));
            }
            let mut report = routing_failure(
                &built.label(),
                board.outline.min,
                &unrouted,
                routing.tries,
                &routing.why,
            );
            if let Some(cost) = &cost {
                report.push_str(&format!("\n{cost}\n"));
            }
            Incomplete {
                message: msg,
                report,
                cost,
                json: routing_report(&routing.why, &routing.crowding),
                lines: unrouted
                    .lines
                    .iter()
                    .map(|line| format!("ERROR: not routed: {line}"))
                    .collect(),
            }
        });
        if let Some(incomplete) = &incomplete {
            say(format!(
                "{}: INCOMPLETE: {} connections are not routed; writing the board and the Gerbers anyway, marked incomplete",
                built.label(),
                incomplete.lines.len()
            ));
        } else {
            say(format!("{}: routed, writing files", built.label()));
        }
        // The datasheets' layout guidelines the board does not meet (ADR 0030, issues #829
        // and #994), and the voltage the routed tracks of each power net drop (issue #652):
        // guidelines, so warnings, and an error that stops here when guidelines are strict.
        let layout_findings = crate::part_layout::findings(
            &board,
            layout.as_ref().and_then(|(l, _)| l.boards.get(&board.name)),
        );
        for w in &layout_findings.waived {
            say(format!("{}: note: {w}", built.label()));
        }
        let mut drops: Vec<String> = layout_findings
            .reported
            .into_iter()
            .map(|m| {
                format!(
                    "{}: {} guideline: {m}",
                    built.label(),
                    crate::part_layout::RULE
                )
            })
            .collect();
        if incomplete.is_none() {
            drops.extend(
                crate::track_drop::drops_of(&board, &built.trace.model)
                    .iter()
                    .filter_map(crate::track_drop::NetDrop::finding)
                    .map(|m| {
                        format!(
                            "{}: {} guideline: {m}",
                            built.label(),
                            crate::track_drop::RULE
                        )
                    }),
            );
        }
        // The impedance targets no width gives, or that a floor moves off (issue #1025): a
        // guideline too, as the width against current is.
        drops.extend(
            board
                .impedance_findings()
                .into_iter()
                .map(|m| format!("{}: {IMPEDANCE_RULE} guideline: {m}", built.label())),
        );
        if catalogs.strict_guidelines() && !drops.is_empty() {
            return Err(FabFailure {
                text: drops
                    .iter()
                    .map(|d| {
                        format!(
                            "error: {d}; this is an error because guidelines are strict (`guidelines = \"error\"` or `--strict`)\n"
                        )
                    })
                    .collect(),
                ..FabFailure::default()
            });
        }
        for d in &drops {
            say(format!("warning: {d}"));
        }

        let dir = built.dir(out);
        let pcb_path = dir.join(format!("{name}.kicad_pcb"));
        fs::create_dir_all(&dir).map_err(|e| fail(format!("{}: {e}", dir.display())))?;
        // The board file holds netsmith's own fill, so it opens poured in KiCad (issue #1184);
        // netsmith reads it back as the board it was written from. Where the filler refuses the
        // board, the file is written unfilled and the package says why, as its Gerbers fail. A
        // zone whose fill KiCad could not read is written unfilled, and the notes say which
        // (issue #1221): a board its owner cannot open is worse than one KiCad fills.
        let (mut pcb, unfilled_notes) = match kicad_pcb_filled(&board) {
            Ok(filled) => (filled.text, filled.unfilled),
            Err(_) => (kicad_pcb(&board), Vec::new()),
        };
        for note in &unfilled_notes {
            say(format!("{}: warning: {note}", built.label()));
        }
        // A self-contained project (`--portable`, issue #1198): the libraries and the 3D models
        // the board file names are copied into the folder, and the file names them there.
        let mut project_files = options.portable.then(|| libraries.portable(&dir));
        if let Some(project_files) = &mut project_files {
            project_files
                .libraries(&built.design)
                .map_err(|e| fail(format!("{}: {e}", dir.display())))?;
            pcb = project_files
                .pcb(&pcb, &built.design.features.models)
                .map_err(|e| fail(format!("{}: {e}", dir.display())))?;
        }
        for (file, contents) in [
            (pcb_path.clone(), pcb),
            (dir.join(format!("{name}.kicad_pro")), kicad_pro(&board)),
            (dir.join(format!("{name}.kicad_dru")), kicad_dru(&board)),
            (dir.join(format!("{name}.svg")), render_svg(&board)),
        ] {
            fs::write(&file, contents).map_err(|e| fail(format!("{}: {e}", file.display())))?;
        }

        // The pin map for firmware, beside the board: written for an incomplete board too,
        // whose pins are the ones it was routed with. A board with no request has none, and
        // the two files an earlier run left are removed.
        let pin_files = crate::pin_export::write(&dir, name, pin_map.as_ref())
            .map_err(|e| fail(format!("{}: {e}", dir.display())))?;

        // The board's 3D scene, beside the other files that are not for the board house. A failure
        // to write it is a note, never a failed fab: the package does not need it.
        // One scene for both, its parts drawn from their STEP models (ADR 0108).
        let scene = (options.glb || options.step).then(|| crate::board3d::scene(built, &board));
        for w in scene.iter().flat_map(|s| &s.warnings) {
            say(format!("{}: warning: {w}", built.label()));
        }
        let (glb_file, glb_note) = write_glb(&dir, name, built, scene.as_ref(), options.glb);
        if let Some(note) = &glb_note {
            say(format!("{}: warning: {note}", built.label()));
        }

        // The board as STEP, the same way: a failure is a note, never a failed fab.
        let (step_file, step_note) = write_step(&dir, name, built, scene.as_ref(), options.step);
        if let Some(note) = &step_note {
            say(format!("{}: warning: {note}", built.label()));
        }

        // The schematic, a page for each sheet, the same way: a failure is a note.
        let (pdf_file, pdf_notes) = crate::schematic::write_pdf(
            &dir,
            name,
            &[built],
            libraries,
            options.schematic.as_ref(),
            options.pdf,
        );
        for note in &pdf_notes {
            say(format!("{}: warning: {note}", built.label()));
        }

        // The project's own libraries and the standard library's own files that the board
        // uses, so KiCad finds the footprints; with `--portable`, the tables and the notice of
        // the self-contained folder.
        let mut portable_notes: Vec<String> = Vec::new();
        match project_files {
            Some(project_files) => {
                let written = project_files
                    .finish()
                    .map_err(|e| fail(format!("{}: {e}", dir.display())))?;
                for w in &written.warnings {
                    say(format!("{}: warning: {w}", built.label()));
                }
                portable_notes = written.warnings;
            }
            None => libraries
                .write_tables(&dir, &built.design)
                .map_err(|e| fail(format!("{}: {e}", dir.display())))?,
        }

        let paths: Vec<String> = built.design.parts.iter().map(|p| p.path.clone()).collect();
        let entries = bom_entries(&sourcing, &paths, &libraries.mapping).map_err(fail)?;
        // Guidelines that the design goes against (ADR 0036) are for the person ordering, so
        // they are in `fab-notes.txt`.
        let mut extras = board.process_extras();
        extras.splice(0..0, pin_notes.iter().cloned());
        extras.extend(glb_note.map(|n| format!("warning: {n}")));
        extras.extend(step_note.map(|n| format!("warning: {n}")));
        extras.extend(pdf_notes.iter().map(|n| format!("warning: {n}")));
        extras.extend(portable_notes.iter().map(|n| format!("warning: {n}")));
        extras.extend(unfilled_notes.iter().map(|n| format!("warning: {n}")));
        if let Some(scene) = &scene {
            extras.extend(scene.warnings.iter().map(|w| format!("warning: {w}")));
            extras.extend(model_notes(scene));
        }
        extras.extend(crate::no_supplier::note(&sourcing));
        // What the owner buys, by channel (ADR 0143): the consigned and hand-soldered parts, each
        // with its channel's sku.
        extras.extend(crate::buying::notes(&crate::buying::lines(&sourcing)));
        if options.offline {
            extras.push(OFFLINE_NOTE.to_string());
        }
        // The notes say which part hangs over which edge, how far, and who said it may; each
        // is a warning (owner decision, 2026-11-03, ADR 0045 section 10).
        extras.extend(board.hangs().into_iter().map(|h| format!("warning: {h}")));
        extras.extend(unchecked.iter().map(|u| format!("warning: {u}")));
        extras.extend(unknown.iter().map(|w| format!("warning: {w}")));
        extras.extend(netsmith_pipeline::heights::notes(
            &built.design,
            &built.catalog_heights,
            &board,
        ));
        extras.extend(crate::case_fit::notes(&fit));
        extras.extend(
            board
                .trims()
                .iter()
                .flat_map(Trim::unchecked)
                .map(|u| format!("warning: {u}")),
        );
        extras.extend(
            built
                .guideline_warnings_except(&environment.replaced)
                .into_iter()
                .chain(environment.findings)
                .map(|w| format!("warning: {w}")),
        );
        extras.extend(drops.iter().map(|d| format!("warning: {d}")));
        // The guidelines the layout file waives, with its reasons (issue #994).
        extras.extend(
            layout_findings
                .waived
                .iter()
                .map(|w| format!("note: {}: {w}", built.label())),
        );
        // What the check of the environment did and did not do, for a board that has one.
        extras.extend(environment.notes);
        // The conditions of each radio's grant that a person must verify (issue #1278).
        extras.extend(netsmith_solve::radio::verify_lines(&built.trace.model));
        // The notes lead with what is wrong, so that nobody orders an incomplete board.
        if let Some(incomplete) = &incomplete {
            extras.insert(0, format!("ERROR: INCOMPLETE: {}", incomplete.headline()));
            for (k, line) in incomplete.lines.iter().take(10).enumerate() {
                extras.insert(1 + k, line.clone());
            }
            if incomplete.lines.len() > 10 {
                extras.insert(
                    11,
                    format!(
                        "ERROR: ... and {} more not routed, all in {ROUTING_FAILURE}",
                        incomplete.lines.len() - 10
                    ),
                );
            }
            if let Some(cost) = &incomplete.cost {
                let after = 1 + incomplete.lines.len().min(11);
                extras.insert(after, cost.clone());
            }
        }
        // A board the router left open is packaged as before, marked INCOMPLETE, unless a
        // finding was waived for review before routing: it is never packaged to order from.
        let for_review = options.for_review && (incomplete.is_none() || !waived.is_empty());
        let pictures = if for_review && options.review_pictures {
            review_pictures(&board, name)
        } else {
            Vec::new()
        };
        // The alternates (issue 1312): not fitted, so in no line of bom.csv or cpl.csv; issue 1415
        // lists each in bom-alternates.csv.
        extras.extend(alternate_notes(&built.alternates, for_review));
        let package_options = Options {
            // The check finds the unrouted connections as unconnected pads; the package is made
            // anyway, for the person to look at, and the run still fails.
            allow_drc_violations: options.allow_drc_violations || incomplete.is_some(),
            for_review,
            waived: waived.iter().map(Waived::line).collect(),
            pictures: pictures.iter().map(|(file, _)| file.clone()).collect(),
            drc: netsmith_geom::DrcExtras {
                // A guideline of the check that a strict run makes an error (issue #1509).
                strict_guidelines: catalogs.strict_guidelines(),
                ..drc_extras(&board)
            },
            trim: board.trims().iter().map(Trim::note).collect(),
            process: Some(board.process_note()),
            extras,
            fit: fit_of(&built.sourcing),
            dnp: built
                .sourcing
                .iter()
                .filter(|s| s.not_fitted.is_some())
                .map(|s| s.refdes.clone())
                .collect(),
            alternates: alternate_entries(&built.alternates),
            revision: options.revision.clone(),
            // The design's requirements on where parts go (ADR 0045): an error that stops
            // the package, or a warning with `constraints = "warn"`.
            constraints: netsmith_pipeline::constraints::of_design(
                &built.design,
                catalogs.constraints_warn(),
            ),
            // The board's diff pairs with their skew limits: a pair over its limit is a
            // `pair_skew` finding, as `board drc` has it (issue #1597).
            pairs: board.diff_pairs_with_skew(),
            ..Options::default()
        };
        // The pictures of a package made for review are of the board just routed (issue #1213);
        // a package that is not for review has none, and those an earlier run left go.
        let draw_pictures = |pkg: &mut Package| -> Result<Vec<PathBuf>, String> {
            let wanted = if pkg.for_review() { &pictures[..] } else { &[] };
            let (files, notes) = write_review_pictures(
                &dir,
                name,
                &board,
                package_options.constraints.as_ref(),
                wanted,
            )?;
            for note in notes {
                say(format!("{}: warning: {note}", built.label()));
                if !pkg.notes.is_empty() && !pkg.notes.ends_with('\n') {
                    pkg.notes.push('\n');
                }
                pkg.notes.push_str(&format!("warning: {note}\n"));
            }
            Ok(files)
        };
        if let Some(incomplete) = incomplete {
            let mut packaged = package(&pcb_path, &entries, &package_options);
            let requirements = packaged
                .as_ref()
                .err()
                .map(unmet_requirements)
                .unwrap_or_default();
            let mut review = None;
            if let Ok(pkg) = &mut packaged {
                draw_pictures(pkg).map_err(fail)?;
                if pkg.for_review() {
                    let mut waived = waived;
                    waived.extend(pkg.drc_report.errors().map(Waived::of_drc));
                    review = Some(Box::new(Review {
                        board: label.clone(),
                        waived,
                    }));
                }
            }
            return Err(FabFailure {
                text: fail(write_incomplete(&incomplete, &dir, packaged)),
                requirements,
                review,
                ..FabFailure::default()
            });
        }
        // Reads the written board back, runs the design rules check, plots the Gerbers and
        // drill files and makes the BOM and the placement file.
        let pkg = timings::stage("fab.package", &label, || {
            package(&pcb_path, &entries, &package_options)
        })
        .map_err(|e| FabFailure {
            text: fail(match &e {
                // The first line says that nothing was written, and what each flag does.
                netsmith_fab::Error::Drc { counts, report, .. } => {
                    format!(
                        "{}\n\n{}",
                        netsmith_fab::drc_stop_line(counts, Some(&dir)),
                        report.trim_end()
                    )
                }
                e => e.to_string(),
            }),
            requirements: unmet_requirements(&e),
            ..FabFailure::default()
        })?;
        let mut pkg = pkg;
        let picture_files =
            timings::stage("fab.pictures", &label, || draw_pictures(&mut pkg)).map_err(fail)?;
        if pkg.for_review() {
            // Written for a person to look at; the run still fails, as the check does.
            let mut written = vec![
                pcb_path.clone(),
                pcb_path.with_extension("kicad_pro"),
                pcb_path.with_extension("svg"),
            ];
            written.extend(pin_files);
            written.extend(glb_file);
            written.extend(step_file);
            written.extend(pdf_file);
            written.extend(
                timings::stage("fab.write", &label, || pkg.write(&dir))
                    .map_err(|e| fail(e.to_string()))?,
            );
            written.extend(picture_files);
            let names: Vec<String> = written
                .iter()
                .filter_map(|f| f.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .collect();
            let mut waived = waived;
            waived.extend(pkg.drc_report.errors().map(Waived::of_drc));
            return Err(FabFailure {
                text: fail(format!(
                    "{} finding{} waived for review: {}: files to look at are in {}: {}; no bom.csv and no cpl.csv, and `project order` refuses this package; the run still fails while a waived finding stands\n\n{}",
                    waived.len(),
                    if waived.len() == 1 { "" } else { "s" },
                    netsmith_fab::FOR_REVIEW,
                    dir.display(),
                    names.join(", "),
                    waived
                        .iter()
                        .map(Waived::line)
                        .collect::<Vec<_>>()
                        .join("\n")
                )),
                review: Some(Box::new(Review {
                    board: label.clone(),
                    waived,
                })),
                requirements: pkg
                    .constraints
                    .iter()
                    .filter(|o| !o.satisfied)
                    .map(netsmith_pcb::RequirementMiss::of_outcome)
                    .collect(),
                ..FabFailure::default()
            });
        }
        // The viewer comparison of `--cross-check` (issue #1158): the copper of the board model
        // and of the Gerbers just made, each layer, with no KiCad. Its lines are in the notes
        // before they are written, so `fab-notes.txt` has them, a difference too.
        let viewer_check = if options.cross_check {
            say(format!(
                "{}: cross-checking against netsmith's viewer",
                built.label()
            ));
            let check = crate::gerber_compare::check_board(&board, &pkg.gerber_zip)
                .map_err(|e| fail(format!("cross-check against netsmith's viewer: {e}")))?;
            if !pkg.notes.is_empty() && !pkg.notes.ends_with('\n') {
                pkg.notes.push('\n');
            }
            pkg.notes.push_str(&check.to_string());
            if kicad.is_none() {
                pkg.notes.push_str(
                    "cross-check: ran netsmith's viewer comparison alone (KiCad's plots and design rules check need KiCad, which was not found)\n"
                );
            }
            Some(check)
        } else {
            None
        };
        let mut files = vec![pcb_path.clone()];
        files.extend(pin_files);
        files.extend(glb_file);
        files.extend(step_file);
        files.extend(pdf_file);
        files.extend(
            timings::stage("fab.write", &label, || pkg.write(&dir))
                .map_err(|e| fail(e.to_string()))?,
        );
        let mut notes = pkg.notes.clone();
        // What stops the run, from either check: the viewer's differences are in the notes
        // already.
        let mut differences = String::new();
        if let Some(check) = viewer_check.as_ref().filter(|c| !c.ok()) {
            differences.push_str(&check.to_string());
        }
        if let Some(kicad) = kicad.filter(|_| options.cross_check) {
            say(format!("{}: cross-checking against KiCad", built.label()));
            let report = netsmith_fab::cross_check(
                &pcb_path,
                kicad,
                &netsmith_fab::CrossCheckLimits {
                    pictures: true,
                    ..Default::default()
                },
            )
            .map_err(|e| fail(e.to_string()))?;
            // The pictures of the differences, for a person to look at.
            for layer in &report.layers {
                if let Some(png) = layer.diff_png.as_ref().filter(|_| !layer.ok) {
                    let file = dir.join(format!("cross-check-{}.png", layer.file));
                    fs::write(&file, png).map_err(|e| fail(format!("{}: {e}", file.display())))?;
                    files.push(file);
                }
            }
            if report.ok() {
                notes.push_str(&format!("{report}"));
            } else {
                differences.push_str(&format!("{report}"));
            }
        }
        if !differences.is_empty() {
            return Err(fail(differences).into());
        }

        let fabricated = Fabricated {
            board: built.label(),
            dir,
            gerber_files: pkg.gerber_files.clone(),
            summary: pkg.summary(),
            notes,
            files,
            facts: BoardFacts::of(&board, &pkg).bought_by(&sourcing),
            origins: board.origins(),
            requirements: pkg
                .constraints
                .iter()
                .filter(|o| !o.satisfied)
                .map(netsmith_pcb::RequirementMiss::of_outcome)
                .collect(),
            place_warnings: precheck.place_warnings(),
            routes: pkg.routes.clone(),
            stages_run: 0,
            reused: false,
        };
        Ok(BoardRun {
            fabricated,
            fit: package_options.fit.clone(),
            options: package_options,
            board,
            package: pkg,
            entries,
            pcb: pcb_path,
        })
    }
}

/// The system scene of `system` from its members as made (ADR 0110): its findings across
/// boards, and `<out>/<System>/<System>.glb` and `.step` with the switches of a board's. A
/// collision of known heights and a misaligned mate stop the run, and `--allow-drc-violations`
/// does not waive them; a collision through a package standard's height, a part of unknown height
/// facing another board and a member drawn apart are warnings, errors with strict constraints.
/// A pin 1 that cannot be told from its pads is a guideline: a warning, an error when
/// `strict_guidelines`.
fn fab_system(
    system: &netsmith_pipeline::SolvedSystem,
    members: &[(&Built, &Board)],
    out: &Path,
    libraries: &Libraries,
    options: &FabOptions,
    strict_guidelines: bool,
    say: &dyn Fn(String),
) -> Result<(), String> {
    let name = &system.name;
    let scene = timings::stage("fab.system", name, || {
        crate::system3d::scene(system, members)
    });
    let mut errors = String::new();
    for f in &scene.findings {
        let strict = if f.code.guideline() {
            strict_guidelines
        } else {
            options.strict_heights
        };
        if f.code.is_error(strict) {
            let why = if f.code.error() {
                "it stops `fab`, and `--allow-drc-violations` does not waive it"
            } else if f.code.guideline() {
                "it is an error because guidelines are strict (`guidelines = \"error\"` or `--strict`)"
            } else {
                "it is an error because constraints are strict (`--constraints error` or `constraints = \"error\"`); run with `--constraints warn` to list it instead"
            };
            errors.push_str(&format!(
                "error: {name}: {}: {}; {why}\n",
                f.code.word(),
                f.message
            ));
        } else {
            say(format!("{name}: warning: {}: {}", f.code.word(), f.message));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let dir = out.join(name);
    fs::create_dir_all(&dir).map_err(|e| format!("error: {name}: {}: {e}\n", dir.display()))?;
    for (write, extension, what) in [
        (options.glb, "glb", "3D scene"),
        (options.step, "step", "STEP file"),
    ] {
        let file = dir.join(format!("{name}.{extension}"));
        if !write {
            if let Err(e) = fs::remove_file(&file)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                say(format!(
                    "{name}: warning: {}: {e}; an earlier run's {what} stays in the package",
                    file.display()
                ));
            }
            continue;
        }
        let bytes = if extension == "glb" {
            netsmith_3d::system_glb(&scene)
        } else {
            netsmith_3d::system_step(&scene).into_bytes()
        };
        match fs::write(&file, bytes) {
            Ok(()) => say(format!(
                "{name}: wrote the system's {what} {}",
                file.display()
            )),
            Err(e) => say(format!(
                "{name}: warning: the system's {what} {} was not written ({e}); the boards' packages are complete without it",
                file.display()
            )),
        }
    }
    // The system's schematic: its own sheet, then each member's (issue #1091).
    let boards: Vec<&Built> = members.iter().map(|(b, _)| *b).collect();
    let (pdf, notes) = crate::schematic::write_system_pdf(
        &dir,
        system,
        &boards,
        libraries,
        options.schematic.as_ref(),
        options.pdf,
    );
    for note in notes {
        say(format!("{name}: warning: {note}"));
    }
    if let Some(file) = pdf {
        say(format!(
            "{name}: wrote the system's schematic {}",
            file.display()
        ));
    }
    Ok(())
}

/// The pictures of the copper a package made for review has (issue #1213), each with its file
/// name: the top layer, each inner signal layer (an inner plane is left out) and the bottom
/// layer, as `<name>-top-REVIEW.png`, `<name>-inner<n>-REVIEW.png` and `<name>-bottom-REVIEW.png`.
/// The names end `-REVIEW`, as the review package's Gerber zip does, so that nobody orders from
/// them; the pixels carry no mark.
pub fn review_pictures(board: &Board, name: &str) -> Vec<(String, netsmith_pcb::CopperLayer)> {
    use netsmith_pcb::CopperLayer;
    std::iter::once(CopperLayer::Front)
        .chain(board.signal_layers())
        .chain(std::iter::once(CopperLayer::Back))
        .map(|layer| {
            let word = netsmith_pcb::routes_layer_word(layer.name()).unwrap_or_default();
            (format!("{name}-{word}-REVIEW.png"), layer)
        })
        .collect()
}

/// What `view pcb --layer <layer> --drc --islands` asks for, with its other flags left as they
/// are: the request of a picture of a package made for review (issue #1213), so that its bytes
/// are the bytes `view pcb` writes of the same board.
pub fn review_picture_request(layer: netsmith_pcb::CopperLayer) -> crate::pcb_picture::Request {
    crate::pcb_picture::Request {
        layers: netsmith_pcb::PictureLayers::One(layer),
        from_below: None,
        region: None,
        scale: crate::pcb_picture::DEFAULT_SCALE,
        courtyards: false,
        designators: false,
        highlight: Vec::new(),
        only_open: false,
        drc: true,
        islands: true,
        stitching: false,
        rule_areas: true,
        format: crate::pcb_picture::Format::Png,
    }
}

/// Writes `pictures` ([`review_pictures`]) of `board` into `dir`, after removing every picture
/// for review of `name` an earlier run left there, so that the folder holds only the pictures of
/// this package (none for a package that is not for review). `constraints` are the design's, for
/// the markers of the check. A picture that cannot be drawn is a note, never a failed fab; a file
/// that cannot be written or removed is an error.
fn write_review_pictures(
    dir: &Path,
    name: &str,
    board: &Board,
    constraints: Option<&netsmith_geom::Constraints>,
    pictures: &[(String, netsmith_pcb::CopperLayer)],
) -> Result<(Vec<PathBuf>, Vec<String>), String> {
    let prefix = format!("{name}-");
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            if file.starts_with(&prefix) && file.ends_with("-REVIEW.png") {
                fs::remove_file(entry.path())
                    .map_err(|e| format!("{}: {e}", entry.path().display()))?;
            }
        }
    }
    let mut written = Vec::new();
    let mut notes = Vec::new();
    for (file, layer) in pictures {
        let path = dir.join(file);
        match crate::pcb_picture::draw(board, &review_picture_request(*layer), constraints.cloned())
        {
            Ok(drawn) => {
                fs::write(&path, &drawn.bytes).map_err(|e| format!("{}: {e}", path.display()))?;
                written.push(path);
            }
            Err(e) => notes.push(format!(
                "the picture {} was not drawn ({e}); `view pcb` draws the board",
                path.display()
            )),
        }
    }
    Ok((written, notes))
}

/// The board's 3D scene as `<name>.glb` in `dir` (or, with `write` false, the removal of one an
/// earlier run left): the bytes `dev board3d --glb` writes for `board`, which is the board as fab
/// placed and routed it, with its copper (the editor's `/api/pcb/3d` draws the same). Returns the
/// file written and a note for the person ordering when it could not be written.
fn write_glb(
    dir: &Path,
    name: &str,
    built: &Built,
    scene: Option<&netsmith_3d::Scene>,
    write: bool,
) -> (Option<PathBuf>, Option<String>) {
    let file = dir.join(format!("{name}.glb"));
    if !write {
        return match fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => (
                None,
                Some(format!(
                    "{}: {e}; an earlier run's 3D scene stays in the package",
                    file.display()
                )),
            ),
            _ => (None, None),
        };
    }
    let Some(scene) = scene.filter(|_| write) else {
        return (None, None);
    };
    match fs::write(&file, netsmith_3d::glb(scene, &built.label())) {
        Ok(()) => (Some(file), None),
        Err(e) => (
            None,
            Some(format!(
                "the 3D scene {} was not written ({e}); the package is complete without it",
                file.display()
            )),
        ),
    }
}

/// The board as `<name>.step` in `dir` (or, with `write` false, the removal of one an earlier
/// run left): the bytes `dev board3d --step` writes for `board`, the board as fab placed and routed
/// it; the STEP holds no copper, so they are those of `dev board3d --step` for the same placement.
/// Returns the file written and a note for the person ordering when it could not be written.
fn write_step(
    dir: &Path,
    name: &str,
    built: &Built,
    scene: Option<&netsmith_3d::Scene>,
    write: bool,
) -> (Option<PathBuf>, Option<String>) {
    let file = dir.join(format!("{name}.step"));
    if !write {
        return match fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => (
                None,
                Some(format!(
                    "{}: {e}; an earlier run's STEP file stays in the package",
                    file.display()
                )),
            ),
            _ => (None, None),
        };
    }
    let Some(scene) = scene.filter(|_| write) else {
        return (None, None);
    };
    match fs::write(
        &file,
        netsmith_3d::scene_step(scene, &built.label()).into_bytes(),
    ) {
        Ok(()) => (Some(file), None),
        Err(e) => (
            None,
            Some(format!(
                "the STEP file {} was not written ({e}); the package is complete without it",
                file.display()
            )),
        ),
    }
}

/// The lines of `fab-notes.txt` that name the parts drawn from a STEP model (ADR 0108 "Cost"):
/// the `.glb` and the `.step` hold the model, which its maker's license may keep from being
/// shared.
fn model_notes(scene: &netsmith_3d::Scene) -> Vec<String> {
    let drawn: Vec<String> = scene
        .bodies
        .iter()
        .filter_map(|b| {
            let m = b.model.as_ref()?;
            Some(format!(
                "{} (`{}`) from `{}` (sha256 {})",
                b.refdes, b.path, m.file, m.sha256
            ))
        })
        .collect();
    if drawn.is_empty() {
        return Vec::new();
    }
    vec![format!(
        "note: drawn from a STEP model in the .glb and the .step (never measured: heights are the parts' own): {}; share those two files only as the models' licenses allow",
        drawn.join(", ")
    )]
}

/// Why a finding of the case fit check stops `fab` (ADR 0109), and what to do.
fn case_refusal(f: &netsmith_3d::FitFinding) -> String {
    use netsmith_3d::{CaseWeight, CaseWhy};
    let kind = f.kind.word();
    match (f.weight(), &f.why) {
        (CaseWeight::Error, CaseWhy::Unreadable { .. }) => format!(
            "{kind}: {f}; a case model netsmith cannot read stops `fab`, and `--allow-drc-violations` does not waive it"
        ),
        (CaseWeight::Error, _) => format!(
            "{kind}: {f}; a collision with the case stops `fab`, and neither `--allow-drc-violations` nor `--constraints warn` waives it: move or lower the part, or correct the case model's placement (`.x`, `.y`, `.z`, `.rx`, `.ry`, `.rz`)"
        ),
        _ => format!(
            "{kind}: {f}; this is an error because constraints are strict (`--constraints error` or `constraints = \"error\"`), and `--allow-drc-violations` does not waive it: state the height, close the shell, or run with `--constraints warn`"
        ),
    }
}

/// A board the router left connections of: the board as it was placed, for the pin optimizer,
/// and what it left.
struct Stuck {
    placed: Board,
    left: usize,
    crossings: usize,
}

/// What `fab_board_pinned` needs to elaborate the source again with another pin map.
struct Pass<'a> {
    name: &'a str,
    text: &'a str,
    loader: &'a Loader,
}

/// The pin map of the source as the run stands: what the check gave each board's requests, what
/// the optimizer has changed, and what the pin optimizer may move.
struct PinState {
    /// The pin file as the run began, and as it is now.
    original: PinFile,
    file: PinFile,
    spaces: BTreeMap<String, Vec<PartSpace>>,
    /// The proposal of `--pins locked`, with every board's moves.
    proposed: Option<PinFile>,
}

impl PinState {
    /// What the check decided about the requests of the parts of `built`, as the run began: none
    /// for a board of a system, whose requests are the system's.
    fn spaces_of(&self, built: &Built) -> &[PartSpace] {
        match self.spaces.get(&built.board) {
            Some(spaces) if built.system.is_none() => spaces,
            _ => &[],
        }
    }

    fn new(pins: &netsmith_check::PinsOutcome) -> Self {
        Self {
            original: pins.file.clone(),
            file: pins.file.clone(),
            spaces: pins.spaces.clone(),
            proposed: None,
        }
    }
}

/// What a pass of `fab_board` writes the pin map for firmware from: the pins of the requests of
/// the board that pass made, and the source file's name for the files.
struct PinMapOf<'a> {
    spaces: &'a [PartSpace],
    source: &'a str,
}

impl PinMapOf<'_> {
    /// The pin map of `built` (`None`: no request, or a board of a system).
    fn render(&self, built: &Built) -> Result<Option<crate::pin_export::PinMap>, String> {
        if built.system.is_some() {
            return Ok(None);
        }
        crate::pin_export::render(&built.design, self.spaces, self.source)
            .map_err(|message| format!("{message}\n"))
    }
}

/// The requests whose pin differs between two maps, one line each: `Board.g01: p00 to p09`.
fn pin_changes(old: &PinFile, new: &PinFile) -> Vec<String> {
    let mut out = Vec::new();
    for (board, entries) in &new.boards {
        for (key, e) in entries {
            if let Some(was) = old.board(board).and_then(|b| b.get(key))
                && was.pin != e.pin
            {
                out.push(format!("{board}.{key}: {} to {}", was.pin, e.pin));
            }
        }
    }
    out
}

/// [`fab_board`], and when the router leaves connections and the board has pin requests, the
/// pin optimizer (`pin_swap`) and a new pass of stages 1 to 5 with the pins it proposes
/// (ADR 0101; owner decisions of 2026-11-09, Q11 to Q13 and Q16 of
/// `docs/research/pin-assignment-options.md`).
///
/// With `--pins auto` each round searches the placed board for a better pin map, elaborates the
/// source with it and places and routes again, at most `pin_swap::MAX_ROUNDS` times. A round's
/// map is kept when it routes, or when it leaves no more connections and no more crossings than
/// the map before it; otherwise the map before stands, and the board is made again with it so
/// that the files are its own. The pin file is written with the map that stands. The rounds
/// share one time budget, the route timeout, counted from the first failure.
///
/// With `--pins locked` nothing is changed: the search runs once on the placed board, its
/// proposal is written to `pins.proposed.toml` in `out`, and the run fails as before.
///
/// A board with no requests, a board that routes, and a board of a system (its requests are in
/// the system's table too) are made by [`fab_board`] alone.
#[allow(clippy::too_many_arguments)]
fn fab_board_pinned(
    pass: &Pass,
    built: &Built,
    state: &mut PinState,
    out: &Path,
    layout: &LayoutFile,
    libraries: &Libraries,
    kicad: Option<&dyn KicadCli>,
    options: &FabOptions,
    catalogs: &Catalogs,
    say: &dyn Fn(String),
) -> Result<BoardRun, FabFailure> {
    let mut first_stuck = None;
    let source = options.pin_map_source.as_deref().unwrap_or(pass.name);
    let first = fab_board(
        built,
        out,
        layout,
        libraries,
        kicad,
        options,
        catalogs,
        say,
        &mut first_stuck,
        &[],
        &PinMapOf {
            spaces: state.spaces_of(built),
            source,
        },
    );
    let (Some(pins), Err(message), Some(first_stuck)) = (&options.pins, &first, first_stuck) else {
        return first;
    };
    let board = built.board.clone();
    let label = built.label();
    let spaces = match state.spaces.get(&board) {
        Some(s) if built.system.is_none() => s.clone(),
        _ => return first,
    };
    // One budget for the search and every route after it, counted from the first failure.
    let budget = pin_swap::Budget::start(options.route_timeout, &pins.clock);
    let design = &built.design;

    if pins.mode == PinsMode::Locked {
        let proposal = pin_swap::improve(&spaces, design, &first_stuck.placed, &budget);
        let mut message = message.text.clone();
        match proposal {
            None => message.push_str(&format!(
                "  pins: {given}, so no pin moves; no change of the unlocked pins of {board} lowers its crossings or its ratsnest\n",
                given = pins.locked_by.given()
            )),
            Some(proposal) => {
                let base = state.proposed.clone().unwrap_or_else(|| state.file.clone());
                let file = proposal.applied_to(&base, &board);
                let target = out.join(PROPOSED_PINS);
                let wrote = fs::create_dir_all(out).and_then(|()| {
                    fs::write(
                        &target,
                        format!(
                            "# Proposed by `netsmith board fab` ({}): placed and not routed, so not yet known to route. Copy it over the pin file to take it.\n{}",
                            pins.locked_by.given(),
                            file.to_text()
                        ),
                    )
                });
                message.push_str(&format!(
                    "  pins: {}, so no pin moves. Moving the unlocked pins of {board} would lower the ratsnest from {} to {} (placed and not routed):\n",
                    pins.locked_by.given(),
                    proposal.before,
                    proposal.after
                ));
                for line in &proposal.moves {
                    message.push_str(&format!("    {line}\n"));
                }
                match wrote {
                    Ok(()) => message.push_str(&format!(
                        "  the proposed pin map is in {}; copy it over the pin file and run again, or {}\n",
                        target.display(),
                        pins.locked_by.way_out()
                    )),
                    Err(e) => message.push_str(&format!("  ({} could not be written: {e})\n", target.display())),
                }
                state.proposed = Some(file);
            }
        }
        return Err(message.into());
    }

    // `--pins auto`: rounds of search, place and route.
    let pin_file_name = pass
        .loader
        .pin_file(pass.name)
        .map(|(file, _)| file.to_string())
        .unwrap_or_else(|| crate::pins::file_name(pass.name));
    // How many connections the files on disk leave: the first pass's, then each later pass's.
    let mut on_disk_left = Some(first_stuck.left);
    let mut best_stuck = first_stuck;
    let mut best_message = message.text.clone();
    let mut best_map = state.file.clone();
    let mut best_spaces = spaces;
    let mut best_built: Option<Built> = None;
    let mut best_is_last = true;
    let mut rounds = 0;
    for round in 1..=pin_swap::MAX_ROUNDS {
        let time_is_spent = || {
            say(format!(
                "{label}: pins: the time budget of the route timeout is spent, no further round"
            ));
        };
        if budget.spent() {
            time_is_spent();
            break;
        }
        let current = best_built.as_ref().unwrap_or(built);
        let Some(proposal) =
            pin_swap::improve(&best_spaces, &current.design, &best_stuck.placed, &budget)
        else {
            say(format!(
                "{label}: pins: round {round}: no change of the unlocked pins lowers the crossings or the ratsnest, stopping"
            ));
            break;
        };
        // The search used time of the budget: what is left is read now, after it, and the route
        // below gets that and no more. A round that has no time to route is not made, and the
        // files on disk stay the last pass's, which are the best so far.
        if budget.spent() {
            time_is_spent();
            break;
        }
        say(format!(
            "{label}: pins: round {round}: {} moves; {} to {}; placing and routing again",
            proposal.moves.len(),
            proposal.before,
            proposal.after
        ));
        let map = proposal.applied_to(&best_map, &board);
        let mut loader = pass.loader.clone();
        loader.attach_pins(pass.name, &pin_file_name, &map.to_text());
        let Ok(again) = netsmith_pipeline::outcome_with(
            pass.name,
            pass.text,
            &loader,
            catalogs,
            Some(libraries),
        ) else {
            say(format!(
                "{label}: pins: round {round}: the board does not build with the new pins, stopping"
            ));
            best_is_last = true;
            break;
        };
        let spaces_again = again.pins.spaces.get(&board).cloned().unwrap_or_default();
        let Some(next) = again.boards.into_iter().find(|b| b.label() == label) else {
            break;
        };
        let notes = pin_notes(&state.original, &map);
        // The elaboration took time too: the route gets what is left now.
        let route_timeout = match budget.left() {
            Some(left) if left.is_zero() => {
                time_is_spent();
                break;
            }
            left => left,
        };
        let attempt_options = FabOptions {
            route_timeout,
            ..options.clone()
        };
        rounds = round;
        let mut stuck = None;
        match fab_board(
            &next,
            out,
            layout,
            libraries,
            kicad,
            &attempt_options,
            catalogs,
            say,
            &mut stuck,
            &notes,
            &PinMapOf {
                spaces: &spaces_again,
                source,
            },
        ) {
            Ok(run) => {
                say(format!("{label}: pins: round {round}: the board routes"));
                finish_pins(state, pins, &map, say);
                return Ok(run);
            }
            Err(m) => {
                // This pass wrote the files on disk now: how many connections they leave.
                on_disk_left = stuck.as_ref().map(|s| s.left);
                match stuck {
                    Some(s) if s.left <= best_stuck.left && s.crossings <= best_stuck.crossings => {
                        say(format!(
                            "{label}: pins: round {round}: {} connections left (was {}), {} crossings (was {}); keeping the new pins",
                            s.left, best_stuck.left, s.crossings, best_stuck.crossings
                        ));
                        best_stuck = s;
                        best_message = m.text;
                        best_map = map;
                        best_spaces = spaces_again;
                        best_built = Some(next);
                        best_is_last = true;
                    }
                    _ => {
                        say(format!(
                            "{label}: pins: round {round}: no better than before, keeping the earlier pins"
                        ));
                        best_is_last = false;
                        break;
                    }
                }
            }
        }
    }
    // The files on disk are the last pass's. When the map that stands is an earlier one, make
    // the board again with it, unless that does worse than the files already there.
    let mut result = best_message;
    if !best_is_last {
        let current = best_built.as_ref().unwrap_or(built);
        let notes = pin_notes(&state.original, &best_map);
        let mut again = None;
        let remake_options = FabOptions {
            route_timeout: remake_timeout(&budget, options.route_timeout),
            ..options.clone()
        };
        let dir = current.dir(out);
        let aside = set_aside(&dir);
        let remade = fab_board(
            current,
            out,
            layout,
            libraries,
            kicad,
            &remake_options,
            catalogs,
            say,
            &mut again,
            &notes,
            &PinMapOf {
                spaces: &best_spaces,
                source,
            },
        );
        let remake_left = match (&remade, &again) {
            (Ok(_), _) => Some(0),
            (Err(_), Some(stuck)) => Some(stuck.left),
            // No unrouted connection was counted: either the pass stopped before it routed, and
            // wrote nothing, or it routed everything and failed after (the design rules check, say),
            // and wrote the folder.
            (Err(_), None) => dir.is_dir().then_some(0),
        };
        let keep = aside.is_some() && keep_earlier_files(remake_left, on_disk_left);
        if keep && let Some(aside) = &aside {
            let back = set_back(&dir, aside);
            say(format!(
                "{label}: pins: the board made again with the pins that stand left {} connection(s), the files before it {}: keeping the files before it",
                remake_left.map_or_else(|| "no board".to_string(), |n| n.to_string()),
                on_disk_left.map_or_else(|| "?".to_string(), |n| n.to_string()),
            ));
            result.push_str(&format!(
                "  pins: the files in {} are those of the last round of pins, which is not the pin map written ({}); do not order from them\n",
                dir.display(),
                why_earlier_files_stay(remake_left)
            ));
            if let Err(e) = back {
                result.push_str(&format!(
                    "  ({} could not be restored: {e})\n",
                    dir.display()
                ));
            }
        } else {
            if let Some(aside) = &aside {
                let _ = fs::remove_dir_all(aside);
            }
            if let Err(m) = remade {
                result = m.text;
            }
        }
    }
    finish_pins(state, pins, &best_map, say);
    if rounds > 0 {
        result.push_str(&format!(
            "  pins: tried {rounds} round(s) of swapping the unlocked pins; the board still has {} connection(s) left\n",
            best_stuck.left
        ));
    }
    Err(result.into())
}

/// The shortest route the board is made again with when the budget is spent ([`remake_timeout`]).
pub(crate) const REMAKE_FLOOR: Duration = Duration::from_secs(10);

/// The route timeout of the pass that makes the board again with the pin map that stands, after a
/// round that was no better (the files on disk are that round's). It gets what is left of the
/// budget, so that the run as a whole keeps to the route timeout. When less than [`REMAKE_FLOOR`]
/// is left (nothing, or a second: a router cut off after a second leaves most of the copper
/// out) it gets the floor instead (never more than the route timeout itself): the files must be
/// the ones of the pin file that is written, and the board is marked incomplete either way, so
/// the run is longer than the route timeout by at most the floor. A remake that still routes worse
/// than the files already written does not replace them ([`keep_earlier_files`]).
pub(crate) fn remake_timeout(
    budget: &pin_swap::Budget,
    route_timeout: Option<Duration>,
) -> Option<Duration> {
    match (budget.left(), route_timeout) {
        (Some(left), Some(total)) if left < REMAKE_FLOOR => Some(REMAKE_FLOOR.min(total)),
        (left, _) => left,
    }
}

/// Whether the files of the pass before the remake stay, because the remake did not do better
/// than they do: `remake_left` is the connections the remake left (`None`: it made no board),
/// `on_disk_left` those of the files on disk (`None`: not known, so the remake is not held
/// against them). A remake that leaves more connections than the files on disk, or that made no
/// board, never replaces them.
fn keep_earlier_files(remake_left: Option<usize>, on_disk_left: Option<usize>) -> bool {
    match (remake_left, on_disk_left) {
        (Some(remake), Some(on_disk)) => remake > on_disk,
        (Some(_), None) => false,
        (None, _) => true,
    }
}

/// What to say of a remake whose board did not replace the files before it: it made no board
/// (`None`: it failed before it routed, in placement, say) or it left more connections.
fn why_earlier_files_stay(remake_left: Option<usize>) -> &'static str {
    match remake_left {
        Some(_) => "making the board again with it left more connections",
        None => "making the board again with it made no board",
    }
}

/// Move the folder `dir` of a board's files aside, so that a pass can write it afresh and the
/// old files come back if that pass does worse ([`set_back`]). `None` when there is no folder, or
/// it could not be moved (then the pass writes over it, as every pass does).
fn set_aside(dir: &Path) -> Option<PathBuf> {
    let name = dir.file_name()?.to_string_lossy().into_owned();
    let aside = dir.with_file_name(format!(".{name}.before-remake"));
    let _ = fs::remove_dir_all(&aside);
    dir.is_dir()
        .then(|| fs::rename(dir, &aside).ok().map(|()| aside))
        .flatten()
}

/// Put the folder moved aside by [`set_aside`] back where it was, in place of what a pass wrote.
fn set_back(dir: &Path, aside: &Path) -> std::io::Result<()> {
    let _ = fs::remove_dir_all(dir);
    fs::rename(aside, dir)
}

/// The proposed pin map of `--pins locked`, in the output folder.
const PROPOSED_PINS: &str = "pins.proposed.toml";

/// The line of `fab-notes.txt` that says the pin map is not the one the run began with: a changed
/// pin is a change of the firmware.
fn pin_notes(old: &PinFile, new: &PinFile) -> Vec<String> {
    let changes = pin_changes(old, new);
    if changes.is_empty() {
        return Vec::new();
    }
    vec![format!(
        "pin map changed by this run, so the firmware's pin map changes: {}",
        changes.join("; ")
    )]
}

/// Make `map` the pin map the run ends with, and under `--pins auto` write the pin file when it
/// differs from the one the run began with.
fn finish_pins(state: &mut PinState, pins: &PinsUse, map: &PinFile, say: &dyn Fn(String)) {
    state.file = map.clone();
    if state.file == state.original {
        return;
    }
    match fs::write(&pins.file, state.file.to_text()) {
        Ok(()) => {
            say(format!("wrote {}", pins.file.display()));
            for line in pin_changes(&state.original, &state.file) {
                say(format!("  {line}"));
            }
        }
        Err(e) => say(format!("error: {}: {e}", pins.file.display())),
    }
}

/// What is written for a board the router left connections of.
pub const ROUTING_FAILURE: &str = "routing-failure.txt";
/// Beside it, for a program: why each line is left and the crowding map, as JSON.
pub const ROUTING_REPORT: &str = "routing-report.json";

/// A board with connections the router could not make: the message of the error `fab` ends
/// in, the text of [`ROUTING_FAILURE`], and one line of the notes for each connection.
struct Incomplete {
    message: String,
    report: String,
    /// What refusing vias in pads cost in open connections, for a board that refuses them
    /// (`netsmith_router::via_in_pad::cost_line`, issue #1118).
    cost: Option<String>,
    /// The text of [`ROUTING_REPORT`].
    json: String,
    lines: Vec<String>,
}

impl Incomplete {
    /// The first line of the notes.
    fn headline(&self) -> String {
        format!(
            "{} connection(s) have no copper; this package is NOT ready to order, see {ROUTING_FAILURE}",
            self.lines.len()
        )
    }
}

/// The text of [`ROUTING_REPORT`]: why each line is left and the crowding map (see
/// [`netsmith_router::Routing`]), in the board's own coordinates.
pub fn routing_report(
    why: &[netsmith_router::LineReport],
    crowding: &netsmith_router::Crowding,
) -> String {
    let report = serde_json::json!({
        "why": why,
        "crowding": crowding,
    });
    serde_json::to_string_pretty(&report).unwrap_or_default() + "\n"
}

/// The text of `routing-failure.txt`: every connection that has no copper, with its net and
/// its two pads and where they are, the parts most of them end on, and for each line the
/// router left the stage it last failed in and the nets in its way.
pub fn routing_failure(
    label: &str,
    origin: netsmith_pcb::Pt,
    left: &netsmith_pcb::Ratsnest,
    tries: usize,
    why: &[netsmith_router::LineReport],
) -> String {
    let at = |p: netsmith_pcb::Pt| (p.x - origin.x, p.y - origin.y);
    let mut text = format!(
        "INCOMPLETE: {label} is not fully routed. {} connection(s) have no copper after {tries} tries of the router, so this board does not work. The board file and the Gerbers beside this file are only for looking at the part that is routed; do not order them.\n\nEach line: the net, its two pads, and where they are in mm from the outline's top left corner, y down.\n\n",
        left.len()
    );
    let mut ends: BTreeMap<&str, usize> = BTreeMap::new();
    for line in &left.lines {
        let (from, to) = (at(line.from_at), at(line.to_at));
        if line.to_plane {
            text.push_str(&format!(
                "net {}: {} to the plane, at ({:.2}, {:.2})\n",
                line.net, line.from, from.0, from.1
            ));
        } else {
            text.push_str(&format!(
                "net {}: {} at ({:.2}, {:.2}) to {} at ({:.2}, {:.2})\n",
                line.net, line.from, from.0, from.1, line.to, to.0, to.1
            ));
        }
        let mut parts = vec![line.from.refdes.as_str()];
        if !line.to_plane && line.to.refdes != line.from.refdes {
            parts.push(line.to.refdes.as_str());
        }
        for part in parts {
            *ends.entry(part).or_default() += 1;
        }
    }
    let mut ends: Vec<(&str, usize)> = ends.into_iter().collect();
    ends.sort_by_key(|&(part, n)| (std::cmp::Reverse(n), part));
    text.push_str("\nThe parts the unrouted connections end on, most first (a part that most of them end on has pads that are walled in by their neighbours or by the copper of other nets):\n");
    for (part, n) in ends.iter().take(8) {
        text.push_str(&format!("  {part}: {n}\n"));
    }
    if !why.is_empty() {
        text.push_str(&format!(
            "\nWhy each line is left, by the router's own lines (which can pair the same pads otherwise than above): the stage of the router it last failed in, and the nets whose copper stands in its way (the fewest that a path allowed to cross other nets' copper meets), or, where no such path exists, the pads, vias and edge that wall an end of it in. {ROUTING_REPORT} has the same, and a map of where the board was fought over, in the board's coordinates:\n"
        ));
        for w in why {
            let mut parts = Vec::new();
            if !w.blockers.is_empty() {
                parts.push(format!("net(s) {}", w.blockers.join(", ")));
            }
            if !w.pads.is_empty() {
                let pads: Vec<String> = w
                    .pads
                    .iter()
                    .map(|p| format!("{}.{}", p.refdes, p.number))
                    .collect();
                parts.push(format!("pad(s) {}", pads.join(", ")));
            }
            if !w.vias.is_empty() {
                let vias: Vec<String> = w
                    .vias
                    .iter()
                    .map(|v| {
                        let (x, y) = at(v.at);
                        format!("{} at ({x:.2}, {y:.2})", v.net)
                    })
                    .collect();
                parts.push(format!("via(s) of {}", vias.join(", ")));
            }
            if w.edge {
                parts.push("the board edge".to_string());
            }
            let nets = if parts.is_empty() {
                "no net's copper, pad or edge found (no time to look, or the room a via needs)"
                    .to_string()
            } else {
                parts.join("; ")
            };
            text.push_str(&format!("  {}: {}; in its way: {nets}\n", w.line, w.stage));
        }
    }
    text
}

/// Write what an incomplete board has: `routing-failure.txt`, and the Gerbers, the bill of
/// materials, the placement file and the notes if the package could be made (the notes lead
/// with the error). Returns the message `fab` fails with.
/// The requirements of the design on where parts go that the check of the board file found not
/// met and so stopped the package, as data (issue #816); none for any other error.
fn unmet_requirements(e: &netsmith_fab::Error) -> Vec<netsmith_pcb::RequirementMiss> {
    match e {
        netsmith_fab::Error::Drc { unmet, .. } | netsmith_fab::Error::Constraints { unmet, .. } => {
            unmet
                .iter()
                .map(netsmith_pcb::RequirementMiss::of_outcome)
                .collect()
        }
        _ => Vec::new(),
    }
}

fn write_incomplete(
    incomplete: &Incomplete,
    dir: &Path,
    package: Result<Package, netsmith_fab::Error>,
) -> String {
    let mut message = incomplete.message.clone();
    let file = dir.join(ROUTING_FAILURE);
    let wrote = fs::write(&file, &incomplete.report)
        .and_then(|()| fs::write(dir.join(ROUTING_REPORT), &incomplete.json));
    message.push_str(&format!(
        "\n  INCOMPLETE: the board file is in {}",
        dir.display()
    ));
    match package.map(|pkg| pkg.write(dir).map(|_| pkg)) {
        Ok(Ok(pkg)) => message.push_str(&format!(
            " with the Gerbers ({}) and fab-notes.txt; they are NOT ready to order",
            pkg.zip_name()
        )),
        Ok(Err(e)) => message.push_str(&format!("; the Gerbers could not be written: {e}")),
        Err(e) => message.push_str(&format!("; the Gerbers were not made: {e}")),
    }
    match wrote {
        Ok(()) => message.push_str(&format!(
            "\n  every unrouted connection with its net and pads is in {}",
            file.display()
        )),
        Err(e) => message.push_str(&format!(
            "\n  ({} could not be written: {e})",
            file.display()
        )),
    }
    message
}

/// The panel of `spec`, made from the board `run` made: the panel board file, its Gerbers,
/// drill files, bill of materials and placement file, in `dir` beside the board's own
/// package in `dir/cell/`.
fn fab_panel(
    spec: &PanelSpec,
    run: &BoardRun,
    dir: &Path,
    kicad: Option<&dyn KicadCli>,
    options: &FabOptions,
    say: &dyn Fn(String),
) -> Result<Fabricated, String> {
    let fail = |what: String| format!("error: panel {}: {what}\n", spec.name);
    let pcb =
        fs::read_to_string(&run.pcb).map_err(|e| fail(format!("{}: {e}", run.pcb.display())))?;
    let pro = fs::read_to_string(run.pcb.with_extension("kicad_pro")).ok();
    let assembled = &run.package.plan.assembled;
    let panel = PanelBuildSpec {
        name: spec.name.clone(),
        columns: spec.columns,
        rows: spec.rows,
        rail: spec.rail,
        rules: panel_rules(&spec.rules),
    };
    let input = PanelCellInput {
        pcb: &pcb,
        pro: pro.as_deref(),
        entries: &run.entries,
        facts: PanelCellFacts {
            board: spec.cell.clone(),
            thickness: run.board.process.stackup.chosen_thickness(),
            edge_clearance: run.board.process.min_edge_clearance,
            assembled: !assembled.is_empty(),
            bottom_assembled: assembled.iter().any(|p| p.side == Side::Bottom),
        },
    };
    let package_options = Options {
        fit: run.fit.clone(),
        ..run.options.clone()
    };
    let pkg = netsmith_fab::build_panel(&panel, &input, &package_options)
        .map_err(|e| fail(e.to_string()))?;
    let mut files = pkg.write(dir).map_err(|e| fail(e.to_string()))?;
    let mut notes = pkg.notes.clone();
    if options.cross_check {
        // The viewer comparison covers a board: a panel's files have no board model in memory to
        // compare with, so its cross-check is KiCad's alone.
        let kicad = kicad.ok_or_else(|| {
            fail(
                "--cross-check of a panel compares with KiCad's own files, and KiCad is not installed (netsmith's viewer comparison covers the board of a panel, not the panel)"
                    .to_string(),
            )
        })?;
        say(format!("{}: cross-checking against KiCad", spec.name));
        let report = netsmith_fab::cross_check(
            &dir.join(format!("{}.kicad_pcb", spec.name)),
            kicad,
            &netsmith_fab::CrossCheckLimits {
                pictures: true,
                ..Default::default()
            },
        )
        .map_err(|e| fail(e.to_string()))?;
        for layer in &report.layers {
            if let Some(png) = layer.diff_png.as_ref().filter(|_| !layer.ok) {
                let file = dir.join(format!("cross-check-{}.png", layer.file));
                fs::write(&file, png).map_err(|e| fail(format!("{}: {e}", file.display())))?;
                files.push(file);
            }
        }
        if !report.ok() {
            return Err(fail(format!("{report}")));
        }
        notes.push_str(&format!("{report}"));
    }
    let process = &run.board.process;
    let facts = BoardFacts {
        layers: process.stackup.layers,
        width_mm: pkg.geometry.width,
        height_mm: pkg.geometry.height,
        thickness_mm: process.stackup.chosen_thickness(),
        finish: process.finish.clone(),
        mask_colour: process.stackup.mask_colour.clone(),
        assembled_parts: crate::order::stock_parts(&pkg.plan),
        bom: pkg.plan.bom.clone(),
        consigned: pkg.plan.consigned.clone(),
        hand_solder: pkg.plan.hand_solder.clone(),
        not_fitted: pkg.plan.not_fitted.clone(),
        unassembled: pkg.plan.unassembled.clone(),
        unverified: pkg.unverified.clone(),
        panel: Some(PanelFacts {
            board: spec.cell.clone(),
            columns: spec.columns,
            rows: spec.rows,
            cell_width_mm: pkg.geometry.cell_width,
            cell_height_mm: pkg.geometry.cell_height,
            rail_mm: spec.rail,
        }),
        // Each board of the panel has its cell's vias in pads (issue #1118).
        vias_in_pads: {
            let mut cell = run.board.filled_vias_in_pads();
            cell.count *= (spec.columns * spec.rows) as usize;
            cell
        },
        vias_touching_pads: {
            let mut cell = run.board.drilled_vias_touching_pads();
            cell.count *= (spec.columns * spec.rows) as usize;
            cell
        },
        // The panel's parts are its cells', named as bom.csv names them (`R1_3`).
        bought: run
            .fabricated
            .facts
            .bought
            .iter()
            .map(|l| crate::buying::BuyLine {
                designators: (1..=(spec.columns * spec.rows) as usize)
                    .flat_map(|k| {
                        l.designators
                            .iter()
                            .map(move |d| netsmith_fab::panel_designator(d, k))
                    })
                    .collect(),
                ..l.clone()
            })
            .collect(),
        bom_channels: run.fabricated.facts.bom_channels.clone(),
    };
    Ok(Fabricated {
        board: spec.name.clone(),
        dir: dir.to_path_buf(),
        gerber_files: pkg.gerber_files.clone(),
        summary: pkg.summary(),
        notes,
        files,
        facts,
        origins: Vec::new(),
        requirements: Vec::new(),
        place_warnings: Vec::new(),
        routes: Default::default(),
        stages_run: 0,
        reused: false,
    })
}

/// The rules of the board house for the writer, from the language's.
fn panel_rules(r: &netsmith_core::PanelRules) -> netsmith_fab::PanelBuildRules {
    netsmith_fab::PanelBuildRules {
        fabricator: r.fabricator.clone(),
        source: r.source.clone(),
        min_rail: r.min_rail,
        min_size: r.min_size,
        max_size: r.max_size,
        max_assembled_size: r.max_assembled_size,
        max_cuts: r.max_cuts,
        min_cut_spacing: r.min_cut_spacing,
        advised_cut_spacing: r.advised_cut_spacing,
        min_edge_clearance: r.min_edge_clearance,
        advised_edge_clearance: r.advised_edge_clearance,
        min_thickness: r.min_thickness,
        tooling_hole: r.tooling_hole,
        fiducial: r.fiducial,
        fiducial_offset: r.fiducial_offset,
    }
}

/// The lines of `fab-notes.txt` for the alternates of a board (issue 1312): each is not fitted
/// (DNP), on the footprint of the part it may take the place of, and passed the board's checks.
fn alternate_notes(
    alternates: &[netsmith_pipeline::alternates::FittedAlternate],
    for_review: bool,
) -> Vec<String> {
    alternates
        .iter()
        .map(|a| {
            let number = match (&a.sourcing.mpn, &a.sourcing.sku) {
                (Some(mpn), Some(lcsc)) => format!(" ({mpn}, {lcsc})"),
                (Some(n), None) | (None, Some(n)) => format!(" ({n})"),
                (None, None) => String::new(),
            };
            // A package made for review has no bill of materials, so no list of alternates.
            let listed = if for_review {
                ""
            } else {
                ", listed in bom-alternates.csv"
            };
            format!(
                "alternate (not fitted, DNP; not in bom.csv{listed}): {} may take `{}`{number} in the place of `{}` ({}:{}); the board's checks pass with it fitted",
                a.refdes, a.kind, a.primary, a.at.file, a.at.line
            )
        })
        .collect()
}

/// The rows of `bom-alternates.csv` (issue 1415): each alternate with the designator it replaces,
/// its manufacturer's number (its kind's name where its row has none) and its LCSC number.
fn alternate_entries(
    alternates: &[netsmith_pipeline::alternates::FittedAlternate],
) -> Vec<netsmith_fab::AlternateEntry> {
    alternates
        .iter()
        .map(|a| netsmith_fab::AlternateEntry {
            designator: a.refdes.clone(),
            part_number: a.sourcing.mpn.clone().unwrap_or_else(|| a.kind.clone()),
            lcsc: a.sourcing.sku.clone(),
        })
        .collect()
}

/// What the design says about how each part is fitted, for the packager, which would
/// otherwise read it from the footprint's attributes.
fn fit_of(sourcing: &[Sourced]) -> BTreeMap<String, Fit> {
    sourcing
        .iter()
        .map(|s| {
            let fit = match s.assembly {
                Assembly::Fab => Fit::Assembled,
                Assembly::Hand => Fit::HandSolder,
                Assembly::None => Fit::NotFitted,
                Assembly::Consigned => Fit::Consigned,
            };
            (s.refdes.clone(), fit)
        })
        .collect()
}

/// The bill of materials: one entry per distinct part, with every place it is used. Parts
/// the board house does not place are left out; a consigned part is kept, with its
/// manufacturer's number as the comment and no part number unless its row has one. A part it
/// should place but that has no part number is kept without one, so the packaging step can
/// refuse it by name.
///
/// Why a consigned part looks like this (ADR 0084; the owner accepted its seven
/// choices as built on 2026-11-11). `Fab` is "the board house
/// buys it" and `Consigned` is "the board house places it, and the owner supplies it"; they
/// share everything but the supplier. Both are in `cpl.csv` and in this bill. `Fab` needs an
/// `lcsc` (`fab` refuses without); `Consigned` does not, and its comment is the manufacturer's
/// number, the LCSC cell empty unless the catalog row has a number. The `fab-notes.txt` of a
/// consigned part has a `consigned:` line with each part's manufacturer's number and quantity.
/// The stock note (#202) and the refusal of an unresolved generic (#208) never apply to it: no
/// row stands for a part the owner supplies, so a consigned generic stays generic and no LCSC
/// number is invented for it. A consigned part and a part from stock are never one line, even
/// with equal value, footprint and number, because JLCPCB does not mix the two in one line.
///
/// What the board house reads, read on 2026-11-08 as pages by an agent (netsmith itself asks no
/// supplier, ADR 0039): JLCPCB's article "How to consign parts to JLCPCB" (the owner registers
/// the components on JLCPCB's site, ships them, and "the component seller must place PCBA orders
/// to use the registered components" within the free storage period) says nothing of the BOM or
/// the CPL. "Common BOM and CPL matching issues and explanations" and "Component matching
/// guidelines for PCBA orders": consigned components, like pre-ordered and globally sourced
/// ones, are chosen by the customer in the order page's component matching interface; JLCPCB
/// goes on only when all of them are in its warehouse; a row it cannot match is "No Part
/// Selected" and is matched by hand by the manufacturer's number or a C-code. JLCPCB's BOM guide
/// lists designator, footprint, comment and LCSC part number and no column or word that marks a
/// row as consigned (a different assembler, PCBasic, asks for the row to be marked; that is no
/// JLCPCB rule). So the format has no marker, and the choice was between an empty LCSC cell and
/// a made-up column or word. An unknown column is at best ignored and at worst fails the
/// upload; a word in the LCSC cell fails the check of `C` plus digits. The row therefore keeps
/// designator, footprint and comment, and the marking is in `fab-notes.txt` and in `order.txt`
/// of an order, not in `bom.csv`. Not tested: JLCPCB's order page. Nobody has uploaded a
/// `bom.csv` with an empty cell for a registered consigned part from netsmith (a person must do
/// this; checklist in issue #10); the notes tell the person ordering to look at the matching
/// interface (the `prepare-fab-order` step). If
/// JLCPCB documents a marker, the mark changes in this one function.
///
/// The supply channel (ADR 0143). The LCSC cell is filled from the part's sku only when the
/// channel that buys it reads LCSC's offers (`jlcpcb-assembly`, `lcsc-retail`), so a project
/// that buys from those alone gets the bill it got before, byte for byte. A consigned part bought
/// in `digikey-retail` or `mouser-retail` keeps its manufacturer's number as the comment and an
/// empty cell, and `fab-notes.txt` lists it with its channel and sku ([`crate::buying`]). A part
/// the board house is to buy from such a channel is refused before anything is placed
/// ([`crate::no_supplier`]): JLCPCB buys by LCSC's numbers.
///
/// `paths` is the member path of each part of `sourcing`, in its order, so that a part a
/// `[[member]]` row of `kicad.toml` gives its own footprint is listed with it; a part past its
/// end takes the footprint of its kind and package.
pub fn bom_entries(
    sourcing: &[Sourced],
    paths: &[String],
    mapping: &Mapping,
) -> Result<Vec<BomEntry>, String> {
    let mut entries: Vec<BomEntry> = Vec::new();
    for (i, s) in sourcing
        .iter()
        .enumerate()
        .filter(|(_, s)| s.assembled() || s.not_fitted.is_some())
    {
        let path = paths.get(i).map_or("", String::as_str);
        let footprint = mapping
            .map_part(path, &s.kind, s.package.as_deref())
            .map_err(|e| format!("{}: {e}", s.refdes))?
            .footprint;
        // The board names a footprint without its library: `R_0402_1005Metric`.
        let footprint = footprint
            .rsplit(':')
            .next()
            .unwrap_or(&footprint)
            .to_string();
        // The comment of a consigned part is the manufacturer's number: JLCPCB has no column that
        // marks a part as consigned, so the board house reads what the owner shipped from it
        // (ADR 0084).
        let value = match (&s.not_fitted, s.assembly, &s.mpn) {
            (Some(dnp), _, _) => dnp_comment(&s.value, dnp),
            (None, Assembly::Consigned, Some(mpn)) => mpn.clone(),
            _ => s.value.clone(),
        };
        // A part that is not fitted is a line of its own with no number to buy (issue #1273).
        // The cell holds an LCSC number only: a part bought in another channel has none here
        // (ADR 0143), and `fab-notes.txt` names its channel and sku.
        let lcsc = if s.not_fitted.is_some() {
            None
        } else {
            s.lcsc_number().map(str::to_string)
        };
        match entries
            .iter_mut()
            .find(|e| e.value == value && e.footprint == footprint && e.lcsc == lcsc)
        {
            Some(entry) => entry.designators.push(s.refdes.clone()),
            None => entries.push(BomEntry {
                designators: vec![s.refdes.clone()],
                value,
                footprint,
                lcsc,
            }),
        }
    }
    for e in &mut entries {
        e.designators.sort_by_key(|d| designator_key(d));
    }
    entries.sort_by_key(|e| designator_key(&e.designators[0]));
    Ok(entries)
}

/// The comment of the `bom.csv` line of a part that is not fitted (issue #1273): `DNP`, the
/// value it starts from when the design states one, and the range it is tuned in:
/// `DNP 4.7pF (tune 1pF..10pF)`, or `DNP (tune 1pF..10pF)` for a part that has no value but
/// its range. The word comes first, so that a person reading the bill, and the board house,
/// see it before the value.
pub fn dnp_comment(value: &str, dnp: &netsmith_core::NotFitted) -> String {
    match &dnp.tune {
        Some(range) if range == value => format!("DNP (tune {range})"),
        Some(range) => format!("DNP {value} (tune {range})"),
        None => format!("DNP {value}"),
    }
}

/// `C2` before `C10`: the letters, then the number.
fn designator_key(designator: &str) -> (String, u32) {
    let split = designator
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(designator.len());
    let (letters, digits) = designator.split_at(split);
    (letters.to_string(), digits.parse().unwrap_or(0))
}

/// Route `board` with signals at the project's track margin where there is room (issue #431):
/// the router tries the margin first, in one order of the lines and a quarter of the route
/// time, and a board it leaves unfinished is routed again from the same placement at the
/// process's least track, in what is left. The short trial keeps a board that has no room for
/// the margin from spending most of its time on it first.
/// `netsmith dev route` routes a saved placed board with it too, so the two route alike. `base`
/// gives the router's other options (`fab` gives the defaults, `route` may set the costs, the
/// tries and the snapshots); its time limit and retries are set here.
pub fn route_with_margin(
    board: &mut Board,
    route_timeout: Option<Duration>,
    base: RouteOptions,
    track_margin: f64,
    say: &dyn Fn(String),
) -> netsmith_router::Routing {
    let routing = with_margin(
        board,
        route_timeout,
        base,
        track_margin,
        say,
        route_with,
        |routing| routing.left.len(),
    );
    say_rerouted(&board.name, &routing, say);
    routing
}

/// The nets the router's reroute pass laid again (issue #1538), one line each.
pub fn say_rerouted(board: &str, routing: &netsmith_router::Routing, say: &dyn Fn(String)) {
    for r in &routing.rerouted {
        let before = if r.with.is_empty() {
            String::new()
        } else {
            format!(", laid before {}", r.with.join(", "))
        };
        say(format!(
            "{board}: rerouted net {}{before}: {:.1} mm of track by the weights is now {:.1} mm",
            r.net, r.before, r.after
        ));
    }
}

/// [`route_with_margin`], keeping the router's copper of `before` for every net the placement
/// left alone and `rip` does not name (`netsmith_router::carry::reroute`, `board fab --reuse-routes`).
pub fn reroute_with_margin(
    board: &mut Board,
    before: &Board,
    rip: &std::collections::BTreeSet<String>,
    route_timeout: Option<Duration>,
    base: RouteOptions,
    track_margin: f64,
    say: &dyn Fn(String),
) -> netsmith_router::carry::Rerouted {
    let done = with_margin(
        board,
        route_timeout,
        base,
        track_margin,
        say,
        |board, options| netsmith_router::carry::reroute(before, board, rip, options),
        |done| done.routing.left.len(),
    );
    say_rerouted(&board.name, &done.routing, say);
    done
}

fn with_margin<T>(
    board: &mut Board,
    route_timeout: Option<Duration>,
    base: RouteOptions,
    track_margin: f64,
    say: &dyn Fn(String),
    run: impl Fn(&mut Board, RouteOptions) -> T,
    left: impl Fn(&T) -> usize,
) -> T {
    // The trial at the margin is one try with no rounds of rip-up (issue #1088): a board it
    // leaves goes to the least track as before, where the rounds run.
    let route = |board: &mut Board, time: Option<Duration>, full: bool| {
        run(
            board,
            RouteOptions {
                retry_orders: full,
                time_limit: time,
                rip_rounds: if full { base.rip_rounds } else { 0 },
                ..base
            },
        )
    };
    let least = board.rules.min_track_width;
    let wide = (least * track_margin * 100.0).round() / 100.0;
    if wide <= board.rules.track_width + 1e-9 {
        return route(board, route_timeout, true);
    }
    let started = std::time::Instant::now();
    let mut trial = board.clone();
    trial.rules.track_width = wide;
    let routing = route(&mut trial, route_timeout.map(|t| t / 4), false);
    if left(&routing) == 0 {
        *board = trial;
        return routing;
    }
    say(format!(
        "{}: {} connection(s) left with signals at {wide} mm; routing again at the least track, {least} mm",
        board.name,
        left(&routing)
    ));
    let left = route_timeout.map(|t| t.saturating_sub(started.elapsed()));
    route(board, left, true)
}

/// The name of the guideline that an impedance target a board cannot give breaks (issue
/// #1025), as its message names it: `Board: impedance target guideline: ...`.
pub const IMPEDANCE_RULE: &str = "impedance target";

/// The pads of a pin that carries `current` on `net`, one entry for each part of a bank, as the
/// design rules check reads a branch (issues #1113 and #1333).
fn drc_branches(
    net: &str,
    path: &str,
    pins: &[netsmith_core::PinRef],
    current: f64,
) -> Vec<netsmith_geom::DrcBranch> {
    let mut parts: Vec<&str> = pins.iter().map(|p| p.refdes.as_str()).collect();
    parts.sort_unstable();
    parts.dedup();
    parts
        .into_iter()
        .map(|part| netsmith_geom::DrcBranch {
            net: net.to_string(),
            path: path.to_string(),
            part: part.to_string(),
            pads: pins
                .iter()
                .filter(|p| p.refdes == part)
                .map(|p| p.pin.clone())
                .collect(),
            current,
        })
        .collect()
}

/// What the design rules check needs that the board file does not say: the process's mask
/// web and widest filled via, the rules a project file cannot say and the least width of the
/// nets that carry current (issue #431), the
/// courtyards a protruding pin excuses (ADR 0045 section 15), the stackup the process makes
/// (issue #993), and the layout file's waivers of findings of a part's own footprint (issue
/// #1196).
pub fn drc_extras(board: &Board) -> netsmith_geom::DrcExtras {
    netsmith_geom::DrcExtras {
        mask_web: Some(board.rules.mask_bridge),
        via_in_pad: Some(board.process.via_in_pad),
        excused_courtyards: board.excused_courtyards(),
        mask_to_track: Some(board.rules.mask_to_track),
        least_widths: board.least_widths(),
        inner_least_widths: board.inner_least_widths(),
        target_widths: board.target_track_widths(),
        neck_reach: netsmith_router::route::NECK_REACH,
        via_counts: board.via_counts(),
        via_group_gap: board.rules.group_gap(),
        track_angle: true,
        round_track_angle: board.process.min_round_track_angle,
        marks: Some(marks_of(board)),
        stackup: Some(netsmith_geom::DrcStackup {
            layers: board.process.stackup.layers as usize,
            thickness: board.process.stackup.thickness,
            tolerance: netsmith_pcb::thickness_tolerance(
                board
                    .process
                    .stackup
                    .chosen_thickness()
                    .unwrap_or(board.process.stackup.thickness.1),
            ),
        }),
        vias: Some(via_rules(board)),
        branches: board
            .branch_currents
            .iter()
            .flat_map(|b| drc_branches(&b.net, &b.path, &b.pins, b.current))
            .collect(),
        // The pins whose current the model knows (issue #1333).
        pin_currents: board
            .pin_currents
            .iter()
            .flat_map(|l| drc_branches(&l.net, &l.path, &l.pins, l.current))
            .collect(),
        sizing: netsmith_design::guidelines::sizing_of(&board.process),
        net_currents: board
            .net_currents
            .iter()
            .map(|c| netsmith_geom::DrcNetCurrent {
                net: c.net.clone(),
                current: c.current,
                source: c.source.clone(),
            })
            .collect(),
        // The layout file's waivers of findings of a part's own footprint (issue #1196).
        waivers: board.drc_waivers(),
        strict_guidelines: false,
        open: Vec::new(),
        // The rules the `.kicad_dru` beside the board holds for KiCad too (issue #1185).
        ..board.rules.process_rules()
    }
}

/// The vias the board's process makes, for the `via_span` and `via_aspect_ratio` rules (issue
/// #1015): the blind and buried spans of an HDI process, none of any other, and its aspect
/// ratios against the board's build-up.
fn via_rules(board: &Board) -> netsmith_geom::DrcViaRules {
    let n = board.copper_count();
    let spans = board
        .via_offers()
        .iter()
        .filter_map(|o| {
            let (a, b) = o.span?.positions(n);
            Some((a, b, o.laser))
        })
        .collect();
    let dielectrics = board
        .process
        .stackup
        .inner
        .as_ref()
        .map(|i| i.dielectrics.iter().map(|d| d.thickness).collect())
        .unwrap_or_default();
    match &board.process.hdi {
        Some(h) => netsmith_geom::DrcViaRules {
            spans,
            dielectrics,
            min_laser_drill: h.min_laser_drill,
            max_laser_drill: h.max_laser_drill,
            min_ring: h.min_ring,
            max_buried_drill: h.max_buried_drill,
            max_laser_aspect: h.max_laser_aspect,
            max_drill_aspect: h.max_drill_aspect,
        },
        None => netsmith_geom::DrcViaRules {
            spans,
            ..Default::default()
        },
    }
}

/// The fiducials and test points of a board, which of its sides the board house assembles
/// parts on, and the process's rules for them (issue #761). A part is assembled when it is
/// surface mount, is not left to be fitted by hand or not at all, and is no hole, fiducial,
/// test point or label.
pub fn marks_of(board: &Board) -> netsmith_geom::DrcMarks {
    use netsmith_core::PartKind;
    use netsmith_pcb::{Mount, Side};
    let p = &board.process;
    let named = |kind: PartKind| -> Vec<String> {
        board
            .parts
            .iter()
            .filter(|part| part.declared_kind == Some(kind))
            .map(|part| part.refdes.clone())
            .collect()
    };
    let assembled = |side: Side| {
        board.parts.iter().any(|part| {
            part.side == side
                && !part.hand_fitted
                && part.footprint.mount == Mount::Smd
                && !part.declared_kind.is_some_and(|k| {
                    k.is_mechanical()
                        || matches!(k, PartKind::TestPoint | PartKind::Label | PartKind::Logo)
                })
        })
    };
    // A net is a supply or a ground when a `Rail` of the design has a pin on it (the design
    // stage hands the board its `rail_nets`, issue #1006), or a pin the part's pin table calls
    // `power` is on it (ADR 0016, `power_pads`, what the placer reads to find the supply nets): a
    // chip's supply and ground pins. A test point's own pin is `passive`, so it never makes its
    // net one.
    let power_nets: Vec<String> = board
        .nets
        .iter()
        .filter(|net| {
            board.rail_nets.contains(&net.name)
                || net.pads.iter().any(|pad| {
                    board
                        .parts
                        .iter()
                        .any(|p| p.refdes == pad.refdes && p.power_pads.contains(&pad.number))
                })
        })
        .map(|net| net.name.clone())
        .collect();
    // A part is tall when the design or its catalog row states a height above the process's. An
    // unknown height is no height: the part is judged as any other (the package standard's
    // figure is an upper bound and none of the table is above 6.4 mm, so it never shows a part
    // to be tall).
    let tall_parts: Vec<(String, f64)> = board
        .parts
        .iter()
        .filter_map(|part| Some((part.refdes.clone(), part.height?)))
        .filter(|(_, h)| p.tall_part_height > 0.0 && *h > p.tall_part_height + 1e-9)
        .collect();
    netsmith_geom::DrcMarks {
        fiducials: named(PartKind::Fiducial),
        test_points: named(PartKind::TestPoint),
        assembled_front: assembled(Side::Front),
        assembled_back: assembled(Side::Back),
        min_fiducials: p.min_fiducials.round().max(0.0) as usize,
        fiducial_diameter: p.fiducial_diameter,
        fiducial_mask_ratio: p.fiducial_mask_ratio,
        min_fiducial_edge: p.min_fiducial_edge,
        min_test_point_pad: p.min_test_point_pad,
        min_test_point_pitch: p.min_test_point_pitch,
        min_test_point_body: p.min_test_point_body,
        min_test_point_edge: p.min_test_point_edge,
        power_nets,
        min_power_test_point_pad: p.min_power_test_point_pad,
        tall_parts,
        tall_part_height: p.tall_part_height,
        min_test_point_tall_body: p.min_test_point_tall_body,
        round_angle: p.fiducial_round_angle,
        fine_pitch: p.fine_pitch,
        uneven_finish: p.finish.starts_with("HASL").then(|| p.finish.clone()),
        min_single_sided_assembly_size: p.min_single_sided_assembly_size,
        min_double_sided_assembly_size: p.min_double_sided_assembly_size,
        assembly_size_source: p.assembly_size_source.clone(),
    }
}

#[cfg(test)]
mod tests {
    use netsmith_router::Weights;

    fn project_with(keys: &str) -> crate::project::Manifest {
        crate::project::Manifest::parse(&format!("[project]\nname = \"a\"\n{keys}")).unwrap()
    }

    /// The weights of a run (issue 1536): none stated is the router's own routing; a key states
    /// the weights it names and the router's own stand for the rest; a flag wins over its key.
    #[test]
    fn a_flag_wins_over_the_key_of_the_same_name() {
        let none = (None, None, None);
        assert_eq!(super::weights(None, none), Ok(None));
        assert_eq!(super::weights_with(Some(&project_with("")), none), Ok(None));
        let keyed = project_with("via_weight = 40\nlayer_change_weight = 2\n");
        let own = Weights::DEFAULT;
        assert_eq!(
            super::weights_with(Some(&keyed), none),
            Ok(Some(Weights {
                via: 40.0,
                layer_change: 2.0,
                ..own
            }))
        );
        assert_eq!(
            super::weights_with(Some(&keyed), (Some(0.5), Some(8.0), None)),
            Ok(Some(Weights {
                length: 0.5,
                via: 8.0,
                layer_change: 2.0
            }))
        );
        // A flag with no project, and a flag out of its range.
        assert_eq!(
            super::weights_with(None, (None, Some(45.0), None)),
            Ok(Some(Weights { via: 45.0, ..own }))
        );
        let e = super::weights_with(Some(&keyed), (Some(0.01), None, None)).unwrap_err();
        assert!(e.contains("the length weight is 0.01"), "{e}");
    }

    fn moved_seed(stated: bool) -> crate::layout::Seeds {
        crate::layout::Seeds {
            first: 3,
            stated,
            limit: 8,
            used: 4,
            placed: true,
            missed: vec![(
                3,
                "the courtyards of `usb` (J2) and `c37` (C37) overlap by 0.42 mm2".into(),
            )],
            blockers: Vec::new(),
            budget: netsmith_core::job::Limit::default(),
            ran_out: false,
        }
    }

    #[test]
    fn a_seed_that_moved_is_written_to_the_layout_file_and_said() {
        let dir = netsmith_testkit::TempDir::new("settle-seed");
        let file = dir.path().join("main.pcb.toml");
        std::fs::write(&file, "version = 1\n\n[Board]\nseed = 3\n").unwrap();
        let options = FabOptions {
            layout: Some(file.clone()),
            ..FabOptions::default()
        };
        let said = std::cell::RefCell::new(Vec::new());
        settle_seed(
            ("Board", "Board"),
            &moved_seed(true),
            &None,
            &options,
            &|m| said.borrow_mut().push(m),
        )
        .unwrap();
        let name = file.display();
        assert_eq!(
            said.into_inner(),
            vec![format!(
                "Board: note: the seed 3 that {name} states no longer places the board, so it is stale: the placer tried seed 3 (the courtyards of `usb` (J2) and `c37` (C37) overlap by 0.42 mm2); seed 4 places it, and is written to {name}"
            )]
        );
        let layout =
            netsmith_pcb::Layout::parse(&std::fs::read_to_string(&file).unwrap(), "f").unwrap();
        assert_eq!(layout.boards["Board"].seed, Some(4));
    }

    #[test]
    fn an_order_or_the_editors_release_stops_on_a_seed_that_moved_and_writes_nothing() {
        let dir = netsmith_testkit::TempDir::new("settle-seed-order");
        let file = dir.path().join("main.pcb.toml");
        std::fs::write(&file, "version = 1\n\n[Board]\nseed = 3\n").unwrap();
        let name = file.display();
        for (writes, what) in [
            (SeedWrites::Order, "an order"),
            (SeedWrites::EditorRelease, "a release from the editor"),
        ] {
            let options = FabOptions {
                layout: Some(file.clone()),
                seed_writes: writes,
                ..FabOptions::default()
            };
            let e = settle_seed(
                ("Board", "Board"),
                &moved_seed(true),
                &None,
                &options,
                &|_| {},
            )
            .unwrap_err();
            assert_eq!(
                e,
                format!(
                    "error: Board: the seed 3 that {name} states no longer places the board, so it is stale: the placer tried seed 3 (the courtyards of `usb` (J2) and `c37` (C37) overlap by 0.42 mm2); seed 4 places it, and {what} is a release, which never changes a choice by itself: run `netsmith board fab` on the source to write `seed = 4` to {name}, review it and commit it, then make the release again\n"
                )
            );
            assert_eq!(
                std::fs::read_to_string(&file).unwrap(),
                "version = 1\n\n[Board]\nseed = 3\n"
            );
        }
    }

    #[test]
    fn with_locked_a_seed_that_moved_is_an_error_and_nothing_is_written() {
        let dir = netsmith_testkit::TempDir::new("settle-seed-locked");
        let file = dir.path().join("main.pcb.toml");
        let options = FabOptions {
            layout: Some(file.clone()),
            lock: Some(netsmith_pipeline::pick_lock::Use {
                file: dir.path().join("main.lock.toml"),
                locked: true,
                off: false,
            }),
            ..FabOptions::default()
        };
        let e = settle_seed(
            ("Board", "Board"),
            &moved_seed(false),
            &None,
            &options,
            &|_| {},
        )
        .unwrap_err();
        let name = file.display();
        assert_eq!(
            e,
            format!(
                "error: Board: seed 3 (the default: {name} states no seed) does not place the board: the placer tried seed 3 (the courtyards of `usb` (J2) and `c37` (C37) overlap by 0.42 mm2); seed 4 places it, and --locked says {name} must not be written: run without --locked to write `seed = 4` to it, and commit it\n"
            )
        );
        assert!(!file.exists());
    }

    #[test]
    fn the_remake_gets_what_is_left_of_the_budget_or_a_stated_floor() {
        use crate::pin_swap::{Budget, Clock};
        use std::sync::atomic::{AtomicU64, Ordering};
        let secs = Duration::from_secs;
        // A clock that has moved `elapsed` seconds since the budget started.
        let at = |total: Option<Duration>, elapsed: u64| {
            let now = std::sync::Arc::new(AtomicU64::new(0));
            let clock = {
                let now = now.clone();
                Clock::from_fn(move || Duration::from_secs(now.load(Ordering::SeqCst)))
            };
            let budget = Budget::start(total, &clock);
            now.store(elapsed, Ordering::SeqCst);
            budget
        };
        // Time left: the remake gets that, not the whole timeout again.
        assert_eq!(
            remake_timeout(&at(Some(secs(120)), 100), Some(secs(120))),
            Some(secs(20))
        );
        // Spent: the floor, but never more than the timeout itself.
        assert_eq!(
            remake_timeout(&at(Some(secs(120)), 130), Some(secs(120))),
            Some(REMAKE_FLOOR)
        );
        assert_eq!(
            remake_timeout(&at(Some(secs(3)), 9), Some(secs(3))),
            Some(secs(3))
        );
        // Nearly spent: a second of 300 s left is no time to route in, and the router cut off
        // after it leaves the board worse than the pass before. The floor applies below it too.
        assert_eq!(
            remake_timeout(&at(Some(secs(300)), 299), Some(secs(300))),
            Some(REMAKE_FLOOR)
        );
        assert_eq!(
            remake_timeout(&at(Some(secs(300)), 291), Some(secs(300))),
            Some(REMAKE_FLOOR),
            "9 s left is below the floor"
        );
        assert_eq!(
            remake_timeout(&at(Some(secs(300)), 290), Some(secs(300))),
            Some(secs(10)),
            "exactly the floor is what is left"
        );
        assert_eq!(
            remake_timeout(&at(Some(secs(300)), 280), Some(secs(300))),
            Some(secs(20))
        );
        assert_eq!(
            remake_timeout(&at(Some(secs(3)), 2), Some(secs(3))),
            Some(secs(3)),
            "never more than the timeout itself"
        );
        // No limit stays none.
        assert_eq!(remake_timeout(&at(None, 500), None), None);
    }

    #[test]
    fn a_remake_that_leaves_more_connections_does_not_replace_the_files_before_it() {
        // Fewer or as many connections left: the remake is as good, and the pin file's map
        // matches the files.
        assert!(!keep_earlier_files(Some(2), Some(3)));
        assert!(!keep_earlier_files(Some(3), Some(3)));
        assert!(!keep_earlier_files(Some(0), Some(3)));
        // More: the files before it stay.
        assert!(keep_earlier_files(Some(4), Some(3)));
        // No board made: nothing to put in their place.
        assert!(keep_earlier_files(None, Some(3)));
        // What is said of it depends on which happened.
        assert!(why_earlier_files_stay(Some(4)).contains("left more connections"));
        assert!(why_earlier_files_stay(None).contains("made no board"));
        assert!(!why_earlier_files_stay(None).contains("left more connections"));
        assert!(keep_earlier_files(None, None));
        // The files before are not known: the remake is not held against them.
        assert!(!keep_earlier_files(Some(9), None));
    }

    #[test]
    fn the_files_set_aside_come_back_whole() {
        let root = netsmith_testkit::TempDir::new("fab-set-aside");
        let dir = root.join("Board");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Board.kicad_pcb"), "earlier").unwrap();
        fs::write(dir.join("kept-only-here.txt"), "earlier").unwrap();
        let aside = set_aside(&dir).expect("a folder is moved aside");
        assert!(!dir.exists());
        // A pass writes the folder afresh, and less of it.
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Board.kicad_pcb"), "worse").unwrap();
        set_back(&dir, &aside).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("Board.kicad_pcb")).unwrap(),
            "earlier"
        );
        assert!(dir.join("kept-only-here.txt").is_file());
        assert!(!aside.exists());
        // No folder, nothing to set aside.
        assert!(set_aside(&root.join("Nothing")).is_none());
    }
    #[test]
    fn the_note_without_kicad_says_netsmiths_own_check_still_runs() {
        let note = super::no_kicad_note(&netsmith_kicad::Search::under("/nowhere"));
        assert!(note.contains("KiCad is not installed"), "{note}");
        assert!(note.contains("--cross-check"), "{note}");
        assert!(
            note.contains("netsmith's own design rules check still runs"),
            "{note}"
        );
        assert!(!note.contains("none of its own"), "{note}");
    }

    #[test]
    fn the_note_says_when_kicad_is_switched_off_and_not_that_it_is_not_installed() {
        let note = super::no_kicad_note(&netsmith_kicad::Search::under("/nowhere").with_no_kicad());
        assert!(
            note.contains("KiCad is switched off (NETSMITH_NO_KICAD is set)"),
            "{note}"
        );
        assert!(!note.contains("not installed"), "{note}");
        assert!(
            note.contains("netsmith's own design rules check still runs"),
            "{note}"
        );
    }

    use super::*;

    fn sourced(
        refdes: &str,
        kind: &str,
        value: &str,
        package: Option<&str>,
        lcsc: Option<&str>,
        assembly: Assembly,
    ) -> Sourced {
        Sourced {
            refdes: refdes.into(),
            kind: kind.into(),
            value: value.into(),
            package: package.map(Into::into),
            sku: lcsc.map(Into::into),
            channel: None,
            manufacturer: None,
            mpn: None,
            library: None,
            assembly,
            assembly_stated: false,
            catalog_height: None,
            generic: None,
            resolved_row: None,
            not_fitted: None,
        }
    }

    #[test]
    fn identical_parts_share_one_line_with_designators_in_order() {
        let entries = bom_entries(
            &[
                sourced(
                    "C10",
                    "Capacitor",
                    "100n",
                    Some("0402"),
                    Some("C1525"),
                    Assembly::Fab,
                ),
                sourced(
                    "R1",
                    "Resistor",
                    "330",
                    Some("0402"),
                    Some("C25104"),
                    Assembly::Fab,
                ),
                sourced(
                    "C2",
                    "Capacitor",
                    "100n",
                    Some("0402"),
                    Some("C1525"),
                    Assembly::Fab,
                ),
            ],
            &[],
            &Mapping::standard(),
        )
        .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].designators, ["C2", "C10"]);
        assert_eq!(
            entries[0].footprint, "C_0402_1005Metric",
            "the library prefix is dropped"
        );
        assert_eq!(entries[0].lcsc.as_deref(), Some("C1525"));
        assert_eq!(entries[1].designators, ["R1"]);
    }

    #[test]
    fn parts_that_are_not_assembled_stay_out() {
        let entries = bom_entries(
            &[
                sourced("J1", "Header1x2", "Conn_01x02", None, None, Assembly::Hand),
                sourced(
                    "R1",
                    "Resistor",
                    "330",
                    Some("0402"),
                    Some("C25104"),
                    Assembly::Fab,
                ),
            ],
            &[],
            &Mapping::standard(),
        )
        .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].designators, ["R1"]);
    }

    #[test]
    fn an_assembled_part_without_a_number_is_kept_so_it_can_be_refused_by_name() {
        let entries = bom_entries(
            &[sourced("D1", "Led", "LED", None, None, Assembly::Fab)],
            &[],
            &Mapping::standard(),
        )
        .unwrap();
        assert_eq!(entries[0].lcsc, None);
    }

    #[test]
    fn the_same_value_with_different_part_numbers_stays_two_lines() {
        let entries = bom_entries(
            &[
                sourced(
                    "R1",
                    "Resistor",
                    "1k",
                    Some("0402"),
                    Some("C11702"),
                    Assembly::Fab,
                ),
                sourced(
                    "R2",
                    "Resistor",
                    "1k",
                    Some("0402"),
                    Some("C99999"),
                    Assembly::Fab,
                ),
            ],
            &[],
            &Mapping::standard(),
        )
        .unwrap();
        assert_eq!(entries.len(), 2);
    }
    fn line(
        net: &str,
        from: (&str, &str, f64, f64),
        to: (&str, &str, f64, f64),
    ) -> netsmith_pcb::Ratline {
        let pad = |(refdes, number, _, _): (&str, &str, f64, f64)| netsmith_pcb::PadRef {
            refdes: refdes.into(),
            number: number.into(),
        };
        netsmith_pcb::Ratline {
            net: net.into(),
            from: pad(from),
            to: pad(to),
            from_at: netsmith_pcb::Pt::new(from.2, from.3),
            to_at: netsmith_pcb::Pt::new(to.2, to.3),
            to_plane: false,
        }
    }

    #[test]
    fn the_routing_failure_lists_each_connection_with_its_net_pads_and_places() {
        let left = netsmith_pcb::Ratsnest {
            lines: vec![
                line("SIG_A", ("U1", "A3", 15.0, 25.5), ("R1", "1", 12.0, 21.0)),
                line("SIG_B", ("U1", "C1", 16.1, 25.5), ("J1", "2", 30.0, 5.0)),
            ],
        };
        let text = routing_failure("Demo", netsmith_pcb::Pt::new(10.0, 20.0), &left, 4, &[]);
        assert!(
            text.starts_with(
                "INCOMPLETE: Demo is not fully routed. 2 connection(s) have no copper after 4 tries"
            ),
            "{text}"
        );
        assert!(
            text.contains("net SIG_A: U1.A3 at (5.00, 5.50) to R1.1 at (2.00, 1.00)\n"),
            "{text}"
        );
        assert!(
            text.contains("net SIG_B: U1.C1 at (6.10, 5.50) to J1.2 at (20.00, -15.00)\n"),
            "{text}"
        );
        assert!(
            text.contains("  U1: 2\n") && text.contains("  R1: 1\n"),
            "{text}"
        );
        assert!(!text.contains("Why each line is left"), "{text}");
    }

    #[test]
    fn the_routing_failure_says_where_each_line_failed_and_what_is_in_its_way() {
        let left = netsmith_pcb::Ratsnest {
            lines: vec![
                line("SIG_A", ("U1", "A3", 15.0, 25.5), ("R1", "1", 12.0, 21.0)),
                line("SIG_B", ("U1", "C1", 16.1, 25.5), ("J1", "2", 30.0, 5.0)),
            ],
        };
        let why = vec![
            netsmith_router::LineReport {
                line: left.lines[0].clone(),
                stage: netsmith_router::Stage::Negotiation,
                blockers: vec!["GND".into(), "SIG_C".into()],
                pads: vec![],
                vias: vec![],
                edge: false,
            },
            netsmith_router::LineReport {
                line: left.lines[1].clone(),
                stage: netsmith_router::Stage::OutOfTime,
                blockers: vec![],
                pads: vec![],
                vias: vec![],
                edge: false,
            },
            netsmith_router::LineReport {
                line: left.lines[1].clone(),
                stage: netsmith_router::Stage::RipRounds,
                blockers: vec!["SIG_D".into()],
                pads: vec![
                    netsmith_pcb::PadRef {
                        refdes: "U1".into(),
                        number: "C2".into(),
                    },
                    netsmith_pcb::PadRef {
                        refdes: "U1".into(),
                        number: "D1".into(),
                    },
                ],
                vias: vec![netsmith_router::WallVia {
                    net: "GND".into(),
                    at: netsmith_pcb::Pt::new(16.5, 26.0),
                }],
                edge: true,
            },
        ];
        let text = routing_failure("Demo", netsmith_pcb::Pt::new(10.0, 20.0), &left, 4, &why);
        // What readers match comes first, as it was.
        assert_eq!(
            text,
            routing_failure("Demo", netsmith_pcb::Pt::new(10.0, 20.0), &left, 4, &[])
                + &text[text.find("\nWhy each line is left").unwrap()..],
        );
        assert_eq!(text.matches("\nnet ").count(), 2, "{text}");
        assert!(
            text.contains(
                "  net SIG_A: U1.A3 to R1.1: failed in the negotiation; in its way: net(s) GND, SIG_C\n"
            ),
            "{text}"
        );
        assert!(
            text.contains("  net SIG_B: U1.C1 to J1.2: not finished, the time ran out; in its way: no net's copper"),
            "{text}"
        );
        // A line walled in (issue #1088): the pads, the vias, in the outline's coordinates, and
        // the edge.
        assert!(
            text.contains(
                "  net SIG_B: U1.C1 to J1.2: failed in the rounds that rip up whole nets; in its way: net(s) SIG_D; pad(s) U1.C2, U1.D1; via(s) of GND at (6.50, 6.00); the board edge\n"
            ),
            "{text}"
        );
        assert!(text.contains("routing-report.json"), "{text}");
    }

    #[test]
    fn an_incomplete_board_writes_its_report_and_the_error_says_where_everything_is() {
        let dir = netsmith_testkit::TempDir::new("incomplete");
        let incomplete = Incomplete {
            message: "1 connections could not be routed (after 4 tries):\n  net A: U1.1 to R1.1"
                .into(),
            report: "INCOMPLETE: Demo\n".into(),
            cost: None,
            json: "{}\n".into(),
            lines: vec!["ERROR: not routed: net A: U1.1 to R1.1".into()],
        };
        // The package could not be made (for instance a requirement is unmet): the report is
        // still written and the error says why there are no Gerbers.
        let message = write_incomplete(
            &incomplete,
            &dir,
            Err(netsmith_fab::Error::Io("no package".into())),
        );
        assert!(
            message.starts_with("1 connections could not be routed"),
            "{message}"
        );
        assert!(
            message.contains("INCOMPLETE: the board file is in")
                && message.contains("the Gerbers were not made: no package")
                && message.contains("routing-failure.txt"),
            "{message}"
        );
        assert_eq!(
            fs::read_to_string(dir.join("routing-failure.txt")).unwrap(),
            "INCOMPLETE: Demo\n"
        );
        assert_eq!(
            fs::read_to_string(dir.join("routing-report.json")).unwrap(),
            "{}\n"
        );
        assert_eq!(
            incomplete.headline(),
            "1 connection(s) have no copper; this package is NOT ready to order, see routing-failure.txt"
        );
    }

    /// The fields of a process that are not a rule the router and the design rules check keep,
    /// each with the reason (issue #431).
    const NOT_A_COPPER_RULE: [(&str, &str); 38] = [
        (
            "finish",
            "the surface finish, a word for the notes and the order",
        ),
        (
            "finishes",
            "the finishes the process offers, which the design stage checks `.finish` against",
        ),
        (
            "hole_plating",
            "the track width check rates a via for current by it (issue #431); the counts it gives are the router's",
        ),
        (
            "track_width_tolerance",
            "the track width check sizes for the narrowest finished track (issue #431); the widths it gives are the router's",
        ),
        (
            "min_plane_hole_clearance",
            "kept by the plane's fill, which the check does not judge (ADR 0037)",
        ),
        ("inner1", "a plane or a signal layer, which the board says"),
        ("inner2", "a plane or a signal layer, which the board says"),
        ("inner3", "a plane or a signal layer, which the board says"),
        ("inner4", "a plane or a signal layer, which the board says"),
        ("inner5", "a plane or a signal layer, which the board says"),
        ("inner6", "a plane or a signal layer, which the board says"),
        ("inner7", "a plane or a signal layer, which the board says"),
        ("inner8", "a plane or a signal layer, which the board says"),
        ("fabricator", "a name, for the notes"),
        ("source", "where the numbers are published, for the notes"),
        (
            "assembly_size_source",
            "where the assembly size figures come from, a sentence the `assembly_size` finding quotes (issue 1393)",
        ),
        (
            "stackup",
            "the build of the board: the writers' and the current check's",
        ),
        ("top", "a pour, which the board names the net of"),
        ("bottom", "a pour, which the board names the net of"),
        (
            "max_temperature_rise",
            "the track width check's rise (ADR 0036), not a distance",
        ),
        (
            "min_silk_width",
            "checked as the text's stroke (`min_text_thickness`) only; a drawn line is the footprint's",
        ),
        (
            "min_fiducials",
            "a count of fiducials a side has, judged on the placed board: the router draws none of it; `the_marks_rules_a_process_states_reach_the_check` proves the check keeps it",
        ),
        (
            "fiducial_diameter",
            "a fiducial's size, judged on the placed board: the router draws none of it; `the_marks_rules_a_process_states_reach_the_check` proves the check keeps it",
        ),
        (
            "fiducial_mask_ratio",
            "a fiducial's mask opening, judged on the placed board: the router draws none of it; `the_marks_rules_a_process_states_reach_the_check` proves the check keeps it",
        ),
        (
            "min_fiducial_edge",
            "a fiducial's space to the board edge, judged on the placed board: the router draws none of it; `the_marks_rules_a_process_states_reach_the_check` proves the check keeps it",
        ),
        (
            "min_test_point_pad",
            "a test point's pad, judged on the placed board: the router draws none of it; `the_marks_rules_a_process_states_reach_the_check` proves the check keeps it",
        ),
        (
            "min_test_point_pitch",
            "the space between test points, judged on the placed board: the router draws none of it; `the_marks_rules_a_process_states_reach_the_check` proves the check keeps it",
        ),
        (
            "min_test_point_body",
            "a test point's space to a part, judged on the placed board: the router draws none of it; `the_marks_rules_a_process_states_reach_the_check` proves the check keeps it",
        ),
        (
            "min_test_point_edge",
            "a test point's space to the board edge, judged on the placed board: the router draws none of it; `the_marks_rules_a_process_states_reach_the_check` proves the check keeps it",
        ),
        (
            "min_power_test_point_pad",
            "the pad of a test point on a supply or a ground, judged on the placed board: the router draws none of it; `the_marks_rules_a_process_states_reach_the_check` proves the check keeps it",
        ),
        (
            "tall_part_height",
            "the height above which a part is tall to a test point, judged on the placed board: the router draws none of it; `the_marks_rules_a_process_states_reach_the_check` proves the check keeps it",
        ),
        (
            "min_test_point_tall_body",
            "a test point's space to a tall part, judged on the placed board: the router draws none of it; `the_marks_rules_a_process_states_reach_the_check` proves the check keeps it",
        ),
        (
            "fiducial_round_angle",
            "the angle between the fiducials of a round board, judged on the placed board: the router draws none of it; `the_marks_rules_a_process_states_reach_the_check` proves the check keeps it",
        ),
        (
            "fine_pitch",
            "the pitch of an ultra-fine-pitch part, to the finish advice, judged on the placed board: the router draws none of it; `the_marks_rules_a_process_states_reach_the_check` proves the check keeps it",
        ),
        (
            "min_single_sided_assembly_size",
            "the least single board the assembly house places parts on one face of, judged on the placed board: the router draws none of it; `the_marks_rules_a_process_states_reach_the_check` proves the check keeps it",
        ),
        (
            "min_double_sided_assembly_size",
            "the least single board the assembly house places parts on both faces of, judged on the placed board: the router draws none of it; `the_marks_rules_a_process_states_reach_the_check` proves the check keeps it",
        ),
        (
            "min_round_track_angle",
            "the angle at which a track may leave round copper, unstated by every process and judged on the routed board by `acid_trap` only where a board states it: the router draws none of it; `a_round_track_angle_a_board_states_reaches_the_check` proves the check keeps it",
        ),
        (
            "via_group_gap",
            "the space at which vias of one net stand side by side as one group, unstated by every process (the least clearance then): `a_via_group_gap_a_board_states_reaches_the_router_and_the_check` proves both keep it",
        ),
        (
            "switch_node_keepout",
            "how near foreign copper may come to a switching regulator's switch node, unstated by every process (1 mm then) and judged on the routed board by `board drc`'s `switch_node_copper` note: the router draws none of it; `a_switch_node_keepout_a_board_states_reaches_the_note` proves the note keeps it",
        ),
    ];

    /// The fields declared at the top level of `interface <name>` (or `generic <name>`) in
    /// `std/mfg/jlcpcb.nsm` or `std/mfg/acme.nsm`, with the value its `&=` or `?=` line gives,
    /// if any, or the line of `interface <provider> provides <name>` (ADR 0113).
    fn process_fields(name: &str, provider: Option<&str>) -> Vec<(String, Option<String>)> {
        let (_, text) = std_module(name);
        let block = |header: &str| -> &str {
            let start = text.find(header).expect("the process is in the file");
            let body = &text[start..];
            &body[..body.find("\n}").expect("the definition ends")]
        };
        let body = if text.contains(&format!("generic {name} {{")) {
            block(&format!("generic {name} {{"))
        } else {
            block(&format!("interface {name} {{"))
        };
        let own = provider.map(|p| block(&format!("interface {p} provides {name} {{")));
        let mut fields: Vec<(String, Option<String>)> = Vec::new();
        for line in body.lines().chain(own.into_iter().flat_map(str::lines)) {
            let Some(rest) = line.strip_prefix("    .") else {
                continue;
            };
            let word: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let after = rest[word.len()..].trim_start();
            if after.starts_with(':') {
                fields.push((word, None));
            } else if let Some(value) = after
                .strip_prefix("&=")
                .or_else(|| after.strip_prefix("?="))
                && let Some(f) = fields.iter_mut().find(|f| f.0 == word)
            {
                f.1 = Some(value.trim().to_string());
            }
        }
        fields
    }

    /// The std module that defines the process `name`, and its text.
    fn std_module(name: &str) -> (&'static str, &'static str) {
        if matches!(name, "EightLayer" | "TenLayer") {
            ("acme", include_str!("../../../std/mfg/acme.nsm"))
        } else {
            ("jlcpcb", include_str!("../../../std/mfg/jlcpcb.nsm"))
        }
    }

    /// The process of a board that states each of `narrowings` inside `.process { }`. A name
    /// with a weight, `TwoLayer@2`, is `TwoLayer` at that copper weight (ADR 0113).
    fn process(name: &str, narrowings: &[String]) -> netsmith_core::Process {
        let mut lines: Vec<String> = narrowings.to_vec();
        let name = match name.split_once('@') {
            Some((name, weight)) => {
                lines.push(format!(".stackup.copper_weight &= {weight}"));
                name
            }
            None => name,
        };
        let inner = match name {
            "FourLayer" => 2,
            "SixLayer" => 4,
            "EightLayer" => 6,
            "TenLayer" => 8,
            _ => 0,
        };
        let nets = if inner > 0 {
            lines.push(".inner1.net &= \"GND\"".into());
            lines.push(".inner2.net &= \"3V3\"".into());
            for n in 3..=inner {
                lines.push(format!(".inner{n}.use &= \"signal\""));
            }
            "        .a.net &= \"GND\"\n        .b.net &= \"3V3\"\n"
        } else {
            lines.push(".stackup.thickness &= 1.6mm".into());
            ""
        };
        let body: String = lines.iter().map(|l| format!("        {l}\n")).collect();
        let (module, _) = std_module(name);
        let source = format!(
            "import std::mfg::{module}::{name}\nimport std::parts::passive::Resistor\nboard T {{\n    .process: {name} {{\n{body}    }}\n    .r: Resistor {{\n{nets}    }}\n}}\n"
        );
        netsmith_pipeline::build_with(
            "t.nsm",
            &source,
            &netsmith_check::Loader::std(),
            &netsmith_solve::Catalogs::jlcpcb(),
            &netsmith_pipeline::Libraries::sample(),
        )
        .unwrap_or_else(|e| panic!("{e}"))
        .remove(0)
        .design
        .process
        .expect("the board has a process")
    }

    /// What the router keeps and what the design rules check is given for a process: the
    /// router's rules, and the project file with the extras `fab` passes.
    fn what_is_kept(p: &netsmith_core::Process) -> Result<(String, String), String> {
        let rules = netsmith_pcb::Rules::for_process(p)?;
        let board_extras = netsmith_geom::DrcExtras {
            mask_web: Some(rules.mask_bridge),
            via_in_pad: Some(p.via_in_pad),
            hole_to_track: Some(rules.hole_clearance),
            plated_hole_clearance: Some(rules.plated_hole_clearance),
            pad_to_pad: Some(rules.pad_to_pad),
            pad_hole_to_hole: Some(rules.hole_to_hole),
            mask_to_track: Some(rules.mask_to_track),
            plated_slot: Some(rules.plated_slot),
            npth_slot: Some(rules.npth_slot),
            npth_drill: Some(rules.npth_drill),
            pad_ring: Some(rules.min_annular_ring),
            text_to_mask: Some(rules.silk_to_pad),
            round_track_angle: p.min_round_track_angle,
            via_group_gap: rules.group_gap(),
            marks: Some(netsmith_geom::DrcMarks {
                min_fiducials: p.min_fiducials as usize,
                fiducial_diameter: p.fiducial_diameter,
                fiducial_mask_ratio: p.fiducial_mask_ratio,
                min_fiducial_edge: p.min_fiducial_edge,
                min_test_point_pad: p.min_test_point_pad,
                min_test_point_pitch: p.min_test_point_pitch,
                min_test_point_body: p.min_test_point_body,
                min_test_point_edge: p.min_test_point_edge,
                min_power_test_point_pad: p.min_power_test_point_pad,
                tall_parts: vec![("U1".into(), p.tall_part_height + 1.0)],
                tall_part_height: p.tall_part_height,
                min_test_point_tall_body: p.min_test_point_tall_body,
                round_angle: p.fiducial_round_angle,
                fine_pitch: p.fine_pitch,
                min_single_sided_assembly_size: p.min_single_sided_assembly_size,
                min_double_sided_assembly_size: p.min_double_sided_assembly_size,
                assembly_size_source: p.assembly_size_source.clone(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let check = format!("{}\n{board_extras:?}", rules.kicad_pro("T", &[]));
        // `fiducial_edge` is in `Rules` for the placer, which keeps the fiducials it puts in the
        // corners that far from the edge (issue #761); the router reads none of it, so it is
        // left out of what is compared as the router's rules.
        let router = netsmith_pcb::Rules {
            fiducial_edge: 0.0,
            ..rules
        };
        Ok((format!("{router:?}"), check))
    }

    /// The fiducial and test point rules (issue #761) are judged on the placed board, so they
    /// reach the design rules check and not the router: raising each changes what the check is
    /// given and leaves the router's rules as they were.
    #[test]
    fn the_marks_rules_a_process_states_reach_the_check() {
        let base = what_is_kept(&process("TwoLayer", &[])).unwrap();
        for (field, raised) in [
            ("min_fiducials", "3.."),
            ("fiducial_diameter", "1.2mm"),
            ("fiducial_mask_ratio", "3.."),
            ("min_fiducial_edge", "4mm.."),
            ("min_test_point_pad", "1mm.."),
            ("min_test_point_pitch", "2.54mm.."),
            ("min_test_point_body", "1mm.."),
            ("min_test_point_edge", "3.8mm.."),
            ("min_power_test_point_pad", "1.5mm.."),
            ("tall_part_height", "7mm"),
            ("min_test_point_tall_body", "5mm.."),
            ("fiducial_round_angle", "120"),
            ("fine_pitch", "0.5mm"),
            ("min_single_sided_assembly_size", "20mm.."),
            ("min_double_sided_assembly_size", "100mm.."),
        ] {
            let narrowed = format!(".{field} &= {raised}");
            let after =
                what_is_kept(&process("TwoLayer", std::slice::from_ref(&narrowed))).unwrap();
            assert_eq!(after.0, base.0, "`{narrowed}` changes the router's rules");
            assert_ne!(after.1, base.1, "`{narrowed}` does not reach the check");
        }
    }

    /// The angle a track may leave round copper at (`.process.min_round_track_angle`) is stated
    /// by no process, so a board that states it changes what the check is given and leaves the
    /// router's rules as they were; one that does not changes neither.
    #[test]
    fn a_round_track_angle_a_board_states_reaches_the_check() {
        let base = what_is_kept(&process("TwoLayer", &[])).unwrap();
        assert!(base.1.contains("round_track_angle: None"), "{}", base.1);
        let stated = what_is_kept(&process(
            "TwoLayer",
            &[".min_round_track_angle &= 60".to_string()],
        ))
        .unwrap();
        assert_eq!(stated.0, base.0, "the router's rules changed");
        assert_ne!(stated.1, base.1, "it does not reach the check");
        assert!(
            stated.1.contains("round_track_angle: Some(60.0)"),
            "{}",
            stated.1
        );
    }

    /// The space at which vias of one net stand side by side as one group
    /// (`.process.via_group_gap`, issue #1505) is stated by no process and is the least
    /// clearance then; a board that states it changes both the router's rules and what the
    /// check is given.
    #[test]
    fn a_via_group_gap_a_board_states_reaches_the_router_and_the_check() {
        let p = process("TwoLayer", &[]);
        assert_eq!(p.via_group_gap, None);
        let rules = netsmith_pcb::Rules::for_process(&p).unwrap();
        assert_eq!(rules.group_gap(), p.min_clearance);
        let base = what_is_kept(&p).unwrap();
        let stated = process("TwoLayer", &[".via_group_gap &= 0.15mm".to_string()]);
        assert_eq!(stated.via_group_gap, Some(0.15));
        let after = what_is_kept(&stated).unwrap();
        assert_ne!(after.0, base.0, "it does not reach the router");
        assert_ne!(after.1, base.1, "it does not reach the check");
        assert!(after.1.contains("via_group_gap: 0.15"), "{}", after.1);
    }

    /// `.process.switch_node_keepout` (issue #1585) is stated by no process and is 1 mm then;
    /// a board that states it changes how near foreign copper the `switch_node_copper` note
    /// judges.
    #[test]
    fn a_switch_node_keepout_a_board_states_reaches_the_note() {
        let p = process("TwoLayer", &[]);
        assert_eq!(p.switch_node_keepout, None);
        assert_eq!(
            crate::power_stage::keepout(&p),
            netsmith_fab::power_stage::DEFAULT_SWITCH_NODE_KEEPOUT
        );
        let stated = process("TwoLayer", &[".switch_node_keepout &= 2mm".to_string()]);
        assert_eq!(stated.switch_node_keepout, Some(2.0));
        assert_eq!(crate::power_stage::keepout(&stated), 2.0);
    }

    /// Every rule a process states reaches both the router and the design rules check: a
    /// little more of it changes what each keeps, or makes the router refuse the process (a
    /// rule of the via, which the via the process offers must meet). A field that is not such
    /// a rule is listed with its reason (issue #431).
    #[test]
    fn every_rule_a_process_states_reaches_the_router_and_the_check() {
        // Each provider of a generic process, by the weight that picks it (ADR 0113).
        for (name, provider) in [
            ("TwoLayer", Some("JlcTwoLayer1oz")),
            ("TwoLayer@2", Some("JlcTwoLayer2oz")),
            ("FourLayer", Some("JlcFourLayerA04161H7628")),
            ("SixLayer", Some("JlcSixLayerA06161H3313")),
            ("EightLayer", None),
            ("TenLayer", None),
        ] {
            let base = what_is_kept(&process(name, &[])).unwrap();
            let generic = name.split('@').next().expect("a name");
            for (field, value) in process_fields(generic, provider) {
                if NOT_A_COPPER_RULE.iter().any(|(f, _)| *f == field) {
                    continue;
                }
                let value = value.unwrap_or_else(|| panic!("{name}.{field} states no value"));
                // A lower bound is raised by 0.01 mm, an upper bound lowered, a value moved.
                let number: f64 = value
                    .trim_start_matches("0mm..")
                    .split("mm")
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or_else(|| panic!("{name}.{field}: cannot read {value}"));
                let narrowed = if value.ends_with("..") {
                    format!(".{field} &= {}mm..", number + 0.01)
                } else if value.starts_with("0mm..") {
                    format!(".{field} &= 0mm")
                } else {
                    format!(".{field} &= {}mm", number + 0.05)
                };
                let after = what_is_kept(&process(name, std::slice::from_ref(&narrowed)));
                match after {
                    Err(_) => {}
                    Ok((router, check)) => {
                        assert_ne!(
                            router, base.0,
                            "{name}: `{narrowed}` does not change the router's rules"
                        );
                        assert_ne!(
                            check, base.1,
                            "{name}: `{narrowed}` does not reach the design rules check"
                        );
                    }
                }
            }
        }
    }
}
