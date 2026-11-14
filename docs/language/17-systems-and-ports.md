## Systems and ports

A system holds boards and says which of their ports meet. A port is an interface that a board declares directly, such as `.output: Rail`. Everything else of a board is private (ADR 0010).

```nsm solve
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::parts::regulator::LinearRegulator
import std::sources::Rail

module Stage {
    .vin: Rail
    .vout: Rail
    .regulator: LinearRegulator
    .vin ~ .regulator.vin
    .regulator.vout ~ .vout
}

/// Turns the 5 V it is given into 3.3 V for another board.
board Source {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
    }
    .stage: Stage {
        .vout.voltage &= 3.3V +/- 5%
    }
    .output: Rail {
        .pos.net &= "3V3"
        .current &= 0mA..50mA
    }
    .input ~ .stage.vin
    .stage.vout ~ .output
    .in_header: Header1x2
    .out_header: Header1x2
    .input.pos ~ .in_header.p1
    .input.neg ~ .in_header.p2
    .output.pos ~ .out_header.p1
    .output.neg ~ .out_header.p2
}

board Lamp {
    .input: Rail {
        .pos.net &= "3V3"
    }
    .led: Led { .current &= 5mA..20mA }
    .r1: Resistor
    .in_header: Header1x2
    .input.pos ~ .in_header.p1
    .input.neg ~ .in_header.p2
    // The LED is a part, so the board states what it takes from its input.
    .input.current == -.led.current
    .input.pos ~> .r1 ~> .led ~> .input.neg
}

system Demo {
    .power: Source
    .lamp: Lamp
    .power.output ~ .lamp.input
}
```

Rules.

- A system holds boards and nothing else. The same board may be placed twice, each as its own member.
- `~` in a system joins two ports of the same kind on different boards. It equates their values and balances their currents. It does not put pins on a net. A connector that carries a port is mated instead, which joins the ports it carries (below).
- A system narrows a board through the board's ports and declared values: `.power.output.voltage &= 3.3V +/- 5%`.
- A system names no nets.
- `build` and `fab` write each board of a system to `<out>/<System>/<member>/`.

Misuse.

```nsm fails
import std::sources::Rail

board A {
    .output: Rail
}

board B {
    .input: Rail
}

system S {
    .a: A
    .b: B
    .a.output.pos.net &= "X"
    .a.output ~ .b.input.pos
}
```

```text
error: `a.output.pos.net` names a net, and a system does not: each board names its own nets, and a system only says which ports meet
   --> main.nsm:14:5
   |
14 |     .a.output.pos.net &= "X"
   |     ^^^^^^^^^^^^^^^^^^^^^^^^
error: a system joins two ports of the same kind; a pin is not a port
   --> main.nsm:15:17
   |
15 |     .a.output ~ .b.input.pos
   |                 ^^^^^^^^^^^^
```

### Connectors that mate

Boards that plug into each other do it through connectors, and a system says which. `~` between the `.mating` of two connectors says they are mated, contact N to contact N (ADR 0040). Every pin header and socket of the standard library states its `.mating` (`std::parts::connector::Mating`): its `kind`, `gender`, `pitch`, `rows` and `positions`, and, when its datasheet rates it, the current of one contact, `pin_current`, and when its drawing gives it, its `seat` (below). A system can reach a connector's `.mating` and nothing else of it.

The system names no pin and no net. After the nets of the boards are known, the checker reads what each board has behind each contact, a role: ground (the net is the `neg` of a rail), power (the `pos` of a rail), a signal (a part's `input`, `output` or `bidirectional` pin is on it) or open. Then:

- The two connectors must mate: the same kind, rows, positions and pitch, and a header with a socket. A header does not mate with a header.
- Contact N of one board meets contact N of the other, with the rules a board already has on a net: two outputs are refused, an `input` with nothing that drives it on either board is refused, ground against power or a signal is a short. A power contact against a power contact equates their voltages, as `~` of rails does, and an open contact is fine against anything but an input.
- A mate that carries power or a signal must carry a ground: a contact that is ground on both boards.
- The ports the mate carries are joined by it: a port of one board whose pins are all on contacts that a port of the same kind on the other board has its pins on is joined to it, as an explicit `~` of the two would join them (values equated, currents balanced). A `~` between ports that the mate carries is refused with a message naming the mate, so a join has one spelling. A port no connector carries (a soldered lead) is still joined with `~`. If a board has two ports on the same contacts, the mate cannot say which is joined, and says so.
- Each board keeps its own ground net. The system reports the ground groups: which boards share a ground through the mated ground contacts.
- The contacts carry the current. The rated current of one contact (`pin_current`) times the contacts that carry a rail's `pos`, or the ground the rails return through, is at least the current of the rail, in either direction. A connector that is rated is picked from the `header` catalog (`data/jlcpcb/catalog/header.csv`, one row for each series and pin count from a public datasheet), so the solver chooses a row that works. A connector with no rating is not checked, and the check is listed as not evaluated.
- A pinout that also works with one connector turned end for end, and that neither connector keys (`.mating.keyed`), is a guideline finding (`mating`): the plug-in can go in the wrong way round and the checker cannot tell.

A carrier with two sockets and two plug-in cards, with power, a UART and an SPI bus over the mates. The parts are invented, so the example is checked and solved and not exported:

```nsm evaluate file=examples/carrier_and_cards.nsm
//! A carrier board with two sockets and two plug-in cards, mated in a system.
//!
//! Every board is a PCB of its own with its own `GND` and `3V3` nets. The carrier has a controller
//! and two 1x10 sockets; the sensor card plugs into one and the serial card into the other. A
//! card has a 1x10 header wired to what it takes from the carrier: power on one card and an SPI
//! bus on the other. The system mates each socket with a header, and the checker does the rest:
//! it pairs contact N with contact N, finds what each board has behind each contact, joins the
//! rails the mates carry (so there is no `~` of the ports to write), checks that a signal has
//! one driver and that every mate carries a ground, and reports which grounds are one.
//!
//! The controller and the two chips are invented parts, written here to keep the example
//! whole; they have no KiCad symbol, so this file is checked and solved but not exported.

import std::bus::{Spi, SpiRole, Uart}
import std::parts::connector::{Header1x10, Socket1x10}
import std::parts::passive::Fuse
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

/// A controller with a UART and an SPI master. Invented; not a real part.
part Controller {
    .vdd: Rail
    .uart: Uart
    .spi: Spi
    // Its table drives `sclk`, `mosi` and `cs` and reads `miso`: the master's end of the bus.
    .spi.role &= SpiRole::Master

    pins {
        .vdd.pos  power  "VDD"  "1"
        .vdd.neg  power  "VSS"  "2"
        .uart.tx  output "TX"   "3"
        .uart.rx  input  "RX"   "4"
        .spi.sclk output "SCK"  "5"
        .spi.mosi output "MOSI" "6"
        .spi.miso input  "MISO" "7"
        .spi.cs   output "CS"   "8"
    }
}

/// An SPI sensor. Invented.
part SpiSensor {
    .vdd: Rail
    .spi: Spi
    // Its table reads `sclk`, `mosi` and the select and drives `miso`: a slave's end of the bus.
    .spi.role &= SpiRole::Slave

    pins {
        .vdd.pos  power  "VDD"  "1"
        .vdd.neg  power  "GND"  "2"
        .spi.sclk input  "SCLK" "3"
        .spi.mosi input  "SDI"  "4"
        .spi.miso output "SDO"  "5"
        .spi.cs   input  "CSN"  "6"
    }
}

/// A UART transceiver. Invented.
part SerialChip {
    .vdd: Rail
    .uart: Uart

    pins {
        .vdd.pos  power  "VDD"  "1"
        .vdd.neg  power  "VSS"  "2"
        .uart.tx  output "TX"   "3"
        .uart.rx  input  "RX"   "4"
    }
}

/// The carrier: it takes 3.3 V from outside, gives it to the cards through its sockets, and
/// brings the controller's UART and SPI to them.
///
/// Both sockets have the same pinout, 1 GND, 2 3V3, 3 to 6 the SPI bus (the card is the
/// slave), 7 and 8 the UART (7 is the controller's `tx`), 9 3V3 and 10 GND.
board Carrier {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }

    /// Supplied from outside the system.
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .peak &= 3.6V
        .pos.net &= "3V3"
        .min_track &= 0.5mm..1m
    }
    /// What the carrier gives to its cards, through a fuse: up to 500 mA.
    .cards: Rail {
        .pos.net &= "3V3_CARDS"
        .min_track &= 0.5mm..1m
        .current &= 0mA..500mA
    }
    .fuse: Fuse
    .supply.pos ~> .fuse ~> .cards.pos
    // The fuse drops a little of the supply; the cards get what is left.
    .cards.voltage == .supply.voltage - .fuse.drop

    .mcu: Controller {
        .vdd.current &= 20mA..60mA
    }
    .supply ~ .mcu.vdd

    .slot_a: Socket1x10
    .slot_b: Socket1x10

    .cards.neg ~ .slot_a.p1
    .cards.pos ~ .slot_a.p2
    .mcu.spi.sclk ~ .slot_a.p3
    .mcu.spi.mosi ~ .slot_a.p4
    .mcu.spi.miso ~ .slot_a.p5
    .mcu.spi.cs ~ .slot_a.p6
    .cards.pos ~ .slot_a.p9
    .cards.neg ~ .slot_a.p10

    .cards.neg ~ .slot_b.p1
    .cards.pos ~ .slot_b.p2
    .mcu.uart.tx ~ .slot_b.p7
    .mcu.uart.rx ~ .slot_b.p8
    .cards.pos ~ .slot_b.p9
    .cards.neg ~ .slot_b.p10
}

/// A card with a sensor on SPI. Its header goes into the carrier's first socket.
board SensorCard {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }

    .supply: Rail {
        .pos.net &= "3V3_CARD"
        .current &= -30mA..-5mA
    }
    .sensor: SpiSensor
    .supply ~ .sensor.vdd

    .j1: Header1x10
    .supply.neg ~ .j1.p1
    .supply.pos ~ .j1.p2
    .sensor.spi.sclk ~ .j1.p3
    .sensor.spi.mosi ~ .j1.p4
    .sensor.spi.miso ~ .j1.p5
    .sensor.spi.cs ~ .j1.p6
    .supply.pos ~ .j1.p9
    .supply.neg ~ .j1.p10
}

/// A card with a UART chip. Its header goes into the carrier's second socket. The UART
/// crosses: the card's `tx` is on contact 8, where the controller has its `rx`.
board SerialCard {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }

    .supply: Rail {
        .pos.net &= "3V3_CARD"
        .current &= -60mA..-10mA
    }
    .chip: SerialChip
    .supply ~ .chip.vdd

    .j1: Header1x10
    .supply.neg ~ .j1.p1
    .supply.pos ~ .j1.p2
    .chip.uart.rx ~ .j1.p7
    .chip.uart.tx ~ .j1.p8
    .supply.pos ~ .j1.p9
    .supply.neg ~ .j1.p10
}

system CarrierAndCards {
    .carrier: Carrier
    .sensor: SensorCard
    .serial: SerialCard

    // The sensor card's header goes into socket A, the serial card's into socket B. A mate
    // joins contact N to contact N and derives the rails it carries, so no port is joined here.
    .carrier.slot_a.mating ~ .sensor.j1.mating
    .carrier.slot_b.mating ~ .serial.j1.mating
}
```

Mistakes in a mate. Some invented parts for the examples:

```nsm module=src::kit
import std::parts::connector::Mating

/// A part with one output pin. Invented.
part Out {
    pins {
        .y output "Y" "1"
    }
}

/// A part with one input pin. Invented.
part In {
    pins {
        .a input "A" "1"
    }
}

/// A 1x4 header at 2 mm pitch, which does not fit a 2.54 mm socket. Invented.
part Header1x4Fine {
    pins {
        .p1 passive "P1" "1"
        .p2 passive "P2" "2"
        .p3 passive "P3" "3"
        .p4 passive "P4" "4"
    }

    .mating: Mating {
        .kind &= "pin_header"
        .gender &= "header"
        .pitch &= 2mm
        .rows &= 1
        .positions &= 4
    }
}
```

A header does not go into a header:

```nsm fails
import std::parts::connector::Header1x4
import std::sources::Rail

board Carrier {
    .gnd: Rail
    .slot: Header1x4
    .gnd.neg ~ .slot.p1
}

board Card {
    .gnd: Rail
    .j1: Header1x4
    .gnd.neg ~ .j1.p1
}

system Kit {
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j1.mating
}
```

```text
error: `.carrier.slot` and `.card.j1` are both headers (Header1x4 and Header1x4); a header mates with a socket
   --> main.nsm:19:5
   |
19 |     .carrier.slot.mating ~ .card.j1.mating
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

The count and the pitch must agree:

```nsm fails
import std::parts::connector::{Header1x2, Socket1x4}
import src::kit::Header1x4Fine
import std::sources::Rail

board Carrier {
    .gnd: Rail
    .slot_a: Socket1x4
    .slot_b: Socket1x4
    .gnd.neg ~ .slot_a.p1
    .gnd.neg ~ .slot_b.p1
}

board Short {
    .gnd: Rail
    .j1: Header1x2
    .gnd.neg ~ .j1.p1
}

board Fine {
    .gnd: Rail
    .j1: Header1x4Fine
    .gnd.neg ~ .j1.p1
}

system Kit {
    .carrier: Carrier
    .short: Short
    .fine: Fine
    .carrier.slot_a.mating ~ .short.j1.mating
    .carrier.slot_b.mating ~ .fine.j1.mating
}
```

```text
error: `.carrier.slot_a` (Socket1x4) has 4 positions and `.short.j1` (Header1x2) has 2; a mate joins contact N to contact N, so both need the same number
   --> main.nsm:29:5
   |
29 |     .carrier.slot_a.mating ~ .short.j1.mating
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: `.carrier.slot_b` (Socket1x4) has a pitch of 2.54mm and `.fine.j1` (Header1x4Fine) has 2mm; they do not fit together
   --> main.nsm:30:5
   |
30 |     .carrier.slot_b.mating ~ .fine.j1.mating
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A signal needs one driver. Two outputs on one contact, or an input that nothing drives, are refused:

```nsm fails
import src::kit::{In, Out}
import std::parts::connector::{Header1x4, Socket1x4}
import std::sources::Rail

board Carrier {
    .gnd: Rail
    .tx: Out
    .rx: In
    .slot: Socket1x4
    .gnd.neg ~ .slot.p1
    .tx.y ~ .slot.p2
    .rx.a ~ .slot.p3
}

board Card {
    .gnd: Rail
    .tx: Out
    .rx: In
    .j1: Header1x4
    .gnd.neg ~ .j1.p1
    .tx.y ~ .j1.p2
    .rx.a ~ .j1.p3
}

system Kit {
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j1.mating
}
```

```text
error: contact 2 of the mated connectors is an output on both boards, and two outputs cannot share a net: `.carrier.slot` pin 2 is on net `net-slot.p2`, a signal (output `carrier.tx.y`); `.card.j1` pin 2 is on net `net-j1.p2`, a signal (output `card.tx.y`); a mate joins contact N to contact N, so if one is meant to be turned end for end, its pin 2 would meet pin 3
   --> main.nsm:28:5
   |
28 |     .carrier.slot.mating ~ .card.j1.mating
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
error: contact 3 of the mated connectors has an input and nothing that drives it on either board: `.carrier.slot` pin 3 is on net `net-rx.a`, a signal (input `carrier.rx.a`); `.card.j1` pin 3 is on net `net-j1.p3`, a signal (input `card.rx.a`); a mate joins contact N to contact N, so if one is meant to be turned end for end, its pin 3 would meet pin 2
   --> main.nsm:28:5
   |
28 |     .carrier.slot.mating ~ .card.j1.mating
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A mate that carries a signal and no ground is refused, because the two boards would share no reference:

```nsm fails
import src::kit::{In, Out}
import std::parts::connector::{Header1x4, Socket1x4}

board Carrier {
    .tx: Out
    .slot: Socket1x4
    .tx.y ~ .slot.p2
}

board Card {
    .rx: In
    .j1: Header1x4
    .rx.a ~ .j1.p2
}

system Kit {
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j1.mating
}
```

```text
error: the mate of `.carrier.slot` and `.card.j1` carries power or signals (contact 2 is a signal on `.carrier.slot` and a signal on `.card.j1`) but no ground: no contact is ground on both boards, so the two boards share no ground
   --> main.nsm:19:5
   |
19 |     .carrier.slot.mating ~ .card.j1.mating
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A port that a mate carries is not joined again with `~`:

```nsm fails
import std::parts::connector::{Header1x2, Socket1x2}
import std::sources::Rail

board Carrier {
    .out: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .current &= 0mA..100mA
    }
    .slot: Socket1x2
    .out.neg ~ .slot.p1
    .out.pos ~ .slot.p2
}

board Card {
    .supply: Rail {
        .pos.net &= "3V3"
        .current &= -50mA..-10mA
    }
    .j1: Header1x2
    .supply.neg ~ .j1.p1
    .supply.pos ~ .j1.p2
}

system Kit {
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j1.mating
    .carrier.out ~ .card.supply
}
```

```text
error: `.carrier.out` and `.card.supply` are already joined by the mate of `.carrier.slot` and `.card.j1` at contacts 1, 2; the mate derives this join, so remove this one (a port join is for ports that no connector carries)
   --> main.nsm:29:5
   |
29 |     .carrier.out ~ .card.supply
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A system reaches a connector's `.mating` and nothing else of it:

```nsm fails
import std::parts::connector::{Header1x2, Socket1x2}
import std::sources::Rail

board Carrier {
    .slot: Socket1x2
}

board Card {
    .supply: Rail
    .j1: Header1x2
}

system Kit {
    .carrier: Carrier
    .card: Card
    .carrier.slot.p1 ~ .card.supply.pos
}
```

```text
error: `p1` is inside `slot`, a `Socket1x2`; a system can reach only the `.mating` of a connector
   --> main.nsm:16:19
   |
16 |     .carrier.slot.p1 ~ .card.supply.pos
   |                   ^^
```

A pinout that reads the same from both ends is insertable the wrong way round, and the two rails here would swap. The finding is a guideline (ADR 0036): `warning:` on the command line, an error with `--strict`. A keyed connector (`.slot: Socket1x4 { .mating.keyed &= "polarised" }`) goes in one way and is not reported:

```nsm solve warns
import std::parts::connector::{Header1x4, Socket1x4}
import std::sources::Rail

board Carrier {
    .a: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "RAIL_A"
        .min_track &= 0.5mm..1m
    }
    .b: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "RAIL_B"
        .min_track &= 0.5mm..1m
    }
    .slot: Socket1x4
    .a.neg ~ .slot.p1
    .a.pos ~ .slot.p2
    .b.pos ~ .slot.p3
    .b.neg ~ .slot.p4
}

board Card {
    .a: Rail {
        .pos.net &= "RAIL_A"
        .min_track &= 0.5mm..1m
    }
    .b: Rail {
        .pos.net &= "RAIL_B"
        .min_track &= 0.5mm..1m
    }
    .j1: Header1x4
    .a.neg ~ .j1.p1
    .a.pos ~ .j1.p2
    .b.pos ~ .j1.p3
    .b.neg ~ .j1.p4
}

system Kit {
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j1.mating
}
```

```text
Kit/carrier: mating guideline: the pinout of the mate of `.carrier.slot` and `.card.j1` also works with one connector turned end for end, and neither is keyed: contact 2 (RAIL_A on `.carrier.slot`, RAIL_A on `.card.j1`) and contact 3 (RAIL_B on `.carrier.slot`, RAIL_B on `.card.j1`) would swap places and every contact would still be accepted; only a keyed connector (`.mating.keyed`) goes in one way
```

#### The seat and the mated height

A connector states its seat, `.mating.seat`: how far above the face of its board its partner rests when the two are mated (ADR 0110). A header's seat is the top of its plastic, a socket's the top of its body. It is a fact of the part, whatever it mates with, so a mate's mated height, the distance between the faces of the two boards, is the two seats added, with both tolerances. The std 2.54 mm headers and sockets state it through their `header` catalog row, from the maker's drawing the row's `source` names: 2.50 +/- 0.20 mm for a header and 8.50 +/- 0.15 mm for a socket, so the header of `examples/two_boards.nsm` in its socket gives a mated height of 10.65 mm to 11.35 mm. The column is `optional`: a project's `header` row may leave it empty. The system scene places the boards by the mated height; a mate whose seat nothing states has none, and is never given a default or one from a part's `.height`, which is the tallest point of the part and not where its partner rests.

A project's own connector states its seat as its drawing gives it:

```nsm
import std::parts::connector::{Connector, Mating, Socket1x2}

/// A low-profile 2-pin header of a project's own. Its maker's drawing gives the plastic
/// 1.50 +/- 0.10 mm above the board.
part LowHeader provides Connector {
    pins {
        .p1 passive "P1" "1"
        .p2 passive "P2" "2"
    }
    .mating: Mating {
        .kind &= "pin_header"
        .gender &= "header"
        .pitch &= 2.54mm
        .rows &= 1
        .positions &= 2
        .seat &= 1.5mm +/- 0.1mm
    }
}

board Card {
    .j1: LowHeader
}

board Carrier {
    .slot: Socket1x2
}

system Kit {
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j1.mating
}
```

A seat is at or above the face of its board, and one below it is refused with its number:

```nsm fails
import std::parts::connector::{Connector, Mating, Socket1x2}

part SunkHeader provides Connector {
    pins {
        .p1 passive "P1" "1"
        .p2 passive "P2" "2"
    }
    .mating: Mating {
        .kind &= "pin_header"
        .gender &= "header"
        .pitch &= 2.54mm
        .rows &= 1
        .positions &= 2
        .seat &= -1mm
    }
}

board Card {
    .j1: SunkHeader
}

board Carrier {
    .slot: Socket1x2
}

system Kit {
    .carrier: Carrier
    .card: Card
    .carrier.slot.mating ~ .card.j1.mating
}
```

```text
error: Kit: `card.j1.mating.seat` is -1mm, and a connector's partner does not rest below its board's face
```
