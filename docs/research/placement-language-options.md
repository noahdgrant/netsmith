# Placement language: four open questions from tight boards

Options note for issue #99 and three questions beside it. Written while the owner was away. It decides nothing and builds nothing, and it changes no code. Language-design choices are the owner's (`AGENTS.md`), so every spelling below is a placeholder invented to compare options. Snippets marked `nsm-new` use constructs that do not exist. Snippets marked `nsm-fragment` use only constructs that exist today, as pieces of a file. Messages marked "invented" show what a message could say; the wording is the implementation's to settle.

Nothing was run, built or parsed for this note. The repo is public, so every board, part and number is invented, including the sizes. Where a sentence says "I read" a rule, it is from the code or an ADR and was not executed.

Sources: ADR 0031 and 0033; ADR 0045 sections 3, 8, 10, 11, 12 and 13; ADR 0048; `docs/LANGUAGE.md` (the physical file, keepouts, height limits, overhang); `docs/research/physical-file-syntax-options.md`; `examples/enclosure_sensor/`; issue #99; and, where a section says so, `crates/netsmith-pcb` (`board.rs`, `anneal.rs`, `place.rs`, `explain.rs`, `export.rs`) and `crates/netsmith-fab`.

## Where things stand

| # | Question | Today | My lean (a lean, not a decision) |
|---|---|---|---|
| 1 | Through-hole tails. A flat part on the back over the trimmed pin tails of a socket on the front | The pads on the back are a hard obstacle. Nothing can state a trim | A built-in `.tail_height` that a part may state and a board narrows per instance (option B) |
| 2 | A courtyard past an edge while the body stays inside | Refused. Only a statement about the body's edge allows it, and that statement allows the body past too | Forgive a courtyard's margin at the outline, bounded by a number, with a library warning above it (option B) |
| 3 | A pinned part inside a keepout that forbids parts (issue #99) | A named conflict, the same for every seed | A keepout names the parts it does not forbid (option B); improve the message now |
| 4 | Name a known conflict before placing | Proved only after a failed placement | A pre-check with no new syntax, shown in `fab` and as editor squiggles |

Question 1 is first because it is design practice the language has to be able to state. The others are refusals with workarounds, or no language change at all.

## 1. Through-hole tails

### 1.1 What a person needs to say

On a tight board a flat part sits on one face over the pins of a through-hole part that comes through from the other face. The pins are cut short after soldering, so the stubs that stand out of the board are lower than the flat part's underside. The flat part would not fit anywhere else. Nothing in the language can say "these pins are cut to 0.2 mm" today, so the placer treats the pins' pads as an obstacle and the flat part goes elsewhere or the board does not place.

The case below is invented, and it uses the radio carrier of `examples/enclosure_sensor/`. On the front are `.radio.row_a` and `.radio.row_b`, two `Socket1x10` rows (ten pads each, 2.54 mm pitch, 22.86 mm apart). On the back is `.cell`, an invented flat part, about 30 x 26 mm and 3.4 mm tall, that lies across both rows. The case leaves 3.6 mm under the board.

### 1.2 How it is decided today

Two rules judge the same stretch of the back face, and neither knows about the other.

**The two-face rule (ADR 0031).** A socket has drilled pads, so it takes room on both faces (`Placed::has_hole`, `Placed::room_on` in `crates/netsmith-pcb/src/board.rs`). On the face it is not on, it counts as its whole courtyard if its footprint draws the courtyard on both layers or on neither. If the footprint draws it on its own face's layer only, the other face is judged by the socket's pad boxes, each grown by the process's clearance. Either way, `.cell` standing on the back with its courtyard over a pad box is an overlap. The annealer prices it as a hard cost (`Engine::rooms_on` and `room_pairs` in `anneal.rs`), `Board::check_placement` reports it, and `could_fit` in `place.rs` counts the pad area on that face when it decides whether the parts can fit at all. No statement a person can write changes any of that.

**The height zones (ADR 0045 section 11).** A keepout over the board with `.layers &= "bottom"` and `.max_height &= 3.6mm` limits the parts on the back to 3.6 mm, and `.cell.height &= 3.4mm` passes it. The section says that "a through-hole part is measured on the face it is placed on, by its `.height`; the part of it below the board is not limited". So nothing measures the tails. The zone knows the cell's top and not that pins are under it. The two-face rule knows pins are under the cell and not how tall they are.

### 1.3 The numbers, and what I assumed

The clearance is 3.6 mm and the part is 3.4 mm tall, so 0.2 mm is left. I read that as a stack. The tails stand `t` out of the back face, the flat part rests on them, its body adds its own height, and the sum has to fit the clearance. Here `t + 3.4 <= 3.6`, so `t <= 0.2` mm, which is a pin cut flush, leaving the solder fillet and a stub.

There is a second reading. The flat part might stand on the board with its own gap `g` under the body, and then the rule is `t <= g` with the part's height measured from the board, as `.height` is today. That needs one more fact on the covering part. The options below are written for the stack reading, and each says what changes under the gap reading. Which one is true of the owner's parts is question 1b at the end.

### 1.4 The rule, and what the options change

The rule is the same for all three options, in one sentence:

> A part on the other face may overlap the pads of a through-hole part when those pins are stated as trimmed to a length `t`, the covering part's height is known, and `t` plus that height fits the limit a height zone puts on that face there. The covering part's own pads still keep their copper clearance from those pads.

The options differ in one thing, where `t` is stated and how finely. A part with no stated tail is unknown, not zero, as a part with no `.height` is in section 11. Unknown keeps today's rule, so adding the member changes no existing board.

**What relaxes, and what does not.** The overlap test between the covering part's body and the socket's pad boxes relaxes. Copper clearance between the two parts' pads, hole to hole and hole to copper do not. A trimmed pin is still a drilled, plated hole with a pad.

### 1.5 Option A: on the part, or its footprint

**(a) What the person writes, and where.** The part kind states it for every instance, in its definition. `.tail_height` would be a built-in member like `.height`. A footprint is not a good home: the mapping `kicad.toml` holds KiCad's names and nothing else (GLOSSARY, _Mapping_), and a KiCad footprint draws no pin length, so a number there is a netsmith fact in a file for KiCad names.

```nsm-new
/// A 1x10 socket whose pins end 0.2 mm out of the board once soldered. The maker's
/// short-tail part; every unit has them.
part Socket1x10Short {
    .designator &= "J"
    .tail_height &= 0.2mm
    pins { ... }
    .package: Package
}
```

In `examples/enclosure_sensor/` the two `Socket1x10` rows of `RadioSockets` would become `Socket1x10Short`, and the physical file gains the zone and the cell's lines of section 1.6 without the two `tail_height` lines.

**(b) What the checker and placer compute.** The shared rule, with `t` taken from the part kind. The room test for the cell on the back stops treating the pad boxes of a socket with a stated `t` as an obstacle when `t` plus the cell's height fits the zone, and keeps them for the cell's own pads.

**(c) The board house, and what the check still flags.** There is no trim step, because the pins are short when bought. `fab-notes.txt` already lists `hand-solder: J2, J3`; the line would carry the reason, for example `hand-solder: J2, J3 (short-tail socket, pins 0.2 mm: buy that part, not a 1x10 socket)`. A person who orders a long-pin socket instead is the failure. The design rules check cannot see which socket was bought. A part with no stated tail under a covering part is still the overlap finding of today.

**(d) Failure modes.** Covering part too tall is F1 and unknown height is F2 (section 1.8). The real failure is a long-pin socket on the bench, caught only by the note. A person cannot state a trim by this option at all, which is the main weakness. A trim is a decision of the board, and "libraries state only what is always true" (`docs/DESIGN-RULES.md`) rules out a library part that claims its pins are trimmed. The cost is a part definition for each tail length, and sourced numbers for any such part in `std/`, since a number from memory is not allowed there.

### 1.6 Option B: per instance, in the physical file

**(a) What the person writes, and where.** The part kind declares the member as a range that tops out at the untrimmed length, or leaves it unknown. The board's physical file narrows it for an instance, as it narrows `.height` (ADR 0048 decision 9) and as a board narrows `.assembly`. The statement is a requirement on the board's parts, so it belongs in the physical file.

```nsm-new
// enclosure_sensor.physical.nsm, after section 8

    // 9. The back of the case leaves 3.6 mm under the board (drawing slot D, invented).
    .back_room: Keepout {
        .x &= 0mm
        .y &= 0mm
        .width &= 90mm
        .height &= 62mm
        .layers &= "bottom"
        .max_height &= 3.6mm
    }

    // 10. The flat cell lies across both socket rows on the back. It is 3.4 mm tall.
    .cell.place.side &= "bottom"
    .cell.height &= 3.4mm
    .cell.place.x - .radio.row_a.place.x == 11.43mm +/- 0.5mm
    .cell.place.y - .radio.row_a.place.y == 0mm +/- 1mm
    // The sockets' pins are cut to 0.2 mm above the back face after soldering, before the
    // cell is fitted.
    .radio.row_a.tail_height &= 0.2mm
    .radio.row_b.tail_height &= 0.2mm
```

The circuit source gains the cell and says who fits it: `.cell: FlatCell { .assembly &= "hand" }`, with an invented `FlatCell` part. The assembly stays in the circuit source because a circuit check consumes it (ADR 0048 decision 6). Two sockets need two lines. There is no one-line form for a module, because the module's parts are separate parts.

**(b) What the checker and placer compute.** The shared rule, with `t` from the board's line. The statement replaces the part's untrimmed top. A value above the part's own top is the usual empty-window error of narrowing, naming both lines (F7). The other-face room test uses `t` where it used the full pad box: the cell may overlap the socket's pad boxes when `t + 3.4 mm <= 3.6 mm`, else the boxes are an obstacle as today. Under the gap reading the test is `t <= g` and `3.4 <= 3.6`.

**(c) The board house, and what the check still flags.** The statement is the manufacturing instruction, so the note can be made from it. It is a hand step, as nothing in the repo says the board house trims leads (I did not look up whether JLCPCB offers it, and the claim is not made). The `fab-notes.txt` line, next to `hand-solder:`:

```text
trim: J2 (radio.row_a), J3 (radio.row_b): after soldering, cut the pins flush to 0.20 mm above the back face, before C5 is fitted; C5 lies over them (enclosure_sensor.physical.nsm:41)
```

It would also appear in `order.txt` under "before you order", where the skill `netsmith-prepare-fab-order` already tells the agent to read each board's lines to the user. Gerbers cannot carry it, as they cannot carry a filled via (ADR 0031). The design rules check still flags a covering part over pads that are not stated as trimmed (the overlap of today, with a new hint line, F3), and a stated tail that is too long for the zone (F1).

**(d) Failure modes.** F1, F2, F3, F5 and F7 apply, and F4 (the pins are not trimmed in reality) is caught only by the human notes. F5 is the one this option adds: a trim after soldering means the covering part cannot already be on the board, so a covering part that the board house assembles is an error (section 1.7).

### 1.7 Option C: per group of pads

**(a) What the person writes, and where.** A statement in the physical file that names the pads of a part, for the case where only some pads are under the flat part. A header that also serves as a plug on the back has pins that stay long to mate, and only the pins under the cell are cut. The statement names pads, not pins: a pin is the circuit's, and ADR 0048 refuses a physical-file statement that touches what a pin is. The range of names follows the `row` statement's `..` (ADR 0048 decision 10).

```nsm-new
    .radio.row_a.pads 1 .. 10 tail_height &= 0.2mm
    .radio.row_b.pads 1 .. 6  tail_height &= 0.2mm
```

**(b) What the checker and placer compute.** The shared rule, per pad. A pad with a stated `t` that passes stops being an obstacle for the cell, and a pad without one stays an obstacle. The placer already builds one box per pad (`pad_boxes` in `anneal.rs`, the per-pad branch of `Placed::room_on`), so the room test is natural. The cost is in the checker: the physical file would reach into a part's pads, and the lowering carries a table per part.

**(c) The board house, and what the check still flags.** As B, with the pad range in the line (`trim: J3 (radio.row_b) pads 1 to 6: ...`). The check flags a long pad left under a part, named by pad, which B can only name by part.

**(d) Failure modes.** As B. A partial statement fails safe: an unstated pad remains an obstacle, so a forgotten pad refuses the placement and does not allow a collision. A new failure is a range that covers a pad the person did not trim; it looks like F4.

### 1.8 How a stated trim fails, and what catches it

The messages are invented. The wording is the implementation's.

| | Failure | What catches it |
|---|---|---|
| F1 | Tails are stated, but the covering part is too tall for what they leave | An error at `fab`: ``error: C5 (`cell`) lies over the pins of J2 (`radio.row_a`), which are trimmed to 0.40 mm, and stands 3.40 mm tall: 3.80 mm from the back face, and the keepout `back_room` (enclosure_sensor.physical.nsm:28) allows 3.60 mm there (0.20 mm too much); trim the pins to 0.20 mm or less (line 41) or use a lower part`` |
| F2 | The covering part has no height | An error, not the warning of section 11, because a missing fact would loosen a hard rule: ``error: C5 (`cell`) has no height, so it cannot lie over the pins of J2 that are trimmed to 0.20 mm; state `.cell.height` `` |
| F3 | A part overlaps pads that are not stated as trimmed | Today's overlap error, with a hint: ``... if the pins of J2 are cut short after soldering, state `.radio.row_a.tail_height` `` |
| F4 | The pins are not trimmed in reality, or not to the stated length | Software cannot see it. The `fab` run prints a warning for every stated trim as long as the statement exists, `fab-notes.txt` and `order.txt` carry the `trim:` line, and the skill `netsmith-prepare-fab-order` lists it as a step to verify and to measure one tail |
| F5 | A trim after soldering, and the covering part is assembled by the board house | An error: ``error: C5 lies over pins of J2 that are cut after soldering, and C5 is assembled by the board house, which fits it first; make C5 `hand`, or use a socket whose pins are short (option A)`` |
| F6 | No height zone covers those pads on the back, so there is no limit to compare against | The exemption is granted (nothing to violate), the `trim:` line is printed, and a note says that no height limit was checked there |
| F7 | A stated tail longer than the part's own untrimmed length | The empty-window error of narrowing, naming both lines |

The conflict explanation of ADR 0045 section 13 gets one more proved kind: a part pinned over trimmed pads whose stack exceeds the zone wherever its window puts it. That is a new test inside `explain.rs`, with no new mechanism.

### 1.9 The order of work

A trim after soldering has an order. The pins are soldered, then cut, then the covering part is fitted. If the board house assembles the covering part, it is on the board before any through-hole part is soldered, so the pins cannot be cut under it afterwards. Either the covering part is fitted by hand last (F5 makes the checker say so), or the pins are short by design (option A).

This is a question for the owner that changes the lean: is the covering part fitted by hand after the trim, or assembled by the board house? If the board house assembles it, option A is the only one that works, and B and C are pointless. I assumed hand.

### 1.10 Cross-check against KiCad

The same relaxation has to appear in every place that judges the overlap, in one rule, as the overhang and the height zones do: `Placed::room_on`, `Engine::rooms_on`, `Board::check_placement`, `could_fit`, and the design rules check's `courtyards` rule in `crates/netsmith-geom/src/drc/checks.rs`. `could_fit` is the quiet one. If it still counts the trimmed pads' area on the back, its "certain no" test (the courtyards' area alone is more than the faces hold) can refuse a board that fits.

ADR 0031's follow-up found that KiCad's `courtyards_overlap` agrees with netsmith layer by layer. For a socket footprint that draws a courtyard on the back layer, KiCad's own check would flag the cell over it even though netsmith allows it. For a footprint that draws it on one layer only (the case the amendment covers), KiCad's default check flags nothing, and `pth_inside_courtyard` is off by default. I did not run KiCad. If the cross-check disagrees, the options are to list the finding as expected in `fab-notes.txt`, or to write the cell's courtyard into the board file without the part over the pins. That is an implementation call.

### 1.11 Baseline: do nothing

The person moves the part. On a tight board there may be no place for it, and that is the case the question is about. Nothing here costs anything to build, and the cost falls on the person every time.

### 1.12 The options side by side

| | A: on the part | B: per instance | C: per pad group |
|---|---|---|---|
| Written in | the part definition | the physical file | the physical file |
| Says | what the part is | what this board does with this part | what this board does with some of its pads |
| Unstated means | unknown, today's rule | the part's untrimmed top, today's rule | the same |
| New language | a built-in member | the same member | the member plus a statement naming pads |
| Can state a trim after soldering | no | yes | yes |
| `fab-notes.txt` | a buy note | a trim step | a trim step with pads |
| The covering part must be hand-fitted | no | yes (F5) | yes (F5) |
| Placer | per part kind | per instance | per pad |
| Cost | a part per tail length, sourced numbers | two lines for two sockets | a pad-reach in the checker |

A and B are one member with two places it may be narrowed from (the part, then the board), which is the pattern `.height` already has. ADR 0048 marks that two-place pattern as "two spellings until the owner says which stays". So the lean is B, with A allowed where a part really is short by design. C is B plus a finer grain and should wait for a board that needs it.

### 1.13 Migration, ADR 0001

No example states a tail, so no existing file changes meaning. Unstated is unknown and unknown keeps today's rule. What moves:

- `GLOSSARY.md` gains an entry, for example _Tail_ (the end of a through-hole pin that stands out of the board on the face opposite its part; _Avoid_: lead, pin length, stub), and _Height limit_ and _Side_ get a pointer.
- `docs/LANGUAGE.md` gets a subsection after "Height limits", and, for B and C, a line in "The physical file". The guide's tested blocks (`crates/netsmith/tests/guide.rs`) need an `nsm-physical` block pair for it.
- `examples/enclosure_sensor/` is the natural place for a worked case. It is optional, and it adds a part to the circuit.
- Skills: `netsmith-prepare-fab-order` (a `trim:` row in the notes table next to `hand-solder:` and `vias in pads:`), `netsmith-use-constraints`, `netsmith-read-errors` (F1 to F7), and `netsmith-write-nsm`. A rule belongs in `netsmith-use-constraints` for agents: **never state a trim to make a finding go away**. A trim is a physical act the person has to do, and an agent that states one without asking has invented a manufacturing step.

### 1.14 Lean

Build option B first (a lean, not a decision). It puts the trim where the decision is, on the board, as a requirement in the physical file. It reuses the narrowing that `.height` and `.assembly` already have, adds no construct, and leaves every existing board unchanged. The statement is also the manufacturing instruction, so the `trim:` note and the check for the order of work (F5) come from one line. A can come with it for free, because the member is the same, and C waits for a board that needs a partial row.

The lean flips to A if the covering part is assembled by the board house (section 1.9). It also depends on the stack or gap reading (section 1.3), which changes one comparison and not the shape of the options.

## 2. A courtyard past an edge, with the body inside

### 2.1 Today

Overhang statements are about the body's edges (ADR 0045 section 10). A footprint whose courtyard is wider than its package, by an assembly margin, on a board as wide as the package is refused: the courtyard is past the edge, and `OutsideOutline` names it. The invented case is a 20.0 mm wide jack block with a courtyard 0.5 mm wider on each side, on a 20.0 mm wide board.

The refusal suggests `.j.place.left <= 0mm`. I read section 10 and think that line allows the courtyard, and the body, which may then pass the edge by any distance (the owner decided there is no limit). Exactly flush, `.j.place.left &= 0mm` and `.j.place.right &= .outline.width`, should state the body inside and allow the courtyard, and `fab` would then print an "overhang" warning for what is a margin. I did not run this. If it works, the gap is that a person cannot say "the courtyard may pass, the body may not" for a body that is not exactly flush, and the warning names the wrong thing. If it does not work, the gap is as the brief states it: there is no statement.

### 2.2 Option A: a statement that allows the courtyard past an edge

The courtyard gets edge fields on `.place`, with the same window semantics as the body's: `.court_left`, `.court_right`, `.court_top` and `.court_bottom`. A bound says how far past the edge the courtyard may go.

```nsm-new
    // The jack block's body is flush with the left edge; its 0.5 mm courtyard margin may pass.
    .jack.place.left >= 0mm
    .jack.place.court_left >= -0.5mm
```

**Checker.** The courtyard passes an edge only up to the stated depth, and the body keeps the old rule, so a body past an edge nobody stated is still an error. Section 10 rule 3 gains one case. **Placer.** The part counts as its courtyard clipped to the stated depth, the way a stated overhang clips it today (`overhang::needs`, `Engine::room`). **Editor.** The findings list shows the line, and "pin here" can write the field. **Agents.** The refusal's hint points at the right statement (`court_left`), where today it points at the body. **Warnings.** Whether `fab` warns on a courtyard-only pass is a sub-question; the owner's decision of 2026-11-03 makes a stated overhang a warning, and a margin past an edge is not a protrusion of the part. **Cost.** Four new fields, a second statement beside the body's, and the question of whether a person who writes `court_left` has read anything in the case drawing, since the margin is the library's number.

**Migration.** No existing file states a courtyard edge. `docs/LANGUAGE.md` ("A part that hangs over the board edge") gains a paragraph. The GLOSSARY entries _Overhang_ and _Body_ gain a sentence.

### 2.3 Option B: a library-side rule, with a bound and a warning

The footprint's courtyard may not exceed its fabrication outline by more than a margin `M`. At the outline's edge the test forgives a courtyard that meets this, as long as the body is inside. A footprint that exceeds `M` gets a warning when it is loaded, for example ``footprint J1 draws a courtyard 1.2 mm beyond its fabrication outline; the edge test forgives at most 0.5 mm; trim the courtyard, or the part cannot stand at an edge``. For a footprint that breaks the rule, the courtyard counts whole at the edge and the refusal stays, with the hint to trim.

**Checker.** The edge test measures the body plus the allowance, not the courtyard. Overlap with other courtyards, keepouts and cutouts still uses the whole courtyard. Pads are checked as ever. **Placer.** The fit and the wall use the same box. The owner already accepted the idea for height zones: the placer keeps a part out of a zone by its body plus a small margin (section 11, 2026-11-04). **Editor and agents.** No statement to learn. An agent sees a warning on the footprint, which is the library's, and a person with a project library can trim it (ADR 0041). **Where `M` lives.** It is a number somewhere: a constant like `HEIGHT_ZONE_MARGIN` (0.25 mm), or a field of the process, as `Rules::edge_wall` follows the process. The owner chooses.

**What it reverses.** The owner decided on 2026-11-03 that a courtyard past an edge nobody stated is an error. B keeps that for the body and the pads, and lets the courtyard's margin go, bounded by `M`. The case that decision protects, a part hanging off the board by accident, is still caught by the body test.

**Migration.** `docs/LANGUAGE.md`, ADR 0045 section 10 rule 3 and the _Overhang_ entry change. A guide example or a test that refuses a courtyard-only excess would change; I did not check which (`crates/netsmith/tests/overhang.rs` and the guide's refused blocks are the places to look).

### 2.4 Option C: a per-part courtyard override in the physical file

The board states the part's courtyard for this design, as a margin around the body.

```nsm-new
    // This board is assembled by hand; the jack needs no pick-and-place margin.
    .jack.courtyard_margin &= 0.1mm
```

**Checker and placer.** Every test that uses the courtyard sees the smaller one: the edge, neighbours, keepouts. That is the option's strength and its risk, since a person who wanted only the edge relaxed also moves the neighbours closer. **Design rules check and KiCad.** The board file would have to write the overridden courtyard into the footprint, or KiCad's own `courtyards_overlap` keeps the old one and disagrees. **Editor and agents.** One statement, and an agent can lower a margin to make a finding go, which weakens the clearance between neighbours. **Design rules.** It is the library's fact, narrowed by the board, which fits "a design narrows the rest". **Migration.** None for existing files; the guide and _Body_/_Overhang_ glossary entries change.

### 2.5 Baseline: do nothing

The person copies the footprint into the project library and trims the courtyard (ADR 0041, self-contained libraries). It works today, costs a footprint per board and hides the fact that the library's courtyard was wrong for this use.

### 2.6 Lean

Option B (a lean, not a decision), because the margin is the library's number and no board statement adds information the footprint lacks. A person who writes A's `court_left` is copying a number. The honest cost is the number `M` and the partial reversal of the 2026-11-03 decision. A is the second choice if the owner wants every pass past an edge stated. C changes too much at once.

## 3. A pinned part inside a keepout that forbids parts (issue #99)

### 3.1 Today

A part that the physical file pins at a point can stand inside a keepout that forbids parts. It is a finding (``H1 overlaps the keepout `peg_bases`, which forbids parts``), and the explanation of ADR 0045 section 13 names the lines and the keepout. `crates/netsmith/tests/conflicts.rs` (`a_part_the_lines_pin_inside_a_keepout_is_named_with_the_keepout`, and the second test the issue counts) and `crates/netsmith-pcb/tests/tight_limits.rs` (the B6 test, seeds 0 to 7) hold this reading. The issue could not reproduce, on an invented board, a run where one seed accepted what another refused. The rule is the owner's, and the answer should not depend on the annealer.

Invented case, in the syntax of `examples/enclosure_sensor/`: the case has a post under each mounting hole, so the physical file draws a keepout around hole 1 for the post's base.

```nsm-fragment
    .hole1.place.x &= 4.5mm
    .hole1.place.y &= 4.5mm
    .post_a: Keepout {
        .x &= 0mm
        .y &= 0mm
        .width &= 9mm
        .height &= 9mm
    }
```

This refuses today: `hole1` is pinned inside `post_a`, which states none of its four fields, so it forbids all four, parts included.

### 3.2 Option A: a requirement that pins a part overrides a keepout for parts

A part whose requirements leave one place is not refused for standing in a keepout that forbids parts. Other parts still keep out. A cutout, a keepout that forbids tracks, and a height limit stay findings. It is the issue's option 2, about 20 lines (a `pinned` set from `constraints::plan`, a filter in `anneal::Engine::single` and `outside_at`, and a skip in `Board::check_placement`).

**What the person writes.** Nothing new. The example above passes with no edit. **Checker and placer.** One rule by kind, independent of line order. A mitigation: `fab` prints a warning for each part that stands in a keepout by its pin (``warning: H1 is pinned inside the keepout `post_a` (line 12); the pin wins``), as a stated overhang warns. **Editor.** The keepout is drawn as it is; a part in it is shown with the warning. **Agents.** The riskiest: an agent can silence any keepout finding by pinning the part, and the keepout stops protecting what it was written to protect. **Export.** The KiCad rule area forbids footprints without exceptions (`crates/netsmith-pcb/src/export.rs`, "a keepout that forbids parts forbids their pads as well"), and netsmith's own `items_not_allowed` rule reads the board file. Both would flag the hole unless the rule area is dropped or the check taught the exception. This cost is the same for options A, B and C and is the largest hidden one.

**Migration.** The two tests above flip (ADR 0045 section 13 and the "pinned inside a keepout" paragraph of section 10 change), `docs/LANGUAGE.md` "Keepouts" gains a rule, and the issue's own text says the rule was written and taken back.

### 3.3 Option B: a keepout names the parts it does not forbid

```nsm-new
    .post_a: Keepout {
        .x &= 0mm
        .y &= 0mm
        .width &= 9mm
        .height &= 9mm
        .parts &= "forbid"
        .except .hole1          // placeholder spelling: parts this keepout does not forbid
    }
```

The exception is by path, in the style of the physical file's other reaches, so it can name a part inside a module. A by-kind form (``.except_kind &= "MountingHole"``) is the smaller variant and covers the case in the issue.

**Checker and placer.** The named parts are not measured against this keepout, wherever they are, pinned or placed. Any other part pinned inside it is still the finding. Order does not matter. **Editor.** The keepout can show its exceptions hatched. **Agents.** A refusal's hint can say "name the part in the keepout, or shrink the keepout". The exemption is written on the thing that protects, so an agent adding a pin cannot silence the keepout without editing it. **Cost.** New syntax, a path that breaks on a rename (the checker names the missing path, as the reach in ADR 0048 does), and the export problem of section 3.2. **Migration.** None for existing files. `docs/LANGUAGE.md` "Keepouts", GLOSSARY _Keepout_, `std/pcb.nsm` (the `Keepout` interface) and the lowering change.

### 3.4 Option C: a keepout that forbids parts except mounting holes

`.parts` gets a third value besides `"forbid"` and `"allow"`, for example `"forbid_except_holes"`.

```nsm-new
    .post_a: Keepout {
        ...
        .parts &= "forbid_except_holes"
    }
```

It is B's by-kind form with no list. It is the least code and covers the case in the issue, and it is a special case that the language then carries: the next exception is another value. ADR 0045 K7 (the screw-head clearance round a mounting hole) points at a cleaner version, where the hole carries its own clearance as a keepout it does not forbid itself (ADR 0029: a part carries its keepout). That would remove the board-level keepout for this case, but K7 is not built.

### 3.5 Baseline: the current rule, with a better message

Keep the finding for every seed and improve the explanation, and make the answer the same for every seed by refusing before placing (the issue's option 3, which is question 4's pre-check). The message can say what to do: ``the keepout `post_a` (x 0 to 9, y 0 to 9 mm) forbids parts, and `hole1` is pinned at (4.5, 4.5) inside it (lines 7 and 8); shrink the keepout so its edge is past the hole's courtyard, move the pin, or make the keepout allow parts``, with the largest shrink that frees the part worked out. This costs no language decision and can be built whichever option is chosen.

### 3.6 Lean

Option B by kind or path (a lean, not a decision). It states the keepout's intent where it is written (no other parts here), keeps the check strict for every other part and adds one field to a type that already exists. Option A reverses a rule merged on the same day and gives agents a silent way past a protection. Build the message and the pre-check of section 4 now, since they hold whichever the owner picks. The export problem needs a decision with B, and I would take it to the owner with the syntax.

## 4. Naming a known conflict before placing

### 4.1 Today

PR #98 proves the conflicting lines after a failed placement, which can take a minute. `constraints::plan` already refuses some conflicts before placing (a relation between two parts fixed outside its window, ADR 0045 section 12). The rest of section 13's proofs run in `explain.rs` only on a refusal. I read `explain.rs`: the proofs use the windows `plan` works out, the parts' footprints, the keepouts, cutouts and height limits, and the outline. They do not use the annealer's result. That is why a pre-check looks possible. An implementer should confirm it.

No language change is involved. The three options are three ways to show one check.

### 4.2 Option 1: a pre-check before placing

A pass over the same statements, run by `fab` before the annealer when the design has requirements. If it proves a conflict, `fab` stops with the block it writes to `placement-failure.txt` today, and says that it stopped before placing. It claims only what section 13 proves: a relation against the windows of its two parts, two relations on the same fields with no value in common, two parts whose courtyards overlap wherever their windows put them, a part and a keepout or cutout, a stated face under a height limit, a window with no place in the outline. Chains of three or more lines stay unproved and unclaimed. With `--constraints warn` the block is a warning and placing goes on, as the later check does now.

**Checker and placer.** The placer is untouched. The check gains an entry point that needs the design, the parts' footprints and no placement. `build` may not load footprints, so the pre-check may belong to `fab` and the editor first; I did not check. **Editor.** The same function is the engine of option 3. **Agents.** The loop shortens from a minute to a read, and the text is what an agent already parses from `placement-failure.txt`. **Migration.** None. ADR 0045 section 13 and the skills `netsmith-read-errors` and `netsmith-use-constraints` gain a paragraph.

### 4.3 Option 2: a `check` command output section

There is no `check` command: the CLI has `init`, `build`, `fab`, `editor`, `compare`, `catalog` and `dxf`. ADR 0030 named `netsmith layout check` and did not build it. A section in the output of an existing command (`build` or `fab --dry-run`) or a new `check` is the option. It would print the circuit's findings, then a "physical" section with the conflicts proved from the statements and the lines not yet measurable. A `--json` form would serve agents (ADR 0045 section 5). **Cost.** A new command is a surface to keep, and it competes with the name in ADR 0030. **Benefit.** An agent can run the physical check on its own, without paying for placement or routing.

### 4.4 Option 3: an editor squiggle

The editor already checks the physical file and lists its findings. The pre-check's lines have `file:line` spans (`Features::line_texts`), so a proved conflict can underline both lines and name the other in the tooltip, with a click to zoom. **Cost.** The check has to be fast enough to run on each edit, which the pre-check should be, and I did not measure. **Limit.** Agents do not see squiggles, so this is the human view of option 1.

### 4.5 Lean

Build option 1 first, with option 3 in the same change (a lean, not a decision). One engine serves both, and the editor already has the places to show it. Option 2 is the owner's call about the command line and can wait. It is also what makes question 3's answer independent of the annealer.

## 5. What would change, option by option

| Q | Option | Files and ADR sections |
|---|---|---|
| 1 | any | ADR 0031 "Parts on the back" (the amended two-face paragraph); ADR 0045 section 11 ("Both faces", "What a violation reports", "The placer") and section 13 (one more proved kind); a new section in ADR 0045 or an ADR of its own for the member (the owner's call); `GLOSSARY.md`; `docs/LANGUAGE.md` "Height limits"; `crates/netsmith-check/src/checker.rs` (the built-in member, where `.height` is defined), `crates/netsmith-lower/src/features.rs`, `crates/netsmith-core` (the design feature); `crates/netsmith-pcb/src/board.rs` (`Placed::room_on`, `rooms_overlap`, `check_placement` and a finding beside `TooTall`), `anneal.rs` (`rooms_on`, `room_pairs`), `place.rs` (`could_fit`, `face_forbidden`), `explain.rs`; `crates/netsmith-fab/src/lib.rs` (notes) and `drc/checks.rs` (`courtyards`); `crates/netsmith/src/fab.rs`; `crates/netsmith-workspace/src/findings.rs`; the skills `netsmith-prepare-fab-order`, `netsmith-use-constraints`, `netsmith-read-errors`, `netsmith-write-nsm` |
| 1 | A | adds `std/parts/connector.nsm` or a project part, with sourced numbers |
| 1 | B | adds `docs/LANGUAGE.md` "The physical file"; `crates/netsmith/src/physical.rs` and the checker's reach rules (a part's `.tail_height` is a statement it may make); the guide's physical block pair; the F5 check reads `.assembly` from the circuit source |
| 1 | C | adds `crates/netsmith-lang` (the parser, a statement naming pads), and the lowering's per-pad table |
| 2 | A | ADR 0045 section 10 rule 3; `GLOSSARY.md` _Overhang_, _Body_; `docs/LANGUAGE.md` "A part that hangs over the board edge"; `crates/netsmith-check` and `-lower` (the new fields), `crates/netsmith-pcb/src/overhang.rs`, `constraints.rs` (`stated_edges`), `board.rs` (`OutsideOutline`), `anneal.rs` (`Engine::room`) |
| 2 | B | ADR 0045 section 10 rule 1 and 3 and the owner decision of 2026-11-03; `GLOSSARY.md`; `docs/LANGUAGE.md`; `crates/netsmith-pcb/src/footprint.rs` (the warning at load), `board.rs`, `anneal.rs`, `place.rs` (`turn_to_fit`, the fit), `rules.rs` (if `M` is the process's); `crates/netsmith/tests/overhang.rs` |
| 2 | C | the physical-file reach rules, `docs/LANGUAGE.md`, `GLOSSARY.md` _Body_, `crates/netsmith-pcb/src/export.rs` (the courtyard written to the board file), `footprint.rs` |
| 3 | A | ADR 0045 section 13 and the "pinned inside a keepout" paragraph of section 10; ADR 0029; `GLOSSARY.md` _Keepout_; `docs/LANGUAGE.md` "Keepouts"; `crates/netsmith-pcb/src/constraints.rs` (`plan`), `anneal.rs` (`Engine::single`, `outside_at`), `board.rs` (`check_placement`), `explain.rs`, `export.rs`; `crates/netsmith-geom/src/drc` (`items_not_allowed`); tests `conflicts.rs`, `tight_limits.rs` flip |
| 3 | B | as A for the check, plus `std/pcb.nsm` (the `Keepout` interface), the lowering, and the same export change |
| 3 | C | as B, with one more value of `.parts` in `std/pcb.nsm` |
| 3 | baseline | `explain.rs` and `crates/netsmith/src/fab.rs` (message only); no ADR |
| 4 | 1 | ADR 0045 section 13; `crates/netsmith-pcb/src/explain.rs` and `place.rs` (an entry point before the annealer), `crates/netsmith/src/fab.rs`; the skills `netsmith-read-errors`, `netsmith-use-constraints`; `conflicts.rs` |
| 4 | 2 | `crates/netsmith/src/cli.rs` and `main.rs`; ADR 0030 (`layout check`); `README.md`; the skills |
| 4 | 3 | `crates/netsmith-workspace/src/findings.rs` and `crates/netsmith-editor/web` (then `scripts/web-bundle update`) |

## 6. Questions for the owner, ranked

Each lean is mine and may be wrong.

1. **Through-hole tails (section 1).** Which option states a trim: A on the part, B per instance in the physical file, or C per group of pads? *Lean: B first, building the one member that A also uses; C when a board needs a partial row.* Sub-questions that can flip it:
   - 1a. Is the covering part fitted by hand after the trim, or assembled by the board house? If assembled, only A works. *I assumed by hand.*
   - 1b. Does the covering part stand on the tails (the stack, `t + h <= clearance`) or on the board with its own gap (`t <= g`)? *I assumed the stack.*
   - 1c. May the exemption stand with no height zone over the pads (F6)? *Lean: yes, with a note.*
   - 1d. Is a covering part with no height an error (F2)? *Lean: yes, because the missing fact loosens a rule.*
2. **A courtyard past an edge with the body inside (section 2).** A: a statement allowing the courtyard; B: a bounded forgiveness with a library warning; C: a per-part override. *Lean: B, with a number `M` the owner picks, and it partly reverses the 2026-11-03 decision.*
3. **A pinned part inside a keepout for parts (section 3, issue #99).** A: the pin wins; B: the keepout names its exceptions; C: a keepout kind for mounting holes; baseline: the finding with a better message. *Lean: B; build the better message and the pre-check now.* The export to KiCad's rule area needs a decision with A, B or C.
4. **Naming a known conflict early (section 4).** A pre-check, a `check` section, an editor squiggle. *Lean: the pre-check and the squiggle together; the `check` command later.* No language change, and it makes question 3's answer independent of the annealer.

If one question is built first, build question 1 option B, with the order-of-work check (F5), the `trim:` note and the zone comparison in one change. It is the only one that blocks a design from existing.

## 7. What I did not check

- Nothing was built or run. No snippet was parsed; the `nsm-new` ones cannot be.
- Whether the flush statement of section 2.1 allows the courtyard as I read it.
- Whether `Placed::room_on`'s pad-box branch is the only place that judges a through-hole part's other face, beyond the ones named in section 1.10. I searched `board.rs`, `anneal.rs`, `place.rs` and `explain.rs` by name, not every caller.
- Whether KiCad's default check flags the cell over a socket in the cases of section 1.10. ADR 0031's follow-up is the only evidence.
- Whether `explain.rs`'s proofs need only the plan and the footprints (section 4.1). I read the file's header and two functions.
- Whether JLCPCB offers to trim leads. Nothing in the repo says so, and the note treats it as a hand step.
- Timings: nothing here was measured, including "a minute" for a failed placement, which is the brief's figure.
