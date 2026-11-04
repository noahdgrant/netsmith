---
status: accepted (user decision 2026-11-02); item 7A amended by ADR 0048 (requirements on placement go to the physical file, solutions stay in the layout file)
---

# Board features: net classes, pours, outline, mechanical parts, silkscreen, panels, placement

## Decisions

The user accepted the recommendations below on 2026-11-02, with three additions. The sections after this one are the proposal as it was weighed and are kept for the reasons.

- **1A, net classes** are a field on `Rail`: `.min_track`, the minimum track width of the nets the rail names. A net takes the widest of its rails (GND takes the widest of its rails). Ranges are allowed.
- **2A, outer pours** are `.top` and `.bottom` members (`Pour`) of the process beside the planes, with stitching vias on a grid after routing. **Added: keepouts**, which the proposal deferred, are built now (ADR 0029).
- **3A, the outline** is an `Outline` board member (width, height, corner radius; a missing outline means size to fit; ranges allowed on width and height). **Added: internal cutouts** are `Cutout` board members, rectangles with an optional corner radius.
- **4A, mechanical items** are parts, with `.assembly`; D5 is settled by ADR 0024 and ADR 0028 (the manifest gives the default).
- **5A, silkscreen**: reference designators unchanged, a `Label` part for free text, no logos.
- **6, panels**: deferred here; ADR 0040 designed them and built stage P0.
- **7A and 7C, placement**: geometry lives in one place, a sidecar file keyed by instance path (its format is ADR 0032); what the circuit implies is derived by the placer. No `near` statement in the language.


A board that goes to a fab today is a rectangle sized by the placer, drawn with one track width, with planes on the inner layers and nothing else. A real four-layer product also needs wider power tracks, a ground pour on the outer layers, a fixed outline with mounting holes, fiducials and test points, assembly choices, silkscreen text, and a way to say where a part goes. This ADR proposes how each is written in `.nsm` and what the crates and the DRC gate must do. **Nothing here is decided: syntax is the user's choice (AGENTS.md), so each item lists options, the rules it is judged against, and a recommendation.** Nothing is implemented.

Snippets marked `nsm` parse with today's parser (checked with `cargo run -p netsmith-lang --example parse`); they use names that do not exist yet (`std::pcb::...`), so they would fail later at import. Snippets marked `nsm-new` need a new construct and do not parse. Two statements on one line need `;`, so the snippets use one statement per line.

The rules every option is judged against:

- **R1 one way** (ADR 0001): one spelling per thing; no alias, no second place to say it.
- **R2 order** : statement order never matters, so a value is a narrowed range or an equation, never an assignment.
- **R3 library truth** (ADR 0002): the library states only what is always true, and holds no KiCad names. A narrowed value cannot be loosened, so a library default is a permanent constraint on every design.
- **R4 interface** (ADR 0004): a board reaches a module only through its interface; whatever a user may need to set, the module author exposes.
- **R5 nets** (ADR 0005): only a board names a net, and the check refuses a field called `net` outside a board.
- **R6 process** (ADR 0008, 0012): the process is a board member the router, `.kicad_pcb` writer and DRC gate read from the `Design`; a plane's net is stated on it.
- **R7 tools** (ADR 0006): a generated `.kicad_pcb` is never read back, so hand layout survives regeneration only if netsmith owns where it is stored.

Atopile v2 is the comparison. The points below come from a summary of its language that is not in this repository, and atopile's public pages (docs.atopile.io, the README and the `ato` skill file in its repository) do not show them, so they were not re-checked. It has no net-class or pour syntax in the language: an `ato` file attaches a trait for a net name (`has_net_name`), the outline is a type from a geometry project (`RoundedRectangle`), and layout lives in a separate layout file edited in its editor (the screenshots show `layouts/default/default.pcb.hl` beside the source and a "Re-autoplace" button), with `provides_layout` to reuse a sub-layout in `locked` or `seed` mode. Every field is reachable from outside; netsmith does not do that (R4).

## What was built, and what was decided where the decisions left room

- **Net classes.** `Rail` has `.min_track`; lowering gives each net the largest bottom of the ranges of the rails with a pin on it (`Features::net_widths`), and a width under the process's least is the least. The router draws the net at that width and keeps clear of other copper by it. A track leaves a pad at the net's width. The `.kicad_pro` has a net class for each distinct width (`Track_0.8mm`) with a pattern for each net. The class has a `priority`, which KiCad needs to read one, and no clearance, so the nets in it keep the space the board is checked to.
- **Outline.** A width or height range is the size the board may be; the placer (`PlaceOptions::fit`, ADR 0031) lays the parts out in the least size that holds them (rounded up to 0.1 mm for a limited range, the arrangement's own whole millimetres for an unlimited one, so a board with an `.outline` that limits nothing is the board it was). When the parts need more than the top of the range the error names the need, the allowance and the overflow in x and y. The corner radius is checked against the shorter side. The edge is written as `gr_line` and `gr_arc` on `Edge.Cuts` (`gr_rect` when square, which is also what every older board has), the router keeps off it as a closed run of short pieces, and the placement check measures the pads' and courtyards' depth in a rounded corner. Mounting holes, plated holes and fiducials go in the corners of a fixed outline, in that order, clockwise from the top left, with their reference text inside the board; a board with no `.outline` lays them out with the other parts as before.
- **Cutouts** are `Cutout` members of the board with `.x`, `.y`, `.width`, `.height` and `.corner_radius`; the layout file may give the place instead of `.x` and `.y` (both is an error). The router and the placement check keep off them, and they are `Edge.Cuts` like the outline.
- **Pours.** `Pour` is next to `Plane` in `std::mfg::stackup`, and `TwoLayer` and `FourLayer` have `.top` and `.bottom`. A pour is a zone on the outer layer written like a plane's (KiCad's thermal relief defaults); `fab` fills it with the planes. `.stitch_pitch` is read at the top of the range. After routing, a grid of vias at that pitch, every other row shifted by half, joins a pour to an inner plane on its net or to the pour on the other side; a grid point takes the nearest free spot within a third of the pitch. A via keeps the space KiCad checks (0.2 mm, see README) from other nets' copper, the rules' holes space, pads, the edge, cutouts and the keepouts that forbid vias.
- **Keepouts** are ADR 0029. **The layout file** is ADR 0032.
- **Mechanical parts** were already parts (ADR 0024); `Label` is one more: `.text` and `.height`, a footprint generated from the text, reference `LBL1`, no symbol, and `none` assembly.
- **Not built, as decided or deferred:** panels (item 6), logos, the IPC-2221 check, clearance per class, an outline that is not a rectangle.

## 1. Net classes: track width and clearance per net

**Need.** Power nets 0.5 to 2 mm, signals at the process minimum or a width the board chooses. Today the router draws every net at `process.min_track`, and a board can already widen all of them (`.process.min_track &= 0.2mm..1m`), so what is missing is a width for some nets only. Clearance wider than the process minimum is rare below about 50 V; it would take the same shape as the width and is left out until a design needs it.

**Option 1A: a field on `Rail`.** The width is a lower bound written as a range, like a process rule, and the rule used is the bottom of what is left. Both pins of a rail get it (the return current is the same current).

```nsm
board Example {
    .input: Rail {
        .pos.net &= "VIN"
        .neg.net &= "GND"
        .min_track &= 0.8mm..1m
    }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .min_track &= 0.5mm..1m
    }
}
```

`Rail` gains `.min_track: Length` with no value (R3: a field, not a default). A net's width is the largest bottom over every rail on it: `GND` above is 0.8 mm without anyone saying so, which is the right answer for a shared return, and a statement order cannot change it (R2). `~` between rails already equates their fields except `current` (`connect.rs`), so `.stage.vout ~ .output` carries the width to the module's output rail and a module exposes it with no new rule (R4). A signal that must be wide has no rail; it is covered by 1C or by the board-wide `.process.min_track`.

**Option 1B: a class that lists nets.**

```nsm
board Example {
    .power: NetClass {
        .min_track &= 0.5mm..1m
        .nets &= "VIN,3V3,GND"
    }
}
```

A class can carry clearance, via size and a diff-pair gap too. But it names nets a second time, in a string list the language has no type for (R1, R5: the net is named by `.net &=` and again here, and a typo is a silent miss unless the check is written to catch it), and a net in two classes needs its own tie-break rule.

**Option 1C: a `net` statement in the board** (`nsm-new`).

```nsm-new
board Example {
    net "VIN" { .min_track &= 0.8mm..1m }
}
```

It covers signals as well and names a net only in a board (R5). It adds a second statement shape, `net "name" {}`, to the grammar for a thing that a rail's field already says for power nets, and it is another place that spells the net's name (R1).

**Derived check, any option.** A rail's `.current` is known to the solver. IPC-2221 relates current, copper weight and temperature rise to width. A check "this rail can carry up to 1.2 A, and its resolved width of 0.2 mm is too narrow for a 10 K rise, outer layer" fits the project (a worst-case check with the numbers named) better than silently widening. It needs the formula and its constants sourced (not from memory) and a stated temperature rise, so it is a later step.

**Recommendation: 1A.** It adds no construct and reuses `~`, narrowing and the module interface. 1C is the fallback if a design needs a wide signal that is not a rail. Via size per class and several vias where a wide net changes layer are not proposed; the router uses the process via and the limit is recorded.

## 2. Copper pours, keepouts, thermal relief, stitching

**Need.** A ground pour on both outer layers, tied to the inner ground plane by vias, with thermal relief on the pads it touches.

**Option 2A: outer layers next to the inner ones in the process.** ADR 0012 puts the net of each inner layer on the process (`.process.inner1.net`). Add the outer layers the same way, as a `Pour` (an interface, because a pour is not a plane: it fills what the tracks leave, and the glossary keeps the words apart):

```nsm
board Example {
    .process: FourLayer {
        .top.net &= "GND"
        .inner1.net &= "GND"
        .inner2.net &= "3V3"
        .bottom.net &= "GND"
        .top.stitch_pitch &= 0mm..8mm
    }
}
```

`FourLayer` and `TwoLayer` gain `.top: Pour` and `.bottom: Pour`. An inner plane with no net is an error (a four-layer board has to do something with it); an outer pour with no net is no pour, which is what a board with none says. `.stitch_pitch` is a largest spacing, so the tool uses the top of the range (the opposite end to a `min_` rule; the field is named `stitch_pitch` and its doc comment says so, or it is `max_stitch_pitch`). Unset means pads and their vias only. Where two layers carry the same net, stitching vias join them on that grid. `Pour` has only `.net` and `.stitch_pitch` at first; `.thermal_gap` and `.spoke_width` follow the same range convention when a design needs to change KiCad's defaults, which are what planes use today. R5 holds because `net` is the field name; R6 holds because it is the place layers already get nets.

**Option 2B: a board member that names its layer.**

```nsm
board Example {
    .ground_top: Pour {
        .layer &= "top"
        .net &= "GND"
    }
    .ground_bottom: Pour {
        .layer &= "bottom"
        .net &= "GND"
    }
}
```

The pour is a design choice, not something the factory makes, so it reads naturally as a board member. But then two places say what is on a layer (the process for the inner ones, the board for the outer), the layer is a string with no check but ours, and "two pours on one layer" must be refused by hand (R1).

**Option 2C: a `Plane` that may be poured.** Reuse `Plane` for the outer layers and give it a `.fill` field. This puts one meaning (a whole layer, nothing routed) and a different one (a fill round the routing) in one type, and the router would have to ask which. Not recommended.

**Keepouts.** A keepout is geometry: it needs a position and size, which needs the board's coordinates (item 3) and a place to keep placements (item 7). Two kinds exist: ones a part's footprint brings (an antenna module's no-copper area; KiCad footprints carry them and the exporter keeps them) and ones a board draws. Board-drawn keepouts are deferred until item 7 is decided; the one the proposal does need, the copper-free ring round a mounting hole, comes from the hole part (item 4).

**Thermal relief.** KiCad's default (as planes use now), no field until a design needs one.

**Stitching and the router.** Today a pad on a plane's net gets a via beside it and a track. With a pour on the pad's layer on the same net the pad is already joined to the pour, but a pad inside a ring of tracks may sit on an island that KiCad's fill removes, so the proposal keeps the pad vias (sound, not tidy) and adds the stitching grid after routing in free cells. Dropping the pad via where a pour reaches the pad is a later optimisation, to be taken only with KiCad's own unconnected-item check as the gate.

**Recommendation: 2A**, with stitching by grid after routing and no keepouts yet.

## 3. Outline and size

**Need.** A fixed rectangle, rounded corners, mounting holes (item 4) and, later, notches. Today the placer returns the outline: the arrangement plus a margin.

**Option 3A: a board member `.outline`, found by name like `.process` (ADR 0008).**

```nsm
board Example {
    .outline: Outline {
        .width &= 60mm
        .height &= 40mm
        .corner_radius &= 2mm
    }
}
```

A range is also allowed: `.width &= 0mm..60mm` says "at most 60 mm" and the placer takes the smallest that fits, which is how a process rule reads too (R2). A board with no `.outline` is sized by the placer, as now; that is one rule ("the outline is what the board says, or the least that holds the parts"), not two ways. The origin is the outline's top left, in mm, y down, and the sidecar of item 7 uses it. Fields: `.width`, `.height`, `.corner_radius`. Notches and slots are not proposed; an outline that is not a rounded rectangle is a later `Outline` kind or an imported shape, which needs an import format and is out of scope.

**Option 3B: geometry types in the library** (the atopile way: `RoundedRectangle`, `Rectangle`, a polygon). More general, but each new shape is a type with its own fields and the first one wanted is the rectangle; the extra types would be speculation (R1).

**Option 3C: a field on the process.** Not recommended: the size is a design choice, not what the factory makes.

**Placer.** With a fixed outline the placer lays the parts out inside it. If they do not fit (the annealer is run in the size whatever the rows need, and the parts do not fit only if it ends with courtyards over each other or parts outside; ADR 0045, section 10), `fab` stops with the numbers: for example `the parts need 63.4 x 41.2 mm (courtyards and 2.0 mm margin) and the outline is 60 x 40 mm: 3.4 mm over in x, 1.2 mm over in y`. Today's row layout (`ROW_WIDTH` 30 mm, connectors in left and right columns) is not a fitter, so a tight board fails often; making placement fit a given width and height is the large part of this item. `Board::check_placement` already tests courtyards and pads against the outline; it needs the rounded corner (the corner arcs in `.kicad_pcb` `gr_arc`, Gerber arcs in the profile, `edge_distance` against a curve).

**Recommendation: 3A**, rectangle and corner radius only, a missing outline sized as now.

## 4. Mounting holes, fiducials, test points, assembly flags

**Need.** Plated and non-plated holes with pad sizes and a copper-free ring, fiducials, test points that sit on a net, and parts that are not placed by the fab or not fitted at all.

**Option 4A: mechanical items are parts.** One mechanism for every footprint on the board: it has a reference designator, a footprint through the export mapping (R3, ADR 0006, from the installed KiCad libraries), a place from the placer or the sidecar, and a line in the schematic if it has a pin.

```nsm
import std::parts::mechanical::MountingHole
import std::parts::mechanical::PlatedHole
import std::parts::mechanical::Fiducial
import std::parts::mechanical::TestPoint
import std::sources::Rail

board Example {
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .mount1: MountingHole {
        .drill &= 3.2mm
        .keepout &= 6mm
    }
    .earth: PlatedHole {
        .drill &= 3.2mm
        .pad &= 6mm
    }
    .fid1: Fiducial
    .tp1: TestPoint
    .output.pos ~ .tp1.pin
}
```

`MountingHole` is non-plated and has no pin; `PlatedHole` is plated and has one pin that is joined to a net (a screw to ground); the language has no boolean, and two kinds are clearer than a flag. `Fiducial` has no pin. `TestPoint` has one pin and a `.diameter` range; the mapping takes the least stock footprint inside it and names the sizes it knows if none fits. The checks name numbers: a hole's drill below `process.min_drill`, a plated hole whose pad leaves less than `min_annular_ring`, a hole nearer the edge than `min_edge_clearance`. The fab's fiducial and test-point rules (pad size, mask opening, copper-free ring) are not in the published page that `std/mfg/jlcpcb.nsm` cites; they are read and sourced before the parts land, not stated here.

**Option 4B: features of the outline.** `.outline.corner_holes &= 4`, `.outline.hole_diameter &= 3.2mm`, `.outline.hole_inset &= 4mm` make holes at the corners. Short for the common case, but holes elsewhere need a second way (R1), and a plated hole with a net or a test point cannot be an outline feature.

**Option 4C: board members with coordinates** (`.at &= ...`, in the source). Geometry in the circuit file, and a second place for position beside the sidecar (item 7, R1).

**Where they go.** The placer already treats connectors by kind (`is_connector`). It would put holes in the corners of a fixed outline, inset by the keepout, and fiducials in the corners that are left; the sidecar overrides any of it.

**Assembly: per part instance, per catalog row, or neither.**

- A fiducial, a hole and a label are never fitted, and that is always true of them, so the library states it: `.assembly &= "none"` in the part, a `String` field read by the BOM and CPL writers (no BOM line, no CPL row).
- A real part the fab does not stock (no supplier number) or cannot place (not in its assembly stock) is a fact about the catalog row: the number is already there, and today's rule "stop if a part that should be assembled has no number" becomes a row-level `assembly` column: `fab` or `hand`, with its source.
- Whether *this* instance is hand-soldered or not fitted is a design choice. The board writes it on parts it owns (`.j1: Header1x2 { .assembly &= "hand" }`) and a module exposes it for the parts inside (R4). The values are `"fab"`, `"hand"` and `"none"`, checked by lowering with the list in the message. A through-hole part is not hand-soldered by default any more: today `mapping.rs` sets `through_hole` per kind and the exporter treats it as hand, which the fab may not need. Where that default goes (catalog row, or the instance) is part of decision D5.

```nsm
import std::parts::connector::Header1x2

board Example {
    .j1: Header1x2 {
        .assembly &= "hand"
    }
    .j2: Header1x2 {
        .assembly &= "none"
    }
}
```

A part marked `none` is not removed from the electrical model: the solver and the balance still count it, which is conservative for a supply. Real assembly variants (a part present in one variant and not another, a net that changes with it) are not proposed.

**Recommendation: 4A**, test points and holes as parts, `assembly` on the part, with the catalog column for what the fab can or cannot do.

## 5. Silkscreen

**Need from the fab.** Text no smaller than the process allows (`min_text_height` 1 mm, `min_silk_width` 0.15 mm, `min_silk_to_pad` 0.15 mm in `FourLayer`), none over a pad or the board edge. The placer already moves a reference designator out of the way (`Placed::reference_at`) and the DRC gate checks `silk_overlap`, `silk_over_copper` and `silk_edge_clearance`; reference designators need nothing new, except that a fixed small outline may leave no room, and the error then names the designator.

**Free text and labels.**

**Option 5A: a `Label` part with no pins.** The text is a string, the size a `.height` range (the least wins, never below `min_text_height`), and the part is placed like any other (item 7); its footprint is a generated text object.

```nsm
import std::parts::mechanical::Label

board Example {
    .rev: Label {
        .text &= "REV A"
    }
}
```

**Option 5B: a board-level statement** (`nsm-new`: `text "REV A" at ...`) which needs coordinates in source (R1 against item 7).

**Option 5C: derived text.** Print the net name beside each connector pad. That is derivable from the design with no statement, and a board might want it off; it is a later switch on the connector, not text in the source.

**Logos and images.** Out of scope. An image needs a bitmap-to-polygon importer, a file format and a licence record; a logo that is a footprint in a KiCad library could come through the part mapping once IC parts load libraries (issue #64), without a new construct. The fab's order-number marker (whether it prints and where) is a fact on the fab's pages to source; it is not stated here.

**Recommendation: 5A**, reference designators unchanged, no logos.

## 6. Panelisation

Several boards in one order, with a frame, tabs and mouse bites, rails, tooling holes and fiducials. Two shapes were weighed.

- **A feature of `system`.** A system is the electrical join of boards (ADR 0010: it names no nets and adds no copper). A panel is a manufacturing unit: boards of one system may be ordered separately, one board may be panelised in copies, and two systems may share a panel.
- **A separate top-level `panel`** (`nsm-new`):

```nsm-new
panel Order {
    .a: BoardA
    .b: BoardB
    .frame {
        .rail_width &= 5mm
    }
}
```

**Recommendation: defer.** Panels are a fab step and not a circuit step, they add a new definition kind and a new package writer (merging outlines, tabs as routed slots, rails and fiducials), and a fab can panelise from the separate packages that `fab` already writes. It is tracked in issue #62; if built, it is a `panel` and not a part of `system`. Until then each board is ordered on its own.

## 7. Placement: constraints, grouping and where hand placement lives

**Need.** Decoupling capacitors near the supply pins of their part, parts of one module together, a connector on an edge, a part kept away from heat, and a way to keep a layout somebody drew.

**Option 7A: a sidecar layout file beside the source, with no placement in the source.** `Board.layout` (format to choose; one `path: x y rotation side [locked]` line per instance, keyed by instance path such as `stage.output_cap`, which does not change when reference designators are renumbered), read by `build` and `fab`. A part with no entry is placed by the autoplacer; a `locked` entry is fixed and the autoplacer places the rest round it; an entry for a path that no longer exists is an error naming it. The viewer-to-editor step (issue #59) writes the file, and until then `netsmith layout` could write what autoplace chose, so that it can be edited and locked. This is the atopile arrangement the screenshots show (a layout per build beside `main.ato`, plus "Re-autoplace" that re-places what is not locked) and the one that fits R7: netsmith owns the stored placement, the generated `.kicad_pcb` is a pure function of source plus sidecar, and nothing is read back from KiCad. Source stays free of geometry (R2, R1).

**Option 7B: constraints in the source** (`nsm-new`):

```nsm-new
module Decoupled {
    .u: Ic
    .c1: Capacitor
    .c1 near .u.vdd
}
```

A module author can state what is always true of the circuit (a decoupling capacitor belongs by the pin it decouples), and the hint travels with the module, which a sidecar cannot do. But `near` is an unordered relation over geometry that needs a distance and a checker, it cannot say a fixed position (so a sidecar is needed anyway, R1), and the module can name only its own parts (R4).

**Option 7C: derive it.** The circuit already says it: a capacitor between a rail's two pins on the net of a supply pin of a part is a decoupling capacitor (the checker derives `.bias` the same way). The autoplacer weighs those connections by pad distance, and groups parts by module path (the instance path is known). This needs the IC parts of issue #64 for typed supply pins.

**Recommendation: 7A and 7C, no `near` in the language.** Placement geometry is stored in one place (the sidecar), and what the circuit implies is derived. A `near` or `group` statement waits for a case the derived placement gets wrong; adding it then is a decision with a real example. The same sidecar holds fixed positions of mechanical parts, labels and, later, keepouts and the outline's notches.

## Order and effort

Sizes: S under a day, M a few days, L a week or more.

| Step | lang | check | solve | core/lower | pcb | fab | kicad | cli |
|------|------|-------|-------|------------|-----|-----|-------|-----|
| 1 net class on `Rail` (1A) | none | none | none | S: net width into `Design` | M: per-net width in `Rules`, router reach per net, stub at pad width | S: `.kicad_pro` widths | none | none |
| 3 fixed outline (3A) | none | S: `Outline` member found, fields | none | S | M: fit check and error; L: placer that fits; M: rounded edge | S: arcs in profile | none | none |
| 4 mechanical parts, `assembly` | none | S: field check | none | S | M: footprints, placement rule | S: skip BOM/CPL | M: symbols and stock footprints (mapping) | none |
| 2 pours (2A) | none | S: net rule already covers `.top.net` | none | S | M: zones on outer layers, L: stitch via grid after routing | M: fill outer layers before check and plot | none | none |
| 7 sidecar and locked placement | none | none | none | S: instance paths to `Design` | M: fixed parts in the placer | none | none | M: read, write, stale-path error |
| 7 placement quality (derived grouping) | none | none | none | M | L | none | none | none |
| 5 `Label` | none | S | none | S | M: text footprint, placement | S | M | none |
| 6 panels | deferred | | | | | | | |

No new grammar is needed by the recommended options, which is deliberate: each is an interface or a part, narrowed with `&=`. The std additions are `std::pcb` (`Pour`, `Outline`) and `std::parts::mechanical` (`MountingHole`, `PlatedHole`, `Fiducial`, `TestPoint`, `Label`).

Suggested order, each step passing the existing gate: 1, 3, 4, 2, 7 (sidecar), then placement quality, 5.

## What the router, placer and DRC gate need

- **Router.** A width per net (reach and clearance per net in the masks; a track at the process minimum for the last stretch into a pad narrower than the track, so a wide net does not violate pad clearance); the board edge as a rounded rectangle; keepout disks round holes and fiducials; stitching vias placed after routing in cells the via mask leaves open; a pour's clearance reserved when the pour is cut (KiCad does the fill, the router only leaves it room).
- **Placer.** Fit inside a given outline and fail with the overflow in x and y; fixed (locked) parts from the sidecar; mechanical parts at the corners; connectors toward edges; decoupling and module grouping by connectivity; reference text left room.
- **DRC gate.** KiCad's check as now, with the outer zones filled by the staged copy (ADR 0012's Python step, extended to the outer layers), and the Gerbers plotted from the filled copy. New netsmith-side checks before KiCad: outline holds every courtyard; holes inside the outline and clear of the edge; plated pad leaves the least annular ring; text height at least the process's; every rail's resolved width at least what its current needs (the derived check of item 1, later). A package whose `.kicad_pcb` stays unfilled and deterministic on disk is unchanged.

## Decisions the user must make first

1. **D1, where placement lives (item 7):** sidecar file (7A, recommended), or constraints in source (7B). It decides items 3, 4, 5 and 7, because it says where positions of holes, labels and locked parts are written.
2. **D2, net class carrier (item 1):** field on `Rail` (1A), a class listing nets (1B), or a `net` statement (1C).
3. **D3, where outer pours are declared (item 2):** in the process beside the planes (2A) or as a board member (2B).
4. **D4, the outline (item 3):** a board member `Outline`, a missing one meaning "size to fit", and ranges allowed on width and height.
5. **D5, assembly (item 4):** a string field `.assembly` on the part with the values `fab`, `hand` and `none`, with a catalog column for what the fab cannot place, and whether a through-hole part defaults to the fab or to hand. Built provisionally in ADR 0024; the decision is still the user's.
6. **D6, panels:** defer (recommended) or design a `panel` now.

## Consequences

Each recommended option adds an interface or a part and no grammar, so ADR 0001 and ADR 0004 hold, and a board names its nets and its stackup layers as before. The cost is a sidecar file, a second artefact next to the source that has to be kept in step with it (the stale-path error is the guard), and a placer that has to become much better than the present row layout before a fixed outline is useful. Known limits that remain until their steps land are in issue #60; this ADR changes neither until a decision is made.
