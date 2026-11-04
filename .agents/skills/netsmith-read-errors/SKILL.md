---
name: netsmith-read-errors
description: Read netsmith's checker, solver, lowering and design-rule messages and decide what to change in the .nsm file. Use when netsmith build, editor or fab prints an error or a note, or the editor's problems list shows one.
---

# Read netsmith's messages

Fix errors one at a time, from the first one. Later errors are often caused by earlier ones. The language guide (`docs/netsmith/LANGUAGE.md`) has each message with its cause, under the construct it belongs to.

## The shape

`error: <what is wrong>` then `--> file:line:col` and the source line. The message names the field or path. The caret marks the statement.

## Checker messages (the file is malformed)

| Message starts | Change |
|---|---|
| `... is not a member of this X` | Fix the spelling. Read the part's fields in the guide's catalogue. |
| `... is not defined in any enclosing block; write .x` | Add the leading dot. |
| `... holds Ω, but this value is A` | Use a unit of the field's dimension. |
| `contradiction on ...` | Two narrowings do not overlap. Widen one or remove it. |
| `... names a net, and only a board names nets` | Move `.net &=` to the board, through the module's rails. |
| `... is inside X and is not part of its interface` | Set it through a rail, pin or declared value of X. Or declare a value in X. |
| `... cannot be used in a ~> chain` | The part has no `bridge`. Use `~` between pins. |
| `these pins are connected but name their net differently` | One net, one name. Remove one. |
| `cannot connect a X to a Y` | `~` joins interfaces of one kind. Join their pins. |
| `... must be connected (pad N, NAME), but nothing else is on its net` | The part's pin table says this pin cannot float. Connect it, or the datasheet allows it open and the table should say `optional`. |
| `... must be on the same net as ... (X ties them)` | The part has a `tie`. Join the two pins in the board. |
| `... is a no_connect pin ... but it is on net ...` | Leave that pin alone. |
| `... does not exist in package P of X` | The package has no such pin. Drop the connection, or use a package that has it. |
| `... has several functions ... choose one with` | Add `.u.pin.function &= "NAME"` for the function the pin has in this design. |
| `not evaluated: the logic levels of ...`, `not evaluated: the high level from ...`, `not evaluated: the absolute maximum of ...` | A note, not a pass: the checker could not compare the levels (the grounds are different nets, a level has no value, nothing sets the net's voltage). State the missing `.vih`, `.voh` and so on in the part, or join the grounds. |
| `net N: ... .voh is ... but ... .vih needs ...` | A driver's lowest high is below what the input needs (or its highest low above what the input allows). Change the supply of one side, a level shifter, or the part. |
| `net N reaches V above its ground G (...) but ... .abs_max is ...` | The net can go past what the pin survives. The text in brackets says what sets that voltage. |
| `net N has two outputs` | Two outputs drive one net. Remove one connection. A pin shown as `` `a.pa2` (`Mcu` function `UART1_TX`, an output) `` is an output because the function chosen for it is: choose another function for one of them. |
| `... are both headers`, `... both sockets`, `... has N positions and ... has M`, `... has a pitch of A and ... has B` | The two mated connectors do not fit: a header goes with a socket, with the same positions, rows and pitch. Use the matching part. |
| `contact N of the mated connectors ...` | The boards disagree about what contact N is: ground against power or a signal, two outputs, or an input nothing drives. The message names the pin and net on each side; fix the wiring of one connector. |
| `the mate of ... carries power or signals ... but no ground` | Wire a ground to a contact of both connectors. |
| `... are already joined by the mate of ...` | Remove the `~` between those ports: the mate joins them. |
| `... is inside ..., a X; a system can reach only the .mating of a connector` | A system mates connectors and names no pin: join `.board.connector.mating` to another. |

## Solver messages (the file is fine, the numbers fail)

| Message starts | Meaning and change |
|---|---|
| `the X catalog has nothing for P, which needs ...` | No real part lies inside the ranges. Loosen the range or pick another value. The location points into the library, because the part is what has no candidate. |
| `no X candidate satisfies P: the closest ...` | Candidates fit, but a relation fails. The text names the closest one and the relation with its numbers. Loosen the requirement, tighten a tolerance, or change the circuit. |
| `no combination of candidates satisfies A and B together` | Same, for parts that depend on each other. |
| `... is unknown for some candidates` | The capacitor catalog has no bias curve for some parts, or the bias is unknown. Place the capacitor between a rail's pos and neg, or state `.bias`. |
| `... must always be at least / at most ...` | A bound fails in the worst case. The numbers show by how much. |
| `the currents on net ... do not add up` | Load and supply disagree. `must stay within A, but the others on it work out to B` means a stated current is a budget and the circuit draws more (ADR 0038); a stated range need only contain what the rest of the net determines. Check the stated draws and the supplier's `max_current`. |
| `the currents at the join ...` | Two boards disagree about a port's current. |
| `no header candidate satisfies ...: ... (N * X.mating.pin_current) must always be at least ...` | The contacts of a mate cannot carry the rail: N is how many contacts carry it and the rating is the datasheet's. Use more contacts for the supply and the ground, or a connector with a higher rating. |
| `gave up after trying 250000 combinations` | Narrow what the parts need. |
| `... name the part with .mpn: A, B` | The part's catalog is keyed. State `.mpn &= "A"`. |
| `... rows were left out because package P has no pin` | No catalog row's package has a pin the board connects. Use another package or leave the pin unconnected. |
| `... cannot be chosen from the X catalog: F:N: ...` | A catalog file is wrong at line N of file F: a column the part does not list, a cell in the wrong unit, or a missing column. Fix the file. |
| `F:N: the row for ... has no source` (or `... is already a row at ...`, `... library ... is not Basic ...`) | The catalog file fails to load. Add the source, remove the repeated part, fix the cell. |
| `pad N of X is given to both ...`, `... have no pads` | A fault in the part's `pins` table: fix the table. |

## Lowering and fab messages

- `does not say how it is made`: add `import std::mfg::jlcpcb::TwoLayer` and `.process: TwoLayer`.
- `process.inner1 ... has no net`: name the plane's net on the board.
- `there is no KiCad symbol for a X yet`: the mapping (`std/kicad.toml`, and the project's `kicad.toml`) has no row for this kind. Add a `[[part]]` row with the symbol and footprint, and a `[[library]]` for the library that holds them if it is the project's own.
- `symbol library L not found` / `footprint L:F not found`: `build` and `fab` read KiCad's libraries; when the message says `no KiCad ... libraries found` and lists the places looked at, install KiCad 9 or set `NETSMITH_KICAD_SYMBOLS` and `NETSMITH_KICAD_FOOTPRINTS` to the folders that hold them, or list the library in `kicad.toml`.
- `--cross-check` reports `DIFFERENCES`: a layer, drill file or placement differs from KiCad's own plot, or a design rules finding is only netsmith's or only KiCad's (the lines say which, with the rule and the place). Open the `cross-check-<file>.png` written beside the package (grey both, white only netsmith, dark grey only KiCad) and report it as a bug in netsmith's writers.
- `the board outline on Edge.Cuts is not closed`: the outline of a ported board has a gap; the message gives the coordinates.
- `N connections could not be routed (...)` with `net X: A.1 to B.2` lines: the router gave up on those connections. If it says `stopped after the route timeout`, raise `--route-timeout <secs>` (default 300; `0` is no limit); otherwise the lines are ones it cannot route (a pad walled in by its neighbours where the process or the board refuses a via in the pad, a crowded board): give the parts more room, narrow the process's track and space within the factory's limits, or move the pads named.
- A design rules failure stops `fab` (`the design rules check found N violation(s), M unconnected pad(s)...`). Each line names the rule (`clearance`, `hole_to_hole`, `copper_edge_clearance`...), the numbers (the distance found and the least allowed), the place in mm and the items (`SMD pad 1 [VIN] of C1`). Fix the board or the placement; `--allow-drc-violations` packages anyway and the notes say `NOT clean`. KiCad is not needed to run the check.

- `[constraint]` findings (`constraint usb.place.left (widget.physical.nsm:14) is violated: ... it allows -0.55 to -0.45 mm (0.73 mm over)`) are the board's own requirements on where a part goes (ADR 0045) and stop `fab` like a design rules error. Each names the line, the part, the drawing it measured (`F.Fab`, `F.CrtYd` or `pads`), the measured value, the window allowed and how far over. Change the range or the circuit, not the layout file. `netsmith-use-constraints` has the loop. `--allow-drc-violations` does not waive them (the error says so); `fab --constraints warn` lists them as warnings for one run, `--constraints error` forces errors over a manifest's `constraints = "warn"`.
- `... is physical intent, and a board's circuit source does not hold it (ADR 0048); declare it in ... `physical for Board` in `x.physical.nsm``: the outline, a cutout, a mounting hole, a fiducial, a label, a keepout the board draws or a `.place` requirement was written in the board. Move the lines to the board's physical file (`case { }` for the first five; the message names the unit and the file). `... is not physical intent: a physical file states where a part is ...` and `a physical file states where things are, and a connection with `~` changes the circuit`: a statement in the physical file sets something other than a `.place`, a `.height` or what the file declares; put it in the circuit source. `physical for X names no board of the source it is paired with; its boards are ...`: the unit's name is not a board of the source beside it. `... belongs in the `case { }` section of `physical for B``: wrap that declaration in `case { ... }`.

## Warnings: guidelines

`warning: B: IPC-2221 guideline: net N carries up to I (...) but its tracks are W mm wide ...` is the track width check (ADR 0036). netsmith draws a net no rail states a width for at the width IPC-2221 needs, so the warning means a rail's `.min_track` is narrower than that, or a current on the net is unknown (a `note: the current of ... is not known ... tracks of net N are not sized` comes with it: state that current). It gives the net's worst-case current (the sum of the loads on it or returning through it, all on at once; the message lists the largest terms), the width it is drawn at, the copper and temperature rise (10 K unless the board states `.process.max_temperature_rise`), what that width carries and the width IPC-2221 needs. It is a guideline and not a fabricator's limit, and IPC-2221 is conservative: widen the rail's `.min_track` as the message says (or remove it, so the router sizes the net), or state the higher rise you accept. With `guidelines = "error"` in `netsmith.toml` or `--strict` it is an `error:` at the rail's line instead. `fab` also writes it to `fab-notes.txt`.

## Notes are not errors

`note: not evaluated: ...` means a relation waited on a value nothing set. `not chosen: ...` means a part got no pick. Decide whether either matters for your board.

`note: the balance on net X is incomplete: P is not modelled, so Q is not set from it` means a pin on the net is neither a rail, a counted chain, nor a part that draws nothing, so the current of the part Q is not worked out by subtraction from the others. State Q's current, or give the part at P a `current` in a chain or a `Rail` of its own (ADR 0027). An `input` pin (an enable) of a part that has a `Rail` is not listed; one on a part with no `Rail` is.
