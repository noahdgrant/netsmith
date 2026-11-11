---
status: accepted
---

# A field the checker reads by its name is on a list

Owner decision of 2026-11-11, from the review of ADR 0086. Meets the second test: a part gets a value because one of its fields has a certain name, which the part's own code does not show, so the obvious reading of a part gets it wrong.

**Decision.** Eight member names of a part are read by the checker or the solver, and only they. They are a convention of a name, not a keyword; a part opts in by declaring a member of that name and the right dimension, and a part that does not declare it is not given a value.

- `bias` (ADR 0013): the voltage across a part with a `bridge`, from the rail it sits across.
- `bridge_voltage` (ADR 0088): the same with a sign, the first terminal of the `bridge` to the second.
- `current` (ADR 0014): the current a chain draws from the net it starts on.
- `supply_peak` (ADR 0086): the highest voltage of the rail a `~>` chain starts on.
- `supply_reverse_peak` (ADR 0091): the highest voltage the supply reaches the wrong way round, as a positive number.
- `limit_current` (ADR 0091): the most current a part lets through, such as a fuse's rating.
- `supply_fault_current` (ADR 0091): the largest current the part in front of it that has a `limit_current` lets through.
- `count` (ADR 0052): the number of identical capacitors a part stands for.

The list is the list: the checker's lookups are `bias_of`, `bridge_voltage_of`, `supply_peak_of`, `supply_reverse_peak_of`, `supply_fault_current_of`, `limit_current_of` in `crates/netsmith-check/src/model.rs`, which reads `count` the same way, and the solver reads it too. A ninth name is a decision of its own: an ADR that adds it here, with the part it is for.

**Why a name and not syntax.** The owner reviewed the alternatives of ADR 0086 on 2026-11-11 and kept the convention. A syntax that names the carrying (`.fuse: Fuse { .supply_peak ~ rail.peak }`) adds a keyword and still has to find the rail. Naming the rail (`.fuse.rail ~ .input`) makes every board write the line the rule removes. A marker on the field would be a second spelling of what the name already says, and `docs/DESIGN-RULES.md` asks for one way.

**Cost.** A newcomer will not find the rule by reading a fuse alone. The doc comments of `Fuse`, `Diode`, `Tvs` and `Rail`, and the guide, say it, and this list is where a new part's author looks. Each part that follows the convention makes a later move to syntax dearer; the list keeps that number known (today the names are used in `std/parts/passive.nsm`, `std/parts/diode.nsm` and `std/sources.nsm`).

**Left open.** Nothing about the names. A part in a chain through a module's port, a drop of earlier parts that is not subtracted, and `Switch` and `Inductor` having no `supply_peak` are limits of ADR 0086, kept there.
