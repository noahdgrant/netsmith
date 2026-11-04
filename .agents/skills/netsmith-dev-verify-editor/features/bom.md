# Bill of materials

## How to get to it

The section "BOM" (key 4). One line for each distinct part, as `bom.csv` groups them; parts soldered by hand are below, under their own heading.

## Driving it

`scripts/editor-verify drive bom`

1. For `TwoBoards/led`: `D1` (`KT-0603R`, `C2286`, Basic) and `R1` (`100`, `C25076`, Basic), with price and stock `unknown` (the standard catalogs ship none and the fixture has no `[supply]` table; ADR 0047), not marked stale; `J1` under "Soldered by hand".
2. The header has no total (no line is priced) and the note says where prices come from (a project's own catalog files; the standard ones ship none) and that the feeder fee is not included.
3. Click a row: the editor goes to the first designator's declaration.
4. Each line's price has the day it was read under it (`unknown` if nobody has it; its tooltip says where it comes from and which stock it is); a line older than 90 days has it in amber with a dot before its designators, and a chip over the table counts them (ADR 0039). "Check parts against LCSC" (with "preview only" and "offline") runs `catalog refresh` over the project's own catalog files: it verifies each row's manufacturer's number and package against LCSC and writes only the day in a `checked` column, never a price, stock or library. The result lists the rows that are a CONFLICT, MISSING or SKIPPED or carry a note (for example "LCSC lists stock 0 where the row has stock 42"), counts the verified ones, and says in words under it that JLCPCB data is not fetched, what is never written, and that LCSC's terms of use are unread (the button's tooltip says the same); with the standard rows only, it says the project lists no catalogs.

Refresh needs `scripts/editor-verify launch --fixture catalog_refresh` (the LED example, a project catalog with an invented part and a stand-in for LCSC's endpoint on a local port, which answers 404 and logs `UNEXPECTED REQUEST` in the server log to anything but `GET /lcsc`) and `drive refresh`: it asserts the stale mark, a preview that writes nothing, the stock note, the MISSING row, and that after the real run the BOM still shows the hand-entered price and stock and is still stale, and screenshots each. Never point it at the real endpoints.

## Gotchas

- Price and stock are recorded data, not live. The solver never needs the network. Only "Check parts against LCSC" does, and only for the project's own catalog files.
- A part with no LCSC number shows `none`; `fab` refuses to package it.
- The table scrolls sideways in a narrow pane.
