# Bill of materials

## How to get to it

The section "BOM" (key 5). One line for each distinct part, as `bom.csv` groups them; parts soldered by hand are below, under their own heading.

## How it looks

A header (title, the board, an info popover with the note about prices; at the right the `Check parts` button with `Preview only` and `Offline` switches and, when some are old, a pill `N lines read more than 90 days ago`), four summary tiles (parts, unique values, cost per board, problems), a filter, a `Problems` toggle, `Flat | By value | By module` (kept in the browser), a `Columns` menu (kept in the browser), then the table: a sticky header, rows of 34 px, designators as chips (`C1–C3` for a run of three or more; a click on a chip selects that part, a click on the row the line), `Qty`, `Value`, `Package`, `LCSC`, `Library`, `Sourcing`, `Each`, `Stock` and `Checked`, the numbers right-aligned. The sourcing chip says what the page can: `Found` (a number with a price that is not old), `Stale`, `Not checked` (a number but no provider has a price: unknown is not found), `Ambiguous` (a generic part no single row stands for; the tooltip has the nearest rows), `Missing` (no number), `By hand`. Grouped by module, a line that spans two modules is split in two, each with its own designators. `web/src/bomview.ts` has these rules, tested in `bomview.test.ts`.

## Driving it

`scripts/editor-verify drive bom`

1. For `TwoBoards/led`: `D1` (`KT-0603R`, `C2286`, Basic) and `R1` (`100`, `C25076`, Basic), with price and stock `unknown` (the standard catalogs ship none and the fixture has no `[supply]` table; ADR 0047), not marked stale; `J1` under "Soldered by hand".
2. The header has no total (no line is priced) and the note says where prices come from (a project's own catalog files; the standard ones ship none) and that the feeder fee is not included.
3. Click a row: the editor goes to the first designator's declaration.
4. Each line's price has the day it was read under it (`unknown` if nobody has it; its tooltip says where it comes from and which stock it is); a line older than 90 days has it in amber with a dot before its designators, and a chip over the table counts them (ADR 0039). "Check parts" (its accessible name and its tooltip are "Check parts against LCSC"; with "Preview only" and "Offline") runs `catalog refresh` over the project's own catalog files: it verifies each row's manufacturer's number and package against LCSC and writes only the day in a `checked` column, never a price, stock or library. The result lists the rows that are a CONFLICT, MISSING or SKIPPED or carry a note (for example "LCSC lists stock 0 where the row has stock 42"), counts the verified ones, and says in words under it that JLCPCB data is not fetched, what is never written, and that LCSC's terms of use are unread (the button's tooltip says the same); with the standard rows only, it says the project lists no catalogs.

Refresh needs `scripts/editor-verify launch --fixture catalog_refresh` (the LED example, a project catalog with an invented part and a stand-in for LCSC's endpoint on a local port, which answers 404 and logs `UNEXPECTED REQUEST` in the server log to anything but `GET /lcsc`) and `drive refresh`: it asserts the stale mark, a preview that writes nothing, the stock note, the MISSING row, and that after the real run the BOM still shows the hand-entered price and stock and is still stale, and screenshots each. Never point it at the real endpoints.

## Gotchas

- Price and stock are recorded data, not live. The solver never needs the network. Only "Check parts against LCSC" does, and only for the project's own catalog files.
- A part with no LCSC number shows `none`; `fab` refuses to package it.
- The table scrolls sideways in a narrow window or pane (`.bom-scroll` inside `.bom-frame`): the first column (Designators) stays under a soft edge once it has moved, the right edge fades into the surface while there is more to scroll to (`useScrollEdges`, `components/scrollEdges.ts`), and the rows scroll under a sticky header.

## Tables sized to their content, and the help column (round 8)

A table's frame (the BOM's `.bom-frame`, Compare's list `.cmp-scroll`) is as tall as its content: the header, the rows and a counting foot (`.bom-foot`, `.cmp-foot`, which says how many lines or rows are shown, and of how many when a filter is on) right under the last row, never an empty box below it (round 8 had made them fill the page and four rows sat above a hollow frame; the round 8 fix reverted that). Only when the content is taller than the pane does the frame stop at the pane's height and scroll inside, with a sticky header and the foot kept at the frame's end; the tiles and tabs above stay put (they are outside the scrolling box). The recipes assert exactly that with `expectFrameFits` (`flow-helpers.mjs`): short, the foot is at most 2 px under the last row and ends the frame; tall, the frame stays inside the page and the foot is its end. On a pane with room (`ViewHelp`: the pane is at least the page's width plus 2 x 18.5 rem, so 1872 px at 100 % and a pane of a split never) a column `What does this view show?` stands in the right margin, a landmark `About this view`, folded with one click (kept per view in `netsmith.viewhelp.v1`); on a pane without the room it is not in the page. Compare, BOM and Build have one. `drive view-help` (and the steps of `compare`).
