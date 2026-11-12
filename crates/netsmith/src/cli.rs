//! The command line: `netsmith init`, `build`, `fab`, `editor`, `view` and `compare`.

use std::path::PathBuf;

pub const USAGE: &str = "usage: netsmith --version\n       netsmith init [--name <name>]\n       netsmith build [<file.nsm>] [--out <dir>] [--strict] [--locked] [--format text|json] [--notes all|summary|none]\n       netsmith check [<file.nsm>] [--strict] [--format text|json] [--notes all|summary|none]\n       netsmith fab [<file.nsm> | --order [<name>]] [--out <dir>] [--route-timeout <secs>] [--allow-drc-violations] [--cross-check] [--strict] [--locked] [--constraints warn|error] [--pins auto|locked] [--no-glb] [--quiet] [--format text|json] [--notes all|summary|none]\n       netsmith lock (--update | --check) [<file.nsm>]\n       netsmith editor [<file.nsm>] [--port <n>]\n       netsmith board3d [<file.nsm>] [--board <name>] [--json] [--glb <file.glb>] [--step <file.step>]\n       netsmith render [<file.nsm>] [--board <name>] [--view top|bottom|front|back|left|right|iso|iso-bottom] [--out <file.png>] [--size <w>x<h>] [--section x=<mm>|y=<mm>|z=<mm>] [--highlight <id>[,<id>...]]... [--xray] [--no-caps] [--theme light|dark] [--ortho | --perspective]\n       netsmith view [--port <n>] [--board <name>] [--view top|bottom|front|back|left|right|iso|iso-bottom] [--section x=<mm>|y=<mm>|z=<mm>|none] [--highlight <id>[,<id>...]|none]... [--select <id>[,<id>...]|none]... [--xray | --no-xray] [--ortho | --perspective] [--colour-by provenance|kind|margin] [--height-preview <mm>|on|off] [--layers-apart | --no-layers-apart] [--json] [--picture <file.png>]\n       netsmith compare <a> <b> [--board <name>] [--map <file> | --by-refdes] [--ignore-unconnected] [--loose-values] [--format text|json]\n       netsmith parity <board.kicad_pcb> [--table <placements.toml>] [--map <designators.toml>] [--svg <overlay.svg>] [--format text|json]\n       netsmith kicad-libs import <sym-lib-table> [<fp-lib-table>]\n       netsmith catalog refresh [<catalog.csv | dir>...] [--dry-run] [--offline] [--only <catalog>] [--lcsc <C123>] [--delay <ms>] [--cache-ttl <hours>] [--no-cache]\n       netsmith catalog add --lcsc <C123> [--to <catalog.csv>] [--offline] [--no-cache]\n       netsmith catalog age [<catalog.csv | dir>...] [--days <n>] [--only <catalog>]\n       netsmith dxf import <file.dxf> [--layer <name>]... [--holes-layer <name>]... [--units mm|cm|m|inch] [--board <name>] [--write <file.nsm>]";

/// Every command the dispatcher accepts after `--version`, with the line `--help` shows for
/// it. [`parse_args`] takes its commands from here, so the help cannot list one it rejects
/// or leave one out.
pub const COMMANDS: &[(&str, &str)] = &[
    ("init", "scaffold a project in the current directory"),
    ("build", "write a KiCad schematic of the boards"),
    (
        "check",
        "check and solve the boards, write nothing, print the findings only",
    ),
    (
        "fab",
        "make the fab package (Gerbers, drill, bom.csv, cpl.csv, fab-notes.txt, the 3D scene as .glb) or an order",
    ),
    (
        "lock",
        "refresh the pick lock (`--update`) or say whether it is current (`--check`)",
    ),
    ("editor", "the source and the board in a browser"),
    (
        "board3d",
        "the board's 3D scene: heights, limits and overlaps (`--json` for agents)",
    ),
    (
        "render",
        "draw the board in 3D to a PNG (own rasterizer, no GPU, no KiCad)",
    ),
    (
        "view",
        "drive the 3D view of a running editor, as `render` flags say, and save its picture",
    ),
    ("compare", "are two designs the same circuit?"),
    (
        "parity",
        "compare a fab board with the board of a port's original, part by part",
    ),
    (
        "kicad-libs",
        "print the [[library]] stanzas for KiCad's library tables",
    ),
    (
        "catalog",
        "check catalog rows against LCSC's record (`refresh`), list the old ones (`age`) or add a row from it (`add`)",
    ),
    (
        "dxf",
        "print an enclosure's outline, cutouts and holes as source to paste (`import`)",
    ),
];

/// What `netsmith <command> --help` says after the command's usage lines: what each flag does
/// (#455 moved this out of `netsmith --help`, which lists the commands and their flags).
pub const PROSE: &[(&str, &str)] = &[
    (
        "build",
        "With no file, `build` acts on the project's boards (see netsmith.toml).\n\n`--locked` ends the run with an error, before anything is written, when the parts it picks differ from the pick lock beside the source (`<file>.lock.toml`) or when it does not exist, when the pins that its requests get differ from the pin file beside it (`<file>.pins.toml`, ADR 0101), or when the picks file beside it (`<file>.picks.toml`, #462) names a part that is gone.\n\n`--strict` makes a guideline finding an error for this run.\n\n`--format json` prints one JSON document on stdout and nothing else there: the boards and the folders written, and the findings, each with `severity` (`error`, `warning` or `note`), `code` (the stage that found it, or `note.<cause>`), `board`, `path` (the member path), `file`, `line`, `column` and `message`, and the count of the notes for each cause.\n\n`--notes summary` prints a count of a board's notes for each cause (`not_stated`, `part_model`, `unjudged`, `circular`, `supply`, `pick`, `track`, `other`) in place of each note, `--notes none` prints none, and `--notes all`, the default, prints each. With `--format json` the notes are findings only with `all`; their counts are always there.",
    ),
    (
        "check",
        "`check` checks and solves the boards as `build` does and writes nothing: no schematic, no pick lock, no pin file. It prints the findings only (errors, warnings and notes, as `--notes` says), not the bill of materials. The exit code is 0 when every board builds, 1 when one does not, and 2 when the command cannot run (a file that cannot be read, no boards). With no file it acts on the project's boards. `--strict`, `--format` and `--notes` are those of `build`.",
    ),
    (
        "fab",
        "With no file, `fab` acts on the project's boards (see netsmith.toml).\n\n`--locked` is that of `build`, and with `fab` it also means `--pins locked`, so a board that does not route never has its pin file rewritten (`--locked` wins over `--pins auto`).\n\n`--pins auto|locked` says what a board that does not route may do with the pins that its requests got (ADR 0101): `auto`, the default, swaps interchangeable pins that are not locked, places and routes again (three rounds at most) and writes the pin file; `locked` moves none, writes the swaps that would help to `pins.proposed.toml` in the output folder and fails as before (`pins` in netsmith.toml sets it for a project).\n\n`fab` also writes the board's 3D scene as `<board>.glb` beside `fab-notes.txt` (outside the Gerber zip; the scene of the routed board, the bytes `board3d --glb` writes for it; docs/BOARD3D.md), and `--no-glb` leaves it out for a run (`glb = false` in netsmith.toml does it for a project; a stale `<board>.glb` of an earlier run is removed).\n\n`--order <name>` makes one of the project's orders, and `--order` alone lists them; `--order <name> --locked` applies `--locked` to each source file that builds a board of the order.\n\n`--quiet` prints the result of each board and the findings, not the stages and the router's passes.\n\n`--format` and `--notes` are those of `build`; with `--format json` the progress goes nowhere, and the notes of `fab-notes.txt` (\"before you order\") are findings with the code `fab`.",
    ),
    (
        "lock",
        "`lock --update` solves each board without the pick lock's holding (the picks file still holds its parts) and writes the lock with the rows the solver picks now, printing for each part that changed the old row and the new one (nothing when nothing changed). `lock --check` does the same without writing, prints the same and exits 1 when the lock is not current or does not exist, or when the picks file names a part that is gone (for CI). Both exit 2 when they cannot tell: the lock or the picks file is not readable, or the design does not build.",
    ),
    (
        "editor",
        "With no file, `editor` acts on the project's boards (see netsmith.toml). It serves on 127.0.0.1 and prints the address to open.",
    ),
    (
        "board3d",
        "`board3d` places a board as `fab` does (the layout file respected, nothing routed, nothing written but the `--glb` and `--step` files, no KiCad needed) and prints its 3D scene: with `--json` the report an agent reads (each part's body box, Z and where its height came from, each height limit's tallest part and margin, bodies that overlap, unknown heights with the reason, the height coverage; docs/BOARD3D.md), else a short summary; with `--glb <file.glb>` it writes the scene as glTF 2.0 binary instead of the summary (the file the editor's 3D view shows; docs/BOARD3D.md), and with `--json` as well it does both. `--step <file.step>` writes the board and its parts as a STEP file (AP214) for mechanical CAD, in place of the summary as `--glb` is: the board through its thickness with its cutouts and holes, each part a product named by its designator, as exact solids; a part of unknown height is its outline only, never a guessed solid (ADR 0107; docs/BOARD3D.md). A file of several boards needs `--board`.",
    ),
    (
        "render",
        "`render` places a board as `board3d` does and draws it to a PNG with netsmith's own software rasterizer (deterministic bytes, no GPU, no browser, no KiCad; docs/BOARD3D.md, Pictures for agents): `--view` (default `iso`; `top` and `bottom` look straight at a face, `front`, `back`, `left` and `right` along the board's edges, `iso` and `iso-bottom` from a corner), `--out` (default `<board>-<view>.png` in the current folder; the path written is printed), `--size` (default 1600x1000; 64 to 8192 pixels a side, at most 16 million pixels), `--section x=12.5` (cut the scene at that position in mm of the scene's frame, y up from the outline's bottom-left corner, z up from the board's top face, and keep the side away from the camera; the cut faces are filled as a section of the stackup, each layer at its true thickness in its own colour, unless `--no-caps` leaves them open), `--highlight` (a part's path or designator, `net:<name>` or `limit:<name>`, comma-separated or repeated; they are drawn in the highlight colour and the rest is dimmed; an id the board does not have is an error), `--xray` (everything see-through), `--theme` (`light`, the default, or `dark`), and `--ortho` (the default) or `--perspective`.",
    ),
    (
        "view",
        "`view` drives the 3D view of a running editor (start `netsmith editor` and open its page; docs/BOARD3D.md, Driving a running editor) with the names of `render`, so that an agent changes the verb to switch between the two: `--port` (default 8787, the editor's), `--board`, `--view`, `--section` (or `none`), `--highlight` and `--select` (a part, `net:<name>` or `limit:<name>`, comma-separated or repeated; `none` lights or selects nothing; the editor's selection holds parts or one net, so `--select` takes no limit, and the two are not given together), `--xray` or `--no-xray`, `--ortho` or `--perspective`, and the editor's own tools: `--colour-by` (what the bodies are coloured by), `--height-preview` (a height in mm from 0.1 to 50, `on` or `off`) and `--layers-apart` or `--no-layers-apart` (the stackup pulled apart); with none of them it prints the state of the page (a short summary, or with `--json` the editor's JSON as it came); with any, it changes the page, which draws it, and prints the state that results; `--picture <file.png>` saves what the page's canvas shows after the change (and opens the 3D view if it is not on screen); the page driven is the one focused last; a change the page refuses (an id the board does not have, a section, a preview height) exits 2 with the editor's message, as does an editor that is not running or has no page open; `--theme` is refused, since the theme is the person's own setting for the whole editor (the state reports it); nothing changes the design or a file.",
    ),
    (
        "compare",
        "`compare` checks two designs against each other by connectivity (ADR 0017). Each of `<a>` and `<b>` is a `.kicad_sch`, a `.kicad_pcb`, a KiCad netlist (XML) or a `.nsm` file, which is built as `build` does; `--board` picks the board of a `.nsm` file that has several. `--map` fixes pairs of designators from a file, `--by-refdes` takes equal designators as the same part, `--ignore-unconnected` leaves out nets of one pin and `--loose-values` reads values as numbers. The exit code is 0 for a match, 1 for a difference and 2 for a mistake or a comparison that could not be decided. `--format json` prints the result as one JSON document: `match`, the pairs of designators and of net names, and each difference.",
    ),
    (
        "parity",
        "`parity` compares a board that `fab` wrote (`<board.kicad_pcb>`) with the original of a port, part by part: the place, the rotation, the face and the pads of each part the table names. `--table` is the table of the original's parts (default `reference/placements.toml` of the project), `--map` the map of the original's designators to the fab board's (default `reference/designators.toml`), and `--svg` writes an overlay of the two boards. The exit code is 1 when a part the table marks `must_match` fails, 0 when none does and 2 when a file cannot be read. `--format json` prints the report as one JSON document.",
    ),
    (
        "dxf",
        "`dxf import` prints the board outline, cutouts and mounting holes of an enclosure's ASCII DXF as source to paste, and with `--write` puts them, with the places of the cutouts and holes, in the `case` section of the physical file beside the source it names (ADR 0045, ADR 0048).",
    ),
    (
        "catalog",
        "`catalog refresh` checks the manufacturer's number and package of the rows of the project's catalogs (or the standard ones, in the netsmith repository) against LCSC's record (never JLCPCB, so JLCPCB data is not fetched), writes only the day in a `checked` column, never a price or stock, and is never run by another command. `catalog age` lists the rows whose date is older than `--days` days. `catalog add --lcsc <C123> --to <catalog.csv>` asks LCSC's record of the number (never JLCPCB's) and appends a row with `lcsc`, `mpn`, `package` and a `source` that names the record and the day, leaving every other cell empty and listing the columns that need the datasheet or JLCPCB's page; without `--to` it prints the record and writes nothing.",
    ),
];

/// What `netsmith --help` prints: the commands, one line each, then every command with its
/// flags. Printed to stdout; the exit status is 0. What each flag does is in
/// [`command_help`].
pub fn help() -> String {
    let mut text = format!(
        "netsmith {}: electronics as code\n\ncommands:\n",
        env!("CARGO_PKG_VERSION")
    );
    for (name, what) in COMMANDS {
        text.push_str(&format!("  {name:<11} {what}\n"));
    }
    text.push_str("  help        print this list (also --help, -h; `<command> --help` too)\n");
    text.push_str("  --version   print the version (also -V, version)\n\n");
    text.push_str(USAGE);
    text.push_str("\n\n`netsmith <command> --help` says what the flags of a command do.\n");
    text
}

/// What `netsmith <command> --help` prints: the command's line of [`COMMANDS`], its usage lines
/// and what its flags do ([`PROSE`]).
pub fn command_help(command: &str) -> String {
    let what = COMMANDS
        .iter()
        .find(|(name, _)| *name == command)
        .map_or("", |(_, what)| *what);
    let mut text = format!("netsmith {command}: {what}\n\n");
    let lines: Vec<&str> = USAGE
        .trim_start_matches("usage: ")
        .split('\n')
        .map(str::trim)
        .filter(|l| {
            l.starts_with(&format!("netsmith {command} ")) || *l == format!("netsmith {command}")
        })
        .collect();
    for (i, line) in lines.iter().enumerate() {
        text.push_str(if i == 0 { "usage: " } else { "       " });
        text.push_str(line);
        text.push('\n');
    }
    if let Some((_, prose)) = PROSE.iter().find(|(name, _)| *name == command) {
        text.push('\n');
        text.push_str(prose);
        text.push('\n');
    }
    text.push_str("\n`netsmith --help` lists every command.\n");
    text
}

/// The port `netsmith editor` listens on unless told otherwise.
pub const DEFAULT_PORT: u16 = 8787;

/// What `netsmith catalog refresh` is asked to do (ADR 0039).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RefreshRequest {
    /// Catalog files or directories of them; none is the project's catalogs and, in the
    /// netsmith repository, the standard ones.
    pub paths: Vec<PathBuf>,
    pub dry_run: bool,
    pub offline: bool,
    pub only: Vec<String>,
    pub lcsc: Vec<String>,
    /// The pause between requests, in milliseconds, if not the default.
    pub delay_ms: Option<u64>,
    /// How long a kept answer is used, in hours, if not the default.
    pub cache_ttl_hours: Option<u64>,
    pub no_cache: bool,
}

/// What `netsmith lock` does with the pick lock (ADR 0100).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    /// `--update`: write the rows the solver picks now.
    Update,
    /// `--check`: write nothing; fail when the lock is not current.
    Check,
}

/// What `netsmith fab --order` asks for (ADR 0035).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderRequest {
    /// `--order` with no name: list the project's orders.
    List,
    /// `--order <name>`: make that order.
    Make(String),
}

#[derive(Debug, PartialEq)]
pub enum Command {
    /// `--version`, `-V` or `version`: print the version this binary was built as.
    Version,
    /// `--help`, `-h` or `help`: print the commands and their flags. `<command> --help` (or
    /// `help <command>`) prints what the flags of that command do.
    Help(Option<String>),
    /// Scaffold a project in the current directory, named `name` or after the directory.
    Init { name: Option<String> },
    /// `file` is `None` to act on the project's top-level files; `out` is `None` for the
    /// default directory.
    Build {
        file: Option<String>,
        out: Option<PathBuf>,
        /// `--strict`: a guideline finding stops the build for this run (ADR 0036).
        strict: bool,
        /// `--locked`: neither the pick lock nor the pin file (ADR 0101) may change; a run that
        /// would change either stops before it writes anything.
        locked: bool,
        /// `--format`: text for a person, or one JSON document (#455).
        format: crate::findings::Format,
        /// `--notes`: each note, a count for each cause, or none (#455).
        notes: crate::findings::NotesMode,
    },
    /// `check`: build without writing anything, and print the findings only (#455).
    Check {
        file: Option<String>,
        strict: bool,
        format: crate::findings::Format,
        notes: crate::findings::NotesMode,
    },
    /// `parity`: compare a board `fab` wrote with the board of a port's original (#455).
    Parity {
        board: PathBuf,
        table: Option<PathBuf>,
        map: Option<PathBuf>,
        svg: Option<PathBuf>,
        format: crate::findings::Format,
    },
    Fab {
        file: Option<String>,
        out: Option<PathBuf>,
        /// Seconds the router may take for a board before `fab` stops and lists the
        /// connections left; `Some(0)` is no limit, `None` the default.
        route_timeout: Option<u64>,
        allow_drc_violations: bool,
        cross_check: bool,
        /// `--strict`: a guideline finding stops the run (ADR 0036).
        strict: bool,
        /// `--locked`: neither the pick lock nor the pin file (ADR 0101) may change; a run that
        /// would change either stops before it writes anything, and a board that does not
        /// route is handled as with `--pins locked` (the flag wins over `--pins auto`).
        locked: bool,
        /// `--constraints warn` or `error` for this run, over the manifest's `constraints`
        /// (ADR 0045): `Some(true)` lists a requirement on where a part goes that the board
        /// does not meet instead of stopping the run, `Some(false)` stops even if the
        /// manifest says `warn`, `None` leaves it to the manifest.
        constraints_warn: Option<bool>,
        /// `--pins auto` or `locked` for this run, over the manifest's `pins` (ADR 0101):
        /// `Some(false)` lets a board that does not route have its unlocked pins swapped,
        /// `Some(true)` leaves every pin and writes the proposal, `None` leaves it to the
        /// manifest, whose default is `auto`.
        pins_locked: Option<bool>,
        /// `--no-glb`: leave the board's 3D scene (`<board>.glb`) out of the package for this
        /// run. `false` leaves it to the manifest's `glb`, whose default is to write it.
        no_glb: bool,
        /// Make a named order of the project, or list them, instead of every board.
        order: Option<OrderRequest>,
        /// `--quiet`: the result of each board and the findings, not the stages (#455).
        quiet: bool,
        format: crate::findings::Format,
        notes: crate::findings::NotesMode,
    },
    /// Refresh or check the pick lock of `file`, or of each top-level file of the project.
    Lock {
        file: Option<String>,
        mode: LockMode,
    },
    /// The editor in a browser. `file` is `None` to act on the project's top-level files.
    Editor { file: Option<String>, port: u16 },
    /// The 3D scene of one board of `file` (or of the project's top-level files), as a JSON
    /// report with `json`, as a `.glb` file written to `glb`, as a STEP file written to `step`,
    /// and as a summary with none of them
    /// (ADR 0105).
    Board3d {
        file: Option<String>,
        board: Option<String>,
        json: bool,
        glb: Option<PathBuf>,
        step: Option<PathBuf>,
    },
    /// The 3D scene of one board of `file` drawn to a PNG written to `out` (`<board>-<view>.png`
    /// when `None`), as `request` says (ADR 0105).
    Render {
        file: Option<String>,
        board: Option<String>,
        out: Option<PathBuf>,
        request: crate::render::Request,
    },
    /// Read or change the 3D view of a running editor's page, and save its picture
    /// (P8b of issue 430).
    View(crate::view_client::Request),
    /// Compare two designs by connectivity. Each is a `.kicad_sch`, `.kicad_pcb`, KiCad
    /// netlist XML or `.nsm` file.
    /// Check catalog rows against LCSC's record and record the day in `checked` (ADR 0039).
    /// JLCPCB is not asked, and no price or stock is written.
    CatalogRefresh(RefreshRequest),
    /// Append a row made from LCSC's record of `lcsc` to the catalog file `to`, or print the
    /// record when there is none (#454).
    CatalogAdd {
        lcsc: String,
        to: Option<PathBuf>,
        offline: bool,
        no_cache: bool,
    },
    /// List the catalog rows whose date is older than `days` days.
    CatalogAge {
        paths: Vec<PathBuf>,
        only: Vec<String>,
        days: u32,
    },
    /// Print the outline, cutouts and holes of an enclosure's DXF file as source to paste
    /// (ADR 0045).
    DxfImport {
        file: PathBuf,
        layers: Vec<String>,
        holes_layers: Vec<String>,
        units: Option<netsmith_dxf::Units>,
        board: Option<String>,
        /// The source file whose layout file receives the places of the holes and cutouts.
        write: Option<PathBuf>,
    },
    /// Print the `[[library]]` stanzas for KiCad's library tables, to paste into
    /// `kicad.toml`.
    KicadLibsImport {
        sym_lib_table: String,
        fp_lib_table: Option<String>,
    },
    Compare {
        a: String,
        b: String,
        /// Which board of an `.nsm` file, if it has more than one.
        board: Option<String>,
        /// A file of `A_ref B_ref` lines that fixes pairs of designators.
        map: Option<PathBuf>,
        by_refdes: bool,
        ignore_unconnected: bool,
        loose_values: bool,
        format: crate::findings::Format,
    },
}

/// Read the arguments after the program name. The error is what to tell the user.
pub fn parse_args<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    let command = args.first().ok_or_else(|| USAGE.to_string())?.clone();
    let known = COMMANDS.iter().any(|(name, _)| *name == command);
    let is_command = |name: &str| COMMANDS.iter().any(|(n, _)| *n == name);
    if ["--help", "-h", "help"].contains(&command.as_str()) {
        // `help <command>` is that command's help.
        return Ok(Command::Help(
            args.get(1).filter(|a| is_command(a)).cloned(),
        ));
    }
    if known && args[1..].iter().any(|a| a == "--help" || a == "-h") {
        return Ok(Command::Help(Some(command)));
    }
    let mut args = args.into_iter().skip(1).peekable();
    if ["--version", "-V", "version"].contains(&command.as_str()) {
        return match args.next() {
            None => Ok(Command::Version),
            Some(extra) => Err(format!(
                "`{command}` takes no argument, not `{extra}`\n{USAGE}"
            )),
        };
    }
    if command == "kicad-libs" {
        let rest: Vec<String> = args.collect();
        return match rest.as_slice() {
            [import, sym] if import == "import" => Ok(Command::KicadLibsImport {
                sym_lib_table: sym.clone(),
                fp_lib_table: None,
            }),
            [import, sym, fp] if import == "import" => Ok(Command::KicadLibsImport {
                sym_lib_table: sym.clone(),
                fp_lib_table: Some(fp.clone()),
            }),
            _ => Err(format!(
                "`kicad-libs` takes `import <sym-lib-table> [<fp-lib-table>]`\n{USAGE}"
            )),
        };
    }
    if command == "catalog" {
        return parse_catalog(args.collect());
    }
    if command == "dxf" {
        return parse_dxf(args.collect());
    }
    if !known {
        return Err(format!(
            "unknown command `{command}`: `netsmith --help` lists the commands\n{USAGE}"
        ));
    }

    let mut files: Vec<String> = Vec::new();
    let mut format = crate::findings::Format::Text;
    let mut notes = crate::findings::NotesMode::All;
    let mut quiet = false;
    let (mut table, mut svg): (Option<PathBuf>, Option<PathBuf>) = (None, None);
    let mut board: Option<String> = None;
    let mut map: Option<PathBuf> = None;
    let (mut by_refdes, mut ignore_unconnected, mut loose_values) = (false, false, false);
    let mut name: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut route_timeout: Option<u64> = None;
    let (mut allow_drc_violations, mut cross_check, mut strict, mut locked) =
        (false, false, false, false);
    let mut constraints_warn: Option<bool> = None;
    let mut pins_locked: Option<bool> = None;
    let mut no_glb = false;
    let mut order: Option<OrderRequest> = None;
    let mut lock_mode: Option<LockMode> = None;
    let mut port = DEFAULT_PORT;
    let mut json = false;
    let mut glb: Option<PathBuf> = None;
    let mut step: Option<PathBuf> = None;
    let mut render = crate::render::Request::default();
    let (mut ortho, mut perspective) = (false, false);
    let mut view = crate::view_client::Request {
        port: DEFAULT_PORT,
        change: netsmith_editor::api::ViewChange::default(),
        json: false,
        picture: None,
    };
    while let Some(arg) = args.next() {
        match (command.as_str(), arg.as_str()) {
            ("init", "--name") => {
                name = Some(args.next().ok_or("--name needs a project name")?);
            }
            ("render", "--view") => {
                let given = args.next().ok_or("--view needs a view")?;
                render.view = netsmith_pcb::raster3d::View::from_name(&given).ok_or_else(|| {
                    let names: Vec<&str> = netsmith_pcb::raster3d::View::ALL
                        .iter()
                        .map(|v| v.name())
                        .collect();
                    format!("--view takes one of {}, not `{given}`", names.join(", "))
                })?;
            }
            ("render", "--size") => {
                let given = args.next().ok_or("--size needs <width>x<height>")?;
                render.size = crate::render::parse_size(&given)?;
            }
            ("render", "--section") => {
                let given = args
                    .next()
                    .ok_or("--section needs an axis and a position")?;
                render.section = Some(crate::render::parse_section(&given)?);
            }
            ("render", "--highlight") => {
                let given = args
                    .next()
                    .ok_or("--highlight needs a part, `net:<name>` or `limit:<name>`")?;
                render.highlight.extend(
                    given
                        .split(',')
                        .map(str::trim)
                        .filter(|id| !id.is_empty())
                        .map(str::to_string),
                );
            }
            ("render", "--xray") => render.xray = true,
            ("render", "--no-caps") => render.caps = false,
            ("render", "--theme") => match args.next().as_deref() {
                Some("light") => render.dark = false,
                Some("dark") => render.dark = true,
                other => {
                    return Err(format!(
                        "--theme takes `light` or `dark`, not {}\n{USAGE}",
                        other.map_or("nothing".to_string(), |w| format!("`{w}`"))
                    ));
                }
            },
            ("render" | "view", "--ortho") => ortho = true,
            ("render" | "view", "--perspective") => perspective = true,
            ("view", "--view") => {
                let given = args.next().ok_or("--view needs a view")?;
                view.change.view = Some(parse_view(&given)?);
            }
            ("view", "--section") => {
                let given = args
                    .next()
                    .ok_or("--section needs an axis and a position, or `none`")?;
                view.change.section = Some(if given.trim() == "none" {
                    "none".to_string()
                } else {
                    crate::render::parse_section(&given)?;
                    given.trim().to_string()
                });
            }
            ("view", "--highlight") => {
                let given = args
                    .next()
                    .ok_or("--highlight needs a part, `net:<name>`, `limit:<name>` or `none`")?;
                add_ids(&mut view.change.highlight, "--highlight", &given)?;
            }
            ("view", "--select") => {
                let given = args
                    .next()
                    .ok_or("--select needs a part, `net:<name>` or `none`")?;
                add_ids(&mut view.change.select, "--select", &given)?;
            }
            ("view", "--xray") => view.change.xray = Some(true),
            ("view", "--no-xray") => view.change.xray = Some(false),
            ("view", "--layers-apart") => view.change.exploded = Some(true),
            ("view", "--no-layers-apart") => view.change.exploded = Some(false),
            ("view", "--colour-by" | "--color-by") => {
                let given = args
                    .next()
                    .ok_or("--colour-by needs `provenance`, `kind` or `margin`")?;
                view.change.colour_by = Some(match given.as_str() {
                    "provenance" => netsmith_editor::api::ColourBy::Provenance,
                    "kind" => netsmith_editor::api::ColourBy::Kind,
                    "margin" => netsmith_editor::api::ColourBy::Margin,
                    _ => {
                        return Err(format!(
                            "--colour-by takes `provenance` (where each height came from), `kind` (the part's kind) or `margin` (to the height limit over it), not `{given}`"
                        ));
                    }
                });
            }
            ("view", "--height-preview") => {
                let given = args
                    .next()
                    .ok_or("--height-preview needs a height in mm, `on` or `off`")?;
                view.change.height_preview = Some(match given.as_str() {
                    "on" => netsmith_editor::api::PreviewSetting::On(true),
                    "off" => netsmith_editor::api::PreviewSetting::On(false),
                    mm => match mm.parse::<f64>() {
                        Ok(v) if (0.1..=50.0).contains(&v) => {
                            netsmith_editor::api::PreviewSetting::Mm(v)
                        }
                        _ => {
                            return Err(format!(
                                "--height-preview takes a height in mm from 0.1 to 50, `on` or `off`, not `{given}`"
                            ));
                        }
                    },
                });
            }
            ("view", "--json") => view.json = true,
            ("view", "--picture") => {
                view.picture = Some(PathBuf::from(
                    args.next().ok_or("--picture needs a .png file to write")?,
                ));
            }
            ("view", "--theme") => {
                return Err(format!(
                    "`view` has no --theme: the theme is the person's own setting for the whole editor, not a view of the board (the state says which it is); `render --theme light|dark` draws a picture in either\n{USAGE}"
                ));
            }
            ("view", "--out") => {
                return Err(format!(
                    "`view` saves the page's picture with --picture <file.png>, not --out\n{USAGE}"
                ));
            }
            ("view", file) if !file.starts_with("--") => {
                return Err(format!(
                    "`view` takes no file: it drives the editor's open project (`--board` says which board), not a source file; `netsmith render {file}` draws one without an editor\n{USAGE}"
                ));
            }
            ("build" | "fab" | "render", "--out") => {
                out = Some(PathBuf::from(args.next().ok_or(
                    if command == "render" {
                        "--out needs a file to write"
                    } else {
                        "--out needs a directory"
                    },
                )?));
            }
            ("fab", "--order") => {
                order = Some(match args.next_if(|next| !next.starts_with("--")) {
                    Some(name) => OrderRequest::Make(name),
                    None => OrderRequest::List,
                });
            }
            ("fab", "--route-timeout") => {
                let given = args
                    .next()
                    .ok_or("--route-timeout needs a number of seconds")?;
                route_timeout = Some(given.parse().map_err(|_| {
                    format!("--route-timeout needs a whole number of seconds (0 for no limit), not `{given}`")
                })?);
            }
            ("fab", "--allow-drc-violations") => allow_drc_violations = true,
            ("fab", "--cross-check") => cross_check = true,
            ("fab", "--no-glb") => no_glb = true,
            ("build" | "check" | "fab", "--strict") => strict = true,
            ("build" | "fab", "--locked") => locked = true,
            ("lock", "--update" | "--check") => {
                let asked = if arg == "--update" {
                    LockMode::Update
                } else {
                    LockMode::Check
                };
                if lock_mode.is_some_and(|earlier| earlier != asked) {
                    return Err(format!(
                        "`lock` takes `--update` or `--check`, not both\n{USAGE}"
                    ));
                }
                lock_mode = Some(asked);
            }
            ("fab", "--constraints") => match args.next().as_deref() {
                Some("warn") => constraints_warn = Some(true),
                Some("error") => constraints_warn = Some(false),
                other => {
                    return Err(format!(
                        "--constraints takes `warn` (a requirement on where a part goes that is not met is listed instead of stopping `fab`) or `error` (it stops `fab`, whatever netsmith.toml says), not {}\n{USAGE}",
                        other.map_or("nothing".to_string(), |w| format!("`{w}`"))
                    ));
                }
            },
            ("fab", "--pins") => match args.next().as_deref() {
                Some("auto") => pins_locked = Some(false),
                Some("locked") => pins_locked = Some(true),
                other => {
                    return Err(format!(
                        "--pins takes `auto` (a board that does not route may have its unlocked pins swapped, and the pin file written) or `locked` (no pin moves; the swaps that would help are written to pins.proposed.toml), not {}\n{USAGE}",
                        other.map_or("nothing".to_string(), |w| format!("`{w}`"))
                    ));
                }
            },
            ("editor", "--port") => {
                let given = args.next().ok_or("--port needs a number")?;
                port = given
                    .parse()
                    .map_err(|_| format!("--port needs a number from 0 to 65535, not `{given}`"))?;
            }
            ("view", "--port") => {
                let given = args.next().ok_or("--port needs a number")?;
                view.port = given
                    .parse::<u16>()
                    .ok()
                    .filter(|p| *p != 0)
                    .ok_or_else(|| {
                        format!(
                            "--port needs the editor's port, a number from 1 to 65535, not `{given}`"
                        )
                    })?;
            }
            ("board3d", "--json") => json = true,
            ("board3d", "--glb") => {
                glb = Some(PathBuf::from(
                    args.next().ok_or("--glb needs a file to write")?,
                ));
            }
            ("board3d", "--step") => {
                step = Some(PathBuf::from(
                    args.next().ok_or("--step needs a file to write")?,
                ));
            }
            ("build" | "check" | "fab" | "compare" | "parity", "--format") => {
                let given = args.next();
                format = given
                    .as_deref()
                    .and_then(crate::findings::Format::parse)
                    .ok_or_else(|| {
                        format!(
                            "--format takes `text` or `json`, not {}\n{USAGE}",
                            given.map_or("nothing".to_string(), |w| format!("`{w}`"))
                        )
                    })?;
            }
            ("build" | "check" | "fab", "--notes") => {
                let given = args.next();
                notes = given
                    .as_deref()
                    .and_then(crate::findings::NotesMode::parse)
                    .ok_or_else(|| {
                        format!(
                            "--notes takes `all` (each note), `summary` (a count for each cause) or `none`, not {}\n{USAGE}",
                            given.map_or("nothing".to_string(), |w| format!("`{w}`"))
                        )
                    })?;
            }
            ("fab", "--quiet") => quiet = true,
            ("parity", "--table") => {
                table = Some(PathBuf::from(args.next().ok_or("--table needs a file")?));
            }
            ("parity", "--svg") => {
                svg = Some(PathBuf::from(
                    args.next().ok_or("--svg needs a file to write")?,
                ));
            }
            ("compare" | "board3d" | "render" | "view", "--board") => {
                board = Some(args.next().ok_or("--board needs a board name")?);
            }
            ("compare" | "parity", "--map") => {
                map = Some(PathBuf::from(args.next().ok_or("--map needs a file")?));
            }
            ("compare", "--by-refdes") => by_refdes = true,
            ("compare", "--ignore-unconnected") => ignore_unconnected = true,
            ("compare", "--loose-values") => loose_values = true,
            (_, flag) if flag.starts_with("--") => {
                return Err(format!("`{command}` has no option `{flag}`\n{USAGE}"));
            }
            ("init", extra) => return Err(format!("`init` takes no argument `{extra}`\n{USAGE}")),
            ("compare", path) if files.len() < 2 => files.push(path.to_string()),
            (c, path) if c != "compare" && files.is_empty() => files.push(path.to_string()),
            (_, extra) => return Err(format!("unexpected argument `{extra}`\n{USAGE}")),
        }
    }
    let file = files.first().cloned();
    if order.is_some() && file.is_some() {
        return Err(format!(
            "`fab --order` makes an order of the project and takes no file: an order names its boards in netsmith.toml\n{USAGE}"
        ));
    }
    if command == "compare" {
        let [a, b] = <[String; 2]>::try_from(files)
            .map_err(|_| format!("`compare` takes two files, A and B\n{USAGE}"))?;
        if map.is_some() && by_refdes {
            return Err(format!(
                "--map and --by-refdes both say which designators correspond; give one\n{USAGE}"
            ));
        }
        return Ok(Command::Compare {
            a,
            b,
            board,
            map,
            by_refdes,
            ignore_unconnected,
            loose_values,
            format,
        });
    }
    if command == "parity" {
        let board = file.map(PathBuf::from).ok_or_else(|| {
            format!("`parity` takes the board `fab` wrote (<board.kicad_pcb>)\n{USAGE}")
        })?;
        return Ok(Command::Parity {
            board,
            table,
            map,
            svg,
            format,
        });
    }
    if command == "lock" {
        let mode = lock_mode.ok_or_else(|| {
            format!(
                "`lock` needs `--update` (write the rows the solver picks now) or `--check` (say whether the lock is current)\n{USAGE}"
            )
        })?;
        return Ok(Command::Lock { file, mode });
    }
    if command == "view" {
        if ortho && perspective {
            return Err(format!(
                "--ortho and --perspective both say how to project; give one\n{USAGE}"
            ));
        }
        if ortho || perspective {
            view.change.ortho = Some(ortho);
        }
        if view.change.highlight.is_some() && view.change.select.is_some() {
            return Err(format!(
                "--highlight and --select both set the page's selection; give one (--highlight also lights limits)\n{USAGE}"
            ));
        }
        if view
            .change
            .select
            .iter()
            .flatten()
            .any(|id| id.starts_with("limit:"))
        {
            return Err(format!(
                "--select takes parts or one net, as a click does; a `limit:` is lit with --highlight\n{USAGE}"
            ));
        }
        view.change.board = board;
        return Ok(Command::View(view));
    }
    if command == "render" {
        if ortho && perspective {
            return Err(format!(
                "--ortho and --perspective both say how to project; give one\n{USAGE}"
            ));
        }
        render.perspective = perspective;
        return Ok(Command::Render {
            file,
            board,
            out,
            request: render,
        });
    }
    Ok(match command.as_str() {
        "init" => Command::Init { name },
        "build" => Command::Build {
            file,
            out,
            strict,
            locked,
            format,
            notes,
        },
        "check" => Command::Check {
            file,
            strict,
            format,
            notes,
        },
        "board3d" => Command::Board3d {
            file,
            board,
            json,
            glb,
            step,
        },
        "fab" => Command::Fab {
            file,
            out,
            route_timeout,
            allow_drc_violations,
            cross_check,
            strict,
            locked,
            constraints_warn,
            pins_locked,
            no_glb,
            order,
            quiet,
            format,
            notes,
        },
        _ => Command::Editor { file, port },
    })
}

/// `--view`'s name as the editor's route has it, with the error `render --view` gives.
fn parse_view(given: &str) -> Result<netsmith_editor::api::RenderView, String> {
    let view = netsmith_pcb::raster3d::View::from_name(given).ok_or_else(|| {
        let names: Vec<&str> = netsmith_pcb::raster3d::View::ALL
            .iter()
            .map(|v| v.name())
            .collect();
        format!("--view takes one of {}, not `{given}`", names.join(", "))
    })?;
    serde_json::from_value(serde_json::Value::String(view.name().to_string()))
        .map_err(|e| format!("--view `{given}` is not a view of the editor: {e}"))
}

/// Ids of `--highlight` or `--select`, comma-separated or repeated, added to `slot`; `none`
/// alone makes it an empty list (lights or selects nothing), which no id may join.
fn add_ids(slot: &mut Option<Vec<String>>, flag: &str, given: &str) -> Result<(), String> {
    let clash = || format!("{flag} `none` clears it, so no id goes with it");
    for id in given.split(',').map(str::trim).filter(|id| !id.is_empty()) {
        let cleared = slot.as_ref().is_some_and(Vec::is_empty);
        if id == "none" {
            if slot.is_some() && !cleared {
                return Err(clash());
            }
            *slot = Some(Vec::new());
        } else if cleared {
            return Err(clash());
        } else {
            slot.get_or_insert_with(Vec::new).push(id.to_string());
        }
    }
    Ok(())
}

/// `dxf import`.
fn parse_dxf(args: Vec<String>) -> Result<Command, String> {
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some("import") => {}
        other => {
            return Err(format!(
                "`dxf` takes `import <file.dxf>`, not `{}`\n{USAGE}",
                other.unwrap_or("")
            ));
        }
    }
    let (mut file, mut layers, mut holes_layers) = (None, Vec::new(), Vec::new());
    let (mut units, mut board, mut write) = (None, None, None);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--layer" => layers.push(args.next().ok_or("--layer needs a layer name")?),
            "--holes-layer" => {
                holes_layers.push(args.next().ok_or("--holes-layer needs a layer name")?);
            }
            "--units" => {
                let given = args.next().ok_or("--units needs mm, cm, m or inch")?;
                units =
                    Some(netsmith_dxf::Units::parse(&given).ok_or_else(|| {
                        format!("--units needs mm, cm, m or inch, not `{given}`")
                    })?);
            }
            "--board" => board = Some(args.next().ok_or("--board needs a board name")?),
            "--write" => {
                write = Some(PathBuf::from(
                    args.next().ok_or("--write needs the board's .nsm file")?,
                ));
            }
            flag if flag.starts_with("--") => {
                return Err(format!("`dxf import` has no option `{flag}`\n{USAGE}"));
            }
            path if file.is_none() => file = Some(PathBuf::from(path)),
            extra => return Err(format!("unexpected argument `{extra}`\n{USAGE}")),
        }
    }
    let file = file.ok_or_else(|| format!("`dxf import` takes a .dxf file\n{USAGE}"))?;
    Ok(Command::DxfImport {
        file,
        layers,
        holes_layers,
        units,
        board,
        write,
    })
}

/// `catalog refresh` and `catalog age`.
fn parse_catalog(args: Vec<String>) -> Result<Command, String> {
    let mut args = args.into_iter();
    let sub = args
        .next()
        .ok_or_else(|| format!("`catalog` takes `refresh`, `age` or `add`\n{USAGE}"))?;
    if sub == "add" {
        return parse_catalog_add(args.collect());
    }
    if sub != "refresh" && sub != "age" {
        return Err(format!(
            "`catalog` takes `refresh`, `age` or `add`, not `{sub}`\n{USAGE}"
        ));
    }
    let mut request = RefreshRequest::default();
    let mut days: Option<u32> = None;
    let number = |args: &mut std::vec::IntoIter<String>, flag: &str, what: &str| {
        let given = args.next().ok_or(format!("{flag} needs {what}"))?;
        given
            .parse::<u64>()
            .map_err(|_| format!("{flag} needs {what}, not `{given}`"))
    };
    while let Some(arg) = args.next() {
        match (sub.as_str(), arg.as_str()) {
            ("refresh", "--dry-run") => request.dry_run = true,
            ("refresh", "--offline") => request.offline = true,
            ("refresh", "--no-cache") => request.no_cache = true,
            (_, "--only") => request
                .only
                .push(args.next().ok_or("--only needs a catalog name")?),
            ("refresh", "--lcsc") => request.lcsc.push(
                args.next()
                    .ok_or("--lcsc needs an LCSC number like C25104")?,
            ),
            ("refresh", "--delay") => {
                request.delay_ms = Some(number(&mut args, "--delay", "a number of milliseconds")?);
            }
            ("refresh", "--cache-ttl") => {
                request.cache_ttl_hours =
                    Some(number(&mut args, "--cache-ttl", "a number of hours")?);
            }
            ("age", "--days") => {
                days = Some(
                    u32::try_from(number(&mut args, "--days", "a whole number of days")?)
                        .map_err(|_| "--days is too large".to_string())?,
                );
            }
            (_, flag) if flag.starts_with("--") => {
                return Err(format!("`catalog {sub}` has no option `{flag}`\n{USAGE}"));
            }
            (_, path) => request.paths.push(PathBuf::from(path)),
        }
    }
    if sub == "age" {
        return Ok(Command::CatalogAge {
            paths: request.paths,
            only: request.only,
            days: days.unwrap_or(crate::refresh::STALE_DAYS),
        });
    }
    if request.offline && (request.delay_ms.is_some() || request.cache_ttl_hours.is_some()) {
        return Err(format!(
            "--offline asks nothing, so --delay and --cache-ttl mean nothing with it\n{USAGE}"
        ));
    }
    Ok(Command::CatalogRefresh(request))
}

/// `catalog add --lcsc <C123> [--to <catalog.csv>] [--offline] [--no-cache]`.
fn parse_catalog_add(args: Vec<String>) -> Result<Command, String> {
    let (mut lcsc, mut to, mut offline, mut no_cache) = (None, None, false, false);
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--lcsc" => {
                lcsc = Some(
                    args.next()
                        .ok_or("--lcsc needs an LCSC number like C25104")?,
                );
            }
            "--to" => {
                to = Some(PathBuf::from(
                    args.next().ok_or("--to needs a catalog file")?,
                ))
            }
            "--offline" => offline = true,
            "--no-cache" => no_cache = true,
            other => {
                return Err(format!(
                    "`catalog add` takes `--lcsc <C123>`, `--to <catalog.csv>`, `--offline` and `--no-cache`, not `{other}`\n{USAGE}"
                ));
            }
        }
    }
    let lcsc = lcsc.ok_or_else(|| {
        format!("`catalog add` needs `--lcsc <C123>`, the number to add\n{USAGE}")
    })?;
    Ok(Command::CatalogAdd {
        lcsc,
        to,
        offline,
        no_cache,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, String> {
        parse_args(args.iter().map(|s| s.to_string()))
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
            Ok(Command::CatalogAdd {
                lcsc: "C2286".into(),
                to: Some(PathBuf::from("catalogs/led.csv")),
                offline: false,
                no_cache: false,
            })
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
            Ok(Command::CatalogAdd {
                to: None,
                offline: true,
                no_cache: true,
                ..
            })
        ));
        let e = parse(&["catalog", "add", "--to", "x.csv"]).unwrap_err();
        assert!(e.contains("`catalog add` needs `--lcsc <C123>`"), "{e}");
        let e = parse(&["catalog", "add", "--lcsc", "C1", "--dry-run"]).unwrap_err();
        assert!(e.contains("not `--dry-run`"), "{e}");
    }

    #[test]
    fn the_version_is_asked_for_three_ways_and_takes_no_argument() {
        for flag in ["--version", "-V", "version"] {
            assert_eq!(parse(&[flag]), Ok(Command::Version));
        }
        let e = parse(&["--version", "build"]).unwrap_err();
        assert!(
            e.contains("`--version` takes no argument, not `build`"),
            "{e}"
        );
    }

    #[test]
    fn help_is_asked_for_by_flag_command_or_after_a_command() {
        for args in [
            &["--help"][..],
            &["-h"],
            &["help"],
            &["build", "--help"],
            &["fab", "--order", "-h"],
            &["catalog", "refresh", "--help"],
            &["dxf", "-h"],
            &["kicad-libs", "--help"],
        ] {
            assert!(matches!(parse(args), Ok(Command::Help(_))), "{args:?}");
        }
    }

    #[test]
    fn an_unknown_command_is_an_error_that_points_to_help() {
        let e = parse(&["frobnicate"]).unwrap_err();
        assert!(e.contains("unknown command `frobnicate`"), "{e}");
        assert!(e.contains("netsmith --help"), "{e}");
        assert!(parse(&["frobnicate", "--help"]).is_err());
    }

    #[test]
    fn the_help_lists_every_command_the_dispatcher_accepts_and_the_flags() {
        let text = help();
        for (name, _) in COMMANDS {
            assert!(text.contains(&format!("  {name} ")), "{name}: {text}");
            // The command is accepted, not rejected as unknown: its own `--help` is help.
            assert_eq!(
                parse(&[name, "--help"]),
                Ok(Command::Help(Some(name.to_string()))),
                "{name}"
            );
        }
        for flag in [
            "--version",
            "--out",
            "--strict",
            "--locked",
            "--order",
            "--route-timeout",
            "--allow-drc-violations",
            "--cross-check",
            "--constraints",
            "--pins",
            "--port",
            "--board",
            "--by-refdes",
            "--dry-run",
            "--write",
        ] {
            assert!(text.contains(flag), "{flag}: {text}");
        }
        // Every command the parser has a branch for is in the table.
        for name in [
            "init",
            "build",
            "fab",
            "editor",
            "compare",
            "kicad-libs",
            "catalog",
            "dxf",
        ] {
            assert!(COMMANDS.iter().any(|(n, _)| *n == name), "{name}");
        }
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
                "--holes-layer",
                "HOLES",
                "--units",
                "inch",
                "--board",
                "Case",
                "--write",
                "case.nsm"
            ]),
            Ok(Command::DxfImport {
                file: "case.dxf".into(),
                layers: vec!["EDGE".into()],
                holes_layers: vec!["HOLES".into()],
                units: Some(netsmith_dxf::Units::Inches),
                board: Some("Case".into()),
                write: Some("case.nsm".into()),
            })
        );
        let e = parse(&["dxf", "import"]).unwrap_err();
        assert!(e.contains("takes a .dxf file"), "{e}");
        let e = parse(&["dxf", "import", "a.dxf", "--units", "furlongs"]).unwrap_err();
        assert!(e.contains("not `furlongs`"), "{e}");
        assert!(parse(&["dxf", "export", "a.dxf"]).is_err());
        assert!(parse(&["dxf", "import", "a.dxf", "--bogus"]).is_err());
    }

    #[test]
    fn kicad_libs_import_takes_one_or_two_library_tables() {
        assert_eq!(
            parse(&["kicad-libs", "import", "sym-lib-table"]),
            Ok(Command::KicadLibsImport {
                sym_lib_table: "sym-lib-table".into(),
                fp_lib_table: None
            })
        );
        assert_eq!(
            parse(&["kicad-libs", "import", "s", "f"]),
            Ok(Command::KicadLibsImport {
                sym_lib_table: "s".into(),
                fp_lib_table: Some("f".into())
            })
        );
        let e = parse(&["kicad-libs"]).unwrap_err();
        assert!(e.contains("`kicad-libs` takes `import"), "{e}");
        assert!(parse(&["kicad-libs", "export", "s"]).is_err());
    }

    #[test]
    fn catalog_refresh_takes_paths_and_the_flags_of_a_polite_opt_in_run() {
        assert_eq!(
            parse(&["catalog", "refresh"]),
            Ok(Command::CatalogRefresh(RefreshRequest::default()))
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
            Ok(Command::CatalogRefresh(RefreshRequest {
                paths: vec!["catalogs/a.csv".into(), "data".into()],
                dry_run: true,
                offline: false,
                only: vec!["ldo".into(), "led".into()],
                lcsc: vec!["C5446".into()],
                delay_ms: Some(1500),
                cache_ttl_hours: Some(6),
                no_cache: true,
            }))
        );
        let Ok(Command::CatalogRefresh(r)) = parse(&["catalog", "refresh", "--offline"]) else {
            panic!()
        };
        assert!(r.offline);
        let e = parse(&["catalog", "refresh", "--delay", "soon"]).unwrap_err();
        assert!(
            e.contains("--delay needs a number of milliseconds, not `soon`"),
            "{e}"
        );
        let e = parse(&["catalog", "refresh", "--offline", "--delay", "9"]).unwrap_err();
        assert!(e.contains("--offline asks nothing"), "{e}");
        let e = parse(&["catalog", "refresh", "--days", "3"]).unwrap_err();
        assert!(
            e.contains("`catalog refresh` has no option `--days`"),
            "{e}"
        );
        assert!(parse(&["catalog", "refresh", "--lcsc"]).is_err());
    }

    #[test]
    fn catalog_age_takes_paths_a_catalog_and_the_days_with_90_as_the_default() {
        assert_eq!(
            parse(&["catalog", "age"]),
            Ok(Command::CatalogAge {
                paths: vec![],
                only: vec![],
                days: 90
            })
        );
        assert_eq!(
            parse(&["catalog", "age", "x.csv", "--days", "30", "--only", "ldo"]),
            Ok(Command::CatalogAge {
                paths: vec!["x.csv".into()],
                only: vec!["ldo".into()],
                days: 30
            })
        );
        let e = parse(&["catalog", "age", "--dry-run"]).unwrap_err();
        assert!(e.contains("`catalog age` has no option `--dry-run`"), "{e}");
        let e = parse(&["catalog"]).unwrap_err();
        assert!(
            e.contains("`catalog` takes `refresh`, `age` or `add`"),
            "{e}"
        );
        let e = parse(&["catalog", "prices"]).unwrap_err();
        assert!(e.contains("not `prices`"), "{e}");
    }

    #[test]
    fn fab_takes_a_file_and_an_out_directory_like_build() {
        assert_eq!(
            parse(&["fab", "a.nsm", "--out", "x"]),
            Ok(Command::Fab {
                file: Some("a.nsm".into()),
                out: Some("x".into()),
                route_timeout: None,
                allow_drc_violations: false,
                cross_check: false,
                strict: false,
                locked: false,
                constraints_warn: None,
                pins_locked: None,
                no_glb: false,
                order: None,
                quiet: false,
                format: crate::findings::Format::Text,
                notes: crate::findings::NotesMode::All,
            })
        );
        assert!(parse(&["fab", "a.nsm", "--port", "9"]).is_err());
    }

    #[test]
    fn fab_takes_the_flags_that_waive_the_design_rules_check_and_that_cross_check_with_kicad() {
        assert_eq!(
            parse(&["fab", "--allow-drc-violations", "--cross-check"]),
            Ok(Command::Fab {
                file: None,
                out: None,
                route_timeout: None,
                allow_drc_violations: true,
                cross_check: true,
                strict: false,
                locked: false,
                constraints_warn: None,
                pins_locked: None,
                no_glb: false,
                order: None,
                quiet: false,
                format: crate::findings::Format::Text,
                notes: crate::findings::NotesMode::All,
            })
        );
        assert!(parse(&["build", "--cross-check"]).is_err());
    }

    #[test]
    fn fab_takes_a_route_timeout_in_whole_seconds() {
        assert_eq!(
            parse(&["fab", "--route-timeout", "30"]),
            Ok(Command::Fab {
                file: None,
                out: None,
                route_timeout: Some(30),
                allow_drc_violations: false,
                cross_check: false,
                strict: false,
                locked: false,
                constraints_warn: None,
                pins_locked: None,
                no_glb: false,
                order: None,
                quiet: false,
                format: crate::findings::Format::Text,
                notes: crate::findings::NotesMode::All,
            })
        );
        let e = parse(&["fab", "--route-timeout", "soon"]).unwrap_err();
        assert!(
            e.contains("whole number of seconds") && e.contains("`soon`"),
            "{e}"
        );
        assert!(parse(&["fab", "--route-timeout"]).is_err());
        assert!(parse(&["build", "--route-timeout", "3"]).is_err());
    }

    #[test]
    fn fab_order_takes_a_name_or_lists_the_orders() {
        let fab = |order| {
            Ok(Command::Fab {
                file: None,
                out: None,
                route_timeout: None,
                allow_drc_violations: false,
                cross_check: false,
                strict: false,
                locked: false,
                constraints_warn: None,
                pins_locked: None,
                no_glb: false,
                order: Some(order),
                quiet: false,
                format: crate::findings::Format::Text,
                notes: crate::findings::NotesMode::All,
            })
        };
        assert_eq!(
            parse(&["fab", "--order", "devkit"]),
            fab(OrderRequest::Make("devkit".into()))
        );
        assert_eq!(parse(&["fab", "--order"]), fab(OrderRequest::List));
        assert_eq!(
            parse(&["fab", "--order", "--out", "x"]),
            Ok(Command::Fab {
                file: None,
                out: Some("x".into()),
                route_timeout: None,
                allow_drc_violations: false,
                cross_check: false,
                strict: false,
                locked: false,
                constraints_warn: None,
                pins_locked: None,
                no_glb: false,
                order: Some(OrderRequest::List),
                quiet: false,
                format: crate::findings::Format::Text,
                notes: crate::findings::NotesMode::All,
            })
        );
        assert_eq!(
            parse(&[
                "fab",
                "--out",
                "x",
                "--order",
                "d",
                "--route-timeout",
                "9",
                "--allow-drc-violations"
            ]),
            Ok(Command::Fab {
                file: None,
                out: Some("x".into()),
                route_timeout: Some(9),
                allow_drc_violations: true,
                cross_check: false,
                strict: false,
                locked: false,
                constraints_warn: None,
                pins_locked: None,
                no_glb: false,
                order: Some(OrderRequest::Make("d".into())),
                quiet: false,
                format: crate::findings::Format::Text,
                notes: crate::findings::NotesMode::All,
            })
        );
        let e = parse(&["fab", "a.nsm", "--order", "d"]).unwrap_err();
        assert!(e.contains("takes no file") && e.contains("usage:"), "{e}");
        assert!(parse(&["build", "--order", "d"]).is_err());
    }

    #[test]
    fn build_and_fab_take_strict_for_one_run() {
        assert_eq!(
            parse(&["build", "--strict"]),
            Ok(Command::Build {
                file: None,
                out: None,
                strict: true,
                locked: false,
                format: crate::findings::Format::Text,
                notes: crate::findings::NotesMode::All,
            })
        );
        let Ok(Command::Fab { strict, .. }) = parse(&["fab", "a.nsm", "--strict"]) else {
            panic!("fab takes --strict");
        };
        assert!(strict);
        assert!(parse(&["editor", "--strict"]).is_err());
    }

    #[test]
    fn build_and_fab_take_locked_and_nothing_else_does() {
        let Ok(Command::Build { locked, strict, .. }) = parse(&["build", "--locked"]) else {
            panic!("build takes --locked");
        };
        assert!(locked && !strict);
        let Ok(Command::Fab { locked, .. }) = parse(&["fab", "a.nsm", "--locked", "--strict"])
        else {
            panic!("fab takes --locked");
        };
        assert!(locked);
        let Ok(Command::Fab { locked, .. }) = parse(&["fab"]) else {
            panic!("fab");
        };
        assert!(!locked, "a run may change the lock by default");
        assert!(parse(&["editor", "--locked"]).is_err());
        assert!(parse(&["compare", "a", "b", "--locked"]).is_err());
    }

    #[test]
    fn fab_takes_no_glb_to_leave_the_3d_scene_out_of_the_package() {
        let no_glb = |args: &[&str]| match parse(args) {
            Ok(Command::Fab { no_glb, .. }) => no_glb,
            other => panic!("{args:?}: {other:?}"),
        };
        assert!(no_glb(&["fab", "a.nsm", "--no-glb"]));
        assert!(
            !no_glb(&["fab", "a.nsm"]),
            "the scene is written by default"
        );
        assert!(parse(&["build", "--no-glb"]).is_err());
        assert!(help().contains("--no-glb"));
    }

    #[test]
    fn fab_takes_pins_auto_or_locked_for_one_run() {
        let pins = |args: &[&str]| match parse(args) {
            Ok(Command::Fab { pins_locked, .. }) => pins_locked,
            other => panic!("{args:?}: {other:?}"),
        };
        assert_eq!(pins(&["fab", "a.nsm", "--pins", "locked"]), Some(true));
        assert_eq!(pins(&["fab", "a.nsm", "--pins", "auto"]), Some(false));
        assert_eq!(
            pins(&["fab", "a.nsm"]),
            None,
            "the manifest decides by default"
        );
        let e = parse(&["fab", "--pins", "swap"]).unwrap_err();
        assert!(
            e.contains("--pins takes `auto`") && e.contains("`swap`"),
            "{e}"
        );
        assert!(parse(&["fab", "--pins"]).is_err());
        assert!(parse(&["build", "--pins", "locked"]).is_err());
    }

    #[test]
    fn fab_takes_constraints_warn_for_one_run() {
        let Ok(Command::Fab {
            constraints_warn,
            strict,
            ..
        }) = parse(&["fab", "a.nsm", "--constraints", "warn"])
        else {
            panic!("fab takes --constraints warn");
        };
        assert!(constraints_warn == Some(true) && !strict);
        let Ok(Command::Fab {
            constraints_warn, ..
        }) = parse(&["fab", "a.nsm", "--constraints", "error"])
        else {
            panic!("fab takes --constraints error");
        };
        assert_eq!(constraints_warn, Some(false));
        let Ok(Command::Fab {
            constraints_warn, ..
        }) = parse(&["fab", "a.nsm"])
        else {
            panic!("fab");
        };
        assert_eq!(constraints_warn, None, "the manifest decides by default");
        let e = parse(&["fab", "--constraints", "off"]).unwrap_err();
        assert!(
            e.contains("takes `warn`") && e.contains("or `error`") && e.contains("`off`"),
            "{e}"
        );
        assert!(parse(&["fab", "--constraints"]).is_err());
        assert!(parse(&["build", "--constraints", "warn"]).is_err());
    }

    #[test]
    fn build_leaves_the_out_directory_to_the_target() {
        assert_eq!(
            parse(&["build", "a.nsm"]),
            Ok(Command::Build {
                file: Some("a.nsm".into()),
                out: None,
                strict: false,
                locked: false,
                format: crate::findings::Format::Text,
                notes: crate::findings::NotesMode::All,
            })
        );
    }

    #[test]
    fn build_takes_an_out_directory_before_or_after_the_file() {
        let expected = Ok(Command::Build {
            file: Some("a.nsm".into()),
            out: Some("x".into()),
            strict: false,
            locked: false,
            format: crate::findings::Format::Text,
            notes: crate::findings::NotesMode::All,
        });
        assert_eq!(parse(&["build", "a.nsm", "--out", "x"]), expected);
        assert_eq!(parse(&["build", "--out", "x", "a.nsm"]), expected);
    }

    #[test]
    fn the_editor_defaults_to_port_8787() {
        assert_eq!(
            parse(&["editor", "a.nsm"]),
            Ok(Command::Editor {
                file: Some("a.nsm".into()),
                port: 8787
            })
        );
    }

    #[test]
    fn the_editor_takes_a_port() {
        assert_eq!(
            parse(&["editor", "a.nsm", "--port", "9000"]),
            Ok(Command::Editor {
                file: Some("a.nsm".into()),
                port: 9000
            })
        );
    }

    #[test]
    fn a_bad_port_says_what_was_given() {
        let e = parse(&["editor", "a.nsm", "--port", "http"]).unwrap_err();
        assert!(e.contains("`http`"), "{e}");
        assert!(parse(&["editor", "a.nsm", "--port", "70000"]).is_err());
        assert!(parse(&["editor", "a.nsm", "--port"]).is_err());
    }

    #[test]
    fn options_belong_to_their_command() {
        assert!(parse(&["build", "a.nsm", "--port", "1"]).is_err());
        assert!(parse(&["editor", "a.nsm", "--out", "x"]).is_err());
    }

    #[test]
    fn mistakes_show_the_usage() {
        for args in [
            &[][..],
            &["frobnicate"],
            &["init", "dir"],
            &["init", "--out", "x"],
            &["editor", "a.nsm", "b.nsm"],
        ] {
            let e = parse(args).unwrap_err();
            assert!(e.contains("usage:"), "{args:?}: {e}");
        }
    }

    #[test]
    fn with_no_file_a_command_acts_on_the_project() {
        assert_eq!(
            parse(&["build"]),
            Ok(Command::Build {
                file: None,
                out: None,
                strict: false,
                locked: false,
                format: crate::findings::Format::Text,
                notes: crate::findings::NotesMode::All,
            })
        );
        assert_eq!(
            parse(&["fab", "--out", "x"]),
            Ok(Command::Fab {
                file: None,
                out: Some("x".into()),
                route_timeout: None,
                allow_drc_violations: false,
                cross_check: false,
                strict: false,
                locked: false,
                constraints_warn: None,
                pins_locked: None,
                no_glb: false,
                order: None,
                quiet: false,
                format: crate::findings::Format::Text,
                notes: crate::findings::NotesMode::All,
            })
        );
        assert_eq!(
            parse(&["editor", "--port", "9"]),
            Ok(Command::Editor {
                file: None,
                port: 9
            })
        );
    }

    #[test]
    fn compare_takes_two_files_and_its_options() {
        assert_eq!(
            parse(&["compare", "a.kicad_sch", "b.nsm"]),
            Ok(Command::Compare {
                a: "a.kicad_sch".into(),
                b: "b.nsm".into(),
                board: None,
                map: None,
                by_refdes: false,
                ignore_unconnected: false,
                loose_values: false,
                format: crate::findings::Format::Text,
            })
        );
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
            Ok(Command::Compare {
                a: "a.xml".into(),
                b: "b.xml".into(),
                board: Some("Led".into()),
                map: Some("m.txt".into()),
                by_refdes: false,
                ignore_unconnected: true,
                loose_values: true,
                format: crate::findings::Format::Text,
            })
        );
    }

    #[test]
    fn compare_mistakes_show_the_usage() {
        for args in [
            &["compare"][..],
            &["compare", "a.nsm"],
            &["compare", "a", "b", "c"],
            &["compare", "a", "b", "--map"],
            &["compare", "a", "b", "--board"],
            &["compare", "a", "b", "--map", "m", "--by-refdes"],
            &["compare", "a", "b", "--port", "1"],
            &["build", "a.nsm", "--loose-values"],
        ] {
            let e = parse(args).unwrap_err();
            assert!(e.contains("usage:") || e.contains("needs"), "{args:?}: {e}");
        }
        let e = parse(&["compare", "a", "b", "--map", "m", "--by-refdes"]).unwrap_err();
        assert!(e.contains("--map") && e.contains("--by-refdes"), "{e}");
        let e = parse(&["compare", "a.nsm"]).unwrap_err();
        assert!(e.contains("two files"), "{e}");
    }

    #[test]
    fn init_takes_only_a_name() {
        assert_eq!(parse(&["init"]), Ok(Command::Init { name: None }));
        assert_eq!(
            parse(&["init", "--name", "demo"]),
            Ok(Command::Init {
                name: Some("demo".into())
            })
        );
        assert!(parse(&["init", "a.nsm"]).unwrap_err().contains("`a.nsm`"));
        assert!(parse(&["init", "--name"]).is_err());
    }

    fn view_request(args: &[&str]) -> crate::view_client::Request {
        let mut line = vec!["view"];
        line.extend_from_slice(args);
        match parse(&line) {
            Ok(Command::View(request)) => request,
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
            (&["--view", "sideways"], "--view takes one of top, bottom"),
            (&["--view"], "--view needs"),
            (&["--section", "w=1"], "--section needs"),
            (&["--section", "x"], "--section needs"),
            (&["--ortho", "--perspective"], "give one"),
            (&["--highlight", "J1", "--select", "J1"], "give one"),
            (&["--select", "limit:lid"], "--highlight"),
            (&["--colour-by", "shade"], "`provenance`"),
            (&["--height-preview", "0"], "0.1 to 50"),
            (&["--height-preview", "51"], "0.1 to 50"),
            (&["--height-preview", "tall"], "0.1 to 50"),
            (&["--port", "0"], "1 to 65535"),
            (&["--port", "http"], "1 to 65535"),
            (&["--picture"], "--picture needs"),
            (&["--out", "a.png"], "--picture"),
            (&["--size", "100x100"], "no option `--size`"),
            (&["examples/led.nsm"], "takes no file"),
        ] {
            let mut line = vec!["view"];
            line.extend_from_slice(args);
            let e = parse(&line).unwrap_err();
            assert!(e.contains(says), "{args:?}: {e}");
        }
    }

    #[test]
    fn view_is_listed_and_has_help() {
        assert!(COMMANDS.iter().any(|(name, _)| *name == "view"));
        assert!(USAGE.contains("netsmith view [--port <n>]"));
        assert_eq!(
            parse(&["view", "--help"]),
            Ok(Command::Help(Some("view".to_string())))
        );
    }
}
