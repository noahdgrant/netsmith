## Quick start

`examples/led.nsm` is an LED on a 5 V supply with a resistor that sets its current. The solver picks the resistor.

```nsm solve report file=examples/led.nsm
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board LedExample {
    /// Made by JLCPCB's two-layer process, on a board 1.6 mm thick.
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }

    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
    }

    .led: Led {
        .current &= 5mA..15mA
    }

    .r1: Resistor {}

    /// The supply comes in through this header.
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2

    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
```

```text
board LedExample
  D1   Led             KT-0603R        LED_SMD:LED_0603_1608Metric                                C2286 Basic
  J1   Header1x2       Conn_01x02      Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical hand-soldered
  R1   Resistor        330             Resistor_SMD:R_0402_1005Metric                             C25104 Basic
  net GND: D1.1, J1.2
  net VCC: J1.1, R1.1
  net net-led.anode: D1.2, R1.2
```

Read it from the top.

- `import` brings in definitions from the standard library.
- `board LedExample { ... }` defines one board.
- `.process: TwoLayer { ... }` gives the board a member named `process` of type `TwoLayer`. It says which factory makes the board. `.stackup.thickness &= 1.6mm` chooses a thickness.
- `.supply: Rail { ... }` is a member named `supply`, a `Rail`. `.voltage &= 5V +/- 5%` narrows its voltage to 4.75 V to 5.25 V. `.pos.net &= "VCC"` names the net on its positive pin.
- `.led: Led { .current &= 5mA..15mA }` is an LED that must carry 5 mA to 15 mA.
- `.r1: Resistor {}` is a resistor with nothing stated. The solver picks its value.
- `.supply.pos ~ .power_in.p1` puts two pins on one net.
- `.supply.pos ~> .r1 ~> .led ~> .supply.neg` is a series chain: positive pin, resistor, LED, negative pin. One current passes through it, and the supply's voltage is shared between the resistor and the LED, so the chain ties the LED current to the resistor the solver will pick (see Chains).

The report shows what the solver chose: a 330 ohm resistor and a red LED, both from JLCPCB's Basic library. The chain decides it. With the supply at 4.75 V to 5.25 V, a forward voltage of 1.8 V to 2.4 V and a 1% 330 ohm resistor, the current stays between 5 mA and 15 mA (about 7.05 mA to 10.56 mA, in fact). The resistor is a 0402 part: its 62.5 mW rating holds for the current it really carries (see Resistor power).

The rest of this guide explains each construct. Statement order never matters, so you may write the statements of a block in any order: the design, the schematic, the board, the bill of materials and the placement file come out the same (only the source lines that a message or a note points at move with the statement).
