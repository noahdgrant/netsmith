# An environment and a grade for a board: options

Options note. It decides nothing and builds nothing. Language-design choices are the owner's (`AGENTS.md`), so every spelling below is an option to pick from. Snippets marked `nsm-new` use constructs invented to compare options, including a Celsius literal (`-40C`, see 1.1); snippets marked `nsm-fragment` use only what the language has today. None was run. Part numbers, ratings and standards in the examples are made up or marked unverified. Vocabulary follows `GLOSSARY.md`. Two collisions with it are listed under the questions.

The direction is the owner's, decided on 2026-11-09: a board-level `.environment` member, like `.process` (ADR 0008); a manifest default, like `assembly` (ADR 0028); and the solver rejects catalog rows that do not meet it. This note is about what is inside that.

Sources read: ADR 0008, 0022, 0028, 0051, 0054, 0055, 0057, 0058, 0065, 0073, 0074, 0078, 0080, 0085, 0086, 0091; `std/pcb.nsm`, `std/sources.nsm`, `std/parts/passive.nsm`, `std/parts/diode.nsm`, `std/parts/regulator.nsm`; `docs/LANGUAGE.md` ("Projects and netsmith.toml", "Who assembles a part", "Catalog files"); `data/jlcpcb/README.md` and the 17 files of `data/jlcpcb/catalog/`.

## Summary

- **Today nothing carries a temperature.** No std part and no catalog column states a temperature range or a qualification. The one temperature in a part is the Zener's optional `.temperature_shift` (ADR 0080). Ratings in the rows are at 25 C and not derated (resistor `max_power`, the fuse row, the Zener `max_power`). Temperature "is not modelled" (guide, bias curves).
- **What an environment states (section 1).** A temperature range, a qualification, and the supply's transients. I recommend std bundles of one `Environment` interface (`std::env::Automotive` and so on), each stating its numbers as `?=` offers that a board replaces, because a narrowing with `&=` can only make the requirement weaker. The transients stay on the `Rail` (ADR 0086, 0091) and the board ties them to the environment by one line, because a bundle cannot cite a source for them.
- **How a part meets it (section 2).** Three built-in optional columns on every catalog, `temp_min`, `temp_max` and `qualification`, as `assembly` already is. Two columns and not one range, because a range in this language means "somewhere in". The check is two relations that already mean the right thing (`environment.temperature <= .temp_max`). A row with an empty cell is offered and named in a guideline finding, not dropped and not passed. A qualification is an enum of families (`None`, `Aec`), not of standards.
- **Derating (section 3).** A margin is the design's policy, not the environment's physics, and no std bundle can state one as always true (ADR 0085 refused a default of 1.25 for that reason). I recommend a nested `.environment.derating` interface whose factors default to 1, stated by the board or by a project's own bundle. Today the one voltage rule is in `BuckStage`; a plain `Capacitor` checks no rating against its bias.
- **Manifest (section 4).** `environment = "std::env::Automotive"` under `[project]`; a board's own `.environment` replaces it whole.
- **Space (section 5).** Public supplier data has no radiation, outgassing or screening figures. A design can claim a temperature range was checked and a family was matched. It cannot claim the rest, and the tool should print that list.
- **Data (section 6).** 351 catalog rows. 46 already print a temperature range in the JLCPCB listing text of their `notes`; 25 more cite a datasheet and state none; 280 (all 219 resistors and 61 capacitors) have only the supplier records and none in their notes. One row states AEC-Q200; one says it is not the qualified grade.

## 0. What exists today

| Thing | Where | What it does |
|---|---|---|
| `.process` | ADR 0008, `std/mfg/jlcpcb.nsm`, lowering reads the member named `process` | A board member of an interface with no pins. The library states a range that is the least the factory can make; the board narrows it upward. No default process. |
| `assembly = "fab"` | ADR 0028, guide "Projects and netsmith.toml" | An optional `[project]` key; one word, others refused. Five-step order from the built-in rule to the design's `&=`. No per-board default. |
| Optional columns | ADR 0055, 0058, 0073, 0078 | A `catalog` column marked `optional` may be missing from a file or empty in a row. Such a row is offered to a design that says nothing about the field, and not offered to one that states it (`asked`: a narrowing, a relation, a `~` join or a balance). Any other empty cell takes the row out. |
| `assembly` as a catalog column | ADR 0028 row 4 | A built-in member of every part that any catalog may give a cell for, without the part listing it. This is the precedent for a built-in column in 2.1. |
| What cannot be judged is named | ADR 0057, 0065 | A relation with a value nothing states is a note; an open circle, an unconstrained pick and an unstated value are guideline findings: `warning:` by default, an error with `--strict` or `guidelines = "error"`. |
| Enum | ADR 0054 | A fixed list of variants, `==` and `&=` only. A mistyped variant is refused where it is written. No order, no set. |
| `.peak`, `.reverse_peak` | `std/sources.nsm`, ADR 0086, 0091 | Per `Rail`. A part in a `~>` chain with `.supply_peak` or `.supply_reverse_peak` is given the rail's value by the checker. The 35 V and 100 V of a real 12 V automotive board are stated by that board as its own assumptions. |
| Voltage margin | ADR 0085, `std/parts/regulator.nsm` | `BuckStage.voltage_margin ?= 1`; the board writes `.reg.stage.voltage_margin &= 1.25`. A rail-level margin and a default of 1.25 were both rejected. |
| `Temperature` | `crates/netsmith-quantity/src/unit.rs` | The one unit is the kelvin. A rise is a difference. There is no absolute-temperature literal (ADR 0080). |
| Zener | `std/parts/diode.nsm`, ADR 0080 | `.temperature_shift: Temperature optional`, kelvin from 25 C (`-65K..100K` is -40 C to 125 C). The corners come from the ends of the range. |
| Track heating | `.process.max_temperature_rise`, ADR 0036 | The rise of a loaded track above the board. The board's own temperature is not an input. |

One finding while reading. A plain `Capacitor` (`std/parts/passive.nsm`) has `.bias` and `.max_voltage` and no relation between them. The rule that a capacitor is rated for its rail (ADR 0058) is stated in `BuckStage` only (`regulator.nsm` lines 279 to 282). A bypass capacitor on a 3.3 V rail is rated by nothing unless the board writes the line. I read the files; I ran nothing.

## 1. What an environment states

### 1.1 A temperature range

An environment's temperature is a range of the board's surroundings, `-40C..125C`. A range is the right shape here, and it is the shape the Zener already uses: a range gives both corners, and the relations below read the ends without a way to name them (ADR 0058: `a >= b` holds when the lowest of `a` is at least the highest of `b`, so `.environment.temperature <= .temp_max` says the hot end is under the part's limit).

The unit needs a decision first. The language has kelvin only.

- **K1. Absolute kelvin.** `233.15K..398.15K`. Works today. Subtracting `298.15K` gives `-65K..100K`, the Zener's own example, so the Zener could read it. Ugly to write and to read in a datasheet comparison.
- **K2. A Celsius literal.** `-40C..125C`, meaning an absolute temperature. A language change: ADR 0080 declined an absolute literal, and `C` is a unit with an offset, which no unit has today (`10K` is a rise and a temperature alike).
- **K3. A shift from 25 C**, as the Zener does (`-65K..100K`). Works today. It is not what a datasheet prints, and a part's `.temp_max` in the same shift would be a number nobody can read off a page.

Recommendation: K2, and K1 as what it means underneath. The examples below use `C` and are `nsm-new` for that reason.

### 1.2 The supply's transients

A vehicle's supply has a load dump (a positive transient) and a reverse battery (a negative one). Today they are per rail: `.input.peak`, `.input.reverse_peak`. Three ways to put them under the environment:

- **T1. They stay on the rail.** The board states them, as it does now. The environment does not carry them.

```nsm-fragment
.input: Rail { .peak &= 35V; .reverse_peak &= 100V }
.fuse: Fuse
.input.pos ~> .fuse ~> .protect ~> .reg.vin.pos
```

- **T2. The environment states them and the board ties the input rail to them.**

```nsm-new
.environment.supply_peak &= 35V
.environment.supply_reverse_peak &= 100V
.input.peak &= .environment.supply_peak
```

- **T3. The environment applies them to every rail nothing on the board drives** (a rail "supplied from outside", `std/sources.nsm`). No tie line. A 5 V rail from a USB connector on the same board would be given 35 V, so the board would need a way to say "not this one".

Cost: T1 is nothing new and loses nothing. T2 adds a line per input rail and lets a bundle carry the numbers. T3 is implicit and wrong for a board with two supplies.

Recommendation: T1 for now. The 35 V and 100 V are a board's assumptions about its own harness (ADR 0086 and 0091 say so), and the standards that define a vehicle's transient pulses are not in the repository and I did not read them, so a std bundle cannot cite a source for them (`docs/DESIGN-RULES.md`: "A part number needs a source", and a library states only what is always true). Revisit as T2 if a bundle can cite one.

### 1.3 Is a named bundle a std definition?

Three shapes.

**A. One interface, no bundles.** The board states every number.

```nsm-new
.environment: Environment
.environment.temperature &= -40C..125C
.environment.qualification &= Qualification::Aec
```

**B. Std bundles of one interface**, imported like `TwoLayer`. Each bundle offers its numbers with `?=` (ADR 0051), so a board replaces them.

```nsm-new
import std::env::Automotive
.environment: Automotive        // offers -40C..125C and Qualification::Aec
.environment.temperature &= -55C..150C   // replaces the offer: this board is hotter than the bundle
```

**C. Two members**, the environment (what the board sees) and a grade (what its parts must be and how they are used).

```nsm-new
.environment: Cabin { .temperature &= -40C..85C }
.grade: Automotive              // qualification and derating
.grade.derating.voltage &= 1.25
```

Why `?=` and not `&=` in a bundle. A narrowing only intersects. `.process.min_track &= 0.15mm..1m` asks for more than the library offers, so it narrows. For a temperature it is the other way: `-40C..125C` narrowed to `-40C..85C` asks for less, and a board that needs `-55C..150C` could not narrow `-40C..125C` at all (the intersection is not what it wants, and an `&=` outside the range is a contradiction). A `?=` default is an offer the board replaces, which is what ADR 0051 is for.

| | Cost | Gain |
|---|---|---|
| A | No std to maintain or source. Every board repeats three lines, and the manifest default can only be values (section 4). | Nothing a bundle name could mislead about. |
| B | Each bundle's numbers need a recorded source, and a name such as `Automotive` claims something. | Follows `.process` and the owner's direction. One word for the common case. |
| C | Two members to learn; splitting later from B is a breaking change. | Physical facts and policy are apart, so a cabin board can be `Automotive` grade. |

Recommendation: B, with the grade's policy part nested inside the environment as `.derating` (section 3), so that C stays a lift of one nested interface to a member and not a redesign. B follows the owner's direction. Bundles to start with: `Commercial`, `Industrial`, `Automotive`, each stating a temperature range and a qualification only, and each with a doc comment that says where its range comes from. A bundle whose range cannot be cited is not shipped; a project writes its own (section 4).

A note on names. Commercial and industrial temperature ranges are conventions of manufacturers, as far as I know; I did not find them in the repository and did not verify the usual figures (0 C to 70 C, -40 C to 85 C). Treat any figure in a bundle as unverified until it has a source.

## 2. How a part meets it

### 2.1 Columns, and why two

New optional columns, as the task suggests: `.temp_min`, `.temp_max` (the operating range the datasheet states, as `Temperature`) and `.qualification` (section 2.4).

Two columns and not one range member. A range as a value in this language means "the value is somewhere in it" (a tolerance, a spread). A part's `-40C..85C` stored as one range would be read as an uncertain figure, and `environment.temperature <= .operating_range` would compare against its lowest end. Two points have no such problem, and a datasheet prints two numbers.

Where the columns live. Three ways to get them to every part:

- **M1. A filter in the solver, nothing in `.nsm`.** The solver compares a row with the environment while it picks. The rule is invisible in the language and breaks "a part states its own requirements" (`docs/DESIGN-RULES.md`); a part with no catalog (`Header2x5`) is not reached.
- **M2. Built-in members of every part, with a column any catalog may fill**, as `.assembly` is (ADR 0028, row 4). The checker adds the relations. The columns need no `catalog` statement entry and no `optional` marker; they are optional by being built in.
- **M3. Each part kind declares them and the checker passes the environment by a field name**, as `supply_peak` is (ADR 0086).

```nsm-new
// M3, in each kind that has a range
.temp_max: Temperature optional
.temp_max >= .environment_temperature   // `environment_temperature` is given by the checker
```

M3 repeats one declaration in every kind, and a kind that forgets it is judged by nothing without a word. M1 hides the rule. M2 is one place, visible in the guide, and has a precedent.

Recommendation: M2. A part's range is true of every part (a resistor, a header, a crystal), which is what a built-in member is for. Parts that are not fitted (`AssemblyMode::None`: holes, fiducials) are exempt; a part soldered by hand or consigned is not, because it sees the same air.

### 2.2 The comparison

What the checker would add for every fitted part (written in the language's own terms; `>=` means "always at least"):

```nsm-new
.environment.temperature <= .temp_max     // the hot end is under the part's limit
.environment.temperature >= .temp_min     // the cold end is above the part's limit
.qualification == .environment.qualification   // only when the environment names a family
```

Three limits on what this proves, to be said in the guide and in the finding:

1. **Which temperature.** A datasheet's range may be the ambient, the junction or the storage temperature. The JLCPCB listing text of the diode rows reads `-55℃~+150℃` (the zener and TVS rows of `notes`) and does not say which. For a diode in an environment of 125 C that matters, because the junction is hotter than the air. A column that holds "the top operating temperature as printed" compares the wrong thing for such a part. The column could say which (a second column, or the word in `notes`), or v1 could take only ranges the datasheet calls ambient.
2. **Ratings at 25 C.** `max_power` of a resistor ("at the ambient temperature the datasheet names, not derated"), the fuse's hold current (the 1206HV row keeps "the 25 C value" and cites the derating curve on page 2), the Zener's `PD` are all 25 C figures. A range check says the part works at 125 C. It does not say it carries its 25 C rating there.
3. **No self-heating.** Nothing here adds a part's own dissipation to the environment.

So the check is necessary and not sufficient, which is the honest reading of "rejects rows that do not meet it".

A catalog row can carry a curve, and the capacitor's `derate capacitance by bias: 0:1 5:0.5` is the model (ADR 0022). A `derate rated_current by temperature: ...` cell would be the same shape for the fuse. It is a later step with its own sources and is not proposed here.

The capacitor's `Dielectric` enum already implies a range in practice: X5R, X7R and C0G have defined temperature limits in the EIA classes (I recall X5R as -55 C to 85 C and X7R as -55 C to 125 C; unverified and not in the repository). If the owner accepts a table in std, 61 capacitor rows would get a range from the class with no datasheet read, and an X5R capacitor in a 125 C environment would be refused. It would be a second source for one number (ADR 0001), so I would not do it before the standard is read and cited.

### 2.3 A row that does not state them

The standard rows state nothing today, and a design that states an environment asks for it by every route ADR 0058 counts. Three options.

- **R1. Not offered.** The rule ADR 0058 applies to an optional column a design asks for. No row has the new columns yet, so with an environment stated none of the 351 rows would be offered, and a board that states `Automotive` would get "the `resistor` catalog has nothing for `r1`" until rows are filled. For 280 of them no figure is recorded anywhere in the repository (section 6).
- **R2. Offered, and named in a guideline finding.** A pick whose row states no range is kept and the finding says so, with what to write (ADR 0057: what cannot be judged is named; ADR 0065: a guideline, `warning:` by default, an error under `--strict`). A row that states a range the environment is not inside is refused, as the owner decided.

```text
Dash: environment guideline: `reg.stage.input_cap` was picked as 10uF 25V X7R 0805 C15850 and its catalog row states no temperature range, so it is not judged against the board's environment (-40C..125C, std::env::Automotive); add `temp_min` and `temp_max` to the row from the datasheet, or state them on the part
```

- **R3. R2, and a row that states a range is tried before one that does not.** The selection policy of ADR 0022 item 6 is file order; this would put the rows that meet the environment first for a board that states one. A change to what the file order means.

Cost. R1 follows an existing rule and is the least code. It makes the feature unusable on the current catalogs. R2 is a new rule: an exception to ADR 0058's "asked means filled", for an ask the board made implicitly through the environment. R3 is R2 plus the policy change, but it is the only one where a board gets a part that is known to be in range whenever one exists. A generic part (ADR 0047, no row at all) would be unjudged under R2, and it already stops `fab` until a row is found.

Recommendation: R2, and decide R3 later with the data in hand. For `qualification` the same rule, because an unknown row is not a qualified one and the finding says exactly that. How strict the finding should be for a board that states a family is question 8.

### 2.4 Matching a qualification

AEC-Q100 (integrated circuits), AEC-Q101 (discrete semiconductors) and AEC-Q200 (passives) are the families a vehicle board meets; the repository itself cites two of them (the 1206HV fuse row: "AEC-Q200 automotive grade (datasheet p.1)"; the Vishay SS3H10 row: "the AEC-Q101 qualified grade is another ordering code and is not this row"). I did not read the standards. AEC-Q100 and AEC-Q200 also define temperature grades; I recall Grade 1 as -40 C to 125 C but have not verified any of them.

The second quote matters: a qualification belongs to an orderable, not to a part kind. So it is a catalog column.

Three options for its type.

- **Q1. Free text or a list of words** (`AEC-Q200`, or `AEC-Q200,Grade 1`), as `Keepout.layers` holds a comma list in a `String` (`std/pcb.nsm`). Matches by text. A typo is a row nothing matches, and the message says "nothing for", not "no such standard" (ADR 0054 gave this reason for the enum).
- **Q2. An enum with a variant per standard** (`AecQ100`, `AecQ101`, `AecQ200`). A typo is refused where it is written. But an environment states one variant, and a board that needs "automotive" needs a different one for each kind of part. An environment that accepts a set of variants needs a set type, which the language does not have (ADR 0055 said so about package sizes).
- **Q3. An enum of families** (`Qualification { None, Aec }`). The row says `Aec`; which standard and grade is in `notes` with the datasheet page, as the Zener and fuse rows record their figures. The environment says `Aec` or `None`. The match is `==` for `Aec` and always passes for `None`.

```nsm-new
enum Qualification { None, Aec }
// catalog cell:  qualification = Aec          (notes: AEC-Q200, datasheet p.1)
.environment.qualification ?= Qualification::Aec
```

Cost of Q3: the standard and its grade are not machine-readable, so a board cannot ask for Grade 0 by this column. The temperature range already says what a grade says, and the range columns are numbers, so the loss is small. A second enum variant (`Space`, a military class) is a library change, and ADR 0054 treats that as the point of a closed list.

Recommendation: Q3. Nothing is derived from the range or the reverse, in either direction: a part's printed range can exceed its grade, and a table that maps one to the other needs a standard I cannot cite.

## 3. Derating as policy

A margin is a choice of the design or the company. 1.25 on a capacitor's voltage in a real 12 V automotive board is its own rule (ADR 0085 begins there). The environment's temperature is a fact about where the board sits. So the factors do not belong with the temperature, and ADR 0085 already declined two nearby things: a margin on the `Rail` ("a rail has no reason to know how parts are derated") and a default of 1.25 ("it changes every existing board").

Options:

- **D1. The factors are in the environment bundle.** `Automotive` states `.voltage 1.25`. A bundle would claim a number nobody has published as always true.
- **D2. The factors belong to a grade** (shape C): `.grade.derating`. Honest about what they are. Needs the second member.
- **D3. A nested interface in the environment, default 1, stated by the board or by a project's own bundle.**

```nsm-new
.environment: Automotive
.environment.derating.voltage ?= 1       // in the interface; a board writes `&= 1.25`
.environment.derating.power ?= 1         // in the interface, the same way
```

Recommendation: D3. It keeps ADR 0085's default of 1 (so every existing board is unchanged), puts the number where the board states it, and leaves C open. A project that has a company rule writes its own bundle (`import std::env::Automotive`, then a definition of its own with `.derating.voltage ?= 1.25`), which is allowed because the project, not the library, is the one claiming it.

### How it relates to what exists

- **The buck stage.** `voltage_margin` is a member of `BuckStage`, and a module cannot see the board (ADR 0004). The smallest link is a line per stage, `.reg.stage.voltage_margin &= .environment.derating.voltage`, which is no better than today's `&= 1.25`. A stage that read the board's environment without a line would be a project-wide value reaching into a module the way the manifest's `assembly` reaches every part (ADR 0028), but it would be the board's member and not the manifest's, which is new.
- **The capacitor rule.** Only `BuckStage` checks `.max_voltage` against the rail (section 0). If the owner wants the margin to apply to every capacitor, `Capacitor` needs a relation `.max_voltage >= .bias * <margin>`. With the default 1 that adds a check to every board that has none. A board whose bypass capacitor is rated below its rail passes today and would fail then. That may be the point, and it is not nothing: it needs the guide, the golden outputs and the examples checked.
- **A rail's peak.** `BuckStage` multiplies the margin by both `.voltage` and `.peak`, and ADR 0085 explains why (a rail with no stated peak would otherwise skip the margin). The same reading applies.

### Which of today's checks would read the environment

| Check | Reads the environment for |
|---|---|
| New: part range | `temperature` against `.temp_min`, `.temp_max` |
| New: qualification | `qualification` against the row's |
| Zener `.temperature_shift` | Could be `.environment.temperature - 25C` instead of a member the design states (one way to say it, ADR 0001). Kept optional meanwhile. |
| `BuckStage.voltage_margin` | `derating.voltage`, by a forwarded line |
| Plain `Capacitor` rating | Nothing today. `derating.voltage` if a rule is added (above) |
| Resistor `max_power` check | `derating.power`; the 25 C rating is the same caveat as 2.2 |
| `Fuse` | `derating.current` against the hold current at 23 C; the temperature curve is not in the row |
| `Rail.peak`, `.reverse_peak` | Only under T2 |
| Track width (ADR 0036) | Nothing. The rise is above the board; the process states no laminate limit for a hot board. Unverified whether one is needed. |
| Capacitor effective capacitance | Nothing. "Temperature and ageing are not part of it yet" (glossary). |

## 4. The manifest default

Spellings, following ADR 0028:

- **M-a. A key naming a bundle.** `environment = "std::env::Automotive"` under `[project]`, or `"src/env.nsm::MyAutomotive"` for a project's own. Fits shape B. It is the first manifest key whose value is a definition path and not one of a closed list of words; ADR 0028 refuses any other word and names the allowed ones, which an open list cannot do.
- **M-b. A table of values in the language's own syntax**, as a catalog cell is (ADR 0022 item 3).

```toml
[environment]
temperature = "-40C..125C"
qualification = "Aec"
```

  Fits shape A. If bundles also exist it is a second way to say the same thing (ADR 0001).
- **M-c. No manifest default**, a board states its own. Not the owner's direction.

Recommendation: M-a, with shape B.

The order, weakest to strongest, in the style of ADR 0028:

| | Says | Example |
|---|---|---|
| 1 | Nothing | No environment: nothing is checked, as today |
| 2 | The manifest's `environment` | `environment = "std::env::Automotive"` |
| 3 | The bundle's own offers | `.temperature ?= -40C..125C` in `Automotive` |
| 4 | The board | `.environment: Industrial`, or `.environment.temperature &= -55C..150C` |

Which wins when the board states one. The board states the last word, and replaces the manifest's bundle whole: a board that writes `.environment: Industrial` is `Industrial`, not the intersection with `Automotive`, as a stated `&=` drops a `?=` default and does not intersect it (ADR 0051). A board that writes only `.environment.temperature &= ...` and no declaration has the manifest's bundle as if it had written the declaration, a member nobody wrote, as the built-in `.assembly` is. There is no per-board default in the manifest (ADR 0028: one default for the project).

Cost, stated plainly: a project that needs `Automotive` everywhere has no way to forbid one board from writing `.environment: Commercial`. The manifest default is a convenience and not a floor. ADR 0008 said "there is no default process" because netsmith will not choose a factory for a board. An environment default is the project's choice, not netsmith's, so the two do not conflict, but the difference is worth a sentence in the guide.

A board with neither a manifest key nor a member has no environment: no check, no finding. Printing a finding for it would put a line on every existing board and example. Question 9 asks.

Not covered: a `system` of boards (ADR 0010). Each board has its own environment in v1. A system whose boards sit in one vehicle but in different places (engine bay, cabin) is a reason not to give the system one.

## 5. Space and other strict grades

What the data in this repository can support. The catalogs hold what the JLCPCB and LCSC records print and what a datasheet table gives for the numbers the models use. Their `notes` carry a line such as `JLCPCB lists -55℃~+150℃ 1A 25A 300uA@40V ...`. No row states a radiation figure, an outgassing figure, a screening level, a lot or date code, a finish or a qualification other than the two AEC mentions. I did not read JLCPCB's or LCSC's pages for any attribute beyond those recorded.

What a design could claim, once the columns exist and are filled:

- Every fitted part's printed operating range covers the environment's range (necessary, not sufficient, 2.2).
- Every fitted part's row says it is in a family (`Aec`).
- Which picks were not judged, by the findings of 2.3.

What it cannot claim, and the tool should print as such. From what I know and have not verified, a strict space grade asks about at least:

- **Radiation**: total ionizing dose and single-event effects. Per part type and often per lot; not in any public record I know of.
- **Outgassing**: a test of a material (I recall ASTM E595 with limits on total mass loss and collected volatile condensable material; unverified). Not a field of a catalog row.
- **Screening and lot traceability**: military and space part classes (I recall MIL-PRF-38535 Class Q and V for integrated circuits and ESCC specifications; unverified) and lot acceptance. A property of the lot bought, not of the part number.
- **Derating standards**: ESA and NASA publish derating rules for space parts (I recall ECSS-Q-ST-30-11 and NASA's EEE-INST-002; unverified, and I did not read their factors). I will not state any.
- **Process and board class**: whether a factory can build to a stricter workmanship class than `std::mfg::jlcpcb::TwoLayer` states. Not in `TwoLayer` today. Unverified for JLCPCB.
- **Environmental tests**: vacuum, thermal cycling, vibration, shock. Not part properties at all, but system tests.

Recommendation. Do not ship a `Space` bundle. A name that says "space" over a temperature range alone would claim what the tool cannot know. A project that has the data (its own rows with the missing columns, in `catalogs/`) writes its own bundle and its own enum variants. Separately, `fab-notes.txt` and the build output could print a short line for any board with an environment, in the style of the existing fab notes: what was checked and for how many parts, what was unjudged, and the sentence "radiation, outgassing, screening and lot traceability are not checked". That is cheap, and it is the claim this tool can honestly make.

## 6. What data the catalogs need

The rule is `data/jlcpcb/README.md` and `docs/DESIGN-RULES.md`: no number enters a catalog without its recorded source. Here that is `datasheet:<url>` with the table and page in `notes` (as the Zener and fuse rows do), or `JLCPCB and LCSC part records, read <date>` for a figure the records print. The columns an environment needs are `temp_min`, `temp_max` (both with the kind of temperature in `notes`, 2.2) and `qualification`.

Counted from `data/jlcpcb/catalog/*.csv` with a CSV reader over the files:

| File | Rows | Temperature range already printed in `notes` (JLCPCB listing text) | Datasheet URL in `source`, no range in `notes` | Supplier records only |
|---|---|---|---|---|
| `resistor.csv` | 219 | 0 | 0 | 219 |
| `capacitor.csv` | 61 | 0 | 0 | 61 |
| `led.csv` | 14 | 0 | 14 | 0 |
| `tvs.csv` | 10 | 10 | 0 | 0 |
| `fuse.csv` | 8 | 6 | 2 | 0 |
| `header.csv` | 8 | 8 | 0 | 0 |
| `zener.csv` | 8 | 8 | 0 | 0 |
| `schottky.csv` | 7 | 5 | 2 | 0 |
| `inductor.csv` | 4 | 0 | 4 | 0 |
| `crystal.csv` | 4 | 4 | 0 | 0 |
| `ferrite_bead.csv` | 3 | 3 | 0 | 0 |
| `n_mosfet.csv` | 2 | 1 | 1 | 0 |
| `tvs_bidirectional.csv` | 2 | 1 | 1 | 0 |
| `ldo.csv` | 1 | 0 | 1 | 0 |
| `level_translator.csv`, `lt8641.csv`, `p_mosfet.csv` | 0 | | | |
| **Total** | **351** | **46** | **25** | **280** |

Reading the table.

- **46 rows** print `℃~℃` in the `JLCPCB lists ...` line of their `notes`, recorded from the record read on 2026-11-01 (and 2026-11-07 to 2026-11-09 for the later rows). That is a recorded source. The figures differ by part (`-40℃~+85℃` for the four crystals and six fuses, `-40℃~+105℃` for the headers, `-55℃~+125℃` for the ferrite beads, `-55℃~+150℃` for the Zeners, `-65℃~+150℃` for the TVS diodes), and none says which temperature it is. Before they become a column, each needs a check against its datasheet's table, and the datasheet URL is already in `source` for all 46.
- **25 rows** have a datasheet and no range in `notes`: the 14 LEDs, the 4 inductors, the LDO, the two one-time fuses (`1206HV-3A`, `1206T3A63V`), two Schottky (`1N5819WS`, `B5819W SL`), the `2N7002` and the `SMDJ36CA`. The range is probably in the datasheet's maximum ratings table (I have not opened them).
- **280 rows** are the 219 resistors and 61 capacitors, whose `source` is "JLCPCB and LCSC part records" and whose `notes` do not print a range. Whether those records carry an operating temperature, I did not check. If JLCPCB's page lists one (I believe it does for resistors and ceramic capacitors; unverified), these 280 rows could be filled from the record by a read like the one that filled `dielectric` for 58 rows from the listing text (ADR 0055). The 280 are 80 % of the catalog and all the passives that boards use most, so this is what decides whether R2 produces a handful of findings or hundreds.
- **Qualification.** One row states a qualification (`1206HV-3A`, AEC-Q200, datasheet p.1). One row says it is *not* the qualified grade (`SS3H10-E3/9AT`). The other 349 say nothing. A reading of each family's datasheet for "AEC" is needed for the 71 rows with a datasheet; the 280 would need the record or the maker's page.

How many could state a range from a datasheet, if you can tell: at most the 71 that cite one, after reading each table; 46 of them already show a figure from the record to check it against. For the other 280 I cannot tell from the repository.

A project that does not wait for the standard rows can add the columns in its own `catalogs/*.csv`; a file that leaves a built-in column out of its first line is read as empty there (ADR 0055's rule).

## 7. Questions for the owner

Each answers in one word or one choice. My recommendation is first.

1. **Temperature unit.** K1 kelvin, K2 a Celsius literal, or K3 a shift from 25 C?
2. **Bundles.** Shape B (std bundles of one interface), A (no bundles), or C (two members)?
3. **Transients.** T1 (stay on the rail), T2 (environment states, rail ties), or T3 (applied to every externally supplied rail)?
4. **How a part sees it.** M2 (built-in members and columns), M1 (a solver filter) or M3 (each kind declares)?
5. **A row that states nothing.** R2 (offered and named), R1 (not offered), or R3 (R2 and rows that state a range first)?
6. **Qualification type.** Q3 (enum of families), Q1 (words), or Q2 (enum per standard)?
7. **Derating.** D3 (nested, default 1), D2 (a grade member), or D1 (in the bundle)?
8. **Finding severity** for an unjudged pick when the environment names a family: warning (as ADR 0065), error, or error only under `--strict`?
9. **A board with no environment** and no manifest key: silent, or a finding?
10. **Manifest spelling.** M-a (a path to a bundle), M-b (a table of values), or none?
11. **Which temperature** a part's column holds: as printed with the kind named in `notes`, or only when the datasheet says ambient?
12. **Capacitor class as a source** of a temperature range (X5R, X7R, C0G), once the standard is cited: yes or no?
13. **A `Space` bundle in std.** No (recommended) or yes?
14. **The Zener's `.temperature_shift`** reads the environment when the board states none: yes or no?
15. **Vocabulary.** The glossary's `Temperature shift` entry lists "Ambient" and "temperature range" under _Avoid_, and `Voltage margin` lists "Derating factor". This note uses "temperature range" for the new members, "environment" for the board's surroundings and "margin" for the factor. Keep those, or rename?
16. **A `system`** of boards: each board its own environment (v1), or one for the system?

## Not verified

Everything here is from my memory and not in the repository or read from a source: the temperature limits of X5R, X7R and C0G; the temperature grades of AEC-Q100 and AEC-Q200 and that Q100, Q101 and Q200 cover integrated circuits, discrete semiconductors and passives; the usual commercial and industrial ranges; ASTM E595, MIL-PRF-38535, ESCC, ECSS-Q-ST-30-11 and NASA EEE-INST-002 and what they cover; that JLCPCB's pages list an operating temperature for resistors and ceramic capacitors; whether JLCPCB builds to a stricter workmanship class; whether a laminate limit matters for a hot board. Read the standard before any of it enters a bundle or a column.

I did not run any build or test. The counts in section 6 are from a script over the CSV files at `origin/main` (commit `4c6b889`); the script is not committed.
