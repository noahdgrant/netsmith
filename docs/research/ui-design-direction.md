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

## Round 3: the drawings, the graphs, one hover, and the whole product at 1 px

Status: 2026-11-06, round 3. The orchestrator's review of rounds 1 and 2: the shell, Build, BOM, Source and the tree look contemporary; what is still off is the board's drawing (saturated yellow, green and red pads, magenta courtyards, a thick yellow outline, white ratsnest, giant references), small graphs lost in a large canvas, no hover that crosses views, a few things that did not move, and empty states that each had a style of their own.

### The board's drawing

The server writes its colours and widths into attributes (`crates/netsmith-pcb/src/svg.rs`, not touched). A CSS rule beats a presentation attribute, so `src/pcbdraw.css` restyles every mark without a change to the server, and `style.css` still maps each of the server's colours to a token (a test reads both).

- **Substrate.** The outline's fill is `--pcb-board`, a step from the canvas `--pcb-bg` (1.07 to 1.16:1), and its line is a thin neutral blue-grey (`--pcb-outline`), not yellow. A cutout and a drill show the canvas.
- **Copper is a layer, not a net.** Front pads and tracks a muted copper (`--pcb-front`), a part on the back (its group's transform mirrors it, so a CSS selector finds it) and its tracks a muted teal-blue (`--pcb-back`); a through-hole pad has a tone of its own (`--pcb-thru`) and a soft ring, a via a cool grey ring (`--pcb-via`) around a hole that shows the canvas; a pad on no net is fainter. The nets lose their colours on the board (a plane's dashed border keeps its net's tone, at half strength), so the net list's swatch is a neutral ring and a dashed square for a plane.
- **Tracks** are drawn as a group at 85 % (`--pcb-track-opacity`), not as lines at 85 %: two round caps that overlap no longer show a darker dot. A power track is fainter still.
- **Hairlines that stay hairlines.** Outline, courtyard, fab, silkscreen, ratsnest, plane borders, keepouts and every overlay mark have widths in screen pixels (`calc(1.5 * var(--px))`; `Canvas` sets `--px` from the zoom, because `vector-effect: non-scaling-stroke` ignores the CSS transform that pans and zooms). At 50 px per millimetre a 0.1 mm line is no longer a 5 px bar.
- **Silkscreen** thin and at 85 %, a pin 1 triangle at 55 % of its size, **courtyards** a faint dashed neutral (no magenta), **references** at 0.62 mm in the page's face, 600 weight, with a halo of the substrate; the number of a pad with a hole is not drawn over the hole.
- **Keepouts** are hatched in a muted red (a height limit in amber), through two patterns that `pcb/drawing.ts` puts in the drawing's `defs`. **Findings** are a solid 2 px ring with a soft glow and a wash.
- **Picks and hover** are the page's blue with a short glow (`--sel`); the rest of a lit net steps back to 20 %. The **grid** is a few dots at 32 %.

New tokens (four values each): `--pcb-board`, `--pcb-thru`, `--pcb-refdes`, `--hover-lit`, `--hover-lit-bg`; the values of `--pcb-bg`, `--pcb-outline`, `--pcb-silk`, `--pcb-front`, `--pcb-back`, `--pcb-via`, `--pcb-ratsnest`, `--pcb-fab`, `--pcb-courtyard`, `--pcb-keepout`, `--pcb-height-limit` and `--pcb-select` (now the page's `--sel`) changed. `theme.test.ts` holds copper, via, outline, ratsnest, courtyard, fab, keepout and the pick to 3:1 on the canvas and the substrate in every theme, silkscreen to 3:1, the references and key to 4.5:1 (7:1 in the high contrast themes), a pad's number on each copper to 4.5:1, the substrate a visible step from the canvas, the four copper tones all different and the front warm and the back cool.

### System and power

- **Framing.** A graph opens fitted with 56 px of air, centred, at most 150 % and at least 45 % (below which it opens at 45 % from its top left), and Fit does the same; `1:1` is true size. `PanZoom` gained `fitPlaces` and a `whole` argument to `placeView`. The dot grid belongs to the window (`--pz-x`, `--pz-y`, `--pz-k` on the host), so it fills it and pans with the drawing.
- **Calm tone.** The path from a pick is the selection's blue (solid upstream, dashed downstream, in the cards' edges, the arrowheads, the legend and the minimap); the rest stays neutral. A board link is a neutral dashed line. Orange is only the primary action and the active indicator again.
- **Mates** fold to their head when there are more than two; they, like the zoom bar, are marked `data-toast-clear`.

### One hover for every view

`src/hover.ts` is a store like the selection (kind `part`, `net`, `module` or `bus`, a key, and optionally the designators or nets that stand for it) with a 70 ms rest before it lights and a 90 ms grace before it goes out, so moving along a list moves the light instead of blinking it. `src/hoverDom.ts` is the one subscriber: it finds everything that stands for the same thing (`[data-ref]`, `[data-net]`, `[data-hv]`, `[data-hv-ref]`) and toggles `.hover-lit` on it, in one `requestAnimationFrame`; no view re-renders and no drawing is replaced (a recipe checks that no node is added or removed). A drawing that is replaced while something is lit is lit again. Pointer and focus both say it, a touch never does, Escape and the window losing focus clear it. The light is `--hover-lit`, a neutral that is not the blue of a pick, with a wash (`--hover-lit-bg`) on rows and chips, a ring on cards and a short glow on a drawing. Sources: BOM rows, chips of both inspectors, the board's part and net lists, power and system cards, the module navigator, a definition of the tree (the parts it declares), a problem that names a part, module or net, a part or net of the schematic, a part, track, via or pad of the board.

### Motion

Checked with real evidence (the notes are in `features/motion.md`): the line under the active tab moves from tab to tab, a view's opacity rises 0 to 1 over 150 ms with no frame at full opacity first, a popover scales in over 120 ms, the dock's height runs 2 to 240 px in 160 ms, a toast comes in over 160 ms, and with `prefers-reduced-motion` all of them are instant. Found and fixed: the toast had no exit (it now fades and folds its height away over 160 ms, so its neighbours do not jump); an even row of the BOM did not respond to the pointer at all (a zebra selector was more specific than the hover and the pick); skeletons shimmered in sections that were not on show.

### Empty, first-run and error states

One component, `EmptyState` (`components/ui.tsx`): a mark that says what kind of moment it is (neutral, ok, warn, bad), a title, one line of help, one clear action, calm words (what is so, what to do, no apology). It stands for: no board built (with the error that stops it), no board, offline (the server never answered), no parts, no line matches, no problems (and none in this view, or none under the filter), nothing found by the checks, KiCad not found, the board that could not be placed or drawn, no file open, no file or module or part or net matches. `compact` is the size for a panel, a list and the dock. The old `.empty-state`, `.dock-empty .big` and a plain `.none` line are gone. What is not covered: a file that is missing on disk says so in a toast only.

### The 1 px audit

Every section at 1280 x 800, 1920 x 1080 and 2560 x 1440, in light and dark (high contrast for the board and the themes recipe), the top bar at 1000 px, the file tabs with 14 open, a toast over the palette; a script checked every control for a focus indicator, every ellipsis for a tooltip, popovers and menus against the window's edge, icon and label centre lines and overflow.

Found and fixed:

1. **The chip of the picked thing was a sliver at 1280** (the search box took 38 % of the window, the Source button took the rest): the search is 24 % now, the Source button folds to its icon below 1400 px (its name stays for a screen reader), the title has a minimum and ends in an ellipsis.
2. **The active file tab was not scrolled into view** (14 open: it was off to the right), and the strip had no cue that it scrolls and its tabs could not be reached by keyboard: it scrolls the active tab in, fades the edge that has more, tabs take Tab, Enter and Space.
3. A hovered or picked **even row of the BOM** did not change colour.
4. **Toasts** covered the zoom bar of every drawing and the mates of the system view; they stand above whatever is marked `data-toast-clear` and on show. A toast that left could stay in the page, invisible, when another left in the same moment (found by `pcb-edit` waiting for `.toast` to go).
5. The **BOM's table** was 2500 px wide at 2560; it keeps 1760 px, centred.
6. A **pad number over a drill hole** (unreadable dark on dark) is not drawn.
7. The **dot grid** of the graphs stopped where the drawing did.
8. **Skeletons** shimmered off screen.

Left, in order of how much I would do it next:

1. At 1000 px the BOM's table clips its right columns (`Each`, `Stock`, `Checked`) inside its scroll box with no cue; hide them below a width, or fade the edge as the file tabs do.
2. At 1000 px a wide power tree opens at about 60 %, with 7 px text: it wants a vertical layout, or a first view of one stage and its neighbours.
3. The dock opens by animating its height, which reflows the view above it every frame (smooth, not free); a transform on a panel that is already its final size would not.
4. Hover lights a few hundred elements through `filter: drop-shadow`; on a board of a thousand parts that has not been measured here.
5. KiCad not found is said twice in the findings list (the old line a recipe reads, and the new state).
6. The through-hole bronze of the light theme is the heaviest mark of that drawing; a lighter tone fails 3:1 against the substrate.
7. A missing file, a failed analysis of the project and a server that goes away after it answered have no state in a view, only the status bar and a toast.
8. Focus rings were checked by a script (a ring exists), not by looking at each one at 200 %.
9. The `Split right` button is dimmed below 1280 px without a word about why (the second pane needs room).

## Round 4: the source knows the electronics

Status: 2026-11-06, round 4 (UI only, no change to the server). The owner's words: the editor must be world-class, and the Source view is where a code-first electronics engineer lives. It was a good CodeMirror editor with a file tree, and nothing in it knew a part from a word. The benchmark's list (`ui-benchmark.md`: VS Code's hovers, breadcrumbs, outline and peek; atopile's value hints; Altium's cross-probing) asked for hover cards, breadcrumbs, an outline, history and live value hints. All of it is built from what `/api/analysis`, `/api/board`, `/api/power`, `/api/bom`, `/api/project`, `/api/definition` and `/api/file` already answer.

### Reading the text, because the server does not say where a definition ends

`Def` is a kind, a name and a line. The breadcrumbs, the outline, the folds, the completion and go to symbol need the extent of each definition and what it declares, and they must work on text that is not saved. `source/symbols.ts` reads the text with the tokenizer that colours it (`nsm.ts`): definitions by their keyword, declarations (`.r1: Resistor { ... }`) by `. name :`, nested by the braces, ignoring braces in strings and comments; the role of a declaration (a part, a module, a port, a field) is the kind of the definition its type names, taken from the project's file list. It is the one place the page parses the language; `Def` gaining an end line and its children would let it go.

### Hover cards

A card shows after 250 ms at rest on the name of a declaration, on a type, or on a string that names a net, and goes when the pointer moves off the token (CodeMirror's `hoverTooltip`); `Ctrl+Alt+I` opens it at the cursor and Escape closes it. `source/card.ts` is two pure steps: `targetAt` reads the token under the pointer, `cardFor` builds the card from the board's detail, the power tree, the bill of materials and, for a definition, the text of its file (signature, the doc comment above it, what it declares, its parts, ports and problems). The card is drawn with the inspector's own parts (`InspectorHead`, `Defs`, `PinChip`) at 12 px radius on `--elev-3`; a pin chip picks its net in the shared selection and lights it through the shared hover while the pointer is on it; the card itself lights the part, net or module in the other views while it is open; and a row says where to see it: Schematic, PCB, BOM. A declaration that the board shown does not hold (the other board's `.r1`) falls back to its type's definition, with a line that says so.

### Breadcrumbs, history, outline

Above the text: Back and Forward (the places the cursor was taken to by go to, a click-through from another view, the outline, a problem; a stack and an index, `source/navhistory.ts`), then the file, the definition and the declaration at the cursor, each a button with a dropdown of its siblings (arrow keys, Enter, Escape), then switches for wrapping, hints and the outline. `Alt+Left` goes back through the history and, at its first place, back to the view the click-through came from; the `Back to <view>` chip stays. The outline is a panel at the foot of the file tree that takes the room the tree leaves: the definitions and their declarations with kind icons, a filter, a badge for the problems in each, the entry of the cursor marked and scrolled into view, a drag handle for its height.

### Hints, completion, find, folds, tabs

- **Inline hints** are decorations at the end of a line, not text: the resolved value, package and LCSC number of a part (`10u · 0402 · C315248`; a module placed twice says `×2`), and the solved voltage of a rail where the source names its net. They are not selected, not copied (a test copies the whole file and compares it with the file), and they move no text. Chosen from the board's detail when the file is as on disk; while it has edits they stay on their lines.
- **Completion** is the palette's list: a kind icon (part, module, interface, net, keyword, unit, field), the name, a detail. After a dot the members of the definition (and, after `.a.`, of `.a`'s type), after `name:` the types, after a number the units, inside `.net &= "` the board's nets, at the start of a line the keywords.
- **Find and replace** is a card at the top right of the text with the page's controls: a count (`3 of 12`), previous and next, case, whole word, regular expression, and a second row to replace (`Ctrl+H`).
- **The gutter** has muted numbers, a soft accent bar at the cursor's line, fold marks that show on hover, and the page's severity icons for problems. **Folds** work on any block that ends a line in `{`, and a folded block says how many lines it hides. **Indent guides** are a background gradient of the line (nothing to select). The lint tooltip is a card: severity, stage, the compiler's message, where, and what to do.
- **Tabs** have the file's icon, an unsaved dot that gives way to the close button under the pointer, a lock on a library file, the folder when two tabs share a name, a middle click to close, a drag to reorder (`Alt+Shift+Left` and `Right` from the keyboard), and a list of every open file when they do not fit. A library file has a one-line read-only note.

Commands: back, forward, show the hover card, open the breadcrumb, fold all, unfold all, zoom in, out and reset (`Ctrl+=`, `Ctrl+-`, `Ctrl+0`, remembered), find and replace, wrap, and switches for hints, cards and the outline named for what turning them does next; `Ctrl+Shift+O` lists the open file's symbols when the editor has the keys, and the project's modules and definitions otherwise.

New tokens: `--cm-hint` (4.5:1 on the canvas in every theme) and `--cm-guide` (a hairline step from the canvas, held between 1.08 and 3:1, and quieter than the hint).

### What the server would make better

- `Def` with an `endLine` and `children` (the declarations), so the page need not read the text for the outline.
- A pin's function name (`VIN`, `EN`, `SW`) next to its pad number in `NetInfo.pins`: a part's card lists pad numbers and nets.
- The doc comment and signature of a definition in `/api/definition`'s answer, so a card need not fetch a library file.
- The solved value of a part (`.effective_capacitance`, the chosen tolerance) in `PartInfo`: a hint shows what the catalog row is, not what the solver derived.
- A board for each file (which boards a module file is in), so a declaration in a module shared by two boards has a card for the one the person looks at.
- `PowerNode.voltage.text` has no space before the unit (`3.135V..3.465V`), where the rest of the page writes `3.135 V`.

### Found while building

- A render of Preact inside another render (CodeMirror builds a tooltip while the page is rendering) turns `hooksAllowed` off in `preact/debug`, and the next hook throws `Hook can only be invoked from render methods`. The card is drawn a microtask after CodeMirror's update, and the gutter and completion icons are SVG elements built from the same drawings (`source/dom.tsx`), not rendered.
- The file tree reset its scroll to where it was when a press redrew it, before the browser's scroll event had said where it was: a row scrolled into view just before the click moved from under the pointer. The press now records the place. (It showed because the outline made the tree short enough to scroll in a recipe.)
- Another part of the page already used the class `crumb`; the breadcrumbs have their own prefix (`bc-`).

## Round 5: the finishing pass

Status: 2026-11-06, round 5. The orchestrator reviewed the merged build's screenshots (the merge agent's list) and asked for a finishing pass: fix that list, a settings panel (theme, density, size, motion), a help menu with About, first-run and brand (the tab, a favicon, a splash, an empty project), and an audit of every section at 1000 x 700 and 1280 x 800. Same method as before: the page in a real browser, dark and light, before any change and after (the merge agent's folder for the list, the recipes for the rest).

### The merge agent's list

- **A toast over a view's controls.** The rule is one: the stack stands above whatever is marked `data-toast-clear` and shown. What was wrong was not the rule but when it was applied: the stack measured the marked elements when the number of toasts changed, so a toast that was already there when a view came into the page (a section switch mounts the PCB's zoom bar, shows the hidden one of another view) stayed where it was and covered it. It now measures again on a section switch (a `MutationObserver` on the panes' area, for what is added and for the class of a section), on a resize of a marked element and on the window's; the problems dock is marked too, so a toast does not cover its rows. It glides to its new place (the `bottom` transition it already had).
- **The board's toolbar.** It folds by the width of its own view (`container-type: inline-size` on `.pcb-view`, thresholds in rem so a larger size folds it earlier), never wrapping at 1000 px or more: below 1180 px the tool choice and Layers keep their icons and lose their words (`.seg-label`, `.tb-label`: out of sight and still in the page for a screen reader, with the tooltips), `took` and the cursor readout go; below 900 px the DRC chip goes (the Build page has the result); below 640 px only the findings pill stays; below 560 px it wraps, which is a pane of a split. No overflow menu: what goes is said elsewhere, and a menu is a place to hide the findings.
- **The Build counts** say what they count: `2 boards in the project` (header), `all 2 boards` (the Schematic card), `7 boards in this order` (the order card), `Total, 2 designs` and the sum in the table.
- **Four-layer boards.** The server draws a plane as a dashed rectangle with a net's colour and nothing else (`svg.rs`, not touched). A plane is now a translucent fill (10 %) and a dashed border in a tone of its layer, `--pcb-plane-1` to `--pcb-plane-4`, and its net's name on its border (`pcb/drawing.ts` numbers the rectangles and adds a `text.plane-label`, staggered along the border so two never meet). The tones are four hues away from the front and back copper and 3:1 on the canvas and the substrate in every theme (`theme.test.ts`). **The layer of a plane is a guess**: the rectangle has no layer attribute, so the n-th rectangle gets the n-th tone. A `data-layer` on it (see What I would ask of the server) makes it exact.
- **System edge labels** stand on a pill of the page's surface with a hairline, as wide as the words (measured), 4 px above the wire. They did touch it: the baseline was 4 px above the line and the halo was the canvas's colour.
- **BOM at 1000 px**: the table scrolls inside a frame; its first column (Designators) is sticky (an opaque layered background so the zebra, hover and pick still show) with a soft edge once the table has moved; the edge that has more to scroll to fades into the surface (`useScrollEdges`, a hook any scrolling box can use). **The power tree and the system view** open at 75 % at least (`GRAPH_MIN_K`; was 45 %, which meant a wide tree opened at 60 % with 7 px text): what does not fit is scrolled to, with the minimap and Fit (which still shows the whole). The recipes `power-tree` and `system-view` asserted "no overview while the whole tree is in the window" right after opening; they now press Fit first (the premise, the whole tree in the window, is what Fit makes; the assertion itself is unchanged).
- **The overview's thumbnails.** All the same paper: the module frame's 4 % green wash filled the whole thumbnail of a module whose frame fills it, and only the inside of a narrow frame (a connector's), so two kinds of card read as two kinds of paper. In a thumbnail the frame is a dashed line on the sheet.
- **Through-hole pads (light)**: a body a step lighter (`--pcb-thru-fill`, #d6c48a) inside a 1.5 px ring of the old tone (`--pcb-thru`, 3:1 on the substrate and the canvas); the dark theme does the same the other way round (a body of #8a7a58 in the old bright ring); the high contrast themes keep one tone. `theme.test.ts`: ring 3:1, body 1.3:1 from the substrate and from the hole, ring 1.5:1 from the body.

### Settings

A gear in the top bar (`Ctrl+,`, the palette): the theme (the five, as a grid with swatches), **Density** (Comfortable, Compact), **Size** (90, 100, 110, 125 %) and **Motion** (System, Reduce). One JSON object in `localStorage` (`netsmith.settings`), applied before the first paint by a second inline script in `index.html` (a test runs it against `parseSettings` and `rootState`), guarded by try/catch everywhere, and a toast says when a choice cannot be kept. Every choice is a palette command (`Density: Compact`, `Size: 125 %`, `Motion: Reduce`, `Settings: back to the defaults`). (The glossary says to avoid "settings" for the manifest; here it is what the person set in the editor, the word the owner used.)

- **Size is the root font size** (`html { font-size: calc(100% * var(--ui-scale)) }`, a share of the browser's own). To make that true, every length of `style.css`, `views.css` and the shared scale of `themes.css` is rem or a token (`scripts/editor-verify-js/css-rem.mjs`, run on those three files: lengths up to 2 px, borders, shadows, radii, transforms, strokes, media queries and the rules of the drawings and the graph cards stay px; running it twice changes nothing; **run it again on a merged stylesheet**). A drawing and the cards of the power tree and the system view have a zoom of their own and a size computed in script (`system/metrics.ts`, `power/metrics.ts`), so inside `.panzoom .canvas` the scale tokens are px again. An icon grows with the size except inside a canvas (`--icon-k`). The source editor's text is CodeMirror's and has its own setting (round 4), not this one.
- **Density** is `[data-density="compact"]` in `settings.css`: `--ctl` 28 to 24, `--ctl-lg` 32 to 28, `--h-*`, `--row`, `--row-lg` 34 to 30, `--row-sm` 30 to 24, `--bar` 52 to 46, `--tab`, `--status`, `--gap` 8 to 6. Thirty-seven hard-coded heights of a control or a row (the BOM's lines, the order table, the toolbar's fields, the tree's rows, the pills) now read those tokens, so the page's own scale is the one place to change.
- **Motion: Reduce** is `[data-motion="reduce"]`, the same rules as the system's request, and `reducedMotion()` for what script decides (the toast's exit, the splash's fade).

New tokens: `--pcb-thru-fill`, `--pcb-plane-1..4` (four values each, in the contrast tests), `--row-lg`, `--row-sm`, `--ui-scale`, `--icon-k`; new files `responsive.css`, `settings.css`, `splash.css`.

### Help and About

The question mark: Keyboard shortcuts, Command palette, Go to anything, `Report a problem` and About netsmith. **About** shows what the server reports and what the page is: the project's title, the boards, the files, whether KiCad was found, the analysis and its time, the connection, where the editor listens and the bundle (the file name of its script). **The server does not report the version of netsmith, the project's folder or where KiCad was found** (`/api/project` is `title`, `hasManifest`, `kicad: boolean`, `files`, `folders`): the dialog says "Not reported by the server" for the version and says why for the others. **Report a problem** copies a block (the page, the server, the analysis, the first problems, the last eight errors the page showed: a failure toast, `window.onerror`, an unhandled rejection; no path and no file text). **Links to the repository's docs are not there**: `docs/LANGUAGE.md` and the rest are files of netsmith's repository, not of the person's project, and the editor opens only the project's files and the standard library's.

### First run and brand

The tab is `netsmith - <board>` (the project's title without a board), a `●` in front while a file is unsaved. The favicon is a bolder net-anvil in an SVG whose orange follows the tab strip (`prefers-color-scheme` inside the file); the PNGs stay as fallbacks. The splash is the mark and one line (`Reading the project…`, then the analysis's own words and clock) over the window until `/api/project` answers, and `index.html` holds the same markup in `#app` so it shows before any script has run; it leaves with a 200 ms fade, and not at all when the server never answers. The empty project: a project whose only error is "has no `board` to build" is not an error to the person: every view says `No boards yet`, names `src/main.nsm` (or the file the checker names) and opens it, and the board picker says so in a neutral tone.

### The audit: 1000 x 700 and 1280 x 800, every section, dark and light (and 760 x 600 for the bars)

Found and fixed: the board's toolbar wrapped at 1000 (above); the BOM's right columns clipped without a cue (above); the power tree and the system view opened too small (above); at 1000 px the theme, the shortcuts, and now the gear and the help would have vanished with `.tb-ghosts`: the theme and shortcuts buttons go (the settings panel and the help menu hold both) and the gear and the help stay; the empty-project state (above); a status chip clipped at 125 % (the board toolbar's thresholds are rem now). Checked and fine: Source (the tree keeps 260 px and the code scrolls), Schematic (the module list and the inspector toggle, the overview's thumbnails), Build (one column), System and the dock at 1000 and 760 px, the status bar (keeps the board, the counts, the analysis and the connection), and every view in compact and at 125 % (a recipe: no horizontal overflow, the cards of the graphs overflow by what they did at 100 %).

### What round 5 leaves, in order of how much I would do it next

1. **The server could say more.** `/api/project` should carry `version` (netsmith's), `root` (the project's folder) and `kicadPath`, and the bundle's `SOURCE-HASH`, so About is complete; the plane rectangle of the board's SVG should carry `data-layer="In1.Cu"` so a plane's tone and name are exact and not by position.
2. At 760 px the Source section's tree (260 px) and the PCB's side panel stay open; below about 800 px they should start closed behind their toggle. The tree's collapse belongs to the sidebar work of round 4, so it is not touched here.
3. Size does not reach the source editor (round 4's setting does) or the text inside a graph card (a card's size is computed in script: it would need the metrics to read the scale).
4. A narrower top bar than 760 px was not designed: the board picker, the selection chip and the actions share what is left.
5. The dock's default height (240 px) is 40 % of a 600 px window; a window that short should start it lower.
6. The favicon was checked as a file and by the browser's own tab-strip colours, not on a real tab strip in a dark browser theme.
7. `scripts/editor-verify-js/css-rem.mjs` is a tool run by hand; a test that fails on a new px length above 2 in `style.css` and `views.css` would hold the page to rem without it. Not written: the existing 200-odd px values that stay (the drawings', the shadows') would need an allow-list the owner should look at.

## Night 2: round 4 and round 5 together, and the server's new fields in the page

Status: 2026-11-06. `integration/night2` is the night-1 integration plus `ui/modern-r4` (the Source editor), `ui/modern-r5` (the finishing pass), `server/compare` and `server/ui-fields`. The two UI rounds touched the same top bar, icons and entry file and merged by keeping both; `css-rem.mjs` was run again on the merged stylesheets (`style.css`, `views.css` and `themes.css` came out unchanged, `source.css`, a round 4 file, had 53 lengths converted so the settings' size reaches the breadcrumbs, the outline and the cards).

What the page now takes from the server, each with a fallback for a server that does not send it:

- **About and the diagnostics**: version, the commit it was built from, the project's folder, where KiCad was found and the hash of the embedded page; a line says whether the page on screen is the one the binary serves now (the file name of the script against the `index.html` the server answers with). The copied block holds the folder.
- **Planes**: a plane's tone is its layer's (`data-layer`), and its label says `GND · In1.Cu`; without the attribute, the place among the planes.
- **The outline and the breadcrumbs** read `Def.endLine` and `Def.children`; they use them when each name stands on the line the server gave and each definition ends on a closing brace, and otherwise (older server, text that changed since the file was read) the page's own reader (`source/symbols.ts`). The server's children are the declarations written directly in a body, so a declaration nested inside another declaration's block is not in the outline of a server's answer; the page's reader nests them.
- **Hover cards**: a definition's signature and doc comment come from `/api/definition`, its members from `Def.children` (no library file is read); the pins of a net and of a part show the datasheet's name and the chosen function; a part has a `Solved` block; a declaration in a module that several boards use, or that the board shown does not hold, offers the boards that use it (`Analysis.moduleUses`, joined by the module's file and line) and shows its card for the one picked (without the shown board's BOM line and rail, which belong to the board shown).
- **Ranges**: `Range.text` is the language's writing, `3.135V..3.465V`; every place the page shows one (the card's rail, the inline hint, the solved block) puts a thin space before the unit and round the dots (`spaceUnits`).
