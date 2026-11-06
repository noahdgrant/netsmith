# Editor visual direction: from flat to layered

Status: design note, 2026-11-05, round 1 (the app shell and the design system). Not a decision of ADR 0019: it changes how the page looks and no behaviour. The owner's words: "The editor feels a little flat and old to me. In our quest to make a world-class editor, I think we should modernize it a bit." The bar is Linear, Vercel's dashboard, Raycast, Figma UI3, Zed, GitHub's Primer and VS Code's 2025 look, for a precision tool: calm, layered, no gimmicks, no decoration.

Method: the page at 1280 x 800 and 1920 x 1080 in the dark and light themes (and the high contrast pair), every section, the palette, the problems dock, menus, a toast and the status popover, before any change. Public: all names are from the repository's own examples.

## What felt flat and old, and why

1. **One level.** `--bg`, `--surface` and `--canvas` were 4 to 6 % apart and the top bar, the tab strip, the toolbars, the sidebar and the dock all used `--surface`, so the page read as one grey plane cut by lines. Nothing sat on anything.
2. **A 1 px border on everything.** Opaque `#2e3035` lines framed the top bar, each tab, the tab strip, the toolbar, the dock, the status bar, every input and every card. A border said "this is a separate thing" where a change of tone should have. Dividers between tabs and in the actions group were hard rules at full strength.
3. **Small radii.** 4 px on rows and chips, 6 on controls, 8 on cards; panels had none, since they were edge to edge. The page was a set of rectangles.
4. **Text-only tabs.** Seven words in a row with 1 px rules between them and a box-shadow underline: nothing said what a section is before reading it, and the active one jumped.
5. **No iconography to speak of.** Four hand-placed glyphs in the top bar, an `×` and `‹ ›` typed as characters, `●` in the theme menu, `⌁` in an empty state, and a different icon style (1.3, 1.4 and 1.5 px strokes) in the palette, the tree and the dock.
6. **Uniform weights and sizes.** Labels in the inspector, the dock's columns, the palette's groups and the cheat sheet were 11 px capitals with wide tracking: a generated-looking device that also read as shouting. Headings, buttons and body were 400 to 600 with no rhythm.
7. **Hard dividers, no elevation.** Menus, the palette and toasts had a shadow, but cards, toolbars and the dock had none, so a popover and a panel looked the same except for a heavier border. The primary button had no edge and no state.
8. **Abrupt state changes.** A section switch repainted at once; the active tab's line appeared where it was needed; the dock came into being; selection in data views used the brand orange (an orange tint on the tree, the navigator, the chip, the palette row), so "selected", "primary" and "brand" were one colour.
9. **A web page with a system font.** On Linux and in headless Chromium `system-ui` is DejaVu Sans, wide and plain: the tool looked different on every machine and worse where it was tested.

## Principles applied

- **Three layers, told by tone.** The frame (`--bg`: top bar, section strip, status bar) is the deepest; the work (`--canvas`) is a sheet set into it by 8 px with a 12 px radius and a hairline; a panel on the sheet is `--surface` (surface 1); a card, a menu and a toast is `--surface-raised` (surface 2); a field or a hovered row is `--surface-3`. A test holds each step visible.
- **Hairlines, not rules.** A border is ink at 7, 13 or 26 % alpha, so it works on every layer; a heavy line is only for the focus ring and in the high contrast themes, where borders are solid and 3:1.
- **Elevation has three levels**, theme-aware: 1 a card or a button (a light top edge and a short shadow), 2 a menu, a toast, the status popover, 3 the palette and dialogs. High contrast has none and uses solid borders.
- **One radius scale.** 4 inside a control (a key cap), 6 a control or a row, 8 a card or an input group, 12 a sheet, a menu, a dialog; menus have 6 px padding so the rows nest inside the box.
- **Orange means two things: the primary action and the active indicator.** The section line, the file tab line, the progress line and Make fab files are orange. What is selected or picked in a data view is a calm blue (`--sel`); problems keep their own colours.
- **Cool and deep in the dark themes, warm and clean in the light one.** A tinted neutral (hue about 220) for the dark layers, never a pure grey; warm paper (hue about 40) for light, with white work on a darker frame. High contrast stays black or white with solid borders.
- **Type: Inter (variable, self-hosted) first, the system face second.** 13 px body, 15 and 18 for titles, a little negative tracking as sizes grow, the alternates that matter in an engineering tool (`cv05`: an `l` with a tail, so `l1` and `I1` differ; `ss01`: open digits), tabular numbers for values. No capitals for labels: sentence case, 11 to 12 px, semibold, in the faint ink.
- **Icons are one set**: 16 px grid, 1.5 px stroke, round caps, `currentColor`, drawn here (`components/Icons.tsx`).
- **Motion answers an action and takes 120 to 180 ms**: the active section line glides, a popover scales in from its corner, the dock opens by height, a view fades in with a 4 px rise (150 ms), rows ease on hover. Nothing runs on its own, and `prefers-reduced-motion` turns all of it off.
- **Keep what works**: the density, the keyboard model, the four themes and the contrast tests (extended to every new token).

## Token additions (`themes.css`)

| Token | Where | What it is |
|---|---|---|
| `--surface-3` | each theme | a field, a hovered row, an `on` toggle (surface 3) |
| `--surface-1`, `--surface-2` | derived | the names of `--surface` and `--surface-raised` |
| `--border`, `--border-subtle`, `--border-strong` | each theme | now alpha hairlines (solid in high contrast) |
| `--sel`, `--sel-text` | each theme | the calm blue of a picked row, and its words |
| `--sel-bar`, `--sel-line`, `--pick`, `--select-bg`, `--active-bg` | derived | the bar, the line and the washes of a selection |
| `--elev-1`, `--elev-2`, `--elev-3` | each theme | elevation; `--shadow-card/float/pop` are their old names |
| `--elev-accent`, `--edge-hi` | each theme | the edge of a primary or danger button, a light top edge |
| `--accent-soft`, `--accent-line` | derived | the brand orange as a wash and as a line |
| `--skeleton`, `--skeleton-hi` | derived | the blocks of a view on its way |
| `--r-xs` 4, `--r-sm` 6, `--r-md` 8, `--r-lg` 12, `--r-xl` 16 | shared | the radius scale |
| `--ctl` 28, `--ctl-lg` 32, `--h-*`, `--bar` 52, `--tab` 40, `--status` 26 | shared | control and bar heights |
| `--gap` 8 | shared | the frame around the sheet |
| `--fs-2xl`, `--ls-body`, `--ls-title`, `--ls-label`, `--font-features` | shared | type |

Tests added in `theme.test.ts`: every text ink on `--surface-3`; `--sel-text` and `--sel` on every layer; the accent and focus ring on the frame; the layers a visible step apart; elevation present in each theme and none in high contrast. The accent of the light theme went from `#d4701a` to `#c8650f` so that the section line is 3:1 on the darker frame.

## What round 1 does not touch

The schematic drawing, the PCB canvas and the power and system cards take what the tokens give them (radius, elevation, surfaces, selection colour) and nothing more. Round 2 is the views: toolbars with content, the tables of the BOM, the inspector, the Build page's layout, the cards.

## Round 2: the views

Status: 2026-11-05, round 2. Round 1 built the frame and the design system; the views still looked like the page before it. Same method: every section in the four themes at 1920 x 1080 and 1280 x 800 before any change, then after.

What read old, view by view:

1. **Build** was a document: three headings and a paragraph each over a button, and the right half of the sheet empty. It is now a header (the board, what the page will make, whether the source builds) and a card for each thing it makes: an icon, a title and one line, the command line that does the same as a chip with a copy button, the button, a last-run line (what came of it, how long ago, how long it took) and, once run, the output as a panel (monospace, a number and a severity colour for each line, folded or open, copyable) with the files it wrote.
2. **The BOM** was an unstyled table under a block of prose. It is a table with a sticky header, 34 px rows, right-aligned tabular numbers, designators as chips (a run of three or more as `C1–C3`), a sourcing chip for each line (found, stale, not checked, ambiguous, missing, by hand), a summary above it (parts, unique values, cost per board, problems), a filter, grouping by value or module, a column menu, and the help in an info popover. "Check parts" is one button; preview and offline are switches beside it.
3. **System and power** cards were large at the default zoom because a small drawing was blown up to fill the window. A drawing now opens at its true size or smaller, never larger (the zoom bar reads the true scale); a card has an icon, a name and the type as a chip, and its ports in columns with the units muted. A picked card is blue, as everywhere; the wires of the card under the pointer show a little more; the legend is a row of chips. The mates are one row for each mated pair (the two connectors, the contacts counted by role, what the mate joins), the contacts folded under it, and the ground groups under those.
4. **Inspectors** each had their own hierarchy. They share `components/Inspector.tsx`: an eyebrow that says what kind of thing, a title row with an icon and icon buttons, a definition list with muted labels, sections that fold, and pins and nets as chips.
5. **The sidebar**'s action rows were text buttons. The header is now a title and five icon buttons with tooltips, the filter has an icon, a row is inset and rounded, and a file with errors or warnings carries a count.
6. **The board's toolbar** is grouped: the primary action and the status pills, a segmented tool choice with undo and redo, the grid, a layers panel that is also the legend (a swatch of the board's own colour for each layer) and the side panel's toggle.
7. **Go to anything** shows a distinct icon for each kind of result (a command has its own, a view the section's), a type label at the right and a count on each group's head.
8. **The board picker** says why it is empty (still reading, nothing built with the first error one click away, or no board written) instead of being a blank select. One tooltip, in the popover's own style, serves the top bar, the status bar, the dock and the new icon buttons (`data-tip`); an element that a recipe finds by its `title` keeps it.
9. **The dark drawing**: part bodies were an olive brown (`--sch-fill` a fixed yellow-brown) with bright pink outlines. A body is now a tint of its own outline over the sheet (11 % in dark, 14 % in high contrast dark; the light paper keeps its cream), wires are a calmer sage, a bus trunk has its own teal (`--sch-bus`), a lit net is amber and a picked part is the page's blue in every theme. The server's lit width (double the wire's) is brought down so a lit net is a line and not a bar at close zoom.

New tokens: `--success-text`, `--seg-track`, `--seg-on`, `--sch-bus`, `--zebra`, `--row-line`; `--sch-fill` may be a `color-mix` of `--sch-body`. `theme.test.ts` holds the four tones' words to AA on every layer a pill, a card and a log sit on, the segmented control's two states, the bus colour as text, the pin text and outline on the tinted body, and that a lit net, a picked part, a bus and a wire are four colours.

What round 2 leaves: no detail panel for a picked card of the system or power views (the card is the detail; the tips use the same parts), the table of the PCB's nets and parts still lists rows the old way, and the rows of the tree are not draggable (nothing drags them).
