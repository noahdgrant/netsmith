---
status: proposed for owner review (the changes were approved by the owner; the name `LevelTranslator`, its members and relations, the choice of a header-only catalog file and the wording of the stage 0 message were made by an agent and are listed below for review)
---

# The P-channel MOSFET and a level translator are generics with no standard rows

Three small changes (a fourth, `I2cPullups.resistance` `required`, was approved and then withdrawn when the owner decided to drop `required`; it is not made) to the standard library and the checker, made together because each is the application of an earlier decision. ADR 0065 removed `required` altogether: `PullUp.resistance` (ADR 0061) and `I2cPullups.resistance` are plain members, and a board that forgets either gets the unconstrained pick finding for the resistor, with the member named.

`PMosfet` is a `generic`, like `NMosfet` (ADR 0059, section 12: the standard definitions with a catalog are generics that are their own first provider). Its pins, kinds, members and relations are what they were: the current flows from source to drain, and every voltage and current is a magnitude. It gains `catalog p_mosfet { .max_drain_source_voltage, .max_drain_current, .max_gate_source_voltage, .gate_threshold, .on_resistance, .package }`, the columns of `n_mosfet`. `Diode`, `Switch` and the connectors are still parts, because they have no catalog. A part of a project may say `provides PMosfet`.

`std::parts::logic::LevelTranslator` is a new generic for a single-supply level-translating buffer, next to `Sn74lvc1g04`. And a `.function &=` on a pin that nothing reads is an error (stage 0 of ADR 0062).

## No standard rows: a header-only file

Neither new catalog has a standard row. A catalog row needs a recorded source (`data/jlcpcb/README.md`): the LCSC number, the library type and the ratings, read from JLCPCB's and LCSC's records and the datasheet. No P-channel MOSFET and no translator has been read that way, and the agent that made this change could not read those pages. A public part that a project already uses has its row in the project's own catalog, and none was copied here.

The language allows an empty catalog: `Table::parse` reads a file with a header and no row, `Catalogs::identity` gives a catalog with no candidate, and the solver says what it says for any catalog with nothing for a requirement: ``the `p_mosfet` catalog has nothing for `q`, which needs nothing in particular``. So the standard files `data/jlcpcb/catalog/p_mosfet.csv` and `data/jlcpcb/catalog/level_translator.csv` are the header and nothing else. This was chosen over giving the generics no file at all, for one reason: the guide says that a project file named like a standard catalog adds its rows after the standard ones. With a header-only file that sentence is true of the two new catalogs, a project writes `p_mosfet.csv` or `level_translator.csv` and lists it in `netsmith.toml`, and nothing in the generic changes the day a first real row is added. Without the file, the same project file would be a new catalog of that name, which is the same to the solver today but is not what the guide says.

Three tests of the catalog files keep their meaning. `every_standard_file_reads_and_every_part_in_it_has_its_source` asserts that each standard file has a row, and its list of `CATALOGS` does not have the two new ones, on purpose; `crates/netsmith/tests/std_additions.rs` asserts instead that they are header-only, with the same header as `n_mosfet.csv` for the P-channel one. When a row is added, the file goes into `CATALOGS` and the test of the header goes.

A design that uses either generic, with no project row and no provider, is refused at the first use, in the same words as for a resistor no row serves. That changes a board only if it used the standard `PMosfet` as a part and stated every rating by hand: its ratings are now requirements that a row or a provider must meet. No example and no test of the repository did; the examples build byte for byte as before.

## The name `LevelTranslator`

The glossary has no word for the part, and its _Avoid_ lines do not list `translator`, `buffer` or `level shifter`. The standard kinds are named for what the part does for the board (`LinearRegulator`, `Tvs`, `Schottky`, `Led`), not for the topology or the family. Three other names were considered. `Buffer` says what the datasheet calls the part and not what it is for: a buffer that does not translate is a different part and would not satisfy a board that needs a 1.8 V input on a 3.3 V line. `SchmittBuffer` and a maker's part number name the input type or the maker's part. `LevelShifter` is the common word, and the glossary entry now lists it under _Avoid_ for the reason that the part's supply, and not a shift, sets the output: the part translates between two voltages and a shifter can be an up-only or a bidirectional one with a direction pin, which this is not. The generic is for the single-supply, one-direction buffer only; a bidirectional translator with two supplies would be another generic.

## What it is

The rail `vcc` (VCC and GND), the pins `a` (input), `y` (output) and `nc` (no-connect), a pin table for SOT-23-5 and one for SOT-353_SC-70-5, with pad 1 NC, 2 A, 3 GND, 4 Y and 5 VCC, the pad order of the 5-pin single-gate logic families (the order of KiCad's stock symbol `74xGxx:74LVC1G17`, which `std/kicad.toml` maps the kind to on the two packages). The catalog `level_translator` has the columns of a datasheet's tables:

- `.vcc_min` and `.vcc_max`, the supply range of the Recommended Operating Conditions. The relations are `.vcc.voltage >= .vcc_min` and `.vcc.voltage <= .vcc_max`.
- `.vcc_abs_max` and `.a_abs_max`, the absolute maximums on the supply and on the input, and the stresses a design states, `.vcc_peak` and `.a_peak`, each at most its maximum.
- `.vt_plus_max` and `.vt_minus_min`, the input thresholds: the highest value of the positive-going threshold (an input above it is a high for sure) and the lowest value of the negative-going threshold (an input below it is a low for sure). No relation in the generic uses them: they are what the design compares its driver's levels with (`.sensor.out.voh >= .u.vt_plus_max`, `.sensor.out.vol <= .u.vt_minus_min`), as the guide's example does.
- `.max_output_current` and the stress `.output_current` (a magnitude the design states), with `.output_current <= .max_output_current`.
- `.max_supply_current` with `.vcc.current <= .max_supply_current`.

It generalises a translator part that a project wrote for its own board (nothing of that project is copied: the rows of the guide and the tests are invented). Differences from that part, each for a reason. The datasheet numbers are catalog columns and not narrowings, so one generic serves every translator. The output current is a rating, `.max_output_current`, and the design's load is a separate stress, `.output_current`: in the part the datasheet's number was a value that nothing was compared with. The supply current is a relation and not the range `&= 0A..750nA`, because a range of the language has fixed numbers at both ends (`both ends of a range must be fixed numbers`) and a column is not one; the cost is that a design that wants the draw counted on its rail states `.u.vcc.current &= 0A..750nA` itself, and until it does the relation is listed as not evaluated, as every stress a design leaves out is.

`.a` and `.y` are plain pins, as in the part it generalises, and not `Logic` lines. A `Logic` input would take `.vih` and `.vil` from the two thresholds and let the solver's net rule judge the driver without a statement of the board, and it would also ask the board to join `.u.a.line` and not `.u.a`. It was left out because the pin paths are the generic's interface and the choice between the two is the owner's; see the open questions.

## Stage 0 of ADR 0062

A function stated on a pin that no part reads is an error. The message is the ADR's: ``error: `hub.scl.function` is stated, but `hub.scl` is a pin of module `Hub` and nothing passes it to a pin of a part, so it has no effect; inside `Hub`, write `.mcu.p04.function == .scl.function` ``, at the board's statement. Built as the ADR describes it, and no more:

- `Model::function_of` follows an `==` whose left side is the pin's `function` and whose right side is another pin's `function`, to a depth of six (a module in a module). The checks of the part then apply to the forwarded function: it must be on the pin, a function with a kind of its own counts on the net, and a function chosen on two pins is refused.
- The pass `pin_faults` reports a stated function on a pin with no row in any table of its part and that no equation passes on to a pin that has one. For a pin of a module (or board) the message is the one above; for a pin of a part with no row of its own (an alias) it says that the pin has no row in the pin table of the part. The hint names the part's pin when exactly one pin with functions is on the same net inside the module, and says `.<part>.<pin>.function == .scl.function` otherwise.
- The message of a function the pin does not offer points at the board's statement through the forwarding. The message of a connected pin that is not chosen names the module's pin that the board writes (`.hub.scl.function &= "GPIO"`) when the pin is forwarded.

What the ADR leaves for later stages is not built: no `peripheral`, no `shared`, no `on`, no `limit`.

Two choices of the agent. When nothing passes the choice on, the part's own pin still says it has several functions to choose from: that message is right (nothing chose one), and it is the message that was there before, so both errors are printed and the new one says where the choice went. And an equation with the exported pin on the left (`.scl.function == .mcu.p04.function`) is not a forwarding: the solver works a value from the right side to the left, and the two would not agree about which pin is chosen. It is the same error, with the same hint.

## Tests that changed

`crates/netsmith-check/tests/std_generics.rs` (new in this branch): `PMosfet` and `LevelTranslator` are in the list of generics (14, was 12), and `PMosfet` is no longer in the list of parts that stay parts. No other existing test changed.

## Open questions

1. Whether `LevelTranslator` should have `.a` and `.y` as `Logic` lines, with `.a.vih == .vt_plus_max` and `.a.vil == .vt_minus_min` and a `.y` with levels of the supply. The solver would then judge the driver by the net rule it has, with no statement of the board.
2. Whether the generic should have a `.vcc.current` range through a catalog column, which needs a range whose ends are values, a change to the language.
3. The pad order is one for all the 5-pin packages. A package with another order (a 6-pin one with two grounds) is another table in a provider; a translator of another family is a provider of its own.
4. A first real row of either catalog, with its source, from whoever can read the JLCPCB and LCSC records.
