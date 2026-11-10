# Pin map and pick lock (read only)

## How to get to it

The schematic's inspector (`Details`, `.sinspector`). Select a part that pin requests ask for (`.status: .u.GPIO`, ADR 0101): its inspector has a **Pin map** section (`[data-testid="pin-map"]`, a `.pm-row` for each request, in the byte order of the requests' paths). The board's **Pick lock** section (ADR 0100) is above the parts list whatever is selected: its count says `current`, `not current`, `absent` or `unreadable` (`[data-testid="pick-lock-state"]`, `data-state`); it is folded when the lock is current and open otherwise. A source that does not build has no lock section (there is nothing to compare). Neither panel has a button: the editor writes neither file. The server's side is `GET /api/board`: `parts[].pinRequests` and `lock` (`crates/netsmith/src/editor/sidecars.rs`).

A row says the request (the key of the pin file's entry), the pin in the part and its pad, the function, a `locked` tag when the entry has `lock = true`, and where the pin came from: `file, hand` or `file, auto` (the pin file holds the entry and it still holds) or `picked now` (this reading picked it; the file has no entry yet). A request that got no pin says `no pin` and the check's message under it, the same text as in the Problems list; the board is then the last good one, marked stale.

## Driving it

`scripts/editor-verify launch --fixture pin_map`, then `scripts/editor-verify drive pin-map`. The fixture is an invented microcontroller with three requests, a pin file that locks one and holds another, and a lock that is not current (it names a part the board no longer has). The recipe selects the microcontroller by its path, reads the rows, runs `netsmith lock --update` in the scratch project (the page says `current` without being touched), deletes the lock (`absent`, and nothing writes it back), and adds a request nothing offers (the message appears). It checks that the pin file and the lock keep their bytes. Look at its screenshots.

## Gotchas

- A part no request asks has no Pin map section.
- The lock is solved for on the first ask after each reading (a solve of the whole source file, with no KiCad): a big project pays for it once per reading, on the first board asked for.
- The editor still solves from the source and does not hold parts to the lock; the panel says what `netsmith lock --check` says.
