# ORBIT

A port of **ORBIT**, an open STM32F030 board with USB, battery charging and a buck-boost
regulator, to netsmith, as the second showcase after the RP2350A Minimal (epic #308). **Only the
scaffold is done:** the licence, the reference files and the reference table, read from their
board file. The parts, the schematic and the physical file are later stages, and
`src/main.nsm` is a placeholder (a small valid board that checks and solves, and nothing of
ORBIT's circuit).

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

- `netsmith.toml`, `src/main.nsm`: the project and its (placeholder) board, `Orbit`.
- `src/main.lock.toml`: the pick lock of the placeholder board (the catalog row each part was
  picked as), written by `netsmith lock --update`; CI checks it with
  `scripts/check-example-locks.sh`. Later stages must run `netsmith lock --update` when the
  board's parts change, and commit the lock.
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
  stock; and the fixture build with no KiCad. **Not run:** KiCad's ERC or DRC (no
  schematic of the board exists yet), and no part was compared with a physical sample.
