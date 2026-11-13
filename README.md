<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/logo-dark.svg">
    <img alt="netsmith" src="docs/assets/logo-light.svg" width="320">
  </picture>
</h1>

A language and tools for designing electronics as code: schematics, solvers, checks and PCB fabrication outputs.
A board is a set of parts and the nets that join their pins,
written as text. netsmith checks it, picks real parts that work in the worst case, and
exports files for CAD tools and for a board house. It writes a KiCad 9 schematic, and a placed
and routed PCB with a Gerber package for JLCPCB, on two layers or on four with ground and
supply planes inside. The editor (`netsmith editor`) shows the source and the board in a
browser.

Status: early (v0.1). The examples in `examples/` build and pass KiCad's checks where KiCad is installed, except `carrier_and_cards.nsm`, whose invented parts have no KiCad symbol (it is checked and solved only). The rest build and fab with no KiCad at all (see Install). Some of the decisions behind the language are still `proposed` and wait for review (see Limitations).

## An example

`examples/led.nsm` is a board with a 5 V supply, a resistor and an LED. The `==` line states
the circuit's equation; netsmith picks a resistor and an LED from a catalog so that it holds in
the worst case, for every tolerance.

```nsm
board LedExample {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }

    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .led: Led {
        .current &= 5mA..15mA
    }
    .r1: Resistor {}

    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2

    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
```

The imports and doc comments are left out here; the complete file is `examples/led.nsm`.

## Install

- Build from a checkout: `cargo install --path crates/netsmith --locked`. It needs only Rust;
  the editor's page is committed as a built bundle.
- Or straight from GitHub, with no checkout:
  `cargo install --git https://github.com/noahdgrant/netsmith netsmith --locked`.
- Or take the `netsmith-<version>-<target>.tar.gz` archive (`.zip` on Windows) from the
  [releases](https://github.com/noahdgrant/netsmith/releases), check it against its `.sha256`
  file and put the binary on your `PATH`. The release workflow builds them for Linux, macOS and
  Windows (`scripts/release-build.sh` makes one) and attaches them to a release when its tag is
  pushed. v0.1.0 has none: the workflow failed on its first run, and the failures are fixed.
- netsmith is not on crates.io yet.

### KiCad is optional

netsmith needs no KiCad to write, check, solve, see, build or fab a board, and it needs no
KiCad library or `kicad-cli` for any of them (ADR 0097). It draws the ICs and the linear
regulator from the part's pin table, as a plain box, and the resistors, capacitors, inductors,
LEDs, diodes, FETs, fuses, crystals, switches, ferrite beads, headers and sockets, USB-C and
coaxial connectors, test points, mounting holes, fiducials and the power flag as conventional
glyphs, with the pins where KiCad's symbols have them. Every footprint of the standard
library is generated or written from the maker's drawing (`std/libs/`). The `.kicad_sch` and
`.kicad_pcb` files it writes are KiCad's formats, and KiCad opens them. CI proves it: the
`no-kicad` job builds and fabs the examples on a runner with no KiCad, with the libraries and
`kicad-cli` hidden (`scripts/check-no-kicad.sh`), and fails if a run says KiCad is missing. No
example names a part of KiCad's own, so every one of them builds and fabs.

KiCad 9 is needed for these, and for nothing else:

- The ERC and DRC cross-check: `kicad-cli`, and in the editor the PCB findings that run
  KiCad's check (without it the findings list says "KiCad not found: set KICAD_CLI or install
  KiCad", once). `fab --cross-check` plots the board with KiCad as well, and `fab` runs
  netsmith's own design rules check without it.
- `netsmith compare` of a `.kicad_sch` file (it exports a netlist with `kicad-cli`; a
  `.kicad_pcb`, a KiCad netlist and a `.nsm` file need none).
- A project whose own `kicad.toml` names a KiCad symbol or footprint that netsmith does not
  make, with no `[[library]]` of the project and no `gen:` footprint for it. Only then does a
  run stop, and the message names the part, the id and the line of `kicad.toml`, lists where
  netsmith looked, and gives the three ways out: install KiCad 9, supply the file through a
  `[[library]]`, or use a `gen:` footprint (or omit `symbol`, to have the symbol drawn from the
  pin table).

netsmith finds KiCad's `symbols` and `footprints` folders by itself, and `kicad-cli` as
`$KICAD_CLI`, else on `PATH`, else (under WSL only) the newest Windows KiCad in
`/mnt/c/Program Files/KiCad/<version>/bin`; set `NETSMITH_KICAD_SYMBOLS` and
`NETSMITH_KICAD_FOOTPRINTS` to name the folders. `NETSMITH_NO_KICAD=1` makes every one of those
searches find nothing, whatever is installed (for tests and CI). KiCad 9 is the version
tested: Windows KiCad 9 under WSL on a real install, KiCad 9 on Linux in the CI `kicad` job,
and, in the release workflow, the starter board's `init`, `build` and `fab` with KiCad 10 on
macOS and Windows runners (#6). The folders of KiCad 8 are looked for too, and nobody has
tried them.

## Use it

```sh
mkdir blink && cd blink
netsmith init       # netsmith.toml, src/main.nsm (a working LED board), agent files
netsmith build      # a KiCad schematic in out/
netsmith editor     # the source and the board in a browser, at 127.0.0.1:8787
netsmith fab        # a package for JLCPCB in fab/: Gerbers, drill, bom.csv, cpl.csv, fab-notes.txt, the board's 3D scene as `<board>.glb` (`--no-glb` or `glb = false` in netsmith.toml leaves it out), the board as STEP for mechanical CAD as `<board>.step` (`--no-step` or `step = false` leaves it out), and for a board with pin requests its pin map for firmware from the final layout (`.pins.h`, `.pins.json`)
netsmith board3d --json   # the board's 3D facts for an agent: heights, limits, overlaps (docs/BOARD3D.md)
netsmith board3d --glb board.glb   # the same scene as glTF 2.0 binary, the file the editor's 3D view shows
netsmith render --view iso --out board.png   # the board drawn in 3D to a PNG by netsmith's own rasterizer, no GPU or KiCad (docs/BOARD3D.md)
netsmith view --view top --highlight J1 --picture j1.png   # drive the 3D view of the running editor with render's flags, and save its picture (docs/BOARD3D.md)
netsmith compare a.kicad_sch b.nsm   # are two designs the same circuit?
```

Without a project, `netsmith build examples/led.nsm --out out` works on one file. `netsmith
--help` and `netsmith <command> --help` document every command and flag, and `docs/LANGUAGE.md` ("Projects and netsmith.toml")
describes the project folder.

The first `build` or `fab` also writes `src/main.lock.toml` beside each source file: the catalog
row each part was picked as. Commit it, as you commit `Cargo.lock`. Later runs hold each part
to its row, and `--locked` makes a run that would change the picks an error (`docs/LANGUAGE.md`,
"The pick lock"). `netsmith lock --update` refreshes the lock after a catalog edit and prints
what changed, and `netsmith lock --check` fails when it is not current, for CI.

The boards in `examples/` carry their locks, committed beside the sources, and CI runs
`netsmith lock --check` over every one of them (`scripts/check-example-locks.sh`, the `locks` job). A
change to a catalog, `std/` or the solver that would move a pick of an example fails the job until
someone runs `netsmith lock --update` in the example and the diff of the lock is reviewed. Each
example also commits its design, `<stem>.design.json` (`netsmith build --emit-design`), which the
`designs` job checks the same way (`scripts/check-example-designs.sh`, whose `--update` rewrites it).
`NETSMITH_TIMINGS=1` prints how long each stage took.

**Read `fab-notes.txt` before you order.** It lists what netsmith could not verify, for
example the pick-and-place rotation of a package that no board of yours has been assembled
with, the parts you solder yourself (`hand-solder:`) and the parts you ship to the board house
(`consigned:`, ADR 0084). Check JLCPCB's order preview for those.

## Where to read next

- `docs/LANGUAGE.md`: the language guide, with a quick start, every construct with a tested
  example and the real error you get when you misuse it, and the standard library.
- `docs/PIPELINE.md`: the stages a design goes through, from source to a fab package, with diagrams.
- `docs/BOARD3D.md`: `netsmith board3d`, the board's 3D scene as a JSON report and as a `.glb` file, and `netsmith render`, the same scene as a PNG, and `netsmith view`, which drives the 3D view of a running editor.
- `docs/CONCEPTS.md`: how parts, modules, boards and systems combine.
  `docs/PROJECT-LAYOUT.md`: a repository with several boards.
- `docs/symbol-gallery.html` (open it in a browser): every symbol netsmith draws, beside the KiCad
  symbol it replaces. `cargo run -p netsmith --example symbol_gallery -- --out docs --file
  symbol-gallery.html` writes it again.
- `GLOSSARY.md`: the vocabulary. `docs/DESIGN-RULES.md`: the rules the language follows.
  `docs/adr/`: the decisions behind the design.
- Contributors and agents: `AGENTS.md`. Open work is in the
  [GitHub issues](https://github.com/noahdgrant/netsmith/issues).

## Limitations

netsmith analyzes a worst-case DC operating point of the picked parts: every voltage, current and power is checked over the ranges the design states (ADR 0076). It does not model time. Startup, inrush, ripple, filter response, AC and heating are not analyzed (a part's power rating is checked; its temperature is not computed). Closed-form checks for some of these, such as an RC time constant or a buck's output ripple, are the next step and are laid out as options in `docs/research/closed-form-checks.md`. Running a circuit simulator on demand is a later option, issue #306.

- Price and stock are unknown until a supply provider is asked, and no provider asks a
  supplier yet (#8, #30). Catalogs hold only verified parts (#66). A generic part (a value
  no catalog row has) exists for resistors, ceramic capacitors, inductors and LEDs; a generic
  capacitor has no bias curve and resolves only to a row that has one (ADR 0047, #31).
- netsmith writes its own Gerbers, drill and placement files, and JLCPCB has not yet taken
  them (#12, #61).
- A board outline is a rectangle with equal rounded corners or a `polygon` of corners, fillets
  and arcs, and a cutout is a rounded rectangle, a slot or a circle (#57). A panel repeats
  one board on a V-cut grid (#62).
- The placer and router can leave a tight board unrouted (#3, #58).
- Physical intent (the outline, cutouts, holes, keepouts and where parts must be) is in a file of
  its own beside a board, `<stem>.physical.nsm`, and the same lines in the board are an error
  (ADR 0048, #52). The board level is built, and the placer and `fab` hold a relation between
  two parts (ADR 0045); the module layer, a physical file for a `system` or a `panel`, and the
  editor writing its keepouts to that file (it writes the layout file) are not. The readings are in the ADR's "Built".
- Nine ADRs in `docs/adr/` are `proposed` and wait for review: 0049, 0050, 0051, 0072, 0098, 0099, 0100, 0102 and 0103.

## Licence

netsmith is MIT licensed (`LICENSE`).

netsmith keeps no copy of KiCad's symbols or footprints. The symbol of an IC is drawn by
netsmith from the part's pin table (a plain box and pins, ADR 0097), and those of the passives,
discrete parts, connectors and mechanical parts from the electrical and mechanical conventions
(IEC 60617, IEEE 315), not from KiCad's files. Only for a symbol or footprint that a project's
own `kicad.toml` takes from KiCad and that netsmith does not make does it read the ones
installed with KiCad (CC-BY-SA 4.0 with a design exception) at run time, and the tests that need
no KiCad write small libraries of their own (the dev-only crate `netsmith-testkit`). For a package or a part that KiCad's libraries lack,
`std/libs/` holds a small symbol and footprint library of netsmith's own, written by netsmith
from a manufacturer's package drawing (MIT, like the rest; ADR 0087). The footprints it makes are original code that computes
IPC-7351B land patterns from published package dimensions (ADR 0043); the test that checks
them reads KiCad's footprints and bundles none. The formulas and tables were learned from the
`kicad-footprint-generator` repository (GPL-3.0-or-later; no code was copied) and the package
dimensions of the QFN rows are numbers from its data files and the datasheets they cite.
