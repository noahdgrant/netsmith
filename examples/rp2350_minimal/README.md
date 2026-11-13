# RP2350A Minimal

A port of Raspberry Pi's open **RP2350A Minimal** reference board to netsmith, as a showcase.
**The schematic is done** (`src/main.nsm`, the board `Rp2350Minimal`): the same circuit as
theirs, proved net by net by a test. **The physical file is done** (`src/main.physical.nsm`: the
outline, the four mounting holes, the places of the connectors and buttons), and `netsmith fab`
writes a PCB that the parity checker finds equal to Raspberry Pi's in every way that must match
(see Parity and The result). It routes completely, in about 35 s.

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

1. **The same nets.** The schematic of the port and Raspberry Pi's design are the same
   circuit: the test `every_pad_has_the_same_pads_on_its_net_as_in_raspberry_pis_board` proves
   it by comparing the wiring alone (see "Same nets"). The bare `netsmith compare` of the two
   files answers `different`, because it pairs parts by footprint name (see "Run it").
2. **A matching PCB where it matters.** A board made by `netsmith fab` is interchangeable
   with theirs in these ways, and a checker with tolerances will fail when one is off:
   - the board outline and the four mounting holes;
   - the connectors (J1 to J4), the buttons (SW1, SW2) and the pin headers: the same
     positions, rotations and pads, with each header pin carrying the same signal.

   The other parts (MCU, flash, crystal, regulator, inductor, decoupling) may be anywhere,
   and the routing will differ; the checker reports them and does not fail on them.

## What is here

- `netsmith.toml`, `src/main.nsm`, `src/oscillator.nsm`, `src/main.lock.toml`: the project, its
  board and the crystal's module (see The board). The lock is the catalog row each part was
  picked as; commit it, refresh it with `netsmith lock --update`.
- `catalogs/capacitor.csv`, `catalogs/resistor.csv`: two rows the standard catalogs lack, the
  10 uF 0805 capacitor and the 0 ohm link (see The catalog rows).
- `src/tactile_switch.nsm`, `catalogs/ldo.csv`, `catalogs/inductor.csv`,
  `catalogs/tactile_switch.csv`, `kicad.toml`, `libs/rp2350_minimal.pretty/`: the regulator U2,
  the inductor L1 and the buttons SW1 and SW2 (see Parts).

- `src/rp2350a.nsm`, `catalogs/rp2350a.csv`, `kicad.toml`: the MCU, U1 (see Parts).
- `src/flash.nsm`, `src/crystal.nsm`, `catalogs/w25q128jv.csv`, `catalogs/abm8_272_t3.csv`,
  `kicad.toml`: the QSPI flash, U3, and the crystal, X1 (see Parts).

- `src/amphenol_10103594.nsm`, `src/jst_sm03b_srss_tb.nsm`, `src/samtec_tsw_118.nsm`,
  `catalogs/micro_usb.csv`, `catalogs/jst_srss.csv`, `catalogs/samtec_tsw_118.csv`,
  `libs/netsmith_rp2350.pretty/`: the
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
- `reference/parts.toml`: the parts file of the port, Raspberry Pi's designator of each part by its
  member path (see Parity): all 43, the four mounting holes `hole1` to `hole4` as MH1 to MH4. The
  nets proof and the parity check read it.
- `src/main.physical.nsm`: the board's physical file: the outline, the mounting holes and the
  places of J1 to J4, SW1 and SW2, each number read from their board (see The physical file).
- `src/main.pcb.toml`: the layout file, with only a placer seed (see The physical file).
- `libs/rp2350_minimal.pretty/MountingHole_2.75mm_M2.5.kicad_mod` and its row in `kicad.toml`:
  the 2.75 mm non-plated hole with the 3.15 mm mask opening of Raspberry Pi's (the standard rows
  have only M2 and M3; the parity checker compares the pad's size).
- `reference/parity-overlay.svg`: the picture of the parity check (see Parity).

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
- **U3, `W25Q128Jv`** (`src/flash.nsm`): Winbond's W25Q128JVSIQ, a 16 MB (128 Mbit) QSPI NOR
  flash in the 8-pin SOIC 208-mil package. The pin table is written by hand from Winbond's
  datasheet "W25Q128JV, 3V 128M-bit serial flash memory with dual/quad SPI", Publication Release
  Date August 23, 2021, Revision I (79 pages), fetched on 2026-11-10 from
  <https://www.winbond.com/resource-files/W25Q128JV%20RevI%2008232021%20Plus.pdf>: Figure 1a and
  section 3.3 "Pin Description SOIC 208-mil, WSON 6x5-mm / 8x6-mm" (printed page 5) for the
  pads, names and I/O, and section 10.1 "8-Pin SOIC 208-mil (Package Code S)" (printed page 67)
  for the package. The datasheet writes `/CS`, `/WP` and `/HOLD`; the table writes `CS#`, `WP#(IO2)`
  and `HOLD#/RESET#(IO3)`, the same signals. Levels, ratings and timing are left out, so the
  part is a boundary of the design.
- **U3 footprint** (`kicad.toml`): no generator id is the 208-mil SOIC (the SOIC-8 of the
  generators is the 3.9 mm one), so the row asks for the generator `SO` with the datasheet's
  numbers (pitch 1.27 mm, body 5.3 x 5.3 mm, 7.7 to 8.1 mm over the leads, lead width 0.35 to
  0.48 mm, lead length 0.50 to 0.80 mm). The datasheet draws no recommended land pattern. The
  pads are at the centres of Raspberry Pi's (x = +-3.6 mm, 1.27 mm apart) and are 1.6 x 0.6 mm
  where theirs are 1.7 x 0.65 mm.
- **X1, `Abm8272T3`** (`src/crystal.nsm`): Abracon's ABM8-272-T3, 12 MHz, a 3.2 x 2.5 x 0.8 mm
  ceramic crystal with four pads: the crystal on pads 1 and 3, ground on 2 and 4. The standard
  `Crystal` has two terminals, so the project has its own part (all four pins `passive`; the pin
  names are ours, the drawing numbers its pads). Source: Abracon's "Source Control Drawing"
  ABM8-272-T3 (drawing # 456603, Revision B, date of issue September 16th, 2024; a PDF of three
  pages, numbered 2, 4 and 6 of 9), fetched on 2026-11-10 from
  <https://abracon.com/datasheets/ABM8-272-T3.pdf>: "Key Electrical Specifications" (page 2)
  and "Mechanical Dimensions" (page 4). Frequency 12 MHz, tolerance +/- 30 ppm at 25 C and
  stability +/- 30 ppm over -40 to +85 C (the catalog row has them added, +/- 60 ppm, as the
  standard crystal catalog does; aging, +/- 5 ppm in the first year, is not included), load
  capacitance 10 pF, equivalent series resistance 50 ohm at most.
- **X1 footprint** (`kicad.toml`): the generator's `Crystal:Crystal_SMD_3225-4Pin_3.2x2.5mm`,
  with pads 1.4 x 1.2 mm at +-1.1 mm and +-0.85 mm: Raspberry Pi's pads exactly. The drawing's
  terminals (0.9 x 0.8 mm, page 4) lie inside them; the extract of the datasheet that was
  read has no recommended land pattern. The pad numbers are the drawing's.
- **Catalog rows** (`catalogs/w25q128jv.csv`, `catalogs/abm8_272_t3.csv`): JLCPCB C97521
  (Basic) and C20625731 (Extended), each with the page of the JLCPCB parts library it was read
  from and the day, 2026-11-10. Price and stock are not recorded.
- **What the reference says about them** (checked by the tests against
  `reference/placements.toml`): U3's pads carry `QSPI_SD0` to `QSPI_SD3` on the pins the
  datasheet calls IO0 to IO3 (pad 5 DI, 2 DO, 3 WP#, 7 HOLD#/RESET#), `QSPI_SCLK` on CLK, `GND`
  and `+3V3`. The one name that differs is pad 1, CS#: it is on `FLASH_SS`, which reaches the
  MCU's `QSPI_SS` through a resistor (R10), has a pull-up to `+3V3` (R1) and a resistor to `GPIO0`
  (R9) on Raspberry Pi's board, so that net is not named like the pin. X1's pads 2 and 4 are `GND`,
  pad 1 is `XIN` and pad 3 is on the net of a damping resistor (R2) to `XOUT`.
- **Left out on purpose:** the alternate functions of the pins (a GPIO has `GPIO`, and
  GPIO26 to GPIO29 also `ADC0` to `ADC3`; the board chooses `GPIO` on those four), the functions of the QSPI pins, and every level, rating and model. With no model the
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
  header, 2.54 mm pitch, through hole; one part for both, 36 pins, `provides Connector`.
  Footprint `PinHeader_2x18_P2.54mm_Samtec_TSW-118-07-L-D`.
  - **Catalog row** (`catalogs/samtec_tsw_118.csv`): LCSC **C6887963**, the `lcsc_pn` that
    Raspberry Pi's board carries on J2 and J3 (`reference/RPI-RP2350A-MINIMAL_R4-S1.kicad_pcb`).
    Read on 2026-11-10 from LCSC's product page (`lcsc.com/product-detail/C6887963.html`): title
    TSW-118-07-L-D, manufacturer Samtec, 36 pins, 2 x 18, 2 rows, pitch and row spacing 2.54 mm,
    "Through Hole,P=2.54mm", so it **is the Samtec part**, not an equivalent. **Not read:** the
    JLCPCB parts page `jlcpcb.com/partdetail/C6887963` answered with its application shell only
    (no MPN, package or library type), so the row's `library` cell is empty (read it from
    JLCPCB's page before ordering). Price and stock are not recorded.
  - **Assembly today:** the manifest says nothing, so the part is **soldered by hand** (ADR 0024:
    a through-hole part is `Hand` unless a board says otherwise, and a test holds it, with the
    LCSC number); the row does not change that. `AssemblyMode::Consigned` is the way to have
    JLCPCB place a header that is shipped to them.
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

## Parity

`netsmith parity <board.kicad_pcb> [--table <placements.toml>]
[--parts <parts.toml>] [--svg <overlay.svg>]`, run in this folder, compares the `.kicad_pcb` that `netsmith fab`
writes (in `fab/<Board>/`) with Raspberry Pi's board, as `reference/placements.toml` states it.
It prints one line per item and `must-match: N ok, M failed`, writes the overlay (both boards at
one scale: Raspberry Pi's in blue, netsmith's in orange, a failing must-match item in red with
its offset) and exits 1 when a must-match item fails. The test
`rp2350_minimal_pcb_matches_raspberry_pi` runs `fab` and the checker in CI and fails when a
must-match item is not `ok`. The checking is `netsmith::parity`;
`crates/netsmith/tests/rp2350_parity.rs` tests it, also on Raspberry Pi's own board against
itself.

**The test does not wait for the router.** It runs `netsmith fab --locked --route-timeout 20`
with no KiCad (`NETSMITH_NO_KICAD=1`), and takes the board file that `fab` writes complete or
marked INCOMPLETE (a run that the timeout stopped exits 1 and says `INCOMPLETE:`; any other
failure fails the test). The placer fixes every part before the router starts, so the checker
sees the same board whether the router finished or not, and the test takes about 25 s. It also
holds that `reference/parity-overlay.svg` is the overlay of that board.

**What is checked.** The report prints what each verdict compared, at its head. Must match (a
failure):

- the outline: the bounding box (width, height), the corner radius, the number of `Edge.Cuts`
  lines and arcs and each arc's radius (a corner cut by a line, or a cut-out inside the board,
  changes the count and fails though the box is the same);
- each hole's place and drill, whether it is plated, its pad's size and its pad's net (by name; a
  made-up name is no net);
- for the connectors, buttons and headers (J1 to J4, SW1, SW2), each part's position, rotation
  and layer;
- each pad, one by one: number, centre in its footprint, size and angle in the footprint's frame
  (a pad turned by 90 degrees has its width and height swapped, so the same copper written two
  ways is equal and a turned pad is not), shape, the box of a custom pad's polygon, drill (both
  sides of a slot, round against slot), type, copper layers (as sets: a pad on the other face
  fails), and net. The pads of one number (J1's shell has a
  surface pad and a plated slot for each of 6, 7, 9 and 10) are paired by the nearest centre in
  the footprint, and each pair is judged.

A part of nothing but non-plated pads is a mounting hole, and so is a plated `MountingHole*`
footprint of KiCad's library (its drilled pad is the hole; a ring of vias round it is not
compared); a connector with a peg is still a part and the peg is a pad of it. Reported, never a failure (`info`): the same figures of every other
part (MCU, flash, crystal, regulator, inductor, decoupling), pads reported as one line per part.
Not compared, because the table does not hold it: a rounded rectangle's corner ratio, a custom
pad's anchor shape and its polygon's shape inside the box, a pad's mask and paste layers, the
silkscreen, the courtyard and the routing. A table with no part, a part with no `must_match` or an
outline with no count of lines or arcs is an error, not a pass.

**The origin.** `fab` gives a board an origin of its own, so both boards are read from the
upper-left corner of the outline's bounding box (the smallest x and y of `Edge.Cuts`, KiCad's y
grows downward): it is a corner of the box in both files and needs no flip of an axis. A board
moved as a whole passes; the two corners as the files state them are an `info` line.

**The map.** Raspberry Pi's references and netsmith's differ, so the checker pairs them by the
member path: `reference/parts.toml` has a line `usb = "J1"` (the member path, then Raspberry Pi's
reference) for each part, and `fab` writes each footprint's member path on the board as a hidden
property (`netsmith_path`); a board with no such property is refused. A part of the table with no line is `unmapped`: a failure if it must match, else a
note. Map every part whose pads carry a net, U1 included: the nets are compared by name where
both names are plain (a leading `/` and the case are ignored; `Net-(...)` and `unconnected-...`
are not plain), and through the connectivity otherwise. A pad's net passes when the names are
equal, or when they differ and the same mapped pads are on the net in both boards (a warning
says so); two header pads on each other's nets fail. A name that differs, or of which one is
made up (`Net-(...)`, `unconnected-...` or none), with no other mapped pad on the net to vouch
for it fails: nothing vouches for it. A pad with no net is not a made-up name: where the pad's
net in one board has other pads (mapped or not) it does not pass against a pad that is on no net,
or alone on its own, in the other (both unconnected passes).

**Tolerances** (mm, degrees). The owner's: outline and mounting holes 0.05, positions of parts
0.1, rotation exact (modulo 360), pad numbers equal, nets equal. Chosen where the owner was
silent: the corner radius 0.05 and a hole's drill 0.05 (as the outline and the holes), a pad's
centre in its footprint 0.1 (as the positions), a pad's width, height and drill 0.05, the rotation
0.001 degree (the rounding of the file; a pad's angle in its footprint uses it too), a custom
pad's box 0.05 (as a pad's size), the layer equal.

## Run it

From this folder:

```sh
netsmith build    # the schematic, in out/ (no KiCad needed)
netsmith fab --locked   # the package for the board house, in fab/Rp2350Minimal/; about 35 s
```

`--locked` stops the run if the parts it picks differ from `src/main.lock.toml` and never
writes the pick lock; it also means `--pins locked`, which is all there is to say about pins
here, because the board has no pin requests (see The board). With KiCad hidden (`NETSMITH_NO_KICAD=1`)
the run is the same. Then the parity check, from the repository's root, and the overlay it
writes:

```sh
cd examples/rp2350_minimal
netsmith parity fab/Rp2350Minimal/Rp2350Minimal.kicad_pcb \
    --svg reference/parity-overlay.svg
```

`reference/parity-overlay.svg` is the picture of the last such run, and the test above checks
that it is current (`fab` is deterministic, so the file only changes when the board does).

`netsmith compare` takes their board file as one side and the source as the other:

```sh
netsmith compare reference/RPI-RP2350A-MINIMAL_R4-S1.kicad_pcb src/main.nsm --loose-values
```

and answers `different` with 82 differences, all of one kind: 43 components of theirs with no
counterpart and 39 of the board's with none, because `compare` pairs components by value and
footprint, and their footprints are named `RPI_COMPILED:generic_capc1005x50`,
`RPI_COMPILED:amphenol_10103594-0001lf` and so on where the board's are
`Capacitor_SMD:C_0402_1005Metric`. Their names are not ours to copy (the
footprints of this port are netsmith's own), so the proof of the nets is a test that compares
the wiring alone (next section).

## The physical file

`src/main.physical.nsm` is `physical for Rp2350Minimal`: every figure is read from their board
file (`reference/placements.toml`), in millimetres from the upper-left corner of the outline,
y down, and the comment above each line says which of theirs it is.

- **The outline**: a 52 x 51 mm `Outline` with a 3 mm corner radius.
- **The mounting holes**: four `MountingHole { .package &= "M2.5" }` at their places. The
  standard rows have only M2 (2.2 mm) and M3 (3.2 mm), and theirs are non-plated 2.75 mm, so
  `kicad.toml` has a row for the package `M2.5` and the footprint is a file of the project's own
  library (`libs/rp2350_minimal.pretty/MountingHole_2.75mm_M2.5.kicad_mod`).
- **J1 to J4, SW1 and SW2**: `.place.x`, `.y`, `.rotation` and `.side` of `.usb`, `.header_low`,
  `.header_high`, `.debug`, `.bootsel` and `.reset`. A requirement is measured from the **body**
  of a part (the `F.Fab` outline, else the courtyard), and the checker compares the footprint's
  **origin**, so where the body is not centred on the origin the line states the body's centre:
  J1's courtyard is not centred (its centre is 1.33 mm from the origin) and J4's is 2.7 mm from
  it. A requirement with a single value for every field fixes the part, so these six are put
  exactly where theirs are, and the checker's offsets are 0.000.
- **Everything else** has no line: the MCU, flash, crystal, regulator, inductor, resistors and
  capacitors are the placer's.

Two more files were needed to make the board route, and neither states a requirement:

- **A ground pour** on the back, `.bottom.net &= "GND"` in the board's `.process` in
  `src/main.nsm`. Their board has a ground zone on `B.Cu` too. The pour carries ground: the
  router has no ground to draw on the front, and routes the 179 connections left. With no pour
  the router left 2 of 179 connections unrouted after 223 s (four tries).
- **A placer seed**, `seed = 3` in `src/main.pcb.toml` (the layout file, which keeps a solution
  and never a requirement; it locks no part). Seeds 0, 2 and 3 route completely (97 s, 43 s and
  35 s of routing); seeds 1 and 4 leave two connections unrouted after 120 s. The seed does not
  move the six parts above, only the others.

## The result

The last run (`fab --locked`, seed 3) on the machine that wrote this (a debug build, in which
`netsmith-pcb` is optimised; one run at a time):

| Step | Result |
|---|---|
| Placement | 43 parts on 52 x 51 mm in 2.6 s |
| Routing | all 179 connections routed, on try 1 of 4, in 34 s (35 s in two earlier runs) |
| Design rules check | passed (0 violations; 6 silkscreen warnings: the texts of J1, J3, H1 and H2 touch the board edge, and two silk lines are 0.09 mm apart) |
| Requirements | 32 of 32 satisfied |
| `fab` in all | 37 s |
| Left to look at | 2 vias stand in pads (C16.1, U1.61): order them as epoxy filled and capped, or `fab-notes.txt` says what else |

What stops a run that does not finish: the router's own limit of 300 s (`--route-timeout`), or
its four tries; it then writes the board marked INCOMPLETE and the run fails (`INCOMPLETE:`, with
the unrouted connections in `routing-failure.txt`). The board is not ready to order then.

The parity checker on that board (`must-match: 225 ok, 0 failed`, which the gate's `min_ok` holds):

| Must match | Result |
|---|---|
| Outline, 52 x 51 mm and radius 3 mm | ok (0.000 off) |
| Mounting holes MH1 to MH4, place, drill 2.75 mm, non-plated, 3.15 mm pad, no net | ok (0.000 off) |
| J1, micro-USB: place, rotation 180, layer, 15 pads | ok |
| J2, 2 x 18 header: place, rotation 270, layer, 36 pads | ok |
| J3, 2 x 18 header: place, rotation 270, layer, 36 pads | ok |
| J4, JST-SH: place, rotation 0, layer, 5 pads | ok |
| SW1, SW2: place, rotation 270 and 180, layer, 4 pads each | ok |
| The pin-to-signal map of J2 and J3 | ok: each header pad is on the pad of U1 that theirs is (the nets are named differently, so the connectivity vouches: 43 warnings say so) |

**What is not matched** (reported, never a failure): the other parts' places and rotations
(U1 is 9.3 mm right and 4.95 mm up from theirs, U2 23.3 mm left and 3.6 mm down, U3 11.1 mm
right and 0.6 mm up, X1 0.75 mm right and 6.1 mm up, L1 9.9 mm right and 10.85 mm up and turned;
the resistors and capacitors are all elsewhere), the pads of those parts where netsmith's
footprint is not theirs (U1: 61 pads differ in centre, size or drill, and 18 of theirs have no
partner), and the routing: the copper is netsmith's own, and nothing is claimed about it. 82 items are `info`.
`reference/parity-overlay.svg` draws both boards at one scale (theirs blue, netsmith's orange,
a failing must-match item red): the outline, the holes, the four edge connectors and the two
buttons coincide, and the chip, the flash, the regulator and the rest are where the placer
put them. Open it in a browser.

## The board

`src/main.nsm` is the circuit of Raspberry Pi's board in netsmith's words, not a netlist:

- **Rails.** `.vbus` (the USB supply), `.v3v3` (the regulator's output) and `.v1v1` (the core
  supply, made by the chip's own switching regulator with L1: there is no std part for a
  regulator inside a chip, so it is a rail the board joins to the pins and the inductor), with
  the nets named `VBUS`, `+3V3`, `+1V1` and `GND` as theirs are. The regulator is the standard
  `LinearRegulator` asked for the SOT-223 row of `catalogs/ldo.csv`.
- **Modules.** `Oscillator` (`src/oscillator.nsm`): the crystal, its two 15 pF load capacitors
  and the 1 kohm damping resistor, joined to the chip by `.xin`, `.xout` and `.gnd`.
- **Generics with their values.** Every resistor and capacitor is a `Resistor` or `Capacitor`
  with Raspberry Pi's value and tolerance and the package of their footprint (`1005` metric is
  `0402`, `2012` is `0805`; the tolerances are the 1 % of the resistors, 10 % of the 100 nF and
  10 uF, 20 % of the 4.7 uF and 5 % of the 15 pF, from the Description property of each part).
  R1 is a `PullUp` that states its resistor's package (`.package &= "0402"`). The 1 kohm in
  front of each button is a plain `Resistor`.
- **The headers' pin map is fixed.** `.header_low` (J2) and `.header_high` (J3) are joined
  pin by pin to **named** pins of the chip (`.header_low.p12 ~ .mcu.gpio4`), exactly as their
  pads are. The pin optimizer of `fab` (ADR 0101) moves only a pin that a *request* got
  (`.x: .mcu.GPIO`), never a pin the source names, and this board has no request, so no
  `main.pins.toml` and nothing to move. A test holds it: the optimizer's input is empty here, and
  the same board with one header pad joined to a request is one it could move.
- **The parts** are theirs: 39 of their 43 footprints; the other four are the mounting holes,
  which belong to the physical file.

### Same nets

`cargo test -p netsmith --test main rp2350_minimal::board::` runs these (none needs KiCad):

- `every_pad_has_the_same_pads_on_its_net_as_in_raspberry_pis_board`: reads the committed
  `.kicad_pcb` as a file (`Netlist::from_kicad_pcb`, netsmith's own s-expression reader, the one
  `compare` uses) and the design's nets, pairs the parts by the table in the test (the path of
  each member and Raspberry Pi's designator), and fails when **any pad of any of the 39 parts has
  a different set of other pads on its net**. Netsmith's net names differ and do not matter.
- `netsmith_compare_says_the_nets_are_the_same`: the same two netlists with value and
  footprint blanked go through `netsmith-netlist`'s `compare` (the engine of `netsmith compare`),
  with the designators paired and again with no pairing at all: both say `match`.
- `the_header_pins_carry_raspberry_pis_signals`: each of the 72 header pads is on the net of
  its reference signal (`+3V3`, `GND`, or the chip pin `GPIOn`, `RUN`, `SWCLK`, `SWDIO`).
- `the_resistors_and_capacitors_have_raspberry_pis_values_and_packages`.
- `the_electrical_rules_check_of_kicad_passes_on_the_schematic` (KiCad's ERC; skipped where
  there is no `kicad-cli`).

**The differences, all of them:**

- **Four mounting holes (MH1 to MH4)** are not in the schematic: the physical file's.
- **U2's tab pad.** Their footprint numbers the NCP1117's tab 4 (net `+3V3`, the output); the
  datasheet's pin is 2 and the generator's footprint numbers it 2. Same pin, same net, another
  number: the test reads their pad 4 as pad 2.
- **Which 100 nF is which.** Their eight on `+3V3` and three on `+1V1` are interchangeable, so
  `C12` of theirs is any of the board's eight: the test's table pairs them in order.
- **Decoupling capacitors belong to no pin** in the source, as in their board file (the netlist
  does not say which pin each serves).
- **Footprints and values' spelling** differ as said above (netsmith's footprints, `100nF` for
  `100n`); `--loose-values` reads them as equal.
- **Not stated, so not compared:** their `dielectric`, voltage rating and part numbers. The
  solver picks the first catalog row that fits the value and package, e.g. C1525, 16 V X7R, for
  the 100 nF. Two ICs' details (the RP2350A's ADC functions) see below.
- **GPIO26 to GPIO29** are on their header pads as `GPIO26_ADC0` to `GPIO29_ADC3`; the part
  offers each pin as GPIO or as an ADC input and a connected pin needs one chosen, so the board
  says `GPIO` on all four (the part marks `GPIO` `class`, which a function may be chosen on
  many pins). The pads are the same either way.

### What the checks say

`netsmith build` prints no error and no warning. The notes it prints are about numbers nobody has:
the RP2350A, the flash and the crystal state no supply current (their parts are boundaries: no
levels, ratings or models), so the regulator's output current, the inductor's current, the series
resistors' currents and the rails' tracks are `not evaluated`, and `+3V3`, `+1V1` and `VBUS` say
`has no source` (the connector J1 states no `dc source`). Stating a current of the chip would
be inventing a datasheet figure, so none is stated.

### The catalog rows

The standard catalogs have no 10 uF 0805 capacitor and no 0 ohm link. Both are rows of the
project with their sources: `catalogs/capacitor.csv` (Murata GRM21BR61C106KE15L, LCSC C77075, the
JLCPCB page and the LCSC page read on 2026-11-10; no bias curve and no temperature range, as no
datasheet was read) and `catalogs/resistor.csv` (Yageo RC0402JR-070RL, C60485: the 0.05 ohm
upper end of its range is their board's `+50mR`, not read from the Yageo datasheet; the datasheet
was read later for the height only). Both are Extended. Their part numbers are those of Raspberry
Pi's board (its `lcsc_pn` property) and the JLCPCB and LCSC pages returned the same part for each.

### Heights

Every part has a height (`netsmith board3d --json`: 43 of 43 known, 22 before), so the 3D view
draws each body to it and a height limit or a case checks every part. 38 come from the `height`
cell of the picked row (17 before), 4 are stated in the source and 1 is a package standard's. The
cells were read on 2026-11-13, each
with a `height:` entry in the row's `source` that names the document, its revision, its SHA-256,
the page and the table or view. The cell is the greatest the document allows:

| Parts | Row | Height | Document |
|---|---|---|---|
| R1, R2, R3, R4, R7, R8, R9 | UNI-ROYAL 0402WGF (std) | 0.4 mm | Thick Film Chip Resistors V3, page 4/9, H 0.35 +/-0.05 |
| R5, R6 | Yageo RC0402JR-070RL | 0.4 mm | RC_L series V.12, page 4, Table 1, RC0402 H 0.35 +/-0.05 |
| C18, C19 | Fenghua 0402CG150J500NT (std) | 0.55 mm | General Series A0001, pages 4/27 and 3/27, code CA, T 0.50 +/-0.05 |
| C2, C3, C4 | Murata GRM21BR61C106KE15L | 1.35 mm | Murata catalog C02E, page 104, T max. |
| U1 | RP2350A | 0.9 mm | RP2350 Datasheet, page 1324, Figure 147, A max. |
| U3 | W25Q128JVSIQ | 2.16 mm | W25Q128JV Revision F, page 67, 10.1, A max. |
| X1 | ABM8-272-T3 | 0.8 mm | ABM8 series sheet, Table 2, 0.80 max. (the part's drawing says "See Table 2") |
| J1 | Amphenol 10103594-0001LF | 2.99 mm | drawing 10103594 rev A, front view, (2.99) REF |
| J4 | JST SM03B-SRSS-TB | 2.9 mm | SR/SZ catalog, page 3, side entry, 2.9 |
| J2, J3 | Samtec TSW-118-07-L-D | 8.38 mm | catalog page F-226, lead style -07, A - B = .330 in |

Three connector figures have **no tolerance** in their document, so the cell is the printed
figure and not a maximum: J1's (2.99) is a reference dimension, outside the drawing's general
tolerances; the JST page and Samtec's catalog page print none. The Samtec figure is the tip of the
post above the board (the body .100 in and the post .230 in), as the standard headers' height is;
the series print, which has the tolerances, carries a notice of confidentiality and is not used.
J1's drawing carries Amphenol's reproduction notice (see J1 above).

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
