# Selection, commands and the problems dock together

## Get to it

The two halves are one thing to the person: what is selected (top bar chip), what is wrong (dock under the view, counts in the status bar) and the keyboard (every key is a command in the palette and the cheat sheet). `Ctrl+J` shows the dock, `F8` and `Shift+F8` step through the problems, `Ctrl+P` then `!` lists them.

## How it is built

- `web/src/commandset.ts` owns every shortcut, the dock's too: `problems.toggle` (Toggle Problems, `Ctrl+J`), `problems.next` and `problems.previous` (`F8`, `Shift+F8`, also in the source editor), `problems.thisview` (Show problems of this view only), `problems.wholeproject`, and `goto.problem` (`!`). `App.tsx` has one key handler, which asks the registry; nothing else listens for these keys.
- `web/src/feedback/derive.ts`, `selectFor(row)`: a row selects through the shared selection (`selection.ts`): a problem inside a part's declaration selects that part (by instance path), a power issue on a rail selects the net, a PCB finding picks its parts on the board (`pcbstore.select`) and names the first of them in the shared selection. A row from the source or the power tree also asks the views to bring the selection into view (`selectAndReveal`).
- The PCB's findings are asked for once the dock is open or the PCB view has been shown (a board that cannot be placed answers 500, which is the PCB view's to say).
- `web/src/quickopen.ts`: the `!` mode lists the dock's rows (`problemItems`); choosing one does what clicking the row does.
- `style.css` has one token block at the top: the floating things (go to anything, menus, popovers, toasts) share `--pop`, `--pop-border` and the shadows; hover, info and warning strips are tokens too.

## Drive it

`scripts/editor-verify drive integration` (fixture `two_boards`, 1280 x 800, then 1920 x 1080): go to anything selects a part; Ctrl+J opens the dock; a mistake saved to disk inside the resistor's line gives a row; F8 opens the source at it and the chip names the same part as the line holds; fixing the file keeps the selection; the palette toggles the dock (Toggle Problems with its `Ctrl J` key caps), lists Next and Previous problem with `F8` and `Shift F8`, sets "Show problems of this view only" and back; the cheat sheet has a Problems group; `!nonsense` lists the problem and Enter goes to it. Look at `f8-source` (chip, strip, source cursor, dock row, status bar together) and `wide-with-dock`.

## Gotchas

- The editor in `launch` serves the committed bundle. To check source changes without rebuilding it (`scripts/web-bundle update` is the owner's), run `npx vite` in `crates/netsmith-editor/web` with `NETSMITH_EDITOR=http://127.0.0.1:<editor port>` and point the driver at it: set `url` in `$NETSMITH_VERIFY_DIR/state.json` to the Vite address.
- The chip's second line (`Resistor · r1`) is hidden below 1400 px; the name and the Source and clear buttons stay. For a net the line is "N pins on M modules" and stays at any width (`.selchip-sub.keep`), with ‹ › buttons while the Schematic is in use.
- Two problems of one line count twice: F8 steps through rows, not lines.
