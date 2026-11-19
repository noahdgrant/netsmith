//! `netsmith board explain <path> [file]` prints, as text, what the solve left at a part or member
//! and why each part has its row (issue 1008).

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

/// The converter of issue 744: a buck with a 5 mA to 50 mA load.
const BUCK: &str = r#"import std::parts::regulator::{Buck, Lmr16006y}
import std::sources::Rail

board Repro {
    .input: Rail {
        .voltage &= 9V..16V
        .peak &= 24V
        .pos.net &= "VIN"
    }
    .reg: Buck<Lmr16006y> {
        .vout.pos.net &= "V3V3"
        .vout.voltage &= 3.3V +/- 5%
    }
    .input ~ .reg.vin
    .load: Rail { .current &= 5mA..50mA }
    .reg.vout ~ .load
}
"#;

fn explain(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["board", "explain"])
        .args(args)
        .current_dir(cwd)
        .env("NETSMITH_NO_KICAD", "1")
        .output()
        .unwrap()
}

#[test]
fn a_part_says_its_row_and_the_rows_passed_over_and_its_members_their_ranges() {
    let dir = TempDir::new("explain-part");
    fs::write(dir.join("buck.nsm"), BUCK).unwrap();
    let run = explain(&dir, &["reg.inductor", "buck.nsm", "--all-rows"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let said = String::from_utf8_lossy(&run.stdout);
    assert!(said.contains("board Repro\n"), "{said}");
    assert!(
        said.contains("  reg.inductor: ") && said.contains(" from the `inductor` catalog"),
        "{said}"
    );
    // A row before it was refused by the converter's least inductance, with where that is.
    assert!(
        said.contains("refused (at std/parts/regulator.nsm:") && said.contains("min_inductance"),
        "{said}"
    );
    // Its members, with the range of current it carries.
    let current = said
        .lines()
        .find(|l| l.trim_start().starts_with("reg.inductor.current "))
        .unwrap_or_else(|| panic!("no current in {said}"));
    assert!(current.contains("..50"), "{current}");
    // Nothing of another part is there.
    assert!(!said.contains("  reg.ic.shdn_pullup:"), "{said}");
}

#[test]
fn a_module_says_every_part_inside_it_and_the_rows_listed_are_capped() {
    let dir = TempDir::new("explain-module");
    fs::write(dir.join("buck.nsm"), BUCK).unwrap();
    let run = explain(&dir, &["reg", "buck.nsm"]);
    assert!(run.status.success());
    let said = String::from_utf8_lossy(&run.stdout);
    assert!(said.contains("  reg.inductor: ") && said.contains("  reg.ic.shdn_pullup: "), "{said}");
    // `reg` is the module, not every name that begins with the letters.
    let none = explain(&dir, &["re", "buck.nsm"]);
    assert_eq!(none.status.code(), Some(1));
    let said = String::from_utf8_lossy(&none.stderr);
    assert!(
        said.contains("no member and no part with a pick at or inside `re`")
            && said.contains("`reg.inductor`"),
        "{said}"
    );
    let board = explain(&dir, &["reg", "buck.nsm", "--board", "Other"]);
    assert_eq!(board.status.code(), Some(1));
}

#[test]
fn a_failed_group_is_explained_after_its_error() {
    let dir = TempDir::new("explain-failed");
    fs::write(
        dir.join("failing.nsm"),
        "import std::parts::passive::Resistor

board Failing {
    .r: Resistor
    .r.resistance >= 1000Gohm
}
",
    )
    .unwrap();
    let run = explain(&dir, &["r", "failing.nsm"]);
    assert_eq!(run.status.code(), Some(1));
    let error = String::from_utf8_lossy(&run.stderr);
    assert!(error.contains("no `resistor` candidate satisfies `r`"), "{error}");
    let said = String::from_utf8_lossy(&run.stdout);
    assert!(said.contains("board Failing (failed)\n"), "{said}");
    assert!(
        said.contains("  r: no row of the `resistor` catalog could be chosen"),
        "{said}"
    );
    // The rows are capped at 20, and the rest are counted.
    assert!(said.contains("... and ") && said.contains("--all-rows"), "{said}");
    assert_eq!(said.matches(": refused (at failing.nsm:5): ").count(), 20, "{said}");
}

fn json(run: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&run.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not one JSON document ({e}): {}",
            String::from_utf8_lossy(&run.stdout)
        )
    })
}

#[test]
fn json_has_the_picks_and_members_the_text_shows_and_caps_the_rows_as_it_does() {
    let dir = TempDir::new("explain-json");
    fs::write(dir.join("buck.nsm"), BUCK).unwrap();
    let all = explain(&dir, &["reg.inductor", "buck.nsm", "--all-rows", "--format", "json"]);
    assert!(all.status.success(), "{}", String::from_utf8_lossy(&all.stderr));
    let doc = json(&all);
    assert_eq!(doc["command"], "explain");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["path"], "reg.inductor");
    let board = &doc["boards"][0];
    assert_eq!(board["board"], "Repro");
    // The text's "reg.inductor: `row` from the `inductor` catalog".
    let pick = &board["picks"][0];
    assert_eq!(pick["path"], "reg.inductor");
    assert_eq!(pick["catalog"], "inductor");
    assert!(pick["row"].as_str().is_some_and(|r| !r.is_empty()));
    // A row passed over has its row, why and message, and a refusal where it is.
    let passed = pick["passed"].as_array().unwrap();
    assert!(!passed.is_empty());
    let refused = passed.iter().find(|p| p["why"] == "refused").unwrap();
    assert!(refused["at"].as_str().unwrap().starts_with("std/parts/regulator.nsm:"));
    assert!(refused["message"].as_str().unwrap().contains("min_inductance"));
    assert!(pick.get("passed_more").is_none());
    // The members at the path, with the current the inductor carries, as the text's range.
    let current = board["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["path"] == "reg.inductor.current")
        .unwrap();
    assert!(current["shown"].as_str().unwrap().contains("..50"));
    // Nothing of another part is there.
    assert!(!board["picks"].to_string().contains("shdn_pullup"));

    // Without `--all-rows` a part lists its first 20 rows, and says how many it leaves out.
    let capped = explain(&dir, &["reg", "buck.nsm", "--format", "json"]);
    assert!(capped.status.success());
    let doc = json(&capped);
    for pick in doc["boards"][0]["picks"].as_array().unwrap() {
        let listed = pick["passed"].as_array().unwrap().len();
        assert!(listed <= 20, "{pick}");
        if let Some(more) = pick.get("passed_more") {
            assert_eq!(listed, 20);
            assert!(more.as_u64().unwrap() > 0);
        }
    }
}

#[test]
fn json_for_a_path_with_nothing_is_a_finding_with_the_exit_code_the_text_has() {
    let dir = TempDir::new("explain-json-none");
    fs::write(dir.join("buck.nsm"), BUCK).unwrap();
    let none = explain(&dir, &["re", "buck.nsm", "--format", "json"]);
    assert_eq!(none.status.code(), Some(1));
    let doc = json(&none);
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["boards"], serde_json::json!([]));
    assert_eq!(doc["findings"][0]["severity"], "error");
    assert_eq!(doc["findings"][0]["code"], "explain");
    assert!(doc["findings"][0]["message"].as_str().unwrap().contains("no member and no part"));
}

/// An invented host processor with six pins, a UART and an I2C controller that share one serial
/// unit and a second UART, and a board whose module `pullups` takes the I2C controller (issue
/// 1284). The footprint is a generator's, so no KiCad is needed.
const HOST: &str = r#"import std::bus::{I2c, I2cRole, Uart}
import std::parts::passive::Resistor

part Zhost {
    peripheral UARTE0 on GPIO as Uart { UARTE0_TX:output as tx; UARTE0_RX:input as rx }
    peripheral TWIM0 on GPIO as I2c I2cRole::Controller { TWIM0_SCL as scl; TWIM0_SDA as sda }
    peripheral UARTE1 on GPIO as Uart { UARTE1_TX:output as tx; UARTE1_RX:input as rx }
    shared SERIAL0 { UARTE0 TWIM0 }

    pins {
        .p00 bidirectional "P0.00" "1" fn GPIO
        .p01 bidirectional "P0.01" "2" fn GPIO
        .p02 bidirectional "P0.02" "3" fn GPIO
        .p03 bidirectional "P0.03" "4" fn GPIO
        .p04 bidirectional "P0.04" "5" fn GPIO
        .p05 bidirectional "P0.05" "6" fn GPIO
    }
}

module Pullups {
    .scl: Pin
    .sda: Pin
    .r1: Resistor { .resistance &= 4.7kohm +/- 5% }
    .r2: Resistor { .resistance &= 4.7kohm +/- 5% }
    .scl ~ .r1.a
    .sda ~ .r2.a
}

board Bench {
    .mcu: Zhost
    .pullups: Pullups
    .led: Resistor { .resistance &= 1kohm +/- 5% }
    .mcu.p00.function &= TWIM0_SCL
    .mcu.p01.function &= TWIM0_SDA
    .mcu.p00 ~ .pullups.scl
    .mcu.p01 ~ .pullups.sda
    .mcu.p02.function &= GPIO
    .mcu.p02 ~ .led.a
}
"#;

fn host_project(label: &str) -> TempDir {
    let dir = TempDir::new(label);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"budget\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(
        dir.join("kicad.toml"),
        "[[part]]\nkind = \"Zhost\"\npackage = \"SOT-23-6\"\ndefault = true\nfootprint = \"gen:SOT23(pins=6)\"\n",
    )
    .unwrap();
    fs::write(dir.join("src/main.nsm"), HOST).unwrap();
    dir
}

#[test]
fn a_host_says_its_pin_budget_by_module_and_the_serial_blocks_left() {
    let dir = host_project("explain-budget");
    let run = explain(&dir, &["mcu"]);
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let said = String::from_utf8_lossy(&run.stdout);
    assert!(
        said.contains(
            "  pin budget of `mcu` (`Zhost`, U1): 6 pins, 3 used, 3 left\n    (board)  1 pin (P0.02)\n    pullups  2 pins (P0.00, P0.01); serial blocks TWIM0\n"
        ),
        "{said}"
    );
    // UARTE0 shares its unit with TWIM0, which is in use, so only UARTE1 is left.
    assert!(
        said.contains("    serial blocks: I2c 1 of 1 used, 0 left; Uart 0 of 2 used, 1 left (UARTE1)\n"),
        "{said}"
    );
    // A part that is no host has no budget.
    let led = explain(&dir, &["led"]);
    assert!(!String::from_utf8_lossy(&led.stdout).contains("pin budget"));
}

#[test]
fn json_has_the_pin_budget_of_a_host() {
    let dir = host_project("explain-budget-json");
    let run = explain(&dir, &["mcu", "--format", "json"]);
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let doc = json(&run);
    let budget = &doc["boards"][0]["pin_budgets"][0];
    assert_eq!(budget["host"], "mcu");
    assert_eq!((budget["pins"].as_u64(), budget["used"].as_u64(), budget["left"].as_u64()), (Some(6), Some(3), Some(3)));
    assert_eq!(budget["modules"][1]["module"], "pullups");
    assert_eq!(budget["modules"][1]["pins"], serde_json::json!(["P0.00", "P0.01"]));
    assert_eq!(budget["modules"][1]["blocks"], serde_json::json!(["TWIM0"]));
    let blocks = budget["blocks"].as_array().unwrap();
    let block = |name: &str| blocks.iter().find(|b| b["name"] == name).unwrap();
    assert_eq!(block("TWIM0")["bus"], "I2c");
    assert_eq!((block("TWIM0")["used"].as_bool(), block("TWIM0")["left"].as_bool()), (Some(true), Some(false)));
    assert_eq!(block("UARTE0")["left"], false);
    assert_eq!(block("UARTE1")["left"], true);
    // The board's own record has no budget for a path that holds no host.
    let led = explain(&dir, &["led", "--format", "json"]);
    assert!(json(&led)["boards"][0].get("pin_budgets").is_none());
}
