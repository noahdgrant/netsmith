---
status: accepted (user review 2026-11-02)
---

# netsmith's own design rules check

ADR 0021's "Direction" says netsmith must not need KiCad installed. ADR 0034 made the fabrication files netsmith's own and left the design rules check to `kicad-cli`: without KiCad `fab` refused to package, or packaged with a note that nothing had been checked. This ADR adds the check, and `fab`, `fab --order` and the editor's Fab button now gate on it. KiCad's check stays as a witness (`fab --cross-check`, the tests) and as a second opinion in the editor. The choices below are provisional, to be reviewed by the maintainer.

## Where it lives, and what it reads

**In `netsmith-fab` (`crates/netsmith-fab/src/drc/`), over the `.kicad_pcb` text and the `.kicad_pro` beside it**, not in `netsmith-pcb` over its `Board`. Three reasons. The package, the Gerbers and the cross-check already work from the file, through one reader (`artwork`), so the check sees exactly what the board house will make, and a board ported from KiCad is checked like one netsmith wrote. `package` calls the check directly: a separate `netsmith-drc` crate would have to sit above `netsmith-fab` (it needs `artwork`, the font and the outline) and `package` could then not call it. And KiCad reads the same two files, so the two checks can be compared on identical input. The editor writes the file in memory (`kicad_pcb(board)`, `kicad_pro(board)`) and checks that; the Board model's own `check_placement` (parts, courtyards as boxes, before anything is routed) stays and is not replaced.

The numbers are the project's: `min_clearance`, `min_track_width`, `min_via_diameter`, `min_via_annular_width`, `min_through_hole_diameter`, `min_hole_to_hole`, `min_hole_clearance`, `min_copper_edge_clearance`, `min_silk_clearance`, the text sizes, each net class's clearance with its net patterns and priority, and each rule's severity. A number the file lacks is KiCad 9's default. The board file's `solder_mask_min_width` is the least mask web (the exporter now writes the process's `mask_bridge` there, so KiCad reads it too); `fab` also passes the process's `via_in_pad` (the widest via the fabricator fills) as an extra, because no KiCad file holds it. The project's `rule_severities` decide what stops `fab`: the exporter now writes `silk_over_copper`, `silk_overlap`, `silk_edge_clearance` and `solder_mask_bridge` as warnings (a fabricator clips silkscreen over a mask opening and keeps a web only where it can; neither stops a board being made) and everything else as errors.

## Findings are data

`drc::check(pcb, project, extras)` returns a `Report` of `Finding`s: the `Rule` (named as KiCad names it, `clearance`, `hole_to_hole`, ...), the severity, a message with the numbers (`clearance is 0.1000 mm between SMD pad 1 [A] of S1 and Via [E]...; the least is 0.1270 mm`), the place in mm and the layer, and the items (what they are, where KiCad would say they are, the part and pad, the net). The package's notes keep their wording (`design rules check: passed`, `... NOT clean, packaged anyway: N violation(s), M unconnected pad(s), K footprint error(s)`) and add `design rules warnings: N` with each warning. The editor's Findings tab lists the findings of a routed board with source `drc` (click to zoom, as for the others); it leaves out the missing connections (the ratsnest's, listed as `unrouted`) and overlapping courtyards (the placement check's), and KiCad's check is a button, "Also ask KiCad". `Project.kicad` stays in the API for that button and is no longer a precondition of anything; the "allow violations (no KiCad)" box is now "allow violations", shown after the check has stopped a package.

## Rules

| Rule | Checks |
|---|---|
| `clearance` | copper of different nets nearer than the net classes' clearance, pad's own clearance or the board's least: track, pad and via against each other, on each layer (KiCad reports a pair once per shared layer, except two pads, once; so does netsmith) |
| `shorting_items`, `tracks_crossing` | copper of two nets that touches; two tracks that cross |
| `unconnected_items` | the pieces of each net (pads, vias, tracks, pours), joined by the shortest connections between them, with both ends' places |
| `track_width`, `via_diameter`, `annular_width`, `drill_out_of_range` | sizes against the least; annular ring of vias and plated holes, slots included |
| `hole_to_hole`, `hole_clearance` | edge to edge between holes; a hole against copper of another net on any layer |
| `copper_edge_clearance`, `hole_to_edge` | copper (and plated holes through their pads) to `Edge.Cuts`; a hole with no copper to the edge, by the same least (netsmith's own rule: KiCad has none) |
| `courtyards_overlap`, `malformed_courtyard` | courtyards on one side that share area (touching is not overlap); a courtyard that is not closed |
| `silk_over_copper`, `silk_overlap`, `silk_edge_clearance`, `text_height`, `text_thickness` | silkscreen on a pad's mask opening; too near other silkscreen or the edge; text smaller than the process allows |
| `solder_mask_bridge` | openings of two pads of different nets that run together, or leave less web than the process makes |
| `items_not_allowed` | a track, via, pad or part inside a rule area that forbids it (a part's own rule area does not forbid the part) |
| `track_dangling`, `via_dangling` | a track end that touches nothing of its net; a via joined on fewer than two layers |
| `invalid_outline` | `Edge.Cuts` that is not closed, no outline, a cutout outside the board |
| `via_in_pad` | a via in a pad wider than the process fills (netsmith's own) |

Geometry is exact for what a board is made of: every shape is a path grown by a radius (a track, a round pad, an oval, a rounded rectangle's polygon) or a solid polygon (custom pads are their anchor and primitives), and the gap between two shapes is the least distance between their paths less both radii. A uniform grid finds the pairs that can be near. A measurement may fall short of its limit by half a micrometre and pass, as in KiCad.

## What KiCad does that the comparison taught

Found by comparing, and now mirrored: a pad's angle in the file is absolute and a missing angle is 0 (`artwork` took the footprint's turn; this changed the Gerbers of a board that omits it, as KiCad's do); two tracks of different nets that cross are `tracks_crossing`, not a short; KiCad reports each layer of a via's clearance but a pair of pads once; a zero-length missing connection is not drawn; an open courtyard is reported twice (not closed, self-intersecting); text is compared with any silkscreen but two drawings only if they do not belong to the same part or both to the board; mask openings of pads with no net count against pads with one; mask openings that only touch are not a bridge when the process asks for no web; a track's place is its start and an arc's its centre; a missing connection's ends are the nearest anchors, and where several are equally near KiCad takes the one on a shared layer, then a track, then the longer (the two checks still pick differently at times, and the comparison counts a missing connection that names other ends as agreeing).

## The pour

Neither check computes the pour's fill for clearance: netsmith's fill (ADR 0034) is "everything but the clearances" by construction, so a pour is never checked against other copper, and a fill the file already holds is not either. A pour joins every pad, via and track of its net that touches its outline on its layer; a zone or a pad set to no connection does not join the pad. A pad the real fill would cut off (a spoke into a keepout, an island) is not seen: that is KiCad's `isolated_copper`, `starved_thermal` and `copper_sliver`, which netsmith does not check.

## The validation

The cross-check compares findings by rule and by the places of the items (0.1 mm), in both directions, and fails on a finding either side lacks. A rule only netsmith has (`hole_to_edge`, `via_in_pad`) and a kind of finding netsmith does not check (`OUT_OF_SCOPE` in `drc/kicad.rs`: library mismatches, slivers, starved thermals, isolated copper...) are counted and not compared.

- **A corpus of 28 broken boards** (`crates/netsmith-fab/tests/drc_corpus.rs`, generated by `tests/common`): a track, pad or via too close to each other, shorts, crossing tracks, thin tracks and small vias, thin rings, small and close holes, a hole against a track, copper and holes at the edge, courtyards touching, overlapping and inside each other, an open courtyard, silkscreen over pads and over silkscreen and at the edge, small text, mask bridges, rule areas, missing connections, dangling tracks and vias, an open outline, planes, and pairs on four layers. 127 findings agree with KiCad's, none differ.
- **The synthetic feature board** of ADR 0034 (every pad shape, turned footprints, slots, arcs, planes, a pour): 20 findings, all agree.
- **The tests that already ran KiCad's check** (`crates/netsmith-pcb/tests/kicad_drc.rs`: the examples routed and unrouted, 33 hard boards, the 70-part synthetic board, land grids with vias in pads, holes, custom pads, keepouts, cutouts, rounded outlines, parts on both faces, wide tracks, four-layer planes) now run netsmith's check on the same files and fail on any difference: 37 boards, 163 findings agree, none differ (the 193 findings of KiCad's that are left out are all about its library tables: `lib_footprint_issues` and `lib_footprint_mismatch`, because the tests' footprints are not KiCad's).
- **Random boards** (`crates/netsmith-pcb/tests/drc.rs`): 100 seeded boards routed by netsmith have no error in netsmith's check and the same number of missing connections as the ratsnest (before and after routing); a sample compared with KiCad's, and four synthetic boards, agree; a synthetic board of 70 parts, 711 tracks and 161 vias is checked in 0.2 s in a debug build.
- A copy of a real project's board, routed by netsmith (nothing of it is in the repository, so no figures from it are kept): no error; KiCad agrees on the warnings except three, which differ for the reasons below (two silkscreen texts that our font puts a hair nearer a pad, and a mask opening drawn as a graphic on `F.Mask`, which is not checked). Running it found the footprint-level settings below.
- All six examples with `fab --cross-check`: both checks find nothing.

## Found by running on a real board

A library footprint can state `solder_mask_margin`, `solder_paste_margin`, `solder_paste_ratio`, `clearance` and `zone_connect` for all its pads, and `artwork` read only a pad's own. They are read now (a pad's own wins), which also corrects the mask and paste Gerbers of such footprints. Rule areas that a library footprint carries (an antenna module's) are not enforced, as KiCad does not enforce them: only the board's own are (a keepout a part carries in the source is the board's, ADR 0029). A courtyard drawn with lines of no length (a library does) is closed by the other lines.

## What is not checked

Impedance and controlled-impedance rules, acid traps and copper slivers, islands and starved thermals of a pour, a pour's fill (so a pad the fill cannot reach), differential pairs and lengths, custom rules (`.kicad_dru`), blind, buried and micro vias, stackup rules, unrouted-by-design nets, footprint-against-library and schematic parity (KiCad's `lib_footprint_*`, `extra_footprint`), and the electrical rules check (the solver does most of it). Silkscreen text uses netsmith's own font (ADR 0034), so a text that only just clears a pad can be judged differently; a mask opening drawn as a graphic on `F.Mask` or `B.Mask` is not an aperture the mask-bridge check sees. JLCPCB's own check has not been run on a package; the cross-check says netsmith agrees with KiCad, not that a fabricator will accept the board.

## Consequences

`package` has no KiCad argument; `KicadCli::drc` is `drc_json` (every severity, as KiCad's JSON). `Options::drc` carries the process's extras. `fab --cross-check` also compares the findings, and its report says how many agree. The editor's Fab button, the order button and the CLI work with no KiCad installed. `Package::drc_report` holds all the findings for a caller to show.
