---
status: accepted (user review 2026-11-02)
---

# Keepouts

ADR 0020 deferred keepouts until placement had a home. The user needs them (the board has areas where copper or parts must not go), so they are built with ADR 0020's other board features. This ADR records the design.

## What a keepout is

`std::pcb::Keepout` is an interface with a rectangle (`.x`, `.y`, `.width`, `.height`), the copper layers it applies to (`.layers`), and four fields for what it forbids (`.tracks`, `.vias`, `.copper`, `.parts`). It is a member of a board, with its place in the board's frame (the outline's top left corner is the origin, x to the right, y down), or a member of a part, with its place in the part's own frame (the footprint's, turned and moved with the part). A keepout in a module, or anywhere else, is an error: only a board and a part have a place.

**The rectangle is stated or left to the layout file.** On a board, `.x` and `.y` together give the corner nearest the origin, or neither does and the layout file's `[Board.keepouts.name]` entry does (ADR 0032). Stating both is an error, since a thing has one place. A keepout with no place makes the placement check fail, naming it. In a part the place is required: a part's library states only what is always true, and the offsets of an antenna's no-copper zone are.

**No boolean.** The language has no boolean, and ADR 0020 already chose two kinds over a flag. The four fields are text, `"forbid"` or `"allow"`, and a field that is not stated forbids. A keepout that forbids nothing is an error. The user asked for booleans that default to forbidden; this is the same default in the language's own words, and is provisional until the user prefers another spelling (a `Boolean` type would be a language change).

**Layers.** `.layers` is `"all"` (the default, every copper layer of the process) or `top`, `bottom`, `inner1`, `inner2` separated by commas. A layer the board lacks is an error listing the ones it has.

**A polygon** was considered and is not built: a rectangle covers the real cases (an antenna, a connector's mating area, a screw head), and a shape that is not one is several keepouts. A part that turns by other than a right angle gives the box that holds the turned rectangle.

## Who obeys it

- **The router** keeps tracks out of the areas that forbid them on a layer, and vias out of those that forbid vias. Its cells serve tracks and vias alike, so an area that forbids tracks keeps vias out too (a known limit; it only makes the router more careful).
- **The stitching vias** keep out of the areas that forbid vias.
- **The placement check** names a part whose courtyard overlaps an area that forbids parts, and a pad inside an area that forbids tracks on its layer (no track could reach it). A part is not held to the keepout it carries itself.
- **KiCad** gets every keepout as a rule area, a zone with no net and `(keepout (tracks ...) (vias ...) (pads ...) (copperpour ...) (footprints ...))`; a keepout that forbids parts forbids their pads and footprints. KiCad's check flags an item in one (`items_not_allowed`) and its fill leaves the area bare. Both are tested against real KiCad (`crates/netsmith-pcb/tests/kicad_drc.rs`). A rule area carried by a part is written at the part's place in the board file and does not follow the part if it is moved in KiCad.
- **The editor's SVG** draws the area as a dashed red box titled with what it forbids.

## Where a keepout the editor draws lives

In the layout file, not in the source (ADR 0032): the source states what is true of the design, and a rectangle someone dragged on a canvas is a decision of this board. The editor writes `[Board.keepouts.keepout1]` with `at` and a `size`; the entry with a `size` is a keepout of its own, drawn in the file, and it also gives `layers` (`all`, the default, or layer names separated by commas) and `tracks`, `vias`, `copper` and `parts`, each `"forbid"` (the default) or `"allow"`, the same words and defaults as the source's. It is a keepout of the board in every other way: the router, the stitching vias, the placement check and KiCad's rule areas obey it, and a part it forbids is kept out by the placer. The name is the editor's (`keepout1`, `keepout2`...) and the person may rename it in the file; a name the source already has for a keepout is an error that says so, and an entry that has `layers`, `tracks`, `vias`, `copper` or `parts` and no `size` is an error naming the field (only a keepout drawn in the file says what it forbids). A keepout the source sizes but does not place is placed by an entry that has only `at`, as before, and the editor moves it by writing that. A keepout whose place the source states, and one a part carries, are shown and not moved: change them in the source. The editor does not resize a keepout the source sizes. The outline and the cutouts are the source's and are drawn, not edited.

## Not done

A keepout on an inner layer has no effect on the router (it draws on the outer layers) and reaches only KiCad's check and fill. A rule area inside a library footprint's own `.kicad_mod` is kept by the exporter as part of the footprint's text, but the router and the placement check do not read it. A keepout that turns into the exact shape of a rotated part.
