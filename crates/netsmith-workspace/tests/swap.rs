//! The editor's swap (issue #800): a generic part that no catalog row stands for, its nearest
//! rows with the board's checks run again with each, and the source edit that chooses one. Every
//! part, number and file here is invented.

use std::fs;

use netsmith_editor::Backend;
use netsmith_editor::api::{SwapApply, SwapCheck};
use netsmith_fabricate::project::{Project, Target};
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

/// A resistor in series with an LED on 5 V.
const LED: &str = include_str!("../../../examples/led.nsm");

/// A resistor only this project has: the board's 249 ohm has no row, so the part is unresolved,
/// and the nearest rows of the standard catalog are its alternatives.
const OWN_CATALOG: &str = "lcsc,mpn,resistance,max_power,package,library,source\n\
     C9999999,ZZR-316,316ohm +/- 1%,250mW,0402,Extended,https://example.invalid/zzr\n";

fn project(label: &str, source: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("catalogs")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"own\"\nseries = \"E96\"\n",
    )
    .unwrap();
    fs::write(root.join("catalogs/resistor.csv"), OWN_CATALOG).unwrap();
    fs::write(root.join("src/main.nsm"), source).unwrap();
    root
}

fn led(resistor: &str) -> String {
    LED.replace(".r1: Resistor {}", resistor)
}

fn open(root: &TempDir) -> Workspace {
    Workspace::open(Target::resolve_from(root, None).unwrap()).unwrap()
}

fn board(editor: &Workspace) -> String {
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    analysis.boards[0].name.clone()
}

fn r1(editor: &Workspace, board: &str) -> String {
    let detail = editor.board(board).unwrap();
    detail
        .parts
        .iter()
        .find(|p| p.path == "r1")
        .unwrap()
        .refdes
        .clone()
}

#[test]
fn the_swap_lists_the_nearest_rows_with_their_checks_and_the_edit_of_each() {
    let root = project(
        "swap-list",
        &led(".r1: Resistor { .resistance &= 249ohm +/- 1% }"),
    );
    let editor = open(&root);
    let board = board(&editor);
    let refdes = r1(&editor, &board);
    let swap = editor.swap(&board, &refdes).unwrap();
    assert_eq!(swap.path, "r1");
    assert_eq!(swap.member.as_deref(), Some("resistance"));
    assert_eq!(swap.refused, None);
    let stated = swap.stated.as_ref().expect("stated once");
    assert_eq!(stated.file, "src/main.nsm");
    let names: Vec<&str> = swap.alternatives.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "240Ω 1% 0402 C25094",
            "270Ω 1% 0402 C25099",
            "220Ω 1% 0402 C25091"
        ]
    );
    assert!(
        swap.message.contains("Nearest in the catalog") && !swap.message.contains("passes every check"),
        "{}",
        swap.message
    );
    let first = &swap.alternatives[0];
    assert_eq!(first.check, SwapCheck::Passes);
    assert_eq!(first.why, None);
    assert_eq!(first.value.as_deref(), Some("240ohm +/- 1%"));
    let edit = first.edit.as_ref().unwrap();
    assert_eq!(edit.old, "249ohm +/- 1%");
    assert_eq!(edit.new, "240ohm +/- 1%");
    assert_eq!(edit.at.file, "src/main.nsm");
}

#[test]
fn a_part_that_a_row_stands_for_has_nothing_to_swap() {
    let root = project(
        "swap-resolved",
        &led(".r1: Resistor { .resistance &= 316ohm +/- 1% }"),
    );
    let editor = open(&root);
    let board = board(&editor);
    let refdes = r1(&editor, &board);
    let e = editor.swap(&board, &refdes).unwrap_err();
    assert_eq!(e.status, 422, "{}", e.message);
    assert_eq!(e.detail.unwrap().code, "not_unresolved");
    assert_eq!(editor.swap(&board, "R99").unwrap_err().status, 404);
}

#[test]
fn applying_an_alternative_writes_its_value_and_the_part_resolves() {
    let root = project(
        "swap-apply",
        &led(".r1: Resistor { .resistance &= 249ohm +/- 1% }"),
    );
    let editor = open(&root);
    let board = board(&editor);
    let refdes = r1(&editor, &board);
    let swapped = editor
        .swap_apply(&SwapApply {
            board: board.clone(),
            refdes: refdes.clone(),
            alternative: "240Ω 1% 0402 C25094".into(),
        })
        .unwrap();
    assert_eq!(swapped.file, "src/main.nsm");
    assert_eq!(swapped.lock, None, "the project has no lock");
    let text = fs::read_to_string(root.join("src/main.nsm")).unwrap();
    assert_eq!(text, led(".r1: Resistor { .resistance &= 240ohm +/- 1% }"));
    // The next analysis reads the file again: the part has a row now.
    let board = self::board(&editor);
    let bom = editor.bom(&board).unwrap();
    let line = bom
        .lines
        .iter()
        .find(|l| l.designators.contains(&refdes))
        .unwrap();
    assert_eq!(line.unresolved, None);
    assert!(line.lcsc.is_some());
    assert_eq!(
        editor.swap(&board, &refdes).unwrap_err().status,
        422,
        "nothing left to swap"
    );
}

#[test]
fn applying_picks_the_part_again_in_the_pick_lock_and_keeps_the_others() {
    let source = led(".r1: Resistor { .resistance &= 249ohm +/- 1% }");
    let root = project("swap-lock", &source);
    // The lock as `project lock --update` writes it, with the generic part at 249 ohm.
    let project = Project::load(&root).unwrap();
    let catalogs = project.catalogs().unwrap();
    let loader = project.loader().unwrap();
    let main = root.join("src/main.nsm");
    let refresh = netsmith_pipeline::pick_lock::refresh(
        &main,
        "src/main.nsm",
        &source,
        &loader,
        &catalogs,
    )
    .unwrap();
    refresh.write().unwrap();
    let lock_file = root.join("src/main.lock.toml");
    let before = fs::read_to_string(&lock_file).unwrap();
    let editor = open(&root);
    let board = board(&editor);
    let refdes = r1(&editor, &board);
    let swapped = editor
        .swap_apply(&SwapApply {
            board,
            refdes,
            alternative: "270Ω 1% 0402 C25099".into(),
        })
        .unwrap();
    assert_eq!(swapped.lock.as_deref(), Some("main.lock.toml"));
    let after = fs::read_to_string(&lock_file).unwrap();
    assert_ne!(before, after);
    let entry = |text: &str, path: &str| {
        let start = text.find(&format!(".parts.\"{path}\"]")).unwrap();
        text[start..].split("\n\n").next().unwrap().to_string()
    };
    assert!(entry(&after, "r1").contains("270"), "{after}");
    assert_eq!(entry(&before, "led"), entry(&after, "led"));
    assert!(
        fs::read_to_string(&main)
            .unwrap()
            .contains(".resistance &= 270ohm +/- 1%")
    );
}

#[test]
fn a_value_stated_in_a_definition_two_parts_share_is_not_written() {
    let source = r#"import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

module Light {
    .power: Rail
    .r: Resistor { .resistance &= 249ohm +/- 1% }
    .led: Led { .current &= 5mA..15mA }
    .power.pos ~> .r ~> .led ~> .power.neg
}

board Two {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail { .voltage &= 5V +/- 5% }
    .a: Light
    .b: Light
    .supply ~ .a.power
    .supply ~ .b.power
}
"#;
    let root = project("swap-shared", source);
    let editor = open(&root);
    let board = board(&editor);
    let detail = editor.board(&board).unwrap();
    let refdes = detail
        .parts
        .iter()
        .find(|p| p.path == "a.r")
        .unwrap()
        .refdes
        .clone();
    let swap = editor.swap(&board, &refdes).unwrap();
    let why = swap.refused.as_deref().expect("refused");
    assert!(why.contains("1 other part shares"), "{why}");
    assert!(swap.alternatives.iter().all(|a| a.edit.is_none()));
    assert!(!swap.alternatives.is_empty());
    let e = editor
        .swap_apply(&SwapApply {
            board,
            refdes,
            alternative: swap.alternatives[0].name.clone(),
        })
        .unwrap_err();
    assert_eq!(e.status, 422);
    assert_eq!(fs::read_to_string(root.join("src/main.nsm")).unwrap(), source);
}
