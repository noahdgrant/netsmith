---
status: accepted (user review 2026-11-02)
---

# KCL solves rails, verifies parts

Amends ADR 0014. Amended by ADR 0027: the two-term rule holds only for a complete balance. Amended by ADR 0038: what a balance verifies when every current is known. The user was away; the choice below was taken for the user and is reviewed afterwards.

## The fault

ADR 0014 put the through-current of a chain's first part into the balance of the net the chain starts on, and ADR 0007's propagation solves the one unknown current of a balance as the negative of the rest. With the resistor power rule of ADR 0025 (`.drop * .current <= .max_power`) this gave false failures on a real board: `(r13.drop * r13.current) must always be at most r13.max_power, but -1.7e-199 fW..461 kW is not always below 250 mW`. The resistor heads a divider that starts on a regulated rail and ends on a signal pin. The balance of the rail holds the regulator's supplied current, several loads and that resistor, and the resistor's current was the only unknown. The residual of several wide intervals (the supply 0 to 2.1 A, the loads 0 to 2 A and others) is -2.1 A to 2.1 A, or worse, and a resistor cannot carry that much by itself. It is the dependency problem again: the terms of a balance are independent ranges, so the difference of their sums is wider than any one of them and says nothing about a part.

## Decision

Balance propagation sets the current of a rail or a port (ADR 0007), and sets a part's through-current only from a balance of two terms. From a longer balance a part's current is verified (an inconsistency is an error naming the net and the terms, as before) and never set. Its own relations determine it: Ohm's law, the LED equation, a stated value, or a neighbour in the chain. If they do not, it stays unknown, and every relation that needs it (the power of a resistor, the rating of a fuse, an inductor's saturation, a Zener's power, a Schottky's or a ferrite bead's current) is listed as `not evaluated` like any other relation with an unknown, and is no failure. A relation whose current is determined keeps failing with its numbers.

A balance of two terms is an equality, not a subtraction: the range of one is the negative of the range of the other, and nothing is widened. Two things need it. A fuse in series with a load (`supply.pos ~> fuse ~> load.pos`) carries the load's current, and a rating is checked against it (the guide's fuse example). The inductor of `BuckStage` is in a chain from the IC's switch pin to the output, and its current is the load's: both ends of the chain are two-term balances. Refusing these would have meant stating `.fuse.current == .load.current` in every board, which is what ADR 0014 removed. The rule the user gave (rails and ports only) is kept for everything else.

## What changed

Only `solve_balance` in `netsmith-solve`. No library file changes: the ratings above are already relations that evaluate to unknown when a current is, and the solver already lists those. Existing tests hold, among them the LED branch counted against the regulator (its current follows from the LED equation and the chain), `parts_alone_can_overload_the_regulator`, the derived input of `two_boards` and the 150 mA examples. A first version that refused every part broke four tests that did solve a part's current from a net: the fuse test and three buck-stage tests (the inductor's current from the output), and the guide's buck example, all two-term cases; that is why the rule above has the exception. No test relied on a longer balance.

## Limits

A part at the head of a chain on a busy rail whose current nothing else states is not checked against its rating; the build says so with a `not evaluated` line, and the design can state the current. The worst case is still taken for each current separately (ADR 0007).
