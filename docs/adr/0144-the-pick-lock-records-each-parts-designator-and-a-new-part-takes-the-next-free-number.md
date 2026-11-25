---
status: accepted
---

# The pick lock records each part's designator, and a new part takes the next free number

Owner decision, issue #1653. It changes the numbering of issue #634 (2026-11-13), where every part was numbered by its path from 1 on each build, so adding a part renumbered every part after it with the same letters. It meets two of the three tests: it is hard to reverse (committed lock files carry the numbers, and a board house's order names them) and it rejects an obvious alternative (numbering by path alone).

**Decision.** The pick lock (`<stem>.lock.toml`, ADR 0100) has a table `[<Board>.designators]` for each board and system: each part's path, a member of a bank or group by its index (`bulk[1]`), and a board of a system under its name in the system (`power.r1`), with its designator (`"R3"`). The design stage gives a part the designator the lock records for it when the letters are the part's letters now. Every other part, in path order, takes the number after the highest of its letters that the lock records or that a part already has. Nothing in the source changes. A board the lock records no designators for, and a source with no lock, is numbered by path from 1, as before. `build` and `fab` write the table with the lock and never rewrite it; `project lock --update` keeps the record and records the new parts; `project lock --update --renumber` numbers every part by path again. `project lock --check` calls a lock that misses a part's designator, or records one of a part that is gone, not current, and `--locked` refuses such a run as it refuses a part with no row.

**A number is not taken again while the lock remembers it.** A removed part's entry stays until `--update`, and the next new part of those letters takes a number above it. `--update` drops the entries of parts the board no longer has, as it drops their rows, and gaps are never filled, so after it only the highest number of a set of letters can be taken again.

**A renamed part is a new part.** The lock is keyed by path, as the layout file is (ADR 0032): a part whose path changes takes the next free number, and its old number is a gap. A person keeps the number by renaming the key in the lock's table by hand, in the same change as the source. A part whose letters change takes a new number of its new letters.

**Two parts with one designator** (a lock edited by hand) are an error of the design stage that names both paths and the designator, and says to change one or to renumber.

**Why.** A board that has been ordered, or has a schematic people have printed and marked, keeps its names when a test point or a capacitor is added: `R7` on the bench is still `R7` in the bill of materials, the placement file and the new schematic. The lock is already the file that holds a board to what it was and shows a change as a reviewable line, so the record lives there and not in a file of its own.

**Rejected: numbering by path alone (#634).** Every addition can rename parts that did not change. **Rejected: designators in the source.** The language would gain a second name for each part that the path already gives; issue #634 rejected it. **Rejected: filling the lowest free number.** A removed `R3` would come back on another part, and a person reading an old printout would take it for the part that was removed.

**Cost.** Designators no longer follow the paths once parts are added: a later part has a higher number, wherever its path sorts. The netlist, the placer and the router do not read designators (issue #1048), so nothing moves. Every committed lock of version 2 without the table is not current until `--update` records it.

**Left open.** Recording designators for a source with no part to pick, which has no lock. The editor offering a renumber.
