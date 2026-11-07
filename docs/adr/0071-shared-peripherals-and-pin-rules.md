---
status: implemented, all stages (stage 0 on 2026-11-05, stages 1 to 3 on 2026-11-06; the owner reviewed the syntax and asked for it to be built, and the open questions of section 9 were decided as recommended there; section 11 says what was built and where it differs from the text above)
---

# Peripherals that share hardware, and pin rules a part states once

## Summary

A microcontroller's pins have rules that the pin table cannot state today: this peripheral and that one run on the same hardware, one at a time; this function exists only on these pins; this pin belongs to one core. The language knows four things about a pin's functions (ADR 0016, ADR 0046): the pin lists them (`fn GPIO UART1_TX`), a connected pin with several needs the board to choose one, a function can be chosen on only one pin of a part, and a function can have its own kind. A board that uses two peripherals which cannot run together builds without a message, and the first person to notice is the one with the firmware.

The decision, in five sentences:

- `peripheral NAME { FUNCTION ... }` names the functions that belong together (a UART's `TX` and `RX`). A peripheral is **in use** when the board chooses one of its functions on a pin.
- `shared NAME { MEMBER ... }` says that at most one member (with `limit N`, at most N) is in use at a time. A member is a peripheral, a function or a pin. Using two members of one `shared` is an error that names the part, the shared resource, both uses and their lines.
- `peripheral NAME on CLASS { ... }` offers the peripheral's functions on every pin whose row lists the function `CLASS` (`GPIO`), so a part where any pin can carry any peripheral line does not list every pin times every function. A core that owns a pin is a class too (`APP_GPIO`, `NET_GPIO`).
- A module passes a function choice through its own pin with the equation the language already has, `.mcu.p04.function == .scl.function`, and `.function &=` on a pin that nothing reads is an error from now on (it is accepted and ignored today).
- Rules that depend on a silicon revision, on a speed or on what firmware sets stay in the part's doc comments. Section 3 says why and lists what else is left out.

Nothing here changes a part that states no `peripheral` and no `shared`: every board and every pin table written before this ADR means what it meant.

## The syntax at a glance

Everything below is invented. The part, in `src/zmcu.nsm`:

```nsm
import std::sources::Rail

/// Zmcu, an invented microcontroller: two cores, four serial units and a high-speed SPI
/// on dedicated pins.
part Zmcu {
    .vdd: Rail
    .vdd.voltage >= 1.7V
    .vdd.voltage <= 3.6V
    .vdd.current &= 1mA..25mA

    // A serial unit runs as a UART, an SPI master or an I2C master, one at a time. Their
    // lines go on any pin that the application core can own.
    peripheral UARTE0 on APP_GPIO { UARTE0_TX:output UARTE0_RX:input }
    peripheral SPIM0  on APP_GPIO { SPIM0_SCK:output SPIM0_MOSI:output SPIM0_MISO:input }
    peripheral TWIM0  on APP_GPIO { TWIM0_SCL:bidirectional TWIM0_SDA:bidirectional }
    peripheral UARTE1 on APP_GPIO { UARTE1_TX:output UARTE1_RX:input }
    peripheral SPIM1  on APP_GPIO { SPIM1_SCK:output SPIM1_MOSI:output SPIM1_MISO:input }
    peripheral TWIM1  on APP_GPIO { TWIM1_SCL:bidirectional TWIM1_SDA:bidirectional }
    peripheral UARTE2 on APP_GPIO { UARTE2_TX:output UARTE2_RX:input }
    peripheral SPIM2  on APP_GPIO { SPIM2_SCK:output SPIM2_MOSI:output SPIM2_MISO:input }
    peripheral TWIM2  on APP_GPIO { TWIM2_SCL:bidirectional TWIM2_SDA:bidirectional }
    peripheral UARTE3 on APP_GPIO { UARTE3_TX:output UARTE3_RX:input }
    peripheral SPIM3  on APP_GPIO { SPIM3_SCK:output SPIM3_MOSI:output SPIM3_MISO:input }
    peripheral TWIM3  on APP_GPIO { TWIM3_SCL:bidirectional TWIM3_SDA:bidirectional }

    // The network core has one UART, on pins that core owns.
    peripheral NETUART on NET_GPIO { NETUART_TX:output NETUART_RX:input }

    // The high-speed SPI has no `on`: it is offered only on the pins whose rows list it.
    peripheral HSSPI { HSSPI_SCK:output HSSPI_MOSI:output HSSPI_MISO:input HSSPI_CSN:output }

    // Four serial units, each one peripheral at a time.
    shared SERIAL0 { UARTE0 SPIM0 TWIM0 }
    shared SERIAL1 { UARTE1 SPIM1 TWIM1 }
    shared SERIAL2 { UARTE2 SPIM2 TWIM2 }
    shared SERIAL3 { UARTE3 SPIM3 TWIM3 }
    // Only two of the four UARTs run at once.
    shared UARTS limit 2 { UARTE0 UARTE1 UARTE2 UARTE3 }
    // Two pins that cannot both be used.
    shared REFERENCE { .p10 .p11 }

    pins "QFN-24" {
        .vdd.pos power         "VDD"   "1" "13"
        .vdd.neg power         "VSS"   "7" "25"
        .nrst    input         "NRST"  "2"
        .p00     bidirectional "P0.00" "3"  fn APP_GPIO NET_GPIO
        .p01     bidirectional "P0.01" "4"  fn APP_GPIO NET_GPIO
        .p02     bidirectional "P0.02" "5"  fn APP_GPIO NET_GPIO
        .p03     bidirectional "P0.03" "6"  fn APP_GPIO NET_GPIO
        .p04     bidirectional "P0.04" "8"  fn APP_GPIO NET_GPIO
        .p05     bidirectional "P0.05" "9"  fn APP_GPIO NET_GPIO
        .p06     bidirectional "P0.06" "10" fn APP_GPIO NET_GPIO
        .p07     bidirectional "P0.07" "11" fn APP_GPIO NET_GPIO
        .p08     bidirectional "P0.08" "14" fn APP_GPIO NET_GPIO HSSPI_SCK
        .p09     bidirectional "P0.09" "15" fn APP_GPIO NET_GPIO HSSPI_MOSI
        .p10     bidirectional "P0.10" "16" fn APP_GPIO NET_GPIO HSSPI_MISO
        .p11     bidirectional "P0.11" "18" fn APP_GPIO NET_GPIO HSSPI_CSN
    }

    .package: Package
    .mpn: String
    catalog zmcu by .mpn { .mpn, .package }
}
```

What the part writes: one line for each peripheral, one for each shared resource, and the pin table it already had, one row for each pin. No row grows with the number of peripherals: a pin that any peripheral line may use lists `APP_GPIO` once. The pins of the high-speed SPI list its functions because they are the only pins that offer them.

A board that uses five serial interfaces on the four units. A console on `UARTE0`, a display on `SPIM1`, a sensor on `TWIM2`, an EEPROM on `TWIM3`, and a second console that the author put on `UARTE1`, not seeing that unit 1 already runs the display:

```nsm fails
import src::zmcu::Zmcu
import std::parts::connector::Header1x10
import std::sources::Rail

board Sensors {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u: Zmcu { .mpn &= "ZMCU-A" }
    .j: Header1x10
    .supply ~ .u.vdd
    .u.nrst ~ .supply.pos

    // console on UARTE0
    .u.p00 ~ .j.p1
    .u.p01 ~ .j.p2
    .u.p00.function &= "UARTE0_TX"
    .u.p01.function &= "UARTE0_RX"
    // display on SPIM1
    .u.p02 ~ .j.p3
    .u.p03 ~ .j.p4
    .u.p02.function &= "SPIM1_SCK"
    .u.p03.function &= "SPIM1_MOSI"
    // sensor on TWIM2
    .u.p04 ~ .j.p5
    .u.p05 ~ .j.p6
    .u.p04.function &= "TWIM2_SCL"
    .u.p05.function &= "TWIM2_SDA"
    // EEPROM on TWIM3
    .u.p06 ~ .j.p7
    .u.p07 ~ .j.p8
    .u.p06.function &= "TWIM3_SCL"
    .u.p07.function &= "TWIM3_SDA"
    // a second console on UARTE1
    .u.p08 ~ .j.p9
    .u.p09 ~ .j.p10
    .u.p08.function &= "UARTE1_TX"
    .u.p09.function &= "UARTE1_RX"
}
```

The board states each interface as it does today: a net and a function for each pin. Without lines 36 to 40 it builds.

### The five messages

Each is one line in the output (wrapped here), names the part by its path and type, the shared resource or the rule, both uses with the function and the line, and ends with what the author can do. The span points at the statement of the use that goes over the limit, the second in path order.

**1. Two members of one shared resource (the five interfaces on four units):**

```text
error: `u` (`Zmcu`) uses SPIM1 and UARTE1, which share `SERIAL1` and run one at a time: SPIM1
on `u.p02` as SPIM1_SCK (main.nsm:24) and on `u.p03` as SPIM1_MOSI (main.nsm:25), UARTE1 on
`u.p08` as UARTE1_TX (main.nsm:39) and on `u.p09` as UARTE1_RX (main.nsm:40); the other shared
resources of `Zmcu` are in use as well, `SERIAL0` by UARTE0, `SERIAL2` by TWIM2 and `SERIAL3`
by TWIM3, so none is free
  --> main.nsm:39:5
   |
39 |     .u.p08.function &= "UARTE1_TX"
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

The last clause lists the shared resources that have a member in use and, when one has none, says which are free (`SERIAL2 has nothing in use`). It is what tells the author that the fifth interface has nowhere to go and a free unit is not an option.

**2. More members in use than a limit allows.** The board puts a log-only transmitter on each of `UARTE0`, `UARTE1` and `UARTE3` (lines 20 to 22 of that board):

```text
error: `u` (`Zmcu`) uses 3 of the peripherals of `UARTS`, which allows 2 at a time: UARTE0 on
`u.p00` as UARTE0_TX (main.nsm:20), UARTE1 on `u.p02` as UARTE1_TX (main.nsm:21), UARTE3 on
`u.p04` as UARTE3_TX (main.nsm:22); `UARTS` lists UARTE0, UARTE1, UARTE2 and UARTE3
  --> main.nsm:22:5
   |
22 |     .u.p04.function &= "UARTE3_TX"
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

**3. Two pins that cannot both be used.** A pin is in use when the board chooses its function or connects it (lines 19 and 20 of a board that joins `.u.p10` and `.u.p11` to the header and chooses `APP_GPIO` for both):

```text
error: `u.p10` and `u.p11` of `Zmcu` are both in use, but they share `REFERENCE`, which
allows 1 at a time: `u.p10` as APP_GPIO (main.nsm:19) and `u.p11` as APP_GPIO (main.nsm:20)
  --> main.nsm:20:5
   |
20 |     .u.p11.function &= "APP_GPIO"
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

**4. A function on a pin that does not offer it** (a dedicated pin). This message exists today ("`u.pa2` cannot be `I2C1_SDA`; its functions are ...") and gets two changes, because a pin that takes the lines of many peripherals has too many functions to list, and because a function that exists on other pins should say where (line 18 of a board that chooses `HSSPI_SCK` on `.u.p02`):

```text
error: `u.p02` cannot be `HSSPI_SCK`; that function is offered only on `u.p08`; `u.p02` offers
APP_GPIO, NET_GPIO and the lines of the peripherals on those pins: UARTE0, SPIM0, TWIM0, UARTE1,
SPIM1, TWIM1, UARTE2, SPIM2, TWIM2, UARTE3, SPIM3, TWIM3, NETUART
  --> main.nsm:18:5
   |
18 |     .u.p02.function &= "HSSPI_SCK"
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

A name that no pin offers is a slip, and the message names the nearest function: ``error: `u.p00` cannot be `UARTE0_TXD`; did you mean `UARTE0_TX`? ...``, the form ADR 0054 gives a variant.

**5. A function choice that a module does not pass on** (today's silent no-op). The module `Hub` of section 5 exports two pins, and the board states a function on one of them:

```text
error: `hub.scl.function` is stated, but `hub.scl` is a pin of module `Hub` and nothing passes
it to a pin of a part, so it has no effect; inside `Hub`, write `.mcu.p04.function == .scl.function`
  --> main.nsm:16:5
   |
16 |     .hub.scl.function &= "TWIM0_SCL"
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

Three more are for the part's author, at the line of the statement that is wrong:

```text
error: `SPIM9` in `shared SERIAL1` is not a peripheral, a function or a pin of `Zmcu`; did you mean `SPIM1`? the peripherals are UARTE0, SPIM0, TWIM0, UARTE1, SPIM1, ...
error: `peripheral UARTE0 on GPIO`: no row of any `pins` table of `Zmcu` lists the function `GPIO`, so UARTE0 is offered on no pin; the functions that rows list are APP_GPIO, NET_GPIO, HSSPI_SCK, HSSPI_MOSI, HSSPI_MISO and HSSPI_CSN
error: `.p08` lists `HSSPI_SCK:output`, a function of `peripheral HSSPI`, which gives it its kind; write `HSSPI_SCK` without a kind
```

## 1. What the part model declares today

The pin table (`pins` block, ADR 0016) is one row for each pin: path, kind, datasheet name, pads, `fn` list, `optional` or `required`. A function is a text; the board's `.function &= "UART1_TX"` narrows the text field every pin has (`add_pin` in `crates/netsmith-check/src/checker.rs`, `FieldKind::Text`). The checker knows the rows (`PinRow.functions`, `Instance.tables`) and reads the choice with `Model::function_of` (`pinout.rs`). Four rules in `faults.rs`, all about a single pin or a single function: the function must be one the row lists (`function`), a connected pin with several needs a choice (same function), one function is chosen on one pin (`functions_used_twice`), and a function with its own kind changes how the net rules see the pin (ADR 0046).

Not stated anywhere: that two functions of different pins cannot be used together, that a pin belongs to one core, and a short way to say that almost every pin offers almost every function (a row for each pin would list hundreds of names). That a function exists on some pins only is stated, by the rows that list it.

## 2. The constructs

### `peripheral`

`peripheral NAME [on CLASS] { FUNCTION[:kind] ... }`, a statement of a part, among its `pins`, `tie` and `catalog`. The functions are separated by spaces or line ends, each with an optional kind as in a pin row (ADR 0046). The name is the datasheet's name for the thing.

- A peripheral is **in use** when one of its functions is the chosen function of a pin of the part. Using two of its lines is still one use.
- With `on CLASS`, every pin whose row (in any table) lists the function `CLASS` offers the peripheral's functions as well. The pin table is not changed, and a pin in a package that lacks it simply has no row. `CLASS` must be a function that some row lists, or the part is refused (message above). A class is an ordinary function: `APP_GPIO` is also what a board chooses for a pin that is a plain GPIO of the application core.
- Without `on`, a function is offered only on the pins whose rows list it by name, and each must be listed by at least one row. That is a dedicated pin.
- A function belongs to one peripheral, and a row may not list a function that its peripheral already offers on every pin of a class. A function a peripheral names takes its kind from the peripheral; a row that writes a kind for it is refused. A function that no peripheral names is as before.
- A peripheral name and a function name are in one namespace: a part that uses a name for both is refused.

### `shared`

`shared NAME [limit N] { MEMBER ... }`. A member is a peripheral name, a function name, or a pin (`.p10`). At most N members (default 1, a whole number of at least 1 and fewer than the number of members) are in use at once. A member is in use as follows: a peripheral as above, a function when it is chosen on a pin, a pin when the board chooses its function or connects it (the net it is on has a pin that is not the part's own, or the board names the net, `Model::is_connected`).

- The rule is judged for each instance of the part, in the board pass of `pin_faults`, before the solve: the choices are statements of the text, so no catalog row or package is needed, and a part whose package the solver has not chosen is judged all the same. A part inside a module is judged with its full path.
- A peripheral, function or pin may be a member of several shared resources (`UARTE0` is in `SERIAL0` and in `UARTS`). A name twice in one shared resource is refused, and so is a function that is also a member through its peripheral.
- Order of statements does not matter: a `shared` may come before the peripherals and the rows it names.

### What "the choices" are

`function_of(pin)` returns the pin's own stated function, or else the function a forwarding equation gives it (section 5). The rule counts uses by it and by nothing else. A function chosen on a pin the board leaves unconnected is a use: the statement is deliberate.

## 3. What is carried and what is left out

**Carried.**

- Peripherals that share one piece of hardware and run one at a time, including several units of the same kind (`shared`).
- A cap on how many of a kind run at once (`limit`), which is the form "you can have two UARTs if the third's timer is free" takes: the timer's use is a member of the same resource.
- Two pins, two functions or a pin and a function that exclude each other (`shared` with those members).
- A function that exists only on some pins (a row lists it), and a function that any pin of a class can carry (`on`). The same two shapes cover a part with a fast fixed pin path and a slower any-pin path: the fast one is a peripheral with no `on` whose rows list it, the slow one a peripheral `on` the class, and a `shared` of the two keeps one unit from using both routes.
- Which core owns a pin, as a class: every pin lists the cores that may own it (`fn APP_GPIO NET_GPIO`), a peripheral of a core is `on` that core's class, and the function the board chooses names the core by its name. A pin the silicon gives to one core lists one class. There is no `.owner` field: a second statement on each pin would say what the function already says (one correct way).
- Parts of other families that have the same shape: a part whose pins list many functions and need one chosen is what ADR 0016 already does; a part with programmable I/O units that reach any pin is `on` a class; pairs of pins that share an analogue path, a clock or a reference are `shared` pins.

**Left out, with the reason.**

- **Silicon revision.** A rule that holds only for one revision (an I2C clock too fast on one build code) is a property of one catalog row, the board's author can do nothing about it by choosing pins, and the workaround is software. A rule the author cannot clear is noise, so these stay in the part's doc comments with the erratum's number, where the editor shows them (the part's `docs`). Section 9, question 7, asks whether a one-line `note` on a peripheral that the checker prints when it is used is worth a construct.
- **A speed.** A rule that depends on the rate the interface runs at (the dedicated pins are needed above a rate, any pin serves below it) depends on a number the board does not state: firmware sets it. The design carries what the board's choice shows: a high-speed SPI is its own peripheral on its dedicated pins, and choosing it says "high speed". A rule that reads a rate waits for an interface that carries the board's rate (a bus with a `.frequency`), which does not exist.
- **What firmware sets**: which core runs at what clock, a regulator's setting against a core's frequency, the security attribute of a peripheral, a drive strength a dedicated pin needs. None is a pin choice. They are doc comments.
- **A DMA stream** that a request may use one of several of: it is an assignment problem that software solves, and no pin depends on it.
- **A pair that must be used together** (both antenna pins or neither): left open (section 9, question 6).
- **The tool picking the pin** (connect by function): ADR 0016 left it open and this does not close it. A `peripheral` is the data that would drive it.

## 4. Four candidates, and why this one

| | Carries the shared unit | Count limit | Pins | "Any pin" | Names part, resource and both uses | New constructs |
| --- | --- | --- | --- | --- | --- | --- |
| **A. Exclusive sets of functions** | yes | no (a limit of one) | with pin members | no | yes | one |
| **B. A resource with a role (`SerialRole`) and a count** | yes | yes | no | no | yes | a resource kind, a role field |
| **C. A `limit N of ...` statement** | yes | yes | with pin members | no | only with a name | one |
| **D. Relations over the chosen functions** | no | no | no | no | no | an expression form |
| **This ADR: `peripheral` plus `shared ... limit N`** | yes | yes | yes | with `on` | yes | two |

**D, relations.** Can `used(UART0) + used(SPI0) <= 1` be written today? No, and the reason is in the checker, not in syntax. A `.function` is a text field of one pin (`FieldKind::Text`, domain `Domain::Text`); a relation holds `RExpr` of `Const`, `Text`, `Field`, `Neg` and `Bin`, and `Bin` of a text is the type error `EvalError::Type` (`netsmith-solve/src/eval.rs`). `==` on two texts is accepted (`lower.rs`) and compares them, and a text field narrows from another by it. There is no way to turn "some pin of this part has the function F" into a number, because no operator looks across the pins of a part, and an unset function (a pin with several that is not connected) is "unknown" to the solver and not 0. Relations also run after the check, in the solver, and say "relation does not hold" with the fields' paths, not the part, the resource and the lines. D would need a new aggregate over pins, a three-valued 0/1, and its own messages, for a rule a datasheet states as a list.

**B, a role.** `.u.serial0.role &= SerialRole::Uart` states what `.u.p00.function &= "UARTE0_TX"` already says, so the board says one fact twice (ADR 0001), unless the role replaces the functions, and it cannot: a UART's `TX` is not an SPI's `SCK`, so the function still names the role. A role pays when a tool picks the unit for the board (assignment, not built) or when a message wants to suggest another unit of the same role. Neither is needed to carry the rule, and B does not carry pins or any pin.

**A and C** are the same rule: a list of members and a limit, in one the limit is always 1 and in the other it is named by a statement. The shared hardware of the datasheet has a name (`SERIAL0`) and a list of peripherals, which is what `shared NAME { ... }` is; the name is what a message prints. Joined, they carry A, C, the pins and the count with one construct and one clause (`limit`).

`peripheral` is needed because a member of `SERIAL1` is a UART with its lines, not one function: `UARTE1_TX` and `SPIM1_SCK` conflict, and `UARTE1_TX` and `UARTE1_RX` do not.

## 5. Modules

A module hides the parts inside it (ADR 0004), so a board cannot write `.hub.mcu.p04.function &= ...`: it fails with "`mcu` is inside `Hub` and is not part of its interface". A module exports a pin as `.scl: Pin` and joins it to the part's pin. `.function &=` on that exported pin is accepted and nothing reads it: `add_pin` gives every pin a `function` field, only a row of a part's pin table is judged, and the part's own pin still reports that it has several functions.

**The silent no-op is a bug, to fix now.** A statement that is accepted and has no effect is the failure the checker exists to prevent, and it sends the reader to the wrong place. The fix is the error of message 5, for every pin that has a stated function, is no row of a part's table, and is read by no forwarding equation. It needs no syntax decision and does not wait for review. The hint names the part pins on the same net inside the module when there is exactly one with functions, and otherwise says `.<part>.<pin>.function == .scl.function`.

**How a module exports a function choice: an equation.** A module that wants its board to choose the function on an exported pin forwards it with the form the language has for a value that another provides:

```nsm
import src::zmcu::Zmcu
import std::sources::Rail

/// A Zmcu with its supply, and two pins for the board to use.
module Hub {
    .supply: Rail
    .scl: Pin
    .sda: Pin
    .mcu: Zmcu { .mpn &= "ZMCU-A" }
    .supply ~ .mcu.vdd
    .mcu.nrst ~ .supply.pos
    .scl ~ .mcu.p04
    .sda ~ .mcu.p05
    .mcu.p04.function == .scl.function
    .mcu.p05.function == .sda.function
}
```

The part's pin takes the function that the exported pin is given, as any field takes the value of the right side of an `==` (the solver's `propagate`). The board states `.hub.scl.function &= "TWIM0_SCL"`, and the checks of `Zmcu` apply to `hub.mcu.p04` as if the board had written it: the function must be on the pin, the shared resources count it, the net rules read its kind. Two changes in the checker make that so before the solve:

- `Model::function_of` follows an `==` whose left side is the pin's `function` and whose right side is another `function` field, to a depth of a few (a module in a module). Today it reads the pin's own field only. The lowering (`netsmith-lower` reads `function_of`) then shows the function on the schematic and in the exports.
- A message about a function names the board's statement and the path the board knows. A connected pin that is not chosen and is forwarded says `choose one with `.hub.scl.function &= "APP_GPIO"``, the module's pin, which the board can write, in place of `.mcu.p04.function`, which it cannot. The error for a function the pin does not offer points at the statement.

Both sides stated (the module says `.mcu.p04.function &= "TWIM0_SCL"` and the board a different one) is left to the solver's relation check, "relation does not hold" (the same as for any two values that disagree).

A module that always uses the same peripheral writes `.mcu.p04.function &= "TWIM0_SCL"` inside itself, which works today, and exports nothing. A module whose processor is inside it and whose board chooses many pins forwards one equation for each pin it exports; this is the cost ADR 0004 names.

**Rejected here.** The checker joining the function by the net: the exported pin and a part pin are on one net, but the net has every other pin the board joined to it, so "the part pin it means" is a guess. An interface that carries a selection (`interface Uart { .tx: Pin  .rx: Pin  .peripheral: SerialRole }`): it needs a way to turn an enum variant into function names, and `SerialRole` would still be a second statement of what the functions say; left for when a module needs to pick a peripheral as a unit (question 8). Letting a board's physical file state a function: it is not physical intent (ADR 0048).

## 6. Generics and `required`

A generic (ADR 0068) states what every provider has: pins with kinds, values, relations. A provider's pins, names, pads and functions are its own, and a use of the generic reaches only the generic's members, so a board that uses a generic MCU cannot state `.function &=` on a pin: the generic has none to name. Consequences:

- `peripheral` and `shared` are refused in a `generic` ("a generic states what every provider has; a peripheral is a fact of one part's datasheet"), in a module, an interface, a board, a system, a panel and a physical file. The providers' `Item` rewriting (`providers.rs`) would copy them into every provider, which is wrong for pins.
- What a generic can say about capability is a number the provider states (`.serial_units >= 3` in the generic's own members, `&= 4` in the provider), with the existing quantities. A board that needs functions on pins writes the part as the type (`.u: Zmcu`), which is the escape hatch of ADR 0068 already.
- `required` (ADR 0069, removed by ADR 0065: now only a pin row's) marks a pin that must be connected, and marked values too while ADR 0069 stood. It is not a word of a peripheral, and a `shared` resource is not a value, so the two do not meet. A pin row's `required` and the rule that a connected pin with several functions needs a choice both stay.
- A provider of a generic may state its own `peripheral` and `shared`: they are the part's, in its own body, and are judged on the instance the solver picks like the rest of its checks (a trial that fails the shared rule is passed over with the message as its reason, ADR 0068 section 5).

## 7. Staged plan

Counts are from reading the code on this branch. A "test" is a Rust or TypeScript test; a "guide example" is a fenced block of `docs/LANGUAGE.md`, which `crates/netsmith/tests/guide.rs` runs.

**Stage 0: module pins (no new syntax; can be built before the review).** The error of message 5, `function_of` following a forwarding equation, and the board's path in the hint of the existing "several functions" message. Files: `crates/netsmith-check/src/pinout.rs`, `faults.rs` (and `model.rs` if the relation lookup wants a helper). Tests: 5 in `crates/netsmith-check/tests/functions.rs` (a forwarded function is read; the no-op is an error at the board's line; two modules deep; a wrong forwarded function points at the board's line; the hint names the module's pin). Guide: one subsection with 3 examples under Functions. Docs: the `netsmith-write-a-module` skill, 2 lines. About 5 files and 130 lines.

**Stage 1: `peripheral` (no `on`) and `shared` (limit 1, peripheral members).** Delivers message 1 and the three definition errors for those two constructs. It catches the five interfaces on four units: each unit is a `shared` of its peripherals, and the part's table lists the functions by pin as it does today (a part with few pins, or a pin table written with the lines of the units, works from this stage; only the pin table of a part with any-pin peripherals is long until stage 2).

- `netsmith-lang`: `ast.rs` (two `Item`s and their structs), `parser.rs` (two contextual keywords after the `bridge` and `tie` ones, two functions, the `only_decls` list), `printer.rs` (two arms). Tests: 7 in `crates/netsmith-lang/tests/pins.rs` (parse, print round trip, the errors of the parser).
- `netsmith-check`: `crates/netsmith-check/src/shared.rs` (new: declaration, the three definition errors, the rule and message 1, about 350 lines), `model.rs` (the two records on `Instance`; the struct literal is written in three places in `checker.rs`), `checker.rs` (the arms of `process`, the `not_physical` arms, refusal outside a part), `faults.rs` (`part()` calls the rule), `providers.rs` (refuse in a generic), `lib.rs`. Tests: about 10 in a new `crates/netsmith-check/tests/shared.rs`.
- `netsmith` (the binary crate): `src/editor/definition.rs`, two arms for go-to-definition.
- Editor page: `crates/netsmith-editor/web/src/nsm.ts` (the two keywords), `nsm.test.ts` (1 test), and the bundle in `web-dist` rebuilt by `scripts/web-bundle update`.
- Docs: `docs/LANGUAGE.md` (a subsection under Functions with 6 guide examples), `GLOSSARY.md` (Peripheral, Shared resource; Function amended), the `netsmith-write-nsm` and `netsmith-read-errors` skills, this ADR's status.

Totals: 3 Rust crates, the editor page, about 11 source files, 3 generated, 3 test files, about 5 documents; about 18 tests and 6 guide examples.

**Stage 2: `on CLASS`.** The offered functions of a row are the row's own and those of the peripherals `on` a class it lists; `PinRow` cannot hold them, because a `shared` or a `peripheral` may come after the table, so `Model::offered(part, row)` computes them. Callers that read `row.functions` change: `faults::function`, `PinRow::kind_with` (4 call sites, in `faults.rs` and `pinout.rs`), `function_kind_note`. The messages of 4 and of the class. Tests: about 8. Guide: 3 examples. 4 files and 150 lines.

**Stage 3: `limit N`, and function and pin members.** The clause in the parser, the count in the rule, message 2, and message 3. Tests: about 8. Guide: 3 examples. 3 files.

Stages 1 and 2 are independent of stage 0. Stage 1 alone makes the part honest about hardware sharing; stage 2 makes a real chip's table short; stage 3 adds the counts and the pin rules.

### Stage 0 as built (ADR 0063)

Built as described, and nothing of stages 1 to 3: the error of message 5 (in `faults.rs`, the pass `pin_faults` through `stray_functions`), `Model::function_of` following `.mcu.p04.function == .scl.function` to a depth of six (`pinout.rs`), the board's statement as the place of a message about a forwarded function, and the module's pin as the path of the hint of "several functions". Six tests in `crates/netsmith-check/tests/functions.rs` (the forwarded function is read and counts on the net; the no-op is an error at the board's line, with one part pin and with several; two modules deep, and the break in the chain; a wrong forwarded function points at the board's line; the hint names the module's pin; an alias with no row). The guide has one subsection under Functions with one passing and one failing block. Differences from the text above: the part's own pin still reports that it has several functions when nothing passes the choice on (both errors are printed), and the same error covers a stated function on a pin of a part that its table has no row for (an alias).

## 8. Rejected alternatives

- **Guessing the member of a shared unit from function names** (`UARTE0_*`): ADR 0046 refuses to guess a kind from a name, and the same reasoning applies.
- **An `id` number on a peripheral** (the datasheet's "peripheral ID"; same ID, same unit): carries the shared unit with one number and no list, but only that, and a second spelling of what `shared` states for every other rule. One construct that carries all of them is smaller than two.
- **The sharing written on each row** (`fn UARTE0_TX@SERIAL0`): repeats the rule for every pin that offers the function.
- **A `.owner` field on a pin, or on a peripheral, with an enum of cores**: a second statement for each pin that the function it chooses already makes. The class carries the core.
- **A pool declared apart from the rows** (`pool Any { ... }`, rows list `Any`): the class is the function every row already lists, and a separate name is one more thing to keep in step with the table. The cost is that a class is also a function a board may choose (`APP_GPIO` as a plain GPIO), which is usually true.
- **A relation form (`used(...)`)**: section 4.
- **Making a rule of an erratum or a speed**: section 3.
- **Accepting a function on a module's pin and forwarding it by the net**: section 5.
- **One statement for each interface** (`.u.uarte0 { .tx ~ .j.p1  .rx ~ .j.p2 }`, the tool picks the pins): needs an assignment solver that knows ownership, dedicated pins and the shared resources, and the placer's wire length; each of them is now data the part states, so a later ADR can build it without changing these constructs.

## 9. Open questions

Each has a recommendation.

1. **The words.** `peripheral`, `shared`, `on` and `limit`. Recommend them: they are the datasheet's words, they are read at the start of a statement or after a name (so a member called `shared` is still fine, because a member has a leading dot), and they avoid the terms the glossary retires (group, block, role, mode). The alternatives are `exclusive` for `shared` and `resource` for `shared`.
2. **The class is a function name.** `on APP_GPIO`, where `APP_GPIO` is a function the rows list. Recommend yes. The alternative is a declared pool, rejected above. The risk is a part that wants a class which is not a function and must invent one.
3. **A pin is in use when its function is chosen or it is connected.** Recommend yes: "if you use pin X you can't use pin Y" is about the copper. The alternative, a function only, misses a pin used as a plain input.
4. **A function chosen on an unconnected pin is a use.** Recommend yes.
5. **`limit`, function members and pin members: stage 3 or stage 1?** Recommend stage 3: the sharing of units is known to be needed, the counts are a guess about which parts need them.
6. **A pair that must be used together** (`needs`: both pins of an antenna or neither). Recommend not now, and one construct, if a second part has one.
7. **A `note` on a peripheral** (`peripheral TWIM0 ... note "erratum 47: ..."`), printed as a note when the peripheral is used. Recommend not now. A revision's erratum is reachable as a doc comment, and a note a board cannot clear is noise; a note that names the use and says what firmware must do is more useful and is a feature of its own (a message kind that is never an error).
8. **A module that exports a peripheral as a unit** (an interface that carries a selection). Recommend not now; the equation costs one line for each exported pin and works today.
9. **The no-op as an error now.** Recommend yes, and building stage 0 at once. It changes only the boards that wrote a statement that did nothing.
10. **Peripherals in generics.** Recommend refused, as in section 6, until a generic of microcontrollers exists with two providers.
11. **The list of free shared resources in message 1.** Recommend keeping it as written (it turns "there is a conflict" into "there is no free unit"). It would list every free resource of the part, which for a part with many unrelated shared resources is noise; list only those with a member that has the same number of functions if that shows.
12. **The function names, text or variants.** Recommend text for now (ADR 0016) with the nearest name in the message; an enum for each peripheral would give typed names and a long import for a part with a hundred of them.

## 10. What to look at

- The two statements and their words: `peripheral NAME [on CLASS] { FUNCTION[:kind] ... }` and `shared NAME [limit N] { MEMBER ... }`.
- That `on CLASS` is what keeps the pin table one row per pin, and that the class is a function.
- That the owner of a pin is carried as a class, with no `.owner` field.
- The five messages, above all the first: that it names the part, the resource, both uses and their lines, and says whether another unit is free.
- That a function choice goes through a module by the equation `.mcu.p04.function == .scl.function`, and that `.function &=` on a pin nothing reads is an error now.
- Section 3: what is carried and what is left out, above all the revision, speed and firmware rules, which stay in doc comments.
- Section 4: why relations cannot carry this, and why a role does not.
- The stage counts, and that stage 0 can be built first.
- `GLOSSARY.md` is not edited by this ADR. The entries it proposes: **Peripheral** (the functions of one part that belong together and are in use together; in use when one is chosen on a pin), **Shared resource** (`shared`: the hardware that its members use at most N of at once), and a changed **Function** (a function may be listed by a peripheral, and a class makes every pin that lists it offer the peripheral's functions). Avoid for the new terms: group, block, role, mode, mux, conflict set.

## 11. As built (stages 1 to 3)

Built as described in sections 2 and 7: `peripheral NAME [on CLASS] { FUNCTION[:kind] ... }` and `shared NAME [limit N] { MEMBER ... }`, a member being a peripheral or a function by name, or a pin (`.p10`). The checker is `crates/netsmith-check/src/shared.rs` (the definition errors when a part's body is done, so statement order never matters; the rule `overuse`, called from the board pass of `faults.rs` for every part, whatever its package), `Model::offers`, `class_peripherals` and `offered_count` in `pinout.rs` (what a pin offers is computed when asked, because a `peripheral` may follow the table), and the parser, AST and printer in `netsmith-lang`. Tests: `netsmith-lang/tests/peripherals.rs` (16), `netsmith-check/tests/shared.rs` (20, stage 1), `classes.rs` (14, stage 2), `limits.rs` (13, stage 3); the guide has the examples of each stage.

Open questions, as decided: 1 the words as written; 2 the class is a function; 3 and 4 a pin is in use when its function is chosen or it is connected, and a function chosen on an unconnected pin is a use; 5 `limit`, function and pin members are stages 2 and 3 as planned; 6, 7 and 8 not built; 9 stage 0; 10 a generic refuses both statements (`a generic states what every provider has; a peripheral is a fact of one part's datasheet, so `peripheral` goes in a part that provides it`) and a provider does not receive them; 11 the clause of message 1 is kept, see below; 12 function names stay text.

Differences from the text above:

- **Message 3's example is impossible.** It chooses `APP_GPIO` on both `u.p10` and `u.p11`, and the rule that one function is chosen on one pin of a part (ADR 0016) already refuses that, with its own message. The test and the guide choose different functions (`APP_GPIO` on one pin, a peripheral's line on the other); the message is otherwise as written. A pin connected and with no function chosen reads `u.q0 connected to probe.a12`, and points at the part's declaration, because a board's `~` has no statement the checker keeps.
- **The clause about other shared resources (question 11).** It lists only the other resources whose members are of the same kind (all peripherals, all functions, all pins) and that have the same limit, so `UARTS` and `REFERENCE` of the example are not listed with the `SERIAL` units. Members are listed in the path order of their first use, so the example's message names `UARTE0` before `SPIM1` when `u.p00` comes first.
- **Messages.** The nearest name for a member picks the first of equally near ones (`SPIM9` offers `SPIM0`, not `SPIM1`); the list of names is cut after six with `...`. A pin that is connected and lists only a class has several functions, because its peripherals' lines are functions of it, and needs a choice (`... has several functions (APP_GPIO and the lines of the peripherals on those pins) and is connected`). A part with no `peripheral` keeps every message it had, word for word.
- **Added refusals** (section 2 states them in prose): a peripheral declared twice, a shared resource declared twice, a function listed twice in a peripheral, a limit that is not fewer than the members (`limit 1` is allowed and is the default spelled out), and a member that is a function of a peripheral which is also a member of the same resource.
- **`limit`** is a plain word after the name of a shared resource and `on` after the name of a peripheral; both are plain names anywhere else, and the editor page highlights them only there.
- **Not built, as the ADR says:** a `note` on a peripheral, a pair that must be used together, a module that exports a peripheral as a unit.
- **The bus clause** (`peripheral NAME on CLASS as Spi SpiRole::Master { F:kind as line }`) is ADR 0064, stage 4: it makes a peripheral the end of a bus for the rules of that ADR.
