# Closed-form checks: options for the owner

Status: options, written 2026-11-09. Nothing here is implemented except check 3 (buck output ripple, `BuckStage.output_ripple`, in the guide's "Switching regulators") and check 5 (bus rise time, `I2cPullups.rise_time` and `.bus_capacitance`, `I2c.capacitance`, in the guide's "The rise time of an I2C bus"; UM10204 Rev. 7.0 was read: the rise-time limits are in its Table 11, and the bus limit is 400 pF, 550 pF in Fast-mode Plus; and its lower bound on the pull-up from the weakest driver's sink current, `I2cPullups.sink_current` and `.low_level`, `I2c.sink_current`, in "The least pull-up of an I2C bus"), and the checker now derives the bus capacitance and the least pull-up from the interfaces on the bus, so a board states only its estimate of the traces (ADR 0102, proposed; the owner's decision 2026-11-10: the checker derives the datasheet figures a board cannot reach inside a module; the buck stage's ESR is not yet derived, see that ADR's last paragraph). The language and the std library's interface are the owner's choice, so this document gives options and asks (the questions are at the end).

## What was decided

netsmith's analysis is a constraint solver plus a worst-case, static DC operating point of the picked parts (ADR 0076, `crates/netsmith-solve/src/dc/`). It does not model time: no startup, inrush, ripple, filter response, AC or heating. The guide says so for inrush: "A capacitor is open, so nothing charges" (`docs/LANGUAGE.md`, DC analysis, Limits).

On 2026-11-09 the owner decided that "simulation" for now means **closed-form relations written in the std library** (option A). The solver already evaluates a relation in the worst case over the ranges a design states. Running a circuit simulator on demand is option B and has its own issue, #306. Issue #306 asks which cheap cases belong to option A: this document is the answer to that question.

## What a closed-form check can use today

Facts about the language that decide what can be written (read from `crates/netsmith-lang/src/ast.rs`, `crates/netsmith-solve/src/eval.rs` and `std/`):

- An expression has `+`, `-`, `*`, `/` and negation (`BinOp` is those four). **There is no `ln`, `exp`, `sqrt` or power.** A formula with one of them needs the design or the part to write the number (`2.303` for the time to 90 %), or the language needs a function. That is a language choice (question 3).
- The quantity kinds a member can have include `Time`, `Frequency`, `Capacitance`, `Inductance`, `Charge`, `Temperature`. There is no kind for energy or for I²t.
- A relation holds "in the worst case". The evaluator (`eval.rs`) takes interval arithmetic and, for a variable that appears twice, checks the sign of the derivative: if the formula is monotone in it, the two end points give the exact range; if not, it cuts the range into pieces. So a formula that is monotone in each variable (a product or a quotient of positive quantities) is exact. A formula that is not monotone is still sound (the range is never too narrow) and may be wider than the truth.
- An `optional` member (ADR 0074) switches off the relations that read it until something states it. An `optional` catalog column (ADR 0078) lets a row leave the cell empty and offers the row only to a design that does not state the field. The Zener's `.temperature_shift` and `.voltage_over_temperature` (`std/parts/diode.nsm`) are the model for both.

## What already exists (so it is not proposed)

| Exists | Where | What it is not |
|---|---|---|
| Buck inductor ripple current `(vin - vout) * vout / (vin * L * fsw)`, saturation against load plus half of it, on and off time against the IC's minimums, output and input capacitance against the IC's minimum (effective, with the bias curve) | `BuckStage`, `std/parts/regulator.nsm` | Not a ripple of the output voltage: no ESR, no limit the design states. |
| DC bias derating of a capacitor and its tolerance, `.effective_capacitance` | `Capacitor`, `std/parts/passive.nsm` | The value at the DC bias of the rail, not the value while it charges. |
| A clamp's hot and cold voltage from a temperature coefficient | `Zener.voltage_over_temperature` | Only that part. |
| Clock of a bus against the limit each part states | `crates/netsmith-solve/src/busclock.rs` | A frequency bound only. No rise time, no capacitance. |
| Pull-up resistance and the current it draws | `PullUp`, `I2cPullups`, `std/bus.nsm` | No bus capacitance, no rise time, no lower bound from the sink current. |
| A TVS's clamp against what it protects | `Tvs.clamping_voltage <= .protected_limit` | `.peak_pulse_current` is a catalog column that no relation reads. |
| A fuse's current and voltage ratings, `.limit_current` | `Fuse`, `std/parts/passive.nsm` | No I²t. |
| Open-drain level checks, leakage `.ii`, output resistances `.roh`, `.rol` | `std/logic.nsm` | Levels only, no time. |

Not found in std: any member for ESR, soft-start, source impedance of a rail, pin capacitance, an ADC input or an enable threshold.

## Candidate checks

Sizes: small is one relation and no new data, medium is a relation plus new members and catalog data, large is a new concept in the model. "Worst case" says how the range is handled. Sources named with "unverified" were not re-read for this document.

| Check | Formula and its source | Inputs today | Missing | Worst case over ranges | Wrong or optimistic when | Size |
|---|---|---|---|---|---|---|
| **1. Inrush into a capacitor bank through a fuse or a TVS.** Does the first charge or a surge stay inside the part's rating? | Peak current `(V - V0) / (R_src + R_fuse + ESR / n)`. Charge dissipation `C * V^2 / (2 * R)` (derived here from `i = (V/R) * e^(-t/RC)`: unverified in a text, easy to re-derive). The fuse's melting I²t and the TVS's Ipp are the datasheet's. | `Fuse.resistance`, `.rated_current`; `Capacitor.capacitance`, `.count`; `Tvs.clamping_voltage`, `.peak_pulse_current` (a column nothing reads); `Rail.peak`. | Source resistance and cable inductance of the supply; capacitor ESR; fuse I²t (a column and a kind); a derating factor; the surge waveform. | Peak current is largest at the highest voltage and the lowest total resistance. With no source resistance stated it is unbounded, so the honest answer is a note, not a pass. | Cable inductance rings the bank to nearly twice the input (Linear Technology AN88, from memory, unverified). A datasheet Ipp is for one waveform (10/1000 µs for the SMDJ series, unverified) and a real surge differs. A resettable fuse has no I²t. | large |
| **2. RC time constant and filter corner.** How long is a delay, and where is the corner of an RC filter? | `tau = R * C`, `fc = 1 / (2 * pi * R * C)`. Textbook (for example Horowitz and Hill, *The Art of Electronics*, chapter 1; page unverified). Time to a threshold needs `ln`. | `Resistor.resistance` with tolerance; `Capacitor.capacitance` and `.effective_capacitance` (both ends of the range); `Logic.vih`, `.vil`. | Nothing about the network: the design says what the RC is for. A threshold time needs `ln` or a stated constant. | Monotone in both. Both ends matter (a delay is bounded above and below), and the range gives both. | The node is loaded by a pin's leakage (`.ii` exists) or driven through a source resistance. Capacitance moves with temperature and age: the catalog has a bias curve only. | small |
| **3. Buck output ripple.** Is the output voltage ripple inside a limit? | `dV = dI / (8 * fsw * C) + dI * ESR`. The first term is the charge balance of the capacitor: Erickson and Maksimović, *Fundamentals of Power Electronics*, chapter 2 (equation number unverified). TI's SLVA477B ("Basic Calculation of a Buck Converter's Power Stage") and TI design guides give the same two terms (read as search summaries only; their equations were not read). Adding the two terms is the conservative sum. | `BuckStage.ripple` (`dI`), `.buck.fsw`, `.output_cap.effective_capacitance` and `.output_count` (the ripple's capacitance is the AC value, the bias-derated one is the safe side). | ESR (a capacitor member and a catalog column). A limit the design states. | Worst at the highest input, the lowest `L`, the lowest `fsw`, the lowest effective capacitance and the highest ESR. Each is monotone, and `vin` appears twice in `dI` with a positive derivative. | The converter skips pulses or runs in burst at light load (the ripple is then larger: the LT8641 has such a mode). Ceramic ESL adds spikes at the switch edges. A constant-on-time or hysteretic converter sets its ripple by ESR. A parallel bank divides ESR by the count. | small (formula) and medium (ESR data) |
| **4. Soft-start and startup time.** How long does the output take to rise, and what current does that charge draw? | Soft-start by a capacitor: `t = C_ss * V_ref / I_ss`, the datasheet's own equation for the TR/SS pin (the LT8641 page was not re-read here). Without soft-start, charge-limited: `t >= C_out * V_out / (I_limit - I_load)` from `i = C * dV/dt`. The charging current is `C_out * V_out / t`. | `Buck.max_output_current`; `.output_cap.capacitance` and `.count`; the load range `-.vout.current`. The LT8641 has a `TR/SS` pin that the model leaves passive (its note says the capacitor on it is not evaluated). | `soft_start_time`, or `I_ss` and `V_ref`, on the IC; a fixed time for an LDO or a converter with an internal ramp. | Both ends matter: too fast is an inrush, too slow misses a reset or a sequence. Monotone in each. The charging current needs the **marked** capacitance at its high end, not the bias-derated one: the bias is small while the output rises, so the derated value is optimistic here. | A load that draws a fixed current or has no undervoltage lockout can stall the rise, and there is no closed form for that. A foldback current limit, a pre-biased output and an input that sags are not in the formula. | small to medium |
| **5. Bus rise time against bus capacitance.** Does an I2C line reach its high level in time? | `tr = 0.8473 * Rp * Cb` between 30 % and 70 % of the supply. The constant is `ln(7/3)`, checked here. The limits are the I2C-bus specification's, UM10204 section 7 and its timing table: 1000 ns standard mode, 300 ns fast mode (both seen in secondary sources only), 120 ns fast-mode plus (unverified), and `Cb <= 400 pF` (unverified). | `Resistor.resistance` with tolerance in `I2cPullups`; `I2c.frequency`. The bus has no capacitance, no rise time and no mode table (`busclock.rs` checks frequency only). | Pin capacitance of each part on the bus (datasheets list Ci). Trace and connector capacitance of the board. The sink-current lower bound on `Rp`, `(Vdd - VOL) / IOL`: `.rol` exists, `IOL` does not. | Monotone: worst at the highest `Rp` and the highest `Cb`. | `Cb` is underestimated (traces, connectors and cable are the large part). Two pull-up pairs on one bus are in parallel. A current-source pull-up. A 1.8 V bus whose 30 % and 70 % points differ. | small (formula) and medium (data) |
| **6. ADC input: source impedance against the sampling capacitor.** Does the input settle within the acquisition time? | `t_acq >= (R_src + R_sw) * C_s * k`, with `k` set by the error budget (`ln(2^(N+1))` for half an LSB). The exact form differs by vendor: TI's SPRACV0A describes the model (read as a search summary), and STM32 datasheets give a limit on `R_AIN` with `ln(2^(N+2))` (from memory, unverified). So the part must state its own form. | None. No part has an analog input, and `Logic` is digital. `FeedbackDivider` shows how a resistor pair is read. | `C_s`, `R_sw`, `t_acq`, resolution, ADC clock, all per MCU; the Thevenin resistance of the source network. | Monotone. The Thevenin resistance of a divider repeats variables, which the evaluator handles. | An external capacitor on the pin that acts as a charge bucket changes the model. Input leakage adds an offset the formula does not have. | medium |
| **7. Power-up sequencing.** Does rail A come up before rail B? | Compare two times, each from check 2 or 4. | Nothing. | The enable and power-good thresholds, and the times of check 4. | Not exact: two relations that share the input range are judged on independent ranges, so the ordering looks worse than it is. | See below. | large |

### What was dropped, and why

- **General power-up sequencing (7).** It is not a formula of its own: it is an ordering of the times of checks 2 and 4, and it is only as good as them. Keep it for after check 4 exists. The shared-input correlation (both rails depend on one `vin`) is not tightened across two relations.
- **Filter response beyond one pole.** An LC filter's corner `1 / (2 * pi * sqrt(L * C))` and its peaking need `sqrt` and a damping factor that the load and the ESR set. An input filter that interacts with a converter's negative input resistance (Middlebrook's criterion) is a frequency-domain argument over the loop. Neither is a closed form the model has the inputs for.
- **Load transient and loop stability** of a regulator. The datasheets give plots, not a formula, and the model says "the converter's stability and its loop are not modelled" (`BuckStage`).
- **Ringing and EMC.** They depend on parasitic inductance that no part states.
- **Junction temperature** `Tj = Ta + P * theta_JA` fits a closed form, since `.max_power` exists. It is not studied here: `theta_JA` is the datasheet's, measured on the maker's test board, so it is optimistic for a real one, and it is not a simulation question. A separate question if wanted.

Everything that does not fit in a formula (a regulator's response, a waveform, a startup into a nonlinear load) is for a simulator, issue #306.

## Recommendation: the first three

1. **Buck output ripple (3).** The stage already has `dI`, `fsw`, the output bank and the effective capacitance, so one new capacitor member (ESR) and one limit give a check on the most common converter in a design. It is exact in the worst case and needs no new language.
2. **Soft-start and startup time (4), with its charging current.** It is the first check that answers the inrush question for a board that has a converter, with the data on the IC and no source impedance (the hard unknown of check 1). It also gives the first time value for sequencing later.
3. **Bus rise time (5).** The formula is one line with a derived constant. The pull-ups and the clock are already in std. The new data is a pin capacitance that each part states, and a bus capacitance that the board states.

Check 2 (RC) is the cheapest, but no std part is an RC: it fits as a small module later (`RcDelay` with a resistor and a capacitor, like `PullUp`). Check 1 is the one the owner's real 12 V automotive board needs most, and the one with the most unknowns: it waits on a source impedance. Check 6 waits on per-MCU data.

## Two shapes for stating and reading a check

Each example is three lines of `.nsm`. They are written to parse and are **not tested**. The member names are placeholders.

### Shape (a): members and relations on the std part

The part carries the datasheet's number as a member and the equation as a relation, like `.voltage_over_temperature` on the Zener. A design reads the result and bounds it.

Buck output ripple, in `BuckStage` (std), then the design:

```nsm
.output_ripple: Voltage
.output_ripple == .ripple / (8 * .buck.fsw * .output_count * .output_cap.effective_capacitance) + .ripple * .output_cap.esr / .output_count
.stage.output_ripple <= 20mV
```

Soft-start charging current, in `BuckStage` (std) with `.soft_start_time: Time optional` on `Buck`, then the design:

```nsm
.startup_current: Current
.startup_current == .output_count * .output_cap.capacitance * .vout.voltage / .buck.soft_start_time
.startup_current + -.vout.current <= .buck.max_output_current
```

Bus rise time, in `I2cPullups` (std) with `.capacitance: Capacitance optional` on `I2c`, then the design:

```nsm
.rise_time: Time
.rise_time == 0.8473 * .resistance * .bus.capacitance
.pullups.rise_time <= 300ns
```

Cost of (a):

- Each check adds members to a std definition, so each is a std interface change (the owner's, and `docs/LANGUAGE.md` and its tested examples change with it).
- The number comes from a datasheet and lives in the part, as `docs/DESIGN-RULES.md` asks ("a part states its own requirements"). A design states only its limit.
- The relation reads an optional member, so a design that states no ESR gets **no ripple check and no note** (ADR 0074). That is silent. The alternative is a non-optional member, which makes every buck stage ask for an ESR it has no row for today, and an unstated-value note on each (question 5).
- Work: the relation and its tests are small. The data is the cost (next section).

### Shape (b): the board writes the relation itself

No std change. The board declares the value it knows and states the relation with the existing members, as the `PinClamp` example in the guide does for the Zener. This works today for the bus (the capacitance is the board's own estimate):

```nsm
.bus_capacitance: Capacitance
.bus_capacitance &= 60pF..120pF
0.8473 * .pullups.resistance * .bus_capacitance <= 300ns
```

Cost of (b):

- Nothing to build. It works for any check whose inputs a board already has (the bus rise time, the RC delay), and it can be tried at once on the real board.
- Every board repeats the formula and its constant, and the source of the constant is in a comment, not in the library. That is two spellings of one rule if std later gets it (design rule "one correct way"), and a part's datasheet number (Ci, ESR) has nowhere to live, so the board guesses it.
- It cannot read a number a part would carry. The ripple and the soft-start examples above cannot be written this way: `esr` and `soft_start_time` are not members.
- Shape (b) is not the same as a `check` statement or a new kind of block for time. That would be a language addition and is not proposed here.

### Recommendation

Use (a) for the three checks. Their numbers are datasheet numbers, and the rules say the part states them. Use (b) at once, with no change, for the one case it covers, the bus, while (a) is decided: it costs a board three lines and gets the check this week.

## What a catalog row or part would have to state

None of these is in std or in a catalog today. A catalog number goes in only with a recorded source (`data/jlcpcb/README.md`, and the rule in `docs/DESIGN-RULES.md`: "A part number needs a source"). An empty cell means unknown, never zero, and a design that states the field is offered only rows that fill it (ADR 0078).

| Datum | Where | Source it would need | Used by |
|---|---|---|---|
| Capacitor ESR | `capacitor.csv` column `esr`, optional | Manufacturer's table or tool, with the frequency, the DC bias and the temperature it was read at, in `source` (for example the Samsung component library for the 39 Samsung rows; not read for this document). A datasheet "typical" is not the worst case: the maximum is wanted. | 1, 3 |
| Soft-start time, or `I_ss` and `V_ref` | on the IC's `Buck` (or on a part like `LinearRegulator`) | The IC datasheet's table or equation, with the page: the same rule as the LT8641's other numbers. | 4 |
| Pin capacitance `Ci` | optional member of `I2c` (and of other interfaces later) | The part's datasheet table, with the page; a part that gives only a typical states the maximum if one exists, else nothing. | 5 |
| Rail source resistance and surge | member of `Rail`, stated by the design | The design's own supply, not a catalog: a design states it as it states `.peak`. A vehicle's range is an ISO 16750-2 figure (unverified here). | 1 |
| Fuse I²t and Ipp waveform | `fuse.csv` and `tvs.csv` columns | The datasheet's table. The notes of the fuse rows already hold the pre-arcing limits, which are not columns. | 1 |
| `C_s`, `R_sw`, `t_acq`, resolution | optional members of an ADC interface of an MCU | The MCU datasheet's ADC table, with the page. | 6 |

The coverage on the first day is zero rows. The first rows to fill are the ones a real board uses, each read from its datasheet, and a row whose figure could not be read leaves the cell empty. A catalog tool that fills a column from a vendor's web tool (not a PDF) needs the owner's rule on what counts as a recorded source (question 7).

## Questions for the owner

Each is answerable in one word or one choice.

1. First three checks: buck output ripple, soft-start and startup, bus rise time. Accept, or name others?
2. Shape: (a) members on std parts, (b) the board writes the relation, or both as recommended?
3. Expression language: add functions (`ln`, `sqrt`), or keep the four operators and write constants such as `2.303`? (`functions` or `constants`.)
4. A source resistance on `Rail` (and a surge range) so that check 1 can be written: yes, no or later?
5. A check that reads an `optional` member with no value: stay silent (ADR 0074, as the Zener's shift) or give an unstated-value note? (`silent` or `note`.)
6. Where does the ripple limit live: `BuckStage.output_ripple`, or on the output `Rail` (a requirement of the rail)? (`stage` or `rail`.)
7. May ESR and `Ci` be read from a vendor's web tool, with the tool, the conditions and the date as the recorded source? (`yes` or `no`.)
8. A kind for I²t (ampere-squared seconds) and for energy, for fuses: add now, or not before check 1?
9. A mode table (standard, fast, fast-mode plus, with their `tr` and `Cb` limits) in `std::bus`: yes or no?
10. Drop general sequencing until startup time exists: yes or no?
