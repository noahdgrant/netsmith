---
status: accepted
---

# A case model is a STEP file the board is checked against, not a source of its shape

Refs issue #430, P7, the third STEP item, and #84. The owner wants an enclosure STEP shown around the board with a fit check (2026-11-12). Syntax, the std interface and what an error rule refuses were the owner's; on 2026-11-12 the owner chose the recommended option of each (#430). It meets test 1 (a std interface and a file that projects will name), test 2 (a newcomer expects the STEP to give the outline and the limits) and test 3 (it rejects that and a CAD kernel, below). The word is _Case model_: _Enclosure_ already names a solver range (ADR 0076), and the physical file says `case`.

**What it is.** A project names a STEP file of its case, in its own tree. netsmith reads it with ADR 0108's reader into triangles at 0.01 mm, places it in the board's frame (`docs/BOARD3D.md`), draws it and checks the board and the parts' bodies against its solids. netsmith never bundles or downloads one. A case model is a drawing and a check, never a height, an outline or a hole.

**Where it is named (the owner chose A).** (A) A `CaseModel` in the `case { }` section of the physical file, with the fields and the rotation order of ADR 0108's `StepModel`: `.shell: CaseModel { .file &= "case/box.step"  .z &= -1.6mm  .rx &= 180 }`, which moves the model into the board's frame. Base and lid are two members or one file with two solids. The side up is a rotation, the gap an offset. (A2) The same section, but the board's origin and turn in the case's frame, as a CAD assembly states it; netsmith inverts it. (B) A field of the board in the circuit source: refused by ADR 0048 (a case number is physical intent). (C) A `netsmith.toml` key: no expression can relate it to `.outline`.

**It complements the DXF route (the owner's choice).** The outline, cutouts, mounting holes and height limits stay statements of the case section (by hand or `dxf import`, ADRs 0045 and 0048), and the check tells where they disagree with the model. A later `netsmith case import` may print them as source from the model's section at the board plane and its inner heights (#84's ask), reviewed as `dxf import` output is. Rejected: deriving them from the STEP at build time. A maker's license often keeps the file out of the repository, so a clone or CI without it would build another board (ADR 0108's reason for not reading a model's height).

**The fit check (the owner chose what is an error).** Measured on what the height checks measure: the _Body_ to its height from ADR 0106's four sources; a part's drawn `.model` is not read (ADR 0108). Findings, each with its numbers, the part and the solid:
- `case_collision`: a body or the board's own solid overlaps a case solid by more than 0.01 mm. Touching (a board resting on a boss) is contact, not a collision.
- `case_clearance`: a body nearer a case surface than a minimum (a std field; 0.5 mm proposed).
- `case_reach`: a _Connector_ whose body stands at an edge, with the case solid between its face and the outside, or its face more than a distance inside the wall's outer face.
- `case_boss`: a mounting hole with no case solid within a gap above or below its ring.
- `case_unchecked`: a part of unknown height under case material, or a standard height that collides (ADR 0106: a part in that package may be lower).

Chosen: only `case_collision` of a stated or catalog height, or of the board, is an error, under the `constraints` key as `too_tall` is; `fab` refuses on it, and `--allow-drc-violations` never waives it. The rest are warnings; `case_unchecked` is an error with strict constraints, as `height_unchecked` is. A missing file is a warning (the fit is not checked), an error with strict constraints. Alternatives: every finding a warning until a release has run on real cases; or every finding an error with strict constraints.

**Geometry: triangles, not exact solids.** Body boxes and package shapes against the reader's triangles (separating axes, an axis-aligned prefilter), and ray parity for a body wholly inside a solid. An exact B-rep test needs surface-surface intersection, a solid kernel that ADRs 0107 and 0108 rejected. The cost is 0.01 mm of chord, well below the clearance and reach figures. A shell that is not closed gets no containment test and says so.

**Measured** on six enclosures of open-hardware projects on GitHub (MIT, Apache-2.0, GPL-2.0, CERN-OHL-P; fetched for a local run, not committed), with #528's reader, release build: 51 to 130 faces, 92 to 304 kB, 5,660 to 59,654 triangles. Reading took 11 to 92 ms for four files, 1.1 s for one with 24 B-spline faces and 6.2 s for one with 25. One file has a shell that is not closed. 400 bodies against every triangle, unindexed: 2.5 to 36 ms; 400 containment rays: 20 to 269 ms. The check is cheap; the B-spline faces are the cost, which a cache keyed by the file's SHA-256 meets. Hammond's site refuses scripted downloads, so no maker's box was measured; die-cast boxes have drafts and fillets this set lacks.

**How it is shown.** The 3D view draws the case see-through (a material `case`, as `limit` is), with a toggle; a finding flies the camera to its part and lights the solid. `render` draws it see-through, `--highlight case:<name>` lights it, `--section` cuts it. `board3d --json` lists the models (file, SHA-256, solids, triangles) and the findings; the `.glb` holds the case's triangles under `case:<name>` with the SHA-256 in `extras`. The STEP export leaves the case out (the owner chose it): the engineer who reads it has the case, and the copy would spread the maker's file. Each `fab` output that holds the model is the user's to share under its license, as ADR 0108 says.

**Cost.** A check module and a material in the scene, the editor and `render`. A case model makes `build` and the editor slower by its reading time. Two homes for the outline (the statement and the model) are kept apart on purpose, and their disagreement is a finding.

**Left open.** `case import`; the parts' `.model` triangles in the check; a case of several boards (a `system`, ADR 0040); the clearance and reach figures, in the std doc comments.
