# Glossary: F

**Fabrication package**:
The files a board house needs: Gerbers and drill files, a bill of materials, and a placement file. netsmith writes them itself from the board file (ADR 0034).

**Fault current**:
The `.limit_current` of a `Fuse` (its rating: the most it lets through) and the `.supply_fault_current` of a part behind it in a `~>` chain (a `Diode` or a `Schottky`), which the checker gives it as the `.limit_current` of the nearest part in front of it that has one (ADR 0091), so a diode is rated for what the fuse can pass before it opens, and not only for the load's draw (its `.current`).
_Avoid_: Trip current, surge current

**Feedback divider**:
The two resistors that set the output of an adjustable regulator, `std::parts::regulator::FeedbackDivider`: the output is `vref * (1 + rtop / rbottom)` and the solver picks resistors that keep it inside the range the board asks for, with the tolerance of the reference and of both resistors.
_Avoid_: Voltage divider (that is any divider; this one sets a regulator's output)

**Fictional fabricator**:
A fabricator std names that does not exist, ACME (`std::mfg::acme`), whose 8- and 10-layer processes have numbers chosen, not sourced (ADR 0112). `fab` makes a board on one into a package that says it is not for ordering, and `fab --order` refuses it.
_Avoid_: Mock fab, dummy process, generic fabricator

**Flag**:
How the editor's schematic shows a net that goes on to another module: a pentagon pointing at the end of the short stub of a pin, like KiCad's hierarchical label, with the net's name in it (none for a net with a generated name). A net that goes on has a flag at the end of the stub of each group of its pins in the module (a main part and the small parts that serve its pins).
_Avoid_: Port (a `Rail`'s and a module's ports are pins), off-sheet label, sheet

**Footprint**:
The copper pads and outline a part has on a board, named by the CAD tool's library.

**Function**:
One of the things a pin can be, listed with `fn` in its pinout row (`fn GPIO UART1_TX`). A connected pin with more than one function needs the board to choose, `.u.pa2.function &= UART1_TX`; there is no default, and one function can be chosen on only one pin of a part, except a **class** (next paragraph), which any number of pins can be. A function may carry its own kind after a colon (`fn GPIO:bidirectional UART1_TX:output`, ADR 0046); the net rules judge a pin by the kind of the function chosen, and the pin's kind is the default for any function without one.
A function may be listed by a peripheral instead of a row, and a **class** is a function that rows list (`fn APP_GPIO NET_GPIO`) and peripherals are `on`: the function a board chooses for a pin that a core owns names the core (ADR 0071).
A module's own pin (`.scl: Pin`) passes a choice on to a pin of a part inside with an equation, `.mcu.p04.function == .scl.function`; `.function &=` on a module's pin that no such equation reads is an error, because it has no effect (ADR 0071, stage 0).
_Avoid_: Alternate function, mux, mode
