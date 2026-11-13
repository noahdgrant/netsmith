---
status: accepted
---

# A hand pick of a catalog row is a narrowing in source, and there is no picks file

The owner decided on 2026-11-13 (issue #629, epic #638) that a person who chooses a part's row writes it in the `.nsm` source, as `.lcsc &= "C307331"` or `.mpn &= "..."`, that `<stem>.picks.toml` (#462) goes, and that the pick lock stays the only generated file. It meets two tests: it is hard to reverse (boards carry the narrowings, and a file format goes), and it rejects an obvious alternative (keep the picks file with a format of its own). It changes what ADR 0100 holds a part to and adds two members to every part beside the catalog columns of ADR 0022.

**Decision.** Every part has two text members, as it has `.designator` and `.height`: `.lcsc`, the supplier's part number, and `.mpn`, the manufacturer's (`HAND_PICK` in `crates/netsmith-check/src/checker.rs`, and the list of members a use of a generic may reach, ADR 0068). A design that narrows either on a part that picks from a catalog makes a hand pick: the solver offers the part only the row whose `lcsc` and `mpn` cells equal what it states, and judges that row by everything the design says (`hold_to_choice` in `crates/netsmith-solve/src/solve.rs`). A row that fails a relation stops the run with the relation and a second message at the narrowing that no other row was tried; a number no row has stops it at the narrowing. Two narrowings that differ are the usual contradiction. A part inside a module is hand picked from outside by its path (ADR 0116).

**A hand pick comes before the lock.** The lock records what the solver picked; a hand pick is a requirement. A part with a hand pick is not held to its locked row. If the two differ, the run takes the hand-picked row and says that the lock differs, `--locked` refuses, and `lock --update` records the new row, as for any pick that moved.

**The picks file is refused, not ignored.** A source with `<stem>.picks.toml` beside it stops `build`, `fab`, `lock` and the editor with an error that lists, for each entry, the narrowing to write in the body of its board (`netsmith_catalog::picks::refuse`). An ignored file would let its parts move to other rows with no word. The one committed picks file, `examples/power_manifold_charger/src/main.picks.toml`, is now 19 narrowings in `src/main.nsm`, with the same rows in its lock. No command wrote or edited the picks file, so no command changes.

**Rejected.** *Keep the picks file with a format of its own*: a second hand-written file beside the source, read by the solver, for what one line in the source says; adding one IC already touches up to nine files. *`.lcsc` declared on each standard generic's `catalog` statement*: fifteen copies of one line, and a project's parts would need it too. *A hand pick that loses to the lock*: the lock would have to be deleted to make a person's choice take effect.

**Cost.** Every part has two members more, including parts with no catalog. `.mpn` was already declared as `.mpn: String` by 32 parts and generics (std's `Lt8641` and connectors, the examples' ICs); the declaration is still accepted and is now redundant. `lcsc` is the name of one supplier's numbering in the language, because it is the column every standard catalog has. A `.mpn &=` that chose a row of a keyed catalog (`catalog mcu by .mpn`) is now a hand pick too, so it comes before that part's lock entry; the rows of every example are unchanged (`scripts/check-example-locks.sh`).

**Owner decisions (2026-11-13).**

- `.lcsc` and `.mpn` are members of every part, given by the checker, rather than declared in std. A part's own `.mpn: String` or `.lcsc: String` is to be refused and the 32 declarations removed (issue #696).
- `.lcsc` narrowed on a part with no catalog is an error at the narrowing (`` `sw.lcsc` holds the part to a catalog row, but `sw` (`Switch`) picks from no catalog ``). `.mpn` on such a part is not, since it may choose a provider of a generic (ADR 0116's `.receptacle.mpn`).
- A picks file left beside a source is an error with the narrowings to write, rather than a note or a silent skip.

**Left open.** Whether a hand pick should also be written by a command (the editor's compare of alternatives, `Catalogs::with_pin`, could offer "keep this row" as a narrowing). Whether the other per-board files (`<stem>.pins.toml`, `<stem>.pcb.toml`) can fold into the source or into one file, which #629 also asked; nothing here changes them.
