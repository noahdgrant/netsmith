---
status: accepted
---

# `~` joins two ports member by member, by name

The owner decided on 2026-11-13 (issue #622, epic #638) that the members of two ports are joined in one statement, with a rename map for members whose names differ and a list of members left out, and that a member with no partner is an error at the line, naming the members and a near name where one exists. Large boards read like netlists: easyduino joined 37 header pins to module pins one line each. The first form built was `.headers ~ .module by name`. On review the owner chose plain `~` instead (same day, PR #677): `~` already pairs the members of two interfaces of one kind by name, and the join of two other ports is the same idea taken further, so a keyword would be a second spelling of a join (ADR 0001).

## Decision

- `.a ~ .b` joins two ports member by member when both are parts, modules or interfaces and they are not two interfaces of one kind (those join whole, as before: pins, values and bus crossings, ADR 0064). `~` between two pins is unchanged, and so is a part with a `bridge` against a pin.
- `.a ~ .b { x = y; ... }` pairs `a.x` with `b.y`, renames separated by `;` or new lines, the member of `a` first. `.a ~ .b except { p q }` leaves members out on both sides. Both may follow one join, the map first. A statement with either is its own item in the syntax tree (`Item::JoinMap`); a map or `except` on two pins or on two interfaces of one kind is an error, and so is one after a chain (`.a ~ .b ~ .c { ... }`, `~>`), a parse error.
- The members of a side are its pins and its interfaces that hold a pin, one level deep. Values, the parts and modules inside a module, and the built-in `.place` and `.model` take no part. A use of a generic offers only the members the generic declares (ADR 0068). Names in the map and `except` are bare (`gpio4`, not `.gpio4` or `vdd.pos`).
- The join is strict. After renames and `except`, every member on either side must have a partner; otherwise it is an error at the line that names each one with its side's path and suggests `x = y` from the checker's `closest`. Nothing is left unconnected in silence.
- Each pair joins as one `~` line would: two pins share a net, two interfaces of one kind join whole, two interfaces of different kinds join by name in turn (with the same strictness). A pin against an interface is an error naming both.
- A rename or `except` names a member that exists (with a near name if not); a member has one partner; a name is not both renamed and left out; `x = x` is refused.
- Nothing downstream changes: the checker joins the pairs before the nets are made, so the solver, the design stage and the exporters see the nets the `~` lines would have made, with the same names.

## No existing board changes meaning

Every case that now joins by name was an error before: a part, a module or two interfaces of different kinds on both sides of `~` (`cannot connect a Rail to a Logic`, `~ joins two pins or two interfaces`, `not the whole module`). Those that pair now join; the rest give the strict error, which is the message the guide's `Rail` against `Logic` example now shows. Every example's design file and lock is byte for byte the same; easyduino's source changed only in its header joins.

## Rejected

A keyword, `.a ~ .b by name` (the first form): two spellings of a join. Pairing by name and ignoring members with no partner: a header pin left off a module would be found only on the board. Joining only the members a rename map lists: that is the netlist again.

## Cost

A rename map is close to the netlist it replaces when two ports name everything differently (easyduino's map pairs `gpioN` with `ioN`: its 40 lines of header joins became 13, and the file has 133 code lines instead of 162). power_manifold_charger does not use it: its joins pair one rail pin with many pins of the edge connector, or one microcontroller pin with pins of several chips, and no two of its ports share names. A `~` between two ports can now make many nets, which a reader finds by reading the two member lists. `~` between two interfaces of different kinds no longer says "cannot connect"; it says which members found no partner.

## Owner decisions (2026-11-13)

- Plain `~`, with no keyword, joins by name, and strictly.
- Members are one level deep, `except` leaves out on both sides, names are bare, and a map does not follow a chain.

## Left open

Joining a rail to many pins of one port (power_manifold_charger's ground and supply pins) is a different construct. A rename map or `except` on two interfaces of one kind is refused for now (proposed, for the owner): `~` joins them whole, and a by-name join there would drop their values and a bus's crossings.
