//! An order reuses the package a plain `fab` just made when its inputs are unchanged (issue #841),
//! and shows it by a count of the stages it ran. None of it needs KiCad (ADR 0097).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use netsmith_fabricate::fab::{FabOptions, Reuse, fab_with_options};
use netsmith_fabricate::order::{Made, OrderLocks, fab_order};
use netsmith_fabricate::project::Target;
use netsmith_fabricate::reuse::RECORD;
use netsmith_pipeline::Libraries;
use netsmith_testkit::TempDir;

const LED: &str = include_str!("../../../examples/led.nsm");
const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");

const ORDERS: &str = "
[order.small.boards]
LedExample = 10

[order.members.boards]
\"TwoBoards/led\" = 3
";

thread_local! {
    /// The progress lines of the run on this thread.
    static SAID: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn say(line: &str) {
    SAID.with(|s| s.borrow_mut().push(line.to_string()));
}

fn said() -> Vec<String> {
    SAID.with(|s| std::mem::take(&mut *s.borrow_mut()))
}

fn project(name: &str) -> TempDir {
    let root = TempDir::new(&format!("order-reuse-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/led.nsm"), LED).unwrap();
    fs::write(root.join("src/two.nsm"), TWO_BOARDS).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        format!(
            "[project]\nname = \"kit\"\ntop_level = [\"src/led.nsm\", \"src/two.nsm\"]\n{ORDERS}"
        ),
    )
    .unwrap();
    root
}

/// What `netsmith fab` does on every file of the project: one run, with `reuse` as given.
fn plain_fab(root: &Path, reuse: Reuse) {
    let target = Target::resolve_from(root, None).unwrap();
    let loader = target.loader().unwrap();
    let catalogs = target.catalogs().unwrap();
    let libraries = Libraries::for_project(Some(root)).unwrap();
    for source in &target.files {
        let text = fs::read_to_string(&source.path).unwrap();
        fab_with_options(
            &source.name,
            &text,
            &loader,
            &target.fab_out,
            &catalogs,
            &libraries,
            None,
            &FabOptions {
                layout: Some(netsmith_fabricate::layout::layout_path(&source.path)),
                pin_map_source: Some(source.stable_name(target.project.as_ref())),
                reuse: reuse.clone(),
                progress: Some(say),
                ..FabOptions::default()
            },
        )
        .unwrap_or_else(|e| panic!("{e}"));
    }
}

/// The order `name`, made as `netsmith fab --order` makes it, reusing what a plain fab made.
fn order(root: &Path, name: &str, options: FabOptions) -> Made {
    let target = Target::resolve_from(root, None).unwrap();
    let loader = target.loader().unwrap();
    let catalogs = target.catalogs().unwrap();
    let libraries = Libraries::for_project(Some(root)).unwrap();
    let o = target
        .project
        .as_ref()
        .unwrap()
        .manifest
        .orders
        .iter()
        .find(|o| o.name == name)
        .unwrap();
    fab_order(
        o,
        &target,
        &loader,
        &catalogs,
        &libraries,
        None,
        &FabOptions {
            reuse: Reuse::From(target.fab_out.clone()),
            progress: Some(say),
            ..options
        },
        OrderLocks::Held,
        &target.fab_out,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// Every file under `dir`, by its path under it.
fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, into: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, into);
            } else {
                let name = path.strip_prefix(root).unwrap().display().to_string();
                into.insert(name, fs::read(&path).unwrap());
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(dir, dir, &mut files);
    files
}

#[test]
fn an_order_copies_the_package_a_plain_fab_made_and_runs_no_stage() {
    let fresh = project("fresh");
    let reusing = project("reusing");
    plain_fab(&reusing, Reuse::Record);
    assert!(reusing.join("fab/LedExample").join(RECORD).is_file());
    said();

    let made = order(&reusing, "small", FabOptions::default());
    let lines = said();
    assert_eq!(made.reused(), 1, "{lines:?}");
    assert_eq!(made.stages_run(), 0);
    assert_eq!(made.reuse_line(), "reused 1 of 1 boards from a plain fab; 0 stages run");
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("LedExample: reused the package a plain fab made")),
        "{lines:?}"
    );

    // The same order with no plain fab before it makes the board, and runs its stages.
    let from_scratch = order(&fresh, "small", FabOptions::default());
    assert_eq!(from_scratch.reused(), 0);
    assert!(from_scratch.stages_run() > 0);

    // The package is the one the order would have made, file for file, and the summary says the
    // same of it. The record is not part of it.
    let copied = tree(&reusing.join("fab/orders/small"));
    assert_eq!(copied, tree(&fresh.join("fab/orders/small")));
    assert!(copied.keys().all(|name| !name.ends_with(RECORD)));
    assert_eq!(made.summary, from_scratch.summary);
    assert_eq!(made.bom, from_scratch.bom);
    let (_, reused) = &made.boards[0];
    let (_, built) = &from_scratch.boards[0];
    assert_eq!(reused.facts, built.facts);
    assert_eq!(reused.notes, built.notes);
    assert_eq!(reused.gerber_files, built.gerber_files);
}

#[test]
fn a_changed_source_makes_the_board_again() {
    let root = project("source");
    plain_fab(&root, Reuse::Record);
    fs::write(
        root.join("src/led.nsm"),
        LED.replace("5V +/- 5%", "9V +/- 5%"),
    )
    .unwrap();
    said();
    let made = order(&root, "small", FabOptions::default());
    assert_eq!(made.reused(), 0);
    assert!(made.stages_run() > 0);
    assert!(
        said().iter().any(|l| l.starts_with("LedExample: made again, not reused: the source, the data, the options or netsmith itself changed")),
    );
}

#[test]
fn an_edit_that_leaves_the_design_as_it_was_does_not_make_the_board_again() {
    // The package is of the design and the files beside it, not of the text of the source: a
    // comment at the end of a line moves nothing a package holds (a line that moves does, for the
    // design says where each statement is made).
    let root = project("comment");
    plain_fab(&root, Reuse::Record);
    fs::write(
        root.join("src/led.nsm"),
        LED.replacen("board LedExample {", "board LedExample { // a note nobody reads", 1),
    )
    .unwrap();
    said();
    let made = order(&root, "small", FabOptions::default());
    assert_eq!(made.reused(), 1, "{:?}", said());
    assert_eq!(made.stages_run(), 0);
}

#[test]
fn an_option_that_changes_the_package_makes_the_board_again() {
    let root = project("option");
    plain_fab(&root, Reuse::Record);
    said();
    let made = order(
        &root,
        "small",
        FabOptions {
            glb: false,
            ..FabOptions::default()
        },
    );
    assert_eq!(made.reused(), 0);
    assert!(made.stages_run() > 0);
}

#[test]
fn a_package_that_was_touched_after_the_fab_is_not_reused() {
    let root = project("touched");
    plain_fab(&root, Reuse::Record);
    let bom = root.join("fab/LedExample/bom.csv");
    let mut text = fs::read_to_string(&bom).unwrap();
    text.push_str("edited by hand\n");
    fs::write(&bom, text).unwrap();
    said();
    let made = order(&root, "small", FabOptions::default());
    assert_eq!(made.reused(), 0);
    assert!(made.stages_run() > 0);
    let lines = said();
    assert!(
        lines.iter().any(|l| l.contains("made again, not reused: the files of the package are not as fab wrote them (bom.csv)")),
        "{lines:?}"
    );

    // A file that is added counts as a change as well.
    let root = project("added");
    plain_fab(&root, Reuse::Record);
    fs::write(root.join("fab/LedExample/notes.txt"), "x").unwrap();
    said();
    assert_eq!(order(&root, "small", FabOptions::default()).reused(), 0);
}

#[test]
fn a_fab_that_left_no_record_is_not_reused() {
    let root = project("norecord");
    plain_fab(&root, Reuse::Off);
    assert!(!root.join("fab/LedExample").join(RECORD).exists());
    said();
    let made = order(&root, "small", FabOptions::default());
    assert_eq!(made.reused(), 0);
    assert!(made.stages_run() > 0);
    assert!(said().iter().any(|l| l.contains("made again, not reused: no record of a plain fab")));
}

#[test]
fn a_board_of_a_system_is_always_made_again() {
    let root = project("system");
    plain_fab(&root, Reuse::Record);
    assert!(!root.join("fab/TwoBoards/led").join(RECORD).exists());
    said();
    let made = order(&root, "members", FabOptions::default());
    assert_eq!(made.reused(), 0);
    assert!(made.stages_run() > 0);
}
