# Moving a project to a newer netsmith

netsmith keeps one spelling for each thing (ADR 0001). When a release changes a spelling, the old one is an error at once, with no release in which both work. A project that built on an earlier release can then stop with many errors. Each error names the spelling to write, so the errors themselves are the list of what to change; this file says why there are so many and what each change is, release by release, with the old spelling and the new one side by side.

How to move a project:

1. Run `netsmith board check` in the project with the new netsmith. A project whose `netsmith.toml` names an older release (`netsmith = "0.1.0"`), or names none, and whose check fails, ends the run with a note that points here.
2. Read the section of each release after the project's own, oldest first, and fix what the errors name. Most changes are mechanical: the errors' line numbers find every place.
3. Run `netsmith project lock --check`: a release may pick other parts for the same source (a new check, a new catalog row). `netsmith project lock --update` takes the new picks; review the lock's diff. A release also adds rows to the built-in catalogs (its notes list them): when a project's own catalog file already has one of those parts, the build stops with "a part is in a catalog once", naming the built-in row and what differs from the project's. Remove the project's row from its catalog file; a pick lock that names that row (`row = "..."`) is then repaired by `netsmith project lock --update`, which sets it to the built-in row of the same LCSC number and changes no other row.
4. When the project builds, set `netsmith = "<release>"` in the `[project]` table of `netsmith.toml`. `netsmith project refresh` brings the project's guide and skills up to the release (ADR 0131); it does not touch the sources.

Every change below that makes source an error was a pull request whose title has `!` (a breaking change in Conventional Commits), and the release notes on GitHub list those commits. A change that will make source an error adds its entry under _Unreleased_ in the same pull request (`AGENTS.md`, ADR 0134).

## Unreleased

## 0.7.0: changes since 0.6.0

### A provider or a use does not declare a member std now declares (issues #1258, #1324, #1405)

The same break as 0.6.0's below, for members added since that the entries after this one do not cover. Delete the declaration and keep the narrowing:

| Where it is declared | Members it gained | Pull request |
|---|---|---|
| `std::parts::regulator::LinearRegulator`, `BuckIc`, `std::parts::switch::LoadSwitch` | `.reverse`, a `std::sources::Reverse`: how current flows backwards through the part when its output is held up and its input is not (a body diode by default, or `ReversePath::Blocking`), which a board's `scenario` reads | #1369 |
| `std::parts::connector::Mating` | `.body_height`, how far above its board a connector's body reaches where that is not its seat | #1395 |
| `std::mfg::stackup::Stackup` | `.loss_tangent`, the build-up's dissipation factor | #1420 |

```text
error: `reverse` is declared by `LoadSwitch`, which this part provides; do not declare it again: state its values with statements, `.reverse.<field> &= ...`
error: `body_height` is declared by `Mating`; do not declare it again in a use of it: narrow it with `.body_height &= ...`, or state it with a relation
error: `loss_tangent` is declared by `Stackup`; do not declare it again in a use of it: narrow it with `.loss_tangent &= ...`, or state it with a relation
```

A `.reverse` of your own that was not a `Reverse` is renamed; the generic's is a block, narrowed member by member (`.reverse.path &= ReversePath::Blocking`). `.process.stackup.loss_tangent` (with a `TrackPath`'s `.frequency`) is what `.budget_loss` is judged at; no build-up of the standard library states one, so a board that states neither builds as before. `TrackPath` itself is new in 0.7.0 (#1409), so nothing written for 0.6.0 declares its members.

### `LoadSwitch` gains `.needs` (issue #1402)

`std::parts::switch::LoadSwitch` declares `.needs: String optional`, what a board needs of a load switch's order code, so that a board that uses the generic reaches it (`.sw: LoadSwitch`, then `.sw.needs &= QuickDischarge`) and a provider with a `.variants` table fills it. A provider of yours that declared a member `.needs` for itself stops at the declaration (#1416):

```text
error: `needs` is declared by `LoadSwitch`, which this part provides; do not declare it again: narrow it with `.needs &= ...`, or state it with a relation
```

Delete the declaration. An IC sold as several order codes lists them in a `.variants` table (guide chapter 10, _Order codes and their features_). There is no ADR: the owner decided it on the issue. A part of your own that provides no generic may still declare `.needs` until it states a `.variants` table.

### `Resistor`, `Capacitor` and `Inductor` gain `.fitted` and `.tune`; `Bool`, `true` and `false` are known everywhere (issue #1273)

A part that provides `std::parts::passive::Resistor`, `Capacitor` or `Inductor`, or a use of one whose block declared `.fitted` or `.tune` for itself (`.r: Resistor { .fitted: String }`), stops at the declaration (#1408):

```text
error: `tune` is declared by `Capacitor`, which this part provides; do not declare it again: narrow it with `.tune &= ...`, or state it with a relation
error: `fitted` is declared by `Resistor`; do not declare it again in a use of it: narrow it with `.fitted &= ...`, or state it with a relation
```

Delete the declaration. `.fitted` is a `Bool` (`.fitted &= false` marks a position that is not fitted until it is tuned on the bench), and `.tune` is the value's own quantity (`.tune &= 1pF..10pF`). `true` and `false` are written bare; `Bool::true` is refused. An `enum Bool` of your own still builds.

There is no ADR: the owner decided it on the issue.

### A provider does not declare `.assembly_size_source` (issue #1393)

`std::mfg::jlcpcb::TwoLayer`, `FourLayer`, `SixLayer`, `HdiFourLayer`, `HdiSixLayer`, `HdiEightLayer` and `std::mfg::acme::EightLayer`, `TenLayer` declare `.assembly_size_source`, the page, service and day of their assembly size figures, which the `assembly_size` finding quotes (#1398). A provider or a use of yours that declared a member of that name (it is unlikely) deletes the declaration and keeps the narrowing (ADR 0134):

```text
error: `assembly_size_source` is declared by `TwoLayer`, which this interface provides; do not declare it again: narrow it with `.assembly_size_source &= ...`, or state it with a relation
```

### A net with a `TrackImpedance` is routed on one layer with no via (issue #1274)

A net of a `TrackImpedance` is now an RF net: the router routes it first, on the outer layer all its pads are on, with no via (#1386). A board whose RF line could only cross other copper through a via now leaves it unrouted, and `fab` stops with `N connections could not be routed`; copper of the routes file that gives such a net a via is a finding of the rule `constraint`:

```text
error [constraint] constraint rf_line route one_layer (an RF net: its impedance is stated; `route any_layer` lets it change layers) is violated: the net `RF` has tracks on B.Cu, F.Cu and 1 via, ...
```

Old: nothing written, and the line changed layers where the router chose. New: give the line room on one layer, or let it change layers with a line in the board's physical file that names a pin on the net:

```nsm-fragment
.rf.p1 route any_layer
```

There is no ADR: the owner decided it on the issue.

### What changed with no error

The new statements (`alternate`, `direction`, `scenario`, `.variants`, and in the physical file `keep ... between`, `soft margin`, `within` and `route`) and the members every part or pin now has (`.underside_keepout`, a pin's `.pour_connection` and `.spoke_width`, a pad's `.contact_offset`) are read only where they are written, so a part of your own that declares a member of one of those names still builds. These change what `board check`, `board drc`, `board fab` and `import` report or write without an error in the source:

- **Heights.** Every row of the built-in resistor, capacitor and LED catalogs has a height, its maker series' greatest (#1421), so a chip part that was `height_unknown` is measured: under a lid, a case or another part's `.underside_keepout` (#1407) it can now be `too tall`, and its height reaches the 3D view and the design file.
- **`board drc`.** An item that only touches a rule area's edge, or sits in a hole of the area, is no longer in it, as in KiCad's check, so `items_not_allowed` flags fewer pads of a footprint's own keepout; each such finding ends with what of the area flags it, `(no tracks)`, `(no vias)`, `(no pads)` or `(no footprints)` (#1378, #1396). A footprint's keepout drawn with arcs is read and written moved with the footprint (#1403). `track_current` and `via_current` name the width or vias the copper has, the width or vias needed, the current and where it comes from, and `--format json` carries them as fields (#1381).
- **The `assembly_size` finding** quotes where the board house's least size comes from and spells out a panel, and its JSON carries `source` (#1398).
- **Bill of materials.** A passive with `.fitted &= false` and each `alternate` of a member are listed not fitted (DNP) in `fab-notes.txt`; `fab` writes `bom-alternates.csv` beside `bom.csv` when a board has an alternate, with the designator each replaces (#1408, #1413, #1428).
- **A routes file record whose pads moved** keeps all its copper; the warning names each end that now runs into nothing of its net, and the router routes the connections left open. `routes lock` is refused only for an error in that net's own copper (#1385).
- **Systems.** Each board of a system has its own process (#1404); a mated connector's body is measured up to its seat (#1395); a board keeps its tall parts out from under the board mated on it (#1390); and logic levels, an open-drain pull-up, a driver behind a series part and the DC analysis are read across a mate (#1425, #1434, #1437, #1442, #1447). A system that built may get a finding it did not have, or lose one.
- **New checks.** A load's burst reflected to the input of the converter that feeds it (#1412), a host's pin budget in `board explain` (#1380), and a contradiction every provider has said at the narrowing rather than at the use (#1419).
- **Land patterns.** The tapped supercapacitor and a coin cell with tabs use netsmith's own footprints and export with no KiCad install (#1417).
- **`netsmith import footprint`** is new: it takes a land pattern from a KiCad, Eagle or Altium file or rebuilds one from a reference board's Gerber files (#1435, #1439, #1443, #1445, #1446).

## 0.6.0: changes since 0.5.0

### A provider, a use or a part does not declare a member the language or std now declares (issue #1371)

The same break as 0.5.0's below, for the members added since. Delete the declaration and keep the narrowing:

| Where it is declared | Members it gained | Pull request |
|---|---|---|
| every part and every board (built in) | `.routing_margin`, the room for routing kept round a part's courtyard | #1356 |
| `std::mfg::jlcpcb::TwoLayer`, `FourLayer`, `SixLayer`, `HdiFourLayer`, `HdiSixLayer`, `HdiEightLayer`, `std::mfg::acme::EightLayer`, `TenLayer` | `.min_single_sided_assembly_size`, `.min_double_sided_assembly_size` | #1358 |
| `std::sources::Rail` | `.voltage_floor`, `.charge_time_limit` | #1346, #1367 |

```text
error: `routing_margin` is already declared in this block: every part has it; do not declare it: narrow it with `.routing_margin &= ...`, or state it with a relation
error: `min_single_sided_assembly_size` is declared by `TwoLayer`, which this interface provides; do not declare it again: narrow it with `.min_single_sided_assembly_size &= ...`, or state it with a relation
error: `voltage_floor` is declared by `Rail`; do not declare it again in a use of it: narrow it with `.voltage_floor &= ...`, or state it with a relation
```

## 0.5.0: changes since 0.4.0

The pull requests below did not have `!` in their titles, so the release notes of 0.5.0 do not list them as breaking (issue #1371).

### A provider does not declare a member its standard generic now declares (issue #1371)

```text
error: `enable_threshold` is declared by `BuckIc`, which this part provides; do not declare it again: narrow it with `.enable_threshold &= ...`, or state it with a relation
  --> src/buck.nsm:12:5
   |
12 |     .enable_threshold: Voltage
   |     ^^^^^^^^^^^^^^^^^
```

A part that provides a generic has every member of the generic and does not declare it again (the guide's chapter 14, _A provider_). 0.5.0 added members to some standard generics, so a part of your own that provides one and declared the same name for itself on 0.4.0 stops at the declaration. Delete the declaration and keep the narrowing (`.enable_threshold &= 1.2V`) and the relations: they now state the generic's member, which the checks read.

| Generic | Members it gained | Pull request |
|---|---|---|
| `std::parts::regulator::LinearRegulator` | `.output_current`, `.enable_delay`, `.soft_start_time`, `.enable_threshold` | #1302, #1350 |
| `std::parts::regulator::BuckIc` | `.high_side_resistance`, `.enable_delay`, `.enable_threshold` | #1302, #1350 |
| `std::parts::passive::Capacitor`, `CapacitorBank` | `.capacitance_at_end_of_life`, `.esr_growth` | #1303 |

| Before | Now |
|---|---|
| `.enable_threshold: Voltage` and `.enable_threshold &= 1.2V` in a `part X provides BuckIc` | `.enable_threshold &= 1.2V` |
| `.output_current: Current` in a `part X provides LinearRegulator` | nothing: the generic states `.output_current == -.vout.current`, the current the regulator gives its load |

Each member keeps the generic's meaning, so read its doc comment in `std/` before you keep a narrowing of your own: `LinearRegulator`'s `.output_current` is the current its output supplies (`-.vout.current`), not the most the part gives (that is `.max_current`); `.enable_delay` is the time from the enable to the start of the output's rise and `.soft_start_time` the length of the rise, which a board's `sequence` reads (the guide's chapter 28, _The power-up order of a board_); `.high_side_resistance` is the resistance of a buck IC's high-side switch held on, which with the inductor's DC resistance gives the buck's dropout; `.capacitance_at_end_of_life` and `.esr_growth` are what a board's corner at `Life::End` applies to the capacitance and the ESR. A member of your own that meant something else is renamed.

### A part does not declare `.height_growth` (issue #1263)

```text
error: `height_growth` is already declared in this block: every part has it; do not declare it: narrow it with `.height_growth &= ...`, or state it with a relation
```

Every part has `.height_growth`, how much its body may grow in height over its life, and the height check adds it to the part's height (#1288). A part of your own that declared `.height_growth: Length` deletes the declaration and keeps its narrowing (0.5.0 itself says only the first half of the message).

| Before | Now |
|---|---|
| `.height_growth: Length` and `.height_growth &= 0.5mm` in a part | `.height_growth &= 0.5mm` |

### A use does not declare a member its standard interface now declares (issue #1371)

```text
error: `burst` is declared by `Rail`; do not declare it again in a use of it: narrow it with `.burst &= ...`, or state it with a relation
```

A use's block may declare a member of its own (`.load: Rail { .on_time: Time }`), and that member is refused once the interface declares the same name. 0.5.0 itself says "`burst` is already declared in this block". Delete the declaration, keep the narrowing, and check that the interface's meaning is the one you meant (its doc comment in `std/`); a member of your own that meant something else is renamed.

| Interface | Members it gained | Pull request |
|---|---|---|
| `std::sources::Rail` | `.burst` (how long a peak lasts), `.duty` (the part of the time the peak is drawn) | #1341 |
| `std::sources::DiffPair`, `PairImpedance`, `std::bus::Usb2`, `Can` | `.skew` (the most the pair's two lengths may differ) | #1253 |

| Before | Now |
|---|---|
| `.load: Rail { .burst: Time; .burst &= 2ms }` | `.load: Rail { .burst &= 2ms }` |
| `.usb: Usb2 { .skew: Length; .skew &= 0mm..0.15mm }` | `.usb: Usb2 { .skew &= 0mm..0.15mm }` |

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

The rule is general, and only `Attiny13a` is changed for you: any part that gave its I/O pins one table of levels as `Logic` lines (`.pb0: Logic`, `.pb0.vih &= ...`, `.pb0.line` in `pins`) can state the table once as `.levels` and make the pins plain. A project's own part that does so changes the same way for its users:

| Before | Now |
|---|---|
| `.pin.line` in the part's `pins`, `.pin: Logic` with the same `.vih`, `.vil`, `.voh`, `.vol` and `.reference` on each pin | the pin plain in `pins`; `.levels: Levels` with `.levels.reference`, `.vih`, `.vil`, `.voh`, `.vol` stated once |
| `.u.pin.line ~ ...`, `.u.pin.voh`, `.u.pin.vih` in a board | `.u.pin ~ ...`, `.u.levels.voh`, `.u.levels.vih` |

A pin with levels of its own (a reset input) stays a `Logic`: a pin of an interface has its interface's levels and never the part's. The `.levels` gives its levels to the plain pins whose row's kind is `input`, `output`, `bidirectional`, `tri_state` or `open_drain`; a `power`, `passive` or `no_connect` pin takes none. A part that keeps its `Logic` lines still builds, so a part of your own is moved when you choose to.

### What changed with no error

These change what `board drc` and `board fab` report, or what `fab` writes, without an error in the source. A project may see a finding it did not have, or lose one:

- **The rules file.** The `.kicad_dru` that `fab` writes states the pad-to-pad rule as a `physical_clearance` and the hole-to-track rules as `physical_hole_clearance`, which a clearance written on a pad or its footprint (KiCad's own libraries give one to fine-pitch pads) does not silence, as it did the `clearance` constraint before. KiCad's check now reports pads too close, and a track near the hole of a plated pad, where a pad's own clearance made the rule silent; netsmith's check reports the same (`pad_to_pad`, `hole_to_track`), and the cross-check counts KiCad's second finding of two touching pads as the short's (issues #1235, #1231). The guide's chapter 27 says for each rule whether KiCad reports the case where both items are of one footprint.
- **The pour's fill.** netsmith's fill makes the spokes and islands KiCad's refill makes: a spoke as wide as the pour's `min_width` is made (a narrower `spoke_width` or a smaller `thermal_gap` never starves more pads), an island is found through pieces of the net's zones that touch, and a zone of the net with a higher priority is taken out of the fill last. So `starved_thermal`, `isolated_copper` and `copper_sliver` can name other pads and pieces than on 0.3.1 (issue #1239).
- **`unjoinable_thermal`.** A pad that no spoke and no track joins to the fill (and no other fill of its net) is no longer a `starved_thermal` but `unjoinable_thermal`, a warning of netsmith's own with the reason for each spoke: another net's clearance by name, a thermal gap, the edge, a keepout, or a strip of fill under `min_width`. KiCad leaves such a pad to its connection check; the pad's own via or track, which the router keeps, joins it. A project that listed `starved_thermal` as ignored or as an error in its `.kicad_pro` `rule_severities` sets `unjoinable_thermal` the same way (issues #1239, #1292). `board fill` lists the pads, and the editor's 2D view rings them.
- **A footprint's keepout.** A keepout zone that a library's footprint draws is written to the `.kicad_pcb` in board coordinates, turned and moved with the footprint and mirrored on the back, as KiCad saves it, where it was in the footprint's own frame and KiCad's refill poured copper where the footprint forbids it; a panel moves it with the cell. netsmith reads the zone of a footprint in a `.kicad_pcb` as board coordinates too, so a board ported from KiCad keeps its keepout where KiCad has it (issues #1237, #1242).

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
