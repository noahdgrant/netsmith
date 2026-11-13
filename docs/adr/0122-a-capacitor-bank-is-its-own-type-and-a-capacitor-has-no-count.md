---
status: proposed
---

# A capacitor bank is its own type, `CapacitorBank`, and a `Capacitor` has no `.count`

The owner decided on 2026-11-13 (issue #685) that `Capacitor` should not have `.count` and that a bank should be a separate `CapacitorBank` type. The choices below that the decision did not settle are the implementing agent's, marked _proposed, for the owner_; this ADR waits for the owner, and until it is accepted ADR 0052 stands for what it does not contradict.

`Capacitor` is one part, as every other std kind is. A number the board knows is `N x Capacitor` (ADR 0119). `CapacitorBank` is for identical capacitors in parallel whose number the solver chooses or a module passes in:

```nsm-fragment
.bulk: CapacitorBank { .capacitance &= 22uF +/- 20%  .count &= 1..4 }
```

How it relates to `Capacitor` (_proposed, for the owner_). `CapacitorBank` is a std `generic` in `std/parts/passive.nsm` with the members of `Capacitor` (`.capacitance`, `.effective_capacitance`, `.bias`, `.max_voltage`, `.dielectric`, `.package`, the pins `a` and `b`) and `.count`. It lists the same `catalog capacitor`, so the same rows, the same bias curves and the same E-series part serve both (`standard.rs` keeps one entry, and a test compares the two `catalog` statements). Every field but `.count` is the value of one capacitor, so a narrowing on the bank is one on every member and a rating holds for each. The bank is one instance in the source, so the solver still picks one row and the count together, fewest first (ADR 0052), and the relations that read the bank's total are written `.count * .effective_capacitance`, as `Buck` writes them. Its members are what the design stage makes: one part of kind `Capacitor` for each, at `bulk[0]` to `bulk[n-1]` (a bank of one is `bulk`), on the same nets, so the schematic, the bill of materials, `kicad.toml` and the placer see `Capacitor`s. A provider of `CapacitorBank` is a bank too, and its parts keep the provider's kind and provide `Capacitor`.

A fixed count (_proposed, for the owner_). A whole number written as the `.count` of a board's own `CapacitorBank` is refused, with a message that points at `N x Capacitor`; a count of one points at a plain `Capacitor`. A range, a default, a relation (`.count == .held`) and a narrowing through a module's path (`.reg.output_cap.count &= 2`, ADR 0116) stay. A `.count` on a `Capacitor` is an unknown member, with a hint that names `CapacitorBank` and `N x Capacitor`.

What the bank exposes (_proposed, for the owner_): `.count` and the fields of one capacitor, no member for a single capacitor of it (`.bulk[1].package` is refused, as there is one row) and no total (`.total_effective_capacitance` would be `.count * .effective_capacitance`, and the solver finds a count-dependent relation by the fields it reads, so a derived field is not added until a second user wants it).

The seam (_proposed, for the owner_). Which parts are banks is decided in one place, `Model::bank(part)` in `netsmith-check/src/model.rs`, which returns the `.count` field and the kind of the parts. It reads one list, `BANKS`, of `(bank generic, member generic)`: `[("CapacitorBank", "Capacitor")]`. The solver and the design stage ask `Model::bank` and name no generic. The list stays, because a marker in the syntax (`#[bank]`) is not built; when it is, `Model::bank` is the one function to change.

Rejected. A `module CapacitorBank` holding `N x Capacitor`: the count must be known before the checker runs (ADR 0119, left open), and the solver chooses it. A built-in kind that adds `.count` to `Capacitor` at declaration: it hides in the checker what a std definition says. A part that provides `Capacitor` and adds `.count`: a provider does not declare members the generic does not have (ADR 0068). Keeping `.count` on `Capacitor` and refusing the fixed number: a capacitor that is one part and several at once.

Cost. `CapacitorBank` repeats the members of `Capacitor` (a test keeps the catalog statements equal; the fields are written twice). A plain `Capacitor` is no longer searched as a bank, so the search for it is shorter (the LDO example's trials went from 62 to 13; its picks did not move). A project with `.count` on a `Capacitor` changes it to `CapacitorBank` or `N x Capacitor`. The `.count` of a `CapacitorBank` reads in a message as `bulk.count`, as before.

Left open. What replaces ADR 0052's "a `Capacitor` has a member `.count`" and its `#2` paths (this ADR and ADR 0119 do, once accepted, and 0052 becomes `superseded`). A bank of other kinds (an `InductorBank`, a resistor bank): none exists, and the table is the place.
