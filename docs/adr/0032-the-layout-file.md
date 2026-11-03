---
status: accepted (user review 2026-11-02)
---

# The layout file

ADR 0020 decided that placement geometry lives in one place, a file beside the source keyed by instance path (7A), and that what the circuit implies is derived by the placer (7C). ADR 0030 sketched its contents. This ADR records the format that was built, and the function the placer calls.

## The file

`<stem>.pcb.toml` beside the top-level `.nsm` file (`led.nsm` has `led.pcb.toml`). It is plain TOML, optional, never needed to build, and written by an editor or a placer, never by the language. The name is ADR 0030's; `<stem>.layout.toml` is ADR 0019's file for the schematic, so a regeneration of one cannot disturb the other.

```toml
version = 1

[LedExample.parts."stage.output_cap"]
at = [31.5, 22.0]
rotation = 90
side = "top"
lock = true
source = "hand"

[LedExample.keepouts.antenna]
at = [40.0, 0.0]

[LedExample.cutouts.slot]
at = [5.0, 4.0]

[LedExample.keepouts.keepout1]     # drawn in the editor: it has a size
at = [2.0, 2.0]
size = [10.0, 6.0]
layers = "top"                     # "all" if left out
vias = "allow"                     # what is left out is forbidden
```

- `version` is 1; any other is an error.
- `seed` (a whole number, absent is 0) in a board's table asks the placer for another arrangement of the parts the file does not place: the same seed always gives the same board, another seed another one (ADR 0033).
- One table for each board of the file, keyed by the board's definition name. A table for a board the source lacks is an error listing the boards it has.
- `parts` are keyed by the part's path in the design (`stage.output_cap`), which does not change when reference designators are renumbered. `at` is the footprint's origin in millimetres from the outline's top left corner, y down; `rotation` is degrees counterclockwise on screen (default 0); `side` is `"top"` or `"bottom"` (default top); `lock` says the placer leaves the part where it is (default false); `source` is `"hand"`, `"auto"` or `"regen"` and is kept as written. ADR 0030 wrote the sides as `front` and `back`; `top` and `bottom` are the words the process uses for its layers (`.top`, `.bottom`).
- `keepouts` and `cutouts` give the place (`at`, the corner nearest the origin) of one whose source states no `.x` and `.y`. Stating both is an error. A `keepouts` entry that also has a `size` is a keepout drawn in the editor, which the source does not have: it gives `layers`, `tracks`, `vias`, `copper` and `parts` as well (ADR 0029).
- An unknown key is an error that names it. An entry for a part, keepout or cutout that the board does not have is an error that names the entries and lists what the board has, so a stale layout is found and not ignored.

## The function boundary

`netsmith_pcb::apply_layout(board: &mut Board, layout: &Layout, file: &str) -> Result<(), LayoutError>` is the one place a layout reaches a board. It runs after `Board::from_design_placed` has placed everything, so a part without an entry keeps the placer's position. The placer is told the seed and the locked entries first (`place_options` gives it `PlaceOptions::fixed` for each `lock = true`, ADR 0033), so the parts without one are placed round the locked ones and the outline holds them all; an entry that is not locked is applied afterwards, and the placer does not react to it. A build puts every entry where the file says. It validates everything before it changes anything. `Layout::parse` and `Layout::to_text` read and write the file (deterministically: boards in name order), and `Layout::capture(board, keep)` is the layout of a placed board, keeping the lock and source of the entries in `keep`, for a placer or an editor to write. A part with `side = "bottom"` is on the back: `place_options` tells the placer, and `apply_layout` sets the side. The editor (ADR 0030, E1 to E3) writes the file as `Layout::to_text` has it, which is the same text for the same layout: a comment in a file the editor writes is not kept (a known limit), and the `seed` and every `text` are kept. It writes only an entry that was edited, so a part with no entry stays derived, and it keeps the tables of the other boards.

`netsmith_pcb::place_options(design, layout)` is what the placer is told before it places: the face of each part the layout names, and the boxes (cutouts, keepouts that forbid parts) to keep clear of, wherever the source or the file places them; the size comes from the design's `.outline` (`PlaceOptions::fit`, ADR 0031). `fab` and the editor's PCB view read the file when it exists (`netsmith::layout::read`, `FabOptions::layout`), call `place_options`, build the board with `Board::from_design_placed`, and then `apply_layout`.

## Compatible with ADR 0030

Paths, units, the frame, `lock` and `source` are ADR 0030's. What it adds that is not read yet (`modules`, `waive`, `override`, and the `routes` cache) is refused by name until the step that builds it raises `version` or adds the key; `[Board] seed` is read since ADR 0033. The `Part` of the design now carries its instance path (`netsmith_core::Part::path`), which ADR 0030 listed as to be added.
