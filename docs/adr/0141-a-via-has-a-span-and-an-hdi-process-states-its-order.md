---
status: proposed
---

# A via has a span, and an HDI process states its order and its buried vias

ADR 0133 left every via a through via, because no process in std stated a blind or buried via. The owner decided (issue #1015) that std gets JLCPCB's HDI service and that netsmith draws its vias end to end. Std now has `HdiFourLayer`, `HdiSixLayer` and `HdiEightLayer` (`std/mfg/jlcpcb.nsm`), each with every number quoted from JLCPCB's two HDI pages.

A process states the vias it makes by two fields, not by a list of spans. `.hdi_order` is how many build-up layers each face has, the pages' "1st-order" to "3rd-order", narrowed in std to what leaves a core of two layers (1 on four layers, up to 2 on six, 3 on eight). A laser-drilled blind via joins each pair of neighbouring layers among the top and the bottom `.hdi_order + 1`. `.buried_vias` is `"none"`, the default, or `"core"`: a mechanically drilled via between the two inner layers the build-up ends on. A through via joins every layer, as on any process. The design file carries them as `Process::hdi`, written only when the process is HDI. The laser's hole and copper are an offer (`.laser_drill ?= 0.1mm`, `.laser_via_diameter ?= 0.25mm`) as `.via_drill` is; the limits (the laser's hole, the ring, the two aspect ratios, the dielectric under a blind via, the core under a buried one) are the process's, and `Rules::for_process` holds the router's vias to them against the board's build-up when the board is placed.

The HDI pages publish no build-up table, and the service behind JLCPCB's impedance page returns none, so an HDI board states its own build-up (`.stackup.thickness`, `.dielectricN` and the rest). The process states none rather than an invented one.

`netsmith_pcb::Via` has `span: Option<ViaSpan>`, the two copper layers it ends on, the upper first; `None` is a through via and is not written, so every board, design file, routes file and board file without a blind via is byte for byte what it was. The routes file writes a span as the via's `layers = ["top", "inner1"]` (two words, as a zone's `layers`), and `netsmith routes import` reads KiCad's `micro` and `blind` vias into it. The board file writes a span between neighbouring layers as KiCad's `micro` via and any other as `blind`, which is KiCad's name for a buried via too. The Excellon output adds one drill file for each span, named as KiCad names it (`-front-in1.drl`, `Plated,1,2,Blind`), and keeps `-PTH.drl` for the through holes. A via's copper, mask opening and room in a plane are on its own layers only, in the Gerbers, the ratsnest, the router and the design rules check, whose `via_span` and `via_aspect_ratio` rules judge the vias against the process; two holes of vias that share no layer are not checked against each other.

The router changes layer through the via of fewest layers that joins the two, the first of the process's order on a tie, and only where that via has room on its own layers. On a board that is not HDI the search runs exactly as before: the per-layer via masks exist only on an HDI board.

Rejected: a list of spans in the process (`.vias &= "1-2, 2-5, 5-6"`). It says more than JLCPCB's pages offer, which name structures by order and "with or without mechanically drilled buried vias", and every list would have to be checked against those structures by hand. The order and the core say the same in two fields a board narrows like any other.

Rejected: skip vias (`L1-L3`). The pages make one as stacked blind and buried vias and say staggered vias are easier to make, so the router joins the same layers with blind vias side by side. Rejected too: choosing a larger span when the smallest has no room. The via the router draws between two layers is then a function of the two layers alone, so the realised via is the one the search priced.

The cost: every struct literal of `Via` names its span; the design rules check has two rules more; a via keeps another via's hole space on every layer in the router (conservative), though the check measures it only between holes that meet.

Left open: a blind via in a pad (the pages fill and planarize blind vias, which would let one stand in a fine-pitch pad), the editor's and `routes add-via`'s choice of a span, the 3D view's drawing of a via by its span, and the coarse plan and escape lanes on inner layers (issue #1015, part 2).
