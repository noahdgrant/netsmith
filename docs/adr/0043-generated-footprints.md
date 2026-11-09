---
status: provisional
---

# Generated footprints

Carries out the user's decision recorded in ADR 0041: netsmith makes the footprints of the common packages itself, from original generators in the style of IPC-7351, checked against KiCad's own footprints to 0.01 mm. KiCad's `.kicad_mod` stays the file format; nothing of KiCad's library is bundled.

## What is generated

A generator turns the parameters of a package into a `Footprint`, the model that the loader makes from a `.kicad_mod` (`crates/netsmith-pcb/src/generate/`). It writes the text of a `.kicad_mod` and reads it back with the loader, so the placer, the fabrication writers and the `.kicad_pcb` export cannot tell a generated footprint from a read one. Each generator answers to the name KiCad gives the footprint (`Resistor_SMD:R_0402_1005Metric`, `Package_TO_SOT_SMD:SOT-23`), so the rows of `std/kicad.toml` and a project's `kicad.toml` name them as before. The parameters are tables in the generator's source (typed, tested, versioned with the binary), not a data file: nobody extends them at run time, and a project that needs another package writes a library file (ADR 0021).

| Family | Names | How the pads are made |
|---|---|---|
| Chips: R, C, L, ferrite, LED, D, fuse | `R_`, `C_`, `L_`, `LED_`, `D_`, `Fuse_` + 0201 ... 2512 (C: to 1210; fuse: from 0402) | IPC formulas, body and terminal envelopes; 0201 has paste-only openings |
| SOD-123, SOD-323, SMA, SMB, SMC | `Diode_SMD:D_SOD-123` ... | recommended land patterns, recorded pad sizes |
| SOT-23 (-3, -5, -6), SOT-223 | `Package_TO_SOT_SMD:SOT-23` ... | SOT-23: IPC formulas from JEDEC TO-236 and MO-178; SOT-223: recorded pads |
| SC-70 (SOT-323, SOT-343, SOT-353, SOT-363, SC-70-8) | `Package_TO_SOT_SMD:SOT-353_SC-70-5`, `SOT-323_SC-70_Handsoldering` ... (10 names) | recorded pads and courtyards, one row for reflow and one for hand soldering of each |
| SOIC, TSSOP, MSOP, SSOP | `Package_SO:SOIC-8_3.9x4.9mm_P1.27mm` ... (18 names) | IPC formulas from JEDEC MS-012, MS-013, MO-153, MO-187, MO-150 |
| QFN, DFN, WSON, HVSON | `Package_DFN_QFN:QFN-16-1EP_3x3mm_P0.5mm_EP1.7x1.7mm` ... (30 names) | IPC formulas for flat no-lead, exposed pad kept 0.2 mm clear, paste openings covering 65 % |
| LQFP, TQFP, VQFP | `Package_QFP:TQFP-48_7x7mm_P0.5mm` ... (16 names) | IPC formulas for gull wings, fine-pitch fillets from 0.625 mm, from JEDEC MS-026: 2.0 mm over the leads beyond the body, leads 0.45 to 0.75 mm |
| DPAK (TO-252), SOT-89 | `Package_TO_SOT_SMD:TO-252-3_TabPin2`, `SOT-89-3` ... (6 + 4 names) | recorded pads (Infineon PG-TO252, Microchip 3L SOT-89, Ricoh SOT-89-5); the DPAK tab is one pad with four paste openings, the SOT-89 tab a `custom` pad |
| SMD crystals | `Crystal:Crystal_SMD_3225-4Pin_3.2x2.5mm` ... (18 names, 0603 to 7050, 1.2 x 1.0 mm and the hand-soldering pads) | recorded pads and courtyards |
| Pin headers and sockets | `PinHeader_1x04_P2.54mm_Vertical`, `PinSocket_2x05_P2.00mm_Vertical`, 1 or 2 rows, 1 to 40 per row | pad 0.7 of the pitch, drill 1.0 mm (0.8 mm at 2.0 mm), pin 1 square |

`generate::names()` lists the 802 names; a name outside the list is not generated (`Package_SO:SOIC-9_...` is `None`, not a guess). The QFN family is a list of named packages, because the lead and exposed pad dimensions of a QFN of a given outline differ from one maker to the next (KiCad has one footprint per datasheet); a package not in the list needs a library file. The same holds for the QFPs: KiCad's other QFPs are from one maker's drawing each (NXP's SOT outlines, ON Semiconductor's, Microchip's hand-drawn rectangular pads, with other lead spans and lengths), and only those 16 that are the plain MS-026 pattern are generated.

## The rule: IPC-7351B, nominal density

Pads follow the land pattern formulas of IPC-7351B at the median ("nominal", Level B) density, with the fabrication tolerance F = 0.1 mm and the placement tolerance P = 0.05 mm (`generate/ipc.rs` has the formulas and the fillet goal tables):

```text
Zmax = Lmin + 2 Jt + sqrt(CL^2 + F^2 + P^2)       outer edge to outer edge
Gmin = Smax - 2 Jh - sqrt(CS^2 + F^2 + P^2)       inner edge to inner edge
Xmax = Wmin + 2 Js + sqrt(CW^2 + F^2 + P^2)       pad width
```

with `Snom = Lnom - 2 Tnom` and `CS = sqrt(CL^2 + 2 CT^2)` when the datasheet gives the lead length T, rounded to 0.02 mm (chips below 0603) or 0.05 mm. The courtyard is the pads and the body plus 0.25 mm, rounded to 0.01 mm. Pad corners have a radius of 0.25 mm at most a quarter of the shorter side, as KiCad's generated pads have.

## Sources and licences (read 2026-11-02)

- **IPC-7351B and IPC-SM-782** are paid documents and were not read. The formulas, the fillet goals (tables 3-2, 3-3, 3-15, median column) and the rounding grids were taken from the published configuration files and comments of `kicad-footprint-generator`, which tabulate them; the body and terminal envelopes of chips are the IPC-SM-782 numbers as the same repository's data files record them. These are numbers and formulas, which are facts, not the repository's code.
- **`kicad-footprint-generator`** (`gitlab.com/kicad/libraries/kicad-footprint-generator`): its README says GPL-3.0-or-later, with some MIT and LGPL files. Its Python (`ipc_pad_size_calculators.py`, the no-lead generator) was read to learn the formula for the span of a lead, the exposed-pad clearance rule and the paste coverage; the Rust here was written from that maths, shares no code or structure with it, and is MIT like the rest of netsmith. Whether reading GPL code to learn a published formula taints what is written is the owner's to judge; the formulas are IPC's.
- **`kicad-footprint-generator`'s `data/package/no_lead/*.yaml`** (fetched 2026-11-04) hold the body, lead, exposed pad and paste inputs of the QFN/DFN/SON rows added in the second pass, each entry citing the manufacturer's datasheet. They are numbers from those datasheets; they were read as data and not copied into the repository as files: the rows are `generate/qfn_rows.rs`, one line each.
- **JEDEC MS-026** (the outline of the thin and low profile quad flat packs) gives the QFP rows: a lead span of the body plus 2.0 mm as a basic dimension (no tolerance), a lead 0.45 to 0.75 mm long. The lead widths per pitch (0.13 to 0.23 mm at 0.4 mm, 0.17 to 0.27 mm at 0.5 mm, 0.30 to 0.45 mm at 0.8 mm, and for the two 0.65 mm rows 0.17 to 0.27 mm or 0.22 to 0.33 mm) are the ranges that reproduce KiCad 9.0's pads; the datasheets of the packages differ from one maker to the next within MS-026's ranges, and a width of a datasheet that was not read is not claimed.
- **Recorded pads** (DPAK, SOT-89, SC-70, crystals): the pads and courtyards are numbers read from KiCad 9.0's footprints of the same name (the SC-70 ones cite no drawing, only their package names), and the others cite Infineon's PG-TO252 drawings, Microchip's 3L SOT-89 drawing (C04-029C), Ricoh's SOT-89-5 outline and the crystal makers' datasheets named in each row of `generate/crystal.rs`. The drawings themselves were not read. The numbers are facts about those footprints, not their expression; the same basis as SOT-223 above.
- **JEDEC outlines** (MS-012, MS-013, MO-150, MO-153, MO-187, MO-178, TO-236) and manufacturers' drawings are the lead dimensions of the gull-wing and SOT rows. The QFN, DFN, WSON and HVSON rows take the body, lead and exposed pad dimensions from the datasheets that KiCad's data files cite per package (ST, Analog Devices, Microchip, Nordic and others); the datasheet URL of each is in that repository's `data/package/no_lead/`.
- **KiCad's footprints** (CC-BY-SA 4.0 with the design exception, ADR 0041) are used as the check, not copied: the test below reads them from an installed KiCad; none is in this repository. The recorded pad sizes of SOD, SMx and SOT-223, and a few fitted inputs (below), are facts about those footprints (numbers), stated as such.

## Lookup order

ADR 0087 adds a step before these. The footprints that netsmith ships as files in `std/libs/`, for packages that are neither in KiCad's libraries nor a rule the generators can state, answer to the library name `netsmith` and are read from the binary first (the first of them is the LT8641's QFN-18).


`FootprintLibrary::load` takes the first of: a project's own `.pretty` library of that name; the generator for the name; the file in the installed KiCad folder. **The generator wins for the names it makes**, so a build gives the same pads on every machine whatever KiCad (or nothing) is installed, and the pads do not move with a KiCad patch release. KiCad's folder serves only the names without a generator. A project library named like a generated library (`Fuse`, `Package_SO`) is refused with both named (`FootprintError::AmbiguousGenerated`), as one named like an installed library already is. The mapping needs no change: it names footprints as before, and a row that names a generated footprint now needs no library for it. `FootprintLibrary::installed()` has the generators; `FootprintLibrary::at(root)` reads only its folder (the sample libraries of the tests, and the comparison with KiCad); `.with_generators()` adds them.

## Validation

`crates/netsmith-pcb/tests/generated_vs_kicad.rs` compares every generated footprint with the footprint of the same name in the installed KiCad 9.0 libraries (skipped without them): the pad count, numbers, kind, shape, layers, hole, and the box of every pad (so a pad turned a quarter agrees), within 0.01 mm, and the courtyard within 0.01 mm. A difference needs an entry in the test's `DIFFERENCES` table with its bound and reason, and an entry that nothing needs fails the test. `cargo test -p netsmith-pcb --test main generated_vs_kicad:: -- --nocapture` prints the table. Result against KiCad 9.0 (installed, 2026-11-04 for the TO-263, BGA, exposed-pad QFP and the added QFN/DFN/SON/CSP rows, 2026-11-09 for the SC-70 rows, 2026-11-03 for the QFP, DPAK, SOT-89 and crystal rows, 2026-11-02 for the others): 722 of 802 agree; 80 differ, all in the courtyard (the 2.54 mm sockets).

| Family | Generated | Agree within 0.01 mm | Largest pad deviation | Largest courtyard deviation |
|---|---|---|---|---|
| Chips (6 kinds) | 45 | 45 | 0.000 | 0.000 |
| SOD, SMA, SMB, SMC | 5 | 5 | 0.000 | 0.000 |
| SOT-23, -3, -5, -6, SOT-223 | 6 | 6 | 0.000 | 0.000 |
| SC-70 (SOT-323, -343, -353, -363, SC-70-8), reflow and hand soldering | 10 | 10 | 0.000 | 0.000 |
| SOIC, TSSOP, MSOP, SSOP | 18 | 18 | 0.000 | 0.000 |
| QFN, DFN, WSON, HVSON, VSON, USON, LFCSP and others (`Package_DFN_QFN`, `Package_SON`, `Package_CSP`) | 253 | 253 | 0.008 | 0.010 (a rounding tie in the courtyard) |
| LQFP, TQFP, VQFP | 16 | 16 | 0.000 | 0.000 |
| LQFP, TQFP, EQFP with an exposed pad | 11 | 11 | 0.006 | 0.000 |
| TO-252 (DPAK), SOT-89 | 10 | 10 | 0.000 | 0.000 |
| TO-263 (D2PAK) | 11 | 11 | 0.000 | 0.000 |
| BGA on a full regular grid | 79 | 79 | 0.000 | 0.010 (a rounding tie in the courtyard) |
| SMD crystals | 18 | 18 | 0.000 | 0.000 |
| Pin headers, 2.54 and 2.0 mm, and 2.0 mm sockets | 240 | 240 | 0.000 | 0.000 |
| 2.54 mm pin sockets | 80 | 0 | 0.000 | 0.050 |

Where KiCad differs by design or was not matched:

- **2.54 mm pin sockets**: KiCad draws the courtyard 0.04 to 0.05 mm inside body plus 0.5 mm, which its headers and 2.0 mm sockets do not. The pads agree. Not imitated (a 0.05 mm courtyard is immaterial; recorded in `DIFFERENCES`).
- **SSOP**: only SSOP-16 and SSOP-20 (5.3 mm) are generated. KiCad's other 5.3 mm SSOPs (14, 24, 28) are older hand-drawn footprints with square pads of other sizes, and some of its SSOP-8 variants come from one vendor's drawing each; they are not IPC patterns, so they have no generator.
- **Inputs fitted to KiCad 9.0.** SOT-23 has lead length 0.3 to 0.6 mm (the current generator data has 0.4 to 0.6, which would give an inner gap 0.05 mm longer); MSOP-10 has 4.65 to 5.15 mm over the leads. These are the values that reproduce the footprints of the libraries the user has.
- **QFP courtyard**: KiCad draws the courtyard of its generated QFPs as a polygon with the corners notched; the `Footprint` model keeps a rectangle, and the rectangle is the polygon's bounding box, which is what the test compares. The generated courtyard is therefore the larger shape near each corner (conservative).
- **Second pass (2026-11-04)** added the families below. Each row was generated, compared with KiCad's footprint of the same name, and kept only if it agreed within 0.01 mm in every pad and the courtyard; the rest were dropped, not tuned.
  - **TO-263 (D2PAK)**: 11 of KiCad's 12 (the PG-TO263 pads, `generate/tab.rs`, sharing the DPAK code with a second land pattern). The nine-pin parts have their four paste openings as stencil openings without a number, as KiCad draws them. `TO-263-2_TabPin1` (Wolfspeed's numbering, the tab is pin 1) is left to KiCad's library, as is `TO-252-2_TabPin1`.
  - **QFN, DFN, SON, CSP rows**: 223 more rows (253 in all) in `generate/qfn_rows.rs`. The inputs (body, lead width and length, exposed pad, paste grid, heel reduction) are those of KiCad's `data/package/no_lead/*.yaml` files (the same repository as above: numbers from the manufacturers' datasheets, which the files cite), run through the same rule as the first 30 rows; 232 entries of those files whose footprints exist in KiCad 9.0 were tried and 223 agree. The nine that do not (older shapes: square-cornered exposed pads, a different paste count, `_ThermalVias` names) are left to the library. Entries with per-pad overrides, chamfered edge pins, a custom pad length/centre distance, a lead-to-edge rule, or a paste coverage other than the 65 % of the rule were not tried.
  - **QFPs with an exposed pad**: 11 of 20 (LQFP, TQFP and EQFP, with the MS-026 leads and the exposed pad and its paste grid of the QFN rule). The other nine (`HTQFP-64-1EP_10x10mm_P0.5mm_EP8x8mm`, `EQFP-144-1EP_..._EP8.93x8.7mm`, `LQFP-64-1EP_..._EP5x5mm`, `TQFP-44`, `TQFP-48 ... EP3.5x3.5mm`, `TQFP-52`, `TQFP-64 ... EP5.305x5.305mm` and `... EP8x8mm`, `TQFP-80`) differ from the rule by 0.013 to 0.075 mm in a pad or the courtyard (five have square-cornered pads in KiCad, which look like older hand-drawn footprints) and are left to the library.
  - **BGA**: 79 of the `Package_BGA` footprints whose balls fill the whole grid (`generate/bga.rs`): circular pads on the pitch, lettered A to Y without I, O, Q, S, X, Z, then AA..., a courtyard 1 mm outside the body, the pad diameter recorded per row (it is not a function of the pitch: 0.5 mm pitch alone has pads from 0.2 to 0.35 mm). Of KiCad's 200 `Package_BGA` footprints, 83 have a full grid of one pitch and one pad size and are named with their layout; 79 of those agree. Not generated: grids with balls left out (the others, among them most TI DSBGA, Micron and Xilinx footprints), two pitches, two pad sizes, and the four whose courtyard is another number (`BGA-25_6.35x6.35mm_Layout5x5_P1.27mm`, `BGA-36_3.396x3.466mm_...`, `XFBGA-36` and `XFBGA-121`). Whether a depopulated grid can be a rule (a list of missing balls per row) is open.
- **SC-70**: the ten footprints of the SC-70 family that KiCad 9.0.0 has (`SOT-323_SC-70`, `SOT-343_SC-70-4`, `SOT-353_SC-70-5`, `SOT-363_SC-70-6`, `SC-70-8`, each with a `_Handsoldering` twin) are hand-drawn: pads 0.65 x 0.4 mm at 0.65 mm pitch, one drawn body and a courtyard that is not a margin round the pads, so they are recorded rows, not a rule. Later 9.0.x libraries (9.0.7) regenerate them with IPC pads from JEDEC MO-203 and drop the hand soldering twins, so a name here agrees with 9.0.0, the version the tests read (issue #103), and not with 9.0.7; if the project moves to a later library the rows are to be replaced by a rule from MO-203. The std part `SOT-353_SC-70-5` is now generated and needs no library.
- **Not generated**: X2SON (`Texas_X2SON-4_1x1mm_P0.65mm`, `Texas_X2SON-5_0.8x0.8mm_P0.48mm`: no `no_lead` data file describes them, they are TI drawings one each; the IPC rule does not reproduce them), LGA, the QFPs of one maker's drawing and the nine exposed-pad QFPs named above, PQFP/MQFP, the BGAs named above, crystals named for a maker's part and through-hole crystals, connectors other than pin headers, mounting holes and fiducials (the std rows that name those still read KiCad's library: `Button_Switch_SMD`, `MountingHole`, `Fiducial`). A project's own library named `Package_BGA` or `Package_CSP` is now refused like the other generated libraries (`AmbiguousGenerated`).

A pad that is wrong makes a wrong board, so the check matters more than the generator. Without KiCad installed the test cannot run (CI without it skips); the unit tests in `generate/mod.rs` pin the numbers of one footprint per family from the values above, and the examples' DRC and fabrication files are the second check: all six examples (`examples/*.nsm`) pass `fab`'s design rules check with generated footprints, and `fab --cross-check` of `linear_regulator.nsm` shows 0 findings that KiCad's check has and netsmith's lacks, or the reverse (the pad, mask and copper layers agree with KiCad's plot; the silkscreen differs, as expected, because the drawing is netsmith's own).

## Differences a build shows

The generated footprints draw their silkscreen their own way (clear of copper by 0.26 mm, centre line to edge; a bracket for polarity on LEDs and diodes; a marker triangle for pin 1), and carry the `(model ...)` path of the KiCad 3D library under the same name, which `fab` never reads. A board built with generated footprints differs from one built with KiCad's in silkscreen and in text positions, and in pads by less than 0.01 mm. Where a footprint's own graphics matter (the DRC's silkscreen checks), the generated one passes the same rules the examples ran with.

## Consequences

- `build` and `fab` of a design that uses only generated packages need no footprint library at all (symbols still do: ADR 0041, stage 5).
- ADR 0041's stage 6 is this ADR; stages 2 and 3 (`libs fetch`, `libs vendor`) remain for the packages that have no generator.
- Provisional: the families grow one validated package at a time; a new row must agree with KiCad's footprint or state why not.
