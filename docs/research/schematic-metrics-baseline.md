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

Reported by `crates/netsmith-view/tests/stability.rs` (run with `-- --nocapture`); nothing is asserted. "Wire kept" is the share of the old drawing's wire, in grid steps, that lies on the new drawing's wire of the same net. "Moved" counts parts, other than the one edited, whose origin moved.

| board | edit | wire kept | parts moved | greatest move |
|---|---|---|---|---|
| bench | rename `+3V3` to `VDD33` | 86% | 10 of 54 | 1.3 mm |
| bench | add a 100n capacitor on `+3V3` and `GND` in the microcontroller's module | 88% | 5 of 54 | 12.7 mm |
| bench | delete `R9` | 97% | 0 of 53 | 0 |
| three supplies | rename `out2` to `vout2` | 100% | 0 of 30 | 0 |
| three supplies | add a capacitor on `out2` and `GND` in the second module | 97% | 0 of 30 | 0 |
| three supplies | delete `RC2` | 72% | 13 of 29 (1 turned) | 36.8 mm |

A rename of a net should move nothing; on the bench board it moves ten parts by up to 1.3 mm; the cause was not looked for. Deleting one resistor of a supply moves 13 of the 29 other parts.

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
