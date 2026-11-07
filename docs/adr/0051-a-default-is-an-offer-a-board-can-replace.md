---
status: proposed for owner review (the spelling `?=` is the owner's choice of 2026-11-05; the meaning below was written by an agent and is for review)
---

# A default is an offer a board can replace

A module or a part sometimes knows a good value for a field and cannot know that every board agrees. A lamp module has a current that suits most boards. A sensor module has a supply voltage that its datasheet recommends. `&=` cannot say this. It only narrows, so a library that writes `.current &= 2mA..3mA` forces that range on every board, and a board that needs 6 mA to 9 mA gets a contradiction. The library rule (`docs/DESIGN-RULES.md`) is that a library states only what is always true, so the library today leaves the field open and every board repeats the same line.

Now a module or part writes `.field ?= value` to give the field a default. The owner chose the spelling `?=` on 2026-11-05 because it is intentionally different from `&=`: a reader sees at once that the line is an offer and not a requirement.

The meaning, as the checker implements it:

The default is the field's value only if no other statement states the field. A field is stated when an `&=` statement on that same field exists anywhere that can state it: the board, a module around the field, or the part itself. When a board states `.field &= w`, the default is dropped and not intersected, so `w` need not overlap the default. Two `?=` statements on one field are an error that names both lines, wherever they are and whatever else states the field. The right side of `?=` can be anything the right side of `&=` can be: a number with a unit, a range, `x +/- y%`, text, or an expression over other fields. `?=` on a name that is not declared, or of the wrong type, is refused as `&=` is.

A value that only reaches the field through a relation (`==`, `>=`, `<=`) or a join (`~`) does not state it. The default stays, and the solver then judges it with everything else that reaches the field.

The checker reads every statement of the board first and then settles the defaults. Each default that survives becomes the same narrowing a `&=` would have made (a constant narrows the field at once, an expression becomes a relation for the solver). The solver, the lowering and the exporters never see `?=`, and a statement's order never matters.

We did this because the alternatives are worse. Leaving the field open makes every board repeat the library's guess. A `&=` in the library cannot be undone by a board. Intersecting the default with the board's value would make the default a hidden requirement, and an empty intersection would be a contradiction that points at a line the board's author never wrote. Dropping the default when the board states the field gives the board the last word and makes the board's line mean the same with or without the library's offer. Refusing two defaults on one field avoids a precedence rule between a module and the board (or a module and a module inside it), and a rule of that kind would make the meaning depend on which file a line is in.

Counting only statements, and not values that arrive through `~` or `==`, keeps the rule local and checkable before the solver runs: whether a default applies is known from the text, not from a solution. The cost is a trap that the guide shows: a board that joins a module's supply to a rail of another voltage and states nothing on the module's field gets the module's default, and the solver then reports that two joined values disagree. The message names both fields. The fix is one `&=` on the module's field.

How it relates to the design rules. A default states nothing that must hold, because any board may replace it, so it does not break "a library states only what is always true". It does add a way for a library to fix a value that a design might change, which the rule names as something a part never does. This ADR does not edit `docs/DESIGN-RULES.md`; the owner decides whether the rule's text should mention `?=`.

Limits and left open. A module that wraps another cannot give a different default to a field the inner module already defaults, because that is two defaults; it states the field with `&=`, which cannot be loosened later. `?=` is accepted wherever `&=` is, including in a board's own body and in a board's physical file, and no use there has been looked at. A default on a field that is also a catalog column narrows the candidates like any narrowing and is not a row's value. The editor's syntax highlighter reads `?=` as an operator, like `&=` (ADR 0054 added the `enum` keyword to it in the same change).
