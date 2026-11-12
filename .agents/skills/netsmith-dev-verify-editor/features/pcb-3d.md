# PCB view in 3D

The board's _Scene_ (ADR 0105) drawn by three.js in the PCB section: the same file `netsmith board3d --glb` writes, served at `GET /api/pcb/3d?board=<name>`. Read-only. What it is and is not, for a person: `docs/BOARD3D.md`, "The editor's 3D view".

## How to get to it

The section "PCB", then `3D` in the toolbar's `2D | 3D` switch (`[data-testid=pcb-3d-toggle]`), the key `D` while the PCB has the keys, or "PCB: Switch between 2D and 3D" in the palette. The first time, the page fetches the 3D chunk (`assets/View3d-*.js`, three.js and the view, about 172 kB gzipped); the 2D view never loads it. The view's host is `[data-testid=pcb-3d]`; it gets `data-ready` once the first frame is drawn (headless Chromium compiles the shaders in 2 to 3 s) and `data-stamp`, the layout stamp of the scene it shows.

## Driving it

`scripts/editor-verify launch --fixture height_limits`, then `drive pcb-3d`. The fixture has a part of each kind: `tall` (9 mm stated, over the lid's 3 mm limit), `flat` (1.5 mm stated, fits) and `bare` (no height).

1. It opens on the isometric view: the board as thick as its stackup, the laminate's edge, the mask, the bodies of known height as their package shapes (a header's plastic and pins) or as boxes, the body tinted by its height's source and the leads and marks in their own colours, the part with no height as a flat hatched outline with a dashed edge, and the lid's volume see-through in the error colour (it is broken). The coverage chip says `2 of 3 parts have a known height`; the legend counts `Stated 2`, `Catalog 0`, `Unknown 1` and shows `Limit exceeded` (or `Height limit` for one that holds).
2. Hover a part: the tooltip (`[data-testid=tip-3d]`) names the designator and value, the height and where it came from (`9 mm tall, stated in the design`, `from the catalog row`, or `Height not known: drawn flat and hatched, not guessed`), the face, and the 3D findings about it (`Too tall for the limit lid`). The shared hover lights the part's row in the side panel and in the other views; a hover elsewhere lights the part here. Hover a limit's volume: what it allows and its margin.
3. Click a part: it is the page's selection (the inspector names it; the 2D view, the schematic and the tree show it), drawn in the selection's blue. Shift+click adds. A click on nothing clears. A pick made in the side panel, the schematic or the problems dock lights here; go to anything frames the part. Double click: the source opens at the part's declaration.
4. The bar at the top left: `Top`, `Bottom`, `Front`, `Back`, `Left`, `Right`, `Iso` (`[data-testid=view-<id>]`; keys `T`, `B`, `I`), the flip (`Shift+F`, the other face), `Perspective`/`Orthographic` (`P`). The zoom bar: `−`, `+`, `Fit` (keys `-`, `+`, `F` or `0`). Drag orbits, right or middle drag pans, the wheel zooms at the pointer, the arrow keys orbit (Shift+arrows pan), `Esc` clears the selection. Camera moves take 180 ms, and none with reduced motion.
5. The coverage chip opens the list of the parts with no height (`[data-testid=unknown-heights]`); a click on a row selects the part and frames it.
6. A theme change repaints the view in the theme's `--pcb3d-*` tokens (themes.css; dark, light and both high contrast).
7. `2D` brings the editing canvas back; the 3D view keeps nothing that the 2D view needs.

## Tools

The bar under the camera's (`[data-testid=tool-measure]`, `tool-section`, `tool-explode`, the `…` menu `.v3-more`). Recipes: `pcb-3d-measure` (fixture `height_limits`) and `pcb-3d-stackup` (fixture `four_layer`).

1. **Measure**: `M` (the view's own key; the schematic's M mirrors, so the palette's command has no key). The panel `[data-testid=measure-panel]` says what to do. Over a pad the snap word `[data-testid=measure-snap]` (with `data-kind`: `pad`, `corner`, `edge`, `top`, `face`, `surface`) names it, `Pad J3.1`, with its coordinates. Two clicks draw a line and its label `[data-testid=measure-label]`: `5.68 mm` and `dx 2.54 · dy -5.08 · dz 0.00`. It stays when the camera moves and when the tool is left. Shift+click a part: `[data-testid=measure-readout]` says `9.00 mm above the top face` and `6.00 mm over the limit lid (3.00 mm)`, with two upright lines on the part (`measure-height`, `measure-limit`). `Esc` leaves a half-made measurement, then the tool. `Clear all` (`measure-clear`) removes them.
2. **Section**: `S`, or `X`/`Y`/`Z`. The panel `[data-testid=section-panel]` has the axis, the handle (`section-handle`, a range) and the place in mm (`section-at`; Enter sets it). Seen square to the cut (a `Y` cut from the front), the board's thickness shows its bands: mask, prepreg and core in two tones. The plane's outline is dashed in `--pcb3d-cut`.
3. **Layers apart**: `E`. Each layer has a label `[data-testid=stack-label]` with `data-layer`: `In1.Cu copper · 0.015 mm`. `E` again puts them back.
4. **Copy as command**: the `…` menu shows `[data-testid=render-command-text]`, e.g. `netsmith render src/main.nsm --board FourLayerExample --view front --section y=3.87 --theme dark`, with a copy button.

For a recipe the host carries a handle: `document.querySelector('[data-testid=pcb-3d]').view3d` with `project(path)` (a point of the window where a pick finds the part, found by raycasting over its bounds: a package shape has leads and a lower body, not a box; null when it is hidden), `sample()` (the share of the canvas that is not the background, and how many colours), `colourAt(x, y)`, `parts()`, `preset()`, `ortho()`, and for the tools `toScreen([x, y, z])` (a point of the scene's frame of docs/BOARD3D.md in the window), `snap(x, y)`, `slots()` (the stackup's layers and where each is now), `exploded()`, `command()` and `board()` (the root's extras).

## Gotchas

- The digits are the sections' keys: the presets and the tools use letters.
- A pad under a body is not snapped to from above (the body is in front of it): a recipe tries the pads and keeps those whose snap word names them.
- The section's bands are thin: 1.6 mm of board. Zoom with the wheel at a point `toScreen` gives on the cut, in orthographic.
- The first frame is slow in headless Chromium (SwiftShader): wait on `data-ready`, not a time.
- Pixels are read in the same task as a fresh draw (`sample`, `colourAt`): the canvas keeps no buffer, so a screenshot is the way to see it (`canvas.toDataURL` may come back blank).
- A recipe never hard-codes a designator: take the part from the 2D scene (`/api/pcb/scene`) by its path and ask `project(path)` where it is.
- The view draws only the file's nodes: the copper per net (`copper:<layer>:<net>`), the mask with its openings, the finish, the silkscreen and the package shapes are drawn by their material's name (`--pcb3d-finish`, `--pcb3d-mask-copper` for the two added with the copper); a node of a kind the view does not know is drawn and not picked.
- A pad's snap point is on its outer copper's surface (`z = -0.01` on a board with a 0.01 mm mask), under the mask's opening.
