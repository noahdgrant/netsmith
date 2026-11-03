---
status: accepted (user review 2026-11-02)
---

# Layout knowledge, derived placement, module regeneration, agents and the PCB editor

**Built so far (ADR 0032, provisional):** the sidecar file and its first fields, with the choices that differ from the sketch in section (f): the file is `<stem>.pcb.toml` with `version = 1`, one table per board; a part's `side` is `"top"` or `"bottom"` (not `front` and `back`); keepouts are members of the board or of a part in the source (ADR 0029), and the file gives only the place of one that states none, as `[Board.keepouts.name]` with `at`, and likewise `[Board.cutouts.name]`; an unknown key is an error. Waivers, overrides, modules and the routes cache are not read yet (the seed is, ADR 0033), and a file that has them is refused by name until the step that adds them. The function boundary for the placer is `netsmith_pcb::apply_layout(board, layout, file)`, which runs after the placement and changes nothing if an entry is wrong.

**Derived placement is built (ADR 0033, provisional):** stages P0 (the placer places the rest round the parts a layout file fixes), P1 (annealing on pad-centre wire length with rotations, module cohesion by instance path, fixed outline fit) and P2 (decoupling pairs derived from power pins) of section (b), with a seed in the layout file and `place_subset`, the function section (c) needs. P3, the `layout` block, is language and is not built; the decoupling pull needs no number and uses none.

**Editor built so far (the `pcbedit` branch, provisional).** The PCB view is a canvas (`web/src/pcb/`): select by click, box, Shift/Ctrl and the parts list, with the pads, nets and ratsnest of the selection, tooltips (reference, value, net), a snapping grid (a setting of the page), pan with the right button and zoom with the wheel; move by dragging, rotate (`R`), flip (`F`), lock (`L`) with a padlock drawn on a locked part, hand a part back to the placer (`Delete`); undo and redo (`Ctrl+Z`, `Ctrl+Shift+Z`) over all of it. The page keeps the history, and the server keeps none: an edit is a `PcbEdit` of `PcbChange`s that each say what an entry of the layout file was and what it becomes (`POST /api/pcb/edit`, typed in `api.rs`), the inverse of an edit is the same edit turned round, and the server answers 409 if the file or an entry is not as the page last saw it, 400 if the board does not take the result (it is placed with the new layout first and the file is written only then, atomically). `GET /api/pcb/scene` has the drawing (`svg::render_view`, with `data-ref`, `data-path`, `data-pad`, `data-net`, `data-keepout`, `data-cutout`, and the ratsnest lines' `data-from` and `data-to`) and the data that finds things in it. An edit throws the routed copper away (no routes cache yet) and a flip changes what the derived placer is told, so parts not placed by hand may move. Keepouts are drawn with a rectangle tool, moved, resized and deleted, and kept in the layout file (ADR 0029, ADR 0032); the outline and cutouts are the source's and are drawn. Findings (`GET /api/pcb/findings`, `POST /api/pcb/drc`): netsmith's placement problems, the connections left on a routed board, netsmith's own design rules check of the routed copper (ADR 0037, source `drc`), and KiCad's design rules check as a second opinion (JSON report, run on demand and kept until the layout changes), each with a point and a box; a click zooms to it and marks it, and the list follows the edits. Not built: waivers (the page has no way to write one), constraint findings, regeneration of a module (E4), partial routing (E5), hand-drawn tracks (E6), a copy of the findings for an agent, `layout show` overlays.

**User decisions (2026-11-02):** (1) layout knowledge lives in a `layout` block in the part, not in a library file ("the fewer places to find the information the better"); (2) routes are stored in a committed file next to the sidecar, so a rebuild never changes an accepted layout and only an explicit regenerate does; (3) manufacturing-rule violations (DRC) block `fab` by default and datasheet layout-guideline violations only warn, each with a default key in `netsmith.toml` and a command-line flag that overrides it for one run; (4) derived placement is built first, the `layout` language block after it. Decision (3) is built as a general mechanism in ADR 0036 (`guidelines = "warn" | "error"` in `netsmith.toml`, `--strict` for one run); the first guideline is the track width against current, and the layout constraints use it when they land.

ADR 0020 item 7 decided where a placement is stored (a sidecar file, 7A) and that what the circuit implies is derived (7C). This ADR designs what that leaves open: where the layout knowledge of an IC lives (the datasheet's "put the capacitor next to pin 3"), how placement is derived from it, how one module is laid out again on its own, how a coding agent drives all of it, and what the PCB editor must do. **Nothing here is decided and nothing is implemented. Syntax is the user's choice (AGENTS.md): each part lists options and a recommendation.**

Snippets marked `nsm` parse with today's parser (checked with `cargo run -p netsmith-lang --example parse`). Snippets marked `nsm-new` need a construct that does not exist; the first error is at the new word (`layout "SOT-223" {` stops at the string, as `pins` did before ADR 0016). Numbers in examples are **illustrative**: a real constraint carries the datasheet and the page it comes from, and is never written from memory (the rule for catalog rows, applied here).

The user's goals, as this ADR reads them: (1) agents as good at layout as possible with as little as possible in `.nsm`, a balance that moves over time; (2) placement derived from the circuit wherever feasible, manual positions in the sidecar; (3) select a module in the editor and regenerate only its layout; (4) datasheet layout guidance stored, or known to the tool, because it makes placement and routing better.

## What exists

- `place.rs`: connectors in left and right columns, the rest in rows of at most 30 mm, nothing rotated, front side only, order improved by pair swaps (8 passes) on the length of the ratsnest. The outline is the arrangement plus 2 mm. Deterministic.
- `route.rs`: A* on a 0.1 mm grid, two outer layers, via beside each pad of a plane net, rip-up of blocking nets, several net orders, a time limit. `route_with` builds its obstacles from the pads and the edge (`initial_obstacles`) and from the copper it routed itself in the run (`copper_of`), so tracks already on the board are not obstacles, and it cannot route a chosen set of nets.
- `Board::check_placement` returns `PlacementProblem` values (overlap, outside, pad near edge, hole neighbour) as text; DRC is KiCad's, reported as counts (`netsmith-fab/src/drc.rs`). No finding has a coordinate yet.
- `netsmith_core::Part` carries `refdes`, `kind`, `package` and `PartPin { name, datasheet, kind, pads, function }`, so pin kinds and pads reach the PCB crate. It does not carry the instance path (`stage.output_cap`) or which pins form a `Rail`. The editor knows the instance of each part (ADR 0019).
- The editor's PCB view is a read-only SVG with layer toggles, a "Place and route" button and a banner of problems (`views/Pcb.tsx`, `editor/pcb.rs`). There is no sidecar yet.

## The balance: three tiers

Everything about layout goes in exactly one of three places, judged in this order.

1. **Derived from the circuit, nothing written.** Grouping by module path, connectivity, decoupling pairs from rail topology, connectors to edges, power-pin pull. Free, and right for every design. It carries weights, never numbers that need a source.
2. **Knowledge of a part, written once in the library, with a source.** A datasheet number that the circuit does not imply and that is true in every design (a distance, a keepout, a via count). Checked like the rest of the part.
3. **A decision of this design, in the sidecar.** Positions, locks, keepouts the board draws, a waived constraint with its reason. Never in `.nsm`.

A fact moves from tier 3 to 2 when agents keep writing it for every design, and from 2 to 1 when a rule derives it. The test for tier 2 is three questions: does a datasheet say it, can the tool not derive it, does it hold in every design. The language grows by one block (below), not by a vocabulary of placement.

## (a) Where layout knowledge lives

**Option A1: a `layout` block in the part, beside `pins`** (`nsm-new`). Constraints are about the part's own pins and are resolved through the circuit: "the capacitor that bridges `.vin.pos` and `.vin.neg`" is found on the board by topology, so the part names no part and no net (R4, R5 of ADR 0020 hold; ADR 0002: the block names no tool). A block may be keyed by package like `pins`, because pad numbers and keepout rectangles depend on the footprint; an unkeyed block holds what is true in every package.

**Option A2: a sidecar per library part**, `std/layout.toml` keyed by kind and package, like `std/kicad.toml` (ADR 0016). No language change. But ADR 0016 chose TOML for tool names, which a layout fact is not; the knowledge sits away from the `pins` it refers to, the pin names are not checked by the compiler, and a library part would have two files (ADR 0001).

**Option A3: prose only** (a doc comment on the part, and an agent that reads it). Costs nothing, is read by every agent, and is checked by nothing; the placer cannot use it and a violation cannot be reported. Kept as the complement of A1: the datasheet's sentence is the doc comment above the constraint.

**Option A4: module layouts, reusable sub-layouts** (atopile's `provides_layout`, locked or seeded). A module records the relative positions of its parts, and each instance applies them as a rigid group or as a start for the placer. The strongest form of "known good" for a module, and the output of (c) below. It is a sidecar (tier 3 of the library: `<module>.layout.toml` next to the module's file), not language. Later.

**Option A5: learned from hand-laid boards.** `netsmith layout learn` reads a ported, hand-laid `.kicad_pcb` and reports what distances its authors used for each pair a rule would constrain (median pad-to-pad distance of each decoupling pair, say). It proposes tier-2 numbers for a person to cite and accept; it is never authoritative. It is also the best test of the placer: the same board derived and laid out by hand compared on those distances, loop areas and total length.

**Recommendation: A1, with A3's doc comment above each constraint, then A4 as the result of regeneration, with A5 as a tool.** A1 keeps pin names checked, keeps the knowledge in the part it describes, and is the only form the placer, the checker and the findings can all read. The language stays small by a closed list of seven statements, no expressions, no variables, no loops, one statement per line, order irrelevant, and a mandatory `source`.

### The vocabulary (A1)

| Statement | Says | Datasheet phrases it carries |
|---|---|---|
| `near NAME SELECTOR to PIN within D [same_side]` | the selected part's pad on PIN's net is within D (pad centre to pad centre) of PIN's pad | "as close as possible to pin 5", "within 2 mm", decoupling, feedback divider, inductor at the switch pin |
| `away NAME SELECTOR from PIN at_least D` | no pad of the selected part is nearer than D to PIN | "keep the feedback trace away from the switch node and the inductor" |
| `loop NAME PIN PIN area_at_most A` | the polygon pin, bridging capacitor's pads, pin has at most area A | "minimise the input loop", "hot loop" |
| `keepout NAME layers L rect X0 Y0 X1 Y1` | no copper of the layers L in the rectangle, in the footprint's frame (rotates with the part) | antenna clearance, no copper under an inductor or a crystal |
| `edge NAME side S within D` | the part's side S is within D of the board edge | antenna module at the edge, connectors, a sensor that must face out |
| `vias NAME pad P count N..M` | the pad carries N to M plated vias to the plane of its net | exposed pad, thermal vias, "connect the tab to the ground plane" |
| `route NAME PIN [width_at_least W] [length_at_most D] [plane_via_within D]` | the copper on PIN's net near the pad meets these | "short wide trace from the switch node", "ground vias next to the pad" |

`SELECTOR` is `bridging PIN PIN` (a two-terminal part with one terminal on each pin's net) or `on PIN [kind K]` (another part with a terminal on PIN's net, optionally of a kind such as `Inductor`). `D` is a length (`3mm`), `A` an area (`25mm2`), `N..M` a range. Every statement ends with `source "..."` naming the document and section. The name is the constraint's stable id, `u/decouple_vin` once an instance path is added, and is what the sidecar waives and what a finding names. Not in the first cut: `align` (orientation relative to a pin or an axis), `symmetric` (two parts mirrored about an axis, for crystal load capacitors and differential pairs), `match` and `pair` (length match, differential pair gap and skew, which belong to a bus `interface` such as `Usb2` and not to a part), and a plain `group`, since module paths already group.

How a constraint behaves: it is **a term of the placer's cost and a check** at once. The placer pulls toward satisfying it; the check measures the result and reports a finding with the measured and allowed number (below). It is not an error that stops `build`. `fab` reports the findings and a project may choose to refuse a package that has unwaived ones (open question 3).

### Worked example 1: a linear regulator

The datasheet asks for an input capacitor and an output capacitor close to the pins, a short ground return between them, and the tab soldered to copper. With today's circuit (this parses):

```nsm
import std::parts::passive::Capacitor
import std::parts::regulator::LinearRegulator

module DecoupledLdo {
    .reg: LinearRegulator
    .cin: Capacitor
    .cout: Capacitor
    .reg.vin.pos ~> .cin ~> .reg.vin.neg
    .reg.vout.pos ~> .cout ~> .reg.vout.neg
}
```

and, added to `LinearRegulator` itself in `std/parts/regulator.nsm` (`nsm-new`; its `pins` tables are unchanged):

```nsm-new
    layout "SOT-223" {
        /// Datasheet "Layout guidelines": input capacitor close to IN and GND.
        near decouple_in  bridging .vin.pos .vin.neg   to .vin.pos  within 3mm same_side source "<datasheet> 10.1"
        near decouple_out bridging .vout.pos .vout.neg to .vout.pos within 3mm same_side source "<datasheet> 10.1"
        /// Short return between the two capacitors' grounds.
        loop return_loop .vin.neg .vout.pos area_at_most 40mm2 source "<datasheet> 10.1"
        /// The tab is the heat path: copper under it and vias into the plane.
        vias tab pad "4" count 4..9 source "<datasheet> 10.2"
    }
```

The placer reads `near`: it finds `cin` (bridges `vin.pos` and `vin.neg` on the board) and `cout`, pulls their pads to within 3 mm of the regulator's pads. The router reads `vias`: it puts four to nine vias in the tab pad instead of one beside it. With no `layout` block the derived rule of tier 1 still pairs `cin` and `cout` with their pins and pulls them as near as the courtyards allow; the block adds the number and the source, which turn the pull into a check. (The pad number and the numbers are illustrative. The `pins` tables of `LinearRegulator` list pads 1 to 3 only; the tab's pad number is the footprint's and would be added to the table with the `layout` block. `SOT-23` has no tab and gets no `vias`.)

### Worked example 2: a buck converter IC

A concrete buck IC is a `part` with a `.buck: Buck` member (ADR 0025). `BuckStage` holds the inductor, capacitors and divider; the IC's layout facts name the IC's pins and find those parts by topology, wherever the module that holds them is:

```nsm-new
    layout "QFN-16" {
        /// Input capacitor directly across VIN and PGND: this is the hot loop.
        near input_cap bridging .vin.pos .vin.neg to .vin.pos within 2mm same_side source "<datasheet> 11.1"
        loop hot_loop .vin.pos .vin.neg area_at_most 20mm2 source "<datasheet> 11.1"
        /// The switch node is small and short: inductor next to SW, no copper pour on it.
        near inductor on .buck.sw.pos kind Inductor to .buck.sw.pos within 3mm source "<datasheet> 11.1"
        route sw_node .buck.sw.pos width_at_least 0.5mm length_at_most 8mm source "<datasheet> 11.1"
        /// The feedback divider sits at FB and away from the switch node and the inductor.
        near fb_divider on .buck.fb kind Resistor to .buck.fb within 5mm source "<datasheet> 11.1"
        away fb_from_sw on .buck.fb kind Resistor from .buck.sw.pos at_least 5mm source "<datasheet> 11.1"
        /// Ground reaches the plane next to the input capacitor and under the thermal pad.
        route gnd_return .vin.neg plane_via_within 1mm source "<datasheet> 11.1"
        vias thermal pad "17" count 5..9 source "<datasheet> 11.2"
    }
```

Nothing here is in `BuckStage` or on a board. A buck IC from another maker writes its own block with its own numbers. Where several capacitors bridge VIN and GND, `near` applies to every one of them, and the derived rule of tier 1 orders them so the smallest sits nearest; whether a datasheet needs a selector for "the smallest" is left until one does.

### Worked example 3: an RF module with an antenna

```nsm-new
    layout "Module-SMD" {
        /// Antenna end: no copper on any layer for 15 mm beyond the module, and the antenna at the board edge.
        keepout antenna layers all rect -9 -15 9 0 source "<datasheet> 7.1"
        edge    antenna_edge side top within 0mm source "<datasheet> 7.1"
        /// Ground under the module except below the antenna.
        near    decouple_3v3 bridging .vdd.pos .vdd.neg to .vdd.pos within 4mm source "<datasheet> 7.2"
    }
```

The rectangle is in the footprint's frame (mm, y down, origin at the footprint origin), so it rotates with the part. A keepout is an obstacle for the placer (other courtyards stay out), the router (no track or via inside) and the pour (item 2 of ADR 0020).

### How the three tiers combine

Constraints from `layout` blocks are collected for every part instance on the board and named `<instance path>/<constraint name>`. The sidecar may **waive** one with a reason, or **override** its number for this design, which is a design decision (tier 3) and is reported as such:

```toml
[Board.waive."stage.u/hot_loop"]
reason = "two-layer board, no room for the 20 mm2 loop"
[Board.override."stage.u/input_cap"]
within = "3.5mm"
```

The library cannot be loosened by a board (R3) except through the sidecar, which shows in every report. A waiver of a constraint that no longer exists is an error naming it, like a stale path.

## (b) How placement is derived

**The problem.** Choose for each movable part a position on a 0.05 mm grid, a rotation (0, 90, 180, 270) and later a side, inside the outline, so that the cost below is least. Fixed parts (`lock` in the sidecar, mechanical parts at corners, connectors on edges) never move. A fixed outline from ADR 0020 item 3 is a hard region; with none the outline is the arrangement plus the margin, as now.

**Cost** (all terms in mm or mm squared, weighted; each weight a named constant of the crate):

- wire length: the minimum spanning tree over **pad centres** of each net (as `Wirelength` does now), weighted by the net: a plane net counts little (it ends at a via), a net of fan-out above a limit counts less per pad, a signal net counts most. Using pads and trying rotations is the pin-side awareness: a pad on the wrong face of an IC lengthens the net, and the rotation that fixes it wins.
- overlap and clearance: courtyard overlap with the gap of `place.rs` (`GAP_X`, `GAP_Y`), pad to edge, keepouts, holes (the terms `check_placement` already tests), as a large penalty and finally as a hard rule on the last accepted state.
- module cohesion: the area of each module instance's bounding box, and a pull of each part to its module's centre, so `stage.*` stays together without anyone declaring a group. The instance path is the group (needs `Part.path`).
- decoupling: for each part with `power` pins, each capacitor bridging the two pins of a `Rail` is paired with the pin, and pulled by a high-weight term on pad-to-pad distance. Capacitors on one pin are ordered by value, smallest nearest. This is derived, with no number.
- constraints: for each `layout` constraint, `max(0, measured - allowed)` squared times a large weight; `away` and `keepout` the same the other way. The same functions produce the findings.
- escape and routability: a ring round fine-pitch parts and exposed pads kept free (for fan-out vias and decoupling), and later a congestion estimate.

**Search.** Simulated annealing over moves (shift, rotate, swap two parts, move a module as a group, flip a side later), on integer micrometre coordinates so the result does not depend on the platform's floating point; a fixed number of iterations with geometric cooling; a small PRNG written in the crate and seeded by `(board name, scope path, seed)`. Same source, sidecar and seed always give the same board, which ADR 0006 and R7 need. It is hierarchical: first each leaf module instance is arranged on its own around an anchor (its largest part, or the part with most pins on nets leaving the module), then the module blocks are placed as rectangles with their external pads as ports, then everything is refined at a low temperature. A force-directed start (parts pulled along nets, pushed apart) seeds each level, which is faster to converge than annealing from the row layout. The existing row layout stays as the start for a board of a few parts and for the fixed-outline overflow message.

**What the placer reads from the parts.** Pad centres and sizes in the footprint (`Footprint::pads_numbered`, present), pin kinds and names (`PartPin`, present), the pin-to-pad mapping (present), the **instance path** of each part (to add to `Part`), the **rail pairing** of a pin (the interface instance and role: `vin` and `pos`, to add to `PartPin`), the parsed `layout` constraints with their package (new), the sidecar's positions and locks, and the outline.

**Stages.**

| Stage | What | Needs | Size |
|---|---|---|---|
| P0 | Read the sidecar: fixed and locked parts; write what autoplace chose (`netsmith layout`). Placer places the rest round them. | sidecar (ADR 0020 7A) | M |
| P1 | Annealing on wire length and overlap with pad centres, rotations, module cohesion by path, connectors toward edges, fixed outline fit and its overflow message. No new language. | `Part.path` | L |
| P2 | Derived decoupling pairs and escape rings; typed supply pins. | IC parts with `power` pins on boards (ROADMAP backlog 1) | M |
| P3 | `layout` blocks: parse, check, constraint terms, findings, `vias` and `route` read by the router. | language (this ADR, A1) | L |
| P4 | Hierarchical placement; regenerate a module (c). | P1, partial routing | L |
| P5 | Two sides, congestion-aware cost, loop area from routed copper, learned weights from A5. | double-sided assembly (backlog 4) | L |

P1 and P2 give most of the win with no language change; they are where to start.

## (c) Regenerate one module

The user selects a module instance (`stage`, from the tree or by clicking a part and choosing a level) and presses Regenerate.

1. **Scope.** M is every part whose path starts with the instance path. Every other part is **locked at its present position**, for this run only (no sidecar write for them). The module's anchor, its largest part, stays where it is unless "move anchor" is on; if M has no part of that size, the external parts that share its nets are the fixed terminals.
2. **Place.** The same annealing (P1 to P3) on M alone, in a region: the module's present bounding box grown by a margin (default 3 mm), minus everything fixed (courtyards, keepouts, holes) and clipped to the outline. If it does not fit, say so with the numbers, as the outline overflow message does.
3. **Seed.** The seed is the stored seed of that module plus a counter in the sidecar, so pressing again gives another reproducible arrangement and an explicit `--seed` repeats one. The result is written as the positions of M's parts, with the seed beside them (`source = "regen"`), and nothing else in the sidecar changes.
4. **Route.** All nets with a pad on a part of M are ripped up, and only those. The rest of the copper stays as obstacles. They are routed again; if one fails, the nets whose copper crosses the region (bounding box grown by the margin) are ripped up too and routed after, and the answer lists them ("also re-routed: net SCL"). If a line is still unrouted it stays in the ratsnest and in the findings. The module's plane-net pads get their fan-out via again.
5. **Preview and undo.** The editor shows the result as a proposal over the current board (parts and tracks in a second colour, with the findings before and after). Accept writes the sidecar once; Reject drops it. An accepted regeneration is one entry on the editor's undo stack (undo restores the old positions and the old copper). A CLI run (`netsmith place --module stage --seed 3 --write`) is the same code with `--write` as Accept.

**What the router needs.** Today `route_with` routes every line of the ratsnest. It needs: (a) the tracks and vias already on the board as obstacles, in the same `Obstacle` list that `copper_of` fills for the copper routed in the run; (b) a net filter, "route only these nets"; (c) a rip-up that may remove a given set of nets and no others, and reports which it removed; (d) its result as per-net records rather than only tracks appended to the board. **Stable routes** need one more thing: the copper of untouched nets must not change when a module is regenerated, so the routes themselves are stored. Recommended: a cache file `<stem>.routes.toml`, one record per net with its segments and vias and a hash of the net's pad positions; a net whose hash no longer matches is discarded and routed again. `build` and `fab` stay a pure function of source plus sidecar and cache (a missing cache is routed from scratch, deterministically); the cache is committed or not at the project's choice. Without it, "leave the rest alone" holds only inside one editor session. (Open question 2.)

## (d) The agent story

This repository is agent-neutral: any agent that runs a command and reads files. So the tools are commands with JSON, and the guidance is a skill, not a chat panel.

**Commands (proposed)**, all deterministic, all `--json`:

| Command | Does |
|---|---|
| `netsmith place <board> [--module PATH] [--seed N] [--write]` | derive a placement (whole board or one module), print the cost and findings, write the sidecar with `--write` |
| `netsmith route <board> [--nets A,B] [--module PATH]` | route all, some nets, or the nets of a module; print unrouted and re-routed nets |
| `netsmith layout check <board>` | findings only: constraint violations, overlaps, unrouted, DRC |
| `netsmith layout show <board>` | every part with position, rotation, side, lock, pads (net, centre), its constraints and measured values: the board as data |
| `netsmith layout render <board> [--region x0,y0,x1,y1] [--highlight REF,NET] --out f.png` | a picture for an agent that can look (SVG exists; PNG needs a rasteriser, question 6) |

There is no `layout set` command: the sidecar is a plain TOML file and the agent edits it, one way to say it (ADR 0001); the next command validates every path in it and names a stale one.

**A finding is a value.** The same record the editor draws and the CLI prints:

```json
{ "id": "stage.u/input_cap", "kind": "constraint", "severity": "warning",
  "message": "C3 is 4.8 mm from U1 pin VIN (pad 12); the datasheet asks for at most 2 mm",
  "refs": ["stage.input_cap", "stage.u"], "nets": ["VIN"],
  "at": {"x": 31.2, "y": 22.4, "w": 6.0, "h": 4.0},
  "measured": 4.8, "allowed": 2.0, "unit": "mm", "source": "<datasheet> 11.1",
  "hint": "move C3 toward pad 12, or waive with a reason" }
```

`kind` is `constraint`, `overlap`, `outside`, `edge`, `unrouted` or `drc`. A DRC finding needs its coordinates: until netsmith has its own DRC (ROADMAP, "Self-contained"), `kicad-cli pcb drc --format json` is read for them instead of the counts the gate reads now.

**The loop.** `build` (the circuit is right) then `place` then `route` then `layout check` then, if the agent can see, `layout render`; read the findings, fix the cause, repeat. Each pass is seconds, not the minutes of a long agent run, because the deterministic part is fast and the agent is only asked what a tool cannot decide.

**Deterministic or agent.**

| The tool does, always | The agent (or person) decides |
|---|---|
| derive a placement that satisfies the constraints, route, measure every constraint, produce findings with coordinates, regenerate a module for a seed | the outline size and shape, which edge a connector faces, how a module is oriented to the enclosure, which of several valid arrangements to keep, whether a finding is worth a waiver, and the reason |
| apply a stored layout exactly | locking a part once it is liked; writing the sidecar's decisions |
| refuse a stale path, an unknown pin, a constraint with no source | reading a datasheet the user supplies and adding a `layout` statement with its page, or saying it cannot find the number |

**A skill, `netsmith-use-layout`**, written with the first implementation (ADR 0015 copies it into projects): the loop above and its commands; how to read a finding; when to edit the sidecar and when the source; never write a datasheet number without the document and section in `source`; a regeneration is a proposal (check findings before and after, keep the better); stop conditions (no unwaived finding, or the reason it cannot be reached); what to report to the user (findings left, waivers made, anything the agent could not see).

**Compared with atopile.** In the demo frames an agent chat panel inside the application builds the board in a long working session (the log says "Working for 18m", "Working for 24m"): it edits `main.ato` and other source files, runs the autorouter, adds a fan-out via to every GND and 3V3 SMD pad, notes that "inner 1 is a plane", finds isolated vias, and ends with a 3D render and a list of open items (the USB-C annular ring, unresolved build warnings). The layout is its `default.pcb.hl` file, with a "Re-autoplace" button for the part that is not locked. The same parts exist here: a sidecar, an autoplacer, a plane fan-out in the router (ADR 0012), a 3D view later. The differences we choose: the agent is any agent, not a built-in panel; the knowledge is in checked source with a citation, not in the agent's head or its prompt; the tool returns findings as data so the agent's loop is short and the agent's report can be verified; and a module's layout can be regenerated alone, which the demo's whole-board autoroute cannot.

## (e) The PCB editor

Built on the editor of ADR 0019: Preact with signals, the SVG from `netsmith-pcb::svg`, `axum`, types written once in `api.rs` and generated to `api.ts`. The SVG already has a group for each layer; each part needs `data-ref` and `data-path` so a click finds the instance.

| Stage | Interaction | Server | Size |
|---|---|---|---|
| E0 | Select a part (click, box), highlight its net and the ratsnest lines to it, click through to its source line, show the selected part's constraints | `data-ref` and `data-path` on the SVG; `GET /api/pcb/part?` | S |
| E1 | Move on a grid, rotate (R), flip (F, once there are two sides), lock (L); undo and redo; live overlap and outline check while dragging; save to the sidecar (`Ctrl+S`) | `GET` and `PUT /api/layout?board=` with the stamp (409 on a changed file, as for sources); the page keeps the edit state, the server none | M |
| E2 | Findings panel: constraint, placement, unrouted and DRC findings as a list; click zooms to the finding's rectangle and selects its parts; waive with a reason (writes the sidecar) | `GET /api/pcb/findings?board=`, `POST /api/pcb/drc` | M |
| E3 | Constraint overlays (radius of a `near`, the hot loop polygon, keepout rectangles), draw a keepout (writes the sidecar), the outline from ADR 0020 shown and checked | overlays from the `layout show` data | M |
| E4 | Select a module instance (tree or "select enclosing module"), Regenerate with a seed field, preview as a proposal with findings before and after, accept or reject, one undo step | `POST /api/pcb/regenerate` returning a proposal; progress on the WebSocket | L |
| E5 | Re-route the nets of a selection; keep a hand-drawn track as a locked route | partial routing (c) | M |
| E6 | Hand-drawn tracks, push and shove, a 3D view | out of scope here | L |

The editor has no chat. A "copy findings for an agent" button puts the findings JSON and the selection on the clipboard: the agent still works through the CLI and the files, and the editor shows what it changed because it watches the sidecar like it watches sources (ADR 0019 polling).

## (f) The sidecar

`<stem>.pcb.toml` beside the top-level file (one sidecar for the PCB, apart from the schematic's `<stem>.layout.toml` of ADR 0019, so a regeneration cannot disturb schematic offsets; atopile has the same split, `default.pcb.hl` and `default.sch.hl`). Keys are instance paths, which survive a renumbered reference designator. Coordinates are mm from the outline's top left, y down (ADR 0020 item 3), the footprint's origin.

```toml
version = 1

[Board]
seed = 7                       # the seed `place` starts from

[Board.parts."stage.output_cap"]
at = [31.5, 22.0]
rotation = 90                  # degrees, counterclockwise on screen
side = "front"                 # "back" once both sides are placed
lock = true                    # autoplace and regenerate leave it
source = "hand"                # "hand", "auto" or "regen": who wrote it, shown in the editor

[Board.modules."stage"]        # what the last regeneration used
seed = 3
count = 2

[[Board.keepouts]]
name = "antenna_clear"
rect = [40.0, 0.0, 60.0, 12.0]
layers = "all"                 # "all", "copper", "tracks", "vias" or "pours"

[Board.waive."stage.u/hot_loop"]
reason = "two-layer board, no room for the 20 mm2 loop"

[Board.override."stage.u/input_cap"]
within = "3.5mm"
```

Rules: a part with no entry is placed by the placer; `lock = true` fixes it; an entry for a path that is gone, an unknown field, or a waiver of a constraint that does not exist is an error naming the path or id; `source = "auto"` entries are what `place --write` wrote and are replaced by the next run unless locked. `<stem>.routes.toml`, the cache of (c), has `[[net]]` records with `name`, `hash` (of the net's pad positions), `segments` and `vias`. The TOML above is parsed by Python's `tomllib` as written here.

### The first constraints to write (the catalog)

Ranked by benefit for the standard parts and kinds, every one waiting for its datasheet (a source, never memory):

1. **`LinearRegulator`** (exists): input and output capacitor `near`, `loop` for the ground return, `vias` on the SOT-223 tab. The smallest case and the first test.
2. **A buck IC** (`Buck`, `BuckStage`; the IC itself is a part a project adds): input capacitor and `loop hot_loop`, inductor `near` SW, `route sw_node`, `near` and `away` for the feedback divider, `route gnd_return`, thermal pad `vias`. The biggest quality gain: this is where bad layout makes a converter fail.
3. **An MCU or a module** with supply pins: derived decoupling for every `power` pin (tier 1, no block); `keepout` and `edge` for an antenna module; `near` for the crystal's load capacitors and a reset network.
4. **`Crystal`**: load capacitors `near`, `symmetric` (later), `keepout` of copper under it, short `route`.
5. **`Tvs` and the USB connectors** (`UsbCPower`, `UsbC2`): the protection part `near` the connector pin, then the data lines, ground via next to its ground pad. `edge` for the connector.
6. **`Schottky`** (freewheeling) near the switch node, **`NMosfet`** gate resistor near the gate, **`FerriteBead`** and decoupling at a connector.
7. **`Inductor`** and **`Header` and `ProgrammingPads6`**: `keepout` under a shielded inductor's field, `edge` for a header that has to be reached.

## Order and effort

P0 and E0, E1 first (a sidecar and a board one can move and lock: the editor becomes useful for layout). Then P1 (a placer that is better than the rows; the biggest visible change, no language), E2 (findings as data, which the agent loop and the editor share) and the CLI commands. Then P2 with the IC parts. Then P3, the `layout` block, with the first two parts of the catalog. Then partial routing, P4 and E4 (regenerate a module), and the skill with the first agent-facing commands. The last two steps depend on the first and on the routes cache.

## Consequences

Layout facts live with the part they come from, with a source, and the compiler checks their pin names. The board source keeps no geometry, so ADR 0001 and ADR 0020 R2 hold. The cost is one new block in the language, a second file beside the sidecar (the routes cache), a placer that must become much better than the rows before any of it shows, and a stricter router (obstacle tracks, net filter). Constraints are findings and not blockers: a design that cannot meet a datasheet number says so with a reason, and the report shows it. Known limits stay in `README.md` and `docs/ROADMAP.md` ("Layout") until each step lands.

## Open questions for the user, in order

1. **Where a datasheet's layout fact lives.** A1, a `layout` block in the part with a mandatory `source` (recommended); A2, a library TOML; or A3, doc comments only and a placer that knows nothing. A1 is the one new construct of this ADR. A change here changes the answer to (a) and (d).
2. **Do routes live on disk?** Yes, as a cache `<stem>.routes.toml` keyed by net and pad hash (recommended): it is what lets a module be regenerated and the rest stay exactly as it was between sessions. No means "leave the rest alone" holds in one editor session only, and a full route may change copper everywhere after any placement change.
3. **How strict are constraints?** Findings that never stop `build`, shown by `fab`, optional `--strict-layout` that refuses a package with an unwaived one (recommended); or always a blocker; or never in `fab`.
4. **Order: derived placement first, or the language block first?** Derived (P1, P2) first, then the block with the first real IC parts (recommended), since a block with nothing to run it on is speculation. The alternative gives agents the block now and a weaker placer.
5. **One sidecar or two.** `<stem>.pcb.toml` apart from the schematic's `<stem>.layout.toml` (recommended), or one file for both.
6. **A rasteriser.** PNG for agents that can look needs one dependency (a pure-Rust SVG renderer) or a headless browser the user has already; or SVG only and the agent reads `layout show` data.
7. **DRC with coordinates before netsmith has its own.** Read `kicad-cli pcb drc --format json` (recommended, needs KiCad as today) or wait for the self-contained DRC.
8. **Two-sided placement** (flip) in the first editor cut or later (recommended later: it needs assembly on both sides, backlog 4).
9. **Module layouts as reusable sub-layouts (A4)**: a result of regeneration stored with the module, or not wanted.
10. **An agent chat in the editor**: no (recommended); the agent works through the CLI and the files, and the editor shows the result.
