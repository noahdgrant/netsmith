# Schematic view and click-through

## How to get to it

The right pane, tab "Schematic" (the first). The board shown is the one in the selector at the top. On the right of the drawing is the inspector with the selection and the parts list.

## Driving it

`scripts/editor-verify drive schematic`

1. Select `TwoBoards/led`. The drawing has `data-board="LedBoard"`.
2. Click the resistor `R1` in the drawing: the editor jumps to `.r1: Resistor {}` and the inspector shows its kind, value, package and pins.
3. Click a line in the source that declares the LED: `D1` is highlighted in the drawing (class `mark`). A line that declares a module (`.stage: LdoStage`) marks every part inside it.
4. Click `D1` in the parts list: the inspector shows `Led`.

### A big board: `drive big-schematic` after `launch --fixture big`

A board with more than 16 parts is drawn by module (`crates/netsmith-view/src/sheets.rs`): each module instance is a dashed box titled `instance: Module`, laid out on its own, and the boxes are set out in a grid about 1.6 times wider than tall. The parts that are in no module are a box named for the board. A box with more than 20 parts is cut into `name (1/2)` and `name (2/2)` along the connections. A net that joins parts in two boxes is a short stub with its name at each pin (ground is drawn in every box).

1. The drawing has a `.module-sheet` for each of the 24 instances and one for the rest, in a shape that is not a strip.
2. `VIN` is a named stub in each regulator stage, never a wire across the page.
3. The "Modules" list in the inspector names each box; a click zooms to it (the zoom bar's percentage goes up). "Fit" shows the whole board again. A click on a box's frame, not on a part or wire in it, zooms to the box too.
4. Wheel zooms, a drag pans.

## Gotchas

- Parts have no fill, so a click in the middle of a symbol would fall through to the sheet; the stylesheet gives `.part` `pointer-events: bounding-box`.
- Dragging pans; a click that ends a drag is swallowed. Wheel zooms around the pointer; double-click on empty space fits.
- When the source has a mistake the last drawing that built is shown dimmed under a banner.
- Moving symbols is not built (ADR 0019 designs the sidecar file).
