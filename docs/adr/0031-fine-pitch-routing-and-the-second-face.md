---
status: accepted
---

# Fine-pitch routing, vias in pads, and parts on the back

Amends ADR 0008 (the process) and ADR 0012 (four layers, which left via in pad open).

A real board has a module whose pads are 0.6 mm squares on a 1.1 mm pitch, parts on both faces, and 200 connections. The router left the pads inside the module unrouted: it kept half a cell and a margin beyond the rules from every obstacle, which closed the 0.5 mm between two pads, and a track with its two spaces needs 0.3 mm.

## How a line is routed now

1. The line is searched for with the old room (the rules, a 0.05 mm margin and half a 0.1 mm cell).
2. A line that is left is tried again with 0.02 mm beyond the rules (`ROOM_TIGHT`), which a track between two open cells and a stub from a pad's centre to the nearest cell stay inside of. Only the lines that failed pay for this, so a board that routes at the old room routes as it did.
3. A pad still walled in is reached by a **via in the pad** if the process allows it. The via is at the pad's centre, is no wider than the pad's smaller side, and is checked at that exact place against all other copper and holes. A path that starts or ends on a pad's far layer pays 60 mm of track for it, so it is the last resort. A pad on a plane net that cannot get out gets the via and no track.
4. Where the board draws a track wider than the process's least, the tight try also looks for cells where a track of the least width fits; a track through them is **necked down** to the least width there and is the full width elsewhere.
5. Lines still left are **negotiated** for (PathFinder in a small form): each is routed where it would go if no other net were in the way, the nets it comes near are ripped up, the cells they fought over cost more from then on, and what was ripped up is routed again. This goes six rounds and the best state is kept. The earlier rip-up put the nets back if the line did not fit; this one lets lines fail for a round so the others can move.

There is no fan-out stage that puts a via beside each pad of a dense footprint before routing. The search already puts a via on any free cell next to a pad, and a pad with no free cell beside it (a pad inside a grid with 0.5 mm gaps) is the case a via in the pad is for.

## The process says whether a via may stand in a pad

`std::mfg::jlcpcb` has `.via_in_pad: Length`, the largest via the fabricator fills with epoxy and caps with plating, `0mm..0.55mm`. It is an upper bound where the other fields are lower bounds, so the design uses the top of what is left, and a board that does not want to pay for filled vias writes `.process.via_in_pad &= 0mm`. The number is from JLCPCB's capabilities page ("epoxy filled and capped" vias of 0.15 mm to 0.55 mm; "vias can be placed within BGA pads using filled and plated-over vias"). The page calls the range "via diameters"; the router uses a via in a pad only if the copper of its via is no wider than the number. A soldermask-plugged via is not one: it needs no mask opening, so it cannot stand in a pad. When a board has vias in pads, `fab-notes.txt` says how many, in which pads, and that they are ordered filled and capped on the order form (the Gerbers do not say it). Left unverified: that JLCPCB's order form for the two-layer process offers it, and its price.

## `.kicad_pro` gives the Default net class a priority

Without `"priority"`, KiCad 9 ignored the Default class's clearance and checked every board against its own 0.2 mm (measured: a 0.1 mm project reported "clearance 0.2000 mm"). The old room kept 0.2 mm anyway, so no earlier board saw it; routing at the process's 0.1 mm needs the rule to be the process's. This is one added line in every `.kicad_pro`.

## Parts on the back

A part has a `Side`: `Front` or `Back`. A part on the back is its footprint mirrored in its own frame (x negated), then turned by its rotation, as KiCad flips a footprint; its surface-mount pads are on `B.Cu`, and in the `.kicad_pcb` the footprint is written already flipped, on the back layers, with mirrored text. The router starts and ends a line on the layers a pad has, so lines between faces use vias as lines between layers do. Two courtyards on opposite faces may overlap unless one part has a hole through the board. *Amended (PROVISIONAL, to be reviewed by the maintainer):* a part with a hole takes its courtyard on both faces only if its footprint draws the courtyard on both `F.CrtYd` and `B.CrtYd`, or on neither (then the box made from its pads). If the footprint draws it on one of them only, the face of the other layer is judged by the part's pad boxes (every pad, copper or not) each grown by the process's clearance, instead of the whole courtyard: another part's courtyard on that face may cover the rest of its body, and a pad of the one under a pad box is still an error. The sentence is the same for the annealer's overlap cost (`Placed::room_on` and `Engine::rooms_on`), the placement check and the fit test (`could_fit` counts the pads' area on that face). The hole-to-hole and hole-to-copper rules are unchanged.

The placer takes `PlaceOptions { sides, fit, keepouts }`, which the sidecar layout file will fill. A part in `sides` is on that face. Without a size to fit, parts are on the front unless named. With a size, the arrangement is laid out inside it. *Amended (ADR 0045, section 10, provisional):* the rows no longer decide whether the parts fit, since they never turn a part and count a part that hangs over the edge whole. If the rows are too big the annealer is run in the size anyway, first on the faces the parts are on, then, if that fails, with the parts nobody placed shared between the faces by area, largest first; only if the last run ends with courtyards over each other or parts outside does the error give the numbers ("the parts need 63.4 x 41.2 mm ... and the outline is 60 x 40 mm: 3.4 mm over in x; placing them in the outline anyway ended with 2 pair(s) of courtyards overlapping, ..."). The back is laid out in the same rows as the front, in the same area; a part with a hole takes room on both and the other face keeps clear of it. Boxes in `keepouts` (holes that are not parts) are kept clear of by the rows. The fabrication package already handled a part on the bottom: its position row is `bottom`, and its rotation is not corrected because no board of the evidence had one, so `fab-notes.txt` lists it as unverified.

## Left open

Rotation and mirroring of bottom parts in the position file are unmeasured. A footprint's 3D model is copied unchanged for a part on the back. Connectors are not moved to the back. A mask opening drawn in a footprint keeps vias away but not tracks. Fan-out before routing, push and shove, and a router that sees more than two layers are not done.
