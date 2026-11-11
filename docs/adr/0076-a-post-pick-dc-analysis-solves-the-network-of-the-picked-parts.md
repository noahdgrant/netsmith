---
status: accepted
---

# A post-pick DC analysis solves the network of the picked parts

Cut from 186 KB and 17 amendments to the decision. The text before the cut is `git show 20f8f0c:docs/adr/0076-a-post-pick-dc-analysis-solves-the-network-of-the-picked-parts.md`; source files cite its sections and amendments ("ADR 0076, amendment 10"), and they are in that text. What it said about behaviour is in `docs/LANGUAGE.md` ("What a part is at DC", "DC analysis") and, for a maintainer, in the headers of `crates/netsmith-solve/src/dc/mod.rs` and `src/solve/dcpass.rs`.

The solver picks parts, propagates a range per field through the equations of the library and the design, and adds up the currents of a net (ADR 0007, 0014, 0036). It never solved node voltages and branch currents together. On a real 12 V automotive board of about 60 parts that left 47 notes (26 relations `not evaluated`, 10 nets with no known current) and a hand-written seed that hid that the input chain cannot give less than 8.05 V (issue #153).

One pass is added between the pick and the final verification of the relations: a worst-case DC operating-point analysis of the picked parts. It builds a netlist from the parts that state a DC element and solves it. Where a field is unstated it writes the range the circuit gives; where a stated value or a rating is exceeded it reports an error that names the states giving each end. The relations that waited are then evaluated, tracks are sized from known currents, and the levels check reads node voltages. It adds to a design that already builds: a range it narrows was wider, a relation it evaluates was a note. It does not pick parts (a pick is made first, ADR 0057) and it does not model time: no inrush, ripple, AC or stability, and a capacitor is open.

**Corners per island, not intervals.** Interval arithmetic counts a value once for each time it is written: `10 * R2 / (R1 + R2)` with both resistors `1 kohm +/- 1 %` is 4.90 V..5.10 V that way and 4.95 V..5.05 V for the circuit (ADR 0026 is the same fault on currents). The pass solves the network exactly for every combination of the ends of the ranges and of the states (a diode blocks or conducts, a pin is high, low or high impedance, a MOSFET is on or off), and the range of a quantity is the hull of the results. The hull is the circuit's range only where the quantity is monotone in each value, which a power, or a ladder with two sources, need not be.

**Held nodes cut the network into islands.** A node that an ideal source holds (a supply rail, a regulator's output) separates what is on its two sides as far as voltage goes. Each side is an island, solved on its own; the current one island takes from the node is a range the other treats as a current source. Combinations add and do not multiply: four islands of 96, 24, 80 and 96 combinations, where one island of all four would be 17.7 million. The cost is the lost tie between that current and the node's voltage, so the result is wider and never narrower. A regulator's input current depends on its own input voltage, so the islands behind its output are solved first and the one in front finds a fixed point from zero.

**The owner decided on 2026-11-07** the seven questions the first text asked: (1) a part states its element with a `dc` statement in the part or generic, not a table in the solver and not by the shape of its relations; (2) a pin's drive is the datasheet's own form, a level with its test current (`.voh &= .vdd.voltage - 0.4V at 2mA`); (3) no way to say which states happen together, the pass tries every combination and the guide says so (issue #45); (4) narrow where unstated, check where stated (ADR 0038); (5) a boundary pin takes a stated `.current`; (6) a failed rating, a source that cannot give what its load asks, an input below a regulator's minimum and a node voltage above an absolute maximum are errors, and an island too large to solve is a note; (7) capacitors stay open at DC. On 2026-11-09 the owner decided that a voltage the design states on a rail the analysis solved is checked, not trusted.

**A circuit simulator** (ngspice, or netsmith's own transient solver), for three reasons: it is a dependency that does not run in the editor's loop, it needs a model per part to write, and it gives a transient answer to a question that the design states as a worst case. The owner now wants closed-form relations first and a simulator later (issue #306).

**The pass inside the search**, for each candidate: it multiplies the cost by the search's trials. A small island is solved when the last part of a group is picked (the first text's amendment 10), no more.

**The cheap bound** (Ohm's law over the ends of a net's range) as the engine. It stays the fallback for an island with a pin nothing models. It can refuse a good design, because it does not know a pin draws nothing (ADR 0026).

**Equations only** (issue #153: `~>` over rails carries the drops, circles start from a source's bounds): each is a rule for one shape, and the first needs the regulator's current, which is the circle itself.

**A sound enclosure** by interval Newton or affine arithmetic, which removes the monotone assumption: a harder solver and wider answers. Not now.

**Cost.** An island over 65536 combinations (and less when it is large) is left alone with a note that says to cut it at a node a rail holds. The pass is deterministic. Two corners per part that can reach zero ohm, so an island with several beads has more combinations.

**Accepted as built by the owner, 2026-11-11**, with the 17 amendments of the cut text. Among them, amendments 2, 14, 15 and 17 can turn a design that built into a failure: a stated field is a budget, an efficiency outside `0 < eff <= 1` is an error, a disagreement between the balance and the analysis is an error, and a stated range that does not enclose the circuit's is an error. The owner also accepted "meet, not exceed" for a supply's `.current` budget (ADR 0038's reading).

**Not built, and limits.**

1. A sound enclosure for non-monotone quantities (a stated margin of about `(tolerance/2)^2`, interval Newton, affine arithmetic): no change until the interior check fails on a real design (the owner accepted the digest's recommendation).
2. The island limit (65536 combinations, work 2^28), the cost on a whole board, the fixed point's convergence for a regulator with hysteresis, and the fuse's missing maximum resistance: issue #45.
3. A part with two elements (a body diode, a dual diode, a resistor network), and whether a part with a `bridge` and no `dc` is an error at the checker (a note today): not planned until a board needs it.
4. Which node a buck holds (the IC's switch pin, so a window stated on the rail past the inductor is not checked): issue #63.
5. A load that states nothing gives blind picks (a finding, or the regulator's rated limit as the worst case): issue #18.
6. Which rails are sources: issue #289.
7. The peak along a chain (the chain carries the voltage, not the `.peak`; ADR 0086 gives a fuse the peak), a line with a `.swing` in the analysis, naming each boundary, the cache of an island's result, and the `1 mohm` floors of a MOSFET's `Rds(on)` and a pin's output resistance: not planned until a board needs it.
