# Easyduino ESP32-S3

A port of the open **Easyduino ESP32-S3** board to netsmith, the second showcase board of epic
#308 (after the RP2350A Minimal). **The port is in progress: this folder holds the first stage
only**, the scaffold, the licence and the reference table. `src/main.nsm` is a placeholder
board (an LED on a supply) that stands in until the parts and the circuit are written; it is
not Easyduino's circuit.

## Source

- The design: <https://github.com/Hanqaqa/Easyduino>, folder `ESP32S3/`, read at commit
  `53b14b66d64f25c55f88971c1c1b51656126bd30` (committed 2026-06-28). KiCad 10 files: the board
  has file format version 20260206 and the schematic 20260306 (`generator_version "10.0"`).
  Neither file has a title block, so the design has no revision or date of its own.
- What it is: an ESP32-S3-WROOM-1 module board that replicates Espressif's ESP32-S3-DevKitC-1:
  a CP2102N USB-UART bridge (U3), an AMS1117-3.3 regulator (U5), two USB-C receptacles (J1 for
  the bridge, J2 for the module's own USB), a BOOT and a RESET button (SW2, SW1), two
  auto-reset transistors (Q1, Q2), a power LED (D1) and a GPIO38 LED (D4), and the development
  board's two 22-pin headers. One schematic sheet of parts (`Easyduino_ESP32S3.kicad_sch`, with
  a second sheet, `References.kicad_sch`, that holds only text notes), 35 footprints, four
  copper layers (F.Cu, In1.Cu, In2.Cu, B.Cu), 1.6 mm.
- Licence: **CERN-OHL-P-2.0** (the CERN Open Hardware Licence Version 2, Permissive). The
  author's `README.md` says "this project is distributed under the CERN Open Hardware Licence
  Version 2 - Permissive ... As long as you include a copy of the CERN OHLv2 Permissive
  Licence", and the licence text is `License.txt` at the root of the repository. Neither that
  file nor the design files state a copyright holder or a year; the repository's owner is
  `Hanqaqa` on GitHub. `LICENSE` here is `License.txt`, byte for byte, and stays with every
  copy of anything in this folder that comes from their design (`reference/`). The licence asks
  for the Notices to be kept (section 3.1) and, for a modified copy that is conveyed, a Notice
  that it was modified, with the date and a short description (3.3 b): **modification notice,
  2026-11-10: netsmith's files in this folder are not Covered Source copied from the design;
  `reference/placements.toml` is data derived from the board file (positions, pads and nets
  written out as a table), and the port that follows is written anew in netsmith's language.**
- The reference board takes its module's symbol and footprint (`Espressif:ESP32-S3-WROOM-1`)
  and the development board's footprint (`Espressif:ESP32-S3-DevKitC`) from Espressif's
  `kicad-libraries`, which the author keeps as a copy in `ESP32S3/Esp32_Kicad_libraries/` with
  Espressif's own `LICENSE.md`. **Those libraries are not copied here.** The board file carries
  the two footprints inside it (so `reference/` shows what they are), and the module's footprint
  in the port will be written by netsmith from the module's datasheet, not copied.

**The circuit is Easyduino's. The port, the netsmith source and the tools in this folder are
netsmith's** (MIT, like the rest of the repository).

## Goal

1. **The same nets.** The schematic of the port and Easyduino's design are the same circuit:
   a test will prove it by comparing the wiring alone, as the RP2350A Minimal's does.
2. **A matching PCB where it matters.** A board made by `netsmith fab` is interchangeable with
   theirs in these ways, and the parity checker with tolerances will fail when one is off:
   - the board outline (25.86 x 63.90 mm; see NOTE-FOR-LATER: the file draws it in U2);
   - the two USB-C receptacles (J1, J2), the buttons (SW1, SW2) and the headers (U2): the same
     positions, rotations and pads, with each header pin carrying the same signal.

   The other parts (module, bridge, regulator, transistors, LEDs, passives) may be anywhere,
   and the routing will differ; the checker reports them and does not fail on them. The board
   has no mounting holes: the only non-plated holes are the two 0.65 mm pegs of each receptacle
   (listed as holes of J1 and J2 in the table, and must-match with them).

## What is here

- `netsmith.toml`, `src/main.nsm`, `src/main.lock.toml`: the project, its (placeholder) board and
  the board's pick lock (`netsmith lock --update` refreshes it; CI checks it).
- `LICENSE`: the CERN-OHL-P-2.0 text, as the author ships it.
- `reference/Easyduino_ESP32S3.kicad_pcb`, `Easyduino_ESP32S3.kicad_sch`, `References.kicad_sch`:
  their board and schematic files, unchanged (1.0 MB, 260 kB, 6 kB).
- `reference/Easyduino_ESP32S3_BOM.csv`, `Easyduino_ESP32S3_position-top-pos.csv`: the
  author's JLCPCB bill of materials (reference, value, LCSC number) and component placement
  list, from `ProductionFiles/BOM/`, unchanged. Evidence of the LCSC numbers; the position
  list has y up (the board file has y down).
- `reference/must_match.toml`: which footprints a copy must reproduce (J1, J2, SW1, SW2 and U2),
  with the reason for each.
- `reference/placements.toml`: the facts the checker needs, read from the board file: the
  outline, the holes and, for every footprint, its place, rotation and pads. The header of the
  file says what the origin and axes are, and which entries must match (`must_match = true`).
  Regenerate it with
  `cargo run -p netsmith --example reference_table -- --must-match reference/must_match.toml reference/Easyduino_ESP32S3.kicad_pcb > reference/placements.toml`
  (run from this folder); a test checks that the file is what the program prints.

## Run it

From this folder:

```sh
netsmith build    # the schematic, in out/
netsmith fab      # the package for the board house, in fab/
```

Both build the placeholder for now. `netsmith compare` can read their board file as one side
(`netsmith compare reference/Easyduino_ESP32S3.kicad_pcb out/EasyduinoEsp32s3/EasyduinoEsp32s3.kicad_sch`)
and says nothing useful until the port is finished.

## NOTE-FOR-LATER

Facts for the next stages (parts, the board and its proof, the physical file); read from the
files in `reference/`, nothing from a datasheet.

**What the reader had to learn for this file.** The board's outline is not drawn on the board:
it is four `fp_line`s on `Edge.Cuts` inside the footprint U2 (`Espressif:ESP32-S3-DevKitC`, at
128.605, 59.75136, no rotation), in U2's own frame. The table's reader looked at the board's
own graphics only and found no outline. It now places a footprint's `Edge.Cuts` shapes on the
board. The rectangle spans x 127.105 to 152.965 and y 58.6514 to 122.5486 (25.86 x 63.8972
mm), no rounded corners. The board has no `gr_*` item on `Edge.Cuts`. A copy made by netsmith
will draw its outline itself in the physical file; U2 is not a part to place.

**U2 is the headers.** It has 44 `thru_hole` oval pads (1.2 x 2 mm, drill 0.8 mm, pitch 2.54
mm), two rows 22.86 mm apart: pads 1 to 22 at x = 0, top to bottom, and pads 23 to 44 at x =
22.86, bottom to top (a U-shaped numbering). Pads 20 to 22 are off the 2.54 mm grid by
3.7 um in x and 2.7 um in y (x = -0.00368 and y = 48.25728, 50.79728, 53.33728, where the grid
says 0, 48.26, 50.8, 53.34). U2's
pads carry the signal each header pin has, by net name (the pads have no names): 1 and 2 `+3.3V`,
3 `/EN`, 4 to 7 `/GPIO4` to `/GPIO7`, 8 to 12 `/GPIO15` to `/GPIO18` and `/GPIO8`, 13 to 20 `/GPIO3`,
`/GPIO46`, `/GPIO9` to `/GPIO14`, 21 `VBUS`, 22 and 23 and 24 `GND`, 25 and 26 `/USB2_D-` and
`/USB2_D+`, 27 to 41 `/GPIO21`, `/GPIO47`, `/GPIO48`, `/GPIO45`, `/GPIO0`, `/GPIO35` to
`/GPIO42`, `/GPIO2`, `/GPIO1`, 42 and 43 `/GPIO44` and `/GPIO43`, 44 `GND`. In the schematic it
is one 44-pin symbol, not headers. It is the one part of the 35 with no LCSC number (`-` in the
BOM): it is not bought.

**The module, U1** (`Espressif:ESP32-S3-WROOM-1`, LCSC C2913197 in the BOM): 49 pads, all `smd
rect`, no castellated half-holes (the edge pads are plain pads) and no drills: 40 edge pads of 1.5 x 0.9 mm (14 down the left,
12 along the bottom, 14 down the right, numbered 1 to 40 counter-clockwise from the top left;
the bottom and right pads are turned 90 and 180 degrees in the file) and the ground pad, which
is **nine pads of 0.9 x 0.9 mm all numbered 41**, on a 1.4 mm grid; pad 40 is also ground. The
module's footprint is the hard part: a pin table, and a footprint with a nine-pad pad 41 (or
one pad). The file has no keep-out zone (its five zones are all pours on a net). The pads'
nets: pad 3 `/EN`, 13 and 14 `/USB2_D-` and `/USB2_D+`, 27 `/GPIO0`, 31 `/GPIO38` (the LED D4
through R9), 36 and 37 `/GPIO44` and `/GPIO43` (to U3 pads 26 and 25).

**Pads that share a number**: U1 pad 41 (9 pads), each receptacle's shield `S1` (4 pads),
U5's pad 2 (the output pin and the tab, 2 pads). `Netlist::from_kicad_pcb` takes a repeated
number on one net as one pin; the table lists every pad. **Pads with no number**: the two 0.65
mm pegs of each receptacle (`np_thru_hole`), and nine 0.9 x 0.9 mm paste-only pads on U3's
exposed pad (layer `F.Paste` alone). The 20 pads on nets named `unconnected-(...)` are pins the
design leaves open (16 pins of U3 and the receptacles' SBU1 and SBU2): netsmith
will need them stated as unused.

**The USB-C receptacles, J1 and J2** (`USB_C_Receptacle_G-Switch_GT-USB-7010ASV`, LCSC
C2765186): 22 pads each: 16 `smd rect` signal pads on one row at y = -3.725 (A1, A4, A5, A6,
A7, A8, A9, A12 and B1, B4 to B9, B12; widths 0.6 mm for the power and ground pads and 0.3 mm
for CC, D+, D- and SBU, 1.24 mm long, A and B on the same row but different x), four plated
**oval-slot** shield pads all numbered `S1` (1 x 2.1 mm and 1 x 1.8 mm, slots 0.6 x 1.7 mm and
0.6 x 1.4 mm) and two non-plated 0.65 mm pegs. No rotation (0), both at y = 119.925, x =
133.08 and 146.95; the receptacle's origin is 2.6236 mm from the bottom edge of the outline.
Nets: A1, A12, B1, B12 and `S1` are `GND`; A4, A9, B4, B9 are `/VBUS1` (J1) or `/VBUS2` (J2)
and each reaches `VBUS` through a Schottky diode (D2, D3); A5 `/CC1_1`, B5 `/CC2_1` (J2:
`/CC1_2`, `/CC2_2`), each on one 5.1 k resistor to ground (R2, R3, R7, R8); A6 and B6 are
`/USB1_D+` tied together, A7 and B7 `/USB1_D-` (J2: `/USB2_D+`, `/USB2_D-`, which go to the
module's pads 14 and 13 and to U2 pads 26 and 25 with no series part); A8 and B8 (SBU) are
unconnected. J1's data goes to the bridge, U3 pads 4 and 5.

**Buttons**: SW1 (pad 1 `GND`, pad 2 `/EN`, no rotation) is RESET and SW2 (pad 1 `GND`, pad 2
`/GPIO0`, turned 180) is BOOT; both `SW_Push_SPST_NO_Alps_SKRK`, LCSC C720477, two `smd
roundrect` pads of 0.8 x 2 mm at x = -2.1 and 2.1.

**Other parts and their LCSC numbers** (from the BOM and the board's `LCSC` property, which
agree): C1 1 uF C52923; C2 to C5, C8, C9 100 nF C1525; C6, C7, C10 22 uF tantalum
(`CP_EIA-3216-10_Kemet-I`) C11366; D1, D4 LED C434447 (the same part, so the colour is not in the
file); D2, D3 Schottky C2837790; Q1, Q2 NPN (`SOT-23W`) C916392; R1, R4, R5, R10 10 k C25744; R2, R3, R6 to R8, R11 5.1 k
C25905 (R6 is D1's resistor from `VBUS`, R11 pulls the bridge's `/~{RST}` up to `VBUS`; the
author's notes on the schematic say 4.7 k for that pull-up); R9 2.4 k C25882 (D4's resistor);
U3 `CP2102N-Axx-xQFN28` C6568; U5 AMS1117-3.3 C6186. Every part but U2 has a
number. The BOM's value is the schematic's field ("D_Schottky", "Q_NPN_BCE"): no manufacturer
part names beyond the LCSC numbers, and no datasheet checked here. The catalogs will need these
numbers recorded with a source before they enter `data/jlcpcb/` (see its README): the BOM
here is a source for the number, not for the specification.

**Nets**: names are as KiCad writes them: `GND`, `+3.3V`, `VBUS` (no slash)
and `/EN`, `/GPIOn`, `/USB1_D+`, `/VBUS1`, `/~{RTS}`, `/~{DTR}`, `/~{RST}`, `/~{SUSPEND}`
(a slash for a label of the sheet, `~{...}` for an overbar), plus the generated `Net-(U3-VDD)`,
`Net-(Q1-B)`, `Net-(Q2-B)`, `Net-(D1-A)`, `Net-(D4-A)`. The auto-reset: Q1 has pad 1 on
`Net-(Q1-B)` (R4 joins that to `/~{DTR}`), pad 2 on `/EN` and pad 3 on `/~{RTS}`; Q2 has pad 1
on `Net-(Q2-B)` (R5 joins that to `/~{RTS}`), pad 2 on `/GPIO0` and pad 3 on `/~{DTR}` (the
symbol's name, `Q_NPN_BCE`, lists pins 1 to 3 as base, collector, emitter). C4 and C8 are on
`/EN`, C9 on `/GPIO0`. The supply: `VBUS` feeds U5 (pad 3 in, pad 2 out `+3.3V`,
pad 1 `GND`); `+3.3V` feeds U1, U2 pins 1 and 2 and C5, C7, C10. Five pours:
`GND` on F.Cu, In1.Cu and B.Cu, `+3.3V` on F.Cu and In2.Cu; 137 vias, all 0.5 mm pad and 0.3
mm drill, F.Cu to B.Cu; 703 track segments, 338 on F.Cu and 365 on B.Cu, none on the inner
layers. The stackup in the file: 35 um copper, 0.1 mm prepreg, 1.24 mm core, 0.1 mm
prepreg (the author's notes name JLCPCB's JLC04161H-7628 for the USB pair's impedance).

**Things in the files that surprised the reader**
- The outline lives in a footprint (above), and that footprint is also the headers.
- The author's notes say a polyfuse was added on the USB-C input; there is no fuse among the
  35 footprints.
- All 35 footprints are on the top side; there is no part on the back and no part with
  `board_only`. The reader treats a back-side footprint's pads as the file writes them (KiCad
  stores them already mirrored), which this file does not exercise.
- The schematic's references are not numbered gaplessly: there is no U4.
- The position list has `U2` at y = -59.75136 (y up): the file has y = +59.75136.
- `netsmith compare` needs KiCad's `kicad-cli` to read the schematic; the board file reads with
  no KiCad (`Netlist::from_kicad_pcb` finds 35 parts).

## Parts

### The module, the bridge, the regulator, the transistors and the diodes (U1, U3, U5, Q1, Q2, D2, D3)

None of these is a must-match part of the parity checker: they build, are sourced and are close
to the reference board's pads, and `crates/netsmith/tests/easyduino.rs` (module `ics`) says how
close. Every pin table was **written by hand** from the maker's datasheet, which is copyrighted:
only facts were taken (pad numbers, names, kinds), no table or text. A fixture board of the
test's own (never the project's board) builds with the parts and no KiCad install. The symbols
are drawn from the pin tables (no `symbol` key, ADR 0097) except the diode's, which is netsmith's
own `Device:D_Schottky`. Levels, ratings and timing are left out of the module, the bridge and
the transistor on purpose: with no model each part is a boundary of the design, and the checks
that need a number are listed as not evaluated. Documents were fetched on 2026-11-10.

- **U1, `Esp32S3Wroom1`** (`src/esp32_s3_wroom_1.nsm`): Espressif's ESP32-S3-WROOM-1 with the
  PCB antenna, 41 pins: 40 edge pads and the ground pad, `EPAD`, which is pad 41. Source:
  Espressif, "ESP32-S3-WROOM-1 & ESP32-S3-WROOM-1U Datasheet", Version 1.8 (revision history
  dated 2026-03-02, 53 pages, SHA-256 `27d71971da07`, from
  <https://www.espressif.com/sites/default/files/documentation/esp32-s3-wroom-1_wroom-1u_datasheet_en.pdf>):
  Figure 3-1 (printed page 10) and Table 3-1 (pages 11 and 12) for numbers, names and types;
  Table 4-1 (page 13) for the strapping pins; Figure 10-1 (page 42) and Figure 11-1 (page 45)
  for the package and the land pattern. The names are the datasheet's (`IO4`, `RXD0`, `TXD0`):
  pad 36 `RXD0` is GPIO44 and 37 `TXD0` is GPIO43 (Easyduino's `/GPIO44` and `/GPIO43`), 13
  `IO19` is `USB_D-` and 14 `IO20` is `USB_D+`. Pads 1, 40 and 41 are one pin, `GND`. `3V3` is a
  `power` pin, `EN` an `input` (Table 3-1: "Do not leave the EN pin floating"), the 36 `I/O/T`
  pads are `bidirectional`. **Left out on purpose:** the alternate functions of the pads (the
  table's last column), every level and rating, and the other variants' differences (footnotes b
  and c of Table 3-1).
- **U1 footprint** (`libs/easyduino_ics.pretty/ESP32-S3-WROOM-1_18x25.5mm_P1.27mm.kicad_mod`,
  written for this project, not copied from Espressif's library): 49 pads, the 40 edge pads and
  nine squares numbered 41.
  - **Read** (Figure 11-1, page 45; Figure 10-1, page 42): edge pads 1.5 x 0.9 mm (`40 x1.5`,
    `40 x0.9`) on a 1.27 mm pitch; 16.51 mm between pad 1 and pad 14 (13 pitches); 17.5 mm
    between the two columns' centres; 13.97 mm along the bottom row (11 pitches), its end pads
    2.015 mm from the module's edge; the nine ground squares 0.9 mm on a 1.4 mm pitch (0.9 and
    0.5 mm labels, 3.7 mm over all), their middle column 7.5 mm from the module's left edge and
    their middle row 10.29 mm above the module's bottom edge; the module 18 x 25.5 mm; pad 1's
    centre 7.49 mm below the module's top edge; the antenna area 18 x 6 mm.
  - **Derived by arithmetic:** every pad centre. The bottom row's centre line lies 0.25 mm inside
    the module's bottom edge (the pads are 1.5 mm long and protrude 0.5 mm, the `0.5` label of
    the figure).
  - **Assumed:** the origin and the axes (the datasheet states none): the middle of the module
    across, and the module's bottom edge 9.75 mm below the origin, which puts pad 1 at (-8.75,
    -8.26), the reference board's frame, so that a module placed by the reference's coordinates
    has its pads where theirs are; pad shape a rectangle with square corners (as the figure and
    the reference draw); mask and paste on every pad, nine squares of paste on pad 41 (the
    datasheet draws no stencil; the red circles of the figure are vias for the thermal pad, and
    vias are the board's); the body and the antenna area drawn on `F.Fab` and `Dwgs.User` for
    information (no keep-out zone: the datasheet's text sends the reader to Espressif's hardware
    design guidelines for its size, which were not read, and the reference has none); the
    courtyard, the pads and the module's body and 0.25 mm all round; no silkscreen and no 3D model; the pick-and-place rotation is unverified.
  - **Against the reference** (test): every pad's number and centre agree with Easyduino's
    `Espressif:ESP32-S3-WROOM-1` to 0.01 mm and every size to 0.01 mm (the table has the same
    49 pads).
  - **Catalog row** (`catalogs/esp32_s3_wroom_1.csv`): LCSC **C2913197**, JLCPCB Extended,
    ESP32-S3-WROOM-1-N4 (4 MB quad flash, no PSRAM), package "SMD, 25.5x18mm" as JLCPCB's page
    says; the JLCPCB and LCSC pages were read on 2026-11-10 and agree with the BOM of the
    reference. Price and stock are not recorded.
- **U3, `Cp2102n`** (`src/cp2102n.nsm`): Silicon Labs' (now Skyworks') CP2102N in the QFN28
  package, orderable as CP2102N-A02-GQFN28. Source: "CP2102N Data Sheet", Rev. 1.5 (48 pages;
  the copy LCSC links for C1550553, SHA-256 `70ea7a01045c`, PDF created 2020-11-13): Table 1.1
  (page 2) for the ordering key, Figure 5.1 and Table 5.1 (pages 27 and 28) for the pins,
  Table 3.3 note 1 (page 11) for RSTb, Figure 6.2 and Table 6.2 (page 35) for the land pattern.
  Kinds follow the table's descriptions: DCD, CTS, DSR, VBUS inputs; RTS, DTR, TXD, SUSPEND*,
  CHREN, CHR0 and CHR1 outputs; RI/CLK, the USB pair, RSTb and the GPIO pads bidirectional;
  VDD and VREGIN `power`; pad 10 `no_connect`; the centre pad (29, as the generator numbers it)
  is `GND` with pad 3. **Left out on purpose:** every level, rating and timing, the QFN24 and
  QFN20 packages.
  - **The variant of the reference.** Its schematic's part is `CP2102N-Axx-xQFN28`, and its BOM
    and the board's `LCSC` property say **C6568**, which is **not** a CP2102N: both JLCPCB's
    page and LCSC's page, read on 2026-11-10, name C6568 the **CP2102-GMR** (Silicon Labs'
    older CP2102, WQFN-28-EP 5x5, Extended), and the datasheet the LCSC link serves for it is
    "CP2102/9 Data Sheet". The catalog has the CP2102N rows that the pages confirm, **C1550553**
    (CP2102N-A02-GQFN28) and **C964632** (CP2102N-A02-GQFN28R, the tape-and-reel code), both
    Extended; neither page, as fetched, shows a stock. The pin names and numbers of the two
    chips agree in this package (the datasheet says "pin compatible with CP2101/2/9 (QFN28
    package)"); the reference's net names (`~{RI}/CLK`, `GPIO.3` ...) are the CP2102N's.
  - **Footprint** (`kicad.toml`): the generator's `QFN-28-1EP_5x5mm_P0.5mm_EP3.35x3.35mm`, the
    reference's footprint name. Silicon Labs' land pattern (Table 6.2) has the same pitch (0.50
    mm), pad count and centre pad (3.35 x 3.35 mm); its lead pads are 0.30 x 0.95 mm (X1, Y1)
    with centres 4.80 mm apart (C1, C2) where the generator's are 0.25 x 0.80 mm, 4.90 mm apart,
    which are the reference's too (every numbered pad agrees to 0.01 mm). The generator was not
    changed. The reference has nine paste-only pads on the centre pad, which the generator does
    not draw.
- **U5, the AMS1117-3.3** (std `LinearRegulator`, SOT-223, no file of its own, as the RP2350A
  board's NCP1117): the row of `catalogs/ldo.csv` is Advanced Monolithic Systems' AMS1117-3.3,
  LCSC **C6186**, **Basic**. Source: the AMS1117 datasheet (the copy LCSC links, SHA-256
  `189a2651878a`; 8 pages, no revision or date printed, PDF created 2009-08-27; pages counted
  from the first): output 3.201 V to 3.399 V (page 2, the boldface line of the table, which
  holds over the whole operating range), dropout at most 1.3 V at 0.8 A (page 3, note 4),
  power 1.2 W for the SOT-223 (page 3, note 2), pins 1 ground, 2 output and 3 input with the tab
  as the output (page 1). `max_current` is 0.8 A, as the NCP1117 row has it: the highest current
  at which the dropout is stated (the datasheet's headline figure is 1 A, the current limit 900
  to 1500 mA); the quiescent current (5 mA typical, 11 mA maximum, page 3) is not a catalog
  column, a board states it. The standard footprint is the reference's: every pad agrees to
  0.01 mm. The datasheet's package drawing was not read.
- **Q1 and Q2, `Ss8050`** (`src/ss8050.nsm`): JSMSEMI's SS8050, NPN, 25 V, 1.5 A, SOT-23, LCSC
  **C916392**, Extended. Source: "SS8050 NPN Silicon Epitaxial Planar Transistor" (4 pages, no
  revision printed; the LCSC copy, SHA-256 `198dacbb0442`): page 1 for the pins, pages 1 and 2
  for the ratings (left out), page 4 for the package outline and the soldering footprint. All
  three pins are `passive`. **Pad 1 is the base, pad 2 the emitter, pad 3 the collector**
  (page 1). Easyduino's board wires pad 2 to `/EN` (Q1) and `/GPIO0` (Q2) and pad 3 to
  `/~{RTS}` and `/~{DTR}`, while its symbol (`Q_NPN_BCE`) calls pad 2 the collector: with this
  part, those lines are on the emitters. The port follows the pads (the nets are the reference's). Whether the author meant it, and
  whether the reset circuit works with the emitter on `/EN`, is not checked here and is for the
  owner to decide.
  - **Footprint** (`kicad.toml`): the generator's `SOT-23`: pads 1.475 x 0.6 mm at +-0.9375 mm
    (1.875 mm between the columns). The reference has `SOT-23W`, whose pads are 1.0 x 0.7 mm at
    +-1.2 mm (2.4 mm between the columns), and the datasheet's soldering footprint (page 4) has
    0.80 x 0.90 mm pads with the single pad 2.00 mm from the line of the other two. The numbers
    and the sides agree; the farthest pad centre is 0.26 mm from the reference's and the largest
    size difference 0.475 mm (the test bounds both). No `SOT-23W` is a generator, and no
    drawing of one was read.
- **D2 and D3, the Schottky diodes** (std `Schottky`, package `SMD1006`): ROHM's RB161QS-40
  (LCSC and JLCPCB's **C2837790** is the RB161QS-40T18R, taped), 40 V, 1 A, **Extended**. Source:
  ROHM's "RB161QS-40" datasheet, 2016.02 - Rev.001 (the LCSC copy, 3 pages): page 1 for VR and
  IO, VF at most 600 mV at 1 A, the package (SMD1006, 1.0 x 0.6 mm, thickness 0.22 mm), the
  structure (the cathode at the marked end) and the Land Size Figure. The catalog row
  (`catalogs/schottky.csv`) has the worst-case forward voltage, 0.6 V, as the standard rows do.
  - **Footprint** (`libs/easyduino_ics.pretty/D_ROHM_SMD1006.kicad_mod`): ROHM's land,
    **read**: pads 0.6 mm wide and 0.35 mm long with 0.65 mm between their centres (page 1, Land
    Size Figure). **Assumed:** pad 1 (cathode) on the left, the origin at the body's centre,
    rectangular pads, paste and mask on both, courtyard 0.25 mm round, the body outline and the
    cathode mark on `F.Fab`, no 3D model, the pick-and-place rotation unverified. The
    reference's pads are the generator's 0402 diode, 0.59 x 0.64 mm and 0.97 mm apart: ROHM's
    centres are 0.16 mm nearer and its pads 0.24 mm shorter (the test states both). The pad
    numbers agree: pad 1 on `VBUS` is the cathode, pad 2 the anode.
- **What the reference says about them** (checked by the tests against
  `reference/placements.toml`): every pad of U1 carries the net that names its pin (`/GPIOn` is
  `IOn`, `/GPIO44` and `/GPIO43` the pads `RXD0` and `TXD0`, `/USB2_D-` and `/USB2_D+` pads 13
  and 14, `/EN`, `+3.3V`, `GND`); U3's pads on named nets are `GND` (pad 3 and the centre pad),
  `D+`, `D-`, `VREGIN` and `VBUS` on `VBUS`, `VDD` on its own net, `RSTb`, `SUSPENDb`, `RTS`,
  `DTR`, and `RXD` and `TXD` on the module's `/GPIO43` and `/GPIO44` (cross-wired, as the
  UART is); its 16 open pads are on nets named after the same pins.
- **For the board stage:** the AMS1117's dropout is 1.3 V at most, and the reference feeds it
  from `VBUS` through a Schottky diode (D2 or D3, 0.6 V at 1 A): a 5 V +/- 5 % supply less a
  diode is 4.15 V in the worst case, below 3.4 V plus 1.3 V, so the checker may name it; the
  dropout is at a load of 0.8 A and is lower at the module's lighter load, which the catalog
  cannot say.
