# How the pieces combine

A netsmith design is built from a few kinds of thing, each inside the next. This page says what they are, what may contain what, and who may reach into what. The words are defined in `GLOSSARY.md`; the syntax is in `docs/LANGUAGE.md`.

## The picture

```text
system        several boards and the joins between their ports       (one order's worth of boards)
 └─ board       one physical PCB: a process, parts, modules, nets
     └─ module    a reusable sub-circuit (a regulator stage, a sensor block)
         └─ part    one component: pins, quantities, a catalog of real parts
             └─ pin   a terminal with a kind (power, input, output, ...) and pads
```

`interface` is not a level. It is the type of the things that cross the levels: a `Rail` (a supply), a `Uart`, an `Spi`. Boards, modules, parts and systems all expose interfaces, and `~` joins two interfaces of the same type.

Beside the system is the `panel` (ADR 0040): one board repeated in a grid on one fabrication panel. It joins nothing and names no nets; it is a manufacturing unit, not a circuit. Only identical boards on a V-cut grid are built so far.

## What each one is

| Kind | What it is | It contains | Made by | In the standard library |
|---|---|---|---|---|
| `part` | A component. States only what is always true of it: its pins and pads per package, its ratings, its catalog columns. | pins, quantities, relations | `part Name { ... }` | `Resistor`, `Capacitor`, `Led`, `LinearRegulator`, `Diode`, `NMosfet`, `Header1x2`, ... |
| `interface` | A typed bundle of pins and quantities that is connected as one. | pins, quantities, other interfaces | `interface Name { ... }` | `Rail`, `Uart`, `Spi`, `I2c`, `Swd`, `Usb2`, `Logic`, `TwoLayer` (a process), `Stackup` |
| `module` | A reusable sub-circuit. Placed on a board or inside another module. | parts, modules, interfaces, relations | `module Name { ... }` | `I2cPullups`, `Buck<Ic: BuckIc>`, `FeedbackDivider` |
| `board` | One physical PCB. The only level that names nets and has a manufacturing process. | parts, modules, rails, a `.process` | `board Name { ... }` | none (a board is yours) |
| `system` | Boards and the joins between their ports. Names no nets. | boards | `system Name { ... }` | none |

A catalog is not a level; it is how a `part` becomes a real component. A part lists its catalog columns, and the solver picks a row from a CSV file (`data/jlcpcb/catalog/` in the tool, `catalogs/` in a project) that satisfies every requirement in the worst case.

## Who may reach what

- **A module is configured only through its interface.** From outside you may use a module's interfaces, pins and declared values. The parts and modules inside it are private (ADR 0004). A board therefore cannot narrow a capacitor inside a module unless the module exposes it.
- **Only a board names nets.** A net name is board-wide; a module or part that named a net would give every instance the same name (ADR 0005). Unnamed nets get a name from the instance path.
- **A process belongs to a board.** `.process: TwoLayer`, `FourLayer` or `SixLayer` (or the fictional ACME's `EightLayer` or `TenLayer`, not for ordering, ADR 0112) is a member of the board, so its rules and stackup travel with it (ADR 0008).
- **A system joins ports only.** A port is an interface declared directly on a board. A system says which ports meet and checks that voltages and currents agree (ADR 0010), or mates two connectors, contact N to contact N, which checks that the pinouts fit and derives the ports they carry (ADR 0040). Each board keeps its own ground net, and is still made and ordered on its own.
- **A part states what is always true; the design narrows the rest.** A library part never fixes a value a design might need to change (ADR 0002).

## From source to files

```text
.nsm source ──check──▶ model ──solve──▶ picks ──design─▶ design ──┬─▶ KiCad schematic (.kicad_sch)
 (std + project)        names, units,    real parts     one per   ├─▶ PCB: place, route (.kicad_pcb, SVG)
                        nets, relations  from catalogs  board     ├─▶ fab package: Gerber, drill, BOM, CPL
                                                                  └─▶ the editor's views
```

A `board` is the unit that the design stage and the exporters work on: one board gives one schematic, one PCB and one fabrication package. A `system` is designed one board at a time. The _design stage_ is the step that makes the `Design`, the parts and nets that the exporters read; the stage and what it makes share a word.

## Library and project

- **The standard library** (`std/`, embedded in the binary): common parts, interfaces, manufacturing processes, buses.
- **A project** (a directory with `netsmith.toml`, see `docs/PROJECT-LAYOUT.md`): its own modules and boards under `src/`, its own catalogs under `catalogs/`, and its own KiCad mapping and libraries. A project imports the standard library with `import std::...` and its own files with `import src::...`.

## Not built yet

- **Panels of different boards, routed tabs and mouse bites.** A `panel` of identical boards on a V-cut grid is built (ADR 0040, P0); the rest of ADR 0040's plan (P1 to P3) is not. See issue #62.
- **Hierarchical sheets.** A module is flattened into its board today; drawing it as its own schematic sheet is open.
