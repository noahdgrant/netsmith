# Problems dock, status bar, stale views, toasts

## How to get to it

The dock is under the window in every section: `Ctrl+J` (`Cmd+J`, the command Toggle Problems) shows or hides it, and so do the error and warning counts at the left of the status bar. Its height (drag the line on its top edge, arrows when the line has the focus, double-click for the usual height) and whether it is open are kept in the browser. `F8` and `Shift+F8` (the commands Next problem and Previous problem; all three are in the palette and the cheat sheet, registered in `commandset.ts`) go to the next and previous problem from anywhere, also with the dock closed. `Ctrl+P` and `!` lists the problems; "Show problems of this view only" is a palette command too.

## Driving it

`scripts/editor-verify drive problems-dock` (fixture `two_boards`); `broken-start` and `editor` cover the same strip.

1. Open the Schematic and press `Ctrl+J`: the dock says "No problems" with a check.
2. Write a mistake to `src/main.nsm` on disk: a row with the compiler's message, its stage (Syntax, Checker, Solver, Lowering, Guideline, as the server says in `Problem.stage`; for PCB findings Placement, Routing, Design rules, KiCad; Power) and `file:line`; the counts in the status bar; above the drawing a strip "Showing last good version" with the first error, its line and "Go to error"; the drawing dimmed; the status bar says "Last good version".
3. `F8` from the Schematic: the Source opens at the mistake, the row is marked.
4. Drag the dock's top line: the height is kept after a reload.
5. Fix the file on disk: the strip goes, the dock is empty again.

What else the dock lists: the PCB's findings of the board (`/api/pcb/findings`, asked for a moment after each analysis once the dock is open or the PCB has been shown; a click selects the part and zooms the PCB view to the finding), and the power tree's issues (an overdrawn supply is an error; the rest are notes). "This view only" keeps what the section on screen shows (the source: its open file; the PCB: its findings). Notes are hidden until their toggle is pressed.

The status bar, left to right: the board shown (click to choose another), the counts, the analysis state ("Analysing… 12 s · Solving Beta, 1 of 1" with a spinner after the first moment: the clock is the server's own start, so an edit made outside the editor shows it, and the stage's own count, never a percentage of the whole; then "Analysed in 46 s" for twelve seconds, with the stages in its tooltip; "Last good version" when the drawing is old; "Analysed" otherwise), a running job, unsaved files; at the right, only while a source is on screen, the cursor (click: go to a line) and the language, then the connection (Live, Reconnecting… after 1.5 s without a socket, Offline after 10 s; click for details). A thin line under the tab strip moves while the analysis has run for a second and while a job runs; it does not show a fraction.

The dock's head has a chip for each stage that has rows (Syntax, Checker, Solver, Lowering, Guideline, then Power and the PCB's): pressed, its rows show, and the count is of all its rows. The Stage column and the chips read `Problem.stage`, the Module column `path` (else `board`); a power issue and a PCB finding have `at`, and a click shows that line in the source (a PCB finding's quietly: the board stays in front).

The first load: the page and the tree paint at once, the first analysis request sends `X-Netsmith-Pending: 1` and the `503` it gets shows the skeleton with "Analysing the project… N s" (and the stage); the page takes the analysis from the socket's `rebuilt` with no reload (a safety request every 30 s with the socket open, 1, 2, 4, 5 s without it: each failed request is a line in the browser's console, so a recipe that must be quiet starts after `launch`, which waits for the analysis). `launch` over a project that analyses for half a minute is the way to see it; so is a copy of a big private project run with `netsmith editor` by hand.

A finished build, fab, order or route is a toast with the result and its cause when it failed, and a button to go on (download the Gerbers, open the Build or PCB section when it is not already on screen). A failure stays until it is dismissed.

## Gotchas

- Every section that has been shown is kept in the DOM, so `.stale-strip` exists once for each of them: scope it (`.section:not(.away) .stale-strip`).
- The analysis state needs a slow analysis to be seen: a small project is done in less than a second and the status bar shows nothing. A page that is driven can call `track` of `src/feedback/state.ts` to make the state show (import the module by the URL the page loaded it from).
- The stage, board and instance of a problem are the server's (`Problem.stage`, `.board`, `.path`); `classify`, `splitBoard` and `instancePath` in `src/feedback/problems.ts` read them from the message only for a problem with no stage (a server older than the fields).
