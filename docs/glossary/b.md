# Glossary: B

**Balance**:
The rule that at a net, in one board or module, the draws there (of rails and of the chains that start or end on it) add up to zero. A net with a rail on its `pos` is a supply net and is solved from; one with a rail only on `neg` is a return net, which is checked and never solved from. A module's own rails count with their sign flipped, because from inside they are the supply of what is inside. In a system the joined ports of boards balance the same way, each as its board writes it. A balance is **complete** when every part pin on the net is a term or belongs to a part that draws no DC current by its model, or is an `input` or `no_connect` pin of a part with a rail (ADR 0027); only a complete balance sets a current. The pins that are neither are its **unmodelled** pins. A current the design stated with a range on a complete balance is a **budget** (ADR 0038): what the others determine must lie inside it, and it then holds that narrower value. A range on a current inside a part is the part's claim, not a budget (ADR 0066, section 3).
_Avoid_: KCL, current sum

**Bank**:
Identical capacitors in parallel that stand in one place of the design, a `CapacitorBank` with a `.count` (ADR 0052, ADR 0123). The count is a range the solver chooses from (`1..8`, the fewest that holds every check; the default when unset), or a number a module passes in (`.stage.output_cap.count &= 3`); a whole number written as the `.count` of a board's own bank is refused, because `3 x Capacitor` says it (a group). The fields of a capacitor are the value of one capacitor of the bank; `.total_effective_capacitance` and `.total_capacitance` are the bank's, the count times the bias-derated and the marked value. The design stage makes one `Capacitor` for each, on the same nets, and the bill of materials lists the row once with the quantity; the parts of a bank of two or more have the paths `bulk[0]`, `bulk[1]`, as the members of a group do. A definition that carries `#[bank]` is a bank, with its `#[role(count)]` member as the count (ADR 0125): the standard `CapacitorBank` (`#[bank(Capacitor)]`), a project's own, and a part that provides one; a part with a member named `count` and no `#[bank]` is one part (`Model::bank` is the one place that decides), and a requirement on a bank's part path in the physical file holds for each of its capacitors. A `Capacitor` has no `.count`. A bank is not a repetition construct: the language has none.
_Avoid_: Array, capacitor slots (`cap_a`, `cap_b`), parallel group, multiplier, multiplicity

**Bias**:
The DC voltage across a part: a capacitor, a diode, a Zener or a TVS diode. A part with a `.bias` and a `bridge` placed between a rail's `pos` and `neg` has the rail's voltage as its `.bias` without being told; a design states `.bias` only for a part that is across no rail. A diode's bias is the voltage in the direction it blocks, and it is negative when the diode is wired to conduct (see **Bridge voltage**).

**Blind via**:
A via that joins an outer layer to an inner one and goes no further: on an HDI process (issue #1015), the laser-drilled via from an outer layer to the layer under it, through one dielectric layer, which the board file writes as KiCad's `micro` via. Its copper and its hole are on its own layers only, so the layers past it keep their room. The process fills it with plating, so the router may stand one in a surface-mount pad whatever `.via_in_pad` says (issue #1071). See _Via span_.
_Avoid_: Microvia (KiCad's word for its type, not netsmith's), partial via

**Board**:
One physical PCB, described as a manufacturing process, parts, and the modules placed on it.
_Avoid_: Design (for the board itself: _Design_ is the `netsmith_core::Design` that the design stage makes, and the word for that stage), PCB

**Board default**:
A value a board offers to every part or module of a kind on it, written as a line of its `defaults` block, `defaults { Resistor.package ?= "0402" }` (issue #625, ADR 0128). It reaches every part of the kind at any depth whose value no statement reaches (no `&=` or `?=` on it, or on a value `==` or a join holds equal to it); a line that reaches nothing is a note.
_Avoid_: Project default, global default, house style, scoped default

**Body**:
The box of a part that a placement requirement measures: the outline its footprint draws on the fabrication layer (`F.Fab`), else its courtyard, after the part is turned. Not the footprint's origin, which the layout file's `at` is.
_Avoid_: Footprint box, bounding box

**Body diode**:
The diode inside a MOSFET across its channel, from the source to the drain of an N-channel part (`NMosfet`) and from the drain to the source of a P-channel one. Its `.body_diode_voltage` is the datasheet's drain-source diode forward voltage (VSD) at its test current, the most the diode drops at the small current of a logic line. The DC analysis takes a MOSFET as a switch and leaves the diode out, except where the levels check judges a level that crosses the channel (a translator between two buses, #1045): a part that states it has the diode there, and the low that pulls the drain down then turns the channel on; one that does not leaves that low a note.
_Avoid_: Intrinsic diode, parasitic diode (as a field)

**Boundary**:
A part with a `bridge` and no `dc` statement. A DC analysis cannot see through it, so it ends the network there and says which part and pin it stopped at, and the currents that depend on it stay not evaluated (ADR 0076).
_Avoid_: Black box, unmodelled part

**Branch**:
The copper from the pads of a part's pin to the rest of its net, when the pin states the most it carries with `.current` (`.bias.current &= 1mA`, issue #1113): a converter's bias pin, a feedback divider's top, a meter's probe on a rail of amps. The router joins each such pad to the nearest pad of the net that is no branch by a line of its own, so the pad is a leaf, and draws that line, and its vias, for the branch's current and not the net's; the design rules check holds that copper to it. The rest of the net, what joins its pads that are no branch, is its **trunk**. A branch's current is no term of a balance. Only a pin of a part has one; a port's pin stands for every pin joined to it.
_Avoid_: Stub (a stub is a track that ends at nothing), tap (a sense pin's join to a pad, issue #1148, is another thing), spur

**Bridge**:
The two terminals of a part that a series chain enters and leaves by.

**Bridge voltage**:
The `.bridge_voltage` of a diode (`Diode`, `Schottky`, `Zener`, `Tvs`): the voltage from the first end of its `bridge` to the second, which the checker gives a diode across a rail without being told, with the sign of the way it is wired: the rail's voltage when the first end is on `pos`, its negative when it is on `neg` (ADR 0088). The part's own relations read it, so a diode wired the wrong way round across a rail is found and named.
_Avoid_: Polarity, orientation (as a field)

**Buck stage**:
Everything around a buck converter IC except the IC: `std::parts::regulator::BuckStage`, a module of the inductor, two capacitors and a feedback divider. The IC is a part with a `Buck` interface member that carries its datasheet numbers.
_Avoid_: Buck module, power stage

**Build-up**:
How a board of more than two layers is pressed together, the fabricator's named build (`JLC06161H-3313`): the inner copper and each _Dielectric layer_ from the top, `.stackup.dielectric1` to `.dielectricN` (ADR 0111).
_Avoid_: Lamination, stack-up (for this; the _Stackup_ is the whole)

**Buried via**:
A via that joins two inner layers and reaches neither outer one: on an HDI process with `.buried_vias &= "core"` (issue #1015), the mechanically drilled via through the core, between the inner layers the build-up layers end on, which the fabricator plugs with resin and caps. See _Via span_.

**Burst**:
A stretch of time in which a load draws its `.peak_current`, as a radio does while it transmits: the `.burst` of a `Rail`, how long one lasts, with `.duty`, the fraction of the time it bursts, optional (issue #1279). A load that bursts states `.current` as its average, which the balance and a regulator's dissipation read; during a burst the source gives the peak and the other loads' steady current together, held against `.max_peak_current`, and the capacitors across the rail give what it cannot, as long as the rail stays at or above the least voltage a part on the net states. Each load states its own: `~` does not share it.
_Avoid_: Pulse, transmit current, peak duration

**Bus**:
The interfaces of one definition that share a net: the processor's `spi` and a flash's `spi` are one bus because `sclk` is on one net. The limit of a role is counted for a bus (ADR 0064). The editor's schematic draws the members of a bus's interfaces as one thick line (_Bus drawing_).
_Avoid_: Interface (the type), network

**Bus arrow**:
The small arrowhead on a bus entry that says which way the member's signal goes at that part, as the part's pin table says it (`output`: away from the part, `input`: at the part, `bidirectional`: one each way); a pin of any other kind has none. The thick line has no arrow, because the members of a bus go both ways (ADR 0067).
_Avoid_: Direction mark, flow arrow, signal arrow

**Bus capacitance**:
The capacitance of one line of an I2C bus, `sda` or `scl`: `.bus_capacitance` of `I2cPullups`, an **Optional member**. The checker derives it as the sum of the pins of the parts on the bus (each `I2c` interface that shares a net with the pull-up's `.bus`: its `.capacitance`, the datasheet's `Ci`, which `~` does not equate between two parts) and the board's own estimate of the traces, connector and cable, `.trace_capacitance` (ADR 0102, a **Derived figure**); a board may state `.bus_capacitance` itself and is then judged on its figure. With it the pull-up resistance gives `.rise_time`, `0.8473 * resistance * bus_capacitance`, a **Closed-form check** between 30 % and 70 % of the supply (NXP UM10204 Rev. 7.0, section 7.1), which a board bounds with the limit of its mode. A part with no capacitance leaves the sum without a value, the bound is listed as not evaluated and the part is named. Not the capacitance of one pin (`.i2c.capacitance`).
_Avoid_: Line capacitance, load capacitance, Cb

**Bus drawing**:
How the editor's schematic draws the members of one interface instance (`mcu.uart0`) when three or more of them run between the same two main parts of a module, or leave the module from one part (or two or more of them run between three or more parts, a **shared bus**, or between two parts and on to another module): each member's pin stays on the part's edge and its wire is a short entry that meets one thick line, which runs to the other part (to every other part, as one line with a branch to each, for a shared bus) or ends in one flag where the members go on to another module. The members of an interface nested in the bus's interface are members of it under dotted names (`data.sclk`), an entry whose pin the part's pin table says is an output, an input or bidirectional has an arrowhead for it (a **bus arrow**), and the flag says which instance of the other sheet the members go to (`→ modem.uart: {rx, tx}`). It is named once, by the instance (`uart0`), with the list of members (`{tx, rx, rts, cts}`) and a slash with their count. The line joins nothing: each entry joins, by name, the net of that name at the other end, as a label does. Which nets are one bus comes from the interface instances the design carries (ADR 0049), never from net names. The word is for the drawing; the language's thing is an _Interface_, and the interfaces of one definition that share a net are a _Bus_.
_Avoid_: Bundle, harness, interface (for the drawing), bus wire
