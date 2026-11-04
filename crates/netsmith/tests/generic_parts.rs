//! Generic capacitors, inductors and LEDs in one design, from the solver to the bill of
//! materials and `fab` (ADR 0047, provisional). Every part and number here is invented or
//! stated by the design itself; no test reaches a supplier.

use netsmith_kicad::testlib::TempDir;
use std::fs;

use netsmith::Libraries;
use netsmith::editor::Workspace;
use netsmith::fab::{FabOptions, fab_with_options};
use netsmith::project::{Project, Target};
use netsmith::refresh::Network;
use netsmith_check::Loader;
use netsmith_editor::Backend;
use netsmith_editor::api::Bom;
use netsmith_fab::KicadCliExe;

/// A capacitor of 130 nF (E24 has it, no standard row does), an inductor of 3.3 uH with
/// 15 mA to carry, and a red LED whose forward voltage is lower than any row's.
const MIXED: &str = r#"import std::parts::passive::{Capacitor, Inductor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Mixed {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .c: Capacitor {
        .capacitance &= 130nF +/- 10%
        .max_voltage >= .bias * 1.5
    }
    .l: Inductor {
        .inductance &= 3.3uH +/- 20%
    }
    .led: Led {
        .color &= "RED"
        .forward_voltage &= 1.5V..1.7V
        .current &= 5mA..15mA
    }
    .supply.pos ~ .c.a
    .supply.neg ~ .c.b
    .supply.pos ~> .l ~> .led ~> .supply.neg
}
"#;

fn project(name: &str) -> TempDir {
    let root = TempDir::new(&format!("generic-parts-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"mixed\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), MIXED).unwrap();
    root
}

fn open(root: &TempDir) -> Workspace {
    Workspace::open(Target::resolve_from(root.path(), None).unwrap())
        .unwrap()
        .with_network(Network {
            now: Some(1_793_620_800),
            offline: true,
            cache_dir: None,
            ..Network::default()
        })
}

fn bom_of(editor: &Workspace) -> Bom {
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    editor.bom(&analysis.boards[0].name).unwrap()
}

#[test]
fn a_generic_capacitor_inductor_and_led_are_each_listed_unresolved_by_name() {
    let root = project("bom");
    let bom = bom_of(&open(&root));
    let line = |kind: &str| {
        bom.lines
            .iter()
            .find(|l| l.kind == kind)
            .unwrap_or_else(|| panic!("no {kind} line in {:?}", bom.lines))
    };

    let c = line("Capacitor");
    assert_eq!((c.lcsc.as_deref(), c.value.as_str()), (None, "130n"));
    let why = c.unresolved.as_deref().expect("an unresolved capacitor");
    assert!(
        why.starts_with("unresolved: no row of the `capacitor` catalog can be ordered for 130 nF 10% 0402, rated voltage at least 7.875 V"),
        "{why}"
    );
    assert!(why.contains("needs a recorded DC-bias curve"), "{why}");
    assert!(why.contains("Nearest in the catalog:"), "{why}");

    let l = line("Inductor");
    let why = l.unresolved.as_deref().expect("an unresolved inductor");
    assert!(
        why.starts_with("unresolved: no row of the `inductor` catalog can be ordered for 3.3 uH 20% 0402, saturation current at least 15 mA"),
        "{why}"
    );

    let d = line("Led");
    let why = d.unresolved.as_deref().expect("an unresolved LED");
    assert!(
        why.starts_with("unresolved: no row of the `led` catalog can be ordered for RED LED 0402, forward voltage 1.5 V..1.7 V."),
        "{why}"
    );
    assert!(bom.total_usd.is_none());
}

#[test]
fn fab_refuses_each_unresolved_generic_part_by_name() {
    if KicadCliExe::locate().is_none() {
        eprintln!("skipped: no KiCad to read the footprints from");
        return;
    }
    let root = project("fab");
    let project = Project::load(root.path()).unwrap();
    let source = fs::read_to_string(root.join("src/main.nsm")).unwrap();
    let catalogs = project.catalogs().unwrap();
    let out = TempDir::unused("generic-parts-fab-out");
    let e = fab_with_options(
        "main.nsm",
        &source,
        &Loader::std(),
        &out.path().join("fab"),
        &catalogs,
        &Libraries::standard(),
        None,
        &FabOptions::default(),
    )
    .map(|_| ())
    .unwrap_err();
    for refdes in ["C1", "L1", "D1"] {
        assert!(e.contains(&format!("{refdes}: unresolved")), "{refdes}: {e}");
    }
    assert!(e.contains("DC-bias curve"), "{e}");
}
