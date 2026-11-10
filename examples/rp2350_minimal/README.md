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
- `src/flash.nsm`, `src/crystal.nsm`, `catalogs/w25q128jv.csv`, `catalogs/abm8_272_t3.csv`,
  `kicad.toml`: the QSPI flash, U3, and the crystal, X1 (see Parts).
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
