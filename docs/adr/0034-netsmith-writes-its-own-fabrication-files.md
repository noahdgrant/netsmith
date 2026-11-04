---
status: accepted in design, provisional until a package has been through JLCPCB's order preview (owner review 2026-11-04)
---

# netsmith writes its own fabrication files

ADR 0021's "Direction" says netsmith must not need KiCad installed. Step 2 of the roadmap is the fabrication files: until now `kicad-cli` plotted the Gerbers, drilled the Excellon files and wrote the position file. Now netsmith writes all three from the board file, and `kicad-cli` is only an optional witness. The owner accepted the design on 2026-11-04. The ADR stays provisional until the condition under "When this ADR stops being provisional" is met.

## What was built (`crates/netsmith-fab`)

- **`artwork`** reads everything the files need from a `.kicad_pcb`: pads (every shape KiCad has, with the footprint's turn, drill, margins, zone connection and clearance), vias, tracks and track arcs, zones with their rule areas, drawings and text, the origin and the mask settings. It is the only reader of the board for plotting, so a board ported from KiCad plots as well as one netsmith wrote. What it cannot make it refuses with the pad, via or zone named: a pad with a `padstack`, a blind or buried via, a zone filled in hatch mode, a custom pad primitive other than line, arc, circle, rectangle or polygon.
- **`plot`** writes one RS-274X (Gerber X2) file for each layer: copper (`F.Cu`, `In1.Cu`, `In2.Cu`, `B.Cu`), `F.Mask` and `B.Mask` (a negative: the openings), `F.Paste`, `B.Paste`, `F.SilkS`, `B.SilkS` and `Edge.Cuts`. The names, the folder `gerber/`, the zip and which back layers are left out when empty are the layout JLCPCB accepted (unchanged, `gerber.rs`). Format 4.6 in millimetres, origin the drill/place origin, y up, apertures numbered by first use, the same `RoundRect`, `RotRect` and `HorizOval` macros KiCad writes, `TF`, `TA.AperFunction` and `TO` attributes (pad, net, component) as KiCad writes them. There is no creation date, so equal input gives equal bytes. A pad is a flash where the shape allows it (circle, rectangle, obround, rounded rectangle, and the turned ones through macros) and a region where it does not (trapezoid, chamfered corners, custom pads: the anchor and each primitive).
- **Mask and paste** use the pad's own margins first, then the board's (`pad_to_mask_clearance`, `pad_to_paste_clearance` and its ratio). KiCad's rules, found by the cross-check: mask grows a shape by the margin all round (a rectangle gets round corners of that radius); the paste ratio is of each side of the pad, not of the shorter one (a 1.6 by 1.0 pad with a ratio of -0.1 loses 0.16 along and 0.1 across, each side) and a rounded rectangle keeps its corner ratio; a via is open in the mask only on a face where it is not tented (`(tenting ...)` of the board or of the via); a hole with no copper round it (a pad no larger than its drill) has no copper on any layer.
- **`fill`** fills zones. It is step 4's job in the roadmap, but a board with pours cannot be plotted without it, and KiCad's fill needed KiCad. It uses polarity and no polygon arithmetic: the zone's outline as dark copper; then, in clear polarity, the outside of the board and a band `copper_to_edge` wide along every edge, rule areas that forbid copper pours, the clearance round every pad, via, track and drawing of another net (the larger of the zone's and the pad's own), the hole clearance round holes with no copper, and the thermal gaps round the pads of the zone's own net, drawn as the four pieces between the spokes so that the spokes stay copper (gap and spoke width from the zone; spokes along the pad's own axes); then every pad, via and track dark again. A via of the pour's net is joined solid, as KiCad joins it (the cross-check showed that). Fills the file already holds (`filled_polygon`) are used as they are and not filled again. Two zones of different nets that overlap on one layer are refused.
- **`drill`** writes `<board>-PTH.drl` and `<board>-NPTH.drl` (both always, as before) in the format JLCPCB took: Excellon, metric, decimal coordinates, tools by size with vias first, a slot as a routed path (`G00`, `M15`, `G01`, `M16`, `G05`) with a tool as wide as the slot.
- **`cpl`** takes the position and turn of each part from the board file, relative to the same origin as the Gerbers, y up. It no longer reads a position file (the old reader stays for the cross-check). The rotation offsets and the notes about unverified rotations are as before.
- **`font`** is a stroke font of our own (about 100 glyphs: capitals, digits, lowercase, punctuation, the micro and ohm signs) for silkscreen text. KiCad's font was not used: its glyph data is not ours to copy into the repository, and a few hundred line segments are easy to make. Capitals are as tall as the text and a `D` is 0.6 of that wide, which is what KiCad's font measures. Text keeps its size, thickness, justification, turn, mirror (back layers) and italic. A character the font lacks is drawn as a box and named in `fab-notes.txt`; it is never dropped.
- **`raster`** is a Gerber and Excellon reader with a scanline rasteriser, written for the cross-check and for proving our own files load. It supports apertures C, R, O and P, macros with expressions and the primitives KiCad writes, regions with arcs, clear polarity, aperture transforms, and says the command and line of anything it does not know.

## One way

The writers are the way: there is no flag that gives back KiCad's files. `package` takes no KiCad. `kicad-cli` (`gerbers`, `drill`, `pos`, `drc_json` of the `KicadCli` trait) is used by the cross-check and by tests.

## The design rules check without KiCad

Replaced by ADR 0037: netsmith has a design rules check of its own, which is the gate in `fab`, so there is no "not run (KiCad not installed)" path any more, and `--allow-drc-violations` waives only real violations. `package` takes no KiCad at all; `KicadCli` (`gerbers`, `drill`, `pos`, `drc_json`) is for the cross-check.

## The cross-check

`netsmith fab --cross-check` (and `netsmith_fab::crosscheck::check`) plots the written board with KiCad as well and compares, then fails with the differences and writes a picture of each layer that differs (`cross-check-<file>.png`: grey where both draw, white only netsmith, dark grey only KiCad). It needs KiCad. The method, chosen from three (comparing the Gerber text, comparing lists of flashes and draws after normalising, comparing bitmaps): **bitmaps**. Text comparison breaks on harmless differences (macro or region, an arc kept or cut, order); a semantic list needs a second parser that understands every aperture and macro anyway; the rasteriser is that parser and then it does not matter how a shape was written. Both sets are rendered over one view at 40 pixels per millimetre (0.025 mm) and compared twice: the fraction of pixels that disagree, and the fraction with nothing of the other picture within a tolerance (0.05 mm). Limits, measured on the five examples, a synthetic board with every pad shape and feature (`crates/netsmith-fab/tests/data/features.kicad_pcb`), and synthetic boards of 10, 70 and a land grid (`crates/netsmith-pcb/tests/synthetic.rs`):

| Layers | Worst seen | Limit |
|---|---|---|
| copper, mask, paste, outline | 0.45% of pixels, 0.00% beyond the tolerance | 1% and 0.1% |
| copper with a pour | 0.30% of pixels, 0.23% beyond the tolerance | 1% and 0.5% |
| silkscreen, compared to 0.6 mm because the font is ours | 0.00% beyond 0.6 mm | 5% |

Drill files are compared hole by hole (position, size, slot ends, to a micrometre) and the placement file row by row; both agree exactly on every board tried, including a part on the back turned 90 degrees, a turned slot and a part at 45 degrees. A real mistake is far from these limits: the unit tests move a pad a third of a millimetre and it fails; while the writers were made, a missing default (a rounded rectangle with no ratio is 0.25), a via joined with spokes, a pad's own clearance, the hole clearance and the paste ratio were each found by the picture of the difference.

The pour's difference is islands and spokes: KiCad removes a spoke or a patch of copper that no pad of the net reaches; netsmith leaves it. It is copper outside every clearance, so it cannot make a short, but it is dead copper.

## What is not checked, and the limits

- JLCPCB has not been given these files. They were accepted by KiCad's own Gerber loader only in the sense that KiCad's plot of the same board agrees with them; the check that matters, JLCPCB's order preview and its own design check, is the user's. Every earlier package that JLCPCB accepted was KiCad's, so the notes have no claim that these are accepted. Clear polarity inside a copper file (the pours) is what CAD tools write for planes, but KiCad does not, so look at the planes in the order preview first.
- No other Gerber viewer was available to script. The only independent readers are KiCad's plot (compared, not loaded) and the rasteriser written here, which reads KiCad's own files as well as ours (a test renders every file of three KiCad packages).
- Fill: copper islands and slivers narrower than the zone's minimum thickness are not removed; a spoke into another net's clearance is kept as a stub; `connect_pads` is read (thermal, solid, none, through-hole only), the per-pad override too; hatched fills, zone priorities and per-zone island settings are not.
- A pad's `remove_unused_layers` is approximated: an inner layer keeps the pad if a track of its net ends on the pad or a zone of its net covers it (KiCad asks the connectivity engine).
- Text: our font, so the silkscreen's letters differ from KiCad's; `bold` is ignored; text boxes, curves (`fp_curve`) and images are not plotted and are named in the notes.
- Without a `.kicad_pro` beside the board the zone fill uses no edge clearance and the zone's own hole clearance (netsmith always writes one).

## When this ADR stops being provisional

One real package goes through JLCPCB's order preview and JLCPCB's own design check (issue #12). The person who does it looks at the planes (the pours are written in clear polarity inside a copper file, which KiCad does not do) and at the silkscreen (our font), and writes the result into this ADR: the board, the date, what the preview showed, and any file JLCPCB refused or changed. Only the owner can do this, since it needs an order account. Until that entry exists, nothing in this ADR claims that JLCPCB accepts these files.

Result: none yet.

## The roadmap

Step 2 is done. Step 4's zone fill is done in the form above; reading `.kicad_sch` and `.kicad_pcb` for `compare` is not. Step 3 (our own DRC) is done: ADR 0037.
