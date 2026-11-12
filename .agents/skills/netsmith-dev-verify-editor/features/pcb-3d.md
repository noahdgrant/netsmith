# PCB view in 3D

The board's _Scene_ (ADR 0105) drawn by three.js in the PCB section: the same file `netsmith board3d --glb` writes, served at `GET /api/pcb/3d?board=<name>`. Read-only. What it is and is not, for a person: `docs/BOARD3D.md`, "The editor's 3D view".

## How to get to it

The section "PCB", then `3D` in the toolbar's `2D | 3D` switch (`[data-testid=pcb-3d-toggle]`), the key `D` while the PCB has the keys, or "PCB: Switch between 2D and 3D" in the palette. The first time, the page fetches the 3D chunk (`assets/View3d-*.js`, three.js and the view, about 172 kB gzipped); the 2D view never loads it. The view's host is `[data-testid=pcb-3d]`; it gets `data-ready` once the first frame is drawn (headless Chromium compiles the shaders in 2 to 3 s) and `data-stamp`, the layout stamp of the scene it shows.

## Driving it

`scripts/editor-verify launch --fixture height_limits`, then `drive pcb-3d`. The fixture has a part of each kind: `tall` (9 mm stated, over the lid's 3 mm limit), `flat` (1.5 mm stated, fits) and `bare` (no height).

1. It opens on the isometric view: the board as thick as its stackup, the laminate's edge, the mask, the bodies of known height as solids, the part with no height as a flat hatched outline with a dashed edge, and the lid's volume see-through in the error colour (it is broken). The coverage chip says `2 of 3 parts have a known height`; the legend counts `Stated 2`, `Catalog 0`, `Unknown 1` and shows `Limit exceeded` (or `Height limit` for one that holds).
2. Hover a part: the tooltip (`[data-testid=tip-3d]`) names the designator and value, the height and where it came from (`9 mm tall, stated in the design`, `from the catalog row`, or `Height not known: drawn flat and hatched, not guessed`), the face, and the 3D findings about it (`Too tall for the limit lid`). The shared hover lights the part's row in the side panel and in the other views; a hover elsewhere lights the part here. Hover a limit's volume: what it allows and its margin.
3. Click a part: it is the page's selection (the inspector names it; the 2D view, the schematic and the tree show it), drawn in the selection's blue. Shift+click adds. A click on nothing clears. A pick made in the side panel, the schematic or the problems dock lights here; go to anything frames the part. Double click: the source opens at the part's declaration.
4. The bar at the top left: `Top`, `Bottom`, `Front`, `Back`, `Left`, `Right`, `Iso` (`[data-testid=view-<id>]`; keys `T`, `B`, `I`), the flip (`Shift+F`, the other face), `Perspective`/`Orthographic` (`P`). The zoom bar: `−`, `+`, `Fit` (keys `-`, `+`, `F` or `0`). Drag orbits, right or middle drag pans, the wheel zooms at the pointer, the arrow keys orbit (Shift+arrows pan), `Esc` clears the selection. Camera moves take 180 ms, and none with reduced motion.
5. The coverage chip opens the list of the parts with no height (`[data-testid=unknown-heights]`); a click on a row selects the part and frames it.
6. A theme change repaints the view in the theme's `--pcb3d-*` tokens (themes.css; dark, light and both high contrast).
7. `2D` brings the editing canvas back; the 3D view keeps nothing that the 2D view needs.

For a recipe the host carries a handle: `document.querySelector('[data-testid=pcb-3d]').view3d` with `project(path)` (a part's place in the window, to click it without guessing), `sample()` (the share of the canvas that is not the background, and how many colours), `colourAt(x, y)`, `parts()`, `preset()` and `ortho()`.

## Gotchas

- The digits are the sections' keys: the presets use letters.
- The first frame is slow in headless Chromium (SwiftShader): wait on `data-ready`, not a time.
- Pixels are read in the same task as a fresh draw (`sample`, `colourAt`): the canvas keeps no buffer, so a screenshot is the way to see it (`canvas.toDataURL` may come back blank).
- A recipe never hard-codes a designator: take the part from the 2D scene (`/api/pcb/scene`) by its path and ask `project(path)` where it is.
- The view draws only the file's nodes: copper and package shapes arrive in the same file (#430) and are drawn by their material's name; a node of a kind the view does not know is drawn and not picked.
