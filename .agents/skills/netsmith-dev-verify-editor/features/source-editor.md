# Source editor and live diagnostics

## How to get to it

The section "Source" (the first, key 1): the project tree on the left with the file's outline below it, then tabs for open files, a bar of Back and Forward, breadcrumbs and switches, the CodeMirror editor, and the problems dock under the window (`problems-dock.md`; Ctrl+J or the counts in the status bar open it). The status bar and the dock are in every section. After a click-through from another section the file tabs begin with a "Back to ..." button.

## Driving it

`scripts/editor-verify drive editor`

1. Click a definition in the tree to scroll to `LedBoard`, put the cursor at the start of `.r1: Resistor {}` and type `.oops: Missing` and a newline. Within about a second a red squiggle appears, the status bar shows one error, and the problems dock (Ctrl+J) shows the compiler's own message (`unknown type `Missing``), its stage (Checker) and `src/main.nsm:88`. The file on disk is unchanged: the check is in memory.
2. Click the problem: the cursor goes to that line. Undo (`Ctrl+Z`): the squiggle goes.
3. Type a comment at the top and press `Ctrl+S`: the dot on the tab goes and the file on disk starts with the comment.
4. `Ctrl+click` on `Resistor` in a declaration: a tab for `std/parts/passive.nsm` opens at `part Resistor`.
5. Change the file on disk from outside: the editor shows the new text within a second (it has no unsaved edits). Write a mistake to disk: the squiggle, the count and a strip on the schematic ("Showing last good version", the first error, Go to error) appear; fix it on disk and they go.

### File languages: `drive languages` after `launch --fixture catalog_refresh`

The colouring of a file is chosen by its extension (`src/languages.ts`, `languageOfPath`): `.nsm` and `.physical.nsm` get the netsmith language, `.toml` (the manifest) a small TOML colouring (tables, keys, strings, numbers, comments), anything else plain text. Open `netsmith.toml`: several colours, no red wavy underline, no lint mark (the checker has no problem there). A token the `.nsm` colouring cannot read is drawn red and wavy only in an `.nsm` file, and hovering it shows a note: the checker's own message when it has a problem at that place, else "unexpected `]`: the compiler reports no error here; this is the editor's syntax colouring". The recipe types a stray `]` at the end of `main.nsm` and hovers it. Real problems with a location in the manifest still go through the lint diagnostics. The tree lists only `.nsm` files and the manifest, so a catalog's `.csv` cannot be opened in the editor; the language choice for it is covered by `src/languages.test.ts`.

### Errors, warnings and notes: `drive big-problems` after `launch --fixture big`

The problems dock lists errors and warnings; notes (what the solver left alone: a relation not evaluated, a balance that is incomplete) are hidden until the Notes toggle in its header is pressed, and the toggle carries their count. Each note goes to the line of the instance it is about, in its own file. The status bar's counts and the dock's are errors and warnings only; notes get no squiggle. More in `problems-dock.md`.

### Hover cards, breadcrumbs, history, outline, hints, tabs: `drive source-editor` (fixture `two_boards`)

The recipe puts the board `TwoBoards/led` in front, reads `/api/board` for the facts it checks (the designator of the part at `r1`, its nets, the pins of `3V3`) and never names a designator.

1. **Hover card.** Rest the pointer on `r1` of `.r1: Resistor {}` (250 ms): a card (`[data-testid="hover-card"][data-kind="part"]`) with the designator, value and kind, the package, the catalog line, the pins as chips (`.hc-pin`, one for each net the part is on) and a `Show on` row (Schematic, PCB, BOM). A chip lights its net (`html.has-hover`) and picks it (the selection chip names it). Move off the token: the card goes. On `"3V3"` (a string that names a net): `data-kind="net"` with the pin count and, once the power tree is read, `Voltage`. On the type `Resistor`: `data-kind="definition"` with the signature and the doc comment of `/api/definition` (the page asks for the name at the cursor's text; `source/boarddata.ts`, once for each analysis; the library file is read through `/api/file` only when the server has nothing to say) and its members (`Def.children` of the file list; the page reads the text only for a server that sends no `endLine`). A part's card has a `Solved` block (`[data-testid=hc-solved]`: what a capacitor is worth in use at its bias, the tolerance, the ratings, the catalog row) when `PartInfo.solved` is there, and a pin chip shows the datasheet's pin name (`.hc-pname`) and the chosen function (`.hc-pfn`). A declaration inside a module that several boards use (or that the board shown does not hold) has a `Used by` row of board chips (`[data-testid=hc-boards]`, from `Analysis.moduleUses` by the file and line of the module): a chip makes the card for that board (`data-board`), and `Show it in the views` switches the board. With the power board shown, on `.stage: LdoStage`: `data-kind="module"` and its three parts. `Ctrl+Alt+I` opens the card at the cursor, Escape closes it. The palette's `Hide hover cards` and `Show hover cards` switch them.
2. **Breadcrumbs** (`[data-testid="breadcrumbs"]`, crumbs are `.bc-crumb`): `main.nsm > LedBoard > r1` at the cursor. A click on the definition's crumb opens `[data-testid="crumb-menu"]` with every definition of the file, the current one marked; Arrow keys and Enter go to one (the cursor moves, the crumbs follow).
3. **Outline** (`[data-testid=outline][data-source]`: `server` when the symbols are `Def.endLine` and `Def.children` of the file list and they fit the text, `text` when the page read the text itself (older server, unsaved text); `.ol-row`, with `data-name`, `data-kind`, `data-line`): a row for each definition and declaration; a click puts the cursor on the name and `.ol-row.active` follows the cursor; the filter keeps what matches and its parents; a problem in the file puts a badge (`.ft-problems`) on its entry.
4. **Gutter and lint card.** A mistake shows the page's error icon in the gutter (`[data-testid="gutter-error"]`) and, on the squiggle, `[data-testid="lint-card"]`: severity, the compiler's message, the stage chip (`Checker`), `netsmith src/main.nsm:91` and the actions.
5. **Completion.** After `.oops: Re` the list has `Resistor` and a part icon (`.cm-comp-icon.part`).
6. **History.** Three clicks in the tree go somewhere; `[data-testid="history-back"]` and `Alt+Left` walk back, `[data-testid="history-forward"]` and `Alt+Right` forward, Forward is off at the newest place.
7. **Inline hints** (`[data-testid="inline-hint"]`): the value, package and LCSC number at the end of a part's line and the voltage range at the end of a `.net &= "..."` line. Select all and copy: the clipboard is the file, exactly, without the hints; a token's `left` is the same with the hints on and off. The palette's `Hide inline hints` and the status bar's `Source options` menu (`.source-opts-item`, `[data-testid="opt-hints"]`) switch them.
8. **Folds** (`.cm-fold-mark`, shown when the pointer is on the gutter), `Fold all` and `Unfold all` from the palette, a folded block is a chip `N lines`. **Wrap** (`Alt+Z`, class `cm-lineWrapping`), **text size** (`Ctrl+=` two steps, `Ctrl+-`, `Ctrl+0`) which a reload keeps.
9. **Find and replace** (`.sp`): `Ctrl+F` and a word give a count (`2 found`), `Ctrl+H` shows the replace row, Escape closes the card.
10. **Go to symbol in the file.** `Ctrl+Shift+O` with the editor focused lists `In this file`; typing `r1` and Enter puts the cursor on `.r1`. With the keys anywhere else it lists the project's modules and definitions.
11. **A library file.** `Ctrl+click` on `Resistor`: `std/parts/passive.nsm` in a tab with a lock (`.tab-lock`), `[data-testid="readonly-banner"]` ("Read-only: the standard library") and breadcrumbs `passive.nsm > Resistor`.
12. **Tabs.** An unsaved tab shows `.dot` (the close button has opacity 0); under the pointer the dot is 0 and the close button 1. A drag of one tab onto another reorders them (`.tab[data-path]`). A middle click closes. With eight files open in a 900 px window the strip has `[data-testid="tab-more"]`; its menu (`[data-testid="tab-menu"]`) lists them all and a choice shows the file.

What a screenshot should show: the card at 12 px radius over the code, never cut by the window's edge; the hints muted, in the sans face, a pill a step from the canvas; the cursor's line with a thin accent bar at the left edge of the gutter; guides one level apart; the find card at the top right with the replace row under it.

## Gotchas

- A hover card is drawn a microtask after CodeMirror makes it (a render inside CodeMirror's update, which can be inside the render of the page, turns Preact's hooks off): a recipe waits for the card with `waitForSelector`, never for a fixed time, and the tooltip is measured again when its content arrives.
- The fold marks are not visible until the pointer is on the gutter (opacity 0), and the gutter has a hidden spacer of its own: a recipe waits for `.cm-fold-mark:not(.folded)`.
- `.crumb` is a class of the schematic's sheet bar; the source's breadcrumbs are `.bc-crumb`. There are two `[role=tree]` in the Source section (the files, `.ft-tree`, and the outline): a recipe that counts trees says which.
- Hints are decorations of the line: `.cm-line`'s `textContent` includes them. A recipe that matches a line by text uses a substring (`hasText`), never equality.
- The decorations of the hints and the folds exist only for lines on screen: scroll to a line (a click on its entry in the tree or the outline) before measuring it.
- A file with unsaved edits keeps the hints where they were (moved with the text) until the file is saved and the board is built again.
- The list of problems and the outline's badges count errors and warnings; a note is not drawn in the source.
- Typing fast used to be undone by a stale re-render; the editor keeps typing separate from text replaced from outside (`rev` in the store). If text vanishes while typing, look there.
- `Ctrl+S` is handled by CodeMirror and by the page; a second save at once is dropped, and a stale stamp gives a 409 with a "reload" banner.
- A squiggle is only in the DOM for lines that are on screen.
- Diagnostics from a check carry the version of the text they were made for: only the answer to the latest check is shown.

