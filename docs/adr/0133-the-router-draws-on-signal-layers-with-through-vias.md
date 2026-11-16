---
status: proposed
---

# The router draws on signal layers with through vias, and a net with a known current keeps to the outer layers

ADR 0111 made an inner layer a plane or a signal layer and left tracks on signal layers open: the router drew on the outer layers only, and `CopperLayer` was the two outer layers. Now the router draws on every inner layer the process declares a signal layer (`.process.inner2.use &= "signal"`), as on the outer layers, on any board that declares one; there is no option to turn it on. A board with no signal layer, two layers or every inner layer a plane, routes exactly as before (issue #833).

`CopperLayer` (netsmith-pcb) gains `Inner(n)`, the layer KiCad names `In{n}.Cu`. A track, a teardrop and the routes file's record of a track may be on one. The routes file writes the layer's word, `inner2`, as its zones already do; `top` and `bottom` are unchanged, so every routes file written before reads as it did. A track on an inner plane, or on a layer the board does not have, is an error of the routes file that names its net. `netsmith routes import` keeps a KiCad track on a signal layer and leaves out, with a note, one on any other inner layer. The board file, the Gerbers, netsmith's design rules check and KiCad's already read inner copper by its layer name.

Every via is a through via: in the search a via takes a path from a cell to any other layer at one via's cost, the outer layer first, and it keeps clear of copper on every layer. No process in std states a blind or buried via, and drawing one needs a sourced capability and a process field first.

A net with a known current (the design's `net_currents`, ADR 0036) keeps to the outer layers. Its width is sized for the outer copper with IPC-2221's outer constant; an inner layer has half the constant and thinner copper (0.5 oz against 1 oz on `FourLayer` and `SixLayer`), so the same width would carry far less. Keeping such a net outside costs nothing on the boards that have one today, where such nets are supply rails that a plane usually carries.

A track on a signal layer keeps the process's space between inner copper and a hole (`Rules::plane_clearance`, 0.3 mm on JLCPCB, stated in `std/mfg/jlcpcb.nsm`) from every hole, plated or not. A through-hole pad is an end of a line on every layer and an obstacle on every layer; a surface-mount pad and a mask opening are obstacles of their own face only, and a keepout of the layers it names.

Rejected: an inner layer that costs more than an outer one per step. A line between surface-mount pads reaches an inner layer only through two vias, each of which the search prices as 1.5 mm of track, so it uses the inner layers where the outer ones are crowded, and a small board that routes on its outer layers stays there.

Rejected: sizing an inner track for its current (k = 0.024, inner copper). It would need the design stage to plan a width per layer, and a track's width would change where it changes layer; keeping these nets outside is the smallest step that is right.

The cost: `Track.layer` is no longer one of two, so every match on it was widened, and the router's per-layer grids are vectors.

Left open: blind and buried vias; the editor's track tool on an inner layer (it draws on `top` and `bottom`, and `netsmith routes add-track` too); a via in a pad that leads to an inner layer; the coarse plan and the escape lanes on inner layers; the width of a current-carrying net on an inner layer.
