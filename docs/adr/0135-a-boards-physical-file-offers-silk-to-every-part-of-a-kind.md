---
status: proposed
---

# A board's physical file offers `.silk` to every part of a kind, in a `defaults` block of its own

ADR 0128 put the `defaults` block in a board's source only and refused it in a physical file. Issue #769 asked for a statement of whether the silkscreen prints designators per kind of part, and `.silk` is physical intent (ADR 0048): the physical file states it, and the circuit source may not. Issue #881 left the spelling to the owner. The agent decided it on 2026-11-16 while the owner was away, so this ADR is proposed. It meets two tests of the README: it reverses one sentence of an accepted ADR, and it rejects obvious alternatives (another spelling, `.silk` in the source's block).

```nsm-fragment
import std::parts::passive::Resistor

physical for Orbit {
    defaults {
        Resistor.silk ?= "hide"
    }
}
```

**What it replaces.** Of ADR 0128, only "a physical file [is] refused": a board's physical file holds a `defaults` block. Everything else of ADR 0128 stands, and this block's lines are read by the same code (`crates/netsmith-check/src/defaults.rs`): a line is `Kind.silk ?= v`, the kind is a part, a generic or a module in the physical file's scope (its imports and the prelude, as every name in that file), it reaches every part or module of the kind at any depth, anything a statement says of the field beats it, two lines that reach one field are an error, and a line that reaches nothing is a note.

**Only `.silk`.** A line of the physical file's block offers `.silk` and nothing else: any other member is a value of the circuit and is refused with a message that names the source's block, and the source's block refuses `.silk` with a message that names the physical file. A `.silk_text` line is refused: a text belongs to one part. The value is `"show"` or `"hide"`, checked at the line. A module's physical file and a `case { }` section hold no block: a line speaks for a whole board.

**Precedence.** A part's own statement (`.silk` or `.silk_text`), then its kind's line, then the `.silk` of the nearest module outward, then the board's. It follows from the mechanism with one rule added: the line is a `?=` on the part's own `.silk`, which the design stage reads before it walks outward, and a part that states `.silk_text` is not reached, since its text says what it prints and `.silk &= "hide"` beside a text is an error. A line for a module kind gives the module's own `.silk`.

**Rejected.** A new spelling in the physical file without a block (`Resistor.silk &= "hide"` beside the paths): a second way to write a kind's line, and a name with no leading dot where every statement of the file is a path. `.silk` in the source's `defaults`: the source would state physical intent, against ADR 0048 and against `.silk` on a part, which the source refuses. A member of the process or of `case` per kind: a list of kinds in a value, where a kind's line already exists.

**Cost.** A board can have two `defaults` blocks, one in each file, with different members. A kind the source imports must be imported again in the physical file to be named there.

**Left open.** Other physical members per kind (`Capacitor.height`, `Connector.place.side`): each needs a reason, and `.place` is a path deeper than `Kind.value`, which ADR 0128 refuses.
