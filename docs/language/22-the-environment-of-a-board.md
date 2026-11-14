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
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "VCC"
    }
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
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "VCC"
    }
    .xtal: Crystal { .frequency &= 16MHz +/- 30ppm }
    .xtal.a ~ .supply.pos
    .xtal.b ~ .supply.neg
}
```

```text
error: the `crystal` catalog has nothing for `xtal`, which needs frequency within 15.9995MHz..16.0005MHz (4 rows were left out because the temperatures they state do not cover the board's environment, `.environment.temperature` 233.15 K to 398.15 K (-40degC to 125degC): a row's `temp_min` and `temp_max` are its datasheet's)
   --> main.nsm:11:5
   |
11 |     .xtal: Crystal { .frequency &= 16MHz +/- 30ppm }
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
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .supply.pos ~> .r1 ~> .supply.neg
}
```

A board narrows the offer with `&=`, as it does a process (`.environment.temperature &= -20degC..90degC` keeps the board inside its parts' ranges and no wider than the grade). A stated range replaces the offer and is not intersected with it (ADR 0051), so `&= -55degC..150degC` is a wider environment than `Automotive`.

Rules.

- **A row that states a range the environment is not inside is not offered.** The solver tries the next row, and says in its message how many it left out and why.
- **A row that states none is offered, and named.** A pick whose catalog row has no `temp_min`, no `temp_max` or neither is kept, because an empty cell is unknown and not a refusal, and a guideline finding says so: a `warning:` line by default, an error with `--strict` or `guidelines = "error"` in `netsmith.toml` (the same class as the unconstrained pick, ADR 0065). A check that could not be made is never a pass. The finding names the part, the row, which temperature is missing and what to write; parts picked as one row are one finding. The standard rows that state a range are the ones read from a datasheet (the resistors, capacitors, ferrite beads, fuses, headers, Murata inductors, crystals, most diodes, the AO3400A, the 1N5819WS, the XC6206 and twelve LEDs; `data/jlcpcb/README.md` says which and from what); the others, such as the two LEDs whose datasheet does not name the part, the B5819W SL and the 2N7002, state none. Some of the figures are junction ranges (below).

```nsm solve warns
import std::env::Environment
import std::parts::passive::{Led, Resistor}
import std::sources::Rail

board Probe {
    .environment: Environment { .temperature &= -40degC..85degC }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
    }
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
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
    }
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
mpn,capacitance,effective_capacitance,max_voltage,package,dielectric,library,source
ACME-C33N3,33.3nF +/- 10%,derate capacitance by bias:,16V,0402,X5R,Basic,https://example.invalid/acme-c33n3.pdf
```

```nsm fails catalogs=class_only
import std::env::Environment
import std::parts::passive::Capacitor
import std::sources::Rail

board Hot {
    .environment: Environment { .temperature &= -40degC..125degC }
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .c: Capacitor { .capacitance &= 33.3nF +/- 10% }
    .supply.pos ~> .c ~> .supply.neg
}
```

```text
error: the `capacitor` catalog has nothing for `c`, which needs capacitance within 29.97nF..36.63nF (15 rows were left out because the temperatures they state do not cover the board's environment, `.environment.temperature` 233.15 K to 398.15 K (-40degC to 125degC): a row's `temp_min` and `temp_max` are its datasheet's; a row that states no temperature of its own is judged by the range of its dielectric class (X5R), which is the class's and not its datasheet's: add `temp_min` and `temp_max` to the row to say its own)
   --> main.nsm:11:5
   |
11 |     .c: Capacitor { .capacitance &= 33.3nF +/- 10% }
   |     ^^^^^^^^^^^^^
```

- **A board with no environment is not judged** and says nothing: no check, no finding. The same holds for a board of a project whose manifest names none.
- **A board that has an environment that states no temperature** (`.environment: Environment { }`) is told so, and no part is judged.
- **Parts that are not fitted** (`.assembly` is `AssemblyMode::None`: a mounting hole, a fiducial) and labels are not judged. A part soldered by hand or consigned is, because it sees the same air.
- **Each board of a system has its own environment.** A system does not state one.
- **A junction range is a part's range.** A part whose datasheet gives a junction temperature range (`TJ`, in the Schottky, TVS and Zener diodes and the AO3400A of the standard catalogs, each row's `notes` say so) and no ambient one is judged by that figure as it stands: a junction range that covers the environment is a pass. The check does not compute how far the part's own heating lifts its junction above the air, so this is the limit of it: a part whose junction range just covers the environment can still be too hot at its junction in that air, and the self-heating margin is a design's to check.
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
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
    }
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
    .supply: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= "VIN"
    }
    .d: Schottky {
        .max_reverse_voltage &= 100V
        .max_forward_current &= 3A
    }
    .supply.pos ~> .d ~> .supply.neg
}
```

```text
error: the `schottky` catalog has nothing for `d`, which needs max_reverse_voltage within 100V and max_forward_current within 3A (1 row was left out because it says it is not qualified (`None`) and the board's `.environment.qualification` requires `Aec`: a row's `qualification` is its datasheet's)
   --> main.nsm:14:5
   |
14 |     .d: Schottky {
   |     ^^^^^^^^^^^^
```

What it proves. The check says what the datasheet of the ordering code says and nothing about the board: a part qualified to AEC-Q200 is not shown to have passed any other test, and the sentence of a datasheet is not a certificate. The `Qualification` of a stricter standard is not a stronger claim: there is one variant, `Aec`, so a board cannot ask for AEC-Q100 Grade 0 by this member (the temperature range of the part says what a grade says).

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
