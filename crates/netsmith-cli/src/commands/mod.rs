//! The command line, as clap's derive reads it (#514). Each command is a struct of its own, and
//! its doc comments are its help: the first paragraph of a command's or a flag's comment is what
//! `-h` and `netsmith --help` show, the whole comment what `<command> --help` shows. A usage
//! mistake is clap's error, with exit status 2, netsmith's "cannot run".

use std::ffi::OsString;
use std::path::PathBuf;

use clap::error::ErrorKind;
use clap::{ArgGroup, Args, CommandFactory, Parser, Subcommand, ValueEnum};

use crate::findings::{Format, NotesMode};

/// The port `netsmith editor` listens on unless told otherwise.
pub const DEFAULT_PORT: u16 = 8787;

/// netsmith: electronics as code.
#[derive(Debug, Parser)]
#[command(
    name = "netsmith",
    version,
    about = "electronics as code",
    arg_required_else_help = true,
    max_term_width = 100,
    after_help = "`netsmith <command> --help` says what the flags of a command do.",
    after_long_help = "`netsmith <command> --help` says what the flags of a command do.\n\n\
        `NETSMITH_TIMINGS=1` in the environment of any command prints one line for each stage \
        of the run to stderr (`timing: solve LedExample 12.3 ms`); nothing is printed without it."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum Command {
    Init(InitArgs),
    Build(BuildArgs),
    Check(CheckArgs),
    Fab(FabArgs),
    Route(RouteArgs),
    Lock(LockArgs),
    Editor(EditorArgs),
    #[command(name = "board3d")]
    Board3d(Board3dArgs),
    Render(RenderArgs),
    View(ViewArgs),
    Compare(CompareArgs),
    Parity(ParityArgs),
    /// Print the [[library]] stanzas for KiCad's library tables.
    #[command(name = "kicad-libs", subcommand)]
    KicadLibs(KicadLibsCommand),
    /// Check catalog rows against LCSC's record (`refresh`), list the old ones (`age`) or add a
    /// row from it (`add`).
    #[command(subcommand)]
    Catalog(CatalogCommand),
    /// Print an enclosure's outline, cutouts and holes as source to paste (`import`).
    #[command(subcommand)]
    Dxf(DxfCommand),
    /// Print the version (also --version and -V).
    Version,
}

/// Scaffold a project in the current directory.
///
/// `init` writes `netsmith.toml`, `src/main.nsm` (a working LED board) and the agent files.
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct InitArgs {
    /// The project's name; the default is the directory's.
    #[arg(long)]
    pub name: Option<String>,
}

/// Check, solve and write a KiCad schematic of the boards.
///
/// With no file, `build` acts on the project's boards (see netsmith.toml).
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct BuildArgs {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The folder to write into; the default is the target's.
    #[arg(long, value_name = "DIR")]
    pub out: Option<PathBuf>,
    /// Make a guideline finding an error for this run.
    ///
    /// Like `guidelines = "error"` in netsmith.toml (ADR 0036); without it a finding against a
    /// guideline is a warning.
    #[arg(long)]
    pub strict: bool,
    /// Neither the pick lock nor the pin file may change.
    ///
    /// The run ends with an error, before anything is written, when the parts it picks differ
    /// from the pick lock beside the source (`<file>.lock.toml`) or when it does not exist, when
    /// the pins that its requests get differ from the pin file beside it (`<file>.pins.toml`,
    /// ADR 0101), or when the picks file beside it (`<file>.picks.toml`, #462) names a part that
    /// is gone (see _The pick lock_ in docs/LANGUAGE.md).
    #[arg(long)]
    pub locked: bool,
    /// Text for a person, or one JSON document on stdout.
    ///
    /// `json` prints one JSON document on stdout and nothing else there. It holds `command`,
    /// `ok`, the `boards` with the folders written, the `findings`, and the count of the `notes`
    /// for each cause. Each finding has a `severity` (`error`, `warning` or `note`), a `code`,
    /// and the `board`, the member `path`, the `file`, `line` and `column` and the `message` it
    /// has. The code is the stage that found an error or a warning (`syntax`, `check`, `solve`,
    /// `design`, `guideline`, `power`, `lock`, `place`, `fab`), or `note.` and the cause of a
    /// note (`note.not_stated`, `note.part_model`, `note.unjudged`, `note.circular`,
    /// `note.supply`, `note.pick`, `note.track`, `note.other`): there are no codes for each rule.
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
    /// Each note, a count for each cause, or none.
    ///
    /// `summary` prints a board's notes as one line with a count for each cause
    /// (`note: TwoBuck: 7 notes: 6 not_stated, 1 unjudged`), `none` leaves them out, and `all`,
    /// the default, prints each. With `--format json` the notes are findings only with `all`;
    /// their counts are always there.
    #[arg(long, value_enum, default_value = "all")]
    pub notes: NotesMode,
}

/// Check and solve the boards, write nothing, print the findings only.
///
/// `check` checks and solves the boards as `build` does and writes nothing: no schematic, no
/// pick lock, no pin file. It prints the findings only (errors, warnings and notes, as `--notes`
/// says), not the bill of materials. With no file it acts on the project's boards.
///
/// The exit code is 0 when every board builds, 1 when one does not, and 2 when the command
/// cannot run (a file that cannot be read, no boards).
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct CheckArgs {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// Make a guideline finding an error for this run, as with `build`.
    #[arg(long)]
    pub strict: bool,
    /// Text for a person, or one JSON document on stdout, as with `build`.
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
    /// Each note, a count for each cause, or none, as with `build`.
    #[arg(long, value_enum, default_value = "all")]
    pub notes: NotesMode,
}

/// Make the fab package (Gerbers, drill, bom.csv, cpl.csv, fab-notes.txt, the 3D scene as .glb)
/// or an order.
///
/// With no file, `fab` acts on the project's boards (see netsmith.toml).
///
/// `fab` writes the Gerbers, drill files and placement file itself and runs netsmith's own
/// design rules check (ADR 0037), so it does not need KiCad. An error of the check (a
/// clearance, a short, a missing connection, a hole, the edge, a courtyard, a keepout) stops
/// `fab` unless `--allow-drc-violations` is given, and `fab-notes.txt` then says the board is
/// not clean; its warnings (silkscreen on a pad, a narrow mask web, a via in a pad) are listed
/// in the notes. It writes, for each board, a Gerber zip, `bom.csv`, `cpl.csv`,
/// `fab-notes.txt`, the `.kicad_pcb` and `.kicad_pro`, an `.svg`, and the board's 3D scene as
/// `<board>.glb` (outside the Gerber zip; the scene of the routed board, the bytes
/// `board3d --glb` writes for it; docs/BOARD3D.md). Read `fab-notes.txt` before you order.
///
/// A board the router cannot finish is still written, marked INCOMPLETE, and the run ends in an
/// error. Beside it, `routing-failure.txt` lists each connection left with its net, its two pads
/// and their places and the parts most of them end on, and then, for each line the router left,
/// the stage it last failed in (the first pass, the try with less room, ripping up, the
/// negotiation, the chain of rip-ups, or out of time) and the nets whose copper stands in its
/// way; `routing-report.json` has the same for a program, with a crowding map of the board
/// (1 mm squares, how often the router's searches across other nets' copper passed through
/// each), which shows where the board is fought over.
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct FabArgs {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The folder to write into; the default is the target's.
    #[arg(long, value_name = "DIR")]
    pub out: Option<PathBuf>,
    /// Seconds the router may take for a board (0 for no limit).
    ///
    /// When the time is spent, `fab` stops and lists the connections left. The default is 300.
    #[arg(long, value_name = "SECS")]
    pub route_timeout: Option<u64>,
    /// Package a board that fails the design rules check.
    ///
    /// `fab-notes.txt` then says the board is not clean. It never waives a `[constraint]`
    /// finding: a requirement on where a part goes is relaxed only by `constraints = "warn"` in
    /// netsmith.toml or `--constraints warn`.
    #[arg(long)]
    pub allow_drc_violations: bool,
    /// Compare the files and the findings with KiCad's own plot and check of the board.
    ///
    /// It needs KiCad's command-line tool: `$KICAD_CLI`, then `kicad-cli` on `PATH`, then
    /// Windows KiCad 9 under WSL.
    #[arg(long)]
    pub cross_check: bool,
    /// Make a guideline finding an error for this run, as with `build`.
    #[arg(long)]
    pub strict: bool,
    /// Neither the pick lock nor the pin file may change, as with `build`.
    ///
    /// With `fab` it also means `--pins locked`, so a board that does not route never has its
    /// pin file rewritten (`--locked` wins over `--pins auto`). With `--order <name>` it applies
    /// to each source file that builds a board of the order.
    #[arg(long)]
    pub locked: bool,
    /// Whether a requirement on where a part goes that is not met stops `fab` (ADR 0045).
    ///
    /// It wins over `constraints` in netsmith.toml for this run.
    #[arg(long, value_enum)]
    pub constraints: Option<Constraints>,
    /// What a board that does not route may do with the pins its requests got (ADR 0101).
    ///
    /// `pins` in netsmith.toml sets it for a project; the default is `auto`.
    #[arg(long, value_enum)]
    pub pins: Option<Pins>,
    /// Leave the board's 3D scene (`<board>.glb`) out of the package for this run.
    ///
    /// `glb = false` in netsmith.toml does it for a project; a stale `<board>.glb` of an
    /// earlier run is removed.
    #[arg(long)]
    pub no_glb: bool,
    /// Also write `<board>.placed.json`, the board as placed before it is routed, for `route`.
    ///
    /// The file goes into the board's output folder, with the track margin `fab` routes with.
    /// Nothing else of the package changes, and a run without it removes the file an earlier run
    /// saved. `--order` does not take it.
    #[arg(long, conflicts_with = "order")]
    pub save_placed: bool,
    /// Make one of the project's orders; with no name, list them (ADR 0035).
    ///
    /// An order names its boards in netsmith.toml, so `--order` takes no file. It prints text
    /// only.
    #[arg(long, value_name = "NAME", num_args = 0..=1, conflicts_with = "file")]
    pub order: Option<Option<String>>,
    /// Print the result of each board and the findings, not the stages and the router's passes.
    #[arg(long)]
    pub quiet: bool,
    /// Text for a person, or one JSON document on stdout, as with `build`.
    ///
    /// With `json` the progress goes nowhere, and the notes of `fab-notes.txt` ("before you
    /// order") are findings with the code `fab`.
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
    /// Each note, a count for each cause, or none, as with `build`.
    #[arg(long, value_enum, default_value = "all")]
    pub notes: NotesMode,
}

impl FabArgs {
    /// `Some(true)` lists an unmet requirement on where a part goes instead of stopping the run,
    /// `Some(false)` stops even if the manifest says `warn`, `None` leaves it to the manifest.
    pub fn constraints_warn(&self) -> Option<bool> {
        self.constraints.map(|c| c == Constraints::Warn)
    }

    /// `Some(true)` leaves every pin and writes the proposal, `Some(false)` lets a board that does
    /// not route have its unlocked pins swapped, `None` leaves it to the manifest.
    pub fn pins_locked(&self) -> Option<bool> {
        self.pins.map(|p| p == Pins::Locked)
    }

    /// A named order to make, or the list of them, instead of every board.
    pub fn order(&self) -> Option<OrderRequest> {
        self.order.as_ref().map(|name| match name {
            Some(name) => OrderRequest::Make(name.clone()),
            None => OrderRequest::List,
        })
    }
}

/// `fab --constraints`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Constraints {
    /// An unmet requirement is listed instead of stopping `fab`.
    Warn,
    /// It stops `fab`, whatever netsmith.toml says.
    Error,
}

/// `fab --pins`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Pins {
    /// Swap interchangeable pins that are not locked, place and route again (three rounds at
    /// most) and write the pin file.
    Auto,
    /// Move none, write the swaps that would help to `pins.proposed.toml` in the output folder,
    /// and fail as before.
    Locked,
}

/// What `netsmith fab --order` asks for (ADR 0035).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderRequest {
    /// `--order` with no name: list the project's orders.
    List,
    /// `--order <name>`: make that order.
    Make(String),
}

/// Route a placed board that `fab --save-placed` wrote, alone (no checks; a router tool).
///
/// `route` routes the board that `fab --save-placed` saved again, alone, the way `fab` does
/// (signals at the track margin first where there is room, then at the process's least track),
/// with no checking, solving or placing, so an experiment on the router (a change to its code, a
/// time limit, a track margin) takes the time of the routing and nothing else. It writes
/// `<board>.kicad_pcb`, `.kicad_pro`, `.svg` and, when connections are left,
/// `routing-failure.txt` and `routing-report.json` into `--out`, as `fab` does; it prints the
/// connections left, the tracks, the vias and the time. It is a router tool and not a fab
/// package: it runs no design rules check and writes no Gerbers.
///
/// The exit code is 0 when every connection is routed, 1 when connections are left, and 2 when
/// the file cannot be read: the file's layout changes with netsmith, so a file saved by another
/// version is refused and is saved again with `fab --save-placed`.
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct RouteArgs {
    /// The placed board that `fab --save-placed` wrote.
    #[arg(value_name = "BOARD.PLACED.JSON")]
    pub file: PathBuf,
    /// The folder to write into; the default is `out/`.
    #[arg(long, value_name = "DIR")]
    pub out: Option<PathBuf>,
    /// Seconds the router may take (0 for no limit), as with `fab`.
    #[arg(long, value_name = "SECS")]
    pub route_timeout: Option<u64>,
    /// The track margin, from 1 (the process's least track) to 4.
    ///
    /// It replaces the margin the file was saved with: how many times the least track a signal
    /// is drawn at where there is room.
    #[arg(long, value_name = "X", value_parser = parse_margin)]
    pub margin: Option<f64>,
}

fn parse_margin(given: &str) -> Result<f64, String> {
    given
        .parse::<f64>()
        .ok()
        .filter(|m| (1.0..=4.0).contains(m))
        .ok_or_else(|| {
            format!("takes a number from 1 (the process's least track) to 4, not `{given}`")
        })
}

/// Refresh the pick lock (`--update`) or say whether it is current (`--check`).
///
/// `lock --update` solves each board without the pick lock's holding (the picks file still
/// holds its parts) and writes the lock with the rows the solver picks now, printing for each
/// part that changed the old row and the new one (nothing when nothing changed). `lock --check`
/// does the same without writing, prints the same and exits 1 when the lock is not current or
/// does not exist, or when the picks file names a part that is gone (for CI). Both exit 2 when
/// they cannot tell: the lock or the picks file is not readable, or the design does not build.
/// With no file it acts on the project's boards.
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
#[command(group(ArgGroup::new("mode").required(true).args(["update", "check"])))]
pub struct LockArgs {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// Write the rows the solver picks now.
    #[arg(long)]
    pub update: bool,
    /// Write nothing; fail when the lock is not current.
    #[arg(long)]
    pub check: bool,
}

impl LockArgs {
    pub fn mode(&self) -> LockMode {
        if self.update {
            LockMode::Update
        } else {
            LockMode::Check
        }
    }
}

/// What `netsmith lock` does with the pick lock (ADR 0100).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    /// `--update`: write the rows the solver picks now.
    Update,
    /// `--check`: write nothing; fail when the lock is not current.
    Check,
}

/// The source and the board in a browser.
///
/// With no file, `editor` acts on the project's boards (see netsmith.toml). It serves on
/// 127.0.0.1 and prints the address to open.
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct EditorArgs {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The port to listen on.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,
}

/// The board's 3D scene: heights, limits and overlaps (`--json` for agents).
///
/// `board3d` places a board as `fab` does, with the layout file and the physical file, and
/// prints its 3D scene (ADR 0105) without routing or writing anything but the `--glb` and
/// `--step` files, and without KiCad. With neither `--json`, `--glb` nor `--step` it prints a
/// short summary. A file of several boards needs `--board`. docs/BOARD3D.md has the frame and
/// every field.
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct Board3dArgs {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The board, when the file has several (`Name`, or `System/member`).
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// Print the report an agent reads.
    ///
    /// Each part's body box, its Z range and where its height came from (`stated`, `catalog`
    /// or `unknown`, never a guessed height), each height limit with the tallest part under it
    /// and the margin, the parts too tall, bodies that overlap in 3D, the parts of unknown height
    /// with the reason, and the height coverage. With `--glb` as well it does both.
    #[arg(long)]
    pub json: bool,
    /// Write the scene as glTF 2.0 binary, in place of the summary.
    ///
    /// It is the file the editor's 3D view shows (docs/BOARD3D.md).
    #[arg(long, value_name = "FILE.GLB")]
    pub glb: Option<PathBuf>,
    /// Write the board and its parts as a STEP file (AP214), in place of the summary.
    ///
    /// For mechanical CAD: the board through its thickness with its cutouts and holes, each part
    /// a product named by its designator, as exact solids; a part of unknown height is its
    /// outline only, never a guessed solid (ADR 0107; docs/BOARD3D.md).
    #[arg(long, value_name = "FILE.STEP")]
    pub step: Option<PathBuf>,
}

/// Draw the board in 3D to a PNG (own rasterizer, no GPU, no KiCad).
///
/// `render` places a board as `board3d` does and draws it to a PNG with netsmith's own
/// software rasterizer (ADR 0105): the same source and options give the same bytes, with no
/// GPU, no browser and no KiCad. docs/BOARD3D.md, "Pictures for agents", has the frame and what
/// is drawn.
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct RenderArgs {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The board, when the file has several (`Name`, or `System/member`).
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// Where the camera looks from.
    #[arg(long, value_enum, default_value = "iso")]
    pub view: ViewName,
    /// The PNG to write; the default is `<board>-<view>.png` in the current folder.
    ///
    /// The path written is printed.
    #[arg(long, value_name = "FILE.PNG")]
    pub out: Option<PathBuf>,
    /// The picture's size in pixels, 64 to 8192 a side, at most 16 million pixels.
    #[arg(long, value_name = "WxH", default_value = "1600x1000", value_parser = crate::render::parse_size)]
    pub size: (usize, usize),
    /// Cut the scene at a position in mm, such as `x=12.5`, and keep the side away from the camera.
    ///
    /// The position is in the scene's frame: y up from the outline's bottom-left corner, z up
    /// from the board's top face. The cut faces are filled as a section of the stackup, each
    /// layer at its true thickness in its own colour, unless `--no-caps` leaves them open.
    #[arg(long, value_name = "AXIS=MM", value_parser = crate::render::parse_section)]
    pub section: Option<(netsmith_pcb::mesh::Axis, f64)>,
    /// Light a part, `net:<name>` or `limit:<name>`, and dim the rest (comma-separated or repeated).
    ///
    /// A part is named by its designator or its path. An id the board does not have is an error
    /// that lists the ones it has.
    #[arg(long, value_name = "ID[,ID...]", value_parser = parse_ids)]
    pub highlight: Vec<Ids>,
    /// Draw everything see-through.
    #[arg(long)]
    pub xray: bool,
    /// Leave the cut faces of a section open.
    #[arg(long)]
    pub no_caps: bool,
    /// The picture's colours.
    #[arg(long, value_enum, default_value = "light")]
    pub theme: Theme,
    /// The parallel projection (the default).
    #[arg(long, conflicts_with = "perspective")]
    pub ortho: bool,
    /// The perspective projection.
    #[arg(long)]
    pub perspective: bool,
}

impl RenderArgs {
    pub fn request(&self) -> crate::render::Request {
        crate::render::Request {
            view: self.view.raster(),
            size: self.size,
            section: self.section,
            highlight: self
                .highlight
                .iter()
                .flat_map(|ids| ids.0.clone())
                .collect(),
            xray: self.xray,
            caps: !self.no_caps,
            dark: self.theme == Theme::Dark,
            perspective: self.perspective,
        }
    }
}

/// `--view` of `render` and `view`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ViewName {
    /// Straight at the top face.
    Top,
    /// Straight at the bottom face.
    Bottom,
    /// Along the board's front edge.
    Front,
    /// Along the board's back edge.
    Back,
    /// Along the board's left edge.
    Left,
    /// Along the board's right edge.
    Right,
    /// From a corner, above.
    Iso,
    /// From a corner, below.
    IsoBottom,
}

impl ViewName {
    fn raster(self) -> netsmith_pcb::raster3d::View {
        use netsmith_pcb::raster3d::View;
        match self {
            ViewName::Top => View::Top,
            ViewName::Bottom => View::Bottom,
            ViewName::Front => View::Front,
            ViewName::Back => View::Back,
            ViewName::Left => View::Left,
            ViewName::Right => View::Right,
            ViewName::Iso => View::Iso,
            ViewName::IsoBottom => View::IsoBottom,
        }
    }

    fn editor(self) -> netsmith_editor::api::RenderView {
        use netsmith_editor::api::RenderView;
        match self {
            ViewName::Top => RenderView::Top,
            ViewName::Bottom => RenderView::Bottom,
            ViewName::Front => RenderView::Front,
            ViewName::Back => RenderView::Back,
            ViewName::Left => RenderView::Left,
            ViewName::Right => RenderView::Right,
            ViewName::Iso => RenderView::Iso,
            ViewName::IsoBottom => RenderView::IsoBottom,
        }
    }
}

/// `render --theme`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Theme {
    Light,
    Dark,
}

/// The ids of one `--highlight` or `--select`, split at commas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ids(pub Vec<String>);

fn parse_ids(given: &str) -> Result<Ids, String> {
    let ids: Vec<String> = given
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();
    if ids.len() > 1 && ids.iter().any(|id| id == "none") {
        return Err("`none` clears it, so no id goes with it".to_string());
    }
    Ok(Ids(ids))
}

fn parse_select(given: &str) -> Result<Ids, String> {
    let ids = parse_ids(given)?;
    if ids.0.iter().any(|id| id.starts_with("limit:")) {
        return Err(
            "takes parts or one net, as a click does; a `limit:` is lit with --highlight"
                .to_string(),
        );
    }
    Ok(ids)
}

/// The ids of every `--highlight` (or `--select`): `None` when none is given, an empty list for
/// `none`, which no id may join.
fn joined_ids(flag: &str, given: &[Ids]) -> Result<Option<Vec<String>>, String> {
    let all: Vec<String> = given.iter().flat_map(|ids| ids.0.clone()).collect();
    if all.iter().any(|id| id == "none") {
        if all.iter().any(|id| id != "none") {
            return Err(format!("{flag} `none` clears it, so no id goes with it"));
        }
        return Ok(Some(Vec::new()));
    }
    Ok(if all.is_empty() { None } else { Some(all) })
}

/// Drive the 3D view of a running editor, as `render` flags say, and save its picture.
///
/// `view` drives the 3D view of a running editor (start `netsmith editor` and open its page;
/// docs/BOARD3D.md, "Driving a running editor") with the names of `render`, so that an agent
/// changes the verb to switch between the two. With no flag that changes the page it prints the
/// state of the page (a short summary, or with `--json` the editor's JSON as it came); with any,
/// it changes the page, which draws it, and prints the state that results. The page driven is
/// the one focused last.
///
/// A change the page refuses (an id the board does not have, a section, a preview height) exits
/// 2 with the editor's message, as does an editor that is not running or has no page open.
/// There is no `--theme`: the theme is the person's own setting for the whole editor (the state
/// reports it). Nothing changes the design or a file.
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct ViewArgs {
    /// The editor's port.
    #[arg(long, default_value_t = DEFAULT_PORT, value_parser = clap::value_parser!(u16).range(1..))]
    pub port: u16,
    /// The board (`Name`, or `System/member`).
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// Where the camera looks from, as with `render`.
    #[arg(long, value_enum)]
    pub view: Option<ViewName>,
    /// Cut the scene as `render --section` does, or `none` for no cut.
    #[arg(long, value_name = "AXIS=MM|none", value_parser = parse_view_section)]
    pub section: Option<String>,
    /// Light a part, `net:<name>` or `limit:<name>` (comma-separated or repeated); `none` lights nothing.
    #[arg(long, value_name = "ID[,ID...]|none", value_parser = parse_ids, conflicts_with = "select")]
    pub highlight: Vec<Ids>,
    /// Select parts or one `net:<name>`, as a click does (comma-separated or repeated); `none` clears it.
    ///
    /// The editor's selection holds parts or one net, so `--select` takes no limit.
    #[arg(long, value_name = "ID[,ID...]|none", value_parser = parse_select)]
    pub select: Vec<Ids>,
    /// Draw everything see-through.
    #[arg(long, overrides_with = "no_xray")]
    pub xray: bool,
    /// Draw the bodies solid.
    #[arg(long, overrides_with = "xray")]
    pub no_xray: bool,
    /// The parallel projection.
    #[arg(long, conflicts_with = "perspective")]
    pub ortho: bool,
    /// The perspective projection.
    #[arg(long)]
    pub perspective: bool,
    /// What the bodies are coloured by.
    #[arg(long = "colour-by", alias = "color-by", value_enum)]
    pub colour_by: Option<ColourByName>,
    /// The height preview: a height in mm from 0.1 to 50, `on` or `off`.
    #[arg(long, value_name = "MM|on|off", value_parser = parse_height_preview)]
    pub height_preview: Option<netsmith_editor::api::PreviewSetting>,
    /// Pull the stackup apart.
    #[arg(long, overrides_with = "no_layers_apart")]
    pub layers_apart: bool,
    /// Put the stackup together again.
    #[arg(long, overrides_with = "layers_apart")]
    pub no_layers_apart: bool,
    /// Print the editor's JSON as it came, not the summary.
    #[arg(long)]
    pub json: bool,
    /// Save what the page's canvas shows after the change.
    ///
    /// It opens the 3D view if it is not on screen.
    #[arg(long, value_name = "FILE.PNG")]
    pub picture: Option<PathBuf>,
    /// Refused with the reason: the theme is the person's own setting.
    #[arg(long, hide = true)]
    pub theme: Option<String>,
    /// Refused with the reason: `view` saves with `--picture`.
    #[arg(long, hide = true)]
    pub out: Option<String>,
    /// Refused with the reason: `view` drives the editor's open project.
    #[arg(hide = true)]
    pub file: Option<String>,
}

impl ViewArgs {
    /// The mistakes clap cannot say: an id with `none`, and the flags of `render` that `view`
    /// has not.
    fn check(&self) -> Result<(), String> {
        joined_ids("--highlight", &self.highlight)?;
        joined_ids("--select", &self.select)?;
        if self.theme.is_some() {
            return Err("`view` has no --theme: the theme is the person's own setting for the whole editor, not a view of the board (the state says which it is); `render --theme light|dark` draws a picture in either".to_string());
        }
        if self.out.is_some() {
            return Err(
                "`view` saves the page's picture with --picture <file.png>, not --out".to_string(),
            );
        }
        if let Some(file) = &self.file {
            return Err(format!(
                "`view` takes no file: it drives the editor's open project (`--board` says which board), not a source file; `netsmith render {file}` draws one without an editor"
            ));
        }
        Ok(())
    }

    /// The request to the editor. [`parse_args`] has made the same checks.
    pub fn request(&self) -> Result<crate::view_client::Request, String> {
        self.check()?;
        let switch = |on: bool, off: bool| (on || off).then_some(on);
        let change = netsmith_editor::api::ViewChange {
            board: self.board.clone(),
            view: self.view.map(ViewName::editor),
            section: self.section.clone(),
            highlight: joined_ids("--highlight", &self.highlight)?,
            select: joined_ids("--select", &self.select)?,
            xray: switch(self.xray, self.no_xray),
            ortho: switch(self.ortho, self.perspective),
            colour_by: self.colour_by.map(ColourByName::editor),
            height_preview: self.height_preview,
            exploded: switch(self.layers_apart, self.no_layers_apart),
            ..Default::default()
        };
        Ok(crate::view_client::Request {
            port: self.port,
            change,
            json: self.json,
            picture: self.picture.clone(),
        })
    }
}

fn parse_view_section(given: &str) -> Result<String, String> {
    if given.trim() == "none" {
        return Ok("none".to_string());
    }
    crate::render::parse_section(given)?;
    Ok(given.trim().to_string())
}

fn parse_height_preview(given: &str) -> Result<netsmith_editor::api::PreviewSetting, String> {
    use netsmith_editor::api::PreviewSetting;
    match given {
        "on" => Ok(PreviewSetting::On(true)),
        "off" => Ok(PreviewSetting::On(false)),
        mm => match mm.parse::<f64>() {
            Ok(v) if (0.1..=50.0).contains(&v) => Ok(PreviewSetting::Mm(v)),
            _ => Err(format!(
                "takes a height in mm from 0.1 to 50, `on` or `off`, not `{given}`"
            )),
        },
    }
}

/// `view --colour-by`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColourByName {
    /// Where each height came from.
    Provenance,
    /// The part's kind.
    Kind,
    /// The margin to the height limit over it.
    Margin,
}

impl ColourByName {
    fn editor(self) -> netsmith_editor::api::ColourBy {
        use netsmith_editor::api::ColourBy;
        match self {
            ColourByName::Provenance => ColourBy::Provenance,
            ColourByName::Kind => ColourBy::Kind,
            ColourByName::Margin => ColourBy::Margin,
        }
    }
}

/// Are two designs the same circuit?
///
/// `compare` checks a design against another by connectivity (ADR 0017), for example a port
/// against the KiCad design it came from. Each of `<A>` and `<B>` is a `.kicad_sch`, a
/// `.kicad_pcb`, a KiCad netlist (the XML of `kicad-cli sch export netlist --format kicadxml`)
/// or a `.nsm` file, which is built as `build` does. Net names and, by default, designators may
/// differ between the two; components are matched by value, footprint (without its library) and
/// how their pins are wired. It prints `match` with the designators and net names that
/// correspond, or each difference: a component with no counterpart, a pair that differs in
/// value or footprint, a net whose pins differ (each pin in its own side's designators), a net
/// with no counterpart.
///
/// Power ports and flags (`#PWR`, `#FLG`) are not parts. A pin joined to nothing is on no net,
/// so it differs only from a pin that is joined to something. Reading a `.kicad_sch` needs
/// KiCad's command-line tool, found as for `fab --cross-check`.
///
/// The exit code is 0 for a match, 1 for a difference and 2 for a mistake or a comparison that
/// could not be decided (many interchangeable parts and a search past its bound).
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct CompareArgs {
    /// The first design.
    pub a: String,
    /// The second design.
    pub b: String,
    /// The board of a `.nsm` file that has several (`Name`, or `System/member`).
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// A file of `A_ref B_ref` lines that fix pairs of designators.
    ///
    /// The rest are matched by wiring. Blank lines and `#` comments are ignored.
    #[arg(long, value_name = "FILE", conflicts_with = "by_refdes")]
    pub map: Option<PathBuf>,
    /// Designators are identity: a component matches only the one with the same designator.
    #[arg(long)]
    pub by_refdes: bool,
    /// Leave out nets of a single pin on both sides.
    #[arg(long)]
    pub ignore_unconnected: bool,
    /// Values that read as the same number agree (`4k7` and `4.7k`, `100n` and `0.1uF`).
    #[arg(long)]
    pub loose_values: bool,
    /// Text for a person, or one JSON document on stdout.
    ///
    /// `json` holds the `verdict` and the pairs of a match or each difference.
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
}

/// Compare a fab board with the board of a port's original, part by part.
///
/// `parity` compares a board that `fab` wrote with the original of a port (see the skill
/// `netsmith-port-a-kicad-board`), part by part: the place, the rotation, the face and the pads
/// of each part the table names.
///
/// The exit code is 1 when a part the table marks `must_match` fails, 0 when none does and 2
/// when a file cannot be read.
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct ParityArgs {
    /// The board `fab` wrote.
    #[arg(value_name = "BOARD.KICAD_PCB")]
    pub board: PathBuf,
    /// The table of the original's parts; the default is `reference/placements.toml`.
    #[arg(long, value_name = "PLACEMENTS.TOML")]
    pub table: Option<PathBuf>,
    /// The original's designators to the fab board's; the default is `reference/designators.toml`.
    #[arg(long, value_name = "DESIGNATORS.TOML")]
    pub map: Option<PathBuf>,
    /// Write an overlay of the two boards.
    #[arg(long, value_name = "OVERLAY.SVG")]
    pub svg: Option<PathBuf>,
    /// Text for a person, or one JSON document on stdout.
    ///
    /// `json` holds each item with its figures.
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum KicadLibsCommand {
    Import(KicadLibsImportArgs),
}

/// Print the [[library]] stanzas for kicad.toml, from a KiCad project's library tables (ADR 0021).
#[derive(Debug, PartialEq, Args)]
pub struct KicadLibsImportArgs {
    /// The project's symbol library table.
    #[arg(value_name = "SYM-LIB-TABLE")]
    pub sym_lib_table: String,
    /// The project's footprint library table.
    #[arg(value_name = "FP-LIB-TABLE")]
    pub fp_lib_table: Option<String>,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum CatalogCommand {
    Refresh(RefreshRequest),
    Add(CatalogAddArgs),
    Age(CatalogAgeArgs),
}

/// Check the rows of catalogs against LCSC's record and record the day in `checked` (ADR 0039).
///
/// `catalog refresh` is the one command that uses the network, and it is opt-in (ADR 0039):
/// nothing else, and never the solver, asks anyone anything. JLCPCB data is not fetched: the
/// command asks LCSC only. For each row with an `lcsc` number it reads LCSC's record of that part
/// and checks the manufacturer's number and the package against the row. A row that agrees is
/// `VERIFIED`, and if the file has a `checked` column it gets the day there: that is the only
/// cell the command ever writes. It never writes `price`, `stock`, `price_breaks`, `library` or
/// `date`: the board is assembled from JLCPCB's parts library, those cells are JLCPCB's numbers
/// entered by hand, and LCSC's price (often at a first break above one piece) and stock differ
/// from them. It may say, as a note and not a finding, that LCSC lists stock 0 where the row has
/// some (or the reverse), so you read JLCPCB's parts page by hand; the age of a row's own numbers
/// is still its `date`, so a refresh never makes a stale row fresh. It keeps the order, quoting
/// and line endings of the file, so the result is a small diff.
///
/// With no path it acts on the project's catalog files (the `*.csv` of its `catalog_dirs`
/// folders and the files under `catalogs`) and, in the netsmith repository, on
/// `data/jlcpcb/catalog/`; a path is a catalog file or a directory of them (the standard
/// catalogs inside the binary cannot be written, so they are checked in the repository and
/// arrive with a release). A row is reported, not changed, when the records disagree with it:
/// `CONFLICT` if the manufacturer's number or the package is not the row's, `MISSING` if LCSC no
/// longer lists the number (a row is never deleted); `SKIPPED` is a row `--offline` has no kept
/// answer for.
///
/// LCSC's endpoint is unofficial and LCSC's terms of use have not been read: a person must read
/// them before the command is advertised, and official APIs (JLCAPI, the LCSC API; they need an
/// account) are future work. Meanwhile the command is polite: one request at a time, `--delay`
/// apart, a User-Agent that says what it is, answers kept for `--cache-ttl` and retried with a
/// growing wait.
///
/// The exit code is 0, 1 if any row is `CONFLICT` or `MISSING`, and 2 when the run had to stop
/// (the endpoint refused, or answered in another shape: nothing is written then).
#[derive(Debug, Clone, PartialEq, Eq, Default, Args)]
#[command(args_override_self = true)]
pub struct RefreshRequest {
    /// Catalog files or directories of them; none is the project's catalogs and, in the
    /// netsmith repository, the standard ones.
    #[arg(value_name = "CATALOG.CSV|DIR")]
    pub paths: Vec<PathBuf>,
    /// Print the diff, cell by cell, and write nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Ask nothing, and use the kept answers of any age.
    #[arg(long, conflicts_with_all = ["delay_ms", "cache_ttl_hours"])]
    pub offline: bool,
    /// Only this catalog (repeatable).
    #[arg(long, value_name = "CATALOG")]
    pub only: Vec<String>,
    /// Only the row of this LCSC number (repeatable).
    #[arg(long, value_name = "C123")]
    pub lcsc: Vec<String>,
    /// The pause between requests, in milliseconds (default 500).
    #[arg(long = "delay", value_name = "MS")]
    pub delay_ms: Option<u64>,
    /// How long a kept answer is used, in hours (default 24).
    #[arg(long = "cache-ttl", value_name = "HOURS")]
    pub cache_ttl_hours: Option<u64>,
    /// Keep no answer.
    #[arg(long)]
    pub no_cache: bool,
}

/// Append a row made from LCSC's record of a number to a catalog, or print the record (#454).
///
/// `catalog add --lcsc <C123> --to <catalog.csv>` asks LCSC's record of the number (never
/// JLCPCB's) and appends a row with `lcsc`, `mpn`, `package` and a `source` that names the record
/// and the day, leaving every other cell empty and listing the columns that need the datasheet or
/// JLCPCB's page; without `--to` it prints the record and writes nothing.
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct CatalogAddArgs {
    /// The LCSC number to add, such as C25104.
    #[arg(long, value_name = "C123")]
    pub lcsc: String,
    /// The catalog file to append the row to.
    #[arg(long, value_name = "CATALOG.CSV")]
    pub to: Option<PathBuf>,
    /// Ask nothing, and use a kept answer.
    #[arg(long)]
    pub offline: bool,
    /// Keep no answer.
    #[arg(long)]
    pub no_cache: bool,
}

/// List the catalog rows whose date is older than `--days` days.
///
/// `catalog age` prints the rows whose price and stock were read more than `--days` days ago
/// (the supply provider's answer, else the row's `date`) and, apart, the rows whose price and
/// stock are unknown because no provider has them and the row has no `date` (unknown is not old
/// and not 0); it reads the `[supply]` table of the project that contains the directory.
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct CatalogAgeArgs {
    /// Catalog files or directories of them, as for `refresh`.
    #[arg(value_name = "CATALOG.CSV|DIR")]
    pub paths: Vec<PathBuf>,
    /// The age in days from which a row is old.
    #[arg(long, value_name = "N", default_value_t = crate::refresh::STALE_DAYS)]
    pub days: u32,
    /// Only this catalog (repeatable).
    #[arg(long, value_name = "CATALOG")]
    pub only: Vec<String>,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum DxfCommand {
    Import(DxfImportArgs),
}

/// Print an enclosure's outline, cutouts and holes as source to paste (ADR 0045).
///
/// `dxf import` reads the board outline of an enclosure from an ASCII DXF file and prints, to
/// paste into the `case { }` section of the board's physical file, the `import` lines, an
/// `.outline: Outline`, a `Cutout` with its `.x` and `.y` for each loop inside it, and a
/// `MountingHole` for each circle on the holes layer with the requirement on the centre of its
/// body (`.hole1.place.x &= 5mm`). It writes nothing without `--write`.
///
/// The reader takes `LINE`, `ARC`, `CIRCLE`, `LWPOLYLINE` (closed flag, bulges), `POLYLINE` with
/// `VERTEX`, `SPLINE` and `ELLIPSE` (as chords within 0.005 mm), and `INSERT` of blocks
/// (translate, rotate, scale, mirror); text, dimensions and the like are not read and are counted
/// in a warning. Binary DXF is refused. Units come from `$INSUNITS` (millimetre, centimetre,
/// metre, inch); a file without them needs `--units`. Edges whose ends are within 0.001 mm join
/// into closed loops; a gap is listed with its size and place.
///
/// The one loop that is not inside another is the outline and the loops inside it are cutouts;
/// two outer loops, a loop that crosses itself, a loop that crosses a cutout or only touches the
/// outline, or a loop inside a cutout is an error. A loop that crosses the outline (a window in a
/// case wall that is partly beyond the board) is an opening: the import leaves it out and prints
/// a note with its layer, size and place on the board, and imports the rest (provisional, issue
/// #90). A circle on the outline layer is a round `Cutout`; on the holes layer it is a hole,
/// which must be a `MountingHole` package, `M2` (2.2 mm) or `M3` (3.2 mm); a hole of another
/// diameter is refused with the nearest package and the difference (issue #89). The board's frame
/// is the outline's top left corner with y down, and the output says which point of the drawing
/// that is. The outline must be a rectangle with equal rounded corners and each cutout a rounded
/// rectangle, a slot or a circle: any other shape is refused with its size and edge count (ADR
/// 0045 step C8).
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct DxfImportArgs {
    /// The enclosure's ASCII DXF file.
    #[arg(value_name = "FILE.DXF")]
    pub file: PathBuf,
    /// The layer of the board edge (repeatable).
    ///
    /// With neither `--layer` nor `--holes-layer`, the one layer that has lines or arcs is used,
    /// and if there are several the error lists them with their entity counts. A layer that the
    /// file's layer table declares is accepted when nothing is drawn on it; a name that is
    /// nowhere in the file is an error that lists the layers the file has.
    #[arg(long = "layer", value_name = "NAME")]
    pub layers: Vec<String>,
    /// The layer of the mounting holes (repeatable).
    ///
    /// A case with no holes can still pass it, when the layer table declares the layer.
    #[arg(long = "holes-layer", value_name = "NAME")]
    pub holes_layers: Vec<String>,
    /// The drawing's unit, for a file without `$INSUNITS`: mm, cm, m or inch.
    #[arg(long, value_name = "UNIT", value_parser = parse_units)]
    pub units: Option<netsmith_dxf::Units>,
    /// The board, when the file `--write` names has several.
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// Put the import into the physical file beside this source file (ADR 0048).
    ///
    /// It edits `<stem>.physical.nsm` beside that file: it makes the file, the unit or the
    /// `case` section if they are not there, replaces the members of an earlier import
    /// (`outline`, `cutout1`, `hole1`...) and the lines that state their places, keeps every
    /// other line and every comment, and prints a diff. The circuit source and the layout file
    /// are never written.
    #[arg(long, value_name = "FILE.NSM")]
    pub write: Option<PathBuf>,
}

fn parse_units(given: &str) -> Result<netsmith_dxf::Units, String> {
    netsmith_dxf::Units::parse(given)
        .ok_or_else(|| format!("needs mm, cm, m or inch, not `{given}`"))
}

impl Command {
    /// The mistakes the parser cannot see, with the name of the command they belong to.
    fn check(&self) -> Result<(), (&'static str, String)> {
        match self {
            Command::View(args) => args.check().map_err(|e| ("view", e)),
            _ => Ok(()),
        }
    }
}

/// Read the arguments after the program name. A request for help or the version is an error of
/// its kind, which prints them; [`clap::Error::exit`] prints any of them and exits (0 for help
/// and the version, 2 for a mistake).
pub fn parse_args<I, T>(args: I) -> Result<Command, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let line = std::iter::once(OsString::from("netsmith")).chain(args.into_iter().map(Into::into));
    let cli = Cli::try_parse_from(line)?;
    if let Err((name, message)) = cli.command.check() {
        let mut command = Cli::command();
        command.build();
        let command = command
            .find_subcommand_mut(name)
            .expect("a command of the enum");
        return Err(command.error(ErrorKind::ValueValidation, message));
    }
    Ok(cli.command)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, String> {
        parse_args(args).map_err(|e| e.to_string())
    }

    fn kind(args: &[&str]) -> ErrorKind {
        parse_args(args).unwrap_err().kind()
    }

    #[test]
    fn the_model_is_sound() {
        Cli::command().debug_assert();
    }

    fn build(file: Option<&str>) -> BuildArgs {
        BuildArgs {
            file: file.map(str::to_string),
            out: None,
            strict: false,
            locked: false,
            format: Format::Text,
            notes: NotesMode::All,
        }
    }

    fn fab(args: &[&str]) -> FabArgs {
        let mut line = vec!["fab"];
        line.extend_from_slice(args);
        match parse(&line) {
            Ok(Command::Fab(fab)) => fab,
            other => panic!("{args:?}: {other:?}"),
        }
    }

    #[test]
    fn catalog_add_takes_a_number_and_a_file() {
        assert_eq!(
            parse(&[
                "catalog",
                "add",
                "--lcsc",
                "C2286",
                "--to",
                "catalogs/led.csv"
            ]),
            Ok(Command::Catalog(CatalogCommand::Add(CatalogAddArgs {
                lcsc: "C2286".into(),
                to: Some(PathBuf::from("catalogs/led.csv")),
                offline: false,
                no_cache: false,
            })))
        );
        assert!(matches!(
            parse(&[
                "catalog",
                "add",
                "--lcsc",
                "C2286",
                "--offline",
                "--no-cache"
            ]),
            Ok(Command::Catalog(CatalogCommand::Add(CatalogAddArgs {
                to: None,
                offline: true,
                no_cache: true,
                ..
            })))
        ));
        let e = parse(&["catalog", "add", "--to", "x.csv"]).unwrap_err();
        assert!(e.contains("--lcsc <C123>"), "{e}");
        assert_eq!(
            kind(&["catalog", "add", "--lcsc", "C1", "--dry-run"]),
            ErrorKind::UnknownArgument
        );
    }

    #[test]
    fn the_version_is_asked_for_three_ways() {
        assert_eq!(kind(&["--version"]), ErrorKind::DisplayVersion);
        assert_eq!(kind(&["-V"]), ErrorKind::DisplayVersion);
        assert_eq!(parse(&["version"]), Ok(Command::Version));
        assert!(parse(&["version", "build"]).is_err());
    }

    #[test]
    fn help_is_asked_for_by_flag_command_or_after_a_command() {
        for args in [
            &["--help"][..],
            &["-h"],
            &["help"],
            &["help", "fab"],
            &["build", "--help"],
            &["fab", "--order", "-h"],
            &["catalog", "refresh", "--help"],
            &["dxf", "-h"],
            &["kicad-libs", "--help"],
        ] {
            assert_eq!(kind(args), ErrorKind::DisplayHelp, "{args:?}");
        }
        // No command at all is the help, as a mistake (exit 2).
        assert_eq!(
            kind(&[]),
            ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }

    #[test]
    fn an_unknown_command_is_an_error() {
        assert_eq!(kind(&["frobnicate"]), ErrorKind::InvalidSubcommand);
        assert!(parse(&["frobnicate", "--help"]).is_err());
    }

    #[test]
    fn dxf_import_takes_a_file_layers_units_a_board_and_a_source_to_write() {
        assert_eq!(
            parse(&[
                "dxf",
                "import",
                "case.dxf",
                "--layer",
                "EDGE",
                "--layer",
                "RIM",
                "--holes-layer",
                "HOLES",
                "--units",
                "inch",
                "--board",
                "Case",
                "--write",
                "case.nsm"
            ]),
            Ok(Command::Dxf(DxfCommand::Import(DxfImportArgs {
                file: "case.dxf".into(),
                layers: vec!["EDGE".into(), "RIM".into()],
                holes_layers: vec!["HOLES".into()],
                units: Some(netsmith_dxf::Units::Inches),
                board: Some("Case".into()),
                write: Some("case.nsm".into()),
            })))
        );
        assert_eq!(kind(&["dxf", "import"]), ErrorKind::MissingRequiredArgument);
        let e = parse(&["dxf", "import", "a.dxf", "--units", "furlongs"]).unwrap_err();
        assert!(e.contains("not `furlongs`"), "{e}");
        assert!(parse(&["dxf", "export", "a.dxf"]).is_err());
        assert!(parse(&["dxf", "import", "a.dxf", "--bogus"]).is_err());
    }

    #[test]
    fn kicad_libs_import_takes_one_or_two_library_tables() {
        assert_eq!(
            parse(&["kicad-libs", "import", "sym-lib-table"]),
            Ok(Command::KicadLibs(KicadLibsCommand::Import(
                KicadLibsImportArgs {
                    sym_lib_table: "sym-lib-table".into(),
                    fp_lib_table: None
                }
            )))
        );
        assert_eq!(
            parse(&["kicad-libs", "import", "s", "f"]),
            Ok(Command::KicadLibs(KicadLibsCommand::Import(
                KicadLibsImportArgs {
                    sym_lib_table: "s".into(),
                    fp_lib_table: Some("f".into())
                }
            )))
        );
        assert!(parse(&["kicad-libs"]).is_err());
        assert!(parse(&["kicad-libs", "export", "s"]).is_err());
        assert!(parse(&["kicad-libs", "import", "s", "f", "g"]).is_err());
    }

    #[test]
    fn catalog_refresh_takes_paths_and_the_flags_of_a_polite_opt_in_run() {
        assert_eq!(
            parse(&["catalog", "refresh"]),
            Ok(Command::Catalog(CatalogCommand::Refresh(
                RefreshRequest::default()
            )))
        );
        assert_eq!(
            parse(&[
                "catalog",
                "refresh",
                "catalogs/a.csv",
                "data",
                "--dry-run",
                "--only",
                "ldo",
                "--only",
                "led",
                "--lcsc",
                "C5446",
                "--delay",
                "1500",
                "--cache-ttl",
                "6",
                "--no-cache"
            ]),
            Ok(Command::Catalog(CatalogCommand::Refresh(RefreshRequest {
                paths: vec!["catalogs/a.csv".into(), "data".into()],
                dry_run: true,
                offline: false,
                only: vec!["ldo".into(), "led".into()],
                lcsc: vec!["C5446".into()],
                delay_ms: Some(1500),
                cache_ttl_hours: Some(6),
                no_cache: true,
            })))
        );
        let Ok(Command::Catalog(CatalogCommand::Refresh(r))) =
            parse(&["catalog", "refresh", "--offline"])
        else {
            panic!()
        };
        assert!(r.offline);
        let e = parse(&["catalog", "refresh", "--delay", "soon"]).unwrap_err();
        assert!(e.contains("--delay") && e.contains("soon"), "{e}");
        // --offline asks nothing, so the pace of the asking means nothing with it.
        for pace in ["--delay", "--cache-ttl"] {
            assert_eq!(
                kind(&["catalog", "refresh", "--offline", pace, "9"]),
                ErrorKind::ArgumentConflict
            );
        }
        assert_eq!(
            kind(&["catalog", "refresh", "--days", "3"]),
            ErrorKind::UnknownArgument
        );
        assert!(parse(&["catalog", "refresh", "--lcsc"]).is_err());
    }

    #[test]
    fn catalog_age_takes_paths_a_catalog_and_the_days_with_90_as_the_default() {
        assert_eq!(
            parse(&["catalog", "age"]),
            Ok(Command::Catalog(CatalogCommand::Age(CatalogAgeArgs {
                paths: vec![],
                only: vec![],
                days: 90
            })))
        );
        assert_eq!(
            parse(&["catalog", "age", "x.csv", "--days", "30", "--only", "ldo"]),
            Ok(Command::Catalog(CatalogCommand::Age(CatalogAgeArgs {
                paths: vec!["x.csv".into()],
                only: vec!["ldo".into()],
                days: 30
            })))
        );
        assert_eq!(
            kind(&["catalog", "age", "--dry-run"]),
            ErrorKind::UnknownArgument
        );
        assert!(parse(&["catalog"]).is_err());
        assert_eq!(kind(&["catalog", "prices"]), ErrorKind::InvalidSubcommand);
    }

    #[test]
    fn fab_has_its_defaults_and_takes_a_file_and_an_out_directory() {
        assert_eq!(
            fab(&["a.nsm", "--out", "x"]),
            FabArgs {
                file: Some("a.nsm".into()),
                out: Some("x".into()),
                route_timeout: None,
                allow_drc_violations: false,
                cross_check: false,
                strict: false,
                locked: false,
                constraints: None,
                pins: None,
                no_glb: false,
                save_placed: false,
                order: None,
                quiet: false,
                format: Format::Text,
                notes: NotesMode::All,
            }
        );
        assert!(parse(&["fab", "a.nsm", "--port", "9"]).is_err());
        let f = fab(&[
            "--allow-drc-violations",
            "--cross-check",
            "--quiet",
            "--no-glb",
        ]);
        assert!(f.allow_drc_violations && f.cross_check && f.quiet && f.no_glb);
        assert!(parse(&["build", "--cross-check"]).is_err());
        assert!(parse(&["build", "--no-glb"]).is_err());
    }

    #[test]
    fn fab_takes_a_route_timeout_in_whole_seconds() {
        assert_eq!(fab(&["--route-timeout", "30"]).route_timeout, Some(30));
        let e = parse(&["fab", "--route-timeout", "soon"]).unwrap_err();
        assert!(e.contains("--route-timeout") && e.contains("soon"), "{e}");
        assert!(parse(&["fab", "--route-timeout"]).is_err());
        assert!(parse(&["build", "--route-timeout", "3"]).is_err());
    }

    #[test]
    fn fab_order_takes_a_name_or_lists_the_orders_and_no_file() {
        assert_eq!(
            fab(&["--order", "devkit"]).order(),
            Some(OrderRequest::Make("devkit".into()))
        );
        assert_eq!(fab(&["--order"]).order(), Some(OrderRequest::List));
        assert_eq!(fab(&[]).order(), None);
        // A flag after `--order` is not its name.
        let f = fab(&["--order", "--out", "x"]);
        assert_eq!(
            (f.order(), f.out),
            (Some(OrderRequest::List), Some("x".into()))
        );
        let f = fab(&["--out", "x", "--order", "d", "--route-timeout", "9"]);
        assert_eq!(f.order(), Some(OrderRequest::Make("d".into())));
        assert_eq!(f.route_timeout, Some(9));
        assert_eq!(
            kind(&["fab", "a.nsm", "--order", "d"]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(
            kind(&["fab", "--order", "d", "--save-placed"]),
            ErrorKind::ArgumentConflict
        );
        assert!(fab(&["a.nsm", "--save-placed"]).save_placed);
        assert!(parse(&["build", "--order", "d"]).is_err());
    }

    #[test]
    fn build_and_fab_take_strict_and_locked_and_nothing_else_does() {
        let Ok(Command::Build(b)) = parse(&["build", "--strict", "--locked"]) else {
            panic!()
        };
        assert!(b.strict && b.locked);
        let f = fab(&["a.nsm", "--locked", "--strict"]);
        assert!(f.locked && f.strict);
        assert!(!fab(&[]).locked, "a run may change the lock by default");
        assert!(parse(&["editor", "--strict"]).is_err());
        assert!(parse(&["editor", "--locked"]).is_err());
        assert!(parse(&["compare", "a", "b", "--locked"]).is_err());
    }

    #[test]
    fn fab_takes_pins_and_constraints_for_one_run() {
        assert_eq!(fab(&["--pins", "locked"]).pins_locked(), Some(true));
        assert_eq!(fab(&["--pins", "auto"]).pins_locked(), Some(false));
        assert_eq!(fab(&[]).pins_locked(), None, "the manifest decides");
        assert_eq!(
            fab(&["--constraints", "warn"]).constraints_warn(),
            Some(true)
        );
        assert_eq!(
            fab(&["--constraints", "error"]).constraints_warn(),
            Some(false)
        );
        assert_eq!(fab(&[]).constraints_warn(), None, "the manifest decides");
        let e = parse(&["fab", "--pins", "swap"]).unwrap_err();
        assert!(
            e.contains("swap") && e.contains("auto") && e.contains("locked"),
            "{e}"
        );
        let e = parse(&["fab", "--constraints", "off"]).unwrap_err();
        assert!(
            e.contains("off") && e.contains("warn") && e.contains("error"),
            "{e}"
        );
        assert!(parse(&["fab", "--pins"]).is_err());
        assert!(parse(&["build", "--constraints", "warn"]).is_err());
    }

    #[test]
    fn format_and_notes_have_defaults_and_a_set_of_values() {
        let Ok(Command::Check(c)) = parse(&["check", "--format", "json", "--notes", "summary"])
        else {
            panic!()
        };
        assert_eq!((c.format, c.notes), (Format::Json, NotesMode::Summary));
        let Ok(Command::Check(c)) = parse(&["check"]) else {
            panic!()
        };
        assert_eq!((c.format, c.notes), (Format::Text, NotesMode::All));
        assert_eq!(kind(&["build", "--format", "xml"]), ErrorKind::InvalidValue);
        assert_eq!(kind(&["build", "--notes", "some"]), ErrorKind::InvalidValue);
        assert!(parse(&["editor", "--format", "json"]).is_err());
    }

    #[test]
    fn a_command_takes_one_nsm_file_before_or_after_its_flags() {
        assert_eq!(parse(&["build"]), Ok(Command::Build(build(None))));
        assert_eq!(
            parse(&["build", "a.nsm"]),
            Ok(Command::Build(build(Some("a.nsm"))))
        );
        let expected = Ok(Command::Build(BuildArgs {
            out: Some("x".into()),
            ..build(Some("a.nsm"))
        }));
        assert_eq!(parse(&["build", "a.nsm", "--out", "x"]), expected);
        assert_eq!(parse(&["build", "--out", "x", "a.nsm"]), expected);
        assert!(parse(&["editor", "a.nsm", "b.nsm"]).is_err());
        assert!(parse(&["init", "dir"]).is_err());
    }

    #[test]
    fn a_flag_given_twice_is_the_last_one() {
        let Ok(Command::Build(b)) =
            parse(&["build", "--out", "a", "--out", "b", "--strict", "--strict"])
        else {
            panic!()
        };
        assert_eq!(b.out, Some("b".into()));
        assert!(b.strict);
        let Ok(Command::Lock(l)) = parse(&["lock", "--check", "--check"]) else {
            panic!()
        };
        assert_eq!(l.mode(), LockMode::Check);
    }

    #[test]
    fn lock_takes_update_or_check_and_not_both() {
        let mode = |args: &[&str]| match parse(args) {
            Ok(Command::Lock(l)) => (l.mode(), l.file),
            other => panic!("{args:?}: {other:?}"),
        };
        assert_eq!(mode(&["lock", "--update"]), (LockMode::Update, None));
        assert_eq!(
            mode(&["lock", "--check", "a.nsm"]),
            (LockMode::Check, Some("a.nsm".into()))
        );
        assert_eq!(kind(&["lock"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(
            kind(&["lock", "--update", "--check"]),
            ErrorKind::ArgumentConflict
        );
    }

    #[test]
    fn the_editor_takes_a_port_with_8787_as_the_default() {
        let port = |args: &[&str]| match parse(args) {
            Ok(Command::Editor(e)) => e.port,
            other => panic!("{args:?}: {other:?}"),
        };
        assert_eq!(port(&["editor", "a.nsm"]), 8787);
        assert_eq!(port(&["editor", "--port", "9000"]), 9000);
        assert!(
            parse(&["editor", "--port", "http"])
                .unwrap_err()
                .contains("http")
        );
        assert!(parse(&["editor", "--port", "70000"]).is_err());
        assert!(parse(&["editor", "--port"]).is_err());
        assert!(parse(&["editor", "--out", "x"]).is_err());
        assert!(parse(&["build", "--port", "1"]).is_err());
    }

    #[test]
    fn route_takes_a_placed_board_a_timeout_and_a_margin() {
        assert_eq!(
            parse(&[
                "route",
                "b.placed.json",
                "--out",
                "o",
                "--route-timeout",
                "0",
                "--margin",
                "2.5"
            ]),
            Ok(Command::Route(RouteArgs {
                file: "b.placed.json".into(),
                out: Some("o".into()),
                route_timeout: Some(0),
                margin: Some(2.5),
            }))
        );
        assert_eq!(kind(&["route"]), ErrorKind::MissingRequiredArgument);
        for bad in ["0.5", "5", "wide"] {
            let e = parse(&["route", "b.json", "--margin", bad]).unwrap_err();
            assert!(
                e.contains("1 (the process's least track) to 4"),
                "{bad}: {e}"
            );
        }
    }

    #[test]
    fn compare_takes_two_files_and_map_or_by_refdes() {
        assert_eq!(
            parse(&[
                "compare",
                "--board",
                "Led",
                "a.xml",
                "--map",
                "m.txt",
                "b.xml",
                "--ignore-unconnected",
                "--loose-values"
            ]),
            Ok(Command::Compare(CompareArgs {
                a: "a.xml".into(),
                b: "b.xml".into(),
                board: Some("Led".into()),
                map: Some("m.txt".into()),
                by_refdes: false,
                ignore_unconnected: true,
                loose_values: true,
                format: Format::Text,
            }))
        );
        assert_eq!(
            kind(&["compare", "a.nsm"]),
            ErrorKind::MissingRequiredArgument
        );
        assert_eq!(
            kind(&["compare", "a", "b", "c"]),
            ErrorKind::UnknownArgument
        );
        assert_eq!(
            kind(&["compare", "a", "b", "--map", "m", "--by-refdes"]),
            ErrorKind::ArgumentConflict
        );
        assert!(parse(&["compare", "a", "b", "--map"]).is_err());
        assert!(parse(&["build", "a.nsm", "--loose-values"]).is_err());
    }

    #[test]
    fn parity_takes_the_board_and_its_files() {
        assert!(matches!(
            parse(&["parity", "b.kicad_pcb", "--format", "json"]),
            Ok(Command::Parity(ParityArgs {
                table: None,
                map: None,
                svg: None,
                format: Format::Json,
                ..
            }))
        ));
        assert_eq!(kind(&["parity"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn init_takes_only_a_name() {
        assert_eq!(parse(&["init"]), Ok(Command::Init(InitArgs { name: None })));
        assert_eq!(
            parse(&["init", "--name", "demo"]),
            Ok(Command::Init(InitArgs {
                name: Some("demo".into())
            }))
        );
        assert!(parse(&["init", "a.nsm"]).unwrap_err().contains("a.nsm"));
        assert!(parse(&["init", "--name"]).is_err());
    }

    fn render(args: &[&str]) -> crate::render::Request {
        let mut line = vec!["render"];
        line.extend_from_slice(args);
        match parse(&line) {
            Ok(Command::Render(r)) => r.request(),
            other => panic!("{args:?}: {other:?}"),
        }
    }

    #[test]
    fn render_has_the_defaults_of_a_request_and_takes_its_flags() {
        assert_eq!(render(&[]), crate::render::Request::default());
        let r = render(&[
            "--view",
            "top",
            "--size",
            "64x80",
            "--section",
            "z=-0.8",
            "--highlight",
            "J1,net:GND",
            "--highlight",
            "limit:lid",
            "--xray",
            "--no-caps",
            "--theme",
            "dark",
            "--perspective",
        ]);
        assert_eq!(r.view, netsmith_pcb::raster3d::View::Top);
        assert_eq!(r.size, (64, 80));
        assert_eq!(r.section, Some((netsmith_pcb::mesh::Axis::Z, -0.8)));
        assert_eq!(r.highlight, vec!["J1", "net:GND", "limit:lid"]);
        assert!(r.xray && !r.caps && r.dark && r.perspective);
        assert!(!render(&["--ortho"]).perspective);
        assert_eq!(
            kind(&["render", "--ortho", "--perspective"]),
            ErrorKind::ArgumentConflict
        );
        let e = parse(&["render", "--view", "sideways"]).unwrap_err();
        assert!(e.contains("iso-bottom"), "{e}");
        let e = parse(&["render", "--size", "10x10"]).unwrap_err();
        assert!(e.contains("--size"), "{e}");
    }

    fn view_request(args: &[&str]) -> crate::view_client::Request {
        let mut line = vec!["view"];
        line.extend_from_slice(args);
        match parse(&line) {
            Ok(Command::View(v)) => v.request().unwrap(),
            other => panic!("{args:?}: {other:?}"),
        }
    }

    #[test]
    fn view_with_no_flag_reads_the_state_of_the_editor_on_the_default_port() {
        let request = view_request(&[]);
        assert_eq!(request.port, DEFAULT_PORT);
        assert!(!request.changes() && !request.json && request.picture.is_none());
        assert_eq!(request.change, netsmith_editor::api::ViewChange::default());
    }

    #[test]
    fn view_takes_the_flags_of_render_and_the_editors_own() {
        use netsmith_editor::api::{ColourBy, PreviewSetting, RenderView};
        let request = view_request(&[
            "--port",
            "9001",
            "--board",
            "Demo/main",
            "--view",
            "iso-bottom",
            "--section",
            "x=12.5",
            "--highlight",
            "J1,net:GND",
            "--highlight",
            "limit:lid",
            "--xray",
            "--perspective",
            "--colour-by",
            "margin",
            "--height-preview",
            "2.5",
            "--layers-apart",
            "--json",
            "--picture",
            "j1.png",
        ]);
        let change = &request.change;
        assert_eq!(request.port, 9001);
        assert_eq!(change.board.as_deref(), Some("Demo/main"));
        assert_eq!(change.view, Some(RenderView::IsoBottom));
        assert_eq!(change.section.as_deref(), Some("x=12.5"));
        assert_eq!(
            change.highlight,
            Some(vec!["J1".into(), "net:GND".into(), "limit:lid".into()])
        );
        assert_eq!(change.xray, Some(true));
        assert_eq!(change.ortho, Some(false));
        assert_eq!(change.colour_by, Some(ColourBy::Margin));
        assert_eq!(change.height_preview, Some(PreviewSetting::Mm(2.5)));
        assert_eq!(change.exploded, Some(true));
        assert!(request.json && request.changes());
        assert_eq!(request.picture, Some(PathBuf::from("j1.png")));
        // The reverse of each switch, and the other spelling of colour.
        let request = view_request(&[
            "--no-xray",
            "--ortho",
            "--no-layers-apart",
            "--color-by",
            "kind",
            "--height-preview",
            "off",
            "--section",
            "none",
            "--select",
            "R1",
        ]);
        let change = &request.change;
        assert_eq!(change.xray, Some(false));
        assert_eq!(change.ortho, Some(true));
        assert_eq!(change.exploded, Some(false));
        assert_eq!(change.colour_by, Some(ColourBy::Kind));
        assert_eq!(change.height_preview, Some(PreviewSetting::On(false)));
        assert_eq!(change.section.as_deref(), Some("none"));
        assert_eq!(change.select, Some(vec!["R1".into()]));
    }

    #[test]
    fn view_switches_and_their_reverses_are_the_last_one_given() {
        let change =
            view_request(&["--xray", "--no-xray", "--no-layers-apart", "--layers-apart"]).change;
        assert_eq!((change.xray, change.exploded), (Some(false), Some(true)));
        assert_eq!(
            view_request(&["--no-xray", "--xray"]).change.xray,
            Some(true)
        );
    }

    #[test]
    fn view_says_none_to_light_or_select_nothing() {
        assert_eq!(
            view_request(&["--highlight", "none"]).change.highlight,
            Some(Vec::new())
        );
        assert_eq!(
            view_request(&["--select", "none"]).change.select,
            Some(Vec::new())
        );
        for args in [
            &["view", "--highlight", "none,J1"][..],
            &["view", "--highlight", "none", "--highlight", "J1"],
            &["view", "--select", "J1", "--select", "none"],
        ] {
            let e = parse(args).unwrap_err();
            assert!(e.contains("clears it"), "{args:?}: {e}");
        }
    }

    #[test]
    fn a_picture_alone_still_changes_the_page_so_that_the_view_is_open() {
        let request = view_request(&["--picture", "a.png"]);
        assert!(request.changes());
        assert_eq!(request.change, netsmith_editor::api::ViewChange::default());
    }

    #[test]
    fn view_mistakes_say_what_is_wrong() {
        for (args, says) in [
            (&["--theme", "dark"][..], "person's own setting"),
            (&["--view", "sideways"], "iso-bottom"),
            (&["--view"], "--view"),
            (&["--section", "w=1"], "--section needs"),
            (&["--section", "x"], "--section needs"),
            (&["--ortho", "--perspective"], "cannot be used with"),
            (
                &["--highlight", "J1", "--select", "J1"],
                "cannot be used with",
            ),
            (&["--select", "limit:lid"], "--highlight"),
            (&["--colour-by", "shade"], "provenance"),
            (&["--height-preview", "0"], "0.1 to 50"),
            (&["--height-preview", "51"], "0.1 to 50"),
            (&["--height-preview", "tall"], "0.1 to 50"),
            (&["--port", "0"], "1..=65535"),
            (&["--port", "http"], "http"),
            (&["--picture"], "--picture"),
            (&["--out", "a.png"], "--picture"),
            (&["--size", "100x100"], "--size"),
            (&["examples/led.nsm"], "takes no file"),
        ] {
            let mut line = vec!["view"];
            line.extend_from_slice(args);
            let e = parse(&line).unwrap_err();
            assert!(e.contains(says), "{args:?}: {e}");
        }
        // The mistakes clap cannot see are usage errors too, with the command's usage.
        let e = parse_args(["view", "--theme", "dark"]).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::ValueValidation);
        assert!(e.to_string().contains("netsmith view"), "{e}");
    }
}
