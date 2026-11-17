# Glossary: K

**Keepout**:
An area of a board, or of a part in its own frame, where tracks, vias, copper pours or parts are not allowed (`std::pcb::Keepout`), on the layers it names. It is a KiCad rule area in the board file.
_Avoid_: Exclusion zone, restricted area

**Kept copper**:
The router's copper of a board kept on disk after a route, in `.netsmith/routes/<stem>.<board>.toml` in the project's folder, never committed (ADR 0138): what a later editor session, a `layout regenerate` proposal and `fab --reuse-routes` carry the copper of every net a change left alone from. Carried copper is held as the routes file's is while the router routes the rest; the routes file holds a person's decisions, kept copper only the router's.
_Avoid_: Saved routing (say kept copper; a person's copper is the routes file's)
