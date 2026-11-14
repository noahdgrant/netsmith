//! A bank of capacitors (ADR 0052) from the source to the bill of materials and the files
//! `fab` writes: the row once, with its quantity, and one real part for each capacitor in the
//! schematic, the board and the placement file. The values here are invented.

use netsmith_testkit::TempDir;
use std::fs;

use netsmith_pipeline::Libraries;
use netsmith_workspace::Workspace;
use netsmith_fabricate::fab::{FabOptions, fab_with_options};
use netsmith_fabricate::project::{Project, Target};
use netsmith_fabricate::refresh::Network;
use netsmith_check::Loader;
use netsmith_editor::Backend;
use netsmith_fab::KicadCliExe;

/// A 5 V rail with a bank across it: 60 uF effective, which no single capacitor keeps at 5 V.
const BOARD: &str = r#"import std::parts::passive::CapacitorBank
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Banked {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .bulk: CapacitorBank { .count &= 1..8 }
    .bulk.count * .bulk.effective_capacitance >= 60uF
    .bulk.max_voltage >= 6V
    .supply.pos ~> .bulk ~> .supply.neg
}
"#;

fn project(name: &str) -> TempDir {
    let root = TempDir::new(&format!("bank-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"banked\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), BOARD).unwrap();
    root
}

#[test]
fn the_bill_of_materials_lists_the_row_once_with_the_quantity_of_the_bank() {
    let root = project("bom");
    let editor = Workspace::open(Target::resolve_from(root.path(), None).unwrap())
        .unwrap()
        .with_network(Network {
            now: Some(1_793_620_800),
            offline: true,
            cache_dir: None,
            ..Network::default()
        });
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    let bom = editor.bom(&analysis.boards[0].name).unwrap();
    let caps: Vec<_> = bom.lines.iter().filter(|l| l.kind == "Capacitor").collect();
    assert_eq!(caps.len(), 1, "{:?}", bom.lines);
    let line = caps[0];
    assert!(line.quantity >= 2, "{}", line.quantity);
    let expected: Vec<String> = (1..=line.quantity).map(|i| format!("C{i}")).collect();
    assert_eq!(line.designators, expected);
    assert!(line.lcsc.is_some());
}

#[test]
fn fab_writes_every_capacitor_of_the_bank_and_passes_its_own_design_rules_check() {
    let Some(cli) = KicadCliExe::locate_runnable() else {
        eprintln!("skipped: no KiCad");
        return;
    };
    let root = project("fab");
    let project = Project::load(root.path()).unwrap();
    let catalogs = project.catalogs().unwrap();
    let out = TempDir::unused("bank-fab-out");
    let made = fab_with_options(
        "main.nsm",
        BOARD,
        &Loader::std(),
        &out.path().join("fab"),
        &catalogs,
        &Libraries::standard(),
        Some(&cli),
        &FabOptions::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(made.len(), 1);
    let dir = out.path().join("fab/Banked");
    let bom = fs::read_to_string(dir.join("bom.csv")).unwrap();
    let lines: Vec<&str> = bom.lines().skip(1).collect();
    assert_eq!(lines.len(), 1, "{bom}");
    let designators = lines[0].split('"').nth(1).unwrap();
    let n = designators.split(',').count();
    assert!(n >= 2, "{bom}");
    assert_eq!(
        designators,
        (1..=n)
            .map(|i| format!("C{i}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    let board = fs::read_to_string(dir.join("Banked.kicad_pcb")).unwrap();
    for i in 1..=n {
        assert!(
            board.contains(&format!("\"C{i}\"")),
            "no C{i} in the board file"
        );
    }
}

/// The same board with the case drawn in its physical file: every capacitor of the bank must
/// have its top edge 3 to 6 mm from the top of the outline.
const PHYSICAL: &str = "\
import std::pcb::Outline

physical for Banked {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
    }
    .bulk.place.top &= 3mm..6mm
}
";

#[test]
fn a_requirement_on_the_banks_part_is_checked_for_every_capacitor_and_names_the_one_it_is_about() {
    let Some(cli) = KicadCliExe::locate_runnable() else {
        eprintln!("skipped: no KiCad");
        return;
    };
    let root = project("physical");
    let project = Project::load(root.path()).unwrap();
    let catalogs = project.catalogs().unwrap();
    let out = TempDir::unused("bank-physical-out");
    let run = |layout: Option<&str>| {
        let layout_file = out.join("source").join("banked.pcb.toml");
        if let Some(text) = layout {
            fs::create_dir_all(layout_file.parent().unwrap()).unwrap();
            fs::write(&layout_file, text).unwrap();
        }
        fab_with_options(
            "main.nsm",
            BOARD,
            &Loader::std().with_physical("main.nsm", "main.physical.nsm", PHYSICAL),
            &out.path().join("fab"),
            &catalogs,
            &Libraries::standard(),
            Some(&cli),
            &FabOptions {
                layout: layout.map(|_| layout_file),
                ..FabOptions::default()
            },
        )
    };
    // The placer keeps every capacitor to it, and the check measures one requirement for
    // each: as many as the bill of materials has capacitors.
    let made = run(None).unwrap_or_else(|e| panic!("{e}"));
    let notes = &made[0].notes;
    let bom = fs::read_to_string(out.path().join("fab/Banked/bom.csv")).unwrap();
    let designators = bom.lines().nth(1).unwrap().split('"').nth(1).unwrap();
    let n = designators.split(',').count();
    assert!(n >= 2, "{bom}");
    assert!(
        notes.contains(&format!("constraints: {n} of {n} satisfied")),
        "{notes}"
    );
    // The second capacitor moved by hand out of its window: the finding is about it, by its
    // own path and designator, and the first capacitor is not named. A bank's capacitors are
    // `bulk[0]`, `bulk[1]`, counted from 0 as the members of `N x Kind` are (issue #685).
    let e = run(Some(
        "version = 1\n[Banked.parts.\"bulk[1]\"]\nat = [30.0, 15.0]\n",
    ))
    .expect_err("the second capacitor is out of its window");
    assert!(
        e.contains("constraint bulk[1].place.top") && e.contains("body of C2"),
        "{e}"
    );
    assert!(!e.contains("constraint bulk[0].place.top"), "{e}");
    // The old form is refused with the new one named.
    let e = run(Some(
        "version = 1\n[Banked.parts.\"bulk#2\"]\nat = [30.0, 15.0]\n",
    ))
    .expect_err("`bulk#2` is no part any more");
    assert!(e.contains("`bulk#2` is now `bulk[1]`"), "{e}");
}
