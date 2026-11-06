# Themes, type, buttons, keyboard and motion

## Get to it

Five choices, kept per browser (`localStorage`, key `netsmith.theme`, a JSON string): **System** (follows `prefers-color-scheme`, and `prefers-contrast: more` for the high contrast pair), **Light**, **Dark**, **High contrast light**, **High contrast dark**.

- Status bar, right side: the half-filled circle opens a menu of the five (Arrow keys, Enter).
- Command palette (`Ctrl+Shift+P`): `Theme: Light`, `Theme: Dark`, `Theme: System`, `Theme: High contrast light`, `Theme: High contrast dark`, `Theme: Toggle light and dark`.
- First paint: `index.html` has a small inline script (`#theme-boot`) that sets `<html data-theme>` before anything is drawn. `src/theme.test.ts` runs that script against `resolveTheme` for every stored value and every system setting.

## How it is built

- `web/src/themes.css` holds every colour of the page as a token, one block per theme (`:root` alone is dark). Names are semantic: the layers `--bg` (the frame), `--canvas` (the work), `--surface` (surface 1, a panel), `--surface-raised` (surface 2, a card or a menu) and `--surface-3` (a field, a hovered row); `--border*` (hairlines: ink at 7, 13 and 26 % alpha, solid in the high contrast themes), `--text*`, `--accent*` (the brand orange: the primary action and the active line only), `--sel*` (the calm blue of what is picked in a data view; `--pick`, `--select-bg`, `--sel-bar`, `--sel-line` are derived), `--danger*`, `--warning*`, `--success`, `--info*`, `--hover*`, `--pop*`, `--elev-1..3` (elevation; `--shadow-card/float/pop` are their old names), `--syn-*` (the source editor's colouring), `--sch-*` (the schematic), `--pcb-*` and `--net-0..9` (the board), plus the shared scales (type `--fs-*`, `--ls-*`, `--font-features`, space `--sp-*`, radius `--r-xs/sm/md/lg/xl` = 4/6/8/12/16, heights `--h-*` and `--ctl` 28 / `--ctl-lg` 32, `--gap` 8, motion `--dur-*`, layers `--z-*`).
- The window is three layers told by tone: the frame (top bar, the strip of sections, the status bar) on `--bg`; each section a sheet of `--canvas` set into it by `--gap` with a 12 px radius, a hairline and elevation 1; panels on the sheet (toolbars, the tree, the inspectors, the dock) on `--surface`. `App.tsx` `innerBox` is the width the panes share inside the gap, for the divider and the drop zones.
- Orange is the primary action (`.tool.primary`, the skip link) and the active indicator (`.tab-ind` under the active section, the active file tab's line, the progress line, the divider while it is dragged); picking a row, a card, a part or a mode is blue.
- Type is Inter (variable, Latin and Greek, `web/src/assets/fonts/` with `OFL.txt`; the binary serves it from the bundle like the icons) first and the system face second. `body` sets `font-feature-settings: 'cv05', 'ss01', 'calt'` (an `l` with a tail, open digits) and `text-rendering: geometricPrecision` (without it Chromium at 1x on Linux rounds the width of a space and "Power tree" reads "Powertree"; buttons and inputs inherit it explicitly). Labels are sentence case, never capitals.
- Icons are one set drawn in `components/Icons.tsx` (16 px grid, 1.5 px stroke, round caps, `currentColor`): `<Icon name="…" />`. The file tree has its own (`TreeIcons.tsx`), in the same style.
- Motion (`style.css`, the last blocks): a hover or press eases over 120 ms (every control is in one `:is(...)` list), the active section's line glides (its place and width are measured by `SectionStrip` and given to the strip as `--ind-x` and `--ind-w`), a view fades in with a 4 px rise over 150 ms, a popover scales in from its corner, the dock opens by height over 160 ms. `prefers-reduced-motion` sets every animation and transition to nothing.
- Round 2 added `--success-text` (a success pill's words), `--seg-track` and `--seg-on` (a segmented control), `--sch-bus` (the bus trunk of the schematic), `--zebra` and `--row-line` (a table). `--sch-fill` of the dark themes is a `color-mix` of the part's outline over the sheet; `--sch-mark` (a picked part) is blue in every theme and `--sch-hit` (a lit net) amber. One tooltip serves every element with `data-tip` (`components/Tooltip.tsx`: a pause, then a box in the popover's style; `data-tip-keys="Ctrl+K"` adds key caps); an element that a recipe finds by its `title` keeps it.
- Nothing else names a colour: `src/theme.test.ts` fails on a hex or `rgb(` outside `themes.css` (the one exception is `pcb/colours.ts` and the rules in `style.css` that map the server's board colours, below). A shadow with an alpha is a token (`--elev-*`, `--edge-hi`, `--elev-accent`).
- The schematic is the server's SVG with its own variable names (`--sheet`, `--wire`, ... in `crates/netsmith-view/src/svg.rs`); `style.css` sets them from `--sch-*` on `svg.schematic`. Light is the "paper" look (cream sheet, green wires, dark red parts); dark is the dark sheet.
- The board's SVG (`crates/netsmith-pcb/src/svg.rs`) writes its colours into attributes. `style.css` has one rule per colour (`.pcb-host svg [fill="#d63c3c"] { fill: var(--pcb-front) }`), `pcb/colours.ts` lists them, and `pcb/colours.test.ts` reads `svg.rs` to fail when the server adds one the page does not map. `pcb/drawing.ts` draws copper under the parts, and a track 1 mm or wider as `.wide` (fainter): at its true width, so pads and text stay readable.
- The source editor (`components/Editor.tsx`) reads the tokens; the dark/light flag CodeMirror needs for its own widgets is a `Compartment` that follows the theme.
- Contrast: `theme.test.ts` reads `themes.css` and holds the text tokens to 4.5:1 (7:1 in the high contrast themes) on every layer, the blue's text and the accent text on the frame, graphics (wires, net colours, focus ring, accent and selection marks) to 3:1, and that each layer is a visible step from the one under it and that elevation is none in high contrast. Round 7 added the text on a tint, in all four themes at 4.5:1: every syntax colour on the diff view's new and old lines and their changed words (inside `.diff-panel` each `--syn-*` is its `--diff-syn-*`, the colour pushed halfway to the page's ink; the row tint is 12 % and the word's 24 % of the mark), on the changed lines of the Source with the marks on, the secondary words on the row that is picked (`.qp-row.on` shows the detail and the kind in `--text-muted`), and the status bar's `--*-text` words on the frame. A stale view is drained of colour (`saturate`, which keeps every luminance), never made translucent: opacity would lower the contrast of every word on it.

## Contrast table

Lowest ratio over the surfaces each pair sits on (from `themes.css`, by the same pairs as `theme.test.ts`):

| Pair | dark | light | hc-dark | hc-light |
|---|---:|---:|---:|---:|
| text | 11.45 | 13.36 | 17.40 | 16.52 |
| text-muted | 6.76 | 6.69 | 14.03 | 11.90 |
| text-faint | 4.77 | 5.15 | 11.10 | 8.54 |
| accent-text | 7.36 | 5.06 | 11.23 | 7.88 |
| label on the primary button | 7.00 | 7.00 | 11.03 | 7.29 |
| danger | 5.21 | 5.10 | 8.48 | 7.58 |
| warning | 8.64 | 4.85 | 13.37 | 7.20 |
| success | 7.36 | 4.70 | 12.56 | 7.21 |
| info | 6.40 | 5.51 | 9.93 | 7.39 |
| selection text (the blue) | 8.41 | 6.31 | 13.56 | 8.77 |
| selection mark (graphic, 3:1) | 5.86 | 4.64 | 8.95 | 6.51 |
| danger / warning / info text on their strips (lowest) | 8.13 | 7.14 | 11.47 | 9.78 |
| syntax colours on the editor (lowest of 12) | 6.03 | 5.03 | 10.49 | 7.54 |
| schematic text, net names, parts, pins on the sheet (lowest) | 9.25 | 7.39 | 12.80 | 10.11 |
| schematic wire (graphic, 3:1) | 8.51 | 4.93 | 14.12 | 6.69 |
| success words on a card, a log, a pill (lowest) | 9.48 | 6.95 | 13.34 | 9.80 |
| schematic bus (text and graphic) | 8.35 | 5.24 | 13.76 | 8.26 |
| schematic pin and outline on the part body (the body is a tint of the outline; lowest) | 7.37 | 8.22 | 10.34 | 10.70 |
| segmented control: words off and on (lowest) | 9.03 | 6.88 | 16.93 | 11.90 |
| board reference text and key (on the canvas and the substrate) | 11.43 | 8.71 | 19.57 | 18.76 |
| net colours (a plane's border; lowest of 10, graphic, 3:1) | 6.82 | 3.46 | 8.95 | 5.49 |
| copper, via, outline, ratsnest on the board (graphic, 3:1) | 5.91 | 3.79 | 9.99 | 6.45 |
| courtyard, fab, keepout, silkscreen on the board (graphic, 3:1) | 4.23 | 3.22 | 7.86 | 5.29 |
| a pad's number on copper (lowest) | 6.35 | 4.60 | 10.72 | 7.22 |
| focus indicator (graphic, 3:1) | 6.58 | 3.71 | 17.40 | 16.52 |
| accent marks: the active tab line (graphic, 3:1) | 6.80 | 3.19 | 11.03 | 6.56 |
| border on surfaces (hairline: ink at 13 %; solid and 3:1 in high contrast) | alpha | alpha | 8.33 | 10.37 |

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
10. `prefers-reduced-motion`: the dock's animation and a button's transition take no time; without it a button's transition is 120 ms. The glide of the section line, the fade of a view and the scale of a popover are not asserted: look at them by hand.

Look at: `themes-03-light-schematic` (paper), `themes-04-light-power`, `themes-02-light-source`, `themes-17-hc-light-schematic`, `themes-24-hc-dark-schematic`, `themes-07-light-pcb`, `themes-34-palette`, `themes-40-zoom200-dock`.

## Gotchas

- The page is dark when no script runs (`:root` is dark) and System resolves to dark when the browser gives no answer.
- The driver's contexts have no `colorScheme`, which Chromium reads as light: with no stored choice a recipe's page is **light**. A recipe that wants dark sets `localStorage['netsmith.theme'] = '"dark"'` and reloads.
- In `npx vite` the page's CSS arrives with its script, so a flash cannot be seen there; `themes` checks the attribute, not the paint. The embedded bundle has a `<link>` in the head.
- The tokens are resolved where they are declared: the derived ones (`--pick`, `--select-bg`) are on `:root`, so a theme must be set on `<html>`, not on a wrapper.
- Tooltips are the browser's own (`title`), which the operating system themes; the in-view tips (`.pcb-tip`, `.pt-tip`) use the popover tokens.
- The status popover has priority over toasts: while one is open the toast stack stands to its left (`body:has(.status-popover)`).
- The PCB toolbar's findings pill (`[data-testid=findings-chip]`) opens the Problems dock for this view; the side panel's Findings tab (`.side-tabs`) lists the same findings and has the re-check.
