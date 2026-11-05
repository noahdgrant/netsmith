# Themes, type, buttons, keyboard and motion

## Get to it

Five choices, kept per browser (`localStorage`, key `netsmith.theme`, a JSON string): **System** (follows `prefers-color-scheme`, and `prefers-contrast: more` for the high contrast pair), **Light**, **Dark**, **High contrast light**, **High contrast dark**.

- Status bar, right side: the half-filled circle opens a menu of the five (Arrow keys, Enter).
- Command palette (`Ctrl+Shift+P`): `Theme: Light`, `Theme: Dark`, `Theme: System`, `Theme: High contrast light`, `Theme: High contrast dark`, `Theme: Toggle light and dark`.
- First paint: `index.html` has a small inline script (`#theme-boot`) that sets `<html data-theme>` before anything is drawn. `src/theme.test.ts` runs that script against `resolveTheme` for every stored value and every system setting.

## How it is built

- `web/src/themes.css` holds every colour of the page as a token, one block per theme (`:root` alone is dark). Names are semantic: `--surface`, `--surface-raised`, `--canvas`, `--border*`, `--text*`, `--accent*`, `--danger*`, `--warning*`, `--success`, `--info*`, `--hover*`, `--pop*`, `--shadow-*`, `--syn-*` (the source editor's colouring), `--sch-*` (the schematic), `--pcb-*` and `--net-0..9` (the board), plus the shared scales (type `--fs-*`, space `--sp-*`, radius `--r-*`, heights `--h-*`, motion `--dur-*`, layers `--z-*`).
- Nothing else names a colour: `src/theme.test.ts` fails on a hex or `rgb(` outside `themes.css` (the one exception is `pcb/colours.ts` and the rules in `style.css` that map the server's board colours, below).
- The schematic is the server's SVG with its own variable names (`--sheet`, `--wire`, ... in `crates/netsmith-view/src/svg.rs`); `style.css` sets them from `--sch-*` on `svg.schematic`. Light is the "paper" look (cream sheet, green wires, dark red parts); dark is the dark sheet.
- The board's SVG (`crates/netsmith-pcb/src/svg.rs`) writes its colours into attributes. `style.css` has one rule per colour (`.pcb-host svg [fill="#d63c3c"] { fill: var(--pcb-front) }`), `pcb/colours.ts` lists them, and `pcb/colours.test.ts` reads `svg.rs` to fail when the server adds one the page does not map. `pcb/drawing.ts` draws copper under the parts, and a track 1 mm or wider as `.wide` (fainter): at its true width, so pads and text stay readable.
- The source editor (`components/Editor.tsx`) reads the tokens; the dark/light flag CodeMirror needs for its own widgets is a `Compartment` that follows the theme.
- Contrast: `theme.test.ts` reads `themes.css` and holds the text tokens to 4.5:1 (7:1 in the high contrast themes), graphics (wires, net colours, focus ring, accent marks) to 3:1.

## Contrast table

Lowest ratio over the surfaces each pair sits on (from `themes.css`):

| Pair | dark | light | hc-dark | hc-light |
|---|---:|---:|---:|---:|
| text | 10.99 | 13.10 | 19.30 | 18.76 |
| text-muted | 6.60 | 6.36 | 15.56 | 13.52 |
| text-faint | 4.82 | 4.89 | 12.31 | 9.70 |
| accent-text | 6.75 | 5.59 | 11.23 | 7.88 |
| label on the primary button | 7.00 | 7.00 | 11.03 | 7.29 |
| danger | 4.78 | 4.93 | 8.48 | 7.58 |
| warning | 7.92 | 4.68 | 13.37 | 7.20 |
| success | 6.75 | 4.53 | 12.56 | 7.21 |
| info | 5.86 | 5.32 | 9.93 | 7.39 |
| danger / warning / info text on their strips (lowest) | 8.13 | 7.14 | 11.47 | 9.78 |
| syntax colours on the editor (lowest of 12) | 5.50 | 5.03 | 10.49 | 7.54 |
| schematic text, net names, parts, pins on the sheet (lowest) | 8.63 | 7.39 | 12.80 | 10.11 |
| schematic wire (graphic, 3:1) | 7.93 | 4.93 | 14.12 | 6.69 |
| board reference text and key | 12.26 | 9.53 | 21.00 | 21.00 |
| net colours on the board (lowest of 10, graphic, 3:1) | 6.82 | 3.46 | 8.95 | 5.49 |
| focus indicator (graphic, 3:1) | 6.75 | 4.10 | 19.30 | 18.76 |
| border on surfaces (decorative; 3:1 wanted in high contrast) | 1.28 | 1.34 | 8.33 | 10.37 |

## Drive it

`scripts/editor-verify drive themes` (fixture `two_boards`; against the Vite dev server when you changed `web/`, see `integration.md`). Steps:

1. A stored `hc-dark` is on `<html>` when the document becomes interactive, before the app's script runs.
2. The status bar menu: five radios, the current one checked, ArrowUp and Enter choose.
3. Every theme (menu) x every view (Source, Schematic, Power tree, System, BOM, PCB, Build) at 1280 x 800: `data-theme` matches, no console error (the driver's rule), no horizontal overflow, and no text under 4.5:1 (an in-page audit: computed colour over the composited background; disabled controls, the editor's text and closed menus are left out). 28 screenshots `themes-NN-<theme>-<view>.png`.
4. 1920 x 1080: Schematic and Power tree in light and dark.
5. The palette chooses (`Theme: High contrast dark`), says so in a toast, and `Toggle light and dark` flips.
6. The choice survives a reload; System follows `colorScheme` and `contrast: more` live, and after a reload does not flash.
7. `localStorage` that throws: the page loads, follows the system, a theme choice works for the visit and a toast says it is not remembered; no page error.
8. 200 % zoom of 1280 (a 640 x 400 viewport): no overflow in Source, Schematic, Power, BOM and the Problems dock.
9. Keyboard: the first Tab stop is the skip link, every stop of the next 14 shows a ring, the board picker comes before the section tabs, the tab strip takes ArrowRight, End and Home.
10. `prefers-reduced-motion`: the dock's animation and a button's transition take no time; without it a button's transition is 120 ms.

Look at: `themes-03-light-schematic` (paper), `themes-04-light-power`, `themes-02-light-source`, `themes-17-hc-light-schematic`, `themes-24-hc-dark-schematic`, `themes-07-light-pcb`, `themes-34-palette`, `themes-40-zoom200-dock`.

## Gotchas

- The page is dark when no script runs (`:root` is dark) and System resolves to dark when the browser gives no answer.
- The driver's contexts have no `colorScheme`, which Chromium reads as light: with no stored choice a recipe's page is **light**. A recipe that wants dark sets `localStorage['netsmith.theme'] = '"dark"'` and reloads.
- In `npx vite` the page's CSS arrives with its script, so a flash cannot be seen there; `themes` checks the attribute, not the paint. The embedded bundle has a `<link>` in the head.
- The tokens are resolved where they are declared: the derived ones (`--pick`, `--select-bg`) are on `:root`, so a theme must be set on `<html>`, not on a wrapper.
- Tooltips are the browser's own (`title`), which the operating system themes; the in-view tips (`.pcb-tip`, `.pt-tip`) use the popover tokens.
- The status popover has priority over toasts: while one is open the toast stack stands to its left (`body:has(.status-popover)`).
- The PCB toolbar's findings pill (`[data-testid=findings-chip]`) opens the Problems dock for this view; the side panel's Findings tab (`.side-tabs`) lists the same findings and has the re-check.
