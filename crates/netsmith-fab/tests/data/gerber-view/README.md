# Gerber viewer fixtures

Files the repository did not write, for the Gerber viewer's tests (`tests/gerber_view.rs`,
issue #1105). They are copied unchanged, byte for byte, from the test fixtures of tracespace,
a set of PCB viewing tools under the MIT licence ("Copyright (c) 2015-present, Michael Cousins
and contributors"): <https://github.com/tracespace/tracespace>, commit
`49c9a22addccc70c1d1a830a9225328a91942080`, folder `packages/fixtures/gerbers/`. Only the
names changed, so that the viewer finds a function for each by its extension (Protel's names).

`tracespace/`, a package of five files:

| File | Upstream file | What it has |
| --- | --- | --- |
| `tracespace.gtl` | `macros/clear-with-thermal.gbr` | a square pad, a thermal macro (primitive 7) flashed clear over it; inches |
| `tracespace.gts` | `pads/circle-with-hole.gbr` | a circle aperture with a hole |
| `tracespace.gto` | `strokes/rect-tool-multi-segment.gbr` | strokes of a rectangular aperture |
| `tracespace.gko` | `regions/region-with-arcs.gbr` | a region with single-quadrant arcs (`G74`), format 1.1 |
| `tracespace.drl` | `drill/slot.drl` | a `G85` slot, inches, trailing zeros kept |

`tracespace-unsupported/`, constructs the reader does not draw, beside one it does:

| File | Upstream file | What it has |
| --- | --- | --- |
| `one-polarity.gbr` | `step-repeats/one-polarity.gbr` | a step and repeat (`%SR`) |
| `arc-route.drl` | `drill/arc-route.drl` | slots routed along arcs (`G02`, `G03`) |
| `circle-with-hole.gtl` | `pads/circle-with-hole.gbr` | a circle aperture with a hole |

`golden.txt` holds the FNV-1a hash of the PNG bytes of each picture the tests draw, of these
files and of the Gerbers netsmith writes for `../features.kicad_pcb`.
