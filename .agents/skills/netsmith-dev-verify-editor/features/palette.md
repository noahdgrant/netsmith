# One selection, go to anything, the command palette and the cheat sheet

## Get to it

Open the page. The top bar has "Go to anything" (Ctrl+P, Cmd+P on a Mac), which also opens by Ctrl+Shift+P and Ctrl+K as the command palette; `?` opens the keyboard cheat sheet. The selection chip (kind, name, "Source", ×) is in the top bar whenever something is selected.

## How it is built

- `web/src/selection.ts`: the shared selection `{kind: part | net | module, key}`. The key is the instance path of a part or a module and the name of a net, never a designator. Every view reads `selection` and writes it (`setSelection`, `pickPart`, `pickNet`); none keeps its own. The Schematic marks it, the PCB selects the part (and lights the net), the Power tree and System select the card, the BOM lights the rows, and the top bar chip shows it.
- `web/src/commands.ts` and `commandset.ts`: the command registry (id, title, key chord, `run`, `when`). The palette, the cheat sheet, the top bar buttons and the keyboard all read it. A new command with `keys` shows in the cheat sheet by itself.
- `web/src/quickopen.ts`, `fuzzy.ts`: the index (parts, nets, modules, definitions, files of the shown board and the project) and the matcher. Prefix modes: `>` commands, `@` modules and definitions, `#` nets, `:` a line of the open file, `!` the problems of the dock.

## Drive it

`scripts/editor-verify drive palette` (fixture `two_boards`): Ctrl+P and the designator, Enter, the chip and the schematic mark; the same selection in the BOM, the PCB and the Power tree; `#` for a net; Ctrl+Shift+P and Ctrl+K; recently used first; the last query back; `?`. Look at `goto-part`, `selected-pcb`, `selected-net`, `commands` and `cheat-sheet`.

## Gotchas

- The palette finds parts of the board shown in the top bar's Board; the files and definitions are the whole project's.
- A plain key (`1`..`7`, `?`) is ignored while the focus is in a field or the source editor; a chord with Ctrl or Alt works there.
- Choosing a part while the Source is not in front moves the Source cursor without bringing it forward (`goTo(..., quiet)`), so it is at the declaration when you look; the keys stay where they were.
- `pcb-select` clicks "empty board" at a point where the fixture's connector sits at present and fails on a bundle without these changes too.
