---
status: proposed (the mechanism, what a cancelled job leaves, the routes and the progress units are written down for review; the owner reviews the choice of unwinding over a `Result` through the solver, and that a cancelled analysis keeps the last reading)
---

# Long work can be cancelled, and says how far it has got

On a real 14-sheet board the first analysis takes 28 to 47 s in a debug build and every edit takes about as long. The page could say only `Solving Beta, 1 of 1` for all of it, because `solve` is one call that reports one step, and nothing could be stopped: the checker, the solver and lowering are single long calls with no way out, the analysis holds the state's lock with the version and the stamps already advanced, a compare is one request that goes on after its page left, and `build` and `fab` have no way to stop (only the PCB router has a flag). This ADR adds a way to stop each of them, and a count the solver reports as it goes.

## Where the time goes

Measured in a debug build on generated boards of the `big` fixture's shape (`scripts/editor-verify-js/fixtures/big.mjs` with more rails; a throwaway timer, not committed):

| Stage | 8 rails (89 parts) | 24 rails | 40 rails |
|---|---|---|---|
| check (file and modules) | 14 ms | 39 ms | 71 ms |
| solve | 2.6 s | 19.0 s | 51.8 s |
| lower (with its export) | 5 ms | 23 ms | 68 ms |
| draw | 0.31 s | 0.44 s | 0.59 s |

The solve is more than 99 % of the time that is not drawing, and inside it picking parts is all of it: the 24-rail board has one group of 120 parts that share relations, and the search over that group takes 18.3 s of the 19.0 (the coupling and the grouping 0.9 s, propagation, verification and the level checks together under 0.1 s). Each trial of the search copies the model and propagates, so the cost grows with the square of the board. A board with generic parts adds the alternatives `recheck_alternatives` solves again, each a whole solve. So the checkpoints that matter are in the search; the others are there so that no stage is a place a cancel waits at.

## The mechanism

`netsmith_core::job` (a module of the lowest crate, which the checker, the solver, lowering, the layout and the placer all reach without a cycle) has a `Cancel` (an `Arc<AtomicBool>`), `scope(&token, work)` that makes it the thread's token for the length of `work`, and `checkpoint()` that ends the call if the thread's token is cancelled. It ends it by `std::panic::resume_unwind(Box::new(Cancelled))`, which unwinds without running the panic hook, so nothing is printed; whoever started the call catches it with `job::catch`, which tells `Cancelled` from a panic and lets a panic go on up. The editor's existing containment (`analysis::contained`, which turns a panic of the analysis into a problem) passes a cancellation on.

*Unwinding, not a `Result`.* The long calls are `check`, `solve`, `lower` and the layout, each a tree of hundreds of functions that return values, not results. A `Result` through them would change every signature in five crates and every test, to carry one case that stops the call and drops everything it made. The unwind does exactly that, and drops it correctly because the crates keep no state outside the call: no cache, no lock, no thread, no global (the only interior mutability is a `RefCell` or a `Cell` local to one function's work). It needs `panic = "unwind"`, the default; the release profile does not change it. A thread-local token, not a `&Cancel` parameter, for the same reason: `solve(board, catalogs)` is called from `fab`, `build`, the editor, the examples and a hundred tests, and none of them has a token to pass. The thread-local is set by one scope at the top of a job and nothing else reads it.

*Cost.* A checkpoint is a thread-local read, and inside a scope one relaxed atomic load. They are placed once for each unit of work of about a millisecond, or more: each trial of the solver's search, each sweep of propagation and each relation or balance it visits, each part and net of lowering and of the pin checks, each instance the checker makes, each sheet, pass and block of the layout and each of its rule checks, each move of the placer and each test of the router for its time. Overhead, results and latency are measured in the report of the change and are in `crates/netsmith-solve/tests/cancel.rs` and the other `cancel.rs` files: a call whose token is not cancelled gives the same bytes (the corpus drawings, the metrics and the solver's picks are compared byte for byte in the tests).

*One token for each job, and a job never stops another.* A scope replaces the token of the scope around it. The analysis of the live project (`Workspace::fresh`) runs under a token of its own, which `Backend::cancel_analysis` sets; it is not the token of the request that happened to trigger it. A compare, a build, a fab and an order each run under the token of their request, and the analysis a compare makes of the ref (in a workspace of its own, read for that job) runs under the compare's.

## What a cancelled job leaves

*A cancelled analysis publishes nothing.* `fresh` changes nothing of the state before the analysis has a result (`pcb` is cleared after, not before), and a cancelled analysis ends in one write: `cancelled`, the stamp of the files it was cancelled on. The snapshot, the version, the stamp, `ready` and the PCBs are as they were, to the byte (`a_cancelled_analysis_publishes_nothing`), so every view goes on showing the last good reading. While the files are as they were at the cancel the watcher does not start the analysis again (it would at once: the stamp differs from the reading's); a change to a file, or `POST /api/analysis/run`, does. `Analysis.cancelled` says so until then, for a page that connects later.

*A cancelled first analysis leaves the server ready.* There is no reading to keep, and a server that stayed pending would answer `503` for ever. The reading is an empty one (no boards, no systems) with one note, `The first analysis was cancelled…`, at version 1, so `rebuilt` is sent and the page leaves its skeleton; the views say there is no board, and `Run again` or a change to a file analyses.

*A cancelled compare* answers `499` and keeps nothing of the ref: not the reading (the cache gets a reading only from a whole analysis), not the scratch folder (the unwind drops it, and it is removed as ever), and the live project's analysis is not touched. A compare whose client disconnects is cancelled the same way: the request's future is dropped when the connection closes, and a guard that the handler holds cancels the job if it is dropped before the answer. A compare that waits for its turn behind another ends when it gets it.

*A cancelled build or fab* answers `499`. The checkpoints are all before a board's files are written (in the checks, the solver, the placer and the router), and none is in the writing, so a board has all its files or none: a fab keeps the packages of the boards it finished and nothing of the board it was on; a build keeps the schematics of the files it finished. A page that closes does not cancel a fab or a build: it has files to write, and only the button does. `Backend::build`, `fab` and `fab_order` take the job's token and answer `Result`.

*Not cancellable:* KiCad's own design rules check and the check of parts against LCSC (a program and a network wait that a flag does not reach), and the router's `Stop` button, which is another thing and stays one: it stops the router and keeps what it routed (`/api/pcb/cancel`, `204`). A cancelled `fab` unwinds out of the router instead, because a package that was stopped halfway must not be written as if it were whole.

## The routes and messages

All additive, with the guard header of every `POST`:

| Request | Answer |
|---|---|
| `POST /api/analysis/cancel` | `{cancelled: bool}`: whether an analysis was running |
| `POST /api/analysis/run` | the reading (`Analysis`), after analysing the files if the last analysis was cancelled |
| `POST /api/compare/cancel`, `POST /api/build/cancel`, `POST /api/fab/cancel`, `POST /api/order/cancel` | `{cancelled: bool}`: whether a job of that kind was running |

The cancelled job's own request answers `499` (`code: "cancelled"` for a request that accepts JSON), and the socket says `cancelled` with `{task, board, message}` (`task` is `analyse`, `compare` with `board` the ref, `build`, `fab` with the board, `order` with the order's name). `Analysis` has an optional `cancelled`. The cancel routes never wait for the analysis (they are not in the list of routes that do) and take no lock the analysis holds.

*Progress.* `progress` keeps its shape and has three optional fields, `unit`, `unitDone` and `unitTotal`. The solver counts the one thing it can count as it goes, that grows and has a known total: **parts chosen**, of the parts to choose in the board (`41 of 118 parts chosen`). The search backs up, so the count is the most parts chosen at once, which only grows: it stalls while the search backs up and does not go back. The groups of parts that share relations are searched one after the other, and the count runs over all of them. The re-solves for generic parts (ADR 0047) report **alternative parts re-solved**, with no total, because the number of alternatives is not known before they are made; the solve those make of the board does not report its own count, so the count of the board's solve is not told again from zero. The drawing counts **sheets drawn** of the board's sheets. A line is sent at most once in 100 ms, except the first and the last of a step. No percentage is made up from them, and none is shown.

*The page.* The status bar's analysing item says `Solving Beta: 41 of 118 parts chosen` and has a `Cancel` button beside it (`Cancelling…` until the server says it stopped); after it the bar says `Analysis cancelled · Run again`. The compare card, the build, fab and order cards have `Cancel`, and Escape cancels a comparison.

## Rejected

*A `Result` through the solver*: above. *A cancel that waits for the lock*: the analysis holds the state's lock for its whole length, which is why earlier work refused to cancel; the token is outside it. *Publishing the analysis as it was when cancelled*: a half-made snapshot has boards that are not drawn and problems that are not found; the page would show a project that is neither the old nor the new. *Marking the reading stale instead of keeping `cancelled`*: stale is the word for a file that does not build (the last good boards, with an error); a cancel is the person's choice, and says so in its own words. *A background thread for the analysis that the cancel could abandon*: a thread cannot be stopped from outside, and an abandoned one runs to its end for nothing, holding a core. *Cancelling a fab when its page closes*: it would turn a reload into the loss of a package that takes minutes; only a compare, which has nothing to write, follows its page.

## Limits

A call is stopped at its next checkpoint, so the latency is the longest stretch between two: a trial of the search, and for a solver that is slower per trial than the board measured here, longer. Nothing inside a single `try_candidate`, a single sheet's wire routing or a single placer move is interruptible, and a cancel that arrives while the analysis is writing its result (after its last checkpoint) is too late and is not applied: the analysis is published whole, and `cancelled` in the answer of the route was true for a job that ended anyway. The drawing of a sheet of a real board can be long between two checkpoints (the layout's inner routing has none); its latency is not measured on a board of that kind.
