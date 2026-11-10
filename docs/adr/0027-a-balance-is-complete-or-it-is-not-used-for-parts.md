---
status: accepted
---

# A balance is complete or it is not used for parts

Status notes: Accepted on the user's review, 2026-11-02.

Amends ADR 0026 (and so ADR 0014 and ADR 0007). Amended by ADR 0038: only a complete balance narrows a stated current or checks a budget. Amended on 2026-11-02 (below): an `input` pin of a part with a rail draws nothing. The user was away; the choices below were taken for the user and are reviewed afterwards.

## The fault

ADR 0026 sets a part's through-current from a balance of two terms. A balance is only a statement about its terms when they are all that is on the net, and the checker never asked. Composing a board, the output net of a buck stage had three things on it: the first resistor of a feedback divider (a chain starting there, a term), the stage's own output port (a rail whose supplied current is 0 to 2 A, a term) and the inductor's pin, which is joined with `~` and not in a counted chain, so nothing models its current. The balance had two terms, so the solver set the resistor's current to the port's whole current: up to 2 A through 100 kohm, and the resistor's power check failed with `-461 kW`. The same gap works the other way. A rail that is the only term on a net was set to 0 A although the inductor drew from it, and the ground net, which holds the divider's return, then failed to add up. Both are an incomplete balance read as a complete one.

## Decision

The checker marks each balance (a net in a scope) complete or incomplete, and keeps the pins that make it incomplete (`Balance.unmodelled`). A balance is complete when every part pin on the net, in the scope, is accounted for:

- it is a term: the `pos` pin of a rail, a rail's `neg` pin, or the start or end of a counted chain, and the first part's pin at the start or the last part's pin at the end of that chain;
- it is a pin of a part that draws no DC current by its model (below);
- it is inside a module of the scope that has a rail on the net: the module's port stands for what is inside it (ADR 0007), whether or not the module draws;
- it is outside the scope, and the scope has a rail of its own on the net: the port stands for the outside.

Any other pin makes the balance incomplete: an IC's signal or power pin that is not a rail, an inductor, diode or resistor pin that is not in a counted chain, the pin of a module with no rail on the net (the feedback divider), a connector whose part states a current. The rules for the solver:

- A complete balance behaves as ADR 0026 says: a part's current is set from two terms and verified; rails are solved and verified (ADR 0007).
- In an incomplete balance no part's current is set. A rail's or a port's is, as ADR 0007 has it, from the other terms: a board's supply is still worked out when an IC's enable pin is on its net (`examples` and the buck tests rely on it). The exception is a rail that is the only term: with nothing else known on the net it is not taken to be zero, since the unmodelled pins are what is on the net. Without that exception the stage above set its input port to 0 A, and the ground net, which holds the divider's return, then failed to add up.
- In an incomplete balance with a part among its terms, nothing is verified either. A balance with rails only is still verified, as before.
- A balance of a join between boards in a system has only ports and is always complete.

**What draws nothing.** A part draws no DC current when it states no `current`, has no interface that draws (a `Rail`), and every pin of its `pins` tables is `passive`. By the library, that is a capacitor, a TVS diode, a crystal, a header or socket, a coax connector, and a test point. The rule uses what a part already says; nothing is added to the language. A part with no `pins` table has no kinds to go by and is not taken for one. A part that does conduct, written without a `current` and with only passive pins, would be counted as not drawing: such a part states a `current` or a `Rail`, as ADR 0014 already asks of a part that draws. An IC's `input` pin is not taken to draw nothing (a pulled-up enable or a sense pin draws leakage, the checker does not know how much).

**A note.** The build lists an incomplete balance that kept a part's current from being set, with the pins, as a note beside the `not evaluated` lines: `the balance on net `3V3` in `stage` is incomplete: `stage.l.b` is not modelled, so `stage.r_top.current` is not set from it`. The message of a failed requirement on a current gives the terms of each balance; for an incomplete one it adds the unmodelled pins.

## Consequence for the library

`BuckStage` ignores its divider's current and the IC's quiescent current (ADR 0025), so the divider's pin on the output net is not modelled and the balance there is incomplete. The inductor's current was set from that balance (ADR 0026) and is no longer. The stage states what its model says instead: `.inductor.current == -.vout.current`. All four examples are complete on every net but one: the return net of `status_indicator`, where the LED's cathode goes to a logic enable line; that net was and is not solved from, and its part terms are no longer verified.

## Limits

A rail solved from an incomplete balance can be too narrow if an unmodelled pin draws much (an enable pin draws little, an unmodelled load a lot); only the part currents are protected. Ground nets are mostly incomplete on a real board (an IC's ground pins are not terms), so a part current on a return net is never verified, only the rails' currents are. A pin that draws a negligible current (a logic input) makes a balance incomplete and leaves the parts there to their own relations; a design that wants the stage's inductor or a fuse checked states the current.

## Amendment 2026-11-02: a logic input of a part with a rail draws nothing

On a real board many nets were incomplete only because of an IC's logic input: an enable or a mode pin tied to a rail through a resistor or a chain. The fault is the same as the one above run backwards. The pin draws leakage, micro-amps, and the IC's supply current is its rail pins'. With the pin unmodelled, the drops after the chain (a fuse, a ferrite bead, a diode) were never evaluated, the voltages after them were unknown, and the checks that use them were listed as `not evaluated`.

A pin counts as drawing nothing when its kind (ADR 0016) is `input` or `no_connect` in every `pins` table that has it, and the part that owns it declares its supply through a rail (an interface that draws, as above), so the draw of the part is that rail's. A `no_connect` pin must have nothing connected anyway. An `input` pin of a part with no rail stays unmodelled: nothing says where that part is supplied, so what the pin draws is not known. `output`, `bidirectional` and `power` pins, and the `passive` pins of a part that states a `current` or has a rail, are as before; the earlier rule for a part with only `passive` pins is unchanged.

The leakage of an input is ignored, as the quiescent current of a regulator is (ADR 0025): a balance that is complete by this rule is slightly wrong by micro-amps. The same holds for the clause in Limits above: a logic input on a part with a rail no longer makes a balance incomplete. The build's note for an incomplete balance is unchanged; it names the remaining pins, such as the `input` pin of a part with no rail.
