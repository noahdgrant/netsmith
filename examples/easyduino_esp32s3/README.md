# Easyduino ESP32-S3

A port of the open **Easyduino ESP32-S3** board to netsmith, the second showcase board of epic
#308 (after the RP2350A Minimal). **The schematic is done**: `src/main.nsm` is the board
`EasyduinoEsp32s3`, the same circuit as Easyduino's, with its nets proved equal by a test
(below). **The physical file and the parity gate are done too**: `netsmith fab` writes a
four-layer board with Easyduino's outline, receptacles, buttons and headers where theirs are
(see "The PCB" below).

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
   a test proves it by comparing the wiring alone, as the RP2350A Minimal's does (see "Same
   nets" below).
2. **A matching PCB where it matters.** A board made by `netsmith fab` is interchangeable with
   theirs in these ways, and the parity checker with tolerances will fail when one is off:
   - the board outline (25.86 x 63.90 mm; the file draws it in U2, see NOTE-FOR-LATER);
   - the two USB-C receptacles (J1, J2), the buttons (SW1, SW2) and the headers (U2): the same
     positions, rotations and pads, with each header pin carrying the same signal.

   The other parts (module, bridge, regulator, transistors, LEDs, passives) may be anywhere,
   and the routing will differ; the checker reports them and does not fail on them. The board
   has no mounting holes: the only non-plated holes are the two 0.65 mm pegs of each receptacle
   (listed as holes of J1 and J2 in the table, and must-match with them).

## What is here

- `netsmith.toml`, `src/main.nsm`, `src/main.lock.toml`: the project, its board and the board's
  pick lock (`netsmith lock --update` refreshes it; CI checks it). `src/usb_c_input.nsm` (a
  receptacle and the two 5.1 kohm resistors on its CC contacts, used for J1 and for J2) and
  `src/auto_reset.nsm` (the two transistors and two resistors of the automatic bootloader) are
  the board's modules; the other files of `src/` are the parts (below). `catalogs/capacitor.csv`
  is the row of the 22 uF tantalum capacitor.
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
netsmith fab      # the package for the board house, in fab/ (about 5 s)
```

`build` and `fab` need no KiCad install (`NETSMITH_NO_KICAD=1 netsmith fab` is the same). `fab`
with no pin request needs no `--locked`, but the gate runs it so (`netsmith fab --locked`: the
pick lock must be current, and nothing is written beside the sources).

`netsmith compare` reads their board file as one side, but the bare command is not the proof:
`netsmith compare reference/Easyduino_ESP32S3.kicad_pcb src/main.nsm --loose-values` says that
both have 35 components and 57 nets and then `different: 31 differences`, which are names and
numbering: 11 parts correspond but carry another value or footprint name (the tantalum
capacitors, the diode, the receptacle, the buttons, the transistors, the bridge's variant), and
because the designators come out in another order and several parts are interchangeable (the
CC resistors of the two receptacles, the three 100 nF on EN and GPIO0), the bare run pairs
some of them across and lists nets that are the same as different. The test below pairs the
parts by a table and compares the wiring alone.

## The PCB

`src/main.physical.nsm` is the physical file: the outline and the place, rotation and face of
the five parts that must match, each number read from `reference/placements.toml` with a
comment of the line it comes from; `src/main.pcb.toml` is the layout file (the placer's seed,
and one recorded part, below). `netsmith fab` writes the board as a four-layer JLCPCB board
(`.top.net`, `.bottom.net` and `.inner1.net` ground, `.inner2.net` +3.3 V, from `main.nsm`).

- **The outline** is a 25.86 x 63.8972 mm rectangle with square corners (`.outline: Outline`
  in the `case` section), measured from the board's upper-left corner. Their file draws it in
  four lines inside the footprint U2 (the headers); ours is drawn by the physical file and
  the footprint of U2 draws no `Edge.Cuts` (a test holds that).
- **J1, J2, SW1, SW2, U2** are placed by `.usb_uart.connector`, `.usb_native.connector`,
  `.reset`, `.boot` and `.headers`: x, y, rotation and face. A requirement is measured from a
  part's **body** (what its footprint draws on `F.Fab`, else the courtyard) and the checker
  compares the footprint's origin, so for J1 and J2 (courtyard 1.25 mm above the origin) and U2
  (courtyard centred on the pads, 11.43 and 26.67 mm from the origin) the comment gives the
  conversion. J1, J2 and U2 are not turned, SW2 is turned 180 degrees, all are on the top
  face.
- **The holes** are the receptacles' two 0.65 mm pegs each, which the receptacle's footprint
  draws; there is no line for them, and the checker finds each at the table's place because
  the receptacle is.
- **U2's courtyard is drawn on `B.CrtYd` only** (`libs/easyduino_must_match.pretty`). A
  footprint has one courtyard box, and a through-hole part's box also takes in every hole,
  so the 44 holes of two rows 22.86 mm apart would make the whole board U2's courtyard on the
  top face and no other part could stand between the rows, where Easyduino's module,
  buttons, bridge and passives are. A footprint that draws its courtyard on one face only
  is judged on the other face by its pad boxes (ADR 0031), so drawn on the back it leaves
  the top face free up to the header pads. The price: nothing can be on the back face, which
  is what the original does (all 35 parts are on top). The box is the pads and 0.1 mm (the
  process keeps courtyards 0.4 mm from the edge and the pads are 0.5 mm from it).
- **The module is recorded and locked in the layout file** (`at = [12.96, 18.5]`, top face, not
  turned): with the buttons where Easyduino's are, the placer does not find a place for the
  module (every seed put it on the back face or over the header pads). Easyduino's own module
  is 7 mm higher with its antenna 5.9 mm past the top edge of the board; this one is on the
  board, so no overhang is stated. The module need not match, and the gate does not look at it.

### Run the parity check

```sh
netsmith fab --locked          # from this folder; writes fab/EasyduinoEsp32s3/
cargo run -p netsmith --example parity -- fab/EasyduinoEsp32s3/EasyduinoEsp32s3.kicad_pcb \
    --table reference/placements.toml --map reference/designators.toml \
    --svg reference/parity-overlay.svg
```

The checker (`netsmith::parity`, from the RP2350A Minimal's) reads both files from the upper-left
corner of the outline and exits 1 when a must-match item fails. `reference/designators.toml` pairs
all 35 of Easyduino's parts with the ones `fab` writes (the headers are J3 on our board, the bridge
U2, the regulator U3: references follow declaration order); a test holds it against the design's
own designators, and prints the map when it is stale. The gate is `crates/netsmith/tests/easyduino_parity.rs`
(`NETSMITH_NO_KICAD=1`, `fab --locked --route-timeout 20`, every must-match item `ok`, and the
committed overlay equal to the one the board gives). `reference/parity-overlay.svg` is the
checker's picture of the run: Easyduino's outline, holes, parts and pads in blue and the
board's in orange (the checker's legend says "Raspberry Pi's board" for the blue: it is its
fixed wording, the blue is Easyduino's here). It is regenerated by the last command above.

**Result: `must-match: 202 ok, 0 failed`.** Per item: the outline 3 (size, corner radius, the
four lines), the four holes (J1 and J2's pegs, place and drill, 0.00 mm off), J1 45 and J2 45
(position, rotation, layer, each of 22 pads' geometry and net), SW1 7 and SW2 7 (position,
rotation, layer, two pads' geometry and nets), U2 91 (position, rotation, layer and every one
of the 44 pads' centre, size, shape, drill and net: each header pin carries its original's
signal). Every difference is 0.000 mm, but pads 20 to 22 of U2, which are 0.004 mm off the grid in
their file (the checker's tolerance is 0.05). 51 warnings: a pad's net is named differently on the
two boards (the generated names) and the pads on it are the same.

**Not matched, reported only:** the module (8.65 mm lower than theirs, as it is on the board),
the bridge, the regulator, the transistors, the LEDs, the diodes, the resistors and capacitors
(positions and turns), the pads of D2, D3, Q1 and Q2 (our footprints are the makers' land
patterns or the generators', not Easyduino's: 0.16 and 0.26 mm), and the routing. Two parts of
ours differ from theirs by design (README, "Differences from the reference"): R6 and R11
have their pads exchanged, and U3's chip is the CP2102N.

**`fab`:** placement 3.5 s, routing 137 of 137 connections in 1.4 s (try 1), the design rules
check passed (0 violations, 0 warnings), 20 of 20 requirements met, `fab.total` 4.8 s (a debug
build in which `netsmith-pcb` is optimised, `NETSMITH_TIMINGS=1`). `scripts/check-no-kicad.sh`
runs it.

## Same nets

`crates/netsmith/tests/easyduino.rs`, module `board` (no KiCad needed; the ERC test skips where
there is no `kicad-cli`). The reference's connectivity is read from the committed
`reference/Easyduino_ESP32S3.kicad_pcb` as a file with `Netlist::from_kicad_pcb` (netsmith's own
s-expression reader, which `compare` uses), and the board's from the built design.

- `every_pad_has_the_same_pads_on_its_net_as_in_easyduinos_board`: the 35 parts are paired by a
  table of member path to Easyduino's designator (`PARTS`); for every pad of every part the set
  of the other pads on its net is the same on both sides. **Pads that share a number are one
  pad**, as the reader joins them (and the design's netlist has a pin once however many pads it
  has): the nine pads 41 of the module, the four shield pads `S1` of each receptacle and the
  regulator's output and tab (two pads 2). Passes, with 191 pads on 57 nets on each side.
  The wrong-net control `the_comparison_sees_a_pad_on_the_wrong_net` puts U2 pad 4 on the net of
  pad 5 and requires the comparison to name `U2.4`, and to see a pad that is on a net on one
  side only.
- `netsmith_compare_says_the_nets_are_the_same`: `netsmith-netlist::compare` with every value and
  footprint blanked and the designators paired by the table: `match`.
- `the_resistors_and_capacitors_have_easyduinos_values_and_packages`: all 11 resistors and 10
  capacitors have the value and the package of the reference's footprint (`1005` is 0402, the
  tantalum `CP_EIA-3216` is `CASE-A-3216-18`).
- `the_parts_have_the_lcsc_numbers_of_easyduinos_bill_of_materials`: every part is picked as
  the BOM's LCSC number, but U3 (below) and U2 (hand-soldered, `-` in the BOM).
- `the_open_pins_are_the_ones_the_reference_leaves_open`: the 20 pads on nets named
  `unconnected-(...)` (16 pins of the bridge, SBU1 and SBU2 of each receptacle) are the pads that
  no net of the design names, and no other pad is open. (The reader drops a net of one pad, so
  the proofs above say nothing about them; this one does.)
- The pin map: `the_header_pins_carry_easyduinos_signals` (all 44 pads of U2),
  `the_receptacles_carry_easyduinos_signals` (every contact of J1 and J2: VBUS behind its diode,
  ground and shield, each CC on its own 5.1 kohm resistor, D+ and D-, the sideband contacts open)
  and `the_buttons_have_easyduinos_nets_on_their_pads` (SW1 pad 2 on EN, SW2 pad 2 on GPIO0, pad
  1 on ground). Every header pad is joined to a **named pin of the module** in the source
  (`.headers.gpio4 ~ .module.io4`), never to a request, and the module's table has no function
  rows at all, so the pin optimizer (ADR 0101) has nothing it can move:
  `the_optimizer_cannot_move_a_header_pad` checks that the check's spaces for the board are empty
  and that there is no pin file.
- ERC: `the_electrical_rules_check_of_kicad_passes_on_the_schematic` (clean where `kicad-cli`
  exists, skipped where not).

### Differences from the reference (all of them)

- **R6 and R11 have their pads 1 and 2 exchanged.** Easyduino's R6 (the power LED's resistor)
  has pad 1 on the LED and pad 2 on VBUS, and R11 (the bridge's reset pull-up) pad 1 on the reset
  line and pad 2 on VBUS. netsmith's `~>` chain and the std `PullUp` enter a resistor at its pad
  1, so ours have pad 1 on VBUS. A resistor has no polarity. The test reads the reference's two
  resistors with the pads exchanged (`SWAPPED`), and `only_r6_and_r11_have_their_pads_exchanged`
  shows that without the exchange the two boards differ at exactly the pads on those two nets.
- **U3 is not the same chip number.** The reference's BOM and board say C6568, which is the older
  CP2102 (see U3 below); the port is the CP2102N that its schematic names (C1550553). Same pins.
- **The designators differ** (declaration order: the module's parts are declared in the
  modules and the headers come after the chips, so for example the board's U2 is the bridge,
  not the headers). The tests pair parts by member path. The parity checker's designator map is
  the physical-file item's.
- **Which of C4 and C8 is which** (both 100 nF from EN to ground) is arbitrary.
- **Tolerances.** The reference states values only. The tolerance of each part is the one of the
  LCSC part its BOM names (1 % resistors, 10 % capacitors), which is what the catalog row says.
- **The tantalum capacitor's footprint** is a file of the project's (`CP_EIA-3216-18_Tantalum_A`),
  pads as the reference's `CP_EIA-3216-10_Kemet-I` has them (1.415 x 1.39 mm at +-1.3525 mm);
  the part is Kyocera AVX's TAJA226K010RNJ (the number's own pages), whose land pattern was not
  read.
- **Pours.** The board states the GND pour on both outer layers and the first inner layer and
  the +3.3V pour on the second inner layer; the reference has a +3.3V pour on its top layer too
  (a layer here has one pour net).
- **What the reference has that this does not:** its notes say a polyfuse was added on the USB-C
  input; there is none among the 35 footprints, and none here.

## What the solver checks

Everything here is from the reference's own words or from a datasheet figure that a part file
or a catalog row records with its page; nothing is a number the port made up.

- **The 3.3 V rail against the module's supply window.** The AMS1117-3.3 row states the
  output 3.201 V to 3.399 V (datasheet page 2, over the operating range); the module's `.vdd`
  states 3.0 V to 3.6 V (ESP32-S3-WROOM-1 datasheet Table 6-2). The solver checks the first
  inside the second, and refuses a regulator that gives more
  (`a_regulator_that_gives_more_than_the_modules_window_is_refused`).
- **The regulator's current and the module's required supply current.** The reference's notes
  say "Output Current at 3.3V: 1 A (External Source) 500mA (USB)". The board states the
  regulator's output current as the USB figure, `-500mA..0A`, because nothing on the 3.3 V net
  states a draw (the module has no stated current: the datasheet's figures are typical
  values for radio states, which were not read). It is checked against the row's `max_current`,
  0.8 A: the module's own requirement of 0.5 A from its supply (Table 6-2, "current delivered
  by external power supply") is met by it. The reference's 1 A for an external source is **above
  the row's 0.8 A**: the datasheet's headline is 1 A and its current limit 900 mA minimum, but
  0.8 A is the highest current at which the dropout is stated, so a board that states 1 A is
  refused. No board of this project states it.
- **NOT evaluated: the dropout from USB.** The reference feeds the regulator from VBUS through a
  Schottky diode (D2 or D3). With the USB window 4.75 V to 5.25 V (the figure the RP2350 board
  uses; the reference says "5V (USB UART OR USB ESP32)") and the diode's 0.6 V (ROHM's maximum
  VF at 1 A, the row's figure), the input is at least 4.15 V, and the regulator needs its
  output's top, 3.399 V, plus the dropout, 1.3 V at most, which is 4.699 V. **By the catalog's
  figures the dropout does not hold in the worst case**, and nothing says it does: the 1.3 V is
  the datasheet's maximum at 0.8 A (page 3, note 4) and the board draws at most 0.5 A, the
  diode's 0.6 V is its figure at 1 A (it is less at 0.5 A), and the datasheet gives no
  dropout at a lighter load. The solver does not make the check: the bridge's VBUS and VREGIN
  pins and its reset line (a pull-up to VBUS on a pin that states no drive) are not modelled and
  the datasheet states no maximum supply current for the bridge (Table 3.2 has typical
  values only), so the rail behind the diodes has no voltage and the relation is listed as not
  evaluated (`the_power_path_is_checked_as_far_as_the_parts_state_and_the_rest_is_listed` pins
  that). The arithmetic above is the README's and not a finding of the tool.
- **Not checked, listed as not evaluated:** every diode's current and bias, the CC
  resistors' and pull-ups' power (nothing states the currents of the lines they hold), the
  power LED's current, the buttons' current, the strapping pins' levels at boot (see "For the
  owner" in the pull request).

## NOTE-FOR-LATER

Facts for the next stages (parts, the board and its proof, the physical file); read from the
files in `reference/`, nothing from a datasheet.

**What the reader had to learn for this file.** The board's outline is not drawn on the board:
it is four `fp_line`s on `Edge.Cuts` inside the footprint U2 (`Espressif:ESP32-S3-DevKitC`, at
128.605, 59.75136, no rotation), in U2's own frame. The table's reader looked at the board's
own graphics only and found no outline. It now places a footprint's `Edge.Cuts` shapes on the
board. The rectangle spans x 127.105 to 152.965 and y 58.6514 to 122.5486 (25.86 x 63.8972
mm), no rounded corners. The board has no `gr_*` item on `Edge.Cuts`. A copy made by netsmith
draws its outline itself in the physical file (done: see "The PCB"); U2 is placed like J1, J2, SW1 and SW2.

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
  the same land pattern). `provides Connector`. Nine pins: `.vbus.pos` (A4, A9, B4, B9) and `.vbus.neg` (A1,
  A12, B1, B12), the two ends of one rail `.vbus` (it has `dc source .vbus`, as std's `UsbCPower`
  has, so the DC analysis starts from the host: the board states 4.75 V to 5.25 V; this was
  added with the board, the pads and datasheet names are unchanged), `.cc1` (A5), `.cc2` (B5), `.dp` (A6, B6), `.dm` (A7, B7), `.sbu1` (A8), `.sbu2`
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
    Courtyard: the pads and 0.1 mm, drawn on `B.CrtYd` only (see "The PCB": it leaves the board's top face free between the rows). **The footprint does not draw the board's outline**: the
    reference's U2 draws the four `Edge.Cuts` lines of the 25.86 x 63.90 mm board inside the
    footprint; here they are not copied, and a test checks that no footprint of the library has
    `Edge.Cuts`. The physical file draws the outline itself, with a rectangle from x 127.105 to
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
  PCB antenna, 41 pins: 40 edge pads and the ground pad, `EPAD`, which is pad 41. The supply is a
  rail, `.vdd` (`.vdd.pos` the `3V3` pad, `.vdd.neg` the ground pads), with the datasheet's
  window of Table 6-2 (printed page 27): VDD33 3.0 V minimum, 3.3 V typical, 3.6 V maximum
  (added with the board, so that the regulator's output is checked against it); the module's
  own supply current is not stated. Source:
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
- **U3, `Cp2102n`** (`src/cp2102n.nsm`): (DCD, CTS and DSR are `optional`, added with the board:
  Easyduino leaves them open and the datasheet says the handshake and modem-control pins act only
  when the host enables them, pages 21 and 23; it does not say in words that they may float.) Silicon Labs' (now Skyworks') CP2102N in the QFN28
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
- **C6, C7 and C10, the 22 uF tantalum capacitors** (std `Capacitor`, package `CASE-A-3216-18`,
  the project's row in `catalogs/capacitor.csv`, footprint `libs/easyduino_ics.pretty/CP_EIA-3216-18_Tantalum_A.kicad_mod`):
  the part of the BOM's LCSC **C11366**, Kyocera AVX **TAJA226K010RNJ**, 22 uF +/-10 %, 10 V,
  case A (EIA 3216-18), library type Extended. Source: JLCPCB's page `jlcpcb.com/partdetail/C11366`
  and LCSC's `lcsc.com/product-detail/C11366.html`, read on 2026-11-10 (they agree on the
  number, value, tolerance, voltage and package; JLCPCB lists no manufacturer and an ESR of 3
  ohm at 100 kHz, LCSC names Kyocera AVX and says one document is available, which has no link on
  the page). No datasheet was read, so the row has **no DC-bias curve, no dielectric class, no
  height and no temperature range** (the -55 to +125 C that JLCPCB prints is not a document
  read). The footprint is the reference's pads (`CP_EIA-3216-10_Kemet-I` of KiCad's library:
  1.415 x 1.39 mm roundrects at +-1.3525 mm, pad 1 the positive terminal), written as a file of
  the project with the body outline of the case (3.2 x 1.6 mm) on `F.Fab`: the part's own land
  pattern was not read. The reference board's three are on rails (C6 on VBUS, C7 and C10 on
  +3.3V) with pad 1 on the rail, as ours are.
- **What the reference says about them** (checked by the tests against
  `reference/placements.toml`): every pad of U1 carries the net that names its pin (`/GPIOn` is
  `IOn`, `/GPIO44` and `/GPIO43` the pads `RXD0` and `TXD0`, `/USB2_D-` and `/USB2_D+` pads 13
  and 14, `/EN`, `+3.3V`, `GND`); U3's pads on named nets are `GND` (pad 3 and the centre pad),
  `D+`, `D-`, `VREGIN` and `VBUS` on `VBUS`, `VDD` on its own net, `RSTb`, `SUSPENDb`, `RTS`,
  `DTR`, and `RXD` and `TXD` on the module's `/GPIO43` and `/GPIO44` (cross-wired, as the
  UART is); its 16 open pads are on nets named after the same pins.
- **The dropout** the board stage was to name: see "What the solver checks" (it is not
  evaluated, and the catalog's figures say it does not hold in the worst case).
