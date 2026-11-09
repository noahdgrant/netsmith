---
status: provisional, awaiting the owner's review (the engineer chose the two member names, the relations' form, the quantities they are evaluated at and what is left out; each alternative is under "Choices for the owner")
---

# A buck stage checks the IC's minimum on-time and off-time against its duty cycle

## The problem

A buck converter IC cannot switch for less than a minimum on-time, so at a given frequency the output cannot be made from an input that is too high: the duty cycle `vout / vin` would need an on-time shorter than the IC can make. The same goes for the off-time and an input too close to the output (the IC's dropout). A real 12 V automotive board has two buck ICs on an input that reaches 36 V for a transient. The LT8641 datasheet gives the minimum on-time and off-time and the equations that follow from them (Equations 4 and 5, p.16), and the standard `Lt8641` part writes both as relations of its own. The LM61430-Q1 datasheet gives the same two numbers (7.6 Timing Characteristics, p.9: `tON_MIN` 55 ns typical, 70 ns maximum; `tOFF_MIN` 65 ns typical, 85 ns maximum) and no equation, and the real board's part for it states neither: nothing in the build asked whether 35 V in, 3.8 V out at 2.1 MHz keeps the frequency the board states.

`Buck`, the interface that `BuckStage` reads, has no member for them, so an IC without an equation of its own could not say it. Beyond the datasheet's own words (LM61430-Q1 8.4.3 and 8.1: "frequency is reduced automatically"), what happens past either limit is the same for both ICs: the part still regulates, at a lower frequency, so the ripple of the inductor that the stage computes at the programmed frequency is not the board's. That is why the std `Lt8641` already refuses the board instead of letting the stage compute a ripple at a frequency the part does not hold.

What was already checked, found by building invented boards (a 4.5 V to 60 V IC at 1 MHz, 12 V to 35 V in, 5 V out; a linear regulator fed 3.3 V to 3.6 V for 3.3 V and 0.5 V of dropout):

- A linear regulator's dropout is checked: `LinearRegulator` states `.vin.voltage >= .vout.voltage + .dropout`, and a part written by hand states it the same way. The build says `u.vin.voltage must always be at least (u.vout.voltage + u.dropout), but 3.3V..3.6V is not always above 3.734V..3.866V`. `crates/netsmith-solve/tests/solve.rs` pins it. Nothing is added.
- A buck's minimum on-time was not checked for any IC but the `Lt8641`, and could not be: the interface had no member.

## Decision

`Buck` has two members, both `optional` (ADR 0074):

```nsm-fragment
.min_on_time: Time optional
.min_off_time: Time optional
```

`BuckStage` has two members that say what the stage asks of the IC, and two relations that read the IC's:

```nsm-fragment
.on_time:  Time
.off_time: Time
.on_time == .vout.voltage / (.vin.voltage * .buck.fsw)
.off_time == (.vin.voltage - .vout.voltage) / (.vin.voltage * .buck.fsw)
.on_time >= .buck.min_on_time
.off_time >= .buck.min_off_time
```

The two times are the duty cycle of an ideal buck converter over the switching frequency, evaluated in the worst case the ranges give: the shortest on-time is the lowest output at the highest input and the highest frequency (including the frequency's tolerance), the shortest off-time the lowest headroom at the highest frequency. A violation names the numbers: `stage.on_time must always be at least stage.buck.min_on_time, but 122ns..490.8ns is not always above 150ns`.

A part states a figure from its datasheet with `&=` on its own `buck` (`.buck.min_on_time &= 70ns`), at the largest the datasheet gives, which is the worst case for the design, with the revision, page and table in its doc comment. An IC that states nothing has no check: the relation is skipped, not listed as not evaluated, as for the two inductor limits (ADR 0074). No std part states them: the `Lt8641` keeps its own Equations 4 and 5, which include the switch drops (about 0.15 V and 0.3 V) the stage does not know, and a second, stricter relation beside them would refuse boards the datasheet's own equation accepts. The `Buck` doc comment says that an IC with its own equation writes it.

## What was rejected

- **`.max_duty_cycle` or `.dropout` as the member.** Both datasheets give a time (`tOFF_MIN`), and a maximum duty is `1 - fsw * tOFF_MIN`: it depends on the frequency, so a number would be wrong at every frequency but one. The time is the datasheet's figure and stays right when the frequency is changed.
- **The equation with the switch drops** (Equation 4 and 5 of the LT8641). `Buck` has no switch-drop members, and a datasheet that gives none (the LM61430) would have nothing to state. The ideal duty cycle gives an on-time that is never longer than the real one (the real duty cycle, `(vout + vsw_bottom) / (vin - vsw_top + vsw_bottom)`, is larger), so the check is conservative: it can refuse a board that holds its frequency by a few percent, and never accepts one that does not.
- **Evaluating at the rail's `.peak`.** The peak is a transient (a 36 V clamp on a 16 V rail), where the datasheets say the part lowers its frequency and recovers; the steady `.voltage` is where the frequency is meant to hold. The peak stays a rating (`.vin.peak <= .vin_abs_max`).
- **A required member.** Existing parts and boards would stop building (ADR 0069, `required`, was dropped for this reason).
- **A note instead of an error.** A frequency that is not held makes the stage's ripple and the inductor's saturation check wrong, which is why the check is a refusal, as it is for the `Lt8641`. A board that wants the part to run in foldback widens its own window, or states a lower frequency.
- **Folding the check into the IC's own relations only** (what the `Lt8641` does). That leaves each IC to write the same two lines; the stage is where the output, the input and the frequency meet.

## Cost

Two members on `Buck`, two on `BuckStage`, four lines. Each existing board builds as before (the new relations read members that nothing states, so they are not in the model's list; the two times are solved and cost nothing visible). The two times appear in the stage's values.

## Choices for the owner

1. The names `.min_on_time` and `.min_off_time` (alternatives: `.t_on_min` and `.t_off_min`, which the `Lt8641` already uses for its own fields; the interface's other members spell out `min_`).
2. Whether the on-time should be checked at all for a datasheet that says the part lowers its frequency instead of failing (here yes: the stage's ripple is then not the board's).
3. Whether a std part that states only the on-time (no equation) should be added, and whether the `Lt8641` should state its 50 ns and 110 ns on `Buck` as well, in which case its Equations 4 and 5 become redundant, and stricter than the datasheet by the switch drops, if the stage's relation stays ideal.
4. The ideal duty cycle (here) against one with switch-drop members on `Buck`.
5. Whether `BuckStage` should also read `.vin.peak` against a separately stated figure (the figure at which the part keeps regulating at all).

## Left open

- The real board's LM61430 part states neither number. With `.buck.min_on_time &= 70ns` and `.buck.min_off_time &= 85ns` (the datasheet's maxima) the stage would check them; the numbers the board would see are in the pull request that adds this ADR. A project's part is the board's to change.
- #63's other items stay open: a second, different capacitor on each side (a bank of identical capacitors exists, ADR 0052; a second capacitor with its own value is not in the issue in a form that can be built), loop stability, and the divider's and the IC's quiescent current in a buck stage's load (ADR 0076, amendment 9 did the linear regulator's).
- A pin-free supplier rail inside a module. A rail declared in a part and joined to nothing needs no pad (it builds), and a rail that is connected needs its `pos` on a pad or an alias of one (`.out.pos ~ .sw`, which also builds). What is not possible is a rail whose `pos` is on a net and is no pad's alias: the supply of a module would be a pin that the IC does not have. The question is what a rail is when nothing physical stands for it, and it is a language question, the owner's. It is not changed here.
