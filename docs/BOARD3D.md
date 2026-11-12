# `netsmith board3d` and `netsmith render`: the board in 3D, as data, as glTF and as a picture

`netsmith board3d` places a board as `fab` and the editor place it and prints its _Scene_
(ADR 0105): the board as its stackup makes it, a summary of its copper, each part's body box from
its face to its height, each height limit as a volume with the tallest part under it, and what is
unknown. It is for an
agent, a script or a person who wants the 3D facts of a board as numbers, with no GPU, no browser
and no KiCad.

```sh
netsmith board3d examples/led.nsm                 # a short summary
netsmith board3d examples/led.nsm --json          # the report below, on stdout
netsmith board3d examples/two_boards.nsm --board TwoBoards/led --json
netsmith board3d examples/led.nsm --glb led.glb   # the scene as glTF 2.0 binary (below)
```

- The board is placed exactly as `fab` places it: the pick lock beside the source holds the
  picks, the physical file's requirements and the layout file (`<stem>.pcb.toml`) are read, and
  the layout file's positions are put on the board. Nothing is routed, so the copper has pads,
  planes and pours, and no tracks or vias.
- Nothing is written but the `--glb` file: not the pick lock, not the pin file, not the layout
  file. With `--glb` alone nothing is printed; with `--json` too the report is printed as well.
- No KiCad is needed (ADR 0097); the `no-kicad` job runs it with KiCad hidden.
- The same source gives the same bytes: every list is in a fixed order and every length is
  rounded to a nanometre.
- A file of several boards needs `--board` (`Name`, or `System/member` for a board of a system).
  With no file it acts on the project's top-level file; a project of several needs `--board` or a
  file.
- Exit 0 when the report is printed (and the file written), 1 when the board does not build or
  cannot be placed (the error is `fab`'s), 2 when the command cannot start or the file cannot be
  written.

## The frame

Millimetres, right-handed, z up. The origin is the outline's **bottom-left** corner (as the
layout file and the editor draw the board, with its top edge up), on the board's **top face**:
x runs right, y up toward the outline's top edge, z up out of the top face. The top face is
`z = 0` and the bottom face `z = -thickness`. A face is the outside of its solder mask, which is
what a part's height is measured from.

A layout-file point `(x, y)` (from the outline's top-left corner, y down) is
`(x, outline height - y)` here. A turn is the layout file's: degrees, counterclockwise seen from
above. Each part also carries its place as the layout file writes it (`layout`), to find it there.

A part on the top face stands from `z = 0` up to `z = height`. A part on the bottom face hangs
from `z = -thickness` down to `z = -thickness - height`.

## The report

```json
{
  "schema": 2,
  "units": "mm",
  "frame": "millimetres, right-handed, z up; ...",
  "board": {
    "name": "Lidded",
    "outline": { "width": 40.0, "height": 24.0, "corner_radius": 0.0 },
    "thickness": 1.6,
    "stack_thickness": 1.6,
    "layers": [ { "name": "F.Mask", "kind": "mask", "thickness": 0.01, "z_top": 0.0, "z_bottom": -0.01 }, "..." ],
    "cutouts": [],
    "holes": [ { "kind": "pad", "path": "bare", "refdes": "J3", "pad": "1", "from": [17.0, 12.0], "to": [17.0, 12.0], "diameter": 1.0, "plated": true } ]
  },
  "copper": {
    "layers": [
      { "name": "F.Cu", "z_top": -0.01, "z_bottom": -0.045, "thickness": 0.035, "pads": 8, "tracks": 0, "vias": 0, "pours": 0,
        "nets": [ { "net": "GND", "area": 4.6 }, { "net": null, "area": 1.2 } ] },
      "..."
    ],
    "vias": 0,
    "mask_openings": { "top": 8, "bottom": 2 }
  },
  "parts": [
    {
      "path": "tall", "refdes": "J1", "kind": "Header1x2", "side": "top",
      "layout": { "x": 5.0, "y": 12.0, "rotation": 0.0 },
      "body": { "min": [3.73, 8.19], "max": [6.27, 13.27], "corners": [[3.73, 13.27], "..."], "from": "fab" },
      "z_bottom": 0.0, "z_top": 9.0, "height": 9.0, "provenance": "stated",
      "pins": { "min": [4.15, 8.61], "max": [5.85, 12.85], "protrusion": null, "z_top": null, "z_bottom": null }
    }
  ],
  "height_limits": [
    {
      "name": "lid", "min": [0.0, 0.0], "max": [20.0, 24.0], "max_height": 3.0, "faces": ["top"],
      "tallest": { "path": "tall", "refdes": "J1", "height": 9.0, "face": "top" }, "margin": -6.0,
      "per_face": [ { "face": "top", "z_bottom": 0.0, "z_top": 3.0, "tallest": { "...": "..." }, "margin": -6.0, "unknown": ["bare"] } ]
    }
  ],
  "too_tall": [ { "path": "tall", "refdes": "J1", "limit_name": "lid", "face": "top", "height": 9.0, "limit": 3.0 } ],
  "overlaps": [],
  "unknown_heights": [ { "path": "bare", "refdes": "J3", "reason": "the design states no `.height` and the part has no catalog row to take one from (...)", "under_limits": ["lid"] } ],
  "coverage": { "known": 2, "total": 3, "stated": 2, "catalog": 0 },
  "tallest": { "top": { "path": "tall", "refdes": "J1", "height": 9.0 }, "bottom": null }
}
```

(From `scripts/editor-verify-js/fixtures/height_limits.nsm`, shortened.)

| Field | What it is |
|---|---|
| `schema` | The version of this shape. A field removed or changed in meaning raises it; a field added does not. |
| `board.thickness` | The thickness the board chose, which the board house is asked for. |
| `board.stack_thickness` | The sum of the layers below, the process's own figures. On a four-layer JLCPCB board it is 1.6062 mm against 1.6 mm: the bottom layers then reach a little past `-thickness`. |
| `board.layers` | Top to bottom: mask, copper, dielectrics (`core`, `prepreg`), copper, mask, each with its `z_top` and `z_bottom`. The same list the KiCad board file's stackup is written from (`crates/netsmith-pcb/src/stackup.rs`). |
| `board.cutouts`, `board.holes` | The holes through the board: cutouts as boxes with a corner radius; drilled holes by part `path`, `refdes` and `pad` (or by `net` for a via), a slot from `from` to `to`, `plated` or not. |
| `copper` | A summary of the board's copper, with no polygons (they are in the scene, `netsmith_pcb::scene3d::SceneCopper`, for the 3D view). One entry per copper layer, top to bottom (`F.Cu`, the inner layers, `B.Cu`): `z_top`, `z_bottom` and `thickness` (the same as the layer's entry in `board.layers`), and how many `pads`, `tracks` (straight pieces) and `vias` it has, and how many polygons of `pours` (a pour on an outer layer, a plane on an inner one). `nets` is the copper of each net on the layer in mm², to 1e-4 mm², by net name with the copper that has no net (`null`) first. It adds up each net's pad, track and pour polygons, each by itself: a pad that stands in a pour of its own net is in the number twice, and drill holes and via rings are not taken away or added. `vias` at the top is the count of the board, each a barrel through it with a ring on every layer; `mask_openings` counts the openings of the solder mask on each face. The copper is the copper of the Gerbers (`crates/netsmith-pcb/src/copper.rs`), so the pours have the clearances and thermal reliefs `fab` writes. |
| `parts` | One per part with a body (a label has none), by path. |
| `parts[].body` | The body box: the footprint's fabrication outline (`from: "fab"`), else its courtyard (`"courtyard"`), as a placement requirement and the height check measure it. `corners` are turned with the part; `min` and `max` hold them. |
| `parts[].z_bottom`, `z_top` | The body's Z range. The end at the board face is always known; the far end is `null` when the height is unknown. |
| `parts[].height`, `provenance` | The height and where it came from (_Height provenance_): `stated` (the source or the physical file), `catalog` (the picked row's `height` cell; `catalog_row` names the row) or `unknown`. An unknown height is `null`: never zero and never a guess (ADR 0094). |
| `parts[].pins` | For a part with plated holes: the box of its pads, and its _Pin protrusion_ below the far face where the design states one (`protrusion`, `source`, `z_top`, `z_bottom`); `null` lengths where it states none, which is unknown and not zero. |
| `height_limits` | Each placed keepout with `.max_height`, by name: its box, the faces it limits, and for each face the volume (`z_bottom` to `z_top`), the tallest part of known height whose body is under it, the `margin` (the limit less that height; negative when too tall) and the parts of unknown height under it. `tallest` and `margin` at the top are the face with the least margin. A limit a part carries names it as `carrier`. |
| `too_tall` | The parts over a limit: the placement check's own finding, which stops `fab`. |
| `overlaps` | Two bodies on one face, both of known height, whose turned boxes share area: they fill the same space from the face up to the shorter one (`z_bottom` to `z_top`). New in 3D: the placement check compares courtyards in plan. |
| `unknown_heights` | Each part of unknown height, with the reason (no `.height` and no catalog row, or a row whose `height` cell is empty) and the limits it stands under, whose check it leaves unchecked. |
| `coverage` | _Height coverage_: how many parts have a known height, of all of them, and how many of those are stated and from catalog rows. The same count as the coverage line of `fab-notes.txt`. |
| `tallest` | The tallest part of known height on each face. |

Schema 2 added `copper`: raised, though a field added does not require it, so that a reader can tell a report that has the copper summary from one that has not. The `.glb` file has no copper yet.

## The `.glb` file

`board3d --glb <file.glb>` writes the same scene as glTF 2.0 binary
(`crates/netsmith-pcb/src/gltf.rs`). It is the file the editor's 3D view displays: the editor
serves it at `GET /api/pcb/3d?board=<name>` (`model/gltf-binary`), drawn from the placement its
PCB view shows, with that placement's layout stamp in the `X-Netsmith-Layout-Stamp` header. For
a board the editor has not routed, its bytes are the ones `board3d --glb` writes. Any glTF viewer
opens it, and a script reads the ids from the JSON chunk. The file passes the Khronos glTF
validator with no errors and no warnings.

The contract, schema 1 (a viewer is written against it: a field removed or changed in meaning
raises `schema`, a field added does not):

- **Units and axes are glTF's**: metres, right-handed, Y up. A scene point `(x, y, z)` in mm is
  `(x, z, -y) * 0.001` in the file, in the vertex data (no node has a transform): the top face is
  `y = 0`, the outline's bottom-left corner the origin, its top edge toward `-z`. The numbers in
  the `extras` stay in the frame above, in mm, as in the JSON report (their names end in `_mm`).
- **The same scene gives the same bytes.** The order of nodes, meshes, materials and accessors is
  fixed, and `asset.generator` is `netsmith`, with no version and no time.
- **Nodes.** Each carries `extras.netsmith.kind`. The one root, `board`, has its children in this
  order:

| Node | `extras.netsmith` |
|---|---|
| `board` (the root) | `kind: "board"`, `schema`, `board` (the label, `system/member` for a board of a system), `thickness_mm`, `stack_thickness_mm`, `width_mm`, `height_mm`, `coverage: {known, total}`, `findings: [{code, message, parts: [path...], limit?, face}]` with `code` `too_tall`, `height_unchecked` or `body_overlap` |
| `substrate` | `kind: "substrate"`: the laminate. Children `mask:top`, `mask:bottom` (`kind: "mask"`, `face`) and one per plated hole, `hole:<refdes>.<pad>` (`kind: "hole"`, `path`, `refdes`, `pad`) or `via:<net>` (`kind: "hole"`, `net`), the barrel |
| one per part, named by its designator | `kind: "part"`, `path`, `refdes`, `side` (`top` or `bottom`), `provenance` (`stated`, `catalog` or `unknown`), `height_mm`, `z_bottom_mm`, `z_top_mm`. A known height is a solid (material `body`); an unknown one is the body's flat outline (material `body-unknown`), and `height_mm` and the far end (`z_top_mm` on the top face, `z_bottom_mm` on the bottom) are `null`: never a guessed height |
| `limits` | one child per face of each height limit, `limit:<name>:<face>`: `kind: "height_limit"`, `name`, `face`, `max_height_mm`, `tallest` (a path or `null`), `margin_mm` (or `null`), `violated`, `unknown` (the paths of parts of unknown height under it); its volume is the mesh |
| `protrusions` | one child per part whose pins stand out of the far face by a stated length, `pins:<refdes>`: `kind: "protrusion"`, `path`, `length_mm` |
| `copper:<layer>` | reserved for the copper (#430), with children `kind: "copper"`, `layer`, `net` |

A node whose shape has no area has no mesh, and is still there.

- **Materials**, by name, metallic-roughness: `substrate`, `mask-top`, `mask-bottom`, `copper`,
  `body`, `body-unknown`, `metal`, `mark`, `limit`, `limit-violated` (those two see-through,
  `alphaMode: BLEND`) and `prepreg` (the second laminate tone of a section picture: no mesh of
  the file uses it). All eleven are always written, in this order. Their colours are the
  rasterizer's light palette in linear light. No glTF extension is used.

## Pictures for agents: `netsmith render`

`netsmith render` places the board exactly as `board3d` does (the same code: the pick lock, the
physical file and the layout file are read, nothing is routed, nothing is written but the
picture), makes the scene's meshes (`netsmith_pcb::mesh`, the ones the `.glb` file is written
from) and draws them to a PNG with netsmith's own software rasterizer
(`netsmith_pcb::raster3d`, ADR 0105). It is for an agent that can read an image, a golden test
and a person without a 3D viewer: **no GPU, no browser and no KiCad**, and the same source and
options give the same bytes on every machine (all geometry is `f64` arithmetic in a fixed order,
coverage is decided in integers, and there are no threads).

```sh
netsmith render examples/led.nsm                              # LedExample-iso.png, 1600x1000
netsmith render examples/led.nsm --view top --out top.png     # straight down at the top face
netsmith render examples/led.nsm --highlight J1               # the part lit up, the rest dimmed
netsmith render lidded.nsm --highlight limit:lid --xray       # a height limit's volume, in x-ray
netsmith render examples/led.nsm --view front --section x=7   # cut at x = 7 mm
netsmith render examples/four_layer.nsm --view right --section x=13   # the stackup in the cut
netsmith render examples/four_layer.nsm --view right --section x=13 --no-caps   # the cut left open
netsmith render examples/two_boards.nsm --board TwoBoards/led --view iso-bottom --theme dark
```

The command prints the path it wrote. Read the file with the image viewer of the agent (or
open it): a picture to look at, with the numbers from `board3d --json` to quote.

| Option | What it does |
|---|---|
| `--view` | `top`, `bottom`, `front`, `back`, `left`, `right`, `iso` (the default) or `iso-bottom`. See the table below. |
| `--out <file.png>` | Where to write; default `<board>-<view>.png` in the current folder (a board of a system, `System/member`, is `System-member`). The folder must exist. |
| `--size <w>x<h>` | The picture in pixels; default `1600x1000`. 64 to 8192 a side and at most 16 million pixels, else exit 2. |
| `--section x=<mm>` (or `y=`, `z=`) | One cut at right angles to an axis, at that position in the scene's frame (below). The side nearer the camera is not drawn, so the cut is seen; for a view along the axis the lower side is kept. The cut faces are filled (caps, below). |
| `--no-caps` | With `--section`: leave the cut faces open, as the cut of a hollow shell, and draw the board as one block, as without a section. |
| `--highlight <id>[,<id>...]` | May be repeated. A part by **designator** (`J1`) or **path** (`power_in`), `net:<name>` and `limit:<name>` (a height limit, as `board3d` names it). What is named is drawn in the highlight colour and the rest is dimmed. An id the board does not have is an error (exit 2) that names it and lists the parts, nets and limits the board has; nothing is written. |
| `--xray` | Every mesh see-through, to look through the board and the bodies. |
| `--theme light\|dark` | The palette; default `light`. |
| `--ortho` \| `--perspective` | Parallel projection (the default, so lengths are comparable) or a pinhole with a 30° field of view. |

The views look at the board's middle, framed to fit it with a margin, in the frame of this
document (millimetres, right-handed, z up, origin at the outline's bottom-left corner on the top
face, y up toward the outline's top edge):

| View | Seen from | In the picture |
|---|---|---|
| `top` | above (+z) | x to the right, y up; the board as the editor's PCB view draws it |
| `bottom` | below (-z) | x to the left, y up: the board turned over left to right |
| `front` | -y | x to the right, z up |
| `back` | +y | z up |
| `left` | -x | z up |
| `right` | +x | z up |
| `iso` | the corner at +x, -y, +z | z up |
| `iso-bottom` | the corner at +x, -y, -z | z up |

`--section` is in the same frame: `z=0` is the top face and `z=-1.6` the bottom face of a 1.6 mm
board, `x=7` is 7 mm in from the outline's left edge, and `y=3.5` is 3.5 mm up from its bottom
edge. A layout-file point (from the top-left corner, y down) is `(x, outline height - y)`.

**A section is a drawing of the stackup.** Where the plane cuts a closed solid, the cut face is
filled (a cap, flat in the plane, in the solid's colour), so the cut reads as a real
cross-section. The board is drawn for it as the stackup `board3d --json` lists (`board.layers`):
a slab per layer at its own Z and true thickness, the **solder mask** in green, the **core** and
the **prepreg** in two laminate tones, and the **copper** of each layer in copper colour where
the plane crosses it (the pads, tracks, pours and via rings the scene has, less the drilled
holes; where a layer has no copper at the cut, the insulator it is embedded in: the mask for
an outer layer, the prepreg for an inner one). A plated barrel is a ring of copper 25 µm thick
(a picture's figure, not the fab's), so a cut through a hole shows two strips of copper either
side of it. A part's body, its pins and a height limit's volume are cut solid. No line is drawn
between the bands. The layers are to scale: at the default size a whole board's 10 µm mask and
35 µm copper are a fraction of a pixel, so to read the bands, look at a narrow board or use a
camera framed on the edge (the library's `Camera`, with `netsmith_pcb::mesh::of_scene_section`
for the meshes); the picture is a view of the scene, and a copper layer's polygons are not
checked against the Gerbers here.

What is drawn is what the scene has: the board (laminate with the green solder mask, with its
outline, cutouts and drilled holes), the plated barrels, each part's body as a dark box from its
face to its height, a part of **unknown height as a flat orange outline** on the board (never a
box, ADR 0094), pins that stand out below the far face where the design states a protrusion, and
each height limit as a see-through blue volume, red where a part is too tall for it. Other mesh
ids, such as the copper's nets, pass through to `--highlight` as the scene gives them. A picture
is a view of the scene and not a check: the numbers of `board3d --json` are.

Exit 0 when the picture is written, 1 when the board does not build or cannot be placed (the
error is `fab`'s), 2 when the command cannot start, an option is wrong (a view, a size, a section,
a theme, an id of `--highlight` the board does not have) or the file cannot be written.
