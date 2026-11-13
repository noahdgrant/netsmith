---
status: proposed
---

# Two ports are joined member by member with `by name`

The owner decided on 2026-11-13 (issue #622, epic #638) that the members of two ports are joined in one statement, `.headers ~ .module by name`, with a rename map for members whose names differ, `by name { gpio4 = io4 }`, and a list of members left out, `by name except { gnd }`; and that a member with no partner on the other side is an error at the line unless it is left out, naming the members and a near name where one exists. Large boards read like netlists: easyduino joined 37 header pins to module pins one line each. This ADR records the form built. It is a new construct and not a second spelling of `~` (ADR 0001): `~` names both ends of every join, and `by name` names two ports and lets the checker pair their members.

## Decision

- `.a ~ .b by name [{ x = y; ... }] [except { p q ... }]` is one statement, an item of its own in the syntax tree (`Item::ByName`). `a` and `b` are paths to a part, a module or an interface. A rename map lists `x = y`, a member of `a` and then the member of `b` it is joined to, separated by `;` or new lines. `except` lists bare member names separated by spaces or new lines.
- The members of a side are its pins and its interfaces that hold a pin, at one level. Values, the parts and modules inside a module, and the built-in `.place` and `.model` of a part take no part. A use of a generic offers only the members the generic declares (ADR 0068).
- Each rename pairs its two members; each other member pairs with the member of the same name on the other side. A member with no partner is an error at the statement's line, naming each member with its side's path and suggesting `x = y` with the checker's `closest` where one fits.
- Each pair is then joined as one `~` line joins it: two pins share a net, two interfaces of one kind join whole (a `Rail` with a `Rail`). A pin against an interface, or two interfaces of different kinds, is an error at the line that names both and their kinds.
- A rename or an `except` names a member that exists (with a near name if not); a member has one partner; a name is not both renamed and left out; `x = x` is refused.
- Nothing downstream changes: the checker joins the pairs before the nets are made, so the solver, the design stage and the exporters see the nets the `~` lines would have made, with the same names. easyduino's design file and lock are byte for byte the same.

## Rejected

A spread of `~` over two ports with no keyword (`.a ~ .b` already joins two interfaces of one kind): a typo in a member name would join nothing and say nothing. Pairing by name silently and ignoring members that have no partner: a header pin left off a module would be found only on the board. Joining the members a rename map lists and nothing else: that is the netlist again.

## Cost

A large rename map is close to the netlist it replaces when two ports name everything differently (easyduino's map pairs `gpioN` with `ioN`: its 40 lines of header joins became 13, and the file has 133 code lines instead of 162). power_manifold_charger does not use it: its joins pair one rail pin with many pins of the edge connector, or one microcontroller pin with pins of several chips, and no two of its ports share names. A pin can now be on a net that no line names in full, which a reader finds by reading the two parts' member lists.

## Proposed, for the owner

These were not settled by the decision and are provisional:

1. `.a ~ .b by name` between two interfaces of the same kind is refused, pointing at `.a ~ .b`, so there stays one way (ADR 0001).
2. Members are one level deep: a rename or `except` names a member of the port (`vdd`), not a member inside one (`vdd.pos`). An interface member joins whole.
3. `except` names a member of either side and leaves it out on both.
4. Names in the map are bare (`gpio4 = io4`), not paths (`.gpio4 = .io4`), as the owner's examples wrote them.
5. `by name` is not part of a chain: `.a ~ .b ~ .c by name` and `~> ... by name` are parse errors.

## Left open

Joining a rail to many pins of one port (power_manifold_charger's ground and supply pins) is a different construct.
