---
status: provisional, awaiting the owner's review (the smallest reading of issue 63's "ratings of a diode across a rail do not see which way round it is" is built, by an agent, with the alternatives it did not take under "Choices for the owner")
---

# A diode across a rail has the voltage of the way it is wired

Extends ADR 0013 (a part across a rail has its voltage as `.bias`) to the parts that are not the same either way round.

## The problem

The checker gives a part with a `bridge` and a `.bias` the rail's voltage when its two ends are on the rail's `pos` and `neg`, in either order, because a ceramic capacitor does not care which. `Diode`, `Schottky`, `Zener` and `Tvs` took the same rule, so a rectifier with its anode on `pos` was given the rail's voltage as the voltage it blocks, and `.bias <= .max_reverse_voltage` was checked as if it blocked. A diode wired that way conducts across the rail and is a short. Nothing said so; the std tests and the guide wired their diodes that way round (`.supply.pos ~> .d ~> .supply.neg`) and passed. A real 12 V automotive board with its input protection and diodes across rails had no check that a diode or a TVS diode across a rail was the right way round.

## Decision

A part that has a `bridge` and a voltage field named `.bridge_voltage` is given, when it is across a rail, the voltage from the first end of its bridge to the second: the rail's voltage when the first end is on the rail's `pos`, its negative when it is on the `neg`. It is one relation, `part.bridge_voltage == rail.voltage` or `== -rail.voltage`, made after the nets are known, by the same code and with the same choice of rail (the rails the part's scope can see, the first by path) as for `.bias`. A part that has a `.bridge_voltage` is not given `.bias` by that code: its own relations define the bias from it.

The four diodes of `std::parts::diode` have it:

- `Diode` and `Schottky` (bridge anode to cathode): `.bridge_voltage` is the voltage at the anode relative to the cathode. `.bias == -.bridge_voltage`, so `.bias` is what it was documented to be, the voltage in the direction the diode blocks, and it is negative when the diode conducts. A new relation `.bias >= -.forward_voltage` says that the cathode is never more than a forward drop below the anode: it fails for a diode across a rail with its anode on `pos`.
- `Zener` and `Tvs` (bridge cathode to anode, the way they clamp): `.bridge_voltage` is the voltage at the cathode relative to the anode, which is what `.bias` was documented to be. `.bias == .bridge_voltage`, and `.bias >= 0V` says that a clamp is not forward biased: it fails for one across a rail with its anode on `pos`.

The ratings that were there follow from it with no change of their own: `.bias <= .max_reverse_voltage`, `.bias <= .standoff_voltage` and `.bias * .current <= .max_power` read a bias that is now the voltage the part blocks, and a part wired to conduct has none to block. The finding for a reversed diode is the solver's, as for any rating: it names the part and the relation (`d.bias must always be at least -d.forward_voltage, but -12.6V..-11.4V is not always above -700mV`, as the solver words it), and the guide shows it.

A diode in series, across no rail, has nothing derived, as before: the board states `.bias`, and that is the voltage it blocks.

`.bridge_voltage` is an `optional` member (ADR 0074). A diode across no rail has none, so the relations that read it (`.bias == -.bridge_voltage` and `.bias == .bridge_voltage`) are dropped for it: they are neither checked nor listed as not evaluated, and a series diode's stated `.bias` is what it was. A first version without `optional` listed the two relations as not evaluated for every diode in series, four notes on a board that had none.

What is built is a convention of a field's name, not a keyword: the checker looks for `bridge_voltage` as it looks for `bias`, `supply_peak`, `voltage` and `pos`. No syntax is added.

## Choices for the owner

- **A signed `.bias` for every part.** Let the checker give every part the rail's voltage with its sign and have the capacitor take the magnitude. It makes a capacitor's `.bias` negative on a reversed rail, which `.max_voltage >= .bias * 1.5` and the derating curve (which already ignores the sign) would have to ignore, and it needs a bridge direction that means the same for every part. The bridge of a `Diode` is the way it conducts, the bridge of a `Zener` the way it clamps, so a bridge direction is not enough to say which end is the cathode. Not taken.
- **The sign from the pins.** The checker could know the cathode by the pin's name, or by a marker in the pin table. A name is a convention that names a CAD-neutral library's pins in the checker; a marker is a change to the `pins` grammar. Not taken: `.bridge_voltage` puts the one fact the checker needs (which end is first) in the bridge the part already states.
- **A keyword or attribute.** `bridge_voltage` as syntax (`bridge .anode ~> .cathode across .bridge_voltage`) instead of a field name, so the rule shows in the source. It adds syntax, and the rule is as discoverable from the field's doc comment as `supply_peak` is.
- **A guideline or a checker error for a reversed diode** with its own wording, "diode `d` is wired the wrong way round across `input`". It would need the checker to know a diode, or a marker for it, and says what the relation already says in numbers. A friendlier message could be written from the relation's source line if the owner wants one.
- **`.drop` for the voltage.** `Diode` has `.drop`, the voltage at the anode relative to the cathode, which is the same quantity. The DC analysis reads `.drop` as the forward voltage (`.drop == .forward_voltage`, `dc diode`), so deriving it from the rail would contradict the analysis for every blocking diode. Not taken.
- **The forward current.** A diode across a rail wired to conduct has no current the library can state: it is a short. The finding is the wiring's, and `.current <= .max_forward_current` stays for a diode in a circuit.
- **A bound for a Zener or a TVS diode forward biased.** `.bias >= 0V` is the strictest reading. A datasheet gives a forward voltage for some of them (`VF`), and `.bias >= -.forward_voltage` would allow a clamp across an AC line. `Zener` and `Tvs` have no such field, so it is `0V`.
- **A diode between two rails** (an OR-ing diode from one rail to another) is across no rail and has nothing derived. The rule that finds it is a rule about two rails and is not built.

## Cost

The std tests and the guide had diodes wired to conduct across a rail, with a reverse-voltage rating checked against the rail; they are rewired the right way round, and the guide's `Clamp` example wires its diode with the cathode on `pos`. A board that did the same is found now, which is the point. A project's own diode part is not changed: without `.bridge_voltage` it keeps the magnitude. A diode in series with a load has `.bridge_voltage` unset, and nothing is checked or listed for it (above). The orientation check of a `Diode` compares with `.forward_voltage`, which a `Schottky` has from its catalog row and a bare `Diode` has only when the design states it; a `Diode` across a rail with no `.forward_voltage` is listed as not evaluated, and a reversed one is not found until the design states it. A `Zener` or a `Tvs` is compared with `0V` and needs nothing.

## Left open

- The rule is the one field's name, and a project that copies `Diode` has to know to add it.
- A diode between two rails, and the forward voltage of a `Zener` or `Tvs`, are not checked (above).
