---
status: accepted (owner review 2026-11-04)
---

# Derived placement

ADR 0030 decided (user decision 4) that the placer is built before the `layout` block of the language: a good placement derived from the circuit alone, with nothing new in `.nsm`. This ADR records what was built, which is stages P0 to P2 of that ADR's section (b), and what it measured. P3, the `layout` block, is language and is not built.

## What replaced what

The row layout (connectors in two columns, the rest in rows of 30 mm, nothing turned, order improved by up to 8 passes of pair swaps) is gone as a placer. Its first half is kept as the starting state of the annealer and as the test of whether the parts fit a size that was asked for (`PlaceOptions::fit`); the pair swaps are gone. There is one placer: `place::arrange_best` and `fit::fit` call `anneal::anneal`.

## The algorithm

Simulated annealing over the poses of the parts, in `crates/netsmith-pcb/src/anneal.rs`.

- **State.** For each part: its origin on a grid of 0.05 mm, its turn (0, 90, 180 or 270 degrees from the angle it began at) and which side of it the reference text is on (above, below, left, right). The face is not part of the state (it comes from `PlaceOptions::sides`).
- **Start.** The rows arrangement of the parts, taken in the order of their instance paths, inside the outline the rows made (or the size asked for).
- **Moves.** Shift one part (a step that shrinks as the temperature falls), turn it about its courtyard's centre, move its reference text to another side, swap two parts, put a part beside a part it shares a net with (on the side of that part's courtyard nearest the connected pad, sometimes turned), and shift a whole module instance. The cost of a move is the difference of the terms that involve the parts moved, so a move costs about the same on a board of 70 parts as on one of 700.
- **Schedule.** The first temperature is the median of the uphill changes a sample of 300 moves would make (at most 20 mm of wire), the last is 0.03; it falls by a factor of 0.93 after each stage of moves. The number of moves is 3000 for each free part, at least 2000 and at most 300 000. It is a count, not a clock.
- **Result.** The best state of the last part of the run that breaks no hard rule. If there is none, the parts that overlap something are moved to the nearest free place round them; if that fails too, the rows arrangement stands and `check_placement` says what is wrong, as before.
- **Auto size.** Without a size the board is the arrangement's box plus the wall round the courtyards (and 0.5 mm round the reference text), on whole millimetres, and the arrangement is moved to the corner. With a size the parts are placed inside it.

### The wall (accepted by the owner, 2026-11-04)

The placer kept a fixed 2.0 mm between every courtyard and the outline, which costs about 364 mm2 of each face on a 35 x 60 mm board; a real KiCad layout had parts 0.55 mm from the edge. Nothing recorded why it was 2.0. What it can protect, and what was found: (1) *The pad to edge rule* is `copper_to_edge`, 0.2 mm for JLCPCB: the router keeps tracks and vias that far itself, and a pad is inside its courtyard, so the wall only matters for a pad that sticks out of its courtyard, which the annealer now keeps off the edge with a hard cost of its own. (2) *A routing channel*: several boards routed completely and passed netsmith's check and KiCad's with the wall at 0.5 and 0.9 mm (the pcb crate's tests with the constant changed, then with the rule below); the router needs no channel round the edge, since tracks go inward. (3) *Panels*: a V-cut asks for copper 0.4 mm from the edge (`VCutPanel.min_edge_clearance`), which a board states by narrowing its own `min_edge_clearance`, so the wall follows it. (4) *Silkscreen*: reference text keeps its own 0.5 mm (`TEXT_EDGE`) and was never the wall's job; at a thin wall a layout may give a silk warning (silk to silk, 0.07 mm on one seed) that the soft silkscreen cost does not rule out, and `advised_edge_clearance` (the fabricator's advice for a V-cut, 1 mm) stays a note.

So the wall is now `Rules::edge_wall`: the copper to edge space, the process's clearance and its least track, one track with its space beside a part (0.4 mm for JLCPCB's, 0.75 mm for the test process, more for a process that asks more), and it can never be less than the copper to edge space. A placer given no rules keeps 2.0 mm (`Rules::DEFAULT_EDGE_WALL`); a board always has a process, so only code that calls the placer without one sees it. Reference text, the corner positions of mounting holes (`copper_to_edge` plus 0.3 mm) and the hole-to-hole rules are unchanged. The cost: a board with parts that close to the edge has less room for a track beside them, which the router handles; and a layout's silk may come nearer the edge by chance (a warning).

### Determinism

The same circuit, options and seed give the same board on any machine. Positions are whole units of 0.05 mm; the random numbers come from SplitMix64 written in the crate; `exp` is a series of additions and multiplications; distances use `sqrt` and not `hypot`; nothing iterates a hash map to decide anything. `PlaceOptions::seed` (the layout file's `seed`, a whole number from 0 to 4294967295) changes the run; two seeds give two different boards.

## The cost

Distances are in mm, weights are constants of `anneal.rs`.

| Term | What | Weight |
|---|---|---|
| Wire | Half perimeter of the box round the pad centres of each net, so a pad's place in the footprint and the turn of the part count. A net of many pads counts less per mm (down to 0.35), a plane net 0.15 of that. | 1 |
| Overlap | Courtyards that overlap (hard), and nearer than 1.0 mm (soft; 1.8 mm on a board with a pour on an outer layer, which needs room round a pad for its spokes). A part with a hole through the board keeps the process's clearance and hole to hole space instead. A part with many pads asks 0.03 mm more for each pad, to 2.5 mm, so that its tracks can leave. A decoupling capacitor and the part it decouples are exempt from the extra. Parts on opposite faces share area unless one has a hole, and then only where it takes room on the other face: its courtyard, or its pad boxes if the footprint draws its courtyard on one face's layer only (ADR 0031). | hard 200, soft 4 |
| Walls | A courtyard nearer the outline than the wall (`Rules::edge_wall`, see below), a pad nearer than the process's `min_edge_clearance` where the wall does not already keep it so (a pad that sticks out of its courtyard), reference text nearer than 0.5 mm, a courtyard in a keepout or cutout, and a pad nearer the edge of a cutout than the process's `min_edge_clearance`, the distance the placement check asks (hard); the outline's corners are rounded if the board's are. | 200 |
| Cohesion | Half perimeter of the box round the centres of the parts of each module instance (each prefix of the instance path with two or more parts), and each part of another instance whose centre is in that box. | 0.3, 0.05 |
| Crossing | Pairs of ratsnest lines (the shortest tree over each net's pads, for nets of at most 10 pads that are not planes) of two nets that cross. The lines of the nets a move touches are found again for each try and counted against the lines kept for the others. | 1 |
| Decoupling | Distance from the capacitor's two pads to the nearest power pin pads on the same nets (below). | 6 |
| Edge | A connector's courtyard to the nearest side of the outline, a mounting hole or fiducial to the nearest corner. | 0.4 |
| Area | Half perimeter of the box round all courtyards. | 0.03 |
| Turn | A part that is turned, or has its text off the top, so that a tie goes to the upright one. | 0.3 |
| Silkscreen | Reference text, and the strokes of a footprint's silkscreen, within the process's silk to pad space of another part's pads and mask openings, or within 0.2 mm of its silkscreen or text; and, a little, text over another part's courtyard. | ramp, 3 |

**Silkscreen** is the issue the board-features report raised: a library footprint's silkscreen is clipped by the mask, and KiCad's check refuses text on a pad. It is a cost that grows during the run, from 2 per mm to 40 and then to a ban for the last 30 % of the moves (a ban from the start makes the board 30 % longer in wire, because every part must have a free place for its text before it has a place). The text box is the library's text size (or the process's, if more) by about one text height for each letter, grown by 0.15 mm, and sits where the part's `ReferenceSide` says. The text is upright whatever the part's turn, so `Placed::reference_at` finds the place above (or beside) the part as it lies on the board and turns it back to the footprint's frame; the board file was wrong for a part that is turned before this. The side is in the layout file (`text = "left"`), and `apply_layout` restores it.

**Decoupling pairs** are derived (`quality::decoupling_pairs`), not written: a two-pad capacitor (kind `Capacitor...`) whose pads are on two different nets, such that one same other part has a `power` pin on each. The parts that share most of the capacitor's instance path are kept, so a module's capacitor goes with the module's part. The cost is the nearest power pin pad on each net, so a part with several supply pins pulls the capacitors to different ones. No number and no annotation is read; `Placed::power_pads` is what the pin tables of ADR 0016 give.

## What a layout file does

`place_options(design, layout)` gives the placer the face of every part the file has an entry for, its seed, and `PlaceOptions::fixed` for the parts whose entry has `lock = true`. A locked part is where the file says (its text side too); the others are placed round it and the outline holds all of them. A part that has an entry and is not locked is derived as if it had none and then put where the file says (`apply_layout`), so moving one part by hand does not move the rest and the findings say what it now overlaps (the editor's way, ADR 0030 E1). A regeneration is `place_subset(board, parts, region, seed)` places the parts named inside `region` with everything else fixed (the pads of the fixed parts still pull on the nets), changes nothing and says by how much the region is short if the parts do not fit, and gives the same arrangement for the same seed. The caller chooses the parts (a module's, from `Placed::path`), the region (the module's box grown by a margin) and whether locked ones are left out; that is the rest of ADR 0030 (c).

## Measured

Boards: the synthetic big board of `crates/netsmith-pcb/tests/synthetic.rs` (70 parts, an 80-pad four-sided package, 106 nets, four layers), the same with the structure a design in modules has (78 parts: instance paths of eight parts, 8 decoupling capacitors between 3V3 and GND, `power` pins on the package; `synthetic_board_structured`); a real board with a module was also measured, but its data is not in this repository, so no figure from it is kept. Before is the row layout; after is this change, seed 0. Release build on the same machine; routing is `route_with` with its default options. KiCad's check is KiCad 9's, planes filled, and counts errors.

| | Synthetic big board, before / after | Synthetic with modules and power pins, before / after |
|---|---|---|
| Parts | 70 | 78 |
| Board | 42 x 53 / 42 x 42 mm | 43 x 56 / 42 x 48 mm |
| Wire length (half perimeters of nets) | 1805 / 1441 mm | 1770 / 1381 mm |
| Ratsnest length | 1367 / 1155 mm | 1298 / 1119 mm |
| Ratsnest crossings | 456 / 178 | 364 / 182 |
| Decoupling capacitors, mean / max distance | none paired | 31.8 / 42.9 mm to 5.0 / 10.3 mm |
| Connections left after routing | 0 / 0 | 0 / 0 |
| Route time, release | 3.4 s / 1.1 s | 2.3 s / 2.0 s |
| Placement time, release | 0.4 s / 1.7 s | 0.4 s / 3.1 s |
| KiCad's check, errors | 0 / 0 | not run / 0 |

Over more seeds (seed 0 is not the best): the big board over six seeds averages 1291 mm and 150 crossings; the one with modules over four seeds 1442 mm, 181 crossings and 3.9 mm of decoupling. Debug build (`netsmith-pcb` is built with `opt-level = 2`): the big board is placed in 2.9 s and routed in 2.1 s, the one with modules in 4.9 s and 5.5 s (the row layout, built without that setting: 3.7 s and 19.3 s, 3.5 s and 19.2 s).

What made the difference, found by measuring (on the synthetic boards, four seeds, before the crossing term): with no rule about reference text the big board's wire was 995 mm; with the text banned over other parts from the first move 1412 mm (+42 %), with the weight ramped through the run 1303 mm (+31 %); the text sides (four, not one) recovered little of that, the ramp most. With no term for the lines that cross, the wire was as short but the boards were hard to route (the big board's seed 0 took 54 s and left two lines in a release build); counting crossings halves them, and every board of every seed tried has routed completely since. A pull of the parts of a module to their centre, squared, made the wire longer than the box round them does.

## Not done, and known limits

- **P3, the `layout` block**, and with it any datasheet number: the decoupling pull is a distance with no number from a source.
- **Routability is a proxy.** The placer keeps a gap between parts, room round parts with many pads and few ratsnest lines crossing, but it does not estimate congestion, and a board that is given a size smaller than the arrangement wants is annealed in it, whatever the rows need (ADR 0045, section 10). The structured synthetic board routes in 2 s where the rows took 2.3 s on a board a fifth larger; a board packed harder than the router can untangle is not helped.
- **Two latent faults the new placements showed** are fixed (stitching vias in the gap between two pads, reference text of a turned part) but the router and the pour code have more of them: KiCad's `starved_thermal` (a pad in a pour with one spoke) appeared on some seeds of a real board whose data is not in this repository.
- **Escape rings** round fine-pitch parts and exposed pads are the per-pad extra gap above, not a derived keepout for fan-out vias.
- **Faces.** The placer shares the faces only when a size to fit needs it, and never flips a part for wire length. *Amended (ADR 0045, section 10, provisional):* it used to decide that by the rows test of ADR 0031 before the annealer ran. Now, if the rows are too big for the size, the annealer is tried first on the faces the parts are on, and the faces are shared only if that run fails (the rows are the annealer's start, not its judge; a part that hangs over the edge by the design's statement counts as its courtyard clipped to the outline).
- **Follow-up (owner review, 2026-11-04): a public reference board.** The Measured table uses only boards in this repository (the two synthetic boards of `crates/netsmith-pcb/tests/synthetic.rs`), so anyone can re-measure it. The figures of the private board were removed (PR #37) and the text above keeps none; the two remarks that still mention a board whose data is not in this repository (the `starved_thermal` fault and the four boards the weights were tuned on) give no number. What is missing is a public board of a real design, with a module, whose wire, crossings and check results are measured and kept here, so the weights are tuned on something a reader can run.
- **Hierarchical placement** (arrange each module, then place the blocks) is not built; cohesion is the cost above.
- **The weights** were tuned on four boards (one not in this repository) and a handful of seeds, and the variation between seeds on one board is large. The numbers are for seed 0.
- A footprint's own silkscreen text and strokes that clip each other or the mask (a library module's footprint) are the library's and stay.
