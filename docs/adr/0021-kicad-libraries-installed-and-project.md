---
status: accepted (user review 2026-11-02)
---

# KiCad libraries: installed and project

ADR 0016 decided that the symbols and footprints of a part are read from the installed KiCad libraries and that a part's kind and package are mapped to their names in a TOML file. This ADR records what was built for its steps 1, 2, 3, 6, 11 and 13, and the decisions taken while building them. The choices below are provisional, to be reviewed by the maintainer. Two of them replace ADR 0016: a multi-unit symbol is supported, and a project's own libraries are supported.

## What was built

**A loader for `.kicad_sym` files** (`crates/netsmith-kicad/src/symbols.rs`). `SymbolLib` reads a library file once per run and finds a symbol by `Library:Name`. A symbol that `extends` another is flattened: the parent's drawing, pins and settings, with the child's properties on top, and its units renamed for the child because KiCad wants a unit's name to start with its symbol's. Every KiCad electrical type is read, with the pin's name, number, position, direction, length, whether the symbol hides it, and the unit it is in. A hidden pin that repeats another pin's point (how stock symbols stack the pads of one supply) is an ordinary pin. Only the first body style lists pins. A missing library says which file it looked for and the variable that moves it; a missing symbol names its library.

**The mapping is TOML.** `std/kicad.toml` is compiled into the binary and has a `[[part]]` row for each (kind, package): `symbol`, `footprint`, optionally `default = true` (the row used when the part has no package) and a `[part.pin_names]` table for pins whose symbol name differs from the datasheet's. The Rust `ENTRIES` table is gone, and so is the `through_hole` flag: whether a footprint is through hole is read from the footprint file's `attr`. A (kind, package) pair in both the standard file and a project's, or twice in one, is an error that names both files.

**A pinout is validated against its symbol and footprint** (`crates/netsmith-pcb/src/validate.rs`, ADR 0016 decision 8): every pad of the pinout is a numbered pad of the footprint and a pin of the symbol, and the other way round; the symbol pin on a pad has the datasheet name after `pin_names` (case does not matter, a symbol pin named `~` is unnamed and accepted); the symbol pin's electrical type suits the kind. Every message names the part, the package and both sides. `build` and `fab` run it for every part with a pin table before the schematic is written, and stop with all the messages (`crates/netsmith/tests/pinout.rs`, with invented libraries). A test (`crates/netsmith/tests/kicad_rows.rs`) takes each row of `std/kicad.toml`, the pinout of its kind in that package from the standard library's own source, and checks it against the installed libraries (skipped without KiCad) and, for the kinds they hold, against our sample libraries.

**The emitter reads from the loader.** One net label for each distinct point and net (stacked pads share one), a `no_connect` flag on every pin that no net reaches, and the power-flag rule from the symbol's electrical types as before. The five examples export byte for byte what they did before the loader existed, and pass KiCad's electrical rules check with no violation (`crates/netsmith/tests/erc.rs` runs it).

**The viewer reads from the loader** (`netsmith-view`): `Glyph::for_part` takes the mapping and the `SymbolLib`; a hidden pin that repeats a shown pin's point and name resolves its pad and is not drawn; bidirectional and tri-state pins have a `Flow`; the units of a symbol are drawn one below the other; and the box drawn where there is no symbol lists every pin of the package from the part's pin facts (supplies on top, grounds below, inputs left, outputs right, no-connects last, the chosen function beside the name). A missing library gives the box and the reason as its tooltip.

**The bundled copies are gone**: `crates/netsmith-kicad/symbols/` and `crates/netsmith-pcb/fixtures/` are deleted. Tests write small libraries of their own (`netsmith_kicad::testlib`), and the tests that want the stock libraries skip without KiCad. The licensing note in the README says so.

## Decisions

**Project libraries are supported, and listed one way.** A real design keeps its ICs in a library of its own. The project's `kicad.toml` (the file of the mapping) lists them: `[[library]] name = "acme"`, `symbols = "libs/acme.kicad_sym"`, `footprints = "libs/acme.pretty"`, paths relative to the file, `${NAME}` expanded from the environment and nothing else. The mapping then writes `symbol = "acme:Part"` and `footprint = "acme:Fp"`. A library named like an installed one is an error, not an override. KiCad's own `sym-lib-table` and `fp-lib-table` are not read as a second way to configure: `netsmith kicad-libs import <sym-lib-table> [<fp-lib-table>]` prints the `[[library]]` stanzas to paste, which makes porting a KiCad project easy and is a generator, not a configuration. When it writes a board, `build` (and `fab`, and the editor's build and fab) writes `sym-lib-table` and `fp-lib-table` beside the `.kicad_pro`, with `${KIPRJMOD}`-relative paths, so that KiCad finds the project's libraries when it opens the board.

**Multi-unit symbols are supported.** ADR 0016 refused them and placed unit 1 only. The loader keeps the pin-to-unit assignment; the emitter places every unit of a part as its own instance with the same reference and the right `unit` field, in a column, each unit with its own pins and labels; a pin drawn in the shared unit 0 is labelled in every unit. Units are spaced by their pins' extent. A part's cell on the sheet is as large as its units and, for a symbol of more than four pins, the labels that run from them; a small part keeps the grid it had, so the existing exports did not move. KiCad's ERC passes on a dual op-amp-like symbol with two amplifier units and a supply unit, and on one with a pin in the shared unit (`erc.rs`).

**A `passive` part pin suits any symbol type but a supply or a no-connect.** ADR 0016 allowed `passive` or `unspecified` only. KiCad types a USB-C configuration pin `bidirectional` and a mounting-hole pad `input`; a pin the part calls passive (neither takes nor gives) does not contradict a symbol that says more. A `power_in`, `power_out` or `no_connect` symbol pin still does. The other kinds follow ADR 0016.

**`build` and `fab` need the libraries; `check`, `solve`, `compare` and the editor's reading do not.** `outcome_with` takes `Option<&Libraries>`; with `None` no KiCad file is read, and the board's `schematic`, `project` and `report` are empty. `build`, `fab` and the editor's build and fab read them and stop, once, with the path and the variable when one is missing. The editor draws a box where a symbol is missing.

**A footprint pad of type `connect`** (the Tag-Connect footprints) is read as a surface pad.

## Where the installed libraries are found

One lookup (`crates/netsmith-kicad/src/install.rs`) serves the symbol loader, the footprint loader and everything built on them. For each of `symbols` and `footprints` it takes the first of these:

1. `NETSMITH_KICAD_SYMBOLS` or `NETSMITH_KICAD_FOOTPRINTS`, used as given even when the folder is missing (the error then names it);
2. the folder beside `kicad-cli` (`$KICAD_CLI`, else `kicad-cli` or `kicad-cli.exe` on `PATH`, links followed): `<prefix>/share/kicad/<folder>` for `<prefix>/bin/kicad-cli` (Linux, Windows, snap, flatpak), and `Contents/SharedSupport/<folder>` for `Contents/MacOS/kicad-cli` (macOS);
3. the usual places, with KiCad 9 before 8 where a place carries a version: `/usr/share/kicad`, `/usr/local/share/kicad`, `/snap/kicad/current/usr/share/kicad`, the flatpak `app/org.kicad.KiCad/current/active/files/share/kicad` under `~/.local/share/flatpak` and `/var/lib/flatpak`, `/Applications/KiCad/KiCad.app/Contents/SharedSupport`, `%ProgramFiles%\KiCad\<9.0|8.0>\share\kicad`, and `/mnt/c/Program Files/KiCad/<9.0|8.0>/share/kicad` (Windows seen from WSL).

A folder counts only if it holds a `*.kicad_sym` file (symbols) or a `*.pretty` folder (footprints). netsmith is tested with KiCad 9; 8 is a fallback that nobody has tried. Finding nothing is not an error by itself: it is raised when a symbol or footprint of an installed library is asked for, and says what was looked for, every place tried and why it did not count, and the two variables to set. A design that uses only a project's own libraries, or footprints the generators make (ADR 0043), builds with no KiCad install.

Verification: the WSL place is the only one tried on a real install (Windows KiCad 9.0). The Linux, macOS, Windows, snap and flatpak places follow the layouts those packages are known to use and are tested only against invented install trees (`Search::under`); the machine this was written on has no Linux, snap or flatpak KiCad. The snap and flatpak paths are the least certain.

## The standard rows and the USB-C pads

The rows for ADR 0023's kinds use stock KiCad 9 names, read from the installed libraries and not guessed. Validation confirmed or corrected the pad numbers ADR 0023 had written from memory: the Type-C pads A1, A4, A5, A6, A7, A9, A12, B1, B4, B5, B6, B9, B12 and S1 are right for the sixteen-contact receptacle, which also has the sideband pads A8 and B8 (now the open pins `sbu1` and `sbu2` of `UsbC2`); the six-contact power-only receptacle has no A1, A4, B1 or B4, so `UsbCPower` has VBUS on A9 and B9 and GND on A12 and B12. Kinds with a caveat are listed in ADR 0023.

## Limits

- The PCB side has not been tried on an exposed pad, fine pitch or several pads on a net (ADR 0016, step 10).
- The schematic is a grid of cells with labels and no wires; a unit's body is not measured, only its pins, with a fixed gap between units.
- A symbol's `(alternate ...)` pin names are not compared with a pin's functions.
- The version in a library file's header is not compared with the KiCad that `kicad-cli` is.
- The value and hand-solder flag still come partly from a `match` in `netsmith-lower`. The reference letters no longer do: every part has a `.designator` field that its kind states (ADR 0046).

## Direction (the user, 2026-11-02)

netsmith must not need KiCad installed. The end goal is a self-contained tool: a person with the `netsmith` binary alone can write a design, check it, solve it, see it and make the files a board house takes. KiCad stays a supported export (and a cross-check), never a requirement.

What this ADR builds is therefore a stopgap. Today KiCad is needed for: (1) the symbol and footprint libraries, read from the installed KiCad (this ADR); (2) `kicad-cli`, for the design rules check, the Gerber and drill files, the placement file, zone fill and the netlist export that `netsmith compare` uses; (3) ERC. The steps to remove each dependency are in `docs/ROADMAP.md` ("Self-contained: no KiCad needed"). Until they are done, the tests that need KiCad skip themselves when it is absent, and `build`, `check`, `solve` and `view` for a part with a bundled symbol must keep working without it.
