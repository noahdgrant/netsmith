# Roadmap

Where the project stands and what to do next. Update this file when a step lands; it is the
hand-off between sessions. Vocabulary is in `GLOSSARY.md`, decisions in `docs/adr/`.

## Running everything

```sh
cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all --check
cargo run -p netsmith -- init                                 # a project in the current directory
cargo run -p netsmith -- build examples/led.nsm --out out     # KiCad schematic
cargo run -p netsmith -- editor examples/led.nsm              # the editor in a browser (ADR 0019)
scripts/editor-verify doctor                                  # can the editor be driven in a headless browser here?
scripts/web-bundle check                                      # is the committed editor page what web/ builds to? (Node 20+)
scripts/release-build.sh                                      # release binary, smoke test and archive in dist-release/
cargo run -p netsmith -- fab examples/led.nsm --out fab       # files to upload to JLCPCB
cargo run -p netsmith -- fab examples/two_boards.nsm --out fab # a system: a package per board
cargo run -p netsmith -- compare out/LedExample/LedExample.kicad_sch fab/LedExample/LedExample.kicad_pcb  # same circuit?
scripts/check-kicad.sh out/LedExample/LedExample.kicad_sch    # ERC with real KiCad
crates/netsmith-pcb/scripts/check-kicad.sh fab/LedExample/LedExample.kicad_pcb   # DRC
```

Environment: WSL2 with Windows KiCad 9.0. `kicad-cli` is found from `$KICAD_CLI`, then
`PATH`, then the Windows install; the symbol and footprint folders from their variables, then
beside `kicad-cli`, then the usual install places (ADR 0021). Tests that need KiCad skip themselves when it is absent.
The check scripts need `poppler-utils`. Use a separate `CARGO_TARGET_DIR` when several
agents build at once.

## Release checklist

The aim of v0.1.0 is a real board done end to end in netsmith, installed with no Node
step. The pieces below exist (`scripts/release-build.sh`, `scripts/web-bundle`,
`.github/workflows/release.yml`); a release is made in this order.

1. On a clean checkout of the commit to release, run the gates: `cargo fmt --all --check`,
   `cargo clippy --workspace --all-targets`, `cargo test --workspace`, the `fab` gate on
   and `examples/linear_regulator.nsm` (`netsmith-dev-verify`).
2. Rebuild the editor's bundle if anything in `crates/netsmith-editor/web` changed:
   `scripts/web-bundle update`, and commit `crates/netsmith-editor/web-dist`.
3. Check that the committed bundle is what the sources build to: `scripts/web-bundle check`
   (needs Node 20 or newer; `cargo test -p netsmith-editor --test bundle` does the same when Node
   and the packages are installed, and checks the hash without Node).
4. Set the version in `[workspace.package]` of `Cargo.toml` (the release script refuses a `RELEASE_TAG` that is not
   `v<version>`), rebuild so `Cargo.lock` follows, commit.
5. Run `scripts/release-build.sh` on each system you ship for (it builds the host's target
   only): it stops on a build warning, a wrong `netsmith --version`, a failed smoke test (init,
   build, fab with `design rules check: passed`, the editor serving its page and script) and
   writes `dist-release/netsmith-<version>-<target>.tar.gz` (`.zip` on Windows) and its
   `.sha256`.
6. Human steps, which no script here does: confirm the `LICENSE` holder (a file with the author's
   name was added, not asked for); tag `v<version>` and push the tag (the workflow
   `.github/workflows/release.yml` is then meant to build the archives on Linux, macOS and
   Windows and open a draft release: it has never run); open the draft release and check the
   archives and checksums; publish the release; for crates.io (not prepared): every crate needs
   `description`, `repository` and a `version` next to each `path` dependency, the crates are
   published in dependency order, and the names must be free.

Open: the workflow is an untested draft, and the macOS and Windows builds were never made.
The KiCad libraries are found by one lookup (`crates/netsmith-kicad/src/install.rs`, ADR 0021):
the two variables, then beside `kicad-cli`, then the usual install places of Linux, snap,
flatpak, macOS, Windows and WSL (KiCad 9, then 8). Only Windows KiCad 9 under WSL has been
tried on a real install; the other layouts are tested against invented trees and must be
checked on real Linux, macOS, Windows, snap and flatpak installs before the release (and the
snap and flatpak paths are the least certain). Bundling the libraries is ADR 0041 work.

## Done

- Language, checker, worst-case solver with backtracking, lowering, KiCad schematic export.
- A real JLCPCB parts catalog where every row cites its source (`data/jlcpcb/`).
- The editor (`netsmith editor`, ADR 0019), which replaces the schematic viewer: project tree, CodeMirror source editor with live compiler and solver diagnostics, schematic with click-through to source, power tree, system view, bill of materials, PCB with on-demand routing, and a build panel with `fab`, orders and downloads. A big board (tested at 89 parts in 24 modules, and on a real 73-part board) is drawn by module, with a zoomable outline; its power tree is in columns by supply; the PCB has a net list and routing progress; problems and notes are apart; the project tree is by folder. Verified in a headless browser (`scripts/editor-verify`, skill `netsmith-dev-verify-editor`).
- Catalog rows are data, not code (ADR 0022, provisional until the user reviews it): one CSV
  file per catalog, the standard ones embedded from `data/jlcpcb/catalog/`, a project's own
  listed in `netsmith.toml` (`catalogs = [...]`). Adding a part or an IC's row is a data edit.
  Errors name the file and line.
- `netsmith catalog refresh` and `catalog age` (ADR 0039, provisional): an opt-in **check** of
  catalog rows against LCSC's product record (the manufacturer's number and package; a diff,
  `--dry-run`, `--offline`, a cache, CONFLICT and MISSING rows left alone), recording the day in a
  `checked` column and writing no price, stock, library or date; a row with stock 0 not offered;
  and the editor's BOM with dated, stale-marked prices and a "Check parts against LCSC" button.
  Since 2026-11-02 (user's decisions for v0.1.0) **JLCPCB data is not fetched** and **LCSC's price and stock are never written**: the
  board is assembled from JLCPCB's library and LCSC's numbers differ from JLCPCB's, so the
  library, price and stock are JLCPCB's hand-entered snapshot, aged by `date`; a refresh only
  notes when LCSC lists stock 0. Nothing refreshes them until the official APIs are used.
  Open: **read LCSC's terms of use** before advertising the command or the button (ADR 0039,
  item 8; the terms are unread); **official APIs after
  v0.1.0** (JLCAPI at api.jlcpcb.com for library, price and stock; the LCSC API, free with an
  account and review), which need the user's accounts and keys; a refresh of the standard
  catalogs is a maintainer's job and has not been run over them; the editor cannot check the
  standard rows inside the binary; no live price without writing a file; no second supplier; no
  price by quantity in the BOM.
- Supplier data and generic passives (ADR 0047, provisional, issues #8 and #9; designed, and a first slice built: the provider trait, the cache, a `file` provider, `unknown` in the bill of materials and `catalog age`, generic E-series resistors resolved at BOM and `fab` time): price,
  stock and their date come from a **provider** (a trait: price breaks and stock for an LCSC or
  manufacturer's number, each answer saying whether it is JLCPCB assembly stock or LCSC retail
  stock) when the bill of materials or `fab` is made, cached locally with a timestamp, and are
  **unknown** (never guessed, never 0) offline; identity stays in the catalog rows. The solver
  picks generic passives (E-series value, tolerance, package, power rating) and a separate step
  resolves each to a row, JLCPCB Basic first, or reports it unresolved with the nearest
  alternatives. ADR 0047 supersedes the data-in-files parts of ADR 0022 and ADR 0039 (the
  `price`, `stock` and `date` columns, the stock-0 rule). Open: the nine questions at the end of
  ADR 0047; no provider talks to a supplier (their terms of use are unread). Not built: the LCSC
  and JLCAPI providers, generic capacitors, inductors and LEDs, re-running the checks for each
  nearest alternative, an `--offline` flag, a named preferred-parts file.
- Outline from the enclosure (ADR 0045, C0 piece 3): `netsmith dxf import <file.dxf>` reads an ASCII DXF
  (`netsmith-dxf`: lines, arcs, circles, polylines with bulges, splines and ellipses as chords, block
  inserts, units, gaps reported) and prints an `Outline`, `Cutout`s and `MountingHole`s to paste, with
  their places merged into the layout file by `--write` (a diff, the source never written). Open: a
  **polygon outline** (ADR 0045 step C8: the board model, placer fit test, router, DRC edge distances,
  Gerber and Edge.Cuts writers and the editor all take a rectangle with equal rounded corners), so an L
  or any polygon outline is refused by name; holes only of the `MountingHole` packages; `PlatedHole`s,
  fiducials and other datums on the DXF are not read.
- PCB: footprints, placement, ratsnest, router with rip-up, `.kicad_pcb`/`.kicad_pro`, SVG.
- Fabrication package (`netsmith fab`): Gerbers, drill, BOM, CPL, notes, DRC gate. Output
  matches the layout of a package JLCPCB has accepted. The Gerbers, drill files and placement
  are netsmith's own (ADR 0034, provisional), with `--cross-check` against KiCad's plot.
- Manufacturer design rules and stackup as an importable library (ADR 0008):
  `import std::mfg::jlcpcb::TwoLayer` and `.process: TwoLayer` in the board. The numbers are
  JLCPCB's published ones with their source in `std/mfg/jlcpcb.nsm`; the router, placer,
  `.kicad_pcb`/`.kicad_pro` writers, DRC gate and `fab-notes.txt` read the resolved process
  from the `Design`. Open: other houses, and half-open ranges so a limit need not end in `1m`.
- Board features (ADR 0020, accepted; ADR 0029, ADR 0032, provisional): net classes (`.min_track`
  on a `Rail`), `.outline` with rounded corners and `Cutout`s (the parts are fitted in it, or
  refused naming the overflow), pours on the outer layers with stitching vias, `Keepout`s on a
  board or carried by a part (rule areas in the board file, obeyed by the router and the
  placement check), `Label` silkscreen text, mounting holes and fiducials in the corners, and
  the layout file `<stem>.pcb.toml` with `apply_layout`. `examples/outline_and_pours.nsm`.
- Placement requirements from the enclosure (ADR 0045, C0a and C0b; the user's decisions of
  2026-11-02): every part and module has a built-in `.place` (`x`, `y`, `left`, `right`, `top`,
  `bottom`, `rotation`, `side`) that the board narrows with the ordinary operators, measured from the
  part's body (`F.Fab`, else the courtyard) and the outline's top left corner. The solver keeps the
  bounds as windows, `build` refuses empty ones, a rotation that is not a multiple of 90, a bad side
  and a requirement measured from an outline of no fixed size, and lowering carries them in the
  design (`Features::constraints`). The placer treats them as hard (a single value fixes the part
  like `lock`; a window is a domain for the annealer, the module's `left`/`top`/`right`/`bottom`
  hold for each of its parts; a layout file's lock that breaks one is refused before placing). The
  design rules check has a `constraint` rule (`netsmith-fab::drc::constraints`) that measures them on
  the board file with every number, and `fab` and the editor's findings use it. An unmet one is an
  error that stops `fab`; `constraints = "warn"` or `--constraints warn` makes it a warning, `--constraints error` forces the error over a manifest's `warn`, and `--allow-drc-violations` does not waive it (the user's decision).
  **Not built**: `Board::check_constraints` as a method of `netsmith-pcb` (the check is the rule on
  the board file, so there is one measurement; `netsmith-pcb` cannot depend on `netsmith-fab`),
  module `.place` box checks for `.x`/`.y`,
  relations between two parts (C2), the catalog `height` column and `MountingHole.head_diameter` (C3), the editor's window overlays
  and drag clamp (C4), "pin here" (C5), `layout check --json` (C6), mating across boards (C7), a
  polygon outline (C8). 
- Height limits (ADR 0045, C3, section 11; **provisional, to be reviewed by the maintainer**): a part
  states `.height`, a `Keepout` states `.max_height` (on the faces its `.layers` name, so a
  back-face part has its own limit); `check_placement` reports a part too tall where it stands, by
  name and with the numbers, a part with no stated height under a limit is a named warning
  (`Board::heights_unchecked`, in `fab` and `fab-notes.txt` and the editor), an error when `constraints = "error"` is written or `--constraints error` is given, the annealer keeps a
  tall part out of a zone it is too tall for, and the PCB view draws the limit in amber. **Not built**:
  a height from a catalog column (catalogs are another change's), from the footprint's 3D model
  (no STEP reader, by decision), or from the DXF import; a keepout a part carries is checked but not
  avoided; the editor's drag clamp.
- Overhang (ADR 0045, K2, section 10; **provisional, to be reviewed by the maintainer**): a part whose own `.place.left/right/top/bottom` window is at or beyond the same
  outline edge, or that the layout file locks, may hang over that edge. The placer's fit test
  counts a part that stays as its courtyard clipped to the outline; the check does not report the
  courtyard past a stated edge, measures overlap on the clipped courtyard, and still reports a pad
  outside or too near the edge and an overhang in a direction nobody stated, with the edge and the
  numbers; `fab` prints each overhang and writes it to `fab-notes.txt`. The rows pre-check no
  longer vetoes: the annealer runs in the given size first, and "does not fit" is said only if it
  ends with overlaps or parts outside. Tests: `crates/netsmith/tests/overhang.rs` (an invented
  35 x 60 mm board with a 42 mm plug and 30 small parts), `overhang.rs` and the placer's unit tests,
  and the guide (`nsm solve place`). **Open**: the rule itself (edge fields only, a lock allows
  every edge), a limit on how far a part may hang, whether `fab` should warn, the annealer's
  stricter whole-courtyard separation of two parts hanging over the same edge, and silkscreen over
  the edge (a warning, as KiCad). The courtyard wall is now derived from the process (0.4 mm for JLCPCB's, not a fixed 2.0 mm; ADR 0033, provisional) and the hole-part two-face rule changed (ADR 0031, provisional).
- Fixes on a verification board (`placer2` branch; the last three provisional, to
  review): a pad keeps the process's edge distance from a cutout in the placer as in the check;
  the router's masks looked only as far as a via reaches, so rails wider than a via (0.6 and
  0.9 mm) were drawn too close to other copper (16 and 68 check errors on invented 49 and
  70-part boards, now none) and every line is measured as drawn, a line too near at any width
  being left unrouted; the "do not fit" message names the ten worst pairs and parts and lists
  the rest; a body whose centre is off the board is allowed where an edge field states the
  overhang (ADR 0045 section 10); a through-hole part that draws one courtyard layer is judged
  by its pads on the other face (ADR 0031); the courtyard wall comes from the process (ADR 0033).
  Open: a `--json` listing of the offenders, the rows' start state still counting a hole part
  on both faces, and a silkscreen-aware wall.
- Track width against current (ADR 0036, provisional): IPC-2221 on each net's worst-case current
  (rails and series parts), the process's copper weight and `.process.max_temperature_rise`
  (10 K default); a guideline, so it warns, and `guidelines = "error"` or `--strict` blocks.
  Open: IPC-2152, sums at shared grounds, necked-down runs and vias, tracks on inner layers.
- The router sizes every net (issue #4, ADR 0036 amendment, provisional): a net no rail gives a
  `.min_track` is drawn at the width IPC-2221 needs for the current the solver worked out
  (the fuse-to-diode node of a `~>` chain included); a net with an unknown current is named in
  a note and keeps the process's least; a stated `.min_track` stays an override. Open: the IPC
  constants were not checked against the standard itself, and a net with two unknown currents
  (issue #18) is not sized.
- Four-layer boards (ADR 0012): `std::mfg::jlcpcb::FourLayer` with JLCPCB's multilayer rows and
  its 1.6 mm `JLC04161H-7628` build-up; the board names the net of each inner layer
  (`.process.inner1.net &= "GND"`); the `.kicad_pcb` gets the stackup and a zone per plane;
  the router draws on the outer layers and drops a via beside each surface-mount pad on a
  plane net; the Gerber package gets `In1_Cu` and `In2_Cu`. `examples/four_layer.nsm` passes the
  DRC gate with 0 violations and 0 unconnected, planes filled by KiCad's Python.
- Projects (ADR 0011): `netsmith init` scaffolds `netsmith.toml`, `src/main.nsm` (the LED
  example), `.gitignore` and `README.md`, and (ADR 0015) `AGENTS.md`, the `netsmith-*` skills in
  `.agents/skills/` (`.claude/skills` links to it) and the guide as `docs/netsmith/LANGUAGE.md`; `build`, `fab` and `editor` with no file act on the
  manifest's `top_level` files from anywhere under the root; `src/power.nsm` is imported as
  `src::power`. A top-level file may hold a `system`; its boards come out in
  `<out>/<System>/<member>/` like any other. Open: dependencies between projects (they would be a `[dependencies]` table, ADR 0011).
- Four examples (LED, status indicator, regulator, four-layer regulator) pass ERC and DRC with 0 violations.
- Load current (ADR 0007): `Rail.current`, a balance per net, and the regulator's current and
  power checks. `examples/linear_regulator.nsm` states a 0 to 50 mA load.
- Parts add to a net's current (ADR 0014): the first part of a `~>` chain is drawn from the net
  the chain starts on and the last gives it back where it ends (a rail's `neg` is a draw of
  the opposite sign, so ground balances too), neighbouring parts of a series chain are
  equated, and a failed current requirement lists the terms on the nets. `examples/two_boards.nsm`
  states no draw of its own, and an LED on a regulated rail counts against the regulator.
- KCL solves rails, verifies parts (ADR 0026, provisional): a balance sets a part's through-current only from a net of two terms; ratings on a current nothing determines are `not evaluated`. **Review ADR 0026**: it is provisional, to be reviewed by the maintainer.
- A balance is complete or it is not used for parts (ADR 0027, provisional): a balance whose net has a pin that is not a term and not a part that draws nothing sets and verifies nothing, and the build lists the unmodelled pins. **Review ADR 0027**: provisional, to be reviewed by the maintainer, in particular what counts as drawing nothing (a part with no `current`, no rail and only `passive` pins; and, since 2026-11-02, an `input` or `no_connect` pin of a part with a rail).
- A stated current is a budget (ADR 0038, provisional): a range stated on a current is an allowance that what the rest of a complete net determines must lie inside (the current then holds the narrower value); with two or more stated currents the net only has to be able to reach zero. A current worked out from a balance is worked out again as the intersection with every new value. **Review ADR 0038**: provisional, to be reviewed by the maintainer, in particular that a stated supply larger than its load is now accepted.
- Effective capacitance (ADR 0013, which supersedes ADR 0009's meaning of `.capacitance`):
  `.capacitance` is the marked value with its tolerance and `.effective_capacitance` is what a
  ceramic capacitor keeps at its `.bias`, tolerance included, from sourced Samsung curves
  (the `effective_capacitance` cell of `data/jlcpcb/catalog/capacitor.csv`); a part with no curve is refused only for a design that
  constrains the effective value. The checker derives `.bias` from the rail a capacitor sits
  between. The regulator example asks for 1 uF at the input and 4.7 uF at the output
  effective, and gets a 10 uF 10 V 0402 and a 47 uF 6.3 V 0805 (the 4.7 uF 10 V 0402 keeps
  about 1.2 uF at 3.465 V). The 0805 rotation in the placement file is not verified
  (`fab-notes.txt` says so).
- The plain `=` is gone (ADR 0018): `&=` narrows, `==` states an equation, `>=` and `<=` bound; `=` is an error
  that says so.
- `system` of several boards (ADR 0010): `system Demo { .main: MainBoard  .io: IoBoard  .main.output ~ .io.input }`.
  A port is an interface a board declares directly; the rest of a board stays private. `~` equates
  the ports' values and balances their currents at a join (a board that draws more than the other
  is built to give is reported with both boards, the port and the numbers). The checker builds
  one model for the system and the solver solves it once; lowering reads one board at a time.
  `build` and `fab` write `<out>/<System>/<board>/`, the editor lists the boards. See
  `examples/two_boards.nsm`.
- Netlist comparison (ADR 0017, `netsmith compare`, crate `netsmith-netlist`): two designs
  (`.kicad_sch`, `.kicad_pcb`, KiCad netlist XML, `.nsm`) are compared by connectivity, with
  components matched by value and footprint and by partition refinement with individualization;
  differences are reported by net and pin. The generated schematic of the LED and regulator
  examples matches their own boards. Open: pins that are electrically interchangeable (a
  resistor's two) count as different; a `.kicad_sch` is read through `kicad-cli`, so
  sub-sheets must sit beside the root sheet.
- Interval dependency fixed: a variable that appears twice in a formula (a divider) gets its
  true range, not a wider one (`eval.rs`, `expand.rs`).

- IC parts, phase 1 (ADR 0016): a part declares its pins in a `pins "PACKAGE" { ... }` table
  (kind, datasheet name, pads, `fn` functions, `optional` or `required`; an unnamed table for
  a part whose pads do not depend on the package) and `tie .a ~ .b`; the pad is no longer a
  field or a catalog column. The checker reports the pin faults on the board (a required pin
  open, a tie not made, a no-connect pin connected, two outputs on a net, a function not
  chosen, not offered or used twice, a pin the package lacks) and, after the solve, the pad
  rules (a pad given to two pins, a pin with no pad and no alias). Catalogs may be keyed
  (`catalog mcu by .mpn`), and the solver skips a row whose package lacks a pin the board
  connects. Lowering writes one pin reference for each pad and carries the pins in
  `Design::parts`. The five library parts were migrated and the examples build
  byte-identical.

- Standard library kinds (ADR 0023, provisional): diodes, Zener, TVS, inductor, ferrite bead,
  crystal, fuse, switch, N and P MOSFETs, headers, sockets, coax, USB-C, programming pads,
  mechanical parts and the `Uart`, `Spi`, `I2c`, `Swd` and `Usb2` buses, with ratings checked
  in the worst case and bus roles checked through the pin kinds. Open: catalog rows and the
  KiCad mapping for each kind; a bidirectional TVS diode. A net of only inputs and an I2C line
  with no pull-up are errors (ADR 0044); each kind states its designator letters in `std/`
  (`.designator`, ADR 0046).

- Pin functions with their own kind, logic levels and absolute maximums, and designators from the part
  (ADR 0046, accepted): `fn GPIO:bidirectional UART1_TX:output`, `Logic.vih/vil/voh/vol` and `Pin.abs_min/abs_max`
  checked after the rails are solved, and `.designator` on every part. Open: more datasheet-sourced parts
  (no standard part names functions yet; `Sn74lvc1g04` has no catalog row), levels for a net that is driven by
  an open-drain or plain output, a level check that follows a series resistor or a level shifter, and a
  default designator that a board may replace on a standard kind.
- Who assembles a part (ADR 0024, provisional): every part has `.assembly` (`fab`, `hand`,
  `none`), a catalog file may give a row's default, and `fab` honours it in the BOM, the
  placement file and the notes. The mechanical kinds and the programming pads are `none`.
  `netsmith.toml` may set `assembly` as the project-wide default (ADR 0028, accepted).
  A module may offer the choice for its parts as a `String` value tied to them (ADR 0042,
  accepted). Open: a default that a board may widen (a fiducial on the BOM), an `Assembly`
  type so a wrong word is refused at the board's line. **Review ADR 0024**: it is provisional, to be reviewed by the maintainer.

## Open decisions and designs

- **Notes for the owner's decisions** (`docs/research/`, written 2026-11-03, no decision in them): where schematic and PCB intent live (issue #2), options for hierarchical modules and connectors (issue #16), and a review of each provisional ADR with a recommendation (issue #10).
- **Resistor power** is checked (ADR 0025): `Resistor` has `.max_power` from the catalog and states `.drop * .current <= .max_power`, at the current the equations give (an equation narrows a field that has a range, when it is the only one on that field; a window nothing ties to the circuit is taken at its upper end). Derating for a hot board is not modelled.
- **Switching regulators and catalogs of the standard kinds** (ADR 0025, provisional): `FeedbackDivider`, `Buck` and `BuckStage` are built and tested with an invented IC, and eight standard kinds have a small sourced catalog. Open: a second capacitor on each side, dropout and minimum on-time, loop stability, the divider's and the IC's quiescent current in the load, a pin-free supplier rail inside a module (a module must now borrow the IC's switch pin as the supplier, because a part's rail needs a pad), footprints in `std/kicad.toml` for the 4 mm to 8 mm shielded power inductors and the 4-pad crystals, `Diode` and `PMosfet` catalogs, and rows for more values.
- **Search effort**: the part search backjumps (conflict-directed): a failure goes back to the nearest earlier part it depends on, so parts that are not the cause are not chosen again. A failure whose cause is a chain of parts that really constrain each other is still exponential in the chain, and the search still gives up after 250,000 trials.

### Agent files in a project: what is left

`netsmith init` copies the guide, the user skills and an `AGENTS.md` into a project (ADR 0015).
Open: a way to refresh the copies after an upgrade without overwriting edits, and a
`CLAUDE.md` for Claude Code users.

### Systems: what is left

- **Connector mating is built for pin headers and sockets** (ADR 0040 A and B: `~` between two
  `.mating`, contact N to contact N, the roles read from the boards, the derived port joins, the
  ground groups and the rating of a contact). Not built: flat cables and wire to board (need a
  cable or harness definition), edge connectors, a partial mate (a 1x8 header in a 1x10 socket),
  isolated grounds, pin 1 and stack height (need the layout and 3D data), and ratings for the
  two-row headers (no sourced rows).
- **Hierarchical modules** (a module as a sheet of its board, drawn and listed as its own sheet)
  are still open.
- Several supplying ports on one join are each checked against the whole demand, which is
  pessimistic for boards that share a load.

### Load current: what is left

Load current is a requirement a design can state (ADR 0007): `Rail.current`, summed at each
net by the checker and propagated and verified by the solver, with the regulator checking
its current and its heat. Open:

- Parts in a chain from a rail's net now draw from it (ADR 0014), and a part may draw through
  a `Rail` of its own. Not yet: a pin that draws with no rail to hold its voltage; a chain
  with a tap on its middle (only its start is drawn, and its return is not checked); a chain
  driven from a signal (left out); a capacitor in a chain with parts that pass a current,
  whose DC current is not set to zero.
- A supply's capacity cannot be stated on the supplying rail; it lives in the supplying part
  (`max_current`). A board input that can give only 500 mA needs a way to say so that is not
  `&=` on the exact current.
- The regulator ignores its quiescent current, and its power limit (250 mW) is the
  datasheet's figure at 25 C. Derate it for a hotter board once boards state an ambient
  temperature.
- A net with two or more unknown currents stays unsolved without a message.

### Projects: what is left

- Dependencies between projects: a `[dependencies]` table in `netsmith.toml` and one more
  prefix in the loader (ADR 0011). Not added until something needs it.
- `netsmith editor` reads the manifest once; a changed `top_level` needs a restart.

## Generated footprints (ADR 0043, provisional)

Done: generators for chips, SOD/SMx diodes, SOT-23, SOT-223 and SOT-89, DPAK, SOIC/TSSOP/MSOP/SSOP,
QFN/DFN/WSON/HVSON (30 named packages), LQFP/TQFP/VQFP (16), SMD crystals (18) and pin headers/sockets, consulted before any library file, checked against
KiCad 9.0's footprints by `crates/netsmith-pcb/tests/generated_vs_kicad.rs`. Open: BGA, the QFPs of
a maker's own drawing and those with an exposed pad, TO-263, X2SON, SSOP 14/24/28 and more QFN/DFN rows (each must agree with KiCad's footprint or
state why not); a check of the generators that does not need KiCad installed (a snapshot of the
numbers); pad shapes and silkscreen of the generated footprints have been judged only by netsmith's
own design rules check; `libs fetch` and `libs vendor` of ADR 0041 for the rest.

## Backlog, in priority order

1. **IC parts, phase 3: the PCB side of an IC (ADR 0016, step 10).** Symbols and footprints
   are read from KiCad's libraries (ADR 0021) and a pinout is validated against both. To do:
   the PCB with an exposed pad, fine pitch and several pads on a net (the placer, the
   ratsnest and the router have not been tried on one); the value
   still comes from a `match` in `netsmith-lower` (the reference letters are the part's
   `.designator` now, ADR 0046); the symbols' `(alternate ...)` pin names
   against a pin's functions; two power sources on a net, and more parts that state logic levels (the first is `Sn74lvc1g04`, ADR 0046; a
   net that a plain output or an open-drain line drives, and a pin that is not a `Logic`, are not judged). **Review ADR 0021**: it is provisional, to be reviewed by the maintainer.
2. **The open items of load current and parts that draw**, above, and other processes (other houses, other
   thicknesses of `FourLayer`, other layer counts).
3. **Rotation verification.** CPL rotation offsets are verified only for the packages in the table in
   `crates/netsmith-fab/src/rotation.rs`. `LED_0603` and `SOT-23` are flagged in `fab-notes.txt`. Check them in
   JLCPCB's order preview and record the result in `crates/netsmith-fab/src/rotation.rs`.
4. **Routing and placement quality.** The boards are correct and DRC clean but not tidy.
   Placement is simple and the router is a grid router. Improve, with DRC as the gate.
   Fine-pitch pads, vias in pads, necked-down tracks, negotiated rip-up and parts on the back
   are done (ADR 0031, "Fab for real boards" below), and so is a derived placer (annealing,
   rotation, module cohesion, decoupling pairs, connectors at the edges; ADR 0033, "Derived
   placement" below); what is left is listed there.
   Holes with no copper are kept clear by the process's clearance and hole to hole space, but a
   hole's distance to the board edge, a drill offset and a slot's exact outline are not (README,
   Known limits).
5. **JLCPCB DFM.** netsmith stays inside the published limits but has not run JLCPCB's own
   check. Upload the generated packages once and note what it says.
6. **Model temperature and ageing of ceramic capacitance** (effective capacitance is typical data at 25 C; X5R/X7R lose more over their range, and class II parts age), which needs the dielectric's temperature limit sourced for each part, and find the Murata 22 uF's curve.
7. **Modules as hierarchical sheets** (the `system` itself is done, ADR 0010, and so is mating
   pin headers and sockets, ADR 0040). A real multi-board design with a carrier board and
   plug-in boards also needs panels (below) and the connector kinds that are not built.
8. **Update PCB from Schematic round trip.** Unverified: that regenerating keeps footprints
   in KiCad.
9. **The editor's next steps** (ADR 0019): moving a schematic symbol and keeping it in a sidecar layout file (designed in the ADR, needs the layout to place wires after offsets, inside `netsmith-view`), a 3D or isometric board view, name completion, creating and renaming files, and a desktop shell (Tauri) around the same page.
10. **Real catalog data** beyond the verified set (now only rows in `data/jlcpcb/catalog/*.csv`
    with a `source`, ADR 0022), then a real multi-board design.
    **Review ADR 0022** (catalogs as CSV files); it is provisional, to be reviewed by the maintainer.
    **Review ADR 0047** (supplier data from a provider, generic passives); it is provisional, to be reviewed by the maintainer.

### Four layers: what is left

- Fully routable inner layers: signals on them, vias that go between inner layers, split
  planes (two nets on one layer) and planes that are not the whole board.
- A pad inside a pour of its net could drop the via beside it (to be taken only with KiCad's
  own unconnected check as the gate); a via of another size for a wide net; thermal relief
  settings other than KiCad's defaults (`.thermal_gap`, `.spoke_width` on a `Pour`).
- Board features still to do: clearance per net class, a keepout of a part that turns in KiCad
  with the part, an outline that is not a rectangle,
  logos and images, the fab's fiducial and test-point rules (to read and source), a fit that
  reflows the arrangement beyond what `PlaceOptions::fit` does (the derived placer of ADR 0030),
  (the layout file is written by the editor now, without keeping comments).
- A fan-out via is placed per pad, in the nearest free cell; pads on one plane net could share
  one, and a via in the pad would save the track.
- Other build-ups and thicknesses of `FourLayer` (the page's tables for 0.8, 1.0, 1.2 and
  2.0 mm and its 3313, 1080 and 2116 prepreg variants), 2 oz outer copper, inner copper other
  than 0.5 oz, and a controlled-impedance process (the track width for a target impedance).
- The planes are filled by KiCad's Python on a staged copy for the optional KiCad cross-check only (the Gerbers have netsmith's own fill, ADR 0034, and netsmith's own design rules check needs none); if a later `kicad-cli` fills
  zones (`--refill-zones`), use that and drop the Python.
- JLCPCB's own check of a four-layer package has not been run; upload one and note what it
  says. JLCPCB's help pages ask for Protel file extensions and `.G1`-style inner names; the
  package uses KiCad's names without them, as a package JLCPCB accepted did.

### Fab for real boards: what was done and what is left

Measured on the synthetic boards of `crates/netsmith-pcb/tests/synthetic.rs` and with KiCad 9's
own check (planes filled by KiCad's Python):

| | Before (the row layout) | Now |
|---|---|---|
| Synthetic big board (70 parts, 80-pad IC, 106 nets), routed, debug / release | 15.6 s / 1.7 s, 0 left | 13.7 s / 1.8 s, 0 left |
| Synthetic big board, KiCad check | not run | 0 violations, 0 unconnected (161 vias, 711 tracks) |
| Examples' `fab` output | | byte-identical except one line in each `.kicad_pro` (the Default net class gets a priority; see ADR 0031) |

What changed (ADR 0031): a second try of a failed line with the least room that is safe (this is
what opened the 0.5 mm between pads); vias in pads where the process
fills and caps them (`.via_in_pad`, JLCPCB 0.55 mm, noted in `fab-notes.txt`; also for a pad on
a plane); necked-down tracks; negotiated congestion as the last stage; parts on the back (mirrored
footprints on `B.Cu`, `PlaceOptions { sides, fit, keepouts }`, sharing by area when a size is
given and too small, an error with the numbers when it cannot); the reference text no smaller
than the process prints; vias kept away from mask openings a footprint draws; and the project
file now makes KiCad check the process's clearance and not its own 0.2 mm.

Left, and not fixed:

- **Only the layout file says a part is on the back.** `side = "bottom"` in `<stem>.pcb.toml`
  (ADR 0032) puts a part on the back; `fab` and the editor read it and feed `PlaceOptions`; a
  part it does not name is on the front. The placer, the
  writers, the router and the package handle the back and are tested (a routed board with parts
  on both faces passes KiCad's check), but the CPL rotation and mirroring of a bottom part are
  unmeasured (flagged in `fab-notes.txt`), and the 3D model of a part on the back is copied
  unchanged.
- **Silkscreen of footprints.** A library footprint with silkscreen text or lines closer than
  the process allows fails the check; the router and placer do not move or remove library
  drawings.
- **Tracks and mask openings.** A footprint's mask opening keeps vias out but not tracks.
- **No push and shove, no fan-out stage.** Negotiated rip-up resolved every case tried; a board
  that is denser still routes more slowly in a debug build.
  The derived placer (ADR 0033) counts ratsnest crossings and keeps room round parts with many
  pads, but a board it packs tighter than the router can untangle is not helped.
- **A via in a pad needs the fabricator's filled and capped vias**: JLCPCB's page gives the
  range and not the price or what the order form calls it, and the two-layer process's page
  row was not read separately; check the order preview.
- **Net classes (wider power tracks) and keepouts** are the `boardfeat` branch's; the neck-down
  and the masks read `rules.track_width` and will need that branch's per-net widths.

## Derived placement

ADR 0033 (provisional). `anneal.rs` replaced the row layout as the placer: simulated annealing
over position, right-angle turn and reference-text side of each part, from the circuit alone,
with a seed (`seed` in the layout file) and a fixed number of moves. It keeps the parts of a
module instance together, a capacitor between two nets that a part has `power` pins on next to
those pins, connectors at the edges, wire short over pad centres, the courtyard and outline
rules, reference text and silkscreen clear of other parts' pads and silkscreen, and ratsnest
lines from crossing. Parts a layout file places stay, `place_subset` places some parts of a
placed board and leaves the rest (the call regenerating one module needs).

Measured before (the row layout) and after, release build, seed 0. Ratsnest crossings count
pairs of lines of different nets that cross; decoupling is the mean pad to power-pin distance
of the capacitors the placer pairs.

| | Synthetic big board, before / after | Synthetic with modules and power pins, before / after |
|---|---|---|
| Parts, nets | 70, 106 | 78, 106 |
| Board | 42 x 53 / 42 x 42 mm | 43 x 56 / 42 x 48 mm |
| Wire length (half perimeters of nets) | 1805 / 1441 mm | 1770 / 1381 mm |
| Ratsnest length | 1367 / 1155 mm | 1298 / 1119 mm |
| Ratsnest crossings | 456 / 178 | 364 / 182 |
| Decoupling caps, mean / max distance | none paired | 31.8 / 42.9 mm to 5.0 / 10.3 mm |
| Connections left after routing | 0 / 0 | 0 / 0 |
| Route time, release | 3.4 s / 1.1 s | 2.3 s / 2.0 s |
| Placement time, release | 0.4 s / 1.7 s | 0.4 s / 3.1 s |
| KiCad's check, errors | 0 / 0 | not run / 0 |

The examples' boards are smaller and pass KiCad's check through `fab` as before: `led` 13 x 11
mm (was 19 x 11), `linear_regulator` 18 x 11 (29 x 11), `four_layer` 22 x 11 (29 x 11),
`status_indicator` 20 x 11 (27 x 11), `two_boards` 18 x 11 and 11 x 11, `outline_and_pours` 50 x
36 as its outline says; their files differ from before in every position.

How much a seed matters: the big board over six seeds averages 1291 mm and 150 crossings and
routes in 0.3 to 1.0 s; the one with modules over four, 1442 mm, 181 crossings and 4 mm of
decoupling. A debug build (the
`netsmith-pcb` crate is built with `opt-level = 2` in `[profile.dev.package.netsmith-pcb]`)
places the big board in 2.9 s and routes it in 2.1 s, and the one with modules in 4.9 s and
5.5 s (the row layout, built without that setting: 3.7 s and 19.3 s, 3.5 s and 19.2 s).

What the board-features report asked about silkscreen: a library footprint's silkscreen is
clipped by the mask when another part's pad is near. The placer now keeps the strokes of a
footprint's silkscreen and the box of its reference text off other parts' pads and mask
openings by the process's silk to pad space and off other text and silkscreen by 0.2 mm, the
cost rising through the run to as much as a hard rule; the text goes to whichever side of the
part is free (`text` in the layout file). Findings in a library module footprint's own drawings
stay (the library's, not the placement's). Not done: arcs, circles and filled shapes of a silkscreen are boxes, and a
footprint's other text (the value, `fp_text` of the module) is not read.

Two things the new placements showed in code that was not changed for them, and that were
fixed: a stitching via could sit in the gap between two pads of a connector, where the pour
cannot reach it (KiCad: `via_dangling`), so stitching vias now keep the pour's way past any pad
(`stitch.rs`); and reference text of a turned part was written at a place that is above it only
when the part is not turned (`Placed::reference_at`).

## Layout

Proposed, not decided: `docs/adr/0030-layout-knowledge-and-the-pcb-editor.md` (builds on ADR 0020 item 7). Order, each step passing the existing gate:

1. **Sidecar and the editor's PCB view as a layout tool.** The file and `apply_layout` are built (ADR 0032: `<stem>.pcb.toml` with positions, rotation, side, lock; the positions of keepouts and cutouts; read by `fab` and the editor's PCB view). The editor's PCB view is a canvas that selects, moves, rotates, flips and locks parts, with grid snap, box select, undo and redo, and writes the file (E0, E1 built, `pcbedit` branch); it draws, moves, resizes and deletes keepouts, which it keeps in the layout file (E2 built, ADR 0029). To do: waivers, the seed, `Layout::capture` written by a command.
2. **A better placer, no new language.** Built (ADR 0033; P1 and P2, and P0 for the seed and the fixed parts): annealing on pad-centre wire length with right-angle turns, module cohesion by instance path, fixed outline fit, decoupling pairs from `power` pins, connectors at the edges, crossings and silkscreen; `place_subset` for one module. To do: hierarchical placement (each module on its own, then the blocks), a face for a part chosen for wire length, the escape ring of a fine-pitch part as a keepout, and weights learned from hand-laid boards (ADR 0030 A5).
3. **Findings as data.** The editor's findings panel lists netsmith's placement problems and the connections the router left, and KiCad's check on demand, each with a point and a box that a click zooms to (built, E3). Constraint findings with the numbers are listed too (ADR 0045). To do: the same as `--json` on the CLI (`netsmith layout check`), waivers.
4. **Datasheet layout knowledge.** A `layout` block in a part with a mandatory `source` (near, away, loop, keepout, edge, vias, route), first for `LinearRegulator` and a buck IC (P3).
5. **Regenerate one module.** Partial rip-up and re-route in the router, a routes cache, preview and undo in the editor (P4, E4); then the `netsmith-use-layout` skill.

Open: both-sided placement, a rasteriser for agent screenshots. The open questions are ranked at the end of the ADR.

## Rules for working here

- Nothing is left as a known bug. If it cannot be fixed now, it goes in `README.md` under
  Known limits and here.
- A part number goes into a catalog only with a recorded source.
- Notes and decisions live in the repo, not in anyone's memory.
- Commit only when asked, staging files by explicit path.

## Self-contained: no KiCad needed

The end goal (the user, 2026-11-02; see ADR 0021, "Direction"): the `netsmith` binary alone is enough. Today KiCad is needed for four things; each is a step:

1. **Symbols and footprints.** Read from the installed KiCad libraries, or a project's own. Needed: libraries netsmith ships or fetches itself (an importer from `.kicad_sym`/`.kicad_mod` files into a netsmith format or bundled copies, with each library's licence recorded; KiCad's stock libraries are CC-BY-SA 4.0 with a design exception), and the editor's own symbol/footprint drawing.
2. **Fabrication files. Done (ADR 0034, provisional).** netsmith writes RS-274X Gerber (every layer, with its own zone fill and stroke font), Excellon drill and the placement file; `kicad-cli` is only the optional cross-check (`fab --cross-check`, bitmaps of every layer, holes and placement). Not done: JLCPCB has not yet taken these files in an order (the order preview is the user's), islands and thin slivers of a pour are not removed, silkscreen text is a font of our own.
3. **Design rules check. Done (ADR 0037, provisional); ERC is not.** netsmith's own check of the board file (clearances per net class, shorts, track and via sizes, annular ring, drills, hole spacing and hole clearance, copper and holes to the edge, courtyards, silkscreen, solder mask, rule areas, connectivity with pours, the outline) is the gate in `fab`, `fab --order` and the editor, and its findings are in the editor's Findings tab. KiCad's check is only the optional `--cross-check` witness. Validated against KiCad's on a corpus of broken boards, the examples, synthetic boards and routed random boards (numbers in the ADR). Not done: ERC (the solver does most of the electrical ones), impedance, acid traps, copper slivers and islands of a pour, a computed fill in the check, custom rules, blind and buried vias.
4. **Netlist and zone fill.** `compare` reads a netlist exported by `kicad-cli`. Needed: read `.kicad_sch`/`.kicad_pcb` directly for `compare`. Zone fill for the Gerbers is done in netsmith (step 2, ADR 0034, without island removal); KiCad's Python fills the planes only for the optional cross-check (step 3 is done: netsmith's own design rules check, ADR 0037).

When these are done KiCad becomes an optional exporter (`.kicad_sch`, `.kicad_pcb`) and an independent cross-check.

## Ordering boards and panels

Done (ADR 0035, provisional, chosen by the agent on the user's "you pick"): **orders**. A named `[order.<name>.boards]` table in `netsmith.toml` maps boards (`Board`, `System/member`, or a system for all its members) to quantities; `netsmith fab --order <name>` makes only those boards into `fab/orders/<name>/` with an `order.txt` summary (layers, size, thickness, hand-solder parts, notes to read, unverified items) and `order-bom.csv` (combined BOM, copies times parts, no spares); `--order` alone lists them; the editor's Build panel runs them. Review the spelling and the folder layout. Open: spares or overage in the bill of materials, the surface finish in the summary (needs a field on the process, sourced), reusing a package a plain `fab` just made, and editing orders in the editor.

Not built, and needs a decision from the user:

1. **Panels.** Done (ADR 0040, stage P0): a `panel` definition of identical boards in a V-cut grid with optional rails, its Gerber, drill, BOM, CPL, notes, `.kicad_pcb` and drawing, the panel-level checks with JLCPCB's published numbers, an order key that names a panel, and a read-only preview in the editor's PCB view. Still to do, in order: **P1** different boards on one panel in rows of one height (a V-cut must run across), a board of a system as the cell, a cell in another file, rotation by 90 degrees and per-cell placement from the layout file; **P2** routed tabs with mouse bites, tab placement, mixed spacing, `Panel by JLCPCB` (the fab adds rails and fiducials); **P3** a panel view in the editor that moves cells (writing the layout file). P2 should wait for one real JLCPCB quote with a P0 panel, which also settles the unverified items: the designators of a panel made by the customer and how their viewer reads V-cut lines on `Edge.Cuts`.
