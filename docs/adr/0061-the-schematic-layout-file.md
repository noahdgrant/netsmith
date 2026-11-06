---
status: proposed (the owner decided on 2026-11-06 that the schematic keeps where a person moved a part, and left the choices of the file to the author; every choice below is for the owner to review. Stage 1 of 4 is built: the file's types, reader, writer and matching; the drawing does not read it yet)
---

# The schematic remembers where a person moved a part, in a layout file of its own

Replaces the paragraph of ADR 0019 that sketched moving a symbol (`<stem>.layout.toml`, `[Board.parts]`, `{x, y}`): its name, its keys and its handling of a stale entry change here. It keeps what that paragraph decided: a sidecar, never inside the `.nsm`, an offset and not a place, an entry for a part that is gone is reported and ignored. It follows ADR 0003 (the language stays text; the editor draws and never writes the netlist), ADR 0032 (the PCB's layout file, whose file rules it copies) and ADR 0048 (physical intent is a file of its own; this one is not physical intent).

## Why

The schematic is drawn by `netsmith-view` as a pure function of the design (ADR 0019). That is right for a first drawing, and wrong for a drawing someone has worked on. A person who moves a connector to the left edge, or turns a crystal, loses the move at the next edit, because the layout engine decides again. An engineer's drawing is read the way it was arranged; the arrangement is the work.

The move is not circuit and not a requirement of the product. It says nothing the checker consumes (ADR 0048's test, "would a circuit check consume it?", is no), and nothing about where the part goes on the board. It is the editor's own state about its own drawing, so it has a home of its own, as the PCB's solution has (ADR 0032). The source stays strictly the circuit (ADR 0003, DESIGN-RULES: statement order never matters; a position is not circuit).

## The decision

**File.** `<stem>.sch.toml` beside the top-level `.nsm` file: `src/main.nsm` has `src/main.sch.toml`. It is the PCB file's rule (`netsmith::layout::layout_path` is `source.with_extension("pcb.toml")`; `netsmith_view::schematic_hints_path` is `source.with_extension("sch.toml")`). One file for all the boards of a source file, one table for each board, keyed by the board's definition name. A module's file has no file of its own: a module is drawn as a sheet of a board, and its entries are in that board's table, so a module used by two boards is arranged twice, as it is drawn twice. The file is optional, plain TOML, never needed to build, written by the editor and editable by hand. A comment in a file the editor rewrites is not kept (the PCB file's known limit). `version = 1` is required in a file that has anything else; any other version is an error.

The name is `.sch.toml` and not ADR 0019's `.layout.toml`: the glossary's _Layout file_ is the PCB's, and a file called `layout` that is not it would be read as it. atopile has the same pair, `.pcb.hl` and `.sch.hl` (ADR 0030 section f). The glossary term is _Schematic layout file_; the code says `LayoutHints` and "hints", the entries.

**Keying: by instance path, grouped by sheet.** A part is keyed by its path in the design (`netsmith_core::Part::path`, ADR 0032), never by reference designator, which a renumber changes. The entries are grouped by the sheet the part is drawn on: a sheet is a module instance of the board (`stage`), named as `netsmith_view::Group::name` names it, or the board's own sheet for the parts the board declares itself. Inside a sheet the part is keyed by its path from the sheet: `filter.l1` for `stage.filter.l1` on the sheet `stage`, and the path itself on the board's own sheet. So a module that is renamed keeps its entries, and the edit is one table header; the entries are about the module and not about where it is. A sheet is drawn cut into numbered boxes when it is big; the file does not say which box, because the box is a fact of the drawing (the entry is found by the part).

A sheet is the same whether or not the board is drawn split: a small board drawn as one picture still has the sheets `stage` and `radio` in its file, so an entry does not change meaning when the board grows past 16 parts.

**Value: an offset of the block, in millimetres on the 1.27 mm grid.** `dx` and `dy` (millimetres, x to the right, y down, as on the page) move the part's **block**, the part with the satellites that serve it (GLOSSARY: _Satellite_), from where the automatic drawing puts it. The block moves as one: its satellites, their wires and the power symbols at them are carried with it, and the layout engine routes the wires after the move (stage 2). A satellite also has an entry of its own to move it within its block. The value is a multiple of 1.27 mm (the editor snaps; a hand-written value off the grid is an error that names the nearest two). Optional per part:

- `rotate`: 0, 90, 180 or 270 degrees counterclockwise on the page, **replacing** the automatic turn, from the symbol as the library draws it (0). `-90` and `360` are read as 270 and 0.
- `mirror`: `true` or `false`, **replacing** the automatic mirror. The symbol is flipped left to right first, then turned. With `rotate` this is the eight orientations the drawing engine has (`geom::Orient::ALL`).
- `pin = [x, y]` instead of `dx` and `dy`: the block's origin is **pinned** to a place `x, y` from the sheet's reference part, whatever the automatic drawing does with the rest. It is for a part someone wants fixed (the connector at the left edge) while everything else is arranged round it. Giving `pin` with `dx` or `dy` is an error.

A note (GLOSSARY: _Schematic note_) has `dx` and `dy` only: how far its frame was moved from where the drawing put it. A note is keyed `<path>#doc` or `<path>#relation` by the path it belongs to (`Note::path`, from the sheet), and `#relation2`, `#relation3` for the second and third relation of that path, in the order the design lists them. The path of a note on a module itself is empty (`"#relation"`).

**The frame.** The offset is what a person dragged by, so it is the same in any frame: a move of 2.54 mm to the right stays 2.54 mm to the right when the automatic drawing shifts as a whole. The frame matters for `pin` and for what an offset is relative to. It is the sheet's anchor-relative frame, the one the stable layout (open pull request #114) gives: the sheet's reference part, the largest, is at the origin on the grid, y down. An offset is applied to a part's automatic place in that frame; a pin is a place in it. So an edit that makes the sheet grow upward or leftward does not move a pinned part or any offset one, because the origin is the reference part and not the corner of what is drawn. (#114 has landed, and stage 2 applies `pin` from that origin.) No entry ever holds a coordinate in the absolute page frame: the page's frame moves whenever a sheet's neighbour changes size.

**Staleness is a finding, not an error.** ADR 0032 makes an entry for a part the board lacks an error, because a PCB layout that disagrees with its board is wrong to fab. A schematic hint is a convenience: a refactor that renames a part must not turn the editor's view into an error. So:

- An entry whose path is not in the design on that sheet, or whose sheet is not a sheet of the board, is a **finding**: a note in the Problems list, stage `layout`, naming the entry and the file's line (`main.sch.toml:19: [Trellis.sheets.radio.parts.gone]: the design has no part `gone` on this sheet; the entry is ignored`). It is ignored. An entry whose part is on another sheet says which (`the part `filter.l1` is drawn on the sheet `stage`, not on this one`). A table for a board the source does not have is a finding too (`LayoutHints::unknown_boards`, given the boards of the source file).
- A part or note with no entry is placed automatically.
- An entry that cannot be honoured, because its block would lie on another part's body, is a finding and is ignored. That is found by the drawing, which has the geometry (stage 2: `HintFinding::unusable`).
- What the file says wrong is still an error, with its line, as in ADR 0032: bad TOML, an unknown key, a value off the grid, `rotate` not a multiple of 90, `pin` with `dx`, a key that is not a path. The difference: an error is what the file says; a finding is what no longer matches the design. All errors of a file are returned at once, in file order.

**It changes nothing but the picture.** The drawing is a pure function of the design and the hints, and the hints are an input, never ambient state (a file read inside the engine would make a drawing depend on the disk). Empty hints give byte-for-byte the drawing there is today (`ResolvedHints::default()`). The file holds no wire, label, bus, net, or anything that changes connectivity, and stage 2 routes the wires again after a move, so `netsmith_view::check`'s invariants (the drawing's nets are the design's nets) hold for any hints: a hint can make a drawing ugly and cannot make it wrong. The editor draws and never writes the netlist (ADR 0003): this file does not cross that line.

**Written deterministically.** `to_toml` writes `version`, then boards in name order, in a board its own sheet and then the sheets in name order, in a sheet the parts and then the notes, by key in byte order, with a fixed spelling of numbers (`2.54`, `-1.27`, `0`). The same hints give the same text, and `parse(to_toml(h)) == h`. A hand edit that parses and is written back changes only formatting. An entry with no field is kept; a table with no entry is not written.

## A worked example

The board `Trellis` has a connector `j1` and a resistor `r1` of its own, a module `stage` (parts `output_cap`, and `filter.l1`, `filter.c1` inside a module `filter`) and a module `radio` (`u1`). `src/main.sch.toml`:

```toml
version = 1

[Trellis.parts.j1]
dx = 2.54
dy = -1.27

[Trellis.sheets.stage.parts.output_cap]
rotate = 90
mirror = true

[Trellis.sheets.stage.parts."filter.l1"]
pin = [12.7, 0]

[Trellis.sheets.stage.notes."output_cap#doc"]
dx = 5.08
dy = 0

[Trellis.sheets.radio.parts.gone]
dx = 1.27
dy = 0
```

In the drawing: the connector's block is two steps right and one up from where it was put. On the sheet `stage`, the capacitor is turned a quarter turn counterclockwise and flipped left to right, the inductor stays 12.7 mm right of the reference part of the sheet, and the note about the capacitor (its doc comment) is 5.08 mm further right than the drawing put it. `r1`, `filter.c1` and `radio.u1` have no entry and are placed as before. Resolving the file against the design gives one finding:

```
src/main.sch.toml:18: [Trellis.sheets.radio.parts.gone]: the design has no part `gone` on this sheet; the entry is ignored
```

Rename `radio` to `rf` in the source and the finding becomes `[Trellis.sheets.radio]: the board has no sheet `radio` (its sheets: `rf`, `stage`); its 1 entry is ignored`; changing the table header to `rf` restores the entry.

## What was rejected

- **Absolute page coordinates.** The page's frame moves when any sheet changes size. An offset from where the engine puts the part survives that, and ADR 0019 already chose it.
- **Keying by reference designator.** `R5` is `R6` after an insert. The path is what ADR 0032 uses for the same reason.
- **Keying by full path with no sheet table** (`[Trellis.parts."stage.filter.l1"]`). Simpler, and a renamed module is one edit per part. Grouping by sheet makes the file read as the drawing is read and a module rename one edit. Costs: one more level, and a part cannot be moved to the wrong sheet without a finding.
- **One file for the PCB and the schematic** (ADR 0030, open question 5). A regeneration of one would touch the other, they are edited by different views, and the schematic's frame has nothing to do with the board's.
- **Inside the source** (a `schematic { }` block, ADR 0003's rule that a position is not circuit), **in the physical file** (ADR 0048: it holds requirements, and a move is no requirement; and it is not read by the circuit's checks), **in `netsmith.toml`** (a project file, not a design's).
- **Stale entries as errors**, as ADR 0032 has them. A rename would break the schematic view. Findings keep them visible.
- **Stale entries silently dropped.** A person who dragged a part and finds it back in its old place after a rename has no way to know why. A finding names the entry and its line.
- **Free floating-point offsets.** Parts and wires sit on the 1.27 mm grid, a hand-written 1.3 would put a pin off it, and an exact integer step count makes the file compare and diff exactly. An off-grid number is refused and the nearest two are named.
- **A `lock` flag as the PCB file has.** The placer there needs to know what to leave alone while it places the rest. Here the engine is a drawing, and `pin` says it by place.
- **Storing wire routes.** Wires are routed again after every move by design (stage 2): a stored route would go stale at every edit and could join what the netlist does not.
- **JSON, or a custom syntax.** TOML is what ADR 0032 uses, reads and diffs well, and the reader is already a dependency of the workspace (`toml`, with its spans for line numbers).

## Cost and left open

- **Two sidecars per source file** now (`.pcb.toml`, `.sch.toml`), three with the physical file (ADR 0048). Each is optional.
- **A rename of a part inside a module** leaves entries with a finding. A rename of a module is one table header. The editor can rewrite the file in the same edit that renames, which is not built.
- **A sheet is a top-level module instance of the board** (`Group::name`). A part of a module nested in a module is keyed by its path from that top-level instance. If sheets ever become nested, the key of a part changes with them.
- **A part with several units** (ADR 0050) is moved as the box of its primary unit; the other units follow, and no unit has a key of its own. If someone wants a unit of its own arrangement, it is a key like `u1#2`, not designed here.
- **A satellite's own entry** is applied after its block moves, relative to the place the block puts it. Stage 2 built it as written (see "Built in stage 2"): the entry is an offset of the satellite's own unit from where its block puts it, and needed no change of the file.
- **The sheets' arrangement on the page** (the grid of boxes of a big board) is not hinted: only what is inside a sheet.
- **The numbers**: the 1.27 mm grid is the schematic's; the largest offset is 5080 mm, which is a typo and not a drawing.
- **Hand edits** are fine, a comment is lost when the editor rewrites the file.

## What the stages build

- **Stage 1 (built, this ADR).** `netsmith_view::hints`: `LayoutHints`, `parse`, `to_toml`, `LayoutHints::resolve`, `unknown_boards`, `schematic_hints_path`, `HintFinding`, `ResolvedHints`, the tests, the glossary entry. No change to the engine.
- **Stage 2 (built, "Built in stage 2" below).** The layout engine takes `&ResolvedHints` (an explicit argument of `layout_sheets_hinted`, empty for none) and applies it after the automatic placement: the block is moved with its satellites, the wires are routed on the moved drawing, `check`'s rules and the metrics hold for every hint, and the drawing with no hints is byte for byte today's. It raises `HintFinding::unusable` for a block that would overlap, and decides what a pin needs of the stable layout's origin.
- **Stage 3.** The server reads `<stem>.sch.toml` beside the top-level file when the page asks for the schematic, calls `resolve` with `module_groups`, passes the result to the drawing and adds the findings to the Problems list (stage `layout`, the file and line as the location), and routes `GET` and `PUT /api/schematic-hints` with the `X-Netsmith` header (ADR 0019) that read and write the file with `to_toml`.
- **Stage 4.** The page: drag a part (its block) on the grid and write `dx`, `dy`; a modifier pins it; rotate and mirror keys; a way to clear a part's entry and a sheet's. Dragging only changes the file; undo is the file's own history.
- **Not built, any stage:** wires, labels, buses or nets in the file; hints on the page's arrangement of sheets; a per-unit key; a tool that renames entries with a part.

## Built in stage 2

The drawing reads the file. What was decided while building it, for the owner to review (the file's format is unchanged).

**The input.** `netsmith_view::layout_sheets_hinted(design, groups, rails, marks, &ResolvedHints, mapping, lib) -> Result<Layout, DesignError>` and `layout_sheets_hinted_findings(...) -> Result<(Layout, Vec<HintFinding>), DesignError>`, which also returns what the engine could not honour (sorted by line, then text). `layout_sheets_marked` is the first with `ResolvedHints::default()`, and with no entry the drawing is byte for byte the one there is without a file: the corpus pictures, the public examples (`schematic_metrics --svg`) and a real 14-sheet board were compared with `diff -r` before and after. `render_layout(design, &layout)` draws a layout already made. The hints are an argument and never ambient state; no environment variable changes a drawing.

**Where each hint is applied** (`layout/hinting.rs`; the engine is `layout/engine.rs`):

- `rotate` and `mirror` replace the automatic orientation of the part's box **before** anything is placed. Each field replaces its own half only (`rotate` alone keeps the automatic mirror); "automatic" is what the engine's first choice is (`orientations`). The engine's own searches skip a part that has one: a pair satellite is tried in that orientation only, a series satellite and a part of a spine are not turned again. A part that is turned **leaves its stack**: the parts of a stack stand one above the other by their pads, which a chosen turn would no longer be (a turned LED and its resistor are two pieces joined by a wire).
- `dx`, `dy` and `pin` carry the **block** after every block has its place in the rows and **before any wire is routed**, so the wires go round the result and nothing is moved after. A block is the main part with the satellites that serve it (and what the spine places with it); for a satellite, or a part that stands alone, it is the unit of parts it stands in (a stack moves as one). A satellite's own entry is applied after its block's (the file's meaning: an offset from where its block puts it). The block keeps its **slot in the rows**: the rest of the sheet is laid out as if the block were where the engine put it, so a hinted block is a fixed piece of the arrangement (and the page shape and the cut into rows are what they were) and nothing else re-flows for it. What does shift is what the move changes in the drawing's own decisions (a net whose wire grew long is named, which takes room).
- `pin = [x, y]` puts the block's origin (the main part's origin) at `x, y` from the sheet's **reference part** where the engine put it, whatever the block's own place is. The frame starts at that place, not at where a moved reference part ends: an offset of the reference part moves it and nothing else. A board drawn as one picture (16 parts or fewer) still has the reference part of the entry's own sheet, as the ADR above says; a sheet cut into numbered boxes has one for each box.
- A note's `dx`, `dy` move its frame after the notes are placed, to a place free of everything else (parts, texts, wires, symbols, the other notes, with the clearance a note keeps); the notes without an entry are placed first, as if there were no file.

**What stays.** The engine decides some things for a sheet by trying it: which nets are drawn as names and which of its few [`Tuning`] choices is kept (the order in which the satellites of a pin are placed, for one). A drawing with the file is made with the choices of the drawing without it, so that a moved part does not change them for the rest of the sheet: the first drawing is the one there is without the file, the second is that drawing with the file's turns, moves and notes applied, and a net is named besides only when it touches a part the file moved or turned and its wire grew long. An entry that is not honoured, or a file of entries none of which is, gives the first drawing, byte for byte.

**Which moves are honoured.** The moves are taken in a fixed order: the pinned ones first, then the offsets, a main part's block before a satellite's, each by the path of its part. A move is **not honoured**, and its block stays where the engine put it, when (a) the box of any part it carries (symbol, pins and texts, with 1.27 mm round it) would lie on the box of another part, one it does not carry and no later move carries (a later move is not placed yet), or (b) it would stand more than 127 mm outside the drawing the engine made. So of two hinted blocks that lie on each other the earlier path wins and the other is a finding; a block may move onto the place another is moved from (two connectors swapped); parts that already lay within the clearance of each other where the engine put them are not counted. The final drawing is checked once more as a whole and the latest move of a pair that lies on each other is refused, until none is. A move that is refused leaves the entry's turn in force.

**Rules the drawing keeps.** `check`'s rules and the symbol overlaps must hold for any file. When a drawing with the file breaks a rule that the drawing without it does not break (for each kind of rule, more of it) or has two parts lie on each other, it is drawn again with the entries in the order of `keys` (the parts by path, then the notes), keeping each one that breaks none; the others are findings (`the drawing with it breaks a rule (wire-through-body); it is ignored`). That is a safety net, not the normal path: it costs one drawing for each entry, and only a drawing that breaks a rule pays it.

**A part with several units** (ADR 0050): the entry moves the **primary** unit's box (the one with most pins) and the other units follow, each with its block, by the same steps (so a `pin` carries them by what it carries the primary box by). `rotate` and `mirror` turn the primary box alone: the other units stay as the library draws them, upright. No unit has a key of its own.

**Findings the engine raises**, all `HintFinding::unusable` (kind `Unusable`, with the entry and its line, ending `it is ignored`): `its block would lie on the part `<path>` at the place the file gives it`; `the part would lie on ...` (a satellite); `its block would stand more than 127 mm outside the drawing`; `the sheet has no reference part to pin it to`; `its block would lie on the part `<path>` once the other entries are applied`; `the note would lie on a part, a wire, a text or another note at the place the file gives it`; `the note would stand more than 127 mm outside the drawing`; `the drawing with it breaks a rule (<kind>)`.

