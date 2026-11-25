//! `netsmith routes` (epic #599): the routes file beside a source (`<stem>.routes.toml`, ADR
//! 0030), the copper an agent or a person fixes so that a rebuild keeps it. Its format and what
//! its copper does are in the language guide's "The routes file" and the routes module of
//! `netsmith-pcb`; the file beside a source is `netsmith_fabricate::routes`, the import of a KiCad
//! board `netsmith_tools::routes_import`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args as ClapArgs, Subcommand};
use netsmith_core::timings;
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
        after_long_help = "Every track, arc and through via of F.Cu, B.Cu and the inner signal layers becomes the copper of the design's net its pads are on: a KiCad net is the design's net that most of its pads are on, with the parts paired by a port's parts file (`--parts`, default `reference/parts.toml` of the project when there is one; without one, by reference designator). A KiCad net on no pad of a part the design has keeps its name if the design has a net of that name, and is left out otherwise; copper on an inner layer is left out unless the layer is one of the process's signal layers (an inner plane holds no track); each is said. Each zone of copper that is not the process's own plane or pour becomes a zone of its net's record, on its layers, with its outline, holes and the settings that change its copper (priority, clearance, minimum thickness, pad connection, thermal gap and spoke width, island removal); such a zone of the net of the design's pour or plane on its layer restates it, which is not drawn, unless the KiCad board also has the process's own zone of that net on that layer, which the import leaves out: then the zone is above the pour there and is taken as a patch of it (`patch = true`), which keeps the pour; one of another net cuts it (`fab` writes the zones one priority above the pour, and the import takes that one off again). A zone that is the process's own plane or pour is left out and said, so that `fab` and then `routes import` of its board leaves a file of tracks and vias: one with the same net, layer and outline, and the same clearance, minimum width, pad connection and thermal relief as the process states (what `fab` writes), or one with the uuid `fab` gives the process's zone of its layer, which KiCad keeps (a pour `fab` wrote and changed since, in KiCad or by a process that states other settings now, is still the process's, not a hand zone); `--process-zones` takes them as hand zones instead. Rule areas (the routes file holds copper only), teardrops and hatched zones are left out and said. The text counts the zones taken and those left out by reason (`zones: 2 taken, 3 left out (2 of the process's own planes and pours, 1 rule area(s))`), and with `--format json` `zones_left_out` names each zone left out (`net`, `layers`, `reason`: `process`, `process_changed`, `rule_area`, `teardrop`, `hatched`, `not_copper`, `no_net` or `unreadable`, and `said`). The stitching vias of a pour (a through via of the pour's net and the rules' size near a point of its grid, in no pad, no track ending on it) are left out with `--unlocked`, as `fab` lays them again, and kept when the nets are locked; with `--format json`, `stitching_vias_left_out` names each one left out (`net`, `at`), and `routes show` counts them as the stitching vias `fab` lays.\n\nEach part of the design that the KiCad board has is written to the layout file at the KiCad board's place, turn and face, locked (`source = \"import\"`), unless `--no-places` (which also leaves out the notes of how a part is placed, turned or moved, and prints `places not imported: N` instead), or the physical file pins it (its requirement places it). Places are from the top left corner of the KiCad board's Edge.Cuts box. Then the board is placed again and each net's record takes its pads as they are. Every net is locked (the router adds nothing to it) unless `--unlocked`. The routes file is written over; the layout file keeps its seed, keepouts and the parts the import does not place."
    )]
    Import(ImportArgs),
    /// Print the routes file's copper and what the router would still route.
    #[command(
        after_long_help = "Each record of the board is printed with its tracks, vias and zones numbered from 1, as `delete`, `lock` and `unlock` name them. A net with a stitched pour also has the count of the stitching vias `fab` lays on the pour's grid that the file does not have (`stitching` in the JSON, as `view pcb --stitching` finds them), so the file's vias and these are its vias on the board whether its copper was imported locked or `--unlocked`; a stitched net with no record is listed with those alone. Then what of the file is not used, the connections the router would still route, and those of locked nets that stay unrouted. With `--format json`: `board`, `nets` (each with `net`, `lock`, `tracks`, `vias`, `zones` and `stitching`), `notes`, `to_route` and `locked_left`."
    )]
    Show(ShowArgs),
    /// Add many tracks, vias and zones from a file or stdin, each checked as its verb checks it,
    /// and write the routes file once.
    #[command(
        after_long_help = "The document is TOML (`[[track]]`, `[[via]]` and `[[zone]]` tables) or JSON (the arrays `track`, `via` and `zone`), by the file's extension, `.toml` or `.json`; `-` reads it from stdin, JSON when it starts with `{`. Each entry has the fields of the flags of `add-track` (`net`, `layer`, `width`, `from`, `to`, `mid`, `lock`), `add-via` (`net`, `at` as one place or a list of places for a group, `count`, `pitch`, `diameter`, `drill`, `layers` as two words, `lock`) and `add-zone` (`net`, `layer` as a word or a list, one of `outline`, `rect` and `board_outline`, `clearance`, `min_width`, `thermal_gap`, `spoke_width`, `priority`, `pads`, `patch`, `lock`), with the same defaults; a place is `[x, y]` in mm from the outline's top left corner, y down.\n\nThe entries are checked one after another, the tracks, then the vias, then the zones, each in the document's order, on the board with the shapes made before them, as the verb checks its shape (where it is: issue #1471). Up to 16 in a row are checked together: a group the check finds no new error with is made whole, and one it does is halved until each refused shape is found alone, so a shape is refused when it is refused alone after the shapes before it (an error that a later shape of its group takes away is not counted). A shape refused (a short, a crossing, a clearance, a word that is not a layer) is skipped and said, with the findings that refused it, and the rest go on; with `--stop-on-refusal` the batch stops at the first refusal and keeps the shapes made before it. The routes file is written once, at the end, with every shape made (with `--dry-run`, not at all). The exit code is 0 when every shape was made, 1 when one was refused (the others are written), 2 when it could not run (a document that cannot be read, a board that cannot be built).\n\nWith `--format json`: `board`, `shapes` (one for each entry: `kind`, `index` from 1 among its kind's, `net`, `result` `ok`, `refused` or `not_tried`, `said`, and for a refusal by the design rules check `findings`, each with `rule`, `message`, `at`, `layer` and `shapes`), `made`, `refused`, `not_tried`, `stopped`, `checks`, `items_checked`, `written`, `dry_run` and `file`."
    )]
    Add(AddArgs),
    /// Add a track to a net, refused if it breaks the design rules.
    #[command(name = "add-track")]
    AddTrack(AddTrackArgs),
    /// Add a via, or a group of vias side by side, to a net, refused if it breaks the design rules.
    #[command(
        name = "add-via",
        after_long_help = "One via is checked alone: for a net whose current needs several vias side by side where it changes layer, a lone via is refused by the design rules check (`via_current`). A group is added and checked together, as the router's own group is, and is accepted or refused as a whole (the file is then as it was). Name the places of a group with `--at X,Y` once for each via, or give `--count N --pitch MM` with one `--at`: N vias in a row along x, PITCH mm between their centres, centred on that place. The vias of a group count as side by side when their copper is no more than the group gap apart (the process's least clearance, or the board's `.process.via_group_gap`), so a pitch above the via's diameter and that gap leaves them apart (and the check says so). All vias of a group share the net, `--diameter`, `--drill`, `--layers` and `--lock`; a group has at most 64 vias."
    )]
    AddVia(AddViaArgs),
    /// Add a zone (copper area) of a net on a layer, refused if it breaks the design rules.
    #[command(
        name = "add-zone",
        after_long_help = "The outline is a polygon given by `--outline X,Y` once for each corner (three or more, in order), by `--rect X,Y,W,H` for a rectangle, or by `--board-outline` for the board's whole outline. Places are mm from the outline's top left corner, y down, as for `add-track`. The zone is filled inside the outline on each `--layer` (`top`, `bottom`, `inner1`, ...; repeat the flag for several), joined to its net; where two zones of a layer overlap the higher `--priority` has the area, and two zones of different nets with the same priority must not overlap. Without a setting the zone has the design's own plane's: the clearance the process's planes keep, a minimum width of 0.25 mm, thermal relief gap and spokes of 0.5 mm, priority 0, a thermal relief at each pad of its net.\n\nA zone of the net of the design's pour or plane on its layer restates it: the pour or plane there is not drawn. With `--patch` it is a patch of it instead (a pad joined solid in a thermal pour, a wider clearance in one place): the pour or plane is kept, and the zone's settings (`--pads`, `--clearance`, `--thermal-gap`, `--spoke-width`) hold inside its outline alone, so the pour need not be restated; `--patch` changes nothing on a layer with no pour or plane of the zone's net. A zone of another net cuts it: the zone has the area it covers, at any priority, and the pour is drawn round it, keeping its clearance (`fab` writes the routes file's zones one priority above the pour so that KiCad's fill agrees). The command says which it is. A zone is refused when a corner is off the board, a setting is wrong, or the design rules check finds an error with it that the board did not have (a short, a clearance, a track of another net it covers); the file is then as it was. The zone is written to the net's record (made, with the net's pad hash, if the net has none); `routes show` numbers it, and `delete`, `lock` and `unlock` name it with `--zone N`."
    )]
    AddZone(AddZoneArgs),
    /// Route one net, or one connection of it, round the copper there; say which nets stood in
    /// its way, and with `--rip` route them again after it.
    #[command(
        after_long_help = "The router routes the net (or the connection, `NET:REF.PAD-REF.PAD`: the lines that join those two pads alone) on the board as placed with the routes file's copper, every other net held as it is, and writes its tracks and vias to the net's record, unlocked. A connection joined already is said, and nothing is written. The text says the narrowest place of the new way and the widest track of the net that has room there, with every space of the design rules.\n\nWhen the net has no way round the copper there, it is routed again with the unlocked tracks and vias of the other nets taken away, and the nets whose copper that way crosses are named (exit 1, nothing written); where there is no way even so, what walls it in is said: pads, vias, the copper of locked nets, the edge. With `--rip NET,...` the copper of those nets alone may be taken away: it is removed, the net is routed through the room it left, and each ripped net is routed again after it (it may rip in turn, from the nets named). A locked net or a locked shape is never ripped, nor is a zone. The rounds are bounded by counts, those of the router's rounds of rip-up: 6 rounds, 3 rips of one net, 64 runs of the router. A ripped net left with more connections open than it had, or a result the design rules check finds a new error with, is refused and the file is as it was. `--dry-run` says what it would do and writes nothing."
    )]
    Route(RouteArgs),
    /// Join a pad the fill leaves unjoined to the fill of its net with a short track, or a track
    /// and a via, in the direction with room.
    #[command(
        name = "join-pad",
        after_long_help = "The pad (`REF.PAD`) is joined when a fill of its net that covers its centre joins it (by a spoke or a track, and not starved: `board fill` says how) or another fill of its net does, or, when no fill covers it, when the design rules check finds it connected; a joined pad is said and nothing is written. Otherwise a track of the net's own width is drawn from the pad's centre straight out, in each of 16 directions, to the first place (0.025 mm at a time, at most 5 mm out) where its round end lies wholly on the fill of its net on that layer, as `board fill --format json` outlines it; the shortest is tried first, and each is checked as `add-track` checks a track. When no track fits, a track to a through via of the process placed on the fill of its net on another layer, clear of the pad. The first one the design rules check takes and that joins the pad is written to the net's record, unlocked. At most 24 are tried; when none is taken the command is refused (exit 1, nothing written) and says how many were tried and why the shortest was refused. `--dry-run` says what it would add and writes nothing. With `--format json`: `board`, `pad`, `net`, `before` (how the fill had the pad), `already`, `join` (`layer`, `from`, `to`, `width`, `length`, `direction` in degrees counterclockwise from +x, and `via` and `fill_layer` with a via), `tried` (`layer`, `direction`, `length`, `via`, `why`), `refused`, `said`, `dry_run`, `written` and `file`."
    )]
    JoinPad(JoinPadArgs),
    /// List the places between a part's pads where a via fits, and which net's copper holds each.
    #[command(after_long_help = super::routes_sites::LONG_HELP)]
    Sites(super::routes_sites::Args),
    /// Say whether a via fits at a place, and what is in its way by how much.
    #[command(name = "check-via", after_long_help = super::routes_check_via::LONG_HELP)]
    CheckVia(super::routes_check_via::Args),
    /// Move a via of the routes file and carry every layer's tracks that end in it, laid again in
    /// 45 degree legs, refused if it breaks the design rules or loses a connection.
    #[command(name = "move-via", after_long_help = super::routes_move_via::LONG_HELP)]
    MoveVia(super::routes_move_via::Args),
    /// Delete a net's record, or one of its shapes; a locked one is refused. A record of a net the board no longer has (a net renamed, say: `fab` names it) is deleted whole, even when locked.
    Delete(ShapeArgs),
    /// Lock a net (the router adds no copper to it) or one shape (kept when the net's pads move).
    #[command(
        after_long_help = "Lock also writes the net's record again with its pads as they are now, so a record whose pads moved (fab's warning, which keeps its copper and says where it is open) is current again and its net lock holds; it is refused only when the design rules check finds an error of that record's own copper where it ran to a pad that moved (an error that names a track, via or zone of the net and lies within the moved pad's size and the board's largest clearance of where fab's warning says the copper is open: a track that ran to a moved pad now crossing another net's pad, say), and the refusal lists those errors; other errors of the net or the board do not stop it, and a record whose pads did not move is never refused for errors."
    )]
    Lock(ShapeArgs),
    /// Unlock a net or one shape.
    Unlock(ShapeArgs),
    /// Cut corners to 45 degrees, merge collinear tracks, remove dangling vias, stubs and acid
    /// traps, and with `--widen` widen narrow tracks, refusing any change that adds a finding or
    /// loses a connection.
    #[command(
        after_long_help = "A pass over the routes file's copper (and with `--reuse-routes` the router's copper kept in `.netsmith/routes/`, as `board fab --reuse-routes` starts from it) that makes four repairs (five with `--widen`):\n\n- a corner: two straight tracks of a net that meet outside its pads and bend by more than 45 degrees (the design rules check's `track_angle`) are cut into bends of 45 degrees at most, a 45 degree chamfer at a right angle; the chamfer starts four widths from the corner, and two and one width when the larger is refused;\n- a collinear pair: two straight tracks of one net, layer and width that meet end to end on one line, with nothing else there, become one;\n- a dangling via or a stub: a via joined to copper of its net on fewer than two layers (`via_dangling`) and a track with an end that touches nothing (`track_dangling`) are removed when the copper makes every connection of the net (the router may still start from a stub of a net it has to finish), and a via that joins nothing at all is removed in any net;\n- an acid trap (`acid_trap`): a track that leaves a pad or a via at an acute angle to its edge is made to leave it at a right angle and bend after the edge, and a track that ends on the middle of another at an acute angle is made to meet it at a right angle.\n\nWith `--widen`, a fifth: a narrow track, a straight track narrower than the width its net wants on its layer (the width the router draws the net at there: for its current, its class or its impedance target, and for a net with pins that state the current of their branch the narrowest of its lines), is widened to it; a widening the check refuses (the wider track comes too near copper of another net) is tried at three quarters, half and a quarter of the way, so that each track is as wide as the clearance to its neighbours allows. A track is never made narrower; the text lists each widening with the width it had, the width it has and the width its net wants (`counts.widened` in the JSON, and each change's `width_before` and `width_after`).\n\nWith `--widen-to-guideline`, what `--widen` does, and each track of a net whose path drops more than its track-drop budget (the `track drop` guideline of `board fab`, the path `board nets` lists) is widened towards the width the guideline suggests for it (the path's `needs`), by the same steps and the same rule; a track with room for none of the steps is tried at the width its net wants. The drop is measured again before each pass, and the text ends with the drop of each net that was over, before and after (`drops` in the JSON). A net within its budget is widened as `--widen` widens it.\n\nEach change is made only when the design rules check of the board with it (the check `add-track` makes, with its warnings) finds nothing it did not find before and the connections left to route are the same; a refused change is listed with the first new finding or the connection it changes, and left as it was. The changes of each net are checked together where they are: over the copper, pads and zones within the board's largest space of what they change, with and without them, and the whole board is checked once after each pass, which checks the pass's changes again against the whole board when it finds what they add beyond where they are. The text ends with how many checks ran and the items they looked at (`checks` and `items_checked` in the JSON). A locked net or shape, an arc, a zone and a record whose pads moved are never touched. `--dry-run` says what would change, by kind, net, layer and place, and writes nothing. With `--reuse-routes` the kept file is written again, so that `board fab --reuse-routes` still starts from it; without it, a change to the routes file makes a kept file stale, and the command says so. The exit code is 0 when the pass ran (whether or not it changed or refused anything), 2 when it could not run.\n\nOn stderr each pass says how far it has got, net by net: the net, how many of the pass's nets, the changes made so far and about how long the pass has left; `--progress json` prints each line as `{\"board\", \"stage\": \"tidying\", \"unit\": \"nets tidied\", \"done\", \"total\", \"detail\", \"elapsedMs\", \"message\"}`, and `--to-editor` sends them to a running editor, as `board fab` does. With `--format json` the progress is printed only with `--progress json`. Nothing is written until the whole pass has run, so Ctrl+C leaves the files as they were."
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
    /// Widen each track narrower than its net wants, as far as the clearance to its neighbours
    /// allows.
    #[arg(long)]
    pub widen: bool,
    /// As `--widen`, and widen each track of a net whose path drops more than its track-drop
    /// budget towards the width the guideline suggests, as far as the clearance allows.
    #[arg(long)]
    pub widen_to_guideline: bool,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
    #[command(flatten)]
    pub framing: crate::frame::Framing,
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
    /// Write no places to the layout file, and say nothing of how each part would be placed (a
    /// count of the places not imported instead).
    #[arg(long)]
    pub no_places: bool,
    /// Leave every net unlocked: the router may add copper to complete it.
    #[arg(long)]
    pub unlocked: bool,
    /// Take the zones that are the process's own planes and pours (as `fab` wrote them, changed
    /// since or not) as hand zones too, instead of leaving them out.
    #[arg(long)]
    pub process_zones: bool,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
    #[command(flatten)]
    pub framing: crate::frame::Framing,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct AddArgs {
    /// The shapes: a `.toml` or `.json` file, or `-` for stdin.
    #[arg(value_name = "SHAPES")]
    pub shapes: String,
    #[command(flatten)]
    pub target: Target,
    /// Stop at the first shape refused (the shapes made before it are written).
    #[arg(long)]
    pub stop_on_refusal: bool,
    /// Check every shape and write nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
    #[command(flatten)]
    pub framing: crate::frame::Framing,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct ShowArgs {
    #[command(flatten)]
    pub target: Target,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
    #[command(flatten)]
    pub framing: crate::frame::Framing,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct RouteArgs {
    /// The net, or one connection of it: `NET` or `NET:REF.PAD-REF.PAD` (`VCC:U1.3-C2.1`).
    #[arg(value_name = "NET[:PAD-PAD]", required_unless_present = "region")]
    pub net: Option<String>,
    #[command(flatten)]
    pub target: Target,
    /// Rip up the unlocked copper inside a box and route every connection it carried again,
    /// `X,Y,W,H` in mm (the outline's frame, or the one `--frame` names).
    ///
    /// Tracks crossing the box's edge are cut there and their outside part kept; the nets whose
    /// connections the cut opens, and those with a connection open before through the box, are
    /// routed together by the router's negotiation round the copper outside. Written only when
    /// no connection is left open that was made before and the copper costs no more by the
    /// weights (or fewer connections are open).
    #[arg(long, value_name = "X,Y,W,H", value_parser = parse_rect, allow_hyphen_values = true, conflicts_with_all = ["net", "rip", "reroute", "via"])]
    pub region: Option<[f64; 4]>,
    /// Nets whose unlocked tracks and vias may be removed and routed again after it, `A,B`.
    #[arg(long, value_name = "NET,...", value_delimiter = ',')]
    pub rip: Vec<String>,
    /// How deep a ripped net may rip in turn (default 1).
    ///
    /// The nets `NET` rips are at depth 1, the nets one of them rips (when it finds no way back)
    /// at depth 2, and so on; a ripped net rips in turn only while its depth is at most N, so 0
    /// lets no ripped net rip, and 1 lets the nets `NET` rips rip once more.
    #[arg(long, value_name = "N", default_value_t = netsmith_fabricate::route_net::RIP_DEPTH)]
    pub rip_depth: usize,
    /// The most orders the ripped nets are routed again in, at least 1 (default 3).
    ///
    /// When a ripped net cannot be put back, they are routed again from the start with the nets
    /// that were not put back first; each try is reported, and each may run the router 64 times.
    #[arg(long, value_name = "N", default_value_t = netsmith_fabricate::route_net::TRIES, value_parser = super::route::parse_tries)]
    pub tries: usize,
    /// How the bends of the copper it lays turn, for this run: `rounded` (arcs, where they keep
    /// the rules' space) or `mitered` (45 degree corners). Default: the board's `.corner`.
    ///
    /// A net of a `TrackPath` whose `.corner` is `Mitered` is mitered whatever this says.
    #[arg(long, value_enum, value_name = "STYLE")]
    pub corner: Option<Corner>,
    #[command(flatten)]
    pub weighing: common::Weighing,
    /// Lay the net again, with the nets `--rip` names, the net first.
    ///
    /// Its unlocked tracks and vias and those of the nets `--rip` names are taken away, the net
    /// is routed first and they after it (in other orders, as `--tries` says, when one is not put
    /// back), and the change is written only when every connection is made and their total by
    /// the weights (length, vias and layer changes) gets shorter by more than 0.5 mm. For a net
    /// that went round its neighbour because the neighbour was laid first.
    #[arg(long)]
    pub reroute: bool,
    /// Route the net through a via at `X,Y`, or a blind one at `X,Y,LAYER,LAYER`; repeatable.
    ///
    /// Places are mm in the routes file's frame (`--frame` changes only what is printed); a
    /// blind via names the two layers it ends on, `3,4,top,inner1`. The net's via at that place is used when its record has one; else the via is added, of
    /// the process's size, and refused, with nothing written, when the design rules check finds
    /// an error with it (the refusal names what blocks it). Each pad of the net (of the
    /// connection, for `NET:PAD-PAD`) is joined to the nearest of the vias first, then the rest
    /// of the net is routed. Not taken with `--rip`, `--reroute` or `--region`.
    #[arg(long = "via", value_name = "X,Y[,LAYER,LAYER]", value_parser = parse_via, allow_hyphen_values = true, action = clap::ArgAction::Append, conflicts_with_all = ["rip", "reroute"])]
    pub via: Vec<netsmith_fabricate::route_net::ViaAsked>,
    /// Say what it would do, and write nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
    #[command(flatten)]
    pub framing: crate::frame::Framing,
}

/// A via of `routes route --via` (issue #1642): `X,Y` or `X,Y,LAYER,LAYER`.
fn parse_via(text: &str) -> Result<netsmith_fabricate::route_net::ViaAsked, String> {
    let parts: Vec<&str> = text.split(',').collect();
    let layers = match parts.len() {
        2 => None,
        4 => Some(parse_layer_pair(&format!("{},{}", parts[2], parts[3]))?),
        _ => {
            return Err(format!(
                "`{text}` is not `X,Y` or `X,Y,LAYER,LAYER` (`3,4` or `3,4,top,inner1`)"
            ));
        }
    };
    Ok(netsmith_fabricate::route_net::ViaAsked {
        at: parse_point(&format!("{},{}", parts[0], parts[1]))?,
        layers,
    })
}

/// How tracks turn (`--corner`, issue #1508): the variants of `std::sources::TrackCorner`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Corner {
    Rounded,
    Mitered,
}

impl Corner {
    fn track_corner(self) -> netsmith_core::TrackCorner {
        match self {
            Corner::Rounded => netsmith_core::TrackCorner::Rounded,
            Corner::Mitered => netsmith_core::TrackCorner::Mitered,
        }
    }
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct JoinPadArgs {
    /// The pad, `REF.PAD` (`C1.2`).
    #[arg(value_name = "REF.PAD")]
    pub pad: String,
    #[command(flatten)]
    pub target: Target,
    /// Say what it would add, and write nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
    #[command(flatten)]
    pub framing: crate::frame::Framing,
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
    /// A patch of the pour or plane of the zone's net on its layer: the pour is kept and the
    /// zone's settings (`--pads`, `--clearance`, `--thermal-gap`, `--spoke-width`) hold inside
    /// its outline alone. Without it, a zone of that net replaces the pour on the layer.
    #[arg(long)]
    pub patch: bool,
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

pub(super) fn parse_layer_pair(text: &str) -> Result<[CopperLayer; 2], String> {
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
        Command::Add(a) => add(&a),
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
            patch: a.patch,
            lock: a.lock,
        }),
        Command::Route(a) => route(&a),
        Command::JoinPad(a) => join_pad(&a),
        Command::Sites(a) => super::routes_sites::run(&a),
        Command::CheckVia(a) => super::routes_check_via::run(&a),
        Command::MoveVia(a) => super::routes_move_via::run(&a),
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

/// How far (`[dx, dy]`, mm) the places of a text move from the routes file's frame to the one
/// asked for (`--frame`).
fn text_by(framing: &crate::frame::Framing) -> Result<[f64; 2], ExitCode> {
    match framing.moved(
        crate::frame::Frame::Outline,
        Some(crate::frame::design_corner()),
    ) {
        Ok((_, dx, dy)) => Ok([dx, dy]),
        Err(e) => Err(crate::frame::refused(&e)),
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
    use netsmith_fabricate::routes_tidy::Widen;
    let tidied = common::with_stage_events(|| {
        common::with_placed_board(
            args.target.file.as_deref(),
            args.target.board.as_deref(),
            None,
            |placed| {
                let (board, layout, voltages) = placed.with_board(|built, board| {
                    (
                        board.clone(),
                        placed.layout_of(built).file,
                        netsmith_fabricate::track_drop::rail_voltages(&built.trace.model),
                    )
                })?;
                let widen = if args.widen_to_guideline {
                    Widen::ToGuideline(voltages)
                } else if args.widen {
                    Widen::ToNet
                } else {
                    Widen::Off
                };
                let sink = netsmith_fabricate::layout::tidy_progress(
                    board.name.clone(),
                    crate::progress::tell,
                );
                netsmith_core::job::reporting(sink, || {
                    netsmith_fabricate::routes_tidy::tidy_files(
                        &board,
                        &layout,
                        args.reuse_routes,
                        widen,
                        args.dry_run,
                    )
                })
                .map_err(|e| format!("error: {e}\n"))
            },
        )
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
                common::Format::Json => {
                    if let Err(e) = args.framing.print(
                        &tidied,
                        crate::frame::Frame::Outline,
                        Some(crate::frame::design_corner()),
                        true,
                    ) {
                        return crate::frame::refused(&e);
                    }
                }
                common::Format::Text => {
                    let by = match text_by(&args.framing) {
                        Ok(by) => by,
                        Err(code) => return code,
                    };
                    print!("{}", tidied.text_moved(by));
                }
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

/// The routes file of a board whose layout file is `layout`: beside it.
fn routes_file(layout: &Path) -> PathBuf {
    lib::beside_layout(layout)
}

/// The placed board (no routes on it) and its layout file ([`Placed::layout_of`]: a board of a
/// system defined in another file has its own, issue #1327).
fn board_and_layout(placed: &Placed) -> Result<(Board, PathBuf), String> {
    placed.with_board(|built, board| (board.clone(), placed.layout_of(built).file))
}

/// `then` of the placed board (no routes on it) and its layout file, beside which is its routes
/// file. The error is printed.
fn with_board<T>(
    target: &Target,
    then: impl Fn(&Path, &Board) -> Result<T, String>,
) -> Result<T, ExitCode> {
    common::with_placed_board(
        target.file.as_deref(),
        target.board.as_deref(),
        None,
        |placed| {
            let (board, layout) = board_and_layout(placed)?;
            then(&layout, &board).map_err(|e| format!("error: {e}\n"))
        },
    )
}

fn show(args: &ShowArgs) -> ExitCode {
    let shown = with_board(&args.target, |layout, board| {
        lib::show(board, &lib::load(&routes_file(layout))?)
    });
    match shown {
        Ok(shown) => {
            match args.format {
                common::Format::Json => {
                    if let Err(e) = args.framing.print(
                        &shown,
                        crate::frame::Frame::Outline,
                        Some(crate::frame::design_corner()),
                        true,
                    ) {
                        return crate::frame::refused(&e);
                    }
                }
                common::Format::Text => {
                    let (_, dx, dy) = match args.framing.moved(
                        crate::frame::Frame::Outline,
                        Some(crate::frame::design_corner()),
                    ) {
                        Ok(moved) => moved,
                        Err(e) => return crate::frame::refused(&e),
                    };
                    print!("{}", shown.text_moved([dx, dy]));
                }
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
            let (board, layout) = board_and_layout(placed)?;
            Ok(timings::stage("routes edit", &board.name, || {
                lib::edit_file_all(&board, &routes_file(&layout), &changes(&board))
            }))
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

/// `routes add SHAPES` (issue #1472, `netsmith_fabricate::routes::batch`): exit 0 when every
/// shape was made, 1 when one was refused, 2 when it could not run.
fn add(args: &AddArgs) -> ExitCode {
    use netsmith_fabricate::routes::batch;
    let text = if args.shapes == "-" {
        let mut text = String::new();
        match std::io::Read::read_to_string(&mut std::io::stdin(), &mut text) {
            Ok(_) => text,
            Err(e) => {
                eprintln!("error: stdin: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        match fs::read_to_string(&args.shapes) {
            Ok(text) => text,
            Err(e) => {
                eprintln!("error: {}: {e}", args.shapes);
                return ExitCode::from(2);
            }
        }
    };
    let name = if args.shapes == "-" {
        "stdin"
    } else {
        &args.shapes
    };
    let doc =
        match batch::Document::parse(&text, name, batch::Document::is_json(&args.shapes, &text)) {
            Ok(doc) => doc,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        };
    let done = with_board(&args.target, |layout, board| {
        let file = routes_file(layout);
        let mut routes = lib::load(&file)?;
        let added = timings::stage("routes add", &board.name, || {
            batch::add(board, &mut routes, &doc, args.stop_on_refusal)
        })?;
        let written = added.made > 0 && !args.dry_run;
        if written {
            lib::save(&file, &routes)?;
        }
        Ok((added, file, written))
    });
    let (added, file, written) = match done {
        Ok(done) => done,
        Err(code) => return code,
    };
    match args.format {
        common::Format::Json => {
            let mut doc = serde_json::to_value(&added).expect("plain data");
            doc["written"] = written.into();
            doc["dry_run"] = args.dry_run.into();
            doc["file"] = file.display().to_string().into();
            if let Err(e) = args.framing.print(
                &doc,
                crate::frame::Frame::Outline,
                Some(crate::frame::design_corner()),
                true,
            ) {
                return crate::frame::refused(&e);
            }
        }
        common::Format::Text => {
            print!("{}", args.framing.routes_drc(&added.text()));
            if written {
                println!("wrote {}", file.display());
            } else if args.dry_run {
                println!("dry run: {} is not written", file.display());
            }
        }
    }
    if added.refused > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// `routes route` (issue #1195, `netsmith_fabricate::route_net`): exit 0 when the net is routed
/// (or would be, with `--dry-run`) or joined already, 1 when it is refused, 2 when it could not
/// be tried.
fn route(args: &RouteArgs) -> ExitCode {
    use netsmith_fabricate::route_net;
    // The weights of the router's quality objective: the flags, else the project's keys.
    let project = netsmith_fabricate::project::Target::resolve(args.target.file.as_deref())
        .ok()
        .and_then(|t| t.project);
    let weights = match args.weighing.weights(project.as_ref()) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    if let Some(rect) = args.region {
        return region(args, rect, weights);
    }
    let net = args.net.as_deref().unwrap_or_default();
    let done = with_board(&args.target, |layout, board| {
        let file = routes_file(layout);
        let routes = lib::load(&file)?;
        let ask = match route_net::Ask::of(board, net, &args.rip) {
            Ok(ask) => route_net::Ask {
                depth: args.rip_depth,
                tries: args.tries,
                weights,
                reroute: args.reroute,
                through: args.via.clone(),
                ..ask
            },
            Err(lib::Refusal::Refused(e)) => return Ok(Err(e)),
            Err(lib::Refusal::Failed(e)) => return Err(e),
        };
        // `--corner` is the style of this run's copper (issue #1508).
        let board = styled(board, args.corner);
        let routed = route_net::route_net(&board, &routes, &ask)?;
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
            eprintln!("error: {}", args.framing.routes_drc(&e));
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
            if let Err(e) = args.framing.print(
                &doc,
                crate::frame::Frame::Outline,
                Some(crate::frame::design_corner()),
                true,
            ) {
                return crate::frame::refused(&e);
            }
        }
        common::Format::Text => {
            let by = match text_by(&args.framing) {
                Ok(by) => by,
                Err(code) => return code,
            };
            print!("{}", routed.text_moved(by));
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
                eprintln!("error: {}", args.framing.routes_drc(e));
            }
            ExitCode::FAILURE
        }
        None => ExitCode::SUCCESS,
    }
}

/// The board with `--corner`'s style for this run (issue #1508), or the board as it is.
fn styled(board: &Board, corner: Option<Corner>) -> std::borrow::Cow<'_, Board> {
    match corner {
        Some(c) => {
            let mut b = board.clone();
            b.corner = c.track_corner();
            std::borrow::Cow::Owned(b)
        }
        None => std::borrow::Cow::Borrowed(board),
    }
}

/// `routes route --region` (issue #1641, `netsmith_fabricate::route_region`): exit 0 when the
/// region is routed again (or would be, with `--dry-run`), has nothing to route, or costs more
/// (nothing written, both totals said), 1 when it cannot be closed, 2 when it could not be tried.
fn region(args: &RouteArgs, rect: [f64; 4], weights: Option<netsmith_router::Weights>) -> ExitCode {
    use netsmith_fabricate::route_region;
    // The box in the routes file's frame (the outline's): `--frame board-file` gives it in the
    // board file's.
    let rect = match args.framing.frame {
        Some(crate::frame::Frame::BoardFile) => {
            let (cx, cy) = crate::frame::design_corner();
            [rect[0] - cx, rect[1] - cy, rect[2], rect[3]]
        }
        _ => rect,
    };
    let done = with_board(&args.target, |layout, board| {
        let file = routes_file(layout);
        let routes = lib::load(&file)?;
        let board = styled(board, args.corner);
        let ask = route_region::RegionAsk { rect, weights };
        let routed = route_region::route_region(&board, &routes, &ask)?;
        let written = match (&routed.routes, args.dry_run) {
            (Some(r), false) => {
                lib::save(&file, r)?;
                true
            }
            _ => false,
        };
        Ok((routed, file, written))
    });
    let (routed, file, written) = match done {
        Ok(done) => done,
        Err(code) => return code,
    };
    match args.format {
        common::Format::Json => {
            let mut doc = serde_json::to_value(&routed).expect("plain data");
            doc["dry_run"] = args.dry_run.into();
            doc["written"] = written.into();
            doc["file"] = file.display().to_string().into();
            if let Err(e) = args.framing.print(
                &doc,
                crate::frame::Frame::Outline,
                Some(crate::frame::design_corner()),
                true,
            ) {
                return crate::frame::refused(&e);
            }
        }
        common::Format::Text => {
            let by = match text_by(&args.framing) {
                Ok(by) => by,
                Err(code) => return code,
            };
            print!("{}", routed.text_moved(by));
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
                eprintln!("error: {}", args.framing.routes_drc(e));
            }
            ExitCode::FAILURE
        }
        None => ExitCode::SUCCESS,
    }
}

/// `routes join-pad` (issue #1293, `netsmith_fabricate::join_pad`): exit 0 when the pad is joined
/// (or would be, with `--dry-run`) or was joined already, 1 when it is refused, 2 when it could
/// not be tried.
fn join_pad(args: &JoinPadArgs) -> ExitCode {
    use netsmith_fabricate::join_pad;
    let done = with_board(&args.target, |layout, board| {
        let file = routes_file(layout);
        let routes = lib::load(&file)?;
        let joined = match join_pad::join_pad(board, &routes, &args.pad) {
            Ok(j) => j,
            Err(lib::Refusal::Refused(e)) => return Ok(Err(e)),
            Err(lib::Refusal::Failed(e)) => return Err(e),
        };
        let written = match (&joined.routes, args.dry_run) {
            (Some(r), false) => {
                lib::save(&file, r)?;
                true
            }
            _ => false,
        };
        Ok(Ok((joined, file, written)))
    });
    let (joined, file, written) = match done {
        Ok(Ok(done)) => done,
        Ok(Err(e)) => {
            eprintln!("error: {}", args.framing.routes_drc(&e));
            return ExitCode::FAILURE;
        }
        Err(code) => return code,
    };
    match args.format {
        common::Format::Json => {
            let mut doc = serde_json::to_value(&joined).expect("plain data");
            doc["dry_run"] = args.dry_run.into();
            doc["written"] = written.into();
            doc["file"] = file.display().to_string().into();
            if let Err(e) = args.framing.print(
                &doc,
                crate::frame::Frame::Outline,
                Some(crate::frame::design_corner()),
                true,
            ) {
                return crate::frame::refused(&e);
            }
        }
        common::Format::Text => {
            let by = match text_by(&args.framing) {
                Ok(by) => by,
                Err(code) => return code,
            };
            print!("{}", joined.text_moved(by));
            if written {
                println!("wrote {}", file.display());
            } else if args.dry_run && joined.routes.is_some() {
                println!("dry run: {} is not written", file.display());
            }
        }
    }
    match &joined.refused {
        Some(e) => {
            if args.format == common::Format::Text {
                eprintln!("error: {}", args.framing.routes_drc(e));
            }
            ExitCode::FAILURE
        }
        None => ExitCode::SUCCESS,
    }
}

/// The line of `routes import` that counts the KiCad board's zones (issue #1609): `zones: 2
/// taken, 3 left out (2 of the process's own planes and pours, 1 rule area(s))`.
fn zones_said(import: &routes_import::Import) -> String {
    let mut by: std::collections::BTreeMap<routes_import::ZoneReason, usize> = Default::default();
    for z in &import.zones_left_out {
        *by.entry(z.reason).or_default() += 1;
    }
    let mut said = format!(
        "zones: {} taken, {} left out",
        import.zones(),
        import.zones_left_out.len()
    );
    if !by.is_empty() {
        let why: Vec<String> = by
            .iter()
            .map(|(reason, n)| format!("{n} {}", reason.phrase()))
            .collect();
        said.push_str(&format!(" ({})", why.join(", ")));
    }
    said
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
    let imported = with_board(&args.target, |layout, board| {
        let options = routes_import::Options {
            lock: !args.unlocked,
            process_zones: args.process_zones,
        };
        let import = routes_import::import_with(board, &pcb, parts.as_ref(), options)?;
        if !args.no_places {
            write_places(layout, board, &import)?;
        }
        Ok(import)
    });
    let import = match imported {
        Ok(i) => i,
        Err(code) => return code,
    };
    // Then placed again with those places, for each net's pads as they are, and checked with
    // the copper taken (issue #1472).
    let written = with_board(&args.target, |layout, board| {
        let file = routes_file(layout);
        let mut routes = lib::load(&file).unwrap_or_default();
        routes
            .boards
            .insert(board.name.clone(), import.finish(board));
        lib::save(&file, &routes)?;
        let found = lib::own_copper_findings(&lib::with_routes(board, &routes)?.0)?;
        Ok((file, board.name.clone(), found))
    });
    let (file, board_name, found) = match written {
        Ok(f) => f,
        Err(code) => return code,
    };
    let (tracks, arcs, vias) = import.counts();
    let zones = import.zones();
    if args.format == common::Format::Json {
        let nets: Vec<serde_json::Value> = import
            .nets
            .iter()
            .map(|(net, record)| {
                let arcs = record.tracks.iter().filter(|t| t.mid.is_some()).count();
                serde_json::json!({
                    "net": net,
                    "kicad_net": import
                        .mapped
                        .iter()
                        .find(|(ours, _, _)| ours == net)
                        .map(|(_, theirs, _)| theirs),
                    "locked": record.lock,
                    "tracks": record.tracks.len() - arcs,
                    "arcs": arcs,
                    "vias": record.vias.len(),
                    "zones": record.zones.len(),
                    "refusals": found.get(net).cloned().unwrap_or_default(),
                })
            })
            .collect();
        let doc = serde_json::json!({
            "board": board_name,
            "file": file.display().to_string(),
            "parts_paired_by": parts_file.as_ref().map(|p| p.display().to_string()),
            "locked": !args.unlocked,
            "nets": nets,
            "tracks": tracks,
            "arcs": arcs,
            "vias": vias,
            "zones": zones,
            "places": if args.no_places { 0 } else { import.places.len() },
            "places_not_imported": if args.no_places { import.places.len() } else { 0 },
            // The notes of how a part is placed concern places, which `--no-places` writes none of
            // (issue 1515).
            "refitted": if args.no_places { Vec::new() } else { import.refitted.clone() },
            "skipped": import.skipped,
            // Each zone of the KiCad board not taken, with why (issue #1609).
            "zones_left_out": import
                .zones_left_out
                .iter()
                .map(|z| serde_json::json!({
                    "net": z.net,
                    "layers": z.layers,
                    "reason": z.reason.word(),
                    "said": z.said,
                }))
                .collect::<Vec<_>>(),
            // Each via left out as a stitching via `fab` lays again (issue #1583).
            "stitching_vias_left_out": import
                .stitching_vias
                .iter()
                .map(|(net, at)| serde_json::json!({ "net": net, "at": at }))
                .collect::<Vec<_>>(),
        });
        if let Err(e) = args.framing.print(
            &doc,
            crate::frame::Frame::Outline,
            Some(crate::frame::design_corner()),
            true,
        ) {
            return crate::frame::refused(&e);
        }
        return ExitCode::SUCCESS;
    }
    let by = match text_by(&args.framing) {
        Ok(by) => by,
        Err(code) => return code,
    };
    match &parts_file {
        Some(p) => println!("parts paired by {}", p.display()),
        None => println!("parts paired by reference designator"),
    }
    for (ours, theirs, votes) in &import.mapped {
        if ours != theirs {
            println!("net {ours} is KiCad's {theirs} ({votes} pad(s))");
        }
    }
    // The notes of how each part is placed (turned, moved, its pads numbered the other way
    // round) concern places, so with `--no-places` they give way to a count (issue 1515).
    if args.no_places {
        println!("places not imported: {}", import.places.len());
    } else {
        for said in &import.refitted {
            println!("{said}");
        }
    }
    for s in &import.skipped {
        println!("note: {s}");
    }
    println!("{}", zones_said(&import));
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
    // Each net whose own copper the design rules check finds an error of: a lock of it is
    // refused until the copper is mended (`routes lock`, issue #1328).
    for (net, errors) in &found {
        if import.nets.contains_key(net) {
            println!(
                "net {net}: {} error(s) of the design rules check in its copper, first: [{}] at ({:.3}, {:.3})",
                errors.len(),
                errors[0].rule,
                errors[0].at[0] + by[0],
                errors[0].at[1] + by[1]
            );
        }
    }
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
    fn route_takes_a_net_or_a_region_and_not_both() {
        assert!(matches!(
            parse(&["routes", "route", "--region", "1,2.5,3,4", "--dry-run"]),
            Ok(Leaf::Routes(routes::Args {
                command: routes::Command::Route(routes::RouteArgs {
                    net: None,
                    region: Some([1.0, 2.5, 3.0, 4.0]),
                    dry_run: true,
                    ..
                })
            }))
        ));
        assert!(matches!(
            parse(&["routes", "route", "VCC"]),
            Ok(Leaf::Routes(routes::Args {
                command: routes::Command::Route(routes::RouteArgs { region: None, .. })
            }))
        ));
        assert!(parse(&["routes", "route"]).is_err());
        assert!(parse(&["routes", "route", "VCC", "--region", "1,2,3,4"]).is_err());
        assert!(parse(&["routes", "route", "--region", "1,2,3,4", "--rip", "A"]).is_err());
        assert!(parse(&["routes", "route", "--region", "1,2,0,4"]).is_err());
    }

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
                "--rect", "1,-2,3,4.5", "--pads", "thru-hole-only", "--priority", "2", "--patch"
            ]),
            Ok(Leaf::Routes(routes::Args {
                command: routes::Command::AddZone(routes::AddZoneArgs {
                    rect: Some([1.0, -2.0, 3.0, 4.5]),
                    priority: 2,
                    pads: routes::PadConnection::ThruHoleOnly,
                    patch: true,
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
        // `join-pad` (issue #1293).
        assert!(matches!(
            parse(&["routes", "join-pad", "C1.2", "--dry-run", "--file", "a.nsm"]),
            Ok(Leaf::Routes(routes::Args {
                command: routes::Command::JoinPad(routes::JoinPadArgs {
                    ref pad,
                    dry_run: true,
                    ..
                })
            })) if pad == "C1.2"
        ));
        assert_eq!(
            kind(&["routes", "join-pad"]),
            ErrorKind::MissingRequiredArgument
        );
        // `tidy` (issue #1192).
        assert!(matches!(
            parse(&[
                "routes",
                "tidy",
                "--dry-run",
                "--reuse-routes",
                "--widen",
                "--format",
                "json"
            ]),
            Ok(Leaf::Routes(routes::Args {
                command: routes::Command::Tidy(routes::TidyArgs {
                    dry_run: true,
                    reuse_routes: true,
                    widen: true,
                    format: crate::commands::common::Format::Json,
                    ..
                })
            }))
        ));
        // `--widen-to-guideline` (issue #1611).
        assert!(matches!(
            parse(&["routes", "tidy", "--widen-to-guideline"]),
            Ok(Leaf::Routes(routes::Args {
                command: routes::Command::Tidy(routes::TidyArgs {
                    widen: false,
                    widen_to_guideline: true,
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
