---
name: netsmith-inspect-a-board
description: Look at a netsmith board without a browser - which picture or number answers which question (schematic legible, parts fit, heights, copper, what the board house gets, what the person sees), how to read a picture's frame, side and units, how to light a part or a net, and when to ask the person to look. Use when you need to check a board you made, or when a person asks "does it look right".
---

# Inspect a board

A picture is for your eyes, a number is for your answer. Take the number when a command gives one, quote it, and take the picture to find what the number does not say. Every picture command writes one file and prints its path; read the file with your image viewer. None needs KiCad, a browser or a GPU, and the same source and flags give the same bytes. `netsmith view --help` and each command's `--help` have the flags; `docs/BOARD3D.md` has the 3D frame and sections.

## Which one answers which question

| Question | Take | Quote |
|---|---|---|
| Is the schematic legible, is a net or a sheet where it should be? | `view schematic --format png` (`--sheet NAME` for one sheet, `--board NAME`); PDF or SVG to hand on | the sheet names it prints |
| Do the parts fit, is a part too close to another or past the edge? | `layout summary --format json` (`--near PATH --within MM`, `--between U1.3 C1.1`), `layout check`, `layout fit PATH` for a free place, `layout parts --format json` for geometry | the distances and findings |
| Are a module's parts together, how far does it spread? | `layout summary --format json` (each module's box, `spread` and `intruders`), then `view pcb --highlight module:reg` or `view 3d --highlight module:reg,module:usb` (each module in its own colour) | `spread`, `intruders` |
| Is the height right (a lid, a rib, a stacked board)? | `dev board3d --format json`, then `view 3d --view front --section x=12.5` or `--highlight limit:lid` | `margin` of each limit, `unknown_heights` |
| Is the copper right: what is routed, what is open, what is in the way? | `view pcb --only-open` (a `--region`, a `--layer`), `view pcb --drc`; `board copper` for planes and pours as text; `board fill` for how each pour joins each pad (spokes, solid, none) and its copper islands, and `view pcb --islands --drc` to see the islands its fill took out and the pads it starves; `board drc --format json` | the `open:` lines, the findings by `marker` number |
| How long is each line, how many vias, is a diff pair matched and coupled? | `board nets` (`--route` to route first, `--lengths` longest first, `--format json`): each net's routed and straight length, vias, layer changes and layers; each pair's lengths, skew against the router's 0.1 mm, coupled length, gaps and vias side by side | the `pair` lines, `skew`, `share`, `via_pairs` |
| What will the board house get? | `view gerber fab/<Board>` (`--layer top\|bottom\|inner1\|all`, or one function alone: `--layer top-paste`, `drill`; repeat to stack), `--format json` for files, sizes and holes | `board`, `holes`, `unread` |
| Which pins does a net join, what did the solve leave a part (ratings, effective capacitance, row), is the pick lock current? | `board show` (text, or `--format json`) | the part's lines, the net's pins |
| What does the bill cost, what does a rail carry? | `board bom --offline`, `board power` (text, or `--format json`) | the lines and totals |
| What does the person see in the editor? | `view editor-3d` with the person's `view editor` running; with no flag it prints the page's state | the state it prints |

A picture is a view of the design, not a check: the checks are `board build`, `layout check` and `board drc`. Never say a part is in the right place or under a limit from a picture alone.

## Read a picture

- **3D (`view 3d`, `dev board3d`)**: millimetres, z up, origin at the outline's **bottom-left** corner on the top face, y up. `--view top` has x to the right; `bottom` has x to the left (turned over left to right); `iso` is the corner at +x, -y, +z. `--section x=7` cuts 7 mm in from the left edge; `z=0` is the top face, `z=-1.6` the bottom of a 1.6 mm board. The part's height is from its face; a part of unknown height is a flat orange outline, never a box.
- **Layout, routes, 2D copper and Gerber files**: millimetres from the outline's **top-left** corner, x right, **y down** (`--region X,Y,W,H`). A layout point `(x, y)` is `(x, outline height - y)` in the 3D frame: convert before you carry a number from one picture to another.
- **Sides**: `view pcb --layer bottom` and `view gerber --layer bottom` are drawn as a person turns the board over, mirrored left to right. A part on the bottom is at its layout position, but seen from below.
- **One grid**: `view pcb` and `view gerber` with the same `--layer`, `--region` and `--scale` give pictures of the same size, pixel for pixel over the same place, so the board model and the files can be set side by side. With no `--region` both draw the outline with 1 mm round it.
- **Scale**: `view pcb` and `view gerber` take `--scale` in pixels a millimetre (20 by default; at most 16384 pixels a side). For one pad or a gap take a `--region` of 10 to 20 mm at `--scale 40` instead of the whole board. A section of a board shows the layers to scale: at the default size a 35 µm copper layer is under a pixel, so cut a narrow board.
- **Markers**: `view pcb --drc` rings each finding with a number, red for an error and amber for a warning, and prints `marker 1: error [clearance] ...` (a pad a pour starves is its `starved_thermal`); `--islands` hatches each copper island the fill of a pour took out, labelled `I1`, `I2`, ... and printed as `removed island I1: ...`; an open connection is a bright line from pad to pad, printed as `open: net GND: D1.1 to J1.2`.
- **The schematic**: a PNG is 10 pixels a millimetre with netsmith's own stroke font, enough to see the arrangement; a name or value to quote comes from the PDF's text, `board bom` or `layout show`, never from reading the pixels.

## Light a part or a net

`--highlight` lights what it names and dims the rest, on `view 3d`, `view pcb` and `view editor-3d`: a part by its designator or path (`J1`, `power_in`), `net:GND`, `module:<path>` (every part of a module, by its path in the design as `layout summary` names it; each module named in its own colour), and on the 3D ones `limit:lid` (a height limit's volume, red where a part is too tall) and `case:lid` (a case model). Several go in one flag, `--highlight net:GND,R1`. An id the board lacks is exit 2, with the parts, nets, limits and modules it has. `view pcb --only-open` lights the nets that still have an open connection. `--xray` sees through the bodies; `--case hidden` leaves the case out so the board shows.

## When to look, and when to ask the person

1. After a build, take the schematic and, once the board is placed, the 3D top view and `view pcb --drc`. Say what you saw and what you did not check.
2. Before `fab` ends a job, `view gerber` the package you will hand over (`netsmith-prepare-fab-order`): the outline, the holes and the silkscreen are what the board house gets, not what the source says.
3. When the editor is open, `view editor-3d --highlight ...` lights the same thing in the page the person has open, so they see what you see; it exits 2 with the reason when no page is open. Do not drive their page without telling them: it moves their view.
4. **Ask the person to look** when the answer is a judgement and no number settles it: whether a schematic reads well to them, whether a silkscreen label is where they would want it, whether a part clears the real case (a drawing is not the case), whether a connector's side or orientation is the one their cable needs. Say which picture, which command and what to look at (`netsmith view editor`, the part, the layer). Never ask them to confirm a number you can read.
5. In your answer give the number, the command and the path of the file as evidence; a picture alone is not evidence of a measurement.

## Report

Say which pictures and numbers you took (the command and the file), what each showed, and each finding left. Name what you could not check (a height unknown, a region you did not draw). If a picture and a number disagree, trust the number, say so, and find which was misread (a frame, a side) before you go on.
