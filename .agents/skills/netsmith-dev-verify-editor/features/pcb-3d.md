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

## Tools (recipe `pcb-3d-inspect`, fixture `inspect_3d`)

`scripts/editor-verify launch --fixture inspect_3d`, then `drive pcb-3d-inspect`. The fixture has seven parts of several kinds: under the lid (3 mm, the left half) a header too tall (`power_in`, 9 mm), an LED with 0.6 mm to spare (`led`), a capacitor with plenty (`input_cap`), a resistor and a 2x3 header of unknown height (`r1`, `bare`); outside it the regulator and its output capacitor.

The bar at the bottom left (`.v3-inspect`):

1. `X-ray` (`[data-testid=view-xray]`, key `V`, `aria-pressed`): the laminate, the mask (and the mask over copper), the bodies, the leads and the silkscreen see-through; the copper, the gold, the hatched unknowns and the limits as they were.
2. A net row of the side panel's `Nets` tab, hovered or clicked, lights that net's copper on every layer and its via barrels (`copper:<layer>:<net>` and `via:<net>` nodes, from PR 449 on) in the hover or selection colour, and dims the rest and sees through the board. A click on a pad (the gold), or on any copper in x-ray, picks its net; the tooltip names it. Until the file has copper groups, a lit net only dims the board.
3. `Colour by` (`[data-testid=colour-by]`, a select; key `C` steps): `Height provenance` (the legend `height-legend`), `Kind` (`kind-legend`: a swatch and a count per kind on the board, from the part's `category` extra) or `Height margin` (`margin-legend`: a ramp red, amber, green and the counts `Over`, `Under 1 mm`, `1 mm+`, `No limit over it`). The colours are the `--pcb3d-kind-*` and `--pcb3d-margin-*` tokens of each theme.
4. `Preview at` (`[data-testid=height-preview]`, `role=switch`, key `H`) and its height (`[data-testid=preview-mm]`, Enter to set, 0.1 to 50 mm): the parts of unknown height stand up, striped in `--pcb3d-preview`, and the label `[data-testid=preview-label]` says `PREVIEW · N parts of unknown height drawn X mm tall · not a height, not checked`. The coverage chip does not change. Off after a reload.
5. Problems dock (`Ctrl+J`): a click on the `too tall` row flies the camera to the part, selects it, and lights the limit `lid` (it pulses once). The board's own findings list does the same.

The handle also has `nets()` (the nets with copper in the scene), `previewCount()` and `focused()` (the limits a finding lit, `name:face`).

For a recipe the host carries a handle: `document.querySelector('[data-testid=pcb-3d]').view3d` with `project(path)` (a point of the window where a pick finds the part, found by raycasting over its bounds: a package shape has leads and a lower body, not a box; null when it is hidden), `sample()` (the share of the canvas that is not the background, and how many colours), `colourAt(x, y)`, `parts()`, `preset()` and `ortho()`.

## Gotchas

- The digits are the sections' keys: the presets use letters.
- The first frame is slow in headless Chromium (SwiftShader): wait on `data-ready`, not a time.
- Pixels are read in the same task as a fresh draw (`sample`, `colourAt`): the canvas keeps no buffer, so a screenshot is the way to see it (`canvas.toDataURL` may come back blank).
- A recipe never hard-codes a designator: take the part from the 2D scene (`/api/pcb/scene`) by its path and ask `project(path)` where it is.
- The view draws only the file's nodes: copper and package shapes arrive in the same file (#430) and are drawn by their material's name; a node of a kind the view does not know is drawn and not picked.
