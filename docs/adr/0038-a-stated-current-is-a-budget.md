---
status: accepted (user review 2026-11-02)
---

# A stated current is a budget

Amends ADR 0007, ADR 0026 and ADR 0027 (what a balance verifies), and the update of ADR 0025 (an equation narrows a field that has a range).

## The fault

After an equation began to narrow a field that has a range (ADR 0025, update), a design that built before was refused. A module takes its supply through a port and has a resistor divider on it (`r_top` then `r_bottom` to ground). The board states what the module may draw as an allowance, `.monitor.vin.current &= 0A..20uA`. The divider's current is determined by its own equation, and after narrowing it is 15.84 uA to 17.17 uA. The balance in the module has two terms, the port and the divider, and it was refused:

    the currents on net `3V3` do not add up: `-monitor.vin.current` is -20uA..0A, so the others on it must add up to 0A..20uA, but they can only give 15.84uA..17.17uA

The check ADR 0007 describes asked, for each term, that its negative lie within the range the others can add up to. That holds when the other terms are as wide as the stated one, which was the case while the divider's current was as wide as the budget. It is the wrong direction for a stated range: a term that the circuit determines to be narrower than its allowance is no fault, and a stated range could not be made to pass by writing it narrower (the printed range was refused too, by rounding).

A second fault came with it. A current that propagation worked out from a balance was worked out again when its terms narrowed only if the new value lay inside the old one. A value that overlapped the old one without lying inside it (it came from another net, or from a sum rounded outward) kept the stale, wider end, and the check above then failed on it.

## Decision

A range stated on a current (`.x.current &= a..b`) is an allowance, like the range of a field that an equation states with `==`: the circuit has to stay inside it and need not fill it.

- **One stated current, the rest determined.** On a complete balance (ADR 0027) with exactly one stated current and no current that a balance worked out, the other terms determine it. What they determine must lie inside the stated range. If it does, the stated current takes the narrower value (as an equation narrows a field), so what follows uses the current the circuit really carries: the module's port above becomes 15.84 uA to 17.17 uA, and the board's supply, which follows it, 15.84 uA to 17.17 uA as well. If it does not, the net is reported with the term, the stated range and what the others work out to: `` `-monitor.vin.current` must stay within -20uA..0A, but the others on it work out to ... `` followed by every term. A stated 50 mA load whose determined chain draws 80 mA fails this way, and so does a supply that states 50 mA when the chain draws 100 mA.
- **Otherwise the balance has to be able to reach zero.** When two or more currents are stated, or none is, or a current that a balance worked out is on the net, the sum of all the terms must contain zero. If it does not, the net is reported as before (`so the others on it must add up to ... but they can only give ...`), naming the first term. A stated supply larger than the load (`.input.current &= -100mA..0mA` against a 50 mA load) is now accepted: it is an allowance too. Neither range is narrowed here, because two stated ranges do not determine one another.
- **A current that a balance worked out takes up the rest.** It is the negative of the others by construction, so it says nothing about a stated current beside it; computing a stated current from it would give the stated range back widened by the other terms (the dependency problem of ADR 0026). A net that has one is only checked for being able to reach zero.
- **What is stated.** A current in a balance is stated when its domain is a range when the solver starts and no equation (`==`) has it on its left side. A current that an equation determines is determined, whatever range it also has; the equation checks that range (ADR 0025).
- **Work out again, never leave stale.** A current worked out from a balance is replaced by what it and the new value both say (their intersection) whenever any of the terms narrows, until nothing changes. That is never less than the true value, since each is a superset of it. If they share nothing the current is left and the net is reported by the check above. At the join of a system the old rule stays: the new value replaces the old only when it lies inside it, because clipping a port's draw to what the other board gives would hide a board that asks for more than it is built to give (`verify_join`). A side effect of the fix is that the draw `examples/two_boards.nsm` hands to its regulator board, when that board states none, is the 7.28 to 16.8 mA the resistor lets through and not the stale 5 to 20 mA window of the LED.
- Only complete balances narrow or check a budget, because an unmodelled pin on the net may draw; an incomplete balance with only rails keeps the check that the sum can reach zero. A return net is checked the same way and never narrows. The joins of a system keep ADR 0010's rule, which already had this direction: a port that cannot draw states how much the board gives, and what the others draw must lie inside it.

## What changed

The solver's `verify_balances` (the enclosure test is gone), and propagation, which narrows a stated current and takes the intersection of a current it works out again. An equation on a current that a balance worked out (the regulator's `.vin.current == -.vout.current` against a supply that states 5 to 15 mA) is now met by the intersection of the two, as the equation is a statement about the same value; it is a fault only when they share nothing (`verify` and `propagate` agree). No existing test's expectation changed: the old tests never set a narrower derived value against a wider stated one, and the failing ones still fail with the same message text (`a_supply_that_cannot_give_what_a_chain_draws_is_reported_once` now fails through the budget rule, still naming the net, both terms and the numbers). No example's output changed.

## Limits

The terms are still independent ranges (ADR 0007), so the check is necessary and not sufficient. A net with two stated currents and nothing else is not narrowed; a design that wants the narrower value states it. A stated current with an equation is determined, and its stated range is not a budget on its balance, only on the equation.
