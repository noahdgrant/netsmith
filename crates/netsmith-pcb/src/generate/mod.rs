//! Footprints that netsmith generates itself (ADR 0043).
//!
//! A generator turns the parameters of a package into a [`Footprint`], the same model the
//! loader makes from a `.kicad_mod`, using the land pattern formulas of IPC-7351B at
//! nominal density ([`ipc`]). Each generator answers to the names KiCad's libraries give
//! its footprints, so a mapping row such as `Resistor_SMD:R_0402_1005Metric` finds it, and
//! [`FootprintLibrary`](crate::FootprintLibrary) consults the generators before any file.
//! Every generated footprint is checked against KiCad's own of the same name by a test
//! (`tests/generated_vs_kicad.rs`) when KiCad's libraries are installed.
//!
//! # Why netsmith generates footprints, how, and from what (ADR 0043, ADR 0041)
//!
//! The user decided on 2026-11-02 (ADR 0041) that netsmith makes the footprints of the common
//! packages itself, from original generators in the style of IPC-7351, checked against KiCad's own
//! footprints to 0.01 mm, rather than depending on KiCad's library bytes. KiCad's `.kicad_mod`
//! stays the file format and nothing of KiCad's library is bundled. A generator writes the text
//! of a `.kicad_mod` and reads it back with the loader, so the placer, the fabrication writers and
//! the `.kicad_pcb` export cannot tell a generated footprint from a read one. Each generator
//! answers to the name KiCad gives the footprint (`Resistor_SMD:R_0402_1005Metric`,
//! `Package_TO_SOT_SMD:SOT-23`), so the rows of `std/kicad.toml` and a project's `kicad.toml` name
//! them as before. The parameters are tables in the generator's source (typed, tested, versioned
//! with the binary), not a data file: nobody extends them at run time, and a project that needs
//! another package writes a library file (ADR 0021, ADR 0097). [`names`] lists what is generated; a
//! name outside the list is not generated (`Package_SO:SOIC-9_...` is `None`, not a guess).
//!
//! **The families:** chips (R, C, L, ferrite, LED, D, fuse, 0201 to 2512), SOD-123, SOD-323, SMA,
//! SMB, SMC, SOT-23 (-3, -5, -6), SOT-223, SC-70 (SOT-323, -343, -353, -363, SC-70-8, each for
//! reflow and for hand soldering), SOIC, TSSOP, MSOP, SSOP, QFN, DFN, WSON, HVSON and the other
//! no-lead rows (`Package_DFN_QFN`, `Package_SON`, `Package_CSP`), LQFP, TQFP, VQFP, DPAK
//! (TO-252), D2PAK (TO-263), SOT-89, SMD crystals, BGA on a full regular grid, and pin headers
//! and sockets (1 or 2 rows, 1 to 40 per row, pad 0.7 of the pitch, drill 1.0 mm or 0.8 mm at
//! 2.0 mm, pin 1 square). The QFN family is a list of named packages, because the lead and
//! exposed pad dimensions of a QFN of a given outline differ from one maker to the next (KiCad has
//! one footprint per datasheet); a package not in the list needs a library file. The same holds
//! for the QFPs: KiCad's other QFPs are from one maker's drawing each, and only those that are the
//! plain MS-026 pattern are generated.
//!
//! **The rule: IPC-7351B, nominal density.** Pads follow the land pattern formulas of IPC-7351B at
//! the median ("nominal", Level B) density, with the fabrication tolerance F = 0.1 mm and the
//! placement tolerance P = 0.05 mm ([`ipc`] has the formulas and the fillet goal tables):
//!
//! ```text
//! Zmax = Lmin + 2 Jt + sqrt(CL^2 + F^2 + P^2)       outer edge to outer edge
//! Gmin = Smax - 2 Jh - sqrt(CS^2 + F^2 + P^2)       inner edge to inner edge
//! Xmax = Wmin + 2 Js + sqrt(CW^2 + F^2 + P^2)       pad width
//! ```
//!
//! with `Snom = Lnom - 2 Tnom` and `CS = sqrt(CL^2 + 2 CT^2)` when the datasheet gives the lead
//! length T, rounded to 0.02 mm (chips below 0603) or 0.05 mm. The courtyard is the pads and the
//! body plus 0.25 mm, rounded to 0.01 mm. Pad corners have a radius of 0.25 mm at most a quarter of
//! the shorter side, as KiCad's generated pads have. Chip passives use the body and terminal
//! envelopes of IPC-SM-782; 0201 has paste-only openings. QFN and DFN keep the exposed pad 0.2 mm
//! clear, with paste openings covering 65 %. Gull-wing QFPs use fine-pitch fillets from 0.625 mm
//! and JEDEC MS-026 (2.0 mm over the leads beyond the body, leads 0.45 to 0.75 mm).
//!
//! **Sources and licences (read 2026-11-02 and 2026-11-04).**
//!
//! - IPC-7351B and IPC-SM-782 are paid documents and were not read. The formulas, the fillet
//!   goals (tables 3-2, 3-3, 3-15, median column) and the rounding grids were taken from the
//!   published configuration files and comments of `kicad-footprint-generator`, which tabulate
//!   them; the body and terminal envelopes of chips are the IPC-SM-782 numbers as the same
//!   repository's data files record them. These are numbers and formulas, which are facts, not
//!   the repository's code.
//! - `kicad-footprint-generator` (`gitlab.com/kicad/libraries/kicad-footprint-generator`): its
//!   README says GPL-3.0-or-later, with some MIT and LGPL files. Its Python
//!   (`ipc_pad_size_calculators.py`, the no-lead generator) was read to learn the formula for the
//!   span of a lead, the exposed-pad clearance rule and the paste coverage; the Rust here was written
//!   from that maths, shares no code or structure with it, and is MIT like the rest of netsmith.
//!   **Whether reading GPL code to learn a published formula taints what is written is the owner's
//!   to judge; the formulas are IPC's.** (Carried word for word from ADR 0043. A person must judge
//!   it; checklist in issue #10.) Translating that Python into Rust would make the Rust a
//!   derivative of GPL code (ADR 0041), so a generator works from IPC-7351 and the package
//!   drawings.
//! - Its `data/package/no_lead/*.yaml` (fetched 2026-11-04) hold the body, lead, exposed pad and
//!   paste inputs of the QFN, DFN and SON rows, each entry citing the manufacturer's datasheet.
//!   They are numbers from those datasheets, read as data and not copied into the repository as
//!   files: the rows are in `qfn_rows.rs`, one line each.
//! - JEDEC MS-026 gives the QFP rows. The lead widths per pitch (0.13 to 0.23 mm at 0.4 mm, 0.17 to
//!   0.27 mm at 0.5 mm, 0.30 to 0.45 mm at 0.8 mm, and for the two 0.65 mm rows 0.17 to 0.27 mm or
//!   0.22 to 0.33 mm) are the ranges that reproduce KiCad 9.0's pads; the datasheets of the packages
//!   differ from one maker to the next within MS-026's ranges, and a width of a datasheet that
//!   was not read is not claimed. JEDEC MS-012, MS-013, MO-150, MO-153, MO-187, MO-178 and TO-236
//!   and manufacturers' drawings are the lead dimensions of the gull-wing and SOT rows.
//! - Recorded pads (DPAK, SOT-89, SC-70, crystals, SOD, SMx, SOT-223): numbers read from KiCad
//!   9.0's footprints of the same name, or from Infineon's PG-TO252 drawings, Microchip's 3L SOT-89
//!   drawing (C04-029C), Ricoh's SOT-89-5 outline and the crystal makers' datasheets named in
//!   `crystal.rs`. The drawings themselves were not read. The numbers are facts about those
//!   footprints, not their expression.
//! - KiCad's footprints (CC-BY-SA 4.0 with the design exception, see ADR 0097) are used as the
//!   check and not copied: the test reads them from an installed KiCad, and none is in this
//!   repository.
//! - **Not decided; for a person (ADR 0041 question 2; checklist in issue #10):** "Is the IPC-7351 route acceptable (IPC
//!   documents are paid), or should the dimensions come from manufacturers' drawings, package by
//!   package?" and "Whether the IPC-SM-782 tables can be read freely is the owner's to check (IPC
//!   standards are paid documents)." And: "Whether pad geometry computed from the IPC formulas
//!   can be freely reproduced is not something this ADR can settle."
//!
//! **The lookup order** (`FootprintLibrary::load`): the footprints netsmith ships as files in
//! `std/libs/` (library name `netsmith`, ADR 0087), a project's own `.pretty` library of that
//! name, the generator for the name, then the file in the installed KiCad folder. **The generator
//! wins for the names it makes**, so a build gives the same pads on every machine whatever KiCad
//! (or nothing) is installed, and the pads do not move with a KiCad patch release. A project library
//! named like a generated library (`Fuse`, `Package_SO`, `Package_BGA`, `Package_CSP`) is refused
//! with both named (`FootprintError::AmbiguousGenerated`), as one named like an installed library
//! is. `FootprintLibrary::installed()` has the generators; `FootprintLibrary::at(root)` reads only
//! its folder (the sample libraries of the tests, and the comparison with KiCad);
//! `.with_generators()` adds them. `build` and `fab` of a design that uses only generated
//! packages need no footprint library at all.
//!
//! **By parameters (ADR 0104).** A project's `kicad.toml` may ask for a footprint by generator name and
//! parameters, `gen:QFN(pins=48, pitch=0.5mm, ...)`, in place of a `Library:Name` id, for a package
//! that none of the names above is. [`generate_spec`] makes it with the same code as the table row
//! with the same numbers (`params.rs` reads the parameters and checks them, and its tests compare the
//! pads with [`generate`] for several ids of each generator): `QFN`, `DFN`, `QFP`, `SO`, `SOT23`,
//! `PinHeader`, `PinSocket` and `BGA` ([`GENERATORS`]). The footprint is in the library `gen` under a
//! name made of the generator and its parameters, which [`generate`] reads back. Not exposed: the
//! chips, diodes, crystals, the recorded pads (SOT-223, SC-70, DPAK, SOT-89) and the mechanical parts.
//!
//! **Validation.** `tests/generated_vs_kicad.rs` compares every generated footprint with the
//! footprint of the same name in the installed KiCad 9.0 libraries (skipped without them): the pad
//! count, numbers, kind, shape, layers, hole and the box of every pad (so a pad turned a quarter
//! agrees) within 0.01 mm, and the courtyard within 0.01 mm. A difference needs an entry in the
//! test's `DIFFERENCES` table with its bound and reason, and an entry that nothing needs fails the
//! test. `cargo test -p netsmith-pcb --test main generated_vs_kicad:: -- --nocapture` prints the
//! table of every family: how many agree, the largest pad deviation and the largest courtyard
//! deviation (2026-11-09: 775 of 855 agree, and the 80 that differ are all 2.54 mm pin sockets, in
//! the courtyard, by 0.05 mm at most; the largest pad deviation anywhere is 0.008 mm and the
//! largest courtyard deviation of the others 0.010 mm, a rounding tie). A wrong pad is a wrong
//! board, so the check matters more than the generator. Without KiCad installed the test cannot
//! run (CI without it skips); the unit tests in this module pin the numbers of one footprint per
//! family, and the design rules check and the fabrication files of the examples are the second
//! check: all six examples pass `fab`'s check with generated footprints, and `fab --cross-check`
//! of `linear_regulator.nsm` shows 0 findings that KiCad's check has and netsmith's lacks, or the
//! reverse (the pad, mask and copper layers agree; the silkscreen differs, because the drawing is
//! netsmith's own).
//!
//! **Where KiCad differs by design or was not matched.** The 2.54 mm pin sockets: KiCad draws the
//! courtyard 0.04 to 0.05 mm inside body plus 0.5 mm, which its headers and 2.0 mm sockets do not;
//! not imitated (recorded in `DIFFERENCES`). SSOP: only SSOP-16 and SSOP-20 (5.3 mm) are generated,
//! since KiCad's other SSOPs are older hand-drawn footprints with square pads of other sizes and
//! some SSOP-8 come from one vendor's drawing each. Inputs fitted to KiCad 9.0: SOT-23 has lead
//! length 0.3 to 0.6 mm (the current generator data has 0.4 to 0.6, which would give an inner gap
//! 0.05 mm longer) and MSOP-10 has 4.65 to 5.15 mm over the leads; the TSSOP 10.0 mm span of the
//! 8.0 mm body, its 0.17 to 0.30 mm lead width at 0.65 mm pitch, the 6.1 mm body at 0.65 and
//! 0.4 mm pitch and the 0.45 mm lower lead length are not read from a datasheet, and were fitted to
//! reproduce KiCad 9.0.0's pads, as the QFP lead widths were (TI's SN74LVC16245A datasheet,
//! SCES062Q, gives 6.20 to 6.60 mm over the leads of a 4.4 mm body with leads 0.13 to 0.23 mm wide
//! at 0.4 mm pitch, page 21, drawing MPDS006C, and 7.90 to 8.30 mm over a 6.1 mm body with leads
//! 0.17 to 0.27 mm wide at 0.5 mm pitch and 0.50 to 0.75 mm long, page 25, MTSS003D, which says it
//! falls within JEDEC MO-153; those nominal values are the rule's inputs for those two). QFP
//! courtyard: KiCad draws it as a polygon with the corners notched and the `Footprint` model keeps
//! a rectangle, the polygon's bounding box, which is what the test compares, so the generated
//! courtyard is the larger shape near each corner (conservative). Each added family was
//! generated, compared with KiCad's footprint of the same name, and kept only if it agreed within
//! 0.01 mm in every pad and the courtyard; the rest were dropped and not tuned.
//!
//! **Not generated:** the nine exposed-pad QFPs that differ from the rule by 0.013 to 0.075 mm, the
//! nine no-lead entries of KiCad's data files that do not agree (older shapes with square-cornered
//! exposed pads, a different paste count, `_ThermalVias` names), TSSOP-44 (square corners), the exposed-pad (`-1EP`,
//! `HTSSOP`, `ThermalVias`) rows, `TO-263-2_TabPin1` and `TO-252-2_TabPin1`, MSOP-12/16,
//! VSSOP-8/10, SOIC-10, SOIC-20W and SOIC-28W (other IPC fits one by one), X2SON (TI drawings one
//! each), LGA, PQFP/MQFP, the BGAs with balls left out, with two pitches or two pad sizes and the
//! four whose courtyard is another number, crystals named for a maker's part and through-hole
//! crystals, and connectors other than pin headers, mounting holes and fiducials (the std rows that
//! name those still read KiCad's library: `Button_Switch_SMD`, `MountingHole`, `Fiducial`).
//! Not built (issue #15): a depopulated BGA grid as a rule (a list of missing balls per row). The ten SC-70
//! footprints of KiCad 9.0.0 are hand-drawn (pads 0.65 x 0.4 mm at 0.65 mm pitch, one drawn body
//! and a courtyard that is not a margin round the pads), so they are recorded rows and not a rule;
//! KiCad 9.0.7 regenerates them with IPC pads from JEDEC MO-203 and drops the hand soldering
//! twins, so a name here agrees with 9.0.0, the version the tests read, and not
//! with 9.0.7: if the project moves to a later library the rows are to be replaced by a rule from
//! MO-203 (issue #15). A BGA is circular pads on the pitch, lettered A to Y without I, O, Q, S, X, Z, then AA,
//! a courtyard 1 mm outside the body, the pad diameter recorded per row (it is not a function of the
//! pitch: 0.5 mm pitch alone has pads from 0.2 to 0.35 mm); of KiCad's 200 `Package_BGA`
//! footprints, 83 have a full grid of one pitch and one pad size and 79 of those agree. A TSSOP
//! row is one line of (name, pins, pitch, body width, body length).
//!
//! **Differences a build shows.** The generated footprints draw their silkscreen their own way
//! (clear of copper by 0.26 mm, centre line to edge; a bracket for polarity on LEDs and diodes; a
//! marker triangle for pin 1) and carry the `(model ...)` path of the KiCad 3D library under the
//! same name, which `fab` never reads. A board built with generated footprints differs from one
//! built with KiCad's in silkscreen and in text positions, and in pads by less than 0.01 mm.
//!
//! **Status.** ADR 0043 stays `proposed`: the licence questions above are for a person (issue #10).
//! The families grow one validated package at a time (issue #15), and a new row must agree with
//! KiCad's footprint or state why not.

mod bga;
mod chip;
mod crystal;
mod diode;
mod draw;
mod gullwing;
mod header;
pub mod ipc;
mod mechanical;
mod outline;
mod params;
mod qfn;
mod qfn_rows;
mod qfp;
mod sot;
mod tab;

use std::sync::OnceLock;

use netsmith_kicad::footprint_spec::{self, FootprintSpec};

use crate::footprint::Footprint;

/// The generators a project can ask for by name and parameters (`gen:QFN(...)`, ADR 0104).
pub const GENERATORS: [&str; 8] = [
    "QFN",
    "DFN",
    "QFP",
    "SO",
    "SOT23",
    "PinHeader",
    "PinSocket",
    "BGA",
];

/// The footprint a `gen:` spec asks for: the generator's pads for the parameters it names,
/// in the library `gen` under the name the spec's id gives ([`FootprintSpec::id`]). The message
/// says what is wrong with the spec and does not say where the spec is written.
pub fn generate_spec(spec: &FootprintSpec) -> Result<Footprint, String> {
    let id = spec.id();
    let name = id
        .strip_prefix(&format!("{}:", footprint_spec::LIBRARY))
        .expect("an id starts with its library");
    let library = footprint_spec::LIBRARY;
    let param_names = |known: &[&str]| params::Params::new(spec, known);
    let made = match spec.generator.as_str() {
        "QFN" | "DFN" => {
            let p = param_names(&[
                "pins",
                "pitch",
                "body",
                "ep",
                "lead_width",
                "lead_length",
                "paste",
                "heel",
            ])?;
            qfn::from_params(&p, library, name, spec.generator == "DFN")
        }
        "QFP" => {
            let p = param_names(&["pins", "body", "pitch", "lead_width", "ep", "paste"])?;
            qfp::from_params(&p, library, name)
        }
        "SO" => {
            let p = param_names(&["pins", "pitch", "body", "span", "lead_width", "lead_length"])?;
            gullwing::from_params(&p, library, name)
        }
        "SOT23" => {
            let p = param_names(&["pins", "span", "lead_width", "lead_length", "body"])?;
            sot::from_params(&p, library, name)
        }
        "PinHeader" | "PinSocket" => {
            let p = param_names(&["rows", "pins", "pitch"])?;
            header::from_params(&p, &spec.generator, library, name)
        }
        "BGA" => {
            let p = param_names(&["layout", "pitch", "body", "pad"])?;
            bga::from_params(&p, library, name)
        }
        other => Err(format!(
            "there is no generator `{other}`; the generators are {}",
            GENERATORS.join(", ")
        )),
    }?;
    params::check_pads(&made)?;
    Ok(made)
}

/// The footprint that `Library:Name` names, if a generator makes it. A `gen:` id
/// ([`FootprintSpec::id`]) is made from the parameters its name spells.
pub fn generate(id: &str) -> Option<Footprint> {
    if let Some(spec) = FootprintSpec::from_id(id) {
        return generate_spec(&spec).ok();
    }
    let (library, name) = id.split_once(':')?;
    chip::generate(library, name)
        .or_else(|| gullwing::generate(library, name))
        .or_else(|| sot::generate(library, name))
        .or_else(|| tab::generate(library, name))
        .or_else(|| diode::generate(library, name))
        .or_else(|| qfn::generate(library, name))
        .or_else(|| qfp::generate(library, name))
        .or_else(|| header::generate(library, name))
        .or_else(|| crystal::generate(library, name))
        .or_else(|| bga::generate(library, name))
        .or_else(|| mechanical::generate(library, name))
}

/// Every `Library:Name` that [`generate`] answers, in a fixed order.
pub fn names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut all = chip::names();
        all.extend(gullwing::names());
        all.extend(sot::names());
        all.extend(tab::names());
        all.extend(diode::names());
        all.extend(qfn::names());
        all.extend(qfp::names());
        all.extend(header::names());
        all.extend(crystal::names());
        all.extend(bga::names());
        all.extend(mechanical::names());
        all.sort();
        all.dedup();
        all
    })
}

/// Whether some generated name belongs to the library `library`.
pub fn covers_library(library: &str) -> bool {
    names()
        .iter()
        .any(|n| n.split_once(':').is_some_and(|(l, _)| l == library))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::footprint::{Layer, Mount, PadKind, PadShape};
    use crate::geometry::{Pt, Rect};

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn every_listed_name_is_generated_under_that_name_and_twice_the_same() {
        assert!(names().len() > 400, "{} names", names().len());
        let mut seen = std::collections::HashSet::new();
        for id in names() {
            assert!(seen.insert(id.as_str()), "{id} is listed twice");
            let first = generate(id).unwrap_or_else(|| panic!("{id} is listed but not made"));
            let (_, name) = id.split_once(':').unwrap();
            assert_eq!(first.name, name);
            assert_eq!(generate(id).unwrap(), first, "{id} is not deterministic");
            assert!(!first.pads.is_empty(), "{id} has no pads");
            assert!(first.courtyard.width() > 0.0, "{id} has no courtyard");
        }
    }

    #[test]
    fn a_name_that_is_not_generated_is_none() {
        for id in [
            "Resistor_SMD:R_9999_9999Metric",
            "Resistor_SMD:C_0402_1005Metric",
            "Nothing:R_0402_1005Metric",
            "R_0402_1005Metric",
            "Package_SO:SOIC-9_3.9x4.9mm_P1.27mm",
            // A QFP of a maker's own drawing, which is not the MS-026 pattern.
            "Package_QFP:LQFP-64_10x10mm_P0.5mm",
            "Package_QFP:TQFP-47_7x7mm_P0.5mm",
            // Wolfspeed's pad numbering (the tab is pin 1), not the Infineon pattern.
            "Package_TO_SOT_SMD:TO-263-2_TabPin1",
            "Connector_PinHeader_2.54mm:PinHeader_1x2_P2.54mm_Vertical",
            "Connector_PinHeader_2.54mm:PinHeader_1x41_P2.54mm_Vertical",
            "Connector_PinHeader_2.54mm:PinHeader_3x02_P2.54mm_Vertical",
            "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.00mm_Vertical",
        ] {
            assert!(generate(id).is_none(), "{id}");
        }
    }

    #[test]
    fn the_0402_resistor_has_the_pads_ipc_7351b_gives_a_body_of_1_0_to_1_1_by_0_48_to_0_6() {
        let fp = generate("Resistor_SMD:R_0402_1005Metric").unwrap();
        assert_eq!(fp.mount, Mount::Smd);
        assert_eq!(fp.pad_numbers(), ["1", "2"]);
        let (a, b) = (&fp.pads[0], &fp.pads[1]);
        assert!(near(a.at.x, -0.51) && near(b.at.x, 0.51) && near(a.at.y, 0.0));
        assert!(near(a.width, 0.54) && near(a.height, 0.64));
        assert_eq!((a.kind, a.shape), (PadKind::Smd, PadShape::RoundRect));
        assert_eq!(a.layers, ["F.Cu", "F.Mask", "F.Paste"]);
        // A quarter of the shorter side, as KiCad's generated pads have it.
        assert!(near(a.corner_ratio, 0.25));
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-0.93, -0.47), Pt::new(0.93, 0.47))
        );
        assert!(near(fp.reference_at.y, -1.17));
    }

    #[test]
    fn a_larger_chip_has_a_corner_radius_of_at_most_a_quarter_millimetre() {
        let fp = generate("Resistor_SMD:R_1206_3216Metric").unwrap();
        let pad = &fp.pads[0];
        assert!((pad.corner_ratio * pad.width.min(pad.height) - 0.25).abs() < 1e-4);
    }

    #[test]
    fn the_0201_has_stencil_openings_that_are_not_pads() {
        let fp = generate("Capacitor_SMD:C_0201_0603Metric").unwrap();
        assert_eq!(fp.pads.len(), 4);
        let paste: Vec<_> = fp.pads.iter().filter(|p| p.number.is_empty()).collect();
        assert_eq!(paste.len(), 2);
        assert!(paste.iter().all(|p| p.layers == ["F.Paste"]));
        // Pulled in from the copper pads, and not as wide.
        let copper = &fp.pads.iter().find(|p| p.number == "1").unwrap();
        assert!(paste[0].width < copper.width && paste[0].height < copper.height);
    }

    #[test]
    fn a_led_and_a_diode_mark_the_cathode_side_on_the_silkscreen() {
        for id in ["LED_SMD:LED_0603_1608Metric", "Diode_SMD:D_SOD-123"] {
            let fp = generate(id).unwrap();
            let silk: Vec<_> = fp
                .graphics
                .iter()
                .filter(|g| g.layer == Layer::Silk)
                .collect();
            assert!(!silk.is_empty(), "{id}");
            // Everything on the silkscreen is on the left of pad 1's outer edge, or between
            // the pads: none of it sits on pad 2's side beyond the body.
            let pad1 = &fp.pads.iter().find(|p| p.number == "1").unwrap();
            let outer = pad1.at.x - pad1.width / 2.0;
            assert!(
                silk.iter()
                    .flat_map(|g| g.shape.extent_points())
                    .any(|p| p.x < outer),
                "{id}: no silkscreen by pad 1"
            );
        }
    }

    #[test]
    fn sot_23_has_three_pads_on_the_ipc_pattern() {
        let fp = generate("Package_TO_SOT_SMD:SOT-23").unwrap();
        assert_eq!(fp.pad_numbers(), ["1", "2", "3"]);
        let at = |n: &str| fp.pads_numbered(n).next().unwrap().at;
        assert!(near(at("1").x, -0.9375) && near(at("1").y, -0.95));
        assert!(near(at("2").y, 0.95));
        assert!(near(at("3").x, 0.9375) && near(at("3").y, 0.0));
        assert!(near(fp.pads[0].width, 1.475) && near(fp.pads[0].height, 0.6));
    }

    #[test]
    fn sot_23_5_and_6_number_down_the_left_and_up_the_right() {
        let five = generate("Package_TO_SOT_SMD:SOT-23-5").unwrap();
        let six = generate("Package_TO_SOT_SMD:SOT-23-6").unwrap();
        assert_eq!(five.pad_numbers(), ["1", "2", "3", "4", "5"]);
        assert_eq!(six.pad_numbers(), ["1", "2", "3", "4", "5", "6"]);
        let y = |fp: &Footprint, n: &str| fp.pads_numbered(n).next().unwrap().at.y;
        assert!(y(&six, "1") < y(&six, "3") && y(&six, "4") > y(&six, "6"));
        // The five pin part has no middle pad on the right.
        assert!(five.pads_numbered("5").all(|p| near(p.at.y, -0.95)));
    }

    #[test]
    fn sot_223_has_a_tab_pad_numbered_as_its_name_says() {
        let tab2 = generate("Package_TO_SOT_SMD:SOT-223-3_TabPin2").unwrap();
        assert_eq!(tab2.pad_numbers(), ["1", "2", "3"]);
        assert_eq!(tab2.pads_numbered("2").count(), 2);
        let tab = tab2.pads.iter().find(|p| p.at.x > 0.0).unwrap();
        assert_eq!(tab.number, "2");
        assert!(near(tab.height, 3.8));
        let four = generate("Package_TO_SOT_SMD:SOT-223").unwrap();
        assert_eq!(four.pad_numbers(), ["1", "2", "3", "4"]);
    }

    #[test]
    fn the_tssop_rows_have_pads_1_475_mm_long_at_the_span_of_their_body_width() {
        // (name, pads, centre of the left column, pad width) from the MO-153 spans:
        // 6.4 mm over the leads of a 4.4 mm body, 8.1 mm of a 6.1 mm one, 10.0 mm of an 8 mm one.
        for (name, pads, x, width) in [
            ("TSSOP-20_4.4x5mm_P0.5mm", 20, 2.8625, 0.3),
            ("TSSOP-24_6.1x7.8mm_P0.65mm", 24, 3.7125, 0.4),
            ("TSSOP-48_6.1x9.7mm_P0.4mm", 48, 3.7125, 0.25),
            ("TSSOP-56_8x14mm_P0.5mm", 56, 4.6625, 0.3),
        ] {
            let fp = generate(&format!("Package_SO:{name}")).unwrap();
            assert_eq!(fp.pads.len(), pads, "{name}");
            let first = &fp.pads[0];
            assert!(near(first.at.x, -x), "{name}: {}", first.at.x);
            assert!(
                near(first.width, 1.475) && near(first.height, width),
                "{name}"
            );
            // Pin 1 is at the top of the left column, the last pin at the top of the right.
            let last = &fp.pads[pads - 1];
            assert!(near(last.at.x, x) && near(last.at.y, first.at.y), "{name}");
        }
        let fp = generate("Package_SO:TSSOP-48_6.1x9.7mm_P0.4mm").unwrap();
        let pitch = fp.pads[1].at.y - fp.pads[0].at.y;
        assert!(near(pitch, 0.4), "{pitch}");
    }

    #[test]
    fn the_wide_soic_rows_share_the_ms_013_pads() {
        let fp = generate("Package_SO:SOIC-14W_7.5x9mm_P1.27mm").unwrap();
        assert_eq!(fp.pads.len(), 14);
        assert!(near(fp.pads[0].at.x, -4.65) && near(fp.pads[0].at.y, -3.81));
        assert!(near(fp.pads[0].width, 2.05) && near(fp.pads[0].height, 0.6));
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-5.93, -4.75), Pt::new(5.93, 4.75))
        );
        assert_eq!(
            generate("Package_SO:SOIC-16W_7.5x12.8mm_P1.27mm")
                .unwrap()
                .pads
                .len(),
            16
        );
    }

    #[test]
    fn the_sc_70_family_has_0_65_mm_pitch_pads_0_65_by_0_4_mm_at_plus_or_minus_0_95() {
        let five = generate("Package_TO_SOT_SMD:SOT-353_SC-70-5").unwrap();
        assert_eq!(five.pad_numbers(), ["1", "2", "3", "4", "5"]);
        let at = |n: &str| five.pads_numbered(n).next().unwrap().at;
        assert!(near(at("1").x, -0.95) && near(at("1").y, -0.65));
        assert!(near(at("3").y, 0.65) && near(at("4").x, 0.95) && near(at("4").y, 0.65));
        assert!(near(at("5").y, -0.65));
        assert!(near(five.pads[0].width, 0.65) && near(five.pads[0].height, 0.4));
        assert_eq!(
            five.courtyard,
            Rect::new(Pt::new(-1.6, -1.4), Pt::new(1.6, 1.4))
        );
        // SOT-323: pin 3 is alone on the right, between the two on the left.
        let three = generate("Package_TO_SOT_SMD:SOT-323_SC-70").unwrap();
        let at3 = |n: &str| three.pads_numbered(n).next().unwrap().at;
        assert!(near(at3("1").y, -0.65) && near(at3("2").y, 0.65));
        assert!(near(at3("3").x, 1.0) && near(at3("3").y, 0.0));
        // SC-70-8: 0.5 mm pitch, pads 1.0 by 0.3 mm.
        let eight = generate("Package_TO_SOT_SMD:SC-70-8").unwrap();
        let at8 = |n: &str| eight.pads_numbered(n).next().unwrap().at;
        assert!(near(at8("1").y, -0.75) && near(at8("4").y, 0.75));
        assert!(near(at8("5").y, 0.75) && near(at8("8").y, -0.75));
        assert!(near(eight.pads[0].width, 1.0) && near(eight.pads[0].height, 0.3));
        // The hand soldering pads are 1.5 mm long, 1.33 mm from the middle.
        let hand = generate("Package_TO_SOT_SMD:SOT-363_SC-70-6_Handsoldering").unwrap();
        assert!(near(hand.pads[0].width, 1.5) && near(hand.pads[0].at.x, -1.33));
    }

    #[test]
    fn soic_8_has_its_pads_numbered_counterclockwise_from_the_top_left() {
        let fp = generate("Package_SO:SOIC-8_3.9x4.9mm_P1.27mm").unwrap();
        let at = |n: &str| fp.pads_numbered(n).next().unwrap().at;
        assert!(near(at("1").x, -2.475) && near(at("1").y, -1.905));
        assert!(near(at("4").y, 1.905) && near(at("5").x, 2.475) && near(at("5").y, 1.905));
        assert!(near(at("8").y, -1.905));
        assert!(near(fp.pads[0].width, 1.95) && near(fp.pads[0].height, 0.6));
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-3.7, -2.7), Pt::new(3.7, 2.7))
        );
    }

    #[test]
    fn a_qfn_has_an_exposed_pad_numbered_after_the_leads_and_paste_openings_without_a_number() {
        let fp = generate("Package_DFN_QFN:QFN-16-1EP_3x3mm_P0.5mm_EP1.7x1.7mm").unwrap();
        assert_eq!(fp.pads.len(), 16 + 1 + 4);
        let ep = fp.pads_numbered("17").next().unwrap();
        assert_eq!(ep.shape, PadShape::Rect);
        assert_eq!(ep.layers, ["F.Cu", "F.Mask"]);
        assert!(near(ep.width, 1.7) && near(ep.height, 1.7));
        // 65 % of the exposed pad is paste, in four openings.
        let paste: f64 = fp
            .pads
            .iter()
            .filter(|p| p.number.is_empty())
            .map(|p| p.width * p.height)
            .sum();
        assert!((paste / (1.7 * 1.7) - 0.65).abs() < 0.02, "{paste}");
        // Pin 1 is the top of the left edge; pin 5 the left of the bottom edge.
        let at = |n: &str| fp.pads_numbered(n).next().unwrap().at;
        assert!(at("1").x < 0.0 && at("1").y < 0.0 && at("4").y > 0.0);
        assert!(at("5").y > 0.0 && at("5").x < 0.0 && at("8").x > 0.0);
        assert!(at("9").x > 0.0 && at("13").y < 0.0 && at("16").x < 0.0);
    }

    #[test]
    fn the_leads_of_a_qfn_keep_off_a_large_exposed_pad() {
        // With a 1.9 mm exposed pad the inner edges of the leads are 0.2 mm from it.
        let fp = generate("Package_DFN_QFN:QFN-16-1EP_3x3mm_P0.5mm_EP1.7x1.7mm").unwrap();
        let lead = fp.pads_numbered("1").next().unwrap();
        let inner = lead.at.x.abs() - lead.width / 2.0;
        assert!(inner >= 1.7 / 2.0 + 0.2 - 1e-9, "{inner}");
    }

    #[test]
    fn a_row_whose_name_does_not_say_its_library_is_found_only_in_that_library() {
        let id = "Package_CSP:LFCSP-16-1EP_3x3mm_P0.5mm_EP1.7x1.7mm";
        let fp = generate(id).unwrap();
        assert_eq!(fp.pads.len(), 16 + 1 + 4);
        assert!(names().iter().any(|n| n == id));
        assert!(generate("Package_DFN_QFN:LFCSP-16-1EP_3x3mm_P0.5mm_EP1.7x1.7mm").is_none());
        // A maker's part named by its drawing, in the library its start does not say.
        assert!(generate("Package_SON:Texas_X2SON-4_1x1mm_P0.65mm").is_none());
    }

    #[test]
    fn a_dfn_has_pads_on_two_edges_only() {
        let fp = generate("Package_DFN_QFN:DFN-8-1EP_3x2mm_P0.5mm_EP1.7x1.6mm").unwrap();
        let leads: Vec<_> = fp
            .pads
            .iter()
            .filter(|p| p.number.parse::<u32>().is_ok_and(|n| n <= 8))
            .collect();
        assert_eq!(leads.len(), 8);
        assert!(leads.iter().all(|p| p.at.x.abs() > 1.0));
    }

    #[test]
    fn a_pin_header_is_through_hole_with_a_square_first_pad() {
        let fp = generate("Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical").unwrap();
        assert_eq!(fp.mount, Mount::ThroughHole);
        assert_eq!(fp.pad_numbers(), ["1", "2"]);
        assert_eq!(fp.pads[0].shape, PadShape::Rect);
        assert_eq!(fp.pads[1].shape, PadShape::Oval);
        assert!(near(fp.pads[1].at.y, 2.54) && near(fp.pads[1].width, 1.7));
        assert!(near(fp.pads[0].drill.unwrap().width, 1.0));
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-1.8, -1.8), Pt::new(1.8, 4.35))
        );
    }

    #[test]
    fn the_second_row_of_a_header_is_to_the_right_and_of_a_socket_to_the_left() {
        let header =
            generate("Connector_PinHeader_2.00mm:PinHeader_2x03_P2.00mm_Vertical").unwrap();
        let socket =
            generate("Connector_PinSocket_2.00mm:PinSocket_2x03_P2.00mm_Vertical").unwrap();
        let at = |fp: &Footprint, n: &str| fp.pads_numbered(n).next().unwrap().at;
        assert_eq!(at(&header, "2"), Pt::new(2.0, 0.0));
        assert_eq!(at(&socket, "2"), Pt::new(-2.0, 0.0));
        assert_eq!(at(&header, "5"), Pt::new(0.0, 4.0));
        assert!(near(header.pads[1].drill.unwrap().width, 0.8));
        assert!(near(header.pads[1].width, 1.35));
    }

    #[test]
    fn a_tqfp_48_has_twelve_pads_on_each_edge_numbered_counterclockwise_from_the_left_top() {
        let fp = generate("Package_QFP:TQFP-48_7x7mm_P0.5mm").unwrap();
        assert_eq!(fp.pads.len(), 48);
        let at = |n: &str| fp.pads_numbered(n).next().unwrap().at;
        // JEDEC MS-026: 9.0 mm over the leads, 0.6 mm leads: pads 1.475 x 0.3 mm centred
        // 4.1625 mm from the middle, the courtyard 0.25 mm outside the pads.
        assert!(near(at("1").x, -4.1625) && near(at("1").y, -2.75));
        assert!(near(at("12").y, 2.75) && near(at("13").x, -2.75) && near(at("13").y, 4.1625));
        assert!(near(at("25").x, 4.1625) && near(at("25").y, 2.75));
        assert!(near(at("37").x, 2.75) && near(at("37").y, -4.1625));
        assert!(near(fp.pads[0].width, 1.475) && near(fp.pads[0].height, 0.3));
        // The pads of the top and bottom edges are turned a quarter.
        assert!(near(fp.pads[12].width, 0.3) && near(fp.pads[12].height, 1.475));
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-5.15, -5.15), Pt::new(5.15, 5.15))
        );
    }

    #[test]
    fn a_qfp_with_an_exposed_pad_has_it_numbered_after_the_leads_with_its_paste_in_a_grid() {
        let fp = generate("Package_QFP:LQFP-48-1EP_7x7mm_P0.5mm_EP3.6x3.6mm").unwrap();
        // 48 leads, the pad, and nine paste openings of 0.97 mm.
        assert_eq!(fp.pads.len(), 48 + 1 + 9);
        let ep = fp.pads_numbered("49").next().unwrap();
        assert!(near(ep.width, 3.6) && near(ep.height, 3.6));
        let paste: Vec<_> = fp.pads.iter().filter(|p| p.number.is_empty()).collect();
        assert!(
            paste
                .iter()
                .all(|p| near(p.width, 0.97) && near(p.height, 0.97))
        );
        // The leads are those of the plain package of the same name.
        let plain = generate("Package_QFP:LQFP-48_7x7mm_P0.5mm").unwrap();
        assert_eq!(fp.pads[0], plain.pads[0]);
        assert_eq!(fp.courtyard, plain.courtyard);
    }

    #[test]
    fn a_bga_has_circular_pads_on_the_grid_lettered_from_the_top_left_without_i_o_q_s_x_z() {
        let fp = generate("Package_BGA:BGA-144_13.0x13.0mm_Layout12x12_P1.0mm").unwrap();
        assert_eq!(fp.pads.len(), 144);
        let at = |n: &str| fp.pads_numbered(n).next().unwrap().at;
        assert!(near(at("A1").x, -5.5) && near(at("A1").y, -5.5));
        // Row letters: A B C D E F G H J K L M; the twelfth row is M.
        assert!(near(at("M12").x, 5.5) && near(at("M12").y, 5.5));
        assert!(fp.pads_numbered("I1").next().is_none());
        assert_eq!(fp.pads[0].shape, PadShape::Circle);
        assert!(near(fp.pads[0].width, 0.5));
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-7.5, -7.5), Pt::new(7.5, 7.5))
        );
        // After Y the rows continue AA, AB, ...
        let big = generate("Package_BGA:BGA-1156_35.0x35.0mm_Layout34x34_P1.0mm").unwrap();
        assert!(big.pads_numbered("AA1").next().is_some());
        assert!(big.pads_numbered("AN34").next().is_some());
        assert!(big.pads_numbered("AO1").next().is_none());
    }

    #[test]
    fn a_dpak_has_its_tab_as_one_pad_with_four_paste_openings_of_the_same_number() {
        let fp = generate("Package_TO_SOT_SMD:TO-252-3_TabPin2").unwrap();
        // Three leads, and the tab: pad 2 is the middle lead and the tab, which is a copper
        // pad with the mask opening and four copper pads that carry the paste.
        assert_eq!(fp.pad_numbers(), ["1", "2", "3"]);
        assert_eq!(fp.pads_numbered("2").count(), 1 + 1 + 4);
        let tab = fp
            .pads_numbered("2")
            .find(|p| p.layers == ["F.Cu", "F.Mask"])
            .unwrap();
        assert!(near(tab.at.x, 1.26) && near(tab.width, 6.4) && near(tab.height, 5.8));
        let paste: Vec<_> = fp
            .pads_numbered("2")
            .filter(|p| p.layers == ["F.Cu", "F.Paste"])
            .collect();
        assert_eq!(paste.len(), 4);
        assert!(
            paste
                .iter()
                .all(|p| near(p.width, 3.05) && near(p.height, 2.75))
        );
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-6.39, -3.5), Pt::new(4.71, 3.5))
        );
        // The same tab on a package with the tab as its fourth pin, or its sixth.
        assert_eq!(
            generate("Package_TO_SOT_SMD:TO-252-5_TabPin6")
                .unwrap()
                .pads_numbered("6")
                .count(),
            5
        );
        let four_pin = generate("Package_TO_SOT_SMD:TO-252-4").unwrap();
        let mut four = four_pin.pad_numbers();
        four.sort();
        assert_eq!(four, ["1", "2", "3", "4", "5"]);
    }

    #[test]
    fn a_d2pak_has_the_dpak_structure_on_the_pg_to263_pads() {
        let fp = generate("Package_TO_SOT_SMD:TO-263-5_TabPin3").unwrap();
        // Five leads, the third sharing its number with the tab (copper, and four paste
        // openings of the same number).
        assert_eq!(fp.pads_numbered("3").count(), 1 + 1 + 4);
        let tab = fp
            .pads_numbered("3")
            .find(|p| p.layers == ["F.Cu", "F.Mask"] && p.width > 5.0)
            .unwrap();
        assert!(near(tab.at.x, 1.5) && near(tab.width, 9.4) && near(tab.height, 10.8));
        let lead = fp.pads_numbered("1").next().unwrap();
        assert!(near(lead.at.x, -7.65) && near(lead.at.y, -3.4));
        assert!(near(lead.width, 4.6) && near(lead.height, 1.1));
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-10.2, -5.65), Pt::new(6.45, 5.65))
        );
        // The nine-pin parts' stencil openings are not pads: no number, no copper.
        let nine = generate("Package_TO_SOT_SMD:TO-263-9_TabPin10").unwrap();
        let openings: Vec<_> = nine.pads.iter().filter(|p| p.number.is_empty()).collect();
        assert_eq!(openings.len(), 4);
        assert!(openings.iter().all(|p| p.layers == ["F.Paste"]));
    }

    #[test]
    fn a_sot_89_has_a_tab_pad_that_reaches_beyond_its_anchor() {
        let fp = generate("Package_TO_SOT_SMD:SOT-89-3").unwrap();
        let mut numbers = fp.pad_numbers();
        numbers.sort();
        assert_eq!(numbers, ["1", "2", "3"]);
        let tab = fp.pads_numbered("2").next().unwrap();
        // The anchor is 1.475 mm wide at x = -1.8625; the polygon runs on to x = 2.0.
        assert!(near(tab.width, 1.475) && near(tab.at.x, -1.8625));
        assert!(near(tab.bounds().max.x, 2.0), "{:?}", tab.bounds());
        assert!(near(tab.bounds().min.x, -2.6), "{:?}", tab.bounds());
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-2.85, -2.5), Pt::new(2.25, 2.5))
        );
    }

    #[test]
    fn a_smd_crystal_has_the_recorded_pads_and_the_courtyard_of_its_drawing() {
        let fp = generate("Crystal:Crystal_SMD_3215-2Pin_3.2x1.5mm").unwrap();
        assert_eq!(fp.pad_numbers(), ["1", "2"]);
        let at = |n: &str| fp.pads_numbered(n).next().unwrap().at;
        // Pad 1 is on the right in this drawing (Epson FC-135).
        assert!(near(at("1").x, 1.25) && near(at("2").x, -1.25));
        assert!(near(fp.pads[0].width, 1.0) && near(fp.pads[0].height, 1.8));
        assert_eq!(fp.pads[0].shape, PadShape::Rect);
        assert_eq!(
            fp.courtyard,
            Rect::new(Pt::new(-2.0, -1.15), Pt::new(2.0, 1.15))
        );
        let four = generate("Crystal:Crystal_SMD_3225-4Pin_3.2x2.5mm").unwrap();
        assert_eq!(four.pads.len(), 4);
    }
}
