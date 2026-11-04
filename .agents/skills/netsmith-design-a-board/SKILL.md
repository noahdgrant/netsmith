---
name: netsmith-design-a-board
description: Design a circuit board with netsmith from requirements to a fab package. Use when asked to create, extend or finish a board in a netsmith project (netsmith.toml, src/*.nsm).
---

# Design a board with netsmith

This skill assumes only the `netsmith` command and the project. Entry point: the project's `AGENTS.md`. The language guide (`docs/netsmith/LANGUAGE.md`; `docs/LANGUAGE.md` in netsmith's own repository) defines the syntax. Read its quick start and the sections you use.

## Steps

1. Write the requirements as numbers before any code: supply voltage with tolerance, each load's current range, connectors, number of layers, the factory. Ask the user for what is missing. Do not guess a voltage or a current.
2. Start the project: `netsmith init` in an empty directory. It writes `netsmith.toml`, `src/main.nsm` (a working LED board), `.gitignore`, `README.md`, `AGENTS.md`, these skills and the guide. In an existing project, skip this.
3. Replace `src/main.nsm` with a board that states the requirements. Say what each part must achieve, as ranges. Do not name part numbers. The solver picks real parts from its catalogs.
4. Give the board a process (`.process: TwoLayer { .stackup.thickness &= 1.6mm }`) and name the nets on the board, never in a module.
5. Say what the board looks like only if the requirements do: a fixed size and corners (`.outline: Outline`), cutouts, ground pours on the outer layers (`.process.top.net &= "GND"`), areas where copper or parts are forbidden (`Keepout`), mounting holes, fiducials and a `Label`. Ask the user for the numbers: do not guess a size. If the user has the enclosure as a DXF, take the outline, cutouts and mounting holes from it instead of typing them: `netsmith dxf import case.dxf --layer EDGE --holes-layer HOLES` (with no `--layer` it lists the layers and their entity counts when it cannot tell; ask the user which is the board edge) prints source to paste, and `--write board.nsm` puts the places of the holes and cutouts in the layout file with a diff. It refuses an outline that is not a rectangle with equal rounded corners (no polygon outline yet) and a file with no units (`--units`); tell the user, do not approximate. Where a part ends up is not written in the source: that goes in `<stem>.pcb.toml` beside it, which an editor or a placer writes. What a case or carrier requires of a part's position is (`.usb.place.left &= ...`); use `netsmith-use-constraints`. `docs/LANGUAGE.md` has the section "The shape of a board".
   Give each load rail its worst-case current (`.current &= 0A..2A`): netsmith draws every net at the width IPC-2221 needs for the sum of the currents it works out on it (Kirchhoff's current law, every load on at once, so a shared ground carries all the returns; ADR 0036), including nets no rail touches, so do not state `.min_track` for current. Read each `note: the current of ... is not known` it prints: state the current of one of the things it names, since a net with two or more unknown currents is not sized and stays at the process's least. A `warning: ... IPC-2221 guideline` appears only for a `.min_track` you stated that is too narrow: widen it, or state a higher `.process.max_temperature_rise` (10 K unless stated) and tell the user you did. Ask whether the project wants these to stop the build (`guidelines = "error"` in `netsmith.toml`, or `--strict` for one run).
6. Run `netsmith build`. Read the first error only, fix it, and run again. Use `netsmith-read-errors` for the messages.
7. Run `netsmith editor` and look at the schematic, the power tree and the bill of materials. Check that each pin is on the net you meant and that no rail is asked for more than its supply can give.
8. Run `netsmith fab`. It places and routes the board, runs the design rules check, and writes `fab/`. If the project has several boards and the user wants some of them, or several copies, write an order in `netsmith.toml` (`[order.<name>.boards]`, a board or a system as its folder is named, and its quantity) and run `netsmith fab --order <name>`. Continue with `netsmith-prepare-fab-order`.

## Boards that plug into each other

When the design is several boards (a carrier with sockets, plug-in cards with matching headers), each board is its own `board` with its own nets and a `system` places them (guide: "Systems and ports", "Connectors that mate"). Do not join ports with `~` where a connector carries them: mate the connectors (`.carrier.slot.mating ~ .card.j1.mating`), contact N to contact N, and the checker derives the join of the rails and buses that the contacts carry and refuses a second `~` for them.

1. Give each board its connector and wire it to its own ports and parts by pin (`.supply.pos ~ .j1.p2`). A header goes with a socket; both need the same positions, rows and pitch. Ask the user which contact is which: do not guess a pinout.
2. Every mate that carries power or a signal needs at least one ground contact, and enough of them: a contact is rated from its catalog row (3 A for the standard single-row 2.54 mm parts), and the supply and the ground each need `current / rating` contacts. A connector the library has no rating for is not checked, and `netsmith build` lists the check as not evaluated: tell the user.
3. A pinout that is the same turned end for end gets a `mating guideline` warning: key the connector (`.mating.keyed &= "polarised"` on a part that is) or change the pinout, and tell the user which.
4. Read the System view in `netsmith editor`: each mate with its contacts coloured by role, and the ground groups (which boards share a ground).

## Rules

- State what the circuit needs, not what you hope the part is: `.effective_capacitance >= 4.7uF`, not a marked value.
- Put reusable circuits in `src/*.nsm` as modules (`netsmith-write-a-module`).
- A board that has no `process` builds only as a schematic.
- The standard catalogs hold only the verified JLCPCB parts. If a catalog has nothing for a value, change the requirement or the circuit, and tell the user which. A part the user wants and the catalogs lack goes in the project's own catalog file (`catalogs/<catalog>.csv`, listed under `catalogs` in `netsmith.toml`), with its `source` and only from a record you fetched. Do not invent a part number.
- Who assembles the parts: `assembly = "fab"`, `"hand"` or `"none"` under `[project]` in `netsmith.toml` is the default for every part nothing else speaks for (a part's own `.assembly &=` wins). Ask the user before setting it.
- Statements may come in any order. There is one way to say each thing.

## Done when

`netsmith build` prints no error and no `warning:` you have not told the user about, `netsmith fab` prints `design rules check: passed` for every board, and you can list for the user every note that `fab-notes.txt` raised.
