---
status: accepted
---

# The 3D view is drawn from netsmith's own data and shared as glTF

Refs issues #414 (the 3D view) and #430 (the epic), where the owner made the six decisions below on 2026-11-11. It meets test 1 (the `.glb` file and its node ids are a format that tools and agents will read), test 2 (a newcomer expects KiCad's 3D models, a default height for an unknown part, and a viewer that builds its own geometry) and test 3 (it rejects those and the other alternatives below).

**What is drawn.** The 3D view shows only what netsmith knows. The board is its outline, cutouts and holes, as thick as the board's `Process.stackup` makes it, with each layer at the height the stackup gives (the same stackup the KiCad board file is written from, `crates/netsmith-pcb/src/export.rs`). The copper is the pads, tracks, vias and pours the board has. A part is its body (the footprint's `F.Fab` outline, else its courtyard, as a placement requirement measures it, ADR 0045) extruded to the part's height. Bodies shaped from the data the footprint generators already hold (chip, SOT, QFN and the rest, `crates/netsmith-pcb/src/generate/`) come later and replace the box where a generator made the footprint. No third-party 3D model is bundled: not KiCad's, not LCSC's or EasyEDA's, not SnapEDA's.

**No height is invented.** A part's height comes from where ADR 0094 says it does: a stated `.height`, else the picked catalog row's `height`, else none. A part with none is drawn as a hatched outline flat on the board, and the view shows the height coverage (how many of the fitted parts have a height). There is no default height. A height preview, labelled "preview at N mm", may stand the unknown parts up at N mm in the picture; it changes nothing else, and no check, finding, report or written file reads it.

**Read-only.** The first version moves nothing. A part selected in 3D is selected in the PCB view, the schematic and the source through the editor's shared selection, and is moved in the 2D PCB view, which writes the layout file as before (ADR 0032).

**One scene, written by Rust as glTF.** The Rust side builds one scene and writes it as glTF 2.0 binary (`.glb`). The editor's page displays that file; the CLI writes the same file, and so may `fab` later. Each node carries stable ids in its `extras`: the part's path in the design, its designator, the net, the finding code, and the height provenance (`stated`, `catalog` or `unknown`). So a person, an agent and a test see the same geometry and can name the same part in it.

**Headless pictures from netsmith's own rasterizer.** A PNG of the board in 3D, for an agent that can look or for a golden test, is drawn from the same scene by a software rasterizer in Rust: deterministic pixels, no GPU, no browser, no KiCad. It answers, for 3D, the rasterizer that ADR 0030 left open.

**three.js in a chunk of its own.** The page draws the scene with three.js (MIT licence), in a chunk that is loaded only when the 3D view first opens, so the editor does not grow for anyone who never opens it. WebGL 2 comes first and WebGPU later. A size test fails when that chunk is over 250 kB gzipped. It is embedded in the binary like the rest of the page (ADR 0019).

**Rejected.** Real STEP models now: their licences differ by source and some do not allow redistribution, and reading STEP needs a CAD kernel, which ADR 0045 rejects. KiCad's 3D export (`kicad-cli pcb export glb` or `step`): it needs a KiCad install (ADR 0097) and KiCad's models. A default height for a part with none, or one from the package's name: ADR 0094 refuses both, and a flat hatched part is honest where a box of a guessed height would read as checked. Building the geometry in the browser from JSON: there would be two geometry codes, and what the editor shows would differ from the file an agent reads. Headless Chromium for the PNG: a heavy install for every user, and its pixels depend on the GPU and driver. `kicad-cli pcb render`: ADR 0097. three.js in the main chunk: everyone pays for a view some never open. Loading it from a CDN: ADR 0019, no request leaves the machine. A 3D view that moves parts: a second place to edit the layout, and a harder first version.

**Cost.** A glTF writer and a software rasterizer are netsmith's own code to keep, with golden pictures that change when the drawing does. Boxes look crude next to real models, and a board whose catalog rows lack heights looks mostly flat until the rows are filled (#27). The page carries three.js and its chunk budget.

**Left open**, each for a decision of its own when its work starts: whether a package standard's nominal height may count as a height source; whether `fab` writes the `.glb` by default; a STEP export; user-supplied STEP models (local, never bundled); an enclosure STEP shown with a fit check (#84); boards joined by connectors as one 3D assembly (ADR 0040).
