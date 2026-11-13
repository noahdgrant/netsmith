---
name: netsmith-port-a-kicad-board
description: Port an existing KiCad design into a netsmith project, one sheet at a time, proving each sheet's nets with kicad-cli. Use when asked to move or re-create a KiCad schematic or board in .nsm.
---

# Port a KiCad board

Entry point: the project's `AGENTS.md`. The language guide (`docs/netsmith/LANGUAGE.md`) covers the syntax. Work on a copy. Never edit the original design.

## Survey first

1. List the sheets, and for each: part count, distinct part kinds, nets, connectors.
2. Note each sheet's nets and connectors from its schematic or from `kicad-cli sch export netlist --format kicadxml -o sheet.xml sheet.kicad_sch`.
3. Sort parts into three groups: kinds the netsmith standard library has (resistor, capacitor, LED, linear regulator, header), kinds it lacks, and values the catalog lacks.
   For an IC the library lacks, draft its part file from the original's own symbol: `netsmith part from-symbol original.kicad_sch <symbol>` (or a `.kicad_sym`) prints the pin table with names, pads and the kinds the symbol's types give, and `TODO_KIND` where they do not. It is a draft: check every row against the datasheet's pin table, with the page.
4. Write the survey in the project (for example `docs/PORT.md`) before writing any `.nsm`.

## Start from the skeleton

Do not write a script that reads the board. Run `netsmith port skeleton <original.kicad_pcb> [--out src/main.nsm]` in the project: it reads the `.kicad_pcb` itself (no KiCad) and writes the first source and `reference/parts.toml`, and overwrites neither when it is there.

- One member for each footprint, named by its designator in lower case (`.c5`), after a comment with the designator, the value and the footprint. A resistor, capacitor, inductor or LED with KiCad's own footprint name (`R_0402_1005Metric`) and a plain value is the standard part, with the skeleton's tolerance (1 %, 10 %, 20 %) on a line marked `TODO tolerance`: the command lists each one; check it against the original's bill of materials.
- Every other footprint is a part named after its value, imported from `src/<name>.nsm`. When that file is not there the member is marked `GAP`, and the source does not build until each has a part file. The joins name a gap's pins by the original board's pin names, made into paths as `netsmith part from-symbol` makes them (`PA14-BOOT0` is `.u1.pa14_boot0`), and a pin with no name by its pad (`.u1.'3'`), so a part file drafted from the original's symbol fits them. Write each part file (see `netsmith-write-nsm`); a part file that is there when the skeleton runs names the pins from its own table.
- One join for each net, after a comment with the original's net name, and `.<member>.<pin>.net &= "<name>"` on one pin of each net the original names itself.

The skeleton is flat: move its parts into modules and its nets into rails as you port each sheet, and keep `reference/parts.toml` in step with each member you rename or move.

## Port the smallest sheet first

1. Pick the sheet with the fewest parts and the most supported kinds.
2. Write it as a module in `src/`, or a board if it stands alone. State what each part must achieve as ranges taken from the original's values. Do not copy part numbers.
3. Run `netsmith build`. Fix errors with `netsmith-read-errors`.
4. Compare the port with the original: `netsmith compare original.kicad_sch generated.kicad_sch` (or the `.nsm` file itself, or the exported netlist XML). It compares by connectivity, not by net name or reference designator, which both change, and lists each difference by net and pin with each side's designators. Exit code 0 is a match, 1 a difference, 2 a mistake or "could not decide".
   - Pin numbers are identity: a resistor with its pins swapped is reported. Decide whether the original or the port is right and fix the port, or explain it in the gaps file.
   - Use `--map map.txt` (lines `ORIGINAL_REF PORT_REF`) to fix designators you know correspond, `--loose-values` when the original writes `4k7` and the port `4.7k`, and `--ignore-unconnected` when the original has stub nets of one pin.
5. Run `kicad-cli sch erc` on the generated schematic. Fix real violations.
6. The sheet is done when `netsmith compare` says `match`, or every difference it lists is explained in the gaps file.

Then the next smallest sheet. Reuse modules.

## Record gaps

Put each thing you could not port in a gaps file in the project, with the sheet, the part and the reason:

- a part kind the library lacks;
- a value or part the catalog lacks;
- a connector between boards (a `system` joins ports and does not model pinouts);
- a sheet that is hierarchical (modules are not sheets yet);
- anything the original did that the language cannot say.

Report gaps to the user. Do not invent a part, a catalog row or a workaround that changes the circuit.

## Verify

The parts file, `reference/parts.toml`, which the skeleton wrote, is a table `[parts]` with the original's designator of each part by its member path (`"usb_uart.receptacle" = "J1"`), one line for each part of the original that the design has (a mounting hole of the physical file too, when the original's has a designator). The port's tests read it to prove the nets with the shared harness (`crates/netsmith/tests/common/port.rs`, in this repository): give the harness the port's differences as data (two-pad parts whose pads read the other way round, a pad the original numbers otherwise, parts with no nets) and keep only what is the port's own in its test file.

Build the board with `netsmith fab` once every sheet is in. Compare the part count and the BOM with the original's. Then compare the generated `.kicad_pcb` with the original board or schematic: `netsmith compare original.kicad_pcb fab/<Board>/<Board>.kicad_pcb`. To compare where the parts are, put the original's facts in `reference/placements.toml` (in this repository, `cargo run -p netsmith --example reference_table -- <original>.kicad_pcb > reference/placements.toml`) and run `netsmith parity fab/<Board>/<Board>.kicad_pcb`: it pairs the parts by the parts file and the member path `fab` writes on each footprint (`netsmith_path`), so it needs no map of designators.

## Hold a large original as derived tables

Commit the original's `.kicad_pcb` in `reference/` when it is 10 MB or smaller. A larger one (a board of 36 MB or more) is not committed: commit what the tests read from it instead.

1. Fetch the original from its repository at a commit, not a branch, and keep it outside the repository.
2. In the project, run `netsmith port tables <original.kicad_pcb> --url <URL>` with the file's URL at that commit (`https://raw.githubusercontent.com/<owner>/<repo>/<commit>/<path>`). Put `reference/must_match.toml` there first. The command writes `reference/netlist.xml` (the netlist, in KiCad's netlist XML), `reference/placements.toml` (the outline, the holes, the parts and pads) and `reference/source.toml` (the file name, the URL, the size and the sha256).
3. In the port's test, give the harness `board: None`: it reads `reference/netlist.xml`. `netsmith compare` reads the XML, and `netsmith parity` reads `placements.toml`.
4. To prove the tables, fetch the file again and run `netsmith port tables <original.kicad_pcb> --check`. It exits 1 when the file does not match `source.toml`'s hash, or when a table is not what the file gives. After a change to the readers, run it without `--check` and review the tables' diff.

## Place the parts that must match

The physical file's `.place.x` and `.place.y` measure the part's body (its fabrication drawing, else its courtyard, else its pads and 0.25 mm); parity compares the footprint's origin.

1. State the face and the turn (`.place.side`, `.place.rotation`) first: the distance from the origin to the body's centre changes with them.
2. Run `netsmith fab`, then `netsmith parity`. Each part that must match has an `origin` line in the report with the two lines that put its origin on the original's: copy them into the physical file. `fab` prints the same distance for every part (`dx`, `dy`), and `fab --format json` gives it in `parts`.
3. A footprint of the project for a part on the bottom face is drawn as seen from the top, as any library footprint. netsmith mirrors a bottom part's footprint in x, turns it by `.place.rotation` and moves every layer to the other face. The original's board file, and `placements.toml` read from it, store a bottom footprint already flipped as KiCad flips it (mirrored in y, and turned a half more): to draw the footprint from those figures, negate each y and each pad's own angle (its angle less the footprint's), and put each layer on the other face; the part's `.place.rotation` is the file's rotation less 180.
4. A pad difference names the board's value and the original's: `type (the board's smd, the reference's connect)`. KiCad's `connect` pad and an `smd` pad of the same size are two pads to parity: use the original's type.
