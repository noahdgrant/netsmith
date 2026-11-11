# Pin map (padlock and move write the pin file) and pick lock (read only)

## How to get to it

The schematic's inspector (`Details`, `.sinspector`). Select a part that pin requests ask for (`.status: .u.GPIO`, ADR 0101): its inspector has a **Pin map** section (`[data-testid="pin-map"]`, a `.pm-row` for each request, in the byte order of the requests' paths). The board's **Pick lock** section (ADR 0100) is above the parts list whatever is selected: its count says `current`, `not current`, `absent` or `unreadable` (`[data-testid="pick-lock-state"]`, `data-state`); it is folded when the lock is current and open otherwise. A source that does not build has no lock section (there is nothing to compare). The pick lock has no button: the editor never writes it. The pin map has a padlock and a **Move** button on each row (below). The server's side is `GET /api/board`: `parts[].pinRequests` and `lock` (`crates/netsmith/src/editor/sidecars.rs`).

A row says the request (the key of the pin file's entry), the pin in the part and its pad, the function, a `locked` tag when the entry has `lock = true`, and where the pin came from: `file, hand` or `file, auto` (the pin file holds the entry and it still holds) or `picked now` (this reading picked it; the file has no entry yet). A request that got no pin says `no pin` and the check's message under it, the same text as in the Problems list; the board is then the last good one, marked stale.

## Writing the pin file (ED2)

The editor writes the pin file's `lock`, `pin` and `source` lines of one entry, and no other line (comments, order and spacing stay; `netsmith_check::edit_entry` does it, and refuses a file it cannot edit safely).

- **Padlock** (`[data-testid="pm-padlock"]`, `aria-pressed` is the lock, name `Lock the pin of <request>`, the tooltip says what a click writes): `POST /api/pins/lock`. A request the file has no entry for gets one (pin it has now, `source = "auto"`, `lock = true`) after the last entry of its board.
- **Move** (`[data-testid="pm-move"]`, `aria-expanded`): opens the row's list of the pins that offer the function (`.pm-target`, `data-pin`, `data-state` is `current`, `free` or `held`). Enter or a click on a pin writes `pin` and `source = "hand"`: `POST /api/pins/move`. The pin chip of the row (`[data-testid="pm-pin"]`) can also be dragged onto a target of the open list. A locked request's Move button is `aria-disabled` and says why.
- A refusal (a pin another request or the source holds, a pin that does not offer the function, a `shared` limit, a locked request, a source that does not build) is `422` with the reason, and the page shows it as a toast titled `<request>: the pin file was not written`. Nothing is written.
- The write is a change to a watched file: the project is analysed again and the page reads it, as after a save.

## Driving it

`scripts/editor-verify launch --fixture pin_map`, then `scripts/editor-verify drive pin-lock pin-map` (`pin-lock` puts the pin file back, so `pin-map` finds it as it was). The fixture is an invented microcontroller with three requests, a pin file that locks one and holds another, and a lock that is not current (it names a part the board no longer has). The recipe selects the microcontroller by its path, reads the rows, runs `netsmith lock --update` in the scratch project (the page says `current` without being touched), deletes the lock (`absent`, and nothing writes it back), and adds a request nothing offers (the message appears). It checks that the pin file and the lock keep their bytes. Look at its screenshots.

`pin-lock` clicks the padlock of a request with no entry, unlocks another, moves a request with the keyboard (Tab to the pin, Enter), is refused for a pin another request holds, drags a pin onto a free one, and asserts the pin file's text on disk after each step.

## Gotchas

- A part no request asks has no Pin map section.
- The lock is solved for on the first ask after each reading (a solve of the whole source file, with no KiCad): a big project pays for it once per reading, on the first board asked for.
- A pin the editor writes is one the next reading keeps: the move is judged by the rules the check applies to a pin file (offers the function, one request each, nothing the source holds, `shared`), from the analysis of the files on disk.
- The editor still solves from the source and does not hold parts to the lock; the panel says what `netsmith lock --check` says.
