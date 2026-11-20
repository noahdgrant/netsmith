## The environment of a board

A board can say what it is exposed to: the member `.environment` (ADR 0099, **provisional**: the owner has accepted the recommended answers of `docs/research/environment-and-grade.md`, and this stage is the temperature and the qualification). Every fitted part is then checked against it. The board has the member as it has its `.process`, an interface of the standard library, `std::env::Environment`, whose fields are the range of temperatures the board's surroundings reach and, if the board asks for it, the qualification its parts must have (see Qualification, below).

A temperature is written in degrees Celsius or kelvin, `-40degC..125degC` or `233.15K..398.15K` (the same range, ADR 0103; see Temperatures under Values), and a message shows both. `C` alone is the coulomb, so `-40C` is a charge and is refused where a temperature is wanted.

A part states the range it works at in two built-in members of every part, `.temp_min` and `.temp_max` (the lowest and the highest temperature, each one temperature), or its catalog row does, in the optional fixed columns of the same names (see Catalog files). The check is two comparisons: the hot end of the environment is at or under the part's `.temp_max`, and the cold end at or over its `.temp_min`.

```nsm evaluate
import std::env::Environment
import std::parts::passive::Crystal
import std::sources::Rail

board Clock {
    .environment: Environment { .temperature &= -40degC..85degC }
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC" }
    .xtal: Crystal { .frequency &= 16MHz +/- 30ppm }
    .xtal.a ~ .supply.pos
    .xtal.b ~ .supply.neg
}
```

The standard crystal rows state -40 C to 85 C (the range of the JLCPCB listing, which their frequency figures are given over), so every one of them is offered. The same board for 125 C has none that covers it:

```nsm fails
import std::env::Environment
import std::parts::passive::Crystal
import std::sources::Rail

board Hot {
    .environment: Environment { .temperature &= -40degC..125degC }
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC" }
    .xtal: Crystal { .frequency &= 16MHz +/- 30ppm }
    .xtal.a ~ .supply.pos
    .xtal.b ~ .supply.neg
}
```

```text
error: the `crystal` catalog has nothing for `xtal`, which needs frequency within 15.9995MHz..16.0005MHz (15 rows were left out because the temperatures they state do not cover the board's environment, `.environment.temperature` 233.15 K to 398.15 K (-40degC to 125degC): a row's `temp_min` and `temp_max` are its datasheet's)
  --> main.nsm:8:5
  |
8 |     .xtal: Crystal { .frequency &= 16MHz +/- 30ppm }
  |     ^^^^^^^^^^^^^^
```

### Named environments

A board need not write its range. `std::env` has named environments, each an interface with the field `.temperature` and the range offered with `?=`, so a board that declares one has the range and can narrow it:

| Name | Range | Where it comes from |
|---|---|---|
| `Automotive` | -40degC..125degC | AEC-Q100 Grade 1, ambient operating temperature range |
| `AutomotiveGrade0` | -40degC..150degC | AEC-Q100 Grade 0, the same table |
| `AutomotiveGrade2` | -40degC..105degC | AEC-Q100 Grade 2, the same table |
| `AutomotiveGrade3` | -40degC..85degC | AEC-Q100 Grade 3, the same table |
| `Industrial` | -40degC..85degC | a convention of manufacturers, not a standard |
| `Commercial` | 0degC..70degC | a convention of manufacturers, not a standard |

The four `Automotive` ranges are the part operating temperature grades of AEC-Q100 (Automotive Electronics Council, AEC-Q100-REV-J, August 11, 2023, section 1.3.4, Table 1, page 3 of 44, "Ambient Operating Temperature Range"), and `Automotive` is Grade 1 because the owner chose it as the default grade: the document marks none as the usual one. They are the ranges and nothing more: they do not say a board or its parts were qualified to AEC-Q100, and AEC-Q101 and AEC-Q200 (discrete semiconductors and passives) were not read. `Industrial` and `Commercial` are conventions: no document defines them, and the one page that states them which was read, Renesas's "Temperature Ranges" (read 2026-11-10), lists them as -40 to +85 and 0 to +70 and does not say whether they are ambient or junction. The doc comment of each definition in `std/env.nsm` says the same. A project that needs another range writes its own definition (see The manifest's default).

```nsm evaluate
import std::env::Automotive
import std::parts::passive::Resistor
import std::sources::Rail

board Dash {
    .environment: Automotive {}
    .supply: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC" }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .supply.pos ~> .r1 ~> .supply.neg
}
```

A board narrows the offer with `&=`, as it does a process (`.environment.temperature &= -20degC..90degC` keeps the board inside its parts' ranges and no wider than the grade). A stated range replaces the offer and is not intersected with it (ADR 0051), so `&= -55degC..150degC` is a wider environment than `Automotive`.

Rules.

- **A row that states a range the environment is not inside is not offered.** The solver tries the next row, and says in its message how many it left out and why.
- **A row that states none is offered, and named.** A pick whose catalog row has no `temp_min`, no `temp_max` or neither is kept, because an empty cell is unknown and not a refusal, and a guideline finding says so: a `warning:` line by default, an error with `--strict` or `guidelines = "error"` in `netsmith.toml` (the same class as the unconstrained pick, ADR 0065). A check that could not be made is never a pass. The finding names the part, the row, which temperature is missing and what to write; parts picked as one row are one finding. The standard rows that state a range are the ones read from a datasheet (the resistors, capacitors, ferrite beads, fuses, headers, Murata inductors, crystals, most diodes, the AO3400A and the AO3401A, the 1N5819WS, the XC6206 and twelve LEDs; `data/jlcpcb/README.md` says which and from what); the others, such as the two LEDs whose datasheet does not name the part, the B5819W SL and the 2N7002, state none. Some of the figures are junction ranges (below).

```nsm solve warns
import std::env::Environment
import std::parts::passive::{Led, Resistor}
import std::sources::Rail

board Probe {
    .environment: Environment { .temperature &= -40degC..85degC }
    .supply: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC" }
    .led: Led { .current &= 5mA..15mA }
    .r1: Resistor {}
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
```

```text
Probe: environment guideline: `led` (Led) was picked as KT-0603R RED LED 0603 C2286 and its catalog row states no `temp_min` and no `temp_max`, so it is not judged against the board's environment (`environment.temperature` 233.15 K to 358.15 K (-40degC to 85degC)): add the missing temperature of the datasheet to the row (`temp_min`, `temp_max`, such as `85degC`) or state it on the part, as `.led.temp_max &= 85degC` for a datasheet that says 85degC
```

- **The design can state it instead.** A part that states `.temp_min` or `.temp_max` in a board, a module or its own definition speaks for itself, whatever its row says (as a stated `.height` does, ADR 0094), and a part with both ends stated is judged and not named. A range stated as a range (`.temp_max &= 77degC..87degC`) is read at its worse end: the lowest of the highest temperature, the highest of the lowest.
- **A stated range that does not cover the environment is an error** that names the part, the number and where it came from:

```nsm fails
import std::env::Environment
import std::parts::passive::{Led, Resistor}
import std::sources::Rail

board Warm {
    .environment: Environment { .temperature &= -40degC..85degC }
    .supply: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC" }
    .led: Led { .current &= 5mA..15mA }
    .led.temp_min &= -40degC
    .led.temp_max &= 75degC
    .r1: Resistor {}
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
```

```text
error: Warm: `led` works up to 348.15 K (75degC) only (the design states `.led.temp_max`), under the board's environment, which reaches 358.15 K (85degC) (`environment.temperature` 233.15 K to 358.15 K (-40degC to 85degC)): choose a part rated for it or narrow the environment
```

- **A row that names a dielectric class and states no range is judged by the class's range.** A capacitor row with a `dielectric` cell (`X5R`, `X7R`, `C0G`) and no `temp_min` or `temp_max` of its own takes the operating temperature range that makers print for that class code (X5R -55degC to 85degC, X7R and C0G -55degC to 125degC; the documents are cited in the doc comment of `ClassRange` in `crates/netsmith-check/src/environment.rs`), at the end its cells leave empty. A class range is a property of the class code and not of a particular part, which may be rated narrower, so the row's own cell, or the design's statement, always wins at its end, and every message that rests on a class range says "from the class X5R, per ..." and not that the datasheet states it. A class with no cited range (none in the standard rows) leaves the row unjudged and named, as a row with no class does. A project's own row can use it: the cell is read from any catalog that has a `dielectric` column of the class names. Here the row states a class and nothing else, and 85degC is inside X5R's range while 125degC is not:

```csv file=catalogs/capacitor.csv set=class_only
mpn,capacitance,effective_capacitance,max_voltage,package,dielectric,source
ACME-C33N3,33.3nF +/- 10%,derate capacitance by bias:,16V,0402,X5R,https://example.invalid/acme-c33n3.pdf
```

```nsm fails catalogs=class_only
import std::env::Environment
import std::parts::passive::Capacitor
import std::sources::Rail

board Hot {
    .environment: Environment { .temperature &= -40degC..125degC }
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "3V3" }
    .c: Capacitor { .capacitance &= 33.3nF +/- 10% }
    .supply.pos ~> .c ~> .supply.neg
}
```

```text
error: the `capacitor` catalog has nothing for `c`, which needs capacitance within 29.97nF..36.63nF (15 rows were left out because the temperatures they state do not cover the board's environment, `.environment.temperature` 233.15 K to 398.15 K (-40degC to 125degC): a row's `temp_min` and `temp_max` are its datasheet's; a row that states no temperature of its own is judged by the range of its dielectric class (X5R), which is the class's and not its datasheet's: add `temp_min` and `temp_max` to the row to say its own)
  --> main.nsm:8:5
  |
8 |     .c: Capacitor { .capacitance &= 33.3nF +/- 10% }
  |     ^^^^^^^^^^^^^
```

- **A board with no environment is not judged** and says nothing: no check, no finding. The same holds for a board of a project whose manifest names none.
- **A board that has an environment that states no temperature** (`.environment: Environment { }`) is told so, and no part is judged.
- **Parts that are not fitted** (`.assembly` is `AssemblyMode::None`: a mounting hole, a fiducial) and labels are not judged. A part soldered by hand or consigned is, because it sees the same air.
- **Each board of a system has its own environment.** A system does not state one.
- **A junction range is a part's range.** A part whose datasheet gives a junction temperature range (`TJ`, in the Schottky, TVS and Zener diodes and the AO3400A and AO3401A of the standard catalogs, each row's `notes` say so) and no ambient one is judged by that figure as it stands: a junction range that covers the environment is a pass. The check does not compute how far the part's own heating lifts its junction above the air, so this is the limit of it: a part whose junction range just covers the environment can still be too hot at its junction in that air, and the self-heating margin is a design's to check.
- **A Zener's temperature shift follows the environment.** A board that bounds a Zener's `.voltage_over_temperature` and states no `.temperature_shift` is given the environment's range less 25 C as the shift (see _Voltage across a part_); a shift it states overrides it.
- **What it proves.** The check says the part works at the air temperature of the board. It does not say which temperature a datasheet's range is (ambient, junction or operating: the row's `notes` say what the record prints), it adds no heat of the part's own, and a rating at 25 C (`.max_power` of a resistor, a fuse's hold current) is not derated by it. A part that works at 125 C does not carry its 25 C rating there. Radiation, outgassing, screening and lot traceability are not checked at all.
- **A generic part is judged when `fab` finds its row.** A `Resistor`, `Capacitor` or `Led` named by its value has no catalog row when the board is built (ADR 0047), so the build names it as not judged. `fab` resolves it to the row it would order and judges that row with the same rule: a row inside the environment is met and says nothing, a row outside it stops `fab` before anything is placed or written (the message names the part, the row and its LCSC number, the figures and the environment), and a row that states no range is named and counted as unjudged. A board with no environment is not judged here either.
- **`fab-notes.txt` says what was checked.** A board that has an environment gets two lines there: how many fitted parts were compared with it and met, how many were not met (a part that is not met stops the build and `fab`, so the count is zero in a package that was written), how many were not judged (a range stated at one end only or at neither, a part with no catalog row, a generic part no row stands for), and that radiation, outgassing, screening and lot traceability, and the part's own heating, are not checked. A board with no environment gets neither line.

### Qualification

A board can also require a qualification of every part it fits: `.environment.qualification &= Qualification::Aec`, written like any variant of an enum (ADR 0054). `Qualification` is in the prelude, so it needs no import, and it has two variants: `Aec` (the datasheet says the part is qualified to an AEC standard of its kind: AEC-Q100 for integrated circuits, Q101 for discrete semiconductors, Q200 for passives) and `None` (the datasheet says the part's ordering code is not qualified). It names a family and not a standard: the catalog row's `notes` say which AEC document its datasheet states, with the page, and the enum has no variant for a grade. Nothing is derived from a part's temperature range and nothing the other way: a part's printed range can exceed its grade.

A part states what its datasheet says in the built-in member `.qualification` of every part, or its catalog row does, in the optional fixed column `qualification` (`Aec`, `None`, or empty; see Catalog files). A part is judged only against a stated requirement, and in the same three outcomes as a temperature range:

- **Met.** The part's `.qualification` or its row says `Aec`. Nothing is named.
- **Not met.** The part or its row says `None`. A row that says it is not offered, and the solver's message says how many rows were left out and why; a part whose own `.qualification` says `None` is an error that names the part and where it was stated.
- **Unjudged.** Nothing states it, which is unknown and not a refusal. The part is kept and a guideline finding names it, with what to add (a `warning:` by default, an error with `--strict`, as for a range). A generic part with no row yet is unjudged in the same way until `fab` finds the row that stands for it.

A board that states no qualification requirement, or states `Qualification::None`, which asks for nothing, is not judged by it and says nothing: every board that does not write the line has the same findings as before. The named environments of `std::env` state no qualification, so `Automotive` (a temperature range) does not by itself ask for one: a board that wants both writes `.environment: Automotive { .qualification &= Qualification::Aec }`.

A row says `Aec` only where its datasheet or the maker's product page says so, and the row's `source` has the document, the page and the day. Two standard rows state it today: the prosemi `1206HV-3A` fuse (`Aec`, AEC-Q200, "AEC-Q200 Automotive Grade Certified", datasheet page 1) and the Vishay `SS3H10-E3/9AT` Schottky diode (`None`: its datasheet says the E3 ordering code is the commercial grade and the AEC-Q101 qualified grade is the HE3 code, which is another row). Every other standard row says nothing, so a board that requires `Aec` has them named.

```nsm solve warns
import std::env::Environment
import std::parts::passive::Resistor
import std::sources::Rail

board Dash {
    .environment: Environment {
        .temperature &= -40degC..85degC
        .qualification &= Qualification::Aec
    }
    .supply: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VCC" }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .supply.pos ~> .r1 ~> .supply.neg
}
```

```text
Dash: environment guideline: `r1` (Resistor) was picked as 1kΩ 1% 0402 C11702 and its catalog row states no `qualification`, so it is not judged against the board's environment, which requires `Qualification::Aec` (`environment.qualification`): add `qualification` of the datasheet to the row (`Aec` where it says the part is qualified, `None` where it says it is not) or state it on the part, as `.r1.qualification &= Qualification::Aec` for a datasheet that says the part is AEC qualified
```

A row that says `None` is left out. Here only the `SS3H10-E3/9AT` row is a Schottky diode of 100 V and 3 A, and it says it is not qualified:

```nsm fails
import std::env::Environment
import std::parts::diode::Schottky
import std::sources::Rail

board Rectifier {
    .environment: Environment {
        .temperature &= -40degC..85degC
        .qualification &= Qualification::Aec
    }
    .supply: Rail { .voltage &= 12V +/- 5%; .pos.net &= "VIN" }
    .d: Schottky { .max_reverse_voltage &= 100V; .max_forward_current &= 3A }
    .supply.pos ~> .d ~> .supply.neg
}
```

```text
error: the `schottky` catalog has nothing for `d`, which needs max_reverse_voltage within 100V and max_forward_current within 3A (1 row was left out because it says it is not qualified (`None`) and the board's `.environment.qualification` requires `Aec`: a row's `qualification` is its datasheet's)
   --> main.nsm:11:5
   |
11 |     .d: Schottky { .max_reverse_voltage &= 100V; .max_forward_current &= 3A }
   |     ^^^^^^^^^^^^
```

What it proves. The check says what the datasheet of the ordering code says and nothing about the board: a part qualified to AEC-Q200 is not shown to have passed any other test, and the sentence of a datasheet is not a certificate. The `Qualification` of a stricter standard is not a stronger claim: there is one variant, `Aec`, so a board cannot ask for AEC-Q100 Grade 0 by this member (the temperature range of the part says what a grade says).

### A rating that falls with temperature

The board's `.environment.temperature` is its operating temperature range, and it is the one range that every check which depends on how hot or cold the board is reads. One such check is a rating that falls when the part is hot (issue #1257). Some parts (a supercapacitor, a cell, some capacitors) are rated for less voltage when hot, and their datasheets print it in steps, such as 5.5 V up to 70 C and 4.2 V up to 85 C. A part states it the same way, as its `.max_voltage` in steps over temperature: each step is a voltage, `to`, and the temperature it holds up to and at, with the steps written from the coldest and separated by commas.

```nsm evaluate
import std::env::Environment
import std::sources::Rail

/// A supercapacitor of an invented datasheet, rated 5.5 V up to 70 C and 4.2 V up to 85 C.
part Supercap {
    .designator &= "C"
    pins {
        .a passive "+" "1"
        .b passive "-" "2"
    }
    /// The voltage across it, which the rail it sits across gives it.
    #[role(bias)]
    .bias: Voltage
    .max_voltage: Voltage
    .max_voltage &= 5.5V to 70degC, 4.2V to 85degC
    bridge .a ~> .b
}

board Backup {
    .environment: Environment { .temperature &= -20degC..85degC }
    .supply: Rail { .voltage &= 3.6V +/- 5%; .pos.net &= "VBAK" }
    .hold: Supercap {}
    .supply.pos ~> .hold ~> .supply.neg
}
```

The voltage across the part is its `#[role(bias)]` member, which the checker gives the voltage of the rail it sits across (a `Capacitor` has one, `.bias`; a part of a project's own that states steps declares it, with a `bridge` between the two pins, as above). Each step that the board's range reaches must hold the highest of that voltage, here 3.78 V, which is within both 5.5 V and 4.2 V. The same part on a rail of 5 V is within its rating up to 70 C and not above it, and the error names the temperature:

```nsm fails
import std::env::Environment
import std::sources::Rail

/// A supercapacitor of an invented datasheet, rated 5.5 V up to 70 C and 4.2 V up to 85 C.
part Supercap {
    .designator &= "C"
    pins {
        .a passive "+" "1"
        .b passive "-" "2"
    }
    /// The voltage across it, which the rail it sits across gives it.
    #[role(bias)]
    .bias: Voltage
    .max_voltage: Voltage
    .max_voltage &= 5.5V to 70degC, 4.2V to 85degC
    bridge .a ~> .b
}

board Backup {
    .environment: Environment { .temperature &= -20degC..85degC }
    .supply: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VBAK" }
    .hold: Supercap {}
    .supply.pos ~> .hold ~> .supply.neg
}
```

```text
error: Backup: `hold` is rated for 4.2 V only above 343.15 K (70degC), up to 358.15 K (85degC) (the design states `.hold.max_voltage &= 5.5V to 70degC, 4.2V to 85degC`), under the 5.25 V across it (`hold.bias`), and the board's environment reaches 358.15 K (85degC) (`environment.temperature` 253.15 K to 358.15 K (-20degC to 85degC)): choose a part rated for it, lower the voltage or narrow the environment
```

Rules.

- **A step holds up to and at its temperature.** `5.5V to 70degC, 4.2V to 85degC` is 5.5 V at any temperature up to 70 C, and 4.2 V above 70 C up to 85 C. Each value is one voltage and each temperature one temperature, as the datasheet prints them (`85degC` or `358.15K`; `85C` is a charge), and the temperatures rise from the first step to the last. Above the last step the part is not rated at all, so a board whose range reaches past it is an error.
- **Only the steps the board reaches are judged.** A step is reached when the board's range overlaps the temperatures it covers: the same part on a board of `-20degC..70degC` is judged by its first step alone, and holds 5 V. The cold end is the part's `.temp_min`, which the steps do not replace.
- **`.max_voltage` is the greatest step.** The statement narrows the member to its greatest value, the datasheet's maximum, so a relation that reads `.max_voltage` (a converter's rating check) reads 5.5 V and is not told the temperature; the steps are judged by this check, which is.
- **The voltage is the rail's steady voltage.** It is the voltage across the part, the rail's `.voltage`, not its transient `.peak`: a design that needs the peak within the hot rating states a relation of its own. A diode, whose voltage across it has a sign (`bridge_voltage`), is judged by its size.
- **What cannot be judged is named.** Steps on a board that states no `.environment.temperature`, or on a part whose voltage across it is not known (a part across no rail), are kept, and a guideline finding names the part and what to state. A part whose definition has no voltage across it at all (no `#[role(bias)]` member and no `bridge`) is an error.
- **Only a part's `.max_voltage` takes steps.** Steps elsewhere (another member, a relation, a value inside arithmetic) are refused with a pointer to this form, and so is a second statement of steps for the same part.
- **A catalog row can state them.** A `max_voltage` cell may be written in steps, quoted in the CSV for the commas (see Catalog columns, below). The column then holds the greatest step, and a row whose steps do not hold the voltage across the part at a temperature the board reaches is not offered: the solver tries the next row and says how many it left out and why. Steps stated by the design speak for the part, whatever its row says.

### Named corners

A capacitor is not the same part over its life or its temperatures: by the end of the life its maker rates it for it has lost some of its capacitance and its ESR has grown, and its ESR is higher when it is cold. A board says at which conditions it must still work by stating its corners (issue #1256), each a member of the type `std::env::Corner`: one temperature, `.temperature`, and a point of life, `.life`, which is `Life::New` or `Life::End` (both from `std::env`). The member's name is the corner's, and every message about the corner uses it.

```nsm-fragment
.new: Corner { .temperature &= 25degC; .life &= Life::New }
.eol_cold: Corner { .temperature &= -20degC; .life &= Life::End }
```

Every bound (`<=`, `>=`) that reads a capacitor's `.effective_capacitance` or `.esr`, or a value worked out from them, is judged again at each corner: a bank's `.total_effective_capacitance`, the capacitance a regulator asks for (`.output_cap.total_effective_capacitance >= .ic.min_output_capacitance`), a buck stage's `.output_esr` and `.output_ripple`, and a bound of the board's own. At a corner a capacitor has:

- **its effective capacitance** (after the DC bias and the tolerance), times its `.capacitance_at_end_of_life` at `Life::End`;
- **its ESR** read at the corner's temperature off its row's curve file over `temperature` (see _Curve files_ in the catalogs chapter), times its `.esr_growth` at `Life::End`. A row with no curve gives one figure, which its datasheet states at 25 C, so it stands at a corner of 25degC only; an `.esr` the design states speaks for every temperature.

`capacitance_at_end_of_life` and `esr_growth` are optional columns of the capacitor catalog and optional members of `Capacitor` and `CapacitorBank`, each a plain number from the maker's endurance test: `0.7` for "capacitance change within -30 % of the initial value", `2` for "ESR not more than 200 % of the initial specified value". A curve file of ESR over temperature is `curves/capacitor/<row>.csv` beside the catalog, with the axis `temperature` and the value `esr`:

```text
temperature,esr
-40degC,200mohm
25degC,20mohm
85degC,15mohm
```

No standard row states any of them yet, because none has a recorded source (`data/jlcpcb/README.md`); a project adds them to its own rows. This file is invented, like the file of the catalogs chapter: a 100 uF capacitor that keeps half of its capacitance and has three times its ESR at the end of its life.

```csv file=catalogs/capacitor.csv set=life
mpn,capacitance,effective_capacitance,max_voltage,package,esr,capacitance_at_end_of_life,esr_growth,source
ACME-C100U,100uF +/- 20%,derate capacitance by bias: 0:1 16:1,16V,1210,20mohm,0.5,3,https://example.invalid/acme-c100u.pdf page 4
```

A bank that must keep 66 uF is one capacitor for the row's own figures (80 uF at the least) and two at the end of its life (40 uF each at the least), and the solver picks two, because the trial of each candidate judges the bounds at every corner too:

```nsm evaluate catalogs=life
import std::env::{Corner, Environment, Life}
import std::parts::passive::CapacitorBank
import std::sources::Rail

board Backup {
    .environment: Environment { .temperature &= -20degC..85degC }
    .new: Corner { .temperature &= 25degC; .life &= Life::New }
    .eol: Corner { .temperature &= 25degC; .life &= Life::End }
    .supply: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VBAK" }
    .bulk: CapacitorBank { .capacitance &= 100uF +/- 20% }
    .supply.pos ~> .bulk ~> .supply.neg
    .bulk.total_effective_capacitance >= 66uF
}
```

A bound that no candidate holds at a corner is an error that names the corner and the values it read there. Here the ESR is 20 mohm for the row's figures and 60 mohm at the end of life:

```nsm fails catalogs=life
import std::env::{Corner, Environment, Life}
import std::parts::passive::Capacitor
import std::sources::Rail

board Backup {
    .environment: Environment { .temperature &= -20degC..85degC }
    .eol: Corner { .temperature &= 25degC; .life &= Life::End }
    .supply: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VBAK" }
    .hold: Capacitor { .capacitance &= 100uF +/- 20% }
    .supply.pos ~> .hold ~> .supply.neg
    .loss: Resistance
    .loss == .hold.esr
    .loss <= 50mohm
}
```

```text
error: no `capacitor` candidate satisfies `hold`: the closest of the 1 that fit, ACME-C100U, fails because at the corner `eol` (298.15 K (25degC), at the end of life), loss must always be at most 50mΩ, but 60mΩ is not always below 50mΩ (there `hold.esr` is 60mΩ) (rows with an empty cell are never offered, since a part with no stated rating cannot be shown to stay within it: 33 for `esr`)
   --> main.nsm:13:5
   |
13 |     .loss <= 50mohm
   |     ^^^^^^^^^^^^^^^
```

Rules.

- **A corner is one temperature inside the board's environment, with a life.** A range, a temperature outside `.environment.temperature`, or a corner with no `.life` is an error that names the corner. A board with no environment may state corners at any temperature.
- **Only a board states corners.** A `Corner` in a module is an error; each board of a system has its own.
- **The row's own figures are judged as before.** A corner is a check more, never one less: a board that states no corner is judged as it always was, and the check of the row's figures reads the ESR over the whole environment where the row has a curve.
- **What a corner cannot read is named, never passed.** A row that states no `capacitance_at_end_of_life` or `esr_growth` at an end-of-life corner, or no curve at a corner away from 25degC (or one that does not reach it), leaves each bound that reads it unjudged at that corner. The pick goes on, and the build lists the bound as not evaluated, with the corner, the part and what to add (`at the corner `eol` (298.15 K (25degC), at the end of life), `kept >= 66uF` is not evaluated: `c` states no `capacitance_at_end_of_life` ...`).
- **Only bounds are judged at a corner.** An equation whose value a corner moves is worked out again from the corner's values, and is not held against a window stated on the same field. The marked `.capacitance` (a bill of materials' value) is not moved.

### The manifest's default

`netsmith.toml` can name the environment every board of the project has when it declares none (the key `environment` under `[project]`): the definition of an interface with the field `.temperature`, written as `module::Name`, where the module is a standard one or a file of the project:

```toml
[project]
name = "dash"
environment = "src/vehicle.nsm::Automotive"
```

```nsm-fragment
// src/vehicle.nsm
interface Automotive {
    .temperature: Temperature
    // An offer, so that a board can replace it.
    .temperature ?= -40degC..85degC
}
```

A project that is content with a standard environment names it, and every board of it is then judged against AEC-Q100 Grade 1's range with no line in any board:

```toml
[project]
name = "dash"
environment = "std::env::Automotive"
```

A board that declares `.environment` of its own has that one alone (not the two together), and a board that only states `.environment.temperature &= ...` with no declaration has the manifest's definition as if it had written the declaration, and its statement replaces the offer. The manifest's default is a convenience and not a floor: nothing stops a board of the project from declaring a milder environment. A project can name one of the standard environments, or write its own as above and cite where the range comes from in its doc comment; a range that cannot be cited does not belong in `std`.

### Catalog columns

A catalog file may have the fixed columns `temp_min` and `temp_max` (see Catalog files): each one temperature as the datasheet prints it (`85degC`; the standard rows write kelvin, `358.15K`, the same value), with the document and which temperature it is in the row's `source` and `notes`. Only a row whose datasheet or record says so gets one; an empty cell stays empty. A cell that is not a temperature is refused when the file is read, with its file and line (a figure written bare is a number and not a temperature, and `85C` would be a charge):

```csv fails file=catalogs/resistor.csv
mpn,resistance,max_power,package,temp_max,source
ACME-R316,316ohm +/- 1%,62.5mW,0402,85,https://example.invalid/acme-r316.pdf
```

```text
catalogs/resistor.csv:2: `temp_max`: `85` is a plain number, but a part's highest temperature is a temperature such as `85degC` or `358.15K`
```

A `max_voltage` cell may be written in steps over temperature, as a design writes them after `&=` (see A rating that falls with temperature), quoted for the commas. The column holds the greatest step, and a row whose steps do not hold the voltage across the part at a temperature the board reaches is not offered. Here the one row of 1 F holds 5.5 V only up to 70 C, and the board reaches 85 C on a rail of 5 V:

```csv file=catalogs/capacitor.csv set=supercap
mpn,capacitance,effective_capacitance,max_voltage,package,source
ACME-SC1F,1F +/- 20%,derate capacitance by bias:,"5.5V to 70degC, 4.2V to 85degC",1210,https://example.invalid/acme-sc1f.pdf page 2
```

```nsm fails catalogs=supercap
import std::env::Environment
import std::parts::passive::Capacitor
import std::sources::Rail

board Backup {
    .environment: Environment { .temperature &= -20degC..85degC }
    .supply: Rail { .voltage &= 5V +/- 5%; .pos.net &= "VBAK" }
    .hold: Capacitor { .capacitance &= 1F +/- 20% }
    .supply.pos ~> .hold ~> .supply.neg
}
```

```text
error: the `capacitor` catalog has nothing for `hold`, which needs capacitance within 800mF..1.2F (1 row was left out because its maximum voltage, stated in steps over temperature, is under the 5.25 V across `hold` at a temperature the board's environment reaches, `.environment.temperature` 253.15 K to 358.15 K (-20degC to 85degC))
  --> main.nsm:8:5
  |
8 |     .hold: Capacitor { .capacitance &= 1F +/- 20% }
  |     ^^^^^^^^^^^^^^^^
```
