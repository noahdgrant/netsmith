---
status: provisional, awaiting the owner's review (the owner chose, on issue 175, to pass a value along a `~>` chain so that a part placed in a chain sees the rail's voltage and peak without the board restating it; the smallest reading of it is built, by an agent, with the alternatives it did not take under "Choices for the owner")
---

# A chain carries the peak of its rail to the parts in series

Extends ADR 0013 (a part across a rail has its voltage as `.bias`) and ADR 0014 (a chain draws from the net it starts on) to a part in series.

## The problem

A part reads its own fields. What reaches it from outside is its `.bias`, derived by the checker for a part across a rail (ADR 0013), and its `.current`, which `chain_currents` equates along a `~>` chain (ADR 0014). A fuse in a chain from a rail is across no rail, so it saw the chain's current and nothing of the rail. Its `.rated_voltage`, the highest voltage it can interrupt, was compared with nothing, and every board with a fuse wrote `.fuse.rated_voltage >= .input.peak` itself, or had a fuse picked that was rated below the peak it sits behind (issue 175, found on a real 12 V automotive board). The rule that a part states its own requirements (`docs/DESIGN-RULES.md`) said the fuse should state this one, and could not, because it had no way to name the rail.

## Decision

A part that has a `bridge` and a voltage field named `.supply_peak` and that is in a `~>` chain is given the `.peak` of a rail by the checker, as a relation `part.supply_peak == rail.peak`, made after the nets are known, as the bias is. The rail is one whose `pos` is on the net the chain starts on, among the rails the part's scope can see: the rails the scope declares itself (a module's port, a board's rail), then the rails of what it holds, and the first by path within each, so the choice does not depend on statement order. Every part of the chain gets the value of the start, the parts that come after other parts as well: it is an upper bound, since a part before them only drops voltage, and the drop is not subtracted.

A chain that starts on a net with no such rail (a signal, a rail's `neg`, a part's pin) gives its parts nothing, as ADR 0014 leaves such a chain out of the balance. A part in no chain has no `.supply_peak`.

The standard `Fuse` is the one definition that uses it: it has `.supply_peak` and states `.supply_peak <= .rated_voltage`. With it a fuse is rated for the peak of its rail with no line on the board, the catalog picks a fuse that is, and a board that leaves the rail's `.peak` open has the check listed as not evaluated, as for any rating that waits on a value. A board that has no rail to carry it (a fuse in a chain from a net no rail is on) states `.fuse.supply_peak &= 24V`. A value the board states for a fuse in a chain is checked against the rail's, and a contradiction is reported by the solver, as for a bias.

What is built is a convention of the field's name, not a keyword: the checker looks for the name `supply_peak`, as it looks for `bias`, `current`, `voltage` and `pos`. No syntax is added.

## Choices for the owner

The decision is the smallest reading of "pass a value along a chain". These are the alternatives it did not take; each is a language or std-interface change.

- **A fault current along the chain** (option B of issue 175 read for a current): a diode behind a fuse sees the fuse's `.rated_current`, so `.d.max_forward_current >= .fuse.rated_current` is stated by the diode. This needs a second carried value (a rating of the part upstream, not a rail's value) and a rule for which upstream part gives it. It is not built, and the line stays the board's (the guide's "What a part in a chain does not see of its rail").
- **A reverse peak.** A series diode blocks the input wired backwards, and `Rail` has only a positive `.peak`, no negative transient. Giving `Rail` a `.reverse_peak` and a diode a `.supply_reverse_peak` is the same mechanism with a second field, and a change to `Rail`, which every board uses.
- **A part of the chain says it, with syntax.** A keyword or an attribute (`.fuse: Fuse { .supply_peak ~ rail.peak }`) instead of the field name, so the rule shows in the source. It adds syntax, and the rail still has to be found for the board that writes it.
- **The rail by name rather than by start net.** The part names its rail (`.fuse.rail ~ .input`), which removes the choice of the first rail by path and the case of two rails on the start net with different peaks. It makes every board say it, which is the line this removes.
- **All the rails on the start net, not the first.** `supply_peak >= peak` for each rail on the net, so the worst of them counts. It is sound for rails whose peaks differ, and the equation a design reads is no longer one value. Rails joined with `~` share the peak, so the first stands for them, and the cases where it matters are rails joined by name only.
- **The fuse's own field.** The same value as `.bias`, reading `.bias` for a series part as the voltage across it when it blocks. Rejected here because a fuse in series is across no rail, and a field that means the rail's peak for one part and the drop across another is two meanings under one name.
- **A std module of the input protection** (option C of the issue) with a fixed topology, fuse then diode then load, stating all three itself. No checker change, and a topology a board may not have. It does not replace the rule above: a board that builds the circuit by hand has the check too.
- **`Switch`, `Inductor` and other parts with a voltage rating.** `Switch` has a `.rated_voltage` that nothing compares, and takes the field as `Fuse` does. Not done: a new check on a part is a std-interface change of its own, and it is one line (`.supply_peak <= .rated_voltage`) once this is accepted.
- **`Tvs.protected_limit`.** Left as it is (ADR 0023). A board states `.tvs.clamping_voltage <= .reg.vin_abs_max`, or lets `.vin.peak <= vin_abs_max` of the regulator cover it.

## Cost

A board with a fuse and a rail that states no `.peak` gets one more check listed as not evaluated, and the catalog can pick a different fuse than before for a board that states a peak. The rail's `.peak` has to be stated, as the regulators and `BuckStage` already ask. The rule is a new thing the checker does for a field name, which a newcomer will not find by reading the fuse alone: the docs comment of `Fuse` and the guide say it.

## Left open

- The forward current of a diode behind a fuse and the reverse peak are not checked (above); the guide says what a board writes.
- A part in a chain through a module's port: the chain stops at the port (a chain is a run of `~>` through parts), so a fuse in a module takes the peak of the module's port, which the `~` that joins it to the board's rail shares.
- A part that drops voltage before the fuse (a diode, a second fuse) is not subtracted, so the check is conservative by that drop.
