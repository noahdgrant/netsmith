---
status: accepted
---

# A module may expose assembly

Status notes: Accepted on the user's decision, 2026-11-02. ADR 0054 (proposed) amends the type of `.assembly_mode`: it is an `AssemblyMode`, not a `String`.

Amends ADR 0004 (modules are configured only through their interface) and ADR 0028 (the manifest gives the default assembly), by example. ADR 0024 listed "a module offering the field for the parts inside it" as not done.

## Decision

A board still cannot reach into a module: `.block.j.assembly &= "hand"` is the encapsulation error of ADR 0004. A module whose author wants the board to choose who assembles its parts declares a value of its own and ties its parts to it:

```nsm-fragment
module HeaderBlock {
    .assembly_mode: String
    .j: Header1x2
    .j.assembly &= .assembly_mode
}
```

The board writes `.block.assembly_mode &= "hand"`. Nothing was added to the language for this: a text field relates to another by `&=` like a quantity does, and the solver already carries a text value across such a relation. One thing changed in the solver: a `&=` whose right side is a text field that nothing set is no longer reported as "not evaluated". A module that offers a value the board leaves alone says nothing, which is the point of offering it.

## Precedence

As ADR 0028 has it. The tie is an `.assembly &=` on the part, stated by the module, and the value it carries is the board's. So the board's narrowing of the exposed value is the design's own statement for the parts tied to it (row 5), above the catalog row, the kind's own statement, the project's default and the built-in rule. When the board says nothing the tie says nothing, and the order of ADR 0028 decides as it does for a part on the board. A kind that states `"none"` cannot be tied to another word; that is the usual empty-intersection error at the part.

## Considered

An `Assembly` type of the fixed words `fab`, `hand` and `none`, so that a wrong word is refused at the board's line instead of when the board is lowered. Rejected for now: a second way to say a text value would be a new type for one field, and the lowering error already names the part and the three words. It can be added if wrong words in modules prove a real cost.

## Naming

The idiom is spelled the same everywhere: the module's value is `.assembly_mode`, a `String`. A module that passes it on to a module inside it names it the same way there.
