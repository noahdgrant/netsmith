# Glossary: N

**Narrowing**:
Restricting a field to the values that both its current limit and a new constraint allow. `&=` does it (ADR 0018). A default (`?=`) is not a narrowing: it takes effect only when nothing narrows the field.

**Neck-down**:
A track drawn narrower for a short run where the board's wider track does not fit, such as between two pads: no narrower than the width its net's current needs, except within 1 mm of the pads of a part the line ends on, where it may be the process's least width (issue #431). The router measures the finished copper of every line against the other nets' copper, and a line whose copper is too near at any width it may have is not routed (ADR 0036).

**Net**:
All the pins joined at one electrical node, with its name. Only a board names a net; an unnamed one is called `net-` and its smallest terminal path. A rail's ground is `GND` unless a statement names another (ADR 0121).

**Net class**:
The width the tracks of a net are drawn at. A rail may state it (`.min_track` on a `Rail`, an override; a net takes the widest of its rails). A net no rail states a width for is drawn at the width IPC-2221 needs for its current (the sum of the loads on it or returning through it, all on at once), for the currents the solver knows (when some are not known, those are a lower bound: the width is never less than the process's least, and a note names the net and the missing currents). Not a list of nets; the rail names them. A stated width is checked against the net's current by IPC-2221 (ADR 0036).
_Avoid_: Track class, width rule

**Net current (from the analysis)**:
The most current that flows into a net the DC analysis solved: at each corner the sum of what the elements put into the net, and the largest of those over the corners (ADR 0076, amendment 8). It sizes the net's tracks in place of the sum of every current at its largest (ADR 0036), which is larger where the loads follow one voltage. Only a net between elements that no rail holds has one: a supply, a ground or a regulator's output is on several islands.
_Avoid_: Corner-wise sum, worst-case current (a worst case is an end of a range, and this is a sum taken inside each corner)

**Net label**:
How the editor's schematic shows a signal net that is not drawn as one wire: the net's name as plain text at the end of a short stub out of each group of its pins (a main part and the small parts that serve its pins, which stay wired to that stub), like KiCad's local label. A net is named, not wired, when it joins more than one such group and its wire would be long (over 48 grid steps), fan out to three groups over 30 steps, or cross more than one other wire; a rail is never named (a power symbol is drawn), nor is the net of a part that serves a pin. A net with a generated name is shown by the first pin of its main part (`J2.3`), as KiCad names such a net. The pieces of a net that carry the same name in a module are one net.
_Avoid_: Bare name, off-page connector (a flag is that), global label

**Netlist comparison**:
Deciding whether two designs are the same circuit by connectivity (`netsmith compare`, ADR 0017). Components are matched by value and footprint and by how their pins are wired, so net names and, unless told otherwise, designators may differ; a two-pad part without polarity may have its pads either way round (#67). It succeeds with the pairing of components and nets, or fails with the nets and pins that differ.
_Avoid_: Netlist diff (a diff compares names)

**Nominal value**:
`nominal(.f)`: the value a field is stated at, without its tolerance, as one number of the field's own kind, written in a `<=` or `>=` statement (`nominal(.rprog) >= 2kohm`) for a datasheet limit on the value a part is built with. A range is stored as its two ends, so it is their mean (`2kohm +/- 1%` is 2 kohm, `1kohm..3kohm` is 2 kohm). Read like a range end: never solved for, never prunes, waits while the field has no value. The plain `>=` still reads the worst case.
_Avoid_: Typical value (a datasheet's typical column is a different figure), centre value
