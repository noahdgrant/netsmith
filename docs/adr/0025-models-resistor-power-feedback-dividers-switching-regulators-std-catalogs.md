---
status: accepted
---

# Models: resistor power, feedback dividers, switching regulators, std catalogs

Porting a real board showed four gaps that every board hits, so they are in the standard library and not in the board. The user was away; the choices below were taken for the user and are reviewed afterwards. Nothing here changes the language or the checker.

## 1. Resistor power

`Resistor` has a rating, `.max_power`, and states `.drop * .current <= .max_power`. `max_power` is a column of the `resistor` catalog, read for all 219 rows from the `Power(Watts)` attribute of the JLCPCB record and cross-checked with the LCSC record (equal for every row). It is not derated for a hot board.

The product `.drop * .current` is used because `.drop` already is `.current * .resistance` (Ohm's law, a relation of the part), so it carries the tolerance of the resistance and the window of the current, and the product is the largest power they admit. A current that flows the other way has a negative drop and gives the same power.

**An unrated resistor.** A catalog row with an empty `max_power` is never offered. This is the rule the loader already had for any empty cell (the row has no value there and fits no design), and it is the safe one for a rating: a part that cannot be shown to be within its rating is not picked. Unchecked-with-a-note was rejected because a design that states the current would get an unchecked part without knowing. The solver's "nothing for it" message now counts the rows left out for an empty cell. No row of the standard catalog is blank. A resistor whose current nothing states has no power to check; the relation is listed as not evaluated, as every rating is.

**The worst case is the current the circuit really carries.** `.led.current &= 5mA..15mA` is a window the LED must stay in. The equation `.led.current == (.input.voltage - .led.forward_voltage) / .r1.resistance` says what the current is for the resistor under test, and (see Update) it narrows the window to that value, so the resistor is checked at about 7 mA to 10.6 mA and 36 mW, not at 15 mA and 88 mW. `examples/led.nsm` picks the Basic 0402 330 ohm C25104 (62.5 mW), as before the rule existed.

No pick of an example changed. The test fixtures that build their own candidates give them a 1 kW rating, and `data/sample/resistor.csv` is 1 kW as well, so the power check is not what they test.

## 2. Feedback divider

`std::parts::regulator::FeedbackDivider` is a module of two resistors with three pins (`top`, `fb`, `bottom`) and two voltages, `vref` and `vout`, related by `vout == vref * (1 + rtop / rbottom)`. The solver's tight evaluation makes the output range the real one (the reference once, each resistance once), and it picks `rtop` and `rbottom` inside the wanted range or says which range the closest pair reaches. The currents `(vout - vref) / rtop` and `vref / rbottom` are stated, so each resistor's power is checked.

**Pins and voltages, not a `Rail`.** A first version had an `output: Rail`. Inside a stage that makes the output (below) the divider's rail and the stage's port are two terms on one net with no supplier among them, and the balance of currents (ADR 0007, ADR 0014) attributes the whole load to the divider and then refuses the divider's own current. A divider that is pins and voltages adds no term to any net, so it can sit under any rail. The board says `.out.voltage == .divider.vout` (or a stage does it for the board). The divider's few microamps to a milliamp are not added to the load; this is a limit.

The feedback pin draws nothing. An output below the reference is refused with the numbers (the divider can only raise it).

## 3. Switching regulators

`Buck` (an interface) and `BuckStage` (a module) in `std::parts::regulator`. The language has no inheritance, no generic module over a part and no relations in an interface that reach the part that holds it, so the split is: the IC is a `part` with a `.buck: Buck` member, written by whoever adds the IC (its pin table, its datasheet numbers as narrowings, and the one relation from its output to its input current); the stage is a module of everything else, joined to the IC by `.stage.buck ~ .u.buck`. A frequency that a resistor sets is one more relation the IC writes (`.buck.fsw == 24GHz * 1ohm / .rt_value`), tested.

**The switching node is a rail.** The checker's balance of currents needs a supplier inside the module's scope for a net that has a port, and a rail inside a part must have a pad (a part cannot be placed with a padless pin). The buck's output node is after the inductor and a buck IC usually has no pin on it, so the supplier is the IC's `sw` pin as a rail (`Buck.sw: Rail`, DC voltage the output's, current the negative of the load), and the inductor is a `~>` chain from it to the output. In a chain the inductor's `.current` is the draw, the average (ADR 0014), so the stage states the peak itself: `.inductor.current + .ripple / 2 <= .inductor.saturation_current`, with `ripple = (vin - vout) * vout / (vin * L * fsw)`. The input current, `iout * vout / (vin * efficiency)` over the whole efficiency range, is the IC's relation and reaches the supply through the IC's `vin` rail; the stage's own `vin` and `vout` ports draw no DC current (`vin.current &= 0A`).

**One capacitor on each side**, picked for the effective capacitance the IC asks for at the voltage across it (ADR 0013). The catalog's Basic capacitors lose most of their capacitance at a few volts, so an IC that asks for 10 uF or more at 12 V finds no capacitor; the guide's invented IC is 4.5 V to 6 V. The IC's recommended inductance is its `min_inductance`, one number.

**Not modelled**: loop stability, dropout and minimum on-time (the output must be at most the input), light-load modes, the divider's and the IC's quiescent current in the load, the second capacitor an IC may ask for. Each is a known limit in the ROADMAP.

The worked example is an invented IC, `Zbk2`, in the guide (`docs/LANGUAGE.md`, "Switching regulators") and in `crates/netsmith/tests/std_models.rs`. Nothing about a real part is in the library or the guide.

## 4. Catalogs for the standard kinds

The standard kinds of ADR 0023 had no catalog. Eight now have one, each a file in `data/jlcpcb/catalog/`: `schottky` (6 rows, all Basic), `zener` (7), `tvs` (9), `ferrite_bead` (2), `inductor` (4), `fuse` (6), `n_mosfet` (2, Basic) and `crystal` (4). Every row was fetched on 2026-11-01 from the JLCPCB record and the LCSC record and checked against the manufacturer's datasheet where one could be read; `data/jlcpcb/README.md` has the per-kind choices and `considered.csv` the parts left out and why.

- **The worst case of the table** is the number in the row: maximum forward voltage at the rated current, minimum to maximum Zener voltage at the test current, largest RDS(on) maximum, the whole temperature range of a 32.768 kHz crystal (-216 to +20 ppm, from the datasheet's parabolic coefficient and turnover temperature). Where the record and the datasheet disagree, the part is left out.
- **Only packages that `std/kicad.toml` maps**, so a pick can be exported. That excludes the 4 mm to 8 mm shielded power inductors (the 1210 parts are the ones with a mapping), the 4-pad crystals and HC-49, and the SOT-23 Zeners. A test checks every row's package against the mapping.
- **Basic where one exists**: all the Schottky diodes, the 2N7002, the AO3400A, one ferrite bead and the 32.768 kHz and 8 MHz crystals. The Zener, TVS, fuse and inductor rows are all Extended.
- **A design that states a rating must state a range that contains the row's** (`.zener_voltage &= 4.7V..5.5V` takes `4.8V..5.4V`; `5.1V +/- 5%` does not). The guide's examples of these kinds were changed to let the catalog supply values, and `crates/netsmith/tests/std_kinds.rs` solves with the four older catalogs only, so that its tests keep stating ratings by hand and checking the relations.
- `Diode`, `PMosfet` and the headers have no catalog: the task did not ask for them.

## Limits and what to review

- A current window that nothing ties to the circuit by an equation is still taken at its upper end, and so is a field with two equations on it (Update).
- `FeedbackDivider` and `BuckStage` leave out the quiescent and divider currents.
- A catalog row with an empty cell is never offered, for any column of any catalog. Is "never offered" the rule the user wants for an unrated part, or "offered and not evaluated"?
- The naming (`Buck`, `BuckStage`, `FeedbackDivider`, the catalog names) and the board's four joins for a buck are choices the user may want different.

## Update: an equation narrows a field that has a range

A first version took the stated window at its upper end, because an equation on a field that already had a range was only verified: the LED's current stayed 5 mA to 15 mA, the resistor's power was checked at 15 mA across its largest drop, and `examples/led.nsm` had to take a 390 ohm 0603 part (C23151) where the 330 ohm 0402 part (C25104) works. That was the solver's weakness, not the design's, and it is fixed at the root in the solver's `propagate`:

- **An equation `field == expr` narrows a field that has a quantity range** to the value of `expr` when that range lies inside the field's. The equation says what the value is, so the field's set is the intersection of the two, which in that case is the value of `expr`. Evaluating `expr` is the tight evaluation of ADR 0007's `eval` (a variable that appears twice is not widened).
- **Verification is unchanged**: `expr` must lie inside the range the field was written with. A value that does not (partly or wholly outside) is not narrowed: the field keeps the range as written, so the message still names it and the numbers (`` `led.current` must stay within 5mA..8mA, but ... works out to 7.05mA..10.56mA ``), and nothing is reported about an overlap nobody wrote. A candidate that would have failed still fails, and one that passed still passes; narrowing only makes what follows, such as the power, use the real current. It cannot discard a candidate that works: the narrowed value is one the equation forces.
- **Only quantities, and only where the equation is the only `==` on the field** (or on any field joined to it by `~`). A field that two equations state is a requirement of each against the range as written (`.divider.vout == .vout.voltage` and the divider's own formula are such a pair: narrowing by one would make the other fail). Text is not narrowed.
- **A current that propagation worked out from a balance is worked out again** when its terms narrow (ADR 0007, ADR 0026): the supply's current follows the resistor's, so the balance of the net still adds up. A value a design stated is not touched by this, except that ADR 0038 narrows a stated current to what the rest of its net determines, as an equation does (a stated current is a budget).

Consequences. Currents of boards that the LED equation ties to a resistor are the real ones, not the stated windows: the power tree and the join of `examples/two_boards.nsm` now see the LED board draw 7.3 mA to 16.8 mA, not 5 mA to 20 mA. A window that nothing ties to the circuit (`.current &= 45mA..49mA` on a lone resistor) is still taken at its upper end. The solver's exhaustive-search tests take about 5 to 15 percent longer in a debug build (`parts_alone_can_overload_the_regulator`, the slowest, 41 s to 43..49 s).
