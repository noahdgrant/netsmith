---
status: accepted
---

# A net's pins are in the order of their parts' paths

Issue #1048, split from #1010. The owner chose the key on 2026-11-16. Meets test 3: the obvious orders (the designators, or where the pads are on the board) are rejected below, and a change back to either would change every board's copper again.

**Decision.** The design stage lists each net's pins in the order of their parts in the design, which is the order of the parts' paths (compared segment by segment, a number at the end of a segment as a number, issue #634), and within one part by the pad's name. The placer's nets and the router's lines are built from that list, so they follow it. The placer also takes its mechanical parts for the corners (holes, then fiducials) in the order of their paths. With these two, no order the placer or the router takes reads a designator, so renumbering the parts, or giving a part other letters, changes no placement and no copper (`designators::relettering_the_parts_changes_no_net_placement_or_copper` in the router's tests).

**Rejected.** The designator order, which the stage used before: a designator is a label that counts parts with the same letters, so adding a part that sorts first, or relettering one, reordered the pins of every net it touched, and the placement and the copper moved with them (as when #634 numbered the parts by path). An order by position on the board: the pins are listed before the parts are placed, and an order that changed as the placer moved the parts would feed back into the placer's own nets. The path is fixed before placement and is already the key of a layout file (ADR 0030), so it is the one name of a part that a board does not change by accident.

**Cost.** Every board's placement and copper changed once, and the committed design files, overlays and reproductions with them, in one reviewed diff. Renaming a part (its path) still reorders its pins, as it should: the path is the part's identity.

**Open.** The order of the nets themselves is the order the checker makes them in; this decision does not touch it.
