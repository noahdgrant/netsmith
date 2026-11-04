# netsmith

Circuits as code, in Rust. A board is a set of parts and the nets that join their pins,
written as text. netsmith checks it, picks real parts that work in the worst case, and
exports files for CAD tools and for a board house. It writes a KiCad 9 schematic, and a placed
and routed PCB with a Gerber package for JLCPCB, on two layers or on four with ground and
supply planes inside. The editor (`netsmith editor`) shows the source and the board in a
browser.

Status: early (version 0.0.1). The examples in `examples/` build and pass KiCad's checks.

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

- Build from a checkout: `cargo install --path crates/netsmith`. It needs only Rust; the
  editor's page is committed as a built bundle.
- Or take the `netsmith-<version>-<target>.tar.gz` archive (`.zip` on Windows) from the
  releases, check it against its `.sha256` file and put the binary on your `PATH`.
  `scripts/release-build.sh` makes the archives. No release is published yet (#7).

`build` and `fab` read KiCad's symbol and footprint libraries for the parts netsmith does not
generate itself, so install KiCad 9 (8 also works). netsmith finds the `symbols` and
`footprints` folders by itself; set `NETSMITH_KICAD_SYMBOLS` and `NETSMITH_KICAD_FOOTPRINTS`
to name them. If it finds none, it stops and lists every place it looked (ADR 0021). Only
Windows KiCad 9 under WSL has been tried on a real install (#6).

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
with. Check JLCPCB's order preview for those.

## Where to read next

- `docs/LANGUAGE.md`: the language guide, with a quick start, every construct with a tested
  example and the real error you get when you misuse it, and the standard library.
- `docs/CONCEPTS.md`: how parts, modules, boards and systems combine.
  `docs/PROJECT-LAYOUT.md`: a repository with several boards.
- `GLOSSARY.md`: the vocabulary. `docs/DESIGN-RULES.md`: the rules the language follows.
  `docs/adr/`: the decisions behind the design.
- Contributors and agents: `AGENTS.md`. Open work is in the
  [GitHub issues](https://github.com/noahdgrant/netsmith/issues).

## Limitations

- Price and stock are unknown until a supply provider is asked, and no provider asks a
  supplier yet (#8, #30). Catalogs hold only verified parts (#66); only resistors can be
  generic (#31).
- netsmith writes its own Gerbers, drill and placement files, and JLCPCB has not yet taken
  them (#12, #61).
- A board outline is a rectangle with equal rounded corners (#57). A panel repeats one board
  on a V-cut grid (#62).
- The placer and router can leave a tight board unrouted (#3, #58).
- Physical intent (the outline, cutouts, holes, keepouts and where parts must be) is in a file of
  its own beside a board, `<stem>.physical.nsm`, and the same lines in the board are an error
  (ADR 0048, #52). The board level is built; the module layer, a physical file for a `system` or
  a `panel`, evaluating a relation between two parts, and the editor writing its keepouts to that
  file (it writes the layout file) are not. The provisional readings are in the ADR's "Built".
- Several of the decisions in `docs/adr/` are provisional and unreviewed (#10).

## Licence

netsmith is MIT licensed (`LICENSE`).

netsmith keeps no copy of KiCad's symbols or footprints. It reads the symbols, and the
footprints that it does not make itself, from the ones installed with KiCad (CC-BY-SA 4.0 with
a design exception) at run time, and the tests that need no KiCad write small libraries of
their own (`netsmith_kicad::testlib`). The footprints it makes are original code that computes
IPC-7351B land patterns from published package dimensions (ADR 0043); the test that checks
them reads KiCad's footprints and bundles none. The formulas and tables were learned from the
`kicad-footprint-generator` repository (GPL-3.0-or-later; no code was copied) and the package
dimensions of the QFN rows are numbers from its data files and the datasheets they cite.
