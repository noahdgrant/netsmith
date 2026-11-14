## Generics and providers

A `part` is exact: `.rf: AcmeMicroCoax` is that connector and no other. Often a board does not care which part it is, only what the part must be: a coaxial receptacle that reaches 1.61 GHz. A **generic** states that, and the parts and modules that **provide** it say they can stand in its place (ADR 0068). The board writes a **use**, `.rf: CoaxReceptacle { .max_frequency >= 1.61GHz }`, and the solver picks the first provider for which the whole design holds, and names it.

The layers are part, then generic, then module: a board can name one part, name a generic and put requirements on it, or use a module built from either. Each is complete on its own.

### A generic

A generic is written like a part, with what is always true of every provider and nothing that only one has: pins with their kinds, values with their types, and the checks that hold for all of them. It has no pads and no package.

```nsm module=src::rf
/// A coaxial receptacle that carries an antenna lead off the board.
generic CoaxReceptacle {
    .designator &= "J"

    pins {
        .signal passive required
        .shield passive required
    }

    /// The characteristic impedance. Every receptacle of this generic is 50 ohm.
    .impedance: Resistance
    .impedance &= 50ohm
    /// The highest frequency it is specified for.
    .max_frequency: Frequency
}
```

Rules.

- A generic holds values, pins and interfaces, and statements (`&=`, `?=`, `==`, `>=`, `<=`). It holds no parts and no modules, and no connections between parts.
- Its `pins` table has rows of a kind and no datasheet name and no pads: `.signal passive required`. The kind is what every provider's row must say, because the kind decides what the checker asks of the board. `required` and `optional` hold for the provider's row too, and a provider may leave the word out.
- A row may also end with the pin's limits, `.in input abs_max 6V` (`abs_min`, `abs_max` and `from`, as in "Several packages, and the absolute limits in the row"). They are a contract of the providers (ADR 0126): a provider whose row for the pin states no `abs_max` has the generic's, and likewise `abs_min`, written into its row. A provider that states its own must be at least as strict, `abs_max` no higher and `abs_min` no lower. A value that waits for a pick, `abs_max .in_max`, is the provider's own member.
- A relation or a narrowing in a generic holds for every provider and for every use. The solver judges it with the values of whichever provider is tried.
- A generic does not declare `.mpn` or `.lcsc`: every part has both, as text, and the checker refuses a declaration with an error. A provider whose catalog lists `.mpn` as a column has the number in its row, and a board that states `.mpn &=` finds that provider.
- A doc comment (`///`) goes before a declaration, not before a statement; use `//` there.

### A provider

`part X provides G` (or `module X provides G`) says that `X` can stand wherever `G` is used. `X` is a part as written today: its own pin table, its own numbers, its own one-row catalog. It has every member and every statement of the generic and does not declare them again.

```nsm module=src::parts::acme_micro_coax
import src::rf::CoaxReceptacle

/// ACME-MC-1, a surface-mount micro coaxial receptacle, DC to 6 GHz. Invented.
part AcmeMicroCoax provides CoaxReceptacle {
    pins {
        //  pin     kind     name      pads
        .signal     passive  "SIGNAL"  "1"
        .shield     passive  "GND"     "2" "3"
    }
    .max_frequency &= 6GHz

    .package: Package
    catalog acme_micro_coax { .mpn, .package }
}
```

```csv file=catalogs/acme_micro_coax.csv set=coax
mpn,package,library,source
ACME-MC-1,ACME_MC_1,Extended,https://example.invalid/acme-mc-1.pdf
```

A second provider, a plainer part that stops at 1 GHz:

```nsm module=src::parts::acme_basic_coax
import src::rf::CoaxReceptacle

/// ACME-BC-1, a through-hole receptacle, DC to 1 GHz. Invented.
part AcmeBasicCoax provides CoaxReceptacle {
    pins {
        .signal     passive  "SIGNAL"  "1"
        .shield     passive  "GND"     "2" "3"
    }
    .max_frequency &= 1GHz

    .package: Package
    catalog acme_basic_coax { .mpn, .package }
}
```

```csv file=catalogs/acme_basic_coax.csv set=coax
mpn,package,library,source
ACME-BC-1,ACME_BC_1,Extended,https://example.invalid/acme-bc-1.pdf
```

The project has both catalog files in its `catalogs/` folder and lists both parts in `kicad.toml`, as for any hand-written part. A provider whose own names differ from the generic's states the generic's names with the aliases and equations the language already has, and says nothing more:

```nsm module=src::parts::other_coax
import src::rf::CoaxReceptacle

/// A receptacle whose pin and top frequency have other names. Invented.
part OtherCoax provides CoaxReceptacle {
    pins {
        .rf      passive  "RF"   "1"
        .shield  passive  "GND"  "2"
    }
    .signal ~ .rf               // the generic's name for the pin this table calls .rf
    .fmax: Frequency
    .fmax &= 3GHz
    .max_frequency == .fmax     // the generic's name for the value this part calls .fmax

    .package: Package
    catalog other_coax { .mpn, .package }
}
```

Rules.

- A part or module may provide several generics: `part AcmeMicroCoax provides CoaxReceptacle, RfConnector`. Only a `generic` is provided; `board`, `system`, `panel`, `interface` and `enum` cannot say `provides`.
- Each pin of the generic has a row of the same kind in every `pins` table of the part, or is an alias of a pin that has a row of that kind: the row the alias is joined to is what the checker reads for the net, so it is the one compared (`.signal ~ .rf` with `.rf output` is refused where the generic says `.signal passive`, at the row of `.rf`).
- The providers of one generic are all parts or all modules: a generic is placed as the kind its providers have. A generic that has a `pins` table is provided by parts. A generic that is provided by parts but asks them for no pin writes an empty table, `pins { }` (the standard `Connector`, see _A connector of your own_).
- A provider is checked where it is written, as any part is, so an unused provider's mistakes show.
- No two providers of one generic have the same name, in whatever files they are: a part is known by its name in the bill of materials, in the `[[part]]` entries of `kicad.toml` and in the notes, so two would share a kind and a mapping. The later one (by the order below) is refused at its own name, with both module paths.
- A provider does not contain a use of the generic it provides, directly or through the parts and modules inside it: a pick that could take the provider being made would make it again inside itself. A provider that wraps `BuckConverter` in `.inner: BuckConverter` is refused at that line with the cycle named (`AFilteredBuck > AFilteredBuck`), whatever its name sorts as; wrap a part, or a module that does not provide the generic. Nor does any definition contain itself, directly or through another.

### A use

A board or module declares a member with the generic's type and puts requirements on it. The import of a provider's file is what makes the provider a candidate: it is not named in the board.

```nsm evaluate picks catalogs=coax
import src::rf::CoaxReceptacle
import src::parts::acme_basic_coax::AcmeBasicCoax   // not used by name: brings its file's providers in scope
import src::parts::acme_micro_coax::AcmeMicroCoax   // the same

module AntennaFeed {
    .signal: Pin
    .gnd: Pin

    // Any receptacle specified up to the radio's top frequency. The solver picks one.
    .rf: CoaxReceptacle { .max_frequency >= 1.61GHz }
    .rf.signal ~ .signal
    .rf.shield ~ .gnd
}

board Radio {
    .feed: AntennaFeed
}
```

```text
feed.rf is a CoaxReceptacle: picked AcmeMicroCoax; passed over AcmeBasicCoax (no `acme_basic_coax` candidate satisfies `feed.rf`: the closest of the 1 that fit, ACME-BC-1, fails because feed.rf.max_frequency must always be at least 1.61GHz, but 1GHz is not always above 1.61GHz)
```

Which providers are candidates, and in what order:

- A candidate is a provider in a file that the board's file imports, directly or through other files, or in the board's own file. The import list is the list: no second list is kept. An import used only to bring providers into scope is a plain import, and a comment says so.
- The order is fixed: the generic itself first when it is its own provider (it has pads or a catalog), then the providers of the standard library, then the project's, each by module path and then by name. It never depends on where a definition is written in a file, so moving one changes nothing. The module path of a file is its path under the project's root (`src/main.nsm` is `src::main`) whichever way its name is typed (`./src/main.nsm`, an absolute path), and a file that imports the file being built finds that file, held once, not a second copy of it.
- The pick is the first candidate for which every check of the whole design holds. Nothing compares candidates: a design that cares states a requirement or names the type. A provider that leaves a value open that the use puts a bound on (`.rf: G { .max_frequency >= 1GHz }` and a provider that never states `.max_frequency`) cannot be judged, which is not holding: it is passed over for one that can be, and if none can, the first stands and the build names the bound as `not evaluated`. What cannot be judged is: a bound or an equation that waits on a value inside the use that nothing states (of the use itself, of an interface or a part inside it, such as the datasheet number of the IC inside a module provider), and a logic level or an absolute maximum that could not be checked because a pin of the use states no value for it. A value the use shares with the board (the board's rail that `.vin` is joined to, whose `.peak` the board leaves open) is the design's, and is no reason to try another provider.
- The pick depends on the sources, the catalog files and `netsmith.toml` only: not on the KiCad libraries, the mapping, stock or the time. A provider with no entry in `kicad.toml` is not skipped; the build fails at export and names it.
- A failing design is tried with the other providers only for the uses that matter: a use is varied alone first, and one whose other providers change nothing about what fails is left at its first. At most 64 combinations are tried. When the search stops at that bound without an answer, the build prints the errors of the design as it stands first (the first solve's), and then `gave up after trying 64 provider combinations for ...`, naming the first six uses it did not reach and counting the rest: the bound is never the only thing said about a design that has a mistake of its own.

The three ways to say what part the board wants:

```nsm-fragment
.rf: CoaxReceptacle { .max_frequency >= 1.61GHz }   // a requirement: the solver picks
.rf: CoaxReceptacle { .mpn &= "ACME-MC-1" }          // the part's number: the solver finds its provider
.rf: AcmeMicroCoax                                   // the provider itself: nothing is picked
```

The last line changes only the type. Every other line that mentions `.rf` still holds, because a provider has every member of the generic with the same names. A use reaches only the members the generic declares, so that a board works with every provider; a path to anything else is an error (below).

A part number picks the provider whose catalog has the row, and passes over the ones that do not:

```nsm evaluate picks catalogs=coax
import src::rf::CoaxReceptacle
import src::parts::acme_basic_coax::AcmeBasicCoax
import src::parts::acme_micro_coax::AcmeMicroCoax

board Radio {
    .signal: Pin
    .gnd: Pin
    .rf: CoaxReceptacle { .mpn &= "ACME-MC-1" }
    .rf.signal ~ .signal
    .rf.shield ~ .gnd
}
```

```text
rf is a CoaxReceptacle: picked AcmeMicroCoax; passed over AcmeBasicCoax (`rf` is held by `.mpn &= "ACME-MC-1"` to a row of the `acme_basic_coax` catalog, and the catalog has no row with the manufacturer part number ACME-MC-1: no other row was tried for it; change the narrowing to a number the catalog has, or remove it to let the solver pick)
```

When no provider works, one error at the line of the use says so in a headline (the generic, the use and its line) and lists the providers in scope, in the order they were tried, each with the first requirement it failed and the numbers. A provider whose own pick from its catalog failed has the closest candidate and the requirement as a sub-list under it. The closest provider (the trial with the fewest failures; ties in candidate order) is marked. `--json` carries the same text in the finding's `message`, with the lines separated by newlines:

```nsm fails catalogs=coax
import src::rf::CoaxReceptacle
import src::parts::acme_basic_coax::AcmeBasicCoax
import src::parts::acme_micro_coax::AcmeMicroCoax

board Radio {
    .signal: Pin
    .gnd: Pin
    .rf: CoaxReceptacle { .max_frequency >= 10GHz }
    .rf.signal ~ .signal
    .rf.shield ~ .gnd
}
```

```text
error: no CoaxReceptacle provider satisfies `rf` at main.nsm:8: none of the 2 in scope works
  AcmeBasicCoax (the closest):
    no `acme_basic_coax` candidate satisfies `rf`:
      closest of the 1 that fit: ACME-BC-1
      fails because: rf.max_frequency must always be at least 10GHz, but 1GHz is not always above 10GHz
  AcmeMicroCoax:
    no `acme_micro_coax` candidate satisfies `rf`:
      closest of the 1 that fit: ACME-MC-1
      fails because: rf.max_frequency must always be at least 10GHz, but 6GHz is not always above 10GHz
  --> main.nsm:8:5
  |
8 |     .rf: CoaxReceptacle { .max_frequency >= 10GHz }
  |     ^^^^^^^^^^^^^^^^^^^
```

With one provider in scope the message is the one the part gives when it is written by its type, exactly as before.

When nothing provides the generic, the error is at the use and says how to bring a provider in scope:

```nsm fails
import src::rf::CoaxReceptacle

board Radio {
    .rf: CoaxReceptacle
}
```

```text
error: `rf` is a `CoaxReceptacle`, and no part or module in the files this file imports provides it; import the file of a provider, or name one as the type of `rf` (a part or module declared with `provides CoaxReceptacle`)
  --> main.nsm:4:5
  |
4 |     .rf: CoaxReceptacle
  |     ^^^^^^^^^^^^^^^^^^^
```

A use reaches only what the generic declares:

```nsm fails catalogs=coax
import src::rf::CoaxReceptacle
import src::parts::acme_micro_coax::AcmeMicroCoax

board Radio {
    .signal: Pin
    .gnd: Pin
    .rf: CoaxReceptacle
    .rf.signal ~ .signal
    .rf.shield ~ .gnd
    .rf.package &= "ACME_MC_1"
}
```

```text
error: `package` is not declared by `CoaxReceptacle`; a use of a generic reaches only the members the generic declares, so that it holds for every provider (declare it in `CoaxReceptacle`, or name a provider, `AcmeMicroCoax`, as the type)
   --> main.nsm:10:9
   |
10 |     .rf.package &= "ACME_MC_1"
   |         ^^^^^^^
```

A provider may not declare a member of the generic again. A narrowing or a relation is how it states its own number:

```nsm fails
generic Pad {
    .span: Length
    pins {
        .a passive
        .b passive
    }
}

part WidePad provides Pad {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .span: Length
    .span &= 24mm
}

board Plate {
    .p: Pad
}
```

```text
error: `span` is declared by `Pad`, which this part provides; do not declare it again: narrow it with `.span &= ...`, or state it with a relation
   --> main.nsm:14:5
   |
14 |     .span: Length
   |     ^^^^^
```

A provider's pin row has the generic's kind, and every pin has a row:

```nsm fails
generic Pad {
    pins {
        .a passive
        .b passive
    }
}

part ShortPad provides Pad {
    pins {
        .a input "A" "1"
    }
}
```

```text
error: `.a` is a passive pin in `Pad`, which `ShortPad` provides, but this row says an input; a provider's row has the generic's kind, because the kind decides what the checker asks of the board
   --> main.nsm:10:9
   |
10 |         .a input "A" "1"
   |         ^^^^^^^^^^^^^^^^
error: `pins` of `ShortPad` has no row for `.b`, which `Pad` declares as a passive pin; add `.b passive "NAME" "pad"`, or join it to a pin that has a row (`.b ~ .other`)
  --> main.nsm:9:5
  |
9 |     pins {
  |     ^^^^^^
```

Only a part, a module or an interface provides a generic:

```nsm fails
generic Pad {
    .span: Length
}

board Plate provides Pad {
}
```

```text
error: only a part, a module or an interface provides a generic; a board does not (`part X provides G`, `module X provides G`, `interface X provides G`)
  --> main.nsm:5:13
  |
5 | board Plate provides Pad {
  |             ^^^^^^^^
```

A provider's limit is at least as strict as the generic's:

```nsm fails
generic Limited {
    .designator &= "U"
    pins {
        .gnd passive required
        .in input abs_max 6V from .gnd
    }
}

part Loose provides Limited {
    pins {
        .gnd passive "GND" "1"
        .in input "IN" "2" abs_max 7V from .gnd
    }
}
```

```text
error: `.in` has `abs_max 7V` here, but `Limited`, which `Loose` provides, limits it with `abs_max 6V`: a provider's limit is at least as strict as its generic's, so `abs_max` is at most 6V
   --> main.nsm:12:28
   |
12 |         .in input "IN" "2" abs_max 7V from .gnd
   |                            ^^^^^^^
```

### A module that provides a generic

An IC alone cannot stand for a regulator: its feedback pin and its switch node need a circuit around them. A module can provide a module-shaped generic, which has values and interfaces and no `pins` table. The standard library's `BuckConverter` (`std::parts::regulator`, issue #620) is one: any buck converter, by what a board needs of it. Its members are the two rails, the ripple of the output voltage and the ESR of the output capacitors that the ripple needs, and two statements every provider holds:

```nsm-fragment
generic BuckConverter {
    .vin: Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    .vin.voltage >= .vout.voltage
    .output_ripple: Voltage
    .output_esr: Resistance optional
}
```

A board narrows the output's voltage, the load (the current of the rail it joins to `.vout`), the input (the rail it joins to `.vin`) and, with `.output_esr` stated, a bound on `.output_ripple`. It reaches nothing of how the converter is built. The soft-start time and the capacitors' `.voltage_margin` stay on `Buck`, for a board that names the IC: an IC that states no soft-start time cannot be judged against a bound on it, and a candidate that cannot be judged is not picked, so the bound would refuse every such IC (decided by the agent while the owner was away, 2026-11-13).

### A module that takes a part

The circuit around a buck IC is the same for every IC, and only the IC differs. A module takes the IC as a **type parameter**, bounded by a generic that parts provide (ADR 0115): `module Buck<Ic: BuckIc>` is a module for every part that provides `BuckIc`, written once. The standard library's `Buck` (`std::parts::regulator`, see _Switching regulators_) is the IC with its inductor, capacitors and feedback divider, and it provides `BuckConverter`:

```nsm-fragment
module Buck<Ic: BuckIc> provides BuckConverter {
    .ic: Ic
    .inductor: Inductor
    .ic.vin ~ .vin
    .ic.sw.pos ~> .inductor ~> .vout.pos
    // the capacitor banks, the divider and the checks that join them
}
```

The standard library's buck ICs, each a part that provides `BuckIc` with its datasheet's numbers and application circuit, and a catalog row of its own (`data/jlcpcb/README.md`): `Lmr16006y` (4 V to 60 V, 0.6 A, 2.1 MHz), `Lt8641` (3 V to 65 V, 3.5 A; its catalog has no row, since no record of it showed stock), `Rt8059` (2.8 V to 5.5 V, 1 A, 1.5 MHz) and `Tps54202` (4.5 V to 28 V, 2 A, 500 kHz).

A board that picks by requirement, with the standard library alone. `Buck` stands for one candidate for each provider of `BuckIc` in scope, in the order of their files and names: the four of the standard library, then a project's own. The first that holds every check of the board is the pick. A 3.3 V rail of 500 mA from 12 V:

```nsm evaluate picks
import std::parts::regulator::BuckConverter
import std::sources::Rail

board PowerBoard {
    .input: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= "VIN"
    }
    .output: Rail {
        .pos.net &= "3V3"
        .current &= 0mA..500mA
    }

    .reg: BuckConverter { .vout.voltage &= 3.3V +/- 5% }
    .input ~ .reg.vin
    .reg.vout ~ .output
}
```

```text
reg is a BuckConverter: picked Buck<Lmr16006y>
```

The `Lmr16006y` comes first and takes this board: 12 V is in its input range, 500 mA is under its 0.6 A, and its reference and the 1 % divider hold the output. Its catch diode joins ground and the switch node, two nets the DC analysis holds, where the current an ideal diode moves is not a number the analysis can work out; the diode states it (`.catch.current`, the output current over the part of each period the switch is off), so the trial is judged with that. A diode between two held nets that states no `.current` is a note, and a candidate with such a note is passed over for one that can be judged (ADR 0068). A board that wants another IC names it, `.reg: Buck<Tps54202>`.

The output asks for 5 % and not less: each standard IC's reference is good to 2 % or 2.5 % in the worst case, and the divider's 1 % resistors add about 1.5 % at 3.3 V, so 3 % holds for none of them. `examples/buck_converter.nsm` is this board with its connectors, built and made into a fab package.

A project's own IC joins the candidates when its file is imported. `Zbk9` is an invented IC for 4.5 V to 60 V and 3.5 A:

```nsm module=src::zbk9
import std::parts::regulator::BuckIc

/// A 3.5 A buck converter for 4.5 V to 60 V. Illustrative; not a real part.
part Zbk9 provides BuckIc {
    .vref &= 0.8V +/- 1.5%
    .fsw &= 1.2MHz +/- 15%
    .min_input_voltage &= 4.5V
    .max_input_voltage &= 60V
    .max_output_current &= 3.5A
    .efficiency &= 0.85..0.95
    .min_inductance &= 1.5uH
    .min_inductor_saturation &= 3A
    .max_inductor_dcr &= 0.1ohm
    .min_input_capacitance &= 2uF
    .min_output_capacitance &= 10uF
    .vin.current == -.sw.current * .sw.voltage / (.vin.voltage * .efficiency)

    pins {
        .vin.pos   power   "VIN"  "1" "2"
        .vin.neg   power   "GND"  "3" "9"
        .sw.pos    power   "SW"   "4" "5"
        .fb        input   "FB"   "6"
        .en        input   "EN"   "7"
    }

    support {
        .en ~ .vin.pos
    }
}
```

From 36 V at 1 A no standard IC serves: the `Lmr16006y` takes the input but gives 0.6 A at most, the `Rt8059` and the `Tps54202` stop at 5.5 V and 28 V, and the `Lt8641` has no catalog row. Each is passed over for its own reason, and `Buck<Zbk9>` is the pick:

```nsm evaluate picks
import std::parts::regulator::BuckConverter
import src::zbk9::Zbk9   // not used by name: brings the IC in scope
import std::sources::Rail

board PowerBoard {
    .input: Rail {
        .voltage &= 36V +/- 5%
        .pos.net &= "VIN"
    }
    .output: Rail {
        .pos.net &= "3V3"
        .current &= 0mA..1A
    }

    .reg: BuckConverter { .vout.voltage &= 3.3V +/- 5% }
    .input ~ .reg.vin
    .reg.vout ~ .output
}
```

```text
reg is a BuckConverter: picked Buck<Zbk9>; passed over Buck<Lmr16006y> (no `lmr16006y` candidate satisfies `reg.ic`: the closest of the 1 that fit, LMR16006YDDCR 4-60V 0.6A 2.1MHz buck SOT-23-6 C290195, fails because -reg.ic.sw.current must always be at most reg.ic.max_output_current, but 0A..1A is not always below 600mA (the currents on net `net-reg.ic.boost.b` in `reg`: reg.ic.sw.current (-1A..0A) + reg.inductor.current (0A..1A) add up to zero; net `3V3` in `reg`: -reg.inductor.current (0A..1A) + -reg.vout.current (-1A..0A) add up to zero, but the balance is incomplete: `reg.divider.rtop.a` is not modelled; net `3V3`: output.current (0A..1A) + reg.vout.current (-1A..0A) add up to zero) (`reg.input_cap.effective_capacitance` is unknown for some candidates, so they were not used: no derating data for 27 and data only up to 25 V, but the bias reaches 37.8 V, for 11)), Buck<Lt8641> (the `lt8641` catalog has nothing for `reg.ic`, which needs nothing in particular), Buck<Rt8059> (no `rt8059` candidate satisfies `reg.ic`: the closest of the 1 that fit, RT8059GJ5 2.8-5.5V 1A 1.5MHz buck TSOT-23-5 C20542, fails because reg.vin.voltage must always be at most reg.ic.max_input_voltage, but 34.2V..37.8V is not always below 5.5V (`reg.input_cap.effective_capacitance` is unknown for some candidates, so they were not used: no derating data for 27 and data only up to 25 V, but the bias reaches 37.8 V, for 11)), Buck<Tps54202> (no `tps54202` candidate satisfies `reg.ic`: the closest of the 1 that fit, TPS54202DDCR 4.5-28V 2A 500kHz buck SOT-23-6 C191884, fails because reg.vin.voltage must always be at most reg.ic.max_input_voltage, but 34.2V..37.8V is not always below 28V (`reg.input_cap.effective_capacitance` is unknown for some candidates, so they were not used: no derating data for 27 and data only up to 25 V, but the bias reaches 37.8 V, for 11))
```

Naming the part, with no pick: `.reg: Buck<Zbk9> { .vout.voltage &= 3.3V +/- 5% }`. A board joins a module provider through its interface, `.reg.vin` and `.reg.vout`, never `.reg.inductor`.

Rules.

- Only a `module` takes type parameters, written after its name: `module Buck<Ic: BuckIc>`, or several, `<A: G1, B: G2>`. The bound is a generic that parts provide (it has a `pins` table); a module is not an argument.
- Inside the module a parameter is the type of a member (`.ic: Ic`) or the argument of another module (`Buck<Ic>`), and nothing else. A member made from it reaches only the members its bound declares, as a use of the generic does, so that the module works with every part that provides the bound.
- The module is checked where it is written, with the bound standing for the parameter: a mistake is said at the module's line, never at a board that names a part.
- What only some parts need (a bootstrap capacitor, a resistor on `RT`, an enable divider, a catch diode) is in each part's own `support` section (_Switching regulators_), which comes with the part wherever it is placed. The module wires only what the bound has.
- A board names the part, `.reg: Buck<Zbk9>`: the argument is a part that provides the bound, or the bound itself when it is its own provider (`Resistor`). A module with parameters is never placed without its arguments; to let the solver pick, a board uses a generic the module provides.
- A board, or a module, that names the part (`.reg: Buck<Zbk2>`) may join any pin or interface of that part through the member made from the parameter, `.reg.ic.en ~ .tp.p1`, even one the bound does not declare (ADR 0120). Only a join (`~`, `~>`) reaches it there, besides a narrowing (ADR 0116); the parts of the IC's `support` section are not joined from outside. A board that picks through a generic the module provides (`.reg: BuckConverter`) does not know the IC, so it reaches only what the generic declares. The join follows the parameter: a module that passes its parameter on (`.buck: Buck<Ic>` in a `Converter<Ic>`) exposes the inner IC too, so a board that names `Converter<Zbk2>` joins `.reg.buck.ic.en`, and nothing else of `.reg.buck`. A part named inside a module (`.stage: Buck<Zbk2>` in a module `Supply`) is open to that module's text, not to the board that places `Supply`, and a parametric module's own text reaches only its bound, even through a module it gives its parameter to.
- A module with parameters that provides a generic stands for one candidate for each provider of its bound (with several parameters, for each combination, the last varying fastest), in the order of the bound's providers, at the module's own place in the order of the generic's. `Buck<Zbk9>` is the provider's name in the notes and its type in the design. Each candidate is one trial of the search, counted against its bound of 64.

A module reaches only what the bound declares. `en` is a pin of `Zbk2` and `Zbk9`, but not of `BuckIc`, so a part that provides `BuckIc` with no `en` would leave the line with nothing to join:

```nsm fails
import std::parts::regulator::BuckIc
import std::sources::Rail

module Enabled<Ic: BuckIc> {
    .vin: Rail
    .ic: Ic
    .ic.vin ~ .vin
    .ic.en ~ .vin.pos
}
```

```text
error: `en` is not declared by `BuckIc`, the bound of the type parameter `Ic`; a module reaches only the members of its parameter's bound, so that it works with every part that provides it (declare it in `BuckIc`, or wire it in the `support` section of the parts that have it)
  --> main.nsm:8:9
  |
8 |     .ic.en ~ .vin.pos
  |         ^^
```

The board that names the IC knows its pins. Here a test point probes the enable pin of `Zbk2`, which `Buck` does not wire and `BuckIc` does not declare:

```nsm
import src::zbk2::Zbk2
import std::parts::mechanical::TestPoint
import std::parts::regulator::Buck
import std::sources::Rail

board Probed {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .neg.net &= "GND"
        .current &= 0mA..1A
    }
    .reg: Buck<Zbk2> { .vout.voltage &= 1.8V +/- 3% }
    .tp_en: TestPoint

    .input ~ .reg.vin
    .reg.vout ~ .output
    .reg.ic.en ~ .tp_en.p1
}
```

A board that lets the solver pick the IC cannot join its pins, since which pins there are is decided later:

```nsm fails
import std::parts::regulator::BuckConverter
import src::zbk2::Zbk2

board Unknown {
    .reg: BuckConverter
    .reg.ic.en ~ .reg.vin.pos
}
```

```text
error: `ic` is not declared by `BuckConverter`: this use picks its IC through the generic, so the IC and its pins are not known here, and a join reaches the pins of a module's IC only where the IC is named, as in `Buck<Lmr16006y>`; name it as the type, or join what `BuckConverter` declares
  --> main.nsm:6:10
  |
6 |     .reg.ic.en ~ .reg.vin.pos
  |          ^^
```

The argument provides the bound:

```nsm fails
import std::parts::regulator::Buck
import std::parts::passive::Resistor

board Wrong {
    .reg: Buck<Resistor>
}
```

```text
error: `Resistor` is not an argument for `Ic` of `Buck`, which is a part that provides `BuckIc`: it does not provide `BuckIc`; name a part that does
  --> main.nsm:5:16
  |
5 |     .reg: Buck<Resistor>
  |                ^^^^^^^^
```

A module with parameters is given its arguments:

```nsm fails
import std::parts::regulator::Buck

board Bare {
    .reg: Buck
}
```

```text
error: `Buck` takes a part for `Ic` (a part that provides `BuckIc`): name it, `Buck<...>`, or let the solver pick one by using a generic that `Buck` provides
  --> main.nsm:4:11
  |
4 |     .reg: Buck
  |           ^^^^
```

### An interface that provides a generic: a process

A generic with no `pins` table and no catalog may be provided by interfaces too: `interface X provides G`. All the providers of one generic are parts, or all are modules, or all are interfaces, and a use is made as the kind they are. An interface provider holds values and interfaces, as any interface does, and states its own numbers with `&=`.

The standard library's use of it is the manufacturing process (ADR 0113). `std::mfg::jlcpcb::TwoLayer` is a generic, and each copper weight JLCPCB gives every rule for is a provider, `JlcTwoLayer1oz` and `JlcTwoLayer2oz`, which state the rules that depend on the weight (track, space, annular ring, mask bridge) and the weight itself. A board narrows the weight and the pick follows:

```nsm evaluate picks
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

board Heavy {
    /// 2 oz copper, for a board that carries a few amperes.
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
        .stackup.copper_weight &= 2
    }
    .header: Header1x2
}
```

```text
process is a TwoLayer: picked JlcTwoLayer2oz; passed over JlcTwoLayer1oz (contradiction on `process.stackup.copper_weight`: it is already 1, which does not overlap 2)
```

A narrowing of a use that its provider's own statement contradicts is a mistake of the check, which used to stop the build before any pick. Inside a use with more than one provider it is not the end: the build tries the other providers, as it does for a relation the solver finds false, and stops only when none has what the board asks for, naming each:

```nsm fails
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

board Heavier {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
        .stackup.copper_weight &= 3.5
    }
    .header: Header1x2
}
```

```text
error: no TwoLayer provider satisfies `process` at main.nsm:5: none of the 2 in scope works
  JlcTwoLayer1oz (the closest):
    contradiction on `process.stackup.copper_weight`: it is already 1, which does not overlap 3.5
  JlcTwoLayer2oz:
    contradiction on `process.stackup.copper_weight`: it is already 2, which does not overlap 3.5
  --> main.nsm:5:5
  |
5 |     .process: TwoLayer {
  |     ^^^^^^^^^^^^^^^^^^
```

A board that says nothing about the weight is made by the first provider, 1 oz, as before the process was a generic, and its rules and output are the same. The process is still called by the type the board wrote, `TwoLayer`, in `fab-notes.txt`, and the weight is in the same line.

### The standard passives are generics

`Resistor`, `Capacitor`, `CapacitorBank`, `Led`, `Inductor`, `FerriteBead`, `Crystal`, `Fuse` and `NtcThermistor` (`std::parts::passive`), `Schottky`, `Zener`, `Tvs` and `TvsBidirectional` (`std::parts::diode`), `NMosfet` and `PMosfet` (`std::parts::transistor`), `LinearRegulator` (`std::parts::regulator`) and `LevelTranslator` (`std::parts::logic`) are generics (ADR 0068, stage 2; ADR 0063 for the last two and `PMosfet`). A generic is a definition and not a part: `.r1: Resistor` is the part, and the bill of materials has a line for the part. Each generic here is its own first provider: it has a pin table with pads and a `catalog`, so it stands for itself, with the same members, the same relations, the same catalog rows and, for a resistor, a capacitor, an inductor or an LED, the same series of values that have no row yet (ADR 0047). A design that adds no provider is read and solved exactly as when they were parts: the same picks, the same effort, the same kind names (`Resistor`) in the bill of materials and in `kicad.toml`. The parts of the standard library that have no catalog (`Diode`, `Switch`, the connectors) are not generics. The standard catalogs of `PMosfet` and `LevelTranslator` have no rows (see _Transistors_ and _Logic levels_): a project that uses either adds its own rows.

A project adds a provider of one of them as it adds any provider: a part that says `provides Resistor`, in a file that the board's file imports. The generic's catalog and pads are its own and are not read into a provider: the provider has its own pin table and its own one-row catalog, and what the generic declares (`.resistance`, `.max_power`, `.temperature_coefficient`, `.current`, `.drop`, `.package`), what it states (`.designator &= "R"`, Ohm's law, the power rating, `bridge .a ~> .b`) and the kinds of its pins. The provider does not declare `.package` again, which a part written by hand would.

```nsm module=src::parts::precision_thin_film
import std::parts::passive::Resistor

/// PTF-10K, an invented 10 kohm thin-film resistor with a tolerance of 0.1 %.
part PrecisionThinFilm provides Resistor {
    pins {
        .a  passive  "A"  "1"
        .b  passive  "B"  "2"
    }
    catalog precision_thin_film { .resistance, .max_power, .package }
}
```

```csv file=catalogs/precision_thin_film.csv set=thin
mpn,resistance,max_power,package,library,source
PTF-10K,10kohm +/- 0.1%,125mW,0402,Extended,https://example.invalid/ptf-10k.pdf
```

The standard rows and series come first, because the generic is first in the order. A design that they serve never reaches the provider, and one that they cannot serve, here a resistor good to 0.1 %, is tried with it:

```nsm evaluate picks catalogs=thin
import src::parts::precision_thin_film::PrecisionThinFilm   // not used by name: brings the provider in scope
import std::parts::passive::Resistor

board Reference {
    .r1: Resistor { .resistance &= 10kohm +/- 0.1% }
    .r2: Resistor { .resistance &= 10kohm +/- 1% }
}
```

```text
r1 is a Resistor: picked PrecisionThinFilm; passed over Resistor (the `resistor` catalog has nothing for `r1`, which needs resistance within 9.99kΩ..10.01kΩ)
```

`r2` is served by a standard row, so it says nothing. The provider needs the same three things as a part written by hand: its catalog file in `catalogs/`, its entry in `kicad.toml` (`kind = "PrecisionThinFilm"`), and the recorded source of its row. The other tools see what the generic says of it, not its name: a provider has the attributes of the generics it provides (see Attributes), so it is drawn in series as an inductor is when it provides `Inductor`, `FerriteBead` or `Fuse` (their `#[kind(...)]`), and the placer takes it for a decoupling capacitor when it provides `Capacitor` or `CapacitorBank`, and a provider of `CapacitorBank` is a bank while a provider of `Capacitor` is one part. The rules of a provider apply (above), and two more follow from the generic being a part with its own pads:

- A provider's pin table has a row of the generic's kind for each of the generic's pins: `.a` and `.b` for a resistor, `.anode` and `.cathode` for an LED or a diode, `.gate`, `.source` and `.drain` for a MOSFET, `.vin.pos`, `.vin.neg` and `.vout.pos` for a linear regulator.
- The generic's statements hold for the provider, so a provider cannot state other designator letters than the generic's (`.designator &= "RN"` is a contradiction of `"R"`).
- Only a part provides a standard generic: a module that says `provides Resistor` is refused, because the generic has a `pins` table.

How the parts that have no row are made (ADR 0047: their packages, tolerances and ratings) and the columns that a file of rows may leave out (`.temperature_coefficient` of a resistor, `.dielectric` of a capacitor, `.rms_current` of an inductor) belong to the **catalogs** `resistor`, `capacitor`, `inductor` and `led` (an LED has no series, only its packages), which are the standard generics' own: one table of the solver names them, and it is the only place that does (ADR 0068, stage 3). Which E-series their values come from is the definition's `#[series(...)]` (see Attributes): `Resistor` offers E12, E24 and E96, `Capacitor` and `Inductor` E12 and E24. A catalog is one name that every part listing it shares, so a part or a provider that lists `catalog resistor` has the omitted columns of that catalog, and the made parts of the series its definition offers, whatever the generic it provides. A column may be left out of a file when the part's `catalog` statement marks it `optional` (ADR 0078), also in a catalog that no generic synthesizes: `.breakdown_voltage` of a TVS diode, in the `tvs` catalog (ADR 0073), is marked so in `std`. A provider whose catalog has another name has no series, and its file has every column it lists, unless its statement marks the column `optional`.
