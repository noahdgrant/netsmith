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
| SOIC, TSSOP, MSOP, SSOP | `Package_SO:SOIC-8_3.9x4.9mm_P1.27mm` ... (18 names) | IPC formulas from JEDEC MS-012, MS-013, MO-153, MO-187, MO-150 |
| QFN, DFN, WSON, HVSON | `Package_DFN_QFN:QFN-16-1EP_3x3mm_P0.5mm_EP1.7x1.7mm` ... (30 names) | IPC formulas for flat no-lead, exposed pad kept 0.2 mm clear, paste openings covering 65 % |
| Pin headers and sockets | `PinHeader_1x04_P2.54mm_Vertical`, `PinSocket_2x05_P2.00mm_Vertical`, 1 or 2 rows, 1 to 40 per row | pad 0.7 of the pitch, drill 1.0 mm (0.8 mm at 2.0 mm), pin 1 square |

`generate::names()` lists the 424 names; a name outside the list is not generated (`Package_SO:SOIC-9_...` is `None`, not a guess). The QFN family is a list of named packages, because the lead and exposed pad dimensions of a QFN of a given outline differ from one maker to the next (KiCad has one footprint per datasheet); a package not in the list needs a library file.

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
- **JEDEC outlines** (MS-012, MS-013, MO-150, MO-153, MO-187, MO-178, TO-236) and manufacturers' drawings are the lead dimensions of the gull-wing and SOT rows. The QFN, DFN, WSON and HVSON rows take the body, lead and exposed pad dimensions from the datasheets that KiCad's data files cite per package (ST, Analog Devices, Microchip, Nordic and others); the datasheet URL of each is in that repository's `data/package/no_lead/`.
- **KiCad's footprints** (CC-BY-SA 4.0 with the design exception, ADR 0041) are used as the check, not copied: the test below reads them from an installed KiCad; none is in this repository. The recorded pad sizes of SOD, SMx and SOT-223, and a few fitted inputs (below), are facts about those footprints (numbers), stated as such.

## Lookup order

`FootprintLibrary::load` takes the first of: a project's own `.pretty` library of that name; the generator for the name; the file in the installed KiCad folder. **The generator wins for the names it makes**, so a build gives the same pads on every machine whatever KiCad (or nothing) is installed, and the pads do not move with a KiCad patch release. KiCad's folder serves only the names without a generator. A project library named like a generated library (`Fuse`, `Package_SO`) is refused with both named (`FootprintError::AmbiguousGenerated`), as one named like an installed library already is. The mapping needs no change: it names footprints as before, and a row that names a generated footprint now needs no library for it. `FootprintLibrary::installed()` has the generators; `FootprintLibrary::at(root)` reads only its folder (the sample libraries of the tests, and the comparison with KiCad); `.with_generators()` adds them.

## Validation

`crates/netsmith-pcb/tests/generated_vs_kicad.rs` compares every generated footprint with the footprint of the same name in the installed KiCad 9.0 libraries (skipped without them): the pad count, numbers, kind, shape, layers, hole, and the box of every pad (so a pad turned a quarter agrees), within 0.01 mm, and the courtyard within 0.01 mm. A difference needs an entry in the test's `DIFFERENCES` table with its bound and reason, and an entry that nothing needs fails the test. `cargo test -p netsmith-pcb --test generated_vs_kicad -- --nocapture` prints the table. Result against KiCad 9.0 (installed, 2026-11-02): 344 of 424 agree; 80 differ, all in the courtyard.

| Family | Generated | Agree within 0.01 mm | Largest pad deviation | Largest courtyard deviation |
|---|---|---|---|---|
| Chips (6 kinds) | 45 | 45 | 0.000 | 0.000 |
| SOD, SMA, SMB, SMC | 5 | 5 | 0.000 | 0.000 |
| SOT-23, -3, -5, -6, SOT-223 | 6 | 6 | 0.000 | 0.000 |
| SOIC, TSSOP, MSOP, SSOP | 18 | 18 | 0.000 | 0.000 |
| QFN, DFN, WSON, HVSON | 30 | 30 | 0.004 | 0.010 (a rounding tie in the courtyard) |
| Pin headers, 2.54 and 2.0 mm, and 2.0 mm sockets | 240 | 240 | 0.000 | 0.000 |
| 2.54 mm pin sockets | 80 | 0 | 0.000 | 0.050 |

Where KiCad differs by design or was not matched:

- **2.54 mm pin sockets**: KiCad draws the courtyard 0.04 to 0.05 mm inside body plus 0.5 mm, which its headers and 2.0 mm sockets do not. The pads agree. Not imitated (a 0.05 mm courtyard is immaterial; recorded in `DIFFERENCES`).
- **SSOP**: only SSOP-16 and SSOP-20 (5.3 mm) are generated. KiCad's other 5.3 mm SSOPs (14, 24, 28) are older hand-drawn footprints with square pads of other sizes, and some of its SSOP-8 variants come from one vendor's drawing each; they are not IPC patterns, so they have no generator.
- **Inputs fitted to KiCad 9.0.** SOT-23 has lead length 0.3 to 0.6 mm (the current generator data has 0.4 to 0.6, which would give an inner gap 0.05 mm longer); MSOP-10 has 4.65 to 5.15 mm over the leads. These are the values that reproduce the footprints of the libraries the user has.
- **Not generated**: X2SON (KiCad's are Texas Instruments drawings, one each), BGA, QFP, LGA, DPAK, SOT-89, crystals, connectors other than pin headers, mounting holes and fiducials (the std rows that name those still read KiCad's library: `Crystal`, `Button_Switch_SMD`, `MountingHole`, `Fiducial`).

A pad that is wrong makes a wrong board, so the check matters more than the generator. Without KiCad installed the test cannot run (CI without it skips); the unit tests in `generate/mod.rs` pin the numbers of one footprint per family from the values above, and the examples' DRC and fabrication files are the second check: all six examples (`examples/*.nsm`) pass `fab`'s design rules check with generated footprints, and `fab --cross-check` of `linear_regulator.nsm` shows 0 findings that KiCad's check has and netsmith's lacks, or the reverse (the pad, mask and copper layers agree with KiCad's plot; the silkscreen differs, as expected, because the drawing is netsmith's own).

## Differences a build shows

The generated footprints draw their silkscreen their own way (clear of copper by 0.26 mm, centre line to edge; a bracket for polarity on LEDs and diodes; a marker triangle for pin 1), and carry the `(model ...)` path of the KiCad 3D library under the same name, which `fab` never reads. A board built with generated footprints differs from one built with KiCad's in silkscreen and in text positions, and in pads by less than 0.01 mm. Where a footprint's own graphics matter (the DRC's silkscreen checks), the generated one passes the same rules the examples ran with.

## Consequences

- `build` and `fab` of a design that uses only generated packages need no footprint library at all (symbols still do: ADR 0041, stage 5).
- ADR 0041's stage 6 is this ADR; stages 2 and 3 (`libs fetch`, `libs vendor`) remain for the packages that have no generator.
- Provisional: the families grow one validated package at a time; a new row must agree with KiCad's footprint or state why not.
