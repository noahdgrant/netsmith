# Editor features and how to verify each

Run `scripts/editor-verify doctor`, then `launch`, then the recipe named in each page. Each page has the same three sections: how to get to the feature, how to drive it (steps and what you should see), and gotchas.

| Feature | Page | Recipe | Fixture |
|---|---|---|---|
| First look: the sections, the schematic, status bar | `overview` below | `overview` | `two_boards` |
| Sections, number keys, one pane or two, the divider, drag-to-split, narrow window | `layout.md` | `sections`, `split` | `two_boards` |
| Project tree | `project-tree.md` | `tree`, `big-tree` | `two_boards`, `big` |
| Project tree: nesting, guide lines, icons, keyboard, filter, collapse all, width, click a definition, scroll kept | `project-tree.md` | `tree-nested` | `tree` |
| Project tree: new file and folder, rename, delete (toolbar, right-click menu, F2, Delete, Ctrl+N) | `project-tree.md` | `tree-files` | `tree` |
| Source editor and live diagnostics | `source-editor.md` | `editor` | `two_boards` |
| Source: Ctrl+/ comment, autosave, comment font, Vim keys, sort imports, rebuild | `source-editor.md` | `source-keys` | `two_boards` |
| Source: the cursor and scroll position of each file kept | `source-editor.md` | `source-position` | `two_boards` |
| Source: a plain comment is not analysed again | `source-editor.md` | `comment-only` | `two_boards` |
| Source: hover cards, breadcrumbs, history, outline, inline hints, tabs, folds, find, completion | `source-editor.md` | `source-editor` | `two_boards` |
| Source: a part's card says which statement gave its package (a line of the board's `defaults`, a `?=`, a `&=`) and goes there; a `defaults` line that reaches nothing is a note | `source-editor.md` | `board-defaults` | `board_defaults` |
| Schematic view and click-through | `schematic.md` | `schematic`, `big-schematic` | `two_boards`, `big` |
| Schematic box of a part with a `tri_state` pin (the side it is on, the mark of an open one) | `schematic.md` | `tri-state` | `tri_state` |
| Pin map of the selected part (pin, pad, function, lock, where it came from, the check's message for a request with no pin) and the pick lock's state and diff, read only; the lock file is watched | `pin-map.md` | `pin-map` | `pin_map` |
| Padlock and move of a pin request write the pin file's `lock`, `pin` and `source` lines only; refusals with the server's words | `pin-map.md` | `pin-lock` | `pin_map` |
| Schematic layout file (server side): hints read and drawn, findings in the Problems list at the file and line, `GET`/`PUT /api/schematic-hints`, the watcher | `schematic-hints.md` | `schematic-hints` | `two_boards` |
| Arranging the schematic by hand: drag on the grid with a ghost, Alt pins, Esc cancels, the anchor is inert, a refused drop is put back; arrows, `r`, `m`, Backspace; clear a module or the file; the layout file in the tree and in Source | `schematic-layout.md` | `layout-drag`, `layout-keys`, `layout-clear` | `two_boards` |
| Schematic and the shared selection: go to anything lands on the module at a legible zoom with a ring, a net picked in the drawing, a flag followed, Esc clears everywhere | `schematic-integration.md` | `schematic-integration` | `schematic_bench` |
| Power tree | `power-tree.md` | `power`, `single-board`, `big-power` | `two_boards`, `regulator`, `big` |
| Power tree of an invented board: layout, collapsed loads, path from the source, keys, hover, issues | `power-tree.md` | `power-tree`, `power-issues` | `power_tree` |
| Files coloured by extension (TOML, plain text, .nsm), the note on an invalid-token mark | `source-editor.md` | `languages` | `catalog_refresh` |
| System view | `system-view.md` | `system`, `single-board` | `two_boards`, `regulator` |
| System view of an invented board: layers, ports tables, grouped wires, labels, selection | `system-view.md` | `system-view` | `system_view` |
| System view: mates and ground groups | `system-view.md` | `system-mates` | `carrier_and_cards` |
| Bill of materials | `bom.md` | `bom` | `two_boards` |
| BOM dates, stale mark, Check parts against LCSC (fake endpoint) | `bom.md` | `refresh` | `catalog_refresh` |
| PCB view | `pcb.md` | `pcb`, `four-layer`, `big-pcb` | `two_boards`, `four_layer`, `big` |
| PCB canvas: select, hover, move, rotate, flip, lock, undo | `pcb.md` | `pcb-select`, `pcb-edit` | `outline_and_pours` |
| PCB keepouts: draw, move, resize, delete | `pcb.md` | `pcb-keepout` | `outline_and_pours` |
| PCB plane labels with no room: the chip `+N` pinned to the canvas, a button with a list, keys, zoom to the plane, in four themes | `pcb.md` | `pcb-plane-more` | `four_layer` |
| PCB in 3D: the toggle loads the chunk on demand, the coverage chip and its list, the legend, hover, click to select, a pick from the side panel, preset views, flip, orthographic, light theme | `pcb-3d.md` | `pcb-3d` | `height_limits` |
| `netsmith view`: the 3D view of the open page driven from the command line, its summary and JSON, a picture, refusals, no editor | `pcb-3d.md` | `pcb-3d-view-cli` | `height_limits` |
| PCB findings: list, zoom to, re-check with KiCad | `pcb.md` | `pcb-findings` | `outline_and_pours` |
| PCB constraint findings: the requirement's numbers, zoom, placer clears it | `pcb.md` | `pcb-constraints` | `constrained` |
| Panel preview (V-cut grid of one board) | `pcb.md` | `panel` | `led_panel` |
| The Schematic's keys as commands: palette, cheat sheet group, the `?` button, keys work, not in a text field, not away from the view | `schematic.md` | `schematic-keys` | `schematic_bench` |
| Busses: an interface drawn as one line, picked with its members lit, the chip, the inspector's members and pins, a bundle flag followed to the module that carries the nets, `#` in go to anything | `schematic-bus.md` | `schematic-bus` | `bus_bench` |
| Reveal after a board switch waits for the new board's drawing; dropped when the board changes | `schematic-integration.md` | `schematic-board-switch` | `two_boards` |
| One selection in every view, go to anything (Ctrl+P), command palette (Ctrl+Shift+P, Ctrl+K), keyboard cheat sheet (?) | `palette.md` | `palette` | `two_boards` |
| Tables that fill the page and a help column in the margin: BOM, Compare, Build at 1920 px; folded and kept; absent without the margin | `bom.md` | `view-help` (and `compare`) | `two_boards` |
| The keyboard and screen reader audit: landmarks, names, Tab order, layers closed topmost first, the focus given back | `layout.md` | `a11y` | `two_boards` |
| Cancelling the analysis (status bar, Run again), a comparison (card, Escape), a build and a fab (card) | `cancel.md` | `cancel`, `cancel-jobs` | `big`, `two_boards` (`launch --git`) |
| Build panel (build, fab, downloads) | `build-panel.md` | `build` | `two_boards` |
| Orders (list, run, summary, downloads) | `build-panel.md` | `order` | `two_boards` |
| Problems: errors, warnings, notes | `source-editor.md` | `editor`, `big-problems` | `two_boards`, `big` |
| Problems dock in every view, F8, the strip over a stale view, status bar, toasts | `problems-dock.md` | `problems-dock`, `broken-start` | `two_boards` |
| Problems dock leftovers: the place of a row is a button (Tab, Enter), the picked row stays in view, grouping by stage in a fixed order, a count for each severity in the head and the status bar; notes grouped by cause (the default) with a count and a collapsible head | `problems-dock.md` | `problems-leftovers` | `warn_bench` |
| The warning triangle everywhere (tree, tab, outline, status bar, dock, gutter, ruler): one drawing, the `--warning` colour in four themes, a name that says warning; the same for an error | `problems-dock.md` | `warning-marks` | `warn_bench` |
| The selection, the command registry and the dock together: Ctrl+J, F8, `!` in go to anything, the chip and the dock row agree | `integration.md` | `integration` | `two_boards` |
| Shared hover: a BOM row, a chip, a card, a drawn part or net lights the same thing in the other views; Escape, leave, focus, touch | `hover.md` | `hover` | `two_boards` |
| Motion: the glide, the fade, the pop, the dock, the toast in and out, a row's ease; reduced motion | `motion.md` | `motion` | `two_boards` |
| Themes (light, dark, high contrast, system), type and buttons, the keyboard, 200 % zoom, reduced motion; every view in every theme | `themes.md` | `themes` | `two_boards` |
| Settings (density, size, motion), the help menu, About, the diagnostics copy, the tab title, the favicon, the splash | `settings.md` | `settings` | `two_boards` |
| The top bar, the section tabs and the status bar never overlap or leave the window: widths 760 to 2560, sizes 90 to 125 %, both densities, with nothing picked, a part and a net; the selection chip keeps its words while the bar has room | `settings.md` | `topbar-layout` | `two_boards` |
| A project with no board: the empty state and the button that opens `src/main.nsm`; and, as it is no repository, the Compare section's reason, the palette command with no picker, the splash | `settings.md`, `compare.md` | `empty-project` | `empty` |
| The comparison beside the rest: the status bar's chip and what gives way at 1920/1280/1000/800 px, About and the settings over the chip, a mark with the hover and the pick on one element (schematic, board, power tree), the diff view and the source's Escape, hover card and outline, 125 % and compact on Compare, the key 8 in the palette and the cheat sheet | `compare.md` | `compare-interactions` | `two_boards` (with `--git`) |
| Two refs against each other: the picker's `Base` and `Against` slots, the working tree as the default second side, the answer that reads no file on disk (no notes, no dirty notice), the views left unmarked with their switch off, `Head` opening the file at both refs, a second ref that does not analyse, the way back (needs `launch --git`) | `compare.md` | `compare-refs` | `two_boards` (with `--git`) |
| The comparison beyond one board's circuit: the Systems and PCB tiles and tabs, a port that changed in a system, a part moved in the layout file (with where it was and is), the layout file opened at the ref and on disk (needs `launch --git`) | `compare.md` | `compare-system-pcb` | `two_boards` (with `--git`) |
| The comparison's kind of a part (a header that became a socket) and its warnings and notes by severity, between a ref and the work tree and between two refs, with the severity filter of the Problems tab (needs `launch --git`) | `compare.md` | `compare-kind-warnings` | `two_boards` (with `--git`) |
| Compare with a git ref: the ref picker, summary, tiles, tabs, click-through, Base and Head, the read-only comparison of a file, marks on the Schematic, BOM, Power tree, System and Source, a ref that does not analyse, progress, not a repository (needs `launch --git`) | `compare.md` | `compare` | `two_boards` (with `--git`) |

`overview`: `drive overview` opens the page and expects the eight section tabs in a row (Source first, Compare last), a schematic (`.schematic`), the actions in the top bar and the error and warning counts in the status bar, then opens the Source section with key 1. Look at the screenshot for the overall layout.

A section other than the shown one is hidden (`.section.away`, `visibility: hidden`) and the source is always in the page, so a recipe opens a section first: the driver gives it `section('source')` (one of `schematic`, `power`, `system`, `bom`, `pcb`, `build`, `source`), which clicks the tab in the first pane and waits for it. Scope a locator that two sections share (`.graph-host`, `.zoombar`) to its section (`.sec-power .graph-host`).

The `big` fixture (`launch --fixture big`) is generated by `scripts/editor-verify-js/fixtures/big.mjs`: a board of 89 parts in 24 module instances (8 regulator stages, 8 indicators, 8 resistor banks) in three module files under `src/sheets/power` and `src/sheets/io`. It has no errors, 64 notes (relations the solver left alone) and no warnings. Every recipe names its fixture (`export const fixture`), and `drive all` skips the ones for another. The `big-*` recipes need it and are skipped by `drive all` on another fixture. For a real project of this size, copy it to `/tmp` and run `netsmith editor` in the copy: look, do not write.

When you add a feature, add a page here and a recipe, in the same change.

`compare`, `compare-refs`, `compare-system-pcb`, `compare-kind-warnings`: `launch --git` first (a throwaway git repository over the `two_boards` project; see `compare.md`); `drive all` skips it in a session launched without.

`problems-leftovers` and `warning-marks`: `launch --fixture warn_bench` first (an invented board with two guideline warnings; see `problems-dock.md`). `pcb-plane-more`: `launch --fixture four_layer`.

`schematic-bus`: `launch --fixture bus_bench` first (an invented board: a UART between three modules and an SPI inside one; see `schematic-bus.md`).

`schematic-rules`, `schematic-shots` and `schematic-integration`: `launch --fixture schematic_bench` first; the first asserts the drawing rules of the schematic on the four modules of `examples/schematic_bench` (see `schematic.md`), the second takes the modules' pictures at two window sizes, the third drives the schematic with the shared selection (`schematic-integration.md`).
