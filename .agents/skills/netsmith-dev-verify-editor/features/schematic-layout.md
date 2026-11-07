# Arranging the schematic by hand (the layout file, stage 4)

## How to get to it

The section "Schematic". Press on a part's body and drag; the arrow keys, `r`, `m` and Backspace act on the selected part; the palette has the clear commands (`Ctrl+Shift+P`, then `>clear`). What is written is `<stem>.sch.toml` beside the top-level file (`src/main.sch.toml`), through `PUT /api/schematic-hints` (ADR 0061, "Built in stage 4"). The file is in the tree (the Source section) under "Schematic layout" once it exists.

Which part can move: not the **reference part** of its sheet (the drawing marks it, `data-reference`), and not a part whose block holds it. On `TwoBoards/power` the module `stage` has a regulator (its reference part) and two capacitors that move; the board's own two headers are a sheet of their own, with their own reference part. On `TwoBoards/led` nothing can be arranged: the three parts hang from one another, so the block of each holds the reference part, and a drop is put back with "nothing was arranged".

## Driving it

`scripts/editor-verify drive layout-drag layout-keys` (fixture `two_boards`) and `drive layout-clear` (fixture `schematic_bench`: seven modules, so a module is on show). The recipes share `scripts/editor-verify-js/layout-helpers.mjs` and find parts by their path, never by designator.

`layout-drag`:

1. The drawing marks one reference part for the module and one for the board's own parts (`g.part[data-reference]`); the regulator's `<title>` says "The sheet's reference part: drag a neighbour."; every other part has `data-movable`, `data-origin` and `data-orient`.
2. Press on a part's body and move: a ghost (`.drag-ghost`) stays where it was, the part carries `translate(Xpx, Ypx)` with X and Y whole grid steps (1.27 mm), a readout (`[data-testid=drag-readout]`) says `C1  dx +2.54  dy +6.35 mm`; no file is written while the button is down.
3. Esc: ghost, lift and readout go, no file, the part is where it was.
4. A press that does not move picks the part (the chip names it) and writes nothing.
5. The drop writes `[PowerBoard.sheets.stage.parts.input_cap]` with `dx` and `dy` on the grid; the page dims, shows "Redrawing the schematic" (`[data-testid=redrawing]`) and redraws; the part wears the dashed outline (`.part-hint.hinted`, the third previous sibling of the part), is the pick, the zoom reads as before and the reference part is where it was on the screen.
6. Alt held: the readout says `(pin)` and the file has `pin = [x, y]`, no `dx`, and the first entry is kept.
7. A press on the reference part pans as before; no ghost, no write.
8. A drop of the capacitor on the regulator's body is refused by the drawing: the toast says "cannot put it there" and the engine's reason ("the part would lie on the part `stage.regulator` ..."), "back where it was"; the file is as it was before the drop and the block is where it was.
9. A file changed by another program between the press and the release: the toast says the file changed, the other program's file stays (a `409`; the recipe allows that console line).
10. On the LED board, a drop of the connector: the toast says "nothing was arranged: J1 holds the sheet's reference part", and the file the drop made is deleted again (there was none).

`layout-keys`: with nothing selected, or the reference part selected, the arrows pan (the transform changes, nothing is written) and the palette has no "Nudge"; with a part selected the palette lists the nudge, rotate and mirror commands and no "Pan left", and the cheat sheet has them in the group "Schematic layout"; `ArrowRight` writes `dx = 1.27`, `Shift+ArrowDown` `dy = 12.7`; two presses in a row are both made (the second waits); `r` writes `rotate` a quarter from the orientation drawn (`data-orient`), `Shift+R` goes back, `m` flips `mirror`; Backspace clears the entry and leaves `version = 1`.

`layout-clear`: with no entry neither clear command is in the palette; entries make the outline (`--sch-muted`, dashed, read from the computed style against the token); the file is in the tree ("Schematic layout" section) and opens in Source with TOML colouring; "Clear the layout of this module" asks (`[data-testid=confirm]`, the count of entries and the file's name), Cancel keeps the file, the yes removes this module's entries and keeps the other modules' and boards'; "Clear every layout entry of the file" leaves `version = 1`; a file with a fault gives a warning in the dock whose click opens the file in Source on the line of the fault; an edit by another program (and the file's deletion) redraws and moves the marks.

## Gotchas

- The browser logs a request that answers 4xx as a console error, which fails a recipe: a recipe that makes the page get a `409` says so (`allowConsole(/409/)`).
- A write analyses the project: a recipe waits for the part to be somewhere else on the screen, then for `[data-testid=redrawing]` to go, before it measures (the chip is gone as soon as the answer is, a moment before the drawing is fetched). The page does not poll; a file written by hand is read by the server's watcher within half a second.
- A part's class is its state (picked, lit, marked): the hand-arranged mark is a rectangle (`.part-hint`, class `hinted`, not `on`: the paint of picks clears every `.on`) and "can be dragged" is the attribute `data-movable`.
- The drag's preview moves the part and not its satellites or wires: they jump when the file is written (ADR 0061, "Left open").
- Alt+drag is taken by some Linux window managers; the recipes drive Chromium, which gets the keys as they are.
- The pointer's press on a part stops the canvas's pan (`stopPropagation` at the part); the reference part and a file with a fault do not, so those presses pan.
- The toasts of earlier steps are still on screen: a recipe looks for the toast by its words.
