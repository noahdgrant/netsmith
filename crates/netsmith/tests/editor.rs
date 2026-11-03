//! The editor's backend over a project on disk, and the whole server over a socket.

use std::{
    fs,
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::Arc,
    sync::Mutex,
    thread,
    time::Duration,
};

mod common;

use common::endpoints::{Fake, Reply, Seen};
use netsmith::editor::Workspace;
use netsmith::project::Target;
use netsmith_editor::{
    Backend, Say,
    api::{
        Budget, DrawnKeepout, Drc, EdgeKind, FileRole, FindingSource, KeepoutOrigin,
        KeepoutPlacement, KicadState, PartPlacement, PcbChange, PcbEdit, PcbScene, PcbSide,
        Severity,
    },
};
use netsmith_pcb::FootprintLibrary;

const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");
const MODULE: &str = "//! A module.\n\nimport std::sources::Rail\n\n/// A supply.\ninterface Supply {\n    .rail: Rail\n}\n";

/// A project: `two_boards.nsm` as its one top-level file, and a module beside it.
fn project(name: &str) -> (PathBuf, Workspace) {
    let root = std::env::temp_dir().join(format!(
        "netsmith-editor-test-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), TWO_BOARDS).unwrap();
    fs::write(root.join("src/power.nsm"), MODULE).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

fn line_of(text: &str, needle: &str) -> u32 {
    text.lines().position(|l| l.contains(needle)).unwrap() as u32 + 1
}

#[test]
fn a_project_that_never_built_has_problems_and_no_boards() {
    let root =
        std::env::temp_dir().join(format!("netsmith-editor-test-{}-never", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"demo\"\n").unwrap();
    fs::write(
        root.join("src/main.nsm"),
        TWO_BOARDS.replace(".r1: Resistor {}", ".r1: Resistr {}"),
    )
    .unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    let analysis = editor.analysis();
    assert!(analysis.boards.is_empty() && analysis.systems.is_empty());
    assert_eq!(analysis.errors, 1, "{:?}", analysis.problems);
    // Every view says there is nothing to show, rather than failing.
    assert_eq!(editor.schematic("TwoBoards/led").unwrap_err().status, 404);
    assert_eq!(editor.power("TwoBoards").unwrap_err().status, 404);
    // The file can still be read and fixed in the editor.
    assert!(
        editor
            .read_file("src/main.nsm")
            .unwrap()
            .text
            .contains("Resistr")
    );
}

#[test]
fn the_tree_lists_files_by_role_with_their_definitions() {
    let (_, editor) = project("tree");
    let tree = editor.project();
    assert_eq!(tree.title, "demo");
    assert!(tree.has_manifest);
    // The page is told whether there is a KiCad to run the design rules check.
    assert_eq!(tree.kicad, netsmith_fab::KicadCliExe::locate().is_some());
    let role = |r: FileRole| -> Vec<&str> {
        tree.files
            .iter()
            .filter(|f| f.role == r)
            .map(|f| f.path.as_str())
            .collect()
    };
    assert_eq!(role(FileRole::TopLevel), ["src/main.nsm"]);
    assert_eq!(role(FileRole::Module), ["src/power.nsm"]);
    assert_eq!(role(FileRole::Manifest), ["netsmith.toml"]);
    assert!(role(FileRole::Std).contains(&"std/sources.nsm"));
    let main = &tree.files[0];
    let defs: Vec<(&str, &str)> = main
        .defs
        .iter()
        .map(|d| (d.kind.as_str(), d.name.as_str()))
        .collect();
    assert_eq!(
        defs,
        [
            ("module", "LdoStage"),
            ("board", "PowerBoard"),
            ("board", "LedBoard"),
            ("system", "TwoBoards")
        ]
    );
    assert_eq!(main.defs[3].line, line_of(TWO_BOARDS, "system TwoBoards"));
    assert!(
        tree.files
            .iter()
            .filter(|f| f.role == FileRole::Std)
            .all(|f| f.readonly)
    );
}

#[test]
fn only_the_files_of_the_tree_can_be_read_or_written() {
    let (root, editor) = project("jail");
    fs::write(root.join("secret.txt"), "no").unwrap();
    for path in [
        "../x",
        "/etc/passwd",
        "secret.txt",
        "src/../secret.txt",
        "src/missing.nsm",
    ] {
        let e = editor.read_file(path).unwrap_err();
        assert_eq!(e.status, 404, "{path}");
        assert!(
            e.message.contains("is not a file of this project"),
            "{path}: {}",
            e.message
        );
        assert_eq!(
            editor.write_file(path, "x", "0").unwrap_err().status,
            404,
            "{path}"
        );
    }
    // The library and the manifest are shown and never written.
    for path in ["std/sources.nsm", "netsmith.toml"] {
        let file = editor.read_file(path).unwrap();
        assert!(file.readonly && !file.text.is_empty(), "{path}");
        let e = editor.write_file(path, "x", &file.stamp).unwrap_err();
        assert_eq!(e.status, 403, "{path}");
        assert!(e.message.contains("read-only"), "{}", e.message);
    }
    assert_eq!(fs::read_to_string(root.join("secret.txt")).unwrap(), "no");
}

#[test]
fn a_save_writes_the_file_and_is_refused_if_the_file_changed_meanwhile() {
    let (root, editor) = project("save");
    let file = editor.read_file("src/power.nsm").unwrap();
    assert_eq!(file.text, MODULE);
    let saved = editor
        .write_file("src/power.nsm", "// first\n", &file.stamp)
        .unwrap();
    assert_eq!(
        fs::read_to_string(root.join("src/power.nsm")).unwrap(),
        "// first\n"
    );
    assert_eq!(
        editor.read_file("src/power.nsm").unwrap().stamp,
        saved.stamp
    );
    let names: Vec<String> = fs::read_dir(root.join("src"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names.len(),
        2,
        "no temporary file is left behind: {names:?}"
    );

    // Another program writes; the page's next save carries the stamp it read.
    fs::write(root.join("src/power.nsm"), "// by someone else, longer\n").unwrap();
    let e = editor
        .write_file("src/power.nsm", "// second\n", &saved.stamp)
        .unwrap_err();
    assert_eq!(e.status, 409);
    assert!(e.message.contains("changed on disk"), "{}", e.message);
    assert_eq!(
        fs::read_to_string(root.join("src/power.nsm")).unwrap(),
        "// by someone else, longer\n"
    );
}

#[test]
fn a_check_of_unsaved_text_reports_the_real_message_where_it_is_and_writes_nothing() {
    let (root, editor) = project("check");
    let before = editor.analysis();
    assert_eq!(before.errors, 0, "{:?}", before.problems);
    let text = TWO_BOARDS.replace(".r1: Resistor {}", ".r1: Resistr {}");
    let checked = editor.check("src/main.nsm", &text).unwrap();
    assert_eq!(checked.errors, 1, "{:?}", checked.problems);
    let p = &checked.problems[0];
    assert_eq!(p.severity, Severity::Error);
    assert!(p.message.contains("Resistr"), "{}", p.message);
    let at = p.at.as_ref().unwrap();
    assert_eq!(at.file, "src/main.nsm");
    assert_eq!(at.line, line_of(&text, "Resistr"));
    assert_eq!(
        at.column, 10,
        "the column of the type, as the compiler prints it"
    );
    assert!(
        p.rendered.contains("Resistr") && p.rendered.contains("src/main.nsm"),
        "{}",
        p.rendered
    );
    // Nothing was written, and the build of the disk is as it was.
    assert_eq!(
        fs::read_to_string(root.join("src/main.nsm")).unwrap(),
        TWO_BOARDS
    );
    assert_eq!(editor.analysis().version, before.version);

    // A module's unsaved text is read by the files that import it.
    let module = "//! A module.\n\ninterface Supply {\n    .rail: Nothing\n}\n";
    let checked = editor.check("src/power.nsm", module).unwrap();
    assert_eq!(checked.errors, 1, "{:?}", checked.problems);
    assert_eq!(
        checked.problems[0].at.as_ref().unwrap().file,
        "src/power.nsm"
    );
    assert_eq!(editor.check("std/sources.nsm", "").unwrap_err().status, 400);
}

#[test]
fn a_part_knows_where_it_and_what_contains_it_are_declared() {
    let (_, editor) = project("detail");
    let power = editor.board("TwoBoards/power").unwrap();
    let regulator = power
        .parts
        .iter()
        .find(|p| p.path == "stage.regulator")
        .unwrap();
    assert_eq!(regulator.kind, "LinearRegulator");
    assert_eq!(
        regulator.decl.as_ref().unwrap().line,
        line_of(TWO_BOARDS, ".regulator:")
    );
    assert_eq!(regulator.scopes.len(), 1);
    assert_eq!(
        regulator.scopes[0].line,
        line_of(TWO_BOARDS, ".stage: LdoStage")
    );
    let header = power.parts.iter().find(|p| p.path == "power_in").unwrap();
    assert!(header.scopes.is_empty() && !header.assembled);
    assert!(power.nets.iter().any(|n| n.name == "3V3" && n.named));
    let missing = editor.board("Nope").unwrap_err();
    assert_eq!(missing.status, 404);
    assert!(missing.message.contains("Nope"));
}

#[test]
fn the_power_tree_is_read_from_the_solved_model() {
    let (_, editor) = project("power");
    let tree = editor.power("TwoBoards").unwrap();
    let node = |id: &str| {
        tree.nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap_or_else(|| panic!("{id}: {tree:?}"))
    };

    let out = node("power:3V3");
    assert_eq!(out.voltage.as_ref().unwrap().text, "3.135V..3.465V");
    assert_eq!(
        out.demand.as_ref().unwrap().hi,
        0.05,
        "what the board is built to give"
    );
    assert_eq!(
        out.capacity.as_ref().unwrap().hi,
        0.1,
        "the regulator's max_current, once"
    );
    assert!(
        out.capacity_from
            .as_ref()
            .unwrap()
            .contains("stage.regulator")
    );
    assert_eq!(out.budget, Budget::Within);
    assert_eq!(out.supplies[0].who, "stage.vout");
    assert_eq!(out.loads[0].who, "output");
    assert!(out.loads[0].at.as_ref().unwrap().line > 1);

    let vin = node("power:VIN");
    assert_eq!(vin.voltage.as_ref().unwrap().text, "4.75V..5.25V");
    assert_eq!(vin.capacity, None);
    assert_eq!(
        vin.budget,
        Budget::Unlimited,
        "supplied from outside, no limit stated"
    );

    let led = node("led:3V3");
    // What the board really draws: the stated window of the LED's current narrowed to what
    // the resistor it got lets through (ADR 0025), not the window itself.
    assert_eq!(led.demand.as_ref().unwrap().text, "7.277mA..16.82mA");

    let kinds: Vec<(&str, &str, EdgeKind)> = tree
        .edges
        .iter()
        .map(|e| (e.from.as_str(), e.to.as_str(), e.kind))
        .collect();
    assert!(
        kinds.contains(&("power:VIN", "power:3V3", EdgeKind::Converts)),
        "{kinds:?}"
    );
    assert!(
        kinds.contains(&("power:3V3", "led:3V3", EdgeKind::Join)),
        "{kinds:?}"
    );
    let converts = tree
        .edges
        .iter()
        .find(|e| e.kind == EdgeKind::Converts)
        .unwrap();
    assert_eq!(
        converts.label, "stage (LdoStage)",
        "the stage, not the regulator in it"
    );

    // One board alone has no joins.
    let alone = editor.power("TwoBoards/led").unwrap();
    assert_eq!(alone.nodes.len(), 1);
    assert!(alone.edges.is_empty());
}

#[test]
fn the_system_view_shows_boards_ports_and_joins_or_the_members_of_a_board() {
    let (_, editor) = project("system");
    let system = editor.system("TwoBoards").unwrap();
    assert_eq!(system.kind, "system");
    let names: Vec<&str> = system.nodes.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["power", "led"]);
    let power = &system.nodes[0];
    assert_eq!(power.type_name, "PowerBoard");
    let ports: Vec<&str> = power.ports.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(ports, ["input", "output"], "a process is not a port");
    assert_eq!(power.ports[1].voltage.as_deref(), Some("3.135V..3.465V"));
    assert_eq!(system.edges.len(), 1);
    assert_eq!(
        (system.edges[0].from.as_str(), system.edges[0].to.as_str()),
        ("power", "led")
    );
    assert_eq!(system.edges[0].label, "power.output ~ led.input");

    // The join is the mate's (ADR 0040): the view lists the mated connectors with the role of
    // each contact, and the ground the mate joins.
    assert_eq!(system.mates.len(), 1);
    let mate = &system.mates[0];
    assert_eq!(
        (mate.a.as_str(), mate.b.as_str()),
        ("power.power_out", "led.power_in")
    );
    assert_eq!(
        (mate.a_board.as_str(), mate.b_board.as_str()),
        ("power", "led")
    );
    let contacts: Vec<(u32, &str, &str, Option<&str>)> = mate
        .contacts
        .iter()
        .map(|c| {
            (
                c.number,
                c.a_role.as_str(),
                c.b_role.as_str(),
                c.b_net.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        contacts,
        [
            (1, "power", "power", Some("3V3")),
            (2, "ground", "ground", Some("GND"))
        ]
    );
    assert_eq!(mate.ports, ["power.output ~ led.input"]);
    assert!(mate.reversible.is_none() && mate.at.is_some());
    assert_eq!(system.ground_groups.len(), 1);
    assert_eq!(system.ground_groups[0].nets, ["led.GND", "power.GND"]);
    assert_eq!(
        system.ground_groups[0].contacts,
        ["power.power_out / led.power_in: 1"]
    );

    let board = editor.system("TwoBoards/led").unwrap();
    assert!(board.mates.is_empty() && board.ground_groups.is_empty());
    assert_eq!(board.kind, "board");
    let members: Vec<(&str, &str)> = board
        .nodes
        .iter()
        .map(|n| (n.name.as_str(), n.kind.as_str()))
        .collect();
    assert_eq!(
        members,
        [
            ("input", "port"),
            ("led", "part"),
            ("r1", "part"),
            ("power_in", "part")
        ]
    );
    assert!(
        board.edges.iter().any(|e| e.from == "led" && e.to == "r1"),
        "{:?}",
        board.edges
    );
    // Ground joins everything, so it is not an edge.
    assert!(board.edges.iter().all(|e| e.label != "GND"));
}

#[test]
fn the_bom_groups_parts_like_the_fab_bom_with_price_and_stock() {
    let (_, editor) = project("bom");
    let bom = editor.bom("TwoBoards/led").unwrap();
    assert_eq!(bom.lines.len(), 2);
    let led = bom.lines.iter().find(|l| l.kind == "Led").unwrap();
    assert_eq!(led.designators, ["D1"]);
    assert_eq!(led.lcsc.as_deref(), Some("C2286"));
    assert_eq!(led.library.as_deref(), Some("Basic"));
    // The standard catalogs ship no price, stock or date: unknown, never zero, and no total.
    assert_eq!(
        (led.unit_usd, led.stock, led.date.as_deref()),
        (None, None, None)
    );
    assert_eq!(led.line_usd, None);
    assert_eq!(bom.hand.len(), 1);
    assert_eq!(bom.hand[0].designators, ["J1"]);
    // The socket is soldered by hand and still sourced: the `header` catalog rates its contacts.
    assert!(bom.hand[0].hand_solder);
    assert_eq!(bom.hand[0].lcsc.as_deref(), Some("C42431841"));
    assert_eq!(bom.total_usd, None);
    assert!(bom.note.contains("feeder fee"));

    // Two capacitors of one value and part share a line with both designators.
    let power = editor.bom("TwoBoards/power").unwrap();
    assert_eq!(power.lines.iter().map(|l| l.quantity).sum::<u32>(), 3);
}

#[test]
fn go_to_definition_follows_imports_types_and_members() {
    let (_, editor) = project("definition");
    let at = |needle: &str, nth: usize| TWO_BOARDS.match_indices(needle).nth(nth).unwrap().0;
    let find = |offset: usize| {
        editor
            .definition("src/main.nsm", TWO_BOARDS, offset)
            .unwrap()
    };

    // A type used in a declaration, defined in the library.
    let resistor = find(at("Resistor", 1) + 2).expect("a definition");
    assert_eq!(resistor.at.file, "std/parts/passive.nsm");
    let passive = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std/parts/passive.nsm"),
    )
    .unwrap();
    assert_eq!(resistor.at.line, line_of(&passive, "part Resistor"));

    // The same name in an import.
    let imported = find(at("Resistor", 0) + 1).unwrap();
    assert_eq!(imported.at, resistor.at);

    // The module in an import goes to its file.
    let module = find(at("passive", 0) + 1).unwrap();
    assert_eq!(module.at.file, "std/parts/passive.nsm");

    // A type of this file.
    let ldo = find(at("LdoStage", 1) + 1).unwrap();
    assert_eq!(
        (ldo.at.file.as_str(), ldo.at.line),
        ("src/main.nsm", line_of(TWO_BOARDS, "module LdoStage"))
    );

    // A member of the board, and a member of a member's type.
    let regulator = find(at(".stage.vout", 0) + 2).unwrap();
    assert_eq!(regulator.at.line, line_of(TWO_BOARDS, ".stage: LdoStage"));
    let vout = find(at(".stage.vout", 0) + 8).unwrap();
    assert_eq!(
        (vout.at.file.as_str(), vout.at.line),
        ("src/main.nsm", line_of(TWO_BOARDS, ".vout: Rail"))
    );

    // Nothing to find on a number or a comment.
    assert_eq!(find(at("1.6mm", 0) + 1), None);
    assert_eq!(find(at("Two boards in one", 0) + 3), None);
}

#[test]
fn the_pcb_is_placed_at_once_and_routed_on_demand() {
    if !FootprintLibrary::installed().root().exists() {
        eprintln!(
            "skipped: no KiCad footprint libraries at {}",
            FootprintLibrary::installed().root().display()
        );
        return;
    }
    let (_, editor) = project("pcb");
    let placed = editor.pcb_scene("TwoBoards/led").unwrap().svg;
    assert!(placed.contains("class=\"l-pads\"") && placed.contains("class=\"l-silk\""));
    let (say, heard) = listener();
    let routed = editor.route("TwoBoards/led", &say).unwrap();
    let heard = heard.lock().unwrap().join("\n");
    assert!(
        heard.contains("placed 3 parts")
            && heard.contains("routing")
            && heard.contains("routed in"),
        "the stages are told: {heard}"
    );
    assert!(!routed.stopped && routed.elapsed_ms < 600_000);
    assert!(
        routed.routed && routed.unrouted.is_empty() && routed.placement_problems.is_empty(),
        "{routed:?}"
    );
    assert!(routed.tracks > 0);
    let after = editor.pcb_scene("TwoBoards/led").unwrap().svg;
    assert!(
        after.matches("<line").count() > placed.matches("<line").count(),
        "tracks are drawn"
    );
    assert_eq!(editor.route("Nope", &say).unwrap_err().status, 404);
}

/// A `Say` that keeps what it hears.
fn listener() -> (Say, Arc<Mutex<Vec<String>>>) {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&heard);
    let say: Say = Arc::new(move |m: &str| sink.lock().unwrap().push(m.to_string()));
    (say, heard)
}

#[test]
fn the_editor_answers_while_a_board_is_being_routed_and_while_fab_runs() {
    if !FootprintLibrary::installed().root().exists() {
        return;
    }
    let (_, editor) = project("busy");
    let editor = Arc::new(editor);
    // What the job says is told from inside the job, on its thread: if the job held the lock
    // the editor keeps its state behind, a request made from there would never be answered.
    let answered = Arc::new(Mutex::new(Vec::new()));
    let say: Say = {
        let (editor, answered) = (Arc::clone(&editor), Arc::clone(&answered));
        Arc::new(move |_: &str| {
            let analysis = editor.analysis();
            let drawing = editor.schematic("TwoBoards/led");
            answered
                .lock()
                .unwrap()
                .push((analysis.errors, drawing.is_ok()));
        })
    };
    let report = editor.route("TwoBoards/led", &say).unwrap();
    assert!(report.routed);
    let build = editor.build(&say);
    assert!(build.ok, "{}", build.output);
    let answered = answered.lock().unwrap();
    assert!(answered.len() >= 4, "{answered:?}");
    assert!(
        answered
            .iter()
            .all(|&(errors, drawing)| errors == 0 && drawing)
    );
}

#[test]
fn a_route_can_be_cancelled_and_keeps_what_it_has() {
    if !FootprintLibrary::installed().root().exists() {
        return;
    }
    let (_, editor) = project("cancel");
    let (say, _) = listener();
    // The cancel flag is cleared when a route starts, so one before it changes nothing.
    editor.cancel_route();
    let report = editor.route("TwoBoards/led", &say).unwrap();
    assert!(report.routed && !report.stopped, "{report:?}");
}

#[test]
fn build_writes_a_schematic_for_each_board_of_the_system() {
    let (root, editor) = project("build");
    let (say, heard) = listener();
    let report = editor.build(&say);
    assert!(
        heard
            .lock()
            .unwrap()
            .iter()
            .any(|m| m.contains("writing the schematic"))
    );
    assert!(report.ok, "{}", report.output);
    assert_eq!(report.written.len(), 2);
    assert!(
        root.join("out/TwoBoards/power/PowerBoard.kicad_sch")
            .is_file()
    );
    assert!(root.join("out/TwoBoards/led/LedBoard.kicad_pro").is_file());
    assert!(report.output.contains("board TwoBoards/led"));
}

#[test]
fn fab_makes_the_package_and_offers_its_files() {
    if !FootprintLibrary::installed().root().exists() {
        return;
    }
    let (root, editor) = project("fab");
    let (say, heard) = listener();
    let report = editor.fab("TwoBoards/led", false, &say).unwrap();
    // The stages `netsmith fab` prints are told to the page.
    assert!(
        heard.lock().unwrap().iter().any(|m| m.contains("placing")),
        "{:?}",
        heard.lock().unwrap()
    );
    assert!(report.elapsed_ms > 0);
    assert!(report.ok, "{}", report.output);
    assert_eq!(report.drc, Drc::Passed);
    assert!(
        report.notes.contains("fab-notes for LedBoard"),
        "{}",
        report.notes
    );
    let keys: Vec<&str> = report.downloads.iter().map(|d| d.key.as_str()).collect();
    assert_eq!(keys, ["gerbers", "bom", "cpl", "notes", "pcb"]);
    let zip = editor.download("TwoBoards/led", "gerbers").unwrap();
    assert_eq!(zip.content_type, "application/zip");
    assert_eq!(&zip.bytes[..2], b"PK");
    assert!(root.join("fab/TwoBoards/led").is_dir());
    assert_eq!(
        editor.download("TwoBoards/led", "nope").unwrap_err().status,
        400
    );
    // `fab` makes every board of the file, so the other board's files are there too.
    assert!(editor.download("TwoBoards/power", "bom").is_ok());
    assert_eq!(editor.download("Nope", "bom").unwrap_err().status, 404);
}

/// The same project with an order of the two boards, for the build panel's orders.
fn project_with_order(name: &str, orders: &str) -> (PathBuf, Workspace) {
    let (root, _) = project(name);
    let manifest = fs::read_to_string(root.join("netsmith.toml")).unwrap();
    fs::write(root.join("netsmith.toml"), format!("{manifest}{orders}")).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

#[test]
fn the_orders_of_the_manifest_are_listed_with_their_boards_and_quantities() {
    let (_, editor) = project_with_order(
        "orders-list",
        "[order.devkit.boards]\n\"TwoBoards/led\" = 5\nTwoBoards = 2\n",
    );
    let orders = editor.orders();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].name, "devkit");
    let boards: Vec<(&str, u32)> = orders[0]
        .boards
        .iter()
        .map(|b| (b.name.as_str(), b.quantity))
        .collect();
    assert_eq!(boards, [("TwoBoards/led", 5), ("TwoBoards", 2)]);
    let (_, none) = project("orders-none");
    assert!(none.orders().is_empty());

    let (say, _) = listener();
    assert_eq!(
        editor.fab_order("nope", false, &say).unwrap_err().status,
        404
    );
    assert_eq!(
        editor
            .order_download("devkit", "summary")
            .unwrap_err()
            .status,
        404
    );
}

#[test]
fn an_order_with_an_unknown_board_stops_with_the_line_and_the_boards_that_exist() {
    let (root, editor) = project_with_order("orders-bad", "[order.d.boards]\nTwoBoard = 1\n");
    let (say, _) = listener();
    let report = editor.fab_order("d", false, &say).unwrap();
    assert!(!report.ok);
    assert!(
        report
            .output
            .contains("netsmith.toml:5: order `d` names `TwoBoard`")
            && report
                .output
                .contains("the boards and panels are: TwoBoards/power, TwoBoards/led"),
        "{}",
        report.output
    );
    assert!(report.downloads.is_empty());
    assert!(!root.join("fab").exists());
}

#[test]
fn an_order_is_made_and_its_summary_and_bom_are_offered() {
    if !FootprintLibrary::installed().root().exists() {
        return;
    }
    let (root, editor) =
        project_with_order("orders-make", "[order.d.boards]\n\"TwoBoards/led\" = 5\n");
    let (say, heard) = listener();
    let report = editor.fab_order("d", false, &say).unwrap();
    assert!(report.ok, "{}", report.output);
    assert!(
        heard.lock().unwrap().iter().any(|m| m.contains("placing")),
        "{:?}",
        heard.lock().unwrap()
    );
    assert!(
        report
            .summary
            .contains("order d: 1 board design, 5 boards in all"),
        "{}",
        report.summary
    );
    assert!(root.join("fab/orders/d/TwoBoards/led/bom.csv").is_file());
    assert!(!root.join("fab/orders/d/TwoBoards/power").exists());
    let keys: Vec<&str> = report.downloads.iter().map(|d| d.key.as_str()).collect();
    assert_eq!(keys, ["summary", "bom"]);
    let bom = editor.order_download("d", "bom").unwrap();
    assert_eq!(bom.name, "order-bom.csv");
    assert!(
        String::from_utf8(bom.bytes)
            .unwrap()
            .contains(",C2286,5,5 x 1 TwoBoards/led")
    );
    assert_eq!(editor.order_download("d", "nope").unwrap_err().status, 400);
}

// ---- the whole server -------------------------------------------------------------------

fn http(port: u16, method: &str, target: &str, extra: &str, body: &str) -> (u16, String) {
    let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    write!(
        s,
        "{method} {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Length: {}\r\n{extra}\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut raw = String::new();
    s.read_to_string(&mut raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap();
    (
        head.split(' ').nth(1).unwrap().parse().unwrap(),
        body.to_string(),
    )
}

#[test]
fn the_server_serves_the_project_and_tells_the_page_when_a_file_changes() {
    let (root, editor) = project("served");
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || netsmith_editor::serve(listener, Arc::new(editor)));

    let (status, body) = http(port, "GET", "/api/analysis", "", "");
    assert_eq!(status, 200);
    assert!(
        body.contains("\"errors\":0") && body.contains("TwoBoards/power"),
        "{body}"
    );
    let (_, svg) = http(
        port,
        "GET",
        "/api/schematic.svg?board=TwoBoards%2Fled",
        "",
        "",
    );
    assert!(
        svg.contains("data-ref=\"R1\"") && svg.contains("data-board=\"LedBoard\""),
        "{}",
        &svg[..200.min(svg.len())]
    );

    // Every read of the socket gives up after 20 seconds, so a push that never comes fails.
    let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let (mut socket, _) =
        tungstenite::client(format!("ws://127.0.0.1:{port}/api/events").as_str(), stream).unwrap();
    // The page saves a mistake; the server builds again and pushes the problems.
    let (_, file) = http(port, "GET", "/api/file?path=src/main.nsm", "", "");
    let stamp = file
        .split("\"stamp\":\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .to_string();
    let broken = TWO_BOARDS.replace(".r1: Resistor {}", ".r1: Resistr {}");
    let (status, _) = http(
        port,
        "PUT",
        "/api/file?path=src/main.nsm",
        &format!("X-Netsmith: 1\r\nX-Stamp: {stamp}\r\n"),
        &broken,
    );
    assert_eq!(status, 200);
    assert_eq!(
        fs::read_to_string(root.join("src/main.nsm")).unwrap(),
        broken
    );
    loop {
        let message = socket
            .read()
            .unwrap_or_else(|e| panic!("no `rebuilt` push arrived within 20 seconds: {e}"));
        if let tungstenite::Message::Text(t) = message {
            let v: serde_json::Value = serde_json::from_str(&t).unwrap();
            if v["type"] == "rebuilt" {
                assert_eq!(v["analysis"]["errors"], 1, "{v}");
                assert!(
                    v["analysis"]["problems"][0]["message"]
                        .as_str()
                        .unwrap()
                        .contains("Resistr")
                );
                // The last good drawing is kept, marked stale.
                assert_eq!(v["analysis"]["boards"][0]["stale"], true);
                break;
            }
        }
    }
}

#[test]
fn the_bom_reads_price_and_stock_from_the_projects_own_catalog_files_too() {
    let root = std::env::temp_dir().join(format!(
        "netsmith-editor-test-{}-catalog",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("catalogs")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"own\"\ncatalogs = [\"catalogs/resistor.csv\"]\n",
    )
    .unwrap();
    // A resistor only this project has, with a number that is not a real LCSC part.
    fs::write(
        root.join("catalogs/resistor.csv"),
        "lcsc,mpn,resistance,max_power,package,library,price,stock,date,source\n\
         C9999999,ZZR-316,316ohm +/- 1%,250mW,0402,Extended,0.0123,42,2026-10-31,https://example.invalid/zzr\n",
    )
    .unwrap();
    let led = include_str!("../../../examples/led.nsm").replace(
        ".r1: Resistor {}",
        ".r1: Resistor { .resistance &= 316ohm +/- 1% }",
    );
    assert_ne!(led, include_str!("../../../examples/led.nsm"));
    fs::write(root.join("src/main.nsm"), led).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    let bom = editor.bom(&analysis.boards[0].name).unwrap();
    let resistor = bom.lines.iter().find(|l| l.kind == "Resistor").unwrap();
    assert_eq!(resistor.lcsc.as_deref(), Some("C9999999"));
    assert_eq!(resistor.unit_usd, Some(0.0123));
    assert_eq!(resistor.stock, Some(42));
    assert_eq!(resistor.library.as_deref(), Some("Extended"));
    // The standard row for the LED is still there; it states no price or stock, so only the
    // project's own resistor is in the total.
    assert!(
        bom.lines
            .iter()
            .any(|l| l.kind == "Led" && l.lcsc.is_some() && l.stock.is_none())
    );
    assert_eq!(bom.total_usd, Some(0.0123));

    // The catalog file is watched: a change to it builds again.
    let before = editor.analysis().version;
    fs::write(
        root.join("catalogs/resistor.csv"),
        "lcsc,mpn,resistance,max_power,package,library,price,stock,date,source\n\
         C9999999,ZZR-316,316ohm +/- 1%,250mW,0402,Extended,0.0456,7,2026-11-01,https://example.invalid/zzr\n",
    )
    .unwrap();
    assert!(editor.analysis().version > before);
    let bom = editor.bom(&editor.analysis().boards[0].name).unwrap();
    let resistor = bom.lines.iter().find(|l| l.kind == "Resistor").unwrap();
    assert_eq!((resistor.unit_usd, resistor.stock), (Some(0.0456), Some(7)));
}

/// A project whose one catalog file has a part only it has, a day-old row for the LED example.
fn project_with_priced_catalog(name: &str) -> (PathBuf, String) {
    let root = std::env::temp_dir().join(format!(
        "netsmith-editor-test-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("catalogs")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"own\"\ncatalogs = [\"catalogs/resistor.csv\"]\n",
    )
    .unwrap();
    let catalog = "lcsc,mpn,resistance,max_power,package,library,price,stock,date,checked,source\n\
         C9999999,ZZR-316,316ohm +/- 1%,250mW,0402,Extended,0.0123,42,2026-10-31,,https://example.invalid/zzr\n"
        .to_string();
    fs::write(root.join("catalogs/resistor.csv"), &catalog).unwrap();
    let led = include_str!("../../../examples/led.nsm").replace(
        ".r1: Resistor {}",
        ".r1: Resistor { .resistance &= 316ohm +/- 1% }",
    );
    fs::write(root.join("src/main.nsm"), led).unwrap();
    (root, catalog)
}

/// Answers for the invented part only this project has. LCSC lists other numbers than the row:
/// stock 0 and a price of 0.02, neither of which may reach the file.
fn zzr_server() -> Fake {
    Fake::start(|r: &Seen| {
        if r.lcsc() != "C9999999" {
            return Reply::ok(r#"{"code":200,"msg":null,"result":null,"ok":true}"#);
        }
        Reply::ok(
            r#"{"code":200,"msg":null,"ok":true,"result":{"productCode":"C9999999","productModel":"ZZR-316",
            "encapStandard":"0402","stockNumber":0,"productPriceList":[{"ladder":1,"usdPrice":0.02}]}}"#,
        )
    })
}

/// 2026-11-02 12:00 UTC.
const NOW: u64 = 1_793_620_800;

#[test]
fn the_bom_dates_each_price_and_marks_the_old_ones_stale() {
    let (root, _) = project_with_priced_catalog("bom-dates");
    let open = |now: u64| {
        let fake = zzr_server();
        let network = fake.network(None, now);
        Workspace::open(Target::resolve_from(&root, None).unwrap())
            .unwrap()
            .with_network(network)
    };
    let editor = open(NOW);
    let name = editor.analysis().boards[0].name.clone();
    let bom = editor.bom(&name).unwrap();
    assert_eq!(bom.stale_after_days, 90);
    assert_eq!(bom.catalogs, ["catalogs/resistor.csv"]);
    let resistor = bom.lines.iter().find(|l| l.kind == "Resistor").unwrap();
    assert_eq!(resistor.date.as_deref(), Some("2026-10-31"));
    assert_eq!((resistor.age_days, resistor.stale), (Some(2), false));
    // A standard row ships no date, so its age is unknown and a sourced part is stale.
    let led = bom.lines.iter().find(|l| l.kind == "Led").unwrap();
    assert_eq!(
        (led.date.as_deref(), led.age_days, led.stale),
        (None, None, true)
    );
    assert!(bom.note.contains("Check parts against LCSC"));
    assert!(
        bom.note.contains("JLCPCB's numbers, entered by hand")
            && bom.note.contains("never writes a price or stock"),
        "{}",
        bom.note
    );

    // 91 days after 2026-10-31 is 2027-01-30.
    let later = open(NOW + 91 * 86_400);
    let bom = later.bom(&name).unwrap();
    let resistor = bom.lines.iter().find(|l| l.kind == "Resistor").unwrap();
    assert_eq!((resistor.age_days, resistor.stale), (Some(93), true));
    let led = bom.lines.iter().find(|l| l.kind == "Led").unwrap();
    assert_eq!((led.age_days, led.stale), (None, true));
    // A part by hand has a row too (the `header` catalog, which ships no date: stale as well).
    assert!(
        bom.hand
            .iter()
            .all(|l| l.date.is_none() && l.age_days.is_none() && l.stale),
        "{:?}",
        bom.hand
    );
}

#[test]
fn the_check_runs_the_command_over_the_projects_catalogs_and_writes_only_checked_so_the_bom_is_unchanged()
 {
    let (root, original) = project_with_priced_catalog("refresh");
    let fake = zzr_server();
    let cache = root.join("cache");
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap()
        .with_network(fake.network(Some(&cache), NOW));
    let name = editor.analysis().boards[0].name.clone();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&heard);
    let say: Say = Arc::new(move |m: &str| sink.lock().unwrap().push(m.to_string()));

    // A preview writes nothing.
    let preview = editor.refresh_catalogs(true, false, &say).unwrap();
    assert_eq!(preview.error, None);
    assert!(preview.dry_run && !preview.files[0].written);
    assert_eq!(
        fs::read_to_string(root.join("catalogs/resistor.csv")).unwrap(),
        original
    );
    assert_eq!(*heard.lock().unwrap(), ["resistor 1/1: C9999999"]);
    // The page is told what is never written and why, in the command's words.
    assert_eq!(preview.notice, netsmith::refresh::NOTICE);
    assert!(preview.notice.contains("JLCPCB data is not fetched"));
    assert!(
        preview
            .notice
            .contains("Never written: price, stock, price_breaks, library, date.")
    );
    assert_eq!(
        (preview.files[0].looked_up, preview.files[0].verified),
        (1, 1)
    );
    let row = &preview.files[0].rows[0];
    assert_eq!(
        (row.status.as_str(), row.lcsc.as_str()),
        ("VERIFIED", "C9999999")
    );
    // LCSC lists stock 0 where the row has 42: information, with the row's cells as they were.
    assert_eq!(
        row.notes,
        [
            "LCSC lists stock 0 where the row has stock 42: the row is not changed; check JLCPCB's parts page by hand"
        ]
    );
    let changes: Vec<_> = row
        .changes
        .iter()
        .map(|c| (c.column.as_str(), c.from.as_str(), c.to.as_str()))
        .collect();
    assert_eq!(changes, [("checked", "", "2026-11-02")]);

    // The real run rewrites the file, and the editor sees the change.
    let before = editor.analysis().version;
    let done = editor.refresh_catalogs(false, false, &say).unwrap();
    assert!(done.files[0].written && done.error.is_none());
    assert!(
        done.summary.contains("resistor.csv") || done.summary.contains("catalogs/resistor.csv"),
        "{}",
        done.summary
    );
    let written = fs::read_to_string(root.join("catalogs/resistor.csv")).unwrap();
    assert_eq!(
        written,
        original.replace("0.0123,42,2026-10-31,,", "0.0123,42,2026-10-31,2026-11-02,")
    );
    assert!(editor.analysis().version > before);
    // Price, stock and their date are the hand-entered ones: LCSC's 0.02 and stock 0 are not
    // in the file, the solver still offers the row, and the line is as old as it was.
    let resistor = editor
        .bom(&name)
        .unwrap()
        .lines
        .into_iter()
        .find(|l| l.kind == "Resistor")
        .unwrap();
    assert_eq!(
        (resistor.unit_usd, resistor.stock),
        (Some(0.0123), Some(42))
    );
    assert_eq!(resistor.date.as_deref(), Some("2026-10-31"));
    assert_eq!(resistor.age_days, Some(2));

    // Only LCSC's product endpoint was asked: no POST, no /api path, nothing for JLCPCB.
    assert!(
        fake.seen()
            .iter()
            .all(|s| s.method == "GET" && s.target == "/lcsc?productCode=C9999999"),
        "{:?}",
        fake.seen()
    );

    // Offline, from what the runs kept.
    let asked = fake.seen().len();
    let offline = editor.refresh_catalogs(false, true, &say).unwrap();
    assert!(offline.offline && offline.error.is_none());
    assert_eq!(fake.seen().len(), asked);
}

#[test]
fn the_check_says_why_it_cannot_and_what_a_blocked_endpoint_did() {
    let say: Say = Arc::new(|_: &str| {});
    // A project that lists no catalogs: the standard ones are built in.
    let (_, editor) = project("refresh-none");
    let r = editor.refresh_catalogs(false, false, &say).unwrap();
    let why = r.error.unwrap();
    assert!(
        why.contains("lists no `catalogs`") && why.contains("built into netsmith"),
        "{why}"
    );
    assert!(r.files.is_empty());
    assert!(editor.bom("TwoBoards/led").unwrap().catalogs.is_empty());

    // A server that refuses: nothing is written, and the page is told.
    let (root, original) = project_with_priced_catalog("refresh-blocked");
    let blocked = Fake::start(|_: &Seen| Reply::status(403, "no"));
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap()
        .with_network(blocked.network(None, NOW));
    let r = editor.refresh_catalogs(false, false, &say).unwrap();
    let why = r.error.unwrap();
    assert!(why.contains("answered 403"), "{why}");
    assert_eq!(
        fs::read_to_string(root.join("catalogs/resistor.csv")).unwrap(),
        original
    );
}

/// A rail with resistors across it in parallel: their currents are not settled, so the solver
/// leaves the relations that need them and says so.
const BANK: &str = "//! A rail with a bank of resistors across it.

import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

module Bank {
    .power: Rail
    .power.current &= 0mA..2mA
    .a: Resistor { .resistance &= 10kohm +/- 1% }
    .b: Resistor { .resistance &= 10kohm +/- 1% }
    .c: Resistor { .resistance &= 4.7kohm +/- 1% }
    .power.pos ~> .a ~> .b ~> .power.neg
    .power.pos ~> .c ~> .power.neg
}

board Notes {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
    }
    .bank: Bank
    .power_in: Header1x2
    .supply ~ .bank.power
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
}
";

#[test]
fn what_the_solver_left_alone_is_a_note_and_not_counted_as_a_warning() {
    let (root, _) = project("notes");
    fs::write(root.join("src/main.nsm"), BANK).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    let a = editor.analysis();
    assert_eq!(a.errors, 0, "{:?}", a.problems);
    let notes: Vec<_> = a
        .problems
        .iter()
        .filter(|p| p.severity == Severity::Note)
        .collect();
    assert!(notes.len() >= 5, "{:?}", a.problems);
    assert!(
        notes
            .iter()
            .all(|p| p.message.contains("not evaluated") || p.message.contains("incomplete")),
        "{notes:?}"
    );
    // The status bar's counts leave notes out.
    assert_eq!(a.notes as usize, notes.len());
    assert_eq!(a.warnings, 0, "{:?}", a.problems);
    // A note about a resistor points at the line that declares it.
    let about_a = notes
        .iter()
        .find(|p| p.message.contains("bank.a.drop"))
        .expect("a note about bank.a");
    let at = about_a.at.as_ref().expect("a note says where it is about");
    assert_eq!(at.file, "src/main.nsm");
    assert_eq!(at.line, line_of(BANK, ".a: Resistor"));
    // Every note has a place.
    assert!(notes.iter().all(|p| p.at.is_some()), "{notes:?}");
}

// ---- the PCB canvas: the scene and edits to the layout file ----

const LED: &str = "TwoBoards/led";

/// Whether the KiCad footprint libraries are here; the tests that place a board need them.
fn footprints_here() -> bool {
    let here = FootprintLibrary::installed().root().exists();
    if !here {
        eprintln!("skipped: no KiCad footprint libraries");
    }
    here
}

fn layout_file(root: &Path) -> PathBuf {
    root.join("src/main.pcb.toml")
}

fn placement(x: f64, y: f64) -> PartPlacement {
    PartPlacement {
        x,
        y,
        rotation: 0.0,
        side: PcbSide::Top,
        locked: false,
        source: None,
    }
}

/// An edit that moves one part from `before` (what the page saw) to `after`.
fn moves(
    scene: &PcbScene,
    path: &str,
    before: Option<PartPlacement>,
    after: Option<PartPlacement>,
) -> PcbEdit {
    PcbEdit {
        board: LED.into(),
        stamp: scene.layout_stamp.clone(),
        changes: vec![PcbChange::Part {
            path: path.into(),
            before,
            after,
        }],
    }
}

fn part<'a>(scene: &'a PcbScene, refdes: &str) -> &'a netsmith_editor::api::PcbPart {
    scene.parts.iter().find(|p| p.refdes == refdes).unwrap()
}

#[test]
fn the_scene_has_every_part_with_its_pads_nets_and_place() {
    if !footprints_here() {
        return;
    }
    let (root, editor) = project("scene");
    let scene = editor.pcb_scene(LED).unwrap();
    assert_eq!(scene.board, LED);
    assert_eq!(scene.layout_file, "main.pcb.toml");
    assert_eq!(scene.layout_stamp, "none", "no layout file yet");
    assert!(!layout_file(&root).exists());
    assert_eq!(scene.parts.len(), 3);
    assert!(scene.svg.contains("data-path=\"r1\""));
    let r1 = part(&scene, "R1");
    assert_eq!((r1.path.as_str(), r1.kind.as_str()), ("r1", "Resistor"));
    assert_eq!(
        (r1.side, r1.locked, r1.placement.as_str()),
        (PcbSide::Top, false, "derived")
    );
    assert_eq!(r1.pads.len(), 2);
    assert!(r1.pads.iter().all(|p| p.net.is_some()), "{:?}", r1.pads);
    // The courtyard holds the origin, and the outline holds the courtyard.
    let c = r1.courtyard;
    assert!(c.x <= r1.x && r1.x <= c.x + c.width && c.y <= r1.y && r1.y <= c.y + c.height);
    let o = scene.outline;
    assert!(o.x <= c.x && c.x + c.width <= o.x + o.width + 1e-9);
    assert_eq!((scene.origin_x, scene.origin_y), (o.x, o.y));
    assert_eq!(scene.layers, ["top", "bottom"]);
    assert!(!scene.routed);
    assert!(scene.keepouts.is_empty());
    assert_eq!(editor.pcb_scene("Nope").unwrap_err().status, 404);
}

#[test]
fn a_move_is_written_to_the_layout_file_and_parts_not_touched_stay_derived() {
    if !footprints_here() {
        return;
    }
    let (root, editor) = project("move");
    let scene = editor.pcb_scene(LED).unwrap();
    let others: Vec<_> = scene
        .parts
        .iter()
        .filter(|p| p.refdes != "R1")
        .map(|p| (p.refdes.clone(), p.x, p.y))
        .collect();
    let (ox, oy) = (scene.origin_x, scene.origin_y);

    let moved = editor
        .pcb_edit(&moves(&scene, "r1", None, Some(placement(7.25, 5.5))))
        .unwrap()
        .scene;
    let r1 = part(&moved, "R1");
    assert_eq!((r1.x, r1.y), (ox + 7.25, oy + 5.5));
    assert_eq!(r1.placement, "hand");
    assert_ne!(moved.layout_stamp, "none");
    // Nothing else moved: the file has one entry, in the file's own format.
    let text = fs::read_to_string(layout_file(&root)).unwrap();
    assert_eq!(
        text,
        "version = 1\n\n[LedBoard.parts.r1]\nat = [7.25, 5.5]\nrotation = 0.0\nside = \"top\"\nlock = false\nsource = \"hand\"\n"
    );
    for (refdes, x, y) in &others {
        let p = part(&moved, refdes);
        assert_eq!((p.x, p.y, p.placement.as_str()), (*x, *y, "derived"));
    }
    // A new page reads the same board from the file, the way `apply_layout` puts it.
    let again = editor.pcb_scene(LED).unwrap();
    assert_eq!(again.parts, moved.parts);

    // Undo is the same change with before and after swapped; the part is derived again.
    let undone = editor
        .pcb_edit(&moves(&moved, "r1", Some(placement(7.25, 5.5)), None))
        .unwrap()
        .scene;
    assert_eq!(part(&undone, "R1"), part(&scene, "R1"));
    assert_eq!(
        fs::read_to_string(layout_file(&root)).unwrap(),
        "version = 1\n"
    );
}

#[test]
fn rotate_flip_and_lock_are_kept_in_the_file() {
    if !footprints_here() {
        return;
    }
    let (root, editor) = project("rotate");
    let scene = editor.pcb_scene(LED).unwrap();
    let r1 = part(&scene, "R1");
    let (x, y) = (r1.x - scene.origin_x, r1.y - scene.origin_y);
    let after = PartPlacement {
        x,
        y,
        rotation: -90.0,
        side: PcbSide::Bottom,
        locked: true,
        source: None,
    };
    let edited = editor
        .pcb_edit(&moves(&scene, "r1", None, Some(after)))
        .unwrap()
        .scene;
    let r1 = part(&edited, "R1");
    assert_eq!(
        (r1.rotation, r1.side, r1.locked),
        (270.0, PcbSide::Bottom, true),
        "an angle is kept in 0..360"
    );
    let text = fs::read_to_string(layout_file(&root)).unwrap();
    assert!(
        text.contains("rotation = 270.0")
            && text.contains("side = \"bottom\"")
            && text.contains("lock = true"),
        "{text}"
    );
}

#[test]
fn an_edit_that_is_stale_or_wrong_is_refused_and_writes_nothing() {
    if !footprints_here() {
        return;
    }
    let (root, editor) = project("refused");
    let scene = editor.pcb_scene(LED).unwrap();
    let file = layout_file(&root);

    // A part the board does not have: 400, naming it and what the board has.
    let err = editor
        .pcb_edit(&moves(&scene, "nope.gone", None, Some(placement(1.0, 1.0))))
        .unwrap_err();
    assert_eq!(err.status, 400);
    assert!(
        err.message.contains("has no part `nope.gone`") && err.message.contains("`r1`"),
        "{}",
        err.message
    );
    // A position that is not a number.
    let err = editor
        .pcb_edit(&moves(&scene, "r1", None, Some(placement(f64::NAN, 1.0))))
        .unwrap_err();
    assert_eq!(err.status, 400);
    assert!(err.message.contains("not a number"), "{}", err.message);
    // An entry the page thinks is there and is not: 409, naming the part.
    let err = editor
        .pcb_edit(&moves(&scene, "r1", Some(placement(3.0, 3.0)), None))
        .unwrap_err();
    assert_eq!(err.status, 409);
    assert!(err.message.contains("`r1`"), "{}", err.message);
    assert!(!file.exists(), "nothing was written");

    // A file another program wrote since the page read it.
    fs::write(&file, "version = 1\n").unwrap();
    let err = editor
        .pcb_edit(&moves(&scene, "r1", None, Some(placement(1.0, 1.0))))
        .unwrap_err();
    assert_eq!(err.status, 409);
    assert!(
        err.message.contains("main.pcb.toml changed"),
        "{}",
        err.message
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), "version = 1\n");
}

#[test]
fn an_edit_keeps_the_tables_of_the_other_boards_and_a_broken_file_is_reported() {
    if !footprints_here() {
        return;
    }
    let (root, editor) = project("others");
    let file = layout_file(&root);
    let before = editor.analysis().version;
    fs::write(
        &file,
        "version = 1\n\n[PowerBoard.parts.\"stage.regulator\"]\nat = [10.0, 10.0]\nrotation = 90.0\nside = \"top\"\nlock = true\nsource = \"auto\"\n",
    )
    .unwrap();
    // The file changed under the editor: the next scene is of the new file, and the page is
    // told (the version goes up) as it is for a source.
    let scene = editor.pcb_scene(LED).unwrap();
    assert!(editor.analysis().version > before);
    editor
        .pcb_edit(&moves(&scene, "r1", None, Some(placement(4.0, 4.0))))
        .unwrap();
    let text = fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("[PowerBoard.parts.\"stage.regulator\"]")
            && text.contains("source = \"auto\"")
            && text.contains("[LedBoard.parts.r1]"),
        "{text}"
    );
    // The editor's own write is not a change by another program.
    let version = editor.analysis().version;
    assert_eq!(editor.analysis().version, version);

    // A table for a board the source lacks is the layout's mistake, named.
    fs::write(&file, "version = 1\n[Old.parts.r1]\nat = [1, 1]\n").unwrap();
    let err = editor.pcb_scene(LED).unwrap_err();
    assert!(
        err.message.contains("has a table for the board `Old`"),
        "{}",
        err.message
    );

    // An entry for a part that is gone is found, named, and not edited around: the page shows
    // the message, and an edit says the same and writes nothing.
    fs::write(
        &file,
        "version = 1\n[LedBoard.parts.\"gone.part\"]\nat = [1, 1]\n",
    )
    .unwrap();
    let err = editor.pcb_scene(LED).unwrap_err();
    assert!(
        err.message.contains("has no part `gone.part`")
            && err.message.contains("remove the entry or rename it"),
        "{}",
        err.message
    );
    // (An edit made from the board as it was before the file changed is refused as stale.)
    let err = editor
        .pcb_edit(&moves(&scene, "r1", None, Some(placement(1.0, 1.0))))
        .unwrap_err();
    assert_eq!(err.status, 409);
    assert!(
        !fs::read_to_string(&file)
            .unwrap()
            .contains("[LedBoard.parts.r1]")
    );
}

#[test]
fn an_edit_takes_the_routed_copper_away() {
    if !footprints_here() {
        return;
    }
    let (_, editor) = project("unroute");
    let (say, _) = listener();
    editor.route(LED, &say).unwrap();
    let routed = editor.pcb_scene(LED).unwrap();
    assert!(routed.routed && routed.svg.contains("<line data-net"));
    let tracks = |s: &PcbScene| {
        s.svg
            .matches("stroke-linecap=\"round\" stroke-opacity=\"0.85\"")
            .count()
    };
    assert!(tracks(&routed) > 0);
    let edited = editor
        .pcb_edit(&moves(&routed, "r1", None, Some(placement(9.0, 9.0))))
        .unwrap()
        .scene;
    assert!(!edited.routed);
    assert_eq!(tracks(&edited), 0, "the copper no longer fits the parts");
}

// ---- keepouts on the canvas ----

const POURS: &str = include_str!("../../../examples/outline_and_pours.nsm");
const POURS_LAYOUT: &str = include_str!("../../../examples/outline_and_pours.pcb.toml");
const POURS_BOARD: &str = "OutlineAndPours";

/// The four-layer example with its keepout, cutout and layout file as a project.
fn pours_project(name: &str) -> (PathBuf, Workspace) {
    let (root, _) = project(name);
    fs::write(root.join("src/main.nsm"), POURS).unwrap();
    fs::write(root.join("src/main.pcb.toml"), POURS_LAYOUT).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

fn keepout_edit(
    scene: &PcbScene,
    name: &str,
    before: Option<KeepoutPlacement>,
    after: Option<KeepoutPlacement>,
) -> PcbEdit {
    PcbEdit {
        board: POURS_BOARD.into(),
        stamp: scene.layout_stamp.clone(),
        changes: vec![PcbChange::Keepout {
            name: name.into(),
            before,
            after,
        }],
    }
}

fn drawn(x: f64, y: f64, w: f64, h: f64) -> KeepoutPlacement {
    KeepoutPlacement {
        x,
        y,
        drawn: Some(DrawnKeepout {
            width: w,
            height: h,
            layers: vec![
                "top".into(),
                "inner1".into(),
                "inner2".into(),
                "bottom".into(),
            ],
            tracks: true,
            vias: true,
            copper: false,
            parts: true,
        }),
    }
}

#[test]
fn the_scene_says_where_each_keepout_comes_from_and_the_outline_has_four_layers() {
    if !footprints_here() {
        return;
    }
    let (_, editor) = pours_project("kscene");
    let scene = editor.pcb_scene(POURS_BOARD).unwrap();
    assert_eq!(scene.layers, ["top", "inner1", "inner2", "bottom"]);
    let antenna = scene.keepouts.iter().find(|k| k.name == "antenna").unwrap();
    assert_eq!(
        antenna.origin,
        KeepoutOrigin::Placed,
        "sized by the source, placed by the file"
    );
    let area = antenna.area.unwrap();
    assert_eq!(
        (area.x - scene.origin_x, area.y - scene.origin_y),
        (18.0, 2.0)
    );
    assert_eq!((antenna.width, antenna.height), (14.0, 5.0));
    assert_eq!(antenna.layers, ["top"]);
    assert!(antenna.tracks && !antenna.vias && antenna.copper && antenna.parts);
    assert_eq!(scene.cutouts.len(), 1);
    assert!(scene.cutouts[0].area.is_some());
    assert!(
        scene.svg.contains("data-keepout=\"antenna\"")
            && scene.svg.contains("data-cutout=\"slot\"")
    );
}

#[test]
fn a_keepout_is_drawn_moved_resized_and_deleted_in_the_layout_file() {
    if !footprints_here() {
        return;
    }
    let (root, editor) = pours_project("kdraw");
    let file = layout_file(&root);
    let scene = editor.pcb_scene(POURS_BOARD).unwrap();

    // Draw one: the file gets a keepout with a size, and the antenna's entry stays.
    let d = drawn(2.0, 2.0, 10.0, 6.0);
    let scene = editor
        .pcb_edit(&keepout_edit(&scene, "keepout1", None, Some(d.clone())))
        .unwrap()
        .scene;
    let k = scene
        .keepouts
        .iter()
        .find(|k| k.name == "keepout1")
        .unwrap();
    assert_eq!(k.origin, KeepoutOrigin::Drawn);
    let area = k.area.unwrap();
    assert_eq!(
        (
            area.x - scene.origin_x,
            area.y - scene.origin_y,
            area.width,
            area.height
        ),
        (2.0, 2.0, 10.0, 6.0)
    );
    assert!(!k.copper && k.parts && k.layers.len() == 4);
    let text = fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("[OutlineAndPours.keepouts.antenna]\nat = [18.0, 2.0]")
            && text.contains("[OutlineAndPours.keepouts.keepout1]\nat = [2.0, 2.0]\nsize = [10.0, 6.0]\ncopper = \"allow\""),
        "{text}"
    );
    // The file is what `apply_layout` reads: a new reading shows the same board.
    assert_eq!(
        editor.pcb_scene(POURS_BOARD).unwrap().keepouts,
        scene.keepouts
    );

    // Move and resize it (one entry changes from the old to the new).
    let bigger = drawn(4.0, 3.0, 12.0, 8.0);
    let scene = editor
        .pcb_edit(&keepout_edit(
            &scene,
            "keepout1",
            Some(d.clone()),
            Some(bigger.clone()),
        ))
        .unwrap()
        .scene;
    let k = scene
        .keepouts
        .iter()
        .find(|k| k.name == "keepout1")
        .unwrap();
    assert_eq!((k.width, k.height), (12.0, 8.0));
    // A keepout the source sizes is only moved.
    let scene = editor
        .pcb_edit(&keepout_edit(
            &scene,
            "antenna",
            Some(KeepoutPlacement {
                x: 18.0,
                y: 2.0,
                drawn: None,
            }),
            Some(KeepoutPlacement {
                x: 22.0,
                y: 4.5,
                drawn: None,
            }),
        ))
        .unwrap()
        .scene;
    let antenna = scene.keepouts.iter().find(|k| k.name == "antenna").unwrap();
    assert_eq!(antenna.area.unwrap().x - scene.origin_x, 22.0);
    assert!(
        fs::read_to_string(&file)
            .unwrap()
            .contains("at = [22.0, 4.5]\n")
    );

    // Delete the drawn one: the entry is gone and the antenna's stays.
    let scene = editor
        .pcb_edit(&keepout_edit(&scene, "keepout1", Some(bigger), None))
        .unwrap()
        .scene;
    assert!(scene.keepouts.iter().all(|k| k.name != "keepout1"));
    let text = fs::read_to_string(&file).unwrap();
    assert!(
        !text.contains("keepout1") && text.contains("[OutlineAndPours.keepouts.antenna]"),
        "{text}"
    );
}

#[test]
fn a_keepout_that_is_wrong_is_refused_naming_the_field_and_what_the_board_has() {
    if !footprints_here() {
        return;
    }
    let (root, editor) = pours_project("kwrong");
    let scene = editor.pcb_scene(POURS_BOARD).unwrap();
    let before = fs::read_to_string(layout_file(&root)).unwrap();
    let refuse = |name: &str, after: KeepoutPlacement| {
        // The antenna has the entry that the example's layout file gives it.
        let before = (name == "antenna").then_some(KeepoutPlacement {
            x: 18.0,
            y: 2.0,
            drawn: None,
        });
        editor
            .pcb_edit(&keepout_edit(&scene, name, before, Some(after)))
            .unwrap_err()
    };
    let e = refuse("antenna", drawn(1.0, 1.0, 3.0, 3.0));
    assert_eq!(e.status, 400);
    assert!(
        e.message.contains(
            "`antenna` is drawn in the layout file, and the source has a keepout of that name"
        ),
        "{}",
        e.message
    );
    let e = refuse("k", drawn(1.0, 1.0, 0.0, 3.0));
    assert!(
        e.message.contains("the keepout `k` has the size 0 by 3 mm"),
        "{}",
        e.message
    );
    let mut wrong = drawn(1.0, 1.0, 3.0, 3.0);
    wrong.drawn.as_mut().unwrap().layers = vec!["inner3".into()];
    let e = refuse("k", wrong);
    assert_eq!(e.status, 400);
    assert!(
        e.message.contains("\"inner3\"")
            && e.message
                .contains("\"top\", \"inner1\", \"inner2\", \"bottom\""),
        "{}",
        e.message
    );
    let mut nothing = drawn(1.0, 1.0, 3.0, 3.0);
    let d = nothing.drawn.as_mut().unwrap();
    (d.tracks, d.vias, d.copper, d.parts) = (false, false, false, false);
    let e = refuse("k", nothing);
    assert!(e.message.contains("forbids nothing"), "{}", e.message);
    // An entry the page thinks is there and is not.
    let e = editor
        .pcb_edit(&keepout_edit(
            &scene,
            "ghost",
            Some(drawn(1.0, 1.0, 3.0, 3.0)),
            None,
        ))
        .unwrap_err();
    assert_eq!(e.status, 409);
    assert!(e.message.contains("`ghost`"), "{}", e.message);
    assert_eq!(
        fs::read_to_string(layout_file(&root)).unwrap(),
        before,
        "nothing was written"
    );
}

#[test]
fn a_drawn_keepout_that_forbids_parts_is_a_problem_for_a_part_in_it() {
    if !footprints_here() {
        return;
    }
    let (_, editor) = pours_project("kparts");
    let scene = editor.pcb_scene(POURS_BOARD).unwrap();
    let scene = editor
        .pcb_edit(&keepout_edit(
            &scene,
            "box",
            None,
            Some(drawn(2.0, 20.0, 12.0, 8.0)),
        ))
        .unwrap()
        .scene;
    let c2 = part(&scene, "C2");
    let moved = editor
        .pcb_edit(&PcbEdit {
            board: POURS_BOARD.into(),
            stamp: scene.layout_stamp.clone(),
            changes: vec![PcbChange::Part {
                path: c2.path.clone(),
                before: None,
                after: Some(placement(8.0, 24.0)),
            }],
        })
        .unwrap()
        .scene;
    assert_eq!(part(&moved, "C2").placement, "hand");
    let (say, _) = listener();
    let report = editor.route(POURS_BOARD, &say).unwrap();
    assert!(
        report
            .placement_problems
            .iter()
            .any(|p| p.contains("C2 overlaps the keepout `box`, which forbids parts")),
        "{:?}",
        report.placement_problems
    );
}

// ---- findings ----

/// Move `refdes` of the LED board onto `on`'s place, so that their courtyards overlap.
fn overlapped(editor: &Workspace) -> PcbScene {
    let scene = editor.pcb_scene(LED).unwrap();
    let on = part(&scene, "D1");
    let (x, y) = (on.x - scene.origin_x, on.y - scene.origin_y);
    editor
        .pcb_edit(&moves(&scene, "r1", None, Some(placement(x + 0.5, y))))
        .unwrap()
        .scene
}

#[test]
fn a_placement_problem_is_a_finding_with_the_box_it_is_about() {
    if !footprints_here() {
        return;
    }
    let (_, editor) = project("findings");
    let clean = editor.pcb_findings(LED).unwrap();
    assert_eq!(clean.findings, [], "the placer's own placement is clean");
    assert!(!clean.routed);
    assert_eq!(clean.kicad, KicadState::NotRun);

    let scene = overlapped(&editor);
    let found = editor.pcb_findings(LED).unwrap();
    let overlap = found
        .findings
        .iter()
        .find(|f| f.rule == "overlap")
        .unwrap_or_else(|| panic!("{found:?}"));
    assert_eq!(overlap.source, FindingSource::Placement);
    assert_eq!(overlap.severity, netsmith_editor::api::Severity::Error);
    assert!(
        overlap.message.contains("courtyards of D1 and R1 overlap")
            || overlap.message.contains("courtyards of R1 and D1 overlap"),
        "{}",
        overlap.message
    );
    let mut parts = overlap.parts.clone();
    parts.sort();
    assert_eq!(parts, ["D1", "R1"]);
    // The point is where the two meet: inside both courtyards.
    for refdes in ["D1", "R1"] {
        let c = part(&scene, refdes).courtyard;
        assert!(
            c.x <= overlap.x
                && overlap.x <= c.x + c.width
                && c.y <= overlap.y
                && overlap.y <= c.y + c.height,
            "({}, {}) is in the courtyard of {refdes}",
            overlap.x,
            overlap.y
        );
    }
    assert!(
        overlap.area.width >= 3.0 && overlap.area.height >= 3.0,
        "a box to zoom to"
    );
    // The same finding has the same id the next time.
    assert_eq!(
        editor.pcb_findings(LED).unwrap().findings[0].id,
        found.findings[0].id
    );

    // A part off the board is a finding of its own, at the part.
    let off = editor
        .pcb_edit(&moves(
            &scene,
            "power_in",
            None,
            Some(placement(300.0, 5.0)),
        ))
        .unwrap()
        .scene;
    let found = editor.pcb_findings(LED).unwrap();
    let outside = found
        .findings
        .iter()
        .find(|f| f.rule == "outside_outline")
        .unwrap();
    assert_eq!(outside.parts, ["J1"]);
    let c = part(&off, "J1").courtyard;
    assert!((outside.x - (c.x + c.width / 2.0)).abs() < 1.0);

    // Put them back: nothing left.
    editor
        .pcb_edit(&moves(&off, "power_in", Some(placement(300.0, 5.0)), None))
        .unwrap();
    let back = editor.pcb_scene(LED).unwrap();
    editor
        .pcb_edit(&moves(
            &back,
            "r1",
            Some(placement(
                back.parts
                    .iter()
                    .find(|p| p.refdes == "D1")
                    .map(|d| d.x - back.origin_x + 0.5)
                    .unwrap(),
                part(&back, "D1").y - back.origin_y,
            )),
            None,
        ))
        .unwrap();
    assert_eq!(editor.pcb_findings(LED).unwrap().findings, []);
}

#[test]
fn what_the_router_left_is_a_finding_between_its_pads_once_the_board_is_routed() {
    if !footprints_here() {
        return;
    }
    let (_, editor) = project("unrouted");
    let (say, _) = listener();
    editor.route(LED, &say).unwrap();
    let found = editor.pcb_findings(LED).unwrap();
    assert!(found.routed);
    assert_eq!(
        found
            .findings
            .iter()
            .filter(|f| f.source == FindingSource::Unrouted)
            .count(),
        0,
        "a routed board has nothing left"
    );
}

#[test]
fn kicads_check_is_run_on_demand_and_its_findings_have_places() {
    if !footprints_here() {
        return;
    }
    let (_, editor) = project("kicad");
    let scene = overlapped(&editor);
    let (say, heard) = listener();
    let found = editor.pcb_drc(LED, &say).unwrap();
    if found.kicad == KicadState::Unavailable {
        eprintln!("skipped: {}", found.kicad_note);
        assert!(
            found.kicad_note.contains("kicad-cli"),
            "{}",
            found.kicad_note
        );
        return;
    }
    assert_eq!(found.kicad, KicadState::Ran, "{}", found.kicad_note);
    assert!(
        heard
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("KiCad's design rules check"))
    );
    let kicad: Vec<_> = found
        .findings
        .iter()
        .filter(|f| f.source == FindingSource::Kicad)
        .collect();
    eprintln!("{kicad:#?}");
    assert!(!kicad.is_empty(), "KiCad sees the overlap: {found:?}");
    let d1 = part(&scene, "D1").courtyard;
    assert!(
        kicad.iter().any(|f| f.parts.contains(&"D1".to_string())
            && f.parts.contains(&"R1".to_string())
            && (f.x - (d1.x + d1.width / 2.0)).abs() < 5.0),
        "a KiCad finding names both parts, near them: {kicad:?}"
    );
    // The result is kept for the next reading, and an edit forgets it.
    assert_eq!(editor.pcb_findings(LED).unwrap().kicad, KicadState::Ran);
    let moved = editor
        .pcb_edit(&moves(&scene, "led", None, Some(placement(1.0, 1.0))))
        .unwrap()
        .scene;
    assert_eq!(moved.parts.len(), 3);
    let after = editor.pcb_findings(LED).unwrap();
    assert_eq!(after.kicad, KicadState::NotRun);
    assert!(
        after
            .findings
            .iter()
            .all(|f| f.source != FindingSource::Kicad)
    );
}

const PANEL: &str = include_str!("../../../examples/led_panel.nsm");

#[test]
fn a_panel_is_listed_and_drawn_from_its_boards_placed_size() {
    let root =
        std::env::temp_dir().join(format!("netsmith-editor-test-{}-panel", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/panel.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/panel.nsm"), PANEL).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    assert_eq!(analysis.panels.len(), 1);
    let p = &analysis.panels[0];
    assert_eq!(
        (p.name.as_str(), p.board.as_str(), p.columns, p.rows),
        ("LedPanel", "LedTile", 3, 2)
    );
    // The tree lists the definition with its kind.
    let tree = editor.project();
    let kinds: Vec<_> = tree.files[0]
        .defs
        .iter()
        .map(|d| (d.kind.as_str(), d.name.as_str()))
        .collect();
    assert!(kinds.contains(&("panel", "LedPanel")), "{kinds:?}");

    let view = editor.panel("LedPanel").unwrap();
    assert_eq!((view.width, view.height), (130.0, 70.0));
    assert_eq!((view.cell_width, view.cell_height), (40.0, 30.0));
    // Only advice: copper is as near the edge as JLCPCB allows and less than it recommends.
    assert!(
        view.findings.iter().all(|f| !f.error),
        "{:?}",
        view.findings
    );
    assert!(view.svg.contains("LedTile 6") && view.svg.contains("stroke-dasharray"));
    assert_eq!(editor.panel("Nope").unwrap_err().status, 404);

    // A panel the board house would refuse says so in the preview too.
    fs::write(
        root.join("src/panel.nsm"),
        PANEL.replace(".rows &= 2", ".rows &= 1"),
    )
    .unwrap();
    let view = editor.panel("LedPanel").unwrap();
    assert!(
        view.findings.iter().any(|f| f.error
            && f.message
                .contains("the panel is 40 mm high and a V-cut panel is at least 70 mm each way")),
        "{:?}",
        view.findings
    );
    let _ = fs::remove_dir_all(&root);
}
