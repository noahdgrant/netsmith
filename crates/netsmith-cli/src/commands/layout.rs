//! `netsmith layout` (issue #782): the layout file beside a source (`<stem>.pcb.toml`), shown,
//! written one entry at a time and checked against the board, so that a script or an agent
//! that moves a few parts need not write the TOML itself. The file's format and frame are in the
//! language guide's "The layout file"; the work is `netsmith_fabricate::layout_edit`.
//! `layout summary` and `layout compare` (issue #773) place the board as `dev board3d` does and print
//! what decides whether the placement is good, `netsmith_pcb::Summary` and
//! `netsmith_pcb::PlacementComparison`. `layout regenerate` (issue #825) places one module again
//! with the rest held, `netsmith_fabricate::regenerate`. `layout capture` (issue #828) writes the
//! placement as the layout file, an entry for every part and the seed, `netsmith_fabricate::layout_capture`.
//! `layout fit` (issue #780) says where one part fits with the rest where they are,
//! `netsmith_pcb::Board::fit_part`. `layout parts` (issue #778) gives each part's frame, courtyard
//! shapes and pads as data, `netsmith_pcb::PartGeometries`. `layout export` and `layout import` (issue #781) write the
//! placed board as a KiCad board, also with findings, and bring the parts moved in KiCad back,
//! `netsmith_fabricate::layout_kicad`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args as ClapArgs, Subcommand, ValueEnum};
use netsmith_fabricate::layout_edit as lib;
use netsmith_pcb::LayoutFace;

use super::common;
use super::routes::{Target, parse_point};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "The layout file `<stem>.pcb.toml` sits beside the source and holds the solution: where each part is (`at`, `turn`, `side`, `lock`), keyed by the part's path in the design (`stage.output_cap`; the capacitors of a bank are `bank[0]`, `bank[1]`). The requirements on where a part may be are in the board's physical file, never here. A board of a system defined in another file reads and writes the layout file beside that file, unless the system's layout file still has a table for it (issue #1327).\n\nThe frame is the file's: `--at X,Y` is the footprint's origin in mm from the outline's top left corner, x to the right and y down; a turn is in degrees, counterclockwise on screen, any angle from 0 up to 360; a part on the bottom face is the top face's footprint mirrored left to right (about its own y axis), then turned as it is seen from the top. The guide's \"The layout file\" has an example of a part on each face.\n\n`show`, `set` and `check` check and solve the board as `build` does and place nothing; `summary`, `compare` and `fit` place it as `dev board3d` does and write nothing; `regenerate` and `capture` place it as `fab` does; `export`, `import` and `parts` place it as `dev board3d` does. The exit code is 0 when it did what it was asked, 1 when it refused (a part the board does not have, a turn that is not allowed, a file with an error `check` reports), and 2 when it could not run (a usage mistake included)."
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum Command {
    /// Print the entry of each part (place, turn, face, lock, source) as the placer reads it.
    #[command(
        after_long_help = "Without a PATH every part of the board is listed, with the line of its entry in the file; a part with no entry is said to be placed by the placer. A PATH is a part's path, or a module's, which lists the parts inside it. A path the board does not have is refused with the nearest names. `--format json` prints one JSON document on stdout: the `board`, the `file`, whether it `exists`, and each part's `path`, `refdes` and `entry` (`at`, `turn`, `side`, `lock`, `source`, `line`; left out when the part has none)."
    )]
    Show(ShowArgs),
    /// Write one part's entry, keeping the file's other tables.
    #[command(
        after_long_help = "Writes the entry of the part at PATH to the layout file (made if it does not exist), reading and writing the file as the editor does: every other table is kept, and so is a comment in the file (a comment goes with its entry and stays above it or after it). A part with no entry needs `--at`; for a part that has one, a flag left out keeps what the entry has. A new entry is written with `source = \"hand\"`, and an entry that exists keeps its `source`. Refused, with the file as it was: a PATH the board has no part at (the nearest names are listed), a turn that is not from 0 up to 360, and a part with no entry and no `--at`. A locked entry that breaks a requirement of the physical file is written and said as a warning; `layout check` says it again."
    )]
    Set(SetArgs),
    /// Say which parts have no entry, which entries name no part, and which locked entries break a requirement.
    #[command(
        after_long_help = "One line for each finding, with the line of the entry in the file: an entry that names no part of the board (with the nearest names), an entry with a turn that is not from 0 up to 360 or a position that is not a number, a locked entry that breaks a requirement of the physical file, two parts the file places nearer each other than a `keep 10mm between BuckIc and Crystal` of the physical file asks (a `requirement`), and each fault of the placement check among the parts the file places, together as one list (`placement`: courtyards that overlap, a part past the edge or in a keepout or cutout, too tall, a hole too near), all in the same words `fab` uses (the footprints are loaded and nothing is placed; `fab` runs the same check before its placer starts). A part with no entry is a note: the placer places it. A pad that other parts the file places wall in (no track or via of the process leaves it) is a warning, `walled_in`, which names those parts. A part the file places inside the `soft margin` of another part the file places (`soft margin 3mm round .u1` in the physical file) is a warning, `soft_margin`, which names both; `--strict` makes it an error. A part the placer places (no entry, or an unlocked one) whose courtyard no free rectangle of a face it may go on holds, at any turn the placer may give it, among the locked parts, the edge wall, the cutouts and the keepouts, is a warning, `no_room`, which names the part and the largest free rectangle of each face; `--strict` makes it an error, and `board check` and `fab` say it too, before placing. The exit code is 1 when there is an error, and a note does not make it so. `--format json` prints one JSON document on stdout: the `board`, the `file`, `ok` and the `findings`, each with a `severity` (`error`, `warning` or `note`), a `kind` (`no_entry`, `no_part`, `bad_turn`, `bad_place`, `requirement`, `placement`, `walled_in`, `soft_margin`, `no_room`, `unmeasured`), the part's `path`, the `line` and the `message`."
    )]
    Check(CheckArgs),
    /// Place the board and sum up the placement: wire, decoupling, modules, area per face, nets, free space.
    #[command(
        after_long_help = "The board is placed as `dev board3d` places it (the layout file and the physical file read, nothing written) and summed up in numbers the placer and the checks use, in the layout file's frame (mm from the outline's top left corner, y down): the totals (`hpwl`, the half perimeter wire length; `ratsnest`; `crossings`; `decoupling_mean` and `decoupling_max`); for each face the area the courtyards take against what the outline gives inside the edge wall (`usable`, `courtyards`, `fill`, `free`) and its largest free rectangles; for each module instance of two parts or more its box, the area its courtyards take, its `spread` (the placer's cohesion term: the half perimeter of the box round its parts' courtyard centres) and its `intruders` (parts of other modules whose courtyard centre is in that box); each decoupling capacitor's distance to the pins it serves; for each net its half perimeter, its crossings with other nets and its longest ratsnest line; and the placement check's findings. `--near PATH` adds the parts within `--within` mm (courtyard to courtyard, on either face) of a part, nearest first. `--between PART.PAD PART.PAD` (the part's path or designator, a dot and the pad's number: `U1.3 stage.c1.1`; may be given again) adds the distance between two pads, as `pads`: `distance` between their centres, its `dx` and `dy` (from the first pad to the second), and the `gap` between their copper (0 where they touch), with the net each is on. `--format json` prints one JSON document on one line; the language guide's \"The layout file\" has its fields. The exit code is 0 when the summary is printed, also with findings, and 1 when the board does not build or place."
    )]
    Summary(SummaryArgs),
    /// Place the board with two layout files and say what moved and which findings appeared or went.
    #[command(
        after_long_help = "The board is placed once with the layout file BEFORE and once with AFTER (with none, the layout file beside the source), each as `dev board3d` places it; a part with no entry in a file is placed by the placer, so it may move between the two. Printed: each part that moved, turned or changed face, with its two places and how far its origin moved; the placement check's findings that appeared and those that went; and the totals of each (`hpwl`, `ratsnest`, `crossings`, `decoupling_mean`, `decoupling_max`). `--format json` prints one JSON document on one line: `board`, `moved` (`path`, `refdes`, `from`, `to`, each `x`, `y`, `turn`, `side`, and `distance`), `appeared`, `gone`, `before` and `after`. The exit code is 0 when it is printed, 1 when a board does not build or place, and 2 when a layout file is not there."
    )]
    Compare(CompareArgs),
    /// Place one module again with every other part held, and say what moved and what it changes; `--write` keeps it.
    #[command(
        after_long_help = "The board is placed as `fab` places it (the layout file and the physical file read). The parts of the module at PATH (every part whose path is PATH or starts with `PATH.` or `PATH[`) are placed again, inside the box round them grown by `--margin` mm on each side and cut to the outline; every other part, and every part of the module the layout file locks or a requirement pins, stays where it is. The same board, module, seed and margin always give the same arrangement, and another `--seed` another one.\n\nPrinted: each part placed again with its place before and after (mm from the outline's top left corner, y down, the turn and the face), the nets with a pad on one of them (`fab` and the editor route their copper again and keep the rest), the records of the routes file that no longer match their pads (kept as any record whose pads moved is, open where their copper misses a moved pad), the placement check's findings that appeared and those that went, and the totals before and after. Without `--write` nothing is written. With it, each part placed again gets an entry in the layout file, unlocked, with `source = \"regen\"`, and the rest of the file (its comments too) is kept. `--format json` prints one JSON document on stdout: `board`, `module`, `seed`, `margin`, `region`, `parts` (`path`, `refdes`, `before`, `was`, `after`), `held`, `nets`, `routes_dropped`, `comparison` (as `layout compare` has it) and `written`. The exit code is 0 when the proposal is printed (and written with `--write`), 1 when it is refused (a module the board has no part in, every part of it held, parts that do not fit the region), and 2 when it could not run."
    )]
    Regenerate(RegenerateArgs),
    /// Write the placement as the layout file: an entry for every part, and the seed that placed the board.
    #[command(
        after_long_help = "The board is placed as `fab` places it (the layout file and the physical file read, the seeds after the file's tried as `fab` tries them) and the layout file is written with where every part is: an entry for each part, the seed that placed the board as the file's `seed`, and every other table kept (a comment in the file too, as `set` keeps it). A part that already has an entry keeps its place, its `lock` and its `source`; a part the placer placed gets an entry with `source = \"auto\"`, unlocked. A build of the file then puts every part where it was captured, so the board stays as it is when the placer, a catalog or the circuit around it changes; run `layout regenerate` or `set` to change it. The places are rounded to 0.01 mm, as every entry is.\n\nWith `--dry-run` nothing is written: the report says what would be, and the file is as it was. A board that no seed places is refused with the placer's report and nothing is written. Printed: the seed (and the one the file stated), the parts with a new entry and how many had one. `--format json` prints one JSON document on stdout: `board`, `seed`, `seed_was`, `parts`, `added` (paths), `kept`, `shapes`, `seed_note` (why the seed is not the file's, when it is not) and `written`. The exit code is 0 when it is printed (and written, unless `--dry-run`), 1 when the board does not build or no seed places it, and 2 when it could not run."
    )]
    Capture(CaptureArgs),
    /// Say where one part fits with every other part where it is: the nearest place for each turn and face, or what is in the way.
    #[command(
        after_long_help = "The board is placed as `dev board3d` places it (the layout file and the physical file read, nothing written), and the part at PATH (a path or a designator) is tried on a grid of origins every `--step` mm from the outline's top left corner, for each of the four right-angle turns from its own and on each face (one face when a requirement states it, or `--side` names it; a turn a requirement rules out is not tried). Each place is measured as the placer and `fab` measure it: the placement check of that part against every other part where it is (courtyards, holes, keepouts, height limits, the edge, cutouts) and the requirements of the physical file about it (its windows, its modules' and the relations it is an end of). For each turn and face it gives the place nearest `--near` where nothing is found, or, when there is none, the closest try (the fewest findings, then the nearest) with what is in the way there. `--near` is a part (path or designator: the centre of its courtyard) or a point `X,Y` in mm from the outline's top left corner, y down; with none, where the part is now. Distances are from that point to the centre of the part's courtyard. Nothing is written: `netsmith layout set PATH --at X,Y --turn T --side S` writes a place, as the text says for each.\n\nWith `--as-laid-out` the board is not placed: it is made exactly as the layout file has it, each part where its entry puts it (or where a requirement pins it) and the outline the physical file states, in milliseconds where placing a nearly full board takes seconds or more. It is refused, naming what is missing, when a part has no entry in the layout file and no requirement pins it, or when the outline's size is not stated exactly (a width or height left open or given as a range is known only by placing); `--place-seeds` and `--place-budget` cannot go with it. A fault of the placement among the other parts (two parts whose courtyards overlap, a part past the edge) does not refuse the query, with or without `--as-laid-out`: the part's own result stands, and each such fault is a note on stderr (`note: not about R1, and its result stands: ...`) and, with `--format json`, an entry of `others` (as `placement` has them), so that a part can be tried where two parts have swapped. Run `netsmith layout capture` to give every part an entry. Without it, when the layout file has a place for every part (or a requirement pins it) and the outline's size is stated exactly, a note on stderr says so before the placer starts, since `--as-laid-out` then answers at once; with a part that has no place the placer is the only way to know, and nothing is said.\n\nOn stderr the placing says how far it has got (the seed, the run, the placer's counts, and how long it has been going), as `fab` does; `--progress json` prints each line as `{\"board\", \"stage\": \"placing\", \"unit\", \"done\", \"total\", \"detail\", \"elapsedMs\", \"message\"}` and the note as `{\"message\"}`, and `--to-editor` sends them to a running editor. With `--format json` the progress is printed only with `--progress json`.\n\n`--format json` prints one JSON document on one line: `board`, `part` (`refdes`, `path`), `near` (`x`, `y`, and `part` when it is one), `step`, `now` (the part where it is), `others` (the faults among the other parts, left out when there are none) and `poses`, each a place with `turn`, `side`, `at` (the footprint's origin, as the layout file's `at`), `distance`, `fits`, and where it does not fit `placement` (the placement check's findings as `board fab --format json` has them), `requirements` (the requirements it does not meet) and `messages` (their sentences). The poses that fit come first, nearest first. The exit code is 0 when some pose fits, 1 when none does or the part or board is refused (a part the board does not have, a board that does not build or place), and 2 when it could not run."
    )]
    Fit(FitArgs),
    /// Give each part's frame, courtyard shapes and pads (net, place, size) as data, with no editor.
    #[command(
        after_long_help = "The board is placed as `layout export` and the editor place it (the layout file and the physical file read, nothing written; a layout file the placer refuses gives the parts where the placer ended, and the data is printed with the findings beside it) and each part is given with everything a script needs to check a placement: its `frame` (`at`, the footprint's origin as the layout file's `at`; `turn` in degrees, counterclockwise on screen; `side`; `mirrored`; and the `width` and `height` of the box round its courtyard), its `courtyard` (the box round all its courtyard shapes), `courtyards` (each courtyard shape, its own room), `local_courtyards` (the same shapes in the footprint's own frame), `body`, `height`, and its `pads`: `number`, `net`, `kind`, `shape`, the centre of the copper `at` on the board and `local` in the footprint's frame, its own size `width` and `height`, its `turn` on the board, its `bounds` (the box its copper spans on the board), the faces it has `copper` on and its `drill`. Places are mm from the outline's top left corner, x to the right and y down, as the layout file's.\n\nThe frame is the file's: a footprint's point `(x, y)` is mirrored left to right first on the bottom face (`(-x, y)`), then turned by `turn` degrees counterclockwise on screen (`x' = x cos t + y sin t`, `y' = -x sin t + y cos t`, as y is down), then moved by `at`. A script computes where a part's pads go for a place it has not tried from each pad's `local` and that rule. A turn that is not a right angle gives `courtyards` and `bounds` as the box round the shape as turned.\n\nWithout a PATH every part is given; a PATH is a part's path or designator, or a module's, which gives the parts inside it (it may be given again). A name the board has no part by is refused with the nearest names. With `--as-laid-out` the board is not placed: it is made exactly as the layout file has it, and the parts that have an entry (or that a requirement pins) are given where it puts them; a part with no entry and no pinning requirement is not on that board, so it is listed as having none (`no_entry`, each with its `refdes` and `path`; a line of text for each) and is not refused, also when it is the part or module a PATH names. It is refused, naming what is missing, only when the outline's size is not stated exactly (`--place-seeds` and `--place-budget` cannot go with it). The exit code is the same as for a placed board, 0 when the data is printed, so a script that must know whether every part has a place reads `no_entry`, or runs `netsmith layout check`, whose `no_entry` notes say the same.\n\n`--format json` prints one JSON document on one line: `board`, `width`, `height`, `parts`, and the findings of the placement as `dev board3d --format json` has them: `placement`, `conflicts`, `placement_refused` and `no_entry`. The exit code is 0 when the data is printed, also with findings or with parts that have no entry, 1 when the board does not build or place at all or a name is refused, and 2 when it could not run."
    )]
    Parts(PartsArgs),
    /// Write the placed board, not routed, as a KiCad board, also when the placement has findings.
    #[command(
        after_long_help = "The board is placed as `dev board3d` and the editor place it (the layout file and the physical file read, nothing routed) and written to `--kicad-pcb FILE.KICAD_PCB` with its `.kicad_pro` beside it of the same name, so that a person can look at it in KiCad or move parts with KiCad's tools. It is written whatever the placement check finds, and a layout file the placer refuses gives the board where the placer ended, with the layout file's places. The board has no copper: no track, no via, and the planes and pours as zones that are not filled. It is marked as not for fabrication, in its title block and in a text on `Cmts.User` above the outline; `netsmith board fab` writes the board to order. The findings (the placement check's faults, the lines and locks that cannot all hold, why the placer refused the layout file) are printed and listed in `<stem>.findings.txt` beside the board.\n\nEach footprint carries the part's path in the design in a hidden `netsmith_path` property, which `layout import` reads to bring the parts' moves back to the layout file; renumbering the designators in KiCad changes nothing. `--format json` prints one JSON document on stdout: `board`, `file`, `project`, `findings_file`, `parts`, `placement` and `conflicts` (each as `dev board3d --format json` has them) and `placement_refused`. The exit code is 0 when the board is written, also with findings, 1 when the board does not build or place at all, and 2 when it could not run or write."
    )]
    Export(ExportArgs),
    /// Bring the parts a person moved on a KiCad board back to the layout file.
    #[command(
        after_long_help = "Reads BOARD.KICAD_PCB, a board `layout export` or `fab` wrote and a person then changed in KiCad, and writes an entry to the layout file for each part that KiCad has elsewhere than the board is placed now (its place by more than 0.005 mm, its turn or its face). A footprint is the part whose path its hidden `netsmith_path` property names, never its designator; a footprint without it, or with a path the board has no part at, is said and left out, and so is a second footprint of one path. A part with no footprint on the KiCad board keeps its entry. Each entry is written as `layout set` writes one: an entry that exists keeps its `lock` and `source` and takes the new place, turn and face; a new one is `source = \"hand\"`, unlocked; `--lock` locks every entry written. When a part moved, every part the placer placed (one with no entry) gets an entry where the KiCad board has it, unlocked with `source = \"auto\"` as `layout capture` writes one, so that the placer does not arrange those again round the moved parts and the board is the one left in KiCad. Places are from the outline's top left corner, so move parts, not the board outline. Every other table of the file is kept, and so are its comments. A footprint whose library name differs from the design's is said: only its place is brought back. Copper is not read; `netsmith routes import` brings back tracks and vias.\n\nThe board is placed as `layout export` places it, to know where each part is now. With `--dry-run` nothing is written. `--format json` prints one JSON document on stdout: `board`, `file`, `moved` (`path`, `refdes`, `from` and `to` each with `at`, `turn` and `side`, `existed`, `lock`), `unchanged`, `held` (the parts given an `auto` entry), `missing`, `notes` and `written`. The exit code is 0 when it is printed (and written, unless `--dry-run`), 1 when it is refused (a file that is not a KiCad board, a board that does not build or place), and 2 when it could not run or write. Run `layout check` and `fab` after it."
    )]
    Import(ImportArgs),
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct ExportArgs {
    #[command(flatten)]
    pub target: Target,
    /// The KiCad board to write; the `.kicad_pro` and `.findings.txt` go beside it.
    #[arg(long, value_name = "FILE.KICAD_PCB")]
    pub kicad_pcb: PathBuf,
    #[command(flatten)]
    pub placing: common::Placing,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct ImportArgs {
    /// The KiCad board whose part places to take.
    #[arg(value_name = "BOARD.KICAD_PCB")]
    pub pcb: PathBuf,
    #[command(flatten)]
    pub target: Target,
    /// Lock every entry written: the placer leaves those parts where they are.
    #[arg(long)]
    pub lock: bool,
    /// Write nothing: say what would be written.
    #[arg(long)]
    pub dry_run: bool,
    #[command(flatten)]
    pub placing: common::Placing,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct FitArgs {
    /// The part, by its path in the design (`stage.output_cap`) or its designator.
    #[arg(value_name = "PATH")]
    pub part: String,
    #[command(flatten)]
    pub target: Target,
    /// What to be near: a part (path or designator) or `X,Y` in mm; with none, where the part is.
    #[arg(long, value_name = "PART|X,Y", allow_hyphen_values = true)]
    pub near: Option<String>,
    /// The grid the origins are tried on, in mm.
    #[arg(long, value_name = "MM", default_value_t = netsmith_pcb::FIT_STEP)]
    pub step: f64,
    /// Try one face only: `top` or `bottom`.
    #[arg(long, value_enum)]
    pub side: Option<Side>,
    /// Place nothing: query the board exactly as the layout file has it.
    #[arg(long, conflicts_with_all = ["place_seeds", "place_budget"])]
    pub as_laid_out: bool,
    #[command(flatten)]
    pub placing: common::Placing,
    #[command(flatten)]
    pub watching: common::Watching,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct CaptureArgs {
    #[command(flatten)]
    pub target: Target,
    /// Write nothing: say what would be written.
    #[arg(long)]
    pub dry_run: bool,
    #[command(flatten)]
    pub placing: common::Placing,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct RegenerateArgs {
    /// The module, by its path in the design (`stage`, `channels[1]`).
    #[arg(value_name = "PATH")]
    pub module: String,
    #[command(flatten)]
    pub target: Target,
    /// The seed of the arrangement: the same seed gives the same one, another seed another.
    #[arg(long, value_name = "N", default_value_t = 0)]
    pub seed: u64,
    /// How far round the module's parts they may go, in mm.
    #[arg(long, value_name = "MM", default_value_t = netsmith_fabricate::regenerate::DEFAULT_MARGIN)]
    pub margin: f64,
    /// Write the new places to the layout file (without it, nothing is written).
    #[arg(long)]
    pub write: bool,
    #[command(flatten)]
    pub placing: common::Placing,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct SummaryArgs {
    #[command(flatten)]
    pub target: Target,
    /// A part, by path or designator, whose neighbours to list; may be given again.
    #[arg(long = "near", value_name = "PATH")]
    pub near: Vec<String>,
    /// How far from a `--near` part another may be and be listed, courtyard to courtyard, in mm.
    #[arg(long, value_name = "MM", default_value_t = 2.0)]
    pub within: f64,
    /// Two pads, `PART.PAD PART.PAD` (`U1.3 stage.c1.1`), whose distance to give; may be given again.
    #[arg(long = "between", num_args = 2, value_names = ["PAD", "PAD"])]
    pub between: Vec<String>,
    #[command(flatten)]
    pub placing: common::Placing,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct PartsArgs {
    /// A part by its path or designator, or a module by its path; may be given again. With none, every part.
    #[arg(value_name = "PATH")]
    pub parts: Vec<String>,
    #[command(flatten)]
    pub target: Target,
    /// Place nothing: give the parts that have an entry exactly as the layout file has them, and list the others as having none.
    #[arg(long, conflicts_with_all = ["place_seeds", "place_budget"])]
    pub as_laid_out: bool,
    #[command(flatten)]
    pub placing: common::Placing,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct CompareArgs {
    /// The layout file of the first placement.
    #[arg(value_name = "BEFORE.PCB.TOML")]
    pub before: PathBuf,
    /// The layout file of the second; with none, the one beside the source.
    #[arg(value_name = "AFTER.PCB.TOML")]
    pub after: Option<PathBuf>,
    #[command(flatten)]
    pub target: Target,
    #[command(flatten)]
    pub placing: common::Placing,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct ShowArgs {
    /// The parts to show, by path, or the modules whose parts to show; with none, every part.
    #[arg(value_name = "PATH")]
    pub paths: Vec<String>,
    #[command(flatten)]
    pub target: Target,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

/// The face of the board a part is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Side {
    Top,
    Bottom,
}

#[derive(Debug, PartialEq, ClapArgs)]
#[command(group = clap::ArgGroup::new("change").required(true).multiple(true).args(["at", "turn", "side", "lock", "unlock"]))]
pub struct SetArgs {
    /// The part, by its path in the design (`stage.output_cap`, `decoupling[2]`).
    #[arg(value_name = "PATH")]
    pub path: String,
    #[command(flatten)]
    pub target: Target,
    /// Where the footprint's origin goes, `X,Y` in mm from the outline's top left corner, y down.
    #[arg(long, value_name = "X,Y", value_parser = parse_point, allow_hyphen_values = true)]
    pub at: Option<[f64; 2]>,
    /// The turn in degrees, counterclockwise on screen: any angle from 0 up to 360 (45, 90).
    #[arg(long, value_name = "DEGREES", value_parser = parse_turn, allow_hyphen_values = true)]
    pub turn: Option<f64>,
    /// The face: `top` or `bottom`.
    #[arg(long, value_enum)]
    pub side: Option<Side>,
    /// Lock the entry: the placer leaves the part where it is.
    #[arg(long, conflicts_with = "unlock")]
    pub lock: bool,
    /// Unlock the entry: the placer may move the part.
    #[arg(long)]
    pub unlock: bool,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct CheckArgs {
    #[command(flatten)]
    pub target: Target,
    /// Make a part with no room among the locked parts (`no_room`) and a part inside another's soft margin (`soft_margin`) an error for this run.
    #[arg(long)]
    pub strict: bool,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

fn parse_turn(text: &str) -> Result<f64, String> {
    let turn: f64 = text
        .trim()
        .parse()
        .map_err(|_| format!("`{text}` is not a number"))?;
    match lib::turn_error(turn) {
        Some(why) => Err(why),
        None => Ok(turn),
    }
}

pub fn run(args: Args) -> ExitCode {
    match args.command {
        Command::Show(a) => show(&a),
        Command::Set(a) => set(&a),
        Command::Check(a) => check(&a),
        Command::Summary(a) => summary(&a),
        Command::Compare(a) => compare(&a),
        Command::Regenerate(a) => regenerate(&a),
        Command::Capture(a) => capture(&a),
        Command::Fit(a) => fit(&a),
        Command::Parts(a) => parts(&a),
        Command::Export(a) => export(&a),
        Command::Import(a) => import(&a),
    }
}

/// `layout export`: the placed board, not routed, as a KiCad board, whatever its findings.
fn export(args: &ExportArgs) -> ExitCode {
    use netsmith_fabricate::layout_kicad as lib;
    let made = common::with_placed_board(
        args.target.file.as_deref(),
        args.target.board.as_deref(),
        Some(&args.placing),
        |placed| {
            placed.with_board_shown(|built, board, placing| {
                lib::export(&built.label(), board, placing, &args.kicad_pcb)
            })
        },
    );
    let exported = match made {
        Ok(Ok(exported)) => exported,
        Ok(Err(e)) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
        Err(code) => return code,
    };
    match args.format {
        common::Format::Json => {
            println!("{}", serde_json::to_string(&exported).expect("plain data"))
        }
        common::Format::Text => print!("{}", exported.text()),
    }
    ExitCode::SUCCESS
}

/// `layout import`: the parts moved on a KiCad board, written to the layout file.
fn import(args: &ImportArgs) -> ExitCode {
    use netsmith_fabricate::layout_kicad as lib;
    let pcb = match std::fs::read_to_string(&args.pcb) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("error: {}: {e}", args.pcb.display());
            return ExitCode::from(2);
        }
    };
    let name = args.pcb.display().to_string();
    let made = common::with_placed_board(
        args.target.file.as_deref(),
        args.target.board.as_deref(),
        Some(&args.placing),
        |placed| {
            placed
                .with_board_shown(|built, board, _| {
                    let file = placed.layout_of(built).file;
                    let layout = lib::read_layout(&file)?;
                    lib::import(
                        board,
                        &pcb,
                        &name,
                        layout,
                        &file.display().to_string(),
                        args.lock,
                    )
                    .map(|(imported, layout)| (imported, layout, file))
                })?
                .map_err(|e| format!("error: {e}\n"))
        },
    );
    let (imported, layout, file) = match made {
        Ok(made) => made,
        Err(code) => return code,
    };
    let write = !args.dry_run && !imported.moved.is_empty();
    if write && let Err(e) = netsmith_fabricate::regenerate::accept(&file, &layout) {
        eprintln!("error: {e}");
        return ExitCode::from(2);
    }
    match args.format {
        common::Format::Json => {
            let mut value = serde_json::to_value(&imported).expect("plain data");
            value["written"] = serde_json::Value::Bool(write);
            println!("{value}");
        }
        common::Format::Text => {
            print!("{}", imported.text());
            if write {
                println!("wrote {}", file.display());
            } else if args.dry_run {
                println!("not written (--dry-run)");
            } else {
                println!("nothing to write");
            }
        }
    }
    ExitCode::SUCCESS
}

/// `layout fit`: where one part fits with the rest where they are; 1 when no pose fits.
fn fit(args: &FitArgs) -> ExitCode {
    let near = match netsmith_pcb::FitNear::parse(args.near.as_deref()) {
        Ok(near) => near,
        Err(e) => {
            eprintln!("error: --near: {e}");
            return ExitCode::from(2);
        }
    };
    if !(args.step.is_finite() && args.step > 0.0) {
        eprintln!("error: --step must be above 0 mm");
        return ExitCode::from(2);
    }
    let face = args.side.map(|s| match s {
        Side::Top => netsmith_pcb::Side::Front,
        Side::Bottom => netsmith_pcb::Side::Back,
    });
    // As `fab` and `routes tidy`: with `--format json` the progress is printed only when it is
    // asked for as JSON (issue #1282).
    args.watching.tell_progress(
        "layout fit",
        args.format == common::Format::Json
            && args.watching.progress == crate::progress::Print::Text,
    );
    let made = common::with_stage_events(|| {
        common::with_placed_board(
            args.target.file.as_deref(),
            args.target.board.as_deref(),
            Some(&args.placing),
            |placed| {
                let query = |built: &netsmith_pipeline::Built, board: &netsmith_pcb::Board| {
                    board.fit_part(&built.design, &args.part, &near, args.step, face)
                };
                if args.as_laid_out {
                    placed.with_board_as_laid_out(query)
                } else {
                    // The board is placed, which takes seconds or minutes on a nearly full board:
                    // when the layout file has a place for every part, `--as-laid-out` answers at
                    // once, and the person is told before the placer starts.
                    if placed.with_board_as_laid_out(|_, _| ()).is_ok() {
                        say_as_laid_out_answers_at_once();
                    }
                    placed.with_board(query)
                }
                .and_then(|done| done.map_err(|e| format!("error: {e}\n")))
            },
        )
    });
    crate::progress::ended(
        made.is_ok(),
        if made.is_ok() {
            "layout fit: done"
        } else {
            "layout fit: failed (its output says why)"
        },
    );
    match made {
        Ok(fit) => {
            for note in fit.notes() {
                eprintln!("{note}");
            }
            match args.format {
                common::Format::Json => {
                    println!("{}", serde_json::to_string(&fit).expect("plain data"))
                }
                common::Format::Text => print!("{}", fit.text()),
            }
            if fit.fits() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(code) => code,
    }
}

/// The note up front of a placed `layout fit` on a board the layout file has a place for every part
/// of (issue #1282): text on stderr in any progress mode (as the notes of `fab` are), and posted
/// to the editor when `--to-editor` is given.
fn say_as_laid_out_answers_at_once() {
    const NOTE: &str = "note: the layout file has a place for every part (or a requirement pins it): `--as-laid-out` answers at once, without placing the board, and is what the layout file says; the board is placed now, as `--as-laid-out` is not given";
    eprintln!("{NOTE}");
    crate::progress::post_only(NOTE);
}

/// `layout parts`: each part's frame, courtyard shapes and pads as data, findings beside them.
fn parts(args: &PartsArgs) -> ExitCode {
    let made = common::with_placed_board(
        args.target.file.as_deref(),
        args.target.board.as_deref(),
        Some(&args.placing),
        |placed| {
            let geometry =
                |board: &netsmith_pcb::Board| netsmith_pcb::PartGeometries::of(board, &args.parts);
            if args.as_laid_out {
                placed.with_board_as_laid_out_partial(|_, board, absent| {
                    netsmith_pcb::PartGeometries::of_partial(board, &args.parts, absent)
                })
            } else {
                placed.with_board_shown(|_, board, placing| {
                    geometry(board).map(|mut parts| {
                        parts.conflicts = placing.conflicts;
                        parts.placement_refused = placing.refused;
                        parts
                    })
                })
            }
            .and_then(|done| done.map_err(|e| format!("error: {e}\n")))
        },
    );
    match made {
        Ok(parts) => {
            match args.format {
                common::Format::Json => {
                    println!("{}", serde_json::to_string(&parts).expect("plain data"))
                }
                common::Format::Text => print!("{}", parts.text()),
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

/// `layout capture`: the placement as the layout file, written unless `--dry-run`.
fn capture(args: &CaptureArgs) -> ExitCode {
    use netsmith_fabricate::layout_capture as lib;
    let made = common::with_placed_board(
        args.target.file.as_deref(),
        args.target.board.as_deref(),
        Some(&args.placing),
        |placed| {
            placed
                .with_built(|built| {
                    let file = said_layout_of(placed, built);
                    lib::capture(
                        &built.design,
                        &file,
                        placed.libraries,
                        placed.catalogs.constraints_warn(),
                        placed.seeds,
                    )
                    .map(|(captured, layout)| (captured, layout, file))
                })
                .and_then(|done| done.map_err(|e| format!("error: {e}\n")))
        },
    );
    let (captured, layout, file) = match made {
        Ok(made) => made,
        Err(code) => return code,
    };
    let write = !args.dry_run;
    if write && let Err(e) = lib::accept(&file, &layout) {
        eprintln!("error: {e}");
        return ExitCode::from(2);
    }
    match args.format {
        common::Format::Json => {
            let mut value = serde_json::to_value(&captured).expect("plain data");
            value["written"] = serde_json::Value::Bool(write);
            println!("{value}");
        }
        common::Format::Text => {
            print!("{}", captured.text());
            if write {
                println!("wrote {}", file.display());
            } else {
                println!("not written (--dry-run)");
            }
        }
    }
    ExitCode::SUCCESS
}

/// `layout regenerate`: one module placed again, printed and written when asked.
fn regenerate(args: &RegenerateArgs) -> ExitCode {
    use netsmith_fabricate::regenerate as lib;
    if !args.margin.is_finite() || args.margin < 0.0 {
        eprintln!("error: --margin must be a distance of 0 mm or more");
        return ExitCode::from(2);
    }
    let request = lib::Request {
        module: args.module.clone(),
        seed: args.seed,
        margin: args.margin,
    };
    let made = common::with_placed_board(
        args.target.file.as_deref(),
        args.target.board.as_deref(),
        Some(&args.placing),
        |placed| {
            placed
                .with_built(|built| {
                    let file = said_layout_of(placed, built);
                    lib::propose(
                        &built.design,
                        &file,
                        placed.libraries,
                        placed.catalogs.constraints_warn(),
                        placed.seeds,
                        &request,
                    )
                    .map(|(proposal, layout)| (proposal, layout, file))
                })
                .and_then(|done| done.map_err(|e| format!("error: {e}\n")))
        },
    );
    let (proposal, layout, file) = match made {
        Ok(made) => made,
        Err(code) => return code,
    };
    if args.write
        && let Err(e) = lib::accept(&file, &layout)
    {
        eprintln!("error: {e}");
        return ExitCode::from(2);
    }
    match args.format {
        common::Format::Json => {
            let mut value = serde_json::to_value(&proposal).expect("plain data");
            value["written"] = serde_json::Value::Bool(args.write);
            println!("{value}");
        }
        common::Format::Text => {
            print!("{}", proposal.text());
            if args.write {
                println!("wrote {}", file.display());
            } else {
                println!("not written: run again with --write to keep it");
            }
        }
    }
    ExitCode::SUCCESS
}

/// `layout summary`: the board placed and summed up.
fn summary(args: &SummaryArgs) -> ExitCode {
    let made = common::with_placed_board(
        args.target.file.as_deref(),
        args.target.board.as_deref(),
        Some(&args.placing),
        |placed| {
            placed
                .with_board(|_, board| {
                    let pairs: Vec<(String, String)> = args
                        .between
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|p| (p[0].clone(), p[1].clone()))
                        .collect();
                    netsmith_pcb::Summary::of(board)
                        .neighbours(board, &args.near, args.within)?
                        .pad_distances(board, &pairs)
                })
                .and_then(|done| done.map_err(|e| format!("error: {e}\n")))
        },
    );
    match made {
        Ok(summary) => {
            match args.format {
                common::Format::Json => {
                    println!("{}", serde_json::to_string(&summary).expect("plain data"))
                }
                common::Format::Text => print!("{}", summary.text()),
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

/// `layout compare`: the board placed with two layout files, and what changed between them.
fn compare(args: &CompareArgs) -> ExitCode {
    for file in std::iter::once(&args.before).chain(&args.after) {
        if !file.is_file() {
            eprintln!("error: {}: no such layout file", file.display());
            return ExitCode::from(2);
        }
    }
    let made = common::with_placed_board(
        args.target.file.as_deref(),
        args.target.board.as_deref(),
        Some(&args.placing),
        |placed| {
            let before = placed.with_board_laid_out(&args.before, |_, board| board.clone())?;
            let after = args.after.as_deref().unwrap_or(&placed.layout);
            placed.with_board_laid_out(after, |_, board| {
                netsmith_pcb::PlacementComparison::between(&before, board)
            })
        },
    );
    match made {
        Ok(difference) => {
            match args.format {
                common::Format::Json => println!(
                    "{}",
                    serde_json::to_string(&difference).expect("plain data")
                ),
                common::Format::Text => print!("{}", difference.text()),
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

/// The layout file `built` reads and writes ([`netsmith_fabricate::board3d::Placed::layout_of`]:
/// a board of a system defined in another file has its own, issue #1327), with what is said of
/// it on stderr.
fn said_layout_of(
    placed: &netsmith_fabricate::board3d::Placed,
    built: &netsmith_pipeline::Built,
) -> std::path::PathBuf {
    let at = placed.layout_of(built);
    if let Some(note) = &at.note {
        eprintln!("note: {note}");
    }
    at.file
}

/// `then` of the board picked, checked and solved but not placed, with the layout file's path
/// and the libraries. A refusal of `then` is printed and exits 1; a board that does not build is
/// printed by [`common::with_placed_board`].
fn with_design<T>(
    target: &Target,
    then: impl Fn(
        &netsmith_core::Design,
        &std::path::Path,
        &netsmith_pipeline::Libraries,
    ) -> Result<T, String>,
) -> Result<T, ExitCode> {
    let done = common::with_placed_board(
        target.file.as_deref(),
        target.board.as_deref(),
        None,
        |placed| {
            placed.with_built(|built| {
                let at = said_layout_of(placed, built);
                then(&built.design, &at, placed.libraries)
            })
        },
    )?;
    done.map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    })
}

fn show(args: &ShowArgs) -> ExitCode {
    let shown = with_design(&args.target, |design, file, _| {
        lib::show(design, file, &args.paths)
    });
    match shown {
        Ok(shown) => {
            match args.format {
                common::Format::Json => println!("{}", lib::json(&shown)),
                common::Format::Text => print!("{}", shown.text()),
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

fn set(args: &SetArgs) -> ExitCode {
    let change = lib::Change {
        at: args.at,
        turn: args.turn,
        side: args.side.map(|s| match s {
            Side::Top => LayoutFace::Top,
            Side::Bottom => LayoutFace::Bottom,
        }),
        lock: if args.lock {
            Some(true)
        } else if args.unlock {
            Some(false)
        } else {
            None
        },
    };
    let written = with_design(&args.target, |design, file, libraries| {
        let written = lib::set(design, file, &args.path, &change)?;
        // A locked entry that breaks a requirement is written, and said; so is an entry the
        // placement check `fab` runs finds a fault of (issue #1224).
        let path = written.path.clone();
        let warnings: Vec<String> = lib::check(design, file, libraries, false)
            .map(|checked| {
                checked
                    .findings
                    .into_iter()
                    .filter(|f| match f.kind {
                        "requirement" => f.path.as_deref() == Some(path.as_str()),
                        "placement" => f
                            .fault
                            .as_ref()
                            .is_some_and(|fault| fault.parts.iter().any(|p| p.part.path == path)),
                        _ => false,
                    })
                    .map(|f| f.message)
                    .collect()
            })
            .unwrap_or_default();
        Ok((written, warnings))
    });
    match written {
        Ok((written, warnings)) => {
            match args.format {
                common::Format::Json => println!("{}", lib::json(&written)),
                common::Format::Text => println!("{}", written.text()),
            }
            for warning in warnings {
                eprintln!("warning: {warning}");
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

fn check(args: &CheckArgs) -> ExitCode {
    let checked = with_design(&args.target, |design, file, libraries| {
        lib::check(design, file, libraries, args.strict)
    });
    match checked {
        Ok(checked) => {
            match args.format {
                common::Format::Json => println!("{}", lib::json(&checked)),
                common::Format::Text => print!("{}", checked.text()),
            }
            if checked.ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(code) => code,
    }
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;

    use super::*;
    use crate::commands::common::testing::Leaf as Top;
    use crate::commands::common::testing::{kind, parse};

    #[test]
    fn layout_takes_its_subcommands() {
        assert!(matches!(
            parse(&["layout", "show"]),
            Ok(Top::Layout(Args {
                command: Command::Show(ShowArgs { ref paths, .. })
            })) if paths.is_empty()
        ));
        assert!(matches!(
            parse(&["layout", "show", "a.b", "c", "--format", "json", "--board", "X"]),
            Ok(Top::Layout(Args {
                command: Command::Show(ShowArgs { ref paths, format: common::Format::Json, .. })
            })) if paths == &["a.b", "c"]
        ));
        assert!(matches!(
            parse(&[
                "layout", "set", "u1", "--at", "1.5,-2", "--turn", "90", "--side", "bottom",
                "--lock"
            ]),
            Ok(Top::Layout(Args {
                command: Command::Set(SetArgs {
                    at: Some([1.5, -2.0]),
                    turn: Some(90.0),
                    side: Some(Side::Bottom),
                    lock: true,
                    unlock: false,
                    ..
                })
            }))
        ));
        assert!(matches!(
            parse(&["layout", "check"]),
            Ok(Top::Layout(Args {
                command: Command::Check(_)
            }))
        ));
    }

    #[test]
    fn regenerate_takes_its_arguments() {
        assert!(matches!(
            parse(&["layout", "regenerate", "stage", "--seed", "4", "--margin", "1.5", "--write"]),
            Ok(Top::Layout(Args {
                command: Command::Regenerate(RegenerateArgs { ref module, seed: 4, margin, write: true, .. })
            })) if module == "stage" && margin == 1.5
        ));
        assert!(matches!(
            parse(&["layout", "regenerate", "stage"]),
            Ok(Top::Layout(Args {
                command: Command::Regenerate(RegenerateArgs { seed: 0, margin, write: false, .. })
            })) if margin == 3.0
        ));
    }

    #[test]
    fn fit_takes_its_arguments() {
        assert!(matches!(
            parse(&["layout", "fit", "stage.r1", "--near", "U1", "--step", "0.25", "--side", "top", "--format", "json"]),
            Ok(Top::Layout(Args {
                command: Command::Fit(FitArgs { ref part, near: Some(ref near), step, side: Some(Side::Top), format: common::Format::Json, .. })
            })) if part == "stage.r1" && near == "U1" && step == 0.25
        ));
        assert!(matches!(
            parse(&["layout", "fit", "R1", "--near", "-1,2"]),
            Ok(Top::Layout(Args {
                command: Command::Fit(FitArgs { near: Some(ref near), side: None, .. })
            })) if near == "-1,2"
        ));
        assert_eq!(kind(&["layout", "fit"]), ErrorKind::MissingRequiredArgument);
        // Its progress, as `fab`'s (issue #1282).
        let Ok(Top::Layout(Args {
            command: Command::Fit(f),
        })) = parse(&["layout", "fit", "R1", "--progress", "json", "--to-editor"])
        else {
            panic!("`layout fit --progress json --to-editor` parses");
        };
        assert_eq!(f.watching.progress, crate::progress::Print::Json);
        assert_eq!(f.watching.to_editor, Some(8787));
        assert!(matches!(
            parse(&["layout", "fit", "R1", "--as-laid-out"]),
            Ok(Top::Layout(Args {
                command: Command::Fit(FitArgs {
                    as_laid_out: true,
                    ..
                })
            }))
        ));
        // Nothing is placed, so the placer's seeds and budget have nothing to do.
        assert_eq!(
            kind(&["layout", "fit", "R1", "--as-laid-out", "--place-seeds", "2"]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(
            kind(&[
                "layout",
                "fit",
                "R1",
                "--as-laid-out",
                "--place-budget",
                "5tries"
            ]),
            ErrorKind::ArgumentConflict
        );
    }

    #[test]
    fn capture_takes_its_arguments() {
        assert!(matches!(
            parse(&["layout", "capture", "--dry-run", "--place-seeds", "3"]),
            Ok(Top::Layout(Args {
                command: Command::Capture(CaptureArgs { dry_run: true, .. })
            }))
        ));
        assert!(matches!(
            parse(&["layout", "capture"]),
            Ok(Top::Layout(Args {
                command: Command::Capture(CaptureArgs { dry_run: false, .. })
            }))
        ));
    }

    #[test]
    fn summary_and_compare_take_their_arguments() {
        assert!(matches!(
            parse(&["layout", "summary", "--near", "a.b", "--near", "C1", "--within", "3.5", "--format", "json"]),
            Ok(Top::Layout(Args {
                command: Command::Summary(SummaryArgs { ref near, within, format: common::Format::Json, .. })
            })) if near == &["a.b", "C1"] && within == 3.5
        ));
        assert!(matches!(
            parse(&["layout", "summary", "--between", "U1.3", "a.c1.1", "--between", "R1.1", "R1.2"]),
            Ok(Top::Layout(Args {
                command: Command::Summary(SummaryArgs { ref between, .. })
            })) if between == &["U1.3", "a.c1.1", "R1.1", "R1.2"]
        ));
        assert!(parse(&["layout", "summary", "--between", "U1.3"]).is_err());
        assert!(matches!(
            parse(&["layout", "summary"]),
            Ok(Top::Layout(Args {
                command: Command::Summary(SummaryArgs { ref near, within, .. })
            })) if near.is_empty() && within == 2.0
        ));
        assert!(matches!(
            parse(&["layout", "compare", "old.pcb.toml"]),
            Ok(Top::Layout(Args {
                command: Command::Compare(CompareArgs { after: None, .. })
            }))
        ));
        assert!(matches!(
            parse(&[
                "layout",
                "compare",
                "a.pcb.toml",
                "b.pcb.toml",
                "--place-seeds",
                "2"
            ]),
            Ok(Top::Layout(Args {
                command: Command::Compare(CompareArgs { after: Some(_), .. })
            }))
        ));
        assert_eq!(
            kind(&["layout", "compare"]),
            ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn set_needs_a_change_and_refuses_a_wrong_turn() {
        assert_eq!(
            kind(&["layout", "set", "u1"]),
            ErrorKind::MissingRequiredArgument
        );
        assert_eq!(kind(&["layout", "set"]), ErrorKind::MissingRequiredArgument);
        for turn in ["360", "-90", "720", "ninety"] {
            assert_eq!(
                kind(&["layout", "set", "u1", "--turn", turn]),
                ErrorKind::ValueValidation,
                "{turn}"
            );
        }
        assert_eq!(
            kind(&["layout", "set", "u1", "--lock", "--unlock"]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(
            kind(&["layout", "set", "u1", "--side", "left"]),
            ErrorKind::InvalidValue
        );
        assert_eq!(
            kind(&["layout", "set", "u1", "--at", "1"]),
            ErrorKind::ValueValidation
        );
    }

    #[test]
    fn the_source_is_file_and_the_kicad_board_is_kicad_pcb() {
        assert!(matches!(
            parse(&["layout", "export", "--file", "a.nsm", "--kicad-pcb", "b.kicad_pcb"]),
            Ok(Top::Layout(Args {
                command: Command::Export(ExportArgs { target: Target { file: Some(ref f), .. }, ref kicad_pcb, .. })
            })) if f == "a.nsm" && kicad_pcb == std::path::Path::new("b.kicad_pcb")
        ));
        // Issue #1090: `--out` is a folder wherever it is, and a source is `--file`.
        assert!(parse(&["layout", "export", "--out", "b.kicad_pcb"]).is_err());
        assert!(parse(&["layout", "show", "--into", "a.nsm"]).is_err());
    }
}
