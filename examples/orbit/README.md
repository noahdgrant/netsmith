# ORBIT

A port of **ORBIT**, an open STM32F030 board with USB, battery charging and a buck-boost
regulator, to netsmith, as the second showcase after the RP2350A Minimal (epic #308). **The
schematic is done:** `src/main.nsm` is the board `Orbit`, the circuit of their design with the
same nets (proved by a test, see "The board"). **So is the board:** `src/main.physical.nsm` states
the outline, the four plated holes and the places of the connectors, switches and buttons, and
`netsmith fab` writes a 50 x 50 mm PCB that routes completely in about 10 s and that the parity
checker finds equal to theirs in every must-match item (129 of 129, see "The result").

## Source

- The design: <https://github.com/mohamedboubaker/Battery-Powered-STM32-Board-With-USB>, files
  `Hardware/ORBIT.kicad_sch` and `Hardware/ORBIT.kicad_pcb`, read from the default branch
  (`main`) at commit `a33a81c9199174a9116dfb75a76a126d2341667e`. The author's project page:
  <https://embeddedprojects101.com/design-a-battery-powered-stm32-board-with-usb/>.
- What it is (their README): an STM32 board that runs on a battery with an MCP73831 charger and
  is programmed over USB through a CP2102 USB-to-UART chip. The files are KiCad 7's: the board
  is format version 20221018 (generator `pcbnew`, no title block, so no revision), the schematic
  is format 20230121. One schematic sheet (84 symbols with the power symbols), 53 footprints, two
  copper layers, a 50 x 50 mm outline with 5 mm corners.
- Licence: MIT, `Copyright (c) 2023 mohamedboubaker`. The notice is in `LICENSE`, as they ship it
  (the repository's `LICENSE` file at that commit, byte for byte), and stays with every copy of
  anything in this folder that comes from their design (`reference/`).

**The circuit is theirs. The port, the netsmith source and the tools in this folder are
netsmith's** (MIT, like the rest of the repository).

## Goal

The same as for the RP2350A Minimal: (1) the schematic of the port has the same nets as theirs,
proved by a test; (2) a board made by `netsmith fab` matches theirs where it must, checked with
tolerances by the parity checker: the outline, the mounting holes, the connectors (USB, battery,
the two pin headers), the buttons and switches, and each header pin carrying the same signal.
The ICs, the crystal, the regulator, the passives and the routing may differ and are reported,
not required. Which parts must match is `reference/must_match.toml` (see below).

## What is here

- `netsmith.toml`, `src/main.nsm`: the project and its board, `Orbit`, with the modules
  `src/charger.nsm`, `src/buck_boost.nsm`, `src/usb_uart.nsm` and `src/oscillator.nsm`.
- `src/main.lock.toml`: the pick lock of the board (the catalog row each part was picked as),
  written by `netsmith lock --update`; CI checks it with `scripts/check-example-locks.sh`. A
  change to the board's parts needs `netsmith lock --update` and the new lock committed.
- `src/main.physical.nsm`: the physical file (outline, holes, the must-match parts' places);
  `src/main.pcb.toml`: the layout file, only the placer's seed (see "The physical file").
- `libs/orbit_small_parts.pretty/MountingHole_2.5mm_M2.5_Pad.kicad_mod` and
  `MountingHole_2.2mm_M2_Pad.kicad_mod`: the plated hole footprints of the physical file.
- `reference/parts.toml`: the parts file of the port, ORBIT's designator of each of the 48 parts of
  the circuit by its member path, which the nets proof and the parity check read;
  `reference/parity-overlay.svg`: the picture of the last parity run (a generated file, see
  "Parity").
- `LICENSE`: their licence text.
- `reference/ORBIT.kicad_pcb`, `reference/ORBIT.kicad_sch`: their board and schematic, unchanged
  (680 KB and 211 KB).
- `reference/must_match.toml`: the reference designators that must match (below).
- `reference/placements.toml`: the facts the checker needs, read from the board file: the
  outline, the holes and, for every footprint, its place, rotation and pads. The header of the
  file says what the origin and axes are, and which entries must match (`must_match = true`).
  Regenerate it with
  `cargo run -p netsmith --example reference_table -- --must-match reference/must_match.toml reference/ORBIT.kicad_pcb > reference/placements.toml`
  (run from this folder); a test (`crates/netsmith/tests/orbit.rs`) checks that the file is
  what the program prints.

## What must match

The outline and the holes are always must-match. Of the 53 footprints, these twelve are
(`reference/must_match.toml`), by the owner's rule: the outline, the mounting holes, the
connectors, the buttons and switches, and the header's pin-to-signal mapping (the net of each
header pad is in `placements.toml`).

| Designator | What it is (their schematic and board) | Choice |
| --- | --- | --- |
| `USB1` | mini USB B receptacle, `MINI-USB-SMD_MINI-5PTP`, 9 pads and 2 locating holes | must |
| `BAT1` | battery connector, JST PH 2 pin, `JST_PH_B2B-PH-SM4-TB` | must |
| `J1`, `J2` | the west and east pin headers, 1 x 8, 2.54 mm | must |
| `SW1` | the RESET push button, `SW_Push_1P1T_NO_CK_KSC6xxJ` | must |
| `SW2` | the USER push button, the same part | must |
| `BOOT0` | slide switch (`K3-1270L-F1`) that pulls the MCU's BOOT0 pin up or down | must (a switch) |
| `PWR1` | slide switch (`K3-1270L-F1`) in the battery's path | must (a switch) |
| `H1` to `H4` | the four mounting holes | must |
| `U1` | MCP73831 charger | not required |
| `U10` | 8 MHz crystal | not required |
| `U15` | TPS63802DLAR buck-boost regulator | not required |
| `U16` | STM32F030C8Tx | not required |
| `U17` | AT24C128C I2C EEPROM | not required |
| `U22` | CP2102-GMR | not required |
| `L1` | 0.47 uH inductor | not required |
| `D1` to `D3` | LEDs (charge, LED1, LED2) | not required |
| `R1` to `R12`, `C2` to `C24` | resistors and capacitors, 0603 | not required |
| `O` | a logo (silkscreen, `board_only`) | not required |

## NOTE-FOR-LATER

What the next stages (parts, board, physical file, parity) will need to know. Facts from the
two files; nothing is a datasheet number.

**What the reference reader met that the first board did not** (fixed in this stage, each with a
test): a KiCad 7 footprint states its reference as `(fp_text reference "J1" ...)` and has no
`(property "Reference" ...)`, so the table's reader and `Netlist::from_kicad_pcb` (used by
`netsmith compare` on a `.kicad_pcb`) found no reference (the table failed with "has no
Reference property"; the netlist silently had no component at all); the four mounting holes are
KiCad's plated `MountingHole_*_Pad_Via` footprints (a drilled 5 mm pad and a ring of eight
0.8 mm vias, all nine pads numbered `1`) and no pad is `np_thru_hole`, so the table had no
hole until the reader learned that footprint family (`plated = true` in a `[[hole]]`).
The only `np_thru_hole` pads of the board are USB1's two 0.9 mm locating holes, and they are in
the table as holes (must match, with USB1).

**Mounting holes.** H1 at (170, 74), H2 at (128, 74), H4 at (170, 116): drill 2.5 mm. H3 at
(128, 116) is the M2 one, `MountingHole_2.2mm_M2_Pad_Via`, drill 2.2 mm. The page origin is not
the board's corner: the outline spans x 124 to 174 and y 70 to 120 (four lines and four
5 mm-radius arcs, drawn `start/mid/end`).

**Custom footprints and symbols** (the footprints of `Project Footprints:`, a library of the
author's, not in KiCad's): `K3-1270L-F1` (BOOT0 and PWR1, five pads: three SMD and two
thru-hole circles), `MINI-USB-SMD_MINI-5PTP` (USB1), `QFN28G_0.5-5X5MM` (U22, 29 pads),
`TPS63802DLAR` (U15, 10 pads), `SOIC127P600X175-8N` (U17, 8 pads), `INDC2016X120N` (L1). The
schematic symbols of U1, U15, U17 and USB1 are in the author's `Project Symbols` library, and
U22, L1 and BOOT0/PWR1 come from `CP2102-GMR`, `DFE201612E-2R2M_P2` and `Korean Hroparts Elec
SPDT` libraries. Everything else uses KiCad's standard footprints (0603 passives and LEDs,
SOT-23-5, LQFP-48 7 x 7 mm 0.5 mm pitch, the 1 x 8 headers, JST PH, a 5.0 x 3.2 mm two-pin
crystal).

**Parts with no LCSC number in the schematic:** `U17` (AT24C128C, value `~`), the headers `J1`
and `J2`, and the mounting holes. `U22` has the number under a different key (`LCSC Part#`, no
space, `C6568`); every other part has `LCSC Part #`. The numbers, as written, to check against
the JLCPCB catalog when each part is added (a part number goes into a catalog only with a recorded
source): BAT1 C160352, BOOT0 and PWR1 C145857, SW1 and SW2 C318884, U1 C424093, U10 C115962, U15
C2845237, U16 C23922, L1 C668312, USB1 C2681563, D1 and D2 C72043, D3 C72041.

**The schematic's reference fields are not the instance references.** In the schematic the
`Reference` property of a symbol can be stale: BOOT0's symbol states `#PWR08` as its property
and BOOT0 as its instance (`(instances ... (reference "BOOT0"))`), which is what the board
carries. KiCad's own netlist export (`kicad-cli`) uses the instance. Read the instance, not the
property, if a script reads the `.kicad_sch`.

**Nets of the board file** (91 entries, the numbered `(net N "name")` table at the top of the
file, KiCad 7's form; a pad states `(net 12 "/USB_D+")`, so the name is its last item). Named
signal nets: `+BATT`, `+3.3V`, `VBUS`, `GND`, `NRST`, `/USB_D+`, `/USB_D-`, `/HSE_IN`, `/HSE_OUT`,
`SWDIO`, `SWDCLK`, `USART1_TX`, `USART1_RX`, `I2C1_SCLK`, `I2C1_SDA`, `LED1`, `LED2`,
`USER_Button`, and the pin nets `PA2`, `PA3`, `PA6`, `PA7`, `PB0`, `PB1`, `PB2`, `PB10`, `PB11`,
`PC14`. The rest are `Net-(...)` (for example `Net-(U15-L1)`, `Net-(U15-L2)`, `Net-(U15-FB)`,
`Net-(U15-PG)`, `Net-(U1-PROG)`, `Net-(BAT1-Pin_2)`, `Net-(C18-Pad1)`, `Net-(U16-BOOT0)`,
`Net-(BOOT0-Pad1)`) and 49 `unconnected-(...)` nets (most of the STM32's and the CP2102's pins).

**The connectors and the pins of the headers.** Net by pad number, 1 to 8:
- `J1`, the west header (x 126.5, y 92): GND, SWDIO, SWDCLK, NRST, PC14, PA2, PA3, +3.3V.
- `J2`, the east header (x 171.5, y 92): GND, PB11, PB10, PB2, PB1, PB0, PA7, PA6.
- `BAT1`: pad 1 `GND`, pad 2 `Net-(BAT1-Pin_2)`, the two `MP` pads unconnected. The battery's
  plus reaches `+BATT` through `PWR1` (pad 1 of PWR1 is on `Net-(BAT1-Pin_2)`, pad 3 on
  `+BATT`, pad 2 unconnected).
- `USB1`: pad 1 `VBUS`, 2 `/USB_D-`, 3 `/USB_D+`, 5 `GND`, and the ID pad 4, the shield pad 6 and
  pads 7 to 9 unconnected.
- `BOOT0`: pad 1 `Net-(BOOT0-Pad1)`, 2 `+3.3V`, 3 `Net-(U16-BOOT0)`.
- `SW1` (RESET): pads 1 `NRST`, 2 `GND`. `SW2` (USER): pad 1 `+3.3V`, pad 2 `Net-(C18-Pad1)`
  (with a 10 pF capacitor C18 on that net); the net `USER_Button` is also in the file.
  Both buttons draw four pads (each pad number twice).

**Other things in the file.** The board has no title block. It has one `GND` copper zone on
both layers (`F&B.Cu`), 299 track segments, 49 vias and 20 silkscreen texts (pin labels near
the headers). U22's package is a QFN-28 with an exposed pad (pad 29). `U15` is a 10-pad
package. The 3D model paths are `:KICAD7_3DMODEL_DIR:` and the repository also ships its own
`jlcpcb` folder, a `.step` file and `Project Symbols.kicad_sym`; none of those is copied here.
There are 18 capacitors of one package (0603) and values from 10 pF to 22 uF, and 12 resistors,
all 0603; the same value is repeated (for example 100 nF on eight sites), so the later stage
can group them as the RP2350 board did.

## Parts

### The connectors, switches, crystal, LEDs and inductor

The non-passive, non-IC parts of ORBIT's board are the nine kinds below, plus the four mounting
holes `H1` to `H4` and the logo `O`, which belong to the board stage (the holes are plated
`MountingHole_*_Pad_Via` footprints, 2.5 mm and 2.2 mm; `std/kicad.toml` has non-plated M2 and
M3 rows only; the logo is `board_only`). The resistors and capacitors are standard parts, and
the five ICs (`U1`, `U15`, `U16`, `U17`, `U22`) are not in this section. A part here says what
a part is; which signal is on which pin, where each stands and the designators of ORBIT's
(`U10` is the crystal, `L1`, `D1` to `D3`, `USB1`, `BAT1`, `SW1`, `SW2`, `BOOT0`, `PWR1`) are
the board's (the standard crystal's letter is `Y`, the standard connector's `J`). Each
must-match footprint is compared pad for pad with `reference/placements.toml` by a test
(`crates/netsmith/tests/orbit.rs`, module `small_parts`): number, centre in the footprint's own
frame (0.05 mm), size, shape, kind, drill and angle; the fixture board of the tests builds with
all the parts and no KiCad install. The footprints are files of the project's own library,
`libs/orbit_small_parts.pretty`, listed in `kicad.toml`. **No dimension was guessed**: what is
not on a drawing is listed as assumed, and where the board differs from a drawing the footprint
of a must-match part follows the board and says so. Numbers are mm; x is to the right and y
down, the part seen from the component side, as in the reference table. Every PDF was fetched
on 2026-11-10.

- **USB1, `ShouHanMini5ptp`** (`src/shouhan_mini_5ptp.nsm`): Shou Han MINI 5PTP, mini USB B
  receptacle, 5 contacts, SMT right angle, LCSC C2681563, JLCPCB Extended (the JLCPCB page and
  LCSC's record were read; `catalogs/usb_mini_b.csv`; price and stock not recorded). Pins:
  VBUS 1, D- 2, D+ 3, ID 4, GND 5, and the shield, one pin of the four pads 6 to 9.
  `provides Connector`. Footprint `USB_Mini-B_ShouHan_MINI-5PTP`: 11 pads (the nine numbered
  ones and two unnumbered locating holes of 0.9 mm, which are in the table as holes).
  - Source: the maker's approval sheet for `MINI 5PTP-GZ` (Shenzhen Shouhan Technology Co.,
    three pages, the drawing's revision `A0`, dated 2020.8.24; the PDF that LCSC links for the
    number, `datasheet.lcsc.com/datasheet/pdf/a6202504fc03d733b65a4279f29d71f7.pdf`, SHA-256
    `4d72b9694f5d`): page 1, the view "RECOMMENDED PCB LAYOUT"; page 2, the ratings (1 A per
    contact, 30 V DC). Its pages carry the maker's name as a watermark and state no copyright.
  - **Read:** signal pads 0.50 wide at 0.8 pitch (4 x 0.8, 0.50), 2.30 long (2.30); shell
    pads 2.50 x 2.00 (2.50, 2.00), 5.50 apart in the depth of the connector (5.50) and 8.90 across
    (8.90); two holes of 0.9 mm (2 x 0.9), 4.40 apart (4.40).
  - **Derived by arithmetic:** the shell pads' centres at 2.25 and 7.75 mm from the drawing's
    datum line (1.00 + 1.25; 5.50 apart) and the holes' line at 5.20 from it, so with the
    holes' line as y = 0 the shell pads' centres are at y = -2.55 and 2.95, the board's.
  - **Differs from the drawing, the footprint follows the board:** the board's signal pads are
    2.2 long and centred at y = -2.95 (from -4.05 to -1.85), the drawing's 2.30 long from -3.80
    to -1.50; its shell pads are 2.2 long against 2.5; its holes are at y = 0.1, the drawing's
    at 0 against the shell pads (up to 0.35 mm at an edge). Whether the maker drew another
    revision or the author shortened the pads is not known.
  - **Assumed:** the pad numbers (the drawing numbers none: the board's; the order VBUS, D-,
    D+, ID, GND is the board's nets and the USB-IF specification was not read); the shell on
    pads 6 to 9, one pin; paste on the SMD pads; the courtyard; no silkscreen or 3D model.
- **BAT1, `JstB2bPhSm4Tb`** (`src/jst_b2b_ph_sm4_tb.nsm`): JST B2B-PH-SM4-TB(LF)(SN), 2 contacts at
  2.0 mm, top entry, SMT, two mounting tabs, LCSC C160352, JLCPCB Extended
  (`catalogs/jst_ph_smt.csv`). Pins 1, 2 and `MP` (the two tabs, both numbered `MP` as KiCad
  and the board do). `provides Connector`. Footprint
  `JST_B2B-PH-SM4-TB_1x02-1MP_P2.00mm_Vertical`: 4 pads.
  - Source: JST, "PH CONNECTOR, 2.0mm pitch" (the PDF that LCSC links for C160352,
    `datasheet.lcsc.com/datasheet/pdf/6c6fce4367275d84ec6a99bf09f1250f.pdf`, 3 pages, SHA-256
    `4e41351f0926`; no revision or date is printed): page 1, "SMT type, SM4 type, top entry";
    page 3 for the part number of 2 circuits. No notice of copyright was seen on page 1.
  - **Read:** the contact pitch 2 (+-0.05) and the tab length 3 (+-0.1). The chain of
    dimensions 1.6, 1.6 and 2 (+-0.1, +-0.1, +-0.05) next to them could not be tied to pad
    edges with certainty and is not used.
  - **Assumed (the board's):** every other number: contacts 1.0 x 5.5 centred at y = 0.5, tabs
    1.6 x 3.0 at x = +-3.4, y = -1.75; the origin; the rounded-rectangle shape (ratio 0.25,
    KiCad's default) of the four pads; pad numbers 1 and 2 left to right and `MP`; paste on
    all pads; no body, silkscreen or model.
- **J1 and J2, the standard `Header1x8`:** 1 x 8, 2.54 mm. The generator's
  `PinHeader_1x08_P2.54mm_Vertical` is the board's footprint pad for pad (1.7 x 1.7 mm, drill
  1.0 mm, pin 1 square, the others oval), so no project footprint was written. ORBIT's schematic
  states no LCSC number for them: the standard row is a header of the standard catalog and is
  soldered by hand (ADR 0024).
- **SW1 (RESET) and SW2 (USER), `XkbTs1187a`** (`src/xkb_ts_1187a.nsm`): XKB TS-1187A-B-A-B,
  tactile switch, 5.1 x 5.1 mm, SPST, 1.5 mm high, 160 gf, LCSC C318884, 50 mA 12 V
  (`catalogs/xkb_ts_1187a.csv`; JLCPCB's page printed no library type, so the cell is empty).
  Pins `A` (pad 1, the two upper pads) and `B` (pad 2, the two lower pads), one pin for each pair
  as in the RP2350A Minimal's switch. Footprint
  `SW_Push_XKB_TS-1187A_CK_KSC6-pad-layout`: 4 pads, the board's.
  - Source: XKB Industrial Precision, drawing "TS-1187A-X-X-X", "TACT SWITCH", revision A0, sheet 1/1
    (`datasheet.lcsc.com/datasheet/pdf/56c8799ae5193945a16a1ffbe378246a.pdf`, SHA-256
    `64b75233cada`): the circuit (terminals A and B joined, C and D joined, the button between
    the pairs), the specification (contact rating 50 mA at 12 V DC) and the ordering codes
    (`B` 160 gf, `A` 1.5 mm, `B` brass keystake). No notice of copyright was seen.
  - **The pad pattern is not this part's.** ORBIT's footprint is KiCad's
    `SW_Push_1P1T_NO_CK_KSC6xxJ` (C&K), pads 2.8 x 1.0 at +-2.9 and +-2.0, which are the J-bend
    "PAD LAYOUT" of the C&K KSC series catalogue (page B-50, "KSC Series, Sealed Tact Switch
    for SMT", the Farnell copy `farnell.com/datasheets/2706287.pdf`, SHA-256 `03a6544b6ea7`:
    8.6 over the pads, 3 between them, 4 between the rows), a 6.2 mm sealed switch. XKB's own
    "P.C.B. LAYOUT" draws four pads 7.0 over their outer edges, 5.0 over the inner ones, 4.5 and
    3.0 in the other direction, so (arithmetic) 1.0 x 0.75 mm pads at +-3.0 and +-1.875: each of
    them lies inside the board's pad, so the part can be soldered to the board's pads, though not
    by its recommended layout. The footprint is the board's (must match).
  - **Assumed:** that terminals A and B are pad 1 and C and D pad 2 (the drawing's letters and
    the board's numbers are tied by nothing); the courtyard; no silkscreen or model.
- **BOOT0 and PWR1, `HroK3_1270`** (`src/hro_k3_1270.nsm`): HRO (Korean Hroparts Elec) K3-1270
  slide switch, SPDT, SMT right angle. ORBIT's schematic and board name it **K3-1270L-F1** and
  give LCSC **C145857**, which JLCPCB and LCSC list as the **K3-1270S-F2** (JLCPCB Extended,
  300 mA, 6 V; the ratings are the record's, the drawings state none;
  `catalogs/hro_k3_1270.csv`). Pins `A` (pad 1), `B` (pad 2), `COM` (pad 3), `MOUNT` (the two
  thru-hole posts, pads 4 and 5). Footprint `SW_Slide_HRO_K3-1270L-F1`: 5 pads.
  - Source: HRO, "TOGGLE SWITCHES", drawings K3-1270L-F1 and K3-1270S-F2 (and K3-1270R-F1, not
    used), each one sheet, revision A, 2013/9/25 (the PDFs that LCSC links for C145857:
    `datasheet.lcsc.com/datasheet/pdf/ad2d443178044447afa1cc589acd4217.pdf` (L-F1, SHA-256
    `35c95adcd83d`) and `.../39879f36987cf1f5e28bc2ed9bba2339.pdf` (S-F2, `9064d610d33a`)). The
    sheets carry the maker's name as a watermark and state no copyright.
  - **Read** (the two drawings agree): "PCB Layout (Pattern side)": three pads 0.90 wide and
    2.50 long (0.90, 2.50) at 2.00 pitch (2.00, 2.00), 2 holes of 0.80 (2 x 0.80) 6.30 apart
    (6.30), the first pad 2.15 from the first hole (2.15), 5.00 from the pads' outer ends to
    the holes' line (5.00); the circuit diagram: the third terminal is the wide one, joined in
    position A to the first and in position B to the second.
  - **Derived by arithmetic:** the pads at y = -1, 1 and 3 and the holes at y = -3.15 and 3.15
    (-3.15 + 2.15 = -1), the pads' centres at x = -1.875 and the holes' at 1.875 (5.00 - 2.50 =
    2.50 from the pads' inner ends to the line): the board's numbers to the last digit.
  - **Differs between the two drawings:** the L-F1 marks its second position (b) with an arrow
    and the characters 复位, which I read as a spring-return ("reset") position; the S-F2 has no
    such mark. The operating force is 150 +-100 gf (L-F1) and 120 +-50 gf (S-F2). The pad layout
    is the same. **Which of the two ORBIT's PWR1 should be is for the owner** (see below).
  - **Assumed:** the terminal numbers (the drawings number none: the board's); the 1.4 mm
    annular pads of the posts (the board's); the courtyard; no silkscreen or model.
  - **What the language cannot say:** the knob's position (see "For the owner"). The part
    states `bridge .a ~> .com`, position A, as the closed one in a chain: a convention, the
    path ORBIT's PWR1 uses (pads 1 and 3); BOOT0 has +3.3 V on pad 2 and its pad 1 on a
    net of its own, `Net-(BOOT0-Pad1)`, for the other position.
- **U10, the standard `Crystal`:** the YXC X50328MSB2GI, 8 MHz, 20 pF, 5032 two-pad package,
  LCSC C115962, is already a row of the standard `crystal` catalog (JLCPCB and LCSC records read
  2026-11-01; `data/jlcpcb/catalog/crystal.csv`), and the generator's
  `Crystal_SMD_5032-2Pin_5.0x3.2mm` is ORBIT's footprint pad for pad (2.0 x 2.4 mm at +-1.85),
  so the project has no file for it. LCSC's record re-read on 2026-11-10 is the same part
  (YXC X50328MSB2GI, SMD5032-2P).
- **L1, the standard `Inductor`, package 0806:** Murata DFE201612E-R47M=P2, 0.47 uH, LCSC
  C668312, JLCPCB Extended (`catalogs/inductor.csv`, which adds its row to the standard
  `inductor` catalog; `kicad.toml` maps the package `0806` to `L_Murata_DFE201612E`).
  - Source: Murata, reference specification J(E)TE243A-0006B-01, "DFE201612E" (9 pages,
    marked "Reference Only"; the PDF that LCSC links,
    `datasheet.lcsc.com/datasheet/pdf/b8f89bf2cd49b99b92d10b430994df40.pdf`, SHA-256
    `d4e77d3e1874`): page 1 (physical dimensions 2.0 x 1.6, 1.2 max), page 2 (electrical
    specifications: 0.47 uH +-20%, 26 mohm maximum, 5.5 A by inductance change, 4.5 A by
    temperature rise), page 5 (recommended PCB pattern).
  - **Read:** pads 1.8 long (1.8), 0.8 between them (0.8), 2.4 over both (2.4). **Derived:**
    pads 0.8 x 1.8 mm with centres at +-0.8 mm.
  - **Differs from the board** (not a must-match part): the author's pads are 1.2 x 1.82 mm at
    +-0.86 mm (`INDC2016X120N`, the author's own library), 0.12 mm further apart in pitch and 0.4 mm wider.
    **Assumed:** pad numbers 1 and 2 (the datasheet numbers none); no body or model.
- **D1 and D2 (green) and D3 (blue), `Everlight19217Ghc` and `Everlight19217Bhc`**
  (`src/everlight_19_217.nsm`): Everlight 19-217/GHC-YR1S2/3T (LCSC C72043, JLCPCB Extended)
  and 19-217/BHC-ZL1M2RY/3T (C72041, Extended), 0603, each a provider of the standard `Led`
  with its own catalog (`catalogs/everlight_19_217_ghc.csv`, `..._bhc.csv`), so that a board
  names the exact part (the generic declares no `.mpn`, so a use of `Led` cannot ask for one).
  The footprint is the standard 0603 LED, `LED_0603_1608Metric` from the generator: ORBIT's
  pads exactly (0.875 x 0.95 at +-0.7875). D1 to D3 are not must-match.
  - Source, green: Everlight, "19-217/GHC-YR1S2/3T", Rev. 2, device no. DSE-0006617, prepared
    09-Feb-2012, 10 pages, a distributor's copy (`static.maritex.eu`, SHA-256 `cb21a3fec7ba`:
    the JLCPCB record links no datasheet and LCSC's links for the number are other makers'
    sheets): page 3, forward voltage 2.70 / 3.30 / 3.70 V (min / typ / max) at 20 mA, operating
    temperature -40 to +85 C. Blue: Everlight, "19-217/BHC-ZL1M2RY/3T", Revision 2, released
    Jun.13.2013, device no. DSE-0009312 (the document that LCSC links, SHA-256 `b95337e6421f`):
    page 2, forward voltage 2.50 min and 3.10 max at 5 mA (tolerance +-0.1 V, not added),
    operating temperature -40 to +85 C. Both carry "Copyright (c) 2010, Everlight All Rights
    Reserved" in their footers: nothing was copied, only the numbers were taken.
  - **Assumed:** the pad numbers (pad 1 the cathode, as the board's nets have it; the
    datasheets read number no pad).
- **What the tests check, and what they do not:** the pad tables against the reference table
  for the eight must-match parts (and U10 and D1 to D3 exactly, L1 within 0.15 mm); that the
  pin table names each pad of its footprint once; the nets of the reference board's pads
  against the pins (USB1 pads 1 to 5, BAT1's, the buttons', BOOT0's and PWR1's common
  terminal); that each catalog row records its JLCPCB page and the day, and no price or
  stock; and the fixture build with no KiCad. **Not run:** KiCad's DRC (netsmith's own design
  rules check ran in `fab`), and no part was compared with a physical sample. KiCad's ERC of
  the board's schematic is a test of its own (see "What was not run"): it runs only where
  `kicad-cli` is found, and skips elsewhere.

### The five ICs (`src/`, `catalogs/`, `kicad.toml`, `libs/orbit_ics.pretty/`)

Each IC of ORBIT is a netsmith part with a pin table written by hand from the maker's
datasheet (the datasheets are copyrighted: only facts, that is numbers, names and kinds, are
taken, and the arrangement and the words are ours), a footprint, a catalog row and a symbol
drawn from the pin table (no `symbol` key, ADR 0097). Every document was fetched on
2026-11-10. No std interface fits any of them (`Buck` is a step-down regulator, `LinearRegulator`
a linear one, and std has no charger, microcontroller, memory or bridge), so none `provides`
one: they are the project's own kinds, and, with no model, boundaries of the design (the checks
that need a number are listed as not evaluated). The tests are in `crates/netsmith/tests/orbit.rs`
(module `ics`) and build a small board of their own with all five parts and no KiCad.

| Ref. | Part | Document | Pins read from | Package |
| --- | --- | --- | --- | --- |
| U1 | `Mcp73831` (`src/mcp73831.nsm`), MCP73831T-2ACI/OT, LCSC C424093 | Microchip DS20001984G, July 2014 | Table 3-1 and sections 3.1 to 3.6, printed page 11 | SOT-23-5 |
| U15 | `Tps63802` (`src/tps63802.nsm`), TPS63802DLAR, LCSC C2845237 | TI SLVSEU9D, November 2018, revised January 2021 | Figure 7-1 and Table 7-1, printed page 4 | VSON-10 (DLA, 2 x 3 mm) |
| U16 | `Stm32F030C8` (`src/stm32f030c8.nsm`), STM32F030C8T6, LCSC C23922 | ST DocID024849 Rev 2, January 2015 (a mirror; see below) | Figure 5 (page 25) and Table 11 (pages 28 to 33) | LQFP-48, 7 x 7 mm |
| U17 | `At24C128C` (`src/at24c128c.nsm`), AT24C128C-SSHM-T, LCSC C6478 | Microchip DS20006110A, November 2018 (a mirror) | section 1 (page 4) and Table 2-1 (pages 5 and 6) | SOIC-8, 150 mil |
| U22 | `Cp2102` (`src/cp2102.nsm`), CP2102-GMR, LCSC C6568 | Silicon Labs CP2102/9 Rev. 1.6, 12/13 (a copy) | Table 9 (page 11) and Figure 2 (page 12) | QFN-28, 5 x 5 mm, exposed pad |

What is not what it seems:

- **The charger is the MCP73831, not the MCP73031.** ORBIT's schematic writes `MCP73031` as U1's
  value; its symbol is `MCP73831/2` and its LCSC number, C424093, is the MCP73831T-2ACI/OT
  (LCSC and JLCPCB both say so). STAT is `tri_state`: "a tri-state logic output on the MCP73831"
  (section 3.3, page 11).
- **U16 is the STM32F030C8.** The schematic's symbol is `STM32F030C8Tx` and the LCSC number is
  the STM32F030C8T6. ST's server did not answer, so the document read is a mirror of ST's PDF
  (<https://pdf.direnc.net/upload/stm32f030c8t6-datasheet.pdf>), DocID024849 Rev 2 of
  January 2015, which is the document ORBIT's schematic links (DM00088500). A web search names
  a later revision (Rev 5, November 2021) that was not read: compare Table 11 with it. The
  LQFP48 column of Table 11 is the table; pads 35 and 36 are `PF6` and `PF7` on the `x8`
  devices (`VSS` and `VDD` on the `xC`).
- **U17 has no LCSC number in ORBIT.** Its schematic value is `~` and it has no `LCSC Part #`.
  Searching LCSC for the part number found C6478, the AT24C128C-SSHM-T in SOIC-8 (the 150 mil
  package whose footprint ORBIT draws): the LCSC and JLCPCB pages (read 2026-11-10) both give
  that part, library type Extended. It is the same device in the package of ORBIT's footprint,
  and it is not ORBIT's: confirm the order code before ordering.
- **U22's number is C6568** under the key `LCSC Part#` (no space), as the scaffold noted. JLCPCB
  lists its maker as Skyworks/Silicon Labs. Silicon Labs' server refused the download, so the
  datasheet read is a distributor's copy (<https://hestore.hu/prod_getfile.php?id=8719>);
  a web search says Silicon Labs now recommends the CP2102N for new designs.
- U15 is a 2 x 3 mm part; the datasheet's description says 1.4 x 2.3 mm, which is the drawing
  of an older package (TPS63802RMW); the change of part number is in the datasheet's revision
  history. The package drawing DLA0010A (2.0 x 3.0 mm) is the one the part and the footprint use.

#### What each part states

- `Mcp73831`: the datasheet's charge current equation `IREG = 1000 V / RPROG` (section 5.1.2,
  page 15) as `.charge_current == 1000V / .rprog`, with the nominal `RPROG` between 2 kohm and 67 kohm
  (`nominal(.rprog)`; DC characteristics, "Charge Impedance Range", page 4), and the regulated voltage of the order code,
  `.vreg` 4.168 V to 4.232 V (the "-2" line of "Regulated Output Voltage", page 3), as a column of
  the catalog row. The equation is nominal: the table of page 3 gives 90 to 110 mA at 10 kohm
  (-5 to +55 C), and that spread is not in the relation.
- `Tps63802`: only `.vref`, the feedback voltage, 500 mV +/- 1 % in PWM mode (electrical
  characteristics, page 6), which the standard `FeedbackDivider` takes
  (`.divider.vref == .converter.vref`, a test). Everything else (input and output range,
  current limits, the inductor and capacitor values of section 8.3) is left out, and the part
  has no `dc` statement, so the analysis of a rail stops at it.
- `Stm32F030C8`, `At24C128C`, `Cp2102`: pin tables only. Alternate functions: the MCU's pins
  have `GPIO` and, only for the pins ORBIT's board uses, the oscillator, USART1, I2C1 and SWD
  functions (no direction: the table gives none); a board that connects a pin with more than one
  function chooses it. `CP2102`'s `NC` pin (ten pads) and its `NC/VPP` pin are `no_connect`
  (the datasheet allows leaving them open or tying them to `VDD`; this part keeps them open) and
  its modem inputs are `optional` (note 3, page 11).
- Not modelled: logic levels, ratings and timing of any part, the I2C bus (`std::bus::I2c`) and
  the UART as interfaces (the board stage may add them: the pins are plain pins, as the RP2350
  parts are).

#### Footprints (`kicad.toml`, `libs/orbit_ics.pretty/`)

Parity requires only the connectors, buttons, switches and headers to match the reference; for
the ICs the closeness of each footprint to ORBIT's pads (number, centre, size) is a test, and
here is what it says.

| Part | Footprint | Read | Derived | Assumed | Against ORBIT's pads |
| --- | --- | --- | --- | --- | --- |
| U1 | the generator's `Package_TO_SOT_SMD:SOT-23-5` | drawing C04-091B, page 22: pitch 0.95, 2.20 to 3.20 over the leads, foot 0.35 to 0.80 | nothing | the pads (the datasheet draws no SOT-23 land pattern): the generator's | identical: 5 pads, 1.325 x 0.60 mm at +-1.1375 mm and 0.95 mm pitch |
| U15 | `libs/orbit_ics.pretty/VSON-10-HR_2x3mm_P0.5mm_TI_DLA0010A` | TI drawing DLA0010A, the "Example Board Layout", PDF page 36: five 0.60 x 0.25 pads 0.90 mm from the package centre line, four 0.90 x 0.25 at 0.75, one 1.30 x 0.25 at 0.55 (dimension labels, in parentheses on the drawing: reference values), 0.50 pitch | the centres | the numbering follows the package outline (PDF page 35), rectangles with square corners (the drawing rounds them R0.05), a courtyard of 0.25 mm round the pads and the 2.0 x 3.0 mm body (the middle of 1.9 to 2.1 and 2.9 to 3.1), no silkscreen or 3D model; "example", not "recommended": TI's drawing calls it a land pattern example | identical numbers, centres and sizes (theirs are rounded rectangles) |
| U16 | the generator's `Package_QFP:LQFP-48_7x7mm_P0.5mm` | ST Figure 31, page 86: 1.20 x 0.30 pads at 0.50 pitch, 9.70 mm over the pads | the pad centres, 4.25 mm from the middle | nothing | identical (48 pads, 1.475 x 0.30 mm at +-4.1625 mm). Each of ST's pads lies inside the generator's, which is 0.275 mm longer |
| U17 | the generator's `Package_SO:SOIC-8_3.9x4.9mm_P1.27mm` | Microchip C04-2057-SN Rev B, page 27: pitch 1.27, 5.40 mm between the rows' centres, pads 0.60 x 1.55 mm | the centres at +-2.70 mm | nothing | the generator's pads are 1.95 x 0.60 mm at +-2.475 mm: the width is Microchip's, the outer edges are 0.025 mm apart; ORBIT's are 1.9812 x 0.5588 mm at +-2.4638 mm (the same pitch and numbers) |
| U22 | `libs/orbit_ics.pretty/QFN-28-1EP_5x5mm_P0.5mm_EP3.25x3.25mm_SiliconLabs_CP2102` | Silicon Labs Rev. 1.6, Figure 4 and Table 11, page 14: C1 = C2 = 4.80 mm, E 0.50, X1 0.20 to 0.30, Y1 0.85 to 0.95, centre pad X2 = Y2 = 3.20 to 3.30, and note 8: nine 0.90 mm paste openings on a 1.1 mm pitch for the centre pad | the pad centres, 2.40 mm from the middle; the numbering from Figure 2 | the middle of the ranges: 0.25 x 0.90 mm and 3.25 x 3.25 mm; rectangles with square corners; the paste openings are unnumbered paste-only pads; a courtyard of 0.25 mm round the pads; the 5.0 x 5.0 mm body on `F.Fab` | different: ORBIT's pads are 0.9 x 0.254 mm but 2.59 mm from the middle, 0.19 mm further out, and its centre pad is 3.1 x 3.1 mm |

#### The catalog rows (`catalogs/`)

One row for each part, `lcsc`, `mpn`, `package` and `library`, with the JLCPCB parts page and the
LCSC page and the day (2026-11-10) in `source`; all five are Extended. Price and stock are not
recorded. A first look at JLCPCB's pages with WebFetch gave part data for all five (none was a
JavaScript shell, and the library type was read from the page's label). The `vreg` of the charger's
row is the datasheet's range, with the page.

#### Reading the reference

The tests compare each part with ORBIT's board file: the nets named like a pin or a supply
(`GND`, `+3.3V`, `+BATT`, `VBUS`, `NRST`, `PA2`, `USART1_TX`, `I2C1_SDA`, ...) are on the pins of
that name, the nets named after a pin (`Net-(U15-FB)`, `unconnected-(U22-DCD-Pad1)`) are on that pin,
and every pad of the reference is a pad of the part. Two things ORBIT's board shows: its charger's
STAT drives the LED D1's cathode (`Net-(D1-K)`), and CP2102's `VDD` pad has decoupling on a net of
its own, `Net-(U22-VDD)`, which is not the 3.3 V rail.

## The board

`src/main.nsm` is ORBIT's circuit, 48 parts: their 53 footprints less the four mounting holes
(`H1` to `H4`) and the logo `O`, which are the physical file's. Build it from this folder with
`netsmith build` (with `NETSMITH_NO_KICAD=1` too: the project needs no KiCad install). It builds
with no error and no warning; what the build prints are notes (below).

- **Structure.** Four modules hold the parts around each chip: `Charger` (the MCP73831 U1, its
  programming resistor R3, the status LED D1 with R1, C7 and C8), `BuckBoost` (the TPS63802 U15,
  L1, the feedback divider R7 and R8, the power-good pull-up R6, C13 and C14), `UsbUart` (the
  CP2102 U22 with C20, C21, C23 and C24) and `Oscillator` (the crystal U10, C11 and C12). The
  board has the connectors, the two slide switches and the two buttons, the MCU with its
  decoupling, the two LEDs D2 and D3, the EEPROM U17 with `I2cPullups` (R9, R10), `PullDown`
  for R12 (R2 is a plain resistor, see "Differences"), and three named rails: `VBUS` (5 V +/- 5%, USB's), `+BATT` and `+3.3V`
  (3.3 V +/- 5%).
- **Header pin map** is ORBIT's and FIXED: J1 (`header_west`) is ground, SWDIO, SWCLK, NRST,
  PC14, PA2, PA3, 3.3 V, and J2 (`header_east`) is ground, PB11, PB10, PB2, PB1, PB0, PA7, PA6,
  each pad joined to a named MCU pin (`.header_east.p2 ~ .mcu.pb11`), never to a request
  (`.x: .mcu.GPIO`), so the pin optimizer (ADR 0101) cannot move one. There is no
  `main.pins.toml`.
- **Supplies and what the solver checks.** `VBUS` is declared 5 V +/- 5% (USB's) and `+3.3V`
  3.3 V +/- 5% (the schematic's net name; the window is a stated requirement). The regulator's
  output is not stated but derived: `BuckBoost`'s `FeedbackDivider` holds `vout == vref * (1 + R7 / R8)` with the
  part's `vref` (500 mV +/- 1%, TPS63802 datasheet page 6) and ORBIT's 560 kohm and 100 kohm at
  1%, which is 3.212 V to 3.391 V, inside the window; a wrong divider stops the build (test).
  The charger's `.rprog` is the 2 kohm +/- 1% resistor fitted and `.charge_current == 1000V / .rprog`
  is 495 to 505 mA (datasheet section 5.1.2). The `+BATT` rail has no voltage: ORBIT names no cell.

### Same nets

Tests in `crates/netsmith/tests/orbit.rs`, module `board` (no KiCad; run
`cargo test -p netsmith --test main orbit::board`):

- `every_pad_has_the_same_pads_on_its_net_as_in_orbits_board`: the netlist of the committed
  `reference/ORBIT.kicad_pcb`, read as a file with netsmith's own reader
  (`Netlist::from_kicad_pcb`), against the design's, parts paired by a table of member path to
  ORBIT's designator. For each of the 48 parts' pads, the set of other pads on its net is the same
  (every pad on a net with another pad: both sides have the same number of them).
- `netsmith_compare_says_the_nets_are_the_same`: `netsmith-netlist::compare` (what `netsmith
  compare` runs) with value and footprint blanked: `match`, with the designators paired and with no
  pairing. Why not the bare `netsmith compare reference/ORBIT.kicad_pcb src/main.nsm`: it pairs
  parts by value and footprint, and ORBIT's footprints have other names (`Project Footprints:...`).
- `the_comparison_sees_a_pad_on_the_wrong_net`: the comparison can fail (a pad on another net, a
  pad on no net, and a design with one header pin moved).
- `the_resistors_and_capacitors_have_orbits_values_and_packages` (12 resistors, 18 capacitors,
  all 0603) and `the_solver_picks_the_lcsc_parts_of_orbits_schematic`: each resistor and
  capacitor is picked as the part whose LCSC number ORBIT's schematic names.
- `the_header_pins_carry_orbits_signals`, `the_optimizer_cannot_move_a_header_pad`,
  `every_assembled_part_is_sourced`, `the_board_has_no_warning_and_no_guideline_finding`, and
  the ERC test (KiCad's electrical rules check of the built schematic; it skips where there is
  no `kicad-cli`).

### Differences from ORBIT's board (all of them)

- `H1` to `H4`, `O`: the physical file's (mounting holes, logo); they have no net.
- **Two resistors have their two pads numbered the other way round:** R9 and R10
  (`I2cPullups` runs each from the supply, pad 1, to the line; ORBIT's pad 1 is on the line). The
  pads of a resistor are interchangeable; the test reads them exchanged, lists them, and
  another test (`only_the_two_resistors_have_pads_numbered_the_other_way`) says they are the
  only ones. **R2 had it too, and no longer does:** it was a `PullDown` (whose resistor has its
  pad 1 on the line and its pad 2 on ground), ORBIT's has pad 1 on ground and pad 2 on BOOT0's
  pad 1, and BOOT0 must match: the parity checker compares the pads on BOOT0's net and saw R2.1
  against R2.2. R2 is now a plain `Resistor` joined as ORBIT's (`.a` to ground, `.b` to the
  switch); the part is the same pick, and `src/main.lock.toml` has its path under a new key.
  R9 and R10 are not must-match parts and keep the std module's numbering.
- **C3 to C6** (four 100 nF on `+3.3V`) are interchangeable: which of them is called `C4` is
  arbitrary.
- **Their U1's value is `MCP73031`**; the part is the MCP73831 (see "Parts"). **U17 has no LCSC
  number in ORBIT**; the board's is C6478 (see "Parts"). **U10 is the standard `Crystal`**; ORBIT
  states `8MHz` only and the +/- 30 ppm is the catalog row's.
- **The tolerances are the catalog rows'.** ORBIT's schematic states values and LCSC numbers and
  no tolerance. The board states the tolerance of the row each LCSC number is in the standard
  catalog (1% for the resistors, 10% for the 100 nF, 4.7 uF, 1 uF and 10 uF capacitors, 20% for the
  22 uF, 5% for the C0G 30 pF and 10 pF), so the solver picks the same parts.
- **D2 and D3:** ORBIT's value fields are `LED1` for D2 and `LED2` for D3, and their
  nets are the other way round (D2 is on the net called `LED2`, PA5; D3 on `LED1`, PA4). The
  board follows the nets.
- PC14 and PC15 take the functions `OSC32_IN` and `OSC32_OUT` (a connected pin with several
  functions has one chosen, and `GPIO` cannot be chosen on two pins); they are GPIO in ORBIT, and
  the pad is on the pin either way.

### What the checker found (new problems in ORBIT's circuit and in netsmith)

- **R3 is at the charger's least, so only its nominal value meets it.** The MCP73831's `PROG`
  resistor range is 2 kohm to 67 kohm (DC characteristics, page 4) and ORBIT's R3 is 2 kohm; with
  the 1% of its LCSC part (C22975) it is 1.98 kohm at worst, under a plain `.rprog >= 2kohm`, which
  reads the worst case. A datasheet's design range is for the nominal value, so the part states
  `nominal(.rprog) >= 2kohm` and `<= 67kohm` (`nominal`, `docs/LANGUAGE.md`, "The nominal value")
  and the charger ties the part's `.rprog` to the resistor fitted (`.ic.rprog == .rprog.resistance`),
  so `.charge_current` is worked out from it (495 to 505 mA). A 1.8 kohm resistor is refused (test
  `the_programming_resistor_at_one_percent_meets_the_datasheets_nominal_least`). The datasheet's
  table gives 450 to 550 mA at 2 kohm, so the part is specified at its edge.
- **The green LEDs on a 3.3 V pin.** D2 is green (Everlight 19-217/GHC, forward voltage 2.7 to
  3.7 V at 20 mA, its datasheet page 3) behind 1.5 kohm from a 3.3 V output: at the top of the
  range it does not turn on. The part has no model of its pin's output level, so the board does
  not check it (the LED's current is `not evaluated`); this is a note for the owner of the design.
- **KiCad's ERC refused the CP2102's `NC` pin of ten pads.** A pin of several pads is drawn as
  stacked pins, and the schematic writer typed the extra ones `passive` and flagged each open, so
  a `no_connect` pin with several pads got a flag on the same point as its own `no_connect` pad:
  `no_connect_connected: Pin with 'no connection' type is connected`. `src/cp2102.nsm` first had
  ten `NC` pins of one pad each as a workaround. Pull request #396 fixed the writer (every pad of
  such a stack is `passive`, with the one flag on the point), and `src/cp2102.nsm` is back to the
  datasheet's table: one `no_connect` pin `NC` with the ten pads 10, 13 to 17 and 19 to 22. The
  pads and nets are the same, and the ERC test of the schematic stays clean.
- **The pad numbering of the std pull-ups** (above) and the other gaps are in the pull request's
  "For the owner".

### Notes the build prints (what is not evaluated)

The ICs are boundaries (their pin tables only, #389), so a check that needs a current or a
level is `not evaluated`: the resistor and LED currents behind a pin, the switches' current
against their rating, the inductor's current, the capacitors' bias. `VBUS has no source` (the mini
USB receptacle states no `dc source`, as the RP2350A Minimal's) and `+BATT has no source` (a battery
connector cannot be a source with the voltage of a cell the schematic does not name).

### What was not run

KiCad's DRC (netsmith's own design rules check ran in `fab`) and a comparison of any part with a
physical sample. The ERC test ran only where `kicad-cli` was found.

## Parity

`netsmith parity <board.kicad_pcb> --table reference/placements.toml
--parts reference/parts.toml [--svg <overlay.svg>]` compares the `.kicad_pcb` that `netsmith
fab` writes (in `fab/Orbit/`) with their board, as `reference/placements.toml` states it. What it
compares, the tolerances (outline and holes 0.05 mm, positions of parts 0.1 mm, rotation exact,
pad numbers and nets equal), the origin (the upper-left corner of the outline) and the meaning of
the verdicts are those of the RP2350A Minimal's checker, described in
`examples/rp2350_minimal/README.md` ("Parity"); the checker is `netsmith::parity`. Without
`--table` and `--parts` the command reads those of the project's `reference/` folder. The test
`orbit_pcb_matches_the_original` (`crates/netsmith/tests/orbit_parity.rs`) runs `netsmith fab
--locked --route-timeout 20` with no KiCad, accepts a complete or an INCOMPLETE board (only a
failure that says `INCOMPLETE:`), fails when a must-match item is not `ok`, and fails when
`reference/parity-overlay.svg` is not the overlay of the board it got. It shares its code with
the RP2350A Minimal's test (`crates/netsmith/tests/common/mod.rs`).

**The map.** ORBIT's references and netsmith's differ (netsmith numbers a kind's parts in the
order of declaration: ORBIT's USB1 is J1 here, its J1 is J3, its U10 is Y1), so the checker pairs
them by the member path: `reference/parts.toml` has one line for each of the 48 parts of the
circuit (paired by value, pad count and nets), and `fab` writes each footprint's member path on the
board (`netsmith_path`), so a renumbered part stays paired. H1 to H4 have no line: the checker judges a plated mounting
hole as a hole, by its place, drill, plating, pad size and net (next section), and the logo has no
pad.

**Plated holes.** Their H1 to H4 are `MountingHole_*_Pad_Via` footprints: a plated drilled pad
with a ring of eight vias, all pads numbered 1. The table has each as a `[[hole]]` with
`plated = true` and as a part. The checker used to judge such a part by every pad, so the ring of
vias, which the owner does not require, would have failed it; it now judges a plated hole as a
hole and not as a part, as it does a non-plated one: the same place and drill (0.05 mm), the same
plating, the same pad (size 0.05 mm; theirs are 5 mm and 4.4 mm, twice the drill) and the same net
(none), and not the ring of vias (`crates/netsmith/src/parity.rs`, tests in `orbit_parity.rs`: a
copy of their board with no vias passes; a hole moved by 0.1 mm, with another drill, with a
2.6 mm pad, or drawn as the standard non-plated `MountingHole` does, fails).

**The overlay** `reference/parity-overlay.svg` is a generated file: both boards at one scale,
theirs in blue and netsmith's in orange, a failing must-match item in red with its offset. Make
it again, from this folder, after `netsmith fab --locked`, from the repository's root:

```sh
netsmith parity examples/orbit/fab/Orbit/Orbit.kicad_pcb \
    --table examples/orbit/reference/placements.toml \
    --parts examples/orbit/reference/parts.toml \
    --svg examples/orbit/reference/parity-overlay.svg
```

`fab` is deterministic (the same source, physical file and seed give the same board; three runs
gave byte-identical overlays), and the test says so when the file is stale. It shows: the
outline and the four plated holes coincide (the ring of vias is theirs alone: blue circles round
each hole), the two slide switches, the battery connector, the mini USB connector with its two
locating holes, the two button footprints and the two pin headers sit exactly under their blue
originals (their orange is drawn over the blue), and everywhere else the orange parts are where
the placer put them: the five ICs, the crystal, the inductor, the LEDs and the 30 resistors and
capacitors are all elsewhere, which is allowed. Open it in a browser.

## Run it

From this folder:

```sh
netsmith build          # the schematic, in out/ (no KiCad needed)
netsmith fab --locked   # the package for the board house, in fab/Orbit/; about 10 s
```

`--locked` stops the run if the parts it picks differ from `src/main.lock.toml` and never writes
the pick lock; it also means `--pins locked`, which is all there is to say about pins here,
because the board has no pin requests (the header pins are joined to named MCU pins, so the pin
optimizer has nothing to move). With KiCad hidden (`NETSMITH_NO_KICAD=1`) the run is the same.
`scripts/check-no-kicad.sh` fabs the project too (about 10 s of its 22). Then the parity check
above.

## The physical file

`src/main.physical.nsm` is `physical for Orbit`: every figure is read from their board file
(`reference/placements.toml`), in millimetres from the upper-left corner of the outline (x 124,
y 70 in their file), y down, and the comment above each line says which of theirs it is.

- **The outline:** a 50 x 50 mm `Outline` with a 5 mm corner radius (their four lines and four
  5 mm arcs).
- **The mounting holes:** four plated holes at their places: `.hole1` to `.hole4` are their H1 to
  H4 in order. **The physical file's `case` section takes a `MountingHole` and no `PlatedHole`**
  (a `PlatedHole` there is an error that says a part belongs in the circuit source), and the
  standard `MountingHole` rows are non-plated. So these are `MountingHole`s whose project rows
  (`kicad.toml`, packages `M2.5-plated` and `M2-plated`) name a footprint of
  `libs/orbit_small_parts.pretty` that draws the drilled pad PLATED: `MountingHole_2.5mm_M2.5_Pad`
  (H1, H2 and H4: pad 5 mm, drill 2.5 mm) and `MountingHole_2.2mm_M2_Pad` (H3: pad 4.4 mm, drill
  2.2 mm). **The ring of eight vias of their footprints is not drawn** (not required); the hole,
  its place, its drill and its plated pad are theirs. The two pads are one pad numbered `1` with
  no net, as theirs; the part has no pin in the circuit, so the schematic does not show it. The
  courtyard of each is the circle of its pad, 2.5 and 2.2 mm in radius, which is as large as a
  courtyard may be in a rounded corner of this outline (a courtyard is a box to the placer's
  check). The footprints' names start with `MountingHole`, which is how the checker's reader
  takes their drilled pad as a hole.
- **USB1, BAT1, J1, J2, SW1, SW2, BOOT0 and PWR1:** `.place.x`, `.y`, `.rotation` and `.side` of
  `.usb`, `.battery`, `.header_west`, `.header_east`, `.reset_button`, `.user_button`,
  `.boot_switch` and `.power_switch`. A requirement is measured from the **body** of a part (the
  `F.Fab` outline, else the courtyard), and the checker compares the footprint's **origin**, so
  where the body is not centred on the origin the line states the body's centre: the USB
  connector's courtyard is 0.05 mm off, the battery connector's 0.05 mm, the headers' bodies
  (the origin is at pin 1) 8.89 mm, and the slide switches' 0.25 mm. A requirement with a single
  value for every field fixes the part, so these are put exactly where theirs are.
- **Everything else** has no line: the ICs, the crystal, the inductor, the LEDs, the resistors and
  the capacitors are the placer's.

The ground pour on the back (`.bottom.net &= "GND"` in the process in `src/main.nsm`, as in their
board, whose `GND` zone is on both layers) came with the schematic. The layout file
`src/main.pcb.toml` has only `seed = 1`, a placer seed (it keeps a solution and never a
requirement; it locks no part). The seed does not move the physical file's parts.

## The result

The last run (`fab --locked`, seed 7; a debug build in which `netsmith-pcb` is optimised; one run
at a time, on an otherwise idle machine; `NETSMITH_TIMINGS=1`):

| Step | Result |
|---|---|
| Placement | 52 parts on 50 x 50 mm in 3.5 s |
| Routing | all 125 connections routed, on try 1 of 4, in 5.2 s |
| Design rules check | passed (0 violations; 4 silkscreen warnings: the texts of H1, H2 and J1 touch or nearly touch the board edge, and the polygon of one part overlaps H4's text) |
| Requirements | 40 of 40 satisfied |
| `fab` in all | 8.9 s (9.9 s in the two later runs) |

What stops a run that does not finish: the router's own 300 s limit (`--route-timeout`), or its
four tries; it then writes the board marked INCOMPLETE and the run fails (`INCOMPLETE:`, with the
unrouted connections in `routing-failure.txt`).

**How seed 7 was found.** The first run (seed 0, no layout file; the ground pour was there)
placed in 4 s and the router stopped after four tries, in 24 s, with **1 of 125 connections
unrouted**: the USB data pair, `J1.3` to `U4.4` (D+). The other levers: the seeds 1 to 6 and 8
to 11 each leave one connection of the same pair (`J1.2` to `U4.5` or `J1.3` to `U4.4`) after 17
to 38 s; **seed 7 routes everything, on the first try**. No design rule was weakened (the process
is JLCPCB's TwoLayer, unchanged), no keepout was needed, and the physical file's parts are the
same under every seed. The must-match items passed on every one of those boards.

**Seed 1, since #457.** The placer began to measure a part that the requirements pin where it
is, and not at the nearest point of its 0.05 mm grid, which changed every seed's arrangement:
seed 7 no longer finished, and seed 1 routes everything (`fab --locked` in 57 s, a debug build on
a machine running other builds; seeds 5 and 8 route too). The table above is seed 7's run.

The parity checker on that board, `must-match: 129 ok, 0 failed` (the lines are in the pull
request that added this):

| Must match | Result |
|---|---|
| Outline, 50 x 50 mm, radius 5 mm, 4 lines and 4 arcs | ok (0.000 off) |
| H1 to H4, place and drill (2.5, 2.5, 2.2 and 2.5 mm) | ok (0.000 off) |
| USB1, mini USB: place, rotation 180, layer, 9 pads and 2 locating holes | ok |
| BAT1, JST PH: place, rotation 0, layer, 4 pads | ok |
| J1 and J2, 1 x 8 headers: place, rotation 0, layer, 8 pads each | ok |
| SW1 and SW2, buttons: place, rotation 0 and 180, layer, 4 pads each | ok |
| BOOT0 and PWR1, slide switches: place, rotation 180 and 0, layer, 5 pads each | ok |
| The pin-to-signal map of J1 and J2 | ok: each header pad is on the pad of the MCU that theirs is (the nets are named differently, so the connectivity vouches: 23 warnings say so, for these and the other pads on made-up names) |

**What is not matched** (reported, never a failure; 72 items outside their tolerance): the
places and rotations of the ICs (U1 charger 23.5 mm left and 9.65 mm up from theirs, U15 16.25
left and 12.55 up, U16 the MCU 0.6 right and 10.85 down, U17 3.95 left, U22 5.65 right and 8.86
down), of the crystal (U10, 14.4 right and 11.15 up), the inductor (L1, 18.5 left and 13.6 up),
the LEDs and the 30 resistors and capacitors; the pads of the parts whose footprint is not theirs
(L1 0.06 mm, U15 and U22: the datasheets' land patterns against the author's, README "Footprints";
for U22 nine pads of the two have no partner, which was not looked into: the part is not required); the ring of vias of
the four holes; and the routing: the copper is netsmith's own, and nothing is claimed about it.

## For the owner

- **A plated hole in the physical file.** The `case` section takes `MountingHole` (non-plated, by
  its standard rows) and not `PlatedHole`, whose declaration is refused. Used here: a project row
  that maps the package `M2.5-plated` of `MountingHole` to a footprint with a plated pad; it
  works, and nothing in the language changed. What a language change could be: (1) let `case`
  take a `PlatedHole` too (`.hole1: PlatedHole { .package &= "M2.5" }`; its pin `p1` then has to
  be allowed with no net in a physical file); cost: a part with a pin appears in a file that says
  it states no circuit; (2) a `MountingHole` flag, `.plated &= 1` or a kind `PlatedMountingHole`
  with no pin: the language has no boolean, so a kind (as ADR 0020 chose for the std), cost: a
  second std kind that differs by a footprint; (3) leave it as it is. Recommendation: (3) until a
  second board needs it; the project footprint says what it is.
- **The ring of vias** of their holes (eight 0.8 mm vias on a circle of 1.875 mm radius) is not drawn:
  footprints of the project could carry it (and the placer would then keep copper off it), and
  the owner's list does not ask for it.
- **The checker said `fail` on BOOT0's net because of R2** (above): the checker compares pads, not
  nets of interchangeable pads. Option: leave as is (the design now follows ORBIT's numbering of
  R2), or teach the checker that the two pads of a two-pin passive are interchangeable (the same
  pads on the net up to a swap), which is a rule about what "equal" means and so the owner's.
  R9 and R10 keep the std module's numbering and are not must-match.
