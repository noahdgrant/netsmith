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
  page 15) as `.charge_current == 1000V / .rprog`, with `RPROG` between 2 kohm and 67 kohm (DC
  characteristics, "Charge Impedance Range", page 4), and the regulated voltage of the order code,
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
  function chooses it. `CP2102`'s `NC` pins are `no_connect` (the datasheet allows leaving
  them open or tying them to `VDD`; this part keeps them open) and its modem inputs are
  `optional` (note 3, page 11).
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
