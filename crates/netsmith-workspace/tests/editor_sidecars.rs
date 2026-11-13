//! What the editor shows of the two files beside a source file, read only: the pin map of a part
//! (ADR 0101) and the state of the pick lock (ADR 0100). Reading writes neither file; the pin file's
//! `lock`, `pin` and `source` lines are written by the calls `editor_pin_write.rs` tests.

use std::fs;
use std::path::Path;

use netsmith_workspace::Workspace;
use netsmith_fabricate::project::Target;
use netsmith_editor::{
    Backend,
    api::{PickLockInfo, PinRequestInfo},
};
use netsmith_testkit::TempDir;

/// An invented microcontroller whose pins offer `GPIO`, two of them `ANALOG` as well.
const MCU: &str = r#"
/// Zmcu4, an invented microcontroller.
part Zmcu4 {
    pins {
        .p00 bidirectional "P0.00" "3" fn GPIO
        .p01 bidirectional "P0.01" "4" fn GPIO
        .p02 bidirectional "P0.02" "5" fn GPIO ANALOG
        .p03 bidirectional "P0.03" "6" fn GPIO ANALOG
    }
}
"#;

fn board(requests: &str) -> String {
    format!(
        "import std::parts::passive::Resistor\n{MCU}\nboard Sensors {{\n    .u: Zmcu4\n    .load: Resistor\n{requests}\n}}\n"
    )
}

const GOOD: &str =
    "    .status: .u.GPIO\n    .vbat: .u.ANALOG\n    .status ~ .load.a\n    .vbat ~ .load.b\n";

fn project(label: &str, source: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"pins\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), source).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    (root, editor)
}

fn pin_map(editor: &Workspace) -> Vec<PinRequestInfo> {
    let detail = editor.board("Sensors").unwrap();
    let part = detail.parts.iter().find(|p| p.path == "u").unwrap();
    part.pin_requests.clone().unwrap_or_default()
}

fn pins_file(root: &Path) -> std::path::PathBuf {
    root.join("src/main.pins.toml")
}

#[test]
fn a_request_with_no_file_got_a_pin_this_run_and_nothing_is_written() {
    let (root, editor) = project("sidecars-pins-run", &board(GOOD));
    let map = pin_map(&editor);
    assert_eq!(map.len(), 2, "{map:?}");
    // In the byte order of the paths: `status` before `vbat`.
    let status = &map[0];
    assert_eq!(status.request, "status");
    assert_eq!(status.function, "GPIO");
    assert_eq!(status.pin.as_deref(), Some("p00"));
    // The pad is the package's: `P0.00` is pad 3.
    assert_eq!(status.pad.as_deref(), Some("3"));
    assert!(!status.locked);
    assert_eq!(status.entry.as_deref(), Some("auto"));
    assert_eq!(status.from.as_deref(), Some("run"));
    assert_eq!(status.message, None);
    let vbat = &map[1];
    assert_eq!(vbat.request, "vbat");
    assert_eq!(
        (vbat.pin.as_deref(), vbat.pad.as_deref()),
        (Some("p02"), Some("5"))
    );
    // A part that no request asks has no map.
    let detail = editor.board("Sensors").unwrap();
    let load = detail.parts.iter().find(|p| p.path == "load").unwrap();
    assert!(load.pin_requests.is_none());
    // The editor writes neither file.
    assert!(!pins_file(&root).exists());
    assert!(!root.join("src/main.lock.toml").exists());
}

#[test]
fn a_locked_entry_of_the_file_is_held_and_shown_locked_with_where_it_came_from() {
    let (root, editor) = project("sidecars-pins-file", &board(GOOD));
    let file = "version = 1\n\n[Sensors.pins.status]\npin = \"p03\"\nfunction = \"GPIO\"\nlock = true\nsource = \"hand\"\n\n[Sensors.pins.vbat]\npin = \"p02\"\nfunction = \"ANALOG\"\nsource = \"auto\"\n";
    fs::write(pins_file(&root), file).unwrap();
    // The file is watched: the next reading holds the entries.
    let map = pin_map(&editor);
    assert_eq!(map.len(), 2, "{map:?}");
    let status = &map[0];
    assert_eq!(status.pin.as_deref(), Some("p03"));
    assert_eq!(status.pad.as_deref(), Some("6"));
    assert!(status.locked);
    assert_eq!(status.entry.as_deref(), Some("hand"));
    assert_eq!(status.from.as_deref(), Some("file"));
    let vbat = &map[1];
    assert!(!vbat.locked);
    assert_eq!(vbat.entry.as_deref(), Some("auto"));
    assert_eq!(vbat.from.as_deref(), Some("file"));
    // Reading is not writing: the bytes are the ones put there.
    assert_eq!(fs::read_to_string(pins_file(&root)).unwrap(), file);
}

#[test]
fn an_entry_the_file_cannot_keep_is_picked_again_and_says_it_came_from_this_run() {
    let (root, editor) = project("sidecars-pins-moved", &board(GOOD));
    // `vbat` is held to `p00`, which offers no `ANALOG`: the entry does not hold.
    fs::write(
        pins_file(&root),
        "version = 1\n\n[Sensors.pins.vbat]\npin = \"p00\"\nfunction = \"ANALOG\"\nsource = \"auto\"\n",
    )
    .unwrap();
    let map = pin_map(&editor);
    let vbat = map.iter().find(|r| r.request == "vbat").unwrap();
    assert_eq!(vbat.pin.as_deref(), Some("p02"));
    assert_eq!(vbat.from.as_deref(), Some("run"), "{vbat:?}");
}

#[test]
fn a_request_that_has_no_pin_says_what_the_check_said_beside_the_ones_that_have() {
    let (root, editor) = project("sidecars-pins-none", &board(GOOD));
    assert_eq!(pin_map(&editor).len(), 2);
    // A request nothing offers: the board no longer builds, and the part shows the request with
    // the check's own message; the board is the last good one, marked stale.
    fs::write(
        root.join("src/main.nsm"),
        board(&format!("{GOOD}    .spare: .u.GPIOO\n")),
    )
    .unwrap();
    let map = pin_map(&editor);
    let keys: Vec<&str> = map.iter().map(|r| r.request.as_str()).collect();
    assert_eq!(keys, ["spare", "status", "vbat"], "{map:?}");
    let spare = &map[0];
    assert_eq!(spare.pin, None);
    assert_eq!(spare.pad, None);
    assert_eq!(spare.function, "GPIOO");
    assert_eq!(spare.from, None);
    let message = spare.message.as_deref().unwrap();
    assert!(
        message
            .contains("`spare` asks for GPIOO on `u` (`Zmcu4`), and no pin of `Zmcu4` offers it")
            && message.contains("did you mean `GPIO`?"),
        "{message}"
    );
    // The same words are in the problems list.
    let analysis = editor.analysis();
    assert!(
        analysis.problems.iter().any(|p| p.message == message),
        "{:?}",
        analysis.problems
    );
    // The requests that have pins keep them.
    assert_eq!(map[1].pin.as_deref(), Some("p00"));
    // Mended: the message goes.
    fs::write(root.join("src/main.nsm"), board(GOOD)).unwrap();
    assert!(pin_map(&editor).iter().all(|r| r.message.is_none()));
}

// ---- the pick lock ------------------------------------------------------------------

const ZZ9: &str = r#"import std::sources::Rail

part Zz9 {
    .vin:  Rail
    .vout: Rail
    .vin.neg ~ .vout.neg

    .dropout:     Voltage
    .max_current: Current
    .vin.voltage >= .vout.voltage + .dropout
    .vin.current == -.vout.current
    -.vout.current <= .max_current

    pins "SOT-23-5" {
        .vin.pos       power       "IN"     "1"
        .vin.neg       power       "GND"    "2"
        .en            input       "EN"     "3"
        .nc            no_connect  "NC"     "4"
        .vout.pos      power       "OUT"    "5"
    }

    .package: Package
    catalog zz9 { .vout.voltage, .dropout, .max_current, .package }
}
"#;

const ZZ9_BOARD: &str = r#"import std::sources::Rail
import src::ics::Zz9

board Zz9Board {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .u: Zz9 { .vout.voltage &= 3.0V..3.6V }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..50mA
    }
    .input ~ .u.vin
    .u.vout ~ .output
    .u.en ~ .u.vin.pos
}
"#;

const HEADER: &str =
    "lcsc,mpn,vout.voltage,dropout,max_current,package,library,price,stock,date,source\n";
/// Invented rows: not real parts.
const ROW_S: &str = "C9001,ZZ9-33-S,3.3V +/- 2%,0.3V,0.5A,SOT-23-5,Basic,0.10,1000,2026-10-31,https://example.invalid/zz9-33-s.pdf\n";
const ROW_A: &str = "C9002,ZZ9-33-A,3.3V +/- 1%,0.2V,0.5A,SOT-23-5,Basic,0.10,1000,2026-10-31,https://example.invalid/zz9-33-a.pdf\n";

fn set_rows(root: &Path, rows: &str) {
    fs::write(root.join("catalogs/zz9.csv"), format!("{HEADER}{rows}")).unwrap();
}

fn lock_project(label: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("catalogs")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"zz\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/ics.nsm"), ZZ9).unwrap();
    fs::write(root.join("src/main.nsm"), ZZ9_BOARD).unwrap();
    set_rows(&root, ROW_S);
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    (root, editor)
}

fn lock_of(editor: &Workspace) -> PickLockInfo {
    editor
        .board("Zz9Board")
        .unwrap()
        .lock
        .expect("a lock status")
}

#[test]
fn a_source_that_does_not_build_has_nothing_to_compare_the_lock_with() {
    let (root, editor) = lock_project("sidecars-lock-source-broken");
    write_lock(&root);
    assert!(lock_of(&editor).current);
    fs::write(
        root.join("src/main.nsm"),
        ZZ9_BOARD.replace(".u.en ~ .u.vin.pos", ".u.en ~ .u.nowhere"),
    )
    .unwrap();
    // The last good board is still shown, marked stale, and says nothing of the lock.
    let detail = editor.board("Zz9Board").unwrap();
    assert!(detail.lock.is_none(), "{:?}", detail.lock);
    assert!(editor.analysis().errors > 0);
}

/// Writes the lock the solver picks now, as `netsmith lock --update` would.
fn write_lock(root: &Path) {
    let target = Target::resolve_from(root, None).unwrap();
    let source = &target.files[0];
    let text = fs::read_to_string(&source.path).unwrap();
    let found = netsmith_pipeline::pick_lock::refresh(
        &source.path,
        &source.name,
        &text,
        &target.loader().unwrap(),
        &target.catalogs().unwrap(),
    )
    .unwrap();
    found.write().unwrap();
}

#[test]
fn a_lock_that_is_absent_says_so_and_is_not_written() {
    let (root, editor) = lock_project("sidecars-lock-absent");
    let lock = lock_of(&editor);
    assert_eq!(lock.file, "src/main.lock.toml");
    assert!(!lock.exists && !lock.current, "{lock:?}");
    assert!(lock.diff.is_empty() && lock.error.is_none(), "{lock:?}");
    assert!(!root.join("src/main.lock.toml").exists());
}

#[test]
fn a_lock_that_is_current_says_so_and_its_creation_refreshes_the_view() {
    let (root, editor) = lock_project("sidecars-lock-current");
    let before = editor.analysis().version;
    assert!(!lock_of(&editor).exists);
    write_lock(&root);
    // Only the lock changed, and the reading is made again: the lock is watched.
    let lock = lock_of(&editor);
    assert!(editor.analysis().version > before);
    assert!(lock.exists && lock.current, "{lock:?}");
    assert!(lock.diff.is_empty() && lock.reported.is_empty(), "{lock:?}");
}

#[test]
fn a_lock_that_is_stale_lists_what_lock_check_would_print() {
    let (root, editor) = lock_project("sidecars-lock-stale");
    write_lock(&root);
    assert!(lock_of(&editor).current);
    let text = fs::read_to_string(root.join("src/main.lock.toml")).unwrap();
    // A row the catalog gains before the locked one is what the solver picks now.
    set_rows(&root, &format!("{ROW_A}{ROW_S}"));
    let lock = lock_of(&editor);
    assert!(lock.exists && !lock.current, "{lock:?}");
    assert_eq!(
        lock.diff,
        [
            "Zz9Board: `u`: `ZZ9-33-S` (lcsc C9001, mpn ZZ9-33-S) -> `ZZ9-33-A` (lcsc C9002, mpn ZZ9-33-A)"
        ]
    );
    // The locked row still passes, so the solver has nothing to report; and the file is as it was.
    assert!(lock.reported.is_empty(), "{lock:?}");
    assert_eq!(
        fs::read_to_string(root.join("src/main.lock.toml")).unwrap(),
        text
    );
}

#[test]
fn a_lock_that_is_not_a_lock_is_an_error_not_a_diff() {
    let (root, editor) = lock_project("sidecars-lock-broken");
    fs::write(root.join("src/main.lock.toml"), "version = 9\n").unwrap();
    let lock = lock_of(&editor);
    let error = lock.error.as_deref().unwrap();
    assert!(
        error.contains("version 9 is not a version of the lock this netsmith reads"),
        "{error}"
    );
    assert!(lock.diff.is_empty() && !lock.current, "{lock:?}");
}

#[test]
fn the_lock_file_is_among_the_files_the_editor_watches() {
    let (root, _editor) = lock_project("sidecars-lock-watched");
    let lock = root.join("src/main.lock.toml");
    let target = Target::resolve_from(&root, None).unwrap();
    assert!(!target.watched().contains(&lock));
    write_lock(&root);
    assert!(target.watched().contains(&lock));
}
