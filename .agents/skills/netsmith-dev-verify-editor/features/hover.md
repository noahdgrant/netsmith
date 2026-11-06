# Shared hover

One transient hover for the whole page (`src/hover.ts`, the one subscriber in `src/hoverDom.ts`). The pointer or the keyboard focus on a part, a net, a module or a bus in one view lights the same thing, softly, in every other view that shows it. It is not a pick: the colour is the neutral `--hover-lit` (never the blue of `--sel`), nothing is selected, and Escape or leaving puts it out.

## Get there

Any view. The things that say what they stand for: a row of the bill of materials, a pin or net chip of the schematic's or the board's inspector, a row of the board's part and net lists, a card of the power tree and the system view, an entry of the module navigator, a definition in the project tree, a row of the problems dock that names a part, a module or a net, a part or a net drawn in the schematic, a part, a track, a via or a pad drawn on the board.

## Drive

`scripts/editor-verify launch` (`two_boards`), then `scripts/editor-verify drive hover`. The recipe puts the board on the left and another view on the right (both have to be on show) and checks:

1. A BOM row: the part is lit on the board and its row in the board's list, and only that part. The drawing is not replaced (a mark on its `svg` survives and no node is added or removed).
2. A part of the schematic: lit on the board, its BOM row lit. Escape puts it all out.
3. A net chip of the board's inspector: the net is lit in the schematic and on the board (`[data-net]`), no part is.
4. A card of the power tree: its net is lit on the board; a part row of the board's list lights its card.
5. Keyboard focus on a BOM row does the same as the pointer; a touch (`pointerType: 'touch'`) sets nothing.

What a view does about it: a list or a card gets `.hover-lit` (a wash and a 1 px outline); the board and the schematic get a short glow through `filter`. The elements are found by `data-hv` (`part:<path>`, `net:<name>`, `module:<path>`), `data-hv-ref` (designators, space separated) and, in the drawings, `data-ref` and `data-net`.

## Gotchas

- It lights after a 70 ms rest and goes out 90 ms after the pointer left, so a recipe waits a quarter of a second before it looks.
- The System view's cards are modules and parts of the board; in `two_boards` the cards are its two boards, which stand for no part, so that view is not in the recipe (`carrier_and_cards` has module cards).
- A section that is not on show keeps its DOM, but a view that draws only when shown (the board) does not: put it in a pane.
- `hover-lit` is also in the page while a recipe's mouse rests on something: a recipe that counts `.hover-lit` moves the mouse away first.
- The subscriber puts back what a view that is drawn again took off (a card whose `class` changed), and writes only the difference (`domMarks.ts`), as the comparison's marks do (`compare.md`, _Beside the rest_).

Round 6: **Escape closes any open card**, the one the pointer opened too, whether the editor has the focus or not (`source/cm-card.tsx`: a listener on the document, before the page's own; the card carries `data-esc-closes`, so `Back` and `Clear the selection` step aside for it). In the schematic a hovered part wears a halo (`schematic.md`), not a glow.
