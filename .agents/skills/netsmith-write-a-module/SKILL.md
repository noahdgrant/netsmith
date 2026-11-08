---
name: netsmith-write-a-module
description: Turn a repeated or reusable circuit into a netsmith module or interface in src/, with the right interface. Use when a board repeats a circuit, when a file grows past one screen, or when asked to share a circuit between boards.
---

# Write a module

The language guide (`docs/netsmith/LANGUAGE.md`) has the sections "Modules", "Interfaces" and "Imports".

## When

Make a module when a circuit appears twice or has a name of its own: a regulator stage, a status light, a filter. Keep one-off parts on the board.

## How

1. Create `src/<name>.nsm`. It is the project module `src::<name>`. A file in `src/parts/x.nsm` is `src::parts::x`.
2. Define the module. Declare what a user may set or connect: rails (`.vin: Rail`), pins, and declared values (`.ripple: Voltage`).
3. Put the parts inside. Join them with `~` and `~>`, using the module's own members.
4. Connect the module's rails to the parts' rails with `~`, as the guide's regulator stage does.
5. In the board: `import src::<name>::Module`, declare `.stage: Module`, and set it only through its interface (`.stage.vout.voltage &= 3.3V +/- 5%`).
6. Run `netsmith build src/main.nsm`.

## Rules

- A user cannot reach the parts inside a module, so expose anything a user may need to set, as a rail or a declared value. Relate the declared value to the inner part inside the module. A value a user should state (a balancing resistance with no sensible default) is a plain declared value that the inner part is held to (`.inner.x &= .name`): a board that forgets it gets an `unconstrained pick` or `unstated value` warning, and a bound the module or board writes on it (`.name >= 1kohm`) reaches the part. State what a part decides, or offer a sensible value with `.name ?= value`. A module that wraps another module's value forwards it through its own member (`.inner.x &= .x`) or states it.
- To let the board choose who assembles a module's parts, declare `.assembly_mode: AssemblyMode` and write `.j.assembly &= .assembly_mode` for each part inside (ADR 0042). The board writes `.block.assembly_mode &= AssemblyMode::Hand`; left alone, the parts are assembled as if on the board. Do not offer it unless a user may want it.
- To let the board choose a function on a pin the module exports, pass the choice to the part's pin with an equation inside the module: `.mcu.p04.function == .scl.function`. A board's `.function &=` on an exported pin that no such equation reads is an error (it would have no effect).
- A module states only what is always true. Leave voltages, currents and tolerances for the board to narrow.
- A module never names a net. The board names a module's nets through its rails and pins.
- Two instances are independent. Give each its own values from the board.
- A module with no parts, only quantities and pins, is an `interface`. Use one when several modules need the same bundle of pins.
- The module's own rail is a port: from inside it supplies what is inside. State loads on the board's consuming rail and let the balance carry them in. A `~` from the module's own rail to the rail of the one part inside that draws from it makes the two one current (ADR 0077): the part's stated supply current is the module's, so do not write `.vs.current == .sensor.vs.current`. Another load on that net inside the module (a pull-up chain, a second part) stops it.

## A part or module that stands for a generic

When several parts or modules can do one job (two connectors, two regulator ICs), declare `generic Name { ... }` with what a board may rely on, and write each as `part X provides Name` or `module X provides Name` (a module for an IC that needs support parts). Do not redeclare the generic's members in a provider: narrow them (`.max_frequency &= 6GHz`). The board imports the providers' files, writes `.rf: Name { <requirements> }` and the solver picks. A generic holds values, pins and interfaces, never parts; its `pins` table has rows of a kind only (`.signal passive required`) and only a part provides it. The standard passives, diodes, `NMosfet` and `LinearRegulator` are generics that stand for themselves: write `part X provides Resistor` to add a part that a use of `Resistor` may pick when no standard row serves (it has its own pin table and catalog and no `.package` line). See "Generics and providers" in the guide.

## Boards that plug into each other

A module is a circuit on one board; a connector between boards is not a module. Wire a connector on the board and mate it in a `system` (`.carrier.slot.mating ~ .card.j1.mating`): a system reaches a connector's `.mating` and nothing else of the board, so a module cannot offer its connector's pins to a system. Put the connector on the board, and expose to it the rail or bus the module has (`.stage.vout ~ .out`, then `.out.pos ~ .j1.p2`). Share the card's circuit between boards as a module, and the connector on each board.

## Check

`netsmith build` passes with two instances of the module on a board, each with different values, and no instance reaches another's parts.
- A module that forwards a bus port (`.bus: Spi`) joins it to the part inside with `~` and gives it the part's role with an equation, `.bus.role == .chip.spi.role`. A `Uart` port is drawn as a part draws it (its `tx` is the line the module sends on), joined pin by pin with the same names, because `~` between two `Uart` interfaces crosses.
