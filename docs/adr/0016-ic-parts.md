---
status: accepted
---

# An IC is a part with typed pins, a pinout per package, and a KiCad mapping read from the installed libraries

The user chose a pin table in the part (Option B of the proposal): a line of the first text, "Accepted. The user chose a pin table in the part (Option B of the proposal, kept below as the decision)". Tests 1 (every std part and every project's part is written this way) and 3 (the other two options). Cut from `git show 8023ee9:docs/adr/0016-ic-parts.md`; source files cite its decision numbers (kept here), and the rest is in the places named below.

## The problem

Only five kinds of part exported, and their KiCad names were a Rust table. A real board needs multi-pin ICs. Two cases drive the design: a fixed-output LDO in a package with 9 pads (two are the output, its sense pin must be tied to the output, one pad is no-connect, ground is two pads), and a microcontroller with about 30 pins, several with more than one function, where the design chooses one. The language could not say a pin's electrical kind, its datasheet name or its functions, gave one pin one pad, gave pads by catalog columns in Rust, and checked nothing about a pin being connected or tied.

## Decision

A part declares its pins in a `pins` table, the datasheet's pin table, one table for each package. A block named for a package applies in that package; an unnamed block applies to every package, for a part whose pads do not depend on it. A part has one unnamed block or one for each package, not both, so a pin never has two answers. A row, on one line or ended by `;`:

```text
.path  kind  "NAME"  "pad" ["pad" ...]  [fn FUNCTION ...]  [optional | required]
```

A row names a plain pin (`.sense`), which it declares, or a pin of an interface (`.vout.pos`). `tie .a ~ .b` is a statement of the part. The rules that hold whatever the syntax:

1. A part says what is true of the part and a pinout what is true of one package. The kinds, names and functions are the same in every package; the pads are not. A pinout names no CAD tool, so it is library source (ADR 0002). The `ldo` catalog lost its four `*.pad` columns: the row names the package and the pinout supplies the pads.
2. A pin has a kind from a closed set: `power`, `input`, `output`, `bidirectional`, `passive` and `no_connect` (the KiCad types that mean something to a checker); `tri_state` was added by ADR 0064. Others are added when a part needs them.
3. A pin has the datasheet's name, case-insensitive, which the symbol is validated against.
4. The checker reports pin faults on the board before the solver runs: a required pin unconnected, a tied pin not tied, a no-connect pin connected, two outputs on a net, a function not chosen, not offered, or used twice. The table with the messages is in the header of `crates/netsmith-check/src/faults.rs`. A function is chosen on the board because the board decides what the pin is for; there is no default function.
5. Pads are checked after the solver chose the package: two pins on one pad that the part does not join is an error; a pin joined inside the part to a pin that has pads is an alias and needs none.
6. KiCad names live in a per-tool TOML mapping keyed by part and package, not in the part and not in Rust (`std/kicad.toml`, and a project's `kicad.toml`; a key in both is an error, so there is one answer).
7. Export reads symbols and footprints from the installed KiCad libraries. What was built of 6 and 7 is ADR 0021, which ADR 0097 supersedes where it requires an installed KiCad; the code is in `crates/netsmith-kicad` (`mapping.rs`, `symbols.rs`, `install.rs`).
8. A mismatch between the part, the symbol and the footprint (a pad missing on either side, a pin name or an electrical type that does not suit the kind) is an error naming both sides, at export and in a test over every row of `std/kicad.toml` when KiCad is installed (`crates/netsmith-pcb/src/validate.rs`).
9. A part states the datasheet's envelope (`.vdd.current &= 1mA..25mA`, `.vdd.voltage <= 3.6V`) with the page in a comment; a design narrows it (ADR 0014).
10. A design says "this exact part" by narrowing the catalog's key (`.mpn &= "..."`); a catalog may be keyed (`catalog mcu by .mpn`), and an unset key lists the candidates.

The user also decided: the same pin in two packages has one kind, one name and one `optional` or `required` (the last is a deviation from the user's wording, because the checks run before the solver may know the package); a pin missing from the chosen package does not exist there, so a `tie` that names it is skipped and a board that connects it gets an error naming the packages that have it; a catalog row whose package lacks a pin the board connects is not a candidate; the pad is not a field (`.pad &= "1"` is gone), so there is one way to give a pad.

## What was rejected

- Option A: a built-in type for each pin (`Input`, `NoConnect`) with pads as `.pad` fields or catalog columns. A part in several packages goes back to one catalog column for each pin and pad, and a second way to give a pad stays.
- Option C: pins declared by type in the part, with the pads in a new top-level `package "X" of Part { ... }`. It splits the datasheet's pin table in two places, needs a list-valued assignment and a fifth kind of definition, and has no honest answer for a pin a package lacks.

The pin table is the one with one way to give a pad and one table to check against the datasheet.

## Cost

The language gained a block, three words (`pins`, `tie`, `fn`) and a pin kind set. A part with tables for packages and no package yet is judged twice. Two power sources on one net are not checked (whether a power pin gives or takes is the sign of the rail's current, known only after the solve). A pin with no table at all, on a net, stops lowering. No thermal vias under an exposed pad, no pin-aware placement.

## Left open

Accepted as built by the owner, 2026-11-11.

- Connecting by function (`.u.uart1_tx`) and letting the tool assign the pin: ADR 0101 (accepted, built); issue #109 holds what is left.
- Whether the KiCad version in the manifest should be checked against the version in a library file's header: "A library written by a newer KiCad than the one `kicad-cli` is might not open." Not built; issue #6.
- Checking a symbol's `(alternate "TIM14_CH1" ...)` entries against a pin's functions: they use the vendor's peripheral numbering, which will not match a part's names in general. Not built; issue #5.
- Whether the router copes with a 0.65 mm pitch against the process minimum: "not tried"; issue #3.
