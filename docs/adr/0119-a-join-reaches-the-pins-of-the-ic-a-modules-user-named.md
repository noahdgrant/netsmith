---
status: proposed
---

# A join reaches the pins of the IC that a module's user named

The owner decided on 2026-11-13 (issue #666, epic #638) that a module that takes a part as a type parameter exposes that part: a user who names the IC (`.reg: Buck<Lt8641>`) may join any of its pins through the module, `.reg.ic.pg ~ .pg_pullup.line`, even a pin the bound `BuckIc` does not declare. The owner preferred this to adding optional members to every bound. It relaxes ADR 0116's "only narrowing reaches inside a module" for this one case. The spelling and the reach through a generic below are proposed and wait for the owner.

**Decision.** A path that is an end of a `~` or `~>` may enter the member of a module whose type is one of the module's type parameters (`.ic: Ic`), when the module was placed with the part named (`Buck<Lt8641>`) by the definition being read: a board, or a module that holds it (`examples/two_buck`'s `BuckSupply` joins `.stage.ic.pg`). Inside that member the path reaches the named part's own pins and interfaces (`.reg.ic.pg`, `.reg.ic.vin.pos`), all of them, those the bound declares and those it does not. A name the part lacks is refused at its segment as any missing member is (`` `Zbk2` has no member `pgood` ``). `Checker::joining` is set while an end of a join is resolved (`connect.rs`), and `Checker::visible` lets it through.

**Proposed: the spelling.** No mark. Every member whose type is a type parameter is open to a join from outside; the module writes nothing more. ADR 0116 rejected `expose` for the same reason: an author would have to foresee each use, and std would grow marks. A part has no private pins, and the user who named it can read its pin table. A member that passes the parameter on (`.buck: Buck<Ic>` in a module `Converter<Ic>`) is a module, and stays inside as every module does.

**Proposed: through a generic.** A user who picks through a generic the module provides (`.reg: BuckConverter`) reaches only what the generic declares (ADR 0068), since the IC is chosen after the source is checked and its pins are not known there. The refusal says so: "`buck` is not declared by `BuckConverter`: this use picks its IC through the generic, so the IC and its pins are not known here, and a join reaches the pins of a module's IC only where the IC is named, as in `Converter<Lt8641>`".

**What still stays outside.** The parts of the IC's `support` section: `.reg.ic.boost.a ~ ...` is refused at `boost`, since they are the IC's application circuit and a join would change it (a narrowing still reaches them, ADR 0116). A relation, a default and a read of a value of the IC from outside are refused as ADR 0116 says. The module's own text still reaches only the bound's members (ADR 0115): `.ic.en ~ .vin.pos` in `module Enabled<Ic: BuckIc>` is refused at `en`.

**Rejected.** *Optional members on the bound* (`.pg: Pin optional` in `BuckIc`, wired by the module when present): every IC's extra pin grows every bound, which is what the owner turned down. *Only the pins the bound does not declare*: the owner said any pin, and one rule for every pin of the part is simpler to read than a list that changes with the bound. *A join into every part inside a module*: that is ADR 0116's full reach, rejected there.

**Cost.** A board that joins `.reg.ic.pg` names a pin of the part, so a rename of the pin, or the module's member `.ic`, breaks the board at that line. A user may now join a pin the module also joins (`.reg.ic.fb`), and the module's checks did not foresee it; the net's checks (pin kinds, drivers) still run on the result. `examples/two_buck` has its two power-good pull-ups and test points again (59 parts).

**Left open.** A module that passes its parameter to another (`Converter<Ic>` holding `Buck<Ic>`) does not expose the inner IC: a board that names `Converter<Zbk2>` cannot reach `.reg.buck.ic`. Whether that member counts as typed by the parameter is the owner's.
