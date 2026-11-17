# PCB view: the Gerber files

The files of a board's fab package drawn as the board house gets them (issue #1107): `Gerber` in the PCB toolbar's `2D | 3D | Gerber` switch (`[data-testid=pcb-gerber]`). The server reads the package with the reader of `netsmith view gerber` (`netsmith_fab::viewer`) and answers its summary and the SVG of one view (`GET /api/gerber?board=&layer=`, a `GerberView`), so the drawing is the command's `--format svg`, byte for byte.

## How to get to it

The section "PCB", then `Gerber`. The package is the one `fab` last wrote for the board: in this session, else the `<board>-gerbers.zip` an earlier run left in the fab folder. With none, the view says so (`[data-testid=gerber-none]`) and offers Make fab files and Open package…. Open package… (`[data-testid=gerber-file]`, a hidden file input) sends a `.zip` or one Gerber or drill file (`POST /api/gerber?name=`); the server keeps it and draws its other views (`GET /api/gerber?opened=`); "This board's package" goes back.

## Driving it

`scripts/editor-verify launch` (`two_boards`), then `drive pcb-gerber`.

1. The bar: the views of the package, in the reader's words and order (`[data-testid=gerber-view-<word>]`: Top, Inner n, Bottom, All; a word the reader learns is shown as it is), Ruler, Open package…, Copy command (`netsmith view gerber <path> --layer <view>`).
2. The drawing (`[data-testid=gerber-drawing]`, `data-layer` the view, `data-mirrored` on the bottom) is the reader's SVG in a pan and zoom canvas: each file a `g.layer` with `data-file`, in KiCad's colours on the reader's dark background whatever the theme. The bottom is mirrored left to right.
3. The side panel lists every file (`[data-testid=gerber-layer]`, `data-file`, `data-function`, `data-layer`) with its colour, a checkbox that hides its group, and its name; a file the view does not draw is greyed, one not read is struck through. Then the holes by size, the files the reader did not understand (`[data-testid=gerber-unread]`: file, line, message, not drawn; the bar's chip `[data-testid=gerber-unread-chip]` counts them) and the files that are neither Gerber nor drill.
4. Ruler on: drag from one point to another; the bar's reading (`[data-testid=gerber-ruler]`, `data-mm` the distance) is in mm in the frame of the layout and routes files (the bottom's x turned back). Off, the bar shows where the pointer is.

## Gotchas

- A view the package has no file for answers 400; the page falls back to the first view. An opened package starts at its first view.
- The 404 of a board with no package is logged by the browser: a recipe allows it (`allowConsole(/404/)`).
- The drawing's colours are the reader's, not the theme's: they are in the SVG, not in `themes.css`.
