---
status: accepted
---

# Peripherals that share hardware, and pin rules a part states once

The owner reviewed the syntax and asked for it to be built, and the open questions were decided as recommended: the first lines of the status line, "the owner reviewed the syntax and asked for it to be built, and the open questions of section 9 were decided as recommended there". Tests 1 (`peripheral`, `shared`, `on` and `limit` are words in std parts and projects) and 3 (four candidates were weighed). Cut from `git show 8023ee9:docs/adr/0071-shared-peripherals-and-pin-rules.md`. Source files cite its stages and sections: they are in that text.

## The problem

A microcontroller's pins have rules the pin table cannot state: this peripheral and that one run on the same hardware, one at a time; this function exists only on these pins; this pin belongs to one core. The language knew four things about functions (ADR 0016, ADR 0046): the pin lists them, a connected pin with several needs the board to choose one, one function is chosen on only one pin of a part, and a function can have its own kind. A board that used two peripherals that cannot run together built without a message, and the first to notice was the person with the firmware.

## Decision

- `peripheral NAME [on CLASS] { FUNCTION[:kind] ... }` names the functions that belong together (a UART's `TX` and `RX`). A peripheral is **in use** when the board chooses one of its functions on a pin; two of its lines are still one use.
- `shared NAME [limit N] { MEMBER ... }` says that at most one member (with `limit N`, at most N) is in use at a time. A member is a peripheral, a function or a pin. A pin is in use when the board chooses its function or connects it. Using more members than the limit allows is an error that names the part, the shared resource, every use and its line.
- With `on CLASS`, every pin whose row lists the function `CLASS` (`APP_GPIO`) offers the peripheral's functions, so a part where any pin carries any peripheral line lists the class once on each row and not every function. A core that owns a pin is a class too; there is no `.owner` field, because the function the board chooses already says it.
- A module passes a function choice through its own pin with the equation the language has, `.mcu.p04.function == .scl.function`. `.function &=` on a pin that nothing reads is an error (it was accepted and ignored).
- `peripheral` and `shared` are refused in a generic, a module, an interface, a board, a system, a panel and a physical file: a peripheral is a fact of one part's datasheet. A provider of a generic may state its own. Function names stay text (ADR 0016).

A part that states no `peripheral` and no `shared` means what it meant. The words are plain names anywhere but where they start a statement or follow a name, and the editor highlights them only there. The ADR 0064 clause (`peripheral ... as Spi SpiRole::Master`) makes a peripheral the end of a bus.

What a user needs is in `docs/LANGUAGE.md` ("Peripherals that share hardware" and the sections after it); the five messages, the definition errors and the rules are in the header of `crates/netsmith-check/src/shared.rs`.

## What was carried, and what was left out

Carried: peripherals that share one unit and run one at a time, several units of one kind, a cap on how many of a kind run at once (`limit`), two pins or functions that exclude each other, a function that exists on some pins only (a row lists it), a function any pin of a class can carry (`on`), and which core owns a pin.

Left out, each for a reason: a rule of one silicon revision (the board's author can do nothing about it; it stays in the part's doc comment with the erratum's number); a rule that depends on a speed (firmware sets it and the board does not state it); what firmware sets (clocks, security attributes, drive strength); a DMA stream (software assigns it); a pair that must be used together (question 6, not built); the tool picking the pin (ADR 0016 left it open; a `peripheral` is the data that would drive it).

## What was rejected

Four candidates were compared. Relations over chosen functions (`used(UART0) + used(SPI0) <= 1`): a function is a text field of one pin, no operator looks across the pins of a part, an unset function is unknown and not 0, and the messages would name fields and not the part, the resource and the lines. A resource with a role (`SerialRole`) and a count: it says a second time what `.u.p00.function &= "UARTE0_TX"` says, and carries no pins. A `limit N of ...` statement and exclusive sets of functions: the same rule with a limit named or always 1; joined, they are `shared`. `peripheral` is needed because a member of `SERIAL1` is a UART with its lines, not one function.

Also rejected: guessing a unit from function names (`UARTE0_*`; ADR 0046 refuses to guess a kind from a name); an `id` number on a peripheral; the sharing written on each row (`fn UARTE0_TX@SERIAL0`); a `.owner` field with an enum of cores; a pool declared apart from the rows (a second name to keep in step with the table); one statement for each interface where the tool picks the pins (it needs an assignment solver, and each part of it is now data the part states, so a later ADR can build it); accepting a function on a module's pin and forwarding it by the net (the net has every pin the board joined, so the part pin meant is a guess); an interface that carries a selection (it still needs a way to turn a variant into function names); a function stated in the physical file (not physical intent, ADR 0048).

## Cost

A pin that lists only a class has several functions, because its peripherals' lines are functions of it, and needs a choice when connected. A class is also a function a board may choose (`APP_GPIO` as a plain GPIO); a part that wants a class that is not a function must invent one. A module whose processor is inside it forwards one equation for each pin it exports (the cost ADR 0004 names). A function chosen on an unconnected pin is a use.

## Left open

Decided as recommended, not built:

- A pair that must be used together (`needs`: both pins of an antenna or neither). "Recommend not now, and one construct, if a second part has one."
- A `note` on a peripheral, printed when it is used. "A revision's erratum is reachable as a doc comment, and a note a board cannot clear is noise."
- A module that exports a peripheral as a unit (an interface that carries a selection). "The equation costs one line for each exported pin and works today."
- Peripherals in generics: refused "until a generic of microcontrollers exists with two providers".
- Function names as text or as variants: "text for now (ADR 0016) with the nearest name in the message".
