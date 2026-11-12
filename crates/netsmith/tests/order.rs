//! Orders (ADR 0035): `[order.*]` in `netsmith.toml` and `netsmith fab --order`. The mistakes
//! and the listing and the orders themselves need no KiCad (ADR 0037).

use netsmith_kicad::testlib::TempDir;
use std::fs;
use std::path::Path;

use netsmith::Libraries;
use netsmith::fab::FabOptions;
use netsmith::order::{BOM_FILE, ORDERS_DIR, OrderLocks, SUMMARY_FILE, fab_order};
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
    make_with(root, order, kicad, OrderLocks::Ignored)
}

fn make_with(
    root: &Path,
    order: &str,
    kicad: Option<&dyn KicadCli>,
    locks: OrderLocks,
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
        locks,
        &target.fab_out,
    )
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
    let Some(cli) = KicadCliExe::locate_runnable() else {
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
    let Some(cli) = KicadCliExe::locate_runnable() else {
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
