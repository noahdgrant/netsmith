# RP2350A Minimal

A port of Raspberry Pi's open **RP2350A Minimal** reference board to netsmith, as a showcase.
**The port is in progress.** `src/main.nsm` is a placeholder board (an LED on a supply) that
stands in until the parts and the circuit are written; it is not Raspberry Pi's circuit.

## Source

- The design: <https://pip.raspberrypi.com/documents/RP-010328-CA>, a zip of the KiCad 10
  schematic, board, project, a PDF and a bill of materials.
- Name and revision: `RPI-RP2350A-MINIMAL_R4-S1`. The board file is revision R4 and the
  schematic is revision S1; both title blocks are dated 2026-07-03 ("RP2350A QFN-60 Minimal
  Design Example"). One schematic sheet, 43 parts, two copper layers.
- Licence: MIT, Copyright (c) 2026 Raspberry Pi Ltd. The notice is in `LICENSE`, as they ship
  it, and stays with every copy of anything in this folder that comes from their design
  (`reference/`).

**The circuit is Raspberry Pi's. The port, the netsmith source and the tools in this folder
are netsmith's** (MIT, like the rest of the repository).

## Goal

1. **The same nets.** `netsmith compare` says the schematic of the port and Raspberry Pi's
   design are the same circuit.
2. **A matching PCB where it matters.** A board made by `netsmith fab` is interchangeable
   with theirs in these ways, and a checker with tolerances will fail when one is off:
   - the board outline and the four mounting holes;
   - the connectors (J1 to J4), the buttons (SW1, SW2) and the pin headers: the same
     positions, rotations and pads, with each header pin carrying the same signal.

   The other parts (MCU, flash, crystal, regulator, inductor, decoupling) may be anywhere,
   and the routing will differ; the checker reports them and does not fail on them.

## What is here

- `netsmith.toml`, `src/main.nsm`: the project and its (placeholder) board.
- `src/tactile_switch.nsm`, `catalogs/ldo.csv`, `catalogs/inductor.csv`,
  `catalogs/tactile_switch.csv`, `kicad.toml`, `libs/rp2350_minimal.pretty/`: the regulator U2,
  the inductor L1 and the buttons SW1 and SW2 (see Parts).

- `src/rp2350a.nsm`, `catalogs/rp2350a.csv`, `kicad.toml`: the MCU, U1 (see Parts).
- `LICENSE`: Raspberry Pi's licence text.
- `reference/RPI-RP2350A-MINIMAL_R4-S1.kicad_pcb`: their board file, unchanged (1.6 MB).
- `reference/placements.toml`: the facts the checker needs, read from that file: the outline,
  the mounting holes and, for every footprint, its place, rotation and pads. The header of the
  file says what the origin and axes are, and which entries must match (`must_match = true`).
  Regenerate it with
  `cargo run -p netsmith --example reference_table -- reference/RPI-RP2350A-MINIMAL_R4-S1.kicad_pcb > reference/placements.toml`
  (run from this folder); a test checks that the file is what the program prints.

## Parts

- **U1, `Rp2350A`** (`src/rp2350a.nsm`): Raspberry Pi's RP2350A in the QFN-60, 7 x 7 mm,
  0.4 mm pitch package: a pin table of 60 pads and the exposed pad (pad 61, the ground).
  Source: the RP2350 datasheet (document RP-008373-DS; the PDF's colophon says Release 8, build
  date 2026-10-01), Figure 2 (printed page 15) and Tables 1673 to 1679 (pages 1331 to 1335).
  The table is written by hand: the datasheet is CC BY-ND and the owner's rule is no copied
  tables, so only the facts (numbers, names, kinds) are taken. The datasheet's Table 1679 lists
  the IOVDD pads without pad 1 and with pad 54, which is QSPI_IOVDD; the figure and Raspberry
  Pi's board have IOVDD on pad 1, and the part follows them.
- **Footprint** (`kicad.toml`): the generator's `QFN-60-1EP_7x7mm_P0.4mm_EP3.4x3.4mm`. Its body,
  pitch and 3.4 x 3.4 mm exposed pad are those of the package drawing (page 1324); its lead
  pads are 0.80 x 0.20 mm where Raspberry Pi's board has 0.70 x 0.23 mm, and its pads span
  7.7 mm where the recommended footprint (page 1325) draws 7.75 mm (the generator is not
  changed). The symbol is drawn from the pin table.
- **Catalog row** (`catalogs/rp2350a.csv`): JLCPCB C42411118, Extended, with the page it was
  read from and the day. Price and stock are not recorded.
- **Left out on purpose:** the alternate functions of the pins (a GPIO has `GPIO`, and
  GPIO26 to GPIO29 also `ADC0` to `ADC3`; the pins are fixed by Raspberry Pi's board, so none is
  chosen), the functions of the QSPI pins, and every level, rating and model. With no model the
  part is a boundary of the design, and the checks that need a number are listed as not
  evaluated.

## Run it

From this folder:

```sh
netsmith build    # the schematic, in out/
netsmith fab      # the package for the board house, in fab/
```

`netsmith compare` can read their board file as one side:
`netsmith compare reference/RPI-RP2350A-MINIMAL_R4-S1.kicad_pcb out/Rp2350Minimal/Rp2350Minimal.kicad_sch`
says nothing useful until the port is finished.

## Parts

Every number below was read on 2026-11-10 from the document named; a cell the document does not
give is empty in the catalog row, and the row's `notes` say which. The three catalog rows are
the project's own (`catalogs/`), each with the JLCPCB parts library page and the day it was read
in its `source`; `price` and `stock` are empty (read them from the supplier before ordering).
All three parts are Extended in JLCPCB's library.

### U2: onsemi NCP1117ST33T3G (LCSC C26537)

The standard `LinearRegulator` in the package SOT-223, with a row of `catalogs/ldo.csv`. A board
asks for the row with `.package &= "SOT-223"` (the standard row is a SOT-23 part).

- Document: onsemi NCP1117/D, "August, 2021 - Rev. 31"
  (<https://www.onsemi.com/pdf/datasheet/ncp1117-d.pdf>).
- `vout.voltage` 3.235 V to 3.365 V: ELECTRICAL CHARACTERISTICS (page 3), 3.3 V, Vin 4.75 V to
  10 V, Iout 0 to 800 mA over the temperature range. `dropout` 1.2 V: the same table, 800 mA
  maximum. `max_current` 0.8 A: the highest current at which the datasheet states both; its
  other figures (in excess of 1.0 A, page 1; current limit 1.0 A to 2.2 A, page 3) are not a
  rating. `temp_min`, `temp_max` 0 to 125 degC: operating ambient of the NCP1117 (page 2).
- **Derived, not read:** `max_power` 0.78 W is (150 - 25) / 160: the datasheet says "Internally
  Limited" and gives PD = (TJ(max) - TA) / RthJA, with TJ 150 degC and RthJA 160 degC/W for the
  SOT-223 on a minimum pad (page 2), at a 25 degC ambient. A hotter board has less (0.16 W at
  125 degC).
- Not in the row: the quiescent current (6.0 mA typical, 10 mA maximum, 3.3 V version, page 4)
  has no catalog column; a board states `.quiescent_current &= 0mA..10mA`. The package height
  is left empty (the drawing is a picture).
- Pinout (page 1): 1 Adjust/Ground, 2 Output, 3 Input, tab joined to pin 2. It is the std
  table's (3 input, 1 ground, 2 output), so the std mapping to the generator's
  `Package_TO_SOT_SMD:SOT-223-3_TabPin2` is used and `kicad.toml` has no row (a project may not
  map a pair the standard mapping maps).
- **Differs from Raspberry Pi's board:** their footprint numbers the tab pad 4 (net `+3V3`, the
  output); the generator's numbers it 2, as the datasheet's pin 2. Same net, other number.

### L1: Abracon AOTA-B201610S3R3-101-T (LCSC C42411119)

The standard `Inductor` in the package `0806` (JLCPCB's name for 2016 metric, 2.0 x 1.6 x 1.0 mm;
no generator answers it and `std/kicad.toml` stops at 0805), a row of `catalogs/inductor.csv`.

- Document: Abracon AOTA-B201610S3R3-101-T, "REVISION A : 9/13/2024", 6 pages
  (<https://abracon.com/datasheets/AOTA-B201610S3R3-101-T.pdf>).
- Page 1, Electrical Specifications: 3.3 uH at 1 MHz; `saturation_current` 2.4 A (the Max
  column; the table prints 2.8 A under Typ, and the smaller is taken); `dc_resistance` 0.14 ohm
  (Max; typical 115 mohm); `rms_current` 2.1 A (Max; the current for about a 40 K rise). Page 4:
  height 1.00 mm maximum.
- **Assumed:** the tolerance. The table prints the code `M` and no percentage; the row says
  +/- 20%, which is what the code means. `temp_min` and `temp_max` are empty: the -40 to +125 degC
  of page 1 includes the part's own temperature rise.
- Footprint `libs/rp2350_minimal.pretty/L_Abracon_AOTA-B201610S3R3-101-T.kicad_mod`, from page 4,
  "Recommended Land Pattern" (dimensions read from the picture at 400 dpi): **read** two pads of
  1.00 mm (across) by 1.60 mm, 1.00 mm between them. **Derived:** pad centres at +/-1.00 mm.
  **Assumed:** the numbering (pad 1 on the left, the convention of Raspberry Pi's board; the
  datasheet numbers no pad, and marks only the polarity dot of the winding), square-cornered
  rectangles, paste and mask openings the size of the pads, a courtyard of the pads and 0.25 mm,
  the 2.0 x 1.6 mm body on `F.Fab`, no silkscreen, no 3D model.
- **Differs from Raspberry Pi's board:** their pads are 0.70 x 1.70 mm at +/-0.70 mm, pad 1 on
  `+1V1` and pad 2 on `/VREG_LX`. The pad numbers and nets agree with the part
  (`.a` on pad 1, `.b` on pad 2); the land pattern is Abracon's, not theirs.

### SW1, SW2: Wurth Elektronik 434133025816 (LCSC C5504987)

The project's own `TactileSwitch` (`src/tactile_switch.nsm`): the standard `Switch` has two pins
on one pad each, and a four-pad footprint is not a footprint of it. A row of
`catalogs/tactile_switch.csv`, keyed by `.mpn` (a board names it: `.mpn &= "434133025816"`).

- Document: Wurth Elektronik 434133025816, "WS-TASV J-Bend SMT Tact Switch", REVISION 001.001
  of 2021-01-25, 7 pages (<https://www.we-online.com/components/products/datasheet/434133025816.pdf>;
  the site refuses a plain download, and the file was read as WebFetch fetched it).
- Page 1, schematic: terminals 1 and 2 are joined, 3 and 4 are joined, the button joins the two
  pairs. Each pair is one pin of the part with two pads (`.a` on pads 1 and 2, `.b` on 3 and 4).
  Page 2: rated 50 mA and 16 V DC; operating temperature -40 to +85 degC (not said to be
  ambient). Page 1: height 2.5 +0.2/-0.1 mm, so `height` is 2.7 mm.
- Footprint `libs/rp2350_minimal.pretty/SW_Wurth_434133025816.kicad_mod`, from page 1,
  "Recommended Land Pattern": **read** 5.2 mm over the pads and 3.1 mm between them across, 2.8 mm
  and 1.5 mm along, four pads. **Derived:** each pad is (5.2 - 3.1) / 2 = 1.05 mm by
  (2.8 - 1.5) / 2 = 0.65 mm, centred at +/-2.075 mm and +/-1.075 mm. **Assumed:** square corners,
  paste and mask the size of the pads, a courtyard of the pads and 0.25 mm, the 4.2 x 3.2 mm body
  (page 1) on `F.Fab`, no silkscreen, no 3D model.
- The datasheet is not consistent about the numbers: the land pattern has 1 and 2 on the upper
  row and 3 and 4 on the lower, the outline drawing the other way up. The pairs are the same in
  both, and the footprint follows the land pattern. It is the same as Raspberry Pi's, pad for
  pad (same numbers, places and sizes).
