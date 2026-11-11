# `netsmith board3d`: the board in 3D, as data

`netsmith board3d` places a board as `fab` and the editor place it and prints its _Scene_
(ADR 0105): the board as its stackup makes it, each part's body box from its face to its height,
each height limit as a volume with the tallest part under it, and what is unknown. It is for an
agent, a script or a person who wants the 3D facts of a board as numbers, with no GPU, no browser
and no KiCad.

```sh
netsmith board3d examples/led.nsm                 # a short summary
netsmith board3d examples/led.nsm --json          # the report below, on stdout
netsmith board3d examples/two_boards.nsm --board TwoBoards/led --json
```

- The board is placed exactly as `fab` places it: the pick lock beside the source holds the
  picks, the physical file's requirements and the layout file (`<stem>.pcb.toml`) are read, and
  the layout file's positions are put on the board. Nothing is routed, so there are no vias yet.
- Nothing is written: not the pick lock, not the pin file, not the layout file.
- No KiCad is needed (ADR 0097); the `no-kicad` job runs it with KiCad hidden.
- The same source gives the same bytes: every list is in a fixed order and every length is
  rounded to a nanometre.
- A file of several boards needs `--board` (`Name`, or `System/member` for a board of a system).
  With no file it acts on the project's top-level file; a project of several needs `--board` or a
  file.
- Exit 0 when the report is printed, 1 when the board does not build or cannot be placed (the
  error is `fab`'s), 2 when the command cannot start.

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
  "schema": 1,
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

Copper (pads, tracks, vias, planes and pours, with their nets) is not in the report yet; it comes
with the copper of the 3D view (#430).
