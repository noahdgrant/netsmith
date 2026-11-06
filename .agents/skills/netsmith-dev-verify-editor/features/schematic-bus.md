# Busses in the schematic

## Get to it

Open the Schematic (key 2) on a board whose modules share an interface, such as the invented `bus_bench` (a processor `brain`, a modem `link`, a header `probe` and two chips on an SPI among the board's own parts). An interface instance whose members run between two main parts, or leave a module from one, is drawn as one bus (ADR 0049): a thick line, a slash with the count, the instance's name and the list of members, and at a module's edge one flag for the whole bus.

## How it is built

- The server's SVG has a `g.bus-group` for each bus with `data-bus` (the instance's path, `brain.uart0`), `data-type` (`Uart`), `data-nets` and `data-members` (JSON, `[["tx","MODEM_TX"],...]`, in the order of the pins). `web/src/schematic/drawing.ts` reads them into `index.buses` (with the sheet and the box of the line); `schematic/load.ts` groups the `g.buses` by module like the wires. The drawing's own style (`svg.rs`) lights a bus with `.on`; the page maps its colours from the theme's `--sch-*`.
- **Selection.** A bus is not a kind of the shared selection (`part | net | module`): picking one selects its first member net (the PCB and the BOM light that net's parts) and `views/schematic-selection.ts` remembers the bus (`selectedBus`) while the selection is still that net. Any other pick, Esc, or a click on a net drops it. The Schematic lights all the members and the line; the chip says `bus uart0 (4 nets)`; the inspector (`BusPanel`) lists the instance, the interface, the members with their nets as links and each net's pins by module, and where else the nets go.
- **Bundle flag.** A click on a flag (`schematic/bus.ts`, `busTargets`) goes to the module that carries the same member nets, matched on the nets because the two ends name the interface apart (`uart0` and `serial`): a module carries the bus when it has pins of at least two member nets (all of them for a bus of two). One module: it opens with the bus drawn there picked (else the first shared net); several: the menu of a net's flag.
- **Go to anything.** `#` lists the drawn buses before the nets (the mixed list has a Buses group of up to four); a choice picks the bus and the Schematic frames its line (`goToBus`).

## Drive it

`scripts/editor-verify launch --fixture bus_bench`, then `scripts/editor-verify drive schematic-bus` (1920 x 1080; the inspector needs the width):

1. The `brain` module: `uart0` is one line with the count 4, the writing `uart0` and `{tx, rx, rts, cts}`, one flag and no flag for a single member.
2. The pointer on the line lights the four nets and the line, and the inspector shows the bus ("hover"); nothing is selected.
3. A click picks it: the chip is the bus kind and says `bus uart0 (4 nets)`, all members lit, the rest dimmed, the inspector lists the members `tx=MODEM_TX ...` with pins grouped under `brain`, `link` and `probe`. The BOM lights the parts on the first net.
4. A net link in the inspector picks that net alone (the bus panel goes for the net's).
5. The flag of `uart0`: a menu `uart0 also on` with `link` and `probe` (4 of 4 nets each). `link` opens with `bus serial (4 nets)` picked and the same four nets lit; its own flag offers `brain` and `probe`.
6. The SPI between the board's two chips: a trunk, no flag, no "also on".
7. Go to anything, `#uart`: the Buses group first, with `uart0`, `serial` and `tap`; choosing one opens its module, picks it and lights it.

Look at `brain`, `picked`, `flag-menu`, `followed`, `palette` and `found`.

## Gotchas

- Headings of the inspector and the menu are upper-cased by CSS: match their text without regard to case.
- Hover lights the nets under the pointer; the pointer is where the palette's row was when the window closes, so move it off the drawing before asserting that nothing is lit.
- `bus_bench` has a mapping of its own (`fixtures/bus_bench.kicad.toml` and `bus_bench.libs/`, copied by `launch`), so its PCB view places the parts. A fixture's `x.kicad.toml` is the scratch project's `kicad.toml` and `x.libs/` its `libs/`. The bill of materials shows the selection too.
- The bus's line is a path with a wide invisible `.hit` twin: aim the pointer at a point on the path (`getPointAtLength`), not at the middle of its box.
- The fixture's invented symbols (`bus_bench.libs/bus.kicad_sym`) keep all the signal pins of a part on one side (`Radio` and `Flash` have them on the left). A bus needs three or more members on the same side of both parts (ADR 0049, rule 2): with the `Radio`'s outputs on the right and its inputs on the left, the `link` module drew four loose flags and no bus, and the recipe's step that follows the flag waited 30 s for `bus serial`.
