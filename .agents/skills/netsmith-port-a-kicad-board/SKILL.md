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
4. Write the survey in the project (for example `docs/PORT.md`) before writing any `.nsm`.

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

Write the parts file, `reference/parts.toml`: a table `[parts]` with the original's designator of each part by its member path (`"usb_uart.receptacle" = "J1"`), one line for each part of the original that the design has (a mounting hole of the physical file too, when the original's has a designator). The port's tests read it to prove the nets with the shared harness (`crates/netsmith/tests/common/port.rs`, in this repository): give the harness the port's differences as data (two-pad parts whose pads read the other way round, a pad the original numbers otherwise, parts with no nets) and keep only what is the port's own in its test file.

Build the board with `netsmith fab` once every sheet is in. Compare the part count and the BOM with the original's. Then compare the generated `.kicad_pcb` with the original board or schematic: `netsmith compare original.kicad_pcb fab/<Board>/<Board>.kicad_pcb`. To compare where the parts are, put the original's facts in `reference/placements.toml` (in this repository, `cargo run -p netsmith --example reference_table -- <original>.kicad_pcb > reference/placements.toml`) and run `netsmith parity fab/<Board>/<Board>.kicad_pcb`: it pairs the parts by the parts file and the member path `fab` writes on each footprint (`netsmith_path`), so it needs no map of designators.
