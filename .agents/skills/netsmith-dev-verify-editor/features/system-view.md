# System view

## How to get to it

The section "System" (key 4). It is drawn like the power tree (`power-tree.md`): the layout engine in `src/flow/layout.ts` is shared, so the same rules hold. A board that is part of a `system` shows the system (boards with their ports, voltages and currents, joined by `~` or by a mate), and below the graph the mated connectors with a box for each contact coloured by role (ground, power, signal, open; click the mate's name to go to its `~`), a line for each pinout that works turned end for end, and one line for each ground group (`carrier.GND + sensor.GND + serial.GND`, ADR 0040). A board on its own shows its members (ports, modules, parts) joined by the nets they share; a ground net is left out.

## Driving it

`scripts/editor-verify drive system`; `drive single-board` for the board form (launch `--fixture regulator`).

1. For `two_boards`: cards `power` (PowerBoard) with ports `input` and `output`, and `led` (LedBoard), with a dashed edge labelled `power.output ~ led.input` (derived from the mate of `power.power_out` and `led.power_in`, listed under the graph with the ground group `power.GND + led.GND`).
1a. For `carrier_and_cards` (`launch --fixture carrier_and_cards`, recipe `system-mates`): three cards, two derived joins, two mates of ten contacts each and one ground group of the three boards.
2. For the regulator example: cards `input`, `stage` (a module with ports `vin` and `vout`), `power_in`, `output`, `power_out`, with edges labelled by net name.
3. Click a card's name: the editor goes to its declaration.

### The redesigned view: `drive system-view` after `launch --fixture system_view`

The fixture (`scripts/editor-verify-js/fixtures/system_view.nsm`, invented) has 13 modules: an input connector, a surge guard, three regulators (a buck, a linear regulator, a boost), a processor with 22 ports, a cell modem with 10, a flash chip, a motion sensor, an ADC, a GNSS receiver, a debug header and a lamp driver. It solves; it cannot be written to KiCad (the parts have no pads).

- **Left to right:** power flows from what supplies a net to what is supplied (who supplies a net comes from `/api/power`, so a board's wire runs from the supplier; for a system, from the board port that others draw from). A signal wire runs from the member with fewer connections to the one with more, so the hub (the processor) is on the right. `src/system/model.ts` does this; `src/views/System.tsx` draws it.
- **Cards** (`src/system/metrics.ts`): a title (the name is a link to the source), the type, and a ports table in two columns: the name and a value (the nominal voltage and the range of the current). A row is one line: a name or value that does not fit ends in an ellipsis and has a tooltip with the editor's own words. A table of more than six ports is folded into one line "22 ports" that opens on a click ("List all ports" opens every one).
- **Wires:** one wire for all the nets between two members. One net is labelled with its name; more are labelled "10 nets", and a click or a hover on the label lists them. The label stands on the wire's first run, with a halo; the layout keeps three grid steps between the ends of labelled wires on a card and widens the gap for the longest label, so no two labels touch and none lies on a card. A power wire has an arrowhead; a signal wire has none.
- **Selecting:** a click on a card lights what feeds it (orange) and what it feeds (blue), dims the rest, and shows the same in the minimap; the first module lights its whole downstream, a leaf its upstream, a card in the middle both. Escape clears. Arrow keys move along the drawing.

The recipe asserts: 13 cards; 17 wires (one for each pair); power left to right; the processor rightmost; no stretch of a wire meets a card it does not join, none is shorter than 6 px, every end is on the 8 px grid; a label on each wire and none over another or over a card; every port row 20 px tall and inside its card; no text under 12 px; "22 ports" and "10 ports" fold and open; "10 nets" lists ten; the tints and their minimap; a lit wire keeps its route; the name goes to `.mcu: Processor` and Back returns.

## Gotchas

- A manufacturing process is an interface too but has no pins, so it is not a port or a member.
- A net with more than four members is treated as a supply and not drawn.
