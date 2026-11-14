# Glossary: V

**V-cut**:
A line cut part way through the panel from both faces, straight across the whole panel, along which the boards are broken apart. In the Gerbers the lines are on `Edge.Cuts`, marked "V-CUT" (JLCPCB's instruction); in the panel board file KiCad calls them a malformed outline.
_Avoid_: V-score, scoring

**Variant**:
One value of an enum, written `Enum::Variant` (`AssemblyMode::Hand`). Not a text: `"hand"` is refused where a variant belongs, and the message names the variant it was meant to be.
_Avoid_: Case, member (a member is a `.name` of a definition), mode

**Via in pad**:
A via whose centre is on a surface-mount pad. The fabricator fills it with epoxy and caps it with plating so that the solder stays on the pad; the process says up to what size it does that (`.via_in_pad`), and `fab-notes.txt` lists the pads that have one. The router uses one only when a pad cannot be reached any other way.

**View**:
One way the editor shows a board built from the source: the schematic, the power tree, the system, the bill of materials or the PCB. Each is computed from the checked and solved model, and none is edited: the source is.
_Avoid_: Page

**Voltage margin**:
The factor on the highest voltage of a rail that a capacitor's rating must have: `.voltage_margin` of `BuckStage` (ADR 0085), 1 unless the board states it (`.stage.voltage_margin &= 1.25`). It multiplies both the rail's steady `.voltage` and its `.peak`.
_Avoid_: Derating factor, safety factor, headroom
