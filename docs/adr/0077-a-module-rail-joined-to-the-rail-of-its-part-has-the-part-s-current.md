---
status: provisional, awaiting the owner's review (the owner decided that a `~` between a module's own rail and the rail of a part inside it carries the current, narrowly, on 2026-11-07; the precise rule and the way it is made are written by an agent, with the alternatives it rejected)
---

# A module rail joined to the rail of its part has the part's current

Amends ADR 0007 (`~` does not equate currents) for one case.

## The problem

A module that wraps a part has a rail of its own and joins it to the part's with `~`: an input monitor with its bypass capacitor, a flash with its decoupling. `~` between rails equates every field except `current`, because a rail can feed several loads. So the part's own statement of what it draws (`.vs.current &= 0A..1.1mA`, ADR 0066) did not reach the module's rail unless the module said so in an equation, the same line in every wrapper, `.vdd.current == .sensor.vs.current`. The module's balance (the module's rail, flipped, and the part's rail) is two terms and works out the module's rail, but only while the net inside the module has no other current that is unknown. A part whose logic line names its supply as the line's reference puts a second rail on the net, with no statement of what it draws, and the balance had two unknown currents:

```text
these values depend on each other and none has a value, so the checks that use them are not evaluated: `sheet.monitor.sensor.alert.reference.current`, `sheet.monitor.vdd.current`, `sheet.vdd.current`, `supply.current`
```

A sheet that adds up the draws of its modules (`.vdd.current == .monitor.vdd.current + .storage.vcc.current`) had its check not evaluated, and the supply had no current. With one such module on the net it did not matter, because the supply's balance had one unknown; with a second it did (issue 189). The guide's example is this board.

## Decision

A `~` written inside a module that joins one of the module's own interface members, an interface with a `current` and a `pos` (a `Rail`), to the same kind of member of a part that is directly inside the module (`.vdd ~ .sensor.vs`, in either order) makes the module's rail the part's current. It is made in the balance of the net, in the module's scope, and not as an equation between the two `current` fields: the balance there has the two rails as its only terms. The rail of a logic line or of a bus's levels (a `reference`, ADR 0066) is not a term of it, because it carries nothing; a current that is exactly zero is none either.

The rule applies when the module's rail and the part's are each joined to one other rail by `~`, and the only draws on that net in the module are those two, the zero ones and the references. It does not apply, and the balance keeps every term it had, when:

- the module's rail is joined to the rails of two parts, or a part's rail to two of the module's: the module's current is what they draw together, or the part's is the supply of two;
- a chain starts on the net (a pull-up from `.vdd.pos`): it draws through a part that is not the sensor;
- another part has a rail on the net, joined to the sensor's rail and not to the module's, or another rail of the module that is not a reference (a module that hands its supply on): the module's rail then carries more than the part's;
- the part is not directly inside the module, or the inner member is a module's port and not a part's rail: a module's port joined to the port of an inner module is left to the balance (see Left open);
- the module is a board: a board has no interface, its rails stand for the outside and a part joined to one is a load in the balance as before.

Nothing else changes. `~` anywhere else does not equate currents (the guide's Load current section). The balance stays a balance, so a budget stated at the module's rail is judged as ADR 0038 says: a `.monitor.vdd.current &= 0A..500uA` against a part that claims 0 to 1.1 mA is refused, with the numbers, as for a part that is not in a module.

## What was rejected

- **An equation between the two `current` fields, pushed by `~`.** It is the alias in its plainest form, and the solver merges two connected values to the range they share. A budget stated at the module's rail then narrows the part's claim to fit, and the refusal of ADR 0066 and ADR 0038 is lost for every wrapped part: a supply allowed 0.5 mA of a part that may draw 1.1 mA built without a word, where it is refused for a part that is not in a module. The alias made in the module's balance keeps the budget and the claim apart.
- **The language says that a module's interface rail is an alias of the part's, everywhere.** It makes the module's rail a name for the part's and nothing else, and the module could no longer put another load on its own rail (a pull-up, a second part, a pass-through rail); there the rule above leaves the balance alone. It also changes what the interface of a module means for every module, a change that is broader than the fault.
- **Leave it and write the equation in each wrapper.** The same line in every wrapper, for a fact the `~` already states, and a wrapper that forgets it gives a sheet that cannot be solved, with a message that names the sheet's fields and not the module's missing line. The rules for the language say a thing is said one way.
- **Equate in every module's balance, whatever else is on the net.** A wrapper with a pull-up on its rail would be told that its rail draws what the part draws and nothing more, and fail a balance that is right.

## Left open

- A module that wraps another module (`.vdd ~ .inner.vdd`) is not covered: the inner rail is a port, whose current the inner module's own balance sets from its part, and a module's balance of two terms already equates the two when nothing else is unknown. A reference rail on that net still stops it. The decision named a part inside; extending it to an inner module's port is the owner's.
- A wrapper's bus interface (`.spi.levels.reference ~ .vcc`, issue 188) is a member of the module and joined to the module's rail, not to a part's, so the rule does not see it. The reference of a part's own levels is left out of the balance; the module's own is not, and the module states `.current &= 0A` for it.
- The same applies to the `neg` pins: the return nets are checked and never solved from (ADR 0014), and are as before.
- Which equations the standard library and the tests wrote by hand for this are now redundant is listed in the pull request; they agree with the rule and pass.
