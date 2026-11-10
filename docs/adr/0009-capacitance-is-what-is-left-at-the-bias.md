---
status: superseded by ADR-0013
---

# A capacitor's capacitance is what is left at its bias

Status notes: Superseded by ADR-0013 for what `.capacitance` means and for how `.bias` is given; the curves and the worst case of this ADR stand.

A ceramic capacitor holds less than its marked value when a DC voltage is across it: a 4.7 uF 10 V X5R 0402 keeps about a third of it at 3.3 V. So `.capacitance` on a `Capacitor` now means the capacitance left at `.bias`, the DC voltage across it, and the marked value is a second field, `.nominal`. This replaces the old meaning, where `.capacitance` was the marked value, and every example and test that wrote `.capacitance &= 1uF +/- 20%` was changed; a design states what it needs after derating, usually as `.capacitance >= 1uF`, and has no second spelling for the same thing. The catalog gives each part a curve of the fraction left at each bias, which the solver evaluates in the worst case over the range the bias can take (rail tolerance, sign ignored), taking the lowest value for a minimum and the highest for a maximum. We rejected a flat "keeps at least N% at the rated voltage" rule because no datasheet or standard bounds a class of parts, and invented numbers are worse than none.

A part with no published curve has no `.capacitance` and the solver refuses it whenever a design asks for one, by narrowing it or in a relation. A bias past the last point of the curve, which ends at the rated voltage, is refused the same way, and a design that never states `.bias` for a capacitor it constrains is told so. The model is the voltage dependence only, at 25 C and the AC level of each source; temperature, ageing and the AC level in use are not modelled, and the curves are Samsung's typical data. `.bias` is stated by the design (`.c.bias == .rail.voltage`) because the checker does not yet derive the voltage across a part from a `~>` chain; doing that is left for later.
