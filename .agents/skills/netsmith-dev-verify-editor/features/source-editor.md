# Source editor and live diagnostics

## How to get to it

The section "Source" (the first, key 1): the project tree on the left, then tabs for open files, the CodeMirror editor, and the problems dock under the window (`problems-dock.md`; Ctrl+J or the counts in the status bar open it). The status bar and the dock are in every section. After a click-through from another section the file tabs begin with a "Back to ..." button.

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

## Gotchas

- Typing fast used to be undone by a stale re-render; the editor keeps typing separate from text replaced from outside (`rev` in the store). If text vanishes while typing, look there.
- `Ctrl+S` is handled by CodeMirror and by the page; a second save at once is dropped, and a stale stamp gives a 409 with a "reload" banner.
- A squiggle is only in the DOM for lines that are on screen.
- Diagnostics from a check carry the version of the text they were made for: only the answer to the latest check is shown.
