---
status: provisional, awaiting the owner's review (the owner approved on 2026-11-08 a member of `BuckStage` that states a margin on its capacitors' ratings; the member's name, its default, the quantities it applies to and the alternatives below were written by an agent)
---

# A buck stage states a voltage margin for its capacitors

## The problem

A real 12 V automotive board states a rule: every ceramic capacitor on a power rail is rated at least 1.25 times the rail's peak. `BuckStage` rates its input and output capacitors for the rail's peak and states no margin, and the capacitors are inside the stage: a board or a module cannot reach them (`input_cap is inside BuckStage and is not part of its interface`). The project kept a word-for-word copy of the std stage with two lines changed, which has to follow every change to the std stage by hand.

## Decision

`BuckStage` has a member `.voltage_margin: Number` with the default `?= 1`. Each capacitor of the stage is rated for the margin times the highest voltage of its rail:

```nsm-fragment
.input_cap.max_voltage >= .vin.voltage * .voltage_margin
.output_cap.max_voltage >= .vout.voltage * .voltage_margin
.input_cap.max_voltage >= .vin.peak * .voltage_margin
.output_cap.max_voltage >= .vout.peak * .voltage_margin
```

These four lines replace the four lines that asked for the rail's steady voltage and its peak. With the default 1 each is the line it replaces (`x * 1` is exact), so a board that does not state the margin builds with the same picks and notes, which the existing tests and the guide check; only the text of a failing message differs, which now quotes the product. A board states the margin from outside (`.reg.stage.voltage_margin &= 1.25`), and a module that wraps the stage forwards its own member (`.stage.voltage_margin &= .margin`). The default is a `?=`, so the board's `&=` replaces it (ADR 0051).

The margin applies to the steady voltage as well as to the peak. A rail that states no peak leaves the peak lines not evaluated (the stage's existing behaviour), and a margin that applied only to them would silently do nothing on such a rail. A peak is never below the steady voltage, so a board that states its peak gets the same result as with the peak lines alone.

The stage has no bootstrap or bypass capacitor: those are inside the regulator IC's own circuit, which a part or module writes, so the member covers the two banks the stage has. A module that adds such capacitors states its own bound.

## What was rejected

- A separate margin per side (`.input_margin`, `.output_margin`): two members to state one rule. One margin is the rule the board states; a board that needs another number for one side can still state that capacitor's bound if the stage exposes it, which it does not.
- A margin only on the peak, as the project's copy has it. The result differs only on a rail with no stated peak, where the project's lines wait and nothing is checked: that is the case where a margin is most easily lost.
- Exposing `.input_cap` and `.output_cap` in the stage's interface so a board can state `.stage.input_cap.max_voltage >= ...` itself. It is a larger change to what the stage exposes, and it leaves each board to repeat the rule, the copy this ADR removes.
- A `Rail`-level margin (`.vin.margin`): every part on the rail would need to read it, and a rail has no reason to know how parts are derated.
- A default of 1.25. It changes every existing board and every golden output; the default 1 does not.

## Cost

One member on the stage and the four lines read it. A margin the catalog cannot meet is refused with the numbers of the closest row, as any rating is.

## Choices for the owner

1. The name `.voltage_margin` (alternatives: `.derating`, `.rating_margin`).
2. Whether the margin applies to the steady voltage as well as to the peak (here yes; alternative: the peak only, which matches the project's copy exactly).
3. Whether the std stage should expose the capacitors of its banks instead (rejected above).
4. Whether other std modules that hold capacitors across a rail should take the same member when they are added. None exists today besides `BuckStage` (`std/` has no other regulator module).

## How the project deletes its copy

It states `.stage.voltage_margin &= 1.25` where it uses `BuckStage` (or forwards its own member from the module that wraps it) and deletes its copy of the stage. Its lines are `.vin.peak * margin` and `.vout.peak * margin`; the std lines are the same two, plus the same factor on the steady voltage, so no row it picks gets weaker.
