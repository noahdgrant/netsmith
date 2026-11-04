# Source editor and live diagnostics

## How to get to it

The section "Source" (the last, key 7): the project tree on the left, then tabs for open files, the CodeMirror editor, and a Problems panel under it (click its header or the counts in the status bar to open it). The status bar is in every section. After a click-through from another section the file tabs begin with a "Back to ..." button.

## Driving it

`scripts/editor-verify drive editor`

1. Click a definition in the tree to scroll to `LedBoard`, put the cursor at the start of `.r1: Resistor {}` and type `.oops: Missing` and a newline. Within about a second a red squiggle appears, the status bar shows one error, and the Problems list shows the compiler's own message (`unknown type `Missing``) with `src/main.nsm:88:12`. The file on disk is unchanged: the check is in memory.
2. Click the problem: the cursor goes to that line. Undo (`Ctrl+Z`): the squiggle goes.
3. Type a comment at the top and press `Ctrl+S`: the dot on the tab goes and the file on disk starts with the comment.
4. `Ctrl+click` on `Resistor` in a declaration: a tab for `std/parts/passive.nsm` opens at `part Resistor`.
5. Change the file on disk from outside: the editor shows the new text within a second (it has no unsaved edits). Write a mistake to disk: the squiggle, the count and a "last drawing that built" banner on the schematic (open the Schematic section to see it) appear; fix it on disk and they go.

### Errors, warnings and notes: `drive big-problems` after `launch --fixture big`

The Problems panel lists Errors, then Warnings (a hole in the design: a part not chosen), then a Notes section that is closed until clicked, with its count: what the solver left alone (a relation not evaluated, a balance that is incomplete). Each note goes to the line of the instance it is about, in its own file. The status bar and the header's count are errors and warnings only; notes get no squiggle.

## Gotchas

- Typing fast used to be undone by a stale re-render; the editor keeps typing separate from text replaced from outside (`rev` in the store). If text vanishes while typing, look there.
- `Ctrl+S` is handled by CodeMirror and by the page; a second save at once is dropped, and a stale stamp gives a 409 with a "reload" banner.
- A squiggle is only in the DOM for lines that are on screen.
- Diagnostics from a check carry the version of the text they were made for: only the answer to the latest check is shown.
