# The schematic and the shared selection

## Get to it

Open the Schematic (key 2). Whatever is selected in any view is selected here: Go to anything (Ctrl+P) finds a part, net or module; a row of the Problems dock, a line of the BOM, a part on the PCB and a card of the Power tree or System view select too. The selection chip in the top bar (the only chip: the drawing has none) names it, Esc clears it. For a net the chip says "N pins on M sheets" (the drawing knows; `views/schematic-actions.ts`, `netFacts`) and has ‹ › buttons that run the commands `schematic.pin.previous` and `schematic.pin.next`, the same as Shift+Tab and Tab.

## How it is built

- `web/src/selection.ts` is the one selection (`part | net | module`; the key is the instance path of a part or module, the name of a net). `web/src/views/schematic-selection.ts` is the only place the schematic reads or writes it: it turns the key into the designator the drawing is keyed by (`selectedPart`), and a pick into `pickPart` / `pickNet` of the store.
- A click selects a part (the inspector opens its panel); a double-click, Enter or the inspector's "Go to source" opens the Source with the Back chip. A click on a wire, label, flag or pin selects the net, which stays lit (sticky) with the rest dimmed. A selected module marks the parts under it.
- **Reveal.** A pick that asks to be shown (`selectAndReveal`: Go to anything, the chip, a problem row) is a `revealRequest`. `Schematic.tsx` opens the sheet that draws the thing, then `revealPart` in `web/src/schematic/zoom.ts` (pure, tested in `zoom.test.ts`) decides the view: if the zoom reads (146% or more, 7 px text) and the part is on screen with 24 px to spare, it stays exactly as it is; otherwise the part is put in the middle at the zoom the view had (raised to 187% if too small to read), never above 200% and never above what "Zoom to selection" gives for the part and the parts it is wired to on its sheet (`neighbourBox` in `nav.ts`; nets with more than six pins, ground and supplies, do not count). A ring (`.pulse-ring`) is drawn round the part for about 1.7 s. A net is brought into view by the least pan (`revealBox`); a module opens its sheet and frames its parts if it is nested in a sheet. A pick made since (another selection) is not revealed late. A reveal carries the board that was in front when it was asked (`revealRequest.board`) and waits for the drawing and the board detail of that board (`revealStep` in `schematic/nav.ts`, tested in `nav.test.ts`): after a board switch the previous board's drawing is not shown while the new one is on its way (a skeleton is), and a request for a board that is no longer in front is dropped.
- The ring and the zoom do not happen for a click in the drawing: the person is looking at it.

## Drive it

`scripts/editor-verify launch --fixture schematic_bench`, then `scripts/editor-verify drive schematic-integration` (1280 x 800, then 1920 x 1080). It opens another sheet, then:

1. Go to anything finds a capacitor of the microcontroller's sheet: the sheet opens, the zoom is between 140% and 260%, the part is on screen with a `.pulse-ring`, the chip names it, the drawing marks it, the ring goes.
2. The same part again: the canvas transform does not change.
3. A part off screen (the crystal): it is in the middle of the view, the zoom is the one it was or lower (the crystal is wired to the large IC, whose neighbourhood is as big as the sheet, so it settles at 146%), never below what reads.
4. After zooming in to 2400%, a pick comes back to 200% or less with the part on screen.
5. A wire clicked: the top bar chip and the inspector name the net, the net is lit, Ctrl+J keeps the selection; there is exactly one chip in the page (`.selchip`, no `.sel-chip`), it says "N pins on M sheets", and its ‹ › buttons walk the pins (`.pin.cur` moves).
6. A flag of a net drawn on exactly one other sheet clicked: that sheet opens, the net is lit and selected there, its own flag is drawn.
7. Esc: the chip goes, nothing is marked as selected, the inspector shows the sheet, the BOM has no lit row (the part the source cursor stands on stays marked, dimmer: it is `linked`, not selected). A BOM row selects the part in the schematic. At 1920 x 1080 the pick still fits.

8. A click on a card of "All sheets": the sheet opens and the chip names the module (`.selchip.k-module`), whose parts are marked.

Look at `part-found` (ring, chip, zoom), `far-in-then-found`, `net-picked` (chip with pins and inspector agree), `flag-followed`, `card-selects-module` and `wide-found`.

`drive schematic-board-switch` (fixture `two_boards`) holds the new board's drawing back for 1.2 s, asks for one of its parts in that time and checks that the previous board's drawing is not on screen, no ring is drawn on it, and the ring and the mark are on the new drawing when it arrives; a part asked for and then the board switched is dropped.

## Gotchas

- Hover lights nets as well: take the pointer off the drawing before asserting that nothing is lit.
- Go to anything moves the source cursor quietly, so the part it declares is marked `linked` after Esc.
- The reveal waits for the drawing and the board's detail; a pick made before they are in is shown when they are.
- Only one sheet is shown: scope locators with `.sheet-slot:not([style*="none"])`.
- The chip's pin buttons walk to a pin, which can open another sheet: a recipe that goes on to look for a flag opens its sheet again.
