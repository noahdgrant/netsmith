## The shape of a board

Beyond its parts and nets, a board can say how wide its power tracks are, how big it is and what is cut out of it, where copper is poured, and where tracks, vias, copper or parts are not allowed (ADR 0020, ADR 0029). Each is a member of the board, or a field of one, in the standard library: `std::pcb` has `Outline`, `Cutout` and `Keepout`, `std::mfg::stackup` has `Pour`, and a `Rail` has `.min_track`. The track widths and the pours are what the circuit's checks consume, and they stay in the board. The outline, the cutouts, the keepouts a board draws, the mounting holes, the fiducials, the labels and every requirement on where a part goes are **physical intent**, and a board states them in its physical file (ADR 0048), described first. A board that says none of it is laid out as before.

Lengths are in the board's own frame: the origin is the outline's top left corner, x grows to the right and y downward. A board states how big a shape is. Where a shape is, when the board does not say, is in the layout file (see below).

### The physical file

Physical intent is what the product, the case and the layout designer require of a board, as opposed to what the circuit requires: the outline, openings, mounting holes, where a connector must be, a fixed spacing between two parts, the face of a module, a keepout. It is requirements, never solutions. The circuit source stays strictly the circuit, and a case revision is a diff of its own file (ADR 0048).

It is in `<stem>.physical.nsm` beside the source, found by stem as the layout file is: `board.nsm` has `board.physical.nsm`. The file is optional and never needed for a board that states no physical intent. It holds one unit `physical for Board { ... }` for each board it speaks for (the file may hold the units of several boards), and the statements in a unit are the language of a board's body: declarations, `&=`, `==`, `>=` and `<=`. The units are put with their boards before anything else is checked, so a `panel` or a `system` that places the board sees them too.

```nsm solve place
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

module Sensor {
    .j: Header1x2
}

board Cased {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .power: Header1x2
    .sensor: Sensor
}
```

```nsm-physical
import std::pcb::{Cutout, Keepout, Outline}

physical for Cased {
    // The case: numbers from the case drawing.
    case {
        .outline: Outline { .width &= 40mm; .height &= 24mm; .corner_radius &= 1mm }
        .window: Cutout { .x &= 30mm; .y &= 2mm; .width &= 6mm; .height &= 3mm }
    }

    // Case drawing, slot A: the header's body is 1.5 mm in from the left edge, centred.
    .power.place.left &= 1.5mm +/- 0.1mm
    .power.place.y >= .outline.height / 2 - 0.1mm
    .power.place.y <= .outline.height / 2 + 0.1mm
    // A part inside a module is named by its path: the sensor's header stays right of 20 mm.
    .sensor.j.place.left >= 20mm
    // The sensor block is on the bottom face, every part of it.
    .sensor.place.side &= "bottom"

    .rib: Keepout {
        .x &= 12mm
        .y &= 0mm
        .width &= 6mm
        .height &= 24mm
        .layers &= "top"
        .max_height &= 4mm
    }
}
```

- **`case { ... }`** is the section for what the case fixes: the outline, the cutouts, the mounting holes (`MountingHole`), the fiducials (`Fiducial`) and the labels (`Label`) of the board, and the case's own STEP file (`CaseModel`, see _A case model_). It holds the same statements as the rest of the unit, and lifts out unchanged into an `enclosure` that several boards share (_A shared enclosure_, below). These six belong in it; a keepout may be anywhere in the unit.
- **A path crosses a module boundary here for any statement of physical intent.** `.sensor.j.place.left` names the header inside the `Sensor` module. The circuit source reaches into a module only to narrow a value (ADR 0116); the physical file can also state physical intent about the part. A statement may set or compare only a part's `.place`, a part's `.height` and `.height_growth`, a part's `.standoff`, a part's or the board's `.routing_margin`, a `soft margin` round a part, and what the unit declares itself (the fields of its `.outline`, `.window`, `.rib`). Anything that changes what a part is, or a connection, is refused.
- **A relation between two parts** is the equation of _Where a part goes_: `.b.place.x - .a.place.x == 17.5mm +/- 0.05mm`. The placer holds it and `fab` measures it (_A relation between two parts_).
- **A part near a pin** is `.antenna within 8mm of .radio.ant` (_A part near a pin_), and **a net on one layer** is `.radio.ant route one_layer` (_RF lines and a net on one layer_). The placer and the router hold them and `fab` measures them.
- **`.place.side` on a module** is a requirement on every part of it, at any depth. A module with an exception states the faces of its parts one by one and none for itself; two lines that disagree are an error that names both. `.place.side` with no path, on the board itself, is the same requirement on every part of the board (_A board on one face_).
- **`.silk` and `.silk_text`** say what the silkscreen prints for a part: nothing, or a text in place of its designator (_Silkscreen text_). `.silk` on a module or on the board reaches every part inside it.
- The case numbers are the maintainer's file to review: put the source of each (a drawing, a sheet) in a comment above the line, and never invent one.

`examples/enclosure_sensor/` is a complete invented board that states all of this in its physical file and the enclosure it names (`src/case.nsm`, _A shared enclosure_), and each statement has a comment that says what it asks for: a rounded outline with a cutout, mounting holes and fiducials, a 40 mm plug that hangs over an edge, an antenna and a coax connector on one line, two socket rows a fixed distance apart, a module on the back with one part on the front, a row of twelve test pads on a pitch, a height limit, and a flat cell on the back across the sockets whose pins protrude 0.2 mm (_Pin protrusion_). It is a project, because it defines four invented parts and maps them to KiCad's stock footprints and a footprint of its own in its `kicad.toml`; `crates/netsmith-fabricate/tests/enclosure_sensor.rs` runs `fab` on it and checks that every requirement is met.

The placer keeps to every requirement, `fab` measures them on the board and stops on one not met, and the editor draws the file's outline and keepouts. A requirement that is a single value fixes the part, like a lock in the layout file.

Misuse: the old place.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::pcb::Outline

board Old {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .outline: Outline { .width &= 40mm; .height &= 24mm }
    .j: Header1x2
    .j.place.left &= 1.5mm
}
```

```text
error: `.outline: Outline` is an outline, which is physical intent, and a board's circuit source does not hold it (ADR 0048); declare it in the `case { }` section of `physical for Old` in `main.physical.nsm`
  --> main.nsm:7:15
  |
7 |     .outline: Outline { .width &= 40mm; .height &= 24mm }
  |               ^^^^^^^
error: `j.place.left` is a placement requirement, which is physical intent, and a board's circuit source does not state it (ADR 0048); put it in `physical for Old` in `main.physical.nsm`
  --> main.nsm:9:5
  |
9 |     .j.place.left &= 1.5mm
  |     ^^^^^^^^^^^^^^^^^^^^^^
```

Misuse: a physical file that changes what a part is.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

module Block {
    .j: Header1x2
}

board Wrong {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .b: Block
}
```

```nsm-physical
physical for Wrong {
    .b.j.assembly &= AssemblyMode::Hand
    .b.j.p1 ~ .b.j.p2
}
```

```text
error: `b.j.assembly` is not physical intent: a physical file states where a part is (its `.place`), how tall it is (`.height`) and how much it may grow (`.height_growth`), how high its body stands off the board (`.standoff`), how far its pins protrude (`.pin_protrusion`), the STEP model it is drawn from (`.model`), the room for routing kept round it (`.routing_margin`), what its silkscreen prints (`.silk`, `.silk_text`) and the outline, cutouts, keepouts, holes, fiducials, labels and case models it declares, never what a part is; put this in the circuit source
  --> main.physical.nsm:2:5
  |
2 |     .b.j.assembly &= AssemblyMode::Hand
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: a physical file states where things are, and a connection with `~` changes the circuit; put it in the circuit source
  --> main.physical.nsm:3:5
  |
3 |     .b.j.p1 ~ .b.j.p2
  |     ^^^^^^^^^^^^^^^^^
```

Misuse: a unit for a board that is not there.

```nsm fails
import std::mfg::jlcpcb::TwoLayer

board Right {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
}
```

```nsm-physical
physical for Left {
}
```

```text
error: `physical for Left` names no board of the source it is paired with; its boards are `Right`
  --> main.physical.nsm:1:14
  |
1 | physical for Left {
  |              ^^^^
```

#### A shared enclosure

**Several boards in one case share its description (ADR 0132).** An `enclosure` is a definition whose body is what a `case { }` section holds, lifted out unchanged: the outline, the cutouts, the mounting holes, the fiducials, the labels, the keepouts and height limits of the case, and the case's STEP files (`CaseModel`), with the statements about them. A board's physical unit puts the board in it with `case Name`, and the enclosure's statements are then read as if they were written in the unit's `case { }` section: its members are members of the board, the rest of the unit names them as before (`.outline.height`), and a panel or a system that places the board sees them as it sees any of the board's physical intent. A case revision is then a diff of one file, which every board in the case picks up.

```nsm module=src::snap_case
//! The snap case: numbers from its drawing.

import std::pcb::{Cutout, Keepout, Outline}

enclosure SnapCase {
    // Drawing SC-1: the inside of the base.
    .outline: Outline { .width &= 40mm; .height &= 24mm; .corner_radius &= 1mm }
    // Drawing SC-1: the window for the indicator.
    .window: Cutout { .x &= 30mm; .y &= 2mm; .width &= 6mm; .height &= 3mm }
    // Drawing SC-1: the lid's rib leaves 4 mm above the board along the right side.
    .rib: Keepout {
        .x &= 36mm
        .y &= 0mm
        .width &= 4mm
        .height &= 24mm
        .layers &= "top"
        .max_height &= 4mm
    }
}
```

```nsm solve
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

board Logger {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .power: Header1x2
}

board Relay {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .power: Header1x2
}
```

```nsm-physical
import std::parts::mechanical::Label

import src::snap_case::SnapCase

physical for Logger {
    case SnapCase
    // Case drawing, slot A: the header is 1.5 mm in from the left edge.
    .power.place.left &= 1.5mm +/- 0.1mm
}

physical for Relay {
    case SnapCase
    // What this board adds to the case: its name on the silkscreen.
    case {
        .name: Label { .text &= "RELAY" }
    }
    .power.place.y >= .outline.height / 2 - 0.1mm
}
```

- **Where it is written.** In a file of its own under `src/` (`src/snap_case.nsm` is the module `src::snap_case`), which the physical files of the boards import by name, as a module is imported; or in a physical file, where the units of that file use it. An enclosure is physical intent, so a file that defines a board, a system or a panel does not hold one. The mechanical side owns the file: `netsmith import dxf` prints the outline, cutouts and hole circles of a drawing to paste into it, and its `CaseModel` names the STEP file of ADR 0109.
- **Its names, imports and lines are its file's.** An enclosure imports the types it uses itself, a finding about one of its lines points at its file, and a `CaseModel`'s `.file` is relative to the file of the enclosure, as it is to the file that states it everywhere.
- **A board is in one case.** A unit names at most one enclosure. What one board adds to the case (a fiducial, a label) goes in the unit's own `case { }` section beside `case Name`, and a name that both declare is declared twice. An enclosure names no other enclosure, and holds no `case` of its own.
- **It is not a member.** `.case: SnapCase` is refused: nothing in the circuit refers to the case, and the enclosure's members are the board's, with the names the `case { }` section gave them.
- **On its own**, as the editor and `netsmith board check` read a file of `src/`, an enclosure is checked as the case of a board: what a `case { }` section refuses, it refuses.

Misuse: an enclosure in a board's circuit source.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::pcb::Outline

enclosure Tray {
    .outline: Outline { .width &= 40mm; .height &= 24mm }
}

board Held {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
}
```

```text
error: `enclosure Tray` is physical intent, and a file that defines a board (`Held`) is circuit source, which does not hold it (ADR 0048); put it in `main.physical.nsm`, or in a file of its own under `src/` that the physical files of several boards import
  --> main.nsm:4:11
  |
4 | enclosure Tray {
  |           ^^^^
```

Misuse: a name that is no enclosure.

```nsm fails
import std::mfg::jlcpcb::TwoLayer

board Held {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
}
```

```nsm-physical
physical for Held {
    case SnapCase
}
```

```text
error: `case SnapCase` names no enclosure: this file neither defines nor imports `SnapCase`; import it (`import src::case::SnapCase`), or define `enclosure SnapCase { ... }` in this physical file
  --> main.physical.nsm:2:10
  |
2 |     case SnapCase
  |          ^^^^^^^^
```

### How tracks bend

netsmith's router bends a track by 45 degrees only (issue #415). Its first pass searches in eight directions, where a 45 degree bend costs a little and an acute one is never a move; every right angle a path has is cut into two bends of 45 degrees, measured exactly against the other copper, and a line whose corner cannot be cut is searched for again with no right angle, or left unrouted, never drawn with the corner. Only where a corner falls inside the copper of a pad or a via of its own net (the stub from a pad's centre to its first step) does a track turn more sharply, since no corner shows there. `fab` asks the design rules check for its rule `track_angle`, which refuses a corner of more than 45 degrees outside a pad or a via of the net, so a board netsmith routed has none. The check of a board file alone (the `drc` example, a port read from KiCad) leaves the rule out.

Once the board is routed and has its teardrops, each 45 degree bend where two straight tracks of one width meet becomes an arc tangent to both, a rounded bend, so the copper has no corner (issue #431, step 11): the largest radius up to four track widths that keeps the rules' space from all other copper, the teardrops, the pads, the holes, the keepouts and the edge, measured exactly, or else a smaller one down to one width; a bend that none fits keeps its corner. An arc takes at most half of each track it joins and never the part a teardrop lies along; of a track with a bend at each end it leaves at least the router's shortest piece (0.14 mm, or two track widths where that is more) straight between the two arcs, so no straight piece of a hundredth of a millimetre is left (issue #1486), and a short diagonal that cannot keep that keeps its two corners; and a bend inside a pad or a via of its net, or where another track of the net joins it, is left as it is. The search and the copper it settles are as before, so the same lines route with the same vias and teardrops. The board file writes an arc as KiCad's `(arc (start) (mid) (end))`, the Gerbers draw it as one, `track_angle` reads it at its ends along its tangents, and the scorecard counts arcs and measures length along them. Measured on the three ported boards (2026-11-13, the agent's decision under the owner's delegation): at four widths 78 % of RP2350's 391 bends, 64 % of Orbit's 250 and 77 % of the Easyduino's 186 become arcs, routed length drops by 0.3 % (RP2350 1344 to 1340 mm, Orbit 918 to 916, Easyduino 849 to 846), the check finds no new error and `fab` takes no longer; two widths round a few more bends and save less length, six round fewer. `netsmith dev route --arcs 0` draws none, for comparing.

Before the arcs, the router's copper is drawn clean (issue #1486), with the searches and what they route as before: each line's tracks end at its pads' and vias' centres, with no short track from a centre to a point still inside the pad or via (a pad's run is moved across to pass its centre, or drawn on from the centre to meet the run just outside the pad at a bend of 45 degrees or less); a stair of pieces shorter than 0.14 mm, or than two track widths (what the quality section of `board nets` counts as short), that the grid leaves at a bend is joined into the runs beside it (one bend of 45 degrees or less, a cut right angle, or a bend of 45 degrees each way for a jog); and where two lines of a net join the same two places twice, the longest piece of the loop and what is left going nowhere are taken off, where that leaves every bend as `track_angle` asks. Each change keeps the rules' space from all other copper, or is not made.

### Teardrops

A board netsmith routes gets a teardrop where each track meets a pad or a via (issue #431): copper that widens the track into the pad, so that a drill that wanders or an etch that undercuts does not break the joint. It is as wide at the pad as 90 % of the pad's copper across the track, at most 2 mm, and reaches half of that beyond the pad's edge, at most 1 mm and no farther than the track runs straight, on the track's layer and net. netsmith draws one only where it keeps every space the design rules check asks of a track (to other nets' copper, holes, solder mask openings, the board edge and rule areas, and on a surface-mount pad the process's space between surface-mount pads, so a fine-pitch part's gaps stay open); elsewhere the joint has none. A teardrop no wider than the track would gain nothing and is not drawn. The board file has each as a KiCad 9 teardrop zone with its fill, so the Gerbers carry the copper, a pour of another net keeps its clearance from it, `fab`'s check measures it, and KiCad reads it as a teardrop. The language has no switch for them; the route scorecard counts no teardrop as track.

### Escape lanes round fine-pitch parts

When the router's first try leaves connections on a board with a fine-pitch part (at least 16 surface-mount pads, two of them 0.55 mm apart or nearer, on at least three sides, such as a QFN or a QFP), its second try routes the lines in the same order again with an escape lane in front of each of the part's pads (issue #431): the 1.5 mm straight out from the pad, on its layer, which other nets' tracks pay a little to use and where any via pays a lot, except the pad's own via at the lane's end. A lane is a cost and not a wall, so a line with no other way still crosses one. It keeps the part's lines from closing each other's way out with vias against its pins. A board that routes on its first try is routed as before, and a board with such a part may have one try more.

The placer keeps the lanes free of parts (issue #830): a fine-pitch part carries an escape ring, the box round its surface-mount pads grown by the 1.5 mm of a lane, and no other part's courtyard on its face may reach into it. It is a keepout the placer keeps as it keeps the keepouts a part carries, so a board with no other place for a part does not place, and its message names the part that is in the ring, how far, and the fine-pitch part whose ring it is. A capacitor that decouples the part may stand in it, near the pins it decouples, and so may a part that a datasheet's `near` puts by its pins; but such a part keeps 1 mm from each of the part's pads on a net it is not on (issue #1010), so that the tracks of those pins leave between the two with room for a via. A capacitor pressed against a QFN's signal pins left room for one track and no via, and the board did not route. The ring is the placer's: a part placed by hand in it (a layout file's place for both parts) is not refused, and `fab`'s check measures no ring.

The placer also counts what a part with 16 pads or more leaves its tracks (issues #997 and #1035). Each pad on the edge of the part's pads whose net reaches another part (not a plane) leaves through its side. Where the board's edge (less the copper to edge space) or a part on the same face is nearer that side than 1.5 mm, those pads must leave along the gap, and the pads beyond the tracks that fit there are its escape shortfall. Each net short costs as much as 2 mm of wire. So that a part as big as an 80-pad module can leave an edge it was placed against, the placer also moves such a part with the parts it lands on. Over 32 seeds of a tight test board of 99 parts round such a module, one pass of the router left 692 lines with this and 818 without. A layout file's places and the parts a requirement holds are kept as ever.

### Room for routing round a part

The placer keeps courtyards apart and lets them touch, and `layout fit` packs a part courtyard to courtyard, so a dense board can leave no room between two parts for a track or a via, and the router then reports pads `walled in by pad(s)` of their neighbours (issue #1313). A board and each part have the built-in member `.routing_margin` (a length, one value, zero or more, 0 unless stated): the room for routing kept round the part's courtyard. The placer and `layout fit` grow every courtyard by its part's margin on each side before they measure it against other parts' courtyards, so two neighbours keep the sum of their margins between them, as hard a rule as an overlap: a board with no arrangement that keeps it does not place, and the refusal says that the courtyards were grown by their routing margins. A part's own `.routing_margin` beats the board's, so a busy part can ask for more room than the rest (or a small part for none). It is stated as `.height` is, in the circuit source or the physical file, by the part's definition, a user part or the board.

The margin is between parts only: the board's edge, keepouts, cutouts and height limits measure the courtyard as before, and `fab`'s placement check (and `layout check`) do not read it, so a part placed by hand nearer than the margins is not refused. `layout fit` gives no place nearer another part than their two margins, and of a place that is, it says which part, how near, courtyard to courtyard, and what the two margins ask for (`D1 is 1.09 mm from R1, courtyard to courtyard, and their routing margins ask for 1.5 mm ...`), in its `margins` with `--format json`.

```nsm solve
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::{Capacitor, Resistor}
import std::sources::Rail

board Roomy {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .rail: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC" }
    .r: Resistor { .resistance &= 10kohm +/- 1% }
    .c: Capacitor { .capacitance &= 100nF +/- 20% }
    .rail.pos ~> .r ~> .c ~> .rail.neg
    // Half a millimetre round every part: a track and its space between two neighbours.
    .routing_margin &= 0.5mm
    // The capacitor asks for none: it stands by the resistor.
    .c.routing_margin &= 0mm
}
```

Misuse: a margin is one value, zero or more.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor

board Cramped {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .r: Resistor { .resistance &= 15kohm +/- 1% }
    .r.routing_margin &= -0.2mm
}
```

```text
error: Cramped: `r.routing_margin` is -0.2 mm, and a margin for routing is zero or more: it is the room kept round a courtyard for tracks and vias
```

**A pad walled in.** Where the parts are placed, before a route is tried, `fab` warns of each surface-mount pad that other parts wall in, and `layout check` says it as a `walled_in` warning among the parts the layout file places (it does not make `ok` false or stop `fab`):

```text
warning: B: pad 1 of `stage.r` (R1) (net `VCC`) is walled in by `stage.c` (C1) (pad 2), `stage.r` (R1) (pad 2): no 0.1 mm track leaves it 0.1 mm from other copper and no via fits beside it, within 1.5 mm; move C1 away from it, or give the parts a `.routing_margin`
```

A pad on a net that reaches another part is measured on its copper layer, 1.5 mm round it (the router's escape lanes): a track's centre may not come nearer another net's pad (its own part's too) than the process's clearance and half the router's track, nor nearer the edge than the copper to edge space and half a track. The pad has a way out where its track reaches 1.5 mm from it, or a place beside it where the router's via fits. A pad with neither is walled in, and the finding names the parts whose pads close it, the edge too; a pad only its own footprint walls in is not said. Not measured: a through-hole pad, a pad the process allows a via in (as the router's last way out) that is wide enough for one, the other face under a via, keepouts and cutouts, and copper the routes file fixes, so the router may still find a way, or report a line walled in that this does not.

**A part with no room.** Before the placer starts, `board check`, `board build`, `layout check` and `fab` measure each part the placer places (one with no entry in the layout file, or an unlocked entry) against the room the parts that stay where they are leave it (issue #1283): the layout file's locks and the parts a requirement of the physical file pins. A part that no free rectangle of a face holds is a warning, `no_room` in `layout check`, naming the part and the largest free rectangle of each face it may go on, in the layout file's frame:

```text
warning: B: `radio` (U3) has no room among the parts that stay where they are: no free rectangle of the top or the bottom holds its courtyard of 23 x 27 mm turned 0, 90 degrees, inside the edge wall and clear of the locked and pinned parts, the cutouts and the keepouts; the largest free rectangle is 18 x 30 mm at (2, 4) on the top, and 30 x 12 mm at (2, 34) on the bottom: move or unlock a locked part, give the board more room, or choose a smaller part
```

With `--strict` (or `guidelines = "error"` in netsmith.toml, for `board check`, `board build` and `fab`) it is an error, and `fab` stops at it before placing, instead of trying every seed for its whole budget first. The free space is that of the summary's faces (`largest_free` of `layout summary`): inside the outline and the edge wall, outside the cutouts, the keepouts that keep parts off the face and the room the parts that stay take on it (a part with a hole on both faces, as the placement check counts it). The part's courtyard box is tried at each right angle (and at 45 degrees, as the box round the turned courtyard, for a part whose `.place.turn_step` is 45) on each face it may go on (the one a requirement states, else both). The grid is 0.25 mm (coarser on a board more than 200 mm on a side), and a part needs a cell less than its size on each axis, so a part that fits is never said; one within a cell of the limit may be let through. Not measured, so not said: the other parts the placer places (each part is measured alone against the parts that stay), the parts' routing margins, the other face of a part with a hole, the requirements' regions and heights, a part that may hang over an edge, and a board whose outline is not stated. A part bigger than the whole outline is an error of its own, said before this.

#### A soft margin round a part

A hardware design note often says to keep some distance round a part free of other parts: 3 mm round a radio module for soldering and rework, room for an antenna. It is neither a courtyard nor a copper keepout, and it is no `.routing_margin`, which is as hard as an overlap. It is a soft requirement of the physical file (issue 1310, owner decision): `soft margin 3mm round .u1`. The distance is one fixed length of zero or more, the part is named from the board with a leading dot (a path may cross a module's boundary, as `.radio.u1` does), and a part states one margin. The statement belongs in a board's physical file; in the circuit source and in a module's own physical file it is an error that says so, and a name that is not a part (a module, a pin, a value) is refused.

The placer keeps the courtyards of other parts out of the margin where it can: a courtyard inside it costs more for each mm than a plain gap and than a decoupling capacitor's pull to its pins, so the margin is kept wherever the board has the room, and a stronger pull may still win. It is no wall, so a board that cannot keep it is still placed. The margin is measured from the part's courtyard to the other part's courtyard (the larger of the spaces along x and along y, as `.routing_margin` is) on the part's face, and a part with a hole on both; the board's edge, keepouts and cutouts do not count, and only the part that states the margin asks for it.

```nsm solve place
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::sources::Rail

board Serviced {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .rail: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC" }
    .r1: Resistor { .resistance &= 15kohm +/- 1% }
    .r2: Resistor { .resistance &= 15kohm +/- 1% }
    .rail.pos ~> .r1 ~> .rail.neg
    .rail.pos ~> .r2 ~> .rail.neg
}
```

```nsm-physical
import std::pcb::Outline

physical for Serviced {
    case {
        .outline: Outline { .width &= 30mm; .height &= 20mm }
    }

    // Design note: keep 3 mm round R1 free of other parts, for the soldering iron.
    soft margin 3mm round .r1
}
```

Where the placed board has a part inside a margin, `fab` says it as a warning, after placing and before routing, and it is an error when the run is strict (`--strict`, or `guidelines = "error"` in netsmith.toml), which stops `fab` there:

```text
warning: Serviced: R2 is 1.20 mm from R1, courtyard to courtyard, and the soft margin round R1 asks for 3 mm (main.physical.nsm:9); move R2 away, or give the margin less
```

`layout check` says the same as a `soft_margin` finding among the parts the layout file places (a warning, an error with `--strict`), at the line of the entry of the part that is inside the margin. Not built: a margin round a module's box (name a part), a margin that differs by face, and a margin that excuses the module's own decoupling parts; state a smaller margin, or none, where a part must stand near.

Misuse: a margin is round a part.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

module Pair {
    .j: Header1x2
}

board Misused {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .pair: Pair
}
```

```nsm-physical
physical for Misused {
    soft margin 3mm round .pair
}
```

```text
error: `soft margin 3mm round .pair` names `.pair`, which is a module; a soft margin is kept round a part's courtyard, so name the part (`soft margin 3mm round .u1`)
  --> main.physical.nsm:2:27
  |
2 |     soft margin 3mm round .pair
  |                           ^^^^^
```

### How the router orders the nets

Every connection the router draws has a class, derived from the board with nothing written in the source (issue #416). In the order the router routes them:

1. **RF**: a net with a stated impedance of one track (a `TrackImpedance`, issue #1274; see _RF lines and a net on one layer_). It comes first in every order, the first try's too, and from the second try on its copper is not ripped up for a later class.
2. **Escape**: a connection that must reach a plane, or that ends on a pad deep in a fine-pitch grid of pads, which has the fewest ways out.
3. **Decoupling**: a decoupling capacitor's pad to the power pin it decouples, the pair the route scorecard measures.
4. **Pair**: a net of a diff pair (see Diff pairs).
5. **Clock**: a net on a crystal (a part of the kind `Crystal`, or that provides it), or whose name has `clk`, `xin`, `xout` or `xtal` in it, but not a net with a plane or a pour (a crystal's ground).
6. **Power**: a net that carries at least 100 mA.
7. Everything else.

The order is fixed. When the router's first try leaves connections, its second try routes the lines of the first try again with the classes in turn, each class in the order it had, and every later try keeps the classes in turn too. On those tries the copper of a decoupling, pair or clock connection is never ripped up for a connection of a later class: a later connection goes round it or is left, and the pass that takes track off the pours (see Pours) leaves it where it is too. An RF connection is protected as these are. An escape or power connection keeps its place in the order but may be ripped up and routed again (a power net at its full width). The first try is the router's own order with no classes, so a board that routes on its first try is routed exactly as before. This was decided by the agent under the owner's delegation of 2026-11-13: protecting every class left a dense board with a QFN unrouted, and routing the classes in turn on every try made one try of such a board finish less often (issue #431).

### Connections left: rounds of rip-up

When a try of the router still leaves connections after its other stages (the try with less room, ripping up the nets in a connection's way, the negotiation and the chain of rip-ups), it gets rounds of rip-up of whole nets (issue #1088). In each round, for each connection left, the router finds the cheapest way that may cross other nets' copper at a price, removes all the copper it laid of the nets that way crosses, routes the connection through the room they leave, and then routes those nets again; a connection that then finds no way is left for the next round, and the cells the rounds fought over cost more from then on. A try gets 6 rounds, one net is ripped up at most 3 times, and the router keeps the state with the fewest connections left that any round reached, or the one before the rounds when none did better. Every bound is a count, so a board gives the same copper on every machine; the route timeout stops the rounds as it stops the rest. A board that routes before the rounds is not touched by them, and a try the rounds finish does not end the router's tries: it is kept only when none of the router's other orders of the connections routes the board without the rounds, so a board that routed before keeps its copper. Copper of the routes file is never ripped up, locked or not, and the router adds none to a net with `lock = true`.

`board fab --rip-rounds N` (and `netsmith dev route --rip-rounds N`) sets the rounds for a run, `rip_rounds = N` in `netsmith.toml` for a project (chapter _Projects and netsmith.toml_), and 0 turns them off.

When connections are still left, `routing-failure.txt` says for each the stage it last failed in and what is in its way: the nets whose copper the cheapest crossing way meets, or, when no way crosses only other nets' copper, the pads (by part and pad), the vias of the routes file (by net and place) and the board edge that wall an end of it in. The report looks for these past the route timeout too, for up to 32 connections.

### Nets the routing made long

A line that must be short (a crystal's, a clock's, a boot capacitor's, a switching node's) can come out long on a routed board, round other copper or through vias. `fab-notes.txt`, the notes of `board drc` and the route scorecard list such nets, with nothing written in the source (issue #1194). They measure the nets that no zone pours, from the board file, so a board netsmith routed and one a person routed in KiCad are measured alike:

- `routed long:` each net whose tracks are more than 2 times its _Straight length_ (the shortest tree of straight lines between the centres of its pads) and at least 2 mm over it, the longest ratio first, one line each: `XOUT: 14 mm routed, straight 4 mm (3.5x), 2 vias`. The 2 mm keeps out a net of a millimetre that a short jog doubles.
- `most vias:` the five nets with the most vias, of those with two or more, most first. A poured net's vias are left out: they are stitching, put there on purpose.

With `--format json`, `board fab` and `board drc` have the same lists as data in each board's `routes` (`long` and `most_vias`, each net with `net`, `length`, `straight`, `ratio` and `vias`); the scorecard has them as `nets`. They are notes and never stop a run. A net that must be shorter than a number says so in a part's `layout` block, `route NAME .pin length LENGTH` (chapter _Parts_), and a routed net longer than that is a guideline of `fab`. The router's order does not change for either: a net on a crystal is already a Clock connection (above).

### Decoupling capacitors

The router routes a decoupling capacitor (a _Decoupling pair_ of the placer, see the glossary) the textbook way, with nothing written in the source (issue #431). On the supply net, the line from the power pin the capacitor decouples (the nearest of the part's power pins on the net, if it is at most 5 mm from the capacitor's pad) goes to the capacitor's pad, whatever pad of the net is nearer the pin; the rest of the net joins as it would. On a net that is a plane, that line takes the place of the pin's own via, so the pin reaches the plane through the capacitor's via. The other net of the pair, the return, is the one with a pour or a plane (of two, or of none, the one with more pads), and is routed as before; where it is poured on an outer layer the capacitor's pad is not on (ground poured on the back of a two-layer board, the capacitor on the front), the capacitor's pad gets a via of its own to the pour once a try has routed every connection, on a track of at most 1.5 mm, unless a via of the net is within 1.5 mm of the pad already. The capacitors are taken in the order of their pads' names, so two capacitors near enough to share a via give the same one whatever order the parts come in (issue #718). The via is never in the way of another connection, so a board routes as completely as without it. A bulk capacitor farther than 5 mm from the pins is routed as any other part.

Which part a capacitor decouples is found from the nets, not from names (issue #821). A part whose pin table calls its supply and return pins `power` is decoupled by a capacitor between those two nets. A capacitor between two supply nets (nets that a `power` pin of some part is on) also decouples an IC, a connector or a part with no kind that has a pin of any kind on its rail, whatever the pin table calls that pin: an `input` or `passive` supply pin of a module, an imported part or a header is next to its capacitor as a `power` pin is. A resistor, a diode or an LED that hangs on a rail is not decoupled by its pin, and a capacitor between two signal nets (no `power` pin on either) decouples nothing. Of the parts, the ones that share most of the capacitor's path are kept, so a module's capacitor goes with the module's own part.

A part's datasheet may say more, in the part's `layout` block (chapter _Parts_, _What the datasheet says about layout_): each part a `near` statement finds from the nets is pulled toward the pin it names as a decoupling capacitor is to its part, a part a `loop` finds toward both its pins, a part an `away` finds is pushed out to the distance it gives, and the nearest pad of an `edge` statement's pin is pulled to the outline at the same weight. A `keepout` statement is a keepout the part carries, which the placer, the router and the pour keep to as they keep to any other; a `route` width is the least width of the pin's net; and the router puts the vias a `vias` statement asks for in the pin's pads, after routing and before the pours are stitched, where the process fills vias in pads and the net has a plane or the other outer layer's pour to reach. `fab` warns when the board does not meet a number the datasheet gives, unless the layout file waives the guideline with a reason.

### Kelvin sense connections

A sense pin that a part's `kelvin` statement names (chapter _Parts_, _What the datasheet says about layout_) is routed from the pad it senses, alone (issue #1148). While the board is routed, the sensed pad is cut in two across its longer side: the half nearer the sense pin, with the sense pin's pads, is routed as a net of its own, and the other half stays the net's. So every pass of the router keeps the rules' space between the sense track and the rest of the net, as between two nets, and the sense pin is no end of any other line of its net, wherever the net's shortest tree would have joined it. The gap between the halves lets a track of either end at the centre of its half and keep the space from the other: the larger of the net's clearance and 4/3 of (the clearance, half the wider track and 0.15 mm, less a quarter of the pad's longer side). When the router is done, the pad is whole again and the sense track is the net's, touching the rest of the net in the sensed pad only.

A net with a pour, a plane or a zone of the routes file is routed the same way (issue #1174), as in low-side sensing, where the shunt's ground side is the ground pour. A pour joins every copper of its net it reaches, so the board file carves it round the sense copper: on each layer the net is poured on, a rule area that forbids pours (KiCad's keepout with `copperpour not_allowed`, named `kelvin U1.2`) covers the sense pin's tracks on that layer and their teardrops, its vias (which have copper on a plane's layer too) and the sense pin's pads with copper there, each grown by the larger of the pour's clearance and the net's. The pour keeps the space from the sense copper that it keeps from another net's, in the Gerbers, the design rules check, the 3D view and KiCad's own refill alike, and stays joined to the sensed pad round the rest of the pad. A plane holds no track, so on an inner plane the carve is round the sense copper's vias and a through-hole sense pin's pad, and a surface sense pin routed on an outer layer needs none. While the board is routed the sense net is another net to the pours too, so the stitching vias keep off the sense track and the sense pin's pads get no via to the plane. A sense pin's copper that joins another pad of the net is not alone and is not carved: the pour joins it as before, and `fab` warns as below. Nothing is written in the source for the carve, and a routes file is not changed by it; `netsmith routes import` leaves the rule areas out, as it leaves out every rule area.

The pad is not cut, and the sense pin is routed as any other line of its net, when the statement finds no part or more than one, when the net has copper already (a routes file's, or copper the router keeps) or is locked, when the sensed pad has a hole or a custom shape, when a half would be narrower than the process's least track, and for a second sense pin of the same pad. `fab` then says so: after routing it follows the copper of each sense pin with its sensed pad taken away and warns when it reaches another pad of the net (`` `amp` (U1) asks for `sense_p`: its pin `.in_p` (pads 1) routed alone (a Kelvin connection) from R1.1, and its copper (3 tracks, 0 vias) joins J2.1 besides the pad it senses, so load current may flow in it ``), when it does not reach the sensed pad, and when the statement finds no part or several. Each is a guideline, an error with `--strict`, and the layout file may waive it with a reason.

### Diff pairs

Two nets are a diff pair when their names differ only in a pair of suffixes, such as `dp` and `dm`, `D+` and `D-`, `_P` and `_N`, or `.p` and `.n`; when they are the two nets of a `PairImpedance` (chapter _The process of a board_, "Impedance targets"); and when the design states them a pair with no impedance (issue #1193), with the pins `.p` and `.n` of a `std::sources::DiffPair` joined to them, or as the two lines of a `std::bus::Can` or a `std::bus::Usb2`, which are a pair whatever the board names their nets (below). A pair is routed as one object at one pitch (issue #1223): its two lines run at the pair's gap, edge to edge, which is the stated spacing of a `PairImpedance` and, for any other pair, its nets' clearance, the nearest the rules allow (rounded up to the router's grid of 0.1 mm). When a try of the router has routed every connection, it routes each net of a pair again with a step beside its partner's track at that pitch (within a grid cell) costing half, and keeps the new copper only where more of the net runs coupled and it is no longer than the 45 degree jogs from its pads to the pitch make it (and 0.2 mm) and has no more vias. Then it routes both nets of the pair again together, one and then the other beside it (each first in turn), and keeps that copper only where more of the pair runs coupled, its longer net is no longer than those jogs allow and no net has more vias than the pair's net with more had (issue #1010): the shorter net may grow up to the longer, which the meander makes it anyway, and take as many vias as its partner, so that the two change layer together, their vias side by side. So a pair runs side by side wherever its copper can, and a board routes exactly as completely as before. Then the shorter net of each pair whose nets' track lengths differ by more than its skew limit (below; 0.1 mm unless the design states one) gets a meander, a row of bumps with 45 degree corners on one side of one of its straight runs, sized to add the difference: the fewest bumps no higher than 2 mm, legs three track widths apart edge to edge (or the clearance, if larger), on the longest runs first, at least one bump's pitch from the run's ends, and only where the meander keeps every space the design rules check asks of a track and that gap from its own net's other copper. It goes first where the run is more than 0.5 mm from its partner's track; what those runs cannot take goes on the coupled runs, with the bumps on the side away from the partner, except on a pair with an impedance target, whose coupled run is kept whole. A run with too little room takes what it can and the next the rest; a pair no run has room for keeps its skew, and `fab` says so, as do the notes of `board drc` (`pair skew: ...`). No other net moves. `netsmith board nets` prints each pair's two lengths, its skew against its limit, the length that runs coupled, the least and the largest gap over it and its vias side by side, by the router's own measure (chapter _Commands_). Measured on a CAN pair from a connector and its termination to a transceiver 60 mm away: before issue #1223 the two lines ran 0.9 mm apart, centre to centre, not coupled, with a skew of 2.8 mm; now 84 % of the pair runs coupled at a gap of 0.2 mm, with a skew of 0.03 mm. The route scorecard reports each pair's lengths, skew and the share of its track that runs coupled; it reads the KiCad board, so it knows a pair by its names alone.

A pair can be routed beside copper already on the board (issue #1295). A routed line of another net in the pair's way is copper like any other: the pair goes round it, on one side, and runs coupled where it has room for both lines at its pitch; where it has room for one line only, the two lines part there, and the router moves no copper of a locked net or of the routes file to make room. `netsmith routes route` routes one net: route one line of a pair, then the other, and the second is routed beside the first at the pair's pitch (the first's copper counts as its partner's), and may grow by what it is shorter than the first, which the meander would add anyway; where another net's copper stands in the way, `--rip` names it, as for any net. When the router's tries leave connections on a board with a pair, it tries once more with every pair routed as two separate lines (before the pairs' pass, what sets a pair apart is only that its lines go early and are not ripped up for a later connection) and keeps that try only if it leaves fewer connections; `fab` then says which pairs it routed as two lines and how many connections each way left. Those lines do not run side by side, and on a board routed completely the meander still matches their lengths to the pair's skew limit.

A pair the design states takes no width and no spacing of its own: each net keeps the width and the clearance it has, and the pair runs at its clearance; a pair whose impedance matters is a `PairImpedance`, which is a diff pair too and runs at its spacing. A pair's **skew limit** is its `.skew` (issue #1234), a field of `DiffPair`, `PairImpedance`, `Can` and `Usb2`: the most its two track lengths may differ by. It is the top of the field's range: `DiffPair`, `PairImpedance` and `Usb2` offer `0mm..0.1mm`, the router's grid, and `Can` `0mm..2.5mm` (its standard states no length, and 2.5 mm of track is under a ten-thousandth of its shortest bit), and a board whose link asks for less narrows it, `.lines.skew &= 0mm..0.05mm`. The meander matches the pair to it, and `board nets` and `board drc` judge the pair against it. Two members that state one pair give it the less of their limits, and a pair that only its names make has the router's 0.1 mm. A `.skew` with no top, or with a top of 0, is an error that says what to write. A net is one track of one pair. Two `DiffPair`s that give a net two partners are an error that names both, and so is a `DiffPair` with both pins on one net or a pin on none. A pair that the design states takes its nets from a pair of names that has only one of them, and a `PairImpedance` comes before a `DiffPair` on the same net.

A pair's lengths are those of its two connections, not of all their copper (issue #1487). A net's connection is the copper that takes its pads to each other: for two pads the path from one to the other through tracks and vias, for more the branches its pads need, joined where a track ends on another (a tee), at a via and in a pad. A **second way**, copper that joins pads the connection already joins by a shorter way (a duplicate track of a board routed by hand, a loop), and a **dead stub**, copper that leads to no pad, are left out of the net's length, so they move neither a pair's skew nor a length group's spread nor the meanders' target. `netsmith board nets` says what it left out: a line for each net with such copper, its length, its kind, its layer and a point of it (`net P: 10.00 mm of its 30.00 mm of copper not counted in its length: a second way of 10.00 mm on F.Cu at (12.00, 10.00)`), and a line under each pair that has one; the JSON gives each net's `copper`, all its tracks, beside its `length`, and its `not_counted` runs. Remove a second way or a stub from the board, since the signal sees it even where the length does not: a second way is a loop, and a stub a reflection.

A **length group** is a set of nets matched in length to each other, a parallel bus or a byte lane (issue #1234): the router lengthens each shorter net of the group towards its longest, with the meanders of a pair, until no two differ by more than the group's tolerance, before it matches the pairs (so a pair in a group is matched within itself after it has the group's length); `board nets` lists each group's nets, their lengths and its spread, `board drc` notes a group whose spread is over its tolerance (`length group: ...`) and `fab` says when a group keeps one. The design states a group with a `std::sources::LengthGroup` (issue #1252): its `.lines` are a list of pins (`[Pin]`, chapter _Members_), each joined with `~` to a net of the group, `.lane.lines[0]` up to `.lane.lines[7]` for a byte lane, and the group has as many nets as the indices the board joins; no count is written. Its `.tolerance` is the most the longest and the shortest net may differ by, the top of its range (`.lane.tolerance &= 0mm..0.5mm`), and it has no default, since what a bus allows depends on its clock and its parts' timing. An index skipped (`[0]` and `[2]` joined, `[1]` not) is a line on no net, an error that names it, and so are two lines on one net, a group of fewer than two nets, and a tolerance unstated, with no top or with a top of 0. A module carries a group out through its interface as it carries a `DiffPair`.

```nsm
import std::parts::connector::Header1x4
import std::sources::LengthGroup

/// Four lines of a parallel bus, matched in length to within 0.2 mm.
board Lane {
    .near: Header1x4
    .far: Header1x4
    .lane: LengthGroup { .tolerance &= 0mm..0.2mm }
    .near.p1 ~ .far.p4
    .near.p2 ~ .far.p2
    .near.p3 ~ .far.p3
    .near.p4 ~ .far.p1
    .lane.lines[0] ~ .near.p1
    .lane.lines[1] ~ .near.p2
    .lane.lines[2] ~ .near.p3
    .lane.lines[3] ~ .near.p4
}
```

```nsm
import std::parts::connector::Header1x2
import std::sources::DiffPair

/// Two lines of a link that the board routes as a pair, with no impedance stated.
board Link {
    .near: Header1x2
    .far: Header1x2
    .lines: DiffPair
    // The link asks for half the router's skew.
    .lines.skew &= 0mm..0.05mm
    .near.p1 ~ .far.p1
    .near.p2 ~ .far.p2
    .lines.p ~ .near.p1
    .lines.n ~ .near.p2
}
```

A part with two pads that is no connector, with one pad on each net of a pair, is the pair's termination (a `std::bus::CanTermination`'s resistor): the placer pulls it to the pads of the connectors that have a pad on both nets, as it pulls a decoupling capacitor to its part (issue #1193), since a termination matches the line at its end, where the line leaves the board. A pair with no connector on both its nets has no termination to pull. A board at the end of a CAN bus:

```nsm
import std::bus::{Can, CanTermination}
import std::parts::connector::Header1x4

/// An invented CAN transceiver: its bus pins are `bidirectional`.
part Xcvr {
    .designator &= "U"
    .can: Can
    .can.bitrate <= 1MHz
    pins {
        .txd      input         "TXD"  "1"
        .gnd      power         "GND"  "2"
        .vcc      power         "VCC"  "3"
        .rxd      output        "RXD"  "4"
        .can.canl bidirectional "CANL" "6"
        .can.canh bidirectional "CANH" "7"
    }
}

/// An invented connector the bus leaves the board by.
#[kind(connector)]
part BusPort {
    .designator &= "J"
    .can: Can
    pins {
        .can.canh passive "H" "1"
        .can.canl passive "L" "2"
    }
}

board Node {
    .logic: Header1x4
    .xcvr: Xcvr
    .port: BusPort
    /// This board is an end of the bus.
    .term: CanTermination
    .xcvr.can.bitrate &= 500kHz
    .xcvr.can ~ .port.can
    .term.bus ~ .port.can
    .logic.p1 ~ .xcvr.vcc
    .logic.p2 ~ .xcvr.gnd
    .logic.p3 ~ .xcvr.txd
    .logic.p4 ~ .xcvr.rxd
}
```

The nets of `canh` and `canl` are a pair to the router, matched to the bus's 2.5 mm, and `.term`'s 120 ohm resistor goes by `port`. A board in the middle of the bus states no `CanTermination`: a bus is terminated at both its ends and nowhere between (ISO 11898-2).

When a try of the router has routed every connection, and after the passes for diff pairs, pours and decoupling capacitors, it takes vias out (issue #431, as the "remove vias" pass of KiCad's router does): each connection with vias is routed again with a via three times its usual cost, against all the other copper, and the new copper is kept only if it has fewer vias, is at most 2 mm longer for each via it gives up (and at most 10 % longer), and lays at most 5 mm more track for each via on another net's pour. A connection that cannot lose a via so is tried once more with the one connection its way crosses routed again after it, kept only if the two together have fewer vias on the same terms. Vias that join a plane or a pour (every via of a poured or plane net, and a decoupling capacitor's via), vias side by side on a net whose current needs them, and a diff pair's vias are left where they are. No connection is left unrouted, so a board routes as completely as before; the diff pairs' meanders, which match the lengths it leaves, the teardrops and the rounded bends come after. Measured on the three ported boards (2026-11-13, the agent's decision under the owner's delegation): RP2350 142 to 134 vias, Orbit 94 to 85, the Easyduino 105 to 98, with routed length no longer and track on the pours within 4 mm of before.

A pad of a plane net that is not through-hole gets a via of its own beside it, joined by a short track, as its way to the plane. Two such pads can share one (issue #834): after the vias are taken out, two pads of one plane net on the same face, at most 1.5 mm apart and neither a decoupling capacitor's pad, are joined by a track instead, searched for against all the other copper and kept only if it needs no via and is at most 2 mm long; the second pad's via goes, and the first pad's via is the way to the plane of both. A via is shared by two pads at most, the nearest pairs first, and a net whose current needs vias side by side shares none. And a pad in an outer pour of its own net gives its via up once the pours are stitched: the pour joins the pad, and a stitching via the pour reaches joins the pour to the plane. Whether the pour reaches the pad is the ratsnest's, with the pour filled as the Gerbers fill it, less the copper narrower than its minimum width (0.25 mm) and its copper islands, as KiCad's fill has it; a pad the fill leaves on an island with no via keeps its via. The stitching counts here as `fab` lays it (issue 1484): where a pad's own via stands on the only free place for a stitching via of a grid point on the pad's island, the stitching via goes there all the same, and stays if the pad's via goes, so the pad is left to the stitching via rather than keeping a track and via of its own; a pad that keeps its via (the island has no other way to the plane, or the relief is starved) takes that stitching via out instead, so two vias never stand on one place. A pad keeps its via too where the fill joins it by fewer than two thermal spokes, counted as KiCad's refill leaves them (issue #1115): KiCad's check would call it a starved thermal relief, and its track and via join it as a second spoke would. KiCad's own check of a board routed so finds every pad joined to its plane.

### RF lines and a net on one layer

A line that carries a radio's signal, from its antenna pin to the connector, must be short, on one layer and without a via: a via is a step in its impedance and a stub (issue #1274, owner decision). A net with a stated impedance of one track, a `TrackImpedance` (chapter _The process of a board_, "Impedance targets"), is an **RF net**, with nothing more written: the placer weighs its length ten times a plain net's of two pads, so its parts are placed first and its line short; the router routes it first (_How the router orders the nets_) and on one copper layer with no via: the outer layer every pad of the net has copper on, the front first. A net of a `PairImpedance` is a diff pair, routed as one (_Diff pairs_), and is no RF net, even with a `TrackImpedance` too.

The physical file says how one net is routed, by a pin on it, as a `TrackImpedance` names its net by its `.line`: `.radio.ant route one_layer` routes any net on one layer with no via, and `.radio.ant route any_layer` lets an RF net change layers as any net does (it is still placed and routed first).

```nsm solve place
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::TrackImpedance

board Feed {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .radio: Header1x2
    .antenna: Header1x2
    .sense: Header1x2
    .antenna.p1 ~ .radio.p1
    .feed: TrackImpedance { .impedance &= 50ohm }
    .feed.line ~ .radio.p1
    .sense.p1 ~ .radio.p2
}
```

```nsm-physical
physical for Feed {
    // The sense line runs under no other copper: one layer, no via.
    .sense.p1 route one_layer
}
```

A net with no way on its one layer is left unrouted, as any connection the router cannot make, and `fab` says so; it is never given a via. Copper of the routes file, or a board from elsewhere, that puts such a net on two layers or gives it a via is a finding of the rule `constraint`, which stops `fab`:

```text
error [constraint] constraint sense.p1 route one_layer (feed.physical.nsm:3) is violated: the net `net-radio.p2` has tracks on B.Cu, F.Cu and 1 via, and it is routed on one layer with no via: route it again on one layer (`netsmith routes delete --net net-radio.p2` drops hand-drawn copper), or say `route any_layer` of it in the physical file
```

An RF net's finding is named after the member that states its impedance, `feed route one_layer (an RF net: ...)`. A net whose pads are on both faces, with no hole, has no one layer: the router routes it as any net, and the check reports it. Two `route` statements that say different things of one net are an error of `build` that names both, and so is a `route` of a pin on no net. Not built: a layer named for the net (`route one_layer` takes the layer its pads are on), and a length limit of the physical file (a datasheet's `route NAME .pin length LENGTH` is one, chapter _Parts_).

Misuse: the net is named by a part.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

board Unnamed {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .radio: Header1x2
}
```

```nsm-physical
physical for Unnamed {
    .radio route one_layer
}
```

```text
error: `.radio route one_layer` names the net by a pin on it, and `.radio` is a part
  --> main.physical.nsm:2:5
  |
2 |     .radio route one_layer
  |     ^^^^^^
```

### A line of one impedance through parts in series

An antenna line runs from a radio's pin through a DC-blocking capacitor or a 0 ohm link to its connector: several nets, one line (issue #1275, owner decision). A `std::sources::TrackPath` states it once: `.first` and `.last` are joined with `~` to the pins the line starts and ends at, and `.impedance` is the target. The path's nets are the chain from the net of `.first` to the net of `.last`, each two joined by a part in series, a part with two pins, one on each of two nets; a rail's net (a supply or a ground) is never a link of the chain, so a shunt part of a matching network, from the line to ground, is beside the line and not in it. Each net of the chain takes the target as a `TrackImpedance` of its own would: the router draws it at the width that gives it, it is an RF net (placed and routed first, short, on one layer), and `fab-notes.txt` gives its width on each layer. A net another target names at another impedance is an error that names both.

```nsm solve
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::passive::Capacitor
import std::sources::{TrackCorner, TrackPath}

board Antenna {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .radio: Header1x2
    .connector: Header1x2
    .block: Capacitor { .capacitance &= 100pF +/- 5% }
    .radio.p1 ~ .block.a
    .block.b ~ .connector.p1
    .rf: TrackPath {
        .impedance &= 50ohm
        .budget_length &= 0mm..25mm
        .corner &= TrackCorner::Mitered
        .fence &= 1.5mm
    }
    .rf.first ~ .radio.p1
    .rf.last ~ .connector.p1
}
```

The budget is over the chain: `.budget_length`, the top of its range, is the most the tracks of all its nets may be long together. `netsmith board nets` lists each path's nets, their lengths and the sum against it (`path rf: ...`), `board drc` notes a path over it (`track path: rf: ...`), and `fab-notes.txt` has a `track path:` line for each path with its chain, its length and its budget. `.corner` is `TrackCorner::Rounded`, the default, which rounds the bends with arcs as every net's are, or `TrackCorner::Mitered`, which keeps their 45 degree corners. `.fence` puts ground vias along both sides of the path's tracks at that distance from each other, each as near the track as the rules' space allows, joined to the net of the pour on the track's layer, else to an inner plane of a rail's net; a place no via fits, or with a ground via within half the distance already, is left without one, and a path with no such ground gets no fence and the notes say so.

A chain that is not one is an error of `build`: `.first` and `.last` joined by no chain of parts in series, or by two of the same length (two capacitors side by side), and `.first` or `.last` on no net or on a rail's net. A budget with no top, or a top of 0, is an error too.

The loss is judged as the length is (issue #1405, owner decision). `.frequency` is the frequency the line carries, one value (`.rf.frequency &= 2.4GHz`), and `.budget_loss` the most the tracks of the chain may lose together, in decibels, the top of its range (`&= 0..0.5`). Each track's loss per mm is worked out on the routed board from its width, its layer and the build-up, by the low-loss closed forms of Pozar's *Microwave Engineering* (4th ed., sections 3.7 and 3.8): on an outer layer a microstrip, whose copper loses `8.686 Rs / (Z0 W)` dB per length and whose dielectric loses `27.3 (er / sqrt(eeff)) ((eeff - 1) / (er - 1)) tan(d) / lambda0`; on an inner layer a stripline, by Pozar's stripline forms. `Rs` is the copper's surface resistance at the frequency (annealed copper, 58 MS/m), `Z0` and `eeff` the line's impedance and effective permittivity from the same formulas as its width, and `tan(d)` the loss tangent of the build-up, `.stackup.loss_tangent`. The loss is the sum over the tracks of all the nets, each its length times its loss per mm; the parts in series are not counted, nor the roughness of the copper or the solder mask's own loss, and each makes the real line lose more. `netsmith board nets` adds the loss to the path's line (`; 0.123 dB at 2.4 GHz (within the 0.5 dB budget)`), `board drc` notes a path over its loss budget, and the `track path:` line of `fab-notes.txt` says the loss against the budget.

A loss tangent is a number of the laminate, and a build-up states one only from a source that gives it for that build-up. No build-up of the standard library does: JLCPCB's impedance page gives each prepreg's permittivity and no loss tangent, and names no laminate, so its build-ups leave `.stackup.loss_tangent` unset. A board whose laminate's datasheet gives one states it (`.process.stackup.loss_tangent &= 0.02`). Without one, or without `.frequency`, or on a two-layer board, which records no build-up, the loss is not worked out and the notes say why (`no loss at 2.4 GHz: build-up JLC04161H-7628 records no loss tangent ...`). The skin-effect formula holds where the skin depth is below the copper's thickness, so a frequency of a few MHz or less has no figure either; such a line loses far less than any budget.

```nsm solve
import std::mfg::jlcpcb::FourLayer
import std::parts::connector::Header1x2
import std::parts::passive::Capacitor
import std::sources::TrackPath

board Lossy {
    .process: FourLayer {
        .stackup.thickness &= 1.6mm
        .stackup.loss_tangent &= 0.02
        .inner1.use &= "signal"
        .inner2.use &= "signal"
    }
    .radio: Header1x2
    .connector: Header1x2
    .block: Capacitor { .capacitance &= 100pF +/- 5% }
    .radio.p1 ~ .block.a
    .block.b ~ .connector.p1
    .rf: TrackPath { .impedance &= 50ohm; .frequency &= 2.4GHz; .budget_loss &= 0..0.5 }
    .rf.first ~ .radio.p1
    .rf.last ~ .connector.p1
}
```

A frequency that is a range, or not above 0, is an error of `build`.

### Track widths by rail

A rail's `.min_track` is the narrowest track its nets are drawn with. It is a lower bound, written as a range like a process rule, and the bottom of what is left is the width. Both pins of the rail get it. A net takes the widest `.min_track` of the rails on it, so `GND` below is 0.8 mm without anyone saying so. `~` between rails equates the width too, so a module's rail carries it.

```nsm solve
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Widths {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .input: Rail { .pos.net &= "VIN"; .min_track &= 0.8mm.. }
    .output: Rail { .pos.net &= "3V3"; .min_track &= 0.5mm.. }
    .in: Header1x2
    .out: Header1x2
    .input.pos ~ .in.p1
    .input.neg ~ .in.p2
    .output.pos ~ .out.p1
    .output.neg ~ .out.p2
}
```

The router draws `VIN` and `GND` at 0.8 mm and `3V3` at 0.5 mm, and the `.kicad_pro` has a net class for each width. A width under the process's least track is the process's least. Signals have no rail: widen them all with `.process.min_track`. The width is checked against the rail's current, below.

### Clearance by rail

A rail's `.min_clearance` is the least space between copper of its nets and copper of any other net, for a high-voltage rail or one the board house wants kept further from other copper (issue #758). It follows the rules of `.min_track`: a lower bound written as a range, both pins of the rail get it, a net takes the largest of the rails on it, and `~` carries it out of a module. Unset, or under the process's least (`.process.min_clearance`), a net keeps the process's least.

```nsm solve
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Mains {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .line: Rail {
        .pos.net &= "HV"
        .neg.net &= "HV_RTN"
        .min_clearance &= 1mm..
        .min_track &= 0.5mm..
    }
    .logic: Rail { .pos.net &= "3V3" }
    .hv: Header1x2
    .lv: Header1x2
    .line.pos ~ .hv.p1
    .line.neg ~ .hv.p2
    .logic.pos ~ .lv.p1
    .logic.neg ~ .lv.p2
}
```

Between two nets the larger of their clearances applies, as KiCad applies it: `HV` and `HV_RTN` keep 1 mm from each other and from `3V3` and `GND`, and `3V3` keeps the process's least from `GND`. The router keeps other copper that far from the tracks, vias and pads of `HV` and `HV_RTN`, a pour keeps it from them, and the `.kicad_pro` has a net class for each distinct width and clearance (`Track_0.5mm_Clearance_1mm` here; `Clearance_1mm` for a net that is only kept further) that states the clearance, which netsmith's design rules check and KiCad's both judge the board by. A clearance is the space between copper only: the spaces to holes, to the board edge and to a solder mask opening stay the process's. Signals have no rail: keep every net further with `.process.min_clearance`.

### Track width against current

A track that carries current heats. The solver knows the worst-case current each rail draws (`.current`, see Load current) and, through the series parts of a chain, the current of the nets between them, so netsmith draws every net at a width that can carry it and checks any width you state yourself (ADR 0036). It uses the IPC-2221 formula, `I = k * dT^0.44 * A^0.725`, where `I` is the current in amperes, `dT` the temperature rise in kelvin, `A` the track's cross-section in square mils (its width times the copper's thickness), and `k` is 0.048 for a track on an outer layer and 0.024 on an inner layer. A net's width is for the outer layers, so it uses 0.048 and the outer copper of the process (`.stackup.copper_weight` ounces, 0.035 mm each). On a board with an inner signal layer the net gets a second width too, with 0.024 and the inner copper (`.stackup.inner_copper`, 0.0152 mm on JLCPCB's multilayer boards): the router draws the net's tracks on a signal layer at that width, rounded up to 0.01 mm, and the design rules check holds them to it there, so at the default 10 K rise and JLCPCB's width tolerance a 1 A net drawn 0.38 mm wide outside is 2.25 mm wide inside. The track drop counts an inner track by its own copper.

A net carries the current that Kirchhoff's current law says it must, the way an EE adds it up (ADR 0036): the sum of the currents of the loads that draw from it (a supply net) or return through it (a ground, or any shared return), with every load on at the same time as the worst case. Two loads of 2 A and 1 A on separate supplies that share a `GND` make `GND` a 3 A net, where each supply net carries its own. The things that count are the rails with a pin on the net and the series parts (`~>` chains) whose bridge ends on it, so the nets between the parts of a chain take the chain's current. A regulator is a load on its input net and a supply on its output net, each with the current the solver worked out: a linear regulator draws what its output supplies, and a switching regulator draws `Pout / efficiency / Vin`, from the efficiency its part model states, so its input net is sized for that and not for the output current. A part model that states no efficiency leaves the input current unknown, and the net is sized only for what is known (below). The part may state it itself (`.efficiency &= 0.85..0.95`) or take it from the catalog row that is picked (an `.efficiency optional` column, as the three standard buck ICs have, chapter 12); only a row that leaves the cell empty, for a part that states none, leaves it open. The sum is pessimistic where a current is counted twice: a ground that the load of a regulator's output and the regulator's own input both return through carries the sum of the two, though the load's return goes to the regulator and only the input's goes on to the source. You do not have to state a width for it. A net no rail gives a `.min_track` is drawn at the width IPC-2221 needs for that current (rounded up to 0.01 mm): a 2 A node between a fuse and a diode is drawn at 0.98 mm with nothing written, and the check has nothing to say. The width is sized for the narrowest finished track the process's `.track_width_tolerance` allows (JLCPCB: ±20%), so the drawn width is 1/(1-tolerance) times IPC-2221's (1.25 times for JLCPCB: 2 A needs 0.78 mm by the formula and is drawn at 0.98 mm), and a stated `.min_track` is checked by what its width less the tolerance carries. A `.min_track` on a rail is an override: the net is drawn at the widest one, never narrower than the process's least track, and the check warns when IPC-2221 needs more. The temperature rise is `.process.max_temperature_rise`, in kelvin, an upper bound like `via_in_pad`; it is 10 K when the board states none, and the finding says so. 10 K is a conservative default of netsmith's and is not a number from a fabricator. The planes of a four-layer board and the nets of a pour are areas and not tracks, and are not sized or checked. The width is for the heat, not for the voltage the track drops: the width is chosen before the board is routed, and the drop depends on the routed length. It is not always small (2 A through 50 mm of a 0.98 mm track in 1 oz copper drops about 0.1 V, 2 % of 5 V), so where it matters, give the rail a wider `.min_track`.

The router keeps to that width (issue #431). Where a track of a net that carries current does not fit at its full width, it may be necked down, but never narrower than the width the current needs, except within 1 mm of the pads of a part the line ends on (the box round their centres, grown by 1 mm), where it may be the process's least track to escape between fine-pitch pads that are no wider and carry the same current. A line that would have to be narrower anywhere else is left unrouted rather than drawn too narrow. `fab` hands the design rules check each net's least width, and the check's own rule `track_current` refuses a track narrower than it away from those pads. A via carries current too: the process states its hole plating (`.hole_plating`, 18 μm at JLCPCB), the track width check rates the process's via by the copper of its barrel (IPC-2221 with the inner-layer constant: a 0.2 mm hole carries 0.56 A at 10 K), and a net that needs more than one gets that many side by side wherever it changes layer, their copper overlapping so that no track joins them. The router changes layer only where the cluster fits (never through a via in a pad, since the others of a cluster never stand in a pad), and the check's rule `via_current` refuses a cluster that is short. Both findings say what they hold the copper to (issue #1373): the width or the number of vias it has and the width or number its current needs, the current assumed and where it comes from (the loads that add up to the net's current, as the track width check lists them, or the pins that carry it, by their `.current` or what the model finds through them, for copper that only feeds branches), and the sizing's copper, temperature rise and width tolerance, so that a person can choose a wider track or more vias or a lower current. `board drc --format json` has the same as fields of the finding: `current`, `current_from`, `width` and `needed_width` (a track), `vias` and `needed_vias` (a cluster of vias), `copper` (mm), `rise` (K) and `tolerance`.

When a current on the net is not known (the solver leaves the currents of a net with two or more unknown ones unsolved), netsmith does not guess it, and does not ignore what is known either. The currents that are known are a lower bound on the net's current, because loads only add up: a current that is not known can make the sum larger and never smaller. So the net is drawn at least as wide as the known currents need (the process's least track or a stated `.min_track` still decides when it is wider), and `build` prints a note that says the width is a lower bound and names the net and the carriers whose current is missing. A net with no known current at all stays at the process's least track, and the note says that. A stated `.min_track` that is narrower than the known current needs is still a finding, worded "carries at least" and naming the carriers whose current is missing. The note appears while a current is missing and goes away when you state it. A net whose every current is known is sized as above and has no note.

```nsm solve notes
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail

board Unsized {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail { .voltage &= 12V; .pos.net &= "VIN" }
    .load: Rail { .pos.net &= "VOUT" }
    .r: Resistor { .resistance &= 10kohm +/- 1% }
    .inlet: Header1x2
    .outlet: Header1x2
    .supply.pos ~> .r ~> .load.pos
    .supply.neg ~ .load.neg
    .inlet.p1 ~ .supply.pos
    .inlet.p2 ~ .supply.neg
    .outlet.p1 ~ .load.pos
    .outlet.p2 ~ .load.neg
}
```

```text
the current of rail `supply`, rail `load` is not known (it is not stated and the solver could not work it out; with two or more unknown currents on a net it works none of them out), so the tracks of net `GND` are the process's least, 0.1 mm, a lower bound: no current on the net is known; state that current, or give a rail on the net a `.min_track`
the current of rail `supply`, part `r` is not known (it is not stated and the solver could not work it out; with two or more unknown currents on a net it works none of them out), so the tracks of net `VIN` are the process's least, 0.1 mm, a lower bound: no current on the net is known; state that current, or give a rail on the net a `.min_track`
the current of rail `load`, part `r` is not known (it is not stated and the solver could not work it out; with two or more unknown currents on a net it works none of them out), so the tracks of net `VOUT` are the process's least, 0.1 mm, a lower bound: no current on the net is known; state that current, or give a rail on the net a `.min_track`
```

Here a fan on the output is known to draw up to 1.2 A, but the supply's and the load's currents are not. `GND` carries at least the fan's 1.2 A, so it is drawn at the 0.49 mm that 1.2 A needs and not at the process's least; `VIN` has nothing known and stays at the least:

```nsm solve notes
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail

board Partial {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail { .voltage &= 12V; .pos.net &= "VIN" }
    .load: Rail { .pos.net &= "VOUT" }
    .fan: Rail { .pos.net &= "VOUT"; .current &= 0A..1.2A }
    .r: Resistor { .resistance &= 10kohm +/- 1% }
    .inlet: Header1x2
    .outlet: Header1x2
    .supply.pos ~> .r ~> .load.pos
    .supply.neg ~ .load.neg
    .inlet.p1 ~ .supply.pos
    .inlet.p2 ~ .supply.neg
    .outlet.p1 ~ .load.pos
    .outlet.p2 ~ .load.neg
}
```

```text
the current of rail `supply`, rail `load` is not known (it is not stated and the solver could not work it out; with two or more unknown currents on a net it works none of them out), so the tracks of net `GND` are the process's least, 0.1 mm, a lower bound: no current on the net is known; state that current, or give a rail on the net a `.min_track`
the current of rail `supply`, part `r` is not known (it is not stated and the solver could not work it out; with two or more unknown currents on a net it works none of them out), so the tracks of net `VIN` are the process's least, 0.1 mm, a lower bound: no current on the net is known; state that current, or give a rail on the net a `.min_track`
the current of rail `load`, part `r` is not known (it is not stated and the solver could not work it out; with two or more unknown currents on a net it works none of them out), so the tracks of net `VOUT` are 0.49 mm, a lower bound: sized for the 1.2 A that is known, not for the whole current; state that current, or give a rail on the net a `.min_track`
```

Where the DC analysis solved the net (see "DC analysis": every pin on it is one the analysis models, which no net with a connector, an unmodelled pin or an open island is), the current is the one the analysis found into it (ADR 0076, amendment 8): at each corner of the ranges, the sum of what the elements put into the net, and the largest of those over the corners. It is complete, so a net the analysis solved has no note, and it is not larger than the sum above, which takes every load at its largest at the same time, and smaller where the loads follow one voltage: a regulator that draws most when the supply is low next to a resistor that draws most when it is high is never both at once, and the net is sized for the corner that carries the most. It is used for the nets between two elements that no rail holds (the nets of a chain, a regulator's input behind a fuse), which one island contains; a supply, a ground or a regulator's output is on many islands and keeps the sum above. A net with an open-drain pin that was only inferred keeps it too. A finding that a stated width is too narrow says `the most the DC analysis finds into it over every corner of its ranges` and the sum it replaces.

A current that is not known can still be bounded from above, by Ohm's law for a resistor (see "Resistor power"). The width does not change with the bound. The note goes only when the width the net is drawn at carries the known current and every bound on the net together, in whichever direction they flow, which is so for the pull-down of a logic line (a few hundred microamperes, which the process's least track carries many times over) and not for a part whose bound is amperes. A net with a current that has no bound, a rail's or a transistor's, keeps its note.

The block below states a width that is too narrow for its rail, so it is a finding:

```nsm solve warns
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Heater {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .load: Rail { .pos.net &= "5V"; .current &= 0A..2A; .min_track &= 0.2mm.. }
    .source: Rail { .pos.net &= "5V"; .current &= -2A..0A }
    .supply: Header1x2
    .out: Header1x2
    .supply.p1 ~ .source.pos
    .supply.p2 ~ .source.neg
    .out.p1 ~ .load.pos
    .out.p2 ~ .load.neg
}
```

```text
Heater: IPC-2221 guideline: net `5V` carries up to 2 A (rail `load`: 0 mA to 2 A) but its tracks are 0.2 mm wide (a rail's `.min_track`), in 1 oz (0.035 mm) outer copper; at a temperature rise of 10 K (the default) that width carries 633 mA (at its narrowest, 20% under, the fabricator's tolerance) and IPC-2221 needs 0.98 mm: give a rail on the net `.min_track &= 0.98mm..`, or allow a higher `.process.max_temperature_rise`
Heater: IPC-2221 guideline: net `GND` carries up to 2 A (rail `load`: 0 mA to 2 A) but its tracks are 0.2 mm wide (a rail's `.min_track`), in 1 oz (0.035 mm) outer copper; at a temperature rise of 10 K (the default) that width carries 633 mA (at its narrowest, 20% under, the fabricator's tolerance) and IPC-2221 needs 0.98 mm: give a rail on the net `.min_track &= 0.98mm..`, or allow a higher `.process.max_temperature_rise`
```

A ground is sized for the sum of what returns through it. Here two supplies share `GND`: the 5 V one carries 2 A, the 3.3 V one 1 A, and the `GND` track that the rail `load_a` states at 1 mm carries 2.03 A at 10 K at its narrowest (20% under, the tolerance; 2.39 A drawn exactly), enough for the 2 A of `5V` but not for the 3 A that return together:

```nsm solve warns
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Shared {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .source_a: Rail { .pos.net &= "5V"; .current &= -2A..0A }
    .source_b: Rail { .pos.net &= "3V3"; .current &= -1A..0A }
    .load_a: Rail { .pos.net &= "5V"; .current &= 0A..2A; .min_track &= 1mm.. }
    .load_b: Rail { .pos.net &= "3V3"; .current &= 0A..1A }
    .in_a: Header1x2
    .in_b: Header1x2
    .out_a: Header1x2
    .out_b: Header1x2
    .in_a.p1 ~ .source_a.pos
    .in_a.p2 ~ .source_a.neg
    .in_b.p1 ~ .source_b.pos
    .in_b.p2 ~ .source_b.neg
    .out_a.p1 ~ .load_a.pos
    .out_a.p2 ~ .load_a.neg
    .out_b.p1 ~ .load_b.pos
    .out_b.p2 ~ .load_b.neg
}
```

```text
Shared: IPC-2221 guideline: net `GND` carries up to 3 A (the sum of the currents on it with every load on at once: rail `load_a` 2 A + rail `load_b` 1 A) but its tracks are 1 mm wide (a rail's `.min_track`), in 1 oz (0.035 mm) outer copper; at a temperature rise of 10 K (the default) that width carries 2.03 A (at its narrowest, 20% under, the fabricator's tolerance) and IPC-2221 needs 1.71 mm: give a rail on the net `.min_track &= 1.71mm..`, or allow a higher `.process.max_temperature_rise`
```

A net that needs very wide tracks takes a large part of a small board, and nothing else limits a net's width. When the width IPC-2221 needs for a net is more than a fifth (20%) of the board's shorter side, the build warns with a finding of its own, `net width`. The side is the outline's: the smaller of its width and its height at their largest (`.height &= 0mm..30mm` is up to 30 mm), so a board that may still be big enough is not told; a board with no outline, or none that limits a side, is not checked. Here 8 A needs tracks 6.62 mm wide (drawn at 6.62 mm so that the narrowest finished track, 20% under, carries it), which is 33% of the 20 mm side:

```nsm solve warns
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Charger {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .load: Rail { .pos.net &= "5V"; .current &= 0A..8A }
    .source: Rail { .pos.net &= "5V"; .current &= -8A..0A }
    .supply: Header1x2
    .out: Header1x2
    .supply.p1 ~ .source.pos
    .supply.p2 ~ .source.neg
    .out.p1 ~ .load.pos
    .out.p2 ~ .load.neg
}
```

```nsm-physical
import std::pcb::Outline

physical for Charger {
    case {
        .outline: Outline { .width &= 60mm; .height &= 20mm }
    }
}
```

```text
Charger: net width guideline: net `5V` needs tracks 6.62 mm wide (IPC-2221, for 8 A), which is 33% of the board's shorter side, at most 20 mm (the outline's largest): over 20% leaves little room for anything else; make the board larger, lower the current, or give the net a plane or an outer pour
Charger: net width guideline: net `GND` needs tracks 6.62 mm wide (IPC-2221, for 8 A), which is 33% of the board's shorter side, at most 20 mm (the outline's largest): over 20% leaves little room for anything else; make the board larger, lower the current, or give the net a plane or an outer pour
```

Rules.

- This is a guideline and not a limit of the fabricator (ADR 0030): `build` and `fab` print each finding as `warning: ...`, `fab` also writes it in `fab-notes.txt`, and the editor lists it as a warning at the rail or part that carries the current. The board still builds.
- To make a finding stop the build, put `guidelines = "error"` in the `[project]` table of `netsmith.toml`, or pass `--strict` to `build` or `fab` for one run. The message is then an error at the same line.
- The fifth of the shorter side is netsmith's own choice and, like the constants of IPC-2221, provisional: a rule of thumb for where a power track leaves little room for anything else, not a figure from a standard. The finding is about the width the net needs, whether or not a rail states a `.min_track`; where a current on the net is not known it says "at least". Make the board larger, lower the current, or give the net a plane or an outer pour (a plane or an outer pour is not checked: its copper is an area and not a track).
- IPC-2221 is a conservative design guide, and IPC-2152 is more accurate (it counts the board's thickness and the copper next to the track). netsmith implements IPC-2221 only. A track the check passes is not therefore good for a certain temperature, and one it flags may be fine; read the number as a prompt to decide.
- The router may draw a track narrower than its net's width where there is no room (the stub at a pad is the net's width, and the pad's own copper carries the rest). The check is of the width asked for.

### The current of a branch

A net is drawn at the width of its whole current, and that is right for the copper that carries it, but not for a branch that carries almost none: a converter's bias pin, the top of a feedback divider, a sense resistor's tap, a meter's probe on a rail of amps. Drawn at the rail's width with the rail's vias side by side, such a branch takes room a busy board does not have, and is left open. The pin at the end of the branch says what it carries with `.current`, as a pull-up states its own draw (issue #1113):

```nsm solve
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Probe {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .load: Rail { .pos.net &= "5V"; .current &= 0A..3A }
    .source: Rail { .pos.net &= "5V"; .current &= -3A..0A }
    .supply: Header1x2
    .out: Header1x2
    .probe: Header1x2 { .p1.current &= 1mA; .p2.current &= 1mA }
    .supply.p1 ~ .source.pos
    .supply.p2 ~ .source.neg
    .out.p1 ~ .load.pos
    .out.p2 ~ .load.neg
    .probe.p1 ~ .load.pos
    .probe.p2 ~ .load.neg
}
```

`5V` and `GND` carry 3 A, are drawn 1.71 mm wide and change layer through vias side by side; the tracks to the meter's probe header carry 1 mA and are drawn at the process's least, 0.1 mm, with one via. A part states it for its own pin (`.bias.current &= 0A..1mA` in the part's definition), or a board for a pin of a part it has (`.ldo.bias.current &= 1mA`).

Rules.

- The branch is the copper from the pin's pads to the rest of its net. The router joins each pad of the pin to the nearest pad of the net that is no branch, by a line of its own, and never through another pad: the pad is a leaf, so no current but the branch's passes through its line. That line is drawn at the width the branch's current needs (IPC-2221, as above: rounded up to 0.01 mm, sized for the narrowest finished track), with as many vias as that current needs where it changes layer. The rest of the net is drawn as before.
- It is the size of the range that counts: `1mA`, `0A..1mA` and `-1mA..0A` all say 1 mA. A pin that states nothing is drawn as its net is, as before.
- It is never narrower than the process's least track, a rail's `.min_track` on the net (a stated width is the narrowest of every track of the net), or what a part's `layout` block asks of the net with `route`. A net with an impedance target is drawn at the width that gives it, branches too.
- It is no term of the net's balance and adds nothing to the net's current: what flows through the pin is drawn by the rail or the part that has it, which the balance already counts. It is checked against them: a pin that is a rail's `pos` or `neg`, or an end of a part's `bridge`, cannot carry less than that rail or part, and the build refuses a statement that says it does, with both currents. Here the divider draws up to 17.17 uA through its top resistor:

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::sources::Rail

board Sensor {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail { .voltage &= 3.3V +/- 3%; .pos.net &= "3V3" }
    .r_top: Resistor { .resistance &= 100kohm +/- 1% }
    .r_bottom: Resistor { .resistance &= 100kohm +/- 1% }
    .r_top.current == .supply.voltage / (.r_top.resistance + .r_bottom.resistance)
    .supply.pos ~> .r_top ~> .r_bottom ~> .supply.neg
    .r_top.a.current &= 10uA
}
```

```text
error: Sensor: `r_top.a.current` says its branch carries at most 0.01 mA, but part `r_top` carries up to 0.0172 mA through it: state at least 0.0172 mA on the pin, or leave its current out so that its copper is drawn for its net
```

- The design rules check holds the branch's copper to the branch's current and not the net's: a track or a via cluster narrower or fewer than its net needs passes when taking it out of its net's copper leaves one piece with a pad of no branch (or a pour) and the others with branches only, and it carries what those branches state, each pin once. Copper that joins two pads of no branch, or closes a loop, is the net's. The voltage a net's tracks drop (below) is measured between the pads of no branch.
- The check needs no statement where the model already knows what a pin carries (issue #1333). A pin through which only rails and parts that pass a current (a resistor, a diode) carry a current, every one of them known, is a branch of the sum of those currents at their largest when that sum is less than its net's: the top resistor of a buck's feedback divider carries microamps from an output of amps, and the track from the output's copper to it may be the process's least track, however far from a pad. So the copper is held to what flows through it, the sum of the currents of the pins past it. A pad of a pin whose current is not known (a capacitor's, a connector's that no rail names, a pin of a rail whose current is not worked out) is of the trunk, as is a pin that carries its net's whole current (the inductor's). The router does not read these currents: it draws such a pin's line at its net's width, and only a stated `.current` makes it narrower.
- Only a pin of a part states it. A port's pin, of a module or a board, stands for every pin joined to it, so its branch is no one piece of copper:

```nsm fails
import std::sources::Rail

module Stage {
    .vin: Rail
    .vin.pos.current &= 1mA
}

board B {
    .supply: Rail { .pos.net &= "5V" }
    .stage: Stage
    .supply ~ .stage.vin
}
```

```text
error: `.vin.pos` is a pin of a module, not of a part: the current of a branch is stated on the pin of the part the branch ends at, such as `.ldo.bias.current &= 1mA`, since a module's pin stands for every pin joined to it
  --> main.nsm:5:5
  |
5 |     .vin.pos.current &= 1mA
  |     ^^^^^^^^^^^^^^^^^^^^^^^
```

### The voltage a power track drops

A track is sized for heat, before the board is routed, and not for the voltage it drops: that depends on the length the router then draws. So `fab` measures the drop once the board is routed (issue #652). Each track of a net is a resistance, `rho * length / (width * thickness)`, with copper's resistivity at 20 °C (1.72e-5 ohm mm) and the process's outer copper (0.035 mm for 1 oz). The drop of the net is its worst-case current, the one its tracks are sized for, through the largest resistance between two of its pads along its tracks. 2 A through 50 mm of the 0.98 mm that 2 A is drawn at in 1 oz copper drops about 0.05 V; 0.5 A through 50 mm of 0.2 mm about 0.06 V.

When a net drops more than 1 % of the voltage of its rail, `fab` warns with the guideline `track drop`. For a 3.3 V rail that carries 2 A from one header to another 55 mm away:

```text
warning: Feed: track drop guideline: net `3V3` drops up to 0.0552 V on its tracks at 2 A (1.7% of its 3.3 V; the budget is 1%): from J1.1 to J2.1 through 55 mm of track, 0.98 mm at its narrowest, in 0.035 mm copper, with the whole current between the two pads with the most resistance between them; give a rail on the net `.min_track &= 1.64mm..` (that path at that width drops no more), or give the net an outer pour
```

Rules.

- The rail's voltage is the `.voltage` of a rail whose `pos` is on the net, at the smaller end of its range; with several rails, the smallest. A net whose rails state no voltage, or one whose range holds 0 V, is not checked, and nor is a ground (the net of a `neg`), whose voltage is not its own. A net whose current is not known is not checked either.
- The whole current is taken between the two pads with the most resistance between them, because which pad is the source and how the current divides between the loads is not known. That is the most the net can drop between its source and any load, and exactly the drop for a net with one source and one load.
- The width that the message asks for is the least `.min_track` that would bring that path within the budget, if every run of it narrower were widened to it. The router may then draw another path, so `fab` measures again.
- Not counted: a via's barrel, a pad's copper, a teardrop, a track warm from its current, and a track finished narrower than drawn (the fabricator's tolerance). A net that is a plane, an outer pour or a zone of the routes file is not checked: its copper is an area.
- The budget, 1 %, is netsmith's own choice and not a published figure; a design cannot state its own yet.
- This is a guideline, like the track width against current: `fab` prints it as `warning: ...` and writes it in `fab-notes.txt`. With `guidelines = "error"` in `netsmith.toml` or `board fab --strict`, it is an error and `fab` stops before it writes the package. `build` and the editor do not show it, since the board is not routed there.

### The outline and cutouts

The member `.outline` of a board's physical file (in its `case` section) is the board edge: a rectangle with rounded corners, or a polygon (_A polygon outline_, below). A range is a limit, and the placer takes the smallest board that holds the parts inside it: `.width &= 0mm..60mm` is at most 60 mm. A single value is exactly that size. Without an `.outline` the board is as big as its parts need. A `Cutout` is a rectangular hole inside the outline, with an optional corner radius. Its corner nearest the origin is `.x`, `.y`, or the layout file's entry for its name; stating both is an error. A hole that crosses the outline is refused (`the cutout is not wholly inside the board outline`): a notch in the board's edge is a polygon outline, not a cutout.

```nsm solve
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

board Shaped {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .header: Header1x2
}
```

```nsm-physical
import std::pcb::{Cutout, Outline}

physical for Shaped {
    case {
        .outline: Outline { .width &= 60mm; .height &= 0mm..40mm; .corner_radius &= 3mm }
        .slot: Cutout {
            .x &= 5mm
            .y &= 8mm
            .width &= 3mm
            .height &= 12mm
            .corner_radius &= 1mm
        }
    }
}
```

When the parts do not fit, `fab` stops and says which ones (the margin round the parts is the process's copper to edge space plus one track and its space, 0.4 mm for JLCPCB's, the same as for a board sized to fit; this block is not run by the guide test):

```text
error: Shaped: 3 part(s) do not fit the outline of 60.0 x 40.0 mm: outside the outline or inside its margin, by mm past what is kept clear: `header` (J1) 1.30, `r4` (R4) 0.20; courtyards overlapping, by mm2: `r2` (R2) and `r3` (R3) 1.20; placing them in the outline anyway ended with 1 pair(s) of courtyards overlapping, 2 part(s) outside the outline or inside its 0.4 mm margin
```

(The numbers are invented for the example.) The placer packs and turns the parts in the outline (a part wider than the board is turned, unless its requirements state a rotation) and refuses only if that ends with courtyards over each other or parts outside, so the message leads with the parts that are wrong: how many, every part that is outside the outline or inside its margin with how far, and every pair of courtyards that overlap with its area. It does not give the size the parts need, except when the courtyards alone add up to more than both faces of the outline hold, which no arrangement can fix (then it says so, with the areas, and refuses at once, before the placer runs: the message ends `the placer did not try`, and there is no list of parts, since none was placed). The message then lists the ten pairs of courtyards that overlap the most (the names, the shared area in mm2 and its size, where it is from the outline's top left corner and the side of the outline it is nearest) and the ten parts furthest out (each with how far outside which edge, or inside the margin, or into which cutout), and names all the others. `fab` also writes every part's final position, turn, face and state to `placement-failure.txt` in the board's output folder and names the file in the message. `board fab --format json` gives the refusal as data in `does_not_fit` of its error finding, with no cut at ten: `outline`, `needs`, `margin`, `courtyard_area`, `available_area`, `certain` (whether the courtyards alone are more than both faces hold) and `tries`; `oversize`, each part bigger than the outline with every turn tried and its courtyard in it; `overlapping`, every pair of courtyards that overlap (`a` and `b` as `refdes` and `path`, the shared `area` in mm2, its `width` and `height`, its place `x` and `y`, the `nearest_edge` and `nearest_distance`), the most overlapped first; `outside`, every part outside what is kept clear (`why`, the sentences, and `worst` in mm), the furthest first; `relations` and `conflicts`, the sentences; and `parts`, every part where the placer ended with it, as `placement-failure.txt` lists them (`refdes`, `path`, `kind`, `side`, `x`, `y`, `rotation`, `courtyard`, `fixed` and `state`). The figures are in mm from the outline's top left corner, y down.

When the enclosure is a DXF, `netsmith import dxf` writes the outline, the cutouts and the mounting holes for you (see `netsmith import dxf --help`): the members for the `case` section, the holes as `MountingHole` parts with their drill as `.diameter` (a circle of any size from 0.2 mm to 20 mm: `.hole1: MountingHole { .diameter &= 2.375mm }`), and their places, which come from the case and not from the placer, as requirements in the same file (`.hole1.place.x &= 5mm`). A hole whose place is stated is not put in a corner.

Mounting holes, plated holes and fiducials go in the corners of a fixed outline, holes first, clockwise from the top left, and clear of a rounded corner. A board with no outline lays them out with the other parts. A part in a rounded corner, over a cutout or outside the outline is found by the placement check and named.

**A screw head (ADR 0045, C3).** A screw's head is wider than its hole, so a `MountingHole` may state `.head_diameter`, how wide the head (or its washer) is: `.hole1: MountingHole { .head_diameter &= 6mm }`. The hole then carries room for the head, a keepout centred on the hole that forbids parts, tracks, vias and pours on every copper layer: the placer keeps every other part's courtyard out of it, the router and the stitching vias keep clear of it, no pour is filled in it, the board file has it as a rule area (so KiCad's check and the design rules check keep to it) and the placement check names a part that stands in it: ``R3 overlaps the screw head of `hole1`, which forbids parts``. The room is the square that holds the head's circle, so it is never smaller than the head and a corner of it is a little more than the head needs; the hole itself, and its own courtyard, are not in it. A hole that states no `.head_diameter` has no head, as before. `.head_diameter` is one length above zero: a range or zero is an error that names it. A `PlatedHole` has no `.head_diameter` yet.

Misuse.

```nsm fails
import std::mfg::jlcpcb::TwoLayer

board Tiny {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
}
```

```nsm-physical
import std::pcb::Outline

physical for Tiny {
    case {
        .outline: Outline { .width &= 3mm; .height &= 40mm; .corner_radius &= 2mm }
    }
}
```

```text
error: Tiny: `outline.corner_radius` is 2 mm, more than half of the shape's shorter side (3 mm)
```

#### A polygon outline

An edge that is not a rectangle with four equal corners (an L, a slanted side, corners of different radii) is a `polygon` statement in the outline's block: the corners in order, each `(x, y)` in the board's frame, the edge running from each to the next and from the last back to the first. `round R` after a corner is a tangent fillet of radius R there. `arc through (x, y)` after a corner makes the edge to the next corner an arc through that point, as KiCad's `gr_arc` gives one by its start, middle and end. A long run goes on over lines that begin with `(`, `round` or `arc`. This is the form KiCad's `Edge.Cuts`, IPC-2581's `Polygon` and the ODB++ profile give an outline (owner decision 2026-11-12, issue #57).

```nsm solve
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

board Ell {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .header: Header1x2
}
```

```nsm-physical
import std::pcb::Outline

physical for Ell {
    case {
        // Case drawing: an L, 40 x 30 mm, the top right 15 x 15 mm cut away, a 2 mm
        // radius at the top right and a 3 mm one at the bottom left, and a bulge on the
        // bottom edge.
        .outline: Outline {
            polygon (0mm, 0mm) (25mm, 0mm) round 2mm (25mm, 15mm) (40mm, 15mm)
                (40mm, 30mm) arc through (20mm, 34mm) (0mm, 30mm) round 3mm
        }
    }
}
```

The box round the edge has its top left at the board's origin, so the smallest x and y of the corners (and of an arc's bulge) are 0mm. The polygon is the whole edge: its box is the board's width and height, so `.width`, `.height` and `.corner_radius` are not stated beside it. Every value is a fixed length, the number the case drawing gives: a range, a tolerance or a value read from another field is refused. A part that may hang over an edge of the box (_A part that hangs over the board edge_, below) is also let into the part of the box outside the polygon that touches that edge, where its courtyard reaches the edge: the rounded corners of the edge, and a notch that opens on it, are the board's edge there, as the rounded corners of a rectangle are. A courtyard outside the polygon anywhere else is `not wholly inside the board outline: its courtyard is X mm past the outline's edge by the bottom right corner of the outline's box, at (x, y) mm from the board's top left corner`, which names the side or corner of the box that the place is near (or leaves it out, in the middle), and where it is. The placer keeps the parts inside the polygon and `fab` checks them against it; `Edge.Cuts`, the Gerbers and `dev board3d --step` carry its lines and arcs, and the editor draws it. The router routes inside the polygon, round a notch and along an arc, and keeps its copper the process's copper to edge space from the polygon's lines and arcs; the design rules check holds the same space. `netsmith import dxf` writes a `polygon` for an outline that is not a rounded rectangle. A loop of the drawing that crosses the board edge, such as a window in a case wall that is partly beyond the board, is cut into that outline as a notch: the part of the loop inside the board is removed, the new edge is lines and arcs of the loop, and the report names the opening, its layer, size and place. An opening that cuts the board into several pieces, that has an edge along the outline, or that has a cutout inside it is refused; so is a loop that only touches the edge or crosses a cutout (owner decision 2026-11-15, issue #90).

Misuse: a size beside the polygon, and a corner that is a range.

```nsm fails
import std::mfg::jlcpcb::TwoLayer

board Sized {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
}
```

```nsm-physical
import std::pcb::Outline

physical for Sized {
    case {
        .outline: Outline {
            .width &= 40mm
            polygon (0mm, 0mm) (40mm, 0mm) (40mm, 30mm) (0mm, 30mm)
        }
    }
}
```

```text
error: Sized: `outline.width` is stated beside the `polygon` of `outline`: the polygon is the whole edge, its corners give the size and the roundings, so leave `.width` out
```

```nsm fails
import std::mfg::jlcpcb::TwoLayer

board Loose {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
}
```

```nsm-physical
import std::pcb::Outline

physical for Loose {
    case {
        .outline: Outline { polygon (0mm, 0mm) (40mm +/- 0.1mm, 0mm) (40mm, 30mm) (0mm, 30mm) }
    }
}
```

```text
error: a corner of a `polygon` is one value, and `40mm +/- 0.1mm` is a range (39.9mm..40.1mm): the outline is what the case drawing says, so write the one number
  --> main.physical.nsm:5:49
  |
5 |         .outline: Outline { polygon (0mm, 0mm) (40mm +/- 0.1mm, 0mm) (40mm, 30mm) (0mm, 30mm) }
  |                                                 ^^^^^^^^^^^^^^
```

Misuse: a fillet with no room on its edge.

```nsm fails
import std::mfg::jlcpcb::TwoLayer

board Tight {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
}
```

```nsm-physical
import std::pcb::Outline

physical for Tight {
    case {
        .outline: Outline {
            polygon (0mm, 0mm) (10mm, 0mm) round 6mm (10mm, 10mm) round 6mm (0mm, 10mm)
        }
    }
}
```

```text
error: Tight: `outline`: the edge from (10mm, 0mm) to (10mm, 10mm) is 10 mm long, too short for its fillets: `round 6mm` at (10mm, 0mm) takes 6 mm of it and `round 6mm` at (10mm, 10mm) takes 6 mm of it
```

### Pours

The process has a pour on each outer layer, `.top` and `.bottom`. A pour with a net is copper filled round the tracks, joined to its net with KiCad's thermal reliefs; a pour with no net is no pour. `.stitch_pitch` is the most space between the vias that join the pour to the other layers of its net (an inner plane, or the pour on the other side): the tool uses the top of the range and puts the vias on a grid after routing, in what the routing leaves free. A via is a through via and joins every layer, so a net poured on both outer layers (with or without an inner plane) gets one via per grid point, not one per pour (issue 1482): the second pour's grid point is left without where a via of the first pour's stands within a third of the pitch of it, and `fab-notes.txt` counts the vias that are there. A grid point that already has a via of the pour's net that near before the stitching starts gets none either (issue #1483): the via the router laid there, or one a routes file brought back from an earlier `fab`, is that point's via. So `fab`, `routes import` and `fab` again lay the grid once.

`.thermal_gap` (the space between a pad of the pour's net and the pour round it) and `.spoke_width` (the width of the spokes that join the pad to the pour) set the thermal relief. Both are lengths on the range convention, and the tool takes the least the process allows, the bottom of the range: `.top.thermal_gap &= 0.3mm..` makes the gap 0.3 mm. Unset means KiCad's default, 0.5 mm for both. The Gerbers, the design rules check and the pour's zone in the board file all use the same two numbers; a bottom of zero or less is an error that names the field, and a range with no bottom (`..0.6mm`) states none. The inner planes keep KiCad's defaults.

A pour has the other settings of a zone drawn by hand in the routes file (Zones, below), so that a board whose source states its pour can tune it there (issue #1084):

- `.clearance` is the space between the pour and copper of other nets. Unset means the space the process keeps round a plane (its `.min_clearance`, or its space to a hole in an inner layer if that is more).
- `.min_width` is the narrowest copper the pour keeps: the fill drops what is narrower, slivers and spokes alike (KiCad's minimum thickness of a zone). Unset means 0.25 mm. A spoke is no wider than its pad's narrower side, and a pad whose spokes would be narrower than `.min_width` gets none, as in KiCad, so its relief joins nothing.
- `.pads` is how the pour joins the pads of its net, a variant of the enum `PadConnection` (`import std::mfg::stackup::PadConnection`): `Thermal`, a thermal relief, the default; `Solid`, copper up to the pad; `None`, not joined, so the pad keeps the pour's clearance and needs a track; and `ThruHoleOnly`, a relief round a through-hole pad and solid copper up to a surface pad. The routes file and `netsmith routes add-zone --pads` write the same four as `thermal`, `solid`, `none` and `thru_hole_only` (`thru-hole-only` on the command line).

`.clearance` and `.min_width` are lengths on the range convention, read at the bottom of the range as `.thermal_gap` is: `.top.clearance &= 0.4mm..` keeps 0.4 mm. A bottom under the process's `.min_clearance` or `.min_track` is an error that names the field, the least and the process's field. The Gerbers, the design rules check, the 3D view and the pour's zone in the board file all take the three; the inner planes keep the defaults.

```nsm solve
import std::mfg::jlcpcb::FourLayer
import std::mfg::stackup::PadConnection
import std::parts::connector::Header1x2

board Poured {
    .process: FourLayer {
        .inner1.net &= "GND"
        .inner2.net &= "3V3"
        .top.net &= "GND"
        .bottom.net &= "GND"
        .top.stitch_pitch &= 0mm..8mm
        .bottom.stitch_pitch &= 0mm..8mm
        .top.thermal_gap &= 0.3mm..
        .top.spoke_width &= 0.3mm..
        .bottom.clearance &= 0.4mm..
        .bottom.min_width &= 0.3mm..
        .bottom.pads &= PadConnection::Solid
    }
    .in: Header1x2
    .out: Header1x2
    .in.p1 ~ .out.p1
    .in.p1.net &= "3V3"
    .in.p2 ~ .out.p2
    .in.p2.net &= "GND"
}
```

A clearance closer than the process makes is refused:

```nsm fails
import std::mfg::jlcpcb::FourLayer
import std::parts::connector::Header1x2

board Close {
    .process: FourLayer {
        .inner1.net &= "GND"
        .inner2.net &= "3V3"
        .top.net &= "GND"
        .top.clearance &= 0.05mm..
    }
    .in: Header1x2
    .out: Header1x2
    .in.p1 ~ .out.p1
    .in.p1.net &= "3V3"
    .in.p2 ~ .out.p2
    .in.p2.net &= "GND"
}
```

```text
error: Close: `process.top.clearance` has a bottom of 0.05 mm, under the process's least space between copper (`.min_clearance`) of 0.1 mm: raise the bottom of the range
```

#### A pad's own connection

A pin of a part may say how a pour joins its pads, over what the pour says (issue #1276): a module's ground pads beside its antenna pin joined solid, say, or a heavy pin's relief with wider spokes. Every pin has two fields for it. `.pour_connection` is `PadConnection::Solid` or `PadConnection::Thermal`; `.spoke_width` is the width of the spokes of the pad's relief, a length on the range convention read at the bottom of the range, as a pour's is. Either may be stated without the other: a spoke width alone holds wherever a pour makes a relief round the pad. The part states them in its own body, so that every board that uses it gets them, and a board may narrow them on a part it uses (`.radio.gnd.pour_connection &= PadConnection::Solid`), as it narrows any field of a part.

What a pin states holds for each of its pads, in every package, and in every pour, plane and zone of its net, whatever their `.pads` says: `fab` writes it on the pads of the KiCad board, as KiCad's `zone_connect` and `thermal_bridge_width`, and netsmith's fill (the Gerbers, the design rules check, `board fill`) and KiCad's both take a pad's own setting over the zone's. `netsmith board fill` lists such a pad with the pin that states it, `solid (as its pin `radio.gnd` states)`, and `--format json` has the pin's path as `stated_by` and its spoke width as `spoke_width`.

```nsm evaluate
import std::mfg::jlcpcb::TwoLayer
import std::mfg::stackup::PadConnection
import std::parts::connector::Header1x2

/// A radio module whose ground pads beside its antenna pin are joined solid to the pour, and
/// whose shield's relief has 0.3 mm spokes. Illustrative; not a real part.
part PouredRadio {
    .designator &= "U"
    pins {
        .ant    passive "ANT" "1"
        .gnd    passive "GND" "2" "3"
        .shield passive "SH"  "4"
    }
    .gnd.pour_connection &= PadConnection::Solid
    .shield.spoke_width &= 0.3mm..
}

board Radioed {
    .process: TwoLayer { .top.net &= "GND"; .bottom.net &= "GND" }
    .radio: PouredRadio
    .feed: Header1x2
    .radio.ant ~ .feed.p1
    .radio.gnd ~ .feed.p2
    .radio.shield ~ .feed.p2
    .feed.p2.net &= "GND"
}
```

A pin states only `Solid` or `Thermal`: a pour joins every pad of its net, and a keepout keeps it off one, while `ThruHoleOnly` chooses by the kind of a pad, which a pin's pads already are. A spoke width beside `Solid`, a bottom of zero or less, or one under the process's `.min_track` is refused too, each naming the pin's field:

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::mfg::stackup::PadConnection
import std::parts::connector::Header1x2

board Unjoined {
    .process: TwoLayer { .top.net &= "GND" }
    .in: Header1x2
    .out: Header1x2
    .in.p1 ~ .out.p1
    .in.p2 ~ .out.p2
    .in.p2.net &= "GND"
    .in.p2.pour_connection &= PadConnection::None
}
```

```text
error: Unjoined: `in.p2.pour_connection` is `PadConnection::None`, and a pin states only `Solid` or `Thermal` for its pads: a pour joins every pad of its net, so keep it off this one with a keepout that forbids copper, or join no pad of the pour (its `.pads`)
```

A pour on a net the board does not have is an error that lists the nets, as for a plane. A pad of the pour's net that is on a plane too gives up the via the router puts beside it where the pour joins it to a stitching via (issue #834; the router's passes over vias are described after Diff pairs); netsmith fills the pour itself, for the Gerbers, the design rules check and the KiCad board `fab` writes, which opens with the pour filled (below, with the zones of the routes file).

Track of other nets on a pour cuts it, and a pour on one side of a two-layer board is the return path of the signals over it. So when a try of the router has routed every connection, and one outer layer is poured on a net while the other is not, the router routes each line with track on that pour again, the most first, with a step on the pour costing twice its length and a via 60 mm of track, and keeps the new copper only where it costs less so counted (issue #431). A track keeps to the other layer where the way round is short, and a board routes exactly as completely as before. The nets of a diff pair (see Diff pairs) are left where the pair's own pass put them. Where both outer layers are poured, as on a board with ground on both, nothing moves. The route scorecard reports how much other nets' track each pour has and how many pieces it falls into.

To see which layers of a board are planes, pours or signal layers, on which net and with which settings, without reading the source or the KiCad board, run `netsmith board copper` (issue #1085; the editor's PCB summary tab shows the same). It reads the source and the routes file, places and routes nothing, and prints each copper layer from the top with its use (`plane`, `pour` or `signal`), its net and a pour's fields as the design states them (`default` for one it does not). The zones of the routes file are counted apart as hand zones, with a layer whose plane or pour a zone of its own net restates marked as not drawn, and one whose plane or pour zones of other nets cut marked as cut (_The routes file_ below). Teardrop fills are counted apart from both: the router makes them after it routes the board, and `fab` writes each as a zone of the KiCad board, hundreds of them, none a pour, so the source has none to count (`--format json` says `"counted": false`) and a count of the KiCad board's zones by net and layer is not a count of pours.

### Keepouts

A `Keepout` is an area where something is not allowed. Four fields say what, each `"forbid"` or `"allow"`, and one that states none forbids all four (the language has no boolean, so the words stand for one): `.tracks`, `.vias`, `.copper` (the pours are not filled there) and `.parts` (no part's courtyard, and no pad). `.layers` is `"all"` (the default) or `"top"`, `"bottom"`, `"inner1"`, `"inner2"`... (as many as the board has inner layers) separated by commas. For `.parts` it names the faces: `.layers &= "bottom"` keeps out the courtyards of the parts on the bottom face and lets a part on the top face stand over the same area, which is how a board says "nothing on the face opposite this antenna" (issue #767). A part stands on one face, so a keepout whose layers are inner ones only keeps out no part, and one that says `.parts &= "forbid"` with such layers is refused. A part's own keepout names the board's faces the same way (not the part's own face, so a part on the bottom still reads `"bottom"` as the bottom of the board). The router, the stitching vias and the placement check keep to it, and the board file has it as a KiCad rule area, so KiCad's check and fill do too.

A keepout is a member of a board's physical file, with its place in the board's frame, or a member of a part, in the part's own frame (the footprint's, turned with the part), so that a part carries its own no-copper zone. A part states it relative to itself and nothing else; the keepouts a board draws are physical intent and are in its physical file (a keepout in a board's circuit source is an error that says so). A keepout zone that a library's footprint draws (a rule area over its pads) is the same: `fab` writes it to the `.kicad_pcb` in board coordinates, turned and moved with the footprint and mirrored on the back, as KiCad saves it (a `.kicad_mod` has it in the footprint's own frame), and netsmith reads the zone of a footprint in a `.kicad_pcb` as board coordinates too (issue #1237). The outline of such a zone may hold arcs (a library's keepout round a mounting hole is often one circle drawn as one arc, its start its end): `fab` moves the three points of an arc as it moves the others, and netsmith reads an arc as points no further than 2 µm (`ARC_ERROR`) from it, in the footprint's keepout and in the board file alike (issue #1299). What it forbids holds as a keepout of the board's does (issue #1291): the router, the stitching of a pour and the vias a pin's `vias` guideline puts in its pads keep out of it (of the box round it, where the part's turn or the zone's shape is not a box), the pour's fill keeps out of it where it forbids pours, and `board drc` and `fab` report a track, a via, a pad (the footprint's own among them, as KiCad's check does) or a part in it as `items_not_allowed`, the finding naming the footprint and what of it forbids the item (`inside the rule area of U1's footprint (no pads)`). Copper that only touches the area's edge is not in it, nor is copper in a hole of the area (a cutout round the module's own pads), as in KiCad's check (issues #1372, #1392).

The editor's PCB view draws them there too (ADR 0048, issue #809). A keepout drawn with its rectangle tool (`K`) is written to the board's physical file as a `Keepout` member at the end of the unit, every field a plain `&=` of a literal, with the `import` it needs; the file is made if there is none:

```nsm-fragment
.keepout1: Keepout {
    .x &= 2mm
    .y &= 2mm
    .width &= 10mm
    .height &= 6mm
    .copper &= "allow"
}
```

The view moves, resizes and deletes any keepout of the physical file that has this shape: `.x`, `.y`, `.width` and `.height` in millimetres, `.layers` and what it forbids as strings, and no other field. A change rewrites the declaration where it stands, so its `///` lines and every other line of the file stay; a delete takes it out with its `///` lines. A keepout of the physical file in another shape (a `.max_height`, a length in micrometres, a sum of lengths, a field stated outside its body as `.antenna.x &= 18mm`) is moved and resized by rewriting its numbers alone (issue #810), when each of `.x`, `.y`, `.width` and `.height` is stated once, with `&=`, by a length of numbers alone: a move or a resize writes the new value in millimetres over the right side of each of those statements whose value changes, so `.x &= 10mm + 8mm` moved 2 mm to the right becomes `.x &= 20mm`, and every other character of the file stays. The view does not change what such a keepout forbids or its layers, and does not delete it: that is done in the file. A keepout whose place or size is computed from another member (`.x &= .outline.width - 12mm`), is a range, or is stated twice is drawn and changed in the file. The change is checked as the file would then be read, and refused with the checker's message if the board does not build (a name another member has), and nothing is written. Every change is one step of the view's undo (Ctrl+Z, Ctrl+Shift+Z), which puts the physical file back as the step found it, and an edit is refused, with nothing written, when the physical file or the layout file changed on disk since the view read the board (a reload shows the board as the files have it, and the view's undo starts again from there). A keepout the source sizes and does not place is still moved in the layout file (_The layout file_).

```nsm evaluate
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::pcb::Keepout

part Radio {
    .package: Package
    .antenna: Keepout {
        .x &= -1mm
        .y &= -9mm
        .width &= 12mm
        .height &= 8mm
        .layers &= "top"
        .vias &= "allow"
    }
}

board Wireless {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .radio: Radio
    .header: Header1x2
}
```

```nsm-physical
import std::pcb::Keepout

physical for Wireless {
    .no_copper: Keepout {
        .x &= 40mm
        .y &= 0mm
        .width &= 20mm
        .height &= 6mm
        .tracks &= "allow"
    }
}
```

Misuse.

```nsm fails
import std::mfg::jlcpcb::TwoLayer

board Open {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
}
```

```nsm-physical
import std::pcb::Keepout

physical for Open {
    .k: Keepout {
        .width &= 5mm
        .height &= 5mm
        .tracks &= "allow"
        .vias &= "allow"
        .copper &= "allow"
        .parts &= "allow"
    }
}
```

```text
error: Open: `k` allows tracks, vias, copper and parts, and forbids nothing
```

A keepout that forbids parts on a face keeps out the parts that stand on it, and the other face is free. One that says `.parts &= "forbid"` and names inner layers only keeps out nothing, and says so.

```nsm fails
import std::mfg::jlcpcb::FourLayer

board Buried {
    .process: FourLayer {
        .stackup.thickness &= 1.6mm
        .inner1.use &= "signal"
        .inner2.use &= "signal"
    }
}
```

```nsm-physical
import std::pcb::Keepout

physical for Buried {
    .under_antenna: Keepout {
        .width &= 5mm
        .height &= 5mm
        .layers &= "inner1"
        .parts &= "forbid"
    }
}
```

```text
error: Buried: `under_antenna.parts` is "forbid" and its `.layers` name neither "top" nor "bottom", the faces a part stands on, so it keeps no part out
```

### Height limits

**Height limits (ADR 0045).** A board that fits in plan view can still hit a lid, a bracket or a neighbouring board. A part states how tall its body stands with the built-in member `.height` (one value, from the face of the board it is on to the top of its body; the board's physical file may narrow it for the board), and a `Keepout` in the physical file states the tallest a part may stand where it is with `.max_height`. A keepout that states `.max_height` and none of `.tracks`, `.vias`, `.copper` and `.parts` forbids nothing else: it only limits height. `.layers` says which face it limits (`"top"` for the parts on the top face, `"bottom"` for the parts on the bottom, `"all"` for both), so a part on the back has its own limit: two keepouts over the same area, one for each face. A board of a system also gets one from each mate, over the area the board mated on it covers, which nobody writes (_The area another board covers_ in [Systems and ports](17-systems-and-ports.md), issue #1331).

A part that states no `.height` has the `height` cell of the catalog row it was picked as, if the row has one (ADR 0094): a catalog file's `height` is one length that a datasheet or a drawing gives, the greatest it allows (the maximum, or the nominal plus the tolerance), with the document, its revision and the page in the row's `source`. A height that the design states wins, whatever the row says, and the row's height is never an input of the pick: no row is chosen or refused for it. Every row of the standard chip catalogs (`resistor`, `capacitor`, `inductor`, `ferrite_bead` and `led`) has a `height`, the greatest thickness its maker's series datasheet allows for the size (issue #1394), so a chip picked from them is checked against a limit. For a board that has a height limit, `fab-notes.txt` says how many parts have a known height and names the row each catalog height was read from. A part with neither may have the _Standard height_ of its footprint (ADR 0106): the greatest height its package's JEDEC outline allows, for the generated footprints a maker's drawing of that outline was read for (`SOIC-8_3.9x4.9mm_P1.27mm` is 1.75 mm, MS-012, `SOT-353_SC-70-5` and `SOT-363_SC-70-6` 1.10 mm, MO-203; the list and the drawings are in `crates/netsmith-pcb/src/standard_height.rs`; chip passives, QFNs, DFNs and SOT-323 have none: no standard prints a maximum for a chip's thickness, a QFN's profile is not in its name, and a drawing that cites SOT-323's registration was not read, so a part in one of these states a `.height` or has a catalog row's `height`, issue #1332). It is never the part's `.height`: where it fits a limit, the part passes and `fab` and `fab-notes.txt` say that the pass rests on the standard (``note: `u1` (U1) fits the 4.00 mm the keepout `lid` allows on the top face by its package standard ...``); where it does not, the part is unchecked, as below, with the standard's figure in the message, because a part in that package may be lower. A part with none of the three has no height: it is not taken as flat and not guessed. Where a limit covers its body, `fab` and the editor print a warning that names it (``height of `r3` (R3) is unchecked ...``), and `fab` writes it in `fab-notes.txt`. With `constraints = "error"` in `netsmith.toml`, or `board fab --constraints error`, it is an error that stops `fab` instead (`--constraints warn` wins over the manifest, and `--allow-drc-violations` does not waive it). A part that does state one is kept out from under a zone it is too tall for by the placer, and the placement check names it, with the numbers, if it stands there anyway. A zone a part carries (a `Keepout` member of a part, in the part's own frame) is kept clear of the same way, wherever the part ends up and however it is turned: the placer keeps the other parts out of it, and the check measures it against every part but the one that carries it. The footprint's 3D model is not read, and a part's own STEP model (`.model`, see _A part's STEP model_) is drawn and never gives a height.

```nsm solve place
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::sources::Rail

board Ribbed {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .rail: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC" }
    .tall: Resistor { .resistance &= 15kohm +/- 1% }
    .flat: Resistor { .resistance &= 15kohm +/- 1% }
    .rail.pos ~> .tall ~> .rail.neg
    .rail.pos ~> .flat ~> .rail.neg
    // The datasheet heights.
    .tall.height &= 9mm
    .flat.height &= 0.5mm
}
```

```nsm-physical
import std::pcb::{Keepout, Outline}

physical for Ribbed {
    case {
        .outline: Outline { .width &= 40mm; .height &= 30mm }
    }
    // The case's lid has ribs over the top 18 mm: nothing taller than 3 mm there.
    .ribs: Keepout {
        .x &= 0mm
        .y &= 0mm
        .width &= 40mm
        .height &= 18mm
        .layers &= "top"
        .max_height &= 3mm
    }
}
```

The placer put `.tall` in the part of the board the ribs leave free. Placed by hand under the ribs (a layout file that locks it there), the check says:

```text
error: Ribbed: `tall` (R2) stands 9.00 mm tall on the top face, and the keepout `ribs` allows 3.00 mm there (6.00 mm too tall), where its body is, around (20.0, 6.0) mm from the board's top left corner
```

The same message is the answer where no position helps: a part that states its face (`.place.side`) under a limit that covers that whole face is placed all the same, and the run reports it too tall with the margin, as it does for a part under a lid by hand. It is a finding of the placement and not one of the lines that cannot all hold, so the placement is there to look at; to clear it, take the part from that face, raise the limit, or use a lower part.

Misuse: a height is one value, above or at zero, and a limit is above zero.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor

board Vague {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .r: Resistor { .resistance &= 15kohm +/- 1% }
    .r.height &= 2mm..5mm
}
```

```text
error: Vague: `r.height` is 2mm..5mm, a range, and a position, a size or a height is one value: say it with `&= 2mm`
```

A catalog row's `height` is one length too, and a cell that is not one is found when a board uses the catalog, with its file and line:

```csv file=catalogs/resistor.csv set=range
mpn,resistance,max_power,package,height,source
ACME-R316,316ohm +/- 1%,62.5mW,0402,0.4mm..0.6mm,https://example.invalid/acme-r316.pdf
```

```nsm fails catalogs=range
import std::parts::passive::Resistor

board Range {
    .r1: Resistor { .resistance &= 316ohm +/- 1% }
}
```

```text
error: `r1` cannot be chosen from the `resistor` catalog: catalogs/resistor.csv:2: `height`: `0.4mm..0.6mm` is a range, and a height is one value: write the datasheet's maximum, such as `0.6mm`
  --> main.nsm:4:5
  |
4 |     .r1: Resistor { .resistance &= 316ohm +/- 1% }
  |     ^^^^^^^^^^^^^
```

**The parts the check cannot check (`height_unknown`, issue #1117).** A pass of the height check says nothing about a part whose height is unknown, so `board fab` and `dev board3d` say which parts it left out: one warning, `height_unknown`, that lists every part of unknown height (no `.height`, no catalog row's height and no package standard's) that stands under a height limit (a keepout's `.max_height`, or the standoff of a body) or under the body of another part of known height on its face, each with the limit that would apply:

```text
warning: height_unknown: the height check cannot say whether 2 parts of unknown height fit, so a pass does not cover them: `u1` (U1) on the top face under the keepout `lid`, which limits it to 3.00 mm; `u2` (U2) on the top face under the body of `module` (J1) (8.00 mm tall), which states no `.standoff` (the limit that a standoff would put over it); state each part's `.height` (for example `.u1.height &= 8mm`, with the real number from its datasheet)
```

It is a warning always: `fab` still passes, with strict constraints too (the per-part `height_unchecked` of a part under a limit is the error that strict constraints make). `fab-notes.txt` has the line, `dev board3d --format json` has it as `height_unknown` (with the parts and what each stands under), and the `.glb` as a finding of that code. State each part's `.height` to leave the warning. The second part of the example is a module that stands on pin strips with a gap under its body and states no `.standoff` (the next section): its body is taken to stand on the board, so no limit is over `u2`; stating the `.standoff` of the module makes it the limit `standoff of J1`, and a part that is taller is `too tall`. Where such a module overlaps a part whose height is known, the overlap that `dev board3d` reports names the statement that is missing, `.module.standoff` and `.module.pin_protrusion` (the body of the part with plated holes that is the larger of the two, standing on the board).

### Height growth

A cell or a supercapacitor swells over its life, so the height that fits a lid on the day it is built may not fit it a few years on. A part has the built-in member `.height_growth` (a length, one value, zero or more): the most its body's top may rise above its height over its life, from the datasheet's end-of-life swelling or the maker's figure (issue #1263). The part, a user part or the board states it, in the circuit source or in the physical file, as `.height` is stated. The height check adds it to the part's height, the stated `.height` or the catalog row's `height` (a stated one wins, as above), or the package standard's, before it compares the height with a limit, and the finding says that the part is too tall when grown, with both numbers. The placer keeps a part out from under a limit its grown height is over, and a part that stands over protruding pins is judged at its grown height too. A part that states a growth and no height is still of unknown height: the growth says nothing of how tall it is. The 3D view, the STEP export and the overlap of two bodies draw and compare the height as built, without the growth.

```nsm solve place refused
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Capacitor
import std::sources::Rail

board Backup {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .rail: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC" }
    // Stands for a supercapacitor: 2.5 mm tall, and its datasheet allows 0.8 mm of swelling.
    .store: Capacitor { .capacitance &= 10uF +/- 20% }
    .rail.pos ~> .store ~> .rail.neg
    .store.height &= 2.5mm
    .store.height_growth &= 0.8mm
}
```

```nsm-physical
import std::pcb::{Keepout, Outline}

physical for Backup {
    case {
        .outline: Outline { .width &= 20mm; .height &= 15mm }
    }
    // The lid leaves 3 mm over the whole top face, and the part is on the top face.
    .lid: Keepout {
        .x &= 0mm
        .y &= 0mm
        .width &= 20mm
        .height &= 15mm
        .layers &= "top"
        .max_height &= 3mm
    }
    .store.place.side &= "top"
}
```

```text
error: Backup: `store` (C1) stands 3.30 mm tall on the top face when grown (its 2.50 mm height with its 0.80 mm `.height_growth` added), and the keepout `lid` allows 3.00 mm there (0.30 mm too tall), where its body is, around (2.0, 1.1) mm from the board's top left corner
```

Misuse: a growth is one value, zero or more: a body that shrinks is stated at its lowest `.height`.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor

board Shrinks {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .r: Resistor { .resistance &= 15kohm +/- 1% }
    .r.height &= 2mm
    .r.height_growth &= -0.5mm
}
```

```text
error: Shrinks: `r.height_growth` is -0.5 mm, and a growth does not lower a body: state the height it has at its lowest in `.height` and what it grows by in `.height_growth`
```

### A body that stands off the board

A plug-in module stands on pins, and its body has a gap under it. A part has the built-in member `.standoff` (a length, one value, zero or more): how far the underside of its body is above the face of the board it is on, so that the body stands from the standoff up to its `.height`. The board's physical file states it as it states `.height`, and a path may reach a part inside a module. A part that states none stands on the board, a standoff of zero, as every part did before. A standoff at or above the part's own `.height` is an error: `.height` is the top of the body, measured from the face, and the body would have no thickness.

A standoff changes four things. Another part on the same face may stand under the body only as tall as the standoff: the body is a height limit over its fabrication outline (its courtyard, where the footprint draws none) for the parts on its face, named `standoff of J1`, and a part that is taller is `too tall` with the numbers, as under a keepout's `.max_height`; a part of no stated height under it is reported as unchecked, as under a lid. The placer keeps a part too tall for it out from under the body. The overlap that `dev board3d --format json` reports for two bodies compares Z ranges, each from its standoff up to its height, so a low part under a body that stands off it is no overlap. And the 3D view, the `.glb`, the STEP export and `view 3d` draw the body from the standoff, not from the board; the pins under it and the part's own STEP model (`.model`, placed by its own `.model.z`) are drawn where they are stated. The standoff does not change the courtyard: a footprint that draws a courtyard shape for each socket strip lets a part stand between the strips, under the body, and the standoff is then what limits that part's height.

```nsm solve place
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::sources::Rail

board Module {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .rail: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC" }
    .module: Resistor { .resistance &= 15kohm +/- 1% }
    .low: Resistor { .resistance &= 15kohm +/- 1% }
    .rail.pos ~> .module ~> .rail.neg
    .rail.pos ~> .low ~> .rail.neg
    // The module is 6 mm tall on pins that lift its underside 2 mm off the board.
    .module.height &= 6mm
    .module.standoff &= 2mm
    // A part up to 2 mm tall fits under it.
    .low.height &= 1mm
}
```

```nsm-physical
import std::pcb::Outline

physical for Module {
    case {
        .outline: Outline { .width &= 40mm; .height &= 30mm }
    }
}
```

Were `.low.height` 3 mm and the part put under the module by hand (a layout file that locks it there), the check would say:

```text
error: Module: `low` (R1) stands 3.00 mm tall on the top face, and the standoff of R2 allows 2.00 mm there (1.00 mm too tall), where its body is, around (20.0, 15.0) mm from the board's top left corner
```

Misuse: a standoff is below the height.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor

board Flat {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .r: Resistor { .resistance &= 15kohm +/- 1% }
    .r.height &= 2mm
    .r.standoff &= 6mm
}
```

```text
error: Flat: `r.standoff` is 6 mm and `r.height` is 2 mm: the body stands from its standoff up to its height, so the height is the top of the body, above the standoff
```

### An underside keepout

A module that stands on sockets or spacers covers more of the board than its footprint: the footprint's outline holds only what is soldered (two socket rows), and the maker allows only parts lower than a figure under the module's body (a "0.5 mm under-height" rule). A part states that area as its member `.underside_keepout`, of `std::pcb::UndersideKeepout` (issue #1335), in its own frame (the footprint's, as on the top face), declared where the part is defined or in a use's block:

- a rectangle, `.x` and `.y` (the corner nearest the footprint's origin), `.width` and `.height`; or a `polygon` of straight edges in its block, `polygon (-10mm, -5mm) (10mm, -5mm) (10mm, 5mm) (-10mm, 5mm)`, for a body that is not a rectangle (no `round` or `arc through`, and not with the four fields);
- `.max_height`, the tallest a part may stand under the body, from the face of the board, one length above zero.

It is a keepout the part carries that limits only height: every other part on the face the part is on whose body is in the area, at its height grown by its `.height_growth`, may stand at most `.max_height`, wherever the placer puts the part and whichever face (it turns over with a part on the bottom). A part on the other face is not under the body, and the part itself is not measured. A part too tall is `too tall`, and the finding names it and the part whose keepout it is; a part of no known height under it is unchecked, as under a lid. The placer keeps a part too tall for it out of the box round it (the box of a polygon); the check measures the polygon. Where the part is a connector of a system's mate, the other board's parts are under it too: the area goes over the other connector, turned and mirrored as the mate puts the two together (the pads say how, so neither needs a place), and limits the parts on that board's facing face to the same `.max_height` from its own face, named `System/member.connector.underside_keepout`. A `.standoff` is a different limit: the gap under the footprint's own outline.

The name is the member's: an `UndersideKeepout` under another name, or one of a board or a module, is an error, and a part has one.

```nsm solve place
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::pcb::UndersideKeepout
import std::sources::Rail

board Carrier {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .rail: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC" }
    // Stands for a module whose body covers 20 by 10 mm round its footprint; its maker allows
    // 0.5 mm under it.
    .module: Resistor {
        .resistance &= 15kohm +/- 1%
        .underside_keepout: UndersideKeepout {
            .x &= -10mm
            .y &= -5mm
            .width &= 20mm
            .height &= 10mm
            .max_height &= 0.5mm
        }
    }
    .low: Resistor { .resistance &= 15kohm +/- 1% }
    .rail.pos ~> .module ~> .rail.neg
    .rail.pos ~> .low ~> .rail.neg
    .module.height &= 6mm
    .low.height &= 1.2mm
}
```

```nsm-physical
import std::pcb::Outline

physical for Carrier {
    case {
        .outline: Outline { .width &= 40mm; .height &= 30mm }
    }
}
```

The placer keeps `low` out from under the module. Were it put there by hand (a layout file that locks it 6 mm right of the module), the check would say:

```text
error: Carrier: `low` (R1) stands 1.20 mm tall on the top face, and the underside keepout of `module` allows 0.50 mm there (0.70 mm too tall), where its body is, around (26.0, 15.0) mm from the board's top left corner
```

Misuse: an underside keepout of the board.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::pcb::UndersideKeepout

board Loose {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .area: UndersideKeepout {
        .x &= 0mm
        .y &= 0mm
        .width &= 5mm
        .height &= 5mm
        .max_height &= 1mm
    }
}
```

```text
error: Loose: `area` is an `UndersideKeepout`, and it is the area under a part's body: declare it in the part as its member `.underside_keepout` (issue #1335)
```

### Pin protrusion

**Readings (ADR 0045, section 15); the name `.pin_protrusion` and the rule were decided by the owner on 2026-11-04.** On a tight board a flat part sits on one face over the pins of a through-hole part that come through from the other face. The pins are cut after soldering, so what protrudes is lower than the flat part's underside. A part has the built-in member `.pin_protrusion` (a length): how far the pins of a through-hole part protrude out of the far side of the board, whether they were cut or bought short. The board's physical file states it for one part, as it states `.height`, and a path may reach a part inside a module (`.radio.row_a.pin_protrusion &= 0.2mm`). A part that states none has an unknown protrusion, which is not zero: its pads stay an obstacle for a part on that face, as before.

A part on that face may stand over the pins when all of these hold, and `fab` says which does not:

- it is fitted by hand, after the pins are cut (`.assembly` is `"hand"` or `"none"`; a part the board house assembles is on the board before any pin is soldered);
- it states its `.height`;
- the protrusion plus its height fits the height limit that a keepout puts on that face where its body is (the pins protrude `t`, the part rests on them, so `t + height` must not be more than `.max_height`). With no limit there, nothing is compared, and `fab` warns of that.

Its own pads still keep the process's clearance from the pads of the through-hole part. Here the socket's pins protrude 0.2 mm, the regulator stands 1.8 mm tall and the lid leaves 2.2 mm, so it places over them:

```nsm solve place
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::regulator::LinearRegulator
import std::sources::Rail

board Stacked {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .input: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VIN" }
    .output: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "3V3"; .current &= 0mA..50mA }
    .reg: LinearRegulator
    .reg.vin ~ .input
    .reg.vout ~ .output
    .socket: Header1x2
    .input.pos ~ .socket.p1
    .input.neg ~ .socket.p2
    // The regulator is fitted by hand, after the socket's pins are cut.
    .reg.assembly &= AssemblyMode::Hand
    // Datasheet: it stands 1.8 mm tall.
    .reg.height &= 1.8mm
}
```

```nsm-physical
import std::pcb::{Keepout, Outline}

physical for Stacked {
    case {
        .outline: Outline { .width &= 24mm; .height &= 16mm }
    }
    // Case drawing (invented): the lid leaves 2.2 mm under the back of the board.
    .back_room: Keepout {
        .x &= 0mm
        .y &= 0mm
        .width &= 24mm
        .height &= 16mm
        .layers &= "bottom"
        .max_height &= 2.2mm
    }
    // The regulator lies on the back across the socket's two pads, a hair apart in x.
    .socket.place.side &= "top"
    .socket.place.rotation &= 0
    .socket.place.x &= 12mm +/- 0.02mm
    .socket.place.y &= 8mm +/- 0.02mm
    .reg.place.side &= "bottom"
    .reg.place.rotation &= 0
    .reg.place.x &= 14.75mm +/- 0.02mm
    .reg.place.y &= 8mm +/- 0.5mm
    // The socket's pins are cut after soldering to protrude 0.2mm.
    .socket.pin_protrusion &= 0.2mm
}
```

`fab` then writes a step for a person next to `hand-solder:` in `fab-notes.txt` and `order.txt`: `trim: J1 (socket): after soldering, cut the pins to protrude 0.20 mm from the bottom face, before U1 (reg) is fitted (stacked.physical.nsm:26); measure one protruding pin`. The files cannot carry it, so measure one protruding pin before the covering part goes on. Never state a protrusion to make a finding go away: cutting the pins is a hand step, and it is stated only when it is done.

With pins that protrude 0.6 mm the stack is 2.4 mm. The lines pin both parts over each other, so `fab` refuses before it places anything, naming the lines and the protrusion that would fit (a placement that is checked afterwards says ```reg` (U1) lies over the pins of `socket` (J1), whose pins protrude 0.60 mm ...``):

```nsm solve place refused
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::regulator::LinearRegulator
import std::sources::Rail

board Stacked {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .input: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VIN" }
    .output: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "3V3"; .current &= 0mA..50mA }
    .reg: LinearRegulator
    .reg.vin ~ .input
    .reg.vout ~ .output
    .socket: Header1x2
    .input.pos ~ .socket.p1
    .input.neg ~ .socket.p2
    // The regulator is fitted by hand, after the socket's pins are cut.
    .reg.assembly &= AssemblyMode::Hand
    // Datasheet: it stands 1.8 mm tall.
    .reg.height &= 1.8mm
}
```

```nsm-physical
import std::pcb::{Keepout, Outline}

physical for Stacked {
    case {
        .outline: Outline { .width &= 24mm; .height &= 16mm }
    }
    // Case drawing (invented): the lid leaves 2.2 mm under the back of the board.
    .back_room: Keepout {
        .x &= 0mm
        .y &= 0mm
        .width &= 24mm
        .height &= 16mm
        .layers &= "bottom"
        .max_height &= 2.2mm
    }
    // The regulator lies on the back across the socket's two pads, a hair apart in x.
    .socket.place.side &= "top"
    .socket.place.rotation &= 0
    .socket.place.x &= 12mm +/- 0.02mm
    .socket.place.y &= 8mm +/- 0.02mm
    .reg.place.side &= "bottom"
    .reg.place.rotation &= 0
    .reg.place.x &= 14.75mm +/- 0.02mm
    .reg.place.y &= 8mm +/- 0.5mm
    // The socket's pins are cut after soldering to protrude 0.6mm.
    .socket.pin_protrusion &= 0.6mm
}
```

```text
error: Stacked: no placement meets every requirement of the physical file:
  these lines cannot both hold: main.physical.nsm:19 `.socket.place.x &= 12mm +/- 0.02mm`; main.physical.nsm:20 `.socket.place.y &= 8mm +/- 0.02mm`; main.physical.nsm:18 `.socket.place.rotation &= 0`; main.physical.nsm:23 `.reg.place.x &= 14.75mm +/- 0.02mm`; main.physical.nsm:24 `.reg.place.y &= 8mm +/- 0.5mm`; main.physical.nsm:22 `.reg.place.rotation &= 0`; main.physical.nsm:21 `.reg.place.side &= "bottom"`; main.physical.nsm:26 `.socket.pin_protrusion &= 0.6mm`: they keep the courtyard of `reg` (U1) within x 12.83 to 16.67, y 5.8 to 10.2 mm, over a pad of `socket` (J1) (pins protruding 0.6 mm) by at least 0.02 x 0.775 mm wherever it is put, and the protrusion does not excuse that: `reg` (U1) stands 1.80 mm tall on pins that protrude 0.60 mm: 2.40 mm from the bottom face, and the keepout `back_room` allows 2.20 mm there (0.20 mm too much); cut the pins to protrude 0.40 mm or less, or use a lower part
```

The same refusal is made, with its own sentence at the end, when the covering part states no height (`state .reg.height`) or when the board house assembles it (`make U1 hand`). A part over the pads of a through-hole part that states no protrusion is the plain overlap, with a sentence that names the member to state. A part kind may bound the protrusion, and a board that states more breaks that claim, which is refused with the line of the bound:

```nsm fails
import std::mfg::jlcpcb::TwoLayer

part ShortPins {
    .designator &= "J"
    pins {
        .p1 passive "P1" "1"
    }
    .pin_protrusion <= 3mm
    .package: Package
}

board TooLong {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .j: ShortPins
    .j.pin_protrusion &= 5mm
}
```

```text
error: j.pin_protrusion must always be at most 3mm, but 5mm is not always below 3mm
  --> main.nsm:8:5
  |
8 |     .pin_protrusion <= 3mm
  |     ^^^^^^^^^^^^^^^^^^^^^^
```

### A part's STEP model

**A part may be drawn from a STEP model of the project (ADR 0108).** Every part has the built-in member `.model`, of the prelude's interface `StepModel` (in scope everywhere, no import). `.model.file` names a STEP file of the project, relative to the file that states it; `view 3d`, the `.glb`, the editor's 3D view and the STEP export draw it in place of the part's _Package shape_, and `dev board3d --format json` and `fab-notes.txt` name it with its SHA-256. It is stated where `.height` is: a part's definition states it once for every use of the part, and a board or its physical file states it for one part. The model is placed at the footprint's origin, on the part's face, turned with the part and mirrored on the bottom face. `.model.rx`, `.ry` and `.rz` turn it, in degrees about x, then y, then z, and `.model.x`, `.y` and `.z` then move it, so that the numbers of a KiCad footprint's `(model ... (offset (xyz ...)) (rotate (xyz ...)))` line can be copied in. The frame is KiCad's for a model: x along the footprint's x, y up the footprint as the footprint editor shows it (the footprint's own y runs down), z out of the part's face, and a positive angle turns clockwise seen from the axis's positive end, as KiCad turns a model (`docs/BOARD3D.md` has the order and the test that pins it). Each is one value, and an unstated one is zero. netsmith never bundles a model and never downloads one.

```nsm
import std::parts::connector::Header1x2

/// An invented two-pin receptacle, drawn from its maker's model.
part Receptacle {
    .designator &= "J"
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    // The maker's model stands on its own origin, turned a quarter and sunk 0.2 mm.
    .model.file &= "models/receptacle.step"
    .model.rz &= 90
    .model.z &= -0.2mm
}

board Charger {
    .usb: Receptacle
    .header: Header1x2
    .usb.a ~ .header.p1
    .usb.b ~ .header.p2
}
```

The model is a drawing, never a height: the part's `.height` still comes from what _Height limits_ lists, and a part of unknown height drawn from a model stays unknown to the checks. A model whose top stands above the part's height, by more than the reader's 0.01 mm, is a warning that names both figures and suggests the `.height` to state; a file the reader refuses is a warning, and the part keeps its generated shape. A maker's license often keeps its model out of a public repository, so a file that is not there is a warning, from `build`, `check` and `fab`, and the part is drawn with its generated shape:

```text
warning: Charger: `usb.model.file` names `src/models/receptacle.step` (src/charger.nsm:11), and there is no such file, so `usb` is drawn with its generated shape; add the file, or leave it out on a machine that may not have it
```

A path outside the project (the folder of `netsmith.toml`, or the folder of a file that has none) is an error, and so is an absolute path, and a path stated in two files of two folders that names a different file from each. Two statements of two files are the usual contradiction:

```nsm fails
part Plug {
    .designator &= "J"
    .p1: Pin
    .model.file &= "models/plug.step"
}

board Two {
    .plug: Plug
    .plug.model.file &= "models/other.step"
}
```

```text
error: contradiction on `plug.model.file`: it is already "models/plug.step", which does not overlap "models/other.step"
  --> main.nsm:9:5
  |
9 |     .plug.model.file &= "models/other.step"
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

An offset or a rotation with no file places the model the project's model table gives the part (below), and is refused where the table gives none, as there is nothing to place; so is a rotation or an offset that is a range.

**A part that has no model.** `.model.file &= ""` says the part has none: it is drawn with its generated shape, and the project's model table (below) gives it none. Use it for a part the table would cover but that is not that package, such as a connector whose footprint is named like a stock one. An offset or a rotation stated with an empty file is refused.

**A model that does not sit on its footprint.** Once a board is placed, each model drawn is checked against its part's footprint, in the footprint's own frame: the model's plan box must not reach more than 0.25 mm past the courtyard (a footprint that draws no courtyard says nothing about a body), and must reach to within 0.25 mm of the centres of the pads. A model turned a quarter turn, or moved off its footprint, is both. The check is a warning from `dev board3d` and `fab` and in the editor's 3D view, naming the box and the footprint's, and the model is drawn as it is placed. A model whose pins, tabs or shell stand outside what the footprint draws is a warning too: it is a check to read, not a measurement.

### A model table

**Most parts take their model from their footprint (issue #763).** A standard part (`Resistor`, `Capacitor`, `Schottky`...) is defined in `std`, so a project cannot state its model in the definition, and a model fits the package the solver picks, not the part. So `netsmith.toml` may hold a table of models keyed by the footprint id each part is mapped to (`kicad.toml`, the standard rows or the project's own: `Resistor_SMD:R_0402_1005Metric`). Every part that states no `.model.file` is drawn from the model of its footprint; a part that states one is drawn from its own, so the table is a default and a part's statement the override, with no contradiction between them:

```toml
[project]
name = "sensor"

[models]
folder = "${KICAD9_3DMODEL_DIR}"

[[model]]
footprint = "Package_SO:VSSOP-10_3x3mm_P0.5mm"
file = "${KICAD9_3DMODEL_DIR}/Package_SO.3dshapes/MSOP-10_3x3mm_P0.5mm.step"

[[model]]
footprint = "netsmith:USB_C_Receptacle_GCT_USB4105_16P_TopMnt_Horizontal"
file = "models/usb4105.step"
rotation = [0, 0, 90]
```

A part's footprint `Lib:Name` finds its model this way: a `[[model]]` row of that id first; else, with a `[models] folder`, the file `<folder>/<Lib>.3dshapes/<Name>.step` (or `.stp`, `.STEP`), which is how KiCad lays out its `kicad-packages3D`, so a project that uses KiCad's names needs no row for them; else no model, and the part keeps its generated shape. A generated `gen:` footprint is found by a row only. Nothing is guessed from a similar name: a package KiCad has under another name (a VSSOP-10 is KiCad's MSOP-10) gets a row. A row's `offset` (mm along x, y, z) and `rotation` (degrees about x, then y, then z, before the offset) are those of `.model.x` ... `.model.rz`, zero when left out, so a footprint's `(model ... (offset) (rotate))` numbers can be copied in.

The folder is a second place models are read from, besides the project: it may be outside the project, so a checkout of KiCad's models is used without a copy in every project. It is relative to `netsmith.toml`, or absolute, and may start with an environment variable (`${KICAD9_3DMODEL_DIR}`, KiCad 9's own). A row's `file` is spelled the same way and must be inside the project or inside the folder; one outside both is an error. netsmith still bundles and downloads no model. A row whose file is not there is a warning that names the parts it would draw, and they keep their generated shape, as a part's own missing model does; so is a folder that is not there, or a variable that is not set (a clone or a CI job may have neither). A footprint the folder has no file for is no warning: the folder is only looked in.

A part may state only the placement of the model the table gives it: `.model.rz &= 90` with no `.model.file` draws the table's model turned 90 degrees, and `.model.x` ... `.model.rz` that it states take the place of the row's number of the same name (a row's `rotation = [0, 0, 180]` and a part's `.model.rz &= 90` give a rotation of 0, 0, 90), while those it does not state keep the row's. A part states the whole placement with its own `.model.file`. Where the table has no model for the part (no row, no file in the folder, a row whose file is not there), the placement is an error, as there is nothing to place.

A model from the table is what a part's own model is: a drawing, never a height, so its top above the part's height is the same warning. `build` and `fab` say what the table did, and which parts are drawn with no model, so "every part has a model" is one line:

```text
note: LedExample: the model table (netsmith.toml) draws 2 parts from a STEP model; 1 part has no model, and it is drawn with its generated shape: `power_in` (Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical)
```

`fab-notes.txt` and `dev board3d --format json` name a model from the folder as `<folder>/<Lib>.3dshapes/<Name>.step`, with the folder as `netsmith.toml` spells it, and its SHA-256. A design file (`board build --emit-design`) holds only the models the design states, as the table is the project's and the folder this machine's.

**Rows from a KiCad project's footprint libraries.** `netsmith import kicad-libs models fp-lib-table` prints a `[[model]]` row for each footprint of the table's libraries whose `(model ...)` line names a STEP file, with the line's offset and rotation (an old `(at (xyz ...))` in inches is turned into mm), to review and paste into `netsmith.toml`, as `import kicad-libs libraries` prints the `[[library]]` stanzas for `kicad.toml`. `--library NAME` (repeat for several) reads only those libraries; `${KIPRJMOD}` is the folder of the table and another `${NAME}` is the environment variable. A footprint with no model line, a model that is not STEP (a `.wrl`) or a scaled one is no row: notes on stderr count them. A library whose folder is not there, or whose variable is not set, is a note.

```text
note: the library `acme`: 3 footprints have no model line: left out
[[model]]
footprint = "acme:Chip"
file = "models/chip.step"
offset = [0, 0, 0.1]
rotation = [0, 0, 90]
```

### A case model

**A board may name the STEP file of its case (ADR 0109).** A `CaseModel` (`std::pcb::CaseModel`) is declared in the `case { }` section of the board's physical file, one member for each file: a base and a lid are two members, or one file with two solids. `.file` names a STEP file of the project, relative to the physical file. It has the fields and the rotation order of a part's `StepModel` (_A part's STEP model_), and places the model in the 3D scene's frame (`docs/BOARD3D.md`: millimetres, z up, the origin at the outline's bottom-left corner on the top face): `.rx`, `.ry` and `.rz` turn it, in degrees about x, then y, then z, and `.x`, `.y` and `.z` then move it. Each is one value, and an unstated one is zero. The side up is a rotation and the gap an offset: a box drawn upside down in its CAD file that the board's bottom face rests in is turned half a turn about x and moved down by the board's thickness.

**A pose known as a matrix.** A case model's pose often comes as a matrix: a STEP assembly's transform, or what a CAD program shows for a placed body. Turning it into three angles by hand is where a mistake hides: a pose that is a quarter turn about two axes has two sets of angles, and a sign slip draws a symmetric case mirrored with no finding. `netsmith import case place` takes the matrix, 3 x 4 and row major (each row three numbers of the rotation and then the offset in mm; 16 numbers for a 4 x 4 matrix whose last row is 0 0 0 1), and prints the six lines to paste into the case model: `netsmith import case place "1 0 0 0  0 -1 0 0  0 0 -1 -1.6"` prints `.rx &= 180`, `.ry &= 0`, `.rz &= 0`, `.x &= 0mm`, `.y &= 0mm` and `.z &= -1.6mm`, the upside-down box below. Of the two sets of angles it prints the one with `.ry` from -90 to 90 (and `.rz` 0 when `.ry` is a quarter turn), turns them back into a matrix, and says on stderr how near that is to the one given. A matrix that is not a rotation is refused, within 0.0001: one that mirrors (a determinant of -1: the matrix is of another frame, a y-down drawing, or has a sign slip), scales (a model in inches) or shears. The case model keeps the angles; there is no second way to state a pose.

**The board a case model holds, printed as source.** `netsmith import case model box.step` cuts the model by a plane parallel to the board and prints what the cut says of the board, as `netsmith import dxf` prints a drawing's: to review and paste into the `case { }` section (`--write board.nsm` edits `board.physical.nsm` as `import dxf --write` does). It is never read at build time, so a clone without the maker's file builds the same board (ADR 0109). The model is posed as a `CaseModel` poses it (`--rx`, `--ry`, `--rz`, `--x`, `--y`, `--z`), and the plane is at `--at` mm in that frame (the board's top face is 0, so the default, -0.8, is the middle of a 1.6 mm board). The cut draws closed curves on the model's solids, and how many of the others enclose each is its depth: a curve with none is the outside of a wall and is not used; a **pocket** in the wall (depth 1, the largest if there are several) is the board's outline, with the wall's ribs, notches and rounded corners; a **post, pin or block** standing in the pocket (depth 2) is a `Cutout`, which must be a rounded rectangle, a slot or a circle, as with `import dxf`. With `--holes-at` (a standoff's height, below the board) a second cut gives the **mounting holes**: each circle that is a hole in a post inside the pocket (depth 3) is a `MountingHole` at its centre, stated by its diameter (`.diameter`, as `import dxf` states a hole; a drill the footprint generator cannot draw refuses the import). The cut is of the model's triangles, within the reader's 0.01 mm of chord: the lines and arcs are found again and printed to a micrometre, and a shape that is not lines and circular arcs stays chords. The output says which point of the posed model is the board's origin, and the `.x` and `.y` the model needs to stand where the board's frame has it. A solid that is not closed at the plane is an error that says where. With `--limits` it also prints the **height limits** the case puts over and under the board, each a `Keepout` with `.max_height`, named `limit1`... (the top face's from the lowest, then the bottom face's). Each underside of the case above the board's top face (the lid, a rib or a boss hanging from it) limits the parts on the top face (`.layers &= "top"`, `.max_height` the underside's height from the top face); what the case holds below the board's bottom face (a floor, a rib or a boss standing under the pocket) limits the parts on the bottom face (`.layers &= "bottom"`, `.max_height` from the bottom face down to it). The bottom face is `--thickness` mm below the top face (1.6 by default, as `--at` assumes). A limit is cut to the outline; one whose rectangle lies in another of the same height or a lower one is left out, and what stands through the board or from its face up to the case (a post or a block, which is a cutout) is not a limit. A region that is not a rectangle is covered by rectangles, taken outward to 0.05 mm: an L is two, a lid with a hole through it is the rectangles round the hole, and a round underside is slabs of at most `--limit-step` mm (0.5 by default), so it is limited a little more than it covers, not over its box. A sloped underside (a dome, a slope, a chamfer: a face that leans more than about 10 degrees from upright, so a wall's draft is none) is followed in steps of `--limit-step` mm, each step limited to the height of its lower end: over a step the underside is no lower than the limit and up to a step higher, and a feature thinner than a step is not seen; a smaller step gives more, finer limits. The limits are a review aid, so read the numbers. The command does not read the height of a part's own model; a plane at which a solid is not closed is a warning and the limits it shows are left out. `--write` replaces every numbered member of the kinds it prints (`cutout1`..., `hole1`... with `--holes-at`, `limit1`... with `--limits`), also those above the new count, so an import that finds fewer limits than an earlier one leaves none of the earlier one's. A height a limit does not give a part is unchanged: a part of unknown height under a limit is reported as under any keepout (_Height limits_).

```sh
netsmith import case model case/box.step --holes-at -4
```

```text
.outline: Outline {
    polygon (60mm, 38mm) arc through (59.4114mm, 39.4114mm) (58mm, 40mm) (32mm, 40mm)
            (32mm, 34mm) (28mm, 34mm) (28mm, 40mm)
            (2mm, 40mm) arc through (0.5886mm, 39.4114mm) (0mm, 38mm) (0mm, 2mm) arc through (0.5886mm, 0.5886mm)
            (2mm, 0mm) (58mm, 0mm) arc through (59.4114mm, 0.5886mm) (60mm, 2mm)
}

.cutout1: Cutout { .x &= 41mm  .y &= 18mm  .width &= 8mm  .height &= 4mm }
.hole1: MountingHole { .diameter &= 2.5mm }
.hole1.place.x &= 54mm
.hole1.place.y &= 6mm
.hole2: MountingHole { .diameter &= 3.2mm }
.hole2.place.x &= 6.001mm
.hole2.place.y &= 34mm
```

The listing is shortened: the command prints the `import` lines, the `physical for` block and a comment for each of the model's pose, the section, the frame and what it found.

```nsm solve
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

board Boxed {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .power: Header1x2
}
```

```nsm-physical
import std::parts::passive::Color
import std::pcb::{CaseModel, Outline}

physical for Boxed {
    case {
        // Case drawing BX-2, sheet 1: the inside of the base.
        .outline: Outline { .width &= 40mm; .height &= 24mm }
        // The base, modelled opening down: turned over, and the board rests on its bosses.
        .shell: CaseModel { .file &= "case/base.step"; .rx &= 180; .z &= -1.6mm }
        .lid: CaseModel { .file &= "case/lid.step"; .color &= Color::Black }
        // The plug that mates with the header, drawn where it sits and measured against nothing.
        .plug: CaseModel { .file &= "models/plug.step"; .check &= "none" }
    }
}
```

A case model is a drawing and a check, never a height, an outline or a hole: the outline, the cutouts, the mounting holes and the height limits stay statements of the `case { }` section (by hand, with `netsmith import dxf`, or with `netsmith import case model`, which prints them from the model's section for you to review), and the check says where they disagree with the model. The STEP export leaves the case out. netsmith never bundles a case model and never downloads one.

**The fit check.** `fab`, `dev board3d` and the editor read each file with netsmith's own STEP reader, place it and measure the board against its solids: each part's body box to its height (stated, catalog or the package standard's), the pins of a part that states how far they protrude (`.pin_protrusion`, a box under its plated pads out to that length), and the board's own solid. The findings, each with its numbers, the part and the solid (`docs/BOARD3D.md`, _The case model's fit_, has the table):

- `case_collision`: a body, stated pins or the board inside a case solid by more than 0.01 mm. Touching is contact, so a board resting on a boss is fine. For a part of a stated or catalog height, for stated pins, and for the board, it is an error that stops `fab`, and neither `--allow-drc-violations` nor `--constraints warn` waives it, as a part too tall for a height limit is.
- `case_clearance`: a body or stated pins nearer a case surface than `.clearance`, a warning.
- `case_reach`: a connector at the board's edge with the case between its face and the outside, or its face more than `.reach` inside the wall's outer face, a warning.
- `case_boss`: a mounting hole with no case solid within `.boss_gap` above or below its ring, a warning.
- `case_unchecked`: a part of unknown height under the case, a part of standard height that collides (a part in that package may be lower), or a solid whose shell is not closed, which gets no test for a body wholly inside it. A warning, and an error with strict constraints.

The three figures are fields of `CaseModel` that std offers with `?=` (`.clearance` 0.5 mm, `.reach` 1.5 mm, `.boss_gap` 0.5 mm, PROVISIONAL): a board narrows one for its case, `.shell.clearance &= 1mm`, and a negative one is refused. A file netsmith's reader cannot read stops `fab`, as a path outside the project does. `build` and `check` place nothing, so they do not measure the fit.

```text
error: Boxed: case_collision: J1 (`power`), 9.00 mm tall (stated), is inside the solid `lid` of the case model `lid` by 1.00 mm at (16.74, 9.91, 8.00): the case and the board collide; a collision with the case stops `fab`, and neither `--allow-drc-violations` nor `--constraints warn` waives it: move or lower the part, or correct the case model's placement (`.x`, `.y`, `.z`, `.rx`, `.ry`, `.rz`)
```

The file is found as a part's model is: a path outside the project, an absolute path, or one stated in two folders that names two files is an error. A maker's license often keeps a case's file out of a public repository, so a file that is not there leaves the fit unchecked: a warning from `build`, `check` and `fab`, and an error that stops `fab` when constraints are strict (`constraints = "error"` in `netsmith.toml`, or `board fab --constraints error`; `--allow-drc-violations` does not waive it), as a part of unknown height under a limit is:

```text
warning: Boxed: `shell.file` names `src/case/base.step` (src/boxed.physical.nsm:13), and there is no such file, so the board's fit in its case model `shell` is not checked; add the file, or leave it out on a machine that may not have it
```

**A model's colour (issue #756).** Every case model is drawn in one grey, see-through, unless something gives it a colour. A STEP file that styles its faces (a CAD program writes the colour of a body or a face into the file) is drawn in them: each solid, or each run of faces, in the file's own colour, still see-through, or opaque with `view 3d --case solid` and the editor's `Solid` toggle. A board that wants another colour states `.color` on the model, a variant of the same `Color` an LED's `.color` and a board's solder mask are (`import std::parts::passive::Color`, `.lid.color &= Color::Black`; `Red`, `Green`, `Blue`, `Yellow`, `White`, `Black`, `Purple`): it replaces every colour of that file, for that model only, and changes no check, no height and no finding. A model that states none, and whose file has no colours, is drawn as it always was, so a design file and a picture of a board that states none are unchanged. `dev board3d --format json` lists the stated `color` of a model and the `file_colours` its file has (as `#rrggbb`). Which model is drawn at all is a separate choice: `view 3d --case hidden:lid` and the editor's checkbox for each model (`docs/BOARD3D.md`).

**A model that is only a drawing.** (`.plug` above.) A board often shows a model that is not its case: the connector it mates with, a neighbouring module, a bracket, a cable. Such a model touches the board where it is meant to (a plug's contact tails end on the board's face, a through-hole part's pins go through it), so `case_collision` would stop `fab` for a collision that is meant. Declare it as a `CaseModel` and state `.check &= "none"`: it is placed and drawn as a case model is, in the 3D view and in `view 3d`, and nothing is measured against it (no `case_collision`, `case_clearance`, `case_reach` or `case_boss`, and its solids do not count as a boss for a hole). `.check` is `"fit"` (the default) or `"none"`, and any other word is refused; `dev board3d --format json` lists the model with `"checked": false`. A file that is not there or not readable still gives a `case_unchecked` finding, which says the model is not drawn.

A case model with no `.file`, or an empty one, is refused, as there is nothing to place, and so is a rotation or an offset that is a range. A case is the board's: a `CaseModel` belongs in the `case { }` section, and one in the circuit source, in a module or elsewhere in the unit is refused:

```nsm fails
import std::parts::connector::Header1x2

board Loose {
    .power: Header1x2
}
```

```nsm-physical
import std::pcb::CaseModel

physical for Loose {
    .shell: CaseModel { .file &= "case/base.step" }
}
```

```text
error: a case model belongs in the `case { }` section of `physical for Loose`: wrap `.shell: CaseModel` in `case { ... }`
  --> main.physical.nsm:4:13
  |
4 |     .shell: CaseModel { .file &= "case/base.step" }
  |             ^^^^^^^^^
```

### Labels

A `Label` is silkscreen text: a part with no pins that is fitted by nobody, placed like any other part, and declared in the `case` section of the board's physical file. `.text` is what it prints and `.text_height` how tall (it was `.height` before the part height of _Height limits_ took that name); unset, the process's least text height, and never less. The reference designator of a label is `LBL1`, and it is not in the schematic.

```nsm evaluate
import std::mfg::jlcpcb::TwoLayer

board Marked {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
}
```

```nsm-physical
import std::parts::mechanical::Label

physical for Marked {
    case {
        .revision: Label { .text &= "REV A"; .text_height &= 1.2mm }
    }
}
```

### Logos

A `Logo` is a drawing printed on the silkscreen or etched in the copper, from a vector file kept beside the source: a part with no pins that is fitted by nobody, placed like any other part (with a courtyard round the drawing's box), and declared in the `case` section of the board's physical file. Its designator is `LOGO1`, and it is not in the schematic or the bill of materials.

- **`.file`** is an SVG or a DXF file, relative to the file that states it and inside the project. A file that is missing, outside the project, a raster image (`.png`, `.jpg`: never read) or unreadable is an error that names the file, the line that states it and what is wrong. netsmith bundles and downloads no drawing.
- **`.size`** is how long the longer side of the drawing's box is, a range of which the least wins. It is stated: the file's own units are not read, so the drawing is scaled to it, and its other side follows. It is never less than the process's least text height on the silkscreen, or its least track in copper, which is the smallest mark that is legible; a smaller size is an error that names both numbers.
- **`.layer`** is `"silkscreen"` (the default) or `"copper"`. The drawing is made on the face the part is on, mirrored on the back as text is. A copper logo is copper of no net: pours keep their clearance from it, the router and the placer keep clear of its courtyard, and the design rules check holds it clear of the copper of every net as it holds a pad. Solder mask covers it, as it covers a track.
- **An SVG** gives the filled shapes of its `path`, `polygon`, `rect`, `circle` and `ellipse` elements; curves are cut into short straight pieces. A shape inside another is a hole in it. A `transform`, a shape with no fill (`fill="none"`: a stroke is not drawn), and an element that draws text, an image, a clone or a line are errors that name the element and its line: convert the text and the strokes to paths and apply the transforms in the drawing program. Colours, opacity and the fill rule are not read.
- **A DXF** gives the closed loops of its lines, arcs, circles, splines and polylines on every layer, as the outline reader finds them; a loop with a gap is an error. A loop inside another is a hole.

```nsm evaluate
import std::mfg::jlcpcb::TwoLayer

board Branded {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
}
```

```nsm-physical
import std::parts::mechanical::Logo

physical for Branded {
    case {
        .mark: Logo { .file &= "art/mark.svg"; .size &= 8mm }
        .etched: Logo { .file &= "art/etched.dxf"; .size &= 6mm; .layer &= "copper" }
    }
}
```

### Silkscreen text

Every part prints its reference designator on the silkscreen, in a place the placer keeps clear of pads and of other text, and at least as large as the process's smallest text. What the silkscreen prints is physical intent (ADR 0048), so it is stated in the board's physical file, with two members that every part has: `.silk`, a word (`"show"` or `"hide"`) for whether it prints a designator, and `.silk_text`, a text it prints in place of the designator. A module and the board have `.silk` too, which says it for every part inside them, at any depth.

```nsm solve place
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::passive::Resistor

module Sensor {
    .j: Header1x2
    .r: Resistor { .resistance &= 4.7kohm +/- 5% }
}

board Clean {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .uart: Header1x2
    .power: Header1x2
    .sensor: Sensor
}
```

```nsm-physical
physical for Clean {
    // A clean face: no designators on the silkscreen, except where one is wanted.
    .silk &= "hide"
    // The sensor block is probed by hand: its parts print their designators.
    .sensor.silk &= "show"
    // The connector prints what its pins are, not its designator.
    .uart.silk_text &= "UART"
}
```

- **The most specific statement wins.** A part's own `.silk` beats the `.silk` of the module it is in, which beats the one of the module around that, and the board's comes last. A part that none of them speaks of prints its designator.
- **`.silk_text` replaces the designator on the silkscreen only.** The designator stays on the schematic, in the bill of materials, in the placement file and on the fabrication layer (`F.Fab`); only the text on `F.SilkS` (`B.SilkS` for a part on the back) changes. The text is placed, sized and counted like a designator is: at least the process's smallest text, clear of pads and other text, on the side of the part the placer chooses. A part that states `.silk_text` prints it whatever the board or a module says of designators; it is an error to say `.silk &= "hide"` and `.silk_text` of one part, and an empty text is an error.
- **The characters of a silkscreen text** are those of netsmith's own stroke font: ASCII, the Latin-1 letters with accents (`é`, `Ü`, `ñ`, `å`, `ç`, `ß`, `ø`), the Greek letters of formulas (`Ω`, `µ`, `Δ`, `π`, `θ`) and signs such as `° ± × ÷ ² ³ ½ € £ ≤ ≥ ≈ → ∞`. A character it lacks (a Chinese character, an emoji) is drawn as an open box and named in `fab-notes.txt`, so a board is never plotted with text silently missing; `fab` still succeeds, and the note says which text to change. The same font draws a text box or a curve a board file brings along on the silkscreen layers.
- **A hidden designator takes no room.** The placer does not make room for it and the checks give no finding about it.
- **A value other than `"show"` or `"hide"`** is an error that names the member. A module has no `.silk_text`: a text belongs to one part.
- **Circuit source.** The board's circuit source does not state these members, as it does not state a `.place` (ADR 0048); the message says where they go.

For every part of a kind, the board's physical file has a `defaults` block of its own, which offers `.silk` only (ADR 0135): `Resistor.silk ?= "hide"` hides the designator of every resistor on the board, at any depth, and the other parts keep theirs. Its lines are those of the circuit source's `defaults` (_Narrowing and relations_): `Kind.silk ?= "show"` or `"hide"`, the kind a part, a generic or a module the physical file imports, and a kind reaches every part made from it or providing it. A module's line gives the module's own `.silk`, which then reaches every part inside it.

```nsm solve place
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::passive::Resistor

board Quiet {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .r1: Resistor { .resistance &= 10kohm +/- 5% }
    .r2: Resistor { .resistance &= 4.7kohm +/- 5% }
    .j: Header1x2
}
```

```nsm-physical
import std::parts::passive::Resistor

physical for Quiet {
    defaults {
        // No resistor prints its designator; the header keeps `J1`.
        Resistor.silk ?= "hide"
    }
    // Except the one that is fitted by hand.
    .r2.silk &= "show"
}
```

- **Where a kind's line stands.** A part's own `.silk` or `.silk_text` beats the line of its kind, and the line beats the `.silk` of the module the part is in and the board's: the order is the part, its kind, the nearest module outward, the board. A line that reaches no part (the board has none of the kind, or each states `.silk` or `.silk_text` itself) is a note, as in the circuit source.
- **Only `.silk`.** A line for any other member is refused in the physical file (it is in the circuit source's `defaults`), and `.silk` is refused in the circuit source's. A `.silk_text` line is refused: a text belongs to one part. Only a board's physical file holds the block, not a module's, and not inside `case { }`.

Misuse: the circuit source states what the silkscreen prints.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

board Marked {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .uart: Header1x2
    .uart.silk_text &= "UART"
}
```

```text
error: `uart.silk_text` is what the silkscreen prints, which is physical intent, and a board's circuit source does not state it (ADR 0048); put it in `physical for Marked` in `main.physical.nsm`
  --> main.nsm:7:5
  |
7 |     .uart.silk_text &= "UART"
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^
```

### Where a part goes

A case, a panel or a carrier fixes where some parts must be: a connector's mouth at a slot, a button under its cap. Every part and every module has a built-in member `.place` (ADR 0045) that holds the requirements. It is narrowed with the ordinary operators, like `.assembly`: `&=` for a value or a range (`2mm +/- 0.1mm`, `1mm..2mm`), `>=` and `<=` for a bound or an expression, such as half the outline's height. A tolerance needs a constant on both sides, so a window round an expression is two bounds. Declaring `.place` yourself is an error. Order never matters.

The lines are in the board's physical file (ADR 0048), in the unit `physical for Board`: a board's circuit source that narrows a `.place` is an error that says so. A part inside a module is named by its path, `.sensor.j.place.y`. All the examples below are two blocks, the circuit source and its physical file.

| Field | Meaning |
|---|---|
| `.x`, `.y` | the centre of the part's body |
| `.left`, `.right`, `.top`, `.bottom` | the edges of the body's box, after the part is turned |
| `.rotation` | degrees counterclockwise on screen, as in the layout file; a plain number, any angle (_A turn off the right angles_) |
| `.turn_step` | the step in degrees the placer may turn the part by: 90 unless stated, or 45 to let it try the turns between the right angles too; a plain number, a part's only (_A turn off the right angles_) |
| `.side` | `"top"` or `"bottom"`; on a module it is a requirement on every part of it, and on the board itself (the only field a board's `.place` has) one on every part of the board (_A board on one face_) |

Positions are lengths in the board's frame (the outline's top left corner is the origin, x right, y down), and they are measured from the **body**: the outline the footprint draws on its fabrication layer (`F.Fab`), else its courtyard, else its pads and 0.25 mm (the courtyard the placer gives a footprint that draws none; a courtyard takes in every hole that reaches past it, and every pad that does with 0.15 mm round it, since the placer keeps courtyards apart and lets them touch; a footprint that draws several courtyard shapes keeps them apart, _A courtyard of several shapes_ in the chapter on projects). The box is found in the footprint's own frame and turned with the part, and the placer and the check measure the same one. The case states the shell of a connector, not the origin the footprint's author chose, so a requirement survives a change of package. The layout file's `at` stays the footprint's origin; the check converts, and every finding says which drawing it used. A field the board leaves alone is unconstrained.

**Putting a footprint's origin somewhere.** A port must put a footprint's origin where the original's is, and a requirement measures the body. The two are a fixed distance apart for a part on one face and turned one way. `fab` prints it for every part after the line `wrote ...`, and `board fab --format json` gives it in the `parts` of each board (`origin`, `body_centre` and `offset`, in mm from the outline's top left corner):

```text
where each footprint's origin is, in mm from the outline's top left corner (y down), and how far it is from the centre of the body that `.place.x` and `.place.y` measure: to put the origin at (x, y) with the part on that face and turned so, write `.place.x &= x - dx` and `.place.y &= y - dy`:
  J1 (`power_in`) top, turned 90: origin (10.73, 9), body centre (12, 9), dx -1.27 dy 0
```

Set the face and the turn first, run `fab` once, and write `.place.x` and `.place.y` from the line: for the origin at (10, 9), `.power_in.place.x &= 11.27mm` and `.power_in.place.y &= 9mm`. `netsmith dev parity` writes the two lines out for each part that must match, in an `origin` line of its report (and in `origins` with `--format json`), with the figures of the original's origin.

```nsm solve
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

module Sensor {
    .j: Header1x2
}

board Cased {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .power: Header1x2
    .sensor: Sensor
}
```

```nsm-physical
import std::pcb::Outline

physical for Cased {
    case {
        .outline: Outline { .width &= 40mm; .height &= 24mm }
    }

    // Case drawing, slot A: the header's body is 1.5 mm in from the left edge, turned, and
    // centred along it.
    .power.place.left &= 1.5mm +/- 0.1mm
    .power.place.y >= .outline.height / 2 - 0.1mm
    .power.place.y <= .outline.height / 2 + 0.1mm
    .power.place.rotation &= 90
    .power.place.side &= "top"

    // The whole sensor block stays right of 20 mm.
    .sensor.place.left >= 20mm
    // The sensor's header, a part inside the module, is named by its path.
    .sensor.j.place.y &= 8mm +/- 0.2mm
}
```

The requirements are hard. A requirement that leaves one place and one turn (every field a single value) fixes the part like `lock = true` in the layout file. Any other puts the part in a window the placer keeps to, and it gives the module's parts the half of the module's requirements that keeps them in (a least `.left` or `.top`, a most `.right` or `.bottom`). A part the layout file locks is checked against them before anything is placed; a part it merely moves is measured afterwards. The layout file keeps the solution; the physical file keeps only the requirement, never a solved position.

**A turn off the right angles** (issues #822 and #996). A part turns by any angle its requirement states: `.r1.place.rotation &= 45` places the resistor turned 45 degrees, the check measures it so, and the board file, the Gerber files and the placement file have that turn. Its courtyard is measured turned: each shape of it is a row of up to 24 strips across the board, each strip the box of the turned shape's part in it and no wider than 0.25 mm while there are no more than 24, so other parts come as near the turned courtyard as to a straight one, give or take half a strip (the one box that holds a long part at 45 degrees would keep them far off its two corners). The outline and the requirements still measure the boxes that hold the courtyard and the body turned: `.place.left` of a part at 45 degrees is the left edge of that box. Where the window holds a right angle (`&= 80..100`) the part keeps to the right angles, as before; where it holds none (`&= 30..60`) the part is turned to the middle of the window (45). The placer chooses the right angles unless a part allows more: a part with no requirement on its rotation, or with one that allows a right angle, is turned by 0, 90, 180 or 270 degrees. `.r1.place.turn_step &= 45` (issue #1046) lets the placer turn the resistor by any step of 45 degrees from the turn it starts at: the annealer tries the eight turns, a part too long for the outline in every right angle is started at the first step of 45 degrees that fits, and a requirement on the rotation still holds (`.r1.place.rotation &= 0..90` with a step of 45 leaves 0, 45 and 90). The step is 90 or 45, one value; any other number, or a window, is an error. A module has no `.turn_step`, and a part the layout file locks keeps its turn. A part that does not state it is turned by right angles only, so a board is placed as before until a part says so; a part that may not sit askew, such as a connector at a slot or a crystal by its pins, should not. The layout file takes any turn too (`netsmith layout set --turn 45`), and so does the editor's part inspector, whose turn field takes any angle where the Rotate button steps by 90. JLCPCB's correction for such a turn has not been measured: `cpl.csv` has the turn plus the footprint's offset, as for any turn, and `fab-notes.txt` lists each part on the top face turned so under `turn off the right angles unverified`, to be checked in the order preview.

`netsmith board build` refuses what can be seen without a placement: bounds that leave nothing, a side that is neither `"top"` nor `"bottom"`, and a requirement measured from an outline that has no fixed size.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

board Squeezed {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .j: Header1x2
}
```

```nsm-physical
physical for Squeezed {
    .j.place.y >= 12mm
    .j.place.y <= 3mm
}
```

```text
error: the requirements on `j.place.y` cannot all hold: it must be at least 12mm (`j.place.y >= 12mm`) and at most 3mm (`j.place.y <= 3mm`)
  --> main.physical.nsm:3:5
  |
3 |     .j.place.y <= 3mm
  |     ^^^^^^^^^^^^^^^^^
```

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

board Stretchy {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .j: Header1x2
}
```

```nsm-physical
import std::pcb::Outline

physical for Stretchy {
    case {
        .outline: Outline { .width &= 40mm; .height &= 20mm..30mm }
    }
    .j.place.y >= .outline.height / 2
}
```

```text
error: Stretchy: `j.place.y` is measured from the outline's top left corner, and `outline.height` does not state one size, so the board has no fixed outline; give `outline.height` a single value (`&= 30mm`), or state the position without it
```

#### A board on one face

A board assembled on one face only (one pass through the reflow oven, a case with no room behind the board, a back that lies on a flat surface) states it once, on the board: `.place.side` in its physical file, with no path before it. It is the requirement a module's `.place.side` is, on every part of the board at any depth, so the placer keeps every part on that face and `fab` measures each. `"bottom"` puts every part on the back, for a card whose front is a cover.

```nsm
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::passive::Resistor

board OneFace {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .j: Header1x2
    .r: Resistor {}
}
```

```nsm-physical
physical for OneFace {
    // Assembled in one pass: every part on the top face.
    .place.side &= "top"
}
```

The board's `.place` has only `.side`: a board is not placed, so `.place.x` on it is an error. A part or a module that states the same face as the board is allowed; one that states the other face is an error that names both lines, in either order. A board with a few parts on the other face states no face for itself and states the faces of its parts or modules one by one.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::passive::Resistor

board OneFace {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .j: Header1x2
    .r: Resistor {}
}
```

```nsm-physical
physical for OneFace {
    .place.side &= "top"
    .j.place.side &= "bottom"
}
```

```text
error: the board `OneFace` is on the top face, which is a requirement on every part of it (stated at main.physical.nsm:2), but `j.place.side` is "bottom"; the board `OneFace` is assembled on one face only; to put some parts on the other face, take `.place.side` off the board and state the faces of its parts or modules one by one
  --> main.physical.nsm:3:5
  |
3 |     .j.place.side &= "bottom"
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^
```

#### A relation between two parts

Some requirements are between two parts and fix neither: two rows of sockets a datasheet puts 17.5 mm apart, an antenna header that is on the line of its cable. They are written as the equation of the fields, in the physical file, and they may reach into modules by path, at any depth. A relation is the measure of one part's body minus the measure of another's, along one axis, in a window. `==` with a range is the window (`17.5mm +/- 0.05mm`, or `.a.place.y == .b.place.y` for a window of zero), and `>=` and `<=` give a one-sided window.

```nsm solve place
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

module Radio {
    .row_a: Header1x2
    .row_b: Header1x2
}

board Paired {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .radio: Radio
    .antenna: Header1x2
}
```

```nsm-physical
import std::pcb::Outline

physical for Paired {
    case {
        .outline: Outline { .width &= 50mm; .height &= 30mm }
    }

    // Datasheet figure 4: the two rows are 17.5 mm +/- 0.05 mm apart, centre to centre.
    .radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.05mm
    // Cable drawing: the antenna header is on the line of row A.
    .antenna.place.y == .radio.row_a.place.y
}
```

The solver does not evaluate a relation between two parts, because placement is not a solver output; the placer and the check do (ADR 0045, section 12). The placer treats the relation as a hard requirement like the others: it moves the two parts together until the measure is in the window, and a window narrower than its 0.05 mm grid is met exactly by putting one of the parts off the grid by a few hundredths of a millimetre. The same inputs give the same board. A relation is one equation of two fields of two different parts, each with the coefficient 1 and -1 (a sum, a product, a turn, a module's own `.place` and two fields of one part stay in the file unevaluated, and the build lists them as `not evaluated`), along one axis (`x`, `left` and `right`, or `y`, `top` and `bottom`), with everything else a constant, such as `.outline.width / 2`. A window that can never be held is an error of `build`; two parts that the requirements or the layout file's locks have already fixed, or one fixed and one whose own window cannot reach the relation, are refused before placing with the numbers. A relation that the placed board does not meet is a finding, as any other requirement is, and stops `fab`:

```text
error [constraint] constraint radio.row_b.place.x - radio.row_a.place.x (paired.physical.nsm:11) is violated: the centre of the body of J2 (`radio.row_b`, F.Fab) minus the centre of the body of J1 (`radio.row_a`, F.Fab) is 16 mm along x, and it allows 17.45 to 17.55 mm (1.45 mm over)
```

#### A row of parts

A row of test pads, or of the pins of a bank of switches, is a numbered run of parts on one line, each a fixed distance from the next. The `row` statement says it once, in the physical file only (owner decision, 2026-11-04, ADR 0048): `row .pads.tp1 .. .pads.tp4 along x pitch 2.54mm +/- 0.05mm`. The first and the last part name the run: the same path up to the last segment, then a name that ends in a number, and every number between them is a part (`tp1`, `tp2`, `tp3`, `tp4`). `along x` or `along y` is the axis of the pitch. The statement puts the parts on one line (the other axis equal) and each one at the pitch from the one before it, within the window; a pitch without a window is held exactly, like a relation without one. A negative pitch runs the row the other way. The pitch is a length.

```nsm solve place
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

module Pads {
    .tp1: Header1x2
    .tp2: Header1x2
    .tp3: Header1x2
    .tp4: Header1x2
}

board Probed {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .pads: Pads
}
```

```nsm-physical
import std::pcb::Outline

physical for Probed {
    case {
        .outline: Outline { .width &= 50mm; .height &= 30mm }
    }

    // Test fixture drawing: the four pads are 5 mm apart on one line near the bottom edge.
    .pads.tp1.place.y >= .outline.height - 8mm
    row .pads.tp1 .. .pads.tp4 along x pitch 5mm +/- 0.05mm
}
```

The statement is the same as the equations it stands for: for each pair of neighbours `.pads.tp2.place.x - .pads.tp1.place.x == 5mm +/- 0.05mm` and `.pads.tp2.place.y == .pads.tp1.place.y`, so the placer holds it and `fab` measures it with no new mechanism. The distance of the whole row from the edge, or its face, is stated on a part or a module as before (`.pads.place.side &= "top"` puts every pad on the top face). A finding names the row's file and line, and the two neighbours it is about:

```text
error [constraint] constraint pads.tp3.place.x - pads.tp2.place.x (probed.physical.nsm:13) is violated: the centre of the body of J3 (`pads.tp3`, F.Fab) minus the centre of the body of J2 (`pads.tp2`, F.Fab) is 7 mm along x, and it allows 4.95 to 5.05 mm (1.95 mm over)
```

The parts of a row are numbered in natural order, so the names and the labels agree: `.pads.tp2` is `J2` and `tp10` follows `tp9` (see _Designators_). A run is refused, with the statement quoted, when it is not one: an end that does not end in a number, ends with different names or in different modules, a first number above the last, a run of one part, a part missing between the ends (a gap), an end that is not a part (a module, a value), or a pitch that is not a length. A number written with leading zeros keeps the width of the first (`tp01 .. tp12`). A `row` in the circuit source is an error that says it belongs in the physical file.

Misuse: a gap in the run.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

module Pads {
    .tp1: Header1x2
    .tp2: Header1x2
    .tp4: Header1x2
}

board Gappy {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .pads: Pads
}
```

```nsm-physical
physical for Gappy {
    row .pads.tp1 .. .pads.tp4 along x pitch 5mm
}
```

```text
error: `row .pads.tp1 .. .pads.tp4` has a gap: `pads` has no member `tp3`, and a row is every number from the first to the last
  --> main.physical.nsm:2:5
  |
2 |     row .pads.tp1 .. .pads.tp4 along x pitch 5mm
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

#### A part near a pin

A connector that feeds an antenna, a crystal that drives a pin, a part whose line must be short: a datasheet or an RF layout gives the most distance from a part to one pin of another, and the physical file states it as a requirement (issue #1274): `.antenna within 8mm of .radio.ant`. The first path is a part, or a pin of a part; the path after `of` is a pin of a part; the distance is one length above zero. It may reach into modules by path, as every requirement of the physical file does, and a module's own physical file may state it of its parts.

```nsm solve place
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

module Radio {
    .ant: Pin
    .row_a: Header1x2
    .row_a.p1 ~ .ant
}

board Linked {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .radio: Radio
    .antenna: Header1x2
    .antenna.p1 ~ .radio.ant
}
```

```nsm-physical
import std::pcb::Outline

physical for Linked {
    case {
        .outline: Outline { .width &= 50mm; .height &= 30mm }
    }

    // Radio datasheet, antenna layout: the connector within 8 mm of the antenna pin.
    .antenna within 8mm of .radio.row_a.p1
}
```

It is measured as a datasheet's `near ... within` is (chapter _Parts_): pad centre to pad centre, from the nearest pad of the part on the net of the pin (any of its pads when none is on it; the pin's own pads when the statement names a pin of the part) to the nearest pad of the pin. The placer pulls the part to the pin as it pulls a decoupling capacitor, and holds the distance as a hard rule, as it holds a wall: each millimetre past it costs as much as a millimetre into a wall. `fab` and `board drc` measure it on the board, and a part farther than the distance is a finding of the rule `constraint`, which stops `fab` as any requirement does (`constraints = "warn"` makes it a warning):

```text
error [constraint] constraint antenna within 8 mm of radio.row_a.p1 (linked.physical.nsm:9) is violated: the nearest of pad 1 of J2 is 19.5 mm from pad 1 of J1, centre to centre, and it allows at most 8 mm (11.5 mm over): move J2 nearer J1
```

`netsmith board fab --format json` has the same as data in `requirements`, with `measure` `distance`, `measured`, `max` and `over`. A part of a bank (a `CapacitorBank`'s path) is no part the board has, and a `within` statement about one is not measured.

Misuse: the end is a part, not a pin.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

board Loose {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .radio: Header1x2
    .antenna: Header1x2
}
```

```nsm-physical
physical for Loose {
    .antenna within 8mm of .radio
}
```

```text
error: `.radio` is a part, and `.antenna within 8mm of .radio` measures to a pin of a part, such as `.radio.ant`
  --> main.physical.nsm:2:28
  |
2 |     .antenna within 8mm of .radio
  |                            ^^^^^^
```

#### A distance between kinds of parts

Where a datasheet gives no number, a project keeps rules of thumb about which kinds of parts stand apart: a radio module and a switching regulator, a crystal and a buck converter, an antenna connector and a fast clock. The `keep` statement of a board's physical file says one (issue 1277, owner decision): `keep 10mm between BuckIc and Crystal` asks every part of the first kind to stand at least 10 mm from every part of the second. A kind is a part or a generic the physical file imports, as a line of `defaults` names one: the standard library's (`std::parts::regulator::BuckIc`, `std::parts::passive::Crystal`) or the project's own, and a kind reaches every part made from it or providing it, at any depth of the board. The two kinds may be the same: `keep 20mm between Radio and Radio` keeps every two radios apart. The distance is one fixed length of zero or more, and a pair of kinds states one.

```nsm solve place
import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Capacitor
import std::parts::passive::Resistor
import std::sources::Rail

board Spaced {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .rail: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC" }
    .r1: Resistor { .resistance &= 15kohm +/- 1% }
    .r2: Resistor { .resistance &= 15kohm +/- 1% }
    .c1: Capacitor { .capacitance &= 100nF +/- 10%; .max_voltage >= .bias * 1.5 }
    .rail.pos ~> .r1 ~> .rail.neg
    .rail.pos ~> .r2 ~> .rail.neg
    .rail.pos ~ .c1.a
    .rail.neg ~ .c1.b
}
```

```nsm-physical
import std::parts::passive::Capacitor
import std::parts::passive::Resistor
import std::pcb::Outline

physical for Spaced {
    case {
        .outline: Outline { .width &= 30mm; .height &= 20mm }
    }

    // Project rule of thumb (no datasheet number): the capacitor stands 6 mm from every resistor.
    keep 6mm between Resistor and Capacitor
}
```

It is a requirement on where parts go, as a `.place` line is. The distance is measured from courtyard to courtyard in the plane of the board, whatever faces the two parts are on (a regulator under a radio is no further from it): the least distance between any two shapes of the courtyards, along one axis where they face each other and corner to corner otherwise. The placer keeps it as hard as an overlap, and a board on which it cannot hold is placed as near as the placer gets. Where the placed board has two parts nearer than the statement asks, `fab` stops after placing and before routing, with both parts, the numbers and the line:

```text
error: requirement `keep 6mm between Resistor and Capacitor` (main.physical.nsm:11) is violated: `c1` (C1) is 1.5 mm from `r1` (R1), courtyard to courtyard, and it asks for 6 mm (4.5 mm short); move them apart, or change the rule
```

`board fab --format json` has each pair in `requirements`, with `measure` `distance`, the `measured` distance, the least as `min` and how far short as `over`. `constraints = "warn"` in netsmith.toml (or `board fab --constraints warn`) makes it a warning, and `--for-review` lists it under FOR REVIEW. `layout check` says two parts the layout file places too near as a `requirement` error, at the line of the entry of one of them. A statement that reaches no two parts (the board has none of a kind, or one part of a kind kept from itself) is a note. The statement belongs in a board's physical file, outside `case { }`: in the circuit source and in a module's own physical file it is an error. A module is not a kind here (name the kind of the part inside it), nor is an interface or a value. Not built: a distance by face, and a keepout between two kinds (a band where nothing of either kind goes).

Misuse: an interface is not a kind of part.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

board Apart {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .j: Header1x2
}
```

```nsm-physical
import std::parts::connector::Header1x2
import std::sources::Rail

physical for Apart {
    keep 3mm between Rail and Header1x2
}
```

```text
error: `Rail` is an interface; `keep` names a part or a generic
  --> main.physical.nsm:5:22
  |
5 |     keep 3mm between Rail and Header1x2
  |                      ^^^^
```

#### A module's own physical file

A module can carry the geometry that is true wherever it goes: the 17.5 mm between two rows of sockets that a radio's datasheet gives, two parts on one line. It states it in a physical file of its own, and every board that uses the module gets it (ADR 0048, the module layer; owner decision of 2026-11-11). The module names the file in its circuit source with `physical "<stem>.physical.nsm"`, the file beside its source by stem, and the file holds a unit `physical for Module { ... }`. The same file may hold the units of the source's boards too, as below. A module that names a file that is not there, or a file with no unit for it, is an error, and so is a unit for a module that does not name its file: the geometry never goes missing without a message when the module is copied to another project.

```nsm solve place
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

module Radio {
    // Where the rows stand to one another is in the physical file.
    physical "main.physical.nsm"
    .row_a: Header1x2
    .row_b: Header1x2
}

board Docked {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .radio: Radio
    .antenna: Header1x2
}
```

```nsm-physical
import std::pcb::Outline

physical for Radio {
    // Datasheet figure 4: the two rows are 17.5 mm +/- 0.05 mm apart, centre to centre, on one line.
    .row_b.place.x - .row_a.place.x == 17.5mm +/- 0.05mm
    .row_b.place.y == .row_a.place.y
    // The row a case slot holds, by a name that a rename inside the module does not break.
    anchor .dock = .row_a
}

physical for Docked {
    case {
        .outline: Outline { .width &= 50mm; .height &= 30mm }
    }

    // Case drawing, slot B: the dock row is 8 mm in from the left edge.
    .radio.dock.place.left &= 8mm +/- 0.2mm
    // This board's case holds the rows tighter than the datasheet does.
    .radio.row_b.place.x - .radio.dock.place.x == 17.5mm +/- 0.02mm
    // Cable drawing: the antenna header is on the line of the dock row.
    .antenna.place.y == .radio.dock.place.y
}
```

- **A module's file states only how its parts stand to one another**, which is the same wherever the board puts the module: a relation whose places cancel along each axis (`.b.place.x - .a.place.x`, the centre between two parts minus a third, a `row`), and a part's `.height`, `.height_growth`, `.routing_margin`, `.standoff`, `.pin_protrusion`, `.model` and `.silk`. A position, a turn or a face stated with `&=` (`.row_a.place.x &= 3mm`), or a sum of two places, is in the board's frame and is refused: the board's file places the module. A module's file declares nothing (no `case { }`, no keepout). It reaches the parts of modules inside the module by path, as a board's does.
- **A board's file may only narrow what a module's states.** An offset between two parts that the module's file states may be stated again by the board with a window inside the module's (here 17.5 mm +/- 0.02 mm inside +/- 0.05 mm); a wider window, or one that does not meet it, is an error that names both lines. A `&=` narrows as every `&=` does. A board that needs a different spacing changes the module.
- **An anchor** (`anchor .dock = .row_a`, in the module's file only) is a second name for a part or a module inside it, which every physical file that reaches into the module may use, the module's own too. A board's file reaches a part inside a module by its path, so a rename inside the module breaks every such path; through an anchor, the rename changes the one `anchor` line. An anchor may not take the name of a member. The circuit source does not see anchors.

Misuse: a part renamed inside the module.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

module Radio {
    .row_first: Header1x2
}

board Renamed {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .radio: Radio
}
```

```nsm-physical
physical for Renamed {
    .radio.row_a.place.left &= 8mm
}
```

```text
error: `.radio.row_a.place.left` names nothing: module `Radio` has no member `row_a`. A physical file reaches inside a module by the path of the part, so a rename inside `Radio` breaks this path; an `anchor .name = .part` line in the physical file of `Radio` gives a part a name that a rename inside the module changes in one place
  --> main.physical.nsm:2:12
  |
2 |     .radio.row_a.place.left &= 8mm
  |            ^^^^^
```

Misuse: a board's window wider than the module's.

```nsm fails
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

module Radio {
    physical "main.physical.nsm"
    .row_a: Header1x2
    .row_b: Header1x2
}

board Loose {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .radio: Radio
}
```

```nsm-physical
physical for Radio {
    .row_b.place.x - .row_a.place.x == 17.5mm +/- 0.05mm
}

physical for Loose {
    .radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.1mm
}
```

```text
error: `.radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.1mm` would loosen the physical file of module `Radio`, which states `.row_b.place.x - .row_a.place.x == 17.5mm +/- 0.05mm` (main.physical.nsm:2): it leaves `radio.row_b.place.x - radio.row_a.place.x` between 17.4mm and 17.6mm, and the module's file between 17.45mm and 17.55mm; a physical file may only narrow what a module's states (ADR 0048), so tighten the window to lie inside the module's, or change the module
  --> main.physical.nsm:6:5
  |
6 |     .radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.1mm
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

#### A part that hangs over the board edge

A plug that sticks out of the case has part of its courtyard outside the outline. That is an error ("not wholly inside the board outline") unless the design says the part may hang over that edge. **The owner decided the rule on 2026-11-03 (ADR 0045, section 10); how a module states an overhang for its parts is still open.** The design says it with a requirement it can already write: a window for an edge of the **body** that lies at or beyond the same edge of the outline. `.j.place.left &= -0.5mm +/- 0.1mm` puts the body's left edge 0.5 mm past the board's left edge, and `.plug.place.bottom >= .outline.height` puts its bottom edge on the board's bottom edge or past it (flush counts). Only the part's own `left`, `right`, `top` and `bottom` say it, and only when the whole window is at or beyond the edge. A window on `.x` or `.y`, a bound a module states for its parts, or an entry of the layout file puts the part somewhere but does not say it may hang over, so the courtyard past the edge is the error it always was. That holds for a part the layout file locks (`lock = true`) too: the lock fixes where the part is, and the part must still state every edge its courtyard is past, like any other (owner decision, 2026-11-03). A locked part past an edge nobody stated gets the error below, naming the edge, the distance and the line that would allow it. On a polygon outline the same statements cover the rounded corners at the edge they name (see _A polygon outline_, above); there `.outline.height` is not a value, so the requirement states the edge as a number (`.plug.place.bottom >= 74.5mm`).

```nsm solve place
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x4

board Plugged {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .j: Header1x4
}
```

```nsm-physical
import std::pcb::Outline

physical for Plugged {
    case {
        .outline: Outline { .width &= 40mm; .height &= 24mm }
    }

    // Case drawing, slot C: the header's body sticks 0.5 mm out of the left edge.
    .j.place.left &= -0.5mm +/- 0.1mm
}
```

For a part that may hang over an edge, the placer and the check do this, and nothing else:

- **The room it takes** is its courtyard clipped to the outline, with the margin (0.4 mm for JLCPCB's process) kept on the sides that are on the board and none on the side that hangs over. A connector wider than the board no longer stops the placer before it starts, and a locked part that states its overhang counts as the part that is inside.
- **Overlap** with the courtyards of other parts, keepouts and cutouts is measured on the clipped courtyard, so two parts hanging over the same edge do not conflict beyond it, in the placer's cost as in the check.
- **Copper is not waived.** Every pad is still inside the outline and the process's copper to edge space from it, so the placer keeps them there and a pad outside is an error with its distance.
- **The body's centre may be off the board** in the direction an edge field states. A part whose `.bottom` window starts 25 mm past a 60 mm board may be required with its 40 mm body, and so its centre, 5 mm below it, as long as its pads stay inside. A window on `.x` or `.y` keeps the old rule: a centre that is outside the outline is refused with the numbers, and so is a centre outside by an edge the part's edge fields do not state.
- **A direction nobody stated is an error**, and the finding says how far past which edge, which edges are stated, and the line that would allow it.
- **Not checked:** the body (`F.Fab`) may be anywhere, the part of the courtyard outside the outline is not measured, and neither is how far the part hangs beyond what its window says. Silkscreen and text outside the board still get the design rules check's silk to edge clearance warning, as in KiCad.

`fab` prints each overhang as a warning as it places and lists it among the warnings in `fab-notes.txt`: ``warning: `j` (J1) hangs over the left edge: its courtyard is 0.50 mm past it (`j.place.left (main.physical.nsm:10)` allows -0.6 to -0.4 mm)``. The variants below run the real checks. The first states the position by the body's centre, which is not an overhang.

```nsm solve place refused
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x4

board Plugged {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .j: Header1x4
}
```

```nsm-physical
import std::pcb::Outline

physical for Plugged {
    case {
        .outline: Outline { .width &= 40mm; .height &= 24mm }
    }
    .j.place.x &= 1.3mm
}
```

```text
error: Plugged: `j` (J1) is not wholly inside the board outline: its courtyard is at least 0.50 mm past the left edge wherever its requirements put it, and the design does not say it may hang over an edge; to allow it past the left edge, require the body to reach it, for example `.j.place.left <= 0mm`; the lines that place it: main.physical.nsm:7 `.j.place.x &= 1.3mm`
```

The next states an overhang that is too much: the header's pads end up 0.05 mm from the board edge, nearer than the process allows.

```nsm solve place refused
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x4

board Plugged {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .j: Header1x4
}
```

```nsm-physical
import std::pcb::Outline

physical for Plugged {
    case {
        .outline: Outline { .width &= 40mm; .height &= 24mm }
    }
    .j.place.left &= -1mm +/- 0.1mm
}
```

```text
error: Plugged: a pad of `j` (J1) is 0.05 mm from the board edge
```

The last states the left edge, and its centre along it puts it past the top edge too:

```nsm solve place refused
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x4

board Plugged {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .j: Header1x4
}
```

```nsm-physical
import std::pcb::Outline

physical for Plugged {
    case {
        .outline: Outline { .width &= 40mm; .height &= 24mm }
    }
    .j.place.left &= -0.5mm +/- 0.1mm
    .j.place.y &= 5.1mm
}
```

```text
error: Plugged: `j` (J1) is not wholly inside the board outline: its courtyard is 0.52 mm past the top edge, and the design says it may hang over only the left edge (`j.place.left (main.physical.nsm:7)` allows -0.6 to -0.4 mm); to allow it past the top edge, require the body to reach it, for example `.j.place.top <= 0mm`
```

`netsmith board fab` measures every requirement on the board it made and prints a finding for each one not met, with the measured value, the window allowed and how far over it is. A violated requirement is an error that stops `fab`, like a design rules error; it is not a guideline, so `guidelines` and `--strict` do not govern it. `constraints = "warn"` under `[project]` in `netsmith.toml`, or `--constraints warn` for one run, lists them as warnings instead (`"error"` is the default). `board fab --constraints error|warn` wins over the manifest, so `--constraints error` stops `fab` even when the manifest says `warn`. `--allow-drc-violations` waives design rules, not requirements: with it, an unmet requirement still stops `fab` ("does not waive them"). The notes of the package count the requirements met, and say how many were not and that they are only warnings. Output of the check for a header moved by hand (this block is not run by the guide test):

```text
error [constraint] constraint power.place.left (cased.physical.nsm:13) is violated: the left edge of the body of J1 (F.Fab) is 28.73 mm from the left edge of the outline, and it allows 1.4 to 1.6 mm (27.13 mm over)
```

`board fab --format json` gives each requirement that is not met as data, in `requirements` of the finding it belongs to (the one error finding of a run that stopped, or one warning finding for each requirement with `constraints = "warn"`), whether the placer ended with it unmet or the check of the board file found it: `id` (`power.place.left`), `source` (the line, `cased.physical.nsm:13`), `parts` (the designators), `measure` (`x`, `y`, `left`, `right`, `top`, `bottom`, `rotation`, `side`, or `relation_x` and `relation_y` for two parts, the second term first), `unit` (`mm` or `degrees`), `measured`, the window as `min` and `max` (an end the requirement leaves open is left out) and `over`, how far out the value is. A requirement on the face has `side`, the face the part is on, and `allowed_side`, in place of the figures. For the header above:

```text
{
  "id": "power.place.left",
  "source": "cased.physical.nsm:13",
  "parts": ["J1"],
  "measure": "left",
  "unit": "mm",
  "measured": 28.73,
  "min": 1.4,
  "max": 1.6,
  "over": 27.13
}
```

The same finding gives the lines that cannot all hold as data, in `conflicts`, and the faults the placement check found where the placer ended, in `placement` (issue #779), so that a program that moves parts to clear them reads the parts and boxes and not the sentences. A conflict has `kind` (`overlap`, two courtyards over each other wherever the lines put them; `keepout` or `cutout`, a courtyard over one; `over_pins`, over the pins of a through-hole part; `relation`, a relation the other lines leave no way to meet; `relations`, two relations with no value in common; `outside`, no place inside the outline; `past_edge`, the lines put the courtyard past an edge nobody stated it may hang over, with `past`, each `edge` and `by`; `too_big`, the courtyard is bigger than the outline in every turn, with `turns`, each `angle`, `width` and `height`), `lines` (each `requirement`, with `id`, `source` and `text` and the window the line allows: `measure` (`x`, `y`, `left`, `right`, `top`, `bottom`, `rotation`, `relation_x`, `relation_y` or `side`), `unit`, `min` and `max` (an end the line leaves open is left out) or, for a face, `allowed_side`, as in `requirements`; or `lock`, a layout file's lock, with the `part` and its `x` and `y`), `parts` (`refdes`, `path` and `courtyard`, the box `x_min`, `x_max`, `y_min`, `y_max` the courtyard lies within wherever the lines put it), and where it has them `overlap` (the least `width` and `height`), `area` (the keepout or cutout: `kind`, `name`, its box and the `face` it keeps parts off), `by` (how far apart, in mm) and `outline` (`width`, `height`, `margin` and `at_most`, when the proof needs the outline). A fault has `kind` (`overlap`, `over_pins`, `outside_outline`, `pad_near_edge`, `in_cutout`, `pad_near_cutout`, `in_keepout`, `pad_in_keepout`, `too_tall`, `cutout_outside_outline`, `no_position` or `hole_too_near`, the names the editor's findings have), `parts` with each `courtyard` where it stands, and its numbers: `area` and `shared` (the box two courtyards share), `past` (each `edge` and `by`), `distance` and `least`, `pads` and `near`, `cutout`, `keepout`, `height`, `limit` and `face`. The figures are in mm from the outline's top left corner, y down. What the lines prove before placing is in the same list (the faults about one part are `past_edge` and `too_big`, with the sentence in `message`), and for a lock of the layout file that breaks a requirement `requirements` and `placement` are those of the board as the lock leaves it. `dev board3d --format json` has the same two lists for the board it places. `board build --format json` gives what the lines prove before placing as findings of code `place`: the block of lines that cannot all hold, and each fault about one part, is one finding with its `conflicts` (all the sets, or the one fault), and a lock that breaks a requirement has `requirements` and `placement` too; with `constraints = "warn"` the sets of lines are warnings of the same code, with their `conflicts` (issue #998).

A layout file the placer cannot keep (a part locked where it breaks a requirement, as a drag in the editor leaves it; two locks whose courtyards overlap; locks that leave the free parts no room) is still refused by `fab`. The editor's PCB view and `dev board3d` show it instead: the parts the file puts are where it puts them, the others where the placer ended, and the refusal is said beside them (a `placement_refused` finding in the editor; `placement_refused` in `dev board3d --format json`, which then ends with 1) beside the overlaps and the requirements measured on the board. A refusal the layout file has no part in (the outline too small for the parts, lines that cannot all hold) has no board to show.

The editor lists the same findings with the placement's, a relation between two parts among them (_A relation between two parts_). Its PCB view draws each part's window over the board (ADR 0045, C4; the _Requirements_ switch of its Layers menu hides them): the box the requirements let the body stand in, as the part is turned now, across the whole outline along an axis no requirement bounds, and the requirements of a module the part is in, the half that keeps it inside. A part whose body is outside its window has a ghost, its body drawn dashed at the nearest place the window allows. The hover and the part's inspector list each requirement with its window, and a drag stops at the window's edge, rounded inward to the layout file's hundredth of a millimetre, with a note that names the requirement and its line (`limited by power.place.left (main.physical.nsm:13)`); a part already outside its window may be dragged toward it and into it, never further out. The arrow keys, a turn and a flip are not held, and the finding names what they leave. The line between the two parts of a relation is not drawn. Height limits are in _Height limits_ above.

**Require here** (ADR 0045, C5). The part inspector's _Require here_ writes where the selected parts are as requirements in the physical file, one plain `&=` of a literal for each field, at the end of the unit, so the placer keeps them there and `fab` checks it:

```nsm-fragment
.aux.place.x &= 2.2mm
.aux.place.y &= 19.77mm
.aux.place.rotation &= 0
.aux.place.side &= "top"
```

`x` and `y` are the centre of the body to a tenth of a micrometre, so the requirement holds where the layout file has the part. The layout file is not changed. A part required so cannot be dragged; _Drop requirement_ takes the four lines out, and undo and redo cover both. A part whose place the file states in any other shape (a window, an edge, a field stated twice, a relation or a `row`), or a module's own source states, offers neither: the button says the line to change, as the requirement came from a drawing and is changed in the file. A change rewrites only the numbers of those lines where they stand, so comments and the other lines of the file stay.

### The layout file

Where parts, keepouts and cutouts are, when the physical file does not say, is in `<stem>.pcb.toml` beside the source file: `board.nsm` has `board.pcb.toml`. The file is optional and never needed to build. An editor or a placer writes it, and `fab` and the editor read it. It holds the solution: the physical file holds the requirements on positions (see _Where a part goes_), never a solved position, and the layout file never a requirement.

```toml
version = 1

# One table for each board of the file, keyed by the board's name.
[OutlineAndPours]
seed = 3              # another arrangement of the parts the file does not place; absent is 0

[OutlineAndPours.parts."stage.output_cap"]
at = [31.5, 22.0]     # mm from the outline's top left corner, y down
rotation = 90         # degrees, counterclockwise on screen
side = "top"          # "top" or "bottom"
lock = true           # the placer leaves it where it is
source = "hand"       # "hand", "auto", "regen" or "import": who wrote it

[OutlineAndPours.keepouts.antenna]   # a Keepout or Cutout that states no `.x`, `.y` in the physical file
at = [18.0, 2.0]
```

**A board of a system** (issue #1327) reads the layout file beside the file it is defined in, as it does when it is built alone. A system `Kit` in `src/kit.nsm` whose members are `board PowerBoard` of `src/power.nsm` and `board LampBoard` of `src/lamp.nsm` places `Kit/power` with the table `[PowerBoard]` of `src/power.pcb.toml` and `Kit/lamp` with `[LampBoard]` of `src/lamp.pcb.toml`, and the routes file beside each (_The routes file_); `src/kit.pcb.toml` holds only the tables of the boards defined in `src/kit.nsm`. So a board placed by hand once is placed the same in every system it is a member of, and there is nothing to copy. `layout set`, `capture`, `regenerate` and `import`, the seed `fab` writes and the editor's PCB view write to that file too: `netsmith layout set --file src/kit.nsm --board Kit/lamp r1 --at 4,4` writes `src/lamp.pcb.toml`. A table for a member in the system's layout file (or in the routes file beside it), as a project laid out before this has it, is still read for that member and wins over the member's own file, which is then not read; when the member's own file exists, `fab`, the `layout` commands and the editor say so in a note (`` note: src/kit.pcb.toml: the table `PowerBoard` is read for `Kit/power`, and the layout file beside the source of the board, src/power.pcb.toml, is not: ... ``). Move the table to the member's file and remove it from the system's to end the note. A board of a dependency (chapter _Projects and netsmith.toml_) keeps its table in the system's file: the dependency's files are not the project's to write. A member defined in the system's own file reads the system's file, as before.

Parts are keyed by their path in the design, which a renumbered reference designator does not change. A member of a group is keyed by its path with the index, quoted as TOML needs: `[Board.parts."decoupling[2]"]`. A part with no entry is placed by the placer (ADR 0033): it derives a placement from the circuit alone, keeping the parts of a module together, a decoupling capacitor next to the power pins it bridges, connectors at the edges and wire short, and places them round the parts the file places. The same circuit, file and `seed` always give the same board; another `seed` gives another arrangement. When the file's seed (0 if it states none) does not place the board, the placer tries the seeds after it (the file's seed plus 1, plus 2 and so on), up to `place_seeds` in `netsmith.toml` in all (8 by default): a seed places the board when the parts fit the outline, the placement check finds no fault that another seed could move (one that names no part the file places, and names a part no requirement pins) and, with no layout file, every requirement of the physical file is met. `fab` says which seed placed the board (`placed on a 101.2 by 24 mm board with seed 15`); when it is not the file's, it says what blocked each seed tried, calls the file's seed stale if the file states one, and writes the new seed to the file. With `--locked` it writes nothing and stops with an error, as for a pick lock; an order (`project order`) and the editor's release are releases, which never change a choice by themselves, so a seed that would change stops them with the seed to write by running `fab`. `dev board3d` and the editor's PCB view use the seed they find and write nothing. When no seed places the board, `fab` stops with the closest try (the fewest faults, then the least area and distance; the earlier seed on a tie) and what blocks it, each member by its path with the designator beside it, and by how much: `no seed places the board: the placer tried 8 seeds, 3 to 10 (...), and the closest is seed 7, where this blocks it:` then `` the courtyards of `usb` (J2) and `vbus_2u2` (C37) overlap by 0.04 mm2 ``. `fab` and `dev board3d` say on stderr how the placing goes, one line at a time (issue #764): each seed as it begins with the closest so far (`Board: placing: seed 2 (3 of 8); the closest so far is seed 0, with 7 faults [41 s]`), before a run in a fixed outline what the courtyards need on each face against what a face holds inside the margin, and after each run of the placer what is still out, the worst by name (`run 1 in 35.0 x 60.0 mm: 11 overlaps and 2 parts outside; the worst: R3 0.80 mm outside, ...`), with the counts of a long run (`moves tried`, `parts legalised`) at most every 5 s. `--place-budget` (or `place_budget` in `netsmith.toml`) bounds the search: when it is spent the placer ends the run it is in, begins no other seed, and the run stops with the closest try as when no seed places the board, beginning `placed nothing in 120 s`; `--place-seeds` sets how many seeds for one run. `fab` then says how the routing goes, at most once in two seconds and after each try of the router, on the same lines: `Board: routing: try 1 of 3: 12 of 40 connections done, 0 not routed, now net GND: U1.3 to C4.1; 31 tracks and 4 vias so far [6 s]`, and `Board: routing: try 1 of 3: ended with 38 of 40 connections routed, 2 left: ...`; `--route-timeout` bounds it and keeps what it routed. With `--progress json` (on `fab` and `dev board3d`) each of these lines is a JSON object on stderr, `{"board", "stage", "unit", "done", "total", "detail", "elapsedMs", "message"}` (`stage` is `placing` or `routing`; `total` and `detail` only when there are some), and every other line of progress is `{"message"}`, for a program to read; with `--to-editor` (`--to-editor=9000` for another port than 8787) the same lines go to a running editor, whose status bar shows the run (`fab (terminal): ...`) and how it ended. The editor's PCB view shows its own placing as it goes: the parts where the placer has them, each the box round its courtyard, with the placer's last line and Cancel, which stops the placing and keeps nothing; while it routes, the tracks laid so far are drawn over the board. A build puts every part that has an entry where the file says; the placer places the others round the parts that are locked (`lock = true`) and does not react to the others, so moving a part by hand moves nothing else. `place_subset` places some parts and leaves the rest, but no command asks for it yet. An entry for a part, keepout or cutout that the board does not have, or a table for a board the file does not have, is an error that names it and lists what exists. A keepout or cutout that states `.x` and `.y` in the physical file and has an entry as well is an error: a thing has one place. A keepout entry gives only `at`: a keepout of its own, with its size and what it forbids, is a requirement and is stated in the physical file. An entry with a `size` (a keepout an earlier editor drew here whole) is an error whose message gives the member to write in the physical file and says to remove the entry: `` main.pcb.toml: the keepout `old` is stated whole here (it gives `size`): ... Remove the entry and write `.old: Keepout { .x &= 2mm; .y &= 2mm; .width &= 10mm; .height &= 6mm; .copper &= "allow" }` in `physical for Board { }` of `main.physical.nsm` ``; an entry that gives `layers`, `tracks`, `vias`, `copper` or `parts` with no `size` is an error that names the field. `side = "bottom"` puts a part on the back. `examples/outline_and_pours.nsm`, its physical file `examples/outline_and_pours.physical.nsm` and `examples/outline_and_pours.pcb.toml` use all of this.

**A waiver of a finding of a part's own footprint** (issue #1196). Some findings of the design rules check come from a part's footprint and not from the board: a maker's land pattern whose pads are 0.1448 mm apart where the process asks 0.15 mm (`pad_to_pad`), a plated ring drawn thinner than the process leaves. `board drc` and `fab` end such a finding `(within U1's own footprint)`: every item it names is the one part's (two or more, or one under a rule that judges an item alone: `annular_width`, `plated_ring`, `drill_out_of_range`, `npth_drill`, `slot_width`, `malformed_courtyard`, `text_height`, `text_thickness`). The board cannot change it, so the layout file may waive it, in the table that waives a guideline (chapter _Parts_, _What the datasheet says about layout_) and keyed the same way, by the part's path and the rule's name, with a reason, the source that says the land pattern is right for the part, and the pads the finding is between or its place:

```toml
[Board.waive."reg.ic.pad_to_pad"]
reason = "the maker's land pattern: pads 2 and 3 are 0.1448 mm apart, under the 0.15 mm the process asks"
source = "the maker's datasheet, Recommended land pattern, p.12"
pads = ["2", "3"]         # the findings among these pads of the part; or
# at = [12.40, 8.15]      # the finding at this place, mm from the outline's corner (within 0.1 mm)
```

A key whose name is not a guideline of the part and is a rule of the check is such a waiver. It covers each finding of that rule that is the part's own and whose items are all among `pads` (and at `at`, when it gives both); a finding with a track, a via, a zone, the edge or another part in it is the board's and is never waived. A covered finding is no error and no warning: `board drc` passes and `fab` packages, `board drc` lists it as `waived [pad_to_pad] ..., by `reg.ic.pad_to_pad`: the reason (source: ...)` and says how many on its last line, `fab-notes.txt` lists every one under `design rules waived:`, for a reviewer to check against its source, and `--format json` gives it with the `severity` `waived` and its `waiver`, `reason` and `source`. A waiver that covers no finding is itself an error, `unmatched_waiver` (the part, its footprint or the process changed, and the waiver would hide a finding it was not written for). A waiver with no reason or no source, with neither `pads` nor `at`, naming a pad the part does not have, or of `constraint` or `unmatched_waiver`, is an error of the layout file; a guideline's waiver gives no `pads` and no `at`. `--allow-drc-violations` waives every error of the check at once; a waiver waives the one finding, so a board with an accepted footprint finding is still checked clean against the rest. `board drc --kicad-pcb` reads no layout file and waives nothing.

**The frame, the turn and the faces.** Every number in the file is in millimetres, in the frame of the board: the origin is the outline's top left corner, x grows to the right and y downward. `at` is where the part's footprint origin is, not its body or its centre (the body is what a requirement measures). `rotation` is in degrees, counterclockwise as the board is seen on screen from above, any angle from 0 up to 360 (not 360 itself); the placer turns a part by 0, 90, 180 or 270 (or by a step of 45 degrees, a part whose `.place.turn_step` is 45), and another turn is one a person or a requirement gives (_A turn off the right angles_). A part on the `top` face is the footprint as it is drawn. A part on the `bottom` face is the footprint seen in a mirror (left to right, about its own y axis) and then turned by `rotation`, as it is seen from above: `rotation` turns the mirrored part the same way for both faces, and `at` is still measured from the top left corner of the outline seen from above. One part on each face:

```toml
version = 1

[Widget.parts.r1]            # on the top face, turned a quarter counterclockwise
at = [10.0, 5.0]
rotation = 90
side = "top"

[Widget.parts.power_in]      # on the bottom face, mirrored, not turned
at = [30.0, 12.0]
rotation = 0
side = "bottom"
lock = true
```

**The command line.** A script or an agent changes the file with `netsmith layout`, which reads and writes it as the editor does (the other tables are kept, and so are the comments: a comment stays above its entry or after its line, and goes with the entry when the entry goes; the entries are written in name order) and places nothing:

```sh
netsmith layout show                  # every part and its entry, with the line of the entry
netsmith layout show stage --format json   # the parts of a module, as data
netsmith layout set stage.output_cap --at 31.5,22 --turn 90 --side top --lock
netsmith layout check                 # which entries name no part, which parts the placement check refuses, ...
```

`show` says, for each part, `at`, `turn` (the file's `rotation`), `side`, `lock` and `source` as the placer reads them, and the line of its entry; a part with no entry is said to be placed by the placer. `set PATH` writes one entry: a part with no entry needs `--at X,Y`, for one that has an entry a flag left out keeps what it has (`--unlock` clears a lock), a new entry has `source = "hand"` and an entry that exists keeps its source. It refuses a path the board has no part at, listing the nearest names, and a turn that is not from 0 up to 360, and leaves the file as it was. `check` says one line for each finding with the line of the entry (`main.pcb.toml:13: error: ...`): an entry that names no part, an entry with a turn that is not allowed or a position that is not a number, a locked entry that breaks a requirement of the physical file, and each fault of the placement's own check among the parts the file places (`placement`, issue #1224): the check `fab` runs on the placed board, run on those parts where their entries put them (two courtyards that overlap, each shape of a courtyard of several counted as its own; a part past the edge, in a cutout or a keepout, under a height limit it is too tall for; a hole too near another part), in `fab`'s words and with `fab`'s data (`fault` in the JSON, as `board fab --format json` has it). `fab` puts every part with an entry where the entry says whatever the seed, so a fault among them stops it, and `check` finds it in milliseconds: the footprints are loaded and nothing is placed. So that a group placed by hand (the radio parts, say) is known to be consistent before anything is placed, `fab` runs the same check as soon as the requirements are proved and before the placer starts (issue 1309): a fault among the parts the file places stops it at once, all the faults as one list in `check`'s words (the ones `--for-review` waives are left to the check after placing), and the placer, which can take a minute on a tight board, never starts for a layout file that no seed can make good. An outline whose size is a range is measured at the most it allows, and one the design leaves open only at its left and top edges. A part with no entry is a note, since the placer places it, round the locked parts; a part the placer places that has no room round them is a warning, `no_room` (A part with no room, above), an error with `--strict`; the exit code is 1 when there is an error. `set` says as a warning a fault the check finds of the entry it wrote. The three take `--board NAME` and `--file FILE.NSM` as `netsmith routes` does, and `--format json` for data.

**How good a placement is, as data.** `netsmith layout summary` places the board as `dev board3d` does (the layout file and the physical file read, nothing written) and sums it up in the numbers the placer and the checks use, so that a person, an agent and the placer agree on what "close" means; `netsmith layout compare BEFORE.pcb.toml [AFTER.pcb.toml]` places it with two layout files (the second is the one beside the source when left out) and says what changed:

```sh
netsmith layout summary --format json            # one line of JSON, a few kilobytes for a hundred parts
netsmith layout summary --near stage.output_cap --within 3   # with the parts within 3 mm of one
netsmith layout summary --between stage.output_cap.1 U1.3     # the distance between two pads
netsmith layout compare old.pcb.toml                # what moved since old.pcb.toml, and which findings came or went
```

Every place is in the layout file's frame, in mm, and every number is rounded to 0.01. The summary has: `board`, `width`, `height`, `edge_wall` (what the placer keeps between a courtyard and the edge) and `parts`; `totals`, the board's `hpwl` (the sum of each net's half perimeter wire length), `ratsnest` (its length), `crossings` (pairs of ratsnest lines of different nets that cross) and `decoupling_mean` and `decoupling_max`; `faces`, the top then the bottom, each with its `parts`, the area a courtyard may take (`usable`: inside the outline and the edge wall, outside the cutouts and the keepouts that keep parts off that face), the area the courtyards take (`courtyards`, counted once where they overlap; a part with a hole counts on both faces), `fill` (the one over the other), `free`, and `largest_free`, the five largest empty rectangles (`min`, `max`), largest first, none over another, measured on a grid of `cell` (0.25 mm; heights and holes are not counted, so a part that fits one may still be refused by a height limit); `modules`, each module instance of two parts or more (each leading part of a path: `reg` and `reg.divider`), with its `parts`, its `box` round their courtyards, `box_area`, `courtyards`, `spread` (the half perimeter of the box round their courtyard centres, the placer's cohesion term) and `intruders` (parts of other modules whose courtyard centre is inside that box, as the placer counts them); `decoupling`, each decoupling capacitor (`cap`, `cap_path`) with the `part` it serves and its `distance` (the mean over its two pads of the distance to the nearest pin of the part on the pad's net); `nets`, each net of two pads or more with its `pads`, `hpwl`, `crossings` and `longest` connection (`from`, `to`, `length`); `neighbours`, with `--near` only, the parts within `--within` mm (2 by default) of each part named, courtyard to courtyard on either face, nearest first, each with its `face` and `gap`; `pads`, with `--between PART.PAD PART.PAD` only (given again for more pairs; a part by its path or designator, a dot, the pad's number, so `stage.c1.1` and `C1.1`), the distance between each pair of pads: `from` and `to` as `C1.1`, the `from_net` and `to_net` each is on (left out for a pad on none), `distance` from the centre of the first to the centre of the second, `dx` and `dy` (the way from the first to the second, x to the right and y down), and `gap`, the space between the two pads' boxes (0 where they touch; the face is not looked at), where a footprint has several pads with the number the nearest pair is measured, and a part the board has none of or a pad the part has none of is refused (exit 1) with the nearest names or the part's pad numbers (`--near` measures courtyard to courtyard, `--between` pad to pad); and `findings`, what the placement check finds, in `fab`'s words. The difference has `board`, `moved` (each part that moved, turned or changed face: `path`, `refdes`, `from` and `to`, each `x`, `y`, `turn` and `side`, and the `distance` its origin moved), `appeared` and `gone` (findings), and the `totals` of each placement as `before` and `after`. Both take `--place-seeds` and `--place-budget` as `dev board3d` does, and without `--format json` print a few lines for a person. The editor's PCB view has the same summary as its Summary tab (totals, the near-the-selection list, the distance between two pads typed in, the faces with their largest free rectangles, the modules, the decoupling capacitors and the longest nets; a click on a rectangle, a module or a capacitor zooms to it), and `GET /api/pcb/summary?board=` gives it as JSON with camelCase names (`largestFree`, `edgeWall`), `near=` and `within=` for the neighbours and `between=PART.PAD,PART.PAD` for the pads, each repeatable (`near` and `between`).

`source` says who wrote an entry and changes nothing, with one exception: two parts whose entries `netsmith routes import` wrote (`source = "import"`, see The routes file) may share courtyard area. They stand where a KiCad board has them, and that board may have courtyards that overlap (ORBIT's LQFP-48 covers two of its capacitors), or footprints that draw no courtyard, for which netsmith takes the pads and 0.25 mm; the check before placing does not count the pair as a conflict, and `fab` says each such overlap as a warning (``the courtyards of `mcu` (U4) and `v3v3_100n[2]` (C16) overlap by 1.65 mm2: both stand where the imported KiCad board has them ...``) instead of stopping. Their pads are still checked by the design rules check, as any copper is.

**Regenerating one module.** `netsmith layout regenerate PATH` places the parts of one module again with every other part held where it is (issue #825): the parts whose path is `PATH` or starts with `PATH.` or `PATH[`, inside the box round them grown by `--margin` mm (3 by default) and cut to the outline. A part of the module that the layout file locks, or that a requirement of the physical file pins, stays too. The same board, module, `--seed` and margin always give the same arrangement, another seed another one. It prints a proposal and writes nothing: each part placed again with its place before and after, the nets with a pad on one of them (their copper is routed again), the records of the routes file that no longer match their pads, and the findings and totals before and after, as `layout compare` gives them. With `--write` each part placed again gets an entry, unlocked, with `source = "regen"`, and the rest of the file is kept:

```sh
netsmith layout regenerate stage                  # a proposal: what moves and what it changes
netsmith layout regenerate stage --seed 3 --write # keep the arrangement of seed 3
```

**Where the router's copper is kept.** The router's copper is a cache, never committed (ADR 0138, issue #1019): `.netsmith/routes/<stem>.<board>.toml` in the project's folder, which ignores itself with a `.gitignore` inside `.netsmith`. The editor writes it after every route that ran to its end and starts a route from it when it has routed nothing yet (a new session), so after a regeneration only the nets the module touches are routed again, and the copper of the rest is kept as it was. A net is routed again when its pads are not where they were, a part that moved now covers its copper, or the routes file locks it; a file of another netsmith version, other routes-file copper (tracks, vias, locks, zones) or other design rules is not used at all, and the editor says why. A proposal of `layout regenerate` says what of the kept copper a route after it keeps (`copper` in the JSON: `file`, `kept`, `again`, and `unused` with the reason when the file is not used). `fab` routes every net and keeps nothing unless it is given `--reuse-routes`, so a clean checkout gives the same board on every machine; with the flag it starts from the kept copper, says how many nets it kept and routed again, and keeps what it routed. An order does not take the flag.

```sh
netsmith board fab --reuse-routes   # keep the copper of the nets a change left alone
```

**Capturing the placement.** `netsmith layout capture` writes where the board's parts are now as the layout file (issue #828), so that a result can be frozen. It places the board as `fab` places it (the layout file and the physical file read; the seeds after the file's tried as for `fab`) and writes an entry for every part, the seed that placed the board as the file's `seed`, and every other table as it was, comments too. A part that already has an entry keeps it (its place, `lock` and `source`: a build puts a part with an entry where the entry says); a part the placer placed gets an entry with `source = "auto"`, unlocked. A build of the file then has every part where it was captured, whatever the placer, a catalog or the circuit around it does later, until `layout set` or `layout regenerate` changes one. The places are rounded to 0.01 mm, as every entry is, so a part the placer placed is up to 0.005 mm from where it was. A keepout or cutout the physical file places is not written (it has no entry); one the file places keeps its entry. A board that no seed places is refused with the placer's report (as `fab` gives it) and the file is not written; a capture holds a board that places. `--dry-run` writes nothing and says what would be written, with `--format json` as one document (`board`, `seed`, `seed_was`, `parts`, `added`, `kept`, `shapes`, `seed_note`, `written`); `--place-seeds` and `--place-budget` are as for `fab`. A record of the routes file whose pads moved by the rounding is kept, open where its copper misses a pad, as when any pad moves.

```sh
netsmith layout capture            # an entry for every part, and the seed, in main.pcb.toml
netsmith layout capture --dry-run  # what it would write
```

**Where one part fits.** `netsmith layout fit PATH` says where one part fits with every other part where it is (issue #780): the board is placed as `dev board3d` places it, and the part (a path or a designator) is tried on a grid of origins every `--step` mm (0.5 by default) from the outline's top left corner, for each of the four right-angle turns from its own and on each face (one face when a requirement of the physical file states it, or `--side` names it; a turn a requirement rules out is not tried). Each place is measured as `fab` and `layout check` measure it, by the same function (issue #1224): the placement check of the part against every other part where it is (courtyards, each shape of a courtyard of several counted as its own, holes, keepouts, height limits, the edge, cutouts) and the requirements about it (its windows, its modules' and the relations it is an end of). For each turn and face it gives the place nearest `--near` where nothing is found (a part, by the centre of its courtyard, or a point `X,Y`; with none, where the part is), or, when there is none, the closest try with what is in the way there. It writes nothing; the text gives the `layout set ... --lock` that writes each place, locked: the place is free with every other part where it is, and `fab`'s placer places the parts with no entry round a locked part, where an unlocked entry is put where it says after placing and may find one of them there. The exit code is 1 when no turn or face has a place. `--format json` is one document: `board`, `part`, `near`, `step`, `now` (the part where it is) and `poses`, each with `turn`, `side`, `at` (the footprint's origin, as `at` in the layout file), `distance`, `fits` and, where it does not fit, `placement` (as `board fab --format json` has it), `requirements` and `messages`.

```sh
netsmith layout fit stage.r1 --near U1           # the nearest place of each pose, beside U1
netsmith layout fit R7 --near 12,30 --side top   # near a point, on the top face only
netsmith layout fit R7 --as-laid-out             # no placement: the board as the layout file has it
```

Placing the board first is what takes the time on a nearly full board, and the question is about one part. With `--as-laid-out` (issue #1002) nothing is placed: the board is made exactly as the layout file has it, each part where its entry puts it (or where a requirement of the physical file pins it) and the outline the physical file states, in milliseconds, and the part is tried against those places. It is refused, naming what is missing, when a part has no entry in the layout file and no requirement pins it (`layout capture` gives every part an entry), or when the outline's size is not stated exactly (a width or height left open or given as a range is known only by placing); `--place-seeds` and `--place-budget` cannot go with it. The answer is the one the placed board gives when every part has an entry, since the placer puts the entries last. A fault among the other parts, where they stand (two courtyards that overlap, a part past the edge), is no refusal and does not change the answer (issue 1330): the part is judged against the others, its own result is `now` and the poses, and each fault of the others is a note on stderr (`note: not about R1, and its result stands: the courtyards of `r2` (R2) and `r3` (R3) overlap ...`) and an entry of `others` in `--format json`, as the entries of `placement` are. So two parts that overlap, or that were swapped with `layout set`, are tried one at a time with `layout fit`, as `--as-laid-out` or placed, whatever else overlaps. Without it the placing says how far it has got on stderr, as `fab` does (issue #1282: `--progress json` prints each line as a JSON object, `--to-editor` sends them to a running editor, and with `--format json` the progress is printed only with `--progress json`), and when the layout file has a place for every part (and the outline's size is stated exactly) a note says first that `--as-laid-out` answers at once; with a part that has no place the placer is the only way to know, and nothing is said. The editor asks the same question of the board its PCB view shows, with no placement of its own once the view has drawn the board: `GET /api/pcb/fit?board=&part=` (`near`, `step` and `side` as above) answers `layout fit --format json`'s poses, each with `turn`, `side`, `at`, `distance`, `fits`, the kinds and parts of the placement faults, the requirements it does not meet and their sentences (`PcbFit`); nothing is written, and a place is written as a drag writes one.

**Each part's geometry, as data.** `netsmith layout parts [PATH...]` gives what a script needs to check a placement, with no editor and no `.kicad_pcb` to read (issue #778): where each part is, its courtyard, and its pads with their nets and sizes. It places the board as `layout export` does (a layout file the placer refuses gives the parts where the placer ended), so it answers for exactly the layouts that are being fixed: the faults of the placement are in the same document, `placement`, `conflicts` and `placement_refused`, as `dev board3d --format json` has them, and the exit code is 0 whatever they are (a board `--as-laid-out` has `no_entry` too, empty when every part has a place). A PATH is a part's path or designator, or a module's (every part inside it), and may be given again; with none, every part. A name the board has no part by is refused with the nearest names. `--as-laid-out` places nothing: the board is made as the layout file has it (as for `layout fit`), and the parts with an entry (or that a requirement pins) are given where it puts them. A part with no entry and no pinning requirement is not on that board, so it is no refusal (issue 1308): it is listed as having none, as `no_entry` in the document (each with its `refdes` and `path`) and a line of text, also when a PATH names it or a module it is in, and the exit code stays 0, as for a placed board. Only an outline whose size is not stated exactly is refused. `--format json` is one line:

```sh
netsmith layout parts --format json                  # every part
netsmith layout parts stage.output_cap U1 --as-laid-out   # two parts, no placement
```

The document has `board`, `width` and `height` of the outline, and for each part: `refdes`, `path`, `value`, `footprint`, its `frame` (`at`, the footprint's origin, as the layout file's `at`; `turn`; `side`; `mirrored`; and `width` and `height`, the size of the box round its courtyard on the board), `courtyard` (that box: `min` and `max`), `courtyards` (each courtyard shape on the board, _Courtyard shape_), `local_courtyards` (the same shapes in the footprint's own frame), `body`, `height` (when the design states it), and `pads`. A pad has its `number`, its `net` (`null` for none), `kind` (`smd`, `thru_hole`, `np_thru_hole`), `shape` (`rect`, `round_rect`, `oval`, `circle`), the centre of its copper `at` on the board and `local` in the footprint's frame, its own size `width` and `height` along its own axes, its `turn` on the board, `bounds` (the box its copper spans on the board, which is the size to measure a gap with), the faces it has `copper` on and its `drill` when it has a hole. Every place is in the layout file's frame, in mm, rounded to 0.001; a turn that is not a right angle gives `courtyards` and `bounds` as the box round the shape as turned.

The part's frame is the layout file's, and a script computes where a part's pads go for a place it has not tried from each pad's `local` and one rule: the footprint's point `(x, y)` is mirrored left to right first on the bottom face, `(-x, y)`, then turned by `turn` degrees counterclockwise on screen (with y down, `x' = x cos t + y sin t` and `y' = -x sin t + y cos t`), then moved by `at`. A worked example: a two-pad resistor whose pads are at `local` `(-0.5, 0)` and `(0.5, 0)`, entered at `at = [20, 12]` with `rotation = 90` on the top face, has its pads at `(20, 12.5)` and `(20, 11.5)`: the turn takes the footprint's x axis to the board's negative y (up the screen), so the pad at `-0.5` goes down and the pad at `0.5` goes up. With the same entry on the bottom face the footprint is mirrored first, to `(0.5, 0)` and `(-0.5, 0)`, and then turned: the pad at `local` `-0.5` is at `(20, 11.5)` and the pad at `0.5` at `(20, 12.5)`. The pad's `turn` is `rotation + angle` on the top face and `rotation - angle` on the bottom, so a pad is turned with its part and not mirrored with it.

To place a few free parts round locked ones, give the free parts no entry (or an unlocked one) and lock the rest: the placer places only the parts with no lock and holds every locked part where it is, and `layout regenerate` places one module (or one part, by its path) again with every other part held. A board that the placer leaves with a fault of where its parts are is not placed: `fab` refuses it and `dev board3d` ends with 1, with the faults in `placement`.

**Moving parts in KiCad.** `netsmith layout export --kicad-pcb FILE.kicad_pcb` writes the board as `dev board3d` and the editor place it as a KiCad board, with its `.kicad_pro` beside it (issue #781). It is written whatever the placement check finds: the findings are the reason to look, and KiCad is a place to drag a part across a conflict. A layout file the placer refuses gives the board where the placer ended, with the file's places. The board has no copper (no track or via; the planes and pours are zones that are not filled) and says it is not for fabrication, in its title block and in a text on `Cmts.User` above the outline; `fab` writes the board to order. The findings are printed and listed in `<stem>.findings.txt` beside the board. The editor's PCB view has the same files as a button, "Download KiCad board" (issue #1001): the board as the view shows it placed, not routed, with its findings, also while the placement has findings, as a zip with the `.kicad_pcb`, the `.kicad_pro` and the findings list; bring the moved parts back with `layout import` on the unzipped board.

`netsmith layout import FILE.kicad_pcb` brings back the parts moved in KiCad. A footprint is the part whose path its hidden `netsmith_path` property names (every `.kicad_pcb` netsmith writes has it), so renumbering the designators in KiCad changes nothing; a footprint without one is said and left out. Each part KiCad has elsewhere than the board places it now (by more than 0.005 mm, or another turn or face) gets an entry, as `layout set` writes one: an entry that exists keeps its `lock` and `source`, a new one is `source = "hand"`, unlocked unless `--lock`. When a part moved, every part the placer placed gets an entry where the KiCad board has it too, unlocked with `source = "auto"` as `layout capture` writes one: else the placer would arrange those parts again round the moved ones, and the board would not be the one left in KiCad. Places are from the outline's top left corner, so move the parts in KiCad, not the outline. Copper is not read: `routes import` brings back tracks and vias. `--dry-run` writes nothing; `--format json` prints `moved` (each part's `from` and `to`), `unchanged`, `held`, `missing`, `notes` and `written`.

```sh
netsmith layout export --kicad-pcb kicad/main.kicad_pcb  # the placed board, findings and all, for KiCad
netsmith layout import kicad/main.kicad_pcb        # the parts moved there, as layout file entries
```

The editor's PCB view does the same: a part of a module picked offers "Regenerate" in its inspector, and the side panel's Modules tab lists the board's modules as a tree, each with its own "Regenerate" (a click on a row picks the parts of that module and lights them, in the 2D and 3D views, in the module's colour; the pointer on a row lights them while it is there, and Ctrl or Shift and a click lights another module beside it in another colour, `docs/BOARD3D.md`, "Modules lit"). The proposal is drawn over the board in a second colour with the same list, Another asks for the next seed, and Accept writes it as one edit, one step of undo. A route after an edit keeps the router's copper of every net whose pads did not move and that no part that moved now covers, and routes only the rest; when connections are left it routes once more with the nets whose copper crosses the parts that moved ripped up too. The editor keeps the last few routed boards, so an undo back to a placement it routed shows that copper again at once. `fab` routes the whole board, as before: the router's copper is not kept on disk.

### The routes file

Copper that a person, an agent or an import decided is in `<stem>.routes.toml` beside the source file and its layout file: `board.nsm` has `board.routes.toml` (issue #599; ADR 0030 sketched it). The file is optional: with none, every board is routed as before, byte for byte. Whatever reads the layout file reads it too: `fab`, the editor's PCB view, and `netsmith dev route` through the placed board `board fab --save-placed` saves. It is written by `netsmith routes` (below), never by `fab`, and committed with the source.

```toml
version = 1

# One record for each net of a board, keyed by the board's name and the net's name.
[LedExample.nets.VCC]
pads = "273e3844dab94584"   # a hash of the net's pads when the record was written
lock = true                 # the router adds no copper to this net
tracks = [
  { layer = "top", width = 0.15, from = [5.25, 2.8], to = [8.6, 2.8] },
  { layer = "top", width = 0.15, from = [8.6, 2.8], mid = [9.1, 3.0], to = [9.3, 3.5] },
  { layer = "bottom", width = 0.4, from = [2.0, 5.0], to = [6.0, 5.0], lock = true },
]
vias = [
  { at = [6.0, 5.0], diameter = 0.45, drill = 0.2 },
]

# A zone of the net: one table each, its settings one to a line.
[[LedExample.nets.VCC.zones]]
layers = ["top"]
priority = 1
clearance = 0.2
min_width = 0.2
pads = "solid"
thermal_gap = 0.5
spoke_width = 0.5
islands = "remove"
outline = [[1.0, 1.0], [11.0, 1.0], [11.0, 7.0], [1.0, 7.0]]
holes = [
  [[3.0, 3.0], [4.0, 3.0], [4.0, 4.0], [3.0, 4.0]],
]
```

Places are in mm from the outline's top left corner, y down, as in the layout file, and are written to 0.000001 mm, a KiCad file's precision. A track is straight from `from` to `to`, or an arc through `mid`; `layer` is `top`, `bottom` or an inner signal layer of the process, `inner2` (a track on an inner plane, or on a layer the board does not have, is an error); `width`, `diameter` and `drill` are in mm. Each shape is on a line of its own, so a change to one shape is a change to one line. An unknown key, another `version`, a shape that is wrong in itself (a width of zero, a drill as wide as its via, a zone outline of two corners, a zone on a layer the board does not have) or a table for a board the source does not have is an error that names the file.

A zone is KiCad's zone of the net (issue #724): copper filled inside `outline`, less each of `holes`, on each of its `layers` (`top`, `bottom`, `inner1`, `inner2`, ...), with the settings of KiCad's zone that change the copper. `priority` says which of two overlapping zones of a layer has the area: the higher one, as in KiCad (0 when not given); a zone gives up the outline of a higher zone of its net, and the fill of a higher zone of another net grown by the larger of the two clearances, and two zones of different nets with the same priority whose outlines overlap are an error of `fab`, because KiCad would choose between them by an identifier and not by the design. `clearance` is the space from other nets' copper, `min_width` KiCad's minimum thickness, `pads` how it joins its net's pads (`solid`, `thermal`, the default, with a gap of `thermal_gap` crossed by four spokes `spoke_width` wide, `none`, or `thru_hole_only`, thermal round a through-hole pad and solid to a surface pad), and `islands` which unjoined pieces KiCad removes (`remove`, the default, `keep`, or `remove_below` with `island_min_area` in mm²). Sizes are in mm.

**A zone and the design's pour or plane on its layer** (issue #1314). A zone of another net cuts the pour or plane, as a power zone cuts a ground pour in KiCad: the zone has the area it covers, at whatever `priority` it has, and the pour is filled round it, keeping its clearance from the zone's fill as from any copper of another net, and still joins its net's pads and vias where it reaches them. A 3V8 zone on a board whose process pours the bottom with GND leaves the GND pour on the rest of the bottom. The board file writes the pour at priority 0 and every zone of the routes file one above its own priority, so that KiCad's fill gives the zones the area too; `routes import` of that board takes the one off again (when the board holds the process's own pour or plane on a layer of a zone it takes, and no zone it takes is at 0), so the routes file comes back as it was. A zone of the pour's or the plane's own net restates it instead: the routes file then says where that net's copper is on the layer, and the design's pour or plane there is not drawn (the process still says the layer's net, and the fab notes say the layer has the routes file's zones instead); restate it with a zone of the whole board (`--board-outline`) to keep it all. The fab notes say a pour that zones cut (`B.Cu is a pour on GND, cut by the routes file's zones`), `board copper` marks such a layer (`cut by 1 hand zone(s) of other nets`), `board fill` lists the pour's fill beside the zone's, and the editor's PCB view draws both. The other layers keep theirs. The board file holds each zone as KiCad's, with its settings, so that KiCad's fill makes the copper of the board the zone came from. netsmith's own fill, which the Gerbers, the design rules check and the ratsnest use, keeps every clearance and makes the thermal reliefs, and then, as KiCad's does, takes out a spoke KiCad's fill would not make (issue #1115: one whose end, half of `min_width` beyond the thermal gap, is not on the fill as it is with the reliefs cleared whole and trimmed to `min_width`, so a spoke into a strip of fill thinner than that is not made), the copper narrower than `min_width`, and the copper islands `islands` removes: a piece that touches no pad, via, track or teardrop of the net on its layer, nor a piece of another zone of the net there. **The KiCad board is poured** (issue #1184). The `.kicad_pcb` that `fab` writes holds that fill in each of its zones, the planes, the outer pours and the zones of the routes file, with the rule areas round a Kelvin sense pin's copper kept clear: each piece of the fill on each layer is one `filled_polygon`, its holes joined to its outline by bridges of no width as KiCad keeps a fill, its points to 0.1 µm, and the zone says `(fill yes ...)`. So the board opens in KiCad poured, with the copper of the Gerbers (the clearances, the thermal reliefs and their spokes, the copper islands taken out), and `kicad-cli pcb drc` checks that copper as it is, with no refill, and names the same `starved_thermal` pads as netsmith's check. Nobody needs KiCad to fill it; pressing `B` in KiCad refills it with KiCad's own filler. netsmith reads the fill of a board file it wrote (`(generator "netsmith")`) as none and makes it again, so `netsmith board drc` of the file `fab` wrote, a panel of it and `--cross-check` see the board `fab` checked; a board KiCad has saved again says `pcbnew`, and its fill is then held as below. The fill keeps every space 2.5 µm wider than the rule asks, as KiCad's filler does, so that its polygons, whose curves are cut into straight pieces, keep the space in full and KiCad's check finds none short. The file grows with the fill, mostly by the clearance round each pad and via in a plane: the four-layer example `outline_and_pours` is 50 KB unfilled and 386 KB filled. `netsmith layout export` writes the placed board before it is routed, with its zones unfilled.

A zone is no obstacle to the router: its fill keeps its clearance from whatever the router draws, as KiCad's does. Its fill joins what of its net it reaches, whether the net is locked or not: each piece joins the pads of its net with copper on its layer that it covers (a thermal relief's spokes reach the pad's centre) and a pad whose copper overlaps one of those, the vias it covers, and the tracks of its layer that run into it, so a net a zone joins needs no track between those pads.

The design rules check judges each pour, plane and zone by that fill, not by its outline (issue #838), with two warnings by KiCad's names. `starved_thermal` is a pad the zone joins with a thermal relief, its centre in the zone's outline, that fewer than two of its spokes join to the fill (KiCad's least, its `min_resolved_spokes`): the others ran into another net's clearance, a keepout or the edge band and the fill took them out, so little copper carries the pad's current and heat. A spoke counts where the fill has copper halfway across the thermal gap, as KiCad's check counts it, and netsmith's fill makes the spokes KiCad's refill makes (issue #1115), so the two name the same pads. A track of the pad's net with one end in the pad (grown by half the gap) and the other on the fill counts as a spoke, and a spoke onto a piece of the fill that joins nothing but the pad counts only when nothing else joins it, as KiCad's check counts them (issue #1222), so `board drc` and KiCad's check of the fill `fab` writes into the board file name the same starved pads. A pad that no spoke and no track joins to the fill is not starved but `unjoinable_thermal`, netsmith's own warning (issue #1222; KiCad leaves such a pad to its connection check): the fill can make none of its spokes, and the warning says why, for each spoke what keeps the fill off its end (`the clearance of pad C1.1 [VCC] (0.3 mm)`, a track's or a via's, the thermal gap of another pad of the net, the edge, a keepout, a strip of fill narrower than the zone's `min_width`), or that the spoke is narrower than `min_width` and so is never made. Nothing the fill can do joins it: the pad is joined by its own via or track, which the router keeps for it, as it keeps one for a starved pad. One that a track joins and no spoke is neither, as in KiCad, and so is one that another fill of its net joins (issue #1292): a zone of the net of a higher priority over the pad, which the lower zone gives its area, so that none of the lower zone's spokes is on its fill though the pad is in copper of its net, or, of a pad with a hole, a fill of its net on another layer. `board fill` lists such a pad in the lower fill as `thermal, 0 of 4 spokes (joined by another fill of its net)`, and `board drc`, `fab`, `board fill` and the editor judge every pad by one function, so they name the same pads. A narrower `spoke_width` or a smaller `thermal_gap` never starves more pads (a spoke as wide as `min_width` is made, as KiCad makes it), but a `spoke_width` under `min_width` makes no spoke at all. `isolated_copper` is a copper island the zone's `islands` keeps (`keep`, or `remove_below` for a piece not smaller than `island_min_area`): copper of the net joined to nothing; with `remove`, the default, the fill leaves none. A piece joined only to a piece of another zone of the net that is itself joined to nothing is an island too, as KiCad finds it (issue #1222). Both are warnings (`design rules warnings` in `fab-notes.txt`, and in the editor's problems list) until a real order shows either is a limit: the board house's capabilities page names neither. The project file beside a board (`.kicad_pro`, its `rule_severities`) can make either an error, or ignore it. A zone that holds its fill in a board file KiCad saved (a `filled_polygon`, as a board ported from KiCad does; netsmith's own fill in the file `fab` writes is made again instead) is judged by that fill, since that is the copper the board house makes: there every island is `isolated_copper`, whatever `islands` says, and a spoke counts only where the fill has its copper across the thermal gap. KiCad's cross-check does not compare these two, because each check judges its own fill. A third warning, `copper_sliver` (issue #993), is copper of a fill narrower than the process's least track (`.process.min_track`, the project file's `min_track_width`: 0.1 mm on JLCPCB's two-layer board of one ounce) that runs further than that width from the rest of the fill: a strip between two clearances, or a spike, that the board house may not etch cleanly or that may lift. A corner of the fill is no copper sliver, since trimming the fill to the least track only rounds it, and neither is fill that lies against a pad, a track or another zone of the net, which is as wide as the two together. A corner of the fill (with the copper of its net) that comes to a point sharper than 20 degrees is a `copper_sliver` too, by KiCad's own measure (issue #1222); where two zones of a net meet, each is filled as if the other's area within its `min_width` were its own, as KiCad fills them, so the trimming leaves no notch with such a point beside it. The fill netsmith works out is trimmed to the zone's `min_width`, so a zone whose `min_width` is not under the least track (0.25 mm when the zone does not say) has none; a thinner `min_width`, or a fill the file holds, can. KiCad has a rule of that name with thresholds of its own, which no fabricator publishes, so the two are not compared.

To see how the fill joins every pad, and not only the starved ones, run `netsmith board fill` (issue #1187; the editor's PCB summary tab shows the same). It places the board, puts on it the copper `view pcb` draws (the routes file, the router's copper the editor keeps, the teardrops), fills every pour, plane and zone as `fab` fills them for the Gerbers, and prints for each fill the pads of its net in its outline and how each is joined: `solid`, `thermal` with how many of its spokes reach the fill (`1 of 4 spokes`) and the tracks of its net onto it, or `none`; the pads of other nets it keeps clear of; and its copper islands, those its `islands` keeps and those the fill took out, with a corner and the area of each. It counts the pads joined by two or more spokes, by one, by none, solid and none, and lists the starved ones, which are the `starved_thermal` warnings `board drc` gives for the same board, by the same count. The places are in the board file's frame, as `board drc` gives them, and `--format json` has every number; it writes nothing and exits 0 whatever it finds (`board drc` is the check). Its JSON has the outline of each fill too (issue #1293): `outline`, a list of the fill's pieces, each its `outer` ring and its `holes` (round the copper of other nets and in the thermal gaps), simplified within 2 µm, in the same frame.

A pad that the fill leaves unjoined, a thermal relief whose spokes the clearance of other nets cuts, or starves to one, is joined by `netsmith routes join-pad REF.PAD` (issue #1293): it draws a track of the net's own width from the pad's centre straight out, in each of 16 directions, to the first place, at most 5 mm out, where the track's round end lies wholly on the fill of its net on that layer, and tries the shortest first; where no track fits, a track to a through via placed on the fill of its net on another layer, clear of the pad. Each try is checked as `routes add-track` checks a track, and kept only when the pad is joined after it (`board fill` no longer has it starved or unjoinable; a pad no fill covers, when the copper of its net joins it to a pour, plane or zone of the net); the first kept is written to the net's record of the routes file, unlocked. Like the check of an edit, the join is judged where the pad is (issue #1474): the fill is made of the box round the pad, 5 mm and twice the board's largest space out, with the planes, pours and zones cut to it, and whether a pad joins a fill is asked of the pad's net's copper alone; no step fills or checks the whole board. Which pieces of a fill are islands is the whole board's, and is left to `board drc` and `fab`, as the ends an edit leaves open are. A pad joined already is said and nothing is written; when none of the 24 tries is taken it is refused (exit 1) with why the shortest was refused. `--dry-run` writes nothing.

The check also warns of an `acid_trap` (issue #838): a track that meets copper of its net at an acute angle, under 89 degrees, leaving a narrow wedge where the etchant pools and eats into the track. It looks where a track leaves a pad through a straight edge (a side of a rectangle, a polygon or an oval, or the straight part of a rounded rectangle's side), where a track ends on the middle of another track, and where tracks end at one place outside a pad (on a board netsmith routes `track_angle` judges those, with a stricter limit). A custom pad is judged at the edge the track leaves: a straight side of its anchor, of one of its polygons or of one of its lines. A round pad, a via, an oval's end, a rounded corner and a custom pad's circles and arcs (and the ends of its lines) are not judged unless the board asks: a round edge turns away from the track, and how sharply it does needs an angle that no board house publishes. A board that wants round copper judged states one, `.process.min_round_track_angle &= 60` (degrees, a lower bound: `60..` is the same): the least angle between the track's centre line and the tangent of the round edge at the point where the track leaves it, 90 along the radius and 0 along the edge, so a track 0.3 mm off the centre of a 1.7 mm round pad leaves at 69 degrees. A track that leaves round copper under it is a warning like the others, and a wedge that other copper fills is none. No process states the field, JLCPCB's included (the capabilities page names no such figure), so a board that does not state it is judged exactly as before. A wedge that a teardrop, a pour or other copper of the net fills is no trap. The rule is netsmith's, KiCad has none, and it is a warning until a real order shows it is a limit: the board house's capabilities page names no angle. The project file's `rule_severities` can make it an error or ignore it.

The check also holds the board file to the stackup of its process (`stackup`, netsmith's rule, issue #993): the file has as many copper layers as the process makes, a thickness the process makes (`.process.stackup.thickness`), and a stackup whose layers (copper, dielectric layers and mask) add up to that thickness within the board house's tolerance of it (JLCPCB: 10% from 1 mm, 0.1 mm below). Each is an error, since the board house cannot make the board the file describes. `fab` writes the file from the process, so a board it writes meets them; the rule finds a board file changed after it was written, or checked against another process.

`fab` puts every shape of a record on the placed board before the router runs. The router keeps it as copper of its net, never moves or removes it, and routes only the connections it leaves: a track that ends on another track's middle, a track drawn over a pad of its net, two tracks that cross and a via on a track count as joined, as KiCad counts them. The passes after the router (meanders, rounded bends, parallel vias, stitching, teardrops) leave it as it is, and the design rules check checks it with the rest of the copper. In the board files it comes first: a board drawn from a routes file has the copper of the board it was imported from in another order, so compare two boards' copper as sets.

`lock = true` on a net says its copper is the file's alone: the router adds no track, via or teardrop to it, and the ratsnest counts the board's outer pours of the net as joining what of the net they touch, as the design rules check does (a zone joins what its fill reaches on any net, above). A connection a locked net still lacks is left unrouted, and `fab` says so as it says any connection left. `lock = true` on a shape (a track, a via or a zone) says it is the person's: `netsmith routes delete`, `routes tidy` and a drag in the editor leave it as it is.

A record that no longer matches the board is said, never used silently. `pads` is a hash of the net's pads: each pad's part by its path in the design (a renumbered reference designator does not change it), its number, its centre to 1 µm and its copper layers. When a pad of the net moved, or one joined or left it, `fab` warns and keeps the record's copper, every shape of it (issue #1328): a track end or a via of the record that runs into nothing of its net (no pad, no other track or via, no pour, plane or zone of the net on its layer) is open, and the warning names each by the pad of the net nearest it, `open at J1.2 (track 3's end at 12.5, 4.0)`, the copper that ran to a pad no longer there. The net's lock does not hold for that run, so the router routes the connections the kept copper leaves, as `routes show` and the design rules check list them. Kept copper that a moved pad now lands on is a short, which the design rules check finds as it finds any. A shape that ran to the moved pad is deleted with `routes delete --net NAME --track N`, or the record written again with `routes lock`. A record for a net the board does not have (a net renamed, say) is a warning and is drawn not at all: `netsmith routes delete --net NAME`, with the name the warning gives, deletes it whole (also a locked one, which `routes unlock` could not reach, and one shape of it cannot be named), and says what it deleted. `netsmith routes lock` writes a record again with the pads as they are, and `netsmith routes delete` removes it.

`netsmith routes` keeps the file (`netsmith routes --help` has every flag):

- `import BOARD.kicad_pcb` writes the file from a KiCad board, and the layout file's places of the parts it has (locked, `source = "import"`; not those the physical file pins, and none with `--no-places`). A KiCad net is the design's net that most of its pads are on, the parts paired by the port's parts file (`reference/parts.toml` of the project, or `--parts`) or else by reference designator, as the parity checker pairs them. It takes every track, arc and through via of the outer layers and every zone of copper (on any copper layer, with its outline, holes and settings, written to 0.000001 mm) except the process's own planes and pours (issue #1116: a zone of the net, layer and outline that the process states, with the clearance, minimum width, pad connection and thermal relief it states, which is the zone `fab` writes for it, is left out and said, since taken it would replace the process's with a hand zone that keeps the settings as of the import; a zone that differs in any of these is taken, as a hand zone, so `fab` and then `routes import` of its board leaves a file of tracks and vias), locks every net (`--unlocked` does not, so that the router may add to the copper taken and `delete`, `tidy` and the editor may change it: a locked shape is not deleted until it is unlocked), and says what it leaves out: copper on an inner layer, a via that is not a through via, copper of a net on no pad of the design, a rule area (the design's keepouts are its own), a teardrop (`fab` draws its own), a zone filled with hatching (netsmith fills solid zones only), and the stitching vias of a pour (issue #1483: a through via of the pour's net with the rules' via size, within a third of the pitch of a point of the pour's stitching grid, in no pad and with no track ending on it), which `fab` lays again on the same grid. With every net locked (no `--unlocked`) the stitching vias are kept, since `fab` lays no copper on a locked net, and the import says so. A zone with smoothed corners is taken with its outline's corners as they are, and said. A part with two pads that the KiCad board numbers the other way round from the design's footprint (a resistor drawn turned in one of the two schematics: the pads of the other parts vouch for the KiCad net of its pad 1 being the design's net of its pad 2, and the other way round) is placed turned by half a turn, when the footprint is the same so turned, so that each pad stands where the KiCad board has the pad of its net; the import names it. After it writes the file it checks the board with the copper taken, and says each net whose own copper (a track, a via or a zone of it) the design rules check finds an error of, which a locked net keeps until it is unlocked and mended. With `--format json` it is one document (issue #1472): `board`, `file`, `parts_paired_by`, `locked`, the totals (`tracks`, `arcs`, `vias`, `zones`, `places`), `refitted`, `skipped` (what it left out, each a sentence) and `nets`, each with `net`, `kicad_net`, `locked`, the `tracks`, `arcs`, `vias` and `zones` taken, and `refusals`: each error of the design rules check of its own copper, with its `rule`, `message`, `at` (in the routes file's frame), `layer` and the `shapes` it names.
- `add-track`, `add-via` and `add-zone` add a shape to a net, and `delete`, `lock` and `unlock` act on a net's record or on one of its shapes (`--track N`, `--via N`, `--zone N`, numbered from 1 as `show` lists them). An added shape is refused when it brings an error of the design rules check that the board did not have (a short, a crossing, a clearance), and the file is left as it was. The check is made where the shape is (issue #1471): on the copper, pads and fills within the board's largest space of what the edit adds or removes, and on its findings there, so that an edit takes about as long on a board of thousands of shapes as on a small one; a finding that can lie far from the copper that makes it (a fill's island, the current of a branch of a net) is left to `board drc` and `fab`, which check the whole board; `lock`, which writes the record again with its pads as they are, is refused only when the check finds an error of that net's own copper in the file (a track or via of a record whose pad moved now crossing another net's pad, say), whatever errors other copper has (issue #1328); an end that touches nothing yet is not refused, because a route is drawn one shape at a time (`fab`'s check still finds one left so). A locked net or shape is not deleted until it is unlocked.
- `add SHAPES` (issue #1472) adds many shapes in one call, from a file (`shapes.toml` or `shapes.json`, by the extension) or from stdin (`-`, JSON when it starts with `{`), and writes the file once. The document has `[[track]]`, `[[via]]` and `[[zone]]` tables (in JSON the arrays `track`, `via` and `zone`), each with the fields of the flags of `add-track`, `add-via` and `add-zone` and the same defaults (`at` of a via is one place, or a list of places for a group; `layer` of a zone is a word or a list; one of `outline`, `rect` and `board_outline`). Each shape is checked as its verb checks it, where it is, on the board with the shapes made before it: the tracks, then the vias, then the zones, each in the document's order. A shape refused is skipped and said, with the findings of the design rules check that refused it (the rule, the place in the routes file's frame and the shapes it names), and the rest go on; `--stop-on-refusal` stops at the first and keeps what was made before it, and `--dry-run` writes nothing. The exit code is 1 when a shape was refused (the others are written). Up to 16 shapes in a row are checked together, in their neighbourhoods: a group the check finds no new error with is made whole, and one it does is halved until each refused shape is found alone, on the board with every shape made before it (an error that a later shape of its group takes away is not counted). So a batch costs its shapes' neighbourhoods, and about a check for each 8 shapes when few are refused: 2,000 shapes on a board of 154 parts and four layers take about 20 seconds; `--format json` says how many checks ran and how many items they looked at (`checks`, `items_checked`), with each shape's `result` (`ok`, `refused` or `not_tried`) and `findings`. `netsmith mcp` has it as the tool `routes_add`, whose `shapes` may be the document itself, a JSON object, so a host pays one process start for the batch:

```text
[[track]]
net = "VCC"
layer = "bottom"
from = [5.0, 2.7]
to = [8.0, 2.7]

[[via]]
net = "VCC"
at = [8.0, 2.7]

[[zone]]
net = "GND"
layer = ["top", "bottom"]
rect = [1.0, 1.0, 6.0, 4.0]
```

- `add-via` adds several vias side by side as a group (issue #1114): `--at X,Y` once for each place, or `--count N --pitch MM` with one `--at` for N vias in a row along x, MM between their centres, centred on that place. The group is checked together, as the router's own group is, and is added or refused as a whole. That is what a net whose current needs several vias where it changes layer needs: each of three vias of a 1.2 A net is refused alone by the check `via_current` (a via with fewer vias of its net touching it than the net's current needs), and the three together are accepted when they touch, a pitch of at most the via's diameter. A single via is still checked alone. The editor's PCB view sends several vias as one edit and is checked the same way.
- `add-zone --net GND --layer bottom --rect 1,1,6,4` draws a zone of a net, without an import. The outline is a polygon (`--outline X,Y` for each corner, in order), a rectangle (`--rect X,Y,W,H`, the top left corner and the size) or the board's own outline (`--board-outline`), in the same places as `add-track`'s; `--layer` is repeated for a zone on several layers. A setting it is not given is the design's own plane's: the clearance the process's planes keep, a minimum width of 0.25 mm, a thermal relief with a gap and spokes of 0.5 mm, priority 0 (`--clearance`, `--min-width`, `--thermal-gap`, `--spoke-width`, `--priority`, `--pads` and `--lock` set them). A zone is no short where it covers another net's pad or track, because its fill keeps its clearance round it, so it is not refused for that. It is refused when a corner is off the board, a setting is wrong or less than the process makes, two zones of different nets with the same priority overlap, or the design rules check finds a new error with it. A zone of the net of the design's pour or plane on its layer restates that pour or plane, which is then not drawn there; a zone of another net cuts it, and the pour is drawn round it. The command says which. A refusal names its places from the outline's top left corner, as the command takes them, not as the KiCad board file has them.
- `show` prints each record with its shapes numbered, what of the file is not used, and the connections the router would still route.
- `tidy` repairs the copper of the file (issue #1192), and with `--reuse-routes` the router's copper kept in `.netsmith/routes/` too, as `board fab --reuse-routes` starts from it. A corner where two straight tracks of a net bend by more than 45 degrees outside its pads (`track_angle`) is cut into bends of 45 degrees at most, a 45 degree chamfer at a right angle, four widths from the corner (two, then one, when that is refused). Two straight tracks of one net, layer and width that meet end to end on one line, with nothing else there, become one. A dangling via (`via_dangling`) and a track with an end that touches nothing (`track_dangling`) are removed when the copper makes every connection of the net, since the router may still start from a stub of a net it has to finish; a via that joins nothing at all is removed in any net. An `acid_trap` where a track leaves a pad or a via at an acute angle is removed by making the track leave the edge at a right angle and bend after it, and one where a track ends on the middle of another by making it meet the other at a right angle. Four more are defects the check finds nothing of (issue #1485), each counted in the text and in the JSON's `counts`: a **track inside a pad**, a straight track from the centre of a pad or a via of its net to a point still inside it (a pad held to the circle of its shorter side round its centre), is removed (`in_pad_tracks`); of two **ways** of a net between the same two places, each a run of tracks and the vias it changes layer by with no pad on its middle and nothing else meeting it there, the longer is removed (`duplicates`; a net with a track that ends on the middle of another is left); a straight track whose copper lies wholly inside another of its net and layer is removed, and two of one width on one line that **overlap** along it become one (`overlaps`; each track is compared with the tracks of its net and layer that the board's grid lists near it, so the work grows with the neighbourhood and not with the length of the net, issue 1492); and a **run of tiny tracks**, two or more straight tracks in a row each shorter than two of its widths, joined where no pad, via or third track meets them, becomes one track from its first end to its last, or, when that is refused, two that meet where the lines of the tracks on either side cross, never bending by more than 45 degrees (`tiny_runs`). With `--widen` (issue #1294) a straight track narrower than the width its net wants on its layer is widened to it: the width the router draws the net at there (its rails' width and the rules' track width, the width an inner layer needs for its current, or the width of its impedance target), and for a net with pins that state the current of their branch the narrowest of its lines, since a track is not told which line it is of. A widening that is refused, because the wider track comes too near copper of another net, is tried at three quarters, half and a quarter of the way, so that each track is made as wide as the clearance to its neighbours allows; a track is never made narrower, and the text lists each with the width it had, the width it has and the width its net wants (`counts.widened` in the JSON). Each change is made only when the design rules check with it, warnings too, finds nothing it did not find before (a finding is the same when its rule, message, place and layer are, the lengths of the tracks it names left out) and the connections left to route are the same; the changes of each net are checked together, and a group refused is split in two until the change that brings the finding is found, which is listed with that finding and left as it was. A group is checked where it is (issue #1225): over the copper, pads and zones within the board's largest space (a clearance, the space to a hole or a mask opening, how near a pad a track necks down) of what it changes, with and without it, and the whole board is checked once after each pass; when that check finds what the pass's changes add beyond where they are (the pieces of a net, a zone's fill), they are checked again against the whole board, so the changes made are those the check of the whole board allows. The text ends with how many checks ran and the items they looked at (`checks` and `items_checked` in the JSON). On stderr each pass says how far it has got, net by net: the net, how many of the pass's nets, the changes made so far and about how long the pass has left (`--progress json` prints each line as JSON with `stage` `tidying` and `unit` `nets tidied`, and `--to-editor` sends them to a running editor, as `board fab` does); with `--format json` the progress is printed only with `--progress json`. Nothing is written until the pass has run, so Ctrl+C leaves the files as they were. A locked net or shape, an arc, a zone and a record whose pads moved are never touched. `--dry-run` lists what would change, by kind, net, layer and place, and writes nothing; `--format json` gives the same as one document. With `--reuse-routes` the kept file is written again with the tidied routes file's hash, so that `board fab --reuse-routes` still starts from it; without it, a change to the file makes the kept copper stale, and `tidy` says so.
- `route NET` (issue #1195) routes one net with the router round the copper there, every other net held as it is, and writes its tracks and vias to the net's record, unlocked; `route NET:U1.3-C2.1` routes the one connection between those two pads (the net's other pads count as no net's while it is routed, so the way does not run over them; a net with a plane joins each of the two to it). It says the narrowest place of the new way and the widest track of the net that has room there with every space of the rules. When the net has no way, it is routed again with the unlocked tracks and vias of the other nets taken away (a net whose record is locked, or whose pads moved, keeps its copper, and so does every locked shape and every zone; a net that a pour, a plane or a zone joins is never taken away or named: its fill is no track to remove, and the router spends no run on it, issue #1315), and the nets whose copper that way crosses are named, each given back its copper in turn and kept back when the net still routes; nothing is written and the exit code is 1. With no way even so, it says what walls the net in: pads, vias, locked copper, the edge. `--rip A,B` lets it take away the copper of those nets alone (a net joined by a pour, a plane or a zone is refused: it has no copper to rip), route the net through the room, and route each of them again after it, by name; a ripped net that finds no way may rip in turn from the nets named, never the net asked for, to the depth `--rip-depth N` gives (issue #1488): the nets the net asked for rips are at depth 1, the nets one of them rips at depth 2, and a ripped net rips in turn only while its depth is at most N, so the default, 1, lets the nets the net asked for rips rip once more, and 0 lets no ripped net rip. A ripped net left with more connections open than it had has not been put back: the ripped nets are then routed again from the start in another order, the nets not put back first and then the rest (or, when that order was tried, the order before turned by one), up to `--tries N` orders (default 3), none twice. Each try is said: its order, each net it ripped, by which net and at what depth, in the order they were ripped, the nets not put back, and the nets that found no way and could have ripped in turn but for the depth; the refusal, when no try puts every net back, names `--rip-depth` or `--tries` where one more would try something new. The bounds are counts, those of the router's rounds of rip-up: in each try 6 rounds, 3 rips of one net and 64 runs of the router. The records that change are one edit, checked as `add-track` is: a result that adds an error of the design rules check, or that no try puts every ripped net back in, is refused and the file is as it was. `--dry-run` says what it would do and writes nothing; `--format json` is the same as a document (`in_the_way`, `ripped`, `tries` (each with its `order`, `ripped` with each net's `net`, `by` and `depth`, `failed`, `failed_how`, `depth_stopped`, `runs` and `rounds`), `nets` with each record's removed and new shapes, `narrowest` (the place, the width of its track and `fits`, the widest track that has room there), `walls`, `refused`, `written`).

Each command builds and places the board as `fab` does, and routes nothing but `route`'s net and the nets it rips. It exits 0 when it did what it was asked, 1 when it refused, and 2 when it could not run.

The editor's PCB view marks the file's copper over the board (a solid line on a locked net, dashed on one the router may add to, a padlock on a locked shape), lists its records in the Routes tab of the side panel, and locks, unlocks and deletes a net's record or one shape (`L`, `Delete`, or the inspector) with the same code as `netsmith routes`, refused for the same reasons. It draws copper too: the track tool (`X`) starts a track on a pad, a track's end or a via, takes the net's width from the board's rules and the layer from the toolbar, adds a corner at each click, bending the segment round the copper in the way in corners of 45 degrees at most (the _Walkaround_, with the spaces of the design rules check; where there is none the preview says so and the click is refused; the unlocked tracks and vias of the routes file that are in the way are shoved aside first, as little as clears and with their corners and ends kept, and previewed at their new places with the old ones faded, and a locked shape, a shape of a locked net, a pad or a keepout stays and is gone round instead (the toolbar's mode can be set to go round only); on a routed board the router's copper is no obstacle, since an edit throws it away, and the preview shows the router's copper the track crosses faded, and dashed where routing again would lay those nets round the new track), puts a via at the last corner when another layer is picked while the track is drawn, and ends it with a double-click or Enter; the via tool (`V`) places the process's via on a net; and a drag moves a fixed track's end or a via, with the shapes of the net that meet there. The via and the drag shove the unlocked copper in their way as the track does, and where they have no room they are refused with what is in the way. A track or via that adds an error of the design rules check is refused with the check's errors, as `add-track` refuses it, and the file is not changed. Every change to the file is one step of the view's undo (Ctrl+Z, Ctrl+Shift+Z), which puts the net's record back as it was; a track drawn with shoved copper changes the records of several nets in one edit, and one undo puts them all back. A record that is not used as written is a warning in its findings.

#### Reproducing a KiCad board

To draw a board that a KiCad file already routes, import its copper and places, then run `fab`: in the project, `netsmith routes import reference/board.kicad_pcb` and `netsmith board fab`. The parts the import places stand where the KiCad board has them, every net is locked (a net the KiCad board joins by a zone alone too, since its record holds the zone), and the board's zones replace the design's pours and planes on their layers, so the router adds no copper. What `fab` still lacks is said as any connection left: a pad that netsmith's footprint draws otherwise than the KiCad board's, mostly (a ground pad the KiCad footprint joins to another layer by plated holes of its own, say). A part that must hang over an edge, as a module's antenna does, needs its requirement in the physical file (`.mcu.place.top <= 0mm`), because a place in the layout file allows no overhang.

Three ports keep such a reproduction beside their own build, so that the board the placer and router make (the automatic board, the example as it is) can be compared with the original's as they improve: `examples/rp2350_minimal`, `examples/orbit` and `examples/easyduino_esp32s3` each have the imported routes file and layout file in `reference/agent/`, not in `src/`, so their own build is untouched (`scripts/reproductions --update` writes them again). The tests of `crates/netsmith-cli/tests/reproduce.rs` put those files into `src/` of a copy, run `fab` and require every track, arc, via and zone of the original on the board it writes, on the same layer and net (a zone with the same outline, holes and settings), no connection left but those the test names and no copper the original does not have; each example's README says what the reproduction lacks and why. To draw the parts as the original does, a port draws each with the original's footprint where netsmith's differs, written out of the KiCad board (`cargo run -p netsmith-tools --example footprint_from_board -- BOARD.kicad_pcb REF libs/<name>.pretty`) and named by a `[[part]]` or `[[member]]` row of `kicad.toml` (_One part with a footprint of its own_); the tests compare every footprint of the reproduction with the original's (place, turn, face, pads and their nets, courtyard) and require each error of the design rules check to be one the original has itself.

### The pick lock

Which catalog row a part was picked as is the solver's choice: the first row, in catalog order, that fits and passes every relation. A change to a catalog, to the order of its rows or to the solver can move a pick without a change to the source. `<stem>.lock.toml` beside the source file (`board.nsm` has `board.lock.toml`) keeps the choice: `build` and `fab` write it when it does not exist, and commit it as you commit `Cargo.lock`.

```toml
version = 2

[LedExample.parts."r1"]
catalog = "resistor"
row = "330Ω 1% 0402 C25104"
mpn = "0402WGF3300TCE"
sku = { jlcpcb-assembly = "C25104", lcsc-retail = "C25104" }
```

A use of a generic with more than one provider in scope is locked too, at the provider it was picked as (ADR 0113): `[LedExample.providers."process"]` with `generic = "TwoLayer"` and `provider = "std::mfg::jlcpcb::JlcTwoLayer1oz"`, written before the board's parts, because the provider decides which parts there are. The use is made with that provider and no other is tried: a board that now asks for what another provider has (another copper weight) stops on the contradiction with a note that names the lock, and `project lock --update` picks again and prints the change (`` LedExample: `process`: provider `...JlcTwoLayer1oz` -> `...JlcTwoLayer2oz` ``). A locked provider that is no longer in scope is an error, as a row that is gone is.

There is one table for each board (or system) of the file, keyed by its name, and one for each part, keyed by its path (`r1`, `stage.output_cap`; a bank of capacitors is one entry, its path without an index). `catalog` and `row` say which row. `manufacturer` and `mpn` are the part's identity, written when the row has them (`manufacturer` when its catalog row names one, ADR 0143): they find a row that was renamed or changed in place, tell apart two rows of one `mpn` from two manufacturers, and may be left out. `sku` is what the part resolved to for each supply channel that has an offer of it (`jlcpcb-assembly` from an `lcsc` offer that has a `class`, `lcsc-retail` from any `lcsc` offer, _Offers files_ in the chapter on catalogs): a record, so that an offer whose number changes is a line of the diff, and the solver does not read it. A lock of `version = 1`, which an older netsmith wrote and which records no manufacturer, and one that records a part's `lcsc` in place of its `sku` (as locks did before ADR 0143 moved the number into the offers), is still read and holds its parts as before (a build says so in a note, and `--locked` does not refuse it); `project lock --check` calls it not current, with the line `version 1 -> 2: ...` or `lcsc -> sku: ...`, and `project lock --update` writes it again. The file holds no stock, no price and no solved value; a generic part is locked at the row the solver gave it (`generic 22uF 10% 0603 ...`), and `fab` still resolves it to an orderable row against the stock of the day (ADR 0047).

With a lock, the solver offers each part the lock names only its locked row, and judges that row by everything the design states and every relation, as it judges any pick. What happens next is told for each part:

- **The row still passes.** It is the pick. Nothing is printed.
- **The row no longer passes, or is no longer in the catalog.** The run stops with the relation that fails and its numbers, and a second message says that the row is the lock's and that no other row was tried (`` `r1` is locked to the row `2.2kΩ 1% 0402 C25879` of the `resistor` catalog by led.lock.toml, and no other row was tried for it ``). Change the design or the catalog so the row fits, or delete the lock to pick again. A row whose `manufacturer` or `mpn` is no longer the lock's is the same case.
- **A part has no entry, or an entry belongs to a part the board no longer has.** The run goes on and prints a note that names the part. The lock is not rewritten: an existing lock is never changed by `build` or `fab`.

`--locked` (on `build` and `fab`, as Cargo's, and so on `project order`) turns the last case, and a missing file, into an error, before anything is written. A CI job that builds with `--locked` fails a pull request that changes the picks without changing the lock.

#### Refreshing the lock

A lock that is old is refreshed on purpose, with `netsmith project lock --update [file.nsm]` (every top-level file of the project when no file is named). It solves each board of the file again as if there were no lock, so every part is free to take the row the solver picks now, and writes the lock with those rows. It prints what changed, a line for each part in the order board, part path, with the row's manufacturer part number and the sku of each channel, and nothing at all when the lock is already as it would write it (then the file is not touched):

```sh
$ netsmith project lock --update
updated src/main.lock.toml
  Zz9Board: `u`: `ZZ9-33-S` (mpn ZZ9-33-S, jlcpcb-assembly C9001) -> `ZZ9-33-A` (mpn ZZ9-33-A, jlcpcb-assembly C9002)
```

Each case of the list above ends in a line of that diff:

- **A part the lock has no entry for** is added (`LedExample: `r2`: added as ...`), and the entries of the other parts are as they were unless their own pick moved. With no lock file, every part is added and the file is written.
- **An entry for a part that is gone** is dropped (`LedExample: `r2`: dropped (it was ..., and is no longer a part of the board)`).
- **A row that no longer passes, or that no catalog has**, is not replaced silently. `lock` first solves the board held to the rows of the file, as `build` does, and prints what the solver said about them, in its own words, to stderr after `note:` (the relation that fails and its numbers, or that the catalog has no such row); `--update` then replaces the row with the one the solver picks now, and the diff says which.
- **A row whose part a release added to a built-in catalog under another name** (issue #1119) is rewritten to the built-in row and kept on it, with no other change. A project's own catalog may have a part that a later release adds to the built-in one; the build then stops with "a part is in a catalog once" (_Catalogs and how the solver picks parts_), and the project's row is removed by hand. The lock's `row` still names that row, which no catalog has now, so `build` says so, and names the built-in row with the same manufacturer part number and `project lock --update`. `--update` sets the `row`, the manufacturer, the manufacturer part number and the skus of the entry to that row's and does not take a new pick for the part, also with `--part` or `--except` (a part `--part` names takes a new pick); if the built-in row no longer passes, the part takes a new pick and the `note:` says why. `--check` prints the change and exits 1.
- **A row that still passes but is no longer the first that fits** (a catalog gained an earlier row, or the solver changed) is replaced too: that is what a refresh is for, and the diff is how the change is reviewed.

To take new picks for some parts and keep the rest, name them: `netsmith project lock --update --part r1,stage.cap` refreshes those parts only, and `--except r1` every part but those (a comma-separated list of part paths; the two are not said together, and `--part` is not said with `--check`). The parts that do not take a new pick stay on the row the lock has for them, which is judged by everything the design says, as `build` judges it, and so are the locked providers. A row held this way that no longer passes stops the run, with the solver's words and a line that says to name it, and nothing is written. A part with no entry in the lock takes the solver's pick whether it is named or not, and an entry for a part that is gone is dropped. A path is looked for in every board of the file (or, with no file, of the project's top-level files), and one that no board has is an error that lists the parts there are, with nothing written. This is how a design keeps one part on an earlier row while the rest take the catalog's new ones, with no hand pick in the source and no edit of the lock:

```sh
$ netsmith project lock --update --except u1
updated src/main.lock.toml
  Zz9Board: `u2`: `ZZ9-33-S` (mpn ZZ9-33-S, jlcpcb-assembly C9001) -> `ZZ9-33-A` (mpn ZZ9-33-A, jlcpcb-assembly C9002)
```

The part is held only until the next `project lock --update` that does not name it; a decision that must outlive refreshes is a held row (below) or a hand pick in the source (below that). A part that was renamed has no entry under its new name, so it is added at the solver's pick, and the entry of the old name is dropped; carrying its row to the new name is not something `lock` does.

Which source files have a lock at all is the project's `lock` key (_Projects and netsmith.toml_): `lock = "top_level"` gives one only to the files of `top_level`, so a unit test of one part, or a scratch file, named on the command line (`netsmith board build tests/led_test.nsm`) has none. A source with no lock is not read for one or written one, `--locked` has nothing to refuse for it, and `project lock --update` and `project lock --check` skip it. `lock = "none"` gives none to any file. The default, `"all"`, is as above.

#### A row held on purpose

A person may keep a part on its row on purpose, for a reason the solver does not know: the row a first article was built with, or one that has been qualified. A part's entry in the lock says so with `held = "reason"` (issue #1197), the one key of the file that a person writes:

```toml
[LedExample.parts."r1"]
catalog = "resistor"
row = "330Ω 1% 0402 C25104"
mpn = "0402WGF3300TCE"
sku = { jlcpcb-assembly = "C25104", lcsc-retail = "C25104" }
held = "the row the first article was built with"
```

`reason` is text and not empty. A held row is held to its row as any locked row is, and judged by everything the design states, so one that no longer passes stops the run with the solver's words, and a line that says to remove its `held` line or to name the part with `--update --part` (a held row is never replaced silently, and the part never takes another row behind a person's back). The solver does not read the reason; a build does not print it.

- **`project lock --check` does not call a held row not current**, whatever the solver would pick now, so a CI job that runs it stays green on a deliberate hold. It lists each held row once, after the diff: `held in src/main.lock.toml:` and then a line for each, `LedExample: `r1`: `330Ω 1% 0402 C25104` in the `resistor` catalog (lcsc C25104, mpn 0402WGF3300TCE): the row the first article was built with`. Without the `held` key the same row is not current and `--check` exits 1, as before. A lock whose other rows are not current still fails; the held row is simply not among the differences.
- **`project lock --update` keeps a held row and its `held` key**, with the reason as written, whether it names no part, `--except` or another part with `--part`, and lists it as `--check` does. It takes a new pick for the part only when `--part` names it, and the key is then dropped; the diff says so (`LedExample: `r1`: `A` (lcsc C1) -> `B` (lcsc C2) (no longer held: the row the first article was built with)`). A held row whose part the board no longer has is dropped as any entry is.
- **`--format json`** gives each file a `held` list, one object for each held row with its `board`, `part`, `row` and `reason`, when there are some.

`netsmith project lock --check [file.nsm]` does the same and writes nothing. It prints the same diff and exits 1 when the lock is not current or does not exist (with a message that says which), and exits 0 and prints nothing when it is current (a held row and a part `--except` names are listed, above and below), so a CI job can run it beside `board build --locked`. `--check --except r1,stage.cap` judges every part but those: they stay on the row the lock has for them, which is judged by everything the design says as for `--update`, and are listed once after the diff as `not judged in src/main.lock.toml (--except):` with a line for each (a part that is held is listed as held only), and as `not_judged` in `--format json`, each with its `board`, `part` and `row`. A part named that no board has is an error, as for `--update`. `--part` is for `--update` alone: a check judges every part, or every part but those `--except` names. Both exit 2 when they cannot tell: the lock is not a lock this netsmith reads, or the design does not build even with no lock. Two runs on the same project print the same text and write the same bytes. The pin file is not read for its writing: `lock` writes only the lock, and a pin request that has no pin yet is assigned in memory, as the editor does.

`project order <name>` does with each source file that builds a board of the order what `fab` does with that file, so the order gets the rows `fab` of the file alone gets: it holds the boards of the file to its lock, writes the lock when the file does not exist, and never rewrites one that does (a difference is a note). Only the files that build a board of the order are read, and a lock covers all the boards of its file, whichever of them the order takes. `project order <name> --locked` compares the lock of each of those files with the picks before it removes or writes anything, and refuses, naming each file, one that does not exist or that the picks differ from. An order is a release, and a release never moves a choice by itself: it moves no pin (the pin file) and no row of an existing lock.

The editor holds the parts to the lock as `build` does, so its views (the schematic, the PCB view, the 3D view, the bill of materials) and the editor's `fab` release show the rows the lock names even when the solver would pick others now; a lock that is not current is told in the _Pick lock_ section, and a lock this netsmith cannot read is built as if there were none and says why there. An order (`project order` and the editor's order button) still ignores the lock in the editor. It shows the lock of the selected board's source file and writes nothing: the schematic's inspector has a _Pick lock_ section that says whether the file exists and whether it is current, and lists the diff that `project lock --check` would print (a source that does not build has none). It is read again when the lock file changes on disk, which the editor watches.

### A hand pick

A generic part is picked as the first row that fits, and a port of an existing design, or a bill of materials someone else wrote, often names another row that fits as well: a 50 V capacitor where the solver would take a 16 V one. The design then states the part's number on it, and the part is held to that row (a hand pick, ADR 0122). Every part has two members for it, as it has `.designator`: `.mpn`, the manufacturer's part number, and `.manufacturer`, who makes it (ADR 0143). Both are text. A narrowing of `.mpn` is the hand pick, and `.manufacturer` beside it says which of two parts of one number it is:

```nsm evaluate
import std::parts::passive::{Led, Resistor}
import std::sources::Rail

board HandPicked {
    .supply: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC" }
    .led: Led { .current &= 5mA..15mA }
    // The solver would take a 330 ohm 0402 resistor; the bill of materials names this one.
    .r1: Resistor { .mpn &= "0603WAF3900T5E" }
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
```

The solver offers the part only the row of that part (with a `.manufacturer` too, the row of that maker's part; with none, every row of the number, and the first that fits is taken), and judges it by everything the design states and every relation, as it judges a locked row. A hand pick comes before the pick lock: the lock records what the solver picked, and a hand pick is a requirement. A board whose hand pick differs from its lock builds with the hand-picked row and says that the lock differs (`--locked` makes that an error), and `project lock --update` records the new row. A part inside a module is picked from the board as any member inside it is narrowed (`.stage.output_cap.mpn &= "CL05B104KB54PNC"`, ADR 0116). A part whose catalog has a column `mpn` and lists `.mpn` in its `catalog` statement (`catalog mcu by .mpn`, above) names its row the same way; its `.mpn: String` declares the member every part has, and may be left out.

What happens next is told for each part:

- **The row passes.** It is the pick.
- **The row fails a relation.** The run stops with the relation and its numbers, and a second message at the narrowing says that no other row was tried (`` `r1` is held by `.mpn &= "0402WGF2201TCE"` to one row of the `resistor` catalog, and no other row was tried for it ``).
- **The catalog has no row with the number.** The run stops at the narrowing:

```nsm fails
import std::parts::passive::{Led, Resistor}
import std::sources::Rail

board NoSuchRow {
    .supply: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }
    .led: Led { .current &= 5mA..15mA }
    .r1: Resistor { .mpn &= "ZZ-0000" }
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
```

```text
error: `r1` is held by `.mpn &= "ZZ-0000"` to a row of the `resistor` catalog, and the catalog has no row with the manufacturer part number ZZ-0000: no other row was tried for it; change the narrowing to a number the catalog has, or remove it to let the solver pick
  --> main.nsm:7:21
  |
7 |     .r1: Resistor { .mpn &= "ZZ-0000" }
  |                     ^^^^^^^^^^^^^^^^^
```

Two narrowings of one part's `.mpn` that differ are a `contradiction`, as any two narrowings of one text are. A `.manufacturer` on a part that picks from no catalog (a `Switch`), or with no `.mpn` beside it, is an error at the narrowing. The editor solves with the hand picks too, since they are the source.

A hand pick was once written with the supplier's number, `.lcsc &= "C23151"`. A part is its manufacturer's number now, and a supplier's number is an offer (ADR 0143), so that narrowing is an error at it that gives the `.mpn` to write, read from the offers; `netsmith project migrate-offers` rewrites every such narrowing of a project (`docs/MIGRATING.md`):

```nsm fails
import std::parts::passive::{Led, Resistor}
import std::sources::Rail

board OldHandPick {
    .supply: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }
    .led: Led { .current &= 5mA..15mA }
    .r1: Resistor { .lcsc &= "C23151" }
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
```

```text
error: `.lcsc &= "C23151"` on `r1`: a hand pick is by the manufacturer's part number now, and a supplier's number is an offer (ADR 0143): write `.mpn &= "0603WAF3900T5E"` in its place, the part the lcsc offer C23151 is of (data/offers/lcsc.csv:177); `netsmith project migrate-offers` rewrites these narrowings in a project's sources
  --> main.nsm:7:21
  |
7 |     .r1: Resistor { .lcsc &= "C23151" }
  |                     ^^^^^^^^^^^^^^^^^
```

netsmith once read hand picks from a file beside the source, `<stem>.picks.toml`. It no longer does: a source with that file beside it stops `build`, `fab`, `lock` and the editor with an error that lists the narrowing to write for each entry, by board (`.boot1_cap.mpn &= "CL05B104KB54PNC"` in `board PowerManifoldCharger`); write them, and delete the file.
