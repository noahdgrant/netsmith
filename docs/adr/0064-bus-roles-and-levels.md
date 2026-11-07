---
status: proposed for owner review (a design draft written by an agent that was told to design and not to build; the words `lines`, `role`, `limit`, `single`, `crosses` and `tri_state`, the enum for each bus's roles, the member `.levels` and the shared `.frequency` are the owner's to accept or change, and nothing here is implemented)
---

# Buses state who drives each line, at which levels and how fast

## Summary

The standard buses (`Uart`, `Spi`, `I2c`, `Swd`, `Usb2` in `std/bus.nsm`) name their lines and say nothing else. Who drives a line is the pin kind in each part's table (ADR 0016), and the net rules (ADR 0044, ADR 0046) judge only that: two `output` pins on one net, an `input` with nothing driving it, a net of only inputs. On a board where the processor, a flash and an IMU write every SPI pin as `bidirectional`, a shared MISO is accepted, which is right, and so are nearly all the miswirings: two slaves' chip selects joined, `mosi` on `miso` when no net is named, a slave's data out left open. A UART's `tx` and `rx` cross by hand, and nothing checks that the right line faces the right line. A line at 3.3 V onto a 1.8 V pin is not judged, because only `Logic` lines have levels. A shared MISO needs every driver to be high impedance when its part is not selected, and the pin kinds cannot say that, so a datasheet that is silent about it cannot be recorded as silent. The clock a bus runs at, and the SPI mode, are not stated anywhere.

The decision, in six sentences:

- An interface may declare **`lines`**: for each role of the bus (an `enum` value, ADR 0054), the kind of each line and a few properties of a line (`limit N` on the role, `crosses` and `single` on a line, `optional` and `required` as on a pin row). `std/bus.nsm` declares them for the five buses, so the protocol is written once and in a file that a reader can open.
- A part states the role it plays with an ordinary statement, `.spi.role &= SpiRole::Slave`. Its pin table stays as it is; the role judges the table (a row that contradicts the role is an error) and the checker judges the nets: one line on one net (`mosi` is never on `miso`), a `UART` joined `tx` to `rx` (the language crosses it, `~` between two `Uart` interfaces included), at most N parts of a role on one bus, one select line for each slave, and a line a role requires is connected.
- A seventh pin kind, **`tri_state`**, says that the pin drives only while its part is selected. A line with two or more drivers is an error unless every driver is `tri_state` or `bidirectional`; a part whose datasheet is silent writes `output`, which says exactly what is known, and a board that shares that line is refused with the question to put to the datasheet. A part writes `tri_state` only with the page in a comment (`docs/DESIGN-RULES.md`, "A part states its own requirements").
- Every bus interface carries **`.levels: Levels`**, the four voltages and the reference rail of `Logic` in one place for all the lines of the interface. The solver's existing level checks judge a bus pin the way they judge a `Logic` line: the numbers are those of ADR 0046, and a 3.3 V line onto a 1.8 V pin is refused with them. A level translator is a part with two bus interfaces, one in each role and each on its own rail.
- The clock is **`.frequency`**, one value that `~` equates along the bus, and each part bounds it (`.spi.frequency <= 10MHz`). The bus runs at the lowest bound without a new construct. The SPI mode stays in the part's doc comment, with one optional enum for the parts that support exactly one mode.
- Nothing is required of a part that states no role: its bus is judged as it is today. Roles become a guideline warning once the library's parts state them, and an error later, by the owner's choice.

## The syntax at a glance

Everything below is invented. The three parts, in `src/zparts.nsm`: a processor that is an SPI master, a flash that is a slave and writes `tri_state` for its data out, and an IMU on its own 1.8 V supply.

```nsm
import std::bus::{Spi, SpiRole, Uart}
import std::sources::Rail

/// Zcpu, an invented processor: one SPI master, a second select on a plain pin, and a UART.
part Zcpu {
    .designator &= "U"
    .vdd: Rail
    .spi: Spi
    .uart: Uart
    .sel1: Pin

    .spi.role &= SpiRole::Master
    // Datasheet table 9, the clock: at most 32 MHz.
    .spi.frequency <= 32MHz
    // Datasheet table 11, DC characteristics of the I/O pins.
    .spi.levels.reference ~ .vdd
    .spi.levels.vih &= 0.7 * .vdd.voltage
    .spi.levels.vil &= 0.3 * .vdd.voltage
    .spi.levels.voh &= .vdd.voltage - 0.4V
    .spi.levels.vol &= 0.4V

    pins "QFN-16" {
        .vdd.pos  power         "VDD"  "1"
        .vdd.neg  power         "VSS"  "2"
        .spi.sclk output        "SCK"  "3"
        .spi.mosi output        "MOSI" "4"
        .spi.miso input         "MISO" "5"
        .spi.cs   output        "CS0"  "6"
        .sel1     bidirectional "IO7"  "7"
        .uart.tx  output        "TXD"  "8"
        .uart.rx  input         "RXD"  "9"
    }
}

/// Zflash, an invented serial flash on 3.3 V.
part Zflash {
    .designator &= "U"
    .vdd: Rail
    .spi: Spi

    .spi.role &= SpiRole::Slave
    // Datasheet section 7: the clock is at most 33 MHz.
    .spi.frequency <= 33MHz
    .spi.levels.reference ~ .vdd
    .spi.levels.vih &= 0.7 * .vdd.voltage
    .spi.levels.vil &= 0.3 * .vdd.voltage
    .spi.levels.voh &= .vdd.voltage - 0.4V
    .spi.levels.vol &= 0.4V

    pins "SOIC-8" {
        .vdd.pos  power     "VCC" "8"
        .vdd.neg  power     "GND" "4"
        .spi.sclk input     "CLK" "6"
        .spi.mosi input     "DI"  "5"
        // Page 8, pin description: DO is "high impedance when /CS is high".
        .spi.miso tri_state "DO"  "2"
        .spi.cs   input     "/CS" "1"
    }
}

/// Zimu, an invented IMU. The core is on `vdd`, the pins on their own supply `vddio`.
part Zimu {
    .designator &= "U"
    .vdd: Rail
    .vddio: Rail
    .vddio.neg ~ .vdd.neg
    .spi: Spi

    .spi.role &= SpiRole::Slave
    // Table 4: the clock is at most 10 MHz.
    .spi.frequency <= 10MHz
    // Table 5, the levels of the pins on `vddio`, and table 3, the absolute maximum of an
    // input: VDDIO + 0.3 V.
    .spi.levels.reference ~ .vddio
    .spi.levels.vih &= 0.7 * .vddio.voltage
    .spi.levels.vil &= 0.3 * .vddio.voltage
    .spi.levels.voh &= .vddio.voltage - 0.2V
    .spi.levels.vol &= 0.2V
    .spi.sclk.abs_max &= .vddio.voltage + 0.3V
    .spi.mosi.abs_max &= .vddio.voltage + 0.3V
    .spi.cs.abs_max   &= .vddio.voltage + 0.3V

    pins "LGA-14" {
        .vdd.pos   power     "VDD"   "8"
        .vddio.pos power     "VDDIO" "5"
        .vdd.neg   power     "GND"   "6" "7"
        .spi.sclk  input     "SCL"   "1"
        .spi.mosi  input     "SDI"   "2"
        // Table 14: SDO is "high impedance while CS is high".
        .spi.miso  tri_state "SDO"   "3"
        .spi.cs    input     "CS"    "4"
    }
}
```

What the parts write that they did not before: one statement for the role, the levels they already could write for a `Logic` (once for the whole interface), a bound on the clock, and a seventh kind word. The pin tables are unchanged except `tri_state` where a datasheet says it. A part with a UART peer, `Zlink`, has `.uart: Uart` with `.uart.tx output "TX" "3"` and `.uart.rx input "RX" "4"`, and no role: both ends of a UART are the same.

A board that joins them as the buses want. The flash joins as a whole interface; the IMU shares `sclk`, `mosi` and `miso` and has the select of its own on `sel1`:

```nsm
import src::zparts::{Zcpu, Zflash, Zimu, Zlink}
import std::sources::Rail

board Sensors {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .cpu: Zcpu
    .flash: Zflash
    .imu: Zimu
    .link: Zlink
    .supply ~ .cpu.vdd
    .supply ~ .flash.vdd
    .supply ~ .imu.vdd
    .supply ~ .imu.vddio
    .supply ~ .link.vdd

    .cpu.spi.frequency &= 8MHz
    .cpu.spi ~ .flash.spi
    .cpu.spi.sclk ~ .imu.spi.sclk
    .cpu.spi.mosi ~ .imu.spi.mosi
    .cpu.spi.miso ~ .imu.spi.miso
    .cpu.sel1 ~ .imu.spi.cs
    .imu.spi.frequency == .cpu.spi.frequency

    .cpu.uart ~ .link.uart
}
```

It builds. `.cpu.uart ~ .link.uart` crosses `tx` to `rx` and `rts` to `cts` itself. The six messages follow, each from this board with the one change named. Each is one line in the output (wrapped here), names the part by its path, says which rule and which lines, and ends with what the author can do. The span is the statement of the part that the message is about, as for the other pin rules; line numbers are those of the board above.

### The six messages

**1. `mosi` joined to `miso`.** The right sides of lines 23 and 24 are swapped (`.cpu.spi.mosi ~ .imu.spi.miso`, `.cpu.spi.miso ~ .imu.spi.mosi`):

```text
error: net `net-cpu.spi.mosi` joins two different lines of an SPI bus: `mosi` (`cpu.spi.mosi`,
`flash.spi.mosi`) and `miso` (`imu.spi.miso`); an `Spi` joins `mosi` to `mosi` and `miso` to
`miso`, and `imu.spi.miso` is on the net of `mosi`
  --> main.nsm:10:5
   |
10 |     .cpu: Zcpu
   |     ^^^^^^^^^^
```

The same message for `net-cpu.spi.miso` follows. The rule reads the line each pin is of, so it holds with no net named and with the pins of any part that states a bus. Without it, the swapped board is accepted today: `miso` is `tri_state` or `bidirectional` on every slave and `mosi` an input.

**2. Two masters on one bus.** The board adds `.host: Zcpu` as line 14 and `.host.spi ~ .flash.spi` (and a supply for it):

```text
error: `cpu.spi` and `host.spi` are both `SpiRole::Master` on one SPI bus, and `Spi` allows 1
`Master` on a bus: they are on one bus through `net-cpu.spi.sclk` (`cpu.spi.sclk`,
`host.spi.sclk`, `flash.spi.sclk`, `imu.spi.sclk`); the slaves on it are `flash.spi` and
`imu.spi`; give the second master a bus of its own
  --> main.nsm:14:5
   |
14 |     .host: Zcpu
   |     ^^^^^^^^^^^
```

One message for the pair of interfaces, not one for each line. The rules that say "two outputs" and "only inputs" (ADR 0044) skip a net that a bus rule has already reported, so the board does not get four more lines for the same fault.

**3. A UART's `tx` joined to `tx`.** The board writes `.cpu.uart.tx ~ .link.uart.tx` and `.cpu.uart.rx ~ .link.uart.rx` pin by pin in place of line 28:

```text
error: net `net-cpu.uart.tx` joins `tx` to `tx` (`cpu.uart.tx`, `link.uart.tx`): `tx` is the line
that a `Uart` sends on, so it is joined to the other end's `rx`; write
`.cpu.uart.tx ~ .link.uart.rx`, or join the two interfaces with `.cpu.uart ~ .link.uart`, which
crosses `tx` to `rx` and `rts` to `cts`
  --> main.nsm:10:5
   |
10 |     .cpu: Zcpu
   |     ^^^^^^^^^^
```

The same message for `rx` follows. A third end is the same message about `limit 2`: "a `Uart` has 2 ends on a bus, `a.uart`, `b.uart` and `c.uart` are on one".

**4. A 3.3 V line onto a 1.8 V pin.** The IMU takes its pins' supply from a 1.8 V rail (a rail `.io` of 1.8 V +/- 5% named `1V8` and `GND`, and `.io ~ .imu.vddio` in place of line 17). The messages are those of ADR 0046; what changes is that a bus pin is judged:

```text
error: net `net-cpu.spi.mosi` reaches 3.465V above its ground `GND` (`cpu.spi.mosi`, a line of
`cpu.spi.levels.reference`) but `imu.spi.mosi.abs_max` is 2.01V..2.19V: it must not exceed 2.01V
error: net `net-cpu.spi.miso`: `imu.spi.levels.voh` is 1.51V..1.69V but `cpu.spi.levels.vih`
needs 2.1945V..2.4255V: a high of 1.51V is not always read as a high, which takes 2.4255V
```

The first appears for `sclk` and `mosi`, the inputs that state `abs_max`; `cs` is on `cpu.sel1`, a plain pin whose swing is not known, and is a note that says `cpu.sel1` can drive it and is not a bus pin, so how far it swings is not known. The second is the way back, on `miso`. Both point at `.imu: Zimu`. A translator between the two supplies is a part with an `Spi` on each side (below); the board joins the processor to its `a` side and the IMU to its `b` side, and neither message appears.

**5. Two drivers on a shared MISO that are not all `tri_state`.** Line 12 becomes `.imu: Zgyro`, a part like `Zimu` whose datasheet draws SDO as an output and never says what it does while CS is high, so its row is `.spi.miso output "SDO" "3"`:

```text
error: net `net-cpu.spi.miso` has two drivers and `imu.spi.miso` is not `tri_state`: `imu.spi.miso`
(`Zgyro`, an output) and `flash.spi.miso` (`Zflash`, a `tri_state` output) both drive it, and
`imu` drives it while `flash` is selected; the datasheet of `Zgyro` must say that SDO is high
impedance while CS is high (then write `tri_state` for `.spi.miso` with the page), or `imu` is
given a line of its own through a buffer that is
  --> main.nsm:12:5
   |
12 |     .imu: Zgyro
   |     ^^^^^^^^^^^
```

With `output` on both parts it is the message of ADR 0044, "net `net-cpu.spi.miso` has two outputs: ...", with the same last clause added: a line that several parts share needs every driver `tri_state`.

**6. A bus clock above a part's limit.** Line 20 becomes `.cpu.spi.frequency &= 20MHz`. The IMU's limit is 10 MHz and the flash's 33 MHz:

```text
error: bus `cpu.spi` is joined at 20MHz, which `imu` (`Zimu`) does not allow: its limit
`imu.spi.frequency` is at most 10MHz; `flash` (`Zflash`) allows 33MHz and `cpu` (`Zcpu`) 32MHz,
so the bus runs at 10MHz at most
  --> main.nsm:12:5
   |
12 |     .imu: Zimu
   |     ^^^^^^^^^^
```

Stage 0 gives the board the solver's existing message for the same fault, which names `imu.spi.frequency` and the two values and not the other parts; the text above is stage 3 (section 5).

### Four more

Each is a rule of the same design and is shown because it is one of the problems the design was asked for.

**7. Two slaves' selects on one net** (the IMU's select joined to the flash's, as in the board that began this ADR). Line 25 becomes `.imu.spi.cs ~ .flash.spi.cs`; the flash's select is already on the net of `cpu.spi.cs`:

```text
error: net `net-cpu.spi.cs` carries the `cs` of two slaves, `flash.spi.cs` and `imu.spi.cs`: `Spi`
gives each slave a select of its own (`cs` is `single`), or both are selected together; join
`.imu.spi.cs` to a pin of its own
```

**8. A pin table against the role.** `Zimu` writes `.spi.mosi output "SDI" "2"`:

```text
error: `imu.spi.mosi` is an output in the table of `Zimu`, but `SpiRole::Slave` makes `mosi` an
input of `Spi`; the datasheet's SDI pin is an input
```

**9. A slave's data out left open.** `.imu.spi.miso` joined to nothing (its role requires it, `miso output required`):

```text
error: `imu.spi.miso` is an output that must be connected (pad 3, `SDO`): `SpiRole::Slave` of
`Spi` requires it, and nothing else is on its net
```

**10. A `bidirectional` where the role gives a direction.** A part that writes every pin `bidirectional` is accepted, as today, with a guideline warning (ADR 0030, `guidelines = "warn"`):

```text
warning: `imu.spi.miso` is `bidirectional` in the table of `Zimu`, and `SpiRole::Slave` makes `miso`
an output; write `output` or `tri_state`, as the datasheet says, unless the datasheet says the
pin also receives
```

This is how an old table is moved to honest kinds without breaking its build.

## 1. What the checker judges today

- The pin kind of a row decides the net rules: `outputs` (two `output` on one net) and `floating_inputs` (only inputs), in `crates/netsmith-check/src/faults.rs`; the kind of a chosen function overrides the row's (ADR 0046, `Model::pin_kind` in `pinout.rs`). `bidirectional` is never a conflict and always a driver.
- A bus interface is an interface of `Pin` members. `std::bus::I2c` is also found by its name for the pull-up rule (`crates/netsmith-check/src/buses.rs`); no other bus has a rule. `~` between two interfaces of one definition joins every pin of each name and equates every value that is not a current and not a `Logic` level (`connect_interfaces` in `connect.rs`); two interfaces that nest a rail join the rails' pins too, which is why `Logic ~ Logic` between two supplies is a short and the guide joins `.line` to `.line`.
- The levels are four voltages and a reference rail on `Logic`, whose `.line` is the pin. `crates/netsmith-solve/src/levels.rs` finds a line by `type_name == "Logic"` and `name == "line"` and judges nothing else (guide: "The level checks apply only to lines that are `Logic`").
- A generic's `pins` rows must have the provider's kind (ADR 0068, section 3, rule 2), so a kind the generic names is a contract.
- `required` on a member (ADR 0069, removed by ADR 0065) was judged for every instance in the model but the root's, whether or not the board used it.

## 2. The constructs

### The roles: an `enum` and a `lines` table

A bus that has more than one role declares an enum of them (ADR 0054) and a member of it, and a `lines` table for each. A bus with one role (`Uart`) writes `lines` with no role. The whole of `std/bus.nsm` for `Spi`, `Uart` and `I2c` after this ADR:

```nsm
/// The role of a part on an SPI bus.
enum SpiRole {
    /// Drives `sclk` and `mosi`, and reads `miso`.
    Master,
    /// Reads `sclk` and `mosi`, and drives `miso` when it is selected.
    Slave,
}

interface Spi {
    .role: SpiRole
    .sclk: Pin
    .mosi: Pin
    .miso: Pin
    .cs: Pin
    .levels: Levels
    .frequency: Frequency

    lines SpiRole::Master limit 1 {
        .sclk output
        .mosi output
        .miso input optional
        .cs   output optional
    }
    lines SpiRole::Slave {
        .sclk input
        .mosi input
        .miso output required
        .cs   input single
    }
}

interface Uart {
    .tx: Pin
    .rx: Pin
    .rts: Pin
    .cts: Pin
    .levels: Levels
    .baud: Frequency

    lines limit 2 {
        .tx  output crosses .rx
        .rx  input
        .rts output crosses .cts optional
        .cts input optional
    }
}

enum I2cRole { Controller, Target }

interface I2c {
    .role: I2cRole
    .sda: Pin
    .scl: Pin
    .levels: Levels
    .frequency: Frequency

    lines I2cRole::Controller {
        .sda bidirectional
        .scl bidirectional
    }
    lines I2cRole::Target {
        .sda bidirectional
        .scl input
    }
}
```

`Swd` has `SwdRole { Probe, Target }` (probe: `swdio` bidirectional, `swclk` and `reset` outputs, `swo` an optional input, `limit 1`; target: `swclk` and `reset` inputs, `swo` an optional output) and `Usb2` has `UsbRole { Host, Device }` (`dp` and `dm` bidirectional, `limit 1` each). `Usb2` has no `.levels`: its signalling is not `Logic`. The tables are the protocols and nothing a datasheet decides.

**A row of `lines`** is `.line kind` and optional words, in the form of a pin row with no name and no pads (the form a generic's contract rows already have, ADR 0068). The words:

- `optional` and `required`: as on a pin row, they decide whether the line must be connected. A part's own row wins over the role's, and the role's over the kind's default (an output is optional, an input required). So `miso output required` is what refuses a slave's open data out (message 9).
- `limit N` after the role: at most N interfaces of this role on one bus. A bus is the interfaces of one definition that share a net (section 3). `Master limit 1`, `Probe limit 1`, `Host limit 1`, `Device limit 1`, and for `Uart` with no role, 2 ends.
- `crosses .other`: the line is joined to `.other` of the other end, not to its own name. It is written on one of the two lines and holds for both. A line without `crosses` is joined to its own name.
- `single`: a net carries this line of at most one part of the role. A select line belongs to one slave.

**The part states its role** with `&=` (`.spi.role &= SpiRole::Slave`), like any value. A processor whose SPI unit can be either leaves it out of the part and the board states it, `.cpu.spi1.role &= SpiRole::Slave`, as a board chooses a function. `~` between two interfaces does not equate `role` (it is what the part is, not a value the bus shares), by the rule that already keeps `current` and the four levels apart (`is_flow`, `is_level`); the new test is a field of an enum that a `lines` table keys on.

### The rules, all read from the nets

All of them run in `pin_faults`, after the nets are made, on the terminals (the pins of parts; a port of a module is only a way through, as in ADR 0044). They need no statement order. A bus pin is a terminal that is a member of an interface that has `lines`.

1. **Table against role.** For a part that states a role, each row of a line the role has must have a kind that the role allows: the same kind; `bidirectional`, which any role allows (a pin that can drive and receive is not contradicted); `tri_state` for an `output`; `passive` (a connector pin); and for a role line that is `bidirectional` (I2C), any of input, output, tri_state. Anything else is message 8. A `bidirectional` row on a line whose role gives a plain direction is the guideline warning of message 10.
2. **One line on one net.** All the bus pins on a net, of whatever parts, are the same line, or lines that `crosses` pairs, or an error (messages 1 and 3). Two interface definitions on one net is the same error naming both. A line that `crosses` and meets itself (`tx` to `tx`) is the error of message 3.
3. **The limit.** The interfaces that share a net form a bus; the number of one role on it is judged against `limit` (message 2). The rules of ADR 0044 for a net that a bus rule has reported are skipped.
4. **`single`.** A net has at most one bus pin of a `single` line among the pins of the role (message 7).
5. **Required lines.** A role's `required` or `optional` is the requirement of the pin when its row states none (message 9). It is judged only for a part whose interface has any line connected: an interface a part declares and the board never uses is left alone.
6. **Shared drivers.** A net with two or more drivers (pins whose kind is `output` or `tri_state`; `bidirectional` is as today, never counted) is an error if any of them is a plain `output` (message 5, and the existing message 'two outputs' when every one is). A net whose drivers are all `tri_state` is accepted: that at most one of them is selected at a time is a fact of the protocol, which firmware and the chip select carry, and the checker says no more.
7. **Roles that are not stated.** A part whose interface states no role is not judged by 1 (table against role) and does not count in 3; the rules of 2, 4 and 6 are judged for its pins all the same, since they need no role. When some part on a bus states a role and another does not, the checker prints `not evaluated: the role of `x.spi` is not stated, so the limit and the table of `x` are not judged`. A bus on which no part states a role gets no note: every design written before this ADR means what it meant.

### `tri_state`

A seventh kind of a pin: **`tri_state`**, an output that drives only while its part is selected and is high impedance otherwise. It is KiCad's `tri_state` electrical type, which the symbol check already accepts for an `output` pin (`suits` in `netsmith-pcb/src/validate.rs`) and the schematic view already draws (`Flow::TriState`). It is a kind and also a function kind (`fn SPIS_MISO:tri_state`, ADR 0046), a word where the six already stand.

- It is a driver for the net rules and for the levels (`levels.rs` `is_driver`), and it is an `output` for the pin that must be connected (optional). It is not `bidirectional`: it does not receive.
- **A datasheet that is silent.** The kind a part writes is what its datasheet says. A datasheet that says an output is high impedance when it is not selected gives `tri_state`; one that draws an output and says no more gives `output`. The language does not add a fourth value for "silent": `output` already has that meaning, because it is what the checker refuses to share. A board that shares such a line gets message 5, which says what to read, and a part is never refused for being silent. The alternative, a note, would let a board with two drivers build and leave the fault to the first person who measures the line; ADR 0044 already refuses two outputs on a net, and refusing one `output` with one `tri_state` is the same rule.
- **A part cannot claim `tri_state` without a source.** The checker cannot read a datasheet. The rule is the one that already holds for a rail's range and a level (`docs/DESIGN-RULES.md`, "A part states its own requirements": the page cited): a part writes `tri_state` with the page in a comment, as in the examples, the guide says so, and a review reads it. A generic may state `.spi.miso tri_state` as a contract row, and then a provider whose row says `output` is passed over with the message as its reason (ADR 0068, section 5).
- **`bidirectional` stays accepted** for a driver and a receiver on a shared line. It is what a GPIO is, and the whole library that already writes it keeps building. The warning of message 10 is the way a table is moved to the honest kinds when a role says which kind it is.

### Levels: `Levels` on every bus

`std/logic.nsm` gains one interface beside `Logic`:

```nsm
/// The logic levels of all the lines of a bus interface, as a datasheet gives them for a
/// supply: the four voltages of `Logic` and the rail they are relative to.
interface Levels {
    .reference: Rail
    .vih: Voltage
    .vil: Voltage
    .voh: Voltage
    .vol: Voltage
}
```

and each bus interface but `Usb2` has `.levels: Levels`, as in `Spi` above. A part states them as it states `Logic`: `.spi.levels.reference ~ .vdd`, `.spi.levels.vih &= 0.7 * .vdd.voltage`, and an input's `.spi.sclk.abs_max &= .vddio.voltage + 0.3V`. The levels are of the interface, not of each line: a datasheet gives one VIH, VIL, VOH and VOL for the I/O supply, and the direction of each line comes from the role (so `voh` and `vol` judge the lines the part drives, `vih` and `vil` the lines it reads). A part that has two supplies for two groups of pins states two interfaces.

What changes in the solver is one function. `levels.rs` finds the `Logic` of a pin with `logic_of`; the same function returns the `Levels` of a bus pin (a pin whose parent has a member `levels` of type `Levels`), and everything that follows reads `reference`, `vih`, `vil`, `voh`, `vol` from whichever it got, as it does today. The check of ADR 0046, the absolute maximum against the net's swing, and the notes ("not evaluated" when a level is missing or the grounds differ) are unchanged. A part that states no levels and a part that does are judged as `Logic` is: a pair with no level on either side is not looked at, and one with a level on one side only is a note.

- **`~` between two bus interfaces does not join `levels`.** `connect_interfaces` recurses into every nested interface; it would join the reference rails of two parts on different supplies, a short between a 3.3 V net and a 1.8 V net. The skip is by the name `Levels` (as `Logic` is found by its name), and it also skips `role`. A general declaration for "a member that a join leaves alone" is question 6.
- **A level translator** is a part with two bus interfaces and one role each, each on its own rail. A translator for SPI between a 3.3 V processor and a 1.8 V sensor:

```nsm
part Zshift {
    .vcca: Rail
    .vccb: Rail
    .a: Spi
    .b: Spi
    .a.role &= SpiRole::Slave
    .b.role &= SpiRole::Master
    .a.levels.reference ~ .vcca
    .b.levels.reference ~ .vccb
    ...
}
```

  The processor is a master to `a` and the sensor a slave to `b`; each side's levels are judged on its own rail. `sclk`, `mosi` and `cs` go from `a` to `b`, `miso` the other way, and that is what the roles say, with no statement of direction in the translator. A bidirectional I2C translator is the same with `I2cRole::Target` on one side and `Controller` on the other. A single-supply buffer used as a translator (`LevelTranslator`, ADR 0063) states plain pins and thresholds and is judged by its own relations; a bus does not pass through it by role.
- **A level is a number the part states with its source**, as ADR 0046 does for the first part with levels. No part in `std/` is migrated by this ADR (none has a bus with levels it could cite).

### Speed and mode

**Speed.** `.frequency: Frequency` is a member of `Spi`, `I2c` and `Swd`, and `.baud: Frequency` of `Uart`. `~` between two interfaces equates the two values already (they are not currents and not levels), so one value runs along a bus. A part states its datasheet's limit as a bound on it, `.spi.frequency <= 10MHz`, and the board states the clock it will run, `.cpu.spi.frequency &= 8MHz`. The value is the lowest bound without any new construct, and the solver's existing contradiction is the error when the board's clock is above a bound. This is the smallest representation, and it belongs in the language because it is an ordinary field and an ordinary bound: stage 0 builds it with no change to the checker.

- The board states the clock, but the firmware sets it (ADR 0071 section 3 said so), so the statement is the board's intention and the check is "the intention is within every part's limit". A board that states none is left alone; stage 3 adds a note when a bus has parts with limits and no clock.
- **`required` is not used** on `.frequency` or on `.role`. ADR 0069 (removed by ADR 0065) judged every instance in the model but the root's, so a processor that declares four SPI interfaces and uses one would be refused for the three it does not use. What the design needs is a check on the interfaces that are joined, which is what the note of stage 3 is.
- **A bus joined pin by pin does not share the value**, since only `~` equates (the example's `.imu.spi.frequency == .cpu.spi.frequency` is how a board says it). Stage 3 equates the `frequency` of the interfaces that the nets show to be on one bus, and the line disappears from the example.
- **Mode.** A device that supports modes 0 and 3 cannot say so with a value: there is no set-valued field, and a mode shared by `~` that two parts narrow to different values would refuse a pair that is fine. The mode stays in the part's doc comment, where the editor shows it, as the revision and firmware rules of ADR 0071 do. One optional step covers a part that supports exactly one mode: `enum SpiMode { Mode0, Mode1, Mode2, Mode3 }` and `.mode: SpiMode` on `Spi`, equated along the bus; a part with one mode states it, a part with two states nothing. It catches the pair that cannot work (one mode 1 part and one mode 3 part) and nothing that is fine. Recommended as stage 3's optional item; question 8.

## 3. A bus, and the ports that are not parts

A **bus** in these rules is the set of interfaces of one definition that share a net: the checker takes the bus pins of each net and joins the interfaces they belong to (the processor's `spi` and the flash's `spi` are on one bus because `sclk` is on one net). A part whose bus pin is on a net with no other bus pin is on a bus of its own, so the limit and the table are judged for it alone.

- **Modules.** A module declares `.bus: Spi` as a port. Its pins are not terminals and the module's own `role` is not read by the rules; the part inside is. `.bus ~ .imu.spi` inside a module is a join like any, and a role that a module wants to state for its callers is `.bus.role == .imu.spi.role`, the equation ADR 0071 section 5 uses for a function. A module whose UART is drawn "from the processor's side" (a module that names its port's `tx` after the other end's `rx`) is the case that breaks (section 6).
- **Boards in a system.** A board's port of a bus type is joined to another board's by the system, which equates the values and joins no pins (`join_ports`). The rules above judge pins inside one board and are not run on the ports of a system; `mate.rs` has its own driver counts by row kind. A bus between two boards is carried by the mated connectors' contacts, as in `examples/carrier_and_cards.nsm`. Roles on ports and across a mate are question 7.
- **Generics and providers (ADR 0068).** A generic states what every provider has: `.spi: Spi`, `.spi.role &= SpiRole::Slave`, and `pins { .spi.miso tri_state }` as a contract row, so a board that uses `.mem: SerialFlash` is judged before the solver picks a part, and a provider is passed over when its table has `output` for the line. A provider states its own bound, `.spi.frequency <= 33MHz`, and its levels. `lines` is an interface's item and is refused in a generic, a part, a module and a board.
- **Peripherals that share hardware (ADR 0071).** A processor whose SPI lines are any pin of a class has no `.spi` member to state the role on: its master end is a function chosen on a pin (`SPIM0_SCK` on `.p02`), and a function names no bus. Without a link from the function to a line, the master's end is invisible to the rules above: `mosi` joined to a slave's `miso` is judged only when both pins are bus pins. The link is one clause on the `peripheral` of ADR 0071 and is stage 4:

```nsm
peripheral SPIM0 on APP_GPIO as Spi SpiRole::Master {
    SPIM0_SCK:output  as sclk
    SPIM0_MOSI:output as mosi
    SPIM0_MISO:input  as miso
}
```

  The peripheral is the bus end (`u`, `SPIM0`), a pin that carries one of its functions is a bus pin of that line and role, the `limit` counts it, and the board's `.u.p02.function &= "SPIM0_SCK"` is the statement that puts it on the bus. A slave unit is `as Spi SpiRole::Slave` with `SPIS0_MISO:tri_state as miso`. This depends on ADR 0071 being accepted, and until it is, the master of such a processor is judged by its kinds and by the slave side of every rule, which catches the faults among the slaves (messages 7, 8 and 9) and the pins of a slave that no master line explains (message 1 when the other pin is a slave's).
- **The role is an enum value, not a constant of the checker (ADR 0054).** It is typed, it has the nearest-name message for a typo, it is read by `==` in a module, and `std/bus.nsm` is where a reader finds the roles. The cost is one enum for each bus, and a name for the role that does not collide with the interface (`SpiRole`, not `Spi`). It depends on ADR 0054, which is proposed and implemented in the working tree (`std/prelude.nsm` has `AssemblyMode`); if the owner changes the syntax of an enum, the roles change with it.

## 4. What is carried and what is left out

**Carried.**

- Who drives which line of the five standard buses, as the protocol states it, and which kinds of table row agree with it.
- One line on one net, the crossing of a UART, the select of one slave, the limit of masters, hosts, probes and ends.
- A shared line whose drivers are not all high impedance when not selected, and a way for a datasheet that is silent to say so.
- The levels and absolute maximum of every bus pin, with the numbers of ADR 0046, and a translator as a part with two buses.
- The clock of a bus against the lowest limit of its parts.

**Left out, with the reason.**

- **The address on an I2C bus.** Two targets at one address are not judged. An address is a number that a part states (a fixed one, or a choice made by a pin), and a rule that compares them wants a set of addresses for a part with a strap; left open (question 9).
- **Open-drain.** I2C lines stay `bidirectional` with the pull-up rule of ADR 0044 (by name, `buses.rs`). A kind `open_drain` (KiCad's `open_collector`) would let that rule and the shared-line rule be one; ADR 0044 declined it for the grammar and this one does too. Question 10.
- **Whether a driver really is high impedance.** The checker takes the table at its word. A `tri_state` that the datasheet does not say is the part author's error, as a wrong `abs_max` is.
- **Timing, setup and hold, drive strength, the number of devices a bus can load.** Datasheet numbers that need a model of the traces; doc comments.
- **Bus arbitration, multi-master I2C, SPI daisy chains, QSPI, JTAG, CAN, ethernet.** The language has no way to say a list of roles that repeat; each is another interface with its own `lines`, and none needs a new construct. A `Quad` SPI is an interface with four data lines and `lines` of its own.
- **Dual-role USB.** A part that is a host or a device by what is plugged in (OTG) has no role to state: it leaves `.usb.role` out and is not judged by the limit. A role that a part can take from a pin is not a value.
- **Where the board states the role of a part** that has two (`.cpu.spi1.role &= ...`). It is a statement the board makes about the part's use, as a function choice is, and ADR 0048's physical file does not take it.

## 5. Staged plan

Counts are from reading the code on this branch. A test is a Rust or TypeScript test; a guide example is a fenced block of `docs/LANGUAGE.md`, which `crates/netsmith/tests/guide.rs` runs (the file has 227 `nsm` blocks; the section "Buses" has 6 of them).

**Stage 0: the clock (no language change; can be built before the review).** `.frequency` on `Spi`, `I2c` and `Swd`, `.baud` on `Uart`, in `std/bus.nsm`. Tests: 3 in `crates/netsmith/tests/std_kinds.rs` (a clock inside every bound builds; above a bound is the solver's error naming the field; a clock joined by `~` reaches the third part). Guide: 1 example in "Buses". 3 files, about 40 lines. Delivers message 6 as the solver words it today, for boards that join by `~`.

**Stage 1: `tri_state`, `lines` and the roles.** Delivers messages 1, 2, 3, 5, 7, 8, 9 and 10, which is every fault in the problem list that does not need a level or a clock and does not go through a processor's any-pin peripheral. Can be cut in two (1a, 1b) at the commit: 1a is the kind and the shared-line rule and stands alone.

- **1a, the kind.** `netsmith-lang`: `ast.rs` (`PinKind::TriState`, its word and the `ALL` list), `parser.rs` (the list of words in the two places that read a kind, the message list); `netsmith-core/src/lib.rs`; `netsmith-lower/src/lib.rs` (one arm); `netsmith-check`: `pins.rs` (three arms: the word, the requirement default, the article), `faults.rs` (`outputs` counts drivers, `floating_inputs` unchanged since a `tri_state` is not an input), `mate.rs` (one arm), `model.rs` (two kind tests); `netsmith-solve/src/levels.rs` (`is_driver`); `netsmith-view/src/glyph.rs` (two arms, `Flow::TriState` exists); `netsmith-pcb/src/validate.rs` (`suits` and its test table); `netsmith/src/editor/analysis.rs` (one arm); the editor page (`nsm.ts` keyword, `nsm.test.ts` 1 test, the bundle). 12 Rust files and 3 page files, about 90 lines. Tests: about 8 (parse and print; each net rule with a `tri_state`; the symbol check; a function kind). Guide: 3 examples; `GLOSSARY.md` (Pin kind); the `netsmith-write-nsm` and `netsmith-read-errors` skills.
- **1b, `lines` and the rules.** `netsmith-lang`: `ast.rs` (`Item::Lines`, a struct, the row words), `parser.rs` (a contextual keyword after `tie`, a function that reuses the contract-row parser of a generic, the `only_decls` list), `printer.rs` (one arm). `netsmith-check`: `crates/netsmith-check/src/lines.rs` (new: the declaration, the six rules and their messages, about 450 lines), `model.rs` (`Instance.lines`; the struct literal is written in three places in `checker.rs`), `checker.rs` (the arm of `process`, the `not_physical` arms, the refusal outside an interface), `connect.rs` (`connect_interfaces`: skip `role`, cross the `crosses` pairs, about 40 lines), `pinout.rs` (the requirement of a pin reads the role), `faults.rs` (call the rules; skip the nets a bus rule reported), `providers.rs` (refuse `lines` in a generic), `lib.rs`. `netsmith/src/editor/definition.rs`: one arm. `std/bus.nsm`: 4 enums and 5 tables. Tests: about 8 in `crates/netsmith-lang/tests/pins.rs`, about 22 in a new `crates/netsmith-check/tests/lines.rs`, and 4 existing tests of `nets.rs` and `std_kinds.rs` change their expected text. Guide: "Buses" is rewritten (6 examples change, about 8 are added); `GLOSSARY.md` (Role, Line, Bus, Tri-state); 4 skills.

Totals of stage 1: 8 Rust crates (lang, core, check, lower, solve, view, pcb, netsmith), about 20 source files, 3 page files and the bundle, about 30 new tests, about 14 guide examples touched, about 5 documents. The first step of 1b is a measurement, not code: add `.levels: Levels` to one interface (stage 2's member) and run the workspace, to see whether an unjoined nested rail makes notes on every part with a bus (section 6, item 4).

**Stage 2: levels.** `Levels` in `std/logic.nsm`, `.levels` in four interfaces, the skip in `connect_interfaces`, and `logic_of` in `levels.rs` (it is called by `lines_on`, `absolute_maximums` and `net_range`, three places). Tests: about 10 in `crates/netsmith-solve/tests/levels.rs` (it has 10 now). Guide: 4 examples (a bus at one level; message 4 in both directions; a translator; a part with two supplies). `GLOSSARY.md` (Logic level, amended), ADR 0046 gets a line. 5 files and about 120 lines. Delivers message 4.

**Stage 3: the clock rules.** The equation of `frequency` between interfaces on one bus from the nets, the note for a bus with limits and no clock, the message naming the limiting part (the bound is moved to a field of its own that a join does not equate, `.spi.max_frequency`, and the rule compares the shared `frequency` to the lowest), and the optional `SpiMode`. 3 files in `netsmith-check` and `std/bus.nsm`; about 8 tests; 2 guide examples. Delivers message 6 as written above.

**Stage 4: the peripheral's bus clause, after ADR 0071 stage 1.** `peripheral ... as Spi SpiRole::Master { F as line }` in the parser and `shared.rs`'s data, and the bus pins that the rules of stage 1 read from a pin's chosen function. About 4 files and 10 tests. This is what makes the rules see the master of an any-pin processor.

**Stage 5: the role becomes expected.** A joined bus interface with no role is a guideline warning, an error under `--strict`, and later an error, as the owner decides (question 4). The library's parts that have a bus and the examples state their roles in this stage.

Stage 1 alone makes the checker know the protocols and refuses the miswirings among the parts that have bus members, and it gives a datasheet a way to say that it is silent. It does not judge levels, the clock, or a processor whose bus pins are plain pins.

## 6. What it breaks, and the migration

Counted by `grep` on this branch for `Uart`, `Spi`, `I2c`, `Swd` and `Usb2`:

- `std/`: `std/bus.nsm` (the five interfaces; changes) and `std/parts/connector.nsm` (`UsbC2` has `.usb: Usb2`, four lines; it states no role, since a receptacle is not an end, so it is not judged and does not change).
- `examples/`: `carrier_and_cards.nsm` (5 lines: a controller with a `Uart` and an `Spi`, an SPI sensor, a UART chip; the buses reach the sockets pin by pin through connectors, so the rules find one bus pin on each net and say nothing) and `enclosure_sensor/enclosure_sensor.nsm` (3 lines: an `I2c` sensor and `I2cPullups`; unchanged).
- `docs/LANGUAGE.md`: 28 lines in the sections "Buses" and "Logic levels", 6 `nsm` blocks. The examples `Straight` (a UART joined with `~`, which fails today) and `TwoMasters` (SPI) change: `Straight` becomes a board that builds, and the failing UART example is the pin by pin `tx ~ tx` of message 3.
- Tests: `crates/netsmith-check/tests/nets.rs` (4 lines), `crates/netsmith/tests/std_kinds.rs` (2 lines; `a_uart_joined_straight_has_two_outputs_on_a_net_and_two_inputs_with_no_driver` changes), `std_models.rs` (2), `pullup.rs` (2), `required.rs` (1), and the fixture `scripts/editor-verify-js/fixtures/system_view.nsm` (13 lines; five interfaces joined with `~`, no role).
- Projects outside this repository are not counted; a project is changed by the one case below and by whatever it chooses.

**Can existing designs keep building unchanged?** Yes, except one. Roles are optional and a part that states none is judged as it is today: the table is not compared to a role, `limit` does not count it, and the net rules for two outputs and only inputs are as they were. What changes for a design that states no role:

1. **`~` between two `Uart` interfaces crosses.** A design that joins two UART interfaces with `~` and is right today (a module that draws its `uart0` from the processor's side, so that two processor-side views join `tx` to `tx` in the board) now has its `tx` joined to the other's `rx`, and the net with two `output` pins is reported where the module is. The fix is mechanical: the module draws its port as a part draws it (its `tx` is the line the module sends on) and the board joins with `~`, or the module keeps its drawing and the board joins pin by pin as it does today (a pin by pin join `tx ~ rx` is accepted under both). The message names the net, and the in-repository cases are the guide's `Straight` and one test. Whether to keep a release in which `Uart ~ Uart` does not cross is question 2.
2. **The rules that need no role run for every part that has a bus pin**: `mosi` on `miso`, a `UART` `tx` on `tx`, two `cs` of two slaves on one net, a `tri_state` with an `output`. A design that has any of these has a fault that was accepted before; none is in `std/`, `examples/` or the guide.
3. **`tri_state` is a new word** in the kind slot; a part with a pin named `tri_state` is unaffected (a kind is read after the path).
4. **Every bus interface gains members** (`role`, `levels`, `frequency` or `baud`). A part that declares a bus and never states them has them unset, and unset members are not judged. `levels` holds a rail that no part joins: stage 1's first step measures whether that makes a note on every part with a bus (the power view and the balance both list rails; `Logic` joins its reference rail in every part that has one, and a bus interface does not). If it does, the fallback is `.levels` in a separate interface `SpiLevels` that a part declares when it states levels, and the solver finds the link by the pin's parent.

The library's own parts that could state a role are the connectors and none of them is an end. No part in `std/` is migrated by this ADR.

## 7. Rejected alternatives

- **One interface for each role** (`SpiMaster`, `SpiSlave`): `~` between two interfaces needs one definition (`cannot connect a SpiMaster to a SpiSlave`), a part with both roles needs two members, and a module that forwards a bus needs a type for each side.
- **The roles in the checker, by interface name**, as `I2c` and `Logic` are found now: the quickest, and a project's own bus has none. ADR 0054 rejected a built-in enum for the same reason: the owner asked for std to state it where a reader can open it.
- **Each line's direction written on the interface member** (`.mosi: Pin output`): states one role. A bus with two roles has two directions for one line, which is why the table is for each role.
- **A DTE and a DCE role for `Uart`**: a role for a naming convention. A part's path (`.uart.tx`) is the line it sends on whatever the datasheet calls the pin, and the datasheet's name is the row's name column. `~` between two ends crosses, the case of a part that draws the other end's view is a row written with the other path, and no enum is needed. The cost is the mechanical edit of section 6.
- **A pin kind `open_drain` and `high_impedance` instead of `tri_state`**: KiCad has `tri_state` and `open_collector`. `tri_state` is the one this ADR needs; `open_drain` waits (question 10).
- **A note, not an error, for an `output` on a shared line**: section 2 gives the reason; a net of two drivers that is accepted is the fault this ADR was asked to find.
- **A kind `tri_state` that the role gives** (every `Slave` is `tri_state` on `miso`): invents a datasheet fact for every slave, which `docs/DESIGN-RULES.md` forbids.
- **Role checks at the `~` statement**: a statement that reads the role before the board's statements are all read depends on their order (`docs/DESIGN-RULES.md`, "Statement order never matters"), and a pin by pin join has no interface statement. The net pass reads the final model and serves both.
- **`Logic` for every bus line** (`.mosi: Logic`): the path of every bus pin gains `.line` (`.spi.mosi.line`), which breaks every part and every board that names a bus pin (section 6 counts them by interface use), and each line states four levels where a datasheet gives four for the supply. A `Logic ~ Logic` also joins reference rails, as above.
- **The four levels directly in each bus interface** (no `Levels`): five voltages and a rail repeated in four interfaces, and `is_level` would be a test of five names. A shared interface is the same text once and the skip is one name.
- **A `max_frequency` field for each part and a rule that takes the lowest from the start**: needs an exemption from `~`'s equation for the field and a solver rule, for a message that stage 3 delivers; a bound on the shared field costs nothing and does the check.
- **A mode as a set** (`.modes &= 0 | 3`): there is no set-valued type, and a type with a list of values is the enum of ADR 0054 with a second meaning.
- **`required` on `.role` and on `.frequency`**: judged for every instance and refuses a part that declares a bus it does not use (section 2).
- **Making the role a field of the pin row** (`.spi.mosi input role slave`): a role is the part's, not the pin's, and the table would repeat it for every pin.

## 8. Open questions

Each has a recommendation.

1. **The words.** `lines`, `role` (as the member's name), `limit` (from ADR 0071), `single`, `crosses` and the kind `tri_state`. Recommend them: they are short, read at the start of a statement or after a kind, and avoid the glossary's retired words. Two collisions to settle: **role** is under _Avoid_ for **Generic** in `GLOSSARY.md` (as a name for a generic) and is an enum name inside `crates/netsmith-check/src/mate.rs` (the contact's role in a mate); recommend keeping `role` for a part's end of a bus, adding a **Role** entry and narrowing the Generic's _Avoid_ to "role (of a generic)", and naming the new rules module `lines.rs`, not `roles.rs`. **Bus** is under _Avoid_ for **Interface** (as a name for an interface); recommend defining **Bus** as "the interfaces of one definition that share a net". `master` and `slave` are the datasheets' words and the line names (`mosi`, `miso`) say them; `Controller`/`Target` for SPI is the 2020 usage and avoids a clash with ADR 0071's `peripheral`. Recommend `Master`/`Slave` for `Spi` because the lines already say so, and `Controller`/`Target` for `I2c`; changing either is one line of `std/bus.nsm` and the tests.
2. **`Uart ~ Uart` crosses, with no release in between.** Recommend yes: it is what the owner asked for ("crossing handled by the language"), a design right today has one mechanical edit, and a release that crosses `rts` but not `tx` would be a second way. The alternative is a warning in `connect_interfaces` first.
3. **`~` between two `Spi` joins `cs`** (as it does now) or leaves `single` lines to the board. Recommend keep joining in stage 1: every existing board joins by `~` and nothing changes. A second slave on one master interface is then a board that joins the shared lines pin by pin, which message 7 explains. If the cost shows, stage 3 makes `~` skip a `single` line.
4. **When a joined bus without a role is an error.** Recommend a guideline warning when the library and the examples state their roles (stage 5), an error one release later. The warning belongs under ADR 0030's `guidelines` because it is advice, and the electrical contradictions (messages 1 to 5, 7 to 9) are errors from stage 1.
5. **`tri_state` and a note.** Recommend an error for an `output` that shares a line, as written. A fourth kind for "silent" would only rename `output`.
6. **A general mark for "a member that a join leaves alone"** (`.levels: Levels own`) instead of the name test in `connect_interfaces` and `is_level`. Recommend later, with the next interface that needs it. Three names (`Logic`'s levels, `Levels`, `role`) are not yet a pattern worth a word.
7. **Roles on the ports of a board in a system.** A carrier's SPI port faces its cards as a master, and the rules do not run across a system. Recommend not now; the contacts' row kinds (`mate.rs`) already judge the signal at the mate, and a port's role is a second statement of what its pins say.
8. **`SpiMode`.** Recommend the optional enum in stage 3 and no modes for a device that has several. The alternative is the doc comment only, which is where the information is now.
9. **I2C addresses.** Recommend not now (section 4).
10. **`open_drain` as a kind.** Recommend later, as its own ADR: it would turn the I2C rule from a name test to data and would need `tri_state` first.
11. **A `Levels` that is a separate interface that a part declares** (the fallback of section 6, item 4) versus a member of each bus. Recommend the member, and the fallback only if the measurement finds notes.
12. **A generic's contract row `.spi.miso tri_state`.** Recommend yes: a provider whose table says `output` is passed over, which is the right outcome for a board that needs a shared line.

## 9. What to look at

- The two examples first: the three parts and the board, and that the board's own statements are the same as before (one statement for the clock, joins as it joined) and the part's new statements are the role, the levels and the bound.
- The ten messages, above all 1, 3 and 5: that each names the net, the lines and the parts, and says what to do; and that 5 says what to read in the datasheet.
- `lines` in `std/bus.nsm`: the whole protocol of a bus in one place, with `limit`, `crosses`, `single`, `optional` and `required`.
- `tri_state` as a kind (a seventh word) and the rule that a part writes it only with the page, and that `output` is what a silent datasheet gets.
- That a role is an enum value (ADR 0054) and is stated with `&=`, not by a new construct on the interface member.
- That `Uart ~ Uart` crosses and what that breaks (section 6, item 1), and question 2.
- That `Levels` reuses the four levels and the solver's checks of ADR 0046 with the part's own numbers, and that `~` does not join them.
- Section 3: the any-pin processor (the master is invisible until ADR 0071's `peripheral` names its bus) and where the rules do not reach (the ports of a system).
- Section 5: stage 0 and stage 1a are small and can be built before the review; stage 1 alone does not judge levels, the clock, or an any-pin master.
- `GLOSSARY.md` is not edited by this ADR. The entries it proposes: **Role** (a part's end of a bus: a variant of the bus's enum, stated with `&=` on the interface; the rules compare the table with it and count it), **Line** (a pin of a bus interface, with the kind the role gives it, and the words `limit`, `crosses`, `single`), **Bus** (the interfaces of one definition that share a net), **Tri-state** (the kind of a pin that drives only while its part is selected), a changed **Pin kind** (seven kinds), and a changed **Interface** (a bus interface has a `lines` table). Avoid for the new terms: master and slave as the names of the construct, direction, protocol, mode (for a role), high-Z.
