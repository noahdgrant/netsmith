---
status: proposed
---

# A pick lock holds each part to its catalog row

Owner decision of 2026-11-09 (the recommended answers of `docs/research/solved-design-and-lock.md`, section 6). It meets two of the three tests: it is hard to reverse (a committed file format that projects will carry), and it rejects an obvious alternative (no file, and a solved-design file first). The owner has not seen this text and may amend any answer.

**Decision.** `build` and `fab` write `<stem>.lock.toml` beside each source file when it does not exist. For each board or system, by name, and each part, by its path, it holds the `catalog` and the `row` the solver picked, with the row's `lcsc` and `mpn` as a check against a renamed row. It holds nothing else: no stock, no price, no solved value, no tool version. It is TOML, as the layout files are (ADR 0032), with its own `version`, a file of its own and not a table of another file, and committed. When it exists, the solver offers a locked part only its locked row and judges that row by everything the design says; if the row fails or is gone, the run stops with the relation and its numbers and does not try another row (strict). A part with no entry, or an entry with no part, is a note, and an error with `--locked`. A run never rewrites an existing lock.

**Why.** The solver takes the first row that works, in file order. A catalog edit, a solver change or a new row can move a pick with no change to the source, and nothing says so. Measured on the repository's boards (the addendum of the research document), the solver is under 50 ms and the time of a run is in the KiCad libraries, the export and the placer, so what a lock buys is a stable board and a reviewable change, not speed.

**Rejected: a lock that falls back to another row with a warning.** A build can then differ from the last one with only a warning to say so. Strict costs a deleted file or an edited design when a row is renamed, which is a visible, one-line change.

**Rejected: one `netsmith.lock` for a project.** Boards are reviewed one at a time and the layout files are per source.

**Rejected: `Catalogs::with_pin` as the lock.** It holds a part to one row to test an alternative, and drops the design's statements about the row's numbers. A lock keeps them and judges the row by them.

**Rejected: the solved-design file first.** It records the last solve and does not make picks stay. It stays a later stage.

**Cost.** A file beside every source, whose first write is a surprise to a project that has none. A lock that goes stale on every catalog edit that touches a locked row. An entry for a generic part holds the value the solver chose, not the orderable row, which `fab` still resolves against the stock of the day (ADR 0047).

**Left open.** The file's name and the keys (the research document calls them placeholders). Whether a run that finds new parts rewrites the lock: the document says both that it does (case 3) and that it never does unless the file is absent; this version leaves the file alone and says so. The refresh command (`netsmith lock --update`, `--check`), `fab --order` and the editor reading and writing a lock, and locking the row `fab` resolves a generic to.
