# Glossary: Q

**Qualification**:
The family of automotive qualification (AEC-Q100, Q101 or Q200) that a part's datasheet says its ordering code has: the enum `Qualification { None, Aec }` of the prelude, a part's built-in `.qualification`, which its catalog row gives in the fixed column `qualification` when the design states none, and what a board's `.environment.qualification` requires of every fitted part (ADR 0099, provisional). `Aec` is written only where a datasheet or a maker's product page says so, `None` where it says the ordering code is not the qualified one, and an empty cell says nothing: the part is offered and named, never passed. A family and not a standard: the row's `notes` say which AEC document and the page.
_Avoid_: Grade (the temperature grades of AEC-Q100 are an _Environment_'s range, not a qualification), certification, automotive grade (for the column)

**Quiescent current**:
The current a regulator draws from its input for itself, with its output unloaded: the datasheet's quiescent (or ground pin) current. `LinearRegulator` has it as the member `.quiescent_current` (default none), a part narrows it from its datasheet, the regulator's input current is the load and this, and the part's `dc linear_regulator` statement names it as its third operand, so the DC analysis draws it at the input too (ADR 0076, amendment 9). A buck IC states its own, the same member and the `dc switching_regulator` statement's optional fourth operand (the LT8641's `.quiescent_current`, from `.iq_max` in Burst Mode and `.iq_skipping_max` otherwise, ADR 0096).
_Avoid_: Ground current, standby current, supply current (a regulator's supply current is what its input draws, the load included)
