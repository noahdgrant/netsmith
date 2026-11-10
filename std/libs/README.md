# netsmith's own footprints and symbols

Files in KiCad's formats, compiled into the binary (`crates/netsmith-kicad/src/stdlib.rs`) and answered under the library name `netsmith` (`netsmith:Name`, ADR 0087). A package goes here when KiCad has no footprint for it and no generator of ADR 0043 can make it, and, since ADR 0097 (step 3), when KiCad has one that is one maker's drawing and netsmith writes its own from that drawing instead of needing KiCad's file. Every footprint of `netsmith.pretty/` is named by a row of `std/kicad.toml`, and the tests check its pads against its part's pin table.

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

**The symbol** is not a file here: the part's row in `std/kicad.toml` names no `symbol`, so netsmith draws it from the part's pin table, a rectangle with the table's pins (`netsmith:Lt8641_UDC20_18_out8`, ADR 0097: the name ends in the pad of each pin that gives power, here the switch node, which is a power output). `netsmith.kicad_sym` holds no symbol of its own; a board's output has the symbols drawn for it.

**The name** follows KiCad's style (package, leads, exposed pads, body, pitch, variation) and names the package, not the part, so another UDC20(18) part can map to it.

## The maker-drawn footprints of ADR 0097, step 3

Five footprints that `std/kicad.toml` used to take from KiCad's libraries are written here, each from the maker's drawing, with no copy of KiCad's file (ADR 0097 rejects bundling it). Numbers are mm, read on 2026-11-09. Each drawing's file is the one named below; its SHA-256 is given to the first 12 digits. x is to the right and y down, with the part seen from the component side. **The origin of the four receptacles is the middle of their four shell holes** (the drawings state none; KiCad's footprints use another point, and the proof against KiCad centres both on this one). Each footprint's pads are in the unit test of `crates/netsmith-pcb/src/footprint.rs` and are compared with KiCad's in `crates/netsmith-pcb/tests/maker_vs_kicad.rs`, which prints every difference and bounds it.

**What the maker's terms say.** Each record says what the file carried; where it carried a notice, the notice is quoted. A notice that a drawing must not be copied or disclosed is about the document: nothing here copies a drawing, and the files are written from the dimensions. That reading is the owner's to confirm, and each footprint has its own file, record, row and test, so that one can be dropped.

### `USB_C_Receptacle_GCT_USB4125_6P_TopMnt_Horizontal`, for the power-only receptacle

Source: Global Connector Technology, drawing USB4125, "Type C Receptacle, Horizontal, SMT, For Power Charging Only", Revision A6 (drawing date 28 September 2020, released 20/11/25), https://gct.co/files/drawings/usb4125.pdf (SHA-256 `ea33e0b40545`), sheet 1 of 2: the view "Recommended PCB Layout" (tolerance +/-0.05 mm). The title block carries "This drawing is confidential and copyright of Global Connector Technology, Ltd (GCT). This drawing must not be copied or disclosed without written consent."; the file is a public download of the maker's own site.

**Read from the drawing:**

- Signal pads, 1.20 long (label 1.20), in the order B12, B9, A5, B5, A9, A12 from the left: 0.80, 0.76, 0.70, 0.70, 0.76, 0.80 wide (labels 2x0.80, 2x0.76, 2x0.70). Their centres: 5.50 between B12 and A12 (so +/-2.75), 3.04 between B9 and A9 (+/-1.52), 1.00 between A5 and B5 (+/-0.50).
- Four shell holes, 8.64 apart across (label 8.64) and 3.80 apart along (label 3.80): a plated slot of 1.10 x 1.70 around a 0.60 x 1.20 drill (labels 4x1.10, 4x1.70, 4x0.60, 4x1.20).
- The signal pads' upper edge is 0.68 from the upper shell holes' centre (label 0.68, on the extension lines of the pad edge and the hole centre line).
- The component outline ends 2.60 beyond the lower shell holes (label 2.60), and the front view is 8.94 wide.

**Derived:** the pad centres' y, 0.60 (half of 1.20) below the pad's upper edge, which is 0.08 above the upper hole centre; with the origin at the middle of the holes, the holes are at y = -1.90 and 1.90 and the pads at y = -1.98. The front of the outline, 1.90 + 2.60 = 4.50.

**Assumed:** the shell holes are plated and are pad `S1` (the drawing hatches them as solder area and does not say; its part-number table names the shell GND). The body's back edge is the signal pads' outer edge, 4.47 either side of the centre, its front the outline's. Rectangular signal pads with square corners, as drawn. Paste on the signal pads, none on the shell. Courtyard: the copper and the body and 0.25 all round. No silkscreen, no 3D model. The pick-and-place rotation is unverified.

**Against KiCad's `USB_C_Receptacle_GCT_USB4125-xx-x_6P_TopMnt_Horizontal`:** every pad, hole and size agrees at 0.000 mm once both are centred on the shell holes. Only the courtyard differs (by 0.25).

### `USB_C_Receptacle_GCT_USB4105_16P_TopMnt_Horizontal`, for the USB 2.0 receptacle

Source: GCT drawing USB4105, "USB Type C Receptacle for USB 2.0, SMT Type, PCB Top Mount", Revision B4 (drawing date 4 October 2019, released 18/12/23), https://gct.co/files/drawings/usb4105.pdf (SHA-256 `fb331fbabee8`), sheet 1 of 2: "Recommended PCB Layout, as viewed from component side" (tolerance +/-0.05). The same notice as above.

**Read from the drawing:**

- Twelve pads, 1.15 long (12x1.15). The eight small ones are 0.30 wide (8x0.30) at 0.50 pitch: B8, A5, B7, A6, A7, B6, A8, B5 from the left, at -1.75 to 1.75 (label 3.50 between B8 and B5, 0.50 between neighbours). The four wide ones are 0.60 wide (4x0.60): A1 with B12 at +/-3.20 (label 6.40) and A4 with B9, A9 with B4, A12 with B1 at +/-2.40 (4.80). Where two numbers share a pad (A1 and B12, A4 and B9, B4 and A9, B1 and A12) the drawing has one pad; the footprint has two pads at the same place, one for each number.
- Shell holes at +/-4.32 (8.64): two upper slots of 1.00 x 2.10 with a 0.60 x 1.70 drill (2x2.10, 2x1.70, 4x1.00, 4x0.60), two lower ones of 1.00 x 1.80 with a 0.60 x 1.40 drill (2x1.80, 2x1.40), 4.18 apart (12x4.18, from the pads' lower edge, which is on the line of the upper holes' centre, to the lower holes' centre).
- Two plain holes of 0.65 (2x O0.65) at +/-2.89 (label 5.78), 3.68 above the lower holes' centre (label 2x3.68).
- The PCB edge is 2.60 beyond the lower holes (label 2x2.60), the front view 8.94 wide.

**Read from the picture, not from a label:** that the pads' lower edge is on the line of the upper shell holes' centre (the extension line of the 12x4.18 passes through both).

**Derived:** the pads' y, half of 1.15 above that line, which with the origin at the middle of the holes (+/-2.09) is -2.665; the plain holes at y = 2.09 - 3.68 = -1.59; the PCB edge at 2.09 + 2.60 = 4.69.

**Assumed:** as for the USB4125 (shell holes plated and `S1`, body back edge at the pads' outer edge, 4.47 either side, rectangular pads, paste on the signal pads, courtyard, no silkscreen or model, rotation unverified). The two plain holes are non-plated. The PCB edge is drawn on `Dwgs.User` for information. 

**Against KiCad's `USB_C_Receptacle_GCT_USB4105-xx-A_16P_TopMnt_Horizontal`:** every pad, hole and size agrees at 0.000 mm (centred as above). Only the courtyard differs (0.255).

### `USB_C_Receptacle_HRO_TYPE-C-31-M-12`, for the HRO receptacle

Source: the maker's drawing of the part (HRO Electronics Co., Ltd., part no. TYPE-C-31-M-12, title "Detector Switchs", drawn 2020.12.08, revision A, sheet 1 of 1), the file that LCSC's record of C165948 links (https://datasheet.lcsc.com/datasheet/pdf/9e56b777c022540fcce7c7f67825f55e.pdf, SHA-256 `6ae33d50ac47`), the view "Recommend P.C.B Layout (Component Side)", "Tolerance for PCB layout is +/-0.05". The page carries a watermark of the maker's name and no notice of copyright or confidentiality that was found. The URL that KiCad's footprint cites (krhro.com, in a folder dated 2018-03-20) answered 403 and was not read.

**Read from the drawing:** the same arrangement as the USB4105, with these numbers: eight pads 0.30 wide (8-0.30) at -1.75 to 1.75 (3.50 between B8 and B5, 2.50 between A5 and A8, 1.50 between B7 and B6, 0.50 between A6 and A7); four pads 0.60 wide (4-0.60) at +/-3.20 (6.40) and +/-2.40 (4.80); the pads' upper edge is 1.64 above the centre of the two plain holes and the lower edge 0.50 above it (labels 1.64 and 0.50); shell holes at +/-4.325 (label 8.65 between the upper holes' centres), the upper pair a slot of 0.90 x 2.00 around a 0.60 x 1.70 drill (labels 0.90, 2.00, 0.60, 1.70), the lower pair 0.90 x 1.70 around 0.60 x 1.40 (1.70, 1.40), 4.18 below the pads' lower edge (label 4.18); two plain holes of 0.60 (O0.60) at +/-2.89 (5.78); the PCB edge 5.79 below the plain holes (label 5.79). The front view is 8.94 wide.

**Derived:** the pad length, 1.64 - 0.50 = 1.14, its centre 0.57 above the line of the upper holes (which is the pads' lower edge: the extension line of the 4.18 passes through both); with the origin at the middle of the holes (+/-2.09): pads at y = -2.66, plain holes at y = -1.59 (0.50 below the upper line), the PCB edge at 4.20.

**Assumed:** as for the USB4105 (shell holes plated and `S1`, plain holes non-plated, body back edge at the pads' outer edge, rectangular pads with paste, courtyard, no silkscreen or model, rotation unverified).

**Against KiCad's `USB_C_Receptacle_HRO_TYPE-C-31-M-12`:** they differ, and `maker_vs_kicad.rs` bounds each difference. KiCad's footprint cites an earlier drawing (see above): its signal pads are 1.45 long (ours 1.14), reaching 0.50 farther back and 0.19 farther forward; its corner and inner pads are 0.05 farther out (3.25, 2.45 against 3.20, 2.40); its shell pads are 1.0 wide (ours 0.90), the lower ones 1.6 long with a 1.2 drill (ours 1.70 and 1.40); its plain holes are 0.65. The shell holes are at the same 4.18. Before a board is ordered, compare with the maker's drawing of the revision on the part in hand.

### `SW_SPST_Omron_B3U-1000P`, for the B3U-1000P tactile switch

Source: Omron, "B3U Ultra-small Tactile Switch (SMT)", Cat. No. A162-E1-07 (copyright line "(c) OMRON Corporation 2007-2018"), p.2 "Dimensions", top-actuated, without ground terminal, without boss: the "PCB Pad (Top View)" (SHA-256 `97e36bd56f4e`, a copy of the catalog from static.chipdip.ru; Omron's own site was not reached). "Unless otherwise specified, a tolerance of +/-0.2 mm applies to all dimensions"; the pad's labels carry +/-0.1. No licence terms were seen.

**Read from the drawing:** two pads, 4.2 over the outer edges (4.2 +/-0.1), 2.6 between the inner edges (2.6 +/-0.1) and 1.7 long (1.7 +/-0.1); the terminal arrangement puts terminal 2 on the left and 1 on the right ("No terminal numbers are indicated on the Switches"); the body 3 wide (label 3) and 2.5 deep (label 2.5), the terminals reaching 4 over all (label 4).

**Derived:** each pad is (4.2 - 2.6) / 2 = 0.80 wide and its centre (4.2 + 2.6) / 4 = 1.70 from the middle.

**Assumed:** the origin is the middle of the two pads; pad numbers 2 (left) and 1 (right) follow the drawing's terminal arrangement (the switch has no polarity). Rectangular pads with paste. The body on `F.Fab` is the 3.0 x 2.5 body; the courtyard is the pads and the 4.0 x 2.5 outline of the terminals, and 0.25. No silkscreen, no 3D model. The rotation is unverified.

**Against KiCad's `SW_SPST_B3U-1000P`:** pads are 0.9 wide there (ours 0.8) at the same centres; KiCad numbers the left pad 1.

### `SW_SPST_CK_RS-282G05A3-SM`, for the C&K RS-282G05A3-SM RT tactile switch

Source: the drawing of RS-282G05A-SM RT (Comax Electronics (Huizhou) Co., Ltd., drawing ITEM2325, version A05, 2010-08-11, "Tact switch", 1 of 1), the datasheet file that LCSC's record of C221930 links (https://datasheet.lcsc.com/datasheet/pdf/ec5c6cfb119f91343a2d2e674c254402.pdf, SHA-256 `210388ae5b6c`), the "Piercing Plan" (the land). C&K's own datasheet (the Mouser file that KiCad cites) answered with a script challenge and was not read. The force table's "3" row (250 +/-50 gf) is the A3 of the part number. No notice of copyright or confidentiality was seen.

**Read from the drawing:** two lands, "2-1.5x1.5 LAND", 7.8 between centres (label 7.8); the body 6 wide and 3.6 deep (labels 6 and 3.6), the terminals reaching 8 over all (label 8).

**Assumed:** the origin is the middle of the two lands, pads numbered 1 (left) and 2 (right) (the drawing numbers none; the switch has no polarity); square lands with paste; the body on `F.Fab` 6.0 x 3.6; the courtyard the lands and the 8 x 3.6 outline of the terminals, and 0.25; no silkscreen or model; the rotation unverified.

**Against KiCad's `SW_SPST_CK_RS282G05A3`:** the lands and the courtyard agree at 0.000 mm.

## Left on KiCad's footprints

- `Connector_Coaxial:SMA_Amphenol_132134_Vertical` (SMA jack 132134). Amphenol RF's customer outline drawing, Rev C (10 Nov 2014, sheet 3 of 3, via Octopart's copy; Amphenol's own site refused the script), gives only the holes of the layout (one O1.5 and four O1.7 on a 5.08 square) and no pad size, which ADR 0087 forbids guessing; and it carries the notice that the drawings "are, and remain the property of Amphenol Corp.", "must be returned upon request" and "are confidential and not to be disclosed to anyone other than whom they are given". It is not written here.
- `Connector:Tag-Connect_TC2030-IDC-NL_2x03_P1.27mm_Vertical` (TC2030-IDC-NL). Tag-Connect's drawing (TC2030-IDC-NL-FP Rev B, 12/05/19, https://www.tag-connect.com/wp-content/uploads/bsk-pdf-manager/2019/12/TC2030-IDC-NL-Datasheet-Rev-B.pdf, SHA-256 `46073f8d1d7d`) carries every dimension, but the maker states no terms for the drawing and only an as-is disclaimer for its footprint downloads, so it is left for the owner to decide.
