---
status: proposed for owner review (written overnight on 2026-11-06 by an agent that was told to pick an option and go on; the rule of a range's ends is the agent's choice, and it changes a documented property of `Quantity`)
---

# A range moves past a limit only where the arithmetic rounded

A board states `.after.voltage &= 7V..16V` on a rail that an equation sets as `.supply.voltage - .series.drop`, with the supply at `9V..16V` and the drop a product that starts at zero. The build said `must stay within 7V..16V, but (supply.voltage - series.drop) works out to 8.899V..16V`. The two ranges end at the same 16 V and the check still failed. It was found on the first full build of a real board and appeared again behind a fuse, a diode and a shunt.

## What was found

`netsmith-quantity` rounded every result of `add`, `sub`, `mul` and `div` outward by one unit in the last place, whether or not the operation had rounded anything. A drop of `0A..1A` times `99mohm..101mohm` has its lowest end at `0 * 0.099`, which is exactly zero, and the widening made it `-5e-324`. The supply less that drop is `16 - (-5e-324)`, which is exactly 16 in doubles, and the widening of that sum made it `16.000000000000004`. The display rounds to four digits and shows `16V`. So `7V..16V` did not enclose the result, and the message gave no hint of a difference. It was not the narrowing step of `propagate` and not an open or closed end: the ends are closed everywhere. The same widening is why `a * 3` (297 to 303 ohm, exact) was refused against a window of `297Ω..303Ω` in `crates/netsmith-solve/tests/backjump.rs`, a test that had recorded the refusal as the behaviour ("a range that just misses", with two equal ranges in its message).

## Decision

An end moves only when the operation that made it rounded, and then by one unit towards the side the real result is on. Whether an operation rounded, and which way, is found exactly and not guessed: a sum or difference of two doubles by Knuth's TwoSum, a product by a fused multiply-add (`a * b - p` is exact), and a quotient by the remainder `a - q * b`, also by a fused multiply-add. A product with zero and a quotient of zero are exact. For an operand or a result whose size is below 1e-100 or above 1e100 the error cannot be read reliably (it can underflow), so that end is widened by a full unit as before. A negative zero is stored as a zero, so a range never prints as `-0V`.

This is not a tolerance on the comparison and it does not weaken a check. Every range is still a closed interval that contains every value the real arithmetic can give, because each end is the double that is the correct directed rounding of the real result of the end's operation, which is at least as tight as one unit wider and never tighter than the truth. A check is passed only when the real range is inside the limit.

## Alternatives rejected

A tolerance in `encloses`, `verify` and the narrowing rules (an end within one part in 10^12 counts as inside). It is a rule of the comparison that a board author cannot see, it passes a range that exceeds a limit by a real hair (`14.999999999V` against `15V`), and every place that compares would need it. The cause is the arithmetic, not the comparison.

Not widening a result that is exactly zero, or only after a subtraction. It fixes this board and leaves `a * 3` against `297..303`, `10 V - 1 V * 1`, and every sum of exact values refused. The rule here covers them all with one test: did the operation round.

Rounding with the processor's directed rounding modes. Rust has no stable way to set them, and a change of mode is global state.

Widening both ends of every result as before and teaching the check of a stated range to let the stated end win. A range that ends past a limit by a hair can then pass, which is the unsound direction.

## Cost, limits, left open

A range that really ends a hair outside a limit is refused as before; one that ends exactly on it is now accepted, so a design can pass a check that it used to fail by a unit in the last place. The module's old documentation promised that a design that exactly touches a limit could be rejected ("never accept one that misses it" is kept, the first half is not). `Quantity::tolerance` on a range and the literal forms (`x +/- y%`) are unchanged: a literal point is not widened and the tolerance of a range is widened by a unit on each side. The fused multiply-add is hardware on current processors and software elsewhere, so the quantity arithmetic is somewhat slower where there is no hardware; no timing was measured for that. The failure message shows four digits, so a refusal that is a hair wide still looks like two equal ranges (`works out to 9.956V..15V` against `9V..15V` when the real end is 15.0000001). That is not changed here.

## What to look at

- `crates/netsmith-quantity/src/quantity.rs`: `add_ends`, `mul_ends` and `div_ends`, and the guard values. The soundness argument is in the doc comment.
- `crates/netsmith-quantity/tests/exact_ends.rs`: a product range holds the exact 80-bit product of two 40-bit integers, over 2000 pairs, and a range that is a point only where the product is exact.
- `crates/netsmith-solve/tests/range_ends.rs`: the accepted window that ends at the supply's maximum and the refused windows beside it.
- `crates/netsmith-solve/tests/backjump.rs`: `a_range_that_just_misses_is_reported_as_it_was` is replaced by a test that a range that touches exactly is accepted and one that misses by 0.001 is refused. This is the one existing test whose expected result changed.
