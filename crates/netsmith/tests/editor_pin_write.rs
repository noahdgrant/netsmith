//! The editor writes the pin file's `lock`, `pin` and `source` lines (ADR 0101, ED2 of the pin
//! epic): the padlock, and a move of a request to another pin. Every refusal leaves the file's
//! bytes alone, and a success is read back by the next analysis.

use std::fs;
use std::path::Path;

use netsmith::editor::Workspace;
use netsmith::project::Target;
use netsmith_editor::{
    Backend,
    api::{PinLock, PinMove, PinRequestInfo},
};
use netsmith_kicad::testlib::TempDir;

/// An invented part: `p00` and `p01` may not both be in use (`PAIR`), `p02` and `p03` offer
/// `ANALOG` too.
const MCU: &str = r#"
part Zq {
    shared PAIR limit 1 { .p00 .p01 }
    pins {
        .p00 bidirectional "P0.00" "1" fn GPIO
        .p01 bidirectional "P0.01" "2" fn GPIO
        .p02 bidirectional "P0.02" "3" fn GPIO ANALOG
        .p03 bidirectional "P0.03" "4" fn GPIO ANALOG
        .p04 bidirectional "P0.04" "5" fn GPIO
    }
}

part Probe {
    pins {
        .a00 passive "A00" "1"
        .a01 passive "A01" "2"
        .a02 passive "A02" "3"
        .a03 passive "A03" "4"
    }
}
"#;

/// `p04` is the source's (it is connected and its function is chosen). The search gives `v` the
/// only free `ANALOG` pin it can have first (`p02`), then `x` `p00` and `y` `p03`, because `y`
/// on `p01` would use both pins of `PAIR`.
const REQUESTS: &str = "    .v: .u.ANALOG\n    .x: .u.GPIO\n    .y: .u.GPIO\n    .v ~ .probe.a00\n    .x ~ .probe.a01\n    .y ~ .probe.a02\n    .u.p04 ~ .probe.a03\n    .u.p04.function &= \"GPIO\"\n";

fn source(requests: &str) -> String {
    format!("{MCU}\nboard B {{\n    .u: Zq\n    .probe: Probe\n{requests}}}\n")
}

fn project(label: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"pins\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), source(REQUESTS)).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    (root, editor)
}

fn pins_file(root: &Path) -> std::path::PathBuf {
    root.join("src/main.pins.toml")
}

fn map(editor: &Workspace) -> Vec<PinRequestInfo> {
    let detail = editor.board("B").unwrap();
    let part = detail.parts.iter().find(|p| p.path == "u").unwrap();
    part.pin_requests.clone().unwrap_or_default()
}

fn request(editor: &Workspace, key: &str) -> PinRequestInfo {
    map(editor).into_iter().find(|r| r.request == key).unwrap()
}

fn lock(editor: &Workspace, key: &str, locked: bool) -> Result<String, (u16, String)> {
    editor
        .pin_lock(&PinLock {
            board: "B".into(),
            request: key.into(),
            locked,
        })
        .map(|w| w.file)
        .map_err(|e| (e.status, e.message))
}

fn moved(editor: &Workspace, key: &str, pin: &str) -> Result<String, (u16, String)> {
    editor
        .pin_move(&PinMove {
            board: "B".into(),
            request: key.into(),
            pin: pin.into(),
        })
        .map(|w| w.file)
        .map_err(|e| (e.status, e.message))
}

#[test]
fn the_pin_map_lists_the_pins_each_request_may_be_moved_to() {
    let (_root, editor) = project("pin-write-offers");
    let x = request(&editor, "x");
    assert_eq!(x.pin.as_deref(), Some("p00"));
    // Every pin that offers GPIO and the source does not hold; `p04` is the source's.
    let pins: Vec<&str> = x.offers.iter().map(|o| o.pin.as_str()).collect();
    assert_eq!(pins, ["p00", "p01", "p02", "p03"]);
    let held: Vec<(&str, Option<&str>)> = x
        .offers
        .iter()
        .map(|o| (o.pin.as_str(), o.held_by.as_deref()))
        .collect();
    assert_eq!(
        held,
        [
            ("p00", None),
            ("p01", None),
            ("p02", Some("v")),
            ("p03", Some("y"))
        ]
    );
    assert_eq!(x.offers[1].pad.as_deref(), Some("2"));
    let v = request(&editor, "v");
    let pins: Vec<&str> = v.offers.iter().map(|o| o.pin.as_str()).collect();
    assert_eq!(pins, ["p02", "p03"]);
}

#[test]
fn a_lock_writes_the_entry_the_file_lacks_and_an_unlock_removes_only_the_lock_line() {
    let (root, editor) = project("pin-write-lock");
    assert!(!pins_file(&root).exists());
    assert_eq!(lock(&editor, "x", true).unwrap(), "src/main.pins.toml");
    assert_eq!(
        fs::read_to_string(pins_file(&root)).unwrap(),
        "version = 1\n\n[B.pins.x]\npin = \"p00\"\nfunction = \"GPIO\"\nlock = true\nsource = \"auto\"\n"
    );
    // The next reading holds the entry from the file, locked; the pin did not move.
    let x = request(&editor, "x");
    assert!(x.locked, "{x:?}");
    assert_eq!(
        (x.pin.as_deref(), x.from.as_deref(), x.entry.as_deref()),
        (Some("p00"), Some("file"), Some("auto"))
    );
    assert!(!request(&editor, "y").locked);
    // Locked already: refused, nothing written.
    let before = fs::read_to_string(pins_file(&root)).unwrap();
    let (status, message) = lock(&editor, "x", true).unwrap_err();
    assert_eq!((status, message.as_str()), (422, "`x` is locked already"));
    assert_eq!(fs::read_to_string(pins_file(&root)).unwrap(), before);
    lock(&editor, "x", false).unwrap();
    assert_eq!(
        fs::read_to_string(pins_file(&root)).unwrap(),
        before.replace("lock = true\n", "")
    );
    assert!(!request(&editor, "x").locked);
    let (status, message) = lock(&editor, "x", false).unwrap_err();
    assert_eq!((status, message.as_str()), (422, "`x` is not locked"));
}

#[test]
fn a_write_keeps_the_comments_the_order_and_the_other_entries_of_a_hand_kept_file() {
    let (root, editor) = project("pin-write-keeps");
    let kept = "# Pins of the board, kept by hand.\nversion = 1\n\n# the ADC input\n[B.pins.v]\npin = \"p03\"   # next to the divider\nfunction = \"ANALOG\"\nsource = \"hand\"\n\n[B.pins.y]\npin = \"p02\"\nfunction = \"GPIO\"\nsource = \"auto\"\n\n# an entry of a board that is gone\n[Gone.pins.z]\npin = \"p00\"\nfunction = \"GPIO\"\nsource = \"auto\"\n";
    fs::write(pins_file(&root), kept).unwrap();
    assert_eq!(request(&editor, "v").pin.as_deref(), Some("p03"));
    lock(&editor, "v", true).unwrap();
    assert_eq!(
        fs::read_to_string(pins_file(&root)).unwrap(),
        kept.replace(
            "function = \"ANALOG\"\nsource = \"hand\"",
            "function = \"ANALOG\"\nlock = true\nsource = \"hand\""
        )
    );
    lock(&editor, "v", false).unwrap();
    assert_eq!(fs::read_to_string(pins_file(&root)).unwrap(), kept);
    // `x` has no entry: a move writes one after the last entry of the board, before the comment
    // of the entry of the board that is gone, and nothing else changes.
    moved(&editor, "x", "p01").unwrap();
    let text = fs::read_to_string(pins_file(&root)).unwrap();
    assert_eq!(
        text,
        kept.replace(
            "source = \"auto\"\n\n# an entry",
            "source = \"auto\"\n\n[B.pins.x]\npin = \"p01\"\nfunction = \"GPIO\"\nsource = \"hand\"\n\n# an entry"
        )
    );
    assert!(text.contains("pin = \"p03\"   # next to the divider\n"));
}

#[test]
fn a_move_goes_to_a_pin_that_offers_the_function_and_the_next_reading_keeps_it() {
    let (root, editor) = project("pin-write-move");
    moved(&editor, "x", "p01").unwrap();
    assert_eq!(
        fs::read_to_string(pins_file(&root)).unwrap(),
        "version = 1\n\n[B.pins.x]\npin = \"p01\"\nfunction = \"GPIO\"\nsource = \"hand\"\n"
    );
    let x = request(&editor, "x");
    assert_eq!(
        (
            x.pin.as_deref(),
            x.pad.as_deref(),
            x.from.as_deref(),
            x.entry.as_deref()
        ),
        (Some("p01"), Some("2"), Some("file"), Some("hand"))
    );
    // The others did not move: `v` and `y` were not asked.
    assert_eq!(request(&editor, "v").pin.as_deref(), Some("p02"));
    assert_eq!(request(&editor, "y").pin.as_deref(), Some("p03"));
    // `p00` is free now: `y` may go there? `x` holds `p01`, and `PAIR` allows one of the two.
    let (status, message) = moved(&editor, "y", "p00").unwrap_err();
    assert_eq!(status, 422);
    assert!(message.contains("`shared` resource of `u`"), "{message}");
    // A pin the first move freed is offered as free.
    let y = request(&editor, "y");
    assert_eq!(
        y.offers
            .iter()
            .find(|o| o.pin == "p01")
            .and_then(|o| o.held_by.as_deref()),
        Some("x")
    );
}

#[test]
fn every_refusal_says_why_and_leaves_the_file_as_it_was() {
    let (root, editor) = project("pin-write-refuse");
    lock(&editor, "x", true).unwrap();
    let before = fs::read_to_string(pins_file(&root)).unwrap();
    let cases: [(&str, &str, &str); 7] = [
        ("x", "p02", "`x` is locked: unlock it before moving it"),
        ("y", "p99", "`u` has no pin p99"),
        ("y", "p04", "p04 is used by the source"),
        (
            "y",
            "p00",
            "p00 is held by `x`, which is locked: move that request first",
        ),
        ("y", "p02", "p02 is held by `v`: move that request first"),
        ("v", "p00", "p00 is held by `x`"),
        ("y", "p03", "`y` is on p03 already"),
    ];
    for (key, pin, expect) in cases {
        let (status, message) = moved(&editor, key, pin).unwrap_err();
        assert_eq!(status, 422, "{key} to {pin}: {message}");
        assert!(message.contains(expect), "{key} to {pin}: {message}");
        assert_eq!(fs::read_to_string(pins_file(&root)).unwrap(), before);
    }
    // A free pin that does not offer what the request asks for: `p01` is no `ANALOG`.
    let (status, message) = moved(&editor, "v", "p01").unwrap_err();
    assert_eq!(status, 422);
    assert_eq!(message, "p01 does not offer ANALOG, which `v` asks `u` for");
    // A free pin that offers it but breaks `PAIR`: `x` is on `p00`, and `y` on `p01` is a second use.
    let (status, message) = moved(&editor, "y", "p01").unwrap_err();
    assert_eq!(status, 422);
    assert!(
        message.starts_with("p01 would use more of a `shared` resource of `u`"),
        "{message}"
    );
    // A request that is not there.
    let (status, _) = moved(&editor, "nope", "p01").unwrap_err();
    assert_eq!(status, 404);
    let (status, _) = lock(&editor, "nope", true).unwrap_err();
    assert_eq!(status, 404);
    assert_eq!(fs::read_to_string(pins_file(&root)).unwrap(), before);
}

#[test]
fn a_request_with_no_pin_and_a_source_that_does_not_build_are_refused() {
    let (root, editor) = project("pin-write-nopin");
    fs::write(
        root.join("src/main.nsm"),
        source(&format!("{REQUESTS}    .spare: .u.GPIOO\n")),
    )
    .unwrap();
    let (status, message) = lock(&editor, "spare", true).unwrap_err();
    assert_eq!(status, 422);
    assert!(
        message.starts_with("`spare` has no pin yet: `spare` asks for GPIOO"),
        "{message}"
    );
    // The board is the last good one, marked stale: no write on a map that is not current.
    let (status, message) = moved(&editor, "x", "p01").unwrap_err();
    assert_eq!(status, 422);
    assert!(message.contains("does not build"), "{message}");
    assert!(!pins_file(&root).exists());
}

#[test]
fn a_file_that_does_not_read_is_refused_and_not_touched() {
    let (root, editor) = project("pin-write-broken");
    let broken = "version = 1\n[B.pins.x]\npin = 3\n";
    fs::write(pins_file(&root), broken).unwrap();
    let (status, message) = lock(&editor, "x", true).unwrap_err();
    // A pin file that does not read is a build error: the board is stale, and nothing is written.
    assert_eq!(status, 422);
    assert!(message.contains("does not build"), "{message}");
    assert_eq!(fs::read_to_string(pins_file(&root)).unwrap(), broken);
}
