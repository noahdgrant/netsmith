//! Orders (ADR 0035): `[order.*]` in `netsmith.toml` and `netsmith fab --order`. The mistakes
//! and the listing and the orders themselves need no KiCad (ADR 0037).

use netsmith_kicad::testlib::TempDir;
use std::fs;
use std::path::Path;
use std::process::Command;

use netsmith_fab::KicadCliExe;

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

fn netsmith(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .output()
        .unwrap()
}

#[test]
fn an_order_without_kicad_is_made_and_its_boards_pass_the_check() {
    if netsmith_kicad::testlib::skip_without_kicad_libraries() {
        return;
    }
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
fn the_program_refuses_to_order_a_board_of_a_fictional_fabricator() {
    let root = project("fictional", "\n[order.big.boards]\nTenLayerExample = 1\n");
    fs::write(
        root.join("src/ten.nsm"),
        include_str!("../../../examples/ten_layer.nsm"),
    )
    .unwrap();
    let manifest = fs::read_to_string(root.join("netsmith.toml"))
        .unwrap()
        .replace("\"src/two.nsm\"]", "\"src/two.nsm\", \"src/ten.nsm\"]");
    fs::write(root.join("netsmith.toml"), manifest).unwrap();
    let refused = netsmith(&root, &["fab", "--order", "big"]);
    assert!(!refused.status.success());
    let said = String::from_utf8_lossy(&refused.stderr);
    assert!(
        said.contains(
            "TenLayerExample is made by ACME's `TenLayer`, and ACME is a fictional fabricator"
        ) && said.contains("cannot be ordered"),
        "{said}"
    );
    assert!(!root.join("fab/orders/big").exists());
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

#[test]
fn the_program_makes_an_order_into_the_out_directory() {
    let Some(cli) = KicadCliExe::locate_runnable() else {
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
