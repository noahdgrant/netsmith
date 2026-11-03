# Build panel

## How to get to it

Right pane, tab "Build", or the "Build" and "Fab" buttons at the top (they open the tab and run).

## Driving it

`scripts/editor-verify drive build` (the fab step takes some tens of seconds)

1. "Build schematic" writes a KiCad schematic for every board of the project to its `out` directory and shows the report (parts, nets) and the paths written.
2. "Make fab package" runs `fab` for the file of the selected board: chips `package made` and `design rules check: passed`, download links (Gerber zip, `bom.csv`, `cpl.csv`, `fab-notes.txt`, the `.kicad_pcb`), the summary and the text of `fab-notes.txt` ("read before you order").
3. If `fab` stops, the chip says `stopped` and the output pane shows exactly what the command line would print (a design rules failure, a missing LCSC number, a routing failure).

While it runs the panel shows the stages `netsmith fab` prints to stderr, with a clock, and the status bar shows them in every tab; the editor stays usable. The report says how long it took.

### Orders

`scripts/editor-verify drive order` (needs KiCad; `launch` adds an order `demo` to the scratch project's `netsmith.toml` for the `two_boards` fixture).

1. The "Order" section has a select of the manifest's orders and the boards of the picked one with their quantities. With no orders it says how to add one.
2. "Make order" runs `netsmith fab --order <name>`: stages in the panel and status bar, then the chip `order made`, the folder written, links to `order.txt` and `order-bom.csv`, and the text of `order.txt` (boards, combined bill of materials, notes to read before you order, unverified items).
3. If a board fails (a design rules failure, a missing LCSC number) the chip says `stopped` and the output pane shows what the command line prints. Nothing of the order's summary is written then.

## Gotchas

- Orders are read from `netsmith.toml` when the editor starts: after editing an order, restart it.
- Both buttons are disabled while the source has errors.
- `fab` makes every board of the file, so the other boards of a system get packages too.
- The panel needs no KiCad: the design rules check is netsmith's. After a `fab` that the check stopped, a checkbox "allow violations" appears; ticking it and running again packages anyway and the chip says `FAILED`.
- The scratch project's `out/` and `fab/` are removed by `cleanup`.
