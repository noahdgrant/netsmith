---
status: accepted
---

# A peripheral that is an end of a bus joins as a port

Decided by the agent while the owner was away and accepted by the orchestrating agent under the owner's delegation (2026-11-13); the owner will review it on return. For issue #635 (epic #638): the example boards' processors join their UART, I2C and quad SPI as buses, so that the rules of ADR 0064 read them. Meets the first test (it is syntax a board writes) and the third (it rejects an interface member for each peripheral, which a part author would propose).

A processor's bus end is a peripheral, `peripheral USART1 as Uart { USART1_TX:output as tx USART1_RX:input as rx }` (ADR 0064, ADR 0071), not an interface, because the board decides which pin carries each line (`.mcu.pa9.function &= USART1_TX`). Until now the board then joined each pin by hand (`.mcu.pa9 ~ .bridge.rxd`), crossing a UART's lines itself, and the bus was a bus only for the rules that read the nets.

## Decision

- `~` between a peripheral that is an end of a bus and an interface of that bus, or another such peripheral, joins the two as `~` joins two interfaces of one kind: line by line, with the bus's crossings. The peripheral is named by its part's path and its name, `.mcu.USART1 ~ .bridge.uart`, as a pin request names a function (`.u.UARTE1_TX`, ADR 0101).
- The line of a peripheral is the pin on which the board chose its function. The join does not choose functions. A function offered on several pins (`USART1_TX` on `PA9` or `PB6`) would have no answer, and ADR 0101 says the source or the pin file names the pin.
- The pins are joined after every statement is read and every pin request has its pin, before the nets are made. Statement order never matters, and the nets and their names are the ones the pin-by-pin joins made.
- A line whose function no pin has chosen is an error at the join. The message names the pins that offer it. The exception is a line that the role's `lines` makes `optional` (a master's `cs`): the line joins nothing, so a select that reaches the slave through a link stays the board's to join. A line that only the interface has (a `Uart`'s `rts` and `cts` against a peripheral of `tx` and `rx`) joins nothing, like a line a part's table leaves out.
- The other side must be an interface of the peripheral's bus, or a peripheral of that bus. A pin, a part, an interface of another kind and a peripheral that names no bus are refused at the join. A peripheral is not a member, so a chain, `~>`, a rename map or `except` with one is refused too, by a message that says how it joins.
- A peripheral has no values, so nothing is equated. The bus's clock is still equated along the nets (`equate_bus_values`).

The rules are in the header of `crates/netsmith-check/src/periph_join.rs`, the messages in `crates/netsmith-check/tests/peripheral_ports.rs`, and the user text in `docs/LANGUAGE.md`, "A peripheral that is an end of a bus".

## Rejected

- An interface member of the part for each peripheral (`.usart1: Uart`, its pins aliases of `PA9` and `PA10`). It states the bus twice, once as functions and once as pins. It cannot say that `PA9` is a UART line only when the board chooses it, and an any-pin peripheral has no pins to alias.
- A keyword, such as `.mcu peripheral USART1 ~ ...`. It would be a second way to name something a part has.
- A join that also chooses the functions when exactly one pin offers each. Writing the function would then be a second spelling of the same choice, and the join would mean different things for two parts that differ only in how many pins offer a function.
- A strict join that refuses every unchosen line, `optional` ones included. A board whose select goes through a part could not use the join at all (the RP2350A Minimal's flash select goes through a 0 ohm link).

## Cost

A board writes the function of each line and then the join, so a peripheral on dedicated pins still costs one statement per line. The RP2350A Minimal's five QSPI pins became five function statements and one join. A reader finds which pin carries a line by reading the function statements, not the join. A peripheral is a path in a join and nowhere else: `.mcu.USART1.baud` is refused, and a board states the clock on the other end's interface.

## Left open

- Figures of a peripheral (`.capacitance`, `.levels`, a bound on the clock). Without them, ADR 0102's sum for an I2C bus with a peripheral end is not made, and the solver says so.
- A request for the end of a bus (`.dbg: .u.Uart`, ADR 0101), which would pick the pins and could then be joined the same way.
