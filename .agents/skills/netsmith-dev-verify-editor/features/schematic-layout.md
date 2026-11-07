# Arranging the schematic by hand (the layout file, stage 4)

## How to get to it

The section "Schematic", a board with a module or a few parts (`TwoBoards/led` is three parts on one sheet). Press on a part's body and drag; the arrow keys, `r`, `m` and Backspace act on the selected part; the palette has the clear commands (`Ctrl+Shift+P`, then `>clear`). What is written is `<stem>.sch.toml` beside the top-level file (`src/main.sch.toml`), through `PUT /api/schematic-hints` (ADR 0061, "Built in stage 4").

## Driving it

`scripts/editor-verify drive layout-drag layout-keys layout-clear` (fixture `two_boards`; the recipes share `scripts/editor-verify-js/layout-helpers.mjs` and find parts by their path, never by designator).

`layout-drag`:

1. The drawing says which part is the sheet's anchor (`g.part[data-anchor]`, one for the sheet; its `<title>` says "The sheet's anchor: drag a neighbour."); the others have `.movable`, `data-origin` and `data-orient`.
2. Press on a part's body and move: a ghost (`.drag-ghost`) stays where it was, the part carries `translate(Xpx, Ypx)` with X and Y whole grid steps (1.27 mm), a readout (`[data-testid=drag-readout]`) says `R1  dx +6.35  dy +3.81 mm`; no file is written while the button is down.
3. Esc: ghost, lift and readout go, no file, the part is where it was, nothing is picked.
4. A press that does not move picks the part (the chip names it) and writes nothing.
5. The drop writes `[LedBoard.parts.<key>]` with `dx` and `dy` on the grid; the page dims, shows "Redrawing the schematic" (`[data-testid=redrawing]`) and redraws; the part wears the dashed outline (`.part-hint.on`, the third previous sibling of the part), is the pick, and the zoom reads as before.
6. Alt held: the readout says `(pin)` and the file has `pin = [x, y]`, no `dx`, and the first entry is kept.
7. A press on the anchor pans as before; no ghost, no write.
8. A drop on the anchor's body is refused by the drawing: the toast says "cannot put it there", the engine's reason and "back where it was"; the file is as it was before the drop and the block is where it was.
9. A file changed by another program between the press and the release: the toast says the file changed, the other program's file stays (a `409`; the recipe allows that console line).

`layout-keys`: with nothing selected, or the anchor, the arrows pan (the transform changes, nothing is written) and the palette has no "Nudge"; with a part selected the palette and the cheat sheet list the nudge, rotate and mirror commands and no "Pan left"; `ArrowRight` writes `dx = 1.27`, `Shift+ArrowDown` `dy = 12.7`; two presses in a row are both made (the second waits); `r` writes `rotate` a quarter from the orientation drawn (`data-orient`), `Shift+R` goes back, `m` flips `mirror`; Backspace clears the entry and leaves `version = 1`.

`layout-clear`: with no entry neither clear command is in the palette; entries make the outline (`--sch-muted`, dashed, read from the computed style against the token); the file is in the tree ("Schematic layout" section, `main.sch.toml`) and opens in Source with TOML colouring; "Clear the layout of this module" asks (`[data-testid=confirm]`, the count of entries and the file's name), Cancel keeps the file, the yes removes this board's entries and keeps another board's; "Clear every layout entry of the file" leaves `version = 1`; a file with a fault gives a warning in the dock whose click opens `main.sch.toml` in Source on the line of the fault; an edit by another program (and the file's deletion) redraws and moves the marks.

## Gotchas

- The browser logs a request that answers 4xx as a console error, which fails a recipe: a recipe that makes the page get a `409` says so (`allowConsole(/409/)`).
- A write analyses the project: every recipe waits for `[data-testid=redrawing]` to go before it measures. The page does not poll; a file written by hand is read by the server's watcher within half a second.
- The drag's preview moves the part and not its satellites or wires: they jump when the file is written (ADR 0061, "Left open").
- Alt+drag is taken by some Linux window managers; the recipes drive Chromium, which gets the keys as they are.
- The pointer's press on a part stops the canvas's pan (`stopPropagation` at the part); the anchor and a file with a fault do not, so those presses pan.
