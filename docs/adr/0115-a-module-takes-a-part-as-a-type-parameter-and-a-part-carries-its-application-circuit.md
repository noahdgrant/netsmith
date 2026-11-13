---
status: accepted
---

# A module takes a part as a type parameter, and a part carries its application circuit

The owner decided on 2026-11-13 (issue #619, epic #638) that "any buck that meets these specs" is written with a module that takes the IC as a type parameter bounded by a generic, that an abstract IC has one spelling (`part X provides G`), and that support parts only some ICs need are carried by the IC part. This ADR records the form built. It replaces ADR 0068's rule that a module provider names one part (`Zbk2Buck`, `Zbk9Buck`, one wrapper per IC) and ADR 0025's split of a buck into a part holding `interface Buck` and a `BuckStage` joined by `.stage.buck ~ .u.buck`; the rest of ADR 0068 stands.

## Decision

- `module Buck<Ic: BuckIc> provides BuckConverter { .ic: Ic ... }`. Only a module takes parameters; each is bounded by a generic that parts provide (it has a `pins` table). Inside, a parameter is the type of a member (`.ic: Ic`) or the argument of another module (`Buck<Ic>`), nothing else.
- The module is checked where it is written, with the bound standing for the parameter: a path through `.ic` reaches only the members `BuckIc` declares, as a use of a generic does. A module that is accepted works with every part that provides the bound, and a mistake is said at the module's line, never at a board.
- A board names the part, `.reg: Buck<Lt8641>`, or uses a generic the module provides, `.reg: BuckConverter`. Then the parametric module stands for one candidate per provider of its bound (`Buck<Lt8641>`, `Buck<Zbk2>`), in the bound's order, at the module's place in the generic's order (ADR 0068 section 4); several parameters give every combination, the last varying fastest. `Buck<Lt8641>` is the candidate's name in the notes and the instance's type name; a KiCad sheet file takes `Buck_Lt8641`.
- The trial bound stays `MAX_PROVIDER_TRIALS`, 64 per board, and each `M<P>` is one candidate of it. The search already varies each use alone first, so independent uses cost the sum of their candidates and not the product.
- `interface Buck` is gone from std: `generic BuckIc` holds its members, pins (`.vin`, `.sw`, `.fb`) and checks, and `BuckStage` became `module Buck<Ic: BuckIc>`, which holds the IC (`.ic: Ic`, `.ic.vin ~ .vin`).
- **A `support` section.** What only one IC needs, the parts and joins of its datasheet's application circuit, is a section of the part: `support { .boost: Capacitor { ... }  .bst ~> .boost ~> .sw.pos }`. It holds parts and the statements that wire and size them, and is written only in a part's own definition; a part declares no part outside it. A support part is a member of the part, at `reg.ic.boost`, and a part like any other downstream: picked, checked, drawn, placed and listed. `Lt8641` carries six parts (on BST, INTVCC, VIN1, VIN2, TR/SS and RT) and ties BIAS and EN/UV; its catalog is no longer keyed by `.mpn`, since a board cannot reach the IC inside `Buck`.

## Rejected

Checking each instantiation instead of the definition (C++ templates): a module that misuses a member would be reported at the board that names an unlucky part. A member whose type is the generic and whose providers vary (`.controller: BuckIc` in a non-parametric module): that is a nested pick, so `.reg: Buck` could not name the IC. For the support parts: a module shipped beside the part (`Lt8641App provides BuckIc`), which gives an IC two names, makes the bound module-shaped and loses the pin kinds of the contract; optional members on the generic wired only when present, which puts one IC's pins in every IC's contract; several bounds (`BuckIcWithBst`), one per combination of extras. A larger trial bound: a trial is a whole solve (about 0.2 s on 58 parts), so 64 is already about 14 s.

## Cost

A part may now hold parts, so the model has parts inside parts: the design stage, the placer and the exporters see them as parts at a longer path. A board cannot reach the inside of `Buck<Lt8641>` (ADR 0004), so what a board wired by hand before (the RT resistor, BIAS to the output, SYNC/MODE grounded, a soft-start capacitor, an `.mpn`) is the support section's default until a module's user may narrow its inner parts (#626). `examples/two_buck` lost its power-good pull-ups and test points, which no `BuckIc` member reaches. A parametric provider multiplies the candidates of a generic, so a pick over many ICs reaches the bound sooner.

## Owner decisions (2026-11-13)

- A parametric module placed without arguments (`.reg: Buck`) is refused, and so is a generic that is not its own provider as an argument (`Buck<BuckIc>`). A pick among ICs is written only through the generic the module provides (`.reg: BuckConverter`), as ADR 0001 asks.
- The `support` section inside the part is the form. Its parts are reachable from a board that places the part alone (`.u.boost`), since a part has no private members. A support part's ratings are its part's statements, with no default margin.

## Left open

- A board cannot join a pin of the named IC that the bound does not declare. For example, two_buck's power-good pull-ups needed `.reg.ic.pg`, which `BuckIc` lacks. The owner chose that a parametric module exposes its IC: a board that names the IC (`Buck<Lt8641>`) may join any of that IC's pins, rather than every bound growing optional members. That is issue #666.
