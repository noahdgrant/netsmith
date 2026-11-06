# Schematic metrics baseline (public boards)

What the schematic view draws today, in numbers, so a later change to the layout can be judged by them. Measured 2026-11-05 on `main` at 6800d74 plus the metrics of stage 1 of `schematic-view-architecture.md`; the layout engine is unchanged. Only boards in this repository are listed.

Reproduce with a debug build:

```sh
cargo run -p netsmith --example schematic_metrics -- examples/led.nsm
cargo run -p netsmith --example schematic_metrics -- examples/schematic_bench --violations
```

It draws each board as the editor does (the rails, must-connect pins and module groups the editor gives the view) and prints one line per sheet, then the rules the drawing breaks. `--plain` prints every number as `sheet.name = value`. The definitions are in the module documentation of `crates/netsmith-view/src/metrics.rs`; the rules are in `crates/netsmith-view/src/check.rs`.

Columns: bends are corners of a net; crossings are pairs of wires of different nets that cross square; length ratio is wire length over the half-perimeters of the nets' pins (1 is a straight run); satellite steps is how far, in 1.27 mm grid steps, a two-pin part stands from the nearest pin of another part on one of its nets; side is the share of pins of larger parts on the side a reader expects (input left, output right, supply top, ground bottom); size is the sheet's box with its title strip; aspect is width over height. Collinear overlaps and touching ends between nets are 0 on every board here, so they are not in the tables.

## Single-sheet examples

| board | parts | wires | bends | crossings | length ratio | satellite steps (mean / max) | side | size mm | aspect | wire over text |
|---|---|---|---|---|---|---|---|---|---|---|
| LedExample (`led.nsm`) | 3 | 13 | 4 | 0 | 1.71 | 19.0 / 26.0 | 1.00 | 39x60 | 0.66 | 0 |
| LedTile (`led_panel.nsm`) | 3 | 13 | 4 | 0 | 1.71 | 19.0 / 26.0 | 1.00 | 39x60 | 0.66 | 0 |
| LedBoard (`two_boards.nsm`) | 3 | 13 | 3 | 0 | 1.76 | 19.0 / 26.0 | 1.00 | 41x60 | 0.68 | 0 |
| IndicatorExample (`status_indicator.nsm`) | 5 | 12 | 5 | 1 | 1.07 | 16.0 / 28.0 | 1.00 | 66x48 | 1.37 | 0 |
| RegulatorExample (`linear_regulator.nsm`) | 5 | 28 | 12 | 1 | 1.58 | 12.0 / 12.0 | 0.33 | 114x53 | 2.14 | 1 |
| FourLayerExample (`four_layer.nsm`) | 5 | 28 | 12 | 1 | 1.58 | 12.0 / 12.0 | 0.33 | 114x53 | 2.14 | 1 |
| PowerBoard (`two_boards.nsm`) | 5 | 28 | 12 | 1 | 1.58 | 12.0 / 12.0 | 0.33 | 114x53 | 2.14 | 1 |
| OutlineAndPours (`outline_and_pours.nsm`) | 12 | 28 | 12 | 1 | 1.58 | 12.0 / 12.0 | 0.33 | 114x177 | 0.65 | 1 |
| Carrier (`carrier_and_cards.nsm`) | 4 | 48 | 25 | 29 | 1.44 | 12.0 / 12.0 | 1.00 | 146x61 | 2.40 | 1 |
| SensorCard (`carrier_and_cards.nsm`) | 2 | 28 | 20 | 9 | 1.55 | 0.0 / 0.0 | 1.00 | 77x55 | 1.42 | 0 |
| SerialCard (`carrier_and_cards.nsm`) | 2 | 22 | 14 | 3 | 1.53 | 0.0 / 0.0 | 1.00 | 80x52 | 1.54 | 0 |

`OutlineAndPours` draws the regulator's wires and seven more parts that are on no net, which is why its sheet is taller. The parts of `carrier_and_cards.nsm` have no KiCad symbol, so they are boxes.

## Boards drawn by module

`examples/schematic_bench` (46 parts, 7 sheets):

| sheet | parts | wires | bends | crossings | length ratio | satellite steps (mean / max) | side | size mm | aspect | wire over text |
|---|---|---|---|---|---|---|---|---|---|---|
| adc | 5 | 23 | 11 | 3 | 1.42 | 22.2 / 43.0 | 1.00 | 67x88 | 0.77 | 0 |
| buck | 10 | 57 | 20 | 5 | 1.35 | 40.7 / 90.0 | 0.40 | 173x84 | 2.06 | 0 |
| ldo | 3 | 29 | 5 | 0 | 2.11 | 10.0 / 10.0 | 0.38 | 81x65 | 1.25 | 0 |
| mcu | 4 | 88 | 13 | 12 | 2.81 | 13.3 / 20.0 | 0.00 | 109x188 | 0.58 | 0 |
| modem | 22 | 79 | 33 | 19 | 3.12 | 46.7 / 100.0 | 1.00 | 216x128 | 1.68 | 0 |
| obd | 1 | 11 | 5 | 1 | 11.00 | 0.0 / 0.0 | 1.00 | 46x57 | 0.80 | 0 |
| ten | 1 | 20 | 0 | 0 | 0.00 | 0.0 / 0.0 | 1.00 | 46x47 | 0.97 | 0 |
| total | 46 | 307 | 87 | 40 | 2.28 | 38.1 / 100.0 | 0.23 | 347x358 | 0.97 | 0 |

`examples/enclosure_sensor` (43 parts, 6 sheets):

| sheet | parts | wires | bends | crossings | length ratio | satellite steps (mean / max) | side | size mm | aspect | wire over text |
|---|---|---|---|---|---|---|---|---|---|---|
| ldo | 3 | 20 | 5 | 0 | 2.03 | 12.0 / 12.0 | 0.33 | 61x60 | 1.02 | 0 |
| lights | 6 | 19 | 2 | 0 | 1.35 | 0.0 / 0.0 | 1.00 | 74x61 | 1.21 | 0 |
| pads | 12 | 24 | 12 | 42 | 0.00 | 0.0 / 0.0 | 1.00 | 310x62 | 4.98 | 0 |
| pullups | 2 | 10 | 3 | 1 | 1.77 | 0.0 / 0.0 | 1.00 | 46x51 | 0.91 | 0 |
| radio | 3 | 16 | 6 | 0 | 1.81 | 12.0 / 12.0 | 1.00 | 84x62 | 1.35 | 0 |
| EnclosureSensor | 17 | 84 | 65 | 115 | 2.38 | 16.8 / 26.0 | 0.12 | 291x188 | 1.55 | 0 |
| total | 43 | 173 | 93 | 158 | 2.15 | 15.0 / 26.0 | 0.18 | 387x337 | 1.15 | 0 |

Things the numbers show:

- A sheet where every net has one pin has length ratio 0, because there is no pair of pins to measure (`pads`, `ten`). Each pin is wired to a flag instead, and the crossings are what costs there (`pads` has 42 in a 310 mm strip).
- `EnclosureSensor` (the parts in no module) has 115 crossings and 65 bends for 17 parts.
- The side share is low where the engine turns a part so that its inputs end on the right (the `buck` sheet), and on the generated box of the microcontroller, which puts power pins on the left.

## Rules the drawings break

(Stage 2 fixed all of what follows; the numbers after the fix are in the last section.)

The rebuild of the netlist from the drawing (no short, no open, no extra pin) passes on every board in the tests (`crates/netsmith-view/tests/invariants.rs`) and on every example above (`crates/netsmith/tests/schematic_invariants.rs`, which skips without KiCad). So does "no wire along another net's wire". Two findings are recorded as ignored tests, not fixed:

1. A wire is laid through the plates of a capacitor. Smallest case: `supplies(1)` of the view tests drawn with no rails told (a vertical run of `sense1` one grid step beside `CC1`'s pin line). Also on the invented bench board (`M_CTS` through `C20` with rails told; `GND` through `C4`, `C5`, `C25` and `+1V8` through `C8` without). Tests: `no_wire_goes_through_a_symbol`, `the_smallest_case_of_a_wire_through_a_symbol`.
2. `examples/enclosure_sensor`, board `EnclosureSensor`: pin 13 of the 16-pin connector `J2` leaves upward, and its wire then runs straight down through the connector's body to a flag about 140 mm below. Test: `no_example_has_a_wire_through_a_symbol`.

## Stability under a small edit

Measured by `netsmith_view::stability` (`cargo run -p netsmith --example schematic_metrics -- <project> --stability`, or `cargo test -p netsmith-view --test stability -- --nocapture` on the test boards). It applies eight small edits to every sheet, draws before and after, and compares the sheet: (a) rename a signal net, (h) rename a supply rail, (b) add a decoupling capacitor beside a supply pin of the sheet's largest part, (c) add a pull-up on a signal pin of it, (d) delete a small part, (e) change a value (to one of the same length), (f) add a part to another module, (g) add a part on no net. A new part is inserted in the list after the part it is beside, not at the end, as an edit of the source would.

*Anchor frame*: both drawings are translated so that the same reference part (the largest part present in both) is at the origin, so a drawing that grew on its low side is not "moved" by that. *Raw frame*: the coordinates the layout has. Columns of the summary: the mean and the least, over the sheets of 8 or more parts whose row count did not change, of the share of parts present in both that did not move (to 0.01 mm) and of the wire segments of the old drawing that are, point for point, in the new one. There are no environment switches: a drawing depends on the design alone. The "before" numbers below were measured with seven debug switches, one for each change that keeps the drawing steady (canonical order, the origin at the reference part, the cut into rows, the slots of the widths, the lead of a row, the outward layout, the small parts that do not vote) set together for the engine as it was, at commit dcb45e2, the head of the pull request that made the engine steady. The switches, and the code that existed only for their off side, were removed afterwards, and the numbers stay here as constants. To measure them again, build `main` at 1eac15f (the engine as it was) and run the same commands.

Before (the engine as on `main` at 1eac15f) and after, on the test boards of `crates/netsmith-view/tests/common/corpus.rs` (LedExample, RegulatorExample, Supplies3, UartSheets, Mcu30; five sheets of ten parts or fewer, 8+ parts: the three supplies, the two UartSheets sheets):

| edit | anchor parts before | after | anchor segments before | after | rows changed before / after |
|---|---|---|---|---|---|
| (a) rename a net | 100% | 100% | 100% | 100% | 0 / 0 |
| (b) add a decoupling capacitor | 26% (least 11%) | 92% (least 60%) | 74% (53%) | 91% (70%) | 1 / 0 |
| (c) add a pull-up | 94% (90%) | 80% (50%) | 91% (88%) | 77% (54%) | 0 / 0 |
| (d) delete a part | 65% (62%) | 65% (62%) | 79% (71%) | 79% (73%) | 1 / 0 |
| (e) change a value | 100% | 100% | 100% | 100% | 0 / 0 |
| (f) add a part to another module | 100% | 100% | 100% | 100% | 0 / 0 |
| (g) add a part on no net | 100% | 100% | 99% | 99% | 1 / 0 |
| (h) rename a supply rail | 100% | 100% | 100% | 100% | 0 / 0 |

In the raw frame (the coordinates of the board, where the sheets are set in a grid) (f) moves a sheet whenever an earlier sheet of the grid grows or the grid is cut into other rows (before 40%, after 80% of parts unmoved on the corpus; 50% on `schematic_bench` and `enclosure_sensor`, before and after): the page shows one sheet at a time, in its own frame, where nothing differs.

`examples/schematic_bench` (sheets `buck` and `modem`, 10 and 22 parts) and `examples/enclosure_sensor`, anchor frame, parts unmoved / segments kept (mean over the sheets that are measured, rows unchanged):

| edit | bench before | bench after | enclosure_sensor before | after |
|---|---|---|---|---|
| (a) rename a net | 100% / 100% | 100% / 100% | - | - |
| (b) add a decoupling capacitor | 84% / 84% (2 sheets) | 90% / 95% (1 sheet; the other changed its rows) | 72% / 74% | 72% / 74% |
| (c) add a pull-up | 30% / 42% | 30% / 42% | - | - |
| (d) delete a part | 50% / 50% | 50% / 50% | 68% / 35% | 40% / 25% (one more sheet counted) |
| (e) change a value | 100% / 100% | 100% / 100% | 100% / 100% | 100% / 100% |
| (f) add a part to another module | 100% / 100% | 100% / 100% | 100% / 100% | 100% / 100% |
| (g) add a part on no net | 100% / 100% | 100% / 100% (1 sheet changed its rows) | 87% / 88% | 88% / 88% |
| (h) rename a supply rail | 100% / 100% | 100% / 100% | 100% / 100% | 100% / 100% |

The single-sheet examples (`led.nsm` ... `outline_and_pours.nsm`, `carrier_and_cards.nsm`): (a), (e), (f), (g), (h) are 100% before and after except `OutlineAndPours` (g) (50% before, 100% after) and `Carrier` (g) (25% before, 100% after); a capacitor beside a pin (b) leaves 80% of the five parts of the regulator examples unmoved before and after; on the card boards it is 50% before and after for the two cards, and for `Carrier` 75% before and 25% after (the sheet is cut into two rows now, and the capacitor moves the cut).

### What was found

| suspect | verdict | evidence |
|---|---|---|
| the sheet's coordinates start at the corner of the drawing | **confirmed**, fixed (`layout/origin.rs`) | a part added above or left of the first piece moved every coordinate: raw 0% unmoved on sheets where the anchor frame had 40% |
| ties by index (`sort_by_key(|i| *i)`, `BTreeMap<usize,..>`, the order of the walk) | **confirmed**, fixed (`layout/canon.rs`) | the drawing of a design listed the other way round differed (a board's own notes went to the sheet of part 0); now identical (`a_drawing_does_not_depend_on_the_order_of_parts_and_nets`) |
| a net's name as a tie | **rejected as a cause, avoided**: nets are ordered by their first pin, because ordering them by name made a rename move parts (a rail rename kept 94% of the segments, 75% on one sheet, while that was tried) | |
| the row count (nearest 1.4:1 over many cuts, depending on the total) flips and re-flows the sheet | **confirmed**, reduced (`engine::pack`) | adding one part on no net re-flowed a sheet of 25 parts into other rows (25% unmoved); the cut now depends on a width in coarse steps and on the pieces' widths in steps of 12.7 mm: (g) 100% on the corpus |
| the first piece of a row starts at the left edge, so what a block grows to its left moves its main part | **confirmed**, fixed (the row is laid out outward from the reference part) | a pull-up left of `J1` moved the other 24 parts |
| a small part that serves a pin turns the main part beside it (it votes in the orientation and in the direction of the signal) | **confirmed**, fixed (`Ctx::leaf`) | the IC of the three supply sheets mirrored when a capacitor was added: 0% unmoved, 99 to 147 mm; now 92% (least 60%) |
| global quantities (thresholds relative to the sheet) | **rejected**: the thresholds of the wired-or-named rule are absolute (48 grid steps, 30 for a fan-out) | `policy.rs` |
| the router (net order, ties) | **rejected** as a cause on these edits; it follows the order of nets, which is canonical now | the wires of a net that did not change are the same wherever its pins did not move |
| `Tuning` flips on a tiny edit | **not seen**: a sheet that does not cross is drawn once; one that crosses picks among a few drawings by crossings and wire length | |

Left as they are, and why: a part that needs room makes it (the capacitors of a row beside a pin stand at one pitch, so one more shifts the ones after it; a block that grows moves the pieces beyond it in its row); a pull-up between a signal and a rail changes the hops from ground of the signal's net, which turns the part it is on (the least of (c) on the corpus: 50%, and 0% on one sheet of a real board); a regulator that loses its output capacitor is no longer found as a spine (`spine_of` requires a capacitor on the output node), so deleting that capacitor draws the sheet without a row; the sheets of a board are set in a grid by their sizes, so the raw frame of a sheet moves when an earlier sheet grows.

### Drawing quality, before and after (every public board)

`--plain` before and after, per sheet; every rule of `check` is 0 on every sheet before and after. Crossings, bends and wire length are the same on every sheet of the public boards except:

| sheet | crossings | bends | wire mm | aspect |
|---|---|---|---|---|
| `carrier_and_cards.nsm` Carrier | 29 -> 29 | 13 -> 16 | 525 -> 425 | 1.60 -> 0.88 |
| `enclosure_sensor` radio | = | = | 157 -> 155 | 1.18 -> 1.21 |
| `enclosure_sensor` EnclosureSensor | = | = | = | 1.40 -> 1.69 |
| `outline_and_pours.nsm` OutlineAndPours | = | = | = | 1.34 -> 1.74 |

(The aspect of the single-sheet examples moves by 0.02 to 0.03 where the box grew by a grid step.) Carrier has three bends more and a hundred millimetres less wire in a taller sheet; OutlineAndPours and EnclosureSensor are wider, inside the 1.4 to 1.8 the rows aim for.

## After stage 2 (the three drawing defects fixed)

Measured 2026-11-05 with the same commands, after the router's fixes. Rules broken, from `--violations`: **0 on every sheet of every board here**, where the baseline had one on `EnclosureSensor` and the findings above on the bench board drawn without rails. Every number of every other public board is identical to the tables above (checked by running `--plain` before and after on all of them); only `EnclosureSensor` changed:

| sheet | parts | wires | bends | crossings | length ratio | satellite steps (mean / max) | side | size mm | aspect | wire over text |
|---|---|---|---|---|---|---|---|---|---|---|
| EnclosureSensor | 17 | 84 | 67 | 112 | 2.38 | 16.8 / 26.0 | 0.12 | 291x188 | 1.55 | 0 |
| total | 43 | 173 | 95 | 155 | 2.15 | 15.0 / 26.0 | 0.18 | 387x337 | 1.15 | 0 |

The wire of pin 13 of `J2` now goes round the connector to its flag (two bends more, three crossings fewer), and `wire through body` is 0 where it was 1.

| board | rules broken before | after |
|---|---|---|
| LedExample, LedTile, LedBoard, PowerBoard, IndicatorExample, RegulatorExample, FourLayerExample, OutlineAndPours, Carrier, SensorCard, SerialCard | 0 | 0 |
| SchematicBench (7 sheets) | 0 | 0 |
| EnclosureSensor (6 sheets) | 1 (wire through body) | 0 |

The tests of `crates/netsmith-view/tests/invariants.rs` run the same rules on the invented boards, with and without rails told (the supplies of three and seven modules, the bench board, the microcontroller of thirty pins): the capacitor finding is 0 there too.

## After the spine (regulator sheets)

Measured 2026-11-05 with the same commands, `view/regulator-spine` against c26001d, the commit before it. The power path through a regulator is drawn as one row (`GLOSSARY.md`, _Spine_; `crates/netsmith-view/src/layout/spine.rs`), and two metrics are new: `spine_bends` and `detached_chain` (target 0 both; the "before" numbers are of c26001d, which drew no spine). There is no environment switch to draw without the spine (one did, to compare, and is removed like every other switch of the drawing): to measure again, build c26001d. Rules broken, from `--violations`: 0 on every sheet of every board here. Crossings change on one sheet (`buck`) and fall there; every number of every other public board is identical except on the sheets below.

| sheet | crossings | bends | wire mm | rail span | satellite wire | size mm |
|---|---|---|---|---|---|---|
| RegulatorExample, FourLayerExample, OutlineAndPours (the same regulator) | 1 -> 1 | 9 -> 8 | 364 -> 279 | 68 -> 48 | 4 -> 9 | 116x58 -> 103x46 |
| SchematicBench `buck` | 4 -> 1 | 17 -> 11 | 364 -> 319 | 19 -> 20 | 72 -> 54 | 159x81 -> 144x74 |
| SchematicBench `ldo` | 0 -> 0 | 3 -> 4 | 95 -> 77 | 19 -> 7 | 4 -> 4 | 81x66 -> 74x52 |
| EnclosureSensor `ldo` | 0 -> 0 | 2 -> 0 | 67 -> 48 | 14 -> 8 | 4 -> 4 | 64x60 -> 56x46 |

The satellite wire of the regulator example grows from 4 to 9 steps: its capacitors hang from the row two steps under it and the wire runs along the row to the pin, where they stood beside the pin each with a symbol of its own. The `ldo` of the bench has one bend more for the same reason (the enable pin joins the input row).
