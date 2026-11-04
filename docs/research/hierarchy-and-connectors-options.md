# Hierarchical modules and connectors: options

Options note for issue #16. Written while the owner was away. It decides nothing and builds nothing. Every construct marked `nsm-new` is invented to compare options; snippets marked `nsm-fragment` are pieces of a file that use only constructs that exist today (they were not run). Language-design choices are the owner's (`AGENTS.md`). Vocabulary follows `GLOSSARY.md`. All parts, boards and numbers are made up.

Issue #16 lists six things:

1. Modules as sheets of their board.
2. Flat cable and wire-to-board connectors.
3. Edge connectors.
4. Partial mates, isolated grounds, pin 1 and stack height.
5. Several supplying ports on one join are checked pessimistically.

Sources for what exists: `docs/LANGUAGE.md` ("Modules", "Systems and ports", "Connectors that mate"), ADR 0004, 0005, 0010, 0021, 0040, 0042, 0045, `README.md` (Known limits), `docs/ROADMAP.md` ("Systems: what is left"), `std/parts/connector.nsm`.

## Summary

- **Sheets.** Three ways to say which module instances become their own schematic sheet. I lean to the board deciding, with a built-in member and a manifest default, because that is how `.assembly` and `.place` already work. The export design (one KiCad file per instance or per definition) matters more than the syntax and needs a decision.
- **Connectors.** Three ways. Parameters on the connector members cover partial mates and edge connectors with no new construct. A harness or cable definition is the only one that covers flat cable and wire-to-board, and it changes what a `system` holds (ADR 0010). A contact map on the mate covers all but is verbose. I recommend the first now and the harness only when a design needs a cable.
- **Isolated grounds, pin 1, stack height.** Each is a small decision of its own. Part heights now exist (ADR 0045 section 11), which ADR 0040 said they did not, so the stack height can be checked sooner than planned.
- **Pessimistic supplies.** What the checker does today is the right rule for redundant supplies and the wrong one for supplies that share a load. The fix is a statement of which one a join is. Two forms are below.

## 1. Modules as sheets

### Today

A module is flattened into its board. `docs/CONCEPTS.md` says so ("Hierarchical sheets ... drawing it as its own schematic sheet is open"). The schematic is "a grid of cells with labels and no wires" (ADR 0021, Limits). A module's internal nets are named from its instance path, since only a board names nets (ADR 0005). A board reaches a module only through its interface (ADR 0004), and what a module offers it by tying values is the pattern of ADR 0042.

### What a sheet has to answer

- Which instances are sheets. A pull-up pair should not be one; a buck stage should be.
- What the sheet's pins are. The module's interfaces and pins are its interface, so they are the natural sheet pins, with the port's name as the label.
- One file per instance, or one file per definition used several times. KiCad allows both: each sheet is its own `.kicad_sch`, and one sheet can be used several times ([KiCad schematic editor manual](https://docs.kicad.org/9.0/en/eeschema/eeschema.html)). Per definition is smaller and reads like the source. Per instance is simpler for references, because netsmith numbers designators across the whole board by instance path (`GLOSSARY.md`, _Designator_).
- Net names. A sheet's inner nets would keep their instance-path names. Horizon EDA says that on flattening "the topmost net's name and class are used" ([schematic editor](https://docs.horizon-eda.org/en/latest/imp-sch.html)); netsmith's board-names-the-nets rule gives the same answer without a special case.
- `netsmith compare`: it reads a `.kicad_sch` through `kicad-cli`, which "finds its other sheets only in the root sheet's directory" (README, Known limits). Sheets must be written beside the root, or compare keeps failing on hierarchical output.

### Option S1. Every module instance is a sheet, no syntax

```nsm-fragment
// Today's source, unchanged.
board Lamp {
    .supply: Rail { .voltage &= 5V +/- 5% }
    .lamp: Indicator
    .supply ~ .lamp.supply
}
```

The exporter writes `Lamp.kicad_sch` with a sheet symbol for `lamp`, and `lamp.kicad_sch` below it.

- **Pros.** No language change, no ADR on syntax. Existing boards get sheets for free.
- **Cons.** No control. Every `I2cPullups` becomes a sheet of three parts. A board with 30 module instances gets 30 sheets. The owner cannot say "draw this flat".
- **Editor and agents.** The schematic view needs a way to open a sheet. Agents write nothing new.

### Option S2. The module's author decides

```nsm-new
/// A buck converter stage. Drawn on a sheet of its own.
module BuckStage {
    .sheet &= "own"
    .vin: Rail
    .vout: Rail
    // ... parts
}
```

- **Pros.** A module that is big knows it is. A library module carries the right default everywhere it is used.
- **Cons.** A decision about a drawing is stated in a part of the circuit that is about electrons. A board that wants the stage flat has no way. ADR 0002 says library parts are source without tool names; a drawing hint is close to that line.
- **Editor and agents.** None beyond S1. A module author must know.

### Option S3. The board decides, with a manifest default

```nsm-new
board Lamp {
    .lamp: Indicator
    .stage: BuckStage
    .stage.sheet &= "Power stage"     // own sheet, titled
    // .lamp has no .sheet: drawn on the board's sheet
}
```

```toml
# netsmith.toml
[project]
sheets = "modules"        # default for instances that state nothing: "board" or "modules"
```

`.sheet` would be a built-in member of every module instance, in the way `.assembly` and `.place` are. A string, because the language has no boolean (`GLOSSARY.md`, _Keepout_; `docs/LANGUAGE.md`). The manifest key follows ADR 0028, where the manifest gives the default for assembly.

- **Pros.** The same pattern as two existing decisions, so little to learn. The board owner keeps control. A heading for the sheet can be stated.
- **Cons.** A built-in member on every module, another word in the vocabulary. A sheet title and a "sheet or not" choice share a member, which is slightly two things (ADR 0001 says one way to say a thing, so they would need to be one field or two).
- **Editor.** The schematic view lists sheets and opens one. "Make this a sheet" in the editor writes one line, as "pin here" would for a constraint.
- **Agents.** One more member to learn; `netsmith-write-a-module` would say "state a sheet for a module of more than a screen".

### Recommendation

S3, because it follows two existing patterns, and with a decision first on per-instance or per-definition files. I would not do S1 alone, since forcing every module into a sheet gives noisy output at once. Whatever is chosen, write the sheets beside the root file so `compare` works. This is the owner's decision.

### Questions

1. Which modules should be sheets by default: none, all, or those over a size?
2. One file per instance or per definition? (Per definition means a module placed twice shows as the same sheet twice, with different references.)
3. Does the sheet's name come from the instance path, a stated title, or the module's name?
4. Is a doc comment on a module a good source for the sheet title, or must it be a field?

## 2. Connectors

### Today

A system joins two `.mating` with `~`. `Mating` has `kind`, `gender`, `pitch`, `rows`, `positions`, `pin_current`, `keyed`. Only `pin_header` is accepted. The checker pairs contact N with contact N, reads a role from each board, and derives the port joins the contacts carry (ADR 0040).

```nsm-fragment
// examples/carrier_and_cards.nsm, the system.
system CarrierAndCards {
    .carrier: Carrier
    .sensor: SensorCard
    .serial: SerialCard
    .carrier.slot_a.mating ~ .sensor.j1.mating
    .carrier.slot_b.mating ~ .serial.j1.mating
}
```

A 1x8 header in a 1x10 socket is refused, as is anything but a header with a socket of the same pitch and rows. A system holds boards and nothing else.

### What is missing

| Need | Why today's mate cannot say it |
|---|---|
| Partial mate | Contact N meets contact N, so a 1x8 header would have to land on contacts 1 to 8. A header in the middle of a socket is a different statement. |
| Edge connector | ADR 0040 planned `kind "edge_connector"` and a `card_thickness` check against the plug-in's `process.stackup.thickness`. Not built. |
| Flat cable | Type A and type B cables reverse the contact order, so a cable is a part of the join, not a property of one end. |
| Wire to board | The wires have a colour, a length and a gauge, and no connector at one end. |

### Option C1. Parameters on the connector members, no new construct

The board states, on its own connector, what the connector needs the mate to know.

```nsm-new
board Card {
    .j1: Header1x8
    // Lands on socket contacts 3 to 10 of a 1x10 socket.
    .j1.mating.first_contact &= 3
}

board Carrier {
    .slot: Socket1x10
}

system Kit {
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j1.mating      // unchanged
}
```

```nsm-new
board Backplane {
    .slot: EdgeSocket
    .slot.mating.card_thickness &= 1.6mm +/- 0.1mm
}
// The card's .process.stackup.thickness is checked against it. No new syntax.
```

- **Pros.** Reuses `~` and the `.mating` the reach rule (ADR 0040 R2) already gives the system. Edge connectors are almost free: they are another `kind` and one more field. Partial mates are one number.
- **Cons.** `first_contact` is a fact about a pairing but sits on one end. Two cards in one 1x20 socket would both state it, and the checker must catch an overlap. It does not help cables.
- **Editor.** The System view already draws mates with contacts coloured by role; a partial mate draws fewer.
- **Agents.** One field on the header. Easy to get wrong (off by one), so the checker's message must name both contacts.

### Option C2. A harness or cable definition in the system

A third thing a system can hold: something with two ends and a rule about how contacts map.

```nsm-new
harness Ribbon {
    .a: Header2x5Idc
    .b: Header2x5Idc
    .wiring &= "reversed"        // type B: contact N at one end is contact 11 - N at the other
}

harness SensorLead {
    // Wire to board: one end is bare wire, with a colour for each contact.
    .a: Header1x4
    .colours &= "red black white green"
}

system Rig {
    .ctrl: Controller
    .io: IoBoard
    .ribbon: Ribbon
    .ctrl.j2.mating ~ .ribbon.a.mating
    .ribbon.b.mating ~ .io.j1.mating
}
```

- **Pros.** The only option that models a cable honestly and a wire-to-board lead. It is the general answer ADR 0040 called A4 and said A1 is a special case of. A cable can have its own ratings and a bill-of-materials line.
- **Cons.** Changes ADR 0010 ("a system holds boards and nothing else"), adds a definition kind, a glossary entry and an ADR, and needs a data source for cables. A harness has parts but no copper of its own, a new kind of thing next to _Board_ and _Panel_. Wire-to-board needs an unconnected end, which the pin kinds (ADR 0016) do not say.
- **Editor.** The System view needs a node for the cable. The bill of materials needs cable lines.
- **Agents.** A new concept to explain. Worth it only for a design that has a cable.

### Option C3. A contact map on the mate

```nsm-new
system Rig {
    .ctrl: Controller
    .io: IoBoard
    .ctrl.j2.mating ~ .io.j1.mating { reversed }
    .ctrl.j3.mating ~ .io.j2.mating { 1 -> 3, 2 -> 4, 3 -> 5, 4 -> 6 }
}
```

- **Pros.** Covers partial mates and reversal with one construct, and the system names contacts, not nets, so ADR 0005 still holds. No cable object.
- **Cons.** ADR 0040 rejected pin by pin in a system (A3) because it is a net by another name, and a map is close to that. A block after `~` is new syntax in a language where `~` takes none. Ten contacts, ten pairs. Wire to board is not covered.
- **Editor and agents.** Verbose and easy to mistype; agents will need a worked example in the skill.

### Recommendation

C1 for partial mates and edge connectors, because it costs the least and fits the reach rule. Leave flat cable and wire-to-board until a design has one, then decide between C2 and C3 with that design in hand. C2 is the better model and the heavier change; C3 would need to be a stopgap that C2 later replaces, which ADR 0001 discourages. This is the owner's decision.

### Questions

1. Does the owner have a design now that needs a flat cable or a wire-to-board lead? If not, leave both.
2. May the system hold something other than boards (a harness)?
3. For a partial mate: is the fact "which contacts" best stated on the header, on the socket, or at the mate?
4. Should an edge connector be a `kind` of `Mating`, or its own interface?

## 3. Isolated grounds

### Today

Each board keeps its own ground net. A mate that carries power or a signal must carry a ground contact that is ground on both boards, and the system reports ground groups (`GLOSSARY.md`, _Ground group_). A design with a barrier (a digital isolator, an isolated DC-DC) has two grounds on purpose and no ground contact across the barrier.

### Option G1. A mark on the rail

```nsm-new
board Isolated {
    .field_gnd: Rail { .isolated &= "from GND" }
}
```

- **Pros.** The rail already means a supply with a negative pin. The solver already says "not evaluated" when two logic lines have different grounds (ADR 0046).
- **Cons.** A rail has no word for what it is isolated from. A string stands in for a relation.

### Option G2. A statement on the mate

```nsm-new
system Rig {
    .main.link.mating ~ .field.link.mating
    .main.link.mating.isolation &= 2.5kV     // no ground contact needed; the rating is checked
}
```

- **Pros.** Isolation is a property of the join between two boards, which is where it is stated. A rating can be compared with the barrier part's own.
- **Cons.** The barrier is a part on a board, so this states in the system a fact the board's parts already imply. ADR 0001 would ask which one is the statement.

### Recommendation

Do not build until a real design needs it, and then start with G2 narrowed to "this mate has no ground contact on purpose". The first question is whether the owner has one.

### Questions

1. Is there an isolated design on the roadmap?
2. What does the checker verify across an isolation barrier: nothing, a rating, or only that no ground contact is shared?

## 4. Pin 1 and stack height

### What changed since ADR 0040

ADR 0040 says stack height "needs part heights, which the library does not carry". Part heights now exist: a part's `.height` and a keepout's `.max_height` (ADR 0045 section 11, provisional). The layout file holds each part's face. ADR 0045 C7 plans a mating transform in the carrier's frame that depends on the layout of both boards (ADR 0040 M2). So the data for a first check is coming, though no check exists.

### Option P1. Pin 1 as a requirement

The position of pin 1 is the position of the connector, which `.place` can already state on both boards. A check compares the two for the plug-in's frame in the carrier's frame (C7).

```nsm-fragment
board Carrier {
    .slot: Socket1x10
    // Case drawing: the socket's body is 3 mm from the left edge, centred.
    .slot.place.left &= 3mm +/- 0.1mm
}
```

- **Pros.** No new construct; reuses requirements. Closes the question of "pin 1 at the wrong end" by checking the footprint against the stated placement.
- **Cons.** Depends on C7 and on both layout files existing. A footprint drawn with pin 1 at the other end still passes unless the check reads the pad positions.

### Option P2. A mated height on the connector

```nsm-new
part Socket1x10 {
    // ... pins
    .mating: Mating {
        .mated_height &= 8.5mm +/- 0.2mm     // board to board, a datasheet number
    }
}
```

A check then compares the tallest part facing the gap on each board with the mated height minus the other board's thickness, using `.height`.

- **Pros.** A datasheet number per connector row, in the catalog as ADR 0040 planned. Parts without a `.height` are reported by name, as for height limits.
- **Cons.** Two catalog columns to source (`mated_height` and the connector's own height). It assumes the connector faces are known: a header on the plug-in's bottom face over a socket on the carrier's top face stacks, anything else is a right-angle mate and needs `.mount` (ADR 0040). The check is only as good as the heights stated.

### Recommendation

P2 with P1 as its base, in that order, after the owner has settled the height-limit questions in issue #29 (the same `.height` is used). Sourcing the `mated_height` of a connector needs a datasheet and a recorded `source`, per `data/jlcpcb/README.md`.

### Questions

1. Should a missing `.height` make a stack check an error, as `constraints = "error"` does for a height limit?
2. Is `.mount` (vertical or right angle) a field of `Mating`, or derived from the faces of the two connectors?

## 5. Several supplying ports on one join

### Today

ADR 0010: "The system balance treats several supplying ports on one join as if each had to cover the whole demand alone." That is a pessimistic reading, and it is exactly the right rule for redundant supplies, where any one may fail. It is wrong for two supplies that share a load: a 1.5 A draw met by two 1 A supplies is refused.

```nsm-fragment
system Rig {
    .psu_a: Supply          // .output can give 0 to 1 A
    .psu_b: Supply
    .load: LoadBoard        // .input draws 1.5 A
    .psu_a.output ~ .load.input
    .psu_b.output ~ .load.input
}
```

### Option J1. A role on the join

```nsm-new
system Rig {
    .psu_a.output ~ .load.input
    .psu_b.output ~ .load.input
    .load.input.supplied_by &= "shared"        // or "redundant" (today's behaviour)
}
```

With `"shared"` the supplies' maxima must sum to at least the demand; the minima need not. `"redundant"` is the present rule and the default.

- **Pros.** The default stays safe. One word on the load says which design this is. The existing balance machinery is reused with a different rule for the supply side.
- **Cons.** A load's input states how it is supplied, which is not the load's property. A shared supply with no current sharing (no balancing resistors or ideal diodes) is not safe in hardware, and the checker would accept it.

### Option J2. A share on each supply

```nsm-new
system Rig {
    .psu_a.output ~ .load.input
    .psu_b.output ~ .load.input
    .psu_a.output.share &= 50%
    .psu_b.output.share &= 50%
}
```

Each supply must cover its share. Shares must sum to 100%.

- **Pros.** Explicit, per supply, and checks each one against what it is meant to carry.
- **Cons.** A hardware split is rarely a fixed percent. It adds a number the designer must invent, which the language otherwise avoids (the solver derives, the design states constraints).

### Recommendation

J1. It is one word, keeps today's behaviour as the default, and names the design's intent instead of inventing a split. Add a note that the check does not know whether the hardware shares current. This is the owner's decision.

### Questions

1. Does a design now have two supplies on one load, and which kind?
2. Should `"shared"` require a part that the library marks as a current-sharing element, or is the statement enough?

## 6. Order I would do them in

1. Decide S1 to S3, since the schematic is what the owner sees first, and the export layout (per instance or definition) can be built without more syntax.
2. C1 (partial mates and edge connectors) with the `card_thickness` check, which needs no geometry.
3. J1 (supply role), a small, self-contained change.
4. P2 after issue #29 settles heights.
5. Cables, wire-to-board and isolated grounds when a design needs one. Ask first.
