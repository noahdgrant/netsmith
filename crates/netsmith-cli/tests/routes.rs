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

/// A folder with `examples/led.nsm` and its pick lock, and no routes file.
fn led(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    for name in ["led.nsm", "led.lock.toml"] {
        fs::copy(repo().join("examples").join(name), dir.join(name)).unwrap();
    }
    dir
}

/// `netsmith routes <args> --into led.nsm`.
fn routes(dir: &Path, args: &[&str]) -> Output {
    let mut all = vec!["routes"];
    all.extend_from_slice(args);
    all.extend_from_slice(&["--into", "led.nsm"]);
    netsmith(dir, &all)
}

#[test]
fn add_zone_writes_a_zone_that_show_lists_and_fab_draws() {
    let dir = led("routes-add-zone");
    let shown = ok(&routes(&dir, &["show"]));
    assert!(
        shown.contains("the router would route 3 connection(s)"),
        "{shown}"
    );

    // The board is 14 by 7 mm: the whole outline on top, joined to GND.
    let said = ok(&routes(
        &dir,
        &["add-zone", "--net", "GND", "--board-outline"],
    ));
    assert!(said.contains("added a zone to net GND on top"), "{said}");
    assert!(said.contains("wrote led.routes.toml"), "{said}");
    let file = fs::read_to_string(dir.join("led.routes.toml")).unwrap();
    assert!(file.contains("[LedExample.nets.GND]"), "{file}");
    assert!(file.contains("[[LedExample.nets.GND.zones]]"), "{file}");
    assert!(file.contains("layers = [\"top\"]"), "{file}");
    assert!(
        file.contains("outline = [[0.2, 0.2], [13.8, 0.2], [13.8, 6.8], [0.2, 6.8]]"),
        "{file}"
    );

    // A second, on the bottom, by a rectangle and with its own settings.
    let said = ok(&routes(
        &dir,
        &[
            "add-zone",
            "--net",
            "VCC",
            "--layer",
            "bottom",
            "--rect",
            "1,1,6,4",
            "--priority",
            "2",
            "--pads",
            "solid",
            "--clearance",
            "0.3",
        ],
    ));
    assert!(said.contains("added a zone to net VCC on bottom"), "{said}");
    let file = fs::read_to_string(dir.join("led.routes.toml")).unwrap();
    assert!(file.contains("priority = 2"), "{file}");
    assert!(file.contains("clearance = 0.3"), "{file}");
    assert!(file.contains("pads = \"solid\""), "{file}");

    // `show` numbers it as tracks and vias are, and the top zone joins GND's pads.
    let shown = ok(&routes(&dir, &["show"]));
    assert!(
        shown.contains("net GND: 0 track(s), 0 via(s), 1 zone(s)"),
        "{shown}"
    );
    assert!(
        shown.contains("zone 1: top, priority 0, 4 corners from (0.2, 0.2), 0 hole(s)"),
        "{shown}"
    );
    assert!(shown.contains("zone 1: bottom, priority 2"), "{shown}");
    assert!(
        shown.contains("the router would route 2 connection(s)"),
        "{shown}"
    );
    assert!(!shown.contains("net GND: D1.1 to J1.2"), "{shown}");

    // `fab` writes them into the board and draws them; GND has no connection left to route.
    let said = ok(&netsmith(
        &dir,
        &["fab", "led.nsm", "--locked", "--out", "out"],
    ));
    assert!(said.contains("routing 2 connections"), "{said}");
    assert!(said.contains("netsmith filled GND on F.Cu"), "{said}");
    assert!(said.contains("design rules check: passed"), "{said}");
    let pcb = fs::read_to_string(dir.join("out/LedExample/LedExample.kicad_pcb")).unwrap();
    // Each zone is in the board file, on KiCad's page: the outline's corner is at (20, 20).
    assert!(
        pcb.contains("(xy 20.2 20.2) (xy 33.8 20.2) (xy 33.8 26.8) (xy 20.2 26.8)"),
        "{pcb}"
    );
    assert!(
        pcb.contains("(xy 21 21) (xy 27 21) (xy 27 25) (xy 21 25)"),
        "{pcb}"
    );
}

#[test]
fn a_zone_that_is_refused_leaves_the_file_as_it_was() {
    let dir = led("routes-zone-refused");
    ok(&routes(
        &dir,
        &["add-zone", "--net", "GND", "--rect", "1,1,8,5"],
    ));
    let file = dir.join("led.routes.toml");
    let before = fs::read_to_string(&file).unwrap();
    let refused = |args: &[&str], reason: &str| {
        let mut all = vec!["add-zone"];
        all.extend_from_slice(args);
        let run = routes(&dir, &all);
        let said = text(&run);
        assert_eq!(run.status.code(), Some(1), "{said}");
        assert!(said.contains(reason), "{reason}: {said}");
        assert_eq!(fs::read_to_string(&file).unwrap(), before, "{said}");
        said
    };
    // Another net's zone over it with the same priority: KiCad would choose by an identifier.
    let said = refused(
        &["--net", "VCC", "--rect", "4,2,6,3"],
        "have the same priority (0)",
    );
    assert!(said.contains("the routes file is not changed"), "{said}");
    // A corner off the board, said in the routes file's places.
    refused(
        &["--net", "VCC", "--rect", "10,2,6,3", "--priority", "1"],
        "corner (16, 2) is off the board",
    );
    // A clearance the process does not make.
    refused(
        &["--net", "VCC", "--rect", "10,2,2,3", "--clearance", "0.01"],
        "less than the process's space between copper",
    );
    // A layer the board lacks, a net it lacks.
    refused(
        &["--net", "VCC", "--rect", "10,2,2,3", "--layer", "inner1"],
        "`inner1`",
    );
    refused(&["--net", "NOPE", "--rect", "10,2,2,3"], "has no net `NOPE`");
    // An outline of two corners.
    refused(
        &["--net", "VCC", "--outline", "1,1", "--outline", "2,2"],
        "three or more",
    );
    // The outline is asked for once, in one way.
    let run = routes(&dir, &["add-zone", "--net", "VCC"]);
    assert_eq!(run.status.code(), Some(2), "{}", text(&run));
    let run = routes(
        &dir,
        &[
            "add-zone",
            "--net",
            "VCC",
            "--rect",
            "1,1,2,2",
            "--board-outline",
        ],
    );
    assert_eq!(run.status.code(), Some(2), "{}", text(&run));

    // The same zone with the higher priority is drawn: it takes the area.
    ok(&routes(
        &dir,
        &[
            "add-zone",
            "--net",
            "VCC",
            "--rect",
            "4,2,6,3",
            "--priority",
            "1",
        ],
    ));
}

#[test]
fn a_zone_over_another_nets_pad_keeps_its_clearance_round_it_and_is_no_short() {
    let dir = led("routes-zone-over-pads");
    // VCC's zone covers the whole top, including the GND and anode pads.
    ok(&routes(
        &dir,
        &["add-zone", "--net", "VCC", "--board-outline"],
    ));
    let said = ok(&netsmith(
        &dir,
        &["fab", "led.nsm", "--locked", "--out", "out"],
    ));
    assert!(said.contains("design rules check: passed"), "{said}");
}

#[test]
fn a_locked_zone_is_not_deleted_until_unlocked() {
    let dir = led("routes-zone-locks");
    ok(&routes(
        &dir,
        &["add-zone", "--net", "GND", "--rect", "1,1,4,4", "--lock"],
    ));
    ok(&routes(
        &dir,
        &["add-zone", "--net", "GND", "--rect", "6,1,4,4"],
    ));
    let shown = ok(&routes(&dir, &["show"]));
    assert!(
        shown.contains("zone 1: top, priority 0, 4 corners from (1, 1), 0 hole(s) (locked)"),
        "{shown}"
    );
    assert!(
        shown.contains("zone 2: top, priority 0, 4 corners from (6, 1), 0 hole(s)\n"),
        "{shown}"
    );

    let run = routes(&dir, &["delete", "--net", "GND", "--zone", "1"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run));
    assert!(
        text(&run).contains("zone 1 of net GND is locked"),
        "{}",
        text(&run)
    );
    // So is the net's whole record.
    let run = routes(&dir, &["delete", "--net", "GND"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run));

    ok(&routes(&dir, &["unlock", "--net", "GND", "--zone", "1"]));
    ok(&routes(&dir, &["lock", "--net", "GND", "--zone", "2"]));
    ok(&routes(&dir, &["unlock", "--net", "GND", "--zone", "2"]));
    ok(&routes(&dir, &["delete", "--net", "GND", "--zone", "1"]));
    let shown = ok(&routes(&dir, &["show"]));
    assert!(
        shown.contains("net GND: 0 track(s), 0 via(s), 1 zone(s)"),
        "{shown}"
    );
    assert!(
        shown.contains("zone 1: top, priority 0, 4 corners from (6, 1)"),
        "{shown}"
    );

    // A zone the record does not have is refused, with how many it has.
    let run = routes(&dir, &["delete", "--net", "GND", "--zone", "2"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run));
    assert!(text(&run).contains("has no zone 2"), "{}", text(&run));
    // The record goes with its last zone, and the file with the record.
    ok(&routes(&dir, &["delete", "--net", "GND", "--zone", "1"]));
    assert!(!dir.join("led.routes.toml").exists());
}

#[test]
fn a_refusal_of_a_track_quotes_the_routes_files_places() {
    let dir = imported("routes-frame");
    let run = routes(
        &dir,
        &[
            "add-track",
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
    // The crossing is at (5.3, 1.6) from the outline's corner, not (25.3, 21.6) on KiCad's page.
    assert!(said.contains("at (5.300, 1.600) on F.Cu"), "{said}");
}
