---
status: accepted
---

# A join reaches the pins of the IC that a module's user named

The owner decided on 2026-11-13 (issue #666, epic #638, PR #686) that a module that takes a part as a type parameter exposes that part: a user who names the IC (`.reg: Buck<Lt8641>`) may join any of its pins through the module, `.reg.ic.pg ~ .pg_pullup.line`, even a pin the bound `BuckIc` does not declare. The owner preferred this to adding optional members to every bound. It relaxes ADR 0116's "only narrowing reaches inside a module" for this one case: joins to a named IC's pins.

**Decision.** A path that is an end of a `~` or `~>` (or of a join by name, ADR 0117) may enter a member that stands for a part the definition being read named: a board, or a module that holds the parametric module (`examples/two_buck`'s `BuckSupply` names `Buck<BenchBuck>` and joins `.stage.ic.pg`). Inside that member the path reaches the named part's own pins and interfaces (`.reg.ic.pg`, `.reg.ic.vin.pos`), all of them, those the bound declares and those it does not. A name the part lacks is refused at its segment as any missing member is (`` `Zbk2` has no member `pgood` ``). `Checker::joining` is set while an end of a join is resolved, and `Checker::visible` lets it through.

**No mark.** Every member typed by a type parameter is open to a join where the part is named; the module writes nothing more. ADR 0116 rejected `expose` for the same reason: an author would have to foresee each use, and std would grow marks. A part has no private pins, and the user who named it can read its pin table.

**The join follows the parameter.** A member is open when its type is the parameter the user named, or it is reached through members typed by it. `Converter<Ic>` holding `.buck: Buck<Ic>` exposes the inner IC, so a board that names `Converter<Zbk2>` joins `.reg.buck.ic.pg`; it enters `.reg.buck` only on the way to `.ic` (`.reg.buck.divider` stays inside). `Checker::param_args` records, for each module made with arguments, which of them were the enclosing module's own parameters, and the checker follows that chain outward to the definition that named a part. A part named inside a module (`.stage: Buck<Zbk2>` in `Supply`) is open to `Supply`'s text, not to the board that places `Supply`. A parametric module's own text never reaches beyond its bound, even through a module it gives its parameter to (ADR 0115): the refusal says that the part was not named there.

**Through a generic.** A user who picks through a generic the module provides (`.reg: BuckConverter`) reaches only what the generic declares (ADR 0068), since the IC is chosen after the source is checked and its pins are not known there. The refusal says so: "`buck` is not declared by `BuckConverter`: this use picks its IC through the generic, so the IC and its pins are not known here, and a join reaches the pins of a module's IC only where the IC is named, as in `Converter<Lt8641>`".

**What still stays outside.** The parts of the IC's `support` section: `.reg.ic.boost.a ~ ...` is refused at `boost`, since they are the IC's application circuit and a join would change it (a narrowing still reaches them, ADR 0116). A relation, a default and a read of a value of the IC from outside are refused as ADR 0116 says.

**Rejected.** *An `expose .ic` mark*, for the reason above. *Optional members on the bound* (`.pg: Pin optional` in `BuckIc`, wired by the module when present): every IC's extra pin grows every bound, which is what the owner turned down. *Only the pins the bound does not declare*: the owner said any pin, and one rule for every pin of the part is simpler to read than a list that changes with the bound. *Only a member whose type is the parameter itself*: a board that names `Converter<Zbk2>` named the IC as surely as one that names `Buck<Zbk2>`. *A join into every part inside a module*: that is ADR 0116's full reach, rejected there.

**Cost.** A board that joins `.reg.ic.pg` names a pin of the part and the members on the way to it, so a rename of the pin, of `.ic` or of `.buck` breaks the board at that line. A user may join a pin the module also joins (`.reg.ic.fb`), which the module's checks did not foresee; the net's checks (pin kinds, drivers) still run on the result. `examples/two_buck` has its two power-good pull-ups and test points again (59 parts).

**Left open.** Nothing.
