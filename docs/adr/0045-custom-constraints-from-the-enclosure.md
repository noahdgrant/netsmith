---
status: accepted (user decisions 2026-11-02); C0a, C0b and C0c built, provisional until the user reviews them
---

# Custom constraints from the enclosure

**User decisions (2026-11-02):** requirements on part positions are stated in the board source and the solved position stays in the layout file (this amends ADR 0020 7A and the guide's 'the source holds no positions of parts'); a constraint is a built-in `.place` member on every part and module, written with the ordinary operators; positions are measured from the part's body box (the outline on its fabrication layer); a violated constraint is an error that blocks `fab`, with its own manifest key `constraints = "error" | "warn"` and a `--constraints warn` flag, separate from `guidelines`. A module offers a placeable handle through the tied values of ADR 0042 for now. The remaining questions in this ADR keep the ADR's recommendation unless a later decision says otherwise.

The user's need, in their words: "We will need some way for people to design custom constraints like 'X part must be at Y location on the circuit board to fit in the enclosure'." A board lives inside a case, behind a panel, on a carrier. The case fixes where a connector's mouth, a button, a light pipe, a screw and an antenna window are, and how tall anything may be. Today a person can only put a part where they want it; nothing says why, tolerates a few tenths of a millimetre, or complains when the part moves.

**The user's decisions above are final; C0a and C0b (section 8), C0c, the DXF reader (section 9), and overhang (K2, section 10, provisional, to be reviewed by the maintainer) are built. The rest is the design as proposed, with options and a recommendation for each part, and stages C1 (the edges are built; the module `.place` is checked only as the rest of C0 does), C2 and later are not built.** Snippets marked `nsm` check with today's checker (`netsmith build`); snippets marked `nsm-new` use a construct that does not exist (the first error is at the new word: `Header1x2 has no member place`). Every product, part and number below is invented. A real number comes from the enclosure drawing, and the line that carries it should say which drawing (a comment).

Rules the options are judged against: ADR 0001 (one way to say a thing), statement order never matters, ADR 0004 (a board reaches a module only through its interface), ADR 0005 (only a board names nets), ADR 0020 7A (placement geometry lives in one place, the layout file) and 7C (what the circuit implies is derived), ADR 0030's three tiers and its user decision that the fewer places information lives the better.

## 1. What exists today, and what cannot be said

| Today | What it covers |
|---|---|
| Layout file `<stem>.pcb.toml` (ADR 0032): `[Board.parts."usb"] at, rotation, side, lock` | A part at an absolute pose. With `lock = true` the placer places the rest round it (`PlaceOptions::fixed`, ADR 0033). The editor writes it. |
| `Outline` (width, height, corner radius; ranges allowed), `Cutout` | A rounded rectangle and rounded rectangles cut from it (ADR 0020). Source or layout file. |
| `Keepout` on a board or carried by a part, `.parts = "forbid"` (ADR 0029) | A rectangle no courtyard may touch. The placer keeps out; `check_placement` names the part; KiCad gets a rule area. |
| Mechanical parts (`MountingHole`, `PlatedHole`, `Fiducial`) | In the corners of a fixed outline, in a fixed order, clockwise from the top left. |
| `Board::check_placement`, own DRC `items_not_allowed` (ADR 0037) | Overlap, outside, pad near edge; copper or a part in a rule area. |
| `guidelines = "warn" \| "error"`, `--strict` (ADR 0036) | A way to make a finding stop `fab`. |

A person can therefore write "the USB connector at (2.0, 15.0), turned 270, on top, locked", in a file that is not reviewed with the design. What they cannot say:

- **Why or how closely.** A locked `at` is a decision, not a requirement. There is no tolerance, no name, no source line to read, and nothing is checked afterwards: an unlocked entry is applied after the placer and nothing complains if a drag moves it 3 mm.
- **A position as the enclosure states it.** The layout file's `at` is the footprint's origin, and the enclosure states the connector's body or mouth, flush with an edge, or "centred on the board". An auto-sized board has no right edge to measure from.
- **A relation.** Part to part (12.7 mm pitch, one centreline), part to a feature that is not a part (a light pipe, a button cap), part to a hole.
- **A region for one kind of thing,** or a keep-out for tall parts only. A `Keepout` forbids every part.
- **A height limit.** No part has a height, and there is no height-limited zone.
- **A screw-head clearance** round a mounting hole, or a mounting hole at given coordinates with the right keep-out.
- **An outline from the enclosure.** Only a rounded rectangle is written by hand, and nothing reads a drawing.
- **That two boards mate.** ADR 0040 leaves pin 1's position to "the layout's check" (M2), which does not exist.
- **A result with numbers.** No finding says "measured 3.15, allowed 1.9 to 2.1".

## 2. The constraints real products need

An invented product for the examples: a 62 by 30 mm sensor board in a snap-fit case, with a USB-C receptacle on the left edge, two status LEDs under light pipes, a button, an antenna module at the right edge and four screws. Positions are in the layout file's frame (top left of the outline is the origin, x right, y down, millimetres).

| # | Constraint | Example | Reported as |
|---|---|---|---|
| K1 | **Absolute pose** of a part: position, rotation, side | The USB receptacle's body centre at y = 15.0 (half the height), turned 270, on top | measured against the window per field |
| K2 | **Edge distance, overhang, flush** (built, section 10) | The receptacle's body edge is 0.5 mm past the left edge (so the case's slot lines up); the button 1.5 mm inside the top edge | signed distance from the nearest outline edge; positive is overhang |
| K3 | **Relative to another part or feature** | LED 1 is 8.5 mm right of mounting hole A, and centred (31.25, 15.0) over the light pipe | offset in x and y against the window |
| K4 | **Inside a region** (keep-in) | All three connectors inside a 10 mm bay along the left edge | the box of the part against the rectangle |
| K5 | **Outside a region** (keep-out) | No part in the antenna window; no part taller than 3 mm there | the part and the window it touches |
| K6 | **Alignment and symmetry** | LEDs on a 12.7 mm pitch (a case with two holes cut 12.7 apart), same y; the headers of a row share a centreline | pitch and offset error |
| K7 | **Mounting holes** at coordinates from the case, with a screw-head clearance | Holes at (4, 4), (58, 4), (4, 26), (58, 26); each forbids parts within a 6 mm square on top (a head) | hole position; any part in the clearance |
| K8 | **Height** | At most 3 mm anywhere under the lid ribs; 11 mm elsewhere | the part, its height, the limit; or "unchecked" |
| K9 | **Outline from the enclosure** | The case's inner profile as a DXF: outline and four holes | the import report |
| K10 | **Mating across boards** | The plug-in's header pin 1 lands on the carrier's socket pin 1 within 0.2 mm; the same board's two mounting holes land on the carrier's two standoffs | the transform each mate implies; the difference |

**Tolerance is a range.** The language already has `+/-` and `a..b`. A window of `2.0mm +/- 0.1mm` says the enclosure allows 1.9 to 2.1; the placer aims at the middle of it, and a finding prints the measured value, the window and the margin left (negative is a violation). The measure is exact to half a micrometre, as the DRC is (ADR 0037). The placer's grid is 0.05 mm, so a window narrower than that is an error that says so.

**Height.** A part has no height today. The sources, in the order of ADR 0028 for `assembly`: the picked catalog row's `height` column (a datasheet number, so a row that has it needs a `source`, as every row does); a part that states it for every package (`.height &= 8.5mm`); the board narrowing it on a part it owns. A footprint's 3D model is not read (a STEP reader is a CAD kernel; see section 5). A zone with a height limit applies to the parts whose body is in it: a part there with no height is reported `unchecked` as a **warning that names it**, never as a pass. A board with no height limit reports nothing.

**Outline from the enclosure.** DXF is the practical interchange: every CAD tool exports a section as DXF, and the format is plain text. A minimal reader handles ASCII DXF only (binary is refused with a message), in group-code pairs: `LINE`, `ARC`, `CIRCLE`, `LWPOLYLINE` (closed flag and bulge arcs), the older `POLYLINE` with `VERTEX` and bulge, and `INSERT` of a block with translate, rotate and uniform scale. Units from `$INSUNITS` (millimetre, centimetre, metre, inch); a file with none needs `--units`. Entities are grouped by layer. Segments whose ends are within 0.01 mm join into closed loops; the loop of largest area is the outline, loops inside it are holes. `SPLINE` (control points, non-periodic) and `ELLIPSE` are read as chords within 0.005 mm, which is what the reader does (the user decided on 2026-11-02 to keep it; section 9 below has the detail). 3D entities and text are not read: they are counted and named by layer. A spline with only fit points, or a periodic one, is an error that says to export lines and arcs or a non-periodic spline. The y axis is flipped (DXF is y up) and the bottom left becomes the top left of the board's frame; the report prints that transform, so a hole's position in the enclosure's frame and in the board's differ by a number the person can see. SVG is not planned (px has no unit; curves and transforms need a larger reader); STEP is not read by netsmith.

What the import can express is limited by `Outline`: an outline that is a rectangle with equal rounded corners becomes an `Outline`; a circle, a slot or a rounded rectangle inside it becomes a `Cutout` (a circle is a square with a radius of half its side); circles on a holes layer are listed as hole datums with their diameters. Any other outline is not representable and is reported with its bounding box and vertex count ("outline of 14 segments needs a polygon outline, ADR 0020, not built"): that is step C6.

## 3. Where a constraint lives

The constraint is a **design requirement**, not knowledge of a part: it is true of this product's case, not of the USB connector, so it is not a `layout` block in the part (ADR 0030 tier 2, whose test "does it hold in every design" fails). It is not a decision either: a decision is where the person put the part; a requirement is where the case says it may be. The solved position is a decision, or a derived result, and stays in the layout file.

**Option A: a requirement in the board source, the solution in the layout file (recommended).** The part and module get a built-in member `.place` that the board narrows, as `.assembly` is narrowed today (the same "every part has it, declaring it again is an error" rule). Its fields are all ranges, and unset means unconstrained:

| Field | Meaning |
|---|---|
| `.x`, `.y` | the centre of the part's body, in the board's frame |
| `.left`, `.right`, `.top`, `.bottom` | the edges of the body's box (after the turn) |
| `.rotation` | degrees counterclockwise on screen, as in the layout file; a `Number`; a multiple of 90 |
| `.side` | `"top"` or `"bottom"`, as in the layout file |

The **body** is the footprint's `F.Fab` outline if it has one, else its courtyard; the finding says which it used. Why the body and not the footprint origin: the case states the receptacle's shell, not the origin the library author picked, and a constraint should survive a change of package. The layout file's `at` stays the origin (one frame for the placement and the CPL file); the check converts and shows both. Because every field is a quantity, tolerance (`+/-`, `..`), expressions and other board members (`.outline.height / 2`) work as they do everywhere, and statement order does not matter. One limit of today's language shows: `+/-` needs a constant on both sides (`.a.y &= .outline.height / 2 +/- 0.1mm` is refused with ``error: a tolerance needs a fixed value and a fixed tolerance``), so a window round an expression is two relations, `>=` and `<=`, as above. A window round a constant is `&=` with `+/-` or `..`.

```nsm-new
board Widget {
    .outline: Outline {
        .width &= 62mm
        .height &= 30mm
    }
    .usb: UsbCReceptacle
    // Case drawing, slot A: the shell is centred and its mouth sticks out 0.5 mm.
    .usb.place.y        >= .outline.height / 2 - 0.1mm
    .usb.place.y        <= .outline.height / 2 + 0.1mm
    .usb.place.left     &= -0.5mm +/- 0.05mm
    .usb.place.rotation &= 270
    .usb.place.side     &= "top"
}
```

The line `.j1.place.x &= 2mm +/- 0.1mm` parses today (checked with `cargo run -p netsmith-lang --example parse`); the checker refuses it with ``error: `Header1x2` has no member `place` ``. A datum that is not a part needs nothing new, since a board may declare its own interface and members (this checks today):

```nsm
import std::pcb::Outline

interface Point {
    .x: Length
    .y: Length
}

board Widget {
    .outline: Outline {
        .width &= 62mm
        .height &= 30mm
    }
    /// The light pipe's centre, case drawing, view A.
    .pipe: Point {
        .x &= 31.25mm
        .y &= outline.height / 2
    }
}
```

**Relations between two placements** are written as the language writes any relation, linear in the fields (a sum or difference of two fields against a range). They are not solved by the solver (placement is not a solver output; on fields nothing sets, today's checker accepts them and prints `note: not evaluated: (b.y - a.y) == 12.6mm..12.8mm`, which was run on declared `Length` members); the lowering keeps them as `Offset { a, b, axis, window }` for the placer and the check. `nsm-new`:

```nsm-new
    .led1.place.x  >= .pipe.x - 0.15mm
    .led1.place.x  <= .pipe.x + 0.15mm
    .led1.place.y  >= .pipe.y - 0.15mm
    .led1.place.y  <= .pipe.y + 0.15mm
    .led2.place.x - .led1.place.x == 12.7mm +/- 0.1mm
    .led2.place.y == .led1.place.y
    .hole_a.place.x &= 4mm
    .led1.place.x - .hole_a.place.x == 27.25mm +/- 0.2mm
```

Distance as a radius is not linear and is not offered; the case drawing gives coordinates, and a circle "within 8.5 mm" is a keep-in window in x and y. Order in the file does not matter; an empty intersection (two lines that cannot both hold) is the usual error naming both lines.

**Keep-in** is a window: `.j1.place.left >= bay.x`, `.j1.place.right <= bay.x + bay.width` with a `Rect` datum. It is wordy for several parts; this is the cost of not giving a statement a list of parts (the language has no handle for a part). **Keep-out** is `Keepout`, extended (section 2, K5, K8) with `.max_height`: parts taller than it may not have their body in the rectangle, on the faces in `.layers`. **Mounting holes** get `.head_diameter` (a design choice, the screw; the hole then carries a round keep-out for parts, as a square of that side) and are pinned by `.place.x`, `.place.y` like any part.

**Encapsulation.** A module's parts are private. A module that wants a part placed by the board declares a pose of its own and ties the part to it, the way ADR 0042 does for `assembly`. This works with no new syntax once `.place` exists:

```nsm-new
module UsbBlock {
    .shell_y: Length
    .shell_left: Length
    .j: UsbCReceptacle
    .j.place.y    &= .shell_y
    .j.place.left &= .shell_left
}
// board: .usb.shell_y &= outline.height / 2 +/- 0.1mm
```

Four to eight such ties are tedious. A `Pose` interface in `std::pcb` joined with `~` (the module writes `.j.place ~ .usb_pose`, the board narrows `.usb_pose.y`) would be one line; that is a language change and is question 5. A module also has a `.place` of its own: the box of all the parts inside it, so the board may say "the whole sensor block inside this region" (`.block.place.left >= bay.x`) with nothing exposed. It supports the box edges only (not `.rotation` or `.side`: the placer does not turn a module as a block).

**Option B: layout file only.** Add `[Widget.constraints.usb]` to `<stem>.pcb.toml` with `x = [1.9, 2.1]` and so on. No language change; the editor already writes the file; the file already reaches inside modules by path, so no exposure is needed. Against: the file is a record of decisions and would hold requirements (ADR 0030 tier 3 says "decisions of this design"); a relation, an expression such as half the height, or a link to the outline cannot be written in TOML without a second expression language; the requirement and its solution are in one file and an editor drag can silently rewrite the requirement; the reviewer of a design change does not see the case's numbers beside the circuit; and a requirement, unlike a datasheet fact, is checked and versioned with the design. Two places for the same fact (A and B) is rejected under ADR 0001.

**Option C: a `constraint` statement** (`constrain .usb at x 2mm y 15mm`). It adds a verb and a mini-language beside the quantity algebra the language already has; order, ranges, tolerances and expressions would all be re-specified. Rejected: Option A uses what exists.

**Recommendation: A.** One place for the requirement (the board source), one for the solution (the layout file), as with a keepout whose size is in the source and whose place is in the file (ADR 0029). It amends ADR 0020 7A and the sentence in `docs/LANGUAGE.md` "The source holds no positions of parts": the source holds **requirements on** positions (ranges), never a solved position. ADR 0020 7C stands: nothing says `near`; what the circuit implies stays derived. Exactness: a window narrower than the grid, or a point, is a pinned part: the layout file has no `at` for that axis to hand-edit (an entry that disagrees is a finding, not a second truth). A layout entry inside the window is the solution and is fine, as a keepout's `at` is.

## 4. How they are used

**Lowering.** The checker carries the narrowed `.place` fields (and `.height`) into `Design` as a list of constraints, each named by the source path of the statement (`usb.place.left`). Narrowing that empties a range is an error at `build`, naming both lines. A constraint that needs a fixed outline (`.left` from the left edge, `.y` as half the height) on a board whose `.outline` states a range or none is an error: "usb.place.left is measured from the outline's top left corner, and the outline is not fixed".

**The placer** (ADR 0033, `anneal.rs`, `PlaceOptions`) treats them as hard. A part whose window is a point on both axes, and whose turn and side are given, is `PlaceOptions::fixed` (the same path as `lock = true`, with the body-to-origin offset applied). A part with windows gets a domain: the start state is projected into it, a shift move is clamped to it, and the terms of the cost outside it count with the weight of the walls (200). Relations (K3, K6) are a cost term of the same weight, and parts joined by an exact relation (within the grid) move as one rigid cluster, like a module instance already does. Windows beat cohesion and wire length; if no state satisfies all, the placer returns the least violation and the check says which constraints. The layout file's `lock` and `at` still win over the placer, and the check then measures them (a hand-placed part outside its window is a finding).

**The check** is a new `netsmith-pcb::constraints` module: `Board::check_constraints(&[Constraint]) -> Vec<Finding>`, run by `fab`, the editor and `layout check`. A finding is data (the record of ADR 0030 (d)); the text names every number:

```text
error: constraint usb.place.left (widget.nsm:14) is violated: the body of J1 (F.Fab) is 0.18 mm from the left edge, inside it; the constraint allows -0.55 mm to -0.45 mm (0.63 mm over)
warning: height of J3 is unchecked: the lid zone `ribs` limits parts to 3 mm and J3 has no height (set it in its part, its catalog row, or on the board)
```

```json
{ "kind": "constraint", "id": "usb.place.left", "severity": "error",
  "parts": ["J1"], "measured": 0.18, "allowed": [-0.55, -0.45], "over": 0.63,
  "body": "F.Fab", "at": [0.0, 15.0], "source": "widget.nsm:14" }
```

A mechanical constraint is a requirement the user wrote, so by ADR 0030 decision 3 it is **not a guideline**: it blocks `fab` by default, like a DRC error, with a key and a flag. Recommended: `constraints = "error"` (default) or `"warn"` under `[project]` in `netsmith.toml`, and `--constraints warn` for one run. `--strict` and `guidelines` do not govern it. There is no waiver (`--allow-drc-violations` does not waive it either; see section 8): to relax a requirement the person edits the range, which is in the diff. An `unchecked` height is always a warning (a missing fact is not a violation). `build` checks only what needs no placement (an empty intersection, a constraint with no fixed outline, an unknown name); satisfaction is measured where a placement exists.

**The editor** (ADR 0030 (e), ADR 0019) shows each constraint on the PCB view and the findings tab lists them with the usual click to zoom:

- a **ghost** (dashed body box) where a point constraint puts the part, a **window** (a translucent rectangle from the x and y ranges) round a ranged one, a **keep-in** as a dashed outline with the part names, a **keep-out** as the existing dashed red box, and a height zone as a tinted box titled "max 3 mm";
- a relation as a thin line between the two parts with the offset and its window;
- **drag snaps into the window** and refuses to leave it: past the edge the part stays at the limit and a note says "limited by usb.place.left (widget.nsm:14)". A part with a point constraint cannot be dragged on that axis. A drag that is allowed writes the layout file as today (ADR 0032).

**Editing a constraint from the editor.** The editor already edits the `.nsm` text in its source pane, so a constraint is a line of source and not a second store. The safe form is a formatter-stable line, one field per statement, in one shape (`.path.place.field &= value`). "Pin here" inserts or replaces **only** the statements it owns, found by their parsed span (the parser has spans for errors), writes the new line in canonical form (`netsmith_lang`'s `to_source` of the statement, not of the file), leaves every comment and other line alone, and appears in the editor's undo like a text edit. An edit of a window's bounds writes the right-hand side only. Statements the editor did not generate (relations, expressions of other members) are shown and edited as text. This is C3 and is optional: a person or an agent can always write the line.

## 5. Agents

The repository is agent-neutral (ADR 0030 (d)): commands with JSON, files, and a skill. For an enclosure the loop is:

1. **Read the enclosure.** `netsmith enclosure read <file.dxf> [--units mm] [--layer NAME] --json` lists, per layer, closed loops (area, bounding box, vertex count, whether a rounded rectangle), circles (centre, diameter), and what was skipped (count by entity type), and prints the transform to the board's frame. `--emit` prints the `Outline`, `Cutout` and datum lines (as `nsm`) for the agent to paste. It writes no source file: the agent edits text, one way to say a thing, as it does the layout file.
2. **A STEP model or a PDF drawing** is read by tools the agent already has outside netsmith: a CAD scripting library or FreeCAD section to a DXF at the board plane, a script that prints the bounding boxes and hole centres of the named solids, or reading the dimensions from the drawing. netsmith does not read STEP; the agent turns it into a DXF or a table of numbers and then states the constraints. A number the agent cannot find stays out of the source: an unstated constraint is better than an invented one, and each line names its source in a comment.
3. **Write the constraints** as source lines (section 3) and run `netsmith build`, then `netsmith fab` or `netsmith layout check <board> --json`, which prints the findings above plus the placement and DRC ones, each with `kind`, `id`, `measured`, `allowed`, `over`, `source`, and an exit code that is 0 only if no `error` remains.
4. **Read the failures.** A finding names the line to change. The agent changes the range (a requirement it misread) or the circuit, never the layout file's number to hide a failure: the check measures the layout file too.

A skill, `netsmith-use-constraints`, is written with C0 and copied by `netsmith init` (ADR 0015): the loop, how to read a finding, what to tell the user (constraints written, from which drawing, heights unchecked).

## 6. Staged plan

| Step | What | Size |
|---|---|---|
| **C0a** | `.place` members `x`, `y`, `rotation`, `side` on parts (the `assembly` pattern), narrowing and the lowering to `Design`; `build` errors; `check_constraints` for them with the finding text; the `constraints` key and flag | M |
| **C0b** | Placer: fixed from a point window, domains for windows, keep-in as two windows; the layout file keeps the solution; test that the same inputs give the same board | M |
| **C0c** | DXF reader (`LINE`, `ARC`, `CIRCLE`, `LWPOLYLINE`, `POLYLINE`, `INSERT`, `$INSUNITS`), `netsmith enclosure read`, `--emit` for `Outline`, `Cutout` and datums | M (built as `netsmith dxf import`, see Built: C0 piece 3) |
| C1 | Body edges (`left`, `right`, `top`, `bottom`), overhang and flush, module `.place` box; the body box from `F.Fab` | M |
| C2 | Relations between placements: offsets, alignment, pitch; rigid clusters in the placer | L |
| C3 | `Keepout.max_height`, part `.height` from a catalog column, `unchecked` warnings; `MountingHole.head_diameter` | M |
| C4 | Editor overlays (ghost, window, keep-in, relations); drag clamp | M |
| C5 | Editor "pin here" writes the source line | M |
| C6 | `netsmith layout check --json` for constraints, the skill, `enclosure read --json` | S |
| C7 | Mating across boards: each mate implies the plug-in's pose in the carrier's frame; two mates between the same boards must agree (depends on ADR 0040 M2 and both layout files) | M |
| C8 | Polygon outline (import of any closed loop of lines and arcs, router, placer, `invalid_outline`) | L |

C0a to C0c together are the smallest useful step: a part pinned or windowed by a stated requirement, checked with numbers, and an outline and hole positions read from the case's DXF. They are independent in the order a, then b and c in parallel. Not planned: STEP and SVG reading, a 3D model height, a non-right-angle rotation, a waiver.

**Risks.**

- **The body box is only as good as the footprint's `F.Fab`.** Library footprints differ; a connector whose shell is not drawn there is measured by its courtyard, which is larger. The finding says which it used, and a constraint on `.x` or `.y` (the centre) is less sensitive than one on an edge.
- **Frame mistakes** (DXF y up, inches, an offset origin) put every number wrong by the same amount. The import prints the transform and the editor's overlay lets a person see a hole land on the case's outline.
- **Windows against the placer's other goals.** Hard windows may make a board that does not route; the placer reports the least violation and routing is unaffected, but a tight window near a dense module can cost board area.
- **Amending ADR 0020 7A and the language guide's "no positions in the source"** needs the user's approval; the amendment is that requirements on positions are source, solved positions are not.
- **Relations are not solved by the solver,** so a mistaken relation between a field that is also set elsewhere shows at placement, not at `build`. Only constant ranges are checked at `build`.
- **A height-less library.** Until catalog rows carry `height`, most height checks report `unchecked`. That is honest, and adding a column with a source is the way to fix it (ADR 0039's refresh could fill it if JLCPCB's record has it; unverified).
- **The grid** is 0.05 mm; a window of less is refused by name.

## 7. Questions for the user, ranked

1. **Do requirements on positions belong in the board source (amending ADR 0020 7A), with the solution in the layout file?** Recommend yes (Option A). The alternative is the layout file only (Option B): no language change, but no relations, no expressions and a requirement beside the thing it constrains.
2. **Is the way to say it the built-in `.place` member, narrowed with `&=` and ordinary relations?** Recommend yes (nothing else to learn, order irrelevant). The alternative is a `constrain` statement (Option C).
3. **Is the reference the part's body box (centre and edges), with the layout file keeping the origin?** Recommend the body. The alternative is the footprint origin: no `F.Fab` dependency, but every constraint changes with the package.
4. **Does a violated mechanical constraint block `fab` by default,** under its own key `constraints` and flag `--constraints warn`, and not under `guidelines`? Recommend yes.
5. **How does a module offer a placeable handle?** Recommend ADR 0042's tied values now and a `Pose` interface joined with `~` later (a language change, yours to decide); and every module's own `.place` box with nothing exposed.
6. **Part heights from a catalog column and a part field,** with `unchecked` as a warning. Recommend yes; or is a 3D-model bounding box wanted enough to ask for a STEP reader (large)?
7. **Rotation as a `Number` of degrees** (multiples of 90 only), or a unit `deg` in the language? Recommend `Number`.
8. **May the editor write constraint lines into the source (C5),** or only read them and let the person or an agent write? Recommend read-only first, "pin here" later.
9. **Order after C0:** relations (C2) before heights (C3), or the reverse? Recommend relations: a case drawing gives offsets before it gives heights.

## 8. Built: C0a and C0b (the requirements)

- **Language.** `.place` is a built-in interface member of every part and module (`Place`: `x`, `y`, `left`, `right`, `top`, `bottom`, `rotation`, and `side` as a `String`); a module's has no `rotation` or `side`. The edges `left`, `right`, `top` and `bottom` were built with C0a, ahead of C1, because the case states them. The solver reads the bounds `>=` and `<=` on a `.place` field as windows (`netsmith-solve::place`); an offer a module declares and the board leaves alone is no note. Nothing new in the syntax.
- **Build errors.** Bounds that leave an empty window (naming both lines and both numbers), a rotation with no multiple of 90, a side that is not `"top"` or `"bottom"`, and a requirement whose bound uses `outline.width` or `outline.height` when the outline does not state that size as one value (the message names the member and the requirement). The first two are the solver's and lowering's; all stop `build`.
- **Lowering.** `Design::features::constraints` holds a `Placement` per part or module that has a requirement (`path`, `module`, bounds in millimetres or degrees, side), with the source lines kept in `Lowered::constraint_sites` and put in the design by the caller that has the files (`name_sources`).
- **Placer.** `netsmith-pcb::constraints::plan` works, for each constrained part and each of its four turns, where the footprint's origin may be; a part left with one turn and one place is fixed exactly as `lock = true`; the others get a `Want` that the annealer keeps (the start state is projected, a move is clamped, and the walls do not apply along an axis the design holds). A part's face is the requirement's, else the layout file's. A module's `left`/`top` (at least) and `right`/`bottom` (at most) hold for each of its parts. It reports, by name and with the numbers, a window outside a fixed outline, two decided parts whose courtyards overlap, no turn that meets the rotation, and a layout file lock that breaks a requirement. The same inputs give the same board. The layout file is unchanged: `lock` and `at` still win, and the check measures them.
- **Check.** One measurement, as a design rules rule `constraint` on the board file (`netsmith-fab::drc::constraints`): the body is `F.Fab` (`B.Fab` on the back), else the courtyard, else the pads, and the finding says which. It is what `fab` and the editor's findings list use, so there is no second implementation in `netsmith-pcb`: the ADR's `Board::check_constraints` is `netsmith::constraints::of_design` plus `Constraints::measure`. The tolerance is half a micrometre, as for the rest of the check. An unmet requirement is an error (a warning with `constraints = "warn"`); `--allow-drc-violations` does **not** let it through (user decision, 2026-11-02): `package` refuses with `Error::Constraints`, which says that only `constraints = "warn"` or `--constraints warn` relaxes it.
- **Command line and manifest.** `constraints = "error" | "warn"` in `[project]` and `fab --constraints warn|error` (for one run, over the manifest's key; `build` measures nothing). `guidelines` and `--strict` do not govern it. The notes of a package say `constraints: N of M satisfied`, and when some are not met (only possible with `warn`), how many and that they are warnings.
- **Editor.** The findings tab lists the unmet requirements, from the placed board, with the rest of the placement's. The overlays and the drag limit (C4) are not built.
- **Decided by the user after the build (2026-11-02):** `fab --constraints error` and `warn` both exist and win over the manifest; `--allow-drc-violations` never waives a requirement.
- **Questions left to the user** (the implementation chose; each is easy to change): the spelling of the fields as `String` for `side`; and whether `x`/`y` on a module (its box centre) should be accepted (it is, and measured on the box round all its parts).

## 9. Built: C0 piece 3 (the DXF reader, C0c)

The outline from the enclosure is built, as the command `netsmith dxf import <file.dxf> [--layer NAME]... [--holes-layer NAME]... [--units mm|cm|m|inch] [--board NAME] [--write <file.nsm>]`, not `netsmith enclosure read`. The reader is its own crate, `netsmith-dxf` (no dependencies, so the board crate stays light); `crates/netsmith/src/dxf.rs` translates what it finds into source.

**What differs from section 2 above.** Entities join into loops when their ends are within 0.001 mm (not 0.01): a gap is an error that lists each open end, its position and the distance to the nearest other open end. The outline is the one loop that is not inside another, not the loop of largest area: two such loops are an error naming both with their size, a loop that crosses or touches itself or another is an error with its position, and a loop inside a cutout is an error. `SPLINE` (control points, non-periodic) and `ELLIPSE` are read as chords within 0.005 mm, not refused (the user decided on 2026-11-02 that the reader keeps doing this). Also read: `INSERT` with mirroring and `MINSERT` arrays, a downward extrusion direction (a mirror), duplicate edges (counted once, with a warning). Paper space entities, text, dimensions and hatches are skipped; the ones on a used layer are counted in a warning. `$INSUNITS` 1, 4, 5 and 6 are inch, millimetre, centimetre and metre; a file with 0 or none needs `--units`, and `--units` overrules the file with a warning if they differ. Layers are chosen by name, ignoring case; with no `--layer` the one layer that has lines, arcs, circles or polylines is used and, if there are several, the error lists them with their entity counts. A circle on the outline layer becomes a round `Cutout`; on the holes layer (circles, or a closed polyline that is a circle) it is a mounting hole.

**What the language can say, and the decision on polygons.** A polygon outline touches the board model's `outline: Rect` in the placer's fit test, the router, the DRC edge distances, the Gerber and `Edge.Cuts` writers, the stitching and pours, the editor's drawing and the layout file, so it was not done here: it stays step C8. The importer says exactly what the board has: an outline that is a rectangle with equal rounded corners becomes an `Outline`; each loop inside it that is a rounded rectangle, a slot (radius half the short side) or a circle (a square with a radius of half its side) becomes a `Cutout`; any other outline or cutout is refused, naming its size and its count of edges ("the outline is not a rectangle with equal rounded corners (40 x 30 mm, 6 edges): it needs a polygon outline ... step C8"), and nothing is printed. A rectangle is recognised by measuring, not by counting edges: every edge must lie on the boundary of the fitted rounded rectangle to 2 micrometres and the edges must add up to its perimeter. Holes become `MountingHole` parts of the package whose drill is within 0.05 mm (`M2` 2.2 mm, `M3` 3.2 mm, from `std/kicad.toml`); any other diameter is refused with the packages.

**Frame.** The board's origin is the outline's top left corner, y down; the output prints the drawing point that is the origin and the formula, as the section above asked, so a hole's coordinates in the case's frame and the board's differ by a number the person can see. The positions of the cutouts and holes are not in the source (ADR 0032): `--write <file.nsm>` merges them into the board's table of `<stem>.pcb.toml` (cutouts by `at`, the corner nearest the origin; holes as parts `hole1`... with `lock = true` and `source = "hand"`, so the placer leaves them where the case has them) and prints a line diff of the file. The source is never written. The file is written as `Layout::to_text` writes it, so comments in it are lost, which the diff shows and the command says. Entries are named `cutout1...` and `hole1...` in order of position (top to bottom, then left to right).

**Tested.** `crates/netsmith-dxf/tests/read.rs` reads hand-written files: a rectangle, a rounded rectangle of lines and arcs, an L, a rounded cutout with holes, a bulged `LWPOLYLINE`, an R12 `POLYLINE`, a rotated and scaled block insert, a mirrored insert, a downward extrusion, inches, centimetres, metres, no units, gaps, two outer loops, a bow tie, a cutout that touches the edge, an island, binary DXF, a spline and an ellipse against the exact curves, and the layer choice errors. `crates/netsmith/tests/dxf.rs` imports a rounded 50 x 36 mm case with a slot and four holes, puts it in the outline-and-pours board, runs `fab` with the layout file written by `--write`, and checks that the holes are where the case has them, that netsmith's design rules check passes, and that KiCad's check and plot agree (skipped when KiCad is not installed).

**Not done.** Polygon outlines (C8). Datums other than mounting holes (a `PlatedHole`, fiducials, the screw-head clearance of K7) and `--json` output. A person who wants a hole of another size adds the package to `std/kicad.toml` first. Two things seen while testing, not caused by the import: holes close to the top and bottom edges put their reference designators near the edge, which KiCad's `silk_edge_clearance` warns about and netsmith's check does not, and the placer, given holes locked 6 mm in from the edges of this small board, left two headers overlapping them (it reports that; it is the placer's matter, ADR 0033).

## 10. Built: overhang (K2, PROVISIONAL, to be reviewed by the maintainer)

K2 asks for a body that sticks out of the board ("the receptacle's body edge is 0.5 mm past the left edge"). A real design has a plug whose courtyard is mostly off the board, and netsmith could not place it: the rows pre-check refused a part wider than the outline, a part that stays counted as its whole courtyard and margin, and `check_placement` reported `OutsideOutline` for any courtyard outside, which stopped `fab`. This section is the rule that was built. **Nobody has approved it**: it was decided while the user was away, with no new syntax, and it is easy to change in one place (`netsmith-pcb::overhang` and `constraints::stated_edges`).

**The rule, exactly.**

1. *Declared.* A part may hang over an edge of the outline only if the person said so, in one of two ways. (a) A requirement on the part's own `.place.left`, `.right`, `.top` or `.bottom` whose whole window (the intersection of its bounds) is at or beyond the same edge of the outline: `left` or `top` with an upper bound of at most 0 mm, `right` with a lower bound of at least the outline's width, `bottom` with a lower bound of at least its height. Flush counts (a window that starts exactly on the edge). The outline must state that size as one value. A window on `.x` or `.y`, a bound a module states for its parts, and a window that reaches inside the board do not say it. (b) The layout file locks the part (`lock = true`): every edge its courtyard is then past is allowed, because the lock is the person saying where it is. A moved but unlocked entry is not a statement; it is an undeclared overhang as before.
2. *The fit.* A part that stays (a lock, or a requirement that leaves one place) counts in the placer's fit test as its courtyard clipped to the outline, with the 2.0 mm margin kept on the sides that are on the board and none on a side it hangs over (`overhang::needs`). A part that is placed (a window) keeps its pads, not its courtyard, inside: the annealer adds a hard cost for any pad corner nearer the outline than `copper_to_edge`, and does not apply the margin walls along the axes the requirement holds, as before.
3. *The check* (`Board::check_placement`). For a part that may hang over an edge: no `OutsideOutline` for the courtyard past that edge; overlap with other courtyards, keepouts and cutouts is measured on the courtyard clipped to the outline; **a pad outside the outline or nearer than `copper_to_edge` is still `PadNearEdge`** (its message says "mm outside the board edge" when it is outside); **a courtyard past an edge nobody stated is still `OutsideOutline`**, and its finding now names the part, each edge and how far (`J1 is not wholly inside the board outline: its courtyard is 4.00 mm past the right edge, and the design says it may hang over only the bottom edge (`plug.place.bottom (widget.nsm:99, widget.nsm:100)` allows 74.5 to 75.5 mm); to allow it past the right edge, require the body to reach it, for example `.plug.place.right >= outline.width``). A corner of the clipped courtyard that is outside a rounded corner of the outline is still reported, except on an edge it may hang over. `Board::hangs()` lists what is past a stated edge; `fab` prints each (`overhang: J1 hangs over the bottom edge: its courtyard is 15.50 mm past it (...)`) and writes it to `fab-notes.txt`.
3a. *The body's centre (PROVISIONAL, to be reviewed by the maintainer).* The placer used to refuse a requirement whose window leaves the body's centre wholly outside a fixed outline ("is required to be where its body's centre is ... and the outline is ... mm"), which made a plug whose body is mostly off the board unplaceable even when its edge field stated the overhang. The refusal is now skipped along an axis, in a direction, only for a part whose own `.left`, `.right`, `.top` or `.bottom` requirement states the overhang on that edge (`stated_edges`); the pads still have to be inside, which the overhang rule above demands. `.x` and `.y` windows, and a centre that leaves by an edge the part's edge fields do not state, keep the old refusal. Tested with a 40 mm body whose centre is 5 mm below a 60 mm board.
4. *Not checked.* The body (`F.Fab`) is not required to stay inside. The part of the courtyard outside the outline, its silkscreen and its text are not measured (the design rules check's `silk_edge_clearance` stays a warning, as in KiCad). How far a part hangs is whatever its window says; there is no other limit. A tolerance of the window is the only margin.
5. *Two stages in the placer, not a veto.* The rows arrangement no longer decides that the parts do not fit: it is the annealer's start state. With a size to fit, if the rows are too big the placer first runs the annealer on the faces the parts are on (unless the courtyards' area alone is more than the area inside the margin of a face, which is a certain no), then shares the free parts between the faces, largest first, and runs it again. The parts do not fit only if the last run ends with courtyards over each other or parts outside, and the message keeps the old sentence (what the rows need, what the outline is, how far over) and adds what the annealer ended with: `placing them in the outline anyway ended with 85 pair(s) of courtyards overlapping, 52 part(s) outside the outline or inside its 2 mm margin` (the margin was 2.0 mm when this was written; it is now the process's, ADR 0033). When the rows fit, nothing changes (the rows are still the fallback if the annealer finds no clear arrangement). The same input still gives the same board. *Added (provisional):* the message lists the ten pairs of courtyards that overlap the most, by name, with the shared area, its size, its place from the outline's top left corner and the nearest side, and the ten parts furthest outside (the edge and how far, or the margin, keepout or cutout), and names the remaining ones, so that the full list is in the one message `fab` prints and the editor shows; the board that did not fit is not built, so it has no findings of the placement check, and there is no `--json` for `fab` yet.

**Measured** (debug build, release in the roadmap): the end-to-end board of `crates/netsmith/tests/overhang.rs`, 35 x 60 mm, an invented 42 x 28 mm plug 15 mm out of the bottom edge and 30 small parts, places, routes and passes the design rules check and KiCad's in about half a second; 100 small parts in 35 x 60 mm place in about three seconds (debug). A part wider than the board is turned by the annealer; the rows, which never turn one, used to refuse it before it ran.

**Open (the user's to decide).** (a) Whether a window on `.x` or `.y` should state an overhang (now it does not: only the edge fields do). (b) Whether a locked part should state every edge it is past (now it does). (c) The courtyard wall, which was a fixed 2.0 mm and is now derived from the process (ADR 0033, provisional): a stated overhang keeps it on the other sides. (d) The hole-part two-face rule changed (ADR 0031, provisional): a footprint that draws its courtyard on one face's layer only is judged on the other face by its pads; the courtyard rules are otherwise unchanged. (e) Whether `fab` should list a stated overhang as a warning, not a note. (f) The limit on how far a part may hang. (g) Silkscreen over the edge stays a warning. (h) A module's `.place` does not state an overhang for its parts. (i) Two parts that hang over the same edge may overlap beyond it in the check, but the annealer still keeps their whole courtyards apart (stricter than the check).

## 11. Height limits (C3 and C4, PROVISIONAL, to be reviewed by the maintainer)

A board that fits in plan view can still hit a lid, a bracket or a neighbouring board. This section says where a part's height comes from, how a limit is stated, how the two faces work and what a violation reports. **Nobody has approved any of it.** It was written for GitHub issue 1 with no owner present, with the smallest syntax that works (one new field on a part, one on `Keepout`), and each choice is easy to change in one place. The examples are invented.

**Where a part's height comes from.**

1. *Stated.* A part has a built-in member `.body_height` (a `Length`, the height of the body above the face of the board it is on, in the same way `.assembly` is built in). A part kind, a user part or the board states it (`.sensor.body_height &= 8.5mm`). It is one value; a range is an error; a negative value is an error. Zero is allowed, and is a statement ("flat"), not a default.
2. *Not stated: unknown.* A part with no `.body_height` has **no height**, not a height of zero and not a guess. Where a limit applies to it, it is reported by name as `unchecked` (below).
3. *Not read:* the footprint's 3D model (the issue and section 2 mention it). Section 2 and question 6 already recommend against a STEP reader, and a KiCad footprint has no height field of its own, so there is nothing else in the footprint to read. Not built: a catalog column `height` (section 2's first source) because the catalogs and the solver are another change's; until it exists, a height is stated in the source.
4. The spelling is `.body_height`, not the `.height` that section 2 wrote, because a `Label` already has a `.height` (the letters' height) and a `Keepout` has the size `.height` in plan view. PROVISIONAL: the name.

**How a limit is stated.** `Keepout` gets `.max_height` (a `Length`, one value above zero): the tallest a part may stand where the keepout is. A keepout that states it and none of `.tracks`, `.vias`, `.copper` and `.parts` forbids nothing else (a height zone is not a keepout for parts). If it states one of those, that one still forbids. A keepout that states neither is still the error it was ("forbids nothing"). The keepout's place is as before: `.x` and `.y` in the source, or an entry of the layout file. A region of the outline that carries a limit (the lid's ribs, a bracket) is a `Keepout` with `.max_height` over that region; `netsmith dxf import` does **not** yet read a limit from the drawing (a DXF has no height on a 2D loop, and reading a 3D entity or a layer's name as a limit would be a guess); the importer's output is pasted next to the keepouts a person writes. PROVISIONAL: that a limit is a property of `Keepout` and not a new construct.

**Both faces.** A keepout's `.layers` names the faces it limits: `top` for the parts on the top face, `bottom` for those on the bottom, `all` (the default) for both. A back-face part has its own limit: two keepouts over the same rectangle, one with `.layers &= "top"` and `.max_height &= 3mm`, the other with `.layers &= "bottom"` and `.max_height &= 6mm`. A limit on inner layers only limits no face and is an error. A through-hole part is measured on the face it is placed on, by its `.body_height`; the part of it below the board is not limited. PROVISIONAL: that `.layers` names the faces, and that a part is counted on the face it is placed on.

**What a violation reports, and when.** `Board::check_placement` reports, for each part with a known height, each keepout that limits height on the part's face whose rectangle the part's body overlaps (the footprint's fabrication outline, else its courtyard: the same body as the constraints of section 8), when the height is more than the limit by over half a micrometre. The finding is a `TooTall` placement problem, so it stops `fab` like any other (`fab` refuses; the editor's findings list shows it, rule `too_tall`), and it says the part by reference designator and path, its height, the keepout and its limit, the face, how much too tall, and where the body is, in mm from the board's top left corner:

```
C3 (`stage.output_cap`) stands 9.50 mm tall on the top face, and the keepout `lid_ribs` allows 3.00 mm there (6.50 mm too tall), where its body is, around (31.2, 15.0) mm from the board's top left corner
```

A part with no height that stands under a limit is reported by `Board::heights_unchecked`, never as a pass: `fab` prints a warning for each and writes it to `fab-notes.txt`, and the editor lists it as a warning (rule `height_unchecked`). It does not stop `fab`: a missing fact is not a violation (section 2). A label is left out (it has no body). A board with no height limit reports nothing. A keepout a part carries is checked like any other; the placer does not avoid it (below).

**The placer.** The annealer keeps a part with a known height out of every zone whose limit it is over, on the face the part is on, with the same hard cost as a keepout for parts, and measured on the courtyard (larger than or equal to the body, so a clear result passes the check). A part with an unknown height is not kept out (nobody knows whether it needs to be). A zone is read from the source or the layout file as the keepouts are; one a part carries is not avoided. The face-sharing step of the fit (which splits parts between the faces by area) does not know heights, so a tall part may be put on a face by area and then kept clear of the zone there; if it cannot be kept clear the check reports it by name and the numbers. The same inputs give the same board: the zones are in the placer's fixed order, and nothing in the new cost uses a random choice.

**Editor (C4, the limits only).** The PCB view draws a keepout that limits height in amber with the text `max 3 mm` in its corner and a title that says the limit and the faces. The window overlays and the drag limit of section 4 are still not built.

**Open for the maintainer.**

- The name `.body_height`, and whether the catalog `height` column (section 2) should come first.
- That a height-only keepout forbids nothing else, rather than needing `.parts &= "allow"` written out.
- That `.layers` names the faces a limit applies to.
- Whether `unchecked` should be a warning only (as written), or an error under `constraints = "error"`.
- Whether the body or the courtyard should be what is measured against a zone (now the body; the placer is stricter, with the courtyard).
- Reading a limit from the DXF (a layer that carries a height, or a 3D entity), which needs a rule that is not a guess.
- A limit that a part's own keepout carries, which the placer does not yet avoid.
