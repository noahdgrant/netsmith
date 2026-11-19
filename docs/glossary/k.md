# Glossary: K

**Keep apart**:
A distance a board's physical file asks between two kinds of parts (`keep 10mm between BuckIc and Crystal`, issue 1277): a project's rule of thumb where a datasheet gives no number. The kinds are parts or generics of the standard library or the project, and each part of the one kind stands at least the distance from each part of the other, courtyard to courtyard, whatever faces they are on. A requirement on where parts go: the placer keeps it as hard as an overlap, and `fab` and `layout check` stop where it is not met.
_Avoid_: Spacing rule, isolation distance (a creepage or clearance is the process's)

**Keepout**:
An area of a board, or of a part in its own frame, where tracks, vias, copper pours or parts are not allowed (`std::pcb::Keepout`), on the layers it names. It is a KiCad rule area in the board file.
_Avoid_: Exclusion zone, restricted area

**Kelvin connection**:
A sense pin routed from the pad it senses, alone: a part's `kelvin` statement names the pin, and the pad of the part its selector finds on the pin's net (a shunt's terminal) is the sensed pad (issue #1148). No other copper of the net joins the sense track on the way, so no load current flows in it. The router keeps the track apart by cutting the sensed pad in two while it routes, and the board file carves a pour, a plane or a zone of the net round the sense copper (issue #1174); `fab` warns when the routed copper of the pin joins another pad of the net.
_Avoid_: Kelvin tap, sense tap

**Kept copper**:
The router's copper of a board kept on disk after a route, in `.netsmith/routes/<stem>.<board>.toml` in the project's folder, never committed (ADR 0138): what a later editor session, a `layout regenerate` proposal and `board fab --reuse-routes` carry the copper of every net a change left alone from. Carried copper is held as the routes file's is while the router routes the rest; the routes file holds a person's decisions, kept copper only the router's.
_Avoid_: Saved routing (say kept copper; a person's copper is the routes file's)
