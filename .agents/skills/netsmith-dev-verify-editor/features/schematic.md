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

A board with more than 16 parts is drawn by module (`crates/netsmith-view/src/sheets.rs`): each module instance is a dashed box titled `instance: Module`, laid out on its own, and the boxes are set out in a grid about 1.6 times wider than tall. The parts that are in no module are a box named for the board. A box with more than 20 parts is cut into `name (1/2)` and `name (2/2)` along the connections. Every net on a sheet is wired, whatever its size, with orthogonal wires on a grid that go round parts and texts and cross each other only at right angles (`crates/netsmith-view/src/layout/grid.rs`). A name is used only where a wire cannot do: a net that goes on to another sheet leaves through a flag shaped like KiCad's hierarchical label, wired from its pins, with the net's name in it; a rail is a power symbol (below). A net with a generated name (`net-<module>.<part>.<pin>`) is always wires, never a name or a rail. A module is one sheet (up to 60 parts, `MAX_SHEET_PARTS` in `sheets.rs`; the old drawing cut a module of more than 20 in two because it put every column in one row).

The placement is what a person draws (`crates/netsmith-view/src/layout.rs`, the design goal at its top: a schematic is for a human to read): the main parts (an IC, a connector) in a row left to right along the signal; a two-terminal part that serves one pin (a decoupling capacitor, a pull-up, a feedback divider, a bootstrap capacitor, an inductor, a crystal) next to that pin, on its side; parts in series (a divider) in one stack with what is on the node wired out sideways; supplies above, grounds below. A wire never crosses a text, and texts have a halo. A pin with nothing on it has a muted X (tooltip: `not connected: <part> <pin> (<kind>)`); a pin the checker says must be connected and is not has a red ring and X and the checker's message (a board with one does not build, so this shows only in a part's own pin table under construction).

### Rails and flags (round 2 of the review)

A rail is **one straight wire across its group of parts**, at the bottom (a ground) or the top (a supply) beyond every part, with **one symbol**; every pin or run of pins on the rail hangs from it by a straight wire (pins next to each other on one side share a bus first). The symbol is directly above (a supply) or below (a ground) the terminal of the outermost satellite on the rail (a capacitor), or at the part's own centre if there is one part, else at the left end. The capacitors that decouple a supply pin stand in one level row, an even pitch apart, the same distance from the part, hanging from the supply rail. Flags are in two columns at the edges of the sheet, one per net, each on its own row (one pin pitch apart), joined to the net by wires; a flag is 1.3 mm high so a grid step is left between two. Nothing overlaps: power symbols (with their names), flags, X markers and the texts of parts keep a grid step from each other and 0.6 mm from the wires of other nets (`no_two_items_are_closer_than_a_grid_step`, and the recipe measures it in the page).

### Power symbols

A net that the checked model says is the return of a rail (the `neg` side) is drawn with the ground symbol (bars that shrink), a net a rail draws from with the supply symbol (a stem and an arrow head, the name beside it), by the model and not by name (`crates/netsmith/src/editor/analysis.rs`, `rails_of`). A symbol is only at the end of a wire, never on one, directly above (a supply) or below (a ground) the pins it serves, which share a bus; pins of one rail near each other share one symbol. Grounds carry their names only when the board has several. Hovering a symbol lights the whole net; its tooltip names it. The KiCad export does not use them.

### Proof in a browser

`scripts/editor-verify launch --fixture schematic_bench` (the invented project `examples/schematic_bench`: four sheets, `adc`, `mcu`, `buck`, `modem`) then `drive schematic-rules` measures the page's own SVG with the browser's text boxes and asserts each rule above (wires not names, parts beside their pins, a symbol only at the end of a wire, no wire over any text, an X on every open pin, one box for a three-unit symbol, a table for 97 pins, 22 parts on one sheet). `SHOT_LABEL=after scripts/editor-verify drive schematic-shots` takes each sheet at 1366 x 768 and 2000 x 1000. Look at the screenshots.

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
