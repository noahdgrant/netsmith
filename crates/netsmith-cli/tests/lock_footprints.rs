//! The pick lock records each part's footprint and a hash of its geometry (issue #1683, ADR
//! 0145): a pad resized in a project library is told by `project lock --check`, a difference of
//! `build` (an error with `--locked`), and taken by `project lock --update`; a silkscreen edit is
//! not; a lock with no footprints (as 0.9.0 wrote) is not current once.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

const LED: &str = include_str!("../../../examples/led.nsm");

/// The board's resistor is drawn with the project's own footprint.
const KICAD_TOML: &str = "\
[[library]]
name = \"own\"
footprints = \"libs/own.pretty\"

[[member]]
path = \"r1\"
footprint = \"own:R_0402\"
";

const FOOTPRINT: &str = r#"(footprint "R_0402" (version 20241229) (generator "pcbnew") (layer "F.Cu")
  (property "Reference" "REF**" (at 0 -1.2 0) (layer "F.SilkS") (effects (font (size 1 1) (thickness 0.15))))
  (property "Value" "R_0402" (at 0 1.2 0) (layer "F.Fab") (effects (font (size 1 1) (thickness 0.15))))
  (attr smd)
  (fp_line (start -0.15 -0.38) (end 0.15 -0.38) (stroke (width 0.12) (type solid)) (layer "F.SilkS"))
  (fp_rect (start -0.93 -0.47) (end 0.93 0.47) (stroke (width 0.05) (type solid)) (fill no) (layer "F.CrtYd"))
  (pad "1" smd roundrect (at -0.51 0) (size 0.54 0.64) (layers "F.Cu" "F.Mask" "F.Paste") (roundrect_rratio 0.25))
  (pad "2" smd roundrect (at 0.51 0) (size 0.54 0.64) (layers "F.Cu" "F.Mask" "F.Paste") (roundrect_rratio 0.25))
)
"#;

fn project(label: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("libs/own.pretty")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"led\"\n").unwrap();
    fs::write(root.join("kicad.toml"), KICAD_TOML).unwrap();
    fs::write(root.join("src/main.nsm"), LED).unwrap();
    write_footprint(&root, FOOTPRINT);
    root
}

fn write_footprint(root: &Path, text: &str) {
    fs::write(root.join("libs/own.pretty/R_0402.kicad_mod"), text).unwrap();
}

/// netsmith with no KiCad: the footprints are the project's and the generators'.
fn netsmith(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("NETSMITH_NO_KICAD", "1")
        .output()
        .unwrap()
}

fn text(run: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    )
}

fn read_lock(root: &Path) -> String {
    fs::read_to_string(root.join("src/main.lock.toml")).unwrap()
}

/// The lock's line for `r1`'s footprint.
fn r1_line(lock: &str) -> String {
    lock.lines()
        .find(|l| l.starts_with("\"r1\" = { footprint = "))
        .unwrap_or_else(|| panic!("no footprint of r1 in\n{lock}"))
        .to_string()
}

#[test]
fn a_resized_pad_is_told_refused_with_locked_and_taken_by_update() {
    let root = project("lock-footprints-resize");
    let first = netsmith(&root, &["board", "build"]);
    assert!(first.status.success(), "{}", text(&first));
    let lock = read_lock(&root);
    assert!(lock.contains("\n[LedExample.footprints]\n"), "{lock}");
    let before = r1_line(&lock);
    assert!(before.contains("footprint = \"own:R_0402\", hash = \""), "{before}");
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert!(check.status.success(), "{}", text(&check));

    // A pad resized, its centre where it was.
    write_footprint(
        &root,
        &FOOTPRINT.replacen("(size 0.54 0.64)", "(size 0.60 0.70)", 1),
    );
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert_eq!(check.status.code(), Some(1), "{}", text(&check));
    assert!(
        text(&check).contains("LedExample: `r1`: footprint changed: `own:R_0402` (hash "),
        "{}",
        text(&check)
    );
    // `build` tells it and leaves the lock; `--locked` refuses it.
    let build = netsmith(&root, &["board", "build"]);
    assert!(build.status.success(), "{}", text(&build));
    assert!(
        text(&build).contains("LedExample: `r1`: footprint changed: `own:R_0402` is not the one the lock records"),
        "{}",
        text(&build)
    );
    assert_eq!(r1_line(&read_lock(&root)), before);
    let locked = netsmith(&root, &["board", "build", "--locked"]);
    assert!(!locked.status.success(), "{}", text(&locked));
    assert!(text(&locked).contains("footprint changed"), "{}", text(&locked));

    // `--update` takes it, and the lock is current again.
    let update = netsmith(&root, &["project", "lock", "--update"]);
    assert!(update.status.success(), "{}", text(&update));
    let after = r1_line(&read_lock(&root));
    assert_ne!(after, before);
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert!(check.status.success(), "{}", text(&check));
    let locked = netsmith(&root, &["board", "build", "--locked"]);
    assert!(locked.status.success(), "{}", text(&locked));
}

#[test]
fn a_silkscreen_or_text_edit_is_not_a_change() {
    let root = project("lock-footprints-silk");
    let first = netsmith(&root, &["board", "build"]);
    assert!(first.status.success(), "{}", text(&first));
    let before = read_lock(&root);
    write_footprint(
        &root,
        &FOOTPRINT
            .replace("(start -0.15 -0.38) (end 0.15 -0.38)", "(start -0.3 -0.4) (end 0.3 -0.4)")
            .replace("(at 0 1.2 0) (layer \"F.Fab\")", "(at 0 1.5 0) (layer \"F.Fab\")")
            .replace("(generator \"pcbnew\")", "(generator \"other\")"),
    );
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert!(check.status.success(), "{}", text(&check));
    let locked = netsmith(&root, &["board", "build", "--locked"]);
    assert!(locked.status.success(), "{}", text(&locked));
    assert_eq!(read_lock(&root), before);
}

#[test]
fn a_lock_with_no_footprints_is_not_current_once() {
    let root = project("lock-footprints-old");
    let first = netsmith(&root, &["board", "build"]);
    assert!(first.status.success(), "{}", text(&first));
    // The lock as 0.9.0 wrote it: no footprints table.
    let lock = read_lock(&root);
    let at = lock.find("\n[LedExample.footprints]").unwrap();
    fs::write(root.join("src/main.lock.toml"), &lock[..at + 1]).unwrap();
    // `build --locked` accepts it, as before.
    let locked = netsmith(&root, &["board", "build", "--locked"]);
    assert!(locked.status.success(), "{}", text(&locked));
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert_eq!(check.status.code(), Some(1), "{}", text(&check));
    assert!(
        text(&check).contains("LedExample: footprints recorded for 3 parts, as they are built now"),
        "{}",
        text(&check)
    );
    let update = netsmith(&root, &["project", "lock", "--update"]);
    assert!(update.status.success(), "{}", text(&update));
    assert_eq!(read_lock(&root), lock);
    let check = netsmith(&root, &["project", "lock", "--check"]);
    assert!(check.status.success(), "{}", text(&check));
}
