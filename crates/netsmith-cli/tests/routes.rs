//! `netsmith routes` and the routes file (`<stem>.routes.toml`, issue #599): a board imported
//! from its own KiCad file is drawn again with the same copper and nothing routed, a shape that
//! shorts another net is refused, a locked shape is not deleted, and `fab` says when a record no
//! longer matches the board.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use netsmith_testkit::TempDir;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `netsmith <args>` in `dir`, with KiCad switched off: no command here needs it.
fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn text(run: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    )
}

fn ok(run: &Output) -> String {
    let said = text(run);
    assert!(run.status.success(), "{said}");
    said
}

/// The copper of a `.kicad_pcb` as a sorted list, one item a line with its uuid left out: the
/// routes file's copper comes first on the board, so a board drawn from it has the copper of the
/// one it was imported from in another order.
fn copper(pcb: &Path) -> Vec<String> {
    let pcb = fs::read_to_string(pcb).unwrap();
    let mut items = Vec::new();
    let mut item: Option<String> = None;
    for line in pcb.lines() {
        if ["\t(segment", "\t(arc", "\t(via"].contains(&line) {
            item = Some(line.trim().to_string());
        } else if let Some(text) = &mut item {
            if line == "\t)" {
                items.push(std::mem::take(text));
                item = None;
            } else if !line.trim().starts_with("(uuid") {
                text.push(' ');
                text.push_str(line.trim());
            }
        }
    }
    items.sort();
    items
}

/// A folder with `examples/led.nsm` and its pick lock, fabbed into `first`, and the routes file
/// imported from that fab's board.
fn imported(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    for name in ["led.nsm", "led.lock.toml"] {
        fs::copy(repo().join("examples").join(name), dir.join(name)).unwrap();
    }
    ok(&netsmith(&dir, &["fab", "led.nsm", "--locked", "--out", "first"]));
    let said = ok(&netsmith(
        &dir,
        &[
            "routes",
            "import",
            "first/LedExample/LedExample.kicad_pcb",
            "--into",
            "led.nsm",
        ],
    ));
    assert!(said.contains("wrote led.routes.toml: 3 net(s)"), "{said}");
    dir
}

#[test]
fn a_board_imported_from_its_own_fab_is_drawn_again_with_nothing_routed() {
    let dir = imported("routes-round-trip");
    let routes = fs::read_to_string(dir.join("led.routes.toml")).unwrap();
    assert!(routes.starts_with("version = 1\n"), "{routes}");
    assert!(routes.contains("[LedExample.nets.GND]"), "{routes}");
    // The import placed the parts where the KiCad board has them, locked.
    let layout = fs::read_to_string(dir.join("led.pcb.toml")).unwrap();
    assert!(layout.contains("source = \"import\""), "{layout}");
    assert!(layout.contains("at = [11.6, 3.75]"), "{layout}");

    let said = ok(&netsmith(&dir, &["fab", "led.nsm", "--locked", "--out", "second"]));
    assert!(said.contains("routing 0 connections"), "{said}");
    assert!(said.contains("design rules check: passed"), "{said}");
    assert!(!said.contains("warning: led.routes.toml"), "{said}");
    let board = |out: &str| dir.join(out).join("LedExample/LedExample.kicad_pcb");
    let (first, second) = (copper(&board("first")), copper(&board("second")));
    assert_eq!(first.len(), 15, "{first:#?}");
    assert_eq!(first, second);

    // `show` lists the records and says the router has nothing left.
    let shown = ok(&netsmith(&dir, &["routes", "show", "--into", "led.nsm"]));
    assert!(shown.contains("net GND (locked): 6 track(s)"), "{shown}");
    assert!(shown.contains("the router would route 0 connection(s)"), "{shown}");
}

#[test]
fn a_track_that_shorts_another_net_is_refused_and_the_file_kept() {
    let dir = imported("routes-short");
    let before = fs::read_to_string(dir.join("led.routes.toml")).unwrap();
    // Up from J1's VCC pad, across the GND track that runs at y = 1.6.
    let run = netsmith(
        &dir,
        &[
            "routes",
            "add-track",
            "--into",
            "led.nsm",
            "--net",
            "VCC",
            "--from",
            "5.3,2.8",
            "--to",
            "5.3,1.0",
        ],
    );
    let said = text(&run);
    assert_eq!(run.status.code(), Some(1), "{said}");
    assert!(said.contains("refused"), "{said}");
    assert!(said.contains("tracks_crossing"), "{said}");
    assert_eq!(
        fs::read_to_string(dir.join("led.routes.toml")).unwrap(),
        before
    );
    // A net the board does not have is refused too.
    let run = netsmith(
        &dir,
        &[
            "routes", "add-via", "--into", "led.nsm", "--net", "NOPE", "--at", "1,1",
        ],
    );
    assert_eq!(run.status.code(), Some(1), "{}", text(&run));
}

#[test]
fn a_locked_shape_or_net_is_not_deleted_until_unlocked() {
    let dir = imported("routes-locks");
    let routes = |args: &[&str]| {
        let mut all = vec!["routes"];
        all.extend_from_slice(args);
        all.extend_from_slice(&["--into", "led.nsm", "--net", "VCC"]);
        netsmith(&dir, &all)
    };
    // The import locks every net: its record is not deleted.
    let run = routes(&["delete"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run));
    ok(&routes(&["unlock"]));
    ok(&routes(&["lock", "--track", "1"]));
    let run = routes(&["delete", "--track", "1"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run));
    assert!(text(&run).contains("is locked"), "{}", text(&run));
    ok(&routes(&["unlock", "--track", "1"]));
    ok(&routes(&["delete", "--track", "1"]));
    let shown = ok(&netsmith(&dir, &["routes", "show", "--into", "led.nsm"]));
    assert!(shown.contains("net VCC: 2 track(s)"), "{shown}");
}

#[test]
fn fab_says_a_record_whose_pads_moved_and_routes_the_net_again() {
    let dir = imported("routes-stale");
    let layout = dir.join("led.pcb.toml");
    let text_of = fs::read_to_string(&layout).unwrap();
    let moved = text_of.replace("at = [11.6, 3.75]", "at = [11.6, 4.5]");
    assert_ne!(moved, text_of);
    fs::write(&layout, moved).unwrap();

    let said = ok(&netsmith(&dir, &["fab", "led.nsm", "--locked", "--out", "moved"]));
    assert!(
        said.contains(
            "warning: led.routes.toml: the pads of net `GND` are not where the routes file's record was written"
        ),
        "{said}"
    );
    assert!(said.contains("routing 2 connections"), "{said}");
    assert!(said.contains("design rules check: passed"), "{said}");
}

#[test]
fn routes_lock_writes_a_record_whose_pads_moved_again() {
    let dir = imported("routes-relock");
    // A record written for other pads: its hash is not the board's.
    let file = dir.join("led.routes.toml");
    let routes = fs::read_to_string(&file).unwrap();
    let hash = routes
        .lines()
        .skip_while(|l| *l != "[LedExample.nets.GND]")
        .find_map(|l| l.strip_prefix("pads = "))
        .unwrap()
        .to_string();
    fs::write(&file, routes.replacen(&hash, "\"0000000000000000\"", 1)).unwrap();
    let notes = |dir: &Path| ok(&netsmith(dir, &["routes", "show", "--into", "led.nsm"]));
    assert!(notes(&dir).contains("the pads of net `GND` are not where"));

    ok(&netsmith(
        &dir,
        &["routes", "lock", "--into", "led.nsm", "--net", "GND"],
    ));
    assert_eq!(fs::read_to_string(&file).unwrap(), routes);
    let said = ok(&netsmith(&dir, &["fab", "led.nsm", "--locked", "--out", "again"]));
    assert!(!said.contains("are not where"), "{said}");
    assert!(said.contains("routing 0 connections"), "{said}");
}
