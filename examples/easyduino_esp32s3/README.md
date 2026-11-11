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

Numbers are mm; the origin and axes of every footprint are those of the reference's (x right, y
down, the part seen from the component side). A drawing's copyright or confidentiality notice is
about the document: nothing was copied, only dimensions and facts. Each footprint is in
`libs/easyduino_must_match.pretty` (no KiCad install is needed) and is compared pad for pad with
`reference/placements.toml` by `crates/netsmith/tests/easyduino.rs` (number, centre, size, shape,
kind, angle and drill, all within 0.05 mm: the parity rule's tolerance). Every symbol is drawn from
the part's pin table.

### The interface parts: J1, J2, SW1, SW2, U2 and the LEDs

- **J1 and J2, `ShouhanTypeC16Pin`** (`src/shouhan_type_c_16pin.nsm`): the receptacle the board
  buys is LCSC **C2765186**, Shouhan (SHOU HAN) `TYPE-C 16PIN 2MD(073)`, a 16-contact USB 2.0
  Type-C receptacle, right angle SMT with four shell legs (the board file's footprint is another
  maker's, G-Switch `GT-USB-7010ASV`: the LCSC number is the part, the footprint is a stand-in with
  the same land pattern). `provides Connector`. Nine pins: `.vbus` (A4, A9, B4, B9), `.gnd` (A1,
  A12, B1, B12), `.cc1` (A5), `.cc2` (B5), `.dp` (A6, B6), `.dm` (A7, B7), `.sbu1` (A8), `.sbu2`
  (B8) and `.shield`, whose pad number `S1` is on four plated slots (as the RP2350 example's
  micro-USB has its shield on several pads); the two non-plated pegs have no number and no pin. The
  contacts that the receptacle joins inside (the plug picks A or B) are one pin each.
  - Catalog row (`catalogs/shouhan_type_c_16pin.csv`): C2765186, package `SMD`, library type
    **Extended**: JLCPCB's page `jlcpcb.com/partdetail/C2765186` and LCSC's
    `lcsc.com/product-detail/C2765186.html`, read on 2026-11-10 (both name the part, 16 contacts,
    SMD). Price and stock are not recorded.
  - Source: Shouhan's "SPECIFICATION FOR APPROVAL" for `TYPE-C 16PIN 2MD(073)` (the PDF that
    LCSC's record links, `datasheet.lcsc.com/datasheet/pdf/2d53d8d8199c9dd6666d9bc9ac7c3a1e.pdf`,
    6 pages, SHA-256 `fe10da58945c`, no date printed), read on 2026-11-10: the drawing sheet (title
    block: TYPE-C 16PIN 2MD(073), REV A, SHEET 1 OF 1, scale 1:8, tolerance X.XXX +/-0.05) with its
    "PIN ASSIGNMENTS" table and the view "RECOMMENDED PCB LAYOUT (TOP VIEW)". The drawing's title
    block says "CUSTOMER COPY"; the pages after it are the maker's test conditions.
  - **Read** (labels at 400 dpi): contact pads 0.60 wide for the four power and ground pads
    (0.60 (4X)) and 0.30 wide for the eight signal pads (0.30 (8X)), at +/-3.20 (6.40), +/-2.40
    (4.80), +/-1.75 (3.50), +/-1.25 (2.50), +/-0.75 (1.50) and +/-0.25 (0.50); the names of the
    sixteen contacts (the table); four shell slots: two of 2.10 with a 0.60 x 1.70 hole and two of
    1.80 with a 0.60 x 1.40 hole (2.10, 1.70, 1.80, 1.40, 0.60 (4X)), 8.64 apart (8.64); two plain
    holes of 0.65 (0.65 (2X)), 5.78 apart (5.78); from the lower slots' centre to the upper slots'
    4.18, to the pegs 3.68, and to the pads' inner edge 4.17 and outer edge 5.32.
  - **Differs from the drawing, the footprint follows the reference board** (the parity rule wins):
    the board's contact pads are 1.24 long, centred at y = -3.725, where the drawing gives 1.15 (5.32
    - 4.17), centred 0.055 further in (the inner edges agree to 0.01); the upper and lower slots are
    4.20 apart (the drawing's 4.18); the pegs are at y = -2.605 (the drawing's 3.68 above the lower
    slot gives the same).
  - **Assumed** (not on the drawing): the pad numbers' sides (A and B as the table), the origin
    (the reference's: on the centre line, 1.075 above the lower slots), the shell slots' outer
    width, 1.00 (the drawing labels only the 0.60 hole, and the outer length), that the slots are
    plated and numbered `S1` (the board's), the pegs unplated, paste on the contacts only,
    courtyard (the pads and 0.25), no silkscreen, 3D model or pick-and-place rotation.
- **SW1 (RESET) and SW2 (BOOT), `XunpuTs1088`** (`src/xunpu_ts_1088.nsm`): LCSC **C720477**, XUNPU
  (DongGuan XunPu Electronics) `TS-1088-AR02016`, a 5.0 x 3.0 mm SMD tactile switch (the board
  file's footprint is Alps' `SKRK`; LCSC's number is this part). The standard `Switch` has no
  catalog (so no LCSC number), so the part is the project's own, as the RP2350 example's tactile
  switch is: pins `.a` (pad 1) and `.b` (pad 2), rated 50 mA and 12 V DC.
  - Catalog row (`catalogs/xunpu_ts_1088.csv`): JLCPCB and LCSC pages read on 2026-11-10 (the
    JLCPCB page names the part and the package `SMD, 4x3mm` and **does not say Basic or Extended**,
    so `library` is empty). Price, stock, height and temperatures are empty (the drawing prints no
    height maximum nor temperature range; LCSC's page lists 2 mm and -30 to +80 C, and no document
    read here prints them).
  - Source: XUNPU's drawing "TACT SWITCH" `TS-1088-ARXXXXXX`, rev. A, scale 1:1, one sheet
    (`datasheet.lcsc.com/datasheet/pdf/0475ac02febf455ca9ddcfb380b0df0d.pdf`, 1 page, SHA-256
    `368d4cc2c26b`, no date printed), read on 2026-11-10: the rating table (50 mA, DC 12 V, contact
    resistance 100 mohm, 50 gf, travel 0.2 mm), the circuit diagram (two terminals, normally open)
    and the "PCB WELDING DRAWING": 5.50 overall, 3.40 between the pads' inner edges, 2.00 long.
  - **Differs from the drawing, the footprint follows the reference board:** the drawing's pads are
    1.05 x 2.00 at +/-2.225; the board's, `SKRK`'s, are 0.8 x 2.0 at +/-2.1 (the inner edges agree,
    at +/-1.70). **Not read:** why the board uses Alps' pattern for this part. A board made from
    the footprint solders the switch on pads 0.25 mm narrower than the maker's.
  - **Assumed:** everything but the two pads: roundrect corners (the reference's 0.25 ratio), paste
    on both, courtyard and body outline (the drawing's 5.00 x 3.00 body, no silkscreen), no 3D model.
- **U2, `Esp32S3DevKitCHeaders`** (`src/devkitc_headers.nsm`): the development board's two 22-pin
  headers, 44 pads, one footprint that draws no outline (see below). `provides Connector`.
  The part has 40 pins: the signal each carries, named as the net the reference has on that pad
  (`.v3v3` on pads 1 and 2, `.gnd` on pads 22, 23, 24 and 44, `.vbus` ("5V") on pad 21, `.en`, and
  `.gpio0` to `.gpio48`; pads 25 and 26, which the reference calls `/USB2_D-` and `/USB2_D+`, are
  `.gpio19` and `.gpio20`). All pins are `passive`. **Hand-soldered** (ADR 0024): the board's BOM has
  `-` for it, so there is no catalog and no row, and a test holds that it has no LCSC number.
  - Source: Espressif, "ESP32-S3-DevKitC-1 v1.1" user guide, section "Header Block" (the English
    page `docs.espressif.com/projects/esp-dev-kits/en/latest/esp32s3/esp32-s3-devkitc-1/user_guide_v1.1.html`,
    read on 2026-11-10; the page prints no revision of its own besides the board's v1.1, and its
    footer a copyright of 2016-2026): the J1 and J3 tables for the pin numbers and names (J1 pins 1
    to 22 are footprint pads 1 to 22; J3 pin n is pad 45 - n: the footprint numbers the right row
    from the bottom). The page has no dimensions.
  - **Assumed from the reference board** (no maker document with a land pattern was read): every
    pad (44 ovals, 1.2 x 2.0 mm turned 270 degrees as the file has them, drill 0.8 mm,
    thru-hole), the 2.54 pitch and the 22.86 row spacing, the origin at pad 1. Pads 20 to 22 are at
    the grid's place here and 3.7 um and 2.7 um off it in the file (the parity tolerance is 0.05).
    Courtyard: the pads and 0.25. **The footprint does not draw the board's outline**: the
    reference's U2 draws the four `Edge.Cuts` lines of the 25.86 x 63.90 mm board inside the
    footprint; here they are not copied, and a test checks that no footprint of the library has
    `Edge.Cuts`. The physical-file stage draws the outline itself, with a rectangle from x 127.105 to
    152.965 and y 58.6514 to 122.5486 in the reference's frame (U2's origin is at 128.605,
    59.75136), and places U2, J1, J2, SW1 and SW2 where the table has them.
- **D1 to D4, the standard `Led`** in the 0402 package (the footprint is the generator's, pad 1 the
  cathode: the board has pad 1 on ground and pad 2 on the resistor or net, D1 and D4 alike). The
  board's two LEDs have one part, LCSC **C434447**, Yongyu Photoelectric `SZYY0402B`, **blue**,
  0402 (the board's BOM has "LED" with no colour; the part's page says blue). The project's own
  `catalogs/led.csv` has its row: JLCPCB's page `jlcpcb.com/partdetail/C434447` (Extended) and
  LCSC's, read on 2026-11-10; the manufacturer's specification for approval (issue date 2017/09/04,
  7 pages, SHA-256 `a4f1d15c5c37`, page 2): Vf 2.6 V to 3.2 V at 5 mA, operating temperature -40 to
  +85 C (the document does not say ambient). The datasheet has neither the polarity nor a land
  pattern, so the pads are the standard 0402's. The standard blue 0402 row (XL-1005UBC, 2.6 V to 3.4 V) comes
  first in the solver's order, so a board that wants this part states `.color &= "BLUE"`,
  `.package &= "0402"` and `.forward_voltage &= 2.6V..3.2V`, as the test's fixture does.
- **What is not done here** (the next stages): the module, the bridge, the regulator, the
  transistors and the diodes are other parts; the passives are the standard ones. The board
  (`main.nsm`) is still the placeholder.
