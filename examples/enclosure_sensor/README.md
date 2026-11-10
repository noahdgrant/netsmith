# enclosure_sensor

An invented sensor board that goes in a small enclosure (see `enclosure_sensor.nsm` and
`enclosure_sensor.physical.nsm`). It is not a design to build. It builds and fabs with no KiCad
install: the symbols are drawn by netsmith (ADR 0097) and every footprint is a generator's or
a file of `libs/`, listed in `kicad.toml`.

## Footprints of `libs/`

A footprint is written only from what was read, and a figure that was not read is marked
`assumed` here. Numbers are mm; x is to the right and y down, the part seen from the component
side.

### `johanson.pretty/Johanson_2450AT43F0100`, for the chip antenna `AE1`

Source: Johanson Technology, datasheet "2.45 GHz RF Chip Antenna", P/N 2450AT43F0100001E
(legacy P/N 2450AT43F0100), Doc# 36S0021A, Revision 3.0 (the PDF's metadata says it was made
2024-07-09), <https://johansontechnology.com/docs/3918/Antenna-2450AT43F0100001E-Rev3.0.pdf>
(SHA-256 `7f90ff990f56`), read on 2026-11-10. The page footer says "2024 Johanson Technology,
Inc. All Rights Reserved"; only dimensions were taken, no drawing was copied.

**Read from the datasheet:**

- Page 2, "Mechanical Dimensions": the body is 6.00 +/-0.20 long and 2.00 +/-0.20 wide
  (1.20 high), with a terminal `a` = 0.50 +/-0.30 at each end of the bottom face. Pin 1 is the
  feed, pin 2 "NC", "This terminal must be soldered for anchoring and mechanical stability"
  (the top view puts 1 on the left, 2 on the right).
- Page 3, "Evaluation Board and Recommended Mounting Configuration" (all units mm), the
  enlarged view of the antenna's land: the two end pads are 7 apart over their outer edges
  (label 7) and 4.8 apart between their inner edges (label 4.8, its arrows on the pads' inner
  edges), and 2.2 wide (label 2.2). The datasheet has no separate land pattern drawing; this
  is the evaluation board's copper under the antenna, which it calls the recommended mounting.

**Derived by arithmetic:** each pad is (7 - 4.8) / 2 = 1.10 long, so its centre is 2.40 +
0.55 = 2.95 from the middle (x = -2.95 for pad 1, +2.95 for pad 2), on the body's centre line
(y = 0).

**Read from the picture, not from a label:** that the two pads are of one size and symmetric
about the body (the drawing is not to scale: measured, its 4.8 is not 4.8/7 of its 7, so no
dimension was taken from a length on the page).

**Assumed** (the datasheet states none of them):

- Pad shape: rectangles with square corners, as drawn. Mask and paste openings the size of the
  pads, with the board process's expansion. Pad 1 is the feed and pad 2 the other terminal,
  as the table names them; the part in `enclosure_sensor.nsm` calls pad 2 `TRACE`.
- The body outline on `F.Fab` is 6.00 x 2.00 (nominal L and W); the courtyard is the pads (7.00
  by 2.20) and 0.25 all round. No silkscreen, no 3D model.
- Not in the footprint: the ground clearance. The datasheet mounts the antenna at the end of
  the board with a clear area (10 mm x 20 mm on the evaluation board, p.3) and a matching
  network of the board's own; that is the board's layout, so it is not drawn here and this
  file does not enforce it. A board that is built must state it (`enclosure_sensor.physical.nsm`
  does not).
- Its pick-and-place rotation is unverified (the antenna is hand-soldered on this board).

**Against KiCad's `RF_Antenna:Johanson_2450AT43F0100`:** not compared. That footprint was
replaced because it needs KiCad's library, and no KiCad was at hand to open it. Compare the two
once where KiCad is installed, and the datasheet against both, before a board is ordered.

### `plug.pretty/PinHeader_1x16_P2.54mm_Horizontal`, for the plug `J2`

The part `SensorPlug16` is invented, so there is no maker's drawing. The pads are those of
netsmith's generated vertical 1 x 16 header (`PinHeader_1x16_P2.54mm_Vertical`: 1.70 mm lands
round 1.00 mm holes, pin 1 square, 2.54 mm pitch), which a right-angle header shares: the pins
leave the board in a column at x = 0. **Assumed, this example's own:** the plastic body is
2.54 thick and begins 1.50 from the pin column (x = 1.50 to 4.04, y = -1.27 to 39.37), the pin
tails run on to x = 10.50 on `F.Fab`, and the courtyard is x = -1.80 to 11.00 and y = -1.80
to 39.90. The bend, tail length and body offset are not from any maker, and KiCad's footprint
of the same name, which this one replaces, was not compared.

### `flat.pretty/Cell`, for the flat cell `BT1`

An invented cell, 30 x 26, with a 2 x 2 pad at each end of its long side (y = -8 and 8). No
maker: the numbers are the example's own.

## Symbols

No row of `kicad.toml` names a KiCad symbol but the flat cell's `Connector_Generic:Conn_01x02`,
which netsmith draws itself (ADR 0097, step 2). The plug (16 pins), the memory and the antenna
name none, so each is drawn from its pin table as a box.

## Run it

From this folder, `netsmith build` and `netsmith fab`.
