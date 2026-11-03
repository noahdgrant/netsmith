---
status: accepted
---

# There is no `=`

The language had a plain `=` next to `&=` and `==`. With a constant on the right (`.x = 5V`) it did what `&=` does, and with an expression on the right (`.x = .a * .b`) it stated an equation, the same thing that `==` states. That was three ways to say two things, against ADR 0001, and a reader could not tell from `=` alone whether a value was being narrowed or an equation was being stated.

Now `=` is an error that says so: "there is no `=`: use `&=` to narrow a value, or `==` for an equation that must hold". `&=` narrows a field to the intersection with a constant, a range, a tolerance or an expression, and fails if nothing is left. `==` states an equation: with a range already on the left the computed range must lie inside it (and the left then takes it, ADR 0025), and with none yet the left takes the computed value. Bounds stay `>=` and `<=`. The checker has one relation kind for equations, not two.

We did this because there should be one correct way, and because nothing should silently overwrite: `=` reads as assignment, and a narrowing is the opposite of one. atopile v2 draws the same line ("there is no plain `=`"), so a design that moves between the two languages does not have to relearn it. The cost is a few characters on each constant and equation in existing sources, which the error points out.
