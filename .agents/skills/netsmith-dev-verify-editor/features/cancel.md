# Cancelling long work (ADR 0062)

## How it looks

- **The analysis.** While the project is analysed the status bar has the item `Analysing… 12 s · Solving Beta: 41 of 118 parts chosen` (`.statusbar .analysing`; the count inside the board is what the solver has chosen, with its total, never a percentage) and, beside it, a `Cancel` button (`.cancel-analysis`). A press makes the item say `Cancelling…` until the server says it stopped (a fraction of a second). Then the bar shows `Analysis cancelled · Run again` (`.item.cancelled`), the views go on showing the reading before the cancelled analysis, and nothing starts it again until a file changes or the note is pressed. A first analysis that was cancelled leaves an empty project with the note `The first analysis was cancelled…` in the problems and the same item.
- **A comparison.** Its progress card (`[data-testid=cmp-progress]`) has `Cancel` (`[data-testid=cmp-cancel]`), and Escape does the same. The section then says `Comparison cancelled` (`[data-testid=cmp-cancelled]`) with `Compare again`. A page that closes or reloads while a comparison runs cancels it too (the server sees the connection drop).
- **Build, fab, order.** The card's progress line (`.job`) has `Cancel` (`[data-testid=cancel-build|cancel-fab|cancel-order]`). A cancelled job is a calm toast (`Fab was cancelled. Nothing of it was kept.`), not a failure. The packages of the boards a fab finished before are kept; nothing of the board it was on. (The PCB view's `Stop` for the router is another thing: it keeps what was routed.)

## How to get to it

Change a file of a big project (the `big` fixture, or any board that takes seconds to analyse): the analysis item appears after 0.3 s. A comparison: key 8, pick a ref whose project is big. A job: key 7, start it.

## Driving it

`scripts/editor-verify launch --fixture big`, then `scripts/editor-verify drive cancel`. It writes the `big` board with forty rails to `src/main.nsm` (the solver works at it for tens of seconds in a debug build), reads the solver's count in the bar, presses Cancel, and checks that the old drawing stays, that `/api/analysis` says `cancelled: true` with the version it had, that the watcher does not start the analysis again, that the next edit analyses again, and that `Run again` starts it.

`scripts/editor-verify launch --git` (the `two_boards` project in a throwaway repository), then `scripts/editor-verify drive cancel-jobs`. It makes a branch `bigref` whose project is the big board (with git plumbing and a scratch index, so the work tree is not touched), compares with it, cancels from the card and with Escape, compares with `main` after, holds a build for a moment to see its Cancel button, and cancels a real fab.

## Gotchas

- The routes (`POST /api/analysis/cancel`, `/api/analysis/run`, `/api/compare/cancel`, `/api/build/cancel`, `/api/fab/cancel`, `/api/order/cancel`) answer `{cancelled: bool}` (`run` answers the reading) and need the `X-Netsmith` header. A cancelled job's own request answers `499`.
- A recipe that cancels in the middle of a stage depends on the stage being long. The recipes use forty rails so that a release build is still slow enough; a faster machine needs more.
- The status bar item is a sibling of the analysis item, not inside it (a button in a button is not valid).
