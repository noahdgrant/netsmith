# Glossary: V

**V-cut**:
A line cut part way through the panel from both faces, straight across the whole panel, along which the boards are broken apart. In the Gerbers the lines are on `Edge.Cuts`, marked "V-CUT" (JLCPCB's instruction); in the panel board file KiCad calls them a malformed outline.
_Avoid_: V-score, scoring

**Variant**:
One value of an enum, written `Enum::Variant` (`AssemblyMode::Hand`). Not a text: `"hand"` is refused where a variant belongs, and the message names the variant it was meant to be.
_Avoid_: Case, member (a member is a `.name` of a definition), mode

**Via in pad**:
A via whose centre is on a surface-mount pad. The fabricator fills it with epoxy and caps it with plating so that the solder stays on the pad; the process says up to what size it does that (`.via_in_pad`), and `fab-notes.txt` says how many there are and on which parts, in a line of its own, because the fill costs extra; `project order` stops until the cost is accepted (issue #1118). The router uses one only when a pad cannot be reached any other way.

**Via span**:
The two copper layers a _Blind via_ or a _Buried via_ ends on, the upper first, and the layers between: the via's copper and hole are on them and nowhere else (`netsmith_pcb::ViaSpan`, issue #1015). A through via has none. The router changes layer through the via of fewest layers whose span holds both; the routes file writes a span as `layers = ["top", "inner1"]`, `netsmith routes add-via --layers top,inner1` and the editor's via tool choose one by hand (issue #1071), and the design rules check refuses a span the process does not make (`via_span`).
_Avoid_: Via layer pair, drill pair (KiCad's names)

**View**:
One way the editor shows a board built from the source: the schematic, the power tree, the system, the bill of materials or the PCB. Each is computed from the checked and solved model, and none is edited: the source is.
_Avoid_: Page

**Voltage margin**:
The factor on the highest voltage of a rail that a capacitor's rating must have: `.voltage_margin` of `BuckStage` (ADR 0085), 1 unless the board states it (`.stage.voltage_margin &= 1.25`). It multiplies both the rail's steady `.voltage` and its `.peak`.
_Avoid_: Derating factor, safety factor, headroom
