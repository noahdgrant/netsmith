# Moving a project to a newer netsmith

netsmith keeps one spelling for each thing (ADR 0001). When a release changes a spelling, the old one is an error at once, with no release in which both work. A project that built on an earlier release can then stop with many errors. Each error names the spelling to write, so the errors themselves are the list of what to change; this file says why there are so many and what each change is, release by release, with the old spelling and the new one side by side.

How to move a project:

1. Run `netsmith board check` in the project with the new netsmith. A project whose `netsmith.toml` names an older release (`netsmith = "0.1.0"`), or names none, and whose check fails, ends the run with a note that points here.
2. Read the section of each release after the project's own, oldest first, and fix what the errors name. Most changes are mechanical: the errors' line numbers find every place.
3. Run `netsmith project lock --check`: a release may pick other parts for the same source (a new check, a new catalog row). `netsmith project lock --update` takes the new picks; review the lock's diff. A release also adds rows to the built-in catalogs (its notes list them): when a project's own catalog file already has one of those parts, the build stops with "a part is in a catalog once", naming the built-in row and what differs from the project's. Remove the project's row from its catalog file; a pick lock that names that row (`row = "..."`) is then repaired by `netsmith project lock --update`, which sets it to the built-in row of the same LCSC number and changes no other row.
4. When the project builds, set `netsmith = "<release>"` in the `[project]` table of `netsmith.toml`. `netsmith project refresh` brings the project's guide and skills up to the release (ADR 0131); it does not touch the sources.

Every change below that makes source an error was a pull request whose title has `!` (a breaking change in Conventional Commits), and the release notes on GitHub list those commits. A change that will make source an error adds its entry under _Unreleased_ in the same pull request (`AGENTS.md`, ADR 0134).

## Unreleased

## 0.4.0: changes since 0.3.1

### The ATtiny13A's I/O pins are plain pins with the part's levels (issue #1190)

```text
error: `Pin` has no member `line`
   --> src/main.nsm:15:20
   |
15 |     .j.p1 ~ .u.pb0.line
   |                    ^^^^
```

A part can state one table of logic levels for its plain pins, as its `.levels` (the guide's chapter 28, "The levels of a part's plain pins"). `std::parts::mcu::Attiny13a` does: its pins `pb0` to `pb4` are plain pins now, not `Logic` lines, and `pb5`, the reset pin with levels of its own, is still a `Logic`.

| Before | Now |
|---|---|
| `.u.pb0.line ~ ...`, `.u.pb0.line.function &= GPIO` | `.u.pb0 ~ ...`, `.u.pb0.function &= GPIO` (the same for `pb1` to `pb4`) |
| `.u.pb0.voh &= 4.0V at 20mA` | `.u.levels.voh &= 4.0V at 20mA`, for all of `pb0` to `pb4` |
| `.u.pb5.line` | unchanged |

A message names the part's levels where it named a pin's: `u.levels.vih` for `u.pb2.vih`.

## 0.3.0: changes since 0.2.0

### The commands are grouped under nouns (issue #1090)

```text
error: `fab` is now `board fab`
  write: netsmith board fab src/main.nsm --out fab
  (the command line is grouped under nouns now; docs/MIGRATING.md lists every move)
```

Every command is `netsmith <noun> <verb>` now. An old command or flag is an error that names the new spelling and prints the whole line rewritten, so a script or a CI step is fixed by copying the line it prints. The language guide's commands chapter has the table of every command with its flags.

| Before | Now |
|---|---|
| `netsmith init` | `netsmith project init` |
| `netsmith init --refresh` | `netsmith project refresh` |
| `netsmith lock --update`, `--check` | `netsmith project lock --update`, `--check` |
| `netsmith fab --order [NAME]` | `netsmith project order [NAME]` |
| `netsmith check` | `netsmith board check` |
| `netsmith build` | `netsmith board build` |
| `netsmith fab` | `netsmith board fab` |
| `netsmith explain` | `netsmith board explain` |
| `netsmith compare` | `netsmith board compare` |
| `netsmith layout ... --into FILE.NSM` | `netsmith layout ... --file FILE.NSM` |
| `netsmith layout export --out FILE.KICAD_PCB` | `netsmith layout export --kicad-pcb FILE.KICAD_PCB` |
| `netsmith routes ... --into FILE.NSM` | `netsmith routes ... --file FILE.NSM` |
| `netsmith render ... --out FILE.PNG` | `netsmith view 3d ... --picture FILE.PNG` |
| `netsmith editor` | `netsmith view editor` |
| `netsmith view --json ...` | `netsmith view editor-3d --format json ...` |
| `netsmith dxf import` | `netsmith import dxf` |
| `netsmith kicad-libs import` | `netsmith import kicad-libs libraries` |
| `netsmith kicad-libs models` | `netsmith import kicad-libs models` |
| `netsmith part from-symbol` | `netsmith import part from-symbol` |
| `netsmith port skeleton --out FILE.NSM` | `netsmith import port skeleton --write FILE.NSM` |
| `netsmith port tables` | `netsmith import port tables` |
| `netsmith case import` | `netsmith import case model` |
| `netsmith case place` | `netsmith import case place` |
| `netsmith route` | `netsmith dev route` |
| `netsmith parity` | `netsmith dev parity` |
| `netsmith board3d --json` | `netsmith dev board3d --format json` |

The catalog commands (`netsmith catalog refresh`, `add`, `age`) and the verbs of `layout` and `routes` are as they were. No source file changes. A project's agent files name the old commands until `netsmith project refresh` rewrites them (a file you edited is listed with a diff, ADR 0131).

### What changed with no error

These change what `fab` makes or what the checks find, without an error in the source:

- **Routing.** The router has a further stage when connections are still open: rounds that rip up and re-route whole nets (`--rip-rounds`, `rip_rounds` in `netsmith.toml`; 0 turns them off, issue #1088). A board that routed fully is unchanged; one that did not may now route.
- **Pours.** A pour's thermal spokes follow KiCad's refill (a spoke never wider than its pad, none where not enough pour is left at its end), so `starved_thermal` names the pads KiCad's check does and a pad's own via stays where the pour would join it by fewer than two spokes (issue #1115). A pour also has `.clearance`, `.min_width` and `.pads` (issue #1084).
- **Vias in pads.** `fab-notes.txt` and `order.txt` say how many vias stand in pads; `netsmith project order` stops until the extra cost is accepted (`via_in_pad = "accepted"` in the order's table, or `--accept-via-in-pad`; `.process.via_in_pad &= 0mm` forbids them) (issue #1118).
- **Height check.** One `height_unknown` warning lists the parts of unknown height under a limit or under another part's body (issue #1117).
- **`fab --cross-check`** runs netsmith's own comparison of the Gerbers with the board model as well, and runs it alone where KiCad is not installed (it was an error then) (issue #1158).
- **The schematic.** `board fab` writes `<board>-schematic.pdf` into each package (`pdf = false` or `--no-pdf` leave it out) (issue #1091).
- **Layout.** `netsmith project fmt` is the layout of `.nsm` source; the repository's examples and `std/` are formatted (issue #1103). A project's own files need no change.

## 0.2.0: changes since 0.1.0

### A pin's function is a name from its part's pin table (ADR 0118)

```text
error: the function of `u.io.line` is a name from the pin table of its part, not text: write `UART_TX`, without quotes
```

| 0.1.0 | Now |
|---|---|
| `.u.pa2.function &= "UART1_TX"` | `.u.pa2.function &= UART1_TX` |

A misspelt function is an error with the closest name. Packages stay quoted (`.package &= "0402"`), and are now checked against the part's pin tables and its catalog.

### An LED's colour is a variant of `Color` (ADR 0118)

```text
error: `led.color` holds a variant of `Color`, but this value is text; a variant of `Color` is not written as text: write `Color::Green` (...)
```

| 0.1.0 | Now |
|---|---|
| `.led.color &= "Green"` | `import std::parts::passive::Color` at the top, then `.led.color &= Color::Green` |

A variant of any enum is written `Enum::Variant` where the enum is imported, or bare where the variant itself is imported (`import std::parts::passive::Color::Green`, or `import std::parts::passive::Color::{Red, Green}`).

### A pin's absolute limits are written in its pin row (ADR 0124, ADR 0126)

```text
error: a pin's absolute maximum is written in its pin row, not with `&=`: end the row of `.io.line` in the `pins` table of its part with `abs_max 3.6V`, ... (ADR 0126)
```

| 0.1.0 | Now |
|---|---|
| `.io.line.abs_max &= 3.6V` (or `?=`) as a statement of the part | `abs_max 3.6V` at the end of the row of `.io.line` in the part's `pins` table |
| `.io.line.abs_min &= -0.5V` | `abs_min -0.5V` in the row, before `abs_max` |

A row is `.io.line input "IO" "3" fn GPIO UART_TX abs_min -0.3V abs_max 3.6V`. The limit is measured from the ground of the pin's interface; `from .pin` after it names another pin. A limit in a generic's row binds every provider (docs/language/11, "Pins, packages and ties").

### `.mpn` and `.lcsc` are every part's, and a hand pick is a narrowing in the source (ADR 0122)

```text
error: `mpn` is a member every part has, as text: the number of the catalog row it is (ADR 0122); do not declare it, and narrow it with `.mpn &= "..."`
error: board.picks.toml: netsmith no longer reads a picks file (ADR 0122): a hand pick is a narrowing in the source. ...
```

| 0.1.0 | Now |
|---|---|
| `.mpn: String` or `.lcsc: String` declared in a part | delete the line; `catalog chip { .mpn, .package }` still lists the column |
| `<stem>.picks.toml` beside a source, `[Board] c5 = "C1234"` | `.c5.lcsc &= "C1234"` (or `.mpn &= "..."`) in the body of the board, then delete the file |

The error of a picks file lists the narrowings to write for each of its entries.

### A module's user narrows the parts inside it, and `PullUp` and `PullDown` lost `.resistance` (ADR 0116)

```text
error: `resistance` is not a member of this PullUp
```

| 0.1.0 | Now |
|---|---|
| `.pull: PullUp { .resistance &= 10kohm +/- 5% }` | `.pull: PullUp { .resistor.resistance &= 10kohm +/- 5% }` |
| the same in `PullDown` | the same, `.resistor.resistance` |

A board may now narrow any member inside a module by its path, at any depth, with `&=` (`.stage.divider.rtop.resistance &= 100kohm +/- 1%`); the members that only passed one value to one part inside are gone from std. A join, a relation or a default still stops at the module.

### A buck converter is `Buck<Ic>`, and `BuckStage` is gone (ADR 0115)

```text
error: module `std::parts::regulator` has no definition `BuckStage`
error: `Buck` takes a part for `Ic` (a part that provides `BuckIc`): name it, `Buck<...>`, or let the solver pick one by using a generic that `Buck` provides
```

| 0.1.0 | Now |
|---|---|
| `part Zbk2 { .vin: Rail; .buck: Buck { .vref &= ...; ... } ... }`, with pins `.buck.sw.pos` and `.buck.fb` | `part Zbk2 provides BuckIc { .vref &= ...; ... }`: the members of `Buck` are the part's own (`.sw`, `.fb`, `.vref`, `.efficiency`, ...), and so are its pins (`.sw.pos`, `.fb`) |
| `.u: Zbk2` and `.stage: BuckStage { .vout.voltage &= 1.8V +/- 3% }`, joined with `.stage.buck ~ .u.buck`, `.input ~ .u.vin` and `.input ~ .stage.vin` | `.reg: Buck<Zbk2> { .vout.voltage &= 1.8V +/- 3% }`, joined with `.input ~ .reg.vin` and `.reg.vout ~ .output` |
| `.u.en ~ .input.pos` on the board | `support { .en ~ .vin.pos }` in the part, or `.reg.ic.en ~ ...` on the board (ADR 0120, ADR 0130) |
| `.u: Lt8641` with a `BuckStage` | `.reg: Buck<Lt8641>`; the parts its datasheet asks for come with it, in its `support` section |

`.reg: BuckConverter` lets the solver pick the IC from every part that provides `BuckIc`. The guide's chapter 28, "Switching regulators", has the whole example.

### A capacitor has no `.count`: a bank is a `CapacitorBank` (ADR 0123)

```text
error: `count` is not a member of this Capacitor: a `Capacitor` is one part; for a count the solver chooses write a `CapacitorBank` (`.bulk: CapacitorBank { .count &= 1..8 }`), and for a number you know write `3 x Capacitor` (a group of identical members)
```

| 0.1.0 | Now |
|---|---|
| `.bulk: Capacitor { .count &= 1..8 }` and `.bulk.count * .bulk.effective_capacitance >= 60uF` | `.bulk: CapacitorBank` (import `std::parts::passive::CapacitorBank`) and `.bulk.total_effective_capacitance >= 60uF`; `.count` is chosen from 1 to 8 unless narrowed |
| a number you know, written out one by one | `.c: 3 x Capacitor { ... }`, the members `.c[0]`, `.c[1]` and `.c[2]` (ADR 0119); writing them out still works |

The capacitors of a bank have the paths `bulk[0]`, `bulk[1]` and so on.

### A layout file holds no keepout of its own (ADR 0048)

```text
error: ... is stated whole here (it gives `size`): a keepout of its own is a requirement, and it goes in the physical file (ADR 0048). Remove the entry and write `.k: Keepout { ... }` in `physical for Board { }` of `board.physical.nsm`
```

| 0.1.0 | Now |
|---|---|
| `[Board.keepouts.drawn1]` with `size`, `layers`, `tracks`, ... in `<stem>.pcb.toml` (a keepout an earlier editor drew) | the `Keepout` member the error writes out, in `physical for Board { ... }` of `<stem>.physical.nsm`; the layout file's entry, if any, gives only `at` |

### What changed with no error

These change what `fab` makes or what the checks find, without an error in the source:

- **Vias.** The via the router draws is the process's offer, 0.2 mm in 0.45 mm on JLCPCB (it was 0.15 mm in 0.51 mm). `.process.via_drill` and `.process.via_diameter` state another. A footprint that breaks a rule of JLCPCB's capabilities page now fails the design rules check.
- **Tracks** bend by 45 degrees only, and the design rules check refuses a right or acute corner.
- **Checks.** A chain from a logic pin, or between two rails, sets its current before the pick and is held to the pin's test current (ADR 0127); a board whose window is above it is now an error. The DC analysis may find a stated current or voltage inconsistent with the circuit around it.
- **Stackups.** A multilayer process names its build-up, `.process.stackup.build` or `.process.stackup.thickness`, and says what each inner layer is (`.process.inner1.net &= "GND"` for a plane, `.use &= "signal"` for a signal layer, ADR 0111); `Stackup`'s `prepreg` and `core` and the `Plane` interface are gone.
- **Picks.** New catalog rows and checks may move a pick: `netsmith project lock --check` says which. A new built-in row of a part a project's catalog has is an error ("a part is in a catalog once"): remove the project's row, then `netsmith project lock --update` sets the lock's row to the built-in one (issue #1119).
