# What a capacitor derating rule would newly fail

Facts and numbers for issue #331 ("Next stages", item 5: derating, D3 of `docs/research/environment-and-grade.md`). **No rule is added and none is proposed here.** The owner has not chosen a margin, so every number is given for five: 1.0, 1.25, 1.5, 2.0 and 2.5 (1.0 asks only that the rating be met; the others are the usual factors that capacitor makers and design guides name for ceramic capacitors against DC bias and ageing; none of them is a figure from a datasheet in this repository, and none is claimed as one).

The rule measured is `Capacitor`: `.max_voltage >= .bias * margin`.

## 1. Semantics, read from the code

- **Where `.bias` comes from.** `std/parts/passive.nsm` (`Capacitor`, `.bias`: "the voltage across the capacitor"). A capacitor whose two pins are on the nets of a `Rail`'s `pos` and `neg` (a `~>` chain, `~` joins or net names; the rail must be one the capacitor's module or board can see) is given `bias == rail.voltage` by the checker (`Checker::derive_bias`, `crates/netsmith-check/src/connect.rs`). A rail's voltage is a range (`5V +/- 5%` is 4.75 V to 5.25 V), so `.bias` is a range. A capacitor across no rail (a signal, a floating bootstrap pair, a rail that states no voltage) has `.bias` unset unless the board states it; nothing in the examples does.
- **Where `.max_voltage` comes from.** The picked row's `max_voltage` cell of `data/jlcpcb/catalog/capacitor.csv` (the supplier's rated voltage, one value). A generic part (ADR 0047) has no row; the solver gives it the least rating the checks need.
- **What the rule reads.** A `>=` means "holds for every value in the ranges" (ADR 0058, ADR 0098): the rating's least value must be at least the greatest value of `bias * margin`. So the rule reads **the upper end of the bias range**, which is also the conservative reading, and I used it: ratio = rated voltage / upper end of the bias range, pass when ratio >= margin. The solver's own message says the same (`10V is not always above 8.3V..15.8V`).
- **What it does not read.** `Rail.peak` (the rule reads `.bias`, which is the rail's `voltage`, not its peak), a capacitor's temperature, or its DC-bias loss of capacitance (a separate field, `.effective_capacitance`). A tantalum row (the Easyduino project's own catalog) is judged by the same number although a ceramic's factors do not obviously apply to it.
- **Unset bias.** A rule over an unset `.bias` is listed as `not evaluated: <cap>.max_voltage >= (<cap>.bias * m)` (ADR 0057), never a pass and never a failure. Section 4 lists those capacitors.
- **What exists today.** Only `BuckStage` states a rating rule (`.input_cap.max_voltage >= .vin.voltage * .voltage_margin`, default 1, ADR 0085), and a few examples state their own (`.input_cap.max_voltage >= .vin.voltage * 1.5` in `linear_regulator.nsm`, `four_layer.nsm`, `outline_and_pours.nsm`, `two_boards.nsm`, `enclosure_sensor.nsm`). A plain `Capacitor` checks none.

## 2. Method (reproducible)

Two throwaway edits, never committed (`git checkout crates/` removes them):

1. `crates/netsmith-solve/src/solve.rs`: at the end of `solve_model`, when the environment variable `DERATE_DUMP` is set, print one line to stderr for every `Capacitor` part (and every part that provides it): the unit, the part's path, its `bias` and `max_voltage` domains (`lo..hi`), the pick's row name, LCSC number and bank count, and whether the solve had diagnostics.
2. `crates/netsmith-check/src/loader.rs`: when `DERATE_MARGIN` is set, add the line `.max_voltage >= .bias * <margin>` to `Capacitor` in the embedded `std/parts/passive.nsm` text, after `.dielectric: Dielectric`.

Then, with `cargo build -p netsmith --bin netsmith` and `target/debug/netsmith`:

- **Today's picks and ranges** (sections 3 and 4, no rule): `DERATE_DUMP=1 netsmith build <file.nsm> --out <dir>` in `examples/` for each of the single files, and `netsmith build --out <dir>` inside each of the six projects. 96 capacitor parts in 11 of the 14 examples (`carrier_and_cards`, `led` and `led_panel` have no capacitor); the counts agree with the capacitor entries of the committed pick locks (`grep -c 'catalog = "capacitor"' examples/**/*.lock.toml`). The ratio is computed from the dump.
- **With the rule** ("Result when the solver picks again"): the same, with `DERATE_MARGIN=<m>`, in a copy of `examples/` from which the `*.lock.toml` files were deleted before each margin (`build` writes a missing lock, so the files must be deleted again for every margin). With a lock in place the same rule does not re-pick: the build fails with "is locked to the row ... and no other row was tried" (section 5).
- **Tests and guide**: `DERATE_DUMP=1 cargo test --workspace -- --nocapture --test-threads=4` (4357 tests, all passing without the rule) printed 3093 capacitor lines; 2301 distinct (test, board, part, bias, rating, row) records came from solves without diagnostics, 1764 with a bias (1513 of them on rows of the standard catalog, 251 on fixtures' own rows), 537 without. The test names come from the thread name. With the rule, `DERATE_MARGIN=<m> cargo test --workspace --no-fail-fast` was run for **1.0 and 1.5 only**; 1.25, 2.0 and 2.5 were not run (the owner's rule of 2026-11-11 stopped the run), so section 6 has no test result for them.

Not computed: the effective capacitance of any alternative pick by hand (the solver did it); a datasheet derating factor of any row (none is recorded in the repository, `data/jlcpcb/README.md` "What is not covered"); the reference designators (the solver has paths, not designators: a capacitor is named by its path in the board, `stage.input_cap`).

## 3. The examples, capacitor by capacitor

Today's picks (no rule). "Across" is the upper end of the bias range, the rail's greatest voltage. The ratio is rated / across. Capacitors with no bias are in section 4. `×2` is a bank of two.

| Board | Capacitor | Across (rail, upper end) | Rated | Ratio | Picked row |
|---|---|---|---|---|---|
| easyduino_esp32s3 | `v3v3_100n` | 3.399 V | 16 V | 4.71 | C1525 |
| easyduino_esp32s3 | `v3v3_22u_a` | 3.399 V | 10 V | 2.94 | C11366 |
| easyduino_esp32s3 | `v3v3_22u_b` | 3.399 V | 10 V | 2.94 | C11366 |
| enclosure_sensor | `ldo.input_cap` | 5.25 V | 10 V | 1.90 | C315248 |
| enclosure_sensor | `ldo.output_cap` | 3.465 V | 6.3 V | 1.82 | C59461 |
| enclosure_sensor | `mcu_bypass` | 3.465 V | 16 V | 4.62 | C1525 |
| enclosure_sensor | `radio.bypass` | 3.465 V | 16 V | 4.62 | C1525 |
| four_layer | `stage.input_cap` | 5.25 V | 10 V | 1.90 | C315248 |
| four_layer | `stage.output_cap` | 3.465 V | 6.3 V | 1.82 | C59461 |
| linear_regulator | `stage.input_cap` | 5.25 V | 10 V | 1.90 | C315248 |
| linear_regulator | `stage.output_cap` | 3.465 V | 6.3 V | 1.82 | C59461 |
| orbit | `buck_boost.output_cap` | 3.39013 V | 6.3 V | 1.86 | C59461 |
| orbit | `charger.vbus_cap` | 5.25 V | 16 V | 3.05 | C19666 |
| orbit | `usb_uart.vbus_100n` | 5.25 V | 50 V | 9.52 | C14663 |
| orbit | `usb_uart.vbus_1u` | 5.25 V | 50 V | 9.52 | C15849 |
| orbit | `v3v3_100n_1` | 3.39013 V | 50 V | 14.75 | C14663 |
| orbit | `v3v3_100n_2` | 3.39013 V | 50 V | 14.75 | C14663 |
| orbit | `v3v3_100n_3` | 3.39013 V | 50 V | 14.75 | C14663 |
| orbit | `v3v3_100n_4` | 3.39013 V | 50 V | 14.75 | C14663 |
| orbit | `v3v3_bulk` | 3.39013 V | 16 V | 4.72 | C19666 |
| outline_and_pours | `stage.input_cap` | 5.25 V | 10 V | 1.90 | C315248 |
| outline_and_pours | `stage.output_cap` | 3.465 V | 6.3 V | 1.82 | C59461 |
| rp2350_minimal | `v3v3_100n_1` | 3.365 V | 16 V | 4.75 | C1525 |
| rp2350_minimal | `v3v3_100n_2` | 3.365 V | 16 V | 4.75 | C1525 |
| rp2350_minimal | `v3v3_100n_3` | 3.365 V | 16 V | 4.75 | C1525 |
| rp2350_minimal | `v3v3_100n_4` | 3.365 V | 16 V | 4.75 | C1525 |
| rp2350_minimal | `v3v3_100n_5` | 3.365 V | 16 V | 4.75 | C1525 |
| rp2350_minimal | `v3v3_100n_6` | 3.365 V | 16 V | 4.75 | C1525 |
| rp2350_minimal | `v3v3_100n_7` | 3.365 V | 16 V | 4.75 | C1525 |
| rp2350_minimal | `v3v3_100n_8` | 3.365 V | 16 V | 4.75 | C1525 |
| rp2350_minimal | `v3v3_bulk_a` | 3.365 V | 16 V | 4.75 | C77075 |
| rp2350_minimal | `v3v3_bulk_b` | 3.365 V | 16 V | 4.75 | C77075 |
| rp2350_minimal | `v3v3_bulk_c` | 3.365 V | 10 V | 2.97 | C23733 |
| rp2350_minimal | `vbus_bulk` | 5.25 V | 16 V | 3.05 | C77075 |
| schematic_bench | `buck.c_in` | 5.25 V | 10 V | 1.90 | C19702 |
| schematic_bench | `buck.c_out` | 1.89 V | 6.3 V | 3.33 | C59461 |
| schematic_bench | `ldo.c_in` | 3.99 V | 25 V | 6.27 | C52923 |
| schematic_bench | `ldo.c_out` | 2.94 V | 25 V | 8.50 | C52923 |
| schematic_bench | `mcu.bulk` | 3.465 V | 10 V | 2.89 | C19702 |
| schematic_bench | `mcu.bypass` | 3.465 V | 16 V | 4.62 | C1525 |
| schematic_bench | `modem.c_a` | 3.465 V | 10 V | 2.89 | C19702 |
| schematic_bench | `modem.c_b` | 3.465 V | 16 V | 4.62 | C1525 |
| schematic_bench | `modem.c_c` | 3.465 V | 16 V | 4.62 | C1525 |
| status_indicator | `status_indicator.bypass` | 3.465 V | 16 V | 4.62 | C1525 |
| two_boards | `power.stage.input_cap` | 5.25 V | 10 V | 1.90 | C315248 |
| two_boards | `power.stage.output_cap` | 3.465 V | 6.3 V | 1.82 | C59461 |
| two_buck | `buck_3v3.bulk` | 3.465 V | 6.3 V | 1.82 | C59461 |
| two_buck | `buck_3v3.decoupling` | 3.465 V | 16 V | 4.62 | C1525 |
| two_buck | `buck_3v3.input_decoupling` | 15.8 V | 16 V | 1.01 | C1525 |
| two_buck | `buck_3v3.stage.input_cap` ×2 | 15.8 V | 100 V | 6.33 | C55151 |
| two_buck | `buck_3v3.stage.output_cap` | 3.465 V | 6.3 V | 1.82 | C16780 |
| two_buck | `buck_5v.bulk` | 5.25 V | 6.3 V | 1.20 | C59461 |
| two_buck | `buck_5v.decoupling` | 5.25 V | 16 V | 3.05 | C1525 |
| two_buck | `buck_5v.input_decoupling` | 15.8 V | 16 V | 1.01 | C1525 |
| two_buck | `buck_5v.stage.input_cap` ×2 | 15.8 V | 100 V | 6.33 | C55151 |
| two_buck | `buck_5v.stage.output_cap` | 5.25 V | 6.3 V | 1.20 | C16780 |
| two_buck | `bulk_5v` | 5.25 V | 6.3 V | 1.20 | C59461 |
| two_buck | `bulk_a` | 15.8 V | 10 V | 0.63 | C19702 |
| two_buck | `bulk_b` | 15.8 V | 10 V | 0.63 | C19702 |
| two_buck | `bulk_c` | 15.8 V | 10 V | 0.63 | C19702 |
| two_buck | `ldo_bulk` | 3.465 V | 10 V | 2.89 | C23733 |
| two_buck | `ldo_in` | 5.25 V | 25 V | 4.76 | C52923 |
| two_buck | `ldo_out` | 3.465 V | 25 V | 7.22 | C52923 |

Summary of the failing counts for the 63 judged capacitors (ratio below the margin):

| Margin | Capacitors that fail | Boards | Which |
|---|---|---|---|
| 1.0 | 3 | 1 | `two_buck` |
| 1.25 | 8 | 1 | `two_buck` |
| 1.5 | 8 | 1 | `two_buck` |
| 2.0 | 22 | 8 | `enclosure_sensor`, `four_layer`, `linear_regulator`, `orbit`, `outline_and_pours`, `schematic_bench`, `two_boards`, `two_buck` |
| 2.5 | 22 | 8 | the same eight |

Here **at 1.5, 8 capacitors on 1 board fail; at 2.0, 22 capacitors on 8 boards fail**. The sets at 1.25 and 1.5 are the same, and so are those at 2.0 and 2.5, because the catalog's ratings are 6.3, 10, 16, 25, 50 and 100 V and no pick lies between the margins. Boards that fail nothing at any margin up to 2.5: `easyduino_esp32s3`, `rp2350_minimal`, `status_indicator` (every capacitor of the first two with a bias is rated at least 2.9 times it, and the third 4.6 times; their other capacitors have no bias, section 4).

### Margin 1.0

| Board | Capacitor | Bias range | Rated | Ratio | Needed | Result when the solver picks again (no lock) |
|---|---|---|---|---|---|---|
| two_buck | `bulk_a` | 8.3 to 15.8 V | 10 V | 0.63 | 15.8 V | C96446 (25 V) |
| two_buck | `bulk_b` | 8.3 to 15.8 V | 10 V | 0.63 | 15.8 V | C96446 (25 V) |
| two_buck | `bulk_c` | 8.3 to 15.8 V | 10 V | 0.63 | 15.8 V | C96446 (25 V) |

### Margin 1.25

| Board | Capacitor | Bias range | Rated | Ratio | Needed | Result when the solver picks again (no lock) |
|---|---|---|---|---|---|---|
| two_buck | `buck_3v3.input_decoupling` | 8.3 to 15.8 V | 16 V | 1.01 | 19.75 V | C14663 (50 V) |
| two_buck | `buck_5v.bulk` | 4.75 to 5.25 V | 6.3 V | 1.20 | 6.562 V | C3845593 (10 V) |
| two_buck | `buck_5v.input_decoupling` | 8.3 to 15.8 V | 16 V | 1.01 | 19.75 V | C14663 (50 V) |
| two_buck | `buck_5v.stage.output_cap` | 4.75 to 5.25 V | 6.3 V | 1.20 | 6.562 V | C315248 (10 V) |
| two_buck | `bulk_5v` | 4.75 to 5.25 V | 6.3 V | 1.20 | 6.562 V | C3845593 (10 V) |
| two_buck | `bulk_a` | 8.3 to 15.8 V | 10 V | 0.63 | 19.75 V | C96446 (25 V) |
| two_buck | `bulk_b` | 8.3 to 15.8 V | 10 V | 0.63 | 19.75 V | C96446 (25 V) |
| two_buck | `bulk_c` | 8.3 to 15.8 V | 10 V | 0.63 | 19.75 V | C96446 (25 V) |

### Margin 1.5

| Board | Capacitor | Bias range | Rated | Ratio | Needed | Result when the solver picks again (no lock) |
|---|---|---|---|---|---|---|
| two_buck | `buck_3v3.input_decoupling` | 8.3 to 15.8 V | 16 V | 1.01 | 23.7 V | C14663 (50 V) |
| two_buck | `buck_5v.bulk` | 4.75 to 5.25 V | 6.3 V | 1.20 | 7.875 V | C3845593 (10 V) |
| two_buck | `buck_5v.input_decoupling` | 8.3 to 15.8 V | 16 V | 1.01 | 23.7 V | C14663 (50 V) |
| two_buck | `buck_5v.stage.output_cap` | 4.75 to 5.25 V | 6.3 V | 1.20 | 7.875 V | C315248 (10 V) |
| two_buck | `bulk_5v` | 4.75 to 5.25 V | 6.3 V | 1.20 | 7.875 V | C3845593 (10 V) |
| two_buck | `bulk_a` | 8.3 to 15.8 V | 10 V | 0.63 | 23.7 V | C96446 (25 V) |
| two_buck | `bulk_b` | 8.3 to 15.8 V | 10 V | 0.63 | 23.7 V | C96446 (25 V) |
| two_buck | `bulk_c` | 8.3 to 15.8 V | 10 V | 0.63 | 23.7 V | C96446 (25 V) |

### Margin 2.0

| Board | Capacitor | Bias range | Rated | Ratio | Needed | Result when the solver picks again (no lock) |
|---|---|---|---|---|---|---|
| enclosure_sensor | `ldo.input_cap` | 4.75 to 5.25 V | 10 V | 1.90 | 10.5 V | C55151 (100 V) |
| enclosure_sensor | `ldo.output_cap` | 3.135 to 3.465 V | 6.3 V | 1.82 | 6.93 V | build error (no row and no generic fits) |
| four_layer | `stage.input_cap` | 4.75 to 5.25 V | 10 V | 1.90 | 10.5 V | C55151 (100 V) |
| four_layer | `stage.output_cap` | 3.135 to 3.465 V | 6.3 V | 1.82 | 6.93 V | build error (no row and no generic fits) |
| linear_regulator | `stage.input_cap` | 4.75 to 5.25 V | 10 V | 1.90 | 10.5 V | C55151 (100 V) |
| linear_regulator | `stage.output_cap` | 3.135 to 3.465 V | 6.3 V | 1.82 | 6.93 V | build error (no row and no generic fits) |
| orbit | `buck_boost.output_cap` | 3.212 to 3.39 V | 6.3 V | 1.86 | 6.78 V | no catalog row: a generic part (`fab` stops) |
| outline_and_pours | `stage.input_cap` | 4.75 to 5.25 V | 10 V | 1.90 | 10.5 V | C55151 (100 V) |
| outline_and_pours | `stage.output_cap` | 3.135 to 3.465 V | 6.3 V | 1.82 | 6.93 V | build error (no row and no generic fits) |
| schematic_bench | `buck.c_in` | 4.75 to 5.25 V | 10 V | 1.90 | 10.5 V | C96446 (25 V) |
| two_boards | `power.stage.input_cap` | 4.75 to 5.25 V | 10 V | 1.90 | 10.5 V | C55151 (100 V) |
| two_boards | `power.stage.output_cap` | 3.135 to 3.465 V | 6.3 V | 1.82 | 6.93 V | build error (no row and no generic fits) |
| two_buck | `buck_3v3.bulk` | 3.135 to 3.465 V | 6.3 V | 1.82 | 6.93 V | C3845593 (10 V) |
| two_buck | `buck_3v3.input_decoupling` | 8.3 to 15.8 V | 16 V | 1.01 | 31.6 V | C14663 (50 V) |
| two_buck | `buck_3v3.stage.output_cap` | 3.135 to 3.465 V | 6.3 V | 1.82 | 6.93 V | C315248 (10 V) |
| two_buck | `buck_5v.bulk` | 4.75 to 5.25 V | 6.3 V | 1.20 | 10.5 V | no catalog row: a generic part (`fab` stops) |
| two_buck | `buck_5v.input_decoupling` | 8.3 to 15.8 V | 16 V | 1.01 | 31.6 V | C14663 (50 V) |
| two_buck | `buck_5v.stage.output_cap` | 4.75 to 5.25 V | 6.3 V | 1.20 | 10.5 V | C55151 (100 V) |
| two_buck | `bulk_5v` | 4.75 to 5.25 V | 6.3 V | 1.20 | 10.5 V | no catalog row: a generic part (`fab` stops) |
| two_buck | `bulk_a` | 8.3 to 15.8 V | 10 V | 0.63 | 31.6 V | no catalog row: a generic part (`fab` stops) |
| two_buck | `bulk_b` | 8.3 to 15.8 V | 10 V | 0.63 | 31.6 V | no catalog row: a generic part (`fab` stops) |
| two_buck | `bulk_c` | 8.3 to 15.8 V | 10 V | 0.63 | 31.6 V | no catalog row: a generic part (`fab` stops) |

### Margin 2.5

| Board | Capacitor | Bias range | Rated | Ratio | Needed | Result when the solver picks again (no lock) |
|---|---|---|---|---|---|---|
| enclosure_sensor | `ldo.input_cap` | 4.75 to 5.25 V | 10 V | 1.90 | 13.12 V | C55151 (100 V) |
| enclosure_sensor | `ldo.output_cap` | 3.135 to 3.465 V | 6.3 V | 1.82 | 8.662 V | build error (no row and no generic fits) |
| four_layer | `stage.input_cap` | 4.75 to 5.25 V | 10 V | 1.90 | 13.12 V | C55151 (100 V) |
| four_layer | `stage.output_cap` | 3.135 to 3.465 V | 6.3 V | 1.82 | 8.662 V | build error (no row and no generic fits) |
| linear_regulator | `stage.input_cap` | 4.75 to 5.25 V | 10 V | 1.90 | 13.12 V | C55151 (100 V) |
| linear_regulator | `stage.output_cap` | 3.135 to 3.465 V | 6.3 V | 1.82 | 8.662 V | build error (no row and no generic fits) |
| orbit | `buck_boost.output_cap` | 3.212 to 3.39 V | 6.3 V | 1.86 | 8.475 V | no catalog row: a generic part (`fab` stops) |
| outline_and_pours | `stage.input_cap` | 4.75 to 5.25 V | 10 V | 1.90 | 13.12 V | C55151 (100 V) |
| outline_and_pours | `stage.output_cap` | 3.135 to 3.465 V | 6.3 V | 1.82 | 8.662 V | build error (no row and no generic fits) |
| schematic_bench | `buck.c_in` | 4.75 to 5.25 V | 10 V | 1.90 | 13.12 V | C96446 (25 V) |
| two_boards | `power.stage.input_cap` | 4.75 to 5.25 V | 10 V | 1.90 | 13.12 V | C55151 (100 V) |
| two_boards | `power.stage.output_cap` | 3.135 to 3.465 V | 6.3 V | 1.82 | 8.662 V | build error (no row and no generic fits) |
| two_buck | `buck_3v3.bulk` | 3.135 to 3.465 V | 6.3 V | 1.82 | 8.662 V | C3845593 (10 V) |
| two_buck | `buck_3v3.input_decoupling` | 8.3 to 15.8 V | 16 V | 1.01 | 39.5 V | C14663 (50 V) |
| two_buck | `buck_3v3.stage.output_cap` | 3.135 to 3.465 V | 6.3 V | 1.82 | 8.662 V | C315248 (10 V) |
| two_buck | `buck_5v.bulk` | 4.75 to 5.25 V | 6.3 V | 1.20 | 13.12 V | no catalog row: a generic part (`fab` stops) |
| two_buck | `buck_5v.input_decoupling` | 8.3 to 15.8 V | 16 V | 1.01 | 39.5 V | C14663 (50 V) |
| two_buck | `buck_5v.stage.output_cap` | 4.75 to 5.25 V | 6.3 V | 1.20 | 13.12 V | C55151 (100 V) |
| two_buck | `bulk_5v` | 4.75 to 5.25 V | 6.3 V | 1.20 | 13.12 V | no catalog row: a generic part (`fab` stops) |
| two_buck | `bulk_a` | 8.3 to 15.8 V | 10 V | 0.63 | 39.5 V | no catalog row: a generic part (`fab` stops) |
| two_buck | `bulk_b` | 8.3 to 15.8 V | 10 V | 0.63 | 39.5 V | no catalog row: a generic part (`fab` stops) |
| two_buck | `bulk_c` | 8.3 to 15.8 V | 10 V | 0.63 | 39.5 V | no catalog row: a generic part (`fab` stops) |

"Result when the solver picks again" is what `netsmith build` did with the rule and no lock: a row of the catalog (its LCSC number and rating), a **generic part** (no row exists with the rating and the other checks; the build succeeds with a note and `fab` stops until a row is found, ADR 0047), or a **build error**. The pick can move to a row whose other figures differ (a stage's output capacitor became 10 uF 10 V where it was 47 uF 6.3 V at 1.25: the solver takes the first row that passes, with the bank count free to 1 to 8).

## 4. Capacitors the rule would not judge (bias unset)

33 of the 96 capacitors have no bias, so the rule lists them as not evaluated (a new `not evaluated:` note for each in the build output, which is a changed output for every board that has one) and passes nothing about them. Their ratings are 10 V to 50 V on nets the source gives no voltage; what each is across is read from the source.

| Board | Capacitor | Rated | Across |
|---|---|---|---|
| easyduino_esp32s3 | `boot_100n` | 16 V | GPIO0 to ground (a signal, no rail) |
| easyduino_esp32s3 | `bridge_vdd_cap` | 16 V | the bridge chip's own VDD pin to ground; no rail |
| easyduino_esp32s3 | `en_100n_a` | 16 V | the EN line to ground (a signal, no rail) |
| easyduino_esp32s3 | `en_100n_b` | 16 V | the EN line to ground (a signal, no rail) |
| easyduino_esp32s3 | `vbus_100n` | 16 V | `.vbus`, a rail that states no voltage |
| easyduino_esp32s3 | `vbus_1u` | 25 V | `.vbus`, a rail that states no voltage |
| easyduino_esp32s3 | `vbus_22u` | 10 V | `.vbus`, a rail that states no voltage (a tantalum, not a ceramic) |
| orbit | `buck_boost.input_cap` | 10 V | `.vbatt`, which states no voltage |
| orbit | `charger.vbatt_cap` | 16 V | `.vbatt`, which states no voltage ("the schematic names no cell") |
| orbit | `oscillator.load_in` | 50 V | the crystal pin to ground (a signal) |
| orbit | `oscillator.load_out` | 50 V | the crystal pin to ground (a signal) |
| orbit | `reset_cap` | 50 V | the reset line to ground (a signal) |
| orbit | `usb_uart.vdd_100n` | 50 V | the bridge chip's own regulator output; no rail |
| orbit | `usb_uart.vdd_bulk` | 16 V | the bridge chip's own regulator output; no rail |
| orbit | `user_node_cap` | 50 V | a signal node to ground |
| orbit | `user_pin_cap` | 50 V | a signal pin to ground |
| rp2350_minimal | `avdd_filter_cap` | 10 V | the AVDD node after a 33 ohm resistor from `.v3v3`; no rail |
| rp2350_minimal | `oscillator.load_in` | 50 V | the crystal pin to ground (a signal) |
| rp2350_minimal | `oscillator.load_out` | 50 V | the crystal pin to ground (a signal) |
| rp2350_minimal | `v1v1_100n_1` | 16 V | `.v1v1`, which states no voltage |
| rp2350_minimal | `v1v1_100n_2` | 16 V | `.v1v1`, which states no voltage |
| rp2350_minimal | `v1v1_100n_3` | 16 V | `.v1v1`, which states no voltage |
| rp2350_minimal | `v1v1_bulk` | 10 V | `.v1v1`, which states no voltage ("the chip's choice") |
| schematic_bench | `adc.c_filter` | 16 V | the output of an RC filter to ground (a signal) |
| schematic_bench | `buck.c_boot` | 16 V | the IC's BST to SW (a floating bootstrap pair) |
| schematic_bench | `buck.c_vcc` | 25 V | the IC's VCC pin to ground; no rail |
| schematic_bench | `modem.cf_a` | 50 V | the filter node after a series resistor, to ground (a signal) |
| schematic_bench | `modem.cf_b` | 50 V | the filter node after a series resistor, to ground (a signal) |
| schematic_bench | `modem.cf_c` | 50 V | the filter node after a series resistor, to ground (a signal) |
| schematic_bench | `modem.cf_d` | 50 V | the filter node after a series resistor, to ground (a signal) |
| schematic_bench | `modem.cf_e` | 50 V | the filter node after a series resistor, to ground (a signal) |
| two_buck | `buck_3v3.boost` | 16 V | the IC's BST to SW (a floating bootstrap pair) |
| two_buck | `buck_5v.boost` | 16 V | the IC's BST to SW (a floating bootstrap pair) |

Most are signals or the IC's own pins, where no `Rail` can exist. Seven are across a rail that states no voltage (`vbus` of `easyduino_esp32s3`; `vbatt` of `orbit`; `v1v1` of `rp2350_minimal`): there a board would state the rail's voltage to be judged. I did not compute what they would score, because the source does not state the voltage.

## 5. Board by board: what the fix would be

- **`two_buck`** (the only board that fails at 1.0 to 1.5). The three input capacitors `bulk_a`, `bulk_b` and `bulk_c` are `10uF 10% 10V X5R 0603 C19702` across the 12 V input stated as 8.3 V to 15.8 V: **rated below the rail already today (ratio 0.63)**, so the board is under-rated whatever the margin. The catalog has a row (`C96446`, 10 uF 20% 25 V X5R 0603): the solver takes it at 1.0 (ratio 1.58, passes up to 1.5). At 1.25 and 1.5 five more capacitors move: the two 100 nF 16 V decoupling capacitors on the input go to `C14663` (50 V), the two 22 uF 6.3 V on 5 V (`buck_5v.bulk`, `bulk_5v`) go to `C3845593` (22 uF 10 V, 0402), and the stage output capacitor (47 uF 6.3 V) is picked as 10 uF 10 V. At 2.0 and 2.5 the catalog has no row for them: `bulk_a`, `bulk_b`, `bulk_c` (no 10 uF 20% rating of 31.6 V), `buck_5v.bulk` and `bulk_5v` (22 uF at 10.5 V) become generic parts. With the committed lock (`two_buck.lock.toml`), the build fails at every margin until `netsmith lock --update`.
- **`four_layer`, `linear_regulator`, `outline_and_pours`, `two_boards`, `enclosure_sensor`** (the LDO example, repeated; they fail at 2.0 and 2.5 only). Input capacitor `10uF 20% 10V X5R 0402 C315248` on 5 V (ratio 1.90) moves to `C55151` (2.2 uF 100 V X7R 1210) and `enclosure_sensor`'s two 100 nF 16 V on 3.3 V (ratio 4.6) are fine. The output capacitor `22uF 20% 6.3V X5R 0603 C59461` on 3.3 V (ratio 1.82) has **no row**: the board asks for an effective capacitance, and of the rows of 4.7 uF or more rated 10 V or more, only two have a bias curve (`C23733`, `C315248`); the solver reports a build error ("no combination of candidates satisfies `stage.output_cap` and `stage.regulator` together", the closest being 10 uF 6.3 V `C15525`). These five boards say `.max_voltage >= .vin.voltage * 1.5` themselves, so 1.5 and below adds nothing to them.
- **`orbit`**: `buck_boost.output_cap` (22 uF 20% 6.3 V on about 3.3 V, ratio 1.86) falls back to a generic part at 2.0; locked, the build fails.
- **`schematic_bench`**: `buck.c_in` (10 uF 10 V on 5 V, ratio 1.90) moves to `C96446` (25 V) at 2.0.
- **`easyduino_esp32s3`, `rp2350_minimal`, `status_indicator`**: nothing fails; their rated-to-bias ratios of the judged capacitors are at least 2.94 (a 22 uF 10 V tantalum on 3.4 V in `easyduino_esp32s3`).

## 6. Tests and golden outputs

**Test fixtures, by the rating alone** (no re-pick; counted from the `cargo test` dump; 242 distinct capacitors of fixtures that are not the examples, in 36 test files): 1 fails at 1.0, 14 at 1.25, 25 at 1.5, 78 at 2.0 and 85 at 2.5. The one at 1.0 is a solver test, `dc_search::the_capacitor_is_picked_on_the_node_the_analysis_gave`, a 6.3 V row (C12530) on a node the analysis puts at up to 16 V. At 1.25 and above they are almost all the same two shapes: the guide's `Converter`-style stage (a 22 uF or 47 uF 6.3 V capacitor on 5.25 V, ratio 1.20) and the bank fixtures (47 uF 6.3 V on 5 V, ratio 1.26).

**The suite with the rule** (`cargo test --workspace --no-fail-fast`, whole suite, one run per margin):

- **1.0: 11 tests fail.** `std_catalogs::the_standard_catalogs_solve_and_resolve_as_they_did` (a golden of every example's picks), `guide::every_complete_example_checks_solves_and_fails_as_the_guide_says` (the guide's examples, `docs/LANGUAGE.md`; the test stops at its first mismatch, line 9174, so how many guide blocks change is not counted), and the nine tests of `crates/netsmith-check/tests/bias.rs`, which assert the relations and the derived bias of a capacitor (the one I read lists the new relation `c.max_voltage = (c.bias * 1)`): these nine fail for the new relation itself, whatever the margin.
- **1.5: 38 tests fail.** The 11 above, and: 14 of `bank::*` (a bank of 47 uF 6.3 V on 5 V, picks and bill of materials change); 4 of `editor_compare::*`; 5 of `std_models::*` (`a_bank_of_one_is_the_single_capacitor_the_stage_picked_before`, `a_datasheet_that_asks_for_more_than_one_capacitor_keeps_is_met_by_a_bank`, `a_stage_without_a_voltage_margin_rates_its_capacitors_as_it_did`, `a_voltage_margin_picks_a_capacitor_rated_above_the_peak_by_that_factor`, `the_capacitors_have_the_effective_capacitance_the_ic_asks_for`); and one each of `dc_chain::a_circle_a_pick_waits_on_starts_from_the_networks_bounds`, `joint_bank::an_input_capacitor_bank_no_row_is_rated_for_is_named_though_it_is_searched_with_six_parts`, `lt8641::the_stage_asks_for_the_inductance_the_datasheets_equation_10_gives`, `std_generics::the_picks_and_the_effort_of_the_standard_kinds_are_what_they_were_as_parts`.
- **1.25, 2.0, 2.5: not run.** Expect at least the 1.5 set at 2.0 and 2.5 (the same fixtures fail the rating by more), and the examples' goldens: the CI `locks` job (`scripts/check-example-locks.sh`) fails for `two_buck` at every margin and for eight examples at 2.0 and above (section 3), because `netsmith lock --check` solves with the rule and the committed locks are stale.
- Also changed at any margin: each of the 33 capacitors with no bias adds a `not evaluated:` note to its board's build output (section 4); which tests assert a board's notes was not checked.

## 7. Recommendation

A rule with a default factor of 1 (`.environment.derating.voltage ?= 1`, D3) is safe to introduce as a check now, as far as the 14 examples go: at 1.0 it fails no example when picks are made again (it moves three capacitors of `two_buck`, which are rated 10 V on a rail that reaches 15.8 V today), and 1.25 also builds every example after a re-pick. What has to happen first, in this order: (1) `netsmith lock --update` for `two_buck` with a reviewed diff (C19702 to C96446 for `bulk_a`, `bulk_b`, `bulk_c`), since a stale lock makes `build` fail; (2) update the 11 tests and goldens that the rule changes at 1.0 (nine `bias` tests, the guide and `std_catalogs`); (3) the owner's decision on the 33 capacitors with no bias (a `not evaluated:` note on each, or a rule that is silent when the bias is unset, which would be a language choice), and whether the rails that state no voltage (`vbus` of `easyduino_esp32s3`, `vbatt` of `orbit`, `v1v1` of `rp2350_minimal`) should state one. A stated margin of 1.5 or more is not safe yet: 38 tests change at 1.5, and at 2.0 five boards cannot build (the LDO's 3.3 V output capacitor has no row with a bias curve rated for it) and six capacitors fall back to generic parts that stop `fab`; it needs rows first (22 uF and 10 uF at 10 V or more with a recorded DC-bias curve, and a 10 uF at 31.6 V or more).
