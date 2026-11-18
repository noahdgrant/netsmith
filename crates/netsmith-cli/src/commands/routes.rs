//! `netsmith routes` (epic #599): the routes file beside a source (`<stem>.routes.toml`, ADR
//! 0030), the copper an agent or a person fixes so that a rebuild keeps it. Its format and what
//! its copper does are in the language guide's "The routes file" and the routes module of
//! `netsmith-pcb`; the file beside a source is `netsmith_fabricate::routes`, the import of a KiCad
//! board `netsmith_tools::routes_import`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args as ClapArgs, Subcommand};
use netsmith_fabricate::board3d::Placed;
use netsmith_fabricate::routes as lib;
use netsmith_pcb::{Board, CopperLayer, Layout, ZonePads};
use netsmith_tools::routes_import::{self, Import};

use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "The routes file `<stem>.routes.toml` sits beside the source and its layout file. `fab`, the editor and `netsmith dev route` (through `board fab --save-placed`) draw its copper on the placed board before the router, which keeps it and routes only the connections it leaves; a net with `lock = true` gets no copper from the router. Places are in mm from the outline's top left corner, y down, as in the layout file. The file's format is in the language guide's \"The routes file\".\n\nEach command builds and places the board as `fab` does (nothing is routed, but by `route`). The exit code is 0 when it did what it was asked, 1 when it refused (a shape that breaks the design rules, a locked shape, a net the board does not have), and 2 when it could not run."
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

/// The board a command acts on.
#[derive(Debug, PartialEq, ClapArgs)]
pub struct Target {
    /// The source file; with none, the project's boards (see netsmith.toml).
    #[arg(value_name = "FILE.NSM", long)]
    pub file: Option<String>,
    /// The board, by name, when the source has more than one.
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum Command {
    /// Write the routes file (and the layout file's places) from a KiCad board.
    #[command(
        after_long_help = "Every track, arc and through via of F.Cu, B.Cu and the inner signal layers becomes the copper of the design's net its pads are on: a KiCad net is the design's net that most of its pads are on, with the parts paired by a port's parts file (`--parts`, default `reference/parts.toml` of the project when there is one; without one, by reference designator). A KiCad net on no pad of a part the design has keeps its name if the design has a net of that name, and is left out otherwise; copper on an inner layer is left out unless the layer is one of the process's signal layers (an inner plane holds no track); each is said. Each zone of copper that is not the process's own plane or pour becomes a zone of its net's record, on its layers, with its outline, holes and the settings that change its copper (priority, clearance, minimum thickness, pad connection, thermal gap and spoke width, island removal); a layer with such a zone takes its zones from the routes file alone, and the design's pour or plane on it is not drawn. A zone that is the process's own plane or pour (the same net, layer and outline, and the same clearance, minimum width, pad connection and thermal relief as the process states, which is what `fab` writes) is left out and said, so that `fab` and then `routes import` of its board leaves a file of tracks and vias; one that differs from it is imported, as a hand zone. Rule areas, teardrops and hatched zones are left out and said.\n\nEach part of the design that the KiCad board has is written to the layout file at the KiCad board's place, turn and face, locked (`source = \"import\"`), unless `--no-places`, or the physical file pins it (its requirement places it). Places are from the top left corner of the KiCad board's Edge.Cuts box. Then the board is placed again and each net's record takes its pads as they are. Every net is locked (the router adds nothing to it) unless `--unlocked`. The routes file is written over; the layout file keeps its seed, keepouts and the parts the import does not place."
    )]
    Import(ImportArgs),
    /// Print the routes file's copper and what the router would still route.
    Show(ShowArgs),
    /// Add a track to a net, refused if it breaks the design rules.
    #[command(name = "add-track")]
    AddTrack(AddTrackArgs),
    /// Add a via, or a group of vias side by side, to a net, refused if it breaks the design rules.
    #[command(
        name = "add-via",
        after_long_help = "One via is checked alone: for a net whose current needs several vias side by side where it changes layer, a lone via is refused by the design rules check (`via_current`). A group is added and checked together, as the router's own group is, and is accepted or refused as a whole (the file is then as it was). Name the places of a group with `--at X,Y` once for each via, or give `--count N --pitch MM` with one `--at`: N vias in a row along x, PITCH mm between their centres, centred on that place. The vias of a group count as side by side when they touch, so a pitch above the via's diameter leaves them apart (and the check says so). All vias of a group share the net, `--diameter`, `--drill`, `--layers` and `--lock`; a group has at most 64 vias."
    )]
    AddVia(AddViaArgs),
    /// Add a zone (copper area) of a net on a layer, refused if it breaks the design rules.
    #[command(
        name = "add-zone",
        after_long_help = "The outline is a polygon given by `--outline X,Y` once for each corner (three or more, in order), by `--rect X,Y,W,H` for a rectangle, or by `--board-outline` for the board's whole outline. Places are mm from the outline's top left corner, y down, as for `add-track`. The zone is filled inside the outline on each `--layer` (`top`, `bottom`, `inner1`, ...; repeat the flag for several), joined to its net; where two zones of a layer overlap the higher `--priority` has the area, and two zones of different nets with the same priority must not overlap. Without a setting the zone has the design's own plane's: the clearance the process's planes keep, a minimum width of 0.25 mm, thermal relief gap and spokes of 0.5 mm, priority 0, a thermal relief at each pad of its net.\n\nA layer that has a zone of the routes file takes its zones from the file alone: the design's pour or plane on it is not drawn (the command says so when this zone makes that so). A zone is refused when a corner is off the board, a setting is wrong, or the design rules check finds an error with it that the board did not have (a short, a clearance, a track of another net it covers); the file is then as it was. The zone is written to the net's record (made, with the net's pad hash, if the net has none); `routes show` numbers it, and `delete`, `lock` and `unlock` name it with `--zone N`."
    )]
    AddZone(AddZoneArgs),
    /// Route one net, or one connection of it, round the copper there; say which nets stood in
    /// its way, and with `--rip` route them again after it.
    #[command(
        after_long_help = "The router routes the net (or the connection, `NET:REF.PAD-REF.PAD`: the lines that join those two pads alone) on the board as placed with the routes file's copper, every other net held as it is, and writes its tracks and vias to the net's record, unlocked. A connection joined already is said, and nothing is written. The text says the narrowest place of the new way and the widest track of the net that has room there, with every space of the design rules.\n\nWhen the net has no way round the copper there, it is routed again with the unlocked tracks and vias of the other nets taken away, and the nets whose copper that way crosses are named (exit 1, nothing written); where there is no way even so, what walls it in is said: pads, vias, the copper of locked nets, the edge. With `--rip NET,...` the copper of those nets alone may be taken away: it is removed, the net is routed through the room it left, and each ripped net is routed again after it (it may rip in turn, from the nets named). A locked net or a locked shape is never ripped, nor is a zone. The rounds are bounded by counts, those of the router's rounds of rip-up: 6 rounds, 3 rips of one net, 64 runs of the router. A ripped net left with more connections open than it had, or a result the design rules check finds a new error with, is refused and the file is as it was. `--dry-run` says what it would do and writes nothing."
    )]
    Route(RouteArgs),
    /// Delete a net's record, or one of its shapes; a locked one is refused.
    Delete(ShapeArgs),
    /// Lock a net (the router adds no copper to it) or one shape (kept when the net's pads move).
    #[command(
        after_long_help = "Lock also writes the net's record again with its pads as they are now, so a record whose pads moved (fab's warning) is current again and all its shapes are drawn as written; it is refused, as an added shape is, when that brings a new error of the design rules check."
    )]
    Lock(ShapeArgs),
    /// Unlock a net or one shape.
    Unlock(ShapeArgs),
    /// Cut corners to 45 degrees, merge collinear tracks, remove dangling vias, stubs and acid
    /// traps, refusing any change that adds a finding or loses a connection.
    #[command(
        after_long_help = "A pass over the routes file's copper (and with `--reuse-routes` the router's copper kept in `.netsmith/routes/`, as `board fab --reuse-routes` starts from it) that makes four repairs:\n\n- a corner: two straight tracks of a net that meet outside its pads and bend by more than 45 degrees (the design rules check's `track_angle`) are cut into bends of 45 degrees at most, a 45 degree chamfer at a right angle; the chamfer starts four widths from the corner, and two and one width when the larger is refused;\n- a collinear pair: two straight tracks of one net, layer and width that meet end to end on one line, with nothing else there, become one;\n- a dangling via or a stub: a via joined to copper of its net on fewer than two layers (`via_dangling`) and a track with an end that touches nothing (`track_dangling`) are removed when the copper makes every connection of the net (the router may still start from a stub of a net it has to finish), and a via that joins nothing at all is removed in any net;\n- an acid trap (`acid_trap`): a track that leaves a pad or a via at an acute angle to its edge is made to leave it at a right angle and bend after the edge, and a track that ends on the middle of another at an acute angle is made to meet it at a right angle.\n\nEach change is made only when the design rules check of the board with it (the check `add-track` makes, with its warnings) finds nothing it did not find before and the connections left to route are the same; a refused change is listed with the first new finding or the connection it changes, and left as it was. The changes of each net are checked together where they are: over the copper, pads and zones within the board's largest space of what they change, with and without them, and the whole board is checked once after each pass, which checks the pass's changes again against the whole board when it finds what they add beyond where they are. The text ends with how many checks ran and the items they looked at (`checks` and `items_checked` in the JSON). A locked net or shape, an arc, a zone and a record whose pads moved are never touched. `--dry-run` says what would change, by kind, net, layer and place, and writes nothing. With `--reuse-routes` the kept file is written again, so that `board fab --reuse-routes` still starts from it; without it, a change to the routes file makes a kept file stale, and the command says so. The exit code is 0 when the pass ran (whether or not it changed or refused anything), 2 when it could not run.\n\nOn stderr each pass says how far it has got, net by net: the net, how many of the pass's nets, the changes made so far and about how long the pass has left; `--progress json` prints each line as `{\"board\", \"stage\": \"tidying\", \"unit\": \"nets tidied\", \"done\", \"total\", \"detail\", \"elapsedMs\", \"message\"}`, and `--to-editor` sends them to a running editor, as `board fab` does. With `--format json` the progress is printed only with `--progress json`. Nothing is written until the whole pass has run, so Ctrl+C leaves the files as they were."
    )]
    Tidy(TidyArgs),
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct TidyArgs {
    #[command(flatten)]
    pub target: Target,
    /// Say what would change and write nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Tidy the router's copper kept on disk too, as `board fab --reuse-routes` would start from it.
    #[arg(long)]
    pub reuse_routes: bool,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
    #[command(flatten)]
    pub watching: common::Watching,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct ImportArgs {
    /// The KiCad board to take the copper from.
    #[arg(value_name = "BOARD.KICAD_PCB")]
    pub pcb: PathBuf,
    #[command(flatten)]
    pub target: Target,
    /// The port's parts file: the KiCad board's reference of each member path.
    #[arg(long, value_name = "PARTS.TOML")]
    pub parts: Option<PathBuf>,
    /// Write no places to the layout file.
    #[arg(long)]
    pub no_places: bool,
    /// Leave every net unlocked: the router may add copper to complete it.
    #[arg(long)]
    pub unlocked: bool,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct ShowArgs {
    #[command(flatten)]
    pub target: Target,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct RouteArgs {
    /// The net, or one connection of it: `NET` or `NET:REF.PAD-REF.PAD` (`VCC:U1.3-C2.1`).
    #[arg(value_name = "NET[:PAD-PAD]")]
    pub net: String,
    #[command(flatten)]
    pub target: Target,
    /// Nets whose unlocked tracks and vias may be removed and routed again after it, `A,B`.
    #[arg(long, value_name = "NET,...", value_delimiter = ',')]
    pub rip: Vec<String>,
    /// Say what it would do, and write nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct AddTrackArgs {
    #[command(flatten)]
    pub target: Target,
    /// The net, by its name in the design.
    #[arg(long, value_name = "NET")]
    pub net: String,
    /// `top`, `bottom` or an inner signal layer (`inner2`, ...).
    #[arg(long, value_name = "LAYER", value_parser = parse_layer, default_value = "top")]
    pub layer: CopperLayer,
    /// The width in mm (default: the net's own width).
    #[arg(long, value_name = "MM")]
    pub width: Option<f64>,
    /// Where it starts, `X,Y` in mm from the outline's top left corner.
    #[arg(long, value_name = "X,Y", value_parser = parse_point, allow_hyphen_values = true)]
    pub from: [f64; 2],
    /// Where it ends.
    #[arg(long, value_name = "X,Y", value_parser = parse_point, allow_hyphen_values = true)]
    pub to: [f64; 2],
    /// For an arc, a point on it halfway from `--from` to `--to`.
    #[arg(long, value_name = "X,Y", value_parser = parse_point, allow_hyphen_values = true)]
    pub mid: Option<[f64; 2]>,
    /// Lock the track: it is kept when the net's pads move.
    #[arg(long)]
    pub lock: bool,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct AddViaArgs {
    #[command(flatten)]
    pub target: Target,
    /// The net, by its name in the design.
    #[arg(long, value_name = "NET")]
    pub net: String,
    /// Where it stands, `X,Y` in mm from the outline's top left corner; repeat it for each via of
    /// a group.
    #[arg(long, value_name = "X,Y", value_parser = parse_point, allow_hyphen_values = true, required = true, action = clap::ArgAction::Append)]
    pub at: Vec<[f64; 2]>,
    /// A group of N vias in a row along x, centred on the one `--at` (needs `--pitch`).
    #[arg(long, value_name = "N", requires = "pitch", value_parser = clap::value_parser!(u32).range(1..))]
    pub count: Option<u32>,
    /// The space between the centres of a group's vias, in mm (needs `--count`).
    #[arg(long, value_name = "MM", requires = "count", value_parser = parse_pitch, allow_hyphen_values = true)]
    pub pitch: Option<f64>,
    /// The diameter in mm (default: the process's via).
    #[arg(long, value_name = "MM")]
    pub diameter: Option<f64>,
    /// The drill in mm (default: the process's via drill).
    #[arg(long, value_name = "MM")]
    pub drill: Option<f64>,
    /// A blind or a buried via: the two layers it ends on, `top,inner1`, a via the board's HDI
    /// process makes; its size is then that via's (the laser's for a blind via). Default: a
    /// through via.
    #[arg(long, value_name = "LAYER,LAYER", value_parser = parse_layer_pair)]
    pub layers: Option<[CopperLayer; 2]>,
    /// Lock the via: it is kept when the net's pads move.
    #[arg(long)]
    pub lock: bool,
}

#[derive(Debug, PartialEq, ClapArgs)]
#[command(group = clap::ArgGroup::new("outline_of").required(true).multiple(false))]
pub struct AddZoneArgs {
    #[command(flatten)]
    pub target: Target,
    /// The net, by its name in the design.
    #[arg(long, value_name = "NET")]
    pub net: String,
    /// `top`, `bottom` or an inner layer (`inner1`, ...); repeat for a zone on several.
    #[arg(long, value_name = "LAYER", default_value = "top")]
    pub layer: Vec<String>,
    /// One corner of the outline, `X,Y` in mm from the outline's top left corner; repeat it for
    /// each corner, in order.
    #[arg(long, value_name = "X,Y", value_parser = parse_point, allow_hyphen_values = true, group = "outline_of")]
    pub outline: Vec<[f64; 2]>,
    /// A rectangle for the outline: `X,Y,W,H`, its top left corner and its size in mm.
    #[arg(long, value_name = "X,Y,W,H", value_parser = parse_rect, allow_hyphen_values = true, group = "outline_of")]
    pub rect: Option<[f64; 4]>,
    /// The board's whole outline for the zone's.
    #[arg(long, group = "outline_of")]
    pub board_outline: bool,
    /// The space from other nets' copper, in mm (default: the process's plane clearance).
    #[arg(long, value_name = "MM")]
    pub clearance: Option<f64>,
    /// The narrowest copper the zone keeps, in mm (default: 0.25).
    #[arg(long, value_name = "MM")]
    pub min_width: Option<f64>,
    /// The gap of a thermal relief, in mm (default: 0.5).
    #[arg(long, value_name = "MM")]
    pub thermal_gap: Option<f64>,
    /// The width of a thermal relief's spokes, in mm (default: 0.5).
    #[arg(long, value_name = "MM")]
    pub spoke_width: Option<f64>,
    /// Where two zones of a layer overlap, the higher has the area.
    #[arg(long, value_name = "N", default_value_t = 0)]
    pub priority: u32,
    /// How the zone joins its net's pads.
    #[arg(long, value_enum, default_value = "thermal")]
    pub pads: PadConnection,
    /// Lock the zone: it is kept when the net's pads move.
    #[arg(long)]
    pub lock: bool,
}

/// How a zone joins the pads of its net.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum PadConnection {
    /// Solid copper up to the pad.
    Solid,
    /// A gap round the pad crossed by four spokes.
    Thermal,
    /// Not joined: the pad keeps the zone's clearance.
    None,
    /// A thermal relief at through-hole pads, solid copper at surface-mount pads.
    ThruHoleOnly,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct ShapeArgs {
    #[command(flatten)]
    pub target: Target,
    /// The net, by its name in the design.
    #[arg(long, value_name = "NET")]
    pub net: String,
    /// One track of the net, numbered from 1 as `routes show` lists them.
    #[arg(long, value_name = "N", conflicts_with_all = ["via", "zone"])]
    pub track: Option<usize>,
    /// One via of the net, numbered from 1 as `routes show` lists them.
    #[arg(long, value_name = "N", conflicts_with = "zone")]
    pub via: Option<usize>,
    /// One zone of the net, numbered from 1 as `routes show` lists them.
    #[arg(long, value_name = "N")]
    pub zone: Option<usize>,
}

/// A copper layer by its word: `top`, `bottom`, `inner2`.
pub(super) fn parse_layer(text: &str) -> Result<CopperLayer, String> {
    CopperLayer::of_word(text).ok_or_else(|| {
        format!("`{text}` is not a copper layer: `top`, `bottom` or `inner1`, `inner2`, ...")
    })
}

fn parse_layer_pair(text: &str) -> Result<[CopperLayer; 2], String> {
    let (a, b) = text
        .split_once(',')
        .ok_or_else(|| format!("`{text}` is not two layers, `top,inner1`"))?;
    let (a, b) = (parse_layer(a.trim())?, parse_layer(b.trim())?);
    if a == b {
        return Err(format!(
            "`{text}` names one layer twice: a via joins two layers"
        ));
    }
    Ok([a, b])
}

pub(super) fn parse_point(text: &str) -> Result<[f64; 2], String> {
    let (x, y) = text
        .split_once(',')
        .ok_or_else(|| format!("`{text}` is not X,Y"))?;
    let n = |v: &str| {
        v.trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("`{v}` is not a number"))
    };
    Ok([n(x)?, n(y)?])
}

fn parse_pitch(text: &str) -> Result<f64, String> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| format!("`{text}` is not a pitch above 0 mm"))
}

fn parse_rect(text: &str) -> Result<[f64; 4], String> {
    let parts: Vec<&str> = text.split(',').collect();
    let [x, y, w, h] = parts[..] else {
        return Err(format!("`{text}` is not X,Y,W,H"));
    };
    let n = |v: &str| {
        v.trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("`{v}` is not a number"))
    };
    let r = [n(x)?, n(y)?, n(w)?, n(h)?];
    if r[2] <= 0.0 || r[3] <= 0.0 {
        return Err(format!("`{text}`: the width and height must be above 0"));
    }
    Ok(r)
}

pub fn run(args: Args) -> ExitCode {
    match args.command {
        Command::Import(a) => import(&a),
        Command::Show(a) => show(&a),
        Command::AddTrack(a) => edit(&a.target, |_| lib::Edit::AddTrack {
            net: a.net.clone(),
            layer: a.layer,
            width: a.width,
            from: a.from,
            mid: a.mid,
            to: a.to,
            lock: a.lock,
        }),
        Command::AddVia(a) => match via_places(&a) {
            Ok(places) => edit_all(&a.target, |_| {
                places
                    .iter()
                    .map(|&at| lib::Edit::AddVia {
                        net: a.net.clone(),
                        at,
                        diameter: a.diameter,
                        drill: a.drill,
                        layers: a.layers,
                        lock: a.lock,
                    })
                    .collect()
            }),
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::from(2)
            }
        },
        Command::AddZone(a) => edit(&a.target, |board| lib::Edit::AddZone {
            net: a.net.clone(),
            layers: a.layer.clone(),
            outline: zone_outline(&a, board),
            clearance: a.clearance,
            min_width: a.min_width,
            thermal_gap: a.thermal_gap,
            spoke_width: a.spoke_width,
            priority: a.priority,
            pads: pad_connection(a.pads),
            lock: a.lock,
        }),
        Command::Route(a) => route(&a),
        Command::Delete(a) => edit(&a.target, |_| lib::Edit::Delete {
            net: a.net.clone(),
            shape: shape(&a),
        }),
        Command::Lock(a) => edit(&a.target, |_| lib::Edit::Lock {
            net: a.net.clone(),
            shape: shape(&a),
            on: true,
        }),
        Command::Unlock(a) => edit(&a.target, |_| lib::Edit::Lock {
            net: a.net.clone(),
            shape: shape(&a),
            on: false,
        }),
        Command::Tidy(a) => tidy(&a),
    }
}

/// `routes tidy` (issue #1192): `netsmith_fabricate::routes_tidy::tidy_files` of the target's
/// board; exit 0 when the pass ran, 2 when it could not.
fn tidy(args: &TidyArgs) -> ExitCode {
    // As `fab`: with `--format json` the progress is printed only when it is asked for as JSON
    // (issue #1225).
    args.watching.tell_progress(
        "routes tidy",
        args.format == common::Format::Json
            && args.watching.progress == crate::progress::Print::Text,
    );
    let tidied = common::with_stage_events(|| {
        with_board(&args.target, |placed, board| {
            let sink = netsmith_fabricate::layout::tidy_progress(
                board.name.clone(),
                crate::progress::tell,
            );
            netsmith_core::job::reporting(sink, || {
                netsmith_fabricate::routes_tidy::tidy_files(
                    board,
                    &placed.layout,
                    args.reuse_routes,
                    args.dry_run,
                )
            })
        })
    });
    crate::progress::ended(
        tidied.is_ok(),
        if tidied.is_ok() {
            "routes tidy: done"
        } else {
            "routes tidy: failed (its output says why)"
        },
    );
    match tidied {
        Ok(tidied) => {
            match args.format {
                common::Format::Json => println!(
                    "{}",
                    serde_json::to_string_pretty(&tidied).expect("plain data")
                ),
                common::Format::Text => print!("{}", tidied.text()),
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

/// The places of the vias `add-via` adds: each `--at`, or the row `--count` and `--pitch` make
/// round the one `--at` (issue #1114).
fn via_places(args: &AddViaArgs) -> Result<Vec<[f64; 2]>, String> {
    match (args.count, args.pitch, args.at.as_slice()) {
        (Some(count), Some(pitch), &[at]) => {
            lib::via_row(at, count as usize, pitch).map_err(|e| e.to_string())
        }
        (Some(_), Some(_), _) => Err("`--count` and `--pitch` go round one `--at`; for several places give each with `--at` and no `--count`".to_string()),
        _ => Ok(args.at.clone()),
    }
}

fn shape(args: &ShapeArgs) -> Option<lib::Shape> {
    match (args.track, args.via, args.zone) {
        (Some(n), _, _) => Some(lib::Shape::Track(n)),
        (None, Some(n), _) => Some(lib::Shape::Via(n)),
        (None, None, Some(n)) => Some(lib::Shape::Zone(n)),
        (None, None, None) => None,
    }
}

fn pad_connection(pads: PadConnection) -> ZonePads {
    match pads {
        PadConnection::Solid => ZonePads::Solid,
        PadConnection::Thermal => ZonePads::Thermal,
        PadConnection::None => ZonePads::None,
        PadConnection::ThruHoleOnly => ZonePads::ThruHoleOnly,
    }
}

/// The outline of `add-zone`: the corners, the rectangle or the board's own.
fn zone_outline(args: &AddZoneArgs, board: &Board) -> Vec<[f64; 2]> {
    if let Some([x, y, w, h]) = args.rect {
        vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]]
    } else if args.board_outline {
        lib::board_outline(board)
    } else {
        args.outline.clone()
    }
}

/// The routes file of the placed board's source: beside its layout file.
fn routes_file(placed: &Placed) -> PathBuf {
    lib::beside_layout(&placed.layout)
}

/// `then` of the placed board (no routes on it) and its routes file. The error is printed.
fn with_board<T>(
    target: &Target,
    then: impl Fn(&Placed, &Board) -> Result<T, String>,
) -> Result<T, ExitCode> {
    common::with_placed_board(
        target.file.as_deref(),
        target.board.as_deref(),
        None,
        |placed| {
            let board = placed.with_board(|_, board| board.clone())?;
            then(placed, &board).map_err(|e| format!("error: {e}\n"))
        },
    )
}

fn show(args: &ShowArgs) -> ExitCode {
    let shown = with_board(&args.target, |placed, board| {
        lib::show(board, &lib::load(&routes_file(placed))?)
    });
    match shown {
        Ok(shown) => {
            match args.format {
                common::Format::Json => println!(
                    "{}",
                    serde_json::to_string_pretty(&shown).expect("plain data")
                ),
                common::Format::Text => print!("{}", shown.text()),
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

/// Make `change` to the routes file of the target's board (`netsmith_fabricate::routes::edit`,
/// which the editor's PCB view calls too): `change` is made from the placed board; exit 0 when
/// it is written, 1 when it is refused, 2 when it could not be tried.
fn edit(target: &Target, change: impl Fn(&Board) -> lib::Edit) -> ExitCode {
    edit_all(target, |board| vec![change(board)])
}

/// [`edit`] of several changes at once, checked together (`netsmith_fabricate::routes::edit_all`):
/// all are made or none.
fn edit_all(target: &Target, changes: impl Fn(&Board) -> Vec<lib::Edit>) -> ExitCode {
    let outcome = common::with_placed_board(
        target.file.as_deref(),
        target.board.as_deref(),
        None,
        |placed| {
            let board = placed.with_board(|_, board| board.clone())?;
            Ok(lib::edit_file_all(
                &board,
                &routes_file(placed),
                &changes(&board),
            ))
        },
    );
    match outcome {
        Ok(Ok(said)) => {
            println!("{said}");
            ExitCode::SUCCESS
        }
        Ok(Err(lib::Refusal::Refused(e))) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
        Ok(Err(lib::Refusal::Failed(e))) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
        Err(code) => code,
    }
}

/// `routes route` (issue #1195, `netsmith_fabricate::route_net`): exit 0 when the net is routed
/// (or would be, with `--dry-run`) or joined already, 1 when it is refused, 2 when it could not
/// be tried.
fn route(args: &RouteArgs) -> ExitCode {
    use netsmith_fabricate::route_net;
    let done = with_board(&args.target, |placed, board| {
        let file = routes_file(placed);
        let routes = lib::load(&file)?;
        let ask = match route_net::Ask::of(board, &args.net, &args.rip) {
            Ok(ask) => ask,
            Err(lib::Refusal::Refused(e)) => return Ok(Err(e)),
            Err(lib::Refusal::Failed(e)) => return Err(e),
        };
        let routed = route_net::route_net(board, &routes, &ask)?;
        let written = match (&routed.routes, args.dry_run) {
            (Some(r), false) => {
                lib::save(&file, r)?;
                true
            }
            _ => false,
        };
        Ok(Ok((routed, file, written)))
    });
    let (routed, file, written) = match done {
        Ok(Ok(done)) => done,
        Ok(Err(e)) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
        Err(code) => return code,
    };
    match args.format {
        common::Format::Json => {
            let mut doc = serde_json::to_value(&routed).expect("plain data");
            doc["dry_run"] = args.dry_run.into();
            doc["written"] = written.into();
            doc["file"] = file.display().to_string().into();
            println!(
                "{}",
                serde_json::to_string_pretty(&doc).expect("plain data")
            );
        }
        common::Format::Text => {
            print!("{}", routed.text());
            if written {
                println!("wrote {}", file.display());
            } else if args.dry_run && routed.routes.is_some() {
                println!("dry run: {} is not written", file.display());
            }
        }
    }
    match &routed.refused {
        Some(e) => {
            if args.format == common::Format::Text {
                eprintln!("error: {e}");
            }
            ExitCode::FAILURE
        }
        None => ExitCode::SUCCESS,
    }
}

fn import(args: &ImportArgs) -> ExitCode {
    let pcb = match fs::read_to_string(&args.pcb) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("error: {}: {e}", args.pcb.display());
            return ExitCode::from(2);
        }
    };
    let parts_file = args.parts.clone().or_else(|| {
        let root = netsmith_fabricate::project::Target::resolve(None)
            .ok()
            .and_then(|t| t.project.map(|p| p.root))?;
        Some(root.join("reference/parts.toml")).filter(|p| p.exists())
    });
    let parts = match &parts_file {
        Some(file) => match netsmith_tools::port::PortParts::read(file) {
            Ok(parts) => Some(parts),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        },
        None => None,
    };
    // First the design's board as it is placed now, for its parts, nets and paths.
    let imported = with_board(&args.target, |placed, board| {
        let import = routes_import::import(board, &pcb, parts.as_ref(), !args.unlocked)?;
        if !args.no_places {
            write_places(&placed.layout, board, &import)?;
        }
        Ok(import)
    });
    let import = match imported {
        Ok(i) => i,
        Err(code) => return code,
    };
    // Then placed again with those places, for each net's pads as they are.
    let written = with_board(&args.target, |placed, board| {
        let file = routes_file(placed);
        let mut routes = lib::load(&file).unwrap_or_default();
        routes
            .boards
            .insert(board.name.clone(), import.finish(board));
        lib::save(&file, &routes)?;
        Ok(file)
    });
    let file = match written {
        Ok(f) => f,
        Err(code) => return code,
    };
    let (tracks, arcs, vias) = import.counts();
    let zones = import.zones();
    match &parts_file {
        Some(p) => println!("parts paired by {}", p.display()),
        None => println!("parts paired by reference designator"),
    }
    for (ours, theirs, votes) in &import.mapped {
        if ours != theirs {
            println!("net {ours} is KiCad's {theirs} ({votes} pad(s))");
        }
    }
    for said in &import.refitted {
        println!("{said}");
    }
    for s in &import.skipped {
        println!("note: {s}");
    }
    println!(
        "wrote {}: {} net(s), {tracks} track(s), {arcs} arc(s), {vias} via(s), {zones} zone(s){}",
        file.display(),
        import.nets.len(),
        if args.no_places {
            String::new()
        } else {
            format!("; {} part place(s) in the layout file", import.places.len())
        }
    );
    ExitCode::SUCCESS
}

/// Write the imported places into the layout file, but not those of parts the physical file pins.
fn write_places(file: &Path, board: &Board, import: &Import) -> Result<(), String> {
    let name = file.display().to_string();
    let mut layout = match fs::read_to_string(file) {
        Ok(text) => Layout::parse(&text, &name).map_err(|e| e.to_string())?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Layout::default(),
        Err(e) => return Err(format!("{name}: {e}")),
    };
    let entries = layout.boards.entry(board.name.clone()).or_default();
    for (path, place) in &import.places {
        let pinned = board
            .parts
            .iter()
            .any(|p| &p.path == path && board.pinned.contains(&p.refdes));
        if !pinned {
            entries.parts.insert(path.clone(), place.clone());
        }
    }
    fs::write(file, netsmith_fabricate::layout::text_over(file, &layout))
        .map_err(|e| format!("{name}: {e}"))
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::Leaf;
    use crate::commands::common::testing::{kind, parse};
    use crate::commands::routes;
    use clap::error::ErrorKind;

    #[test]
    fn routes_takes_its_subcommands() {
        assert!(matches!(
            parse(&["routes", "import", "b.kicad_pcb", "--parts", "p.toml"]),
            Ok(Leaf::Routes(routes::Args {
                command: routes::Command::Import(routes::ImportArgs { parts: Some(_), .. })
            }))
        ));
        assert!(matches!(
            parse(&[
                "routes",
                "add-track",
                "--net",
                "GND",
                "--from",
                "1,2",
                "--to",
                "-3,4.5",
                "--layer",
                "bottom"
            ]),
            Ok(Leaf::Routes(routes::Args {
                command: routes::Command::AddTrack(routes::AddTrackArgs {
                    from: [1.0, 2.0],
                    to: [-3.0, 4.5],
                    layer: netsmith_pcb::CopperLayer::Back,
                    ..
                })
            }))
        ));
        assert!(matches!(
            parse(&[
                "routes",
                "add-via",
                "--net",
                "GND",
                "--at",
                "1,1",
                "--layers",
                "inner4,bottom"
            ]),
            Ok(Leaf::Routes(routes::Args {
                command: routes::Command::AddVia(routes::AddViaArgs {
                    layers: Some([
                        netsmith_pcb::CopperLayer::Inner(4),
                        netsmith_pcb::CopperLayer::Back
                    ]),
                    ..
                })
            }))
        ));
        assert!(matches!(
            parse(&[
                "routes", "delete", "--net", "GND", "--via", "2", "--file", "a.nsm"
            ]),
            Ok(Leaf::Routes(routes::Args {
                command: routes::Command::Delete(routes::ShapeArgs { via: Some(2), .. })
            }))
        ));
        assert!(matches!(
            parse(&[
                "routes", "add-zone", "--net", "GND", "--layer", "top", "--layer", "bottom",
                "--rect", "1,-2,3,4.5", "--pads", "thru-hole-only", "--priority", "2"
            ]),
            Ok(Leaf::Routes(routes::Args {
                command: routes::Command::AddZone(routes::AddZoneArgs {
                    rect: Some([1.0, -2.0, 3.0, 4.5]),
                    priority: 2,
                    pads: routes::PadConnection::ThruHoleOnly,
                    ref layer,
                    ..
                })
            })) if layer == &["top", "bottom"]
        ));
        assert!(matches!(
            parse(&[
                "routes", "add-zone", "--net", "GND", "--outline", "1,1", "--outline", "5,1",
                "--outline", "3,4"
            ]),
            Ok(Leaf::Routes(routes::Args {
                command: routes::Command::AddZone(routes::AddZoneArgs { ref outline, .. })
            })) if outline.len() == 3
        ));
        assert!(matches!(
            parse(&["routes", "unlock", "--net", "GND", "--zone", "2"]),
            Ok(Leaf::Routes(routes::Args {
                command: routes::Command::Unlock(routes::ShapeArgs { zone: Some(2), .. })
            }))
        ));
        assert_eq!(
            kind(&[
                "routes", "lock", "--net", "GND", "--track", "1", "--via", "1"
            ]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(
            kind(&[
                "routes", "lock", "--net", "GND", "--via", "1", "--zone", "1"
            ]),
            ErrorKind::ArgumentConflict
        );
        // One way to say the outline, and one is needed; a rectangle has a size.
        assert_eq!(
            kind(&["routes", "add-zone", "--net", "GND"]),
            ErrorKind::MissingRequiredArgument
        );
        assert_eq!(
            kind(&[
                "routes",
                "add-zone",
                "--net",
                "GND",
                "--rect",
                "1,1,2,2",
                "--board-outline"
            ]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(
            kind(&["routes", "add-zone", "--net", "GND", "--rect", "1,1,0,2"]),
            ErrorKind::ValueValidation
        );
        assert_eq!(
            kind(&["routes", "add-via", "--net", "GND", "--at", "1"]),
            ErrorKind::ValueValidation
        );
        // A group of vias (issue #1114): several places, or a count with a pitch.
        assert!(matches!(
            parse(&[
                "routes", "add-via", "--net", "GND", "--at", "1,1", "--at", "2,1", "--at", "-3,1"
            ]),
            Ok(Leaf::Routes(routes::Args {
                command: routes::Command::AddVia(routes::AddViaArgs { ref at, count: None, pitch: None, .. })
            })) if at == &[[1.0, 1.0], [2.0, 1.0], [-3.0, 1.0]]
        ));
        assert!(matches!(
            parse(&[
                "routes", "add-via", "--net", "GND", "--at", "1,1", "--count", "3", "--pitch",
                "0.45"
            ]),
            Ok(Leaf::Routes(routes::Args {
                command: routes::Command::AddVia(routes::AddViaArgs {
                    count: Some(3),
                    pitch: Some(0.45),
                    ..
                })
            }))
        ));
        for args in [
            &["--at", "1,1", "--count", "3"][..],
            &["--at", "1,1", "--pitch", "0.45"],
        ] {
            let mut all = vec!["routes", "add-via", "--net", "GND"];
            all.extend_from_slice(args);
            assert_eq!(kind(&all), ErrorKind::MissingRequiredArgument, "{args:?}");
        }
        for bad in [
            &["--count", "0", "--pitch", "1"][..],
            &["--count", "2", "--pitch", "0"],
            &["--count", "2", "--pitch", "-1"],
        ] {
            let mut all = vec!["routes", "add-via", "--net", "GND", "--at", "1,1"];
            all.extend_from_slice(bad);
            assert_eq!(kind(&all), ErrorKind::ValueValidation, "{bad:?}");
        }
        assert_eq!(
            kind(&["routes", "add-via", "--net", "GND"]),
            ErrorKind::MissingRequiredArgument
        );
        // `tidy` (issue #1192).
        assert!(matches!(
            parse(&[
                "routes",
                "tidy",
                "--dry-run",
                "--reuse-routes",
                "--format",
                "json"
            ]),
            Ok(Leaf::Routes(routes::Args {
                command: routes::Command::Tidy(routes::TidyArgs {
                    dry_run: true,
                    reuse_routes: true,
                    format: crate::commands::common::Format::Json,
                    ..
                })
            }))
        ));
        // Its progress, as `fab`'s (issue #1225).
        let Ok(Leaf::Routes(routes::Args {
            command: routes::Command::Tidy(t),
        })) = parse(&["routes", "tidy", "--progress", "json", "--to-editor"])
        else {
            panic!("`routes tidy --progress json --to-editor` parses");
        };
        assert_eq!(t.watching.progress, crate::progress::Print::Json);
        assert_eq!(t.watching.to_editor, Some(8787));
        // A blind via's two layers (issue #1071): two different layers.
        for bad in ["top", "top,top", "top,middle"] {
            assert_eq!(
                kind(&[
                    "routes", "add-via", "--net", "GND", "--at", "1,1", "--layers", bad
                ]),
                ErrorKind::ValueValidation,
                "{bad}"
            );
        }
    }
}
