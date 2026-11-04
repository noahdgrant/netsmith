# System view

## How to get to it

The section "System" (key 3). A board that is part of a `system` shows the system (boards with their ports, voltages and currents, joined by `~` or by a mate), and below the graph the mated connectors with a box for each contact coloured by role (ground, power, signal, open; click the mate's name to go to its `~`), a line for each pinout that works turned end for end, and one line for each ground group (`carrier.GND + sensor.GND + serial.GND`, ADR 0040). A board on its own shows its members (ports, modules, parts) joined by the nets they share; a ground net is left out.

## Driving it

`scripts/editor-verify drive system`; `drive single-board` for the board form (launch `--fixture regulator`).

1. For `two_boards`: cards `power` (PowerBoard) with ports `input` and `output`, and `led` (LedBoard), with a dashed edge labelled `power.output ~ led.input` (derived from the mate of `power.power_out` and `led.power_in`, listed under the graph with the ground group `power.GND + led.GND`).
1a. For `carrier_and_cards` (`launch --fixture carrier_and_cards`, recipe `system-mates`): three cards, two derived joins, two mates of ten contacts each and one ground group of the three boards.
2. For the regulator example: cards `input`, `stage` (a module with ports `vin` and `vout`), `power_in`, `output`, `power_out`, with edges labelled by net name.
3. Click a card's name: the editor goes to its declaration.

## Gotchas

- A manufacturing process is an interface too but has no pins, so it is not a port or a member.
- A net with more than four members is treated as a supply and not drawn.
