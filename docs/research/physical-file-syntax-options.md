# The physical file: the decided trade-off and the open syntax choices

Options note for issue #2, third round (the owner's decisions of 2026-11-04, recorded in [ADR 0048](../adr/0048-physical-intent-has-a-home-of-its-own.md)). Written while the owner was away. It decides nothing about the language: every syntax below is invented to compare options, and language-design choices are the owner's (`AGENTS.md`). Vocabulary follows `GLOSSARY.md`; "the physical file" is the placeholder name ADR 0048 uses, not a keyword.

Snippets marked `nsm-new` use constructs that do not exist. Snippets marked `nsm-fragment` use only constructs that exist today (or invented library parts, named so) and are pieces of a file. Nothing here was run: no build was allowed while this was written. The repo is public, and the owner's test board is private, so every board, part, name and number below is invented. They are not the test board's.

## Where things stand

The owner confirmed that physical intent is written in a file of its own, apart from the circuit source (decisions 1 and 5), after seeing it beside the `pcb { }` block. Section 1 keeps that comparison for the record, framed as the trade-off that was decided. Sections 2 to 8 are the choices still open. They are written in the file form only, as the owner asked.

The other decisions that hold throughout: layered, board first (2); the case numbers in a separable case section (3); the board's file reaches a part inside a module by path, read-only, with optional anchors (4); the circuit source keeps what the circuit's checks consume (6); the editor writes requirements to the physical file and solutions to the layout file (7); the old form leaves with no grace period (10).

Leans, each marked **Lean** where it appears, are mine and the owner may weigh them differently:

| Choice | Lean |
|---|---|
| 2 name, extension, header, pairing | `physical`, `<stem>.physical.nsm`, header `physical for Board`, paired by stem as the layout file is |
| 3 case section | a nested `case { }` section |
| 4 reach and anchors | plain path; an `anchor` alias as the option |
| 5 relations | the equation form of ADR 0045, no new construct yet |
| 6 face for a module | `.place.side` on the module, hard; a group later if a set across modules is needed |
| 7 body region | a `Region` carried by a part, in the module's file |
| 8 module's file | attached by an explicit statement in the module; a board may only narrow |

## 1. A file or a block: the decided trade-off

### 1.1 The example

One board, `Trellis`, a sensor node with a plug-in radio. It is built the way the owner's test board is built, in shape only: the board declares module instances and a process, and every part that the case or a datasheet fixes is inside a module. The part kinds are invented library parts. The wiring is elided with `...`.

```nsm-fragment
// trellis.nsm: the circuit source, the same in both spellings
import std::mfg::jlcpcb::FourLayer
import trellis::parts::{Plug16, Socket1x10, PatchAntenna, CoaxReceptacle, TestPad, Accelerometer, Microphone}

module RadioDock {
    .row_a: Socket1x10 { .assembly &= "hand" }
    .row_b: Socket1x10 { .assembly &= "hand" }
    ...
}

module GnssFront {
    .antenna: PatchAntenna
    .coax: CoaxReceptacle
    .antenna.impedance == .coax.impedance      // the circuit's own fact, stays here
    ...
}

module Sensing {
    .accel: Accelerometer
    .mic: Microphone
    ...
}

module Fixture {
    .t1: TestPad
    .t2: TestPad
    .t3: TestPad
    .t4: TestPad
    ...
}

module Supply { ... }
module Inlet {
    .plug: Plug16 { .assembly &= "hand" }
    ...
}

board Trellis {
    .process: FourLayer {
        .inner1.net &= "GND"          // consumed by the circuit's checks and the router: stays
        .inner2.net &= "3V3"
    }
    .inlet:   Inlet
    .supply:  Supply
    .radio:   RadioDock
    .gnss:    GnssFront
    .sensing: Sensing
    .fixture: Fixture
    ...
}
```

The physical intent for it, in the form the language has today or the closest invented form, is about 30 lines. It is the same lines in both spellings below.

- The case: a 44 by 72 mm outline with rounded corners, a small window near the top left, the plug ending the bottom edge with its body past it, and a zone over the radio where nothing may stand taller than 7 mm.
- Fixed by datasheets: the radio's two socket rows 17.5 mm apart; the antenna and the coax receptacle on one centreline.
- Decisions of the layout designer that are requirements: the supply module on the bottom face, the GNSS module on the bottom, the test pads on the top at a 3 mm pitch in a line (four pads here, a dozen on a real fixture).

### 1.2 The file form

```nsm-new
// trellis.physical.nsm
physical for Trellis {
    // Case drawing TC-12, sheet 1.
    .outline: Outline { .width &= 44mm  .height &= 72mm  .corner_radius &= 1.5mm }
    .window:  Cutout  { .x &= 3mm  .y &= 60mm  .width &= 5mm  .height &= 3mm  .corner_radius &= 0.5mm }
    // The plug ends the bottom edge and sticks out of it.
    .inlet.plug.place.x      &= 22mm +/- 0.1mm
    .inlet.plug.place.bottom >= .outline.height
    .inlet.plug.place.side   &= "top"
    // The radio's body hovers here: nothing taller than 7 mm under it.
    .radio_zone: Keepout {
        .x &= 8mm  .y &= 20mm  .width &= 28mm  .height &= 24mm
        .max_height &= 7mm
        .layers &= "top"
    }

    // The radio datasheet: two socket rows, fixed apart.
    .radio.row_a.place.left &= 8mm +/- 0.2mm
    .radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.05mm
    // The feed between antenna and receptacle is one short straight run.
    .gnss.antenna.place.y == .gnss.coax.place.y
    .gnss.antenna.place.top &= 4mm +/- 0.2mm
    .sensing.mic.place.left &= 3.5mm +/- 0.1mm

    // Faces.
    .supply.place.side  &= "bottom"
    .gnss.place.side    &= "bottom"
    .fixture.place.side &= "top"
    // The fixture's pitch.
    .fixture.t2.place.x - .fixture.t1.place.x == 3mm +/- 0.05mm
    .fixture.t3.place.x - .fixture.t2.place.x == 3mm +/- 0.05mm
    .fixture.t4.place.x - .fixture.t3.place.x == 3mm +/- 0.05mm
    .fixture.t2.place.y == .fixture.t1.place.y
    .fixture.t3.place.y == .fixture.t1.place.y
    .fixture.t4.place.y == .fixture.t1.place.y
}
```

`trellis.nsm` is exactly the listing in 1.1, with no physical line. `netsmith build trellis.nsm` needs no other file. `fab` and the editor find `trellis.physical.nsm` beside it, as they find `trellis.pcb.toml` (ADR 0032).

### 1.3 The block form (not chosen)

```nsm-new
// trellis.nsm: the circuit source of 1.1, with the physical intent added at the end of the board
board Trellis {
    .process: FourLayer { ... }
    .inlet:   Inlet
    ...
    physical {
        // Case drawing TC-12, sheet 1.
        .outline: Outline { .width &= 44mm  .height &= 72mm  .corner_radius &= 1.5mm }
        .window:  Cutout  { ... }
        .inlet.plug.place.x      &= 22mm +/- 0.1mm
        .inlet.plug.place.bottom >= .outline.height
        ...                        // the same lines as in 1.2
    }
}
```

A variant puts a sibling item `physical for Trellis { ... }` at the end of `trellis.nsm`. It has the block's pairing (none, same file) and the file's header, and none of the file's separation. It is not a third thing to weigh; it fails the owner's reasons for the file in the same way the block does.

### 1.4 Side by side

| | File (decided) | Block (not chosen) |
|---|---|---|
| The circuit source in the example | the listing of 1.1, no case number | the same listing plus the physical lines inside `board Trellis` |
| A case revision (the window moves, the outline grows by 2 mm) | a diff of `trellis.physical.nsm` only | a diff of `trellis.nsm`, inside the block |
| Who reviews a case revision | the owner of the file, by the tools review already uses (a path rule, a code owner) | whoever owns the circuit file; a path rule cannot tell the block from the circuit |
| Reading the circuit source | strictly the circuit, in the editor's source pane too | the block is there, foldable, but it is text of the circuit file |
| An agent that works on the circuit | does not read the physical file | reads the file whole, or must be told to skip the block |
| An agent that lays out the board | reads two files and the layout file | reads one file and the layout file |
| Pairing a board with its physical intent | a rule is needed (stem beside the source, as the layout file) | none |
| Module reuse (issue #16) | the module's file must be found and shipped with the module (section 8) | the block is in the module's source and ships with it for free |
| Renaming a part inside a module | breaks paths in other files; the checker names them; the editor can rewrite both | changed in one file, in one edit |
| Schematic-only run (`build`) | does not read the file | parses and skips the block |
| Cross-file resolution in the checker | needed | not needed |
| The guide's test (`guide.rs`: one complete file per ```` ```nsm ```` block) | a second kind of block that pairs a physical file with its board | unchanged |
| `netsmith init`, skills, examples | a second file in each template and example | unchanged |
| Editor write target | a file for requirements, a file for solutions; the rule is "requirement or solution?" | the same rule, but requirements go into a block of the file the person is editing in the source pane |
| A `system` or a `panel` | a `physical for Kit` file | a block in the `system` |
| ADR 0001 (one way) | holds | holds, and it needs the in-body lines to go as well |

### 1.5 What was decided and what it costs

The owner chose the file. Their reasons are the first two rows of the table that matter to them: a case revision and a circuit change are reviewed by different people, and the circuit source stays strictly schematic. The block would have been cheaper in the rows that touch the checker, the editor, the guide and module reuse. Those are recorded as known costs in ADR 0048:

1. **A module's physical file must travel with the module.** The checker should know the file exists, so that its absence is an error and not a silent loss. Section 8 offers three ways.
2. **A rename touches two files.** The editor's rename should rewrite both. An anchor (section 4) limits it to one alias.
3. **An optional module file is distributed with the module's package.** A module used by another project brings its file, or its geometry is gone.
4. **A pairing rule**, **three files per board**, and **a new kind of guide example**.

## 2. The file's name, extension and header

The placeholder is "the physical file". The word `pcb` is out of the running as a type name: the glossary lists "PCB" under Avoid for _Board_, and `<stem>.pcb.toml` is already the layout file, which holds the solution. A `.pcb.nsm` next to a `.pcb.toml` would invite the wrong file to be edited.

### Option N1. A header that names the board

```nsm-new
// trellis.physical.nsm
physical for Trellis {
    .outline: Outline { .width &= 44mm  .height &= 72mm }
}
```

- **Pros.** The file says which board it belongs to. One file can hold the units of several boards, as the layout file has one table for each board. A board named wrongly is an error that lists the boards the source has, the layout file's message (ADR 0032).
- **Cons.** A new top-level word and a second header form beside `board`, `module`, `system` and `panel`. `for` says "extends something defined elsewhere", a thing the language has not had.
- **Checker.** Resolves the name to a board or module in the paired source; a unit for a board that is gone is an error.
- **Editor.** The file tree pairs by stem, and a tab shows the unit that belongs to the selected board.
- **Agents.** One line to learn ("a physical file starts with `physical for` and the board's name").

### Option N2. No header; the file is the unit

```nsm-new
// trellis.physical.nsm
.outline: Outline { .width &= 44mm  .height &= 72mm }
.inlet.plug.place.bottom >= .outline.height
```

- **Pros.** The least syntax. Nothing to say the file's board; the stem says it.
- **Cons.** The language has no top-level statements today, and statements outside a definition would be the first. A source file with several boards has no way to say which board a line is for. A renamed file silently changes its meaning.
- **Checker.** A new top-level form; the pairing is by name only.
- **Editor.** The same as N1.
- **Agents.** Fewer words, but a bare file is hard to tell from a fragment.

### Option N3. A definition of its own, named like the board

```nsm-new
// trellis.physical.nsm
physical Trellis {
    .outline: Outline { .width &= 44mm  .height &= 72mm }
}
```

- **Pros.** The same shape as `board Trellis` and `panel LedPanel` (ADR 0040).
- **Cons.** Two definitions with the same name, told apart by the kind word, which no pair of definitions has now. A reader may think it declares a new board.
- **Checker.** The kind word is a namespace; names must be unique within a kind.
- **Editor, agents.** As N1.

### The extension and the pairing

| | Example | Note |
|---|---|---|
| `.physical.nsm` | `trellis.physical.nsm` | the same language, so every `.nsm` tool (the formatter, the editor's source pane, syntax highlighting) works at once |
| a new extension | `trellis.nsp` | a path rule can match it with one glob; every tool needs to learn it |
| `.pcb.nsm` | `trellis.pcb.nsm` | rejected above |

The pairing rule is a sibling choice. **P1**, beside the source with the stem and the suffix, the layout file's rule: `netsmith build trellis.nsm` finds `trellis.physical.nsm`. **P2**, listed in `netsmith.toml`, which says nothing in the file tree. **P3**, a statement in the board naming the file (`physical "trellis.physical.nsm"`), which lets the checker error when the file is missing but puts a pointer in the circuit source. P1 is the least to write and the layout file already works that way.

**Lean.** N1 with `.physical.nsm` and P1. N2 is the smallest but the language has no top-level statement and a file with two boards cannot be paired. N3's two definitions with one name is the thing I would avoid. The word `physical` is the glossary's own; I do not lean on it hard.

## 3. The case section

Decision 3 wants the case numbers in a separable section, so that a shared `enclosure` definition (issue #46) can take it unchanged. The question is how a section is marked.

### Option C1. A nested section

```nsm-new
physical for Trellis {
    case {
        .outline: Outline { .width &= 44mm  .height &= 72mm  .corner_radius &= 1.5mm }
        .window:  Cutout  { .x &= 3mm  .y &= 60mm  .width &= 5mm  .height &= 3mm }
        .inlet.plug.place.bottom >= .outline.height
    }
    .radio.row_a.place.left &= 8mm +/- 0.2mm
}
```

- **Pros.** Lifting it out is cutting the lines inside the braces into another definition. The checker knows which lines are the case's, so a finding can say "the case says" and the editor can draw them as an underlay.
- **Cons.** A new nested form. Lines in the case may reach parts (the plug), so a lifted case would carry paths into the board's modules.
- **Checker.** A scope with no new rule: statements are as outside.
- **Editor.** A "case" group in the physical tab; a keepout drawn as the case's goes in it.
- **Agents.** One rule: "numbers from the case drawing go in `case`".

### Option C2. A convention, no syntax

The same lines, with a comment above them. Nothing to build. The checker, the editor and a later lift cannot tell the case's lines from the rest, and a lift is an edit by hand. This fails the word "separable" in decision 3 if it means a tool can separate it.

### Option C3. A second physical file for the case

```nsm-new
// trellis.case.nsm
case for Trellis {
    .outline: Outline { .width &= 44mm  .height &= 72mm  .corner_radius &= 1.5mm }
}
```

- **Pros.** The mechanical owner's diff is a file of its own, which is the owner's own reason for the file. Lifting it to an `enclosure` is a rename.
- **Cons.** Four files per board. A case line that reaches a part in a module (the plug) is in a different file from the other lines about that part.
- **Checker, editor.** Two units to pair for each board.
- **Agents.** Two physical files to read.

**Lean.** C1. It keeps the board's physical intent in one file and still lifts out cleanly. C3 is the one to pick if the case really has its own owner and reviewer.

## 4. Reaching a part by path, and anchors

Decision 4 allows both. The question is how each is written. The examples use the two-row socket module of section 1.

### Option R1. A plain path

```nsm-new
physical for Trellis {
    .radio.row_a.place.left &= 8mm +/- 0.2mm
    .radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.05mm
}
```

- **Pros.** Nothing to learn; it is the path the layout file's keys already use (`"stage.output_cap"`) and the path the circuit's own `.radio.row_a` would have if ADR 0004 allowed it. Any depth reaches without a handle.
- **Cons.** Reach looks the same as an ordinary member access, so a reader cannot see that this one crosses a module boundary. A rename inside `RadioDock` breaks every board's file.
- **Checker.** A new rule: in the physical file a path may cross a module boundary, and only to state physical intent, never to narrow `.value` or change a connection. An attempt to set a part's value is an error that says a physical file states where a part is, not what it is.
- **Editor.** "Pin here" writes the path the click resolved to.
- **Agents.** The rule "the physical file may name any part by its path" fits in one sentence.

### Option R2. A marked reach

```nsm-new
physical for Trellis {
    .radio->row_a.place.left &= 8mm +/- 0.2mm
}
```

- **Pros.** The module boundary is visible; a search for `->` finds every reach into a module.
- **Cons.** A second kind of path punctuation, and a path that is one spelling at the board level and another at the module level (`.row_a` inside the module). ADR 0001 reads this as two spellings for one path.
- **Checker.** The marker is the signal that the reach rule applies.
- **Editor, agents.** One more form to teach.

### Anchors, the option a module author may use

An anchor is a stable name for a part or a place in a module, declared by the module author, that a board's file can use in place of the path.

**Option A1. An alias declaration** in the module's physical file:

```nsm-new
// src/radio_dock.physical.nsm
physical for RadioDock {
    anchor .dock = .row_a
}

// trellis.physical.nsm
physical for Trellis {
    .radio.dock.place.left &= 8mm +/- 0.2mm        // reaches .radio.row_a, by its stable name
}
```

**Option A2. A pose value** joined to the part with `~` (the idea ADR 0045 section 3 raised, moved into the module's file):

```nsm-new
physical for RadioDock {
    .dock: Pose
    .row_a.place ~ .dock
}
// the board: .radio.dock.left &= 8mm +/- 0.2mm
```

- **A1.** Pros: it is a name for a path, nothing more, and a rename inside the module changes the alias in one place. Cons: a new statement kind. The anchor is a second spelling of a path that still works (decision 4 accepts it).
- **A2.** Pros: reuses `~` and an interface. Cons: it is the handle of ADR 0042 again, in another file, and the pose is a value that carries several fields while the alias is a name. A new interface kind, `Pose`, whose join to a built-in `.place` is new.
- **Where an anchor is declared.** The module's circuit source would put a physical name in the circuit's contract, which decision 6 avoids. So both options assume the module's physical file, which does not exist until the module layer (decision 2). A board's file can use paths until it does.

**Lean.** R1 for the reach, A1 for anchors. I see no reason for the marker (R2) when the checker can say where a path crossed a boundary; A1 is what "stable names" says in decision 4.

## 5. Relations between two parts

ADR 0045 C2. The test board needs a fixed row spacing and an aligned pair; a fixture needs a pitch.

### Option L1. The equation form of ADR 0045

```nsm-new
physical for Trellis {
    .radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.05mm
    .gnss.antenna.place.y == .gnss.coax.place.y
    .fixture.t2.place.x - .fixture.t1.place.x == 3mm +/- 0.05mm
    .fixture.t3.place.x - .fixture.t2.place.x == 3mm +/- 0.05mm
    .fixture.t4.place.x - .fixture.t3.place.x == 3mm +/- 0.05mm
}
```

- **Pros.** The language's existing relation (ADR 0045 section 3 shows the checker accepts these lines and prints `note: not evaluated`). No new construct. Order does not matter.
- **Cons.** A pitch of N pads is N-1 lines, and an alignment of N pads another N-1: 22 lines for a dozen pads. The intent ("a row of pads at 3 mm") is not named.
- **Checker.** The lowering keeps them as `Offset { a, b, axis, window }` for the placer and the check (ADR 0045 section 3). The finding prints the measured value, the window and the margin.
- **Editor.** A thin line between the parts with the window; "pin here" writes one of these lines.
- **Agents.** The form they have already written for ADR 0045.

### Option L2. A named relation as a member

```nsm-new
physical for Trellis {
    .rows: Spacing { .a ~ .radio.row_a  .b ~ .radio.row_b  .x &= 17.5mm +/- 0.05mm }
    .feed: Aligned { .a ~ .gnss.antenna  .b ~ .gnss.coax  .axis &= "y" }
}
```

- **Pros.** The relation has a name a finding can quote and the editor can list. The two parts are joined, not named in an expression.
- **Cons.** A new interface kind for each relation, and `~` joins a part, which it does not today. A row of pads still needs a pitch kind.
- **Checker.** A new type check; lowering is the same `Offset`.
- **Editor, agents.** A catalogue of relation names to learn.

### Option L3. A statement for each relation

```nsm-new
physical for Trellis {
    space .radio.row_a .radio.row_b x 17.5mm +/- 0.05mm
    align .gnss.antenna .gnss.coax y
    pitch .fixture.t1 .fixture.t2 .fixture.t3 .fixture.t4 x 3mm +/- 0.05mm
}
```

- **Pros.** The intent is the line. A row of any length is one line.
- **Cons.** Three new verbs and a mini-language beside the quantity algebra, which ADR 0045 section 3 rejected as its Option C. A fourth relation is a fourth verb. An alignment is also `a.y == b.y`, so two ways to say one thing (ADR 0001).
- **Checker.** Each verb lowers to `Offset`s; a list of parts is a new syntax.
- **Editor, agents.** Short lines; every relation shape a new word.

**Lean.** L1 now. It is the form ADR 0045 designed and the owner's test board needs two or three of them. The fixture's pitch is where it is clumsy; if a fixture of a dozen or more pads is written by hand, that is the case to design L3's `pitch` for, with an example in hand.

## 6. A face for a module

Today `.place.side` on a module is refused. The layout designer decides faces by module (the supply on the bottom). The question is what a face on a module means and where a part that disagrees goes.

### Option M1. `.place.side` on the module, a hard requirement

```nsm-new
physical for Trellis {
    .supply.place.side &= "bottom"
    .gnss.place.side &= "bottom"
}
```

- **Pros.** The module's own `.place` gets the one field that was withheld. Every part of the module is on that face. It reads like the module's edges (`.sensor.place.left >= 20mm`, which already bounds every part of a module).
- **Cons.** A part of the module that needs the other face (a connector at an edge) conflicts, and the error names both lines. The module must then state its parts' faces one by one (M3) and cannot use M1.
- **Checker.** An empty intersection of a part's side and the module's side is the usual error. A through-hole part is judged by its pads on the face as ADR 0031 has it.
- **Editor.** The PCB view shades a module's parts by face; flipping a module writes the layout file's `side` for each part (a solution), and the requirement stays.
- **Agents.** One line for a module's face. Exceptions need a per-part list.

### Option M2. A group of paths

```nsm-new
physical for Trellis {
    group back {
        .side &= "bottom"
        .supply
        .gnss
        .sensing.accel
    }
}
```

- **Pros.** A set across modules, a module and part of another. Issue #2 asked for "a module or a set".
- **Cons.** A new construct, a new list-of-paths type, and a name with no use but this. A part in two groups is the empty-intersection error again.
- **Checker.** Membership lists are resolved by path; the rest is as M1.
- **Editor.** A group is a selection that can be named.
- **Agents.** More to teach; sets are needed less often than modules.

### Option M3. Per-part lines only

```nsm-new
physical for Trellis {
    .supply.regulator.place.side &= "bottom"
    .supply.inductor.place.side  &= "bottom"
    .supply.input_cap.place.side &= "bottom"
}
```

- **Pros.** Nothing new; decision 4's reach makes it possible. This is the baseline.
- **Cons.** A module of twenty parts is twenty lines, and a new part in the module is not covered.
- **Checker, editor, agents.** Nothing new.

**Lean.** M1, with M3 as the way to state an exception, and M2 only if a set across modules is really needed. A "face unless a part says otherwise" default is not available: a narrowing cannot be loosened, so a module's face is either a requirement on every part or it is not stated.

## 7. A body region with a height limit

A plug-in module hovers over the board on its sockets. The region under its body is limited in height. Section 1 used `Keepout` with `.max_height` (ADR 0045 section 11) in the board's frame, which works for the board and does not follow the module if it moves.

### Option G1. The keepout, as built

```nsm-new
physical for Trellis {
    .radio_zone: Keepout {
        .x &= 8mm  .y &= 20mm  .width &= 28mm  .height &= 24mm
        .max_height &= 7mm
        .layers &= "top"
    }
}
```

- **Pros.** Built (provisional). Needs no new construct, and a board-level zone is what a case with a rib is.
- **Cons.** Its place is in the board's frame, so the zone and the sockets are two statements that must agree. A module on another board needs the zone restated. A region that is "the module's body" has no name that says so.
- **Checker.** Unchanged: `TooTall`, `height_unchecked`.
- **Editor.** Already drawn in amber with the limit.

### Option G2. A region carried by a part, in the module's file

```nsm-new
// src/radio_dock.physical.nsm
physical for RadioDock {
    .body: Region {
        .origin ~ .row_a.place
        .dx &= 0mm  .dy &= 0mm  .width &= 28mm  .height &= 24mm
        .max_height &= 7mm
    }
}
```

- **Pros.** The region moves with the sockets, as a keepout moves with a part that carries it (ADR 0029). It is the module's fact and reusable. It is first-class, as decision 9 asked.
- **Cons.** A new type, a join of its origin to a part's place, and the placer has to carry a region with a module and rotate it. Rotation by other than a right angle is not offered.
- **Checker.** A `Region` is a `Keepout` that takes its frame from a part; the height check is the same.
- **Editor.** The region follows the module when dragged.

### Option G3. The plug-in as a part

The radio module is a part with its own footprint and `.height`, fitted by nobody (`.assembly &= "none"`). Its courtyard and height are the region.

- **Pros.** No new construct; heights exist (ADR 0045 section 11).
- **Cons.** The circuit source now names a thing that is not part of the circuit's copper, a footprint and a mapping for it, and the bill of materials lists it. It models "a body is over here" as "a part is here", which is true only because a socket holds it.
- **Checker.** Nothing new. **Editor.** It is a part, so it is placed like one.

**Lean.** G2, carried by a part, because "the module's body" is the module's fact and G1 repeats it on every board. G1 is the right stopping point if the owner has only one board that uses the module.

## 8. A module's own physical file

Decision 2 adds a module layer later. This is how it attaches and what a board may do to it. The module's file states the module's relative geometry (the row spacing, the antenna alignment, its body region, its anchors) and never a position in a board's frame.

### How it attaches

**Option T1. By stem beside the module's source**, as the board's does.

```nsm-new
// src/radio_dock.nsm              the module
// src/radio_dock.physical.nsm     physical for RadioDock { ... }
```

Nothing to write, and a module that moves without its file loses the geometry without a message. Section 1's first known cost.

**Option T2. By the board's file naming it.**

```nsm-new
physical for Trellis {
    use physical src::radio_dock
}
```

The board's author decides which module files apply, and a missing file is an error. A board that forgets the line gets no geometry, silently. The geometry is the module's, so the board should not have to remember it.

**Option T3. By a statement in the module.**

```nsm-fragment
// src/radio_dock.nsm
module RadioDock {
    physical "radio_dock.physical.nsm"      // invented: the circuit source names its file
    .row_a: Socket1x10
    ...
}
```

The module declares its file; the checker errors if it is missing; every board that uses the module gets it. The cost is one line of physical pointer in the circuit source.

### What a board may do with it

**Option V1. Narrow only.** A board's file may tighten a range the module states, as every narrowing in the language does. A line that would loosen it, or that disagrees, is the empty-intersection error with both lines named. If a board needs a different spacing, the module changes or offers a value.

**Option V2. V1 plus offered values.** A module's file declares a value, and the board narrows it, the idiom of ADR 0042:

```nsm-new
// src/radio_dock.physical.nsm
physical for RadioDock {
    .row_pitch: Length
    .row_b.place.x - .row_a.place.x == .row_pitch
}
// the board's file
physical for Trellis { .radio.row_pitch &= 17.5mm +/- 0.05mm }
```

**Option V3. A board's file replaces the module's.** Rejected on its face: it is not narrowing, the language's one way to change a range (ADR 0001, ADR 0018).

**Lean.** T3, V1, and V2 when a module needs a board-dependent value. T3 is the one that answers the known cost of a lost file; T1 costs nothing and loses it. V3 I would not build.

A module's file reaches its own parts and, by the same read-only rule as decision 4, those of modules inside it. Whether it may state an overhang stays open (ADR 0045 section 10 (h)).

## 9. Migration of an existing example

`examples/outline_and_pours.nsm` is the one example with all of the old form: `Outline`, `Cutout` and `Keepout`, and a layout file entry for the keepout's place. The after below uses the leans of sections 2, 3 and 8 (N1 and P1, a nested `case { }`) so that the file is whole; the syntax is not chosen.

**Stays in the circuit source.** The process, the plane nets, the pours and their stitching, the rails and their `.min_track`, the stage, the headers. The mounting holes, fiducials and label are parts with no circuit meaning; ADR 0048 leaves where they are declared open, so they stay in this example.

**Before**, in `examples/outline_and_pours.nsm` (lines 51 to 72, and the import on line 12):

```nsm-fragment
import std::pcb::{Cutout, Keepout, Outline}

    /// 50 by 36 mm, with rounded corners.
    .outline: Outline {
        .width &= 50mm
        .height &= 36mm
        .corner_radius &= 3mm
    }

    /// A rounded slot cut through the board, 6 mm in from the left edge.
    .slot: Cutout {
        .x &= 6mm
        .y &= 12mm
        .width &= 3mm
        .height &= 12mm
        .corner_radius &= 1mm
    }

    /// Where an antenna would be: no copper and no parts on the top layer. The layout file
    /// says where.
    .antenna: Keepout {
        .width &= 14mm
        .height &= 5mm
        .layers &= "top"
        .vias &= "allow"
    }
```

and in `examples/outline_and_pours.pcb.toml`:

```toml
# Where the antenna keepout is, in mm from the outline's top left corner.
[OutlineAndPours.keepouts.antenna]
at = [18.0, 2.0]
```

**After**, `examples/outline_and_pours.nsm` loses those lines, the import, and the second paragraph of its doc comment (which now says "the physical file holds the outline, the slot and the antenna's keepout"). The board declares no `.outline`.

New file `examples/outline_and_pours.physical.nsm`:

```nsm-new
//! The case and the antenna's clear area of the four-layer regulator board.

import std::pcb::{Cutout, Keepout, Outline}

physical for OutlineAndPours {
    case {
        /// 50 by 36 mm, with rounded corners.
        .outline: Outline {
            .width &= 50mm
            .height &= 36mm
            .corner_radius &= 3mm
        }

        /// A rounded slot cut through the board, 6 mm in from the left edge.
        .slot: Cutout {
            .x &= 6mm
            .y &= 12mm
            .width &= 3mm
            .height &= 12mm
            .corner_radius &= 1mm
        }
    }

    /// No copper and no parts on the top layer where the antenna is, 18 mm along the top edge.
    .antenna: Keepout {
        .x &= 18mm
        .y &= 2mm
        .width &= 14mm
        .height &= 5mm
        .layers &= "top"
        .vias &= "allow"
    }
}
```

and `examples/outline_and_pours.pcb.toml` becomes `version = 1` alone (or is deleted: a layout file is optional). The keepout's place moved from the layout file to the physical file because the antenna's position is a requirement (decision 7 and ADR 0029, and stating both `.x` and an `at` is an error). The editor's drag of a source-sized keepout now writes the physical file.

What else the migration touches for this example:

- The mounting holes locked in the corners come from the outline: the placer reads the merged design, so nothing else changes for the holes.
- `crates/netsmith/tests/dxf.rs` reads this example's layout file as a fixture and imports a case onto it; its expected output changes with `--write`.
- `docs/LANGUAGE.md` has an example with `file=examples/outline_and_pours.nsm`. That block becomes two blocks, and `guide.rs` needs a kind of block that pairs a physical file with its board (the file-versus-block cost, section 1).

`examples/led_panel.nsm`: the five lines of `.outline` and the `import std::pcb::Outline` move to `examples/led_panel.physical.nsm` as `physical for LedTile { case { .outline: Outline { .width &= 40mm  .height &= 30mm  .corner_radius &= 0mm } } }`. The process's `.min_edge_clearance` stays in the board: the circuit's checks consume it. The panel reads the cell's outline from the merged design, so the checker must put a board's physical file with its board before a `panel` checks its cell. Its comment that a V-cut needs square corners moves with the line.

## 10. Questions for the owner, ranked

The file is decided. **Questions 1 to 7 were answered on 2026-11-04, each as the lean the note gave, with one addition: the board's mounting holes, fiducials and labels go in the case section, and a part's `.height` stays on the part (ADR 0048, "Syntax decisions").** Questions 8 and 9 are open and are not needed until the module layer and a physical file for a `system` or a `panel` are built. The list below is kept as asked.

1. **The file's name, extension, header and pairing** (section 2). Is it `physical for Trellis` in `trellis.physical.nsm`, found beside the source, or something else? Lean: that.
2. **How does the board's file reach into a module, and are anchors an alias or a pose?** (Section 4.) Lean: a plain path, and an `anchor` alias.
3. **Relations between two parts** (section 5, decision 8). Lean: the equation form now.
4. **A face for a module** (section 6, decision 9). Lean: `.place.side` on the module, hard, with per-part lines for an exception.
5. **A body region with a height limit** (section 7, decision 9). Lean: a `Region` carried by a part, in the module's file; the keepout is fine until then.
6. **How the case section is marked** (section 3). Lean: a nested `case { }`.
7. **Where parts that exist for the board's body are declared, and where a part's `.height` is stated.** Mounting holes, fiducials, labels and heights are not placed by decision 6. Section 9 leaves the holes, fiducials and label in the circuit source for now.
8. **How a module's file attaches, what a board may override, and whether the module layer may state an overhang** (section 8). Not needed until the module layer is built. Lean: the module names its file; a board may only narrow. Ask before that work starts, since a module shipped without its file loses geometry silently.
9. **A physical file for a `system` and a `panel`** (ADR 0040). Not designed; its needs are mounting holes of a panel, mated-connector alignment across boards and the panel's rails.

## 11. What I did not check

- Nothing was built or run: the `nsm-new` and `nsm-fragment` snippets were not parsed.
- That each lean fits the checker's existing relation machinery. I read ADR 0045 and the guide's section on `.place`, not the code that lowers a relation.
- The lines of `crates/netsmith/tests/guide.rs` beyond its header: that it would need a new block kind is from the header's list of kinds, not from reading the whole test.
- Whether `case { }` (section 3) or `group` (section 6) collide with a name the lexer reserves. I did not read the grammar.
- The files the migration touches were counted by `grep` for `.place.` and `Outline {`, not by reading each use.
