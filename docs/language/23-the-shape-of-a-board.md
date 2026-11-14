## The shape of a board

Beyond its parts and nets, a board can say how wide its power tracks are, how big it is and what is cut out of it, where copper is poured, and where tracks, vias, copper or parts are not allowed (ADR 0020, ADR 0029). Each is a member of the board, or a field of one, in the standard library: `std::pcb` has `Outline`, `Cutout` and `Keepout`, `std::mfg::stackup` has `Pour`, and a `Rail` has `.min_track`. The track widths and the pours are what the circuit's checks consume, and they stay in the board. The outline, the cutouts, the keepouts a board draws, the mounting holes, the fiducials, the labels and every requirement on where a part goes are **physical intent**, and a board states them in its physical file (ADR 0048), described first. A board that says none of it is laid out as before.

Lengths are in the board's own frame: the origin is the outline's top left corner, x grows to the right and y downward. A board states how big a shape is. Where a shape is, when the board does not say, is in the layout file (see below).

### The physical file

Physical intent is what the product, the case and the layout designer require of a board, as opposed to what the circuit requires: the outline, openings, mounting holes, where a connector must be, a fixed spacing between two parts, the face of a module, a keepout. It is requirements, never solutions. The circuit source stays strictly the circuit, and a case revision is a diff of its own file (ADR 0048).

It is in `<stem>.physical.nsm` beside the source, found by stem as the layout file is: `board.nsm` has `board.physical.nsm`. The file is optional and never needed for a board that states no physical intent. It holds one unit `physical for Board { ... }` for each board it speaks for (the file may hold the units of several boards), and the statements in a unit are the language of a board's body: declarations, `&=`, `==`, `>=` and `<=`. The units are put with their boards before anything else is checked, so a `panel` or a `system` that places the board sees them too.

```nsm solve place
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

module Sensor {
    .j: Header1x2
}

board Cased {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .power: Header1x2
    .sensor: Sensor
}
```

```nsm-physical
import std::pcb::{Cutout, Keepout, Outline}

physical for Cased {
    // The case: numbers from the case drawing.
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
            .corner_radius &= 1mm
        }
        .window: Cutout {
            .x &= 30mm
            .y &= 2mm
            .width &= 6mm
            .height &= 3mm
        }
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

- **`case { ... }`** is the section for what the case fixes: the outline, the cutouts, the mounting holes (`MountingHole`), the fiducials (`Fiducial`) and the labels (`Label`) of the board, and the case's own STEP file (`CaseModel`, see _A case model_). It holds the same statements as the rest of the unit, and lifts out unchanged if a case is ever shared. These six belong in it; a keepout may be anywhere in the unit.
- **A path crosses a module boundary here for any statement of physical intent.** `.sensor.j.place.left` names the header inside the `Sensor` module. The circuit source reaches into a module only to narrow a value (ADR 0116); the physical file can also state physical intent about the part. A statement may set or compare only a part's `.place`, a part's `.height`, and what the unit declares itself (the fields of its `.outline`, `.window`, `.rib`). Anything that changes what a part is, or a connection, is refused.
- **A relation between two parts** is the equation of _Where a part goes_: `.b.place.x - .a.place.x == 17.5mm +/- 0.05mm`. The placer holds it and `fab` measures it (_A relation between two parts_).
- **`.place.side` on a module** is a requirement on every part of it, at any depth. A module with an exception states the faces of its parts one by one and none for itself; two lines that disagree are an error that names both.
- The case numbers are the maintainer's file to review: put the source of each (a drawing, a sheet) in a comment above the line, and never invent one.

`examples/enclosure_sensor/` is a complete invented board that states all of this in its physical file, and each statement has a comment that says what it asks for: a rounded outline with a cutout, mounting holes and fiducials, a 40 mm plug that hangs over an edge, an antenna and a coax connector on one line, two socket rows a fixed distance apart, a module on the back with one part on the front, a row of twelve test pads on a pitch, a height limit, and a flat cell on the back across the sockets whose pins protrude 0.2 mm (_Pin protrusion_). It is a project, because it defines four invented parts and maps them to KiCad's stock footprints and a footprint of its own in its `kicad.toml`; `crates/netsmith-fabricate/tests/enclosure_sensor.rs` runs `fab` on it and checks that every requirement is met.

The placer keeps to every requirement, `fab` measures them on the board and stops on one not met, and the editor draws the file's outline and keepouts. A requirement that is a single value fixes the part, like a lock in the layout file.

Misuse: the old place.

```nsm fails
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer
import std::pcb::Outline

board Old {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .outline: Outline {
        .width &= 40mm
        .height &= 24mm
    }
    .j: Header1x2
    .j.place.left &= 1.5mm
}
```

```text
error: `.outline: Outline` is an outline, which is physical intent, and a board's circuit source does not hold it (ADR 0048); declare it in the `case { }` section of `physical for Old` in `main.physical.nsm`
  --> main.nsm:9:15
  |
9 |     .outline: Outline {
  |               ^^^^^^^
error: `j.place.left` is a placement requirement, which is physical intent, and a board's circuit source does not state it (ADR 0048); put it in `physical for Old` in `main.physical.nsm`
   --> main.nsm:14:5
   |
14 |     .j.place.left &= 1.5mm
   |     ^^^^^^^^^^^^^^^^^^^^^^
```

Misuse: a physical file that changes what a part is.

```nsm fails
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

module Block {
    .j: Header1x2
}

board Wrong {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
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
error: `b.j.assembly` is not physical intent: a physical file states where a part is (its `.place`), how tall it is (`.height`), how far its pins protrude (`.pin_protrusion`), the STEP model it is drawn from (`.model`) and the outline, cutouts, keepouts, holes, fiducials, labels and case models it declares, never what a part is; put this in the circuit source
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
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
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

### How tracks bend

netsmith's router bends a track by 45 degrees only (issue #415). Its first pass searches in eight directions, where a 45 degree bend costs a little and an acute one is never a move; every right angle a path has is cut into two bends of 45 degrees, measured exactly against the other copper, and a line whose corner cannot be cut is searched for again with no right angle, or left unrouted, never drawn with the corner. Only where a corner falls inside the copper of a pad or a via of its own net (the stub from a pad's centre to its first step) does a track turn more sharply, since no corner shows there. `fab` asks the design rules check for its rule `track_angle`, which refuses a corner of more than 45 degrees outside a pad or a via of the net, so a board netsmith routed has none. The check of a board file alone (the `drc` example, a port read from KiCad) leaves the rule out.

Once the board is routed and has its teardrops, each 45 degree bend where two straight tracks of one width meet becomes an arc tangent to both, a rounded bend, so the copper has no corner (issue #431, step 11): the largest radius up to four track widths that keeps the rules' space from all other copper, the teardrops, the pads, the holes, the keepouts and the edge, measured exactly, or else a smaller one down to one width; a bend that none fits keeps its corner. An arc takes at most half of each track it joins and never the part a teardrop lies along, and a bend inside a pad or a via of its net, or where another track of the net joins it, is left as it is. The search and the copper it settles are as before, so the same lines route with the same vias and teardrops. The board file writes an arc as KiCad's `(arc (start) (mid) (end))`, the Gerbers draw it as one, `track_angle` reads it at its ends along its tangents, and the scorecard counts arcs and measures length along them. Measured on the three ported boards (2026-11-13, the agent's decision under the owner's delegation): at four widths 78 % of RP2350's 391 bends, 64 % of Orbit's 250 and 77 % of the Easyduino's 186 become arcs, routed length drops by 0.3 % (RP2350 1344 to 1340 mm, Orbit 918 to 916, Easyduino 849 to 846), the check finds no new error and `fab` takes no longer; two widths round a few more bends and save less length, six round fewer. `netsmith route --arcs 0` draws none, for comparing.

### Teardrops

A board netsmith routes gets a teardrop where each track meets a pad or a via (issue #431): copper that widens the track into the pad, so that a drill that wanders or an etch that undercuts does not break the joint. It is as wide at the pad as 90 % of the pad's copper across the track, at most 2 mm, and reaches half of that beyond the pad's edge, at most 1 mm and no farther than the track runs straight, on the track's layer and net. netsmith draws one only where it keeps every space the design rules check asks of a track (to other nets' copper, holes, solder mask openings, the board edge and rule areas, and on a surface-mount pad the process's space between surface-mount pads, so a fine-pitch part's gaps stay open); elsewhere the joint has none. A teardrop no wider than the track would gain nothing and is not drawn. The board file has each as a KiCad 9 teardrop zone with its fill, so the Gerbers carry the copper, a pour of another net keeps its clearance from it, `fab`'s check measures it, and KiCad reads it as a teardrop. The language has no switch for them; the route scorecard counts no teardrop as track.

### Escape lanes round fine-pitch parts

When the router's first try leaves connections on a board with a fine-pitch part (at least 16 surface-mount pads, two of them 0.55 mm apart or nearer, on at least three sides, such as a QFN or a QFP), its second try routes the lines in the same order again with an escape lane in front of each of the part's pads (issue #431): the 1.5 mm straight out from the pad, on its layer, which other nets' tracks pay a little to use and where any via pays a lot, except the pad's own via at the lane's end. A lane is a cost and not a wall, so a line with no other way still crosses one. It keeps the part's lines from closing each other's way out with vias against its pins. A board that routes on its first try is routed as before, and a board with such a part may have one try more.

### How the router orders the nets

Every connection the router draws has a class, derived from the board with nothing written in the source (issue #416). In the order the router routes them:

1. **Escape**: a connection that must reach a plane, or that ends on a pad deep in a fine-pitch grid of pads, which has the fewest ways out.
2. **Decoupling**: a decoupling capacitor's pad to the power pin it decouples, the pair the route scorecard measures.
3. **Pair**: a net of a diff pair (see Diff pairs).
4. **Clock**: a net on a crystal (a part of the kind `Crystal`, or that provides it), or whose name has `clk`, `xin`, `xout` or `xtal` in it, but not a net with a plane or a pour (a crystal's ground).
5. **Power**: a net that carries at least 100 mA.
6. Everything else.

The order is fixed. When the router's first try leaves connections, its second try routes the lines of the first try again with the classes in turn, each class in the order it had, and every later try keeps the classes in turn too. On those tries the copper of a decoupling, pair or clock connection is never ripped up for a connection of a later class: a later connection goes round it or is left, and the pass that takes track off the pours (see Pours) leaves it where it is too. An escape or power connection keeps its place in the order but may be ripped up and routed again (a power net at its full width). The first try is the router's own order with no classes, so a board that routes on its first try is routed exactly as before. This was decided by the agent under the owner's delegation of 2026-11-13: protecting every class left a dense board with a QFN unrouted, and routing the classes in turn on every try made one try of such a board finish less often (issue #431).

### Decoupling capacitors

The router routes a decoupling capacitor (a _Decoupling pair_ of the placer, see the glossary) the textbook way, with nothing written in the source (issue #431). On the supply net, the line from the power pin the capacitor decouples (the nearest of the part's power pins on the net, if it is at most 5 mm from the capacitor's pad) goes to the capacitor's pad, whatever pad of the net is nearer the pin; the rest of the net joins as it would. On a net that is a plane, that line takes the place of the pin's own via, so the pin reaches the plane through the capacitor's via. The other net of the pair, the return, is the one with a pour or a plane (of two, or of none, the one with more pads), and is routed as before; where it is poured on an outer layer the capacitor's pad is not on (ground poured on the back of a two-layer board, the capacitor on the front), the capacitor's pad gets a via of its own to the pour once a try has routed every connection, on a track of at most 1.5 mm, unless a via of the net is within 1.5 mm of the pad already. The via is never in the way of another connection, so a board routes as completely as without it. A bulk capacitor farther than 5 mm from the pins is routed as any other part.

### Diff pairs

Two nets whose names differ only in a pair of suffixes, such as `dp` and `dm`, `D+` and `D-`, `_P` and `_N`, or `.p` and `.n`, are a diff pair, so the fields of `std::bus::Usb2` make one with nothing more said. When a try of the router has routed every connection, it routes each net of a pair again with a step within 0.5 mm of its partner's track (centre line to centre line) costing half, and keeps the new copper only where more of the net runs coupled and it is no longer (by more than 0.2 mm) and has no more vias. So a pair runs side by side where that costs nothing, and a board routes exactly as completely as before. Then the shorter net of each pair whose nets' track lengths differ by more than 0.1 mm gets a meander, a row of bumps with 45 degree corners on one side of one of its straight runs, sized to add the difference: the fewest bumps no higher than 2 mm, legs three track widths apart edge to edge (or the clearance, if larger), on the longest runs first, only where the run is more than 0.5 mm from its partner's track, at least one bump's pitch from the run's ends, and only where the meander keeps every space the design rules check asks of a track and that gap from its own net's other copper. A run with too little room takes what it can and the next the rest; a pair no run has room for keeps its skew, and `fab` says so. No other net moves. It does not yet hold a fixed gap or mirror the pads' exits (issue #431). The route scorecard reports each pair's lengths, skew and the share of its track that runs coupled.

When a try of the router has routed every connection, and after the passes for diff pairs, pours and decoupling capacitors, it takes vias out (issue #431, as the "remove vias" pass of KiCad's router does): each connection with vias is routed again with a via three times its usual cost, against all the other copper, and the new copper is kept only if it has fewer vias, is at most 2 mm longer for each via it gives up (and at most 10 % longer), and lays at most 5 mm more track for each via on another net's pour. A connection that cannot lose a via so is tried once more with the one connection its way crosses routed again after it, kept only if the two together have fewer vias on the same terms. Vias that join a plane or a pour (every via of a poured or plane net, and a decoupling capacitor's via), vias side by side on a net whose current needs them, and a diff pair's vias are left where they are. No connection is left unrouted, so a board routes as completely as before; the diff pairs' meanders, which match the lengths it leaves, the teardrops and the rounded bends come after. Measured on the three ported boards (2026-11-13, the agent's decision under the owner's delegation): RP2350 142 to 134 vias, Orbit 94 to 85, the Easyduino 105 to 98, with routed length no longer and track on the pours within 4 mm of before.

### Track widths by rail

A rail's `.min_track` is the narrowest track its nets are drawn with. It is a lower bound, written as a range like a process rule, and the bottom of what is left is the width. Both pins of the rail get it. A net takes the widest `.min_track` of the rails on it, so `GND` below is 0.8 mm without anyone saying so. `~` between rails equates the width too, so a module's rail carries it.

```nsm solve
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer
import std::sources::Rail

board Widths {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .input: Rail {
        .pos.net &= "VIN"
        .min_track &= 0.8mm..1m
    }
    .output: Rail {
        .pos.net &= "3V3"
        .min_track &= 0.5mm..1m
    }
    .in: Header1x2
    .out: Header1x2
    .input.pos ~ .in.p1
    .input.neg ~ .in.p2
    .output.pos ~ .out.p1
    .output.neg ~ .out.p2
}
```

The router draws `VIN` and `GND` at 0.8 mm and `3V3` at 0.5 mm, and the `.kicad_pro` has a net class for each width. A width under the process's least track is the process's least. Signals have no rail: widen them all with `.process.min_track`. The width is checked against the rail's current, below.

### Track width against current

A track that carries current heats. The solver knows the worst-case current each rail draws (`.current`, see Load current) and, through the series parts of a chain, the current of the nets between them, so netsmith draws every net at a width that can carry it and checks any width you state yourself (ADR 0036). It uses the IPC-2221 formula, `I = k * dT^0.44 * A^0.725`, where `I` is the current in amperes, `dT` the temperature rise in kelvin, `A` the track's cross-section in square mils (its width times the copper's thickness), and `k` is 0.048 for a track on an outer layer and 0.024 on an inner layer. netsmith draws tracks on the outer layers only, so it uses 0.048 and the outer copper of the process (`.stackup.copper_weight` ounces, 0.035 mm each).

A net carries the current that Kirchhoff's current law says it must, the way an EE adds it up (ADR 0036): the sum of the currents of the loads that draw from it (a supply net) or return through it (a ground, or any shared return), with every load on at the same time as the worst case. Two loads of 2 A and 1 A on separate supplies that share a `GND` make `GND` a 3 A net, where each supply net carries its own. The things that count are the rails with a pin on the net and the series parts (`~>` chains) whose bridge ends on it, so the nets between the parts of a chain take the chain's current. A regulator is a load on its input net and a supply on its output net, each with the current the solver worked out: a linear regulator draws what its output supplies, and a switching regulator draws `Pout / efficiency / Vin`, from the efficiency its part model states, so its input net is sized for that and not for the output current. A part model that states no efficiency leaves the input current unknown, and the net is sized only for what is known (below). The sum is pessimistic where a current is counted twice: a ground that the load of a regulator's output and the regulator's own input both return through carries the sum of the two, though the load's return goes to the regulator and only the input's goes on to the source. You do not have to state a width for it. A net no rail gives a `.min_track` is drawn at the width IPC-2221 needs for that current (rounded up to 0.01 mm): a 2 A node between a fuse and a diode is drawn at 0.98 mm with nothing written, and the check has nothing to say. The width is sized for the narrowest finished track the process's `.track_width_tolerance` allows (JLCPCB: ±20%), so the drawn width is 1/(1-tolerance) times IPC-2221's (1.25 times for JLCPCB: 2 A needs 0.78 mm by the formula and is drawn at 0.98 mm), and a stated `.min_track` is checked by what its width less the tolerance carries. A `.min_track` on a rail is an override: the net is drawn at the widest one, never narrower than the process's least track, and the check warns when IPC-2221 needs more. The temperature rise is `.process.max_temperature_rise`, in kelvin, an upper bound like `via_in_pad`; it is 10 K when the board states none, and the finding says so. 10 K is a conservative default of netsmith's and is not a number from a fabricator. The planes of a four-layer board and the nets of a pour are areas and not tracks, and are not sized or checked. The width is for the heat, not for the voltage the track drops: the width is chosen before the board is routed, and the drop depends on the routed length. It is not always small (2 A through 50 mm of a 0.98 mm track in 1 oz copper drops about 0.1 V, 2 % of 5 V), so where it matters, give the rail a wider `.min_track`.

The router keeps to that width (issue #431). Where a track of a net that carries current does not fit at its full width, it may be necked down, but never narrower than the width the current needs, except within 1 mm of the pads of a part the line ends on (the box round their centres, grown by 1 mm), where it may be the process's least track to escape between fine-pitch pads that are no wider and carry the same current. A line that would have to be narrower anywhere else is left unrouted rather than drawn too narrow. `fab` hands the design rules check each net's least width, and the check's own rule `track_current` refuses a track narrower than it away from those pads. A via carries current too: the process states its hole plating (`.hole_plating`, 18 μm at JLCPCB), the track width check rates the process's via by the copper of its barrel (IPC-2221 with the inner-layer constant: a 0.2 mm hole carries 0.56 A at 10 K), and a net that needs more than one gets that many side by side wherever it changes layer, their copper overlapping so that no track joins them. The router changes layer only where the cluster fits, and the check's rule `via_current` refuses a cluster that is short.

When a current on the net is not known (the solver leaves the currents of a net with two or more unknown ones unsolved), netsmith does not guess it, and does not ignore what is known either. The currents that are known are a lower bound on the net's current, because loads only add up: a current that is not known can make the sum larger and never smaller. So the net is drawn at least as wide as the known currents need (the process's least track or a stated `.min_track` still decides when it is wider), and `build` prints a note that says the width is a lower bound and names the net and the carriers whose current is missing. A net with no known current at all stays at the process's least track, and the note says that. A stated `.min_track` that is narrower than the known current needs is still a finding, worded "carries at least" and naming the carriers whose current is missing. The note appears while a current is missing and goes away when you state it. A net whose every current is known is sized as above and has no note.

```nsm solve notes
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail

board Unsized {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 12V
        .pos.net &= "VIN"
    }
    .load: Rail {
        .pos.net &= "VOUT"
    }
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
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 12V
        .pos.net &= "VIN"
    }
    .load: Rail {
        .pos.net &= "VOUT"
    }
    .fan: Rail {
        .pos.net &= "VOUT"
        .current &= 0A..1.2A
    }
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
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .load: Rail {
        .pos.net &= "5V"
        .current &= 0A..2A
        .min_track &= 0.2mm..1m
    }
    .source: Rail {
        .pos.net &= "5V"
        .current &= -2A..0A
    }
    .supply: Header1x2
    .out: Header1x2
    .supply.p1 ~ .source.pos
    .supply.p2 ~ .source.neg
    .out.p1 ~ .load.pos
    .out.p2 ~ .load.neg
}
```

```text
Heater: IPC-2221 guideline: net `5V` carries up to 2 A (rail `load`: 0 mA to 2 A) but its tracks are 0.2 mm wide (a rail's `.min_track`), in 1 oz (0.035 mm) outer copper; at a temperature rise of 10 K (the default) that width carries 633 mA (at its narrowest, 20% under, the fabricator's tolerance) and IPC-2221 needs 0.98 mm: give a rail on the net `.min_track &= 0.98mm..1m`, or allow a higher `.process.max_temperature_rise`
Heater: IPC-2221 guideline: net `GND` carries up to 2 A (rail `load`: 0 mA to 2 A) but its tracks are 0.2 mm wide (a rail's `.min_track`), in 1 oz (0.035 mm) outer copper; at a temperature rise of 10 K (the default) that width carries 633 mA (at its narrowest, 20% under, the fabricator's tolerance) and IPC-2221 needs 0.98 mm: give a rail on the net `.min_track &= 0.98mm..1m`, or allow a higher `.process.max_temperature_rise`
```

A ground is sized for the sum of what returns through it. Here two supplies share `GND`: the 5 V one carries 2 A, the 3.3 V one 1 A, and the `GND` track that the rail `load_a` states at 1 mm carries 2.03 A at 10 K at its narrowest (20% under, the tolerance; 2.39 A drawn exactly), enough for the 2 A of `5V` but not for the 3 A that return together:

```nsm solve warns
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Shared {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .source_a: Rail {
        .pos.net &= "5V"
        .current &= -2A..0A
    }
    .source_b: Rail {
        .pos.net &= "3V3"
        .current &= -1A..0A
    }
    .load_a: Rail {
        .pos.net &= "5V"
        .current &= 0A..2A
        .min_track &= 1mm..1m
    }
    .load_b: Rail {
        .pos.net &= "3V3"
        .current &= 0A..1A
    }
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
Shared: IPC-2221 guideline: net `GND` carries up to 3 A (the sum of the currents on it with every load on at once: rail `load_a` 2 A + rail `load_b` 1 A) but its tracks are 1 mm wide (a rail's `.min_track`), in 1 oz (0.035 mm) outer copper; at a temperature rise of 10 K (the default) that width carries 2.03 A (at its narrowest, 20% under, the fabricator's tolerance) and IPC-2221 needs 1.71 mm: give a rail on the net `.min_track &= 1.71mm..1m`, or allow a higher `.process.max_temperature_rise`
```

Rules.

- This is a guideline and not a limit of the fabricator (ADR 0030): `build` and `fab` print each finding as `warning: ...`, `fab` also writes it in `fab-notes.txt`, and the editor lists it as a warning at the rail or part that carries the current. The board still builds.
- To make a finding stop the build, put `guidelines = "error"` in the `[project]` table of `netsmith.toml`, or pass `--strict` to `build` or `fab` for one run. The message is then an error at the same line.
- IPC-2221 is a conservative design guide, and IPC-2152 is more accurate (it counts the board's thickness and the copper next to the track). netsmith implements IPC-2221 only. A track the check passes is not therefore good for a certain temperature, and one it flags may be fine; read the number as a prompt to decide.
- The router may draw a track narrower than its net's width where there is no room (the stub at a pad is the net's width, and the pad's own copper carries the rest). The check is of the width asked for.

### The outline and cutouts

The member `.outline` of a board's physical file (in its `case` section) is the board edge: a rectangle with rounded corners, or a polygon (_A polygon outline_, below). A range is a limit, and the placer takes the smallest board that holds the parts inside it: `.width &= 0mm..60mm` is at most 60 mm. A single value is exactly that size. Without an `.outline` the board is as big as its parts need. A `Cutout` is a rectangular hole inside the outline, with an optional corner radius. Its corner nearest the origin is `.x`, `.y`, or the layout file's entry for its name; stating both is an error.

```nsm solve
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

board Shaped {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .header: Header1x2
}
```

```nsm-physical
import std::pcb::{Cutout, Outline}

physical for Shaped {
    case {
        .outline: Outline {
            .width &= 60mm
            .height &= 0mm..40mm
            .corner_radius &= 3mm
        }
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

(The numbers are invented for the example.) The placer packs and turns the parts in the outline (a part wider than the board is turned, unless its requirements state a rotation) and refuses only if that ends with courtyards over each other or parts outside, so the message leads with the parts that are wrong: how many, every part that is outside the outline or inside its margin with how far, and every pair of courtyards that overlap with its area. It does not give the size the parts need, except when the courtyards alone add up to more than both faces of the outline hold, which no arrangement can fix (then it says so, with the areas). The message then lists the ten pairs of courtyards that overlap the most (the names, the shared area in mm2 and its size, where it is from the outline's top left corner and the side of the outline it is nearest) and the ten parts furthest out (each with how far outside which edge, or inside the margin, or into which cutout), and names all the others. `fab` also writes every part's final position, turn, face and state to `placement-failure.txt` in the board's output folder and names the file in the message. There is no `--json` output for it yet.

When the enclosure is a DXF, `netsmith dxf import` writes the outline, the cutouts and the mounting holes for you (see `netsmith dxf import --help`): the members for the `case` section, the holes as `MountingHole` parts, and their places, which come from the case and not from the placer, as requirements in the same file (`.hole1.place.x &= 5mm`). A hole whose place is stated is not put in a corner.

Mounting holes, plated holes and fiducials go in the corners of a fixed outline, holes first, clockwise from the top left, and clear of a rounded corner. A board with no outline lays them out with the other parts. A part in a rounded corner, over a cutout or outside the outline is found by the placement check and named.

Misuse.

```nsm fails
import std::mfg::jlcpcb::TwoLayer

board Tiny {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
}
```

```nsm-physical
import std::pcb::Outline

physical for Tiny {
    case {
        .outline: Outline {
            .width &= 3mm
            .height &= 40mm
            .corner_radius &= 2mm
        }
    }
}
```

```text
error: Tiny: `outline.corner_radius` is 2 mm, more than half of the shape's shorter side (3 mm)
```

#### A polygon outline

An edge that is not a rectangle with four equal corners (an L, a slanted side, corners of different radii) is a `polygon` statement in the outline's block: the corners in order, each `(x, y)` in the board's frame, the edge running from each to the next and from the last back to the first. `round R` after a corner is a tangent fillet of radius R there. `arc through (x, y)` after a corner makes the edge to the next corner an arc through that point, as KiCad's `gr_arc` gives one by its start, middle and end. A long run goes on over lines that begin with `(`, `round` or `arc`. This is the form KiCad's `Edge.Cuts`, IPC-2581's `Polygon` and the ODB++ profile give an outline (owner decision 2026-11-12, issue #57).

```nsm solve
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

board Ell {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
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
            polygon (0mm, 0mm) (25mm, 0mm) round 2mm (25mm, 15mm)
                    (40mm, 15mm) (40mm, 30mm) arc through (20mm, 34mm) (0mm, 30mm) round 3mm
        }
    }
}
```

The box round the edge has its top left at the board's origin, so the smallest x and y of the corners (and of an arc's bulge) are 0mm. The polygon is the whole edge: its box is the board's width and height, so `.width`, `.height` and `.corner_radius` are not stated beside it. Every value is a fixed length, the number the case drawing gives: a range, a tolerance or a value read from another field is refused. The placer keeps the parts inside the polygon and `fab` checks them against it; `Edge.Cuts`, the Gerbers and `board3d --step` carry its lines and arcs, and the editor draws it. The router routes inside the polygon, round a notch and along an arc, and keeps its copper the process's copper to edge space from the polygon's lines and arcs; the design rules check holds the same space. `netsmith dxf import` writes a `polygon` for an outline that is not a rounded rectangle.

Misuse: a size beside the polygon, and a corner that is a range.

```nsm fails
import std::mfg::jlcpcb::TwoLayer

board Sized {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
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
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
}
```

```nsm-physical
import std::pcb::Outline

physical for Loose {
    case {
        .outline: Outline {
            polygon (0mm, 0mm) (40mm +/- 0.1mm, 0mm) (40mm, 30mm) (0mm, 30mm)
        }
    }
}
```

```text
error: a corner of a `polygon` is one value, and `40mm +/- 0.1mm` is a range (39.9mm..40.1mm): the outline is what the case drawing says, so write the one number
  --> main.physical.nsm:6:33
  |
6 |             polygon (0mm, 0mm) (40mm +/- 0.1mm, 0mm) (40mm, 30mm) (0mm, 30mm)
  |                                 ^^^^^^^^^^^^^^
```

Misuse: a fillet with no room on its edge.

```nsm fails
import std::mfg::jlcpcb::TwoLayer

board Tight {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
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

The process has a pour on each outer layer, `.top` and `.bottom`. A pour with a net is copper filled round the tracks, joined to its net with KiCad's thermal reliefs; a pour with no net is no pour. `.stitch_pitch` is the most space between the vias that join the pour to the other layers of its net (an inner plane, or the pour on the other side): the tool uses the top of the range and puts the vias on a grid after routing, in what the routing leaves free.

```nsm solve
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::FourLayer

board Poured {
    .process: FourLayer {
        .inner1.net &= "GND"
        .inner2.net &= "3V3"
        .top.net &= "GND"
        .bottom.net &= "GND"
        .top.stitch_pitch &= 0mm..8mm
        .bottom.stitch_pitch &= 0mm..8mm
    }
    .in: Header1x2
    .out: Header1x2
    .in.p1 ~ .out.p1
    .in.p1.net &= "3V3"
    .in.p2 ~ .out.p2
    .in.p2.net &= "GND"
}
```

A pour on a net the board does not have is an error that lists the nets, as for a plane. The pads of the pour's net keep the via the router puts beside them; the pour is KiCad's to fill (`fab` has it fill the zones before it checks and plots the board).

Track of other nets on a pour cuts it, and a pour on one side of a two-layer board is the return path of the signals over it. So when a try of the router has routed every connection, and one outer layer is poured on a net while the other is not, the router routes each line with track on that pour again, the most first, with a step on the pour costing twice its length and a via 60 mm of track, and keeps the new copper only where it costs less so counted (issue #431). A track keeps to the other layer where the way round is short, and a board routes exactly as completely as before. The nets of a diff pair (see Diff pairs) are left where the pair's own pass put them. Where both outer layers are poured, as on a board with ground on both, nothing moves. The route scorecard reports how much other nets' track each pour has and how many pieces it falls into.

### Keepouts

A `Keepout` is an area where something is not allowed. Four fields say what, each `"forbid"` or `"allow"`, and one that states none forbids all four (the language has no boolean, so the words stand for one): `.tracks`, `.vias`, `.copper` (the pours are not filled there) and `.parts` (no part's courtyard, and no pad). `.layers` is `"all"` (the default) or `"top"`, `"bottom"`, `"inner1"`, `"inner2"`... (as many as the board has inner layers) separated by commas. The router, the stitching vias and the placement check keep to it, and the board file has it as a KiCad rule area, so KiCad's check and fill do too.

A keepout is a member of a board's physical file, with its place in the board's frame, or a member of a part, in the part's own frame (the footprint's, turned with the part), so that a part carries its own no-copper zone. A part states it relative to itself and nothing else; the keepouts a board draws are physical intent and are in its physical file (a keepout in a board's circuit source is an error that says so).

```nsm evaluate
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer
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
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
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
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
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

### Height limits

**Height limits (ADR 0045).** A board that fits in plan view can still hit a lid, a bracket or a neighbouring board. A part states how tall its body stands with the built-in member `.height` (one value, from the face of the board it is on to the top of its body; the board's physical file may narrow it for the board), and a `Keepout` in the physical file states the tallest a part may stand where it is with `.max_height`. A keepout that states `.max_height` and none of `.tracks`, `.vias`, `.copper` and `.parts` forbids nothing else: it only limits height. `.layers` says which face it limits (`"top"` for the parts on the top face, `"bottom"` for the parts on the bottom, `"all"` for both), so a part on the back has its own limit: two keepouts over the same area, one for each face.

A part that states no `.height` has the `height` cell of the catalog row it was picked as, if the row has one (ADR 0094): a catalog file's `height` is one length that a datasheet or a drawing gives, the greatest it allows (the maximum, or the nominal plus the tolerance), with the document, its revision and the page in the row's `source`. A height that the design states wins, whatever the row says, and the row's height is never an input of the pick: no row is chosen or refused for it. For a board that has a height limit, `fab-notes.txt` says how many parts have a known height and names the row each catalog height was read from. A part with neither may have the _Standard height_ of its footprint (ADR 0106): the greatest height its package's JEDEC outline allows, for the generated footprints a maker's drawing of that outline was read for (`SOIC-8_3.9x4.9mm_P1.27mm` is 1.75 mm, MS-012; the list and the drawings are in `crates/netsmith-pcb/src/standard_height.rs`; chip passives and QFNs have none). It is never the part's `.height`: where it fits a limit, the part passes and `fab` and `fab-notes.txt` say that the pass rests on the standard (``note: `u1` (U1) fits the 4.00 mm the keepout `lid` allows on the top face by its package standard ...``); where it does not, the part is unchecked, as below, with the standard's figure in the message, because a part in that package may be lower. A part with none of the three has no height: it is not taken as flat and not guessed. Where a limit covers its body, `fab` and the editor print a warning that names it (``height of `r3` (R3) is unchecked ...``), and `fab` writes it in `fab-notes.txt`. With `constraints = "error"` in `netsmith.toml`, or `fab --constraints error`, it is an error that stops `fab` instead (`--constraints warn` wins over the manifest, and `--allow-drc-violations` does not waive it). A part that does state one is kept out from under a zone it is too tall for by the placer, and the placement check names it, with the numbers, if it stands there anyway. A zone a part carries (a `Keepout` member of a part, in the part's own frame) is kept clear of the same way, wherever the part ends up and however it is turned: the placer keeps the other parts out of it, and the check measures it against every part but the one that carries it. The footprint's 3D model is not read, and a part's own STEP model (`.model`, see _A part's STEP model_) is drawn and never gives a height.

```nsm solve place
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Ribbed {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .rail: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
    }
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
import std::pcb::{Outline, Keepout}

physical for Ribbed {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 30mm
        }
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

Misuse: a height is one value, above or at zero, and a limit is above zero.

```nsm fails
import std::parts::passive::Resistor
import std::mfg::jlcpcb::TwoLayer

board Vague {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
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

### Pin protrusion

**Readings (ADR 0045, section 15); the name `.pin_protrusion` and the rule were decided by the owner on 2026-11-04.** On a tight board a flat part sits on one face over the pins of a through-hole part that come through from the other face. The pins are cut after soldering, so what protrudes is lower than the flat part's underside. A part has the built-in member `.pin_protrusion` (a length): how far the pins of a through-hole part protrude out of the far side of the board, whether they were cut or bought short. The board's physical file states it for one part, as it states `.height`, and a path may reach a part inside a module (`.radio.row_a.pin_protrusion &= 0.2mm`). A part that states none has an unknown protrusion, which is not zero: its pads stay an obstacle for a part on that face, as before.

A part on that face may stand over the pins when all of these hold, and `fab` says which does not:

- it is fitted by hand, after the pins are cut (`.assembly` is `"hand"` or `"none"`; a part the board house assembles is on the board before any pin is soldered);
- it states its `.height`;
- the protrusion plus its height fits the height limit that a keepout puts on that face where its body is (the pins protrude `t`, the part rests on them, so `t + height` must not be more than `.max_height`). With no limit there, nothing is compared, and `fab` warns of that.

Its own pads still keep the process's clearance from the pads of the through-hole part. Here the socket's pins protrude 0.2 mm, the regulator stands 1.8 mm tall and the lid leaves 2.2 mm, so it places over them:

```nsm solve place
import std::parts::connector::Header1x2
import std::parts::regulator::LinearRegulator
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Stacked {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .output: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .current &= 0mA..50mA
    }
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
        .outline: Outline {
            .width &= 24mm
            .height &= 16mm
        }
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
import std::parts::connector::Header1x2
import std::parts::regulator::LinearRegulator
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Stacked {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .output: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .current &= 0mA..50mA
    }
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
        .outline: Outline {
            .width &= 24mm
            .height &= 16mm
        }
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
  these lines cannot both hold: main.physical.nsm:22 `.socket.place.x &= 12mm +/- 0.02mm`; main.physical.nsm:23 `.socket.place.y &= 8mm +/- 0.02mm`; main.physical.nsm:21 `.socket.place.rotation &= 0`; main.physical.nsm:26 `.reg.place.x &= 14.75mm +/- 0.02mm`; main.physical.nsm:27 `.reg.place.y &= 8mm +/- 0.5mm`; main.physical.nsm:25 `.reg.place.rotation &= 0`; main.physical.nsm:24 `.reg.place.side &= "bottom"`; main.physical.nsm:29 `.socket.pin_protrusion &= 0.6mm`: they keep the courtyard of `reg` (U1) within x 12.83 to 16.67, y 5.8 to 10.2 mm, over a pad of `socket` (J1) (pins protruding 0.6 mm) by at least 0.02 x 0.775 mm wherever it is put, and the protrusion does not excuse that: `reg` (U1) stands 1.80 mm tall on pins that protrude 0.60 mm: 2.40 mm from the bottom face, and the keepout `back_room` allows 2.20 mm there (0.20 mm too much); cut the pins to protrude 0.40 mm or less, or use a lower part
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
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
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

**A part may be drawn from a STEP model of the project (ADR 0108).** Every part has the built-in member `.model`, of the prelude's interface `StepModel` (in scope everywhere, no import). `.model.file` names a STEP file of the project, relative to the file that states it; `render`, the `.glb`, the editor's 3D view and the STEP export draw it in place of the part's _Package shape_, and `board3d --json` and `fab-notes.txt` name it with its SHA-256. It is stated where `.height` is: a part's definition states it once for every use of the part, and a board or its physical file states it for one part. The model is placed at the footprint's origin, on the part's face, turned with the part and mirrored on the bottom face. `.model.rx`, `.ry` and `.rz` turn it, in degrees about x, then y, then z, and `.model.x`, `.y` and `.z` then move it, so that the numbers of a KiCad footprint's `(model ... (offset (xyz ...)) (rotate (xyz ...)))` line can be copied in. The frame is KiCad's for a model: x along the footprint's x, y up the footprint as the footprint editor shows it (the footprint's own y runs down), z out of the part's face, and a positive angle turns clockwise seen from the axis's positive end, as KiCad turns a model (`docs/BOARD3D.md` has the order and the test that pins it). Each is one value, and an unstated one is zero. netsmith never bundles a model and never downloads one.

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

An offset or a rotation with no file is refused, as there is nothing to place, and so is a rotation or an offset that is a range.

### A case model

**A board may name the STEP file of its case (ADR 0109).** A `CaseModel` (`std::pcb::CaseModel`) is declared in the `case { }` section of the board's physical file, one member for each file: a base and a lid are two members, or one file with two solids. `.file` names a STEP file of the project, relative to the physical file. It has the fields and the rotation order of a part's `StepModel` (_A part's STEP model_), and places the model in the 3D scene's frame (`docs/BOARD3D.md`: millimetres, z up, the origin at the outline's bottom-left corner on the top face): `.rx`, `.ry` and `.rz` turn it, in degrees about x, then y, then z, and `.x`, `.y` and `.z` then move it. Each is one value, and an unstated one is zero. The side up is a rotation and the gap an offset: a box drawn upside down in its CAD file that the board's bottom face rests in is turned half a turn about x and moved down by the board's thickness.

```nsm solve
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

board Boxed {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .power: Header1x2
}
```

```nsm-physical
import std::pcb::{CaseModel, Outline}

physical for Boxed {
    case {
        // Case drawing BX-2, sheet 1: the inside of the base.
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
        // The base, modelled opening down: turned over, and the board rests on its bosses.
        .shell: CaseModel {
            .file &= "case/base.step"
            .rx &= 180
            .z &= -1.6mm
        }
        .lid: CaseModel {
            .file &= "case/lid.step"
        }
    }
}
```

A case model is a drawing and a check, never a height, an outline or a hole: the outline, the cutouts, the mounting holes and the height limits stay statements of the `case { }` section (by hand or with `netsmith dxf import`), and the check says where they disagree with the model. The STEP export leaves the case out. netsmith never bundles a case model and never downloads one.

**The fit check.** `fab`, `board3d` and the editor read each file with netsmith's own STEP reader, place it and measure the board against its solids: each part's body box to its height (stated, catalog or the package standard's), the pins of a part that states how far they protrude (`.pin_protrusion`, a box under its plated pads out to that length), and the board's own solid. The findings, each with its numbers, the part and the solid (`docs/BOARD3D.md`, _The case model's fit_, has the table):

- `case_collision`: a body, stated pins or the board inside a case solid by more than 0.01 mm. Touching is contact, so a board resting on a boss is fine. For a part of a stated or catalog height, for stated pins, and for the board, it is an error that stops `fab`, and neither `--allow-drc-violations` nor `--constraints warn` waives it, as a part too tall for a height limit is.
- `case_clearance`: a body or stated pins nearer a case surface than `.clearance`, a warning.
- `case_reach`: a connector at the board's edge with the case between its face and the outside, or its face more than `.reach` inside the wall's outer face, a warning.
- `case_boss`: a mounting hole with no case solid within `.boss_gap` above or below its ring, a warning.
- `case_unchecked`: a part of unknown height under the case, a part of standard height that collides (a part in that package may be lower), or a solid whose shell is not closed, which gets no test for a body wholly inside it. A warning, and an error with strict constraints.

The three figures are fields of `CaseModel` that std offers with `?=` (`.clearance` 0.5 mm, `.reach` 1.5 mm, `.boss_gap` 0.5 mm, PROVISIONAL): a board narrows one for its case, `.shell.clearance &= 1mm`, and a negative one is refused. A file netsmith's reader cannot read stops `fab`, as a path outside the project does. `build` and `check` place nothing, so they do not measure the fit.

```text
error: Boxed: case_collision: J1 (`power`), 9.00 mm tall (stated), is inside the solid `lid` of the case model `lid` by 1.00 mm at (16.74, 9.91, 8.00): the case and the board collide; a collision with the case stops `fab`, and neither `--allow-drc-violations` nor `--constraints warn` waives it: move or lower the part, or correct the case model's placement (`.x`, `.y`, `.z`, `.rx`, `.ry`, `.rz`)
```

The file is found as a part's model is: a path outside the project, an absolute path, or one stated in two folders that names two files is an error. A maker's license often keeps a case's file out of a public repository, so a file that is not there leaves the fit unchecked: a warning from `build`, `check` and `fab`, and an error that stops `fab` when constraints are strict (`constraints = "error"` in `netsmith.toml`, or `fab --constraints error`; `--allow-drc-violations` does not waive it), as a part of unknown height under a limit is:

```text
warning: Boxed: `shell.file` names `src/case/base.step` (src/boxed.physical.nsm:12), and there is no such file, so the board's fit in its case model `shell` is not checked; add the file, or leave it out on a machine that may not have it
```

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
    .shell: CaseModel {
        .file &= "case/base.step"
    }
}
```

```text
error: a case model belongs in the `case { }` section of `physical for Loose`: wrap `.shell: CaseModel` in `case { ... }`
  --> main.physical.nsm:4:13
  |
4 |     .shell: CaseModel {
  |             ^^^^^^^^^
```

### Labels

A `Label` is silkscreen text: a part with no pins that is fitted by nobody, placed like any other part, and declared in the `case` section of the board's physical file. `.text` is what it prints and `.text_height` how tall (it was `.height` before the part height of _Height limits_ took that name); unset, the process's least text height, and never less. The reference designator of a label is `LBL1`, and it is not in the schematic.

```nsm evaluate
import std::mfg::jlcpcb::TwoLayer

board Marked {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
}
```

```nsm-physical
import std::parts::mechanical::Label

physical for Marked {
    case {
        .revision: Label {
            .text &= "REV A"
            .text_height &= 1.2mm
        }
    }
}
```

### Where a part goes

A case, a panel or a carrier fixes where some parts must be: a connector's mouth at a slot, a button under its cap. Every part and every module has a built-in member `.place` (ADR 0045) that holds the requirements. It is narrowed with the ordinary operators, like `.assembly`: `&=` for a value or a range (`2mm +/- 0.1mm`, `1mm..2mm`), `>=` and `<=` for a bound or an expression, such as half the outline's height. A tolerance needs a constant on both sides, so a window round an expression is two bounds. Declaring `.place` yourself is an error. Order never matters.

The lines are in the board's physical file (ADR 0048), in the unit `physical for Board`: a board's circuit source that narrows a `.place` is an error that says so. A part inside a module is named by its path, `.sensor.j.place.y`. All the examples below are two blocks, the circuit source and its physical file.

| Field | Meaning |
|---|---|
| `.x`, `.y` | the centre of the part's body |
| `.left`, `.right`, `.top`, `.bottom` | the edges of the body's box, after the part is turned |
| `.rotation` | degrees counterclockwise on screen, as in the layout file; a plain number, a multiple of 90 |
| `.side` | `"top"` or `"bottom"`; on a module it is a requirement on every part of it |

Positions are lengths in the board's frame (the outline's top left corner is the origin, x right, y down), and they are measured from the **body**: the outline the footprint draws on its fabrication layer (`F.Fab`), else its courtyard, else its pads and 0.25 mm (the courtyard the placer gives a footprint that draws none; a courtyard takes in every hole that reaches past it, and every pad that does with 0.15 mm round it, since the placer keeps courtyards apart and lets them touch). The box is found in the footprint's own frame and turned with the part, and the placer and the check measure the same one. The case states the shell of a connector, not the origin the footprint's author chose, so a requirement survives a change of package. The layout file's `at` stays the footprint's origin; the check converts, and every finding says which drawing it used. A field the board leaves alone is unconstrained.

**Putting a footprint's origin somewhere.** A port must put a footprint's origin where the original's is, and a requirement measures the body. The two are a fixed distance apart for a part on one face and turned one way. `fab` prints it for every part after the line `wrote ...`, and `fab --format json` gives it in the `parts` of each board (`origin`, `body_centre` and `offset`, in mm from the outline's top left corner):

```text
where each footprint's origin is, in mm from the outline's top left corner (y down), and how far it is from the centre of the body that `.place.x` and `.place.y` measure: to put the origin at (x, y) with the part on that face and turned so, write `.place.x &= x - dx` and `.place.y &= y - dy`:
  J1 (`power_in`) top, turned 90: origin (10.73, 9), body centre (12, 9), dx -1.27 dy 0
```

Set the face and the turn first, run `fab` once, and write `.place.x` and `.place.y` from the line: for the origin at (10, 9), `.power_in.place.x &= 11.27mm` and `.power_in.place.y &= 9mm`. `netsmith parity` writes the two lines out for each part that must match, in an `origin` line of its report (and in `origins` with `--format json`), with the figures of the original's origin.

```nsm solve
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

module Sensor {
    .j: Header1x2
}

board Cased {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .power: Header1x2
    .sensor: Sensor
}
```

```nsm-physical
import std::pcb::Outline

physical for Cased {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
    }

    // Case drawing, slot A: the header's body is 1.5 mm in from the left edge, turned, and
    // centred along it.
    .power.place.left     &= 1.5mm +/- 0.1mm
    .power.place.y        >= .outline.height / 2 - 0.1mm
    .power.place.y        <= .outline.height / 2 + 0.1mm
    .power.place.rotation &= 90
    .power.place.side     &= "top"

    // The whole sensor block stays right of 20 mm.
    .sensor.place.left >= 20mm
    // The sensor's header, a part inside the module, is named by its path.
    .sensor.j.place.y &= 8mm +/- 0.2mm
}
```

The requirements are hard. A requirement that leaves one place and one turn (every field a single value) fixes the part like `lock = true` in the layout file. Any other puts the part in a window the placer keeps to, and it gives the module's parts the half of the module's requirements that keeps them in (a least `.left` or `.top`, a most `.right` or `.bottom`). A part the layout file locks is checked against them before anything is placed; a part it merely moves is measured afterwards. The layout file keeps the solution; the physical file keeps only the requirement, never a solved position.

`netsmith build` refuses what can be seen without a placement: bounds that leave nothing, a rotation that is not a multiple of 90, a side that is neither `"top"` nor `"bottom"`, and a requirement measured from an outline that has no fixed size.

```nsm fails
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

board Squeezed {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
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
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

board Stretchy {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .j: Header1x2
}
```

```nsm-physical
import std::pcb::Outline

physical for Stretchy {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 20mm..30mm
        }
    }
    .j.place.y >= .outline.height / 2
}
```

```text
error: Stretchy: `j.place.y` is measured from the outline's top left corner, and `outline.height` does not state one size, so the board has no fixed outline; give `outline.height` a single value (`&= 30mm`), or state the position without it
```

#### A relation between two parts

Some requirements are between two parts and fix neither: two rows of sockets a datasheet puts 17.5 mm apart, an antenna header that is on the line of its cable. They are written as the equation of the fields, in the physical file, and they may reach into modules by path, at any depth. A relation is the measure of one part's body minus the measure of another's, along one axis, in a window. `==` with a range is the window (`17.5mm +/- 0.05mm`, or `.a.place.y == .b.place.y` for a window of zero), and `>=` and `<=` give a one-sided window.

```nsm solve place
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

module Radio {
    .row_a: Header1x2
    .row_b: Header1x2
}

board Paired {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .radio: Radio
    .antenna: Header1x2
}
```

```nsm-physical
import std::pcb::Outline

physical for Paired {
    case {
        .outline: Outline {
            .width &= 50mm
            .height &= 30mm
        }
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
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

module Pads {
    .tp1: Header1x2
    .tp2: Header1x2
    .tp3: Header1x2
    .tp4: Header1x2
}

board Probed {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .pads: Pads
}
```

```nsm-physical
import std::pcb::Outline

physical for Probed {
    case {
        .outline: Outline {
            .width &= 50mm
            .height &= 30mm
        }
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
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

module Pads {
    .tp1: Header1x2
    .tp2: Header1x2
    .tp4: Header1x2
}

board Gappy {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
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

#### A part that hangs over the board edge

A plug that sticks out of the case has part of its courtyard outside the outline. That is an error ("not wholly inside the board outline") unless the design says the part may hang over that edge. **The owner decided the rule on 2026-11-03 (ADR 0045, section 10); how a module states an overhang for its parts is still open.** The design says it with a requirement it can already write: a window for an edge of the **body** that lies at or beyond the same edge of the outline. `.j.place.left &= -0.5mm +/- 0.1mm` puts the body's left edge 0.5 mm past the board's left edge, and `.plug.place.bottom >= .outline.height` puts its bottom edge on the board's bottom edge or past it (flush counts). Only the part's own `left`, `right`, `top` and `bottom` say it, and only when the whole window is at or beyond the edge. A window on `.x` or `.y`, a bound a module states for its parts, or an entry of the layout file puts the part somewhere but does not say it may hang over, so the courtyard past the edge is the error it always was. That holds for a part the layout file locks (`lock = true`) too: the lock fixes where the part is, and the part must still state every edge its courtyard is past, like any other (owner decision, 2026-11-03). A locked part past an edge nobody stated gets the error below, naming the edge, the distance and the line that would allow it.

```nsm solve place
import std::parts::connector::Header1x4
import std::mfg::jlcpcb::TwoLayer

board Plugged {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .j: Header1x4
}
```

```nsm-physical
import std::pcb::Outline

physical for Plugged {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
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
import std::parts::connector::Header1x4
import std::mfg::jlcpcb::TwoLayer

board Plugged {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .j: Header1x4
}
```

```nsm-physical
import std::pcb::Outline

physical for Plugged {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
    }
    .j.place.x &= 1.3mm
}
```

```text
error: Plugged: `j` (J1) is not wholly inside the board outline: its courtyard is at least 0.50 mm past the left edge wherever its requirements put it, and the design does not say it may hang over an edge; to allow it past the left edge, require the body to reach it, for example `.j.place.left <= 0mm`; the lines that place it: main.physical.nsm:10 `.j.place.x &= 1.3mm`
```

The next states an overhang that is too much: the header's pads end up 0.05 mm from the board edge, nearer than the process allows.

```nsm solve place refused
import std::parts::connector::Header1x4
import std::mfg::jlcpcb::TwoLayer

board Plugged {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .j: Header1x4
}
```

```nsm-physical
import std::pcb::Outline

physical for Plugged {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
    }
    .j.place.left &= -1mm +/- 0.1mm
}
```

```text
error: Plugged: a pad of `j` (J1) is 0.05 mm from the board edge
```

The last states the left edge, and its centre along it puts it past the top edge too:

```nsm solve place refused
import std::parts::connector::Header1x4
import std::mfg::jlcpcb::TwoLayer

board Plugged {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .j: Header1x4
}
```

```nsm-physical
import std::pcb::Outline

physical for Plugged {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
    }
    .j.place.left &= -0.5mm +/- 0.1mm
    .j.place.y &= 5.1mm
}
```

```text
error: Plugged: `j` (J1) is not wholly inside the board outline: its courtyard is 0.52 mm past the top edge, and the design says it may hang over only the left edge (`j.place.left (main.physical.nsm:10)` allows -0.6 to -0.4 mm); to allow it past the top edge, require the body to reach it, for example `.j.place.top <= 0mm`
```

`netsmith fab` measures every requirement on the board it made and prints a finding for each one not met, with the measured value, the window allowed and how far over it is. A violated requirement is an error that stops `fab`, like a design rules error; it is not a guideline, so `guidelines` and `--strict` do not govern it. `constraints = "warn"` under `[project]` in `netsmith.toml`, or `--constraints warn` for one run, lists them as warnings instead (`"error"` is the default). `fab --constraints error|warn` wins over the manifest, so `--constraints error` stops `fab` even when the manifest says `warn`. `--allow-drc-violations` waives design rules, not requirements: with it, an unmet requirement still stops `fab` ("does not waive them"). The notes of the package count the requirements met, and say how many were not and that they are only warnings. Output of the check for a header moved by hand (this block is not run by the guide test):

```text
error [constraint] constraint power.place.left (cased.physical.nsm:13) is violated: the left edge of the body of J1 (F.Fab) is 28.73 mm from the left edge of the outline, and it allows 1.4 to 1.6 mm (27.13 mm over)
```

The editor lists the same findings with the placement's, a relation between two parts among them (_A relation between two parts_); it does not yet draw the windows or a line between the two parts (ADR 0045, C4). Height limits are in _Height limits_ above.

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

Parts are keyed by their path in the design, which a renumbered reference designator does not change. A member of a group is keyed by its path with the index, quoted as TOML needs: `[Board.parts."decoupling[2]"]`. A part with no entry is placed by the placer (ADR 0033): it derives a placement from the circuit alone, keeping the parts of a module together, a decoupling capacitor next to the power pins it bridges, connectors at the edges and wire short, and places them round the parts the file places. The same circuit, file and `seed` always give the same board; another `seed` gives another arrangement. When the file's seed (0 if it states none) does not place the board, the placer tries the seeds after it (the file's seed plus 1, plus 2 and so on), up to `place_seeds` in `netsmith.toml` in all (8 by default): a seed places the board when the parts fit the outline, the placement check finds no fault that another seed could move (one that names no part the file places, and names a part no requirement pins) and, with no layout file, every requirement of the physical file is met. `fab` says which seed placed the board (`placed on a 101.2 by 24 mm board with seed 15`); when it is not the file's, it says what blocked each seed tried, calls the file's seed stale if the file states one, and writes the new seed to the file. With `--locked` it writes nothing and stops with an error, as for a pick lock; an order (`fab --order`) and the editor's release are releases, which never change a choice by themselves, so a seed that would change stops them with the seed to write by running `fab`. `board3d` and the editor's PCB view use the seed they find and write nothing. When no seed places the board, `fab` stops with the closest try (the fewest faults, then the least area and distance; the earlier seed on a tie) and what blocks it, each member by its path with the designator beside it, and by how much: `no seed places the board: the placer tried 8 seeds, 3 to 10 (...), and the closest is seed 7, where this blocks it:` then `` the courtyards of `usb` (J2) and `vbus_2u2` (C37) overlap by 0.04 mm2 ``. A build puts every part that has an entry where the file says; the placer places the others round the parts that are locked (`lock = true`) and does not react to the others, so moving a part by hand moves nothing else. `place_subset` places some parts and leaves the rest, but no command asks for it yet. An entry for a part, keepout or cutout that the board does not have, or a table for a board the file does not have, is an error that names it and lists what exists. A keepout or cutout that states `.x` and `.y` in the physical file and has an entry as well is an error: a thing has one place. `side = "bottom"` puts a part on the back. `examples/outline_and_pours.nsm`, its physical file `examples/outline_and_pours.physical.nsm` and `examples/outline_and_pours.pcb.toml` use all of this.

`source` says who wrote an entry and changes nothing, with one exception: two parts whose entries `netsmith routes import` wrote (`source = "import"`, see The routes file) may share courtyard area. They stand where a KiCad board has them, and that board may have courtyards that overlap (ORBIT's LQFP-48 covers two of its capacitors), or footprints that draw no courtyard, for which netsmith takes the pads and 0.25 mm; the check before placing does not count the pair as a conflict, and `fab` says each such overlap as a warning (``the courtyards of `mcu` (U4) and `v3v3_100n[2]` (C16) overlap by 1.65 mm2: both stand where the imported KiCad board has them ...``) instead of stopping. Their pads are still checked by the design rules check, as any copper is.

### The routes file

Copper that a person, an agent or an import decided is in `<stem>.routes.toml` beside the source file and its layout file: `board.nsm` has `board.routes.toml` (issue #599; ADR 0030 sketched it). The file is optional: with none, every board is routed as before, byte for byte. Whatever reads the layout file reads it too: `fab`, the editor's PCB view, and `netsmith route` through the placed board `fab --save-placed` saves. It is written by `netsmith routes` (below), never by `fab`, and committed with the source.

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

Places are in mm from the outline's top left corner, y down, as in the layout file, and are written to 0.000001 mm, a KiCad file's precision. A track is straight from `from` to `to`, or an arc through `mid`; `layer` is `top` or `bottom` (tracks are drawn on the outer layers only, as the router's are); `width`, `diameter` and `drill` are in mm. Each shape is on a line of its own, so a change to one shape is a change to one line. An unknown key, another `version`, a shape that is wrong in itself (a width of zero, a drill as wide as its via, a zone outline of two corners, a zone on a layer the board does not have) or a table for a board the source does not have is an error that names the file.

A zone is KiCad's zone of the net (issue #724): copper filled inside `outline`, less each of `holes`, on each of its `layers` (`top`, `bottom`, `inner1`, `inner2`, ...), with the settings of KiCad's zone that change the copper. `priority` says which of two overlapping zones of a layer has the area: the higher one, as in KiCad (0 when not given); a zone gives up the outline of a higher zone of its net, and the fill of a higher zone of another net grown by the larger of the two clearances, and two zones of different nets with the same priority whose outlines overlap are an error of `fab`, because KiCad would choose between them by an identifier and not by the design. `clearance` is the space from other nets' copper, `min_width` KiCad's minimum thickness, `pads` how it joins its net's pads (`solid`, `thermal`, the default, with a gap of `thermal_gap` crossed by four spokes `spoke_width` wide, `none`, or `thru_hole_only`, thermal round a through-hole pad and solid to a surface pad), and `islands` which unjoined pieces KiCad removes (`remove`, the default, `keep`, or `remove_below` with `island_min_area` in mm²). Sizes are in mm.

A layer that has a zone of the file takes its zones from the file alone: the design's pour or plane on that layer is not drawn (the process still says the layer's net, and the fab notes say the layer has the routes file's zones instead), and the other layers keep theirs. The board file holds each zone as KiCad's, unfilled, so that KiCad's fill makes the copper of the board the zone came from. netsmith's own fill, which the Gerbers, the design rules check and the ratsnest use, keeps every clearance and makes the thermal reliefs, but removes no island and trims no sliver narrower than `min_width`. A zone is no obstacle to the router: its fill keeps its clearance from whatever the router draws, as KiCad's does. Its fill joins what of its net it reaches, whether the net is locked or not: each piece joins the pads of its net with copper on its layer that it covers (a thermal relief's spokes reach the pad's centre) and a pad whose copper overlaps one of those, the vias it covers, and the tracks of its layer that run into it, so a net a zone joins needs no track between those pads.

`fab` puts every shape of a record on the placed board before the router runs. The router keeps it as copper of its net, never moves or removes it, and routes only the connections it leaves: a track that ends on another track's middle, a track drawn over a pad of its net, two tracks that cross and a via on a track count as joined, as KiCad counts them. The passes after the router (meanders, rounded bends, parallel vias, stitching, teardrops) leave it as it is, and the design rules check checks it with the rest of the copper. In the board files it comes first: a board drawn from a routes file has the copper of the board it was imported from in another order, so compare two boards' copper as sets.

`lock = true` on a net says its copper is the file's alone: the router adds no track, via or teardrop to it, and the ratsnest counts the board's outer pours of the net as joining what of the net they touch, as the design rules check does (a zone joins what its fill reaches on any net, above). A connection a locked net still lacks is left unrouted, and `fab` says so as it says any connection left. `lock = true` on a shape (a track, a via or a zone) keeps the shape when its record no longer matches the board, and `netsmith routes delete` refuses it.

A record that no longer matches the board is said, never used silently. `pads` is a hash of the net's pads: each pad's part by its path in the design (a renumbered reference designator does not change it), its number, its centre to 1 µm and its copper layers. When a pad of the net moved, or one joined or left it, `fab` warns, drops the record's unlocked shapes and routes the net again for that run (the net's lock does not hold); its locked shapes are kept. A record for a net the board does not have (a net renamed, say) is a warning and is drawn not at all. `netsmith routes lock` writes a record again with the pads as they are, and `netsmith routes delete` removes it.

`netsmith routes` keeps the file (`netsmith routes --help` has every flag):

- `import BOARD.kicad_pcb` writes the file from a KiCad board, and the layout file's places of the parts it has (locked, `source = "import"`; not those the physical file pins, and none with `--no-places`). A KiCad net is the design's net that most of its pads are on, the parts paired by the port's parts file (`reference/parts.toml` of the project, or `--parts`) or else by reference designator, as the parity checker pairs them. It takes every track, arc and through via of the outer layers and every zone of copper (on any copper layer, with its outline, holes and settings, written to 0.000001 mm), locks every net (`--unlocked` does not), and says what it leaves out: copper on an inner layer, a via that is not a through via, copper of a net on no pad of the design, a rule area (the design's keepouts are its own), a teardrop (`fab` draws its own), a zone filled with hatching (netsmith fills solid zones only). A zone with smoothed corners is taken with its outline's corners as they are, and said. A part with two pads that the KiCad board numbers the other way round from the design's footprint (a resistor drawn turned in one of the two schematics: the pads of the other parts vouch for the KiCad net of its pad 1 being the design's net of its pad 2, and the other way round) is placed turned by half a turn, when the footprint is the same so turned, so that each pad stands where the KiCad board has the pad of its net; the import names it.
- `add-track`, `add-via` and `add-zone` add a shape to a net, and `delete`, `lock` and `unlock` act on a net's record or on one of its shapes (`--track N`, `--via N`, `--zone N`, numbered from 1 as `show` lists them). An added shape, or a record that `lock` writes again, is refused when it brings an error of the design rules check that the board did not have (a short, a crossing, a clearance), and the file is left as it was; an end that touches nothing yet is not refused, because a route is drawn one shape at a time (`fab`'s check still finds one left so). A locked net or shape is not deleted until it is unlocked.
- `add-zone --net GND --layer bottom --rect 1,1,6,4` draws a zone of a net, without an import. The outline is a polygon (`--outline X,Y` for each corner, in order), a rectangle (`--rect X,Y,W,H`, the top left corner and the size) or the board's own outline (`--board-outline`), in the same places as `add-track`'s; `--layer` is repeated for a zone on several layers. A setting it is not given is the design's own plane's: the clearance the process's planes keep, a minimum width of 0.25 mm, a thermal relief with a gap and spokes of 0.5 mm, priority 0 (`--clearance`, `--min-width`, `--thermal-gap`, `--spoke-width`, `--priority`, `--pads` and `--lock` set them). A zone is no short where it covers another net's pad or track, because its fill keeps its clearance round it, so it is not refused for that. It is refused when a corner is off the board, a setting is wrong or less than the process makes, two zones of different nets with the same priority overlap, or the design rules check finds a new error with it. A layer that gets its first zone of the file stops drawing the design's pour or plane there, and the command says so. A refusal names its places from the outline's top left corner, as the command takes them, not as the KiCad board file has them.
- `show` prints each record with its shapes numbered, what of the file is not used, and the connections the router would still route.

Each command builds and places the board as `fab` does, and routes nothing. It exits 0 when it did what it was asked, 1 when it refused, and 2 when it could not run.

The editor's PCB view marks the file's copper over the board (a solid line on a locked net, dashed on one the router may add to, a padlock on a locked shape), lists its records in the Routes tab of the side panel, and locks, unlocks and deletes a net's record or one shape (`L`, `Delete`, or the inspector) with the same code as `netsmith routes`, refused for the same reasons. It draws copper too: the track tool (`X`) starts a track on a pad, a track's end or a via, takes the net's width from the board's rules and the layer from the toolbar, adds a corner at each click, bending the segment round the copper in the way in corners of 45 degrees at most (the _Walkaround_, with the spaces of the design rules check; where there is none the preview says so and the click is refused; on a routed board the router's copper is no obstacle, since an edit throws it away, and the preview shows the router's copper the track crosses faded, with a note that routing again lays it round the new track), and ends it with a double-click or Enter; the via tool (`V`) places the process's via on a net; and a drag moves a fixed track's end or a via, with the shapes of the net that meet there. A track or via that adds an error of the design rules check is refused with the check's errors, as `add-track` refuses it, and the file is not changed. Every change to the file is one step of the view's undo (Ctrl+Z, Ctrl+Shift+Z), which puts the net's record back as it was. A record that is not used as written is a warning in its findings.

#### Reproducing a KiCad board

To draw a board that a KiCad file already routes, import its copper and places, then run `fab`: in the project, `netsmith routes import reference/board.kicad_pcb` and `netsmith fab`. The parts the import places stand where the KiCad board has them, every net is locked (a net the KiCad board joins by a zone alone too, since its record holds the zone), and the board's zones replace the design's pours and planes on their layers, so the router adds no copper. What `fab` still lacks is said as any connection left: a pad that netsmith's footprint draws otherwise than the KiCad board's, mostly (a ground pad the KiCad footprint joins to another layer by plated holes of its own, say). A part that must hang over an edge, as a module's antenna does, needs its requirement in the physical file (`.mcu.place.top <= 0mm`), because a place in the layout file allows no overhang.

Three ports keep such a reproduction beside their own build, so that the board the placer and router make (the automatic board, the example as it is) can be compared with the original's as they improve: `examples/rp2350_minimal`, `examples/orbit` and `examples/easyduino_esp32s3` each have the imported routes file and layout file in `reference/agent/`, not in `src/`, so their own build is untouched (`scripts/reproductions --update` writes them again). The tests of `crates/netsmith-cli/tests/reproduce.rs` put those files into `src/` of a copy, run `fab` and require every track, arc, via and zone of the original on the board it writes, on the same layer and net (a zone with the same outline, holes and settings), no connection left but those the test names and no copper the original does not have; each example's README says what the reproduction lacks and why. To draw the parts as the original does, a port draws each with the original's footprint where netsmith's differs, written out of the KiCad board (`cargo run -p netsmith-tools --example footprint_from_board -- BOARD.kicad_pcb REF libs/<name>.pretty`) and named by a `[[part]]` or `[[member]]` row of `kicad.toml` (_One part with a footprint of its own_); the tests compare every footprint of the reproduction with the original's (place, turn, face, pads and their nets, courtyard) and require each error of the design rules check to be one the original has itself.

### The pick lock

Which catalog row a part was picked as is the solver's choice: the first row, in catalog order, that fits and passes every relation. A change to a catalog, to the order of its rows or to the solver can move a pick without a change to the source. `<stem>.lock.toml` beside the source file (`board.nsm` has `board.lock.toml`) keeps the choice: `build` and `fab` write it when it does not exist, and commit it as you commit `Cargo.lock`.

```toml
version = 1

[LedExample.parts."r1"]
catalog = "resistor"
row = "330Ω 1% 0402 C25104"
lcsc = "C25104"
mpn = "0402WGF3300TCE"
```

A use of a generic with more than one provider in scope is locked too, at the provider it was picked as (ADR 0113): `[LedExample.providers."process"]` with `generic = "TwoLayer"` and `provider = "std::mfg::jlcpcb::JlcTwoLayer1oz"`, written before the board's parts, because the provider decides which parts there are. The use is made with that provider and no other is tried: a board that now asks for what another provider has (another copper weight) stops on the contradiction with a note that names the lock, and `lock --update` picks again and prints the change (`` LedExample: `process`: provider `...JlcTwoLayer1oz` -> `...JlcTwoLayer2oz` ``). A locked provider that is no longer in scope is an error, as a row that is gone is.

There is one table for each board (or system) of the file, keyed by its name, and one for each part, keyed by its path (`r1`, `stage.output_cap`; a bank of capacitors is one entry, its path without an index). `catalog` and `row` say which row. `lcsc` and `mpn` are there to find a row that was renamed or changed in place, and may be left out. The file holds no stock, no price and no solved value; a generic part is locked at the row the solver gave it (`generic 22uF 10% 0603 ...`), and `fab` still resolves it to an orderable row against the stock of the day (ADR 0047).

With a lock, the solver offers each part the lock names only its locked row, and judges that row by everything the design states and every relation, as it judges any pick. What happens next is told for each part:

- **The row still passes.** It is the pick. Nothing is printed.
- **The row no longer passes, or is no longer in the catalog.** The run stops with the relation that fails and its numbers, and a second message says that the row is the lock's and that no other row was tried (`` `r1` is locked to the row `2.2kΩ 1% 0402 C25879` of the `resistor` catalog by led.lock.toml, and no other row was tried for it ``). Change the design or the catalog so the row fits, or delete the lock to pick again. A row whose `lcsc` or `mpn` is no longer the lock's is the same case.
- **A part has no entry, or an entry belongs to a part the board no longer has.** The run goes on and prints a note that names the part. The lock is not rewritten: an existing lock is never changed by `build` or `fab`.

`--locked` (on `build` and `fab`, as Cargo's, and so on `fab --order`) turns the last case, and a missing file, into an error, before anything is written. A CI job that builds with `--locked` fails a pull request that changes the picks without changing the lock.

#### Refreshing the lock

A lock that is old is refreshed on purpose, with `netsmith lock --update [file.nsm]` (every top-level file of the project when no file is named). It solves each board of the file again as if there were no lock, so every part is free to take the row the solver picks now, and writes the lock with those rows. It prints what changed, a line for each part in the order board, part path, with the row's LCSC and manufacturer part numbers, and nothing at all when the lock is already as it would write it (then the file is not touched):

```sh
$ netsmith lock --update
updated src/main.lock.toml
  Zz9Board: `u`: `ZZ9-33-S` (lcsc C9001, mpn ZZ9-33-S) -> `ZZ9-33-A` (lcsc C9002, mpn ZZ9-33-A)
```

Each case of the list above ends in a line of that diff:

- **A part the lock has no entry for** is added (`LedExample: `r2`: added as ...`), and the entries of the other parts are as they were unless their own pick moved. With no lock file, every part is added and the file is written.
- **An entry for a part that is gone** is dropped (`LedExample: `r2`: dropped (it was ..., and is no longer a part of the board)`).
- **A row that no longer passes, or that no catalog has**, is not replaced silently. `lock` first solves the board held to the rows of the file, as `build` does, and prints what the solver said about them, in its own words, to stderr after `note:` (the relation that fails and its numbers, or that the catalog has no such row); `--update` then replaces the row with the one the solver picks now, and the diff says which.
- **A row that still passes but is no longer the first that fits** (a catalog gained an earlier row, or the solver changed) is replaced too: that is what a refresh is for, and the diff is how the change is reviewed.

`netsmith lock --check [file.nsm]` does the same and writes nothing. It prints the same diff and exits 1 when the lock is not current or does not exist (with a message that says which), and exits 0 and prints nothing when it is current, so a CI job can run it beside `build --locked`. Both exit 2 when they cannot tell: the lock is not a lock this netsmith reads, or the design does not build even with no lock. Two runs on the same project print the same text and write the same bytes. The pin file is not read for its writing: `lock` writes only the lock, and a pin request that has no pin yet is assigned in memory, as the editor does.

`fab --order <name>` does with each source file that builds a board of the order what `fab` does with that file, so the order gets the rows `fab` of the file alone gets: it holds the boards of the file to its lock, writes the lock when the file does not exist, and never rewrites one that does (a difference is a note). Only the files that build a board of the order are read, and a lock covers all the boards of its file, whichever of them the order takes. `fab --order <name> --locked` compares the lock of each of those files with the picks before it removes or writes anything, and refuses, naming each file, one that does not exist or that the picks differ from. An order is a release, and a release never moves a choice by itself: it moves no pin (the pin file) and no row of an existing lock.

The editor does not hold the parts to the lock: it solves from the source, as before. It shows the lock of the selected board's source file and writes nothing: the schematic's inspector has a _Pick lock_ section that says whether the file exists and whether it is current, and lists the diff that `lock --check` would print (a source that does not build has none). It is read again when the lock file changes on disk, which the editor watches.

### A hand pick

A generic part is picked as the first row that fits, and a port of an existing design, or a bill of materials someone else wrote, often names another row that fits as well: a 50 V capacitor where the solver would take a 16 V one. The design then states the row's number on the part, and the part is held to that row (a hand pick, ADR 0122). Every part has two members for it, as it has `.designator`: `.lcsc`, the supplier's part number, and `.mpn`, the manufacturer's. Both are text, and a narrowing of either is the hand pick:

```nsm evaluate
import std::parts::passive::{Resistor, Led}
import std::sources::Rail

board HandPicked {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .led: Led { .current &= 5mA..15mA }
    // The solver would take a 330 ohm 0402 resistor; the bill of materials names this one.
    .r1: Resistor { .lcsc &= "C23151" }
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
```

The solver offers the part only the row with that number (and, with both, the row that has both), and judges it by everything the design states and every relation, as it judges a locked row. A hand pick comes before the pick lock: the lock records what the solver picked, and a hand pick is a requirement. A board whose hand pick differs from its lock builds with the hand-picked row and says that the lock differs (`--locked` makes that an error), and `lock --update` records the new row. A part inside a module is picked from the board as any member inside it is narrowed (`.stage.output_cap.lcsc &= "C307331"`, ADR 0116). A part whose catalog has a column `mpn` and lists `.mpn` in its `catalog` statement (`catalog mcu by .mpn`, above) names its row the same way; its `.mpn: String` declares the member every part has, and may be left out.

What happens next is told for each part:

- **The row passes.** It is the pick.
- **The row fails a relation.** The run stops with the relation and its numbers, and a second message at the narrowing says that no other row was tried (`` `r1` is held by `.lcsc &= "C25879"` to one row of the `resistor` catalog, and no other row was tried for it ``).
- **The catalog has no row with the number.** The run stops at the narrowing:

```nsm fails
import std::parts::passive::{Resistor, Led}
import std::sources::Rail

board NoSuchRow {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .led: Led { .current &= 5mA..15mA }
    .r1: Resistor { .lcsc &= "C0000000" }
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
```

```text
error: `r1` is held by `.lcsc &= "C0000000"` to a row of the `resistor` catalog, and the catalog has no row with the LCSC number C0000000: no other row was tried for it; change the narrowing to a number the catalog has, or remove it to let the solver pick
   --> main.nsm:11:21
   |
11 |     .r1: Resistor { .lcsc &= "C0000000" }
   |                     ^^^^^^^^^^^^^^^^^^^
```

Two narrowings of one part's `.lcsc` that differ are a `contradiction`, as any two narrowings of one text are, and `.lcsc` on a part that picks from no catalog (a `Switch`) is an error at the narrowing. The editor solves with the hand picks too, since they are the source.

netsmith once read hand picks from a file beside the source, `<stem>.picks.toml`. It no longer does: a source with that file beside it stops `build`, `fab`, `lock` and the editor with an error that lists the narrowing to write for each entry, by board (`.boot1_cap.lcsc &= "C307331"` in `board PowerManifoldCharger`); write them, and delete the file.
