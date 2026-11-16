//! The `layout` guideline end to end (ADR 0030, issues #829 and #994): `fab` of a board whose
//! regulator's datasheet asks for its input capacitor within 2 mm of its IN pin, with the
//! capacitor placed 20 mm away by the layout file. It warns, stops with strict guidelines, and
//! says the guideline is waived when the layout file waives it with a reason. The part, its
//! datasheet and its footprint (netsmith's generated SOT-23, so no KiCad is needed) are
//! invented.

use std::fs;

use netsmith_fabricate::fab::{FabOptions, Fabricated, fab_with_options};
use netsmith_kicad::Search;
use netsmith_pipeline::Libraries;
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

const SUPPLY: &str = r#"import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::passive::Capacitor
import std::sources::Rail

part Ld9 {
    .designator &= "U"
    .vin: Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    pins {
        .vin.pos  power "IN"  "3"
        .vin.neg  power "GND" "1"
        .vout.pos power "OUT" "2"
    }
    layout {
        source "LD9 datasheet, 9.1 Layout Guidelines, p.12"
        /// "Place the input capacitor within 2 mm of the IN pin."
        near input_cap bridging .vin.pos .vin.neg to .vin.pos within 2mm
    }
}

board Supply {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .source: Rail {
        .voltage &= 5V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .feed: Header1x2
    .load: Header1x2
    .reg: Ld9 {
        .assembly &= AssemblyMode::Hand
    }
    .cin: Capacitor { .capacitance &= 1uF +/- 20% }
    .feed.p1 ~ .source.pos
    .feed.p2 ~ .source.neg
    .source ~ .reg.vin
    .load.p1 ~ .reg.vout.pos
    .load.p2 ~ .reg.vin.neg
    .reg.vin.pos ~> .cin ~> .reg.vin.neg
}
"#;

const PHYSICAL: &str = "\
import std::pcb::Outline

physical for Supply {
    case {
        .outline: Outline {
            .width &= 50mm
            .height &= 25mm
        }
    }
}
";

/// The regulator's footprint, generated: the project's own row for its kind.
const KICAD: &str = "\
[[part]]
kind = \"Ld9\"
package = \"SOT-23\"
default = true
footprint = \"gen:SOT23(pins=3)\"
";

/// `fab` of [`SUPPLY`] with the regulator and the capacitor 20 mm apart, the layout file's
/// `more` lines after the parts, and strict guidelines if `strict`.
fn run(name: &str, more: &str, strict: bool) -> (TempDir, Result<Vec<Fabricated>, String>) {
    let dir = TempDir::unused(&format!("part-layout-{name}"));
    let project = dir.join("source");
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("netsmith.toml"),
        "[project]\nname = \"supply\"\ntop_level = [\"supply.nsm\"]\n",
    )
    .unwrap();
    fs::write(project.join("kicad.toml"), KICAD).unwrap();
    let layout = project.join("supply.pcb.toml");
    fs::write(
        &layout,
        format!(
            "version = 1\n\n[Supply.parts.reg]\nat = [10.0, 12.0]\nlock = true\n\n[Supply.parts.cin]\nat = [30.0, 12.0]\nlock = true\n{more}"
        ),
    )
    .unwrap();
    let empty = dir.join("no-kicad");
    fs::create_dir_all(&empty).unwrap();
    let libraries =
        Libraries::for_project_by(Some(&project), &Search::under(&empty).with_no_kicad()).unwrap();
    let made = fab_with_options(
        "supply.nsm",
        SUPPLY,
        &netsmith_check::Loader::std().with_physical("supply.nsm", "supply.physical.nsm", PHYSICAL),
        &dir.join("fab"),
        &Catalogs::jlcpcb().with_strict_guidelines(strict),
        &libraries,
        None,
        &FabOptions {
            layout: Some(layout),
            ..FabOptions::default()
        },
    );
    (dir, made)
}

/// The line of `notes` that names the guideline.
fn line<'a>(notes: &'a str, needle: &str) -> &'a str {
    notes
        .lines()
        .find(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("no {needle:?} in:\n{notes}"))
}

#[test]
fn a_capacitor_further_than_the_datasheet_allows_is_a_warning() {
    let (_dir, made) = run("far", "", false);
    let made = made.expect("a guideline does not stop fab");
    let notes = &made[0].notes;
    let l = line(notes, "layout guideline");
    assert!(
        l.starts_with("warning: Supply: layout guideline: `reg` (U1) asks for `input_cap` within 2 mm of its pin `.vin.pos` (pads 3), and C1 (`cin`) is ")
            && l.contains("(LD9 datasheet, 9.1 Layout Guidelines, p.12; supply.nsm:")
            && l.ends_with(": move C1 nearer U1, or turn it so that its pad faces the pin"),
        "{l}"
    );
}

#[test]
fn with_strict_guidelines_the_capacitor_stops_fab() {
    let (dir, made) = run("strict", "", true);
    let err = made.expect_err("strict guidelines refuse it");
    assert!(
        err.contains("error: Supply: layout guideline: `reg` (U1) asks for `input_cap` within 2 mm")
            && err.contains("this is an error because guidelines are strict"),
        "{err}"
    );
    assert!(
        !dir.join("fab").join("Supply").join("fab-notes.txt").exists(),
        "no package is written"
    );
}

#[test]
fn a_waiver_in_the_layout_file_says_why_and_neither_warns_nor_stops() {
    let waiver = "\n[Supply.waive.\"reg.input_cap\"]\nreason = \"the capacitor sits under IN on the bottom of the next board\"\n";
    let (_dir, made) = run("waived", waiver, true);
    let made = made.expect("a waived guideline does not stop fab, strict or not");
    let notes = &made[0].notes;
    assert!(!notes.contains("layout guideline"), "{notes}");
    let l = line(notes, "is waived by the layout file");
    assert!(
        l.contains("`reg.input_cap` is waived by the layout file (the capacitor sits under IN on the bottom of the next board): `reg` (U1) asks for `input_cap` within 2 mm"),
        "{l}"
    );
}

#[test]
fn a_waiver_of_a_guideline_the_board_does_not_have_is_an_error() {
    let waiver = "\n[Supply.waive.\"reg.output_cap\"]\nreason = \"no such guideline\"\n";
    let (_dir, made) = run("stale", waiver, false);
    let err = made.expect_err("a stale waiver is refused");
    assert!(
        err.contains("the board `Supply` has no guideline `reg.output_cap` (its guidelines: `reg.input_cap`)"),
        "{err}"
    );
}

#[test]
fn a_waiver_gives_its_reason() {
    let waiver = "\n[Supply.waive.\"reg.input_cap\"]\nreason = \"  \"\n";
    let (_dir, made) = run("no-reason", waiver, false);
    let err = made.expect_err("a waiver with no reason is refused");
    assert!(
        err.contains("the waiver of the guideline `reg.input_cap` gives no reason"),
        "{err}"
    );
}
