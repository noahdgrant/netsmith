---
status: proposed
---

# The router draws on signal layers with through vias, and a net with a known current is drawn wider there

ADR 0111 made an inner layer a plane or a signal layer and left tracks on signal layers open: the router drew on the outer layers only, and `CopperLayer` was the two outer layers. Now the router draws on every inner layer the process declares a signal layer (`.process.inner2.use &= "signal"`), as on the outer layers, on any board that declares one; there is no option to turn it on. A board with no signal layer, two layers or every inner layer a plane, routes exactly as before (issue #833).

`CopperLayer` (netsmith-pcb) gains `Inner(n)`, the layer KiCad names `In{n}.Cu`. A track, a teardrop and the routes file's record of a track may be on one. The routes file writes the layer's word, `inner2`, as its zones already do; `top` and `bottom` are unchanged, so every routes file written before reads as it did. A track on an inner plane, or on a layer the board does not have, is an error of the routes file that names its net. `netsmith routes import` keeps a KiCad track on a signal layer and leaves out, with a note, one on any other inner layer. The board file, the Gerbers, netsmith's design rules check and KiCad's already read inner copper by its layer name.

Every via is a through via: in the search a via takes a path from a cell to any other layer at one via's cost, the outer layer first, and it keeps clear of copper on every layer. A via in a pad leads to the pad's far outer layer and to every signal layer, as any through via does. No process in std states a blind or buried via, and drawing one needs a sourced capability and a process field first.

A net with a known current (the design's `net_currents`, ADR 0036) has a width for each kind of layer. Its outer width is sized as before; on a board with a signal layer the design stage sizes a second, `NetCurrent::inner_least_width`, with IPC-2221's inner constant (half the outer) in the inner copper (`stackup.inner_copper`, 0.5 oz against 1 oz outside on `FourLayer` and `SixLayer`), at the same rise and width tolerance. The router draws the net's tracks on a signal layer at that width rounded up to 0.01 mm (`Board::track_width_on`) and never necks them there; the design rules check holds a track on an inner layer to it (`DrcExtras::inner_least_widths`); the track drop counts an inner track by its own copper. A track changes width where it changes layer, at a via. A design file written before this has no inner width, and its nets with a current keep to the outer layers, so such a file routes as it did.

A track on a signal layer keeps the process's space between inner copper and a hole (`Rules::plane_clearance`, 0.3 mm on JLCPCB, stated in `std/mfg/jlcpcb.nsm`) from every hole, plated or not. A through-hole pad is an end of a line on every layer and an obstacle on every layer; a surface-mount pad and a mask opening are obstacles of their own face only, and a keepout of the layers it names.

Rejected: an inner layer that costs more than an outer one per step. A line between surface-mount pads reaches an inner layer only through two vias, each of which the search prices as 1.5 mm of track, so it uses the inner layers where the outer ones are crowded, and a small board that routes on its outer layers stays there.

Rejected: keeping a net with a known current on the outer layers. It is safe, but a board whose outer layers are full has no way for its supply nets, which are the ones the inner layers help most. Rejected too: one width for every layer, the inner one. It would widen every outer track for a layer the net may never use.

The cost: `Track.layer` is no longer one of two, so every match on it was widened, and the router's per-layer grids are vectors.

The editor's track tool and `netsmith routes add-track` draw on the signal layers too, at the net's width on that layer; a plane is refused.

Left open: blind and buried vias (JLCPCB's HDI service has them, but no process in std states them); the coarse plan and the escape lanes on inner layers.
