---
status: accepted
---

# A capacitor bank is its own type, `CapacitorBank`, and a `Capacitor` has no `.count`

The owner decided on 2026-11-13 (issue #685) that `Capacitor` should not have `.count` and that a bank should be a separate `CapacitorBank` type, and on the same day accepted that it is a std generic with a capacitor's fields plus `.count` reading the same catalog, accepted that a fixed whole number `.count` on a board's own bank is refused, and changed the proposal to give the bank a total. The rules that the decisions did not settle are the implementing agent's, stated below.

`Capacitor` is one part, as every other std kind is. A number the board knows is `N x Capacitor` (ADR 0119). `CapacitorBank` is for identical capacitors in parallel whose number the solver chooses or a module passes in:

```nsm-fragment
.bulk: CapacitorBank { .capacitance &= 22uF +/- 20%  .count &= 1..4 }
.bulk.total_effective_capacitance >= 66uF
```

What it is. `CapacitorBank` is a std `generic` in `std/parts/passive.nsm` with the members of `Capacitor` (`.capacitance`, `.effective_capacitance`, `.bias`, `.max_voltage`, `.dielectric`, `.package`, the pins `a` and `b`) and `.count`. It lists the same `catalog capacitor`, so the same rows, bias curves and E-series part serve both (`standard.rs` keeps one entry, and a test compares the two `catalog` statements). Those fields are the value of one capacitor, so a rating holds for each. The bank is one instance in the source, so the solver picks one row and the count together, fewest first (ADR 0052). Its members are what the design stage makes: one part of kind `Capacitor` for each, at `bulk[0]` to `bulk[n-1]` (a bank of one is `bulk`), on the same nets, so the schematic, the bill of materials, `kicad.toml` and the placer see `Capacitor`s. A provider of `CapacitorBank` is a bank too, and its parts keep the provider's kind and provide `Capacitor`.

The totals. The bank has `.total_capacitance == .count * .capacitance` and `.total_effective_capacitance == .count * .effective_capacitance`, two relations in the generic. A board reads and narrows a total, and the solver chooses the count and the row to meet it. The effective total is what a requirement on a bank means: at 5 V a ceramic keeps a third of its marked value, so the marked total is the bill of materials' figure (`4 x 22uF`), not a capacitance to design with, and it is given because it is the number a board states when it means the marked part. `Buck` reads `.total_effective_capacitance` for what the IC asks and for the ripple's charge term; the ripple's ESR term divides by `.count`, which is a count.

The count. A whole number from 1 to 64. Left unset it is chosen from 1 to 8 (`?=` in the generic, which `Buck` stated for its banks and no longer does, since a field has one default), so a board that states only a total gets a count. A range limits it. A whole number written as the `.count` of a board's own bank is refused with a pointer to `N x Capacitor` (a count of one points at a plain `Capacitor`); a relation, a default and a narrowing through a module's path (`.reg.output_cap.count &= 2`, ADR 0116) stay. A `.count` on a `Capacitor` is an unknown member, with a hint that names `CapacitorBank` and `N x Capacitor`.

The seam. Which parts are banks is decided in one place, `Model::bank(part)` in `netsmith-check/src/model.rs`, which returns the `.count` field and the kind of the parts. It reads one list, `BANKS`, of `(bank generic, member generic)`: `[("CapacitorBank", "Capacitor")]`. The solver and the design stage ask `Model::bank` and name no generic, so a marker in the syntax (`#[bank]`, not built) replaces that one function.

Rejected. A `module CapacitorBank` holding `N x Capacitor`: the count must be known before the checker runs (ADR 0119, left open), and the solver chooses it. A built-in kind that adds `.count` to `Capacitor` at declaration: it hides in the checker what a std definition says. A part that provides `Capacitor` and adds `.count`: a provider does not declare members the generic does not have (ADR 0068). Keeping `.count` on `Capacitor`: a capacitor that is one part and several at once. A member for one capacitor of a bank (`.bulk[1].package`): they share one row.

Cost. `CapacitorBank` repeats the members of `Capacitor` (a test keeps the catalog statements equal). A plain `Capacitor` is no longer searched as a bank, so its search is shorter (the LDO example's trials went from 62 to 13, with the same picks). A project with `.count` on a `Capacitor` changes it to `CapacitorBank` or `N x Capacitor`. A bank with no count is no longer one capacitor but the fewest from 1 to 8 that holds the checks.

This replaces ADR 0052's "a `Capacitor` has a member `.count`" and its `#2` paths, and its `BANKS` list of `Capacitor`.

Left open. A bank of other kinds (an `InductorBank`, a resistor bank): none exists, and the list is the place. An effective total that needs a ripple rating: the catalog has none.
