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
    ok(&netsmith(&dir, &["board", "fab", "led.nsm", "--locked", "--out", "first"]));
    let said = ok(&netsmith(
        &dir,
        &[
            "routes",
            "import",
            "first/LedExample/LedExample.kicad_pcb",
            "--file",
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
    assert!(layout.contains("at = [9.85, 4.6]"), "{layout}");

    let said = ok(&netsmith(&dir, &["board", "fab", "led.nsm", "--locked", "--out", "second"]));
    assert!(said.contains("LedExample: routing: 0 connections"), "{said}");
    assert!(said.contains("design rules check: passed"), "{said}");
    assert!(!said.contains("warning: led.routes.toml"), "{said}");
    let board = |out: &str| dir.join(out).join("LedExample/LedExample.kicad_pcb");
    let (first, second) = (copper(&board("first")), copper(&board("second")));
    // The router draws each net of the LED board as one track between its pads' centres
    // (issue #1486).
    assert_eq!(first.len(), 3, "{first:#?}");
    assert_eq!(first, second);

    // `show` lists the records and says the router has nothing left.
    let shown = ok(&netsmith(&dir, &["routes", "show", "--file", "led.nsm"]));
    assert!(shown.contains("net GND (locked): 1 track(s)"), "{shown}");
    assert!(shown.contains("the router would route 0 connection(s)"), "{shown}");
}

#[test]
fn a_track_that_shorts_another_net_is_refused_and_the_file_kept() {
    let dir = imported("routes-short");
    let before = fs::read_to_string(dir.join("led.routes.toml")).unwrap();
    // Down from the VCC track, across the GND track that runs at y = 5.24 to 5.39.
    let run = netsmith(
        &dir,
        &[
            "routes",
            "add-track",
            "--file",
            "led.nsm",
            "--net",
            "VCC",
            "--from",
            "5.0,2.7",
            "--to",
            "5.0,6.0",
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
            "routes", "add-via", "--file", "led.nsm", "--net", "NOPE", "--at", "1,1",
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
        all.extend_from_slice(&["--file", "led.nsm", "--net", "VCC"]);
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
    let shown = ok(&netsmith(&dir, &["routes", "show", "--file", "led.nsm"]));
    assert!(!shown.contains("net VCC ("), "{shown}");
    assert!(shown.contains("net VCC: J1.1 to R1.1"), "{shown}");
}

#[test]
fn a_record_whose_pads_moved_keeps_its_copper_and_says_where_it_is_open() {
    // Issue #1328: the LED moved down by its pads' pitch after its copper was imported, so the
    // GND track that ran to its first pad now ends on its second, the anode's.
    let dir = imported("routes-stale");
    let layout = dir.join("led.pcb.toml");
    let text_of = fs::read_to_string(&layout).unwrap();
    let moved = text_of.replace("at = [9.85, 4.6]", "at = [9.85, 6.175]");
    assert_ne!(moved, text_of);
    fs::write(&layout, moved).unwrap();

    // Every shape is kept; the track into the LED's old pad is open at it.
    let shown = ok(&netsmith(&dir, &["routes", "show", "--file", "led.nsm"]));
    assert!(
        shown.contains(
            "the pads of net `GND` are not where the routes file's record was written: its 1 shape(s) are kept"
        ),
        "{shown}"
    );
    assert!(shown.contains("open at D1.1 (track 1's end at 9.85, 5.3875)"), "{shown}");
    assert!(shown.contains("net GND: D1.1 to J1.2"), "{shown}");

    // `fab` routes what the kept copper leaves, and the kept track on the anode's pad leaves
    // the anode's connection unrouted.
    let run = netsmith(&dir, &["board", "fab", "led.nsm", "--locked", "--out", "moved"]);
    let said = text(&run);
    assert!(said.contains("its 1 shape(s) are kept"), "{said}");
    assert!(said.contains("LedExample: routing: 2 connections"), "{said}");
    assert!(said.contains("INCOMPLETE"), "{said}");

    // Lock is refused for that error of GND's own copper...
    let file = dir.join("led.routes.toml");
    let before = fs::read_to_string(&file).unwrap();
    let run = netsmith(&dir, &["routes", "lock", "--net", "GND", "--file", "led.nsm"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run));
    assert!(text(&run).contains("of net GND's own copper"), "{}", text(&run));
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
    // ...and not for VCC's, though the board has GND's and the anode's errors.
    ok(&netsmith(&dir, &["routes", "lock", "--net", "VCC", "--file", "led.nsm"]));

    // With the track into the old pad deleted, GND locks.
    ok(&netsmith(
        &dir,
        &["routes", "delete", "--net", "GND", "--track", "1", "--file", "led.nsm"],
    ));
    ok(&netsmith(&dir, &["routes", "lock", "--net", "GND", "--file", "led.nsm"]));
    let shown = ok(&netsmith(&dir, &["routes", "show", "--file", "led.nsm"]));
    assert!(!shown.contains("pads of net `GND`"), "{shown}");
}

#[test]
fn routes_delete_deletes_a_record_of_a_net_the_board_no_longer_has() {
    // Issue #1329: the net was renamed in the source, so its record names a net the board lacks;
    // `fab` says to delete it, by the name it prints, and `routes delete` does.
    let dir = imported("routes-gone-net");
    let file = dir.join("led.routes.toml");
    let routes_text = fs::read_to_string(&file).unwrap();
    let renamed = routes_text.replace("LedExample.nets.GND", "LedExample.nets.OLD");
    assert_ne!(renamed, routes_text);
    fs::write(&file, renamed).unwrap();

    let said = ok(&netsmith(&dir, &["board", "fab", "led.nsm", "--locked", "--out", "gone"]));
    assert!(
        said.contains("the routes file has net `OLD`, which the board does not have"),
        "{said}"
    );
    // One shape of it cannot be named: only the whole record goes.
    let run = routes(&dir, &["delete", "--net", "OLD", "--track", "1"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run));
    assert!(
        text(&run).contains("has no net `OLD`") && text(&run).contains("deleted whole"),
        "{}",
        text(&run)
    );
    // The record was locked: it goes all the same, and the command says what it deleted.
    let said = ok(&routes(&dir, &["delete", "--net", "OLD"]));
    assert!(
        said.contains("deleted net OLD's record (1 shape(s)), a net the board does not have"),
        "{said}"
    );
    assert!(said.contains("was locked"), "{said}");
    let after = fs::read_to_string(&file).unwrap();
    assert!(!after.contains("OLD"), "{after}");
    assert!(after.contains("LedExample.nets.VCC"), "{after}");
    let said = ok(&netsmith(&dir, &["board", "fab", "led.nsm", "--locked", "--out", "after"]));
    assert!(!said.contains("does not have"), "{said}");
    // A net that neither the board nor the file has is still refused.
    let run = routes(&dir, &["delete", "--net", "OLD"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run));
    assert!(text(&run).contains("has no net `OLD`"), "{}", text(&run));
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
    let notes = |dir: &Path| ok(&netsmith(dir, &["routes", "show", "--file", "led.nsm"]));
    assert!(notes(&dir).contains("the pads of net `GND` are not where"));

    ok(&netsmith(
        &dir,
        &["routes", "lock", "--file", "led.nsm", "--net", "GND"],
    ));
    assert_eq!(fs::read_to_string(&file).unwrap(), routes);
    let said = ok(&netsmith(&dir, &["board", "fab", "led.nsm", "--locked", "--out", "again"]));
    assert!(!said.contains("are not where"), "{said}");
    assert!(said.contains("LedExample: routing: 0 connections"), "{said}");
}

/// A folder with `examples/led.nsm` and its pick lock, and no routes file.
fn led(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    for name in ["led.nsm", "led.lock.toml"] {
        fs::copy(repo().join("examples").join(name), dir.join(name)).unwrap();
    }
    dir
}

/// `netsmith routes <args> --file led.nsm`.
fn routes(dir: &Path, args: &[&str]) -> Output {
    let mut all = vec!["routes"];
    all.extend_from_slice(args);
    all.extend_from_slice(&["--file", "led.nsm"]);
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

    // The board is 14 by 8 mm: the whole outline on top, joined to GND.
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
        file.contains("outline = [[0.2, 0.2], [13.8, 0.2], [13.8, 7.8], [0.2, 7.8]]"),
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
        &["board", "fab", "led.nsm", "--locked", "--out", "out"],
    ));
    assert!(said.contains("LedExample: routing: 2 connections"), "{said}");
    assert!(said.contains("netsmith filled GND on F.Cu"), "{said}");
    assert!(said.contains("design rules check: passed"), "{said}");
    let pcb = fs::read_to_string(dir.join("out/LedExample/LedExample.kicad_pcb")).unwrap();
    // Each zone is in the board file, on KiCad's page: the outline's corner is at (20, 20).
    assert!(
        pcb.contains("(xy 20.2 20.2) (xy 33.8 20.2) (xy 33.8 27.8) (xy 20.2 27.8)"),
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
        &["board", "fab", "led.nsm", "--locked", "--out", "out"],
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
            "5.0,2.7",
            "--to",
            "5.0,6.0",
        ],
    );
    let said = text(&run);
    assert_eq!(run.status.code(), Some(1), "{said}");
    // The crossing is at (5.0, 5.288) from the outline's corner, not (25.0, 25.288) on KiCad's
    // page.
    assert!(said.contains("at (5.000, 5.288) on F.Cu"), "{said}");
}

/// The six-layer example (its second and third inner layers signal layers) in a folder of its own.
fn six_layer(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    for name in ["six_layer.nsm", "six_layer.lock.toml"] {
        fs::copy(repo().join("examples").join(name), dir.join(name)).unwrap();
    }
    dir
}

#[test]
fn add_track_draws_on_an_inner_signal_layer_and_refuses_a_plane() {
    // Issue #833: a track of the routes file may be on a signal layer; a plane holds none.
    let dir = six_layer("routes-inner-track");
    let add = |layer: &str| {
        netsmith(
            &dir,
            &[
                "routes",
                "add-track",
                "--net",
                "VIN",
                "--layer",
                layer,
                "--from",
                "1.0,1.0",
                "--to",
                "2.0,1.0",
                "--file",
                "six_layer.nsm",
            ],
        )
    };
    let plane = add("inner1");
    let said = text(&plane);
    assert_eq!(plane.status.code(), Some(1), "{said}");
    assert!(said.contains("not a signal layer"), "{said}");
    ok(&add("inner2"));
    let file = fs::read_to_string(dir.join("six_layer.routes.toml")).unwrap();
    assert!(file.contains("layer = \"inner2\""), "{file}");
    let shown = ok(&netsmith(&dir, &["routes", "show", "--file", "six_layer.nsm"]));
    assert!(shown.contains("inner2"), "{shown}");
}

/// The led example with a second load on its rail, so that net VCC carries 1.2 A and needs three
/// vias side by side where it changes layer (issue #1114).
fn heavy_rail(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    let led = fs::read_to_string(repo().join("examples/led.nsm")).unwrap();
    let led = led
        .replace(
            ".pos.net &= \"VCC\"",
            ".pos.net &= \"VCC\"\n        .current &= -1.3A..-1.0A",
        )
        .replace(
            "    .r1: Resistor {}",
            "    .motor: Rail {\n        .voltage &= 5V +/- 5%\n        .pos.net &= \"VCC\"\n        .current &= 1.1A..1.2A\n    }\n    .motor.pos ~ .supply.pos\n    .motor.neg ~ .supply.neg\n\n    .r1: Resistor {}",
        );
    assert!(led.contains(".motor"), "{led}");
    fs::write(dir.join("led.nsm"), led).unwrap();
    dir
}

#[test]
fn vias_side_by_side_are_checked_as_a_group_and_one_alone_is_refused() {
    let dir = heavy_rail("routes-via-group");
    let via = |args: &[&str]| {
        let mut all = vec!["routes", "add-via", "--file", "led.nsm", "--net", "VCC"];
        all.extend_from_slice(args);
        netsmith(&dir, &all)
    };
    let file = dir.join("led.routes.toml");
    // One via alone, or two of the three, is short of the net's current.
    for run in [via(&["--at", "5,5"]), via(&["--at", "5,5", "--at", "5.45,5"])] {
        let said = text(&run);
        assert_eq!(run.status.code(), Some(1), "{said}");
        assert!(said.contains("via_current") && said.contains("so 3 side by side are needed"), "{said}");
        assert!(!file.exists(), "a refusal leaves the file as it was");
    }
    // Three places side by side (the process's via is 0.45 mm), or a row round one place.
    let said = ok(&via(&["--at", "4.55,5", "--at", "5,5", "--at", "5.45,5"]));
    assert!(said.contains("added a via to net VCC (3 times)"), "{said}");
    fs::remove_file(&file).unwrap();
    ok(&via(&["--at", "5,5", "--count", "3", "--pitch", "0.45"]));
    let shown = ok(&netsmith(&dir, &["routes", "show", "--file", "led.nsm"]));
    assert!(shown.contains("net VCC: 0 track(s), 3 via(s)"), "{shown}");
    for place in ["(4.55, 5)", "(5, 5)", "(5.45, 5)"] {
        assert!(shown.contains(place), "{place}: {shown}");
    }
    // A pitch that leaves them apart is no group: refused, and the file is as it was.
    let before = fs::read_to_string(&file).unwrap();
    let run = via(&["--at", "5,7", "--count", "3", "--pitch", "2"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run));
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
    // `--count` goes round one `--at` and with `--pitch`.
    let run = via(&["--at", "1,1", "--at", "2,2", "--count", "3", "--pitch", "0.45"]);
    assert_eq!(run.status.code(), Some(2), "{}", text(&run));
    assert!(text(&run).contains("round one `--at`"), "{}", text(&run));
    let run = via(&["--at", "1,1", "--count", "3"]);
    assert_eq!(run.status.code(), Some(2), "{}", text(&run));
}

#[test]
fn a_track_too_narrow_for_its_current_is_named_with_the_width_needed_and_the_current_assumed() {
    // Issue 1373: net VCC carries 1.2 A or more (two rails). A track of 1 mm is accepted; the
    // file is then changed to 0.15 mm, as a hand edit would, and `board drc` says what the track
    // is, what the current needs, which current it assumed and where it comes from.
    let dir = heavy_rail("routes-track-current");
    ok(&netsmith(
        &dir,
        &[
            "routes", "add-track", "--file", "led.nsm", "--net", "VCC", "--layer", "top",
            "--width", "1.0", "--from", "5.0,5.0", "--to", "7.0,5.0",
        ],
    ));
    let file = dir.join("led.routes.toml");
    let written = fs::read_to_string(&file).unwrap();
    let thin = written
        .lines()
        .map(|l| {
            if l.contains("from = [5.0, 5.0]") {
                let at = l.find("width = ").expect(l);
                let rest = &l[at + "width = ".len()..];
                let value_end = rest.find([',', ' ', '}']).unwrap_or(rest.len());
                format!("{}width = 0.15{}", &l[..at], &rest[value_end..])
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(thin, written, "{written}");
    fs::write(&file, thin).unwrap();
    let run = netsmith(&dir, &["board", "drc", "led.nsm", "--format", "json"]);
    let report: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let found: Vec<&serde_json::Value> = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["rule"] == "track_current")
        .collect();
    assert_eq!(found.len(), 1, "{report}");
    let f = found[0];
    assert_eq!(f["width"], 0.15, "{f}");
    assert!(f["needed_width"].as_f64().unwrap() > 0.5, "{f}");
    assert!(f["current"].as_f64().unwrap() >= 1.2, "{f}");
    assert!(f["current_from"].as_str().unwrap().contains("rail `"), "{f}");
    assert_eq!(f["rise"], 10.0, "{f}");
    let said = f["message"].as_str().unwrap();
    assert!(
        said.contains("its net `VCC` carries") && said.contains("which needs"),
        "{said}"
    );
}

#[test]
fn widen_says_what_it_would_widen_writes_nothing_on_a_dry_run_then_widens() {
    // Issue #1692: net VCC carries 1.2 A or more and wants a wide track; one of 0.15 mm, as a
    // hand edit leaves it, is widened towards it.
    let dir = heavy_rail("routes-widen");
    ok(&netsmith(
        &dir,
        &[
            "routes", "add-track", "--file", "led.nsm", "--net", "VCC", "--layer", "top",
            "--width", "1.0", "--from", "5.0,5.0", "--to", "7.0,5.0",
        ],
    ));
    let file = dir.join("led.routes.toml");
    let before = fs::read_to_string(&file).unwrap();
    let line = before
        .lines()
        .find(|l| l.contains("from = [5.0, 5.0]"))
        .unwrap()
        .to_string();
    let thin = line.replace("width = 1.0", "width = 0.15");
    assert_ne!(thin, line, "{line}");
    fs::write(&file, before.replace(&line, &thin)).unwrap();
    let written = fs::read_to_string(&file).unwrap();

    let run = routes(&dir, &["widen", "--net", "VCC", "--dry-run", "--format", "json"]);
    let said = ok(&run);
    let json: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!((json["dry_run"].clone(), json["fit"].clone()), (true.into(), true.into()), "{said}");
    assert_eq!(json["nets"], serde_json::json!(["VCC"]), "{said}");
    assert_eq!(json["counts"]["widened"], 1, "{said}");
    let change = &json["changes"][0];
    assert_eq!(change["width_before"], 0.15, "{said}");
    assert!(change["width_after"].as_f64().unwrap() > 0.15, "{said}");
    assert!(json["wrote"].as_array().unwrap().is_empty(), "{said}");
    assert_eq!(fs::read_to_string(&file).unwrap(), written, "a dry run writes nothing");

    let said = ok(&routes(&dir, &["widen", "--net", "VCC"]));
    assert!(said.contains("widened 1 track(s) of net(s) VCC"), "{said}");
    assert!(said.contains("wrote led.routes.toml"), "{said}");
    assert_ne!(fs::read_to_string(&file).unwrap(), written);
    // A net the board lacks: the pass does not run.
    let run = routes(&dir, &["widen", "--net", "NOPE"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run));
}

#[test]
fn tidy_says_what_it_would_change_then_cuts_a_corner_and_merges_a_pair() {
    // Issue #1192.
    let dir = led("routes-tidy");
    ok(&routes(
        &dir,
        &[
            "add-track", "--net", "GND", "--layer", "bottom", "--width", "0.15", "--from",
            "1.0,0.8", "--to", "2.0,0.8",
        ],
    ));
    // The rest of the copper is written by hand, as a script would: a right angle (which
    // `add-track` refuses, `track_angle`) and a pair of collinear tracks.
    let file = dir.join("led.routes.toml");
    let text = fs::read_to_string(&file).unwrap();
    let first = text
        .lines()
        .find(|l| l.contains("from = [1.0, 0.8]"))
        .unwrap()
        .to_string();
    let more = [
        first.clone(),
        first.replace("from = [1.0, 0.8], to = [2.0, 0.8]", "from = [2.0, 0.8], to = [3.5, 0.8]"),
        first.replace("from = [1.0, 0.8], to = [2.0, 0.8]", "from = [3.5, 0.8], to = [3.5, 2.0]"),
    ]
    .join("\n");
    fs::write(&file, text.replace(&first, &more)).unwrap();
    let written = fs::read_to_string(&file).unwrap();

    let run = routes(&dir, &["tidy", "--dry-run", "--format", "json"]);
    let said = ok(&run);
    let json: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(json["dry_run"], true, "{said}");
    assert_eq!(json["counts"]["corners"], 1, "{said}");
    assert_eq!(json["counts"]["merges"], 1, "{said}");
    let places: Vec<&serde_json::Value> = json["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| &c["at"])
        .collect();
    assert!(places.contains(&&serde_json::json!([3.5, 0.8])), "{said}");
    assert!(places.contains(&&serde_json::json!([2.0, 0.8])), "{said}");
    assert!(json["wrote"].as_array().unwrap().is_empty(), "{said}");
    assert_eq!(fs::read_to_string(&file).unwrap(), written);

    // The progress, as JSON lines on stderr (issue #1225): each pass's nets, one by one.
    let run = routes(&dir, &["tidy", "--progress", "json"]);
    let said = ok(&run);
    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&run.stderr)
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .filter(|e: &serde_json::Value| e["stage"] == "tidying")
        .collect();
    assert!(!events.is_empty(), "{said}");
    for e in &events {
        assert_eq!(e["board"], "LedExample", "{e}");
        assert_eq!(e["unit"], "nets tidied", "{e}");
        assert!(e["elapsedMs"].is_u64() && e["total"].is_u64(), "{e}");
    }
    assert!(
        events.iter().any(|e| e["detail"]
            .as_str()
            .is_some_and(|d| d.starts_with("pass 1: net GND, 1 of 1 net(s): ")
                && d.contains("change(s) made so far"))),
        "{events:#?}"
    );
    assert!(
        said.contains("tidied 1 corner(s), 1 collinear pair(s)"),
        "{said}"
    );
    assert!(said.contains("corner: net GND on bottom at (3.5, 0.8)"), "{said}");
    assert!(said.contains("wrote led.routes.toml"), "{said}");
    let shown = ok(&routes(&dir, &["show"]));
    assert!(shown.contains("net GND: 3 track(s)"), "{shown}");
    assert!(shown.contains("track 1: bottom, 0.15 mm, (1, 0.8) to (2.96, 0.8)"), "{shown}");
    // Nothing is left to do.
    let said = ok(&routes(&dir, &["tidy"]));
    assert!(said.contains("tidied 0 corner(s), 0 collinear pair(s)"), "{said}");
    assert!(said.contains("nothing to write"), "{said}");
}

/// `board nets --compare` (issue #1629): two routes files of one board, one tidied, compare row
/// by row, from the routes file and from the JSON of an earlier run alike, and the gate fails
/// on a row that is worse.
#[test]
fn board_nets_compares_a_tidied_routes_file_with_the_one_before_and_gates_on_a_worse_row() {
    let dir = imported("routes-nets-compare");
    // The imported copper unlocked, and the GND track split near its pad, as a round of hand
    // routing leaves it: `routes tidy` changes it.
    let file = dir.join("led.routes.toml");
    let written = fs::read_to_string(&file).unwrap();
    let gnd = "{ layer = \"top\", width = 0.15, from = [2.65, 5.24], to = [9.85, 5.3875] },";
    assert!(written.contains(gnd), "{written}");
    let split = "{ layer = \"top\", width = 0.15, from = [2.65, 5.24], to = [2.75, 5.242049] },\n  \
                 { layer = \"top\", width = 0.15, from = [2.75, 5.242049], to = [9.85, 5.3875] },";
    fs::write(
        &file,
        written.replace("lock = true\n", "").replace(gnd, split),
    )
    .unwrap();
    fs::copy(&file, dir.join("before.routes.toml")).unwrap();
    let nets = |args: &[&str]| {
        let mut all = vec!["board", "nets", "led.nsm"];
        all.extend_from_slice(args);
        netsmith(&dir, &all)
    };
    let run = nets(&["--format", "json"]);
    ok(&run);
    fs::write(dir.join("before.json"), &run.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let q = &json["quality"];
    assert_eq!(
        (&q["tracks"], &q["connections_left"], &q["drops_over"], &q["findings"]),
        (&4.into(), &0.into(), &0.into(), &0.into()),
        "{q}"
    );
    let said = ok(&nets(&[]));
    assert!(
        said.contains("routing: 4 track(s), 0 connection(s) left to route, 0 track drop(s) over budget, 0 error(s) of the design rules check"),
        "{said}"
    );

    ok(&routes(&dir, &["tidy"]));
    let tidied = fs::read_to_string(&file).unwrap();
    // The change of a row, the last number of its line (or the one before `worse`).
    let change = |said: &str, words: &str| -> f64 {
        let line = said
            .lines()
            .find(|l| l.trim_start().starts_with(words))
            .unwrap_or_else(|| panic!("no row {words}: {said}"));
        let line = line.trim_end().trim_end_matches("worse").trim_end();
        line.rsplit(' ').next().unwrap().parse().unwrap()
    };
    // The tidied file against the one before, from the routes file and from the JSON alike:
    // every row printed with its change, the tracks up. The signal length moves by rounding only:
    // what the tidy takes away lies in pads, which a net's length does not count (issue #1670).
    for against in ["before.routes.toml", "before.json"] {
        let run = nets(&["--compare", against, "--gate", "--max-unneeded", "9"]);
        let said = text(&run);
        assert!(
            said.contains(&format!("compared with {against} (worse beyond 0 %):")),
            "{said}"
        );
        for words in [
            "signal length (mm)", "short tracks", "layer changes", "via pairs", "vias",
            "tracks", "connections left", "track drops over budget", "design rules errors",
        ] {
            change(&said, words);
        }
        assert!(change(&said, "signal length (mm)").abs() < 0.01, "{said}");
        assert!(change(&said, "tracks") > 0.0, "{said}");
        assert_eq!(change(&said, "design rules errors"), 0.0, "{said}");
        // More tracks is a worse row, and the gate fails on it.
        assert_eq!(run.status.code(), Some(1), "{said}");
        assert!(
            said.contains(&format!("quality gate: tracks worse than in {against}: 4 before")),
            "{said}"
        );
    }
    let run = nets(&["--compare", "before.routes.toml", "--format", "json"]);
    let json: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let rows = json["compare"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 9, "{json}");
    let tracks = rows.iter().find(|r| r["row"] == "tracks").unwrap();
    assert_eq!(tracks["before"], 4.0, "{json}");
    assert_eq!(tracks["worse"], true, "{json}");
    // Within a tolerance as large as the growth, the tracks are no worse.
    let after = tracks["after"].as_f64().unwrap();
    let pct = format!("{}", (after - 4.0) / 4.0 * 100.0);
    ok(&nets(&[
        "--compare", "before.routes.toml", "--gate", "--max-unneeded", "9", "--tolerance", &pct,
    ]));

    // A file whose GND goes a longer way, against the tidied one: the signal is longer, and the
    // gate fails on that row; without `--gate` the comparison judges nothing.
    fs::write(dir.join("tidied.routes.toml"), &tidied).unwrap();
    let longer = fs::read_to_string(dir.join("before.routes.toml")).unwrap();
    let way = "from = [2.75, 5.242049], to = [9.85, 5.3875] },";
    assert!(longer.contains(way), "{longer}");
    let longer = longer.replace(
        way,
        "from = [2.75, 5.242049], to = [6.0, 6.4] },\n  \
         { layer = \"top\", width = 0.15, from = [6.0, 6.4], to = [9.85, 5.3875] },",
    );
    fs::write(&file, longer).unwrap();
    let run = nets(&["--compare", "tidied.routes.toml", "--gate", "--max-unneeded", "9"]);
    let said = text(&run);
    assert_eq!(run.status.code(), Some(1), "{said}");
    assert!(change(&said, "signal length (mm)") > 0.0, "{said}");
    assert!(
        said.contains("quality gate: length worse than in tidied.routes.toml: "),
        "{said}"
    );
    ok(&nets(&["--compare", "tidied.routes.toml"]));
}

/// `netsmith <args>` in `dir` with `input` on stdin.
fn netsmith_with_input(dir: &Path, args: &[&str], input: &str) -> Output {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn routes_add_makes_a_batch_skips_a_refused_shape_and_writes_the_file_once() {
    let dir = imported("routes-add-batch");
    // The first via is on the VCC track; the track runs down from it across the GND track
    // that runs at y = 5.24 to 5.39 (refused, `tracks_crossing`); the second via is on VCC too.
    let doc = r#"{
        "track": [{"net": "VCC", "from": [5.0, 2.7], "to": [5.0, 6.0]}],
        "via": [{"net": "VCC", "at": [5.0, 2.7]}, {"net": "NOPE", "at": [1, 1]}]
    }"#;
    let run = netsmith_with_input(
        &dir,
        &["routes", "add", "-", "--file", "led.nsm", "--format", "json"],
        doc,
    );
    let said = text(&run);
    assert_eq!(run.status.code(), Some(1), "{said}");
    let out: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let results: Vec<&str> = out["shapes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["result"].as_str().unwrap())
        .collect();
    assert_eq!(results, ["refused", "ok", "refused"], "{said}");
    let crossing = &out["shapes"][0]["findings"][0];
    assert_eq!(crossing["rule"], "tracks_crossing", "{said}");
    assert!(crossing["at"][1].as_f64().unwrap() > 4.0, "{said}");
    assert_eq!(out["written"], true, "{said}");
    let routes = fs::read_to_string(dir.join("led.routes.toml")).unwrap();
    assert!(routes.contains("at = [5.0, 2.7]"), "{routes}");
    // With `--stop-on-refusal` nothing after the first refusal is tried, and a TOML file is
    // read by its extension.
    fs::write(
        dir.join("shapes.toml"),
        "[[track]]\nnet = \"VCC\"\nfrom = [5.0, 2.7]\nto = [5.0, 6.0]\n\n[[via]]\nnet = \"VCC\"\nat = [5.0, 2.7]\n",
    )
    .unwrap();
    let run = netsmith(
        &dir,
        &[
            "routes",
            "add",
            "shapes.toml",
            "--file",
            "led.nsm",
            "--stop-on-refusal",
            "--dry-run",
        ],
    );
    let said = text(&run);
    assert_eq!(run.status.code(), Some(1), "{said}");
    assert!(said.contains("track 1 (net VCC): refused"), "{said}");
    assert!(said.contains("stopped at the first refusal (1 not tried)"), "{said}");
}

#[test]
fn routes_import_says_each_net_as_data_and_unlocked_locks_none() {
    let dir = led("routes-import-json");
    ok(&netsmith(&dir, &["board", "fab", "led.nsm", "--locked", "--out", "first"]));
    let run = netsmith(
        &dir,
        &[
            "routes",
            "import",
            "first/LedExample/LedExample.kicad_pcb",
            "--file",
            "led.nsm",
            "--unlocked",
            "--format",
            "json",
        ],
    );
    let said = ok(&run);
    let out: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(out["locked"], false, "{said}");
    let nets = out["nets"].as_array().unwrap();
    assert_eq!(nets.len(), 3, "{said}");
    let gnd = nets.iter().find(|n| n["net"] == "GND").unwrap();
    assert_eq!(gnd["locked"], false, "{said}");
    assert_eq!(gnd["tracks"], 1, "{said}");
    // The board fab made has no error of its copper.
    assert!(
        nets.iter().all(|n| n["refusals"].as_array().unwrap().is_empty()),
        "{said}"
    );
    let routes = fs::read_to_string(dir.join("led.routes.toml")).unwrap();
    assert!(!routes.contains("lock = true"), "{routes}");
}

/// The four-layer example (a ground pour stitched to the ground plane) in a project folder,
/// fabbed once into `first/`: its KiCad board has loose stitching vias, ground vias with no
/// track (issue #1583).
fn stitched(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    for ext in ["nsm", "physical.nsm", "pcb.toml", "lock.toml"] {
        fs::copy(
            repo().join(format!("examples/outline_and_pours.{ext}")),
            dir.join(format!("main.{ext}")),
        )
        .unwrap();
    }
    ok(&netsmith(&dir, &["board", "fab", "main.nsm", "--out", "first"]));
    dir
}

/// Each net's tracks and vias of a `.kicad_pcb`, by the net's name.
fn kicad_counts(pcb: &Path) -> std::collections::BTreeMap<String, (usize, usize)> {
    let pcb = fs::read_to_string(pcb).unwrap();
    let mut names = std::collections::HashMap::new();
    for line in pcb.lines() {
        // `(net 2 "GND")` at the top of the file names the numbers.
        if let Some(rest) = line.strip_prefix("\t(net ") {
            let (n, name) = rest.trim_end_matches(')').split_once(' ').unwrap();
            names.insert(n.to_string(), name.trim_matches('"').to_string());
        }
    }
    let mut counts = std::collections::BTreeMap::<String, (usize, usize)>::new();
    let mut kind: Option<&str> = None;
    for line in pcb.lines() {
        match line {
            "\t(segment" | "\t(arc" => kind = Some("track"),
            "\t(via" => kind = Some("via"),
            "\t)" => kind = None,
            _ => {
                let Some(k) = kind else { continue };
                if let Some(n) = line.trim().strip_prefix("(net ") {
                    let entry = counts
                        .entry(names[n.trim_end_matches(')')].clone())
                        .or_default();
                    if k == "track" {
                        entry.0 += 1;
                    } else {
                        entry.1 += 1;
                    }
                }
            }
        }
    }
    counts
}

/// The routes file's copper of `dir` as `routes show` counts it, each net's tracks and vias on
/// the board (the file's vias and the stitching vias `fab` lays that the file does not have),
/// and `board nets`' vias of each net of its table.
fn shown_counts(
    dir: &Path,
) -> (
    std::collections::BTreeMap<String, (usize, usize)>,
    std::collections::BTreeMap<String, u64>,
) {
    let run = netsmith(
        dir,
        &["routes", "show", "--file", "main.nsm", "--format", "json"],
    );
    let said = ok(&run);
    let shown: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let count = |n: &serde_json::Value, key: &str| n[key].as_array().unwrap().len();
    let show = shown["nets"]
        .as_array()
        .unwrap_or_else(|| panic!("{said}"))
        .iter()
        .map(|n| {
            (
                n["net"].as_str().unwrap().to_string(),
                (
                    count(n, "tracks"),
                    count(n, "vias") + n["stitching"].as_u64().unwrap() as usize,
                ),
            )
        })
        .collect();
    let run = netsmith(dir, &["board", "nets", "main.nsm", "--format", "json"]);
    let said = ok(&run);
    let nets: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let table = nets["nets"]
        .as_array()
        .unwrap_or_else(|| panic!("{said}"))
        .iter()
        .map(|n| {
            (
                n["net"].as_str().unwrap().to_string(),
                n["vias"].as_u64().unwrap(),
            )
        })
        .collect();
    (show, table)
}

/// Import the board `fab` wrote into `first/`, locked or not, and check that `routes show` and
/// `board nets` count each net's copper as the KiCad board has it (issue #1583).
fn imported_counts_are_the_boards(tag: &str, unlocked: bool) {
    let dir = stitched(tag);
    let pcb = "first/OutlineAndPours/OutlineAndPours.kicad_pcb";
    let mut args = vec!["routes", "import", pcb, "--file", "main.nsm", "--no-places"];
    if unlocked {
        args.push("--unlocked");
    }
    args.extend(["--format", "json"]);
    let run = netsmith(&dir, &args);
    let said = ok(&run);
    let import: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let left_out = import["stitching_vias_left_out"].as_array().unwrap();
    let kicad = kicad_counts(&dir.join(pcb));
    let gnd = kicad["GND"].1;
    if unlocked {
        // The import leaves the stitching vias out, names each, and keeps the ground's others.
        assert!(left_out.len() > 4, "the pour is stitched: {said}");
        assert!(left_out.iter().all(|v| v["net"] == "GND"), "{said}");
        assert_eq!(import["vias"].as_u64().unwrap() as usize + left_out.len(), {
            kicad.values().map(|c| c.1).sum::<usize>()
        });
    } else {
        assert!(left_out.is_empty(), "{said}");
    }
    let (show, table) = shown_counts(&dir);
    assert_eq!(show, kicad, "routes show counts the KiCad board's copper: {said}");
    assert!(show["GND"].1 == gnd && gnd > 4, "{show:?}");
    for (net, vias) in &table {
        assert_eq!(*vias as usize, kicad[net].1, "board nets' vias of {net}: {table:?}");
    }
    // The text says the stitching vias the board has past the file's.
    let shown = ok(&netsmith(&dir, &["routes", "show", "--file", "main.nsm"]));
    assert_eq!(
        shown.contains("stitching via(s) `fab` lays on the pour's grid"),
        unlocked,
        "{shown}"
    );
}

#[test]
fn routes_show_counts_the_stitching_vias_of_a_locked_import() {
    imported_counts_are_the_boards("routes-stitching-locked", false);
}

#[test]
fn routes_show_counts_the_stitching_vias_of_an_unlocked_import_as_of_a_locked_one() {
    imported_counts_are_the_boards("routes-stitching-unlocked", true);
}

/// The copper zones of a `.kicad_pcb`, as the import reads them.
fn kicad_zones(pcb: &Path) -> Vec<netsmith_tools::copper::Zone> {
    let text = fs::read_to_string(pcb).unwrap();
    let (zones, unread) = netsmith_tools::copper::read_zones(&text, "board").unwrap();
    assert!(unread.is_empty(), "{unread:?}");
    zones
}

#[test]
fn routes_import_takes_a_hand_zone_leaves_the_processs_pours_and_fab_writes_them_again() {
    // Issue #1609: a hand zone of the 3.3 V net on top, over the ground pour, fabbed, imported
    // and fabbed again with the routes file: the zones of the two boards are the same.
    let dir = TempDir::new("routes-import-zones");
    for ext in ["nsm", "physical.nsm", "pcb.toml", "lock.toml"] {
        fs::copy(
            repo().join(format!("examples/outline_and_pours.{ext}")),
            dir.join(format!("main.{ext}")),
        )
        .unwrap();
    }
    ok(&netsmith(
        &dir,
        &[
            "routes", "add-zone", "--file", "main.nsm", "--net", "3V3", "--layer", "top", "--rect",
            "38,24,6,6",
        ],
    ));
    ok(&netsmith(&dir, &["board", "fab", "main.nsm", "--out", "first"]));
    let pcb = "first/OutlineAndPours/OutlineAndPours.kicad_pcb";
    let run = netsmith(
        &dir,
        &["routes", "import", pcb, "--file", "main.nsm", "--no-places", "--format", "json"],
    );
    let said = ok(&run);
    let import: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(import["zones"], 1, "{said}");
    let left = import["zones_left_out"].as_array().unwrap();
    let reasons = |r: &str| left.iter().filter(|z| z["reason"] == r).count();
    // The two planes and the two outer pours are the process's, the antenna's keepout a rule
    // area.
    assert_eq!(reasons("process"), 4, "{said}");
    assert!(reasons("rule_area") >= 1, "{said}");
    assert!(left.iter().all(|z| z["said"].as_str().is_some_and(|s| !s.is_empty())), "{said}");
    let routes = fs::read_to_string(dir.join("main.routes.toml")).unwrap();
    assert_eq!(routes.matches("[[OutlineAndPours.nets.3V3.zones]]").count(), 1, "{routes}");

    ok(&netsmith(&dir, &["board", "fab", "main.nsm", "--out", "second"]));
    let second = "second/OutlineAndPours/OutlineAndPours.kicad_pcb";
    let (a, b) = (kicad_zones(&dir.join(pcb)), kicad_zones(&dir.join(second)));
    let diff = netsmith_tools::copper::compare_zones(&a, &b, 1e-6);
    assert!(
        diff.missing.is_empty() && diff.extra.is_empty() && diff.differing.is_empty(),
        "{diff:?}"
    );
    assert!(diff.matched >= 5, "{diff:?}");

    // The text counts them.
    let text = ok(&netsmith(
        &dir,
        &["routes", "import", pcb, "--file", "main.nsm", "--no-places"],
    ));
    assert!(
        text.contains("zones: 1 taken, ") && text.contains("4 of the process's own planes and pours"),
        "{text}"
    );
}
