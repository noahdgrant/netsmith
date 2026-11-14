# Glossary: U

**Unconstrained pick**:
A part of a catalog with a value column (a resistor's resistance, a capacitor's capacitance, an inductor's inductance) that was picked while nothing limited the value, so the solver took the first row that fits (ADR 0065). A value is limited by a statement (`&=`, an applied default, a range a member carries in), by a bound that reaches it, or by a check or a current budget that the pick decides and that was judged once the board was solved (a budget counts only if the pick changes whether it holds); a check that holds whatever the size of the values is, such as the output voltage of a divider, limits their ratio and not their size, and the finding says so; a bound on a rating and a relation a part's own definition states do not limit it. A guideline finding: a warning by default, an error with `--strict` or `guidelines = "error"`. A range, however wide, is how a board says that any value will do.
_Avoid_: Unset value, free pick, default pick, loose part

**Unit**:
One of the drawings of a KiCad symbol that has several, such as the two amplifiers and the supply of a dual op-amp, or the power unit and the signal unit of a processor module (`U1A`, `U1B`). The KiCad schematic places all units of a part, each with the part's reference; the editor's schematic does the same: each unit that has pins is a box of its own in the drawing, as the library draws it, with the part's reference and the unit's letter, upright, and the decoupling capacitors stand beside the power unit (ADR 0050). A pad is in one unit. The layout's own stack of two-terminal parts in a column is a different thing and is not called a unit in the docs.
_Avoid_: Sub-part, section, gate (a unit may be a gate, an amplifier or a supply)

**Unstated value**:
A value declared in a module or a board (not in a part, an interface or a catalog column) that a check reads and that nothing states or works out, so the check is not evaluated (ADR 0065). Only the root is named: a member that a wrapper forwards to is not one, the wrapper's is. A guideline finding with the same severity as an **Unconstrained pick**.
_Avoid_: Missing value, unset member, open value

**Unused interface**:
An interface of a part or a module that the board states nothing on it uses, `unused .processor.usb` (ADR 0083). Its pins need no source and no connection, the relations that read only its values (a limit the datasheet states for the function) are not checked or listed as not evaluated, and its levels and absolute maximums do not run. Not "nothing is connected": a net that goes only to test pads, the return to ground, and a series `Resistor`, `Fuse` or `FerriteBead` that pulls a pin to the return or to a rail of the board that nothing on the board sources (the supply: a pull-up or a pull-down), are not uses. A rail that a regulator, a `dc source` or an output pin on the board sources is not the supply: a path to it is a use. A board that also uses it (a pin on a net with a pin of another part that is not passive, also through a series `Resistor`, `Fuse` or `FerriteBead`, however many; a `~` to another interface; a function chosen on a pin) is an error that shows both lines.
_Avoid_: Disabled function, skipped port, ignored interface, `no_connect` interface

**Use of a generic**:
A member declared with a generic's type (`.rf: CoaxReceptacle { ... }`). The solver puts one provider in its place, an instance of that part or module at the use's own path (`rf`), and the build says which and why the providers before it were passed over. A use reaches only the members the generic declares, so a board works with every provider; a board that wants exactly one writes the provider as the type (`.rf: AcmeMicroCoax`), and nothing is picked (ADR 0068).
_Avoid_: Instantiation, binding
