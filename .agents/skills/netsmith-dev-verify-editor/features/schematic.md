# Schematic view and click-through

## How to get to it

The section "Schematic" (the second, key 2; Source is the first). The board shown is the one in the selector at the top. A strip of sheets is above the drawing: Previous and Next, a tab for "All sheets" and one for each sheet (a module instance, or the board's own circuit), and "Details", which shows or hides the inspector (the selection, the Modules list, the parts list) on the right; it is open by default only when the view is 1500 px wide or more.

## Driving it

`scripts/editor-verify drive schematic`

1. Select `TwoBoards/led`. The drawing has `data-board="LedBoard"`.
2. Click the resistor `R1` in the drawing: the Source section opens at `.r1: Resistor {}` and a button "Back to Schematic" appears in its file tabs. Escape (Alt+Left outside the text, the button, or the browser's Back button) returns to the schematic as it was left.
3. Open Source, click a line that declares the LED, go back to the schematic: `D1` is highlighted in the drawing (class `mark`). A line that declares a module (`.stage: LdoStage`) marks every part inside it.
4. Click `D1` in the parts list: the source opens at its line too; Back, and the inspector shows `Led`.

### A big board: `drive big-schematic` after `launch --fixture big`

One sheet is shown at a time, large: the tab strip, Previous/Next, the Left and Right arrow keys and `[` and `]` choose it, so does a click on its entry in the inspector's Modules list; what is outside its frame is clipped away. A board with more than one sheet opens on the first (the order of the Modules list); the pick is kept for each board in localStorage. "All sheets" is the overview below; a click on a box's frame there opens that sheet. Each sheet keeps its zoom and place for the session (leaving and coming back, even through the Source section, finds it as it was); `F` (or 0, or the Fit button) fits it again, `+` and `-` zoom (the drawing must have the focus: click it).

A board with more than 16 parts is drawn by module (`crates/netsmith-view/src/sheets.rs`): each module instance is a dashed box titled `instance: Module`, laid out on its own, and the boxes are set out in a grid about 1.6 times wider than tall. The parts that are in no module are a box named for the board. A box with more than 20 parts is cut into `name (1/2)` and `name (2/2)` along the connections. Rails are drawn with power symbols (below), not wired across the box. Any other net is wired between its pins that are in a box (up to 8 of them; a pin that leaves away from the net's trunk has its wire go round its column), and one that also goes to another box has its name on the wire. A net with more pins than that, or one whose pins are far apart, is a short stub and a name at each pin: pins next to each other on one net share a bar and one name, a name that would be over another is turned to read upward, and a name over 24 letters is cut with an ellipsis (the tooltip has all of it).

### Power symbols

A net that the checked model says is the return of a rail (the `neg` side) is drawn with the ground symbol (bars that shrink), a net a rail draws from with the supply symbol (a stem and an arrow head, the name beside it), by the model and not by name (`crates/netsmith/src/editor/analysis.rs`, `rails_of`). Every pin on a rail has a wire to a symbol; pins next to each other on one rail share a bus and one symbol. Grounds carry their names only when the board has several. Hovering a symbol lights the whole net; its tooltip names it. The KiCad export does not use them. The test `crates/netsmith-view/tests/wires.rs` has the invented bench board (`cargo run -p netsmith-view --example bench_svg -- <folder>` writes its SVG).

### A symbol of several units, or of many pins

A part whose symbol has several units is one box with every pin on the side the symbol puts it; a symbol of more than 60 pads is one box with two columns of pins in labelled groups (Power, Ground, then by the start of the pin names, the unconnected last) and says "N pins". Pin names are as big as net names there. The KiCad export keeps the units.

1. The drawing has a `.module-sheet` for each of the 24 instances and one for the rest, in a shape that is not a strip.
2. `VIN` is a supply symbol with its name in each regulator stage, never a wire across the page.
3. The "Modules" list in the inspector names each box; a click opens its sheet. The keys step through the sheets. A click on a box's frame in "All sheets", not on a part or wire in it, opens the sheet.
4. Wheel zooms, a drag pans; a sheet keeps its zoom, F fits it.

## Gotchas

- Parts have no fill, so a click in the middle of a symbol would fall through to the sheet; the stylesheet gives `.part` `pointer-events: bounding-box`.
- Dragging pans; a click that ends a drag is swallowed. Wheel zooms around the pointer; double-click on empty space fits.
- When the source has a mistake the last drawing that built is shown dimmed under a banner.
- Moving symbols is not built (ADR 0019 designs the sidecar file).
