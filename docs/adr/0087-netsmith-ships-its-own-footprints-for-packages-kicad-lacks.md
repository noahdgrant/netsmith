---
status: provisional (written 2026-11-08 from the owner's decision of the same day; the choices below are the implementer's, for the owner to confirm)
---

# netsmith ships its own footprints for the packages KiCad lacks

The LT8641 (PR #252, `std::parts::regulator::Lt8641`) comes in one package, the 18-lead 3 x 4 mm QFN that the datasheet calls UDC20(18). KiCad's library has no footprint for it (its `UDC-QFN-20-4EP_3x4mm_P0.5mm_EP0.41x0.25mm` is the LT3033's: Exposed Pad Variation AB, four exposed pads, where this one is Variation AA, two), and no symbol either. So `std/kicad.toml` had no row for the part, and a board with it could not be built until its project mapped the part itself. The owner decided on 2026-11-08, asked what `std` should do, for **a netsmith-owned `std` footprint library**. This ADR is that library. It builds on ADR 0041 (netsmith makes the footprints it can and KiCad's file formats stay the format) and on ADR 0043 (the generators), and adds the case neither covers: a package that is one maker's drawing, which is neither in KiCad's libraries nor a rule the generators can state.

## What it is

A folder of KiCad files in the repository, `std/libs/`: footprints as `.kicad_mod` in `std/libs/netsmith.pretty/` and symbols in `std/libs/netsmith.kicad_sym`. They are compiled into the binary (`crates/netsmith-kicad/src/stdlib.rs`, as `std/kicad.toml` is) and answer to one library name, `netsmith`. A row of `std/kicad.toml` names one like any other id, so there is no new key and no new syntax:

```toml
[[part]]
kind = "Lt8641"
package = "UDC20(18)"
symbol = "netsmith:LT8641"
footprint = "netsmith:UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA"
```

`FootprintLibrary::load` and `SymbolLib::symbol` answer `netsmith:` ids from the binary before anything else is read: the same pads on every machine, with no KiCad installed and no folder. A project's own `kicad.toml` may not list a library called `netsmith` (`Mapping::parse` refuses it and says why; `FootprintLibrary` refuses the same clash with `AmbiguousStd`), as it may not name a library like an installed one (ADR 0021) or a generated one (ADR 0043). A project that wants another land pattern for the part writes its own package name in its own row.

**The export copies the library into the project's table like any other.** `Libraries::write_tables` (used by `build`, `fab`, the editor's build and the `place_report` example) writes, for a board with a part that uses a `netsmith:` footprint or symbol, the library beside its `.kicad_pro` (`netsmith.pretty/<footprint>.kicad_mod`, `netsmith.kicad_sym`) and lists it first in `fp-lib-table` and `sym-lib-table`, with `${KIPRJMOD}`-relative paths as for a project's libraries. KiCad then opens the board with the footprint in its library table, and "update footprints from library" finds it. A board that uses none of them gets nothing more than before (no `netsmith` files, no table that was not there). `kicad-libs import` leaves the `netsmith` entry out of the stanzas it prints, so a netsmith-made board can be ported back without a reserved-name error.

**Checks.** `crates/netsmith/tests/kicad_rows.rs` (no KiCad needed) parses every shipped footprint through the loader, checks the pinout of its part against its symbol and its pads with the same `validate_pinout` that `build` runs (every pad of the pin table is a pad of the footprint and a pin of the symbol, and the other way round; names and electrical types agree), and fails for a file of `std/libs/netsmith.pretty/` that no row of `std/kicad.toml` names, since its pads would then be checked against nothing. `stdlib.rs` fails when the folder and the list of embedded files differ. A unit test in `footprint.rs` pins every pad of the LT8641's footprint to the datasheet's numbers (below). `crates/netsmith/tests/lt8641.rs` builds a board around the IC with the sample libraries and the generators (no KiCad), exports its schematic, `.kicad_pcb` and library tables, and checks that the footprint and the library are in them.

## The first footprint: `netsmith:UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA`

Source: Analog Devices, LT8641 Rev. C, the LCSC mirror the std part cites (`std/parts/regulator.nsm`, `data/jlcpcb/README.md`: SHA-256 `09763bcb...965a43`), the package description on **p.24** (UDC20(18), LTC drawing 05-08-1956 Rev C, Exposed Pad Variation AA: "Recommended solder pad pitch and dimensions", and the bottom view), and the pin configuration on **p.2** for the numbering. The drawing says it is not to scale (note 2), so no dimension below was taken by measuring the picture with a ruler; each is a dimension label of the drawing, read at 500 to 1000 dpi to see which extension lines each label belongs to. Numbers are mm; x is to the right and y down in the footprint, the body centre is the origin, pin 1 is at the top left as in the top view of p.2.

**Read from the drawing (nominal values; the datasheet's tolerance, ±0.05 on pads, is not applied):**

- Lead pads, 0.70 long and 0.25 wide (labels 0.70 and 0.25 ±0.05), at 0.50 pitch (0.50 BSC; 1.50 REF over the four of a short side, 2.50 REF over the six slots of a long side).
- The inner edges of the two long-side columns are 2.10 apart and their outer edges 3.50 (labels 2.10 and 3.50); the inner edges of the two short-side rows are 3.10 apart and their outer edges 4.50. These agree with the 0.70 pad length (1.40 = 2 x 0.70 on both).
- Pins 5 and 12 do not exist ("NOTE: PINS 5 AND 12 ARE REMOVED", p.2): the long sides have slots at -1.25, -0.75, -0.25, 0.25, 0.75 and 1.25 with the slot at 0.75 empty, pins 1 to 4 and 6 on the left, 16 to 13 and 11 on the right; the short sides have four pads at -0.75 to 0.75 (7 to 10 below, 20 to 17 above).
- Two exposed pads, both SW ("EXPOSED PAD (PINS 21, 22) ARE SW", p.2), each 0.220 wide; 0.400 long for one and 0.356 long for the other (the bottom view's labels, which sit on the two pads' own extension lines), their centres 0.770 apart (0.770 BSC), on the vertical centre line.
- The place of the exposed pads along the long axis, read two ways that agree: the bottom view gives 2.127 ±0.10 from the body's top edge to the top edge of the 0.356 pad, so its centre is 0.305 below the body centre; the land pattern view gives 0.055 BSC between the centre line of the pair and the centre of the nearest lead pad, whose centre is 0.25 from the body centre, so 0.305 again. The other pad's centre is 0.770 further, 1.075.

**Derived by arithmetic from the above:** the pad centres (a long-side pad's centre is 1.40 from the body centre, 1.75 - 0.35; a short-side pad's 1.90, 2.25 - 0.35).

**Read from the pictures, not from a label:** which exposed pad is 21 and which is 22. On p.2 pad 22 is above pad 21 in the top view; the bottom view has the same vertical order (it is mirrored left to right, as pin 1 moves from the left to the right), and in it the shorter, 0.356 pad is the upper one. Both are SW, so a swap would not change a net, but the numbers follow the drawing. The land pattern view of p.24 is drawn turned a quarter; its two short exposed pads agree (0.400 and 0.356, 0.770 apart, towards the end where pins 5 and 12 are missing).

**Assumed (the datasheet states none of them):**

- Pad shape: rectangle with square corners, as the drawing shows (not the rounded corners of ADR 0043's generated pads).
- Paste: every pad has a paste opening the size of the pad. The datasheet has no stencil recommendation (the text has no "stencil"), and the exposed pads are small (0.088 and 0.078 mm2, against the leads' 0.175 mm2), so a reduced opening for them is not argued; a board house's own stencil rules apply. Mask: every pad has a mask opening; its expansion is the board process's, not this file's. The drawing says "apply solder mask to the areas that are not soldered", which is the same.
- Courtyard: the pads (3.50 by 4.50) and 0.25 all round, as ADR 0043's generated footprints. The body outline on `F.Fab` is the 3.00 x 4.00 body with pin 1's corner cut; the silkscreen corners keep 0.26 from copper centre line to pad edge, and a triangle points at pin 1, all as the generators draw them. These are drawing conventions, not package dimensions.
- No 3D model (none was read, and none is named: the footprint has no `(model ...)`), and no thermal vias (the datasheet's "Recommended PCB Layout", Figure 1 on p.13, is a layout and not a land pattern; vias and copper pours are the board's).
- Its pick-and-place rotation is unverified until a board with it has been assembled (`crates/netsmith-fab/src/rotation.rs` leaves it out, and `fab-notes.txt` flags it).

## Cost and limits

- A wrong pad is a wrong board. Unlike ADR 0043's generators this footprint cannot be compared with KiCad's, because KiCad has none. Its check is the datasheet itself: the unit test holds each number, and a reviewer can read p.24 against the file (`std/libs/netsmith.pretty/*.kicad_mod` is plain text). Before a board is ordered, compare the footprint with the maker's drawing once more, and look at the `fab-notes.txt` line about the rotation.
- The licence is not an issue the way ADR 0041 found it for KiCad's files: the pad numbers and sizes are facts from a manufacturer's drawing, and the files are written by netsmith (MIT), not copied from a library. The symbol's drawing is original (a rectangle with the pins of the pin table).
- Each new footprint needs a package drawing that was read, with the page, and a note of what was measured and what was assumed, as above. A package with a dimension that cannot be read is not added.
- The catalog row for `LT8641IUDC#WPBF` is not part of this: LCSC lists stock 0 and JLCPCB shows none (`data/jlcpcb/README.md`), so the standard `lt8641` catalog stays without a row, and a board states its own.

## Choices for the owner

These were taken conservatively; each is cheap to change now and gets dearer once other footprints depend on it.

1. **Symbols as well as footprints.** The owner's decision named footprints. A row of `std/kicad.toml` needs a symbol too, KiCad has none for the LT8641, and a board cannot be exported without one. Taken: the library holds the symbol (`netsmith:LT8641`) in the same way, from the part's own pin table. Alternatives: (a) make `symbol` optional in a row and have the exporters draw a generated box, as the editor does where a symbol is missing (a change to the mapping's meaning, and the schematic export would have to draw one: larger); (b) leave the symbol to each project (the part stays unbuildable without a project file: the situation that this ADR is for).
2. **The form of the library.** Taken: plain `.kicad_mod` and `.kicad_sym` files in `std/libs/`, embedded, so a person can open them in KiCad's editors and the file is the source (ADR 0041: one library format). Alternatives: (a) a generator row in `crates/netsmith-pcb/src/generate/` with the dimensions as a table (ADR 0043's way; compact, but a single drawing's land pattern is not a rule, and the file would still have to be written as code); (b) fetched at run time from a pinned URL (ADR 0041 option B; needs a network and a place to host it).
3. **Naming and syntax.** Taken: the library name `netsmith`, ids `netsmith:Name` in the existing `footprint` and `symbol` fields (one correct way: no new key), reserved so that no project library shares it. Alternatives: a separate `[[std_footprint]]` table or a `footprint_file = "..."` key (a second spelling of the same thing).
4. **Precedence.** Taken: `netsmith:` is answered first and always from the binary, so no environment, folder or project changes a pad. Alternative: let a project's library of that name override it (ADR 0021 and ADR 0043 refuse that for installed and generated libraries; the same rule is kept).
5. **What the export writes.** Taken: the library's files are written into the board's output folder and listed in its tables when a part of the board uses it, and not otherwise. Alternatives: always write them (simpler, but every board of every project would carry a folder it does not use); never write them, leaving the footprint embedded in the `.kicad_pcb` and no table entry (KiCad then has no library to update the footprint from, which the project libraries of ADR 0021 do offer).
6. **A footprint that is checked only against its datasheet.** Taken: the unit test and the pin-table test above, and the statement in this ADR of what was read and what was assumed. Alternative: a second reader of the drawing before the footprint is used for a real board (a person's check, which no test can replace).
7. **The name `UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA`** follows KiCad's style (package, leads, exposed pads, body, pitch, variation) and names the package, not the part, so another UDC20(18) part can map to it. A package named for a maker's drawing number is the alternative.

## Left open

- The LT8641's catalog row (issue #179) waits for a stock above zero.
- No row of std maps another part to the footprint yet; the LT3033, whose package KiCad does have, needs nothing.
- Whether packages that are common across makers but not a rule, such as the other UDC pad variations of the same family, belong here one by one, or whether ADR 0043's QFN rows (which take their numbers from datasheets as well) should grow to take drawings with two exposed pads; the first is what this ADR does.
