# netsmith

Circuits as code, in Rust. A board is a set of parts and the nets that join their pins,
written as text. netsmith checks it, picks real parts that work in the worst case, and
exports files for CAD tools and for a board house. KiCad 9 is the first target; an in-house
editor and other exporters come later.

Status: early. Source goes in and, for boards made of resistors, capacitors, LEDs, headers
and a fixed-output linear regulator, a KiCad schematic comes out, and so does a placed,
routed PCB with a Gerber package for JLCPCB, on two layers or on four with ground and supply
planes inside. The LED, regulator and four-layer examples in `examples/` do both and pass
KiCad's checks.

## Goals

- Describe a real multi-board design (a carrier board and plug-in boards) in this language.
- Export to KiCad first, and later to other CAD tools and an in-house editor.
- Pick real parts with a solver that checks every candidate against the worst case.

See `docs/LANGUAGE.md` for the language guide, `GLOSSARY.md` for the vocabulary, `docs/adr/` for the decisions behind the design, and
`docs/ROADMAP.md` for what is done, what is open, and where to pick up. `docs/CONCEPTS.md` explains how
parts, modules, boards and systems combine, and `docs/PROJECT-LAYOUT.md` how to lay out a repo with several boards.

## Try it

Install netsmith one of two ways; neither needs Node:

- Download the archive for your system (`netsmith-<version>-<target>.tar.gz`, `.zip` on Windows)
  from the project's releases, check it against its `.sha256` file, and put the `netsmith` binary
  on your `PATH`. `scripts/release-build.sh` makes these archives (a release build, a smoke test,
  the binary with `LICENSE`, `README.md` and checksums). No release has been published yet, and
  only the Linux x86_64 archive has been built and tried; `.github/workflows/release.yml` is an
  untried draft for the macOS and Windows ones.
- Build it from a checkout: `cargo install --path crates/netsmith`. The editor's page is
  committed in `crates/netsmith-editor/web-dist`, so building from a checkout or a package
  needs only Rust.

`build` and `fab` read KiCad's symbol and footprint libraries (see "Known limits"), so install
KiCad 9 (8 also works as a fallback; netsmith is tested with 9). netsmith finds the `symbols` and
`footprints` folders by itself: first the variables `NETSMITH_KICAD_SYMBOLS` and
`NETSMITH_KICAD_FOOTPRINTS` if you set them, then the folders beside the `kicad-cli` that
`$KICAD_CLI` or your `PATH` names, then the usual install places (Linux `/usr/share/kicad`
and `/usr/local/share/kicad`, snap, flatpak, macOS `KiCad.app`, Windows `%ProgramFiles%\KiCad`,
and the Windows install seen from WSL). Only the last was tried on a real install; the others
follow the layouts the packages use and are unverified. If nothing is found, `build` and `fab`
stop with every place they looked at and the two variables to set; a design that needs no
stock library (only a project's own, see `kicad.toml`) builds without KiCad. `netsmith
--version` prints the version.

Start a project in a directory (a git repository or not; netsmith does not run git):

```sh
mkdir blink && cd blink
netsmith init          # netsmith.toml, src/main.nsm (a working LED board), .gitignore, README.md,
                       # AGENTS.md, .agents/skills/, docs/netsmith/LANGUAGE.md
netsmith build         # a KiCad schematic in out/
netsmith editor        # the source and the board in a browser
netsmith fab           # files to upload to JLCPCB in fab/
netsmith dxf import case.dxf --layer EDGE --holes-layer HOLES   # the outline and holes of an enclosure, as source
```

`init` never overwrites: if `netsmith.toml` or `src/main.nsm` exists it writes nothing and
lists them. The agent files are for coding agents working in the project (ADR 0015): the
project's `AGENTS.md`, the `netsmith-*` skills in `.agents/skills/` (with `.claude/skills` a
relative link to them, or a copy where links fail) and the language guide. They are copies
made when you run `init`; a newer netsmith does not update them. An `AGENTS.md` that exists is
left alone and `init` tells you to add a pointer to the guide and skills. With no file, `build`, `fab` and `editor` act on the files in `netsmith.toml`'s
`top_level`, from any directory under the project. Other files under `src/` are modules: a
board in `src/main.nsm` imports `src/power.nsm` with `import src::power::Name`. The
manifest is described in `docs/adr/0011-a-project-has-a-manifest.md`. A top-level file may
hold a `system` as well as boards; its boards are built into `out/<System>/<member>/`.

Or work on one file, with no project, from this repository:

```sh
cargo test
cargo run -p netsmith -- build examples/led.nsm --out out     # a KiCad schematic
cargo run -p netsmith -- editor examples/led.nsm              # the editor in a browser
cargo run -p netsmith -- fab examples/led.nsm --out fab       # files to upload to JLCPCB
cargo run -p netsmith -- fab examples/two_boards.nsm --out fab # two boards, a package each
cargo run -p netsmith -- compare out/LedExample/LedExample.kicad_sch fab/LedExample/LedExample.kicad_pcb
```

A part's price and stock come from a **supply provider** when the bill of materials or `fab` is
made (ADR 0047, provisional), not from the catalog files: the `[supply]` table of `netsmith.toml`
names it (only `provider = "file"` is built, a CSV of quotes on this machine; no provider asks a
supplier), the answer says whether it is JLCPCB assembly stock or LCSC retail stock, and where no
provider has the part price and stock are **unknown** (never guessed, never 0). A catalog row's own
`price`, `stock` and `date` cells (JLCPCB's numbers, entered by hand) are read only as the last
resort; the catalogs that ship with netsmith leave them and `checked` empty, and the solver never
reads them. `netsmith catalog refresh` (opt-in, ADR 0039) is a
**check, not a data update**: it asks LCSC's product record for each row and compares the
manufacturer's number and package with the row's. A row that agrees is `VERIFIED` and gets the day
in its `checked` column, the one cell the command ever writes (a file without the column is
checked and not written). It never writes `price`, `stock`, `price_breaks`, `library` or `date`,
because the board is assembled from JLCPCB's library and LCSC's price and stock differ from
JLCPCB's. Where LCSC's stock looks different (it lists none), the report says so as a note: read
JLCPCB's parts page by hand. A row whose manufacturer's number or package disagrees with LCSC's
record is reported `CONFLICT`, a part LCSC no longer lists `MISSING`; neither is changed or deleted
(`--dry-run` prints the diff; `--only` and `--lcsc` choose rows; `--offline` uses what an earlier
run kept). **JLCPCB data is not fetched**. `netsmith catalog age` lists the rows whose price and stock
were read more than 90 days ago (by the provider, else by the row's `date`) and the rows whose price
and stock are unknown. The solver never needs the network and does not read stock, so a row with
a `stock` of 0 is offered like any other (ADR 0047). LCSC's endpoint is unofficial and **LCSC's terms of use have not been read**: a
person must read them before this is advertised. Official APIs (JLCAPI, the LCSC API; both need an
account) are future work, and would let a refresh fetch JLCPCB's own numbers. Meanwhile it asks
politely and only when told to.

`compare` tells you whether two designs are the same circuit, by connectivity and not by net
name or designator: each argument is a `.kicad_sch`, a `.kicad_pcb`, a KiCad netlist (`.xml`)
or a `.nsm` file. Use it to check a ported design against the KiCad original. It prints
`match` with what corresponds to what, or each difference by net and pin, and exits 0, 1 or 2
(match, difference, or a mistake or an undecided comparison). Reading a `.kicad_sch` needs
KiCad's command-line tool, found as `fab` finds it. See ADR 0017 and `docs/LANGUAGE.md`.

`fab` checks the board, picks parts that keep the LED within its limits, places and routes
a PCB to the board's manufacturing process (`.process: TwoLayer` or `FourLayer`), runs netsmith's own design rules check, and writes
`fab/LedExample/`: `LedExample-gerbers.zip`, `bom.csv`, `cpl.csv`, `fab-notes.txt`, the
`.kicad_pcb` and `.kicad_pro`, and an `.svg` of the board. It stops, with the reason, if a
requirement cannot be met, a connection cannot be routed, the design rules fail, or a part
that should be assembled has no LCSC number. The Gerbers, the drill files and the placement
file and the design rules check are netsmith's own (ADR 0034, ADR 0037); KiCad is not needed for
any of it. The check reads the board it wrote and the `.kicad_pro` beside it and reports each
finding with a rule, the numbers, a place and the items involved; errors stop `fab` (and `fab
--order`) unless you pass `--allow-drc-violations`, in which case `fab-notes.txt` says `design
rules check: NOT clean, packaged anyway` (a requirement on where a part goes is not a design rule and is never waived this way); warnings (silkscreen over a pad, a solder mask web
narrower than the process makes, a via in a pad the process cannot fill) are listed in the
notes. `--cross-check` is the optional witness: it plots the board with KiCad's command-line
tool (`$KICAD_CLI`, then `kicad-cli` on `PATH`, then Windows KiCad 9 under WSL) as well and
compares the two sets of files (bitmaps of every layer, the holes, the placement) and the two
design rules checks (every finding, by rule and place), failing with the differences and a
picture of each layer that differs; it needs KiCad. It says on
stderr which stage it is in (placing, routing with how many connections are done), so a long
run is never silent, and it gives the router 300 seconds for each board: `--route-timeout
<secs>` changes that (`0` is no limit). When the time or the router's tries run out, `fab`
stops and lists each connection left by net and pad.

To order only some boards, and several copies of each, name an order in `netsmith.toml`
(ADR 0035):

```toml
[order.devkit.boards]
"DevKit/carrier" = 2
Unit = 10
```

`netsmith fab --order devkit` makes only those boards into `fab/orders/devkit/` (a system's name
alone is all its members) and adds `order.txt` (layers, size, thickness, hand-solder parts, the
notes to read before you order, the unverified items) and `order-bom.csv` (the bill of materials of
the whole order: copies times parts, no spares). `netsmith fab --order` lists the orders. A key
may be a panel (below), and its quantity then counts panels.

A `panel` repeats one stand-alone board in a grid of V-cut cells (ADR 0040):

```nsm-fragment
panel LedPanel {
    .cell: LedTile
    .fab: VCutPanel {            // import std::mfg::jlcpcb::VCutPanel
        .columns &= 3
        .rows &= 2
        .rail &= 5mm             // handling rails with tooling holes and fiducials
    }
}
```

`netsmith fab examples/led_panel.nsm` writes `fab/LedPanel/`: one Gerber set, drill file, `bom.csv`
and `cpl.csv` for the whole panel (designators carry the cell: `R1_3`), `fab-notes.txt`, the panel
as a `.kicad_pcb` (so `--cross-check` compares it with KiCad's plot) and `panel.svg`, and the
board's own package in `fab/LedPanel/cell/LedTile/`. The checks use JLCPCB's published V-cut
numbers (`std/mfg/jlcpcb.nsm`, sources in `data/jlcpcb/README.md`); the editor draws the panel in
the PCB view. Only identical boards on a V-cut grid are built (P0); mixed boards, routed tabs and
mouse bites are on the roadmap.

A file with a `system` builds each of its boards: `build` writes `out/<System>/<board>/` for
each (`out/TwoBoards/power/`, `out/TwoBoards/led/`), `fab` a package in the same place, and
the editor lists them as `TwoBoards/power` and `TwoBoards/led` in its board selector. A board a
system places is built as that system's and not again alone. See `examples/two_boards.nsm`, whose
two boards are mated through a header and a socket, and `examples/carrier_and_cards.nsm`, a
carrier with two sockets and two plug-in cards that carry power, a UART and an SPI bus over the
mates (the editor's System view lists each mate's contacts and the ground groups).

## The editor

`netsmith editor` (with no file, the project's boards; with a file, that file) serves the
editor on `127.0.0.1:8787` and prints the address to open in a browser. It works offline: the
page is inside the binary, and nothing leaves the machine. On the left is the project tree
(top-level files with the boards and systems in them, modules grouped by folder with a filter
box, the manifest, and the standard library, read-only). In the middle is the source, with `.nsm` highlighting, the compiler's and
the solver's real messages as squiggles while you type (checked in memory, nothing written until
`Ctrl+S`), a problems list that jumps to the line (errors and warnings, with what the solver
left alone in a closed Notes section), and `Ctrl+click` or `F12` to go to a definition. On the right are the views of the board selected at the top: the schematic (click a
part to go to its declaration; the cursor on a declaration marks the part; a board of more
than 16 parts is drawn by module, a box for each instance, with a list to jump to one and nets
between boxes as named stubs), the power tree (each
rail with its voltage, its loads and the current it can give, from the solved model), the
system (the boards of a `system` and their joins, or the members of a board), the bill of
materials (grouped as `bom.csv` is, with LCSC number, Basic or Extended, price and stock where a supply
provider or the row gives them, `unknown` where not), the PCB (placed at once, routed on demand with its stages and a Cancel button, layers to
show and hide; a canvas to select parts by click, box or list, with their pads, nets and ratsnest, move
them by dragging on a grid, rotate (`R`), flip (`F`) and lock (`L`) them, draw and move keepouts, and
undo and redo every edit; each edit is written to the layout file `<stem>.pcb.toml`; a list of what is
wrong with the board, with where, from netsmith's placement check, its connection check and its design rules check, that zooms to a finding on a click and can also ask KiCad's check) and a build
panel (`Build` writes the KiCad schematics, `Fab` runs the design rules check and offers the
Gerber zip, BOM and placement file to download, and an Order section makes a named order of
`netsmith.toml` with its summary and combined BOM to download). The schematic's layout is automatic. A long job (routing, `fab`, a build) runs while the editor
stays usable and says how long it took. The bill of materials says where each line's price and stock come from and when they were read, marks one older than 90 days, says `unknown` where no provider has them, and has a "Check parts against LCSC" button (the same code as `catalog refresh`, for the project's own catalog files, with a preview and an offline mode; it writes no price or stock and says so). A file
that changes on disk is read again; a mistake in the source is shown and the last drawing that
built stays on screen, marked stale.

Building netsmith needs no Node: the editor's page is committed as a built bundle in
`crates/netsmith-editor/web-dist`, stamped with a hash of the bytes of the sources in
`crates/netsmith-editor/web` (Preact and Vite) it was built from, and `cargo build` embeds it when
the hash is the sources' hash (file times are not used). Changing the sources needs Node 20 or
newer: `cargo build` then builds the page with `npm ci && npm run build`, and
`scripts/web-bundle update` rebuilds the committed bundle, which you commit with the change
(`scripts/web-bundle check` and a test fail when it is not what the sources build to). With
changed sources and no Node, the build warns and embeds the older committed bundle; if there is no
bundle, `netsmith editor` says why the page is missing and the command that builds it by hand.

**Read `fab-notes.txt` before you order.** It lists what netsmith could not verify,
for example the pick-and-place rotation of a package that no board of yours has been
assembled with. Check JLCPCB's order preview for those.

A four-layer board (`examples/four_layer.nsm`) names the net of each inner layer in its
process (`.process.inner1.net &= "GND"`), and `fab` pours them as planes (ADR 0012). The Gerbers
hold netsmith's own fill of every plane and pour (clearance round other nets and holes, thermal
reliefs on its own pads, ADR 0034). netsmith's design rules check keeps to the plane by
construction and joins the pads, vias and tracks of a plane's net that lie under its outline
(ADR 0037). KiCad's check does not fill zones, so `fab --cross-check` has KiCad's Python
(`python.exe` beside `kicad-cli.exe`, or `python3` with `pcbnew`) fill them on a copy before it
asks KiCad; the `.kicad_pcb` `fab` writes keeps them unfilled until you open it in KiCad and
press `B`.

A board can also say how wide the tracks of a rail are (`.min_track` on a `Rail`), how big it is
and what is cut out of it (`.outline`, `Cutout`), where the outer layers are poured with a net
(`.process.top.net &= "GND"`, with stitching vias), where tracks, vias, copper or parts are
not allowed (`Keepout`, on a board or carried by a part) and what is printed on the silkscreen
(`Label`). Where things are, when the source does not say, is in an optional layout file beside
it, `<stem>.pcb.toml`. `examples/outline_and_pours.nsm` uses all of it, and `docs/LANGUAGE.md`
("The shape of a board") explains each (ADR 0020, ADR 0029, ADR 0032).

A case, a panel or a carrier can fix where a part goes. The board says so on the built-in `.place`
of the part or module (`.usb.place.left &= -0.5mm +/- 0.05mm`, also `x`, `y`, `right`, `top`,
`bottom`, `rotation` and `side`), measured from the part's body (its `F.Fab` outline, else its
courtyard) and from the outline's top left corner (ADR 0045). The placer keeps to the requirements, a
single value fixes the part like a lock, the layout file keeps the solved position, and `fab` measures
every requirement on the board and stops with the measured value, the window allowed and how far over
when one is not met. That is its own policy, not a guideline: `constraints = "warn"` under
`[project]` in `netsmith.toml`, or `--constraints warn` for one run, lists them as warnings
instead; `--constraints error` forces errors over a manifest that says `warn`, and
`--allow-drc-violations` does not waive them. `docs/LANGUAGE.md` ("Where a part goes") explains it.

A net's track width is checked against the worst-case current the solver found on it, by the
IPC-2221 formula for the copper weight and temperature rise of the process (10 K unless
`.process.max_temperature_rise` says otherwise; ADR 0036). A rail with no `.min_track` is checked
at the process's least track. This is a guideline, not a fabricator's limit: `build` and `fab`
warn (and `fab-notes.txt` lists it), the editor shows a warning, and `guidelines = "error"` in
`netsmith.toml`, or `--strict` for one run, makes it stop the build.

To check a generated schematic or board against Windows KiCad 9 from WSL (needs
`poppler-utils`):

```sh
scripts/check-kicad.sh out/LedExample/LedExample.kicad_sch
crates/netsmith-pcb/scripts/check-kicad.sh fab/LedExample/LedExample.kicad_pcb
```

Smaller steps, each with an example program:

```sh
cargo run -p netsmith-lang --example parse -- examples/led.nsm     # canonical source
cargo run -p netsmith-check --example check -- examples/led.nsm    # values, nets, chains
cargo run -p netsmith-solve --example solve -- examples/led.nsm    # picks and verdicts
cargo run -p netsmith-pcb --example pcb -- examples/led.nsm --out out --route
```

## Layout

| Crate | Role |
|---|---|
| `netsmith-lang` | Lexer, parser and syntax tree for `.nsm` source (`board`, `system`, `part`, `interface`, `module`). |
| `netsmith-quantity` | Dimensions, units, and worst-case interval arithmetic. |
| `netsmith-check` | Resolves imports and names, checks dimensions, and builds the model of each board and each system: values, nets, chains, the joins between boards and the connectors mated between them (`mate.rs`, ADR 0040), the pin tables of parts and the faults of a board's pins, and what is left for the solver. |
| `netsmith-solve` | Picks real parts from catalogs, backtracking over parts that depend on each other, and verifies every relation in the worst case. The catalogs are CSV files (`data/jlcpcb/catalog/*.csv` ship in the binary, a project adds its own in `netsmith.toml`, ADR 0022); `Catalogs::jlcpcb()` holds only parts with a recorded source. |
| `netsmith-lower` | Turns a solved board, or each board of a solved system, into a tool-independent `Design` and the sourcing of each part. |
| `netsmith-core` | The `Design`, `Part` (with its pins: kind, datasheet name, pads, chosen function) and `Net` that exporters read. Knows nothing about any CAD tool. |
| `netsmith-kicad` | S-expression reader and writer, the KiCad name mapping, and the `.kicad_sch` emitter. |
| `netsmith-view` | Draws a schematic as SVG with its own layout. |
| `netsmith-editor` | The editor's server (`axum`, JSON and a WebSocket), its API types, and the web app (TypeScript, Preact, CodeMirror) built from `web/` and embedded in the binary (ADR 0019). |
| `netsmith-pcb` | Footprints, placement, ratsnest, a router for the outer layers, planes for inner ones, the design rules read from the board's process, and the `.kicad_pcb` writer. |
| `netsmith-fab` | Gerbers (with the zone fill and a stroke font), Excellon drill files, a bill of materials and a placement file for JLCPCB, all written by netsmith from a `.kicad_pcb`, and netsmith's own design rules check of the same file; KiCad's command-line tool only for the optional cross-check (ADR 0034, ADR 0037). |
| `netsmith-netlist` | Reads a KiCad netlist or board and compares two netlists by connectivity (`netsmith compare`). |
| `netsmith` | The command line (`init`, `build`, `editor`, `fab`, `compare`) and the project manifest, which run all of the above, and the project the editor shows. |

`std/` holds the standard library as ordinary `.nsm` source (`std/mfg/` holds manufacturing processes, with their sources in its doc comments), `examples/` holds sample boards,
and `data/jlcpcb/` holds the verified part numbers (every row cites where its number came
from; read its `README.md`).

## Design rules

- **One correct way.** If two spellings mean the same thing, one of them goes. A
  supplied rail is a plain `Rail`, not a source part wrapping one; a pin's pads are in the
  part's `pins` table, not also a `.pad` field or a catalog column.
- **Statement order never matters.** A block means the same thing in any order.
- **Libraries state only what is always true.** A design narrows the rest, so a part
  never pre-names a net or fixes a value a design might need to change.
- **A module is configured through its interface.** Its rails, pins and declared values
  can be set from outside; the parts inside it cannot.
- **A system joins ports and mates connectors, and nothing else.** A `system` holds boards and
  says which of their ports meet (`.main.output ~ .io.input`), or which connectors are mated
  (`.carrier.slot.mating ~ .card.j1.mating`, contact N to contact N), which derives the ports
  the connectors carry. A port is an interface a board declares directly; what is inside a
  board stays private, except the `.mating` of a connector (ADR 0004, ADR 0040). A system
  names no nets (ADR 0005), and a net never crosses boards (ADR 0010): each board keeps its
  own ground, and the system reports which grounds the mates join.
- **A board is made by a process it imports.** Design rules and the stackup are the `.process`
  member of the board (`import std::mfg::jlcpcb::TwoLayer`), not tool constants. A board
  narrows them like any value (`.process.min_track &= 0.15mm..1m`); a board with no process
  has a schematic but no PCB.
- **A part number needs a source.** No LCSC number goes into a catalog from memory, and no
  derating curve either.

## What the checks show

netsmith's own design rules check (ADR 0037) passes every example, routed, with no error, and
agrees with KiCad's on the boards in the tests (see the ADR for the numbers). KiCad's, checked
with Windows `kicad-cli.exe` 9.0.0 (run from WSL):

| Example | Schematic ERC | PCB DRC (routed) |
|---|---|---|
| `led.nsm` | 0 violations | 0 violations, 0 unconnected |
| `status_indicator.nsm` | 0 violations | 0 violations, 0 unconnected |
| `linear_regulator.nsm` | 0 violations | 0 violations, 0 unconnected |
| `four_layer.nsm` | 0 violations | 0 violations, 0 unconnected (planes filled by KiCad) |
| `two_boards.nsm` | 0 violations (each board) | 0 violations, 0 unconnected (each board) |

The regulator's input and ground nets are supplied from outside the board, so nothing on it
drives them. The emitter places a `PWR_FLAG` on every net that has a power input pin and no
power output pin, which tells KiCad those nets are supplied.

Not yet checked: that "Update PCB from Schematic" keeps footprints in place after a
regenerate, and anything JLCPCB's own design check would say (netsmith holds to the process's
published limits but cannot run it).

## Known limits

- No provider asks a supplier (ADR 0047): the one built reads a local CSV of quotes, so live price and stock need a person to put them in that file until a real provider is added (the LCSC API and JLCAPI need accounts, and the suppliers' terms of use are unread). Generic resistors (an E-series value with no supplier, `series` in `[project]`, E24 by default) are picked by the solver when no catalog row works and resolved to a row, Basic first, when the bill of materials or `fab` is made; an unresolved one stops `fab` with the nearest rows. Only resistors are generic so far; the alternatives are not re-checked against the design's relations; and the solver still prefers a row to a generic value, so it can pick a row whose stock the provider says is 0 (the bill of materials shows the 0).
- `netsmith catalog refresh` and the editor's "Check parts against LCSC" (ADR 0039) do not fetch JLCPCB data. So the library is JLCPCB's hand-entered number, which nothing refreshes (the standard catalogs ship none) and the price and stock of a row with no provider are the row's hand-entered cells, marked stale after 90 days; the command only verifies each row's manufacturer's number and package against LCSC's record, writes the day in `checked`, and notes when LCSC lists stock 0. LCSC's terms of use have not been read (a person must read them before the command is advertised); the official APIs (JLCAPI, the LCSC API) need an account and come after v0.1.0.
- `netsmith dxf import` (ADR 0045) reads an enclosure's outline from an ASCII DXF, but the board model has only a rectangle with equal rounded corners, so an outline that is an L, a polygon or has unequal corners, and a cutout that is not a rounded rectangle, a slot or a circle, is refused with its size and edge count (a polygon outline is ADR 0045 step C8). Holes must be `MountingHole` packages (`M2` 2.2 mm, `M3` 3.2 mm). Binary DXF, `SPLINE`s without control points or periodic, `INSERT`s with unequal scales on arcs, drawings not flat in the XY plane, and an island inside a cutout are refused with a message; text, dimensions and hatches are skipped with a warning. `--write` writes the layout file as `Layout::to_text` does, so a comment in it is not kept (the diff shows it); the source is never written.
- Fabrication files (ADR 0034, provisional): netsmith writes the Gerbers, drill files and placement file itself, and JLCPCB has not yet taken them (every package it accepted before was KiCad's); the cross-check agrees with KiCad's own plot of the examples and of the synthetic boards, but the order preview is the check that counts. The pours are written with clear polarity, which KiCad does not write; copper islands and slivers under the zone's minimum thickness are not removed, and a thermal spoke into another net's clearance stays as a stub. Silkscreen text is netsmith's own stroke font (not KiCad's), a character it lacks is a box named in `fab-notes.txt`, and text boxes and curves are not plotted. Blind and buried vias, pad stacks, hatched zones and overlapping zones of different nets on one layer are refused with the item named. The design rules check (ADR 0037, provisional) is netsmith's own and checks what KiCad's reports on the boards netsmith makes; it does not check impedance, acid traps, copper slivers and islands of a pour, thermal starvation, stackup rules, differential pairs or lengths, custom rules (`.kicad_dru`) or blind and buried vias, and a pour is checked by its outline, not by a computed fill. Silkscreen text is checked with netsmith's own font, so a text that only just clears a pad can differ from KiCad's verdict.
- Placement requirements (ADR 0045, provisional) are built for a part's or a module's own
  `.place` (C0a and C0b): the fields `x`, `y`, `left`, `right`, `top`, `bottom`, `rotation` and
  `side`, hard in the placer, measured by the design rules check on the board file. Not built:
  a requirement between two parts (a pitch, an offset), part heights and height-limited zones,
  the editor's windows and ghosts and a drag that stops at a
  window (the editor lists the findings only), "pin here" in the editor, a requirement on a part
  inside a module except through a value the module declares, and a polygon outline. A part
  turned or moved by a layout file `at` is measured, not corrected. The check measures the
  body of a footprint as the footprint draws it: a connector whose shell is not on `F.Fab` is
  measured by its courtyard, and the finding says so. A window narrower than the placer's 0.05 mm
  grid is one value, its middle.
- Overhang (ADR 0045, section 10, **provisional**, to be reviewed by the maintainer): a part hangs over the board edge only where its own `.place.left`, `.right`,
  `.top` or `.bottom` window is at or beyond that edge, or the layout file locks it. The
  courtyard past that edge is then clipped, its pads must stay inside, a direction not stated
  is an error, and the body (`F.Fab`), the silkscreen and text outside the board are not
  checked (the silk to edge clearance stays a warning). A window on `.x` or `.y`, a module's
  bounds and an unlocked layout entry do not state an overhang. Open for the user: that rule,
  the courtyard wall and the two-face rule for hole parts (both changed since, see the next items), and a limit on
  how far a part may hang. The annealer keeps the whole courtyards of two parts that hang over
  the same edge apart, which is stricter than the check. A board that is too dense is refused
  only after the annealer has tried, so its refusal takes as long as a placement.
- The courtyard wall (ADR 0033, **provisional**, to be reviewed by the maintainer): the placer kept a fixed 2.0 mm between courtyards and the outline. It now keeps the
  process's copper to edge space plus one track with its space (`Rules::edge_wall`; 0.4 mm for
  JLCPCB's), and 2.0 mm only when no rules are given. Nothing recorded why 2.0 was chosen; the
  router, the design rules check and KiCad's pass on boards with parts that close (ADR 0033
  has the evidence), and a pad that sticks out of its courtyard is kept off the edge by its own
  cost. A layout that close to the edge can have a silkscreen warning that the soft silkscreen
  cost does not rule out; `advised_edge_clearance` stays a note.
- Hole parts on the other face (ADR 0031, **provisional**, to be reviewed by the maintainer): a through-hole part used to take its courtyard on both faces. If
  its footprint draws the courtyard on only one face's layer (`F.CrtYd` or `B.CrtYd`, as KiCad's
  right-angle headers do), a part on the other face is now kept only from its pads, each
  grown by the process's clearance, so it may stand over the rest of the connector's body. A
  footprint that draws both layers, or none, counts on both as before. The placer, the
  placement check and the annealer agree on it, and the fit test counts only the pads' area
  on that face; the rows the placer starts from still count the whole courtyard on both. The
  screw head or the nut of a connector's mating part, which no footprint draws, is the
  person's to leave room for.
- Changing the editor's page (`crates/netsmith-editor/web`) needs Node 20 or newer, and the new bundle must be committed with `scripts/web-bundle update`; using or building netsmith does not. The schematic layout is automatic: a symbol cannot be moved (ADR 0019 designs the sidecar file for it), and a big board's modules are placed in a grid in the order they appear in the design, not by how they connect. A route can be cancelled; `fab`, once started, cannot. There is no completion of names, no 3D view, and the editor does not create, rename or delete files. Price and stock in the bill of materials are the `price` and `stock` of the catalog rows (the standard ones and the project's own) as last read from JLCPCB's parts page by hand, with their date; `netsmith catalog refresh` (ADR 0039) verifies the rows against LCSC's unofficial endpoint, whose terms of use are not settled, and never writes them, and the editor's "Check parts against LCSC" does so for the project's own catalogs only: the standard rows are inside the binary and change with a release, or with `catalog refresh` in the netsmith repository. A check matches the package loosely (`SOT-23` is `SOT-23-3L`), so the manufacturer's number is the real check. Rows are looked up one at a time with a pause, so a whole standard catalog takes minutes.
- Panels (ADR 0040, P0) are one board repeated in a V-cut grid. Not built: different boards on
  one panel, routed tabs and mouse bites, rotation of a cell, a board of a system as the cell, a
  cell in another file than the panel, per-cell placement in the layout file. JLCPCB has not
  accepted a netsmith panel: it publishes no rule for the designators of a panel made by the
  customer (the notes say so, and the board's own `bom.csv` and `cpl.csv` are beside the panel for
  the order path that takes the single board), and the V-cut lines on `Edge.Cuts` (which KiCad
  reports as a malformed outline) are as JLCPCB's pages ask but unproven with their viewer.
- Orders (ADR 0035) have no spares or overage in the bill of materials, and no surface
  finish in the summary (the process definitions do not state one). An order makes each of its
  boards again, even one a plain `fab` made a moment before. An order's board named `orders` is
  refused, as its folder would be the orders' folder. The editor reads orders when it starts.
- The guide, skills and `AGENTS.md` that `netsmith init` writes into a project are copies of
  the ones in the netsmith release that ran it. A newer netsmith does not update them, and
  there is no command to refresh them yet (ADR 0015). The scaffold writes no `CLAUDE.md`, which
  Claude Code reads instead of `AGENTS.md`; add `@AGENTS.md` to one.
- Footprints for the common packages are made by netsmith itself (ADR 0043, provisional):
  chips 0201 to 2512 (R, C, L, LED, diode, fuse), SOD-123/323, SMA/SMB/SMC, SOT-23 and
  SOT-223, SOIC, TSSOP, MSOP and two SSOP, 30 named QFN, DFN, WSON and HVSON, and pin
  headers and sockets. They follow IPC-7351B at nominal density and agree with KiCad 9.0's
  footprints of the same name to 0.01 mm in the pads (344 of 424 in the courtyard too; the
  80 2.54 mm sockets differ by 0.05 mm there). A generated name wins over any file, so
  those footprints need no library. Every other footprint (BGA, QFP, crystals, other
  connectors, fiducials, mounting holes, X2SON) is still read from a library file.
- Symbols, and the footprints without a generator, come from KiCad's libraries at run time (ADR 0016, ADR 0021,
  provisional): the installed ones (`NETSMITH_KICAD_SYMBOLS`, `NETSMITH_KICAD_FOOTPRINTS`, else
  beside `kicad-cli`, else the usual install places of each system, KiCad 9 then 8; only
  Windows KiCad 9 under WSL has been checked on a real install, the Linux, macOS, native
  Windows, snap and flatpak places are unverified) and a project's own, listed in the
  `kicad.toml` at its root. `build` and `fab` need the installed ones only for a library the
  design uses (`check` and `solve` never) and stop with every place looked at and the
  variables to set when there is none; the editor draws a box where a symbol is missing. The
  mapping from a kind of part and its package to a symbol and a footprint is `std/kicad.toml`
  and the project's `kicad.toml`; a test checks every row of the first against the stock
  libraries. What is not done: the PCB side has not been tried on an exposed pad or fine pitch
  (ADR 0016, step 10); the schematic is laid out in a grid with labels, and a symbol with
  several units is placed unit below unit; a symbol's `(alternate ...)` pin functions are not
  compared with a pin's functions; a KiCad `sym-lib-table` is read only by
  `netsmith kicad-libs import`, which prints the stanzas to paste.
- An equation narrows a field that has a range to the equation's value (ADR 0025), but only
  where it is the only `==` on that field or on a field joined to it. A field with two
  equations is checked by both, each against the range the field had, and is not narrowed, so a
  number that depends on it (a resistor's power) is taken at that range.
- Standard parts (ADR 0023, provisional): diodes, a Zener, a TVS diode, an inductor, a ferrite
  bead, a crystal, a fuse, a switch, N and P MOSFETs, headers and sockets, coax, USB-C
  receptacles, programming pads and mechanical parts are in `std/` with KiCad rows, and `Uart`,
  `Spi`, `I2c`, `Swd` and `Usb2` buses. They have no catalog rows yet, so a board writes their
  values by hand. `Tvs` is drawn with the Zener symbol (KiCad has no one-way TVS symbol) and
  `Crystal` maps only two-pad footprints. Ratings of a diode across a rail do not see which
  way round it is. The checker finds a net of only inputs and an I2C line with no resistor to a
  rail (ADR 0044); it does not find a pull-up made of anything but resistors, or one on the
  other side of a connector.
- Holes and pads: the router and the placement check keep copper and vias off a hole with no
  copper (hole to copper by the process's `min_clearance`, hole to hole by `min_hole_to_hole`),
  and a part's courtyard takes in its holes. A hole's distance to the board edge is not
  checked, a slot is treated as a capsule along its axis, and a drill offset is ignored. A
  `custom` pad is routed by the box of its anchor and its primitives (the box can only be
  larger than the copper), and a primitive other than `gr_line`, `gr_rect`, `gr_circle`,
  `gr_arc`, `gr_poly` or `gr_curve` is refused.
- The schematic leaves a net that reaches one pin unlabelled (KiCad calls such a label
  dangling) and flags the pin as open, so that net's name is only in the PCB. A pin table is
  checked against its symbol and footprint when a board is exported (`build`, `fab`), not by
  `check`, because only export reads KiCad's libraries.
- Who assembles a part (ADR 0024, provisional): `.assembly` is `fab`, `hand` or `none`; the
  mechanical kinds and the programming pads are `none` and a board cannot change that, so a
  fiducial or a test point cannot be put on the BOM yet. A module offers the choice for its
  parts by declaring a `String` (`.assembly_mode`) and tying them to it (ADR 0042); the board
  cannot reach the parts, and a wrong word is refused when the board is lowered, not at the
  board's line (there is no `Assembly` type). No standard catalog row has an `assembly` cell. `netsmith.toml` can set
  one project-wide default (`assembly`, ADR 0028); there is no per-board default.
- Designators (ADR 0046): every part has `.designator`, the letters of its reference (`R` in
  `Resistor`), stated by its kind in `std/`; a part you define states its own or is `U`, and a
  board may narrow the letters of a part of its own that has none. The letters of a standard
  kind cannot be changed (one value, the same limit as `.assembly`). Designators that changed
  when the letters moved from Rust into the library (a hole was `U1` and is `H1`) are not
  changed in a fab package that was already ordered; a layout file is keyed by path and is
  unaffected.
- Pin faults (ADR 0016): two power pins that both supply one net are not checked (it needs
  the sign of the rail's current, known only after the solve); a module's port that the
  board leaves open counts as a connection for a pin that must be connected; the solver
  skips a catalog row whose package lacks a pin the board connects, but does not look ahead
  for a package that would break a `tie` or another rule; `optional` and `required` must be
  the same in every package of a pin. Logic levels and a pin's
  absolute maximum voltage are checked after the rails are solved (ADR 0046): a `Logic` line may
  state `.vih`, `.vil`, `.voh` and `.vol`, a pin `.abs_min` and `.abs_max`; a check whose
  level has no value, whose grounds differ or whose net nothing sets is a note, never a pass.
  One real part states levels, `Sn74lvc1g04` (TI SN74LVC1G04, 3 V to 3.6 V, no catalog row); the
  voltage of a net is the hull of its rails, its `Logic` drivers and the rails a resistor joins
  it to, so a net that something else drives (a plain output, an open-drain line) is not
  evaluated, and a line that is not a `Logic` has no ground to judge it against.
  A function may carry its own kind (`fn GPIO:bidirectional UART1_TX:output`, ADR 0046) and the
  net rules (two outputs, only inputs) use the kind of the function chosen; no standard part
  names functions yet, so none states one, and the pin symbol and KiCad's electrical type use
  the pin's own kind.
  (`crates/netsmith-kicad/src/mapping.rs`). A `part` with any other name checks and solves,
  but fails at export with "there is no KiCad symbol for a `X` yet".
- The JLCPCB catalogs hold only the verified parts: 219 1% resistors (the complete E24 series
  from 10 ohm to 1 Mohm in 0402, the Basic 0603 set, a few 250 mW 0805), 58 ceramic
  capacitors (Basic 0402 and 0603), 14 LEDs in five colors, one regulator, and a few diodes
  (Schottky, Zener, TVS), ferrite beads, power inductors in 1210, resettable fuses, N-channel
  MOSFETs and crystals (ADR 0025; the 4 mm to 8 mm shielded power inductors and 4-pad
  crystals are not offered because `std/kicad.toml` has no footprint for them). A design that
  needs another value gets "the catalog has nothing for it". Where several packages hold the
  same value the cheapest Basic part wins, so a design that needs 0402 says `.package &= "0402"`.
  The solver checks the power a resistor dissipates against its `max_power` (62.5 mW for the
  0402 parts, 100 mW for the 0603, 250 mW for the 0805), in the worst case over the current
  the circuit really carries (an equation narrows a stated window to its value, ADR 0025), so a
  window that nothing ties to the circuit asks for a bigger part; a catalog row with an
  empty cell is never offered (ADR 0025). The buck converter model leaves out the loop, the
  quiescent and divider currents and a second capacitor on each side. Add a part to `data/jlcpcb/catalog/<name>.csv` with its source, or, in a
  project, to a catalog file listed in `netsmith.toml` (ADR 0022, provisional).
  `Catalogs::sample()` is a full E24 set with no supplier numbers, for testing the solver
  (`data/sample/`). A catalog file is read for a part when the solver uses it, so a mistake
  in one is found then, with its file and line. The order of the rows is the selection
  policy and nothing sorts it. Stock and price are not in the rows (ADR 0047): a supply provider gives them, and the standard rows state none. There is no shared
  catalog between projects, and a catalog name used by two part kinds with different columns
  is not checked.
- Worst-case values are exact for formulas that rise or fall steadily in each variable, which
  covers dividers and the like. A formula that does not is cut into pieces and is close, not
  exact. Either way the range is never too narrow. Past a fixed amount of work the plain,
  wider range is used (`crates/netsmith-solve/src/eval.rs`).
- Parts that share a relation are searched together, depth first, and the first solution in
  catalog order is used. The search stops after 250,000 combinations. A candidate that is
  equal to one whose every completion failed, in every column a relation or connection uses,
  is skipped (a 0402 and a 0603 resistor of one value, two LEDs of one forward voltage), but
  a design with many unconstrained parts that cannot be met still takes combinations
  exponential in their number: six LED branches that cannot work together reach the limit.
  Narrow the parts (`.color`, `.package`, `.forward_voltage`).
- Series-chain physics is not modelled: write the equation yourself, as `examples/led.nsm`
  does.
- A capacitor's `.capacitance` is the marked value with its tolerance; its
  `.effective_capacitance` is what is left at its DC `.bias` and within the tolerance (ADR
  0013), from the part's published curve (the `effective_capacitance` cell of `data/jlcpcb/catalog/capacitor.csv`).
  31 of the 58 capacitors have a Samsung curve; the others (Murata, Yageo, Fenghua, and five
  Samsung parts of 2.2 uF and more, see `data/jlcpcb/README.md`) have none, so they are never
  picked when a design constrains the effective capacitance (they may be picked for the marked value).
  **Temperature is not modelled**: the curves are typical data at 25 C, so the effective
  capacitance does not include what X5R/X7R lose over their temperature range, and neither
  ageing nor the AC level in use. The bias is derived from the rail a capacitor sits between;
  a capacitor across no rail needs `.bias` stated. With the real curves the regulator
  example's 4.7 uF output capacitor is a 47 uF 0805.
- The regulator is modelled with a 5% output tolerance and a 0.1 A limit, wider than its
  datasheet's 2% at 25 C, to cover temperature and load.
- Load current is stated on the consuming rail (`.output: Rail { .current &= 0mA..50mA }`)
  and summed at the net (ADR 0007). A `~>` chain that starts on a rail's net draws the
  current of its first part from it, so a resistor and an LED count against the regulator
  with nothing stated (ADR 0014), and a part may draw through a `Rail` of its own. A balance
  sets a rail's current and a part's only when it is complete, that is when every pin on the
  net is a term or a part that draws nothing, and a part's only from two terms (ADR 0026 and
  ADR 0027, provisional): a current that nothing else determines is not evaluated, not guessed,
  and the build lists the unmodelled pins. A chain
  with a tap on its middle, one driven from a signal, and a pin that draws with no rail are
  not (or only in part) modelled; a capacitor draws nothing at DC, and so does a logic input of a part
  that has a `Rail` (its leakage is ignored, like a quiescent current). A stated current is a budget
  (ADR 0038, provisional): what the rest of a complete net determines must lie inside it, so a
  supply may state a capacity larger than its load. A regulator's own quiescent current is ignored. Its power limit
  is the datasheet's 250 mW at 25 C, not derated for a hotter board.
- Placement is derived from the circuit by an annealer (ADR 0033) and the router is a grid
  router with rip-up. It draws on the two outer layers. Parts can be on either face (ADR 0031): a layout file (ADR 0032) names the face of a
  part (`side = "bottom"`), and `netsmith fab` puts every part it does not name on the front,
  or shares the faces when a size to fit needs it; the placer API (`PlaceOptions`) takes the
  sides, a size to fit, and boxes to keep clear of. A part on the back is written flipped on the
  back layers; its position row says `bottom`, and its rotation and mirroring are not checked
  against JLCPCB's convention, so `fab-notes.txt` lists it as unverified. The 3D model of a part
  on the back is copied unchanged, and connectors are not moved to the back. Inner layers are
  whole-board planes, one net each (ADR 0012): a pad on a plane's net gets a via beside it (or
  in it, below) and no shared via; there are no signals on inner layers and no split planes. A
  pour on an outer layer (ADR 0020) keeps the via beside each pad of its net, and its stitching
  vias are on a grid in what the routing leaves free; a grid point with no free spot within a
  third of the pitch gets no via.
- The router and the placer are bounded: the placer makes 3000 moves for each part that is free
  (at most 300 000), whatever the clock says, and the router makes at most 4 tries with the lines in different orders, 3
  rip-up passes in each, a second try of what is left with the least room that is safe, and 6
  rounds of negotiation, and stops at the `--route-timeout`. A line is searched for in a window
  round its pads first and in the whole board only if that fails, so a long run is a board
  with many lines it cannot route, not one with many parts. A pad that the clearances of its
  neighbours wall in is found at once. Pads 0.5 mm apart (0.6 mm pads on a 1.1 mm pitch) are
  passed between; pads with 0.2 mm between them are reached by a via in the pad if the process
  fills such vias (`.via_in_pad`, JLCPCB: up to 0.55 mm, at extra cost, ordered on the order
  form; `fab-notes.txt` lists the pads), and are left unrouted if the board refuses them
  (`.process.via_in_pad &= 0mm`) or the pad is narrower than the via. There is no push and
  shove and no fan-out stage: a via goes on any free cell beside a pad as the search finds it.
  A mask opening that a footprint draws keeps vias away, not tracks. The router's result was
  measured on a 73-part, 207-connection board with an 82-pad module (ROADMAP, Fab for real
  boards); a denser or larger board may take minutes in a debug build.
- The router draws tracks at the process's narrowest track and its vias at the smallest the
  process allows, so the boards are as fine as the factory's published limits (JLCPCB: 0.1 mm
  track and space). For more margin, narrow the process in the board. A rail's `.min_track`
  makes the nets on it wider (net classes, ADR 0020); a signal that is not on a rail is as wide
  as the rest, and a wide track is necked down to the least between pads and out of
  fine-pitch pads. The via is the process's, whatever the net. What the router calls routed
  is measured on the copper itself against the clearance of the other nets, so a net that
  cannot pass at any width is reported not routed rather than drawn too close.
- The track width check (ADR 0036, provisional) is IPC-2221, which is a conservative guide; IPC-2152
  is more accurate and is not implemented. The 10 K temperature rise is a default of netsmith's,
  not a sourced figure. A net is checked against the largest single current on it (a ground
  shared by several rails carries their sum where they join), at the width asked for (the router
  may neck a track down between pads), and planes and pours are not checked. The router draws on
  the outer layers only, so the inner-layer constant is unused.
- With an `.outline` the placer lays the parts out inside it (`PlaceOptions::fit`) and refuses,
  naming the overflow, when they need more than it gives plus a margin from the process (the copper to edge space and one track with its space, 0.4 mm for JLCPCB's; ADR 0033, provisional). Mounting holes and
  fiducials go in the corners of a fixed outline (a board without one lays them out with the
  other parts). A layout file moves parts and puts them on a face, and the placer places the
  rest round the locked ones (ADR 0032, ADR 0033). The editor writes the layout file (ADR 0030 E1 to E3) with
  `Layout::to_text`, so a comment in a file it writes is not kept, and every edit throws away the
  routed copper (there is no routes cache yet). The editor moves, rotates, flips and locks parts
  and draws, moves and resizes keepouts (kept in the layout file, ADR 0029), and shows the outline
  and the cutouts without editing them; it has no hand-drawn tracks, no waivers and no module
  regeneration (E4 to E6). A flip changes what the derived placer is told, so the parts that are
  not placed by hand may move too.
- Derived placement (ADR 0033) weighs wire length, ratsnest crossings, module cohesion,
  decoupling distance, edges, silkscreen and the courtyard rules, but not congestion: it keeps
  room round parts with many pads and a gap between parts (more on a board with a pour), and a
  board it packs tighter than the router can untangle is not helped. A decoupling capacitor is
  found only for a part whose pin table calls the pins `power`; a part without a pin table has
  none. The reference text of a part is put on the side of it that is free, and what KiCad's
  check finds in a footprint's own silkscreen is not the placer's. Parts are turned by right
  angles only. The placer does not choose a face for a part (it shares the faces only when a
  size to fit needs it), and it is not told a datasheet's limit on the length of a net (the
  `layout` block of ADR 0030 is not built). The same circuit and seed give the same board on
  any machine, but another seed can give a board with more or less wire (10 % on the reference
  board measured), and a pad in a pour can end with one spoke (`starved_thermal`, on one seed
  in four of that board).
- A keepout that forbids tracks also keeps vias out of its area (the router's cells serve both),
  and one a part carries is written to the board file at the part's place, so it does not
  follow the part if you move it in KiCad. Keepouts apply to the outer layers' routing; a
  keepout on an inner layer only reaches KiCad's check and fill.
- A `Label` is drawn in the editor's schematic as a box that holds its text; it is not in the exported
  schematic. Its width is estimated at seven tenths of its height for each character.
- A process rule is a closed range, so "no upper limit" is written `1m`. A process with a
  layer count other than two or four is refused. `FourLayer` is JLCPCB's 1.6 mm build-up only;
  the other thicknesses' build-ups are not sourced yet. A four-layer board needs KiCad's
  Python to fill the planes only for `fab --cross-check`.
- The process's solder mask bridge goes into `fab-notes.txt` and into the board's
  `solder_mask_min_width`, which both netsmith's and KiCad's checks read (a narrower web is a
  warning). The `.kicad_pcb` stackup converts copper weight to
  thickness at 0.035 mm per ounce, which is a unit conversion and not a JLCPCB figure, and
  the board's thickness must be chosen in the board (`.process.stackup.thickness &= 1.6mm`).
- Only a board names nets (ADR 0005).
- A `system` says which ports meet and checks that their voltages agree and that their
  currents balance (ADR 0010). Several supplying ports on one join are each checked as if alone
  against the whole demand. What a board draws through a chain of parts (an LED and a resistor)
  comes from the chain (ADR 0014), as in `examples/two_boards.nsm`.
- Mated connectors (ADR 0040, built for pin headers and sockets): the checks are by contact
  number and by what each board has on the net of each contact, so the position of pin 1 on the
  board and the height of the stack are not checked (they need the layout and 3D data), a
  connector goes in one mate, a 1x8 header in a 1x10 socket is refused (a partial mate would
  need a statement of which contacts), and grounds that are isolated on purpose cannot be said.
  Flat cables (`ffc`), wire to board and edge connectors are not modelled; a `.mating` of another
  kind is refused. The contact rating (`pin_current`) comes from the `header` catalog and only
  the single-row 2.54 mm headers and sockets of the standard library have rows
  (`data/jlcpcb/README.md`); the two-row headers are unrated, and a rating the datasheet gives
  for several contacts loaded together or for temperature is not modelled, so N contacts carry
  N times the rating of one. Two ports of one board on the same contacts (an input and an output
  on one net) must be one port. `examples/carrier_and_cards.nsm` has invented parts without KiCad
  symbols, so it is checked and solved and not exported.
- `netsmith.toml` is read with the `toml` crate and holds the one `[project]` table; any other
  table or key is refused with its line and column (ADR 0011). There are no dependencies
  between projects yet. `netsmith editor` reads the
  manifest once: after changing `top_level`, restart it.
- `netsmith compare` takes pin numbers as identity: a resistor ported with its two pins
  swapped is reported as a difference, though it is electrically the same. A `.kicad_sch`
  is read with `kicad-cli`, which finds its other sheets only in the root sheet's directory.
  Parts that are interchangeable by connectivity are paired arbitrarily, and a search that
  would take more than 20 000 refinements ends as "could not decide" (exit 2). A `.kicad_pcb`
  is read as KiCad 9 writes it (`(property "Reference" ...)` in each footprint).
- The pick-and-place rotation is verified only for the packages in the table in `crates/netsmith-fab/src/rotation.rs`; others are flagged in `fab-notes.txt`.

## Licensing note

netsmith keeps no copy of KiCad's symbols or footprints. It reads the symbols, and the footprints
that it does not make itself, from the ones installed with KiCad (CC-BY-SA 4.0 with a design
exception) at run time, and the tests that need no KiCad write small libraries of their own
(`netsmith_kicad::testlib`). The footprints it makes are original code that computes IPC-7351B
land patterns from published package dimensions (ADR 0043); the test that checks them reads
KiCad's footprints and bundles none. The formulas and tables were learned from the
`kicad-footprint-generator` repository (GPL-3.0-or-later; no code was copied) and the package
dimensions of the QFN rows are numbers from its data files and the datasheets they cite.

## Next

1. Modules as hierarchical sheets (a `system` of boards is done, ADR 0010).
2. The editor (`netsmith editor`, ADR 0019): moving schematic symbols, a 3D board view and name completion.
3. Real catalog data beyond the verified set, then a real multi-board design.
