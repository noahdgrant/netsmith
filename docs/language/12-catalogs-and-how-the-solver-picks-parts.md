## Catalogs and how the solver picks parts

A catalog is the list of real parts that a part's fields are matched against. Each real part is a candidate with one value per column. `catalog resistor { .resistance, .max_power, .package }` makes `resistance`, `max_power` and `package` the columns of a resistor.

The solver works in four steps.

1. Propagate. Connected interfaces hold one value, so the solver narrows them to what all of them allow. A field that an equation defines takes the computed value.
2. Pick. For each part with an unset catalog column, take candidates in catalog order. A candidate fits when each column value lies within the range the design allows. Use the first candidate for which every relation that mentions the part holds, and every relation that mentions a value worked out from it (a named ratio, a module's declared current) once the parts that value also comes from are chosen (ADR 0057).
3. Verify. Check every relation again with the picks in place.
4. Report. Whatever fails is reported with the numbers. Whatever could not be evaluated is listed as not evaluated.

Rules.

- Order is the policy. The solver takes the first candidate that works, so the order of the rows in the catalog's file decides which part you get, and nothing sorts it. The JLCPCB catalogs list Basic parts first (no feeder fee), then ascending price, then ascending value. The result is the cheapest Basic part that works.
- The check is the worst case. A requirement on a result must contain the whole range the inputs can produce, not a typical value.
- Parts that share a relation are searched together, depth first, and a failed choice backtracks. The search goes back to the nearest part that a failure depends on (the parts whose values the failed relation is worked out from), not to the part just before it, and when it fails it names the parts the failure is made of and the relation that cannot be met, `no combination of candidates satisfies `a` and `z` together, whatever the other parts of the group are: the closest, ... fails because ...`, with no word about parts that played no part in it (ADR 0057). The search stops after 250,000 combinations, or after as much work as a board of its size allows (a trial of a large board costs more than one of a small board, so a large one stops sooner: both are counts, never seconds, so the answer is the same on every machine), and says so: `gave up after trying 250000 combinations of candidates for `a` and `m` and `z`; a bound reads values of all of them, so no part could be blamed alone. State a value for one of them, narrow a window that a bound on them reads, or split the group so that fewer parts share a bound`. It names the parts it was choosing together (the first twelve, and how many more), and a stop for work adds that it was the work and not the number of combinations. It only reaches that when the failure depends on every part (ADR 0057, 2026-11-07 amendment, issue #177). Parts that read only the same stated value (a rail whose voltage the board states) and nothing else in common are searched apart, so a part that fails does not undo the picks of the others. Before a group is searched, a part that no candidate can serve whatever the others are (every candidate that fits is refused by a check that only its own choice decides; for a bank, at every count it may have, and a check on what a capacitor holds at its voltage counts as decided by the part that sets that voltage too, so it does not blame the bank) is named at once: `no resistor candidate satisfies t10: the closest of the 343 that fit, ... fails because ...` (ADR 0057).
- A column the design leaves alone takes the candidate's value, so `.r1: Resistor {}` works.
- A part is picked after the parts whose picks decide what it needs, never by the order of names (below).
- Narrow only what the design needs. A tighter range removes candidates.
- A part number is in a catalog only with a recorded source: a row that names a part has a `source` cell, or the file does not load (next section).
- A catalog may be keyed by one of its columns: `catalog mcu by .mpn { .mpn, .package }`. The solver does not choose among the rows of a keyed catalog for you, because the first fitting row is right for a regulator and wrong for a microcontroller. When exactly one row fits, there is nothing to choose, and the solver takes it: a catalog with one row needs no `.mpn` at the place the part is used, and neither does a part whose narrowings (`.package &= "QFN-32"`) leave one row. When several rows fit, name the part with `.u.mpn &= "..."`, which leaves one row. If you do not, the message lists the candidates, at the line of the use: `` `u` is an `Mcu32`; name the part with .mpn: MCU32-A, MCU32-B ``.
- A catalog's `package` column picks the part's pin table. The pads are in the table (previous section), not in the catalog. A row whose package lacks a pin that the board connects is skipped, and the message says so when nothing else is left.

A bound on a member that a part is held to reaches the part (ADR 0065). A module that writes `.r.resistance &= .balance` holds the resistor to the member `.balance`, and a board that writes `.lamp.balance >= 20kohm` has bounded the member, which is not a value: nothing flows from it into the resistor. The solver reads the bound as a bound on the resistor too, since `&=` puts the resistor's value inside the member's, and a bound on all of the member is a bound on every part of it. It does so for a member that has no value yet, along a chain of such links, and for `==` only when the left side has no range of its own (an equation puts its right side inside the left side's allowance, so a range on the left says nothing about the right). The pick then takes the first row that meets the bound, tolerance included (`20kohm +/- 1%` is not always at least 20 kohm), and a part that is not a pick is checked after the picks, with the numbers. The solver adds nothing but this refusal: no range and no value changes, so a design that meets what it states is never refused.

```nsm solve report
import std::parts::passive::Resistor
import std::sources::Rail

module Lamp {
    .supply: Rail
    .balance_resistance: Resistance
    .top: Resistor
    .bottom: Resistor
    .top.resistance &= .balance_resistance
    .bottom.resistance &= .balance_resistance
    .supply.pos ~> .top ~> .bottom ~> .supply.neg
}

board Bounded {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
    }
    .lamp: Lamp
    .supply ~ .lamp.supply
    .lamp.balance_resistance >= 20kohm
}
```

```text
board Bounded
  R1   Resistor        22k             Resistor_SMD:R_0402_1005Metric                             C25768 Basic
  R2   Resistor        22k             Resistor_SMD:R_0402_1005Metric                             C25768 Basic
  net GND: R1.2
  net VCC: R2.1
  net net-lamp.bottom.a: R1.1, R2.2
```

When no row meets the bound, the message says which statements make it a bound on the part:

```nsm fails
import std::parts::passive::Resistor

module Fixed {
    .value: Resistance
    .r: Resistor
    .r.resistance &= .value
}

board Unreachable {
    .fixed: Fixed
    .fixed.value >= 20kohm
    .fixed.value <= 10kohm
}
```

```text
error: no `resistor` candidate satisfies `fixed.r`: the closest of the 219 that fit, 20kΩ 1% 0402 C25765, fails because fixed.r.resistance must always be at least 20kΩ, but 19.8kΩ..20.2kΩ is not always above 20kΩ: it is held to `fixed.value` (`.r.resistance &= .value`, main.nsm:6), and `.fixed.value >= 20kohm` (main.nsm:11) bounds it
   --> main.nsm:11:5
   |
11 |     .fixed.value >= 20kohm
   |     ^^^^^^^^^^^^^^^^^^^^^^
```

A value that two equations state is held to a window that the board stated, whatever range the part's own member has (ADR 0065). The frequency of an oscillator is worked out from its timing resistor, and the board states the window it wants; the module links the two with `.osc.fsw == .fsw`. Both lines are equations on `.osc.fsw`, and neither is its allowance: a stated range on `.fsw`, which no pick changes, is, and what the circuit works out must lie inside it. A range that the oscillator's own member `.rt_resistance` has makes the frequency a range before any resistor is picked, and the window is then compared with the pick as it is without that line: a resistor outside the window is refused where it is picked, and a stated resistor outside it fails the build with the numbers. Two values that the picks change and that nothing bounds only have to share a value.

```nsm fails
import std::parts::passive::Resistor
import std::sources::Rail

module Oscillator {
    .rt_resistance: Resistance
    .rt_resistance &= 10kohm..230kohm
    .fsw_nominal: Frequency
    .fsw_nominal == 46.5MHz * 1kohm / (.rt_resistance + 5.2kohm)
    .fsw_tolerance: Number
    .fsw_tolerance &= 0.925..1.075
    .fsw: Frequency
    .fsw == .fsw_nominal * .fsw_tolerance
}

module Timer {
    .fsw: Frequency
    .supply: Rail
    .osc: Oscillator
    .timing: Resistor { .resistance &= 15kohm +/- 1% }
    .supply.pos ~> .timing ~> .supply.neg
    .osc.rt_resistance == .timing.resistance
    .osc.fsw == .fsw
}

board Probe {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
    }
    .timer: Timer
    .timer.supply ~ .supply
    .timer.fsw &= 0.9MHz..1.1MHz
}
```

```text
error: `timer.osc.fsw` must stay within 900kHz..1.1MHz, but (timer.osc.fsw_nominal * timer.osc.fsw_tolerance) works out to 2.113MHz..2.494MHz
   --> main.nsm:12:5
   |
12 |     .fsw == .fsw_nominal * .fsw_tolerance
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A part is picked after the parts whose picks decide what it needs. A capacitor behind a diode that the solver picks has the bias of the rail it sits on, which is the supply's voltage less the diode's forward voltage, and that is known only once the diode is chosen. The solver therefore chooses the diode first, and the board below builds with its parts named in either order. A part that needs a value that no pick can set is not delayed: it fails as it would alone, with a message that says which field to state. Ties go by path, and the same inputs give the same board.

```nsm evaluate
import std::mfg::jlcpcb::TwoLayer
import std::parts::diode::Schottky
import std::parts::passive::Capacitor
import std::sources::Rail

board Behind {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= "VIN"
    }
    .load: Rail {
        .pos.net &= "VLOAD"
        .current &= 0mA..100mA
    }
    .supply.neg ~ .load.neg
    .z_diode: Schottky {}
    .supply.pos ~> .z_diode ~> .load.pos
    .load.voltage == .supply.voltage - .z_diode.drop
    .a_cap: Capacitor { .effective_capacitance >= 1uF }
    .load.pos ~> .a_cap ~> .load.neg
}
```

Two groups of parts that read each other's values through a bound (a window that the tap of one divider must stay inside, set by another divider's output) are searched one after the other, in path order, and the one that comes first is picked while the bound waits for the other. Its first fit can leave nothing for the other, and the board would then build or fail by what its instances are called. When a group fails and an earlier group shares a bound with it, the solver searches them again, the failed one first, so that the others are judged by the bound against its picks. If that does not find parts for all of them the first failure and its message stand. Parts that were found this way are the parts that the other order of the names gives (ADR 0065).

A bound that still waits for a part that is chosen later is not judged, but it turns a candidate away when no value in the ranges as they stand can meet it: the parts chosen later only narrow the ranges, so they cannot meet it either. A divider that draws far more than a quiescent bound allows is refused for it even while the input voltage of the stage waits for another group. A candidate that the bound could still hold for, with a narrower range, is kept, and the check after the pick is the judge (ADR 0065, 9.5). Among the candidates that are kept, one for which the bound holds over the whole range as it stands is chosen before one for which it holds only for some values, and within each of the two the first in catalog order is taken; if every candidate holds only for some values, the first of them is taken (ADR 0065, 9.6).

The JLCPCB catalogs hold 1% resistors (0402 E24 from 10 ohm to 1 Mohm, Basic 0603, 250 mW 0805), Basic 0402 and 0603 ceramic capacitors, LEDs in five colors, one fixed-output linear regulator, and a few Schottky, Zener and TVS diodes, ferrite beads, power inductors, resettable fuses, N-channel MOSFETs and crystals (ADR 0025). A resistor's rating, `max_power`, is a column: a row with an empty cell is never offered, because the resistor's own check uses it. An empty cell is no obstacle for a column the design never mentions (`temperature_coefficient` is empty in every standard row), and a design that bounds the column is offered only the rows that state it. `data/jlcpcb/README.md` lists every row and its source.

### Catalog files

The rows of a catalog are data, not code. A catalog is one CSV file named after it: `resistor.csv` is the `resistor` catalog. The standard catalogs are `data/jlcpcb/catalog/*.csv` in netsmith's source and ship inside the program. A project adds files of its own, which is how it gets a part netsmith does not list. Every `*.csv` file in the project's `catalogs/` folder is one, named by its file name: `catalogs/mcu.csv` is the catalog `mcu` (ADR 0022). `netsmith.toml` needs no line for it, because the key `catalog_dirs` has the default `["catalogs"]` (`netsmith init` writes it out, so that a reader sees it):

```toml
[project]
name = "blink"
top_level = ["src/main.nsm"]
catalog_dirs = ["catalogs"]
```

- `catalog_dirs` lists folders, relative to the folder of `netsmith.toml`; `..` is allowed, so projects can share one (`catalog_dirs = ["catalogs", "../shared"]`). `catalog_dirs = []` reads no folder. A folder that does not exist holds no catalogs.
- Only the files directly in a folder are read; **sub-folders are not**. Files that do not end in `.csv` are ignored. The files are read in file-name order, folders in the order the list gives, so the result does not depend on the file system.
- A `.csv` file in a folder whose name is not a catalog name (letters, digits and `_`, not starting with a digit: `my-parts.csv`, `2026.csv`) is an error that names the file, not a file that is skipped: a data file that is not a catalog goes in another folder. The check is on the name only; a `.csv` that is not a catalog but is named like one is read as a catalog and fails with the line of the first thing wrong (an unknown column, a missing `source`).
- `catalogs = ["extra/ldo.csv"]` still lists single files, from any folder inside the project, each named after its catalog, for a file that is not in a `catalog_dirs` folder. A file that a folder and the list both name, and two files of one catalog (`catalogs/ldo.csv` and `../shared/ldo.csv`), are errors that name both files: a catalog is one file. A project that listed `catalogs/x.csv` before this key existed removes the entry (or writes `catalog_dirs = []`).
- A running editor reads the folders again with each build, so a file that is added to a folder, or removed, is noticed without a restart. A change to `catalog_dirs` itself in `netsmith.toml` is read when the editor starts.

The first line of a file names the columns. Two kinds of column can be there, in any order.

- The columns of the part: the paths of the fields in its `catalog X { ... }` declaration, without the leading dot (`resistance`, `package`, `vout.voltage`, `mpn`). A cell of a quantity column is a value as the language writes it, with its unit: `10kohm +/- 1%`, `1.8V..5.5V`, `0.3V`. It must be in the dimension of the field. A cell of a text column is its text as it is, without quotes: `0402`, `SOT-23-5`. An empty cell means the row has no value there: the row fits no design that asks for one, and, for any column a design needs (the package, the resistance, a rating), the row is never offered at all. Only a column that the part's `catalog` statement marks `optional` (ADR 0078, below) may be empty in a row that is offered. The standard parts mark the four that were added after the files were written: `temperature_coefficient` of `resistor`, `rms_current` of `inductor`, `dielectric` of `capacitor` and `breakdown_voltage` of `tvs`. Such a row is offered to a design that says nothing about the field, and a design says something by narrowing it, by a relation that names it, by a `~` that joins it or by a balance of currents that sums it.
- The fixed columns, all optional except as noted: `lcsc` (the supplier's number), `mpn` (the manufacturer's; also a column of the part if its catalog lists `.mpn`), `library` (`Basic`, `Extended` or `Preferred`), `height` (how tall the part's body stands above the board, one length such as `2.62mm`, the greatest the manufacturer's document allows; the part's `.height` is it when the design states none, ADR 0094, and an empty cell means the height is unknown; see Height limits), `temp_min` and `temp_max` (the lowest and the highest temperature the part works at, each one temperature as the datasheet prints it, `85degC` or `358.15K`; a junction range stands for the part's range and the part is judged by it, which is the limit of the check; a board with an `.environment` is checked against them and an empty cell is offered and named, ADR 0099; see The environment of a board), `qualification` (`Aec` where the part's datasheet says its ordering code is AEC qualified, `None` where it says it is not, empty where it says nothing; the part's `.qualification` is it when the design states none; a board that requires `Qualification::Aec` of its `.environment` is checked against it, ADR 0099; see Qualification, under The environment of a board), `price` and `stock` (JLCPCB's snapshot, entered by hand; the solver does not read them, so a row is offered whatever its `stock` says, and the build notes a pick whose `stock` is 0; the bill of materials shows them only when no supply provider has the part, ADR 0047), `price_breaks` (`1:0.0019 1000:0.0015`, the price for one piece from each quantity; entered by hand and read by nothing), `date` (`2026-10-31`, the day the `price` and `stock` were read), `checked` (`2026-11-02`, the day `catalog refresh` last found the row's `mpn` and `package` equal to LCSC's record; empty if never, read by nothing), `name` and `label` (what a message calls the row, and what the schematic's value field shows; both default to the `mpn`, then the `lcsc`), `notes`, and `source`. **A row with an `lcsc` or an `mpn` must have a `source`**: the URL or text that says where its numbers were read. A row with neither is a made-up part and needs a `name`.

`price`, `stock` and `library` are JLCPCB's numbers and `date` is the day they were read: they are a hand-entered snapshot (the standard catalogs that ship in netsmith leave `price`, `stock`, `date` and `checked` empty: read a part's price and stock from the supplier before ordering), and `netsmith catalog refresh` never writes them (it asks LCSC, never JLCPCB, and only verifies the row and records the day in `checked`; see `netsmith catalog refresh --help`). `netsmith catalog age` lists the rows whose price and stock were read long ago and those whose price and stock are unknown. The price and stock of a part come from a supply provider (the `[supply]` table of `netsmith.toml`, ADR 0047, provisional): `provider = "file"` with `file = "supply/quotes.csv"`, a CSV with the columns `lcsc` or `mpn`, `channel` (`jlcpcb-assembly` or `lcsc-retail`: the two stocks differ, and the answer says which it is), `stock`, `price_breaks` (`1:0.0100 100:0.0050`), `currency` and `date`; `channel` and `cache_hours` choose the stock shown and how long a cached answer is used. With no provider, or one that does not list the part, price and stock are unknown in the bill of materials, never guessed and never 0. The manifest has no field for a key or a password. The solver never asks; it reads the rows and no price or stock.

A file named like a standard catalog adds its rows after the standard ones, so the standard parts are tried first. A part, by `lcsc` or by `mpn`, is in a catalog once; a file that repeats a standard part is refused. A capacitor's `effective_capacitance` cell is its curve of bias (ADR 0013): `derate capacitance by bias: 0:1 5:0.5` is the `capacitance` of the row after the part keeps 1 at 0 V and 0.5 at 5 V, and nothing after the colon means that no curve is published for the part.

**Generic parts** (ADR 0047). If no row of a catalog works, the solver may pick a generic part, which has no supplier or part number. The rows are tried first, so a design that built keeps its parts. The series is `series = "E24"` (the default), `"E12"`, `"E96"` or `"none"` in the `[project]` table of `netsmith.toml`; capacitors and inductors use it too, except that E96 becomes E24 for them.

- A **resistor** is a value of the series with a 1% tolerance in a 0402, 0603 or 0805 package at that package's usual power rating (62.5, 100 and 125 mW).
- A **ceramic capacitor** is a value of the series from 1 pF to 100 uF, a tolerance of 20%, 10% or 5%, and a 0402, 0603 or 0805 package. It has **no bias curve**, so its effective capacitance is unknown: a generic capacitor is never used where the design asks for `.effective_capacitance`, and it resolves only to a row that has a recorded curve (owner decision, 2026-11-03: no conservative curve and no derating rule). Its rated voltage is the least that a relation of the design demands (`.max_voltage >= .bias * 1.5`).
- An **inductor** is a value of the series from 1 nH to 1 mH, a tolerance of 30%, 20% or 10%, and a 0402, 0603, 0805 or 1210 package. Its saturation current is the least that any relation demands, the stage's included (`.inductor.current + .ripple / 2 <= .inductor.saturation_current` is worked out after the part's marked inductance has set the ripple, ADR 0057), so it is a requirement on the row that will stand for the part; its DC resistance the most that a relation allows. A rating that nothing bounds is left unknown, and the check that mentions it is listed as not evaluated.
- An **LED** is a package (0402, 0603 or 0805) with the colour and the forward voltage the design states, and is used only where the design states the forward voltage, since other values are worked out from it.

When the bill of materials or `fab` is made, each generic part is resolved to the row of the catalog with the same package and value, a tolerance no wider and every rating it was given no lower (and, for a capacitor, a recorded bias curve): JLCPCB Basic before Preferred before Extended, then the order of the file, passing over a row that a supply provider says has no stock. A generic part no row stands for is *unresolved*: the bill of materials lists it with the nearest rows and `fab` refuses it by name. The message also says, for each nearest row, whether the board's checks pass with that row in the part's place (the part's own stated value left out, every relation kept), at most 30 solves for a board; it ends "Change the value in the design and build again", because a new value is checked against every relation by the build. The nearest rows are ranked by how near they come to the rating that fails (a project row at 5.3 A for a need of 6.05 A before a standard row at 3.6 A), then by package and value. `build` says the same about every assembled generic part that is unresolved, as a guideline finding (see "A generic part that no row can be ordered for").

A project's file, `catalogs/resistor.csv`, adds a 316 ohm resistor to the standard resistors, and a board asks for it:

```csv file=catalogs/resistor.csv set=own
mpn,resistance,max_power,package,library,price,stock,date,source
ACME-R316,316ohm +/- 1%,62.5mW,0402,Basic,0.0011,5000,2026-10-31,https://example.invalid/acme-r316.pdf
```

```nsm solve report catalogs=own
import std::parts::passive::Resistor

board OwnPart {
    .r1: Resistor { .resistance &= 316ohm +/- 1% }
}
```

```text
board OwnPart
  R1   Resistor        ACME-R316       Resistor_SMD:R_0402_1005Metric                             no LCSC number
```

The solver does not read `stock`, so a row that states a stock of 0 is still picked when it is the first that fits. The build says so in a note, and the editor shows the same line among its problems: the part's designator and path, the row's LCSC number and manufacturer's number, the file and line of the row, and what to do (choose another row, or state a different value). A part that the standard `data/jlcpcb/considered.csv` records as out of stock when it was read gets the same note when a project's row offers it and states no stock. An empty `stock` cell states nothing and gets no note, and a positive stock gets none. The note is not a guideline and never stops a build, and the pick is the same with or without it.

```csv file=catalogs/resistor.csv set=sold_out
lcsc,mpn,resistance,max_power,package,library,stock,source
C900001,ACME-R316,316ohm +/- 1%,62.5mW,0402,Extended,0,https://example.invalid/acme-r316.pdf
```

```nsm solve stock catalogs=sold_out
import std::parts::passive::Resistor

board SoldOut {
    .r1: Resistor { .resistance &= 316ohm +/- 1% }
}
```

```text
R1 (`r1`) is C900001 (ACME-R316), and its catalog row (catalogs/resistor.csv:2) states a stock of 0: the solver does not read stock, so it picked the part; choose another row of the catalog or state a different value, and read the supplier's stock before ordering
```

A resistor's `temperature_coefficient` is a column too: how far the resistance moves per kelvin, as a fraction (`0.00005` is 50 ppm/K; take the datasheet's largest figure). The standard rows leave it empty, because no value goes into a standard catalog without a recorded source. A design that does not mention the field picks rows as before. A design that limits it, `.r1.temperature_coefficient <= 0.0001`, is offered only the rows that state it, so it needs a row of a project's file (a file that leaves the column out of its first line is read as empty there, which is why the file above needs nothing new):

```csv file=catalogs/resistor.csv set=tempco
mpn,resistance,max_power,package,temperature_coefficient,library,source
ACME-R100T,100ohm +/- 1%,62.5mW,0402,0.00005,Basic,https://example.invalid/acme-r100t.pdf
```

```nsm solve report catalogs=tempco
import std::parts::passive::Resistor

board Stable {
    .r1: Resistor {
        .resistance &= 100ohm +/- 1%
        .temperature_coefficient <= 0.0001
    }
}
```

```text
board Stable
  R1   Resistor        ACME-R100T      Resistor_SMD:R_0402_1005Metric                             no LCSC number
```

With only the standard rows, the same limit has nothing to pick, and the message says why:

```nsm fails
import std::parts::passive::Resistor

board Unstated {
    .r1: Resistor {
        .resistance &= 100ohm +/- 1%
        .temperature_coefficient <= 0.0001
    }
}
```

```text
error: the `resistor` catalog has nothing for `r1`, which needs resistance within 99Ω..101Ω (rows with an empty cell are never offered, since a part with no stated rating cannot be shown to stay within it: 219 for `temperature_coefficient`)
  --> main.nsm:4:5
  |
4 |     .r1: Resistor {
  |     ^^^^^^^^^^^^^
```

A part that is an IC is named by its key. The catalog of the `Mcu32` above is `catalogs/mcu.csv`:

```csv file=catalogs/mcu.csv set=ic
mpn,package,library,price,stock,date,source
MCU32-A,QFN-32,Extended,1.20,300,2026-10-31,https://example.invalid/mcu32-a.pdf
MCU32-B,QFN-32,Extended,1.45,120,2026-10-31,https://example.invalid/mcu32-b.pdf
```

```nsm fails catalogs=ic
import src::mcu::Mcu32
import std::sources::Rail

board WhichMcu {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .u: Mcu32 { .package &= "QFN-32" }
    .supply ~ .u.vdd
    .u.nrst ~ .supply.pos
}
```

```text
error: `u` is an `Mcu32`; name the part with .mpn: MCU32-A, MCU32-B
  --> main.nsm:9:5
  |
9 |     .u: Mcu32 { .package &= "QFN-32" }
  |     ^^^^^^^^^
```

A name is needed only when the board must choose. `.mpn &= "MCU32-B"` picks that row (a hand pick, _A hand pick_ below; `.lcsc &=` would too):

```nsm evaluate catalogs=ic
import src::mcu::Mcu32
import std::sources::Rail

board NamedMcu {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .u: Mcu32 { .package &= "QFN-32"; .mpn &= "MCU32-B" }
    .supply ~ .u.vdd
    .u.nrst ~ .supply.pos
}
```

A catalog with one row has nothing to choose, so the board does not name it. The same board, with `catalogs/mcu.csv` holding only `MCU32-A`, builds with no `.mpn`:

```csv file=catalogs/mcu.csv set=ic_one
mpn,package,library,price,stock,date,source
MCU32-A,QFN-32,Extended,1.20,300,2026-10-31,https://example.invalid/mcu32-a.pdf
```

```nsm evaluate catalogs=ic_one
import src::mcu::Mcu32
import std::sources::Rail

board OnlyMcu {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .u: Mcu32
    .supply ~ .u.vdd
    .u.nrst ~ .supply.pos
}
```

Two rows that differ in a column the board narrows leave one row for that board, and it is taken as well. A file with `MCU32-A` in `QFN-32` and `MCU32-B` in `QFN-48` (a package the part has no pin table for, so no board that connects its pins can use it):

```csv file=catalogs/mcu.csv set=ic_pair
mpn,package,library,price,stock,date,source
MCU32-A,QFN-32,Extended,1.20,300,2026-10-31,https://example.invalid/mcu32-a.pdf
MCU32-B,QFN-48,Extended,1.45,120,2026-10-31,https://example.invalid/mcu32-b.pdf
```

```nsm evaluate catalogs=ic_pair
import src::mcu::Mcu32
import std::sources::Rail

board NarrowedMcu {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
    }
    .u: Mcu32 { .package &= "QFN-32" }
    .supply ~ .u.vdd
    .u.nrst ~ .supply.pos
}
```

#### An optional column

A column that a row may leave empty says so with the word `optional` after it in the `catalog` statement (ADR 0078), the word that a member (ADR 0074) and a `pins` row already use for "may be absent". The standard library does it for `temperature_coefficient`, `dielectric`, `rms_current` and a TVS diode's `breakdown_voltage`, and a project's own part does it for a column of its own catalog. A fuse that records a surge rating only for some parts (invented, like the file below):

```nsm module=src::zfuse
/// A fuse with an optional surge rating. Illustrative; not a real part.
part Zfuse {
    .designator &= "F"
    .rated_current: Current
    .surge_current: Current
    pins "0805" {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    bridge .a ~> .b
    .package: Package
    catalog zfuse { .rated_current, .surge_current optional, .package }
}
```

The file leaves the cell empty for the row that does not record it, and a file written before the column existed may leave the whole column out:

```csv file=catalogs/zfuse.csv set=surge
mpn,rated_current,surge_current,package,library,source
ZF-1,1A,,0805,Extended,https://example.invalid/zf-1.pdf
ZF-2,1A,8A,0805,Extended,https://example.invalid/zf-2.pdf
```

A design that says nothing about the column is offered every row, the empty one too. This file has only that row, and the design builds:

```csv file=catalogs/zfuse.csv set=unrated
mpn,rated_current,package,library,source
ZF-1,1A,0805,Extended,https://example.invalid/zf-1.pdf
```

```nsm evaluate catalogs=unrated
import src::zfuse::Zfuse

board Plain {
    .f1: Zfuse { .rated_current &= 1A }
}
```

A design that states the column, by a bound, an `&=`, a `~` or a balance of currents, is offered only the rows that record it (ADR 0047, ADR 0058). With the first file it gets `ZF-2`:

```nsm evaluate catalogs=surge
import src::zfuse::Zfuse

board Surge {
    .f1: Zfuse {
        .rated_current &= 1A
        .surge_current >= 5A
    }
}
```

With the file that has no `surge_current` column it has nothing to pick, and the message counts the rows:

```nsm fails catalogs=unrated
import src::zfuse::Zfuse

board Surge {
    .f1: Zfuse {
        .rated_current &= 1A
        .surge_current >= 5A
    }
}
```

```text
error: the `zfuse` catalog has nothing for `f1`, which needs rated_current within 1A (rows with an empty cell are never offered, since a part with no stated rating cannot be shown to stay within it: 1 for `surge_current`)
  --> main.nsm:4:5
  |
4 |     .f1: Zfuse {
  |     ^^^^^^^^^^
```

The columns the statement does not mark are as they were: a row with an empty cell is never offered, and the file must have the column. The marker is refused on a column the part does not have (the same message as for any column), and on the key of a keyed catalog (`catalog mcu by .mpn { .mpn optional }`), because a row with no key could not be chosen.

A file is read when it is used, and a mistake names the file and line. A row with a part number and no source, and a part twice:

```csv fails file=catalogs/resistor.csv
mpn,resistance,package
ACME-R316,316ohm +/- 1%,0402
```

```text
catalogs/resistor.csv:2: the row for mpn ACME-R316 has no source; every part in a catalog needs the URL or text it was read from in its `source` column
```

```csv fails file=catalogs/resistor.csv
mpn,resistance,package,source
ACME-R316,316ohm +/- 1%,0402,https://example.invalid/a
ACME-R316,316ohm +/- 1%,0402,https://example.invalid/b
```

```text
catalogs/resistor.csv:3: the mpn `ACME-R316` is already a row at catalogs/resistor.csv:2; a part is in a catalog once
```

A column the part does not list, and a value in the wrong unit, are found when a board uses the catalog, because only then is it known what the columns hold:

```csv file=catalogs/resistor.csv set=typo
mpn,resistence,package,source
ACME-R316,316ohm +/- 1%,0402,https://example.invalid/acme-r316.pdf
```

```nsm fails catalogs=typo
import std::parts::passive::Resistor

board Typo {
    .r1: Resistor { .resistance &= 316ohm +/- 1% }
}
```

```text
error: `r1` cannot be chosen from the `resistor` catalog: catalogs/resistor.csv:1: `resistence` is not a column of `catalog resistor`, which lists .resistance, .max_power, .package, .temperature_coefficient
  --> main.nsm:4:5
  |
4 |     .r1: Resistor { .resistance &= 316ohm +/- 1% }
  |     ^^^^^^^^^^^^^
```

```csv file=catalogs/resistor.csv set=unit
mpn,resistance,max_power,package,source
ACME-R316,316V +/- 1%,62.5mW,0402,https://example.invalid/acme-r316.pdf
```

```nsm fails catalogs=unit
import std::parts::passive::Resistor

board Unit {
    .r1: Resistor { .resistance &= 316ohm +/- 1% }
}
```

```text
error: `r1` cannot be chosen from the `resistor` catalog: catalogs/resistor.csv:2: `resistance` of `catalog resistor`: `316V +/- 1%` is in V, but this column holds Ω
  --> main.nsm:4:5
  |
4 |     .r1: Resistor { .resistance &= 316ohm +/- 1% }
  |     ^^^^^^^^^^^^^
```

Messages.

When no candidate fits the constraints, the catalog has nothing:

```nsm fails
import std::parts::passive::Resistor

board NoValue {
    .r1: Resistor { .resistance &= 12.34kohm +/- 0.1% }
}
```

```text
error: the `resistor` catalog has nothing for `r1`, which needs resistance within 12.327kΩ..12.353kΩ
  --> main.nsm:4:5
  |
4 |     .r1: Resistor { .resistance &= 12.34kohm +/- 0.1% }
  |     ^^^^^^^^^^^^^
```

The message points at the line that declares the part in your board, and names it by its path (`r1`, or `stage.regulator` when it is inside a module). When several parts are picked together and one of them has no candidate, that part is the one named, whatever the others could do.

When candidates fit but none keeps every relation true, the message names the closest one and the relation that failed:

```nsm fails
import std::parts::passive::Resistor
import std::sources::Rail

board TooTight {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
    }
    .top: Resistor {}
    .bottom: Resistor { .resistance &= 10kohm +/- 1% }
    .out: Rail { .voltage &= 3.3V +/- 5% }
    .supply.pos ~> .top ~> .bottom ~> .supply.neg
    .top.b ~ .out.pos
    .supply.neg ~ .out.neg
    .out.voltage == .supply.voltage * .bottom.resistance / (.top.resistance + .bottom.resistance)
}
```

```text
error: no combination of candidates satisfies `bottom` and `top` together: the closest, bottom = 10kΩ 1% 0603 C25804 and top = 5.1kΩ 1% 0603 C23186, fails because `out.voltage` must stay within 3.135V..3.465V, but ((supply.voltage * bottom.resistance) / (top.resistance + bottom.resistance)) works out to 3.124V..3.501V
   --> main.nsm:15:5
   |
15 |     .out.voltage == .supply.voltage * .bottom.resistance / (.top.resistance + .bottom.resistance)
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

Read it this way. The closest choice was 10 kohm over 5.1 kohm. Its output spans 3.124 V to 3.500 V, wider than 3.135 V to 3.465 V. To fix it, loosen the requirement, use tighter parts, or change the circuit. The solver will not pick a part that only works at the typical value.
