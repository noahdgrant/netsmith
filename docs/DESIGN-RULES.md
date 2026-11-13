# Design rules

The rules that decide what the language accepts. A change to syntax or semantics is checked
against them first (`GLOSSARY.md` has the vocabulary, `docs/adr/` the decisions behind them).

- **One correct way.** If two spellings mean the same thing, one of them goes. A
  supplied rail is a plain `Rail`, not a source part wrapping one; a pin's pads are in the
  part's `pins` table, not also a `.pad` field or a catalog column.
- **Statement order never matters.** A block means the same thing in any order.
- **Libraries state only what is always true.** A design narrows the rest, so a part
  never pre-names a net or fixes a value a design might need to change.
- **A module is configured through its interface.** Its rails, pins and declared values
  can be joined and set from outside; a value inside it can only be narrowed, with `&=`
  (`.stage.divider.rtop.resistance &= 100kohm +/- 1%`, ADR 0116). The one join inside is to
  the pins of the IC that the user named for a type parameter (`.reg.ic.pg` of
  `Buck<Lt8641>`, ADR 0119).
- **A system joins ports and mates connectors, and nothing else.** A `system` holds boards and
  says which of their ports meet (`.main.output ~ .io.input`), or which connectors are mated
  (`.carrier.slot.mating ~ .card.j1.mating`, contact N to contact N), which derives the ports
  the connectors carry. A port is an interface a board declares directly; what is inside a
  board stays private, except the `.mating` of a connector (ADR 0004, ADR 0040). A system
  names no nets (ADR 0005), and a net never crosses boards (ADR 0010): each board keeps its
  own ground, and the system reports which grounds the mates join.
- **A board is made by a process it imports.** Design rules and the stackup are the `.process`
  member of the board (`import std::mfg::jlcpcb::TwoLayer`), not tool constants. A board
  narrows them like any value (`.process.min_track &= 0.15mm..1m`); a board with no process
  has a schematic but no PCB.
- **A part number needs a source.** No LCSC number goes into a catalog from memory, and no
  derating curve either.
- **A part states its own requirements.** What a datasheet says a part needs and draws (its
  supply range, its current, its ratings, what it asks of its neighbours) is stated by the
  part, with the page cited. A design states only what is specific to it, and never a load or
  a rating that a part's datasheet already bounds. An assumption on the board is the last
  resort, when no datasheet bounds the value.
- **The user chooses the level of abstraction.** A design can name a specific part
  (`AcmeCoax200`) or a generic that picks a part from the requirements put on it
  (`CoaxReceptacle`), and can use a std module (`Buck<Lt8641>`) or build the circuit from parts by
  hand. The layers are part, then generic that picks a part, then module; each level is
  complete on its own (ADR 0068, stage 1 implemented).
