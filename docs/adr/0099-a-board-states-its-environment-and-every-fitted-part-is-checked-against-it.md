---
status: proposed
---

# A board states its environment, and every fitted part is checked against it

Refs `docs/research/environment-and-grade.md`, whose recommended answers the owner accepted on 2026-11-09. Meets the first and third tests: the names `.environment`, `.temp_min` and `.temp_max` and the two catalog columns are written by projects and in their files, and it rejects the obvious reading of ADR 0058 (an optional column that a design asks for takes the empty rows out).

A board could not say that it runs from -40 C to 125 C. It has a member `.environment`, an interface of `std::env` with one field, `.temperature`, a range in kelvin, the way it has a `.process` (ADR 0008). The manifest's `environment = "src/vehicle.nsm::Automotive"` names a definition that every board gets when it declares none, as `assembly` is the project's default (ADR 0028); a board's own declaration replaces it whole, and a board with neither has no environment and says nothing. The standard library states no number of its own: a range that cannot be cited is not shipped, and a project writes its own definition with `?=` offers (ADR 0051).

Every part has two built-in members, `.temp_min` and `.temp_max`, the lowest and the highest temperature it works at, each one number, as it has `.height` (ADR 0094). A catalog may give them as the fixed columns of those names, which the part's `catalog` statement does not list. The part's own statement wins over its row's, an end at a time. The solver does not offer a row whose stated end the environment is outside of (the hot end at or under `temp_max`, the cold end at or over `temp_min`). The design stage judges every fitted part again, with the design's statements, and refuses one whose stated range does not cover the environment. A part not fitted (`AssemblyMode::None`) and a label are exempt.

A row that states an end nothing says is offered, and a guideline finding names it: the part, the row, the end, and what to write. The finding is a `warning:` and an error under `--strict`, as ADR 0065. An unknown range is not a refusal and not a pass.

Rejected, each from the research note. One range member per part: a range in this language means "somewhere in it", so `-40C..85C` would be read as an uncertain figure and compared at its lowest end. A filter in the solver with no member: it hides the rule from the language and never reaches a part with no catalog. A declaration in each kind: it repeats one declaration and a kind that forgets it is judged by nothing. Not offering a row with an empty cell, as ADR 0058 does for an asked column: only 4 of the 351 standard rows state a range, so every board with an environment would find the catalogs empty. A table of values in the manifest: a second way to say what a definition says.

Cost: a hidden relation of the checker, found in the guide and not in a part's source. Every part has two more members. A row's range is as the record prints it, and the record does not say if it is the ambient, the junction or the operating temperature, so the check is necessary and not sufficient; nothing derates a 25 C rating or adds the part's own heating. The manifest's default is a convenience and not a floor: a board of the project can declare a milder one.

The spelling of a temperature is `degC` (`-40degC`), the owner's choice of 2026-11-10, recorded in ADR 0102; a temperature in kelvin is the same value, and a message shows both.

Left open, for the owner. Whether the manifest's `module::Name` should also accept a project module's own path (`src::vehicle::Automotive`, which `import` writes) and not only the file. Which standard bundles to ship, once each range has a source.
