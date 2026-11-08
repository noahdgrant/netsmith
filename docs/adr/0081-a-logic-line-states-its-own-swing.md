---
status: provisional, awaiting the owner's review (the owner chose option 2 of issue 220, a line that states its own swing, on 2026-11-08; the member's name, what it is relative to, and the way a part gets a ground for it were written by an agent, each with the alternatives it rejected)
---

# A `Logic` line states its own swing

## The problem

A module whose outputs swing to an internal supply that has no pad (a cellular module's STATUS and UART TX pins swing to an internal 1.8 V rail) could not state its levels. A `Logic` line needs a `reference` rail, and a rail in a part needs pads, so the part could not state the rail without inventing pins. Left as plain pins, the receiver's absolute-maximum check (ADR 0046) said `not evaluated: ... can drive it and is not the line of a Logic or a pin of an interface with levels, so how far it swings is not known`, and a 1.8 V line into a pin that survives 1.0 V passed in silence. Issue 220 listed three options; the owner chose the second.

## Decision

`Logic` and `Levels` (`std/logic.nsm`) have a member `.swing: Voltage`. A part states it as it states any quantity: `.status.swing &= 0V..1.8V`. No new syntax: it is a member of an interface in `std`, and it states nothing until a part says it.

- **What it is relative to.** The line's ground, the `neg` pin of the interface's `.reference` rail, as the four levels and the absolute maximums are. It is the range of voltage the line can have there, so both ends are voltages from that ground and a signed range is read as written: `-0.3V..1.8V` goes 0.3 V below the ground and 1.8 V above it. The ends are the worst cases (`lo` the lowest the line can be, `hi` the highest).
- **The ground still comes from the reference rail.** A part that has no pad for its internal supply joins `.reference` to a supply it does have (`.status.reference ~ .vbat`), or to its ground rail, for the ground only. The voltage of that rail is then not what the line swings to, and the swing says what does. `Levels` already says the reference rail is no load.
- **What reads it.** The absolute-maximum check (`net_range` in `crates/netsmith-solve/src/levels.rs`): a driver that states a swing contributes exactly that range to the worst-case voltage of its net, and the message names it (`a line that states a swing of 0V..1.8V`). The Ohm's-law bound on a resistor's current (`logic_swing` in `ohm.rs`) takes the same range. Nothing else reads it: it is not a level (`voh` and `vol` are what the receiver's `vih` and `vil` are judged against, and a part that states them is unchanged) and does not make a pair of lines "stated" for the levels check.
- **A line that states no swing** swings from its ground to the top of its reference rail, as before. Every existing design and test is unchanged.

## Rejected

- **A rail with no pad inside a part** (option 1). It is a new kind of declaration (a rail that is not placed), with consequences in lowering, the pin tables and the net rules, and it states a voltage when only an excursion of one line is wanted.
- **A part states an absolute maximum on a driven net** (option 3). A net belongs to the board, not to a part, and the check would need a second place that sets a net's range.
- **A swing relative to a rail's `pos`.** A part with no pad has no `pos`, and a range relative to the ground is what the datasheet's table gives.
- **`optional` on the member** (ADR 0074). No relation reads the member, so nothing is skipped; the solver reads whether it has a value.

## Left open

A swing says nothing about what a line drives a receiver to inside the levels check: a driver that states a swing and no `voh` is still a note there. Whether `voh` should default to the swing's high is the owner's.
