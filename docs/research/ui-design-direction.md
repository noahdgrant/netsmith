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
