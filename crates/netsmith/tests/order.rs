//! Orders (ADR 0035): `[order.*]` in `netsmith.toml` and `netsmith fab --order`. The mistakes
//! and the listing and the orders themselves need no KiCad (ADR 0037).

use netsmith_kicad::testlib::TempDir;
use std::fs;
use std::path::Path;
use std::process::Command;

use netsmith::Libraries;
use netsmith::fab::FabOptions;
use netsmith::order::{BOM_FILE, ORDERS_DIR, SUMMARY_FILE, fab_order};
use netsmith::project::Target;
use netsmith_fab::{KicadCli, KicadCliExe};

const LED: &str = include_str!("../../../examples/led.nsm");
const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");

/// A project with a board `LedExample` and a system `TwoBoards` of two boards, and the
/// `[order.*]` tables `orders`.
fn project(name: &str, orders: &str) -> TempDir {
    let root = TempDir::new(&format!("order-test-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/led.nsm"), LED).unwrap();
    fs::write(root.join("src/two.nsm"), TWO_BOARDS).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        format!(
            "[project]\nname = \"kit\"\ntop_level = [\"src/led.nsm\", \"src/two.nsm\"]\n{orders}"
        ),
    )
    .unwrap();
    root
}

const ORDERS: &str = "
[order.small.boards]
LedExample = 10
\"TwoBoards/led\" = 3

[order.system.boards]
TwoBoards = 2
";

fn make(
    root: &Path,
    order: &str,
    kicad: Option<&dyn KicadCli>,
) -> Result<netsmith::order::Made, String> {
    let target = Target::resolve_from(root, None)?;
    let loader = target.loader()?;
    let catalogs = target.catalogs()?;
    let libraries = Libraries::for_project(Some(root))?;
    let o = target
        .project
        .as_ref()
        .unwrap()
        .manifest
        .orders
        .iter()
        .find(|o| o.name == order)
        .unwrap_or_else(|| panic!("no order {order}"));
    fab_order(
        o,
        &target,
        &loader,
        &catalogs,
        &libraries,
        kicad,
        &FabOptions::default(),
        &target.fab_out,
    )
}

fn netsmith(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .output()
        .unwrap()
}

#[test]
fn an_unknown_board_stops_the_order_and_lists_the_boards_that_exist() {
    let root = project(
        "unknown",
        "\n[order.bad.boards]\nLedExample = 1\n\"TwoBoards/powr\" = 1\nNope = 2\n",
    );
    let err = make(&root, "bad", None).expect_err("two names do not exist");
    // Reported before KiCad is asked for, so this runs on a machine without it.
    assert!(
        err.contains("netsmith.toml:7: order `bad` names `TwoBoards/powr`")
            && err.contains("netsmith.toml:8: order `bad` names `Nope`")
            && err.contains(
                "the boards and panels are: LedExample, TwoBoards/power, TwoBoards/led\n"
            ),
        "{err}"
    );
    assert!(!root.join("fab").exists(), "nothing is written");
}

#[test]
fn an_order_without_kicad_is_made_and_its_boards_pass_the_check() {
    let root = project("nokicad", ORDERS);
    // KICAD_CLI points nowhere, so there is no KiCad: the files and the design rules check
    // are netsmith's own, and nothing has to be allowed.
    let made = netsmith(&root, &["fab", "--order", "small"]);
    let said = String::from_utf8_lossy(&made.stderr);
    assert!(made.status.success(), "{said}");
    let dir = root.join("fab/orders/small");
    assert!(dir.join("order.txt").is_file());
    let notes = fs::read_to_string(dir.join("LedExample/fab-notes.txt")).unwrap();
    assert!(notes.contains("design rules check: passed"), "{notes}");
    assert!(dir.join("LedExample/LedExample-gerbers.zip").is_file());
}

#[test]
fn the_program_lists_the_orders_and_refuses_an_unknown_one() {
    let root = project("cli", ORDERS);
    let listed = netsmith(&root, &["fab", "--order"]);
    assert!(listed.status.success());
    assert_eq!(
        String::from_utf8_lossy(&listed.stdout),
        "orders in netsmith.toml:\n  small: LedExample x10, TwoBoards/led x3\n  system: TwoBoards x2\n"
    );

    let unknown = netsmith(&root, &["fab", "--order", "devkit"]);
    assert!(!unknown.status.success());
    let said = String::from_utf8_lossy(&unknown.stderr);
    assert!(
        said.contains("no order `devkit`") && said.contains("the orders are: small, system"),
        "{said}"
    );

    // A board that does not exist is found before KiCad is looked for.
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"kit\"\ntop_level = [\"src/led.nsm\"]\n[order.d.boards]\nOther = 1\n",
    )
    .unwrap();
    let missing = netsmith(&root, &["fab", "--order", "d"]);
    assert!(!missing.status.success());
    let said = String::from_utf8_lossy(&missing.stderr);
    assert!(
        said.contains("netsmith.toml:5: order `d` names `Other`")
            && said.contains("the boards and panels are: LedExample"),
        "{said}"
    );

    // A manifest mistake stops every command, with its line.
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"kit\"\n[order.d.boards]\nLedExample = 0\n",
    )
    .unwrap();
    let broken = netsmith(&root, &["fab", "--order"]);
    assert!(!broken.status.success());
    let said = String::from_utf8_lossy(&broken.stderr);
    assert!(
        said.contains("netsmith.toml:4: order `d`") && said.contains("is 0"),
        "{said}"
    );

    let none = project("cli-none", "");
    let listed = netsmith(&none, &["fab", "--order"]);
    assert!(String::from_utf8_lossy(&listed.stdout).contains("has no orders"));
}

/// How many of the parts with LCSC number `lcsc` one board's `bom.csv` lists.
fn per_board(bom_csv: &str, lcsc: &str) -> usize {
    bom_csv
        .lines()
        .filter(|l| l.contains(&format!(",{lcsc}")))
        .map(|l| {
            // `Comment,Designator,Footprint,LCSC Part #`; designators are one quoted field.
            let designators = l.split('"').nth(1).unwrap_or("");
            designators.split(',').count()
        })
        .sum()
}

#[test]
fn an_order_makes_only_its_boards_and_a_summary_with_the_numbers_scaled() {
    let Some(cli) = KicadCliExe::locate() else {
        return;
    };
    let root = project("make", ORDERS);
    let made = make(&root, "small", Some(&cli)).unwrap_or_else(|e| panic!("{e}"));
    let fab = root.join("fab").join(ORDERS_DIR).join("small");
    assert_eq!(made.dir, fab);

    // Only the boards named, each in a folder of its own, nothing at the top of `fab`.
    assert!(fab.join("LedExample/bom.csv").is_file());
    assert!(fab.join("TwoBoards/led/bom.csv").is_file());
    assert!(!fab.join("TwoBoards/power").exists(), "not in the order");
    assert!(!root.join("fab/LedExample").exists());
    let labels: Vec<(&str, u32)> = made
        .boards
        .iter()
        .map(|(s, _)| (s.label.as_str(), s.quantity))
        .collect();
    assert_eq!(labels, [("LedExample", 10), ("TwoBoards/led", 3)]);

    // The combined bill of materials is copies x parts on one board, summed over boards.
    let led = fs::read_to_string(fab.join("LedExample/bom.csv")).unwrap();
    let ledboard = fs::read_to_string(fab.join("TwoBoards/led/bom.csv")).unwrap();
    for line in &made.bom {
        let expected = 10 * per_board(&led, &line.lcsc) + 3 * per_board(&ledboard, &line.lcsc);
        assert_eq!(line.quantity as usize, expected, "{line:?}");
    }
    let d1 = made
        .bom
        .iter()
        .find(|l| l.lcsc == "C2286")
        .expect("the LED");
    assert_eq!(d1.quantity, 10 + 3, "{d1:?}");

    let summary = fs::read_to_string(fab.join(SUMMARY_FILE)).unwrap();
    assert_eq!(summary, made.summary);
    for expected in [
        "order small: 2 board designs, 13 boards in all",
        "LedExample x10: 2 layers,",
        "TwoBoards/led x3: 2 layers,",
        "hand-solder: J1",
        "mm thick, finish not stated",
        "before you order",
        "unverified items:",
    ] {
        assert!(
            summary.contains(expected),
            "missing {expected:?} in\n{summary}"
        );
    }
    let csv = fs::read_to_string(fab.join(BOM_FILE)).unwrap();
    assert!(
        csv.starts_with("Comment,Footprint,LCSC Part #,Quantity,Used on\n"),
        "{csv}"
    );
    assert!(
        csv.contains(",C2286,13,10 x 1 LedExample; 3 x 1 TwoBoards/led"),
        "{csv}"
    );

    // The unverified total is the sum over the boards of the parts the notes list.
    let unverified: usize = made
        .boards
        .iter()
        .map(|(_, f)| f.facts.unverified.len())
        .sum();
    assert!(
        summary.contains(&format!("unverified items: {unverified}")),
        "{summary}"
    );
}

#[test]
fn a_system_in_an_order_is_all_its_members_and_a_new_run_replaces_the_old_folder() {
    let Some(cli) = KicadCliExe::locate() else {
        return;
    };
    let root = project("system", ORDERS);
    let made = make(&root, "system", Some(&cli)).unwrap_or_else(|e| panic!("{e}"));
    let labels: Vec<(&str, u32)> = made
        .boards
        .iter()
        .map(|(s, _)| (s.label.as_str(), s.quantity))
        .collect();
    assert_eq!(labels, [("TwoBoards/power", 2), ("TwoBoards/led", 2)]);
    let fab = root.join("fab/orders/system");
    assert!(fab.join("TwoBoards/power/bom.csv").is_file());
    assert!(fab.join("TwoBoards/led/bom.csv").is_file());

    // The order now asks for one member only: the other's folder is gone.
    let manifest = fs::read_to_string(root.join("netsmith.toml")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        manifest.replace("TwoBoards = 2", "\"TwoBoards/led\" = 5"),
    )
    .unwrap();
    make(&root, "system", Some(&cli)).unwrap_or_else(|e| panic!("{e}"));
    assert!(!fab.join("TwoBoards/power").exists());
    assert!(fab.join("TwoBoards/led/bom.csv").is_file());
}

#[test]
fn the_program_makes_an_order_into_the_out_directory() {
    let Some(cli) = KicadCliExe::locate() else {
        return;
    };
    let _ = cli;
    let root = project("program", ORDERS);
    let out = root.join("elsewhere");
    let run = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["fab", "--order", "small", "--out", out.to_str().unwrap()])
        .current_dir(&root)
        .output()
        .unwrap();
    let (stdout, stderr) = (
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr),
    );
    assert!(run.status.success(), "{stderr}");
    assert!(out.join("orders/small/order.txt").is_file());
    assert!(!root.join("fab").exists());
    assert!(
        stdout.contains("order small: 2 board designs, 13 boards in all")
            && stdout.contains("wrote "),
        "{stdout}"
    );
}
