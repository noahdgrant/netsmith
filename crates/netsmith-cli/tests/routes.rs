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
    assert_eq!(first.len(), 12, "{first:#?}");
    assert_eq!(first, second);

    // `show` lists the records and says the router has nothing left.
    let shown = ok(&netsmith(&dir, &["routes", "show", "--file", "led.nsm"]));
    assert!(shown.contains("net GND (locked): 4 track(s)"), "{shown}");
    assert!(shown.contains("the router would route 0 connection(s)"), "{shown}");
}

#[test]
fn a_track_that_shorts_another_net_is_refused_and_the_file_kept() {
    let dir = imported("routes-short");
    let before = fs::read_to_string(dir.join("led.routes.toml")).unwrap();
    // Down from the VCC track, across the GND track that runs at y = 5.4.
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
    assert!(shown.contains("net VCC: 3 track(s)"), "{shown}");
}

#[test]
fn a_record_whose_pads_moved_keeps_its_copper_and_says_where_it_is_open() {
    // Issue #1328: the LED moved down 0.75 mm after its copper was imported.
    let dir = imported("routes-stale");
    let layout = dir.join("led.pcb.toml");
    let text_of = fs::read_to_string(&layout).unwrap();
    let moved = text_of.replace("at = [9.85, 4.6]", "at = [9.85, 5.35]");
    assert_ne!(moved, text_of);
    fs::write(&layout, moved).unwrap();

    // Every shape is kept; the short track into the LED's old pad is open at it.
    let shown = ok(&netsmith(&dir, &["routes", "show", "--file", "led.nsm"]));
    assert!(
        shown.contains(
            "the pads of net `GND` are not where the routes file's record was written: its 4 shape(s) are kept"
        ),
        "{shown}"
    );
    assert!(shown.contains("open at D1.1 (track 3's end at 9.9, 5.4)"), "{shown}");
    assert!(shown.contains("net GND: D1.1 to J1.2"), "{shown}");

    // `fab` routes what the kept copper leaves; the corner the kept copper now has outside
    // the moved pad is the design rules check's to find.
    let run = netsmith(&dir, &["board", "fab", "led.nsm", "--locked", "--out", "moved"]);
    let said = text(&run);
    assert!(said.contains("its 4 shape(s) are kept"), "{said}");
    assert!(said.contains("LedExample: routing: 2 connections"), "{said}");
    assert!(said.contains("[track_angle] two tracks of net GND"), "{said}");

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
        &["routes", "delete", "--net", "GND", "--track", "3", "--file", "led.nsm"],
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
        said.contains("deleted net OLD's record (4 shape(s)), a net the board does not have"),
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
    // The crossing is at (5.0, 5.4) from the outline's corner, not (25.0, 25.4) on KiCad's page.
    assert!(said.contains("at (5.000, 5.400) on F.Cu"), "{said}");
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
