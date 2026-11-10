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
