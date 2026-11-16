//! A part's datasheet guidelines in the editor (issue #994): the Summary tab lists what each
//! measures, and the findings panel has a `layout` warning, with both parts named, for one the
//! board does not meet, unless the layout file waives it. The regulator, its datasheet and its
//! generated footprint are invented, as in `netsmith-fabricate`'s `part_layout.rs`.

use std::fs;

use netsmith_editor::Backend;
use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

const BOARD: &str = "Supply";

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

/// The project, with the regulator and its capacitor locked 20 mm apart and the layout file's
/// `more` lines after them.
fn project(name: &str, more: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-guidelines-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"supply\"\n").unwrap();
    fs::write(
        root.join("kicad.toml"),
        "[[part]]\nkind = \"Ld9\"\npackage = \"SOT-23\"\ndefault = true\nfootprint = \"gen:SOT23(pins=3)\"\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), SUPPLY).unwrap();
    fs::write(
        root.join("src/main.pcb.toml"),
        format!(
            "version = 1\n\n[Supply.parts.reg]\nat = [10.0, 12.0]\nlock = true\n\n[Supply.parts.cin]\nat = [30.0, 12.0]\nlock = true\n{more}"
        ),
    )
    .unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

#[test]
fn the_summary_and_the_findings_show_a_guideline_the_board_does_not_meet() {
    let (_root, editor) = project("unmet", "");
    let summary = editor.pcb_summary(BOARD, &[], 2.0, &[]).unwrap();
    // The feed header has two pads, one on VIN and one on GND, so `bridging` finds it too.
    let others: Vec<Option<&str>> = summary.guidelines.iter().map(|g| g.other.as_deref()).collect();
    assert_eq!(others, [Some("C1"), Some("J1")]);
    let g = &summary.guidelines[0];
    assert_eq!((g.part.as_str(), g.name.as_str()), ("reg", "input_cap"));
    assert_eq!(g.statement, "near");
    assert_eq!(g.other.as_deref(), Some("C1"));
    assert_eq!((g.at_most, g.met), (Some(2.0), Some(false)));
    assert!(g.measured.is_some_and(|m| m > 15.0), "{g:?}");
    let findings = editor.pcb_findings(BOARD).unwrap();
    let layout: Vec<_> = findings
        .findings
        .iter()
        .filter(|f| f.rule == "layout")
        .collect();
    let f = layout
        .iter()
        .find(|f| f.parts == ["U1", "C1"])
        .unwrap_or_else(|| panic!("{:?}", findings.findings));
    assert!(
        f.message
            .starts_with("`reg` (U1) asks for `input_cap` within 2 mm of its pin `.vin.pos`"),
        "{}",
        f.message
    );
}

#[test]
fn a_waived_guideline_is_no_finding() {
    let (_root, editor) = project(
        "waived",
        "\n[Supply.waive.\"reg.input_cap\"]\nreason = \"the capacitor is on the next board\"\n",
    );
    let findings = editor.pcb_findings(BOARD).unwrap();
    assert!(
        findings.findings.iter().all(|f| f.rule != "layout"),
        "{:?}",
        findings.findings
    );
}
