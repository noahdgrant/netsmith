---
status: accepted (user decisions 2026-11-02)
---

# Pin direction per function, logic levels and absolute maximums, and designators from the part

ADR 0044 built the checks that needed no new language and listed three that did. The user decided all three on 2026-11-02, and decided where the reference letters of a part come from. This ADR records the four decisions, one section each, with the industry usage each follows. The syntax is the user's; nothing here adds a second spelling of anything.

## Designators: the letters are a field of the part

Every `part` has a built-in text field `.designator`, the letters of its reference (`R` in `R1`), and nothing is declared for it, exactly as for `.assembly` (ADR 0024, ADR 0028, ADR 0042): declaring it again is the error "`designator` is already declared in this block". A standard kind states its own letters in its definition, `.designator &= "R"` in `Resistor`, because they are always true of it. A part you define states its own the same way, or states none and is `U`, as every kind that is not in the table was before. A board may narrow the letters of a part of its own that states none (`.codec.designator &= "IC"`). Lowering reads the field, numbers the parts that share letters in the order of their paths, and refuses letters that are not one or more capital letters, with a message that names the part:

```text
error: Letters: `mcu` has `.designator` "U1", which is not one or more capital letters such as "R" or "IC"
```

The `prefix` table in `crates/netsmith-lower/src/lib.rs` is gone. Its replacement test reads `std/` and fails for any `part` that does not state `.designator &=` with capital letters, so a new standard kind gets its letters on purpose, as the old test made it name a place in the table.

The letters are the ones the table had, which are the usage of IEEE 315 (graphic symbols and class designations) and ASME Y14.44 (reference designations for electrical and electronics parts): `R`, `C`, `L`, `D`, `Q`, `Y`, `F`, `J`, `TP`, `H`, `FID`, `U`, and `FB`, `SW` and `LBL` as KiCad writes them. The same usage gives `K` for a relay, `BZ` for a buzzer, `BT` for a battery and `T` for a transformer. The library has no such kind yet, so a project that defines one writes the letters on it; a new kind added to `std/` states them from the same standard. A switch stays `SW` as it was and as KiCad writes it, where the standards' class letter is `S`; that is continuity, and it changes with one line in `std/parts/passive.nsm`.

A text field narrows to one value, so the letters a standard kind states cannot be changed by a part's user or by a board: the usual "contradiction on `r.designator`: it is already "R", which does not overlap "Z"". It is the limit ADR 0024 records for `.assembly` (a default that a board may replace is a separate language choice) and is accepted here for the same reason: a resistor is `R`.

The layout file is keyed by the path of a part (ADR 0032), so a saved placement does not move when letters change. This was checked two ways. A test (`a_layout_entry_still_places_its_part_when_the_designator_letters_change` in `crates/netsmith-pcb/tests/kicad_drc.rs`) places a part by its path from a layout file, renames every designator of the same design, and finds the part at the same place and turn. The second check built `examples/outline_and_pours.nsm` with a layout file that locks `stage.regulator` and `stage.output_cap`, once with a build from before ADR 0044 (where the regulator was `U7` and the holes `U3` to `U6`) and once with this change (`U1` and `H1` to `H4`), and compared the footprints in the two `.kicad_pcb` files: every part, including the two locked ones, was at the same place and turn.

A fab package that was already ordered keeps the designators it was made with, in its BOM, placement file and board file. The letters of a kind that ADR 0044 changed (a hole was `U1` and is `H1`) and any kind whose letters change later do not reach into a package that was made earlier; re-running `netsmith fab` makes a new package with the new designators, and the board house must be told if it is the same order.

## Pin direction for each function

A pin has one kind in a `pins` table, and a pin that several peripherals share can be a different direction as each of them: a GPIO is bidirectional, a UART transmitter an output, a receiver an input. ADR 0044 could not check "a function chosen that conflicts with the pin" because the table had one kind and a list of names, and guessing from names such as `TX` and `RX` would be a convention and not data.

Each function may now carry its own kind after a colon, with no spaces: `.pa2 bidirectional "PA2" "12" fn GPIO:bidirectional UART1_TX:output UART1_RX:input`. The pin's kind stays the default for the pin and for every function written without a colon. The kind is one of the six pin kinds. The industry usage is KiCad's alternate pin definitions (`(alternate "UART1_TX" output line)`), each of which has its own electrical type; ADR 0016 left them unchecked against a pin's functions and still does (the vendor's names rarely match a part's).

The net rules (`outputs` and `floating_inputs` in `crates/netsmith-check/src/faults.rs`) judge a pin by the kind of the function the board chose when that function has one, and by the pin's kind otherwise. So the check is the existing pair of net rules, not a new one: a function chosen for a pin must agree with how its net uses it. An `output` function on a net with another output is "two outputs", an `input` function on a net with nothing that drives it is "only inputs". The messages name the pin, the part's kind, the function and its kind:

```text
error: net `net-a.pa2` has two outputs: `a.pa2` (`Mcu8` function `UART1_TX`, an output), `b.pa2` (`Mcu8` function `UART1_TX`, an output)
```

The symbol, the KiCad electrical type check (ADR 0016, decision 8), the rule that an input must be connected and the other pin rules still use the pin's own kind. A pin whose functions give no kind, or whose function is not yet chosen, is the pin's kind, so every table written before this decision means what it meant.

Not guessed: no part in `std/` names functions, so none states a function kind, and no library part was migrated. The tests and the guide use invented parts. A function kind that repeats the pin's own (`GPIO:bidirectional` on a bidirectional pin) is allowed, because a datasheet table lists every function's type, and it changes nothing. A function's kind in one package is not compared with its kind in another; a pin's own kind still must be the same in every package.

## Logic levels and absolute maximums

`std::logic::Logic` gains four ordinary `Voltage` fields, `.vih`, `.vil`, `.voh` and `.vol`, and every pin gains `.abs_min` and `.abs_max`. A part states them as its datasheet does, with fixed volts or with an equation on its rail (`.vih &= .reference.voltage * 0.7`, `.voh &= .vdd.voltage - 0.1V`, `.line.abs_max &= 3.6V`). A level is the datasheet's guaranteed number, so `voh` and `vih` are minimums and `vol` and `vil` maximums, and they are ranges when the rail is. This is the design of ADR 0016 (Models), built as decided; the usage is the one every logic datasheet follows (JEDEC JESD8 names VIH, VIL, VOH and VOL; absolute maximum ratings are the usual table 1 of a datasheet).

After the rails are solved, `crates/netsmith-solve/src/levels.rs` checks two things. For every output or bidirectional `Logic` line on a net and every input or bidirectional `Logic` line on it (pins are judged by the kind of their chosen function, above), the output's `voh` must always be at least the input's `vih` and its `vol` at most the input's `vil`, comparing the worst ends of the ranges. And for every pin with an `.abs_min` or `.abs_max`, the net's worst-case voltage relative to the pin's ground, which is the hull of the ground itself, each rail whose `pos` is on the net, each `Logic` line that drives it (between its reference rail's pins, which is what `Logic` means) and each rail or ground a resistor joins it to, must stay within them. A fault is an error naming the fields and the numbers:

```text
error: net `net-a.out.line`: `a.out.voh` is 3.035V..3.365V but `b.in.vih` needs 3.325V..3.675V: a high of 3.035V is not always read as a high, which takes 3.675V
```

A check that cannot be made is a note, never a pass (`Solution::not_evaluated`, printed with the other notes as `not evaluated: ...`): a level with no value, two lines whose grounds are not one net or one ground group, a pin that is not the `.line` of a `Logic` (it has no ground), a net that a driver which is not a `Logic` line drives, a rail on another ground, and a net nothing sets. A pair of lines of which neither states any level is not looked at, so a design that uses `Logic` as it did before gets no new note.

A connection between two `Logic`s does not equate their levels (`Model::is_level`): the levels belong to each pin, and equating them would make every driver and receiver contradict each other. It does join their reference rails, as before, so a driver and a receiver on different supplies are joined line to line.

The first real part that states levels is `std::parts::logic::Sn74lvc1g04`, TI's SN74LVC1G04 inverter (datasheet SCES214AF, revised October 2025), with the numbers read from the PDF: `vih` 2 V and `vil` 0.8 V (table 5.3, VCC = 3 V to 3.6 V), `voh` VCC - 0.1 V and `vol` 0.1 V at 100 uA (table 5.5), absolute maximums from table 5.1, pads from the pin functions table. Each number's table is in the definition and in `data/jlcpcb/README.md`. The datasheet gives other thresholds for other supplies, so the part is restricted to 3 V to 3.6 V and refuses a supply outside it. It is named for the part number, not for an inverter in general, because its numbers are that datasheet's. It is mapped to KiCad's `74xGxx:74LVC1G04` and SOT-23-5. KiCad's symbol leaves the no-connect pad 1 out, so the pin validation (ADR 0016, decision 8) no longer counts a `no_connect` pad that the footprint has and the symbol lacks as a disagreement. It has no catalog row: an LCSC number needs its own evidence (`data/jlcpcb/README.md`), so `fab` refuses a board with it until one is added.

Limits. The voltage of a net is not followed through anything but resistors, so a net that a plain output or an open-drain line drives is not evaluated; a level check does not know the load (the part states the load of its numbers in its comment); and a standard kind cannot be given other levels by a board, because it is one value (the limit of `.assembly`, ADR 0024).

ADR 0064 extends the check to the pins of a bus interface: the same four voltages and the reference rail are the `Levels` of the interface (`std::logic::Levels`, the member `.levels` of `Uart`, `Spi`, `I2c` and `Swd`), and `levels.rs` finds the pin's levels through `Logic` or through `Levels`. Nothing else in this section changes.

## Left open

A default that a board may replace, for the letters of a standard kind and for `.assembly` (one language choice for both). More parts with datasheet levels, and a catalog row for `Sn74lvc1g04` once an LCSC number has its evidence. Levels across a level shifter or a series resistor, and for open-drain lines. A fab package that was ordered before a change of letters keeps its old designators, as said under Designators.
