---
status: proposed
---

# The router's copper is kept in an uncommitted cache, and `fab` reads it only when asked

Issue #1019, what ADR 0030 left as "Not built" (the router's copper kept as a cache, so that partial routing holds across sessions and in `fab`). The owner chose option C of the issue with an opt-in for `fab` on 2026-11-16. The rules below the first paragraph were chosen by the implementing agent and wait for the owner. Meets tests 1 (a file in the project's folder, and a flag of `fab`) and 3 (the routes file itself, or a committed file beside it, are the obvious places).

**Decision.** The router's copper of each board is kept in `.netsmith/routes/<stem>.<board>.toml` in the project's folder (the folder of `netsmith.toml`, or the source's when there is none), which is never committed. The editor writes it after every route that ran to its end and, when it has no routed board of its own (a new session), starts a route from it, so the copper of every net a change left alone is carried (`netsmith_router::carry`) and only the rest is routed. `netsmith layout regenerate` says what of it a route after the regeneration keeps. `fab` reads and writes it only with `--reuse-routes`; without the flag it routes every net as before, so a clean checkout gives the same board on every machine.

- The file holds the router's copper net by net (tracks and vias after the routes file's), each net's pad hash, the place, turn and face of every part it was routed round, the source it is of, and three keys: the router's version (netsmith's version and a format number), a hash of the copper the routes file put on the board (tracks, vias, locks, zones), and a hash of the board's design rules (the rules, the process, the outline, its cutouts, pours, planes and keepouts, and the nets' widths, clearances and currents).
- A file whose router, routes-file copper, rules, board or source is not the board's is not used at all, and the reason is said. Otherwise the board it was routed on is made again from the board placed now, and `carry_copper` decides net by net as it does in memory: a net whose pads are not where the record says, that a part which moved now covers, or that the routes file locks, is routed again.
- The folder `.netsmith` holds a `.gitignore` of its own that ignores everything in it, written with the first file, so that an existing project needs no edit; `netsmith init` also ignores `/.netsmith/` in the project's `.gitignore`.
- A route that the editor or `fab` stopped short is not kept: it is no board to start from.

**Rejected.**

- *The router's records in `<stem>.routes.toml` (option A).* The file of a person's decisions would change on every route, and `fab`'s copper would depend on what an editor session routed last (ADR 0030 decision 2).
- *A committed `<stem>.routes.cache.toml` beside it (option B).* A third file beside each source, and a file that changes on every route in the history.
- *`fab` reading the cache by default.* Two machines would make two boards from the same commit.
- *A hash of the routes file's text as the key.* A comment, or a record of another board, would drop the cache; the copper the file puts on this board is what the router routed round.

**Cost.** `fab --reuse-routes` on two machines can give two boards; a release or an order should not use it, and `--order` does not take it. The key holds netsmith's version, so a build between releases with another router keeps copper the new router would draw otherwise; carried copper is checked by the design rules check as any other. Two sources of one stem in different folders share a file name and overwrite each other's keep.

**Left open.** Pruning the files of boards that are gone, and a command that lists or clears the cache.
