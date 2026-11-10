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
- `src/rp2350a.nsm`, `catalogs/rp2350a.csv`, `kicad.toml`: the MCU, U1 (see Parts).
- `src/amphenol_10103594.nsm`, `src/jst_sm03b_srss_tb.nsm`, `src/samtec_tsw_118.nsm`,
  `catalogs/micro_usb.csv`, `catalogs/jst_srss.csv`, `libs/netsmith_rp2350.pretty/`: the
  connectors J1 to J4 (see Parts). The footprints are files of the project's own library and
  need no KiCad.
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

### The connectors, J1 to J4

Their pads must equal Raspberry Pi's, so the footprints are written pad for pad against the
board's table (`reference/placements.toml`); a test compares number, centre (0.01 mm), size
(0.05 mm), drill (0.01 mm), shape and kind for each of the four, and a fixture board builds with
the parts and no KiCad install. The parts only say what a connector is; which signal is on
which pin, and where each stands, is the board's. Origin and axes are those of Raspberry Pi's
footprints (x right, y down, the part seen from the component side), the same as the drawings'
datum where they have one. Numbers are mm. **No dimension was guessed**; what is not on a drawing
is listed as assumed, and where the board differs from a drawing the footprint follows the board
and says so.

- **J1, `Amphenol10103594`** (`src/amphenol_10103594.nsm`): Amphenol FCI 10103594-0001LF, micro-USB
  B receptacle, LCSC C428495, JLCPCB Extended (the page was read on 2026-11-10;
  `catalogs/micro_usb.csv`; price and stock not recorded). Pins: VBUS 1, D- 2, D+ 3, ID 4, GND 5
  and the shield, one pin of the six pads 6 to 11. `provides Connector`.
  Footprint `USB_Micro-B_Amphenol_10103594-0001LF`, 15 pads (11 numbers: the five contacts, and
  six shell numbers of which 6, 7, 9 and 10 each have a surface pad and a plated slot).
  - Source: Amphenol FCI drawing 10103594 ("MICRO-USB B-TYPE RECEP."), revision A, ECN
    T10-0002, dated 1/7/10, printed 2010-06-02 (the PDF that LCSC links for C428495,
    `datasheet.lcsc.com/datasheet/pdf/ca5a42dbf95ebfb4fb84a094027e7ce9.pdf`, one sheet, SHA-256
    `e119d95457dc`), the view "RECOMMENDED PCB Pattern" (tolerance +/-0.05). The drawing carries
    "Tous droits strictement reserves ... All rights strictly reserved. Reproduction or issue to
    third parties in any form whatever is not permitted without written authority from the
    proprietor" in its margin. Nothing was copied: only the dimensions were taken, and the
    owner accepted that reading for Amphenol's SMA drawing on 2026-11-10; it has **not** been said
    of this one, so it needs the owner's word.
  - **Read** (labels, 500 to 900 dpi): signal pads 0.40 wide, 0.65 pitch (0.65, 0.40), 1.75 long,
    their top 3.70 above the datum line (3.70, 1.75); the datum line passes through the centres of
    the two lower slots (3.03 below the upper slots' centres); upper shell pads 2.00 wide
    (2.00), 0.73 above and 0.73 below the slot centre (0.73 and 2.30), with a slot 0.65 x 1.05 at
    +/-2.425 (4.85, 0.65, 1.05); the T-shaped land from y = -2.05 to -1.35 for its bar (2.05,
    1.35), 7.80 over the bars' outer edges (7.80), 6.75 over the stems' (6.75), 4.15 between the
    stems' inner edges (4.15), stems ending 0.60 short of the PCB end 1.70 below the datum (0.60,
    1.70) with R0.40 corners; lower slots 0.70 x 1.20 with 6.15 between their outer edges (6.15,
    4.75, 0.70, 1.20); the middle pads 0.50 apart and 3.35 over all (0.50, 3.35), from 1.00
    above the datum to 1.50 below it (1.00, 1.50).
  - **Derived by arithmetic:** the shell pads' centre x (+/-2.9, from 7.80 and 2.00) and height
    1.46 (0.73 twice; Raspberry Pi's is 1.45); the slot centres (+/-2.725, from 6.15 and 0.70;
    +/-2.425 from 4.85); the T-land's stem (x from -3.375 to -2.075, bottom at 1.70 - 0.60 =
    1.10), as the polygon of pads 7 and 10 (the same outline as Raspberry Pi's custom pads); the
    contacts' centre y, -2.825.
  - **Differs from the drawing, the footprint follows Raspberry Pi's board:** the two middle
    pads (8 and 11) are 2.10 long and centred at y = 0.05 in Raspberry Pi's board (from -1.00 to
    1.10); the drawing has them 2.50 long, to 1.50. Their centres are x = +/-0.95 there against
    +/-0.9625 from the drawing's 0.50 gap and 3.35 span (0.0125 apart; the width, 1.425, is the
    same). The drawing is revision A of 2010; whether Amphenol drew another revision, or
    Raspberry Pi shortened the pads, is not known.
  - **Assumed:** pad numbers 6 to 11 for the shell (the drawing numbers none: Raspberry Pi's);
    that the contact order VBUS, D-, D+, ID, GND is the USB micro-B's (Raspberry Pi's board
    puts those nets on pads 1 to 5; the USB-IF specification was not read); the plated ovals
    around the slots, 1.05 x 1.45 and 1.1 x 1.8 (Raspberry Pi's; the drawing hatches the lands
    and draws the slots only, a ring of 0.20 and 0.20 / 0.30 around them); copper on all layers
    and mask on the back only for them, as Raspberry Pi's; paste on the contacts only;
    the courtyard, the pads and 0.25 all round; the PCB end on `Dwgs.User`; no 3D model; the
    pick-and-place rotation is unverified.
- **J4, `JstSm03bSrssTb`** (`src/jst_sm03b_srss_tb.nsm`): JST SM03B-SRSS-TB(LF)(SN), 3 contacts,
  1.0 mm pitch, side entry, two mounting tabs, LCSC C160403, JLCPCB Extended (read 2026-11-10;
  `catalogs/jst_srss.csv`). Pins 1, 2, 3 and `MOUNT` (pads 4 and 5). `provides Connector`.
  Footprint `JST_SM03B-SRSS-TB_1x03-2MP_P1.00mm_Horizontal`, 5 pads.
  - Source: JST, "SR/SZ CONNECTOR, 1.0mm pitch" (the PDF that LCSC links for C160403,
    `datasheet.lcsc.com/datasheet/pdf/e03a82bcc66e3c3589917313e54d45e2.pdf`, 3 pages, SHA-256
    `8f01930d3601`; no revision or date is printed): page 1, "PC board layout ... <SMT type>,
    Side entry type" ("Dimensions above should serve as a guideline", tolerances non-cumulative);
    page 3, "Shrouded header (SMT type)" for the part number and the 3-circuit dimensions
    (A 2.0, B 5.0). No notice of copyright or confidentiality was seen.
  - **Read:** contact pads 0.6 wide at 1.0 pitch (0.6, 1.0); the tabs 1.2 wide and 1.8 long
    (1.2, 1.8) with their inner edges 0.7 beyond the outermost contacts' centres (0.7); 4.0 from
    the contacts' lower edge to the tabs' lower edge, 5.55 from the contacts' upper edge to it
    (4.0, 5.55); the tabs' lower edge 0.2 below the header outline's rear edge (0.2).
  - **Derived:** the contacts' length 1.55 (5.55 - 4.0); the tabs at x = +/-(1.0 + 0.7 + 0.6) =
    +/-2.3; with the origin at the rear edge of the header outline, the tabs' centres at y = -0.7
    (0.2 - 0.9) and the contacts' at -4.575 (0.2 - 5.55 + 0.775). They are Raspberry Pi's numbers
    to the last digit.
  - **Assumed:** the origin and the numbering (contacts 1 to 3 from the left, the tabs 4 and 5;
    the drawing numbers neither; Raspberry Pi's board does so); paste on all pads; courtyard,
    the pads and 0.25; no body, silkscreen or model; the rotation unverified.
- **J2 and J3, `SamtecTsw11807LD`** (`src/samtec_tsw_118.nsm`): Samtec TSW-118-07-L-D, 2 x 18 pin
  header, 2.54 mm pitch, through hole; one part for both, 36 pins, `provides Connector`. It has
  **no JLCPCB match**, so it has no catalog row and no LCSC number: a through-hole part is
  **soldered by hand** (ADR 0024: a through-hole part is `Hand` unless a board says otherwise, and a
  test holds it); `AssemblyMode::Consigned` is the way to have JLCPCB place a header that is
  shipped to them. Footprint `PinHeader_2x18_P2.54mm_Samtec_TSW-118-07-L-D`.
  - **Not the generator's footprint:** `gen:PinHeader(rows=2, pins=18, pitch=2.54mm)` has pads of
    0.7 x the pitch, 1.778 mm, and its pins run down columns from the top left; Raspberry Pi's
    pads are 1.6 mm (the generator's are 0.178 mm larger, over the limit of 0.05 mm) and its pins
    run along the long axis, 1 in the row below, 2 above. A test shows the difference.
  - Source: Samtec, "Recommended P.C. Board Layout for (X)TSW Single, Double & Triple (Thru-hole
    & Right Angle)", revision A (`suddendocs.samtec.com/prints/tsw-xxx-xx-x-x-xx-xxx-footprint.pdf`,
    one sheet, SHA-256 `264658121ff2`); the product page
    <https://www.samtec.com/products/tsw-118-07-l-d> read on 2026-11-10 for the 36 pins and the
    pitch. The series print (47 pages) states a notice of confidentiality, and none of its numbers is used.
  - **Read:** holes 0.040 in (1.02 mm) diameter, 0.100 in (2.54 mm) apart, rows 0.100 in (2.54 mm)
    apart. The sheet states **no pad size**.
  - **Differs from the drawing, the footprint follows Raspberry Pi's board:** the drill is 1.00 mm
    (0.02 mm under Samtec's 1.02).
  - **Assumed:** the pads, 1.6 mm round, Raspberry Pi's (a ring of 0.30 mm; the drawing has none);
    pin 1 square; the numbering (pin 1 and the odd pins in the row at y = +1.27, the even pins at
    -1.27, pin 1 at the left end, as Raspberry Pi's); the origin at the middle of the 36 holes; a
    body outline of 2 x 18 pitches on `F.Fab`, the courtyard the pads and 0.25; no model.

## Run it

From this folder:

```sh
netsmith build    # the schematic, in out/
netsmith fab      # the package for the board house, in fab/
```

`netsmith compare` can read their board file as one side:
`netsmith compare reference/RPI-RP2350A-MINIMAL_R4-S1.kicad_pcb out/Rp2350Minimal/Rp2350Minimal.kicad_sch`
says nothing useful until the port is finished.
