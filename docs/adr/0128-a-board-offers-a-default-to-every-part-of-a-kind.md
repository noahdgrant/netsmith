---
status: accepted
---

# A board offers a default to every part of a kind, and any statement on the value beats it

The owner decided on 2026-11-13 (issue #625, epic #638) that a board says once what it would otherwise repeat on every part of a kind, in a `defaults` block; that the block reaches std parts and modules that do not set the field themselves; that there is no project-wide setting in `netsmith.toml`; and that the editor shows where a value came from. The rules the decision did not settle were decided by the agent while the owner was away (2026-11-13), each by keeping one way to write a thing. It meets two tests of the README: a second kind of default needs a precedence rule ADR 0051 avoided, and it rejects an obvious alternative (a project setting).

```nsm-fragment
board Orbit {
    defaults {
        Resistor.package ?= "0402"
        Capacitor.package ?= "0402"
    }
}
```

**What a line is.** `Kind.value ?= v`, nothing else: `&=`, a member path (`.r1.package`), a deeper path (`Resistor.place.side`), a declaration and `at` are refused at the line. The kind is a part, a module or a generic in scope, by the file's imports and the prelude; a name that is none says "did you mean" the nearest part, module or generic in scope, and an interface or an enum says what it is. The value is one the kind declares, or a generic it provides declares, with a value type (a quantity, text, an enum); a pin, a part inside it and the built-in members of every part (`.assembly`, `.mpn`) are not, and a missing one says "did you mean". The right side is lowered beside the value's type and must be a constant: a field read from one part cannot be a value for every part. Only a board holds the block, in its source; a module, a part and a physical file are refused.

**Reach.** A generic reaches every part made from it or providing it, a part's name only that part, a module's name the module's own value (`FeedbackDivider.package`, which its equations carry to both resistors). A bank is its own kind (ADR 0123), so `CapacitorBank.package` reaches banks and `Capacitor.package` does not; the members of `3 x Capacitor` are capacitors. Every part of the kind on the board is reached, at any depth inside modules. Narrowing inside a module is otherwise only `&=` (ADR 0116); the line is not a path into the module and adds no statement there, only an offer for a value nothing states.

**Precedence.** Anything a statement says of the value beats the line: a `&=` on it anywhere (ADR 0051's "stated"), any `?=` on it (the part's own, a module's, a board member's), and a `&=` or `?=` on a value that a `==` relation or a join holds equal to it. The last is wider than ADR 0051, which ignores relations: std's `FeedbackDivider` writes `.rtop.package == .package`, and a board that states `.fb.package &= "0603"` beside `Resistor.package ?= "0402"` would otherwise get a contradiction at a line it wrote for other resistors. The line is the weakest offer because it is the least specific: the part, the module and the member each name this field, and the line names every field of a kind. A field's own `?=` and the line are therefore no "two defaults" error. Where nothing states the value the line is applied as a `?=` that applied is (`Checker::apply_board_defaults` in `crates/netsmith-check/src/defaults.rs`, before `resolve_defaults`), so the solver and the exporters see a narrowing.

**Two lines.** One kind and value has one line in a block. Two lines that reach one field (`Resistor.package` and `Thin.package` for `part Thin provides Resistor`) are an error at the second, as two `?=` are: a rule that the more specific kind wins would be a second precedence to learn, and the board states that part's package with `&=`.

**A line that reaches nothing is a note,** at the line (`Model::default_notes`, a note of the build in the command line and in the editor's Problems list): the board has no part of the kind, or each states the value itself. An error would stop a board whose last resistor was just given its own package.

**The value is checked as usual.** A package none of the part's pin tables has, or no catalog row is in, is refused by the checks of ADR 0118, at the line and once for every part it reached: the message names the line (`Resistor.package ?= "0403"` in the board's `defaults`) instead of each part. A value of the wrong type is refused at the line before any part is reached.

**The editor** shows, on a part's card, the statement that gave its package and whether it is a line of `defaults`, a `?=` or a `&=` (`PartInfo.package_from`), and a click goes to it. `Field.default_from` records which default applied.

**Rejected.** A `netsmith.toml` key (the owner): a board says what its parts are, and two boards of one project may differ. The line beating a module's own `?=`: a module that chose a package for one of its parts knows that part. A precedence of the specific kind over the generic, and intersecting two lines: two rules where an error and a `&=` say it.

**Cost.** A default that reaches a field only through a `>=` or `<=` still meets it in the solver (`Capacitor.max_voltage ?= 16V` against a relation that needs more is a contradiction at the line). The examples gain blocks where they repeated a package for a kind, with their picks and nets unchanged.

**Left open.** A line for a value inside the kind (`UsbCSink.receptacle.package`), and a `defaults` block in a system or a panel for all its boards.
