# Glossary: E

**Editor**:
`netsmith editor`: a web app served on 127.0.0.1 by the netsmith binary, with seven sections (Source, Schematic, Power tree, System, BOM, PCB, Build; the number keys 1 to 7 pick them in that order), shown one at a time or two side by side; the Source section has the project tree and a source editor that shows the compiler's and solver's messages beside their lines, the others are the views of a board; and two actions in the top bar, Write KiCad files and Make fab files (ADR 0019). The schematic is shown one module (instance) at a time. A long job runs on the server while the page stays usable. It writes the source files (in its source pane), the physical file (the requirements a person draws, such as a keepout) and the layout file (the solution the PCB view moves). It reads the physical file for the PCB view and lists it in its tree; its drawn keepouts are still written to the layout file (the write path to the physical file is not built yet, issue #52). It replaces the schematic viewer, which is gone.
_Avoid_: Viewer, IDE, GUI

**Effective capacitance**:
A capacitor's `.effective_capacitance`: the marked value after the DC bias across it and its tolerance, in the worst case over the range the bias can take. A ceramic capacitor holds less than its marked value as the bias rises, so a design states what it needs of the effective capacitance where the circuit depends on it, such as a regulator's output capacitor. The solver works it out from the part's published curve, and a part with no curve cannot be used for it. Temperature and ageing are not part of it yet.
_Avoid_: Derated capacitance, usable capacitance

**Enclosure**:
A range that holds whatever a pick later chooses, written before the picks so that a pick that reads a voltage can be judged: each part still to be picked at the hull of what its candidates give its `dc` value (or, when the design limits its current, a load of at most that limit), and a regulator's load: the sum of what the loads on its output draw, each over every pick it could have, when every load has a current the design's relations give, and otherwise (a load that states nothing) from nothing up to the limit the regulator states on what its output supplies. The pass after the picks narrows it to the circuit that was picked (ADR 0076, amendments 5, 6, 7 and 11).
_Avoid_: Seed, start window (those are a range the board writes by hand and states as a claim; an enclosure is worked out and is not stated), bound (the bound from Ohm's law is another, cheaper thing); for the box a board goes in, say _Case model_ or case

**Enum**:
A type whose values are a fixed list of named variants, declared with `enum Name { A, B }` and imported like a definition (ADR 0054). A variant is written with its enum, `Name::A`; a field of the enum type takes only its variants, with `&=`, `?=` and `==`. `AssemblyMode` is the one enum in scope in every file without an import (`std/prelude.nsm`).
_Avoid_: Option set, choice list, constant, tag, quoted word (for a value that is one of a fixed list)

**Environment**:
What a board is exposed to, the member `.environment` of a board: an interface such as `std::env::Environment` whose `.temperature` is the range of temperatures its surroundings reach, `-40degC..125degC` (ADR 0099, provisional; the `degC` unit is ADR 0103). Every fitted part is checked against it. The manifest's `environment` names the definition a board gets when it declares none; a board with neither is not judged and says nothing. A board can also require a qualification of every fitted part, `.environment.qualification &= Qualification::Aec`. A library states no environment of its own: it is the board's, or the project's.
_Avoid_: Conditions, operating conditions, grade (a grade is a qualification and a margin; only the qualification is built), mission profile

**Escape lane**:
The way out the router keeps in front of a pad of a fine-pitch part: the cells straight out from the pad's edge, away from the part, on its layer, 1.5 mm long. Other nets' tracks pay to use it and any via that would close it pays more, except the pad's own via at its end; the two nets of a diff pair share their lanes. A cost and not a wall, used from the router's second try of a board (`Lanes` in `netsmith_router::route`, issue #431).
_Avoid_: Fan-out (a fixed track and via for each pad, which the router does not draw), reservation, keepout (a wall)
