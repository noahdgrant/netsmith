# Glossary: O

**Ohm bound**:
The largest size of the current through a resistor that Ohm's law allows when the voltage at each of its two ends lies in a range the solver knows: the largest difference between the ends over the least resistance (ADR 0026, amendment). It is used only to prove a check that waits on a current that nothing states: a relation it satisfies is made and its note goes, one it does not satisfy stays not evaluated and says nothing. It is not the part's current: the model never holds it as a value, and a pick, a range or a track width reads no current from it, except that the track note goes when the width already drawn carries the known current and every Ohm bound together (ADR 0036, amendment 4).
_Avoid_: Estimated current, assumed current, worst-case current (a current is worked out or stated; this is neither)

**On-resistance test voltage**:
The `.on_resistance_vgs` of a `NMosfet` or a `PMosfet` and the column of that name in the `n_mosfet` and `p_mosfet` catalogs: the gate-source voltage that the datasheet guarantees `.on_resistance` at, the test condition of its RDS(on) row (ADR 0092). The gate drive the design gives the part (`.gate_source_voltage`) must always be at least it. An optional column: a row that records no voltage is offered and the check is not evaluated for it, and a design that states a requirement on the member is offered only the rows that record one.
_Avoid_: Test Vgs, drive voltage (for the member), gate rating

**Open circle**:
A circle of values that stayed without a value after the solver's own start, with a bound (`>=` or `<=`) that reads it, or a value worked out from it, left unevaluated. The build says so as a guideline finding (rule `open circle`): a `warning:` that names the fields, the checks and a field to start from, and an error under `--strict`. A circle that also reads a value nothing states names that value instead (ADR 0057, proposed). A circle may close through the currents of a net instead of an equation: a net of more than two currents, where the solver does not work a part's current out (ADR 0026), as a divider behind a fuse or a resistor does (issue #18); the finding then names the net and its currents, and a range on the part's current or a voltage in the circle starts it. A current that is exactly zero is not a term of the net.
_Avoid_: Unstarted loop, stuck cycle

**Open-drain line**:
A line that a device only pulls low or releases, and a resistor to a rail holds high, such as the `sda` of an `I2c` bus. The levels check (ADR 0075) recognises a driver of one by what its part states, a `vol` and no `voh`, on a net that a resistor joins to the positive pin of a rail: the line's high is that rail's voltage range, which the receivers' `vih` is judged against. When the DC analysis solves the line (every driver states `vol` at a test current), the high is the voltage of the line with every driver let go, which also sees a divider or a diode in the pull-up (ADR 0076, amendment 8, provisional). There is no kind or marker for it in the language.
_Avoid_: Open-collector (KiCad's name for the pin type, which netsmith does not have), wired-AND

**Operating point**:
One solution of the network of the picked parts at DC: a voltage at each node and a current in each element, for one choice of a value in each range and one state of each diode. The analysis solves every one of them in an island and keeps, for each current and voltage, the range it takes over all of them (ADR 0076).
_Avoid_: Bias point, quiescent point, snapshot (a result is the range over the operating points, not one of them)

**Optional column**:
A column of a part's `catalog` statement that ends with `optional`, `catalog tvs { ..., .breakdown_voltage optional }`, in a standard part or a project's own (ADR 0078). A file of the catalog may leave it out and a row may leave its cell empty; such a row is offered to a design that says nothing about the field and is not offered to one that states it. Any column that is not marked needs a value in every row. It is the same word as in an **Optional member** and in a `pins` row, but a separate thing: a member's marker skips a check, a column's marker lets a row be empty.
_Avoid_: Nullable column, sparse column, optional field

**Optional member**:
A value declared with the word `optional` after its type, `.max_inductor_dcr: Resistance optional`, in a part, a module, an interface or a generic (ADR 0074). A design may leave it unstated, and a relation that reads it is then skipped: not checked, not listed as not evaluated, no finding. It is skipped only when nothing **states** the member: an `&=`, an applied `?=`, an equation with the member alone on its left, or a bound against numbers alone (`<=`, `>=`), anywhere in the design, also on the other side of a `~` join. A relation that reads another value that nothing states is still not evaluated, once the optional member is stated. It is the same word as `optional` in a `pins` row and in a catalog column (an **Optional column**), a modifier that says a thing may be absent. Not the `required` of ADR 0069, which was dropped.
_Avoid_: Nullable, maybe, conditional check

**Order**:
A named list of boards and quantities in `netsmith.toml` (`[order.devkit.boards]`, each key a board as its folder is named, `Board` or `System/member`, a system for all its members, or a panel), made by `netsmith fab --order devkit` into `fab/orders/devkit/`: the package of each board, an `order.txt` summary and a combined bill of materials for the quantities (ADR 0035). It chooses which separate packages are ordered together; a panel's quantity counts panels.
_Avoid_: Batch, job, build set

**Outline**:
The board edge, the board's member `.outline` (`std::pcb::Outline`): a width, a height and a corner radius, or a **polygon**, the statement `polygon (x, y) (x, y) ...` in its block, an ordered run of corners where `round R` after a corner is a tangent fillet and `arc through (x, y)` makes the edge to the next corner an arc (issue #57). Without one the board is as big as its parts need. The origin of the board's frame is its top left corner (of the polygon's bounding box).
_Avoid_: Board size, board shape

**Output ripple**:
The peak-to-peak ripple of a buck stage's output voltage, `.output_ripple` of `BuckStage`: `dI / (8 * fsw * n * C) + dI * ESR / n` for the inductor's ripple current `dI`, the bank's count `n`, one capacitor's effective capacitance `C` and the board's `.output_esr` (an **Optional member**), a **Closed-form check**. A board bounds it (`.stage.output_ripple <= 20mV`); with no ESR stated it has no value and the bound is listed as not evaluated. Not the ripple current of the inductor (`.ripple`).
_Avoid_: Voltage ripple, output noise, ripple voltage

**Overhang**:
A part whose courtyard is past an edge of the outline on purpose, such as a plug that sticks out of the case. The design says it may (ADR 0045; the rule is in the header of `crates/netsmith-pcb/src/overhang.rs`): a requirement on the part's own `.place.left`, `.right`, `.top` or `.bottom` whose whole window is at or beyond the same edge of the outline, A lock in the layout file does not say it: a locked part states every edge it is past, like any other part (owner decision, 2026-11-03). Then the courtyard past that edge is not an error and counts as clipped to the outline, also in the placer's overlap cost; pads are still checked, an edge not stated is still an error (also for a locked part), and `fab` prints each overhang as a warning and lists it among the warnings in `fab-notes.txt`. The body (`F.Fab`) is not checked.
_Avoid_: Hang-over, protrusion, outside the board (an undeclared overhang is the error `not wholly inside the board outline`)
