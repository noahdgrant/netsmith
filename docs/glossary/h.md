# Glossary: H

**Hand pick**:
A narrowing of a part's `.lcsc` or `.mpn` in the source (`.c1.lcsc &= "C307331"`), members every part has: the part is held to the catalog row with that number, before the _Pick lock_, and the row is judged by everything the design says; a row that fails, or a number no row has, stops the run at the narrowing (ADR 0122). The lock then records the row.
_Avoid_: picks file (`<stem>.picks.toml`, which held them once and is now refused), override, pinned (a _Pin_ is a part's lead)

**Heating current**:
The DC current at which an inductor warms by the rise its datasheet states: `Inductor`'s optional `.rms_current`, a catalog column that the standard rows leave empty because the rise differs between makers and no row has a recorded value (ADR 0055, proposed). Distinct from the saturation current, which is where the inductance falls.
_Avoid_: Rated current (a rating of what?), temperature-rise current, Irms (the symbol, not the term)

**Height coverage**:
How many of a board's fitted parts have a known height, out of all of them, as the 3D view shows it (ADR 0105). A _Standard height_ counts as known and is counted apart (ADR 0106). A part with an unknown height lowers it; a _Height preview_ does not raise it.
_Avoid_: Model coverage, completeness, height score

**Height limit**:
The tallest a part may stand where a keepout is (`std::pcb::Keepout`'s `.max_height`, ADR 0045; the rule is in the header of `crates/netsmith-pipeline/src/heights.rs`), on the faces the keepout's `.layers` name. A part states how tall it is with `.height`; one that does not has an unknown height, which is reported by name and never taken as zero. The check measures the part's body against the zone; the placer keeps the body, plus a small margin, out of it (owner decision 2026-11-04); a part with no fabrication outline has only its courtyard.

**Height preview**:
A switch of the 3D view, labelled "preview at N mm", that stands the parts of unknown height up at N mm in the picture only. No check, finding, report, written file or the _Height coverage_ reads it (ADR 0105).
_Avoid_: Default height, assumed height, placeholder height

**Height provenance**:
Where a part's height came from, in the 3D view, its scene and the reports: `stated` (the source or the physical file states `.height`), `catalog` (the picked row's _Catalog height_), `standard` (the footprint's _Standard height_) or `unknown` (none of them; drawn as a hatched outline, never given a height), in that order of strength (ADR 0094, ADR 0106).
_Avoid_: Height source (a row's `source` is its document), height origin, confidence
