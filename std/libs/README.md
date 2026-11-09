# netsmith's own footprints and symbols

Files in KiCad's formats, compiled into the binary (`crates/netsmith-kicad/src/stdlib.rs`) and answered under the library name `netsmith` (`netsmith:Name`, ADR 0087). A package goes here when KiCad has no footprint for it and no generator of ADR 0043 can make it. Every footprint of `netsmith.pretty/` is named by a row of `std/kicad.toml`, and the tests check its pads against its part's pin table.

A footprint is added only from a package drawing that was read. Record here the document, the page, what was read from it, what was derived by arithmetic, and what was assumed. A package with a dimension that cannot be read is not added. A wrong pad is a wrong board, and KiCad has no footprint to compare with: the check is the datasheet, so compare the file with the drawing once more before a board is ordered.

## `UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA`, for the LT8641

Source: Analog Devices, LT8641 Rev. C, the LCSC mirror the std part cites (`std/parts/regulator.nsm`, `data/jlcpcb/README.md`: SHA-256 `09763bcb...965a43`). The package description on p.24 (UDC20(18), LTC drawing 05-08-1956 Rev C, Exposed Pad Variation AA: "Recommended solder pad pitch and dimensions", and the bottom view), and the pin configuration on p.2 for the numbering. The drawing says it is not to scale (note 2), so no dimension was taken by measuring the picture: each is a dimension label of the drawing, read at 500 to 1000 dpi to see which extension lines each label belongs to. Numbers are mm. x is to the right and y down in the footprint, the body centre is the origin, and pin 1 is at the top left as in the top view of p.2. The unit test in `crates/netsmith-pcb/src/footprint.rs` holds each number.

**Read from the drawing** (nominal values; the datasheet's tolerance, 0.05 on pads, is not applied):

- Lead pads, 0.70 long and 0.25 wide (labels 0.70 and 0.25 +/- 0.05), at 0.50 pitch (0.50 BSC; 1.50 REF over the four of a short side, 2.50 REF over the six slots of a long side).
- The inner edges of the two long-side columns are 2.10 apart and their outer edges 3.50 (labels 2.10 and 3.50). The inner edges of the two short-side rows are 3.10 apart and their outer edges 4.50. These agree with the 0.70 pad length (1.40 = 2 x 0.70 on both).
- Pins 5 and 12 do not exist ("NOTE: PINS 5 AND 12 ARE REMOVED", p.2). The long sides have slots at -1.25, -0.75, -0.25, 0.25, 0.75 and 1.25 with the slot at 0.75 empty: pins 1 to 4 and 6 on the left, 16 to 13 and 11 on the right. The short sides have four pads at -0.75 to 0.75: 7 to 10 below, 20 to 17 above.
- Two exposed pads, both SW ("EXPOSED PAD (PINS 21, 22) ARE SW", p.2), each 0.220 wide; 0.400 long for one and 0.356 long for the other (the bottom view's labels, which sit on the two pads' own extension lines), their centres 0.770 apart (0.770 BSC) on the vertical centre line.
- The place of the exposed pads along the long axis, read two ways that agree. The bottom view gives 2.127 +/- 0.10 from the body's top edge to the top edge of the 0.356 pad, so its centre is 0.305 below the body centre. The land pattern view gives 0.055 BSC between the centre line of the pair and the centre of the nearest lead pad, whose centre is 0.25 from the body centre, so 0.305 again. The other pad's centre is 0.770 further, 1.075.

**Derived by arithmetic:** the pad centres. A long-side pad's centre is 1.40 from the body centre (1.75 - 0.35), a short-side pad's 1.90 (2.25 - 0.35).

**Read from the pictures, not from a label:** which exposed pad is 21 and which is 22. On p.2 pad 22 is above pad 21 in the top view. The bottom view has the same vertical order (it is mirrored left to right, as pin 1 moves from the left to the right), and in it the shorter, 0.356 pad is the upper one. Both are SW, so a swap would not change a net, but the numbers follow the drawing. The land pattern view of p.24 is drawn turned a quarter; its two short exposed pads agree (0.400 and 0.356, 0.770 apart, towards the end where pins 5 and 12 are missing).

**Assumed** (the datasheet states none of them):

- Pad shape: a rectangle with square corners, as the drawing shows, not the rounded corners of ADR 0043's generated pads.
- Paste: every pad has a paste opening the size of the pad. The datasheet has no stencil recommendation, and the exposed pads are small (0.088 and 0.078 mm2, against the leads' 0.175 mm2), so a reduced opening for them is not argued; a board house's own stencil rules apply. Mask: every pad has a mask opening, and its expansion is the board process's. The drawing says "apply solder mask to the areas that are not soldered", which is the same.
- Courtyard: the pads (3.50 by 4.50) and 0.25 all round, as ADR 0043's generated footprints. The body outline on `F.Fab` is the 3.00 x 4.00 body with pin 1's corner cut; the silkscreen corners keep 0.26 from copper centre line to pad edge, and a triangle points at pin 1, all as the generators draw them. These are drawing conventions, not package dimensions.
- No 3D model (none was read, and none is named: the footprint has no `(model ...)`), and no thermal vias: the datasheet's "Recommended PCB Layout" (Figure 1, p.13) is a layout and not a land pattern, and vias and copper pours are the board's.
- Its pick-and-place rotation is unverified until a board with it has been assembled (`crates/netsmith-fab/src/rotation.rs` leaves it out, and `fab-notes.txt` flags it).

**The symbol** is not a file here: the part's row in `std/kicad.toml` names no `symbol`, so netsmith draws it from the part's pin table, a rectangle with the table's pins (`netsmith:Lt8641_UDC20_18`, ADR 0097). `netsmith.kicad_sym` holds no symbol of its own; a board's output has the symbols drawn for it.

**The name** follows KiCad's style (package, leads, exposed pads, body, pitch, variation) and names the package, not the part, so another UDC20(18) part can map to it.
