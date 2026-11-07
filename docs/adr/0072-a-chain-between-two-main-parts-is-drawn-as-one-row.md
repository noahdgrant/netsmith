---
status: proposed (the owner asked for this drawing on 2026-11-07 after reading a board's input protection; the rules that find a link and pick its source are for the owner to review)
---

# A chain of series parts between two main parts is drawn as one row, the source on the left

A board that takes power from a vehicle connector through a fuse and a reverse-polarity diode to a header, with a TVS diode on the protected node, was drawn as three pieces: the header on the far left with the clamp under it, the connector in the middle, and the fuse and the diode standing one above the other to its right. The pieces joined only through power symbols (`+VBATT`, `+VBATT_SAFE`). A person draws this as one line: the connector on the left, the fuse, the diode, the header on the right, the clamp hanging from the node. And a symbol on a wire that is joined to one part and nothing else (`+VBATT`) reads as a port to somewhere else on the sheet.

## The decision

**A link.** A chain of series parts between two main parts, with a supply rail on it, is drawn as a row from one of them to the other, as the spine draws the power path through a regulator (GLOSSARY: _Spine_, _Link_). The same machinery is used with no IC: the row, the turned parts, the parts to ground hanging from a node, the corridor kept free for the wire. The spine's code reads a link as a spine whose input side is empty and whose output side is the chain, with the **source** in the place of the IC and a **load** at the far end.

**What is a link** (`layout/spine.rs`, `link_of`). A main part (not a table of pins or a unit of a symbol, not a small part that stands beside a pin) with a net that has exactly one series part on it, and so on along the chain (every node has one more series part and no other main part), until a net that has a pin of another main part. A supply rail is on the chain (the checked model says so, never the name). Neither end is a rail that feeds other series parts too: a rail with pull-ups, or a supply that feeds several branches, is a net with parts beside its pins and not the end of a chain. IC spines are found first and take their parts.

**Which end is the source.** The end with no parts to ground on it. The parts to ground (a clamp, a bulk capacitor) are where the load is, the rule the spine's free chains already use. When the two ends have as many, the chain stays as it was found (the first main part by the sheet's own order). It is never decided by a name, a pin kind or a part's size.

**What it looks like.** The source's pin on the chain's first net leaves to the right, the load's pin on the last net to the left, and the load is level with the row. The load comes right after the source in the sheet's order and in the same row, whatever the width of the sheet. The net at the source's end is a plain wire. The net at the load's end has the one power symbol with its name; the middle nets have none.

**What it does not change.** The netlist, the checker, the KiCad export and the language. A link is a drawing. `check` and the metrics hold as for a spine (`spine_bends`, `detached_chain` are 0 for it).

## Left open

Whether the symbol at the source's end should come back as a net name on the wire; the reverse-polarity diode's own direction is not read (a diode with its cathode toward the connector is drawn the same); a chain that ends in another sheet has no second main part and is drawn as before.
