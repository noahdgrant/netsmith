---
status: accepted (user review 2026-11-02)
---

# Standard library kinds: the generic parts and buses of a two-sided four-layer board

The library had a resistor, a capacitor, an LED, a linear regulator and a two-pin header. A real board also has diodes, protection, inductors, a crystal, MOSFETs, headers of several sizes, USB-C, test points and the buses between ICs. This ADR records what was added and the choices taken without the user. Every choice below is provisional; the user reviews them. Nothing here changes the language, the checker or the solver; the definitions are `.nsm` source under `std/`, tested by `crates/netsmith/tests/std_kinds.rs` and by the examples in the guide's section "Standard parts".

## What was added

Pads are the part's own pin table (ADR 0016), so they are the same in every package. Where a pin table has one row for several pads they are listed. Every part has `.package: Package` (so a board, and later the export mapping, can name the package) and no `catalog`: a `catalog x { ... }` line with no rows would make the solver say "there is no `x` catalog" for every instance, so the catalog line is added together with its rows by whoever adds the data.

| Definition (module) | Pins: kind, pad | Fields | Always true (relations) | Bridge |
|---|---|---|---|---|
| `Diode`, `Schottky` (`std::parts::diode`) | `anode` passive 2, `cathode` passive 1 | `forward_voltage`, `max_reverse_voltage`, `max_forward_current`, `current`, `drop`, `bias` | `drop == forward_voltage`, `current <= max_forward_current`, `bias <= max_reverse_voltage` | anode to cathode |
| `Zener` | same pins | `zener_voltage` (with tolerance), `max_power`, `current` (cathode to anode), `bias` | `bias * current <= max_power` | cathode to anode |
| `Tvs` (unidirectional) | same pins | `standoff_voltage`, `clamping_voltage`, `peak_pulse_current`, `protected_limit`, `bias` | `bias <= standoff_voltage`, `clamping_voltage <= protected_limit` | cathode to anode |
| `Inductor` (`std::parts::passive`) | `a` passive 1, `b` passive 2 | `inductance`, `saturation_current`, `dc_resistance`, `current`, `drop` | `drop == current * dc_resistance`, `current <= saturation_current` | a to b |
| `FerriteBead` | `a`, `b` as above | `impedance`, `impedance_frequency`, `rated_current`, `dc_resistance`, `current`, `drop` | `drop == current * dc_resistance`, `current <= rated_current` | a to b |
| `Crystal` | `a`, `b` as above | `frequency` (with tolerance), `load_capacitance` | none | none |
| `Fuse` | `a`, `b` as above | `rated_current`, `rated_voltage`, `resistance`, `current`, `drop` | `drop == current * resistance`, `current <= rated_current` | a to b |
| `Switch` (SPST) | `a`, `b` as above | `rated_current`, `rated_voltage`, `current` | `current <= rated_current` | a to b |
| `NMosfet`, `PMosfet` (`std::parts::transistor`) | `gate` input 1, `source` passive 2, `drain` passive 3 | `max_drain_source_voltage`, `max_drain_current`, `max_gate_source_voltage`, `gate_threshold`, `on_resistance`, and the stresses a design states: `drain_source_voltage`, `gate_source_voltage`, `current` | each stress is within its maximum; `gate_source_voltage >= gate_threshold` | N: drain to source; P: source to drain |
| `Header1x2`, `1x4`, `1x8`, `1x10`; `Socket1x2`, `1x4`, `1x8`, `1x10`; `Header2x3`, `2x5`, `2x10` (`std::parts::connector`) | `p1` to `pN` passive, pin N on pad N | none | none | none |
| `Coax` | `signal` passive 1, `shield` passive 2 | `impedance`, `max_frequency` | none | none |
| `UsbCPower` | `vbus.pos` power A4 B4 A9 B9; `vbus.neg` power A1 B1 A12 B12; `cc1` passive A5; `cc2` passive B5; `shell` passive S1 | the rail `vbus` | none | none |
| `UsbC2` | as `UsbCPower` with `usb.vbus` for `vbus`, `usb.dp` bidirectional A6 B6; `usb.dm` bidirectional A7 B7 | the port `usb` (`Usb2`) | none | none |
| `ProgrammingPads6` | `p1` to `p6` passive, pad N | none | none | none |
| `MountingHole`, `Fiducial` (`std::parts::mechanical`) | none | none | none | none |
| `PlatedHole` | `p1` passive 1 | none | none | none |
| `TestPoint` | `p1` passive "TP" pad 1, `required` | none | none | none |
| `Uart`, `Spi`, `I2c`, `Swd`, `Usb2` (`std::bus`) interfaces | `Pin` members: `tx rx rts cts`; `sclk mosi miso cs`; `sda scl`; `swdio swclk swo reset`; `dp dm` and the rail `vbus` | none | none | none |
| `I2cPullups` (module, `std::bus`) | `bus` (`I2c`), `supply` (`Rail`) | `resistance` | `resistance == r.resistance` for each of its two resistors | none |

The USB Type-C pad numbers were first written from knowledge of the specification. The mapping (ADR 0021) checks them against KiCad's receptacle symbols and footprints, which corrected two things: a power-only receptacle has six contacts (VBUS on A9 and B9, GND on A12 and B12; A4, B4, A1 and B1 are joined to them inside and are not pads), so `UsbCPower` lists those; and the sixteen-contact USB 2.0 receptacle has the sideband pads A8 and B8, which `UsbC2` now brings out as the open pins `sbu1` and `sbu2`. `UsbCPower` and `UsbC2` name the shell pad `S1`, which is what KiCad's USB-C footprints use.

## Decisions

**Models come from datasheet fields only.** No number is in any definition. A rating is a field the library leaves open, and a relation says what the datasheet limit means. A rating the board does not state is "not evaluated", which is not an error.

**The voltage across a part reuses `.bias`.** The checker already gives a part with a `.bias` and a `bridge` the voltage of the rail it sits across (ADR 0013); it does not look for a capacitor by name. `Diode`, `Schottky`, `Zener` and `Tvs` have a `.bias`, so their reverse-voltage and stand-off ratings are checked against the rail across them with no new line. The limit: the rail's sign is not looked at, so a part wired the wrong way round is not found by the rating. A part across no rail has no bias until a design states it, and its relation waits.

**A Zener states no clamp rule.** The task asked for a check that the rail across a Zener is below its breakdown. A Zener is used as a clamp, where that holds, and as a shunt regulator, where the voltage across it is its breakdown, so the check is not always true (the library states only what is always true). The part states what is always true (`bias * current <= max_power`), and a clamp design writes `.z.bias <= .z.zener_voltage`, one line, tested with its message.

**A TVS diode states a protected limit as a field.** The clamping voltage is checked against `.protected_limit`, the lowest absolute maximum of what is behind it, which the board states from that part's datasheet. Left unstated, the check is not evaluated. A bidirectional TVS (two symmetric pins) is not added.

**Separate definitions for the sizes of a header.** The language has no parameter for a count (a part's pin rows are written out), so there is one definition for each size, generated by hand: single-row 2, 4, 8 and 10 (`Header1x2` existed), the same sockets, and 2x3, 2x5 and 2x10. Other sizes are added when a design needs one. Pins are `pN` on pad N whatever the rows are; the footprint decides which pin is in which row. A socket is its own kind because it is a different symbol and bill-of-materials line from a header. The pitch is the package (`.package &= "2.54mm"`); the names a package can take are the mapping's. `Header1x2` gained `.package` for the same reason.

**`Schottky` repeats `Diode`.** The language has no inheritance, and a Schottky is a different symbol and catalog, so it is its own part with the same fields and ratings. This is the one duplication in the library.

**`NMosfet` and `PMosfet` are two parts.** The bridge runs the other way (drain to source for N, source to drain for P), which decides the direction of `.current` in a `~>` chain. All their quantities are magnitudes, so both kinds use the same relations. The drive and the blocking voltage are fields the design states (`drain_source_voltage`, `gate_source_voltage`), because only the design knows them; a gate drive that is not above the highest threshold is refused. Threshold is only the lowest bound of "on": the drive at which `on_resistance` is specified is not modelled.

**Mechanical parts follow ADR 0020's recommendation.** `MountingHole` is non-plated with no pins, `PlatedHole` has one pin that a screw bonds to a net or leaves open, `Fiducial` has no pins, `TestPoint` has one `required` pin. A part with no pins is on no net and is fitted by nobody. ADR 0020 is still proposed: the `.assembly` field, `TestPoint.diameter` and `Label` are not added here.

**The buses are interfaces of `Pin`s, and the roles are the pin kinds.** Verified against the checker before writing a rule: ADR 0016's faults already cover the roles when the parts have pin tables with `output` and `input` pins.

- A UART joined with `~` is two outputs on a net: `net X has two outputs`. The board crosses `tx` and `rx` a pin at a time. Tested.
- An `input` with nothing on its net is refused (`` `a.uart.rx` is an input that must be connected ``). Tested.
- Two SPI masters (or two slaves with `miso`) on one bus are two outputs on a net. Tested.
- An SPI master and slave, and a USB host and device, join with `~` as named. Tested.

Not covered, and written as known limits (ADR 0044 closes the first: a net of only inputs is now refused): an `input` that is wired only to other inputs (`rx` to `rx`) has a connection and no driver, and nothing is reported; a bus joined to a passive connector is not checked at all (a connector has no direction).

**`Logic` is not a bus member.** Using `Logic` for each line would give every signal a reference rail, whose pins `~` then joins: two chips on separate supplies of the same voltage would have their supply nets merged by a signal join. The buses are plain pins, and logic levels stay with ADR 0016's Models section.

**`Usb2` carries `vbus` as a `Rail`.** Its `pos` is VBUS and its `neg` is the ground, which is the deviation from the task's "vbus, gnd" pins. A rail carries the voltage and the current, so the connector's `vbus.current` follows from the loads on the net (ADR 0007) and a board bounds it with one line, `.usb.vbus.current >= -500mA`. The device side must declare the rail, with its sensing pin an optional `input` and its `neg` joined to its ground (the guide's `Controller` does).

**I2C pull-ups are a module, and there is no `PullUp` or `PullDown`.** `I2cPullups` is the pair for a bus, with the resistance as a value. A single pull-up or pull-down needs nothing: a `Resistor` is already in `~>` chains (`.supply.pos ~> .pull ~> .mcu.en`), and a module cannot be in a chain. A `PullUp` module would be a second way to write that.

**Capacitor is unchanged; the derating rule is one line.** `.max_voltage >= .bias * 1.5` in a capacitor block states the rule without naming the rail, because `.bias` is the rail across it. The guide shows it failing on a 12 V input with a 6.3 V part.

## Proposed, not built (the smallest generic extensions)

These change the checker, so they are the user's to accept.

1. **A driver for an input (built, ADR 0044).** A net whose pins are all `input` (and no rail, no `output`, `bidirectional` or `power` pin) has no driver. This is a net rule in `pin_faults`, with a message such as `` net `X` has inputs (`a.uart.rx`, `b.uart.rx`) and nothing drives it ``. It completes the UART crossover check.
2. **An open-drain pin kind and a pull-up rule (built without the kind, ADR 0044).** A pin kind `open_drain` (ADR 0016 already names `open_collector` as a possible addition), and a net rule: a net with an `open_drain` pin needs a pin of a part on it that is `passive` and joined to a `power` net through a resistor, or a pull-up declared by the part. That makes "an I2C bus without a pull-up" an error. Until then `I2cPullups` is the convention and a missing one is not found.
3. **Both ends of a bidirectional TVS** and a rating by direction, if a design needs them.

## Mapping rows

Every definition above except the interfaces and `I2cPullups` has rows in `std/kicad.toml` (ADR 0021), each checked against the installed KiCad 9 libraries. Packages are named after the footprint (`SOD-123`, `0603`, `2.54mm`, `GCT_USB4105_16P`, `M3`). The hand-soldered kinds are the ones whose footprint says through hole; the kinds that are not assembled (`MountingHole`, `Fiducial`, `PlatedHole`, `TestPoint`) still wait for ADR 0020's assembly flag. No kind had a catalog line when this ADR was written; ADR 0025 added catalogs for eight of them.

Mapped with a caveat: `Tvs` is drawn with `Device:D_Zener`, because KiCad has no one-way TVS symbol (`Device:D_TVS` is the two-way one with pins A1 and A2); `Crystal` has two pins, so only two-pad footprints are mapped, and the four-pad crystals that are most common (3.2 x 2.5 mm) are not; `Coax` maps an SMA connector whose footprint has four ground pads, all on pad 2; `Switch` is mapped to two SMD tactile footprints only.

## Limits

- `.bias` is a magnitude; a diode, Zener or TVS diode wired the wrong way round across a rail is not found by these ratings.
- The two crystal load capacitors are not tied to `load_capacitance`: the stray capacitance of the layout is not a datasheet number.
- A MOSFET's `gate_threshold` is a bound for "on", not the drive that `on_resistance` is given at.
- The guide's examples of these parts are marked `evaluate`: they are checked and solved and not exported, though every kind now has a mapping row.
