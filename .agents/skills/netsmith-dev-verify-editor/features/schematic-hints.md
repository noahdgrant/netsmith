# The schematic layout file (stage 3: the server; no page UI yet)

## How to get to it

There is nothing to click yet: stage 4 of ADR 0061 adds the drag. What exists is the server: it reads `src/main.sch.toml` beside the top-level file (`<stem>.sch.toml`), draws the schematic with it, puts what is wrong with it in the Problems list and serves `GET` and `PUT /api/schematic-hints?board=`. The Schematic section shows the result: a board whose file moves a part is drawn with it.

## Driving it

`scripts/editor-verify drive schematic-hints` (fixture `two_boards`). The recipe drives the API with `fetch` from the page and writes the file as another program would:

1. No file: `GET` says `exists: false`, `stamp: "none"`, no faults, no findings, no tables; no problem of stage `layout`.
2. `PUT` of a text with a comment and loose spacing: 200, the file on disk is the canonical text (`version = 1`, a blank line, the table, `25.4`), the answer is the same as a `GET`, the drawing from `/api/schematic.svg` changed and the page redraws without a reload (the connector is further from the resistor on the page).
3. A file written by hand with an entry for a part that is not there and a table for a board the source lacks: two notes of stage `layout` in `/api/analysis`, each at `src/main.sch.toml` and the line of its table header; the board's own finding has the board, the unknown table has none; the drawing is not changed.
4. A file that does not read (an unknown key): one warning at its line that says the file is ignored; the project still builds (no error), the drawing is byte for byte the one without a file; the dock (Ctrl+J) shows the row with the stage `Layout` and `main.sch.toml:6`.
5. Refused writes: no `X-Netsmith` is 403; a text that does not read is 422 with `code: schematic_hints_invalid`, `file` and `line` in the JSON body; a stale `X-Stamp` is 409; an unknown board is 404. The file is not touched by any.
6. Deleting the file brings the first drawing back, byte for byte, and the page redraws.

## Gotchas

- A request that is refused is made from the recipe's own `fetch` (Node), not the page's: the browser logs a failed request as a console error, and a console error fails a recipe.
- The file is watched with the sources, so an edit by another program waits for the server's next look (up to half a second) and then analyses the whole project once: the drawing is made by the analysis. A `PUT` analyses before it answers, so its answer and the next `GET /api/schematic.svg` are of the file just written; the watcher then finds nothing new.
- A comment in the file is lost when the editor writes it (ADR 0061).
- The dock lists errors, then warnings; the notes (an entry that matches nothing) show only when the notes toggle is on. A fault of the file's text is a warning for that reason.
- The location of a problem is `src/main.sch.toml:N`. The file is not in the project tree, so the page cannot open it yet (stage 4).
