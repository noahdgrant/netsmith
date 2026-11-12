//! Panels (ADR 0040): a `panel` repeats one board in a grid with V-cuts. The package, its
//! checks and the order that names a panel need no KiCad; the cross-check with KiCad's plot of
//! the panel board skips when KiCad is not installed.

use netsmith_kicad::testlib::TempDir;
use std::fs;
use std::path::Path;
use std::process::Command;

const PANEL: &str = include_str!("../../../examples/led_panel.nsm");
/// The physical file of the board the panel repeats: its outline.
const PANEL_PHYSICAL: &str = include_str!("../../../examples/led_panel.physical.nsm");

fn out(name: &str) -> TempDir {
    TempDir::unused(&format!("panel-test-{name}"))
}

// ---- orders ------------------------------------------------------------------------------

fn project(name: &str, orders: &str) -> TempDir {
    let root = out(name);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/panel.nsm"), PANEL).unwrap();
    fs::write(root.join("src/panel.physical.nsm"), PANEL_PHYSICAL).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"kit\"\ntop_level = [\"src/panel.nsm\"]\n{orders}"),
    )
    .unwrap();
    root
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
fn an_order_may_name_a_panel_and_its_quantity_counts_panels() {
    if netsmith_kicad::testlib::skip_without_kicad_libraries() {
        return;
    }
    let root = project("order", "\n[order.run.boards]\nLedPanel = 3\n");
    let made = netsmith(&root, &["fab", "--order", "run"]);
    let said = String::from_utf8_lossy(&made.stderr);
    assert!(made.status.success(), "{said}");
    let dir = root.join("fab/orders/run");
    let summary = fs::read_to_string(dir.join("order.txt")).unwrap();
    for want in [
        "order run: 1 design, 0 boards and 3 panels in all (the panels hold 18 boards)",
        "LedPanel x3: 2 layers, 130 x 70 mm, 1.6 mm thick",
        "a panel of 3 x 2 LedTile (40 x 30 mm each), V-cut, 5 mm rails",
        "hand-solder: J1_1, J1_2, J1_3, J1_4, J1_5, J1_6 (6 parts on each panel, 18 in all)",
    ] {
        assert!(summary.contains(want), "missing {want:?} in\n{summary}");
    }
    // Three panels of six boards: 18 of each part.
    let bom = fs::read_to_string(dir.join("order-bom.csv")).unwrap();
    assert!(
        bom.contains("C2286,18,3 x 6 LedPanel") && bom.contains("C25104,18,3 x 6 LedPanel"),
        "{bom}"
    );
    assert!(dir.join("LedPanel/LedPanel-gerbers.zip").is_file());
    assert!(dir.join("LedPanel/cell/LedTile/bom.csv").is_file());
}

#[test]
fn a_board_and_a_panel_may_both_be_ordered_and_a_plain_fab_makes_both() {
    if netsmith_kicad::testlib::skip_without_kicad_libraries() {
        return;
    }
    let root = project(
        "both",
        "\n[order.mixed.boards]\nLedPanel = 2\nLedTile = 4\n",
    );
    let made = netsmith(&root, &["fab", "--order", "mixed"]);
    let said = String::from_utf8_lossy(&made.stderr);
    assert!(made.status.success(), "{said}");
    let summary = fs::read_to_string(root.join("fab/orders/mixed/order.txt")).unwrap();
    assert!(
        summary.contains(
            "order mixed: 2 designs, 4 boards and 2 panels in all (the panels hold 12 boards)"
        ),
        "{summary}"
    );
    let bom = fs::read_to_string(root.join("fab/orders/mixed/order-bom.csv")).unwrap();
    // 4 loose boards and 2 panels of 6: 16 of each part.
    assert!(
        bom.contains("C25104,16,4 x 1 LedTile; 2 x 6 LedPanel"),
        "{bom}"
    );
    let plain = netsmith(&root, &["fab"]);
    assert!(
        plain.status.success(),
        "{}",
        String::from_utf8_lossy(&plain.stderr)
    );
    assert!(root.join("fab/LedPanel/panel.svg").is_file());
    assert!(root.join("fab/LedTile/LedTile-gerbers.zip").is_file());
}

#[test]
fn an_order_that_names_neither_a_board_nor_a_panel_lists_both() {
    let root = project("unknown", "\n[order.bad.boards]\nLedPanl = 1\n");
    let made = netsmith(&root, &["fab", "--order", "bad"]);
    let said = String::from_utf8_lossy(&made.stderr);
    assert!(!made.status.success());
    assert!(
        said.contains(
            "order `bad` names `LedPanl`, but the project has no such board, panel or system"
        ) && said.contains("the boards and panels are: LedTile, LedPanel"),
        "{said}"
    );
}
