# Glossary: V

**V-cut**:
A line cut part way through the panel from both faces, straight across the whole panel, along which the boards are broken apart. In the Gerbers the lines are on `Edge.Cuts`, marked "V-CUT" (JLCPCB's instruction); in the panel board file KiCad calls them a malformed outline.
_Avoid_: V-score, scoring

**Variant**:
One value of an enum, written `Enum::Variant` (`AssemblyMode::Hand`). Not a text: `"hand"` is refused where a variant belongs, and the message names the variant it was meant to be. A row of a part's `.variants` table is an **Order code**, not a variant.
_Avoid_: Case, member (a member is a `.name` of a definition), mode

**Via group**:
Vias of one net that stand side by side, their copper no more than the group gap apart edge to edge, or through other vias of the group that are: the process's least clearance, or the board's `.process.via_group_gap` (issue #1505). The design rules check's `via_current` counts a group against what its current needs, the router puts vias beside a via until its group has as many, and the quality report's via pairs leave out a group the current needs. Not a _Via pair_, which is two vias of the two nets of a diff pair.
_Avoid_: Via cluster, via array, stitched vias

**Via in pad**:
A via whose centre is on a surface-mount pad's copper (its shape, the edge included, not its box), on the pad's face. A via whose hole only touches the pad's edge, its centre off the copper, is not one: `fab-notes.txt` lists those apart, and the board house's own pages do not say how it judges them (chapter 21, issue #1578). The fabricator fills a via in a pad with epoxy and caps it with plating so that the solder stays on the pad; the process says up to what size it does that (`.via_in_pad`), and `fab-notes.txt` says how many there are and on which parts, in a line of its own, because the fill costs extra; `project order` stops until the cost is accepted (issue #1118). The router uses one only when a pad cannot be reached any other way.

**Via pair**:
Two vias of the two nets of a _Diff pair_ that stand side by side, their copper no more than 0.5 mm apart edge to edge, where the pair changes layer together (issue #1223). The router routes both nets of a pair again together so that they take their vias alike, and the _Net table_ counts how many of a pair's vias stand in pairs.
_Avoid_: Differential via, via couple

**Via span**:
The two copper layers a _Blind via_ or a _Buried via_ ends on, the upper first, and the layers between: the via's copper and hole are on them and nowhere else (`netsmith_pcb::ViaSpan`, issue #1015). A through via has none. The router changes layer through the via of fewest layers whose span holds both; the routes file writes a span as `layers = ["top", "inner1"]`, `netsmith routes add-via --layers top,inner1` and the editor's via tool choose one by hand (issue #1071), and the design rules check refuses a span the process does not make (`via_span`).
_Avoid_: Via layer pair, drill pair (KiCad's names)

**View**:
One way the editor shows a board built from the source: the schematic, the power tree, the system, the bill of materials or the PCB. Each is computed from the checked and solved model, and none is edited: the source is.
_Avoid_: Page

**Voltage floor**:
The least voltage a rail may fall to while the regulator behind it is in _Dropout_, below the least of its `.voltage`: `.voltage_floor` of `Rail`, stated by a board (`.load.voltage_floor &= 2.7V`, issue #1301). The rail's window, from the floor to the top of `.voltage`, is what the dropout check and the loads' supply ranges on the rail are judged over; `.voltage` itself stays the range the DC analysis holds the rail at.
_Avoid_: Brown-out level, minimum voltage (that is `min(.voltage)`), undervoltage

**Voltage margin**:
The factor on the highest voltage of a rail that a capacitor's rating must have: `.voltage_margin` of `BuckStage` (ADR 0085), 1 unless the board states it (`.stage.voltage_margin &= 1.25`). It multiplies both the rail's steady `.voltage` and its `.peak`.
_Avoid_: Derating factor, safety factor, headroom
