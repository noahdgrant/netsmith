---
status: accepted
---

# Buses state who drives each line, at which levels and how fast

The owner accepted the words and the shapes: the status line said "The words `lines`, `role`, `limit`, `single`, `crosses` and `tri_state`, the enum for each bus's roles, the member `.levels` and the shared `.frequency` were reviewed and accepted by the owner as written". The choices made later by an agent "for review" are listed under "Left open", and are not accepted by this line. Tests 1 (words in `std/bus.nsm` that boards and parts write) and 3 (the alternatives below). Cut from `git show 8023ee9:docs/adr/0064-bus-roles-and-levels.md`. Source files cite its stages and sections: they are in that text. In `GLOSSARY.md` a **Bus** is the interfaces of one definition that share a net; the editor's drawing of an interface is ADR 0067.

## The problem

The standard buses (`Uart`, `Spi`, `I2c`, `Swd`, `Usb2`) named their lines and said nothing else. Who drives a line was only the pin kind in each part's table, so a shared MISO was accepted (rightly) and so were nearly all miswirings: two slaves' selects joined, `mosi` on `miso`, a slave's data out left open, a UART's `tx` on `tx`. A 3.3 V line onto a 1.8 V pin was not judged, because only `Logic` lines have levels. A shared MISO needs every driver high impedance when its part is not selected, and the kinds could not say that or that a datasheet is silent. The clock and the SPI mode were stated nowhere.

## Decision

- An interface may declare **`lines`**: for each role of the bus (a variant of an `enum`, ADR 0054) the kind of each line, with `limit N` on the role, `crosses` and `single` on a line, and `optional` and `required` as on a pin row. `std/bus.nsm` declares them for the five buses, so a protocol is written once in a file a reader can open.
- A part states its role with an ordinary statement, `.spi.role &= SpiRole::Slave`. Its pin table stays as it is. The checker judges the table against the role, and the nets: one line on one net, a `Uart` joined `tx` to `rx` (`~` between two `Uart`s crosses), at most N parts of a role on one bus, one select for each slave, a required line connected. All of it is read from the nets after they are made, so statement order never matters.
- A seventh pin kind, **`tri_state`**, says the pin drives only while its part is selected. A line with two or more drivers is an error unless every driver is `tri_state` or `bidirectional`. A silent datasheet writes `output`, which says exactly what is known, and a board that shares that line is refused with the question to put to the datasheet. A part writes `tri_state` only with the page in a comment (`docs/DESIGN-RULES.md`, "A part states its own requirements").
- Every bus interface but `Usb2` carries **`.levels: Levels`**, the four voltages and the reference rail of `Logic` once for all its lines; the solver's level checks (ADR 0046) judge a bus pin as they judge a `Logic` line. A level translator is a part with two bus interfaces, one on each rail. `~` between two bus interfaces joins no levels and no role.
- The clock is **`.frequency`** (`.baud` for `Uart`), one value that `~` equates along the bus and the nets show to be on one bus. Each part bounds it (`.spi.frequency <= 10MHz`), the board states what it runs, and a bound it breaks names the part. The SPI mode is an optional enum that a part with exactly one mode states.
- A part that states no role is judged as before. A processor whose pins are any-pin peripherals names its bus with the ADR 0071 clause `peripheral ... as Spi SpiRole::Master { SPIM0_SCK:output as sclk }`.

User text is in `docs/LANGUAGE.md` ("Buses", "Roles and lines of a bus", "Logic levels of a bus", "The clock of a bus"); the rules, the messages and the as-built choices are in the headers of `crates/netsmith-check/src/lines.rs` and `busnets.rs`, and the protocols in `std/bus.nsm`.

## What was rejected

- One interface for each role (`SpiMaster`, `SpiSlave`): `~` needs one definition, and a part with both roles would need two members.
- The roles in the checker by interface name: a project's own bus has none. A direction on the interface member: it states one role, and a bus with two has two directions for one line.
- A DTE and a DCE role for `Uart`, `~` that does not cross, or a `Uart` role at all: a role for a naming convention. A part's path is the line it sends on whatever the datasheet calls the pin.
- A `tri_state` that the role gives every slave: it invents a datasheet fact. A note, not an error, for a plain `output` on a shared line: the board with two drivers would build and the first person to measure the line would find the fault.
- `open_drain` or `high_impedance` in place of `tri_state`; `Logic` for every bus line (every bus pin gains `.line`, and a `Logic ~ Logic` joins reference rails); the four levels directly in each interface; a `max_frequency` for each part; a mode as a set (there is no set-valued type); `required` on `.role` or `.frequency` (it was judged for every instance and refused a part that declares a bus it does not use); the role as a field of the pin row.
- Role checks at the `~` statement: they would depend on statement order, and a pin-by-pin join has no interface statement.

## Cost

`Uart ~ Uart` crosses now, so a design that joined two UART interfaces with `~` and was right has its `tx` joined to `rx`, and the net with two outputs is reported where the module is. The fix is mechanical, and nothing in std, examples or the guide had it. The rules that need no role run for every part with a bus pin and refuse faults that were accepted before. Every bus interface gained members that are unset until stated. A `tri_state` is the part author's word; the checker cannot read a datasheet. Where the rules do not reach: the ports of a system, the address of an I2C target, timing, drive strength, QSPI, CAN, daisy chains, dual-role USB (a part that states no role). A role that is a variant with no `lines` is transparent, silently, if its author forgets them.

## Left open

Built by an agent "for review", each with its alternatives in the headers named above:

- `UsbRole::Passthrough` for a receptacle (`UsbC2`), the name, and recognising it as a variant with no `lines`; alternatives `Receptacle`, a keyword, a role `none`, asking the receptacle for a role.
- The `bus role` guideline for a joined bus whose end states no role, which stays a guideline (an error only under `--strict` or `guidelines = "error"`). Question 4: "when a joined bus without a role is an error", "not planned", the owner's to say when.
- `Levels` states `.reference.current &= 0A` (provisional, 2026-11-07); `Logic` still does not (ADR 0066, open question 3).
- `~` between two `Spi` joins `cs`, as before (question 3); "if the cost shows, stage 3 makes `~` skip a `single` line".
- `SpiMode`, optional (question 8), and `Uart` without a role.

Not built, as decided: roles on the ports of a board in a system (question 7), I2C addresses (9), `open_drain` as a kind (10: "later, as its own ADR"), and a general mark for "a member that a join leaves alone" (6: three names are not yet a pattern).
