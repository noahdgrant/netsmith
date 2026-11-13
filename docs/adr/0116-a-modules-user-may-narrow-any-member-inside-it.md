---
status: accepted
---

# A module's user may narrow any member inside it, and do nothing else there

The owner decided on 2026-11-13 (issue #626, epic #638) that a module's user may narrow any member inside the module by its path, and that there is no `expose` keyword. This replaces ADR 0004, which kept the parts and modules inside a module out of reach of everything outside it. It meets two tests: it is hard to reverse (std drops the members that only passed a value inward, and boards narrow inside modules), and it rejects an obvious alternative (a module that marks what it exposes).

**Decision.** A path may go through the parts and modules inside a module on the left of a `&=`, at any depth, from a board, from another module, or from a declaration's block: `.stage.divider.rtop.resistance &= 100kohm +/- 1%`, `.usb: UsbCSink { .receptacle.package &= "SMD" }`, `.reg.ic.soft_start.capacitance &= 22nF +/- 10%` into a `support` part (ADR 0115) of the IC that `Buck<Lt8641>` names. A narrowing only takes values away, so whatever the module states still holds: a value the module fixed and the user's narrowing that misses it is the usual contradiction at the user's line, and a module that is right for every value it allows stays right for the narrower one. The solver reads it like any `&=` (`Checker::narrowing` in `crates/netsmith-check`, set while the left side of a `&=` is resolved). A narrowing into a use with several providers that asks for a package the first provider has no pin table for opens the provider search, as a contradiction there does (ADR 0113).

Everything else stays outside: `~`, `~>`, `==`, `<=`, `>=` and `?=` on a path inside a module are refused at the segment that enters it, with "only narrowing (`&=`) reaches inside a module". A join inside would change the module's circuit, a relation would add a rule its author never saw, and a default would compete with the module's own. What ADR 0004 said still holds for them: a module is joined and configured through its rails, pins and declared values. A use of a generic still reaches only what the generic declares (ADR 0068). The IC of a parametric module is narrowed as the part the user named (`Buck<Lt8641>`), so its `support` parts are there, while the module's own text reaches only its bound's members (ADR 0115). A board's physical file keeps ADR 0048's reach by path.

**Removed from std.** The members that existed only to pass one value inward: `UsbCSink`'s `.package`, `.receptacle_mpn` and `.rd_package` (now `.receptacle.package`, whose `?=` default moves onto the receptacle, `.receptacle.mpn`, and `.cc1_rd.package` and `.cc2_rd.package`), and the `.package` of `PullUp` and `PullDown` (now `.resistor.package`). ORBIT's buck-boost uses `FeedbackDivider` and narrows its two resistors, where it wrote the divider's equation itself.

**Kept in std.** A member that does more than pass a value: `.package` of `FeedbackDivider` and `I2cPullups` holds the two resistors to one package, and `Buck`'s `.voltage_margin` is a relation between each capacitor's rating and its rail's voltage, which a `&=` cannot state (a constant rating can be narrowed, `.reg.input_cap.max_voltage &= 16V..100V`). `.assembly_mode` (ADR 0042) stays the way a module offers one choice for all its parts.

**Rejected.** *An `expose .divider` mark* on the members a user may narrow: every module author must foresee every need, which is the problem ADR 0004 had, and std would grow marks instead of members. *Full reach*, as ato v2 has (joins, relations and defaults inside a module): a board could rewire a module or add rules its author never checked, and a second way to join a module would break ADR 0001. *Widening or replacing* a value inside: the module's checks were written for the values it allows.

**Cost.** A board that narrows inside a module names the module's members, so a rename inside the module breaks the board at the line that names it (the checker names the path). A value can now be stated two ways where a module also offers it as a member (`.a.assembly_mode` and `.a.j.assembly`); std removes the members that only forwarded, and a module keeps one only when it does more. Boards that stated a forwarded member change their text once (`examples/easyduino_esp32s3`, `examples/orbit`, `examples/rp2350_minimal`).

**Proposed, for the owner.** Two readings the decision did not settle, built provisionally:

- The right side of a `&=` reads nothing inside a module: `.x.resistance &= .s.r.resistance` is refused, because a read is not a narrowing and a module's inner values may change with its build. The alternative is a read-only reach for every right side.
- A narrowing from outside counts as a statement of the field, so it replaces a `?=` default the module wrote on it (ADR 0051), as `.receptacle.package` does for `UsbCSink`'s default package. The alternative is that a default inside a module can only be narrowed alongside, never replaced.

**Left open.** A board cannot join a pin of a named IC that the bound does not declare (issue #666). The `lt8641` catalog is still not keyed by `.mpn`, though a board may now narrow `.reg.ic.mpn`.
