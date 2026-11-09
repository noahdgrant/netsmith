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

Status: early (v0.1). The examples in `examples/` build and pass KiCad's checks, except `carrier_and_cards.nsm`, whose invented parts have no KiCad symbol (it is checked and solved only). Many of the decisions behind the language are provisional and unreviewed (see Limitations).

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

`build` and `fab` read KiCad's symbol and footprint libraries for the parts netsmith does not
generate itself, so install KiCad 9 (8 also works). netsmith finds the `symbols` and
`footprints` folders by itself; set `NETSMITH_KICAD_SYMBOLS` and `NETSMITH_KICAD_FOOTPRINTS`
to name them. If it finds none, it stops and lists every place it looked (ADR 0021). What has
been tried: Windows KiCad 9 under WSL on a real install, KiCad 9 on Linux in the CI `kicad`
job, and the starter board's `init`, `build` and `fab` with KiCad 10 on macOS and Windows
runners in the release workflow (#6). The program
`kicad-cli` is `$KICAD_CLI`, else on `PATH`, else (under WSL only) the newest Windows KiCad in
`/mnt/c/Program Files/KiCad/<version>/bin`; without any, the editor says
"KiCad not found: set KICAD_CLI or install KiCad".

## Use it

```sh
mkdir blink && cd blink
netsmith init       # netsmith.toml, src/main.nsm (a working LED board), agent files
netsmith build      # a KiCad schematic in out/
netsmith editor     # the source and the board in a browser, at 127.0.0.1:8787
netsmith fab        # a package for JLCPCB in fab/: Gerbers, drill, bom.csv, cpl.csv, fab-notes.txt
netsmith compare a.kicad_sch b.nsm   # are two designs the same circuit?
```

Without a project, `netsmith build examples/led.nsm --out out` works on one file. `netsmith
--help` lists every command and flag, and `docs/LANGUAGE.md` ("Projects and netsmith.toml")
describes the project folder.

**Read `fab-notes.txt` before you order.** It lists what netsmith could not verify, for
example the pick-and-place rotation of a package that no board of yours has been assembled
with, the parts you solder yourself (`hand-solder:`) and the parts you ship to the board house
(`consigned:`, ADR 0084). Check JLCPCB's order preview for those.

## Where to read next

- `docs/LANGUAGE.md`: the language guide, with a quick start, every construct with a tested
  example and the real error you get when you misuse it, and the standard library.
- `docs/PIPELINE.md`: the stages a design goes through, from source to a fab package, with diagrams.
- `docs/CONCEPTS.md`: how parts, modules, boards and systems combine.
  `docs/PROJECT-LAYOUT.md`: a repository with several boards.
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
- A board outline is a rectangle with equal rounded corners (#57). A panel repeats one board
  on a V-cut grid (#62).
- The placer and router can leave a tight board unrouted (#3, #58).
- Physical intent (the outline, cutouts, holes, keepouts and where parts must be) is in a file of
  its own beside a board, `<stem>.physical.nsm`, and the same lines in the board are an error
  (ADR 0048, #52). The board level is built, and the placer and `fab` hold a relation between
  two parts (ADR 0045); the module layer, a physical file for a `system` or a `panel`, and the
  editor writing its keepouts to that file (it writes the layout file) are not. The provisional
  readings are in the ADR's "Built".
- Several of the decisions in `docs/adr/` are provisional and unreviewed (#10).

## Licence

netsmith is MIT licensed (`LICENSE`).

netsmith keeps no copy of KiCad's symbols or footprints. It reads the symbols, and the
footprints that it does not make itself, from the ones installed with KiCad (CC-BY-SA 4.0 with
a design exception) at run time, and the tests that need no KiCad write small libraries of
their own (`netsmith_kicad::testlib`). For a package or a part that KiCad's libraries lack,
`std/libs/` holds a small symbol and footprint library of netsmith's own, written by netsmith
from a manufacturer's package drawing (MIT, like the rest; ADR 0087). The footprints it makes are original code that computes
IPC-7351B land patterns from published package dimensions (ADR 0043); the test that checks
them reads KiCad's footprints and bundles none. The formulas and tables were learned from the
`kicad-footprint-generator` repository (GPL-3.0-or-later; no code was copied) and the package
dimensions of the QFN rows are numbers from its data files and the datasheets they cite.
