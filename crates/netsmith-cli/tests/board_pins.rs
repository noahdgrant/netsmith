//! `netsmith board pins` (issue #1092): the pin file from the command line, with the code the
//! editor's `POST /api/pins/lock` and `/api/pins/move` call. Each write is compared, byte for
//! byte, with what the editor's call writes for the same edit in a copy of the project, and
//! every refusal leaves the file as it was.

use std::fs;
use std::path::Path;
use std::process::Output;

use netsmith_editor::Backend;
use netsmith_editor::api::{PinLock, PinMove};
use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;
use serde_json::Value;

/// An invented part: `p00` and `p01` may not both be in use (`PAIR`), `p02` and `p03` offer
/// `ANALOG` too, and `p04` is the source's.
const SOURCE: &str = r#"
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

board B {
    .u: Zq
    .probe: Probe
    .v: .u.ANALOG
    .x: .u.GPIO
    .y: .u.GPIO
    .v ~ .probe.a00
    .x ~ .probe.a01
    .y ~ .probe.a02
    .u.p04 ~ .probe.a03
    .u.p04.function &= GPIO
}
"#;

fn project(label: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"pins\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), SOURCE).unwrap();
    root
}

fn pins(root: &Path) -> std::path::PathBuf {
    root.join("src/main.pins.toml")
}

fn netsmith(root: &Path, args: &[&str]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(root)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .output()
        .unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("stdout is one JSON document: {e}\n{}", stdout(out)))
}

/// The editor of a copy of the project, to make the same edit with its call.
fn editor(root: &Path) -> Workspace {
    Workspace::open(Target::resolve_from(root, None).unwrap()).unwrap()
}

fn lock_in_editor(root: &Path, request: &str, locked: bool) {
    editor(root)
        .pin_lock(&PinLock {
            board: "B".into(),
            request: request.into(),
            locked,
        })
        .unwrap();
}

fn move_in_editor(root: &Path, request: &str, pin: &str) {
    editor(root)
        .pin_move(&PinMove {
            board: "B".into(),
            request: request.into(),
            pin: pin.into(),
        })
        .unwrap();
}

#[test]
fn show_lists_the_requests_with_their_pins_and_the_pins_they_may_take() {
    let root = project("board-pins-show");
    let out = netsmith(&root, &["board", "pins", "show", "--format", "json"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let doc = json(&out);
    assert_eq!(doc["command"], "board pins show");
    assert_eq!(doc["ok"], true);
    let board = &doc["boards"][0];
    assert_eq!(board["board"], "B");
    assert_eq!(board["file"], "src/main.pins.toml");
    let requests = board["requests"].as_array().unwrap();
    let keys: Vec<&str> = requests
        .iter()
        .map(|r| r["request"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["v", "x", "y"]);
    let x = &requests[1];
    assert_eq!(x["part"], "u");
    assert_eq!(x["function"], "GPIO");
    assert_eq!(x["pin"], "p00");
    assert_eq!(x["locked"], false);
    assert_eq!(x["from"], "run");
    let offers: Vec<&str> = x["offers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["pin"].as_str().unwrap())
        .collect();
    assert_eq!(offers, ["p00", "p01", "p02", "p03"]);
    assert_eq!(x["offers"][2]["heldBy"], "v");
    // It writes nothing.
    assert!(!pins(&root).exists());
    // The text names each request and what it may take.
    let text = netsmith(&root, &["board", "pins", "show"]);
    let text = stdout(&text);
    assert!(text.contains("B (src/main.pins.toml)"), "{text}");
    assert!(text.contains("x on u: GPIO at p00 (pad 1)"), "{text}");
    assert!(text.contains("other pins: p01 (pad 2), p02 (held by `v`)"), "{text}");
}

#[test]
fn lock_and_unlock_write_the_bytes_the_editor_writes() {
    let ours = project("board-pins-lock");
    let theirs = project("board-pins-lock-editor");
    let out = netsmith(&ours, &["board", "pins", "lock", "x"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "wrote src/main.pins.toml: B x is on p00 for GPIO (auto), locked\n"
    );
    lock_in_editor(&theirs, "x", true);
    assert_eq!(fs::read(pins(&ours)).unwrap(), fs::read(pins(&theirs)).unwrap());
    assert_eq!(
        fs::read_to_string(pins(&ours)).unwrap(),
        "version = 1\n\n[B.pins.x]\npin = \"p00\"\nfunction = \"GPIO\"\nlock = true\nsource = \"auto\"\n"
    );

    // Locked already: refused with the editor's words, exit 1, the file as it was.
    let before = fs::read(pins(&ours)).unwrap();
    let again = netsmith(&ours, &["board", "pins", "lock", "x", "--format", "json"]);
    assert_eq!(again.status.code(), Some(1), "{}", stderr(&again));
    let doc = json(&again);
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["error"]["status"], 422);
    assert_eq!(doc["error"]["code"], "pin_refused");
    assert_eq!(doc["error"]["message"], "`x` is locked already");
    assert_eq!(fs::read(pins(&ours)).unwrap(), before);

    let out = netsmith(&ours, &["board", "pins", "unlock", "x", "--format", "json"]);
    assert!(out.status.success(), "{}", stderr(&out));
    lock_in_editor(&theirs, "x", false);
    assert_eq!(fs::read(pins(&ours)).unwrap(), fs::read(pins(&theirs)).unwrap());
    let doc = json(&out);
    assert_eq!(doc["entry"]["lock"], false);
    assert_eq!(doc["entry"]["pin"], "p00");
    assert_eq!(doc["file"], "src/main.pins.toml");
}

#[test]
fn a_move_writes_the_bytes_the_editor_writes() {
    let ours = project("board-pins-move");
    let theirs = project("board-pins-move-editor");
    let out = netsmith(&ours, &["board", "pins", "move", "x", "p01", "--format", "json"]);
    assert!(out.status.success(), "{}", stderr(&out));
    move_in_editor(&theirs, "x", "p01");
    assert_eq!(fs::read(pins(&ours)).unwrap(), fs::read(pins(&theirs)).unwrap());
    let doc = json(&out);
    assert_eq!(doc["command"], "board pins move");
    assert_eq!(doc["request"], "x");
    assert_eq!(doc["entry"]["pin"], "p01");
    assert_eq!(doc["entry"]["source"], "hand");
    // A hand-kept file keeps its comments and the other entries, as the editor keeps them.
    let kept = "# kept by hand\nversion = 1\n\n[B.pins.v]\npin = \"p03\"   # the divider\nfunction = \"ANALOG\"\nsource = \"hand\"\n";
    fs::write(pins(&ours), kept).unwrap();
    fs::write(pins(&theirs), kept).unwrap();
    assert!(netsmith(&ours, &["board", "pins", "move", "x", "p01"]).status.success());
    move_in_editor(&theirs, "x", "p01");
    assert_eq!(fs::read(pins(&ours)).unwrap(), fs::read(pins(&theirs)).unwrap());
    assert!(
        fs::read_to_string(pins(&ours))
            .unwrap()
            .contains("pin = \"p03\"   # the divider\n")
    );
}

#[test]
fn a_refused_move_says_why_lists_the_pins_the_request_may_take_and_writes_nothing() {
    let root = project("board-pins-refused");
    assert!(netsmith(&root, &["board", "pins", "lock", "x"]).status.success());
    let before = fs::read(pins(&root)).unwrap();

    // `p02` is held by `v`; `y` may take `p01`'s... no: `PAIR` allows one of `p00`, `p01`, and
    // `x` holds `p00`. Nothing is free for `y`.
    let out = netsmith(&root, &["board", "pins", "move", "y", "p02"]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("p02 is held by `v`: move that request first"), "{err}");
    assert!(err.contains("`y` (GPIO) may take: p01 (pad 2)"), "{err}");
    assert_eq!(fs::read(pins(&root)).unwrap(), before);

    let out = netsmith(&root, &["board", "pins", "move", "v", "p01", "--format", "json"]);
    assert_eq!(out.status.code(), Some(1));
    let doc = json(&out);
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["board"], "B");
    assert_eq!(doc["request"], "v");
    assert_eq!(
        doc["error"]["message"],
        "p01 does not offer ANALOG, which `v` asks `u` for"
    );
    // `v` may take no other pin: `p03` is held by `y`.
    assert_eq!(doc["mayTake"], serde_json::json!([]));
    assert_eq!(fs::read(pins(&root)).unwrap(), before);

    // The locked request is refused too, and the free pins are listed for a request that has them.
    let out = netsmith(&root, &["board", "pins", "move", "x", "p01", "--format", "json"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        json(&out)["error"]["message"],
        "`x` is locked: unlock it before moving it"
    );
    assert_eq!(fs::read(pins(&root)).unwrap(), before);
}

#[test]
fn a_request_or_board_that_is_not_there_cannot_run_and_writes_nothing() {
    let root = project("board-pins-missing");
    for args in [
        &["board", "pins", "lock", "nope"][..],
        &["board", "pins", "move", "nope", "p01"],
        &["board", "pins", "lock", "x", "--board", "Other"],
    ] {
        let out = netsmith(&root, args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", stderr(&out));
        assert!(stdout(&out).is_empty(), "{args:?}");
        assert!(!pins(&root).exists(), "{args:?}");
    }
    let out = netsmith(&root, &["board", "pins", "lock", "nope", "--format", "json"]);
    assert_eq!(out.status.code(), Some(2));
    let doc = json(&out);
    assert_eq!(doc["ok"], false);
    assert!(
        doc["error"]["message"]
            .as_str()
            .unwrap()
            .contains("no pin request `nope`")
    );
}

/// What `fab --pins locked` would propose: `v` and `y` swap their pins.
const PROPOSAL: &str = "# Proposed by `netsmith board fab`\nversion = 1\n\n[B.pins.v]\npin = \"p03\"\nfunction = \"ANALOG\"\nsource = \"auto\"\n\n[B.pins.x]\npin = \"p00\"\nfunction = \"GPIO\"\nsource = \"auto\"\n\n[B.pins.y]\npin = \"p02\"\nfunction = \"GPIO\"\nsource = \"auto\"\n";

#[test]
fn apply_takes_a_proposal_that_swaps_pins_and_writes_only_the_entries_it_moves() {
    let root = project("board-pins-apply");
    let kept = "# kept by hand\nversion = 1\n\n[B.pins.v]\npin = \"p02\"   # the divider\nfunction = \"ANALOG\"\nsource = \"hand\"\n\n[B.pins.x]\npin = \"p00\"\nfunction = \"GPIO\"\nsource = \"auto\"\n\n[B.pins.y]\npin = \"p03\"\nfunction = \"GPIO\"\nsource = \"auto\"\n";
    fs::write(pins(&root), kept).unwrap();
    fs::create_dir_all(root.join("fab")).unwrap();
    fs::write(root.join("fab/pins.proposed.toml"), PROPOSAL).unwrap();
    let out = netsmith(&root, &["board", "pins", "apply", "--format", "json"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let doc = json(&out);
    assert_eq!(doc["moved"], serde_json::json!(["v: p02 to p03", "y: p03 to p02"]));
    assert_eq!(doc["file"], "src/main.pins.toml");
    let text = fs::read_to_string(pins(&root)).unwrap();
    // `v` is the proposal's entry, `x` was not touched, and the comment of the file stays.
    assert!(text.starts_with("# kept by hand\n"), "{text}");
    assert!(text.contains("[B.pins.v]\npin = \"p03\""), "{text}");
    assert!(text.contains("[B.pins.y]\npin = \"p02\""), "{text}");
    assert!(text.contains("[B.pins.x]\npin = \"p00\"\nfunction = \"GPIO\"\nsource = \"auto\"\n"));
    // Taking it again changes nothing.
    let after = fs::read(pins(&root)).unwrap();
    let again = netsmith(&root, &["board", "pins", "apply"]);
    assert!(again.status.success(), "{}", stderr(&again));
    assert!(stdout(&again).contains("nothing to apply"), "{}", stdout(&again));
    assert_eq!(fs::read(pins(&root)).unwrap(), after);
    // A proposal file elsewhere is named, and the swap goes back; the entries are the proposal's.
    let back_proposal = "version = 1\n\n[B.pins.v]\npin = \"p02\"\nfunction = \"ANALOG\"\nsource = \"auto\"\n\n[B.pins.y]\npin = \"p03\"\nfunction = \"GPIO\"\nsource = \"auto\"\n";
    fs::write(root.join("swap.toml"), back_proposal).unwrap();
    let back = netsmith(&root, &["board", "pins", "apply", "--proposal", "swap.toml"]);
    assert!(back.status.success(), "{}", stderr(&back));
    assert_eq!(
        fs::read_to_string(pins(&root)).unwrap(),
        kept.replace("source = \"hand\"", "source = \"auto\"")
    );
}

#[test]
fn apply_refuses_a_locked_request_and_a_map_the_check_does_not_keep() {
    let root = project("board-pins-apply-refused");
    fs::create_dir_all(root.join("fab")).unwrap();
    // The proposal moves `v`, which is locked in the file.
    let locked = "version = 1\n\n[B.pins.v]\npin = \"p02\"\nfunction = \"ANALOG\"\nlock = true\nsource = \"hand\"\n";
    fs::write(pins(&root), locked).unwrap();
    fs::write(root.join("fab/pins.proposed.toml"), PROPOSAL).unwrap();
    let out = netsmith(&root, &["board", "pins", "apply"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(stderr(&out).contains("`v` is locked: unlock it first"), "{}", stderr(&out));
    assert_eq!(fs::read_to_string(pins(&root)).unwrap(), locked);

    // A map that breaks the `PAIR` limit: `x` is locked on `p00`, and `y` is put on `p01`.
    let held = "version = 1\n\n[B.pins.x]\npin = \"p00\"\nfunction = \"GPIO\"\nlock = true\nsource = \"auto\"\n";
    fs::write(pins(&root), held).unwrap();
    let broken = "version = 1\n\n[B.pins.y]\npin = \"p01\"\nfunction = \"GPIO\"\nsource = \"auto\"\n";
    fs::write(root.join("fab/pins.proposed.toml"), broken).unwrap();
    let out = netsmith(&root, &["board", "pins", "apply", "--format", "json"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    let doc = json(&out);
    assert!(
        doc["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not a pin map the check keeps"),
        "{doc}"
    );
    assert_eq!(fs::read_to_string(pins(&root)).unwrap(), held);

    // No proposal at all cannot run.
    fs::remove_file(root.join("fab/pins.proposed.toml")).unwrap();
    let out = netsmith(&root, &["board", "pins", "apply"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("pins.proposed.toml"), "{}", stderr(&out));
}
