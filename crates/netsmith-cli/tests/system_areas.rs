//! The area another member of a system covers over a board (issue 1331): derived from the
//! system's mates, a keepout-by-height carried by the board's connector, which the board's
//! placer and `layout check` keep to. The boards are invented: a small regulator board and a
//! small LED board on its header, the LED board's socket locked in its own layout file.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use netsmith_testkit::TempDir;

const POWER: &str = "\
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::parts::regulator::LinearRegulator
import std::sources::Rail

board PowerBoard {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .input: Rail { .voltage &= 5V +/- 5%; .pos.net &= \"VIN\" }
    .output: Rail { .pos.net &= \"3V3\"; .voltage &= 3.3V +/- 5%; .current &= 0mA..50mA }
    .regulator: LinearRegulator
    .input ~ .regulator.vin
    .regulator.vout ~ .output
    .power_out: Header1x2
    .output.pos ~ .power_out.p1
    .output.neg ~ .power_out.p2
    /// A load on the input, taller than the 10.65 mm between the two boards' faces.
    .tall: Resistor { .resistance &= 10kohm +/- 5%; .height &= 12mm }
    .input.pos ~> .tall ~> .input.neg
}
";

const POWER_PHYSICAL: &str = "\
import std::pcb::Outline

physical for PowerBoard {
    case {
        .outline: Outline { .width &= 40mm; .height &= 20mm }
    }
}
";

const LAMP: &str = "\
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Socket1x2
import std::parts::passive::{Led, Resistor}
import std::sources::Rail

board LampBoard {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .input: Rail { .pos.net &= \"3V3\" }
    .led: Led { .current &= 5mA..20mA }
    .r1: Resistor {}
    .power_in: Socket1x2
    .input.pos ~ .power_in.p1
    .input.neg ~ .power_in.p2
    .input.pos ~> .r1 ~> .led ~> .input.neg
}
";

const LAMP_PHYSICAL: &str = "\
import std::pcb::Outline

physical for LampBoard {
    case {
        .outline: Outline { .width &= 20mm; .height &= 16mm }
    }
}
";

/// The lamp's socket on its back face, 4 mm from its left edge: the lamp hangs over the power
/// board's header upright, its 20 mm running to the right of the header.
const LAMP_LAYOUT: &str = "\
version = 1

[LampBoard.parts.power_in]
at = [4.0, 6.0]
rotation = 0
side = \"bottom\"
lock = true
source = \"hand\"
";

const KIT: &str = "\
import src::power::PowerBoard
import src::lamp::LampBoard

system Kit {
    .power: PowerBoard
    .lamp: LampBoard
    .power.power_out.mating ~ .lamp.power_in.mating
}
";

fn project(tag: &str, lamp_layout: bool) -> TempDir {
    let dir = TempDir::new(tag);
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"kit\"\ntop_level = [\"src/kit.nsm\"]\n",
    )
    .unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/power.nsm"), POWER).unwrap();
    fs::write(dir.join("src/power.physical.nsm"), POWER_PHYSICAL).unwrap();
    fs::write(dir.join("src/lamp.nsm"), LAMP).unwrap();
    fs::write(dir.join("src/lamp.physical.nsm"), LAMP_PHYSICAL).unwrap();
    if lamp_layout {
        fs::write(dir.join("src/lamp.pcb.toml"), LAMP_LAYOUT).unwrap();
    }
    fs::write(dir.join("src/kit.nsm"), KIT).unwrap();
    dir
}

fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn said(run: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    )
}

fn json(run: &Output) -> serde_json::Value {
    serde_json::from_slice(&run.stdout).unwrap_or_else(|e| panic!("{e}: {}", said(run)))
}

/// The power board's header and its tall resistor, locked: the header at (8, 10), the resistor
/// 6 mm to its right, under the lamp when `under` and 25 mm to the right of it, past the lamp's
/// 20 mm, when not.
fn power_layout(under: bool) -> String {
    let x = if under { 14.0 } else { 33.0 };
    format!(
        "version = 1\n\n[PowerBoard.parts.power_out]\nat = [8.0, 10.0]\nrotation = 0\nside = \"top\"\nlock = true\nsource = \"hand\"\n\n[PowerBoard.parts.tall]\nat = [{x:.1}, 10.0]\nrotation = 0\nside = \"top\"\nlock = true\nsource = \"hand\"\n"
    )
}

#[test]
fn layout_check_says_a_part_too_tall_for_the_board_mated_over_it() {
    let dir = project("system-areas-check", true);
    fs::write(dir.join("src/power.pcb.toml"), power_layout(true)).unwrap();
    let run = netsmith(
        &dir,
        &[
            "layout", "check", "--file", "src/kit.nsm", "--board", "Kit/power", "--format",
            "json",
        ],
    );
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    let checked = json(&run);
    let findings = checked["findings"].as_array().unwrap();
    let tall: Vec<&serde_json::Value> = findings
        .iter()
        .filter(|f| f["kind"] == "placement" && f["path"] == "tall")
        .collect();
    assert_eq!(tall.len(), 1, "{findings:#?}");
    let message = tall[0]["message"].as_str().unwrap();
    assert!(
        message.contains("Kit/lamp over power_out") && message.contains("10.65"),
        "{message}"
    );
    // The header that carries the area is not measured against it, and nothing else is found.
    assert_eq!(
        findings.iter().filter(|f| f["severity"] == "error").count(),
        1,
        "{findings:#?}"
    );

    // Past the lamp's edge the resistor is in no area, and the file is clean.
    fs::write(dir.join("src/power.pcb.toml"), power_layout(false)).unwrap();
    let run = netsmith(
        &dir,
        &["layout", "check", "--file", "src/kit.nsm", "--board", "Kit/power"],
    );
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
}

#[test]
fn the_placer_keeps_a_part_too_tall_for_the_board_over_it_out_from_under_it() {
    let dir = project("system-areas-place", true);
    // The header locked, the resistor left to the placer.
    let header_only = power_layout(true)
        .split("\n\n[PowerBoard.parts.tall]")
        .next()
        .unwrap()
        .to_string();
    fs::write(dir.join("src/power.pcb.toml"), header_only).unwrap();
    let run = netsmith(
        &dir,
        &[
            "dev", "board3d", "src/kit.nsm", "--board", "Kit", "--format", "json",
        ],
    );
    assert!(run.status.success(), "{}", said(&run));
    let scene = json(&run);
    // The lamp is placed on the header, and nothing of the power board stands in it.
    assert_eq!(scene["members"][1]["placed"]["mate"], 0, "{scene:#}");
    let findings = scene["findings"].as_array().unwrap();
    assert!(
        findings.iter().all(|f| f["code"] != "system_collision"),
        "{findings:#?}"
    );
    // The power board's own placement check keeps to the area as a height limit.
    let power = &scene["members"][0]["report"];
    let placement = power["placement"].as_array().cloned().unwrap_or_default();
    assert!(placement.is_empty(), "{placement:#?}");
    // The limit is the lamp's outline, 20 by 16 mm, the least mated height over the header's
    // face, and the resistor the placer placed is not under it.
    let limits = power["height_limits"].as_array().unwrap();
    let lamp = limits
        .iter()
        .find(|l| l["name"] == "Kit/lamp over power_out")
        .unwrap_or_else(|| panic!("{limits:#?}"));
    assert_eq!(lamp["carrier"], "power_out");
    assert_eq!(lamp["max_height"], 10.65);
    assert_eq!(lamp["faces"], serde_json::json!(["top"]));
    let size = |i: usize| lamp["max"][i].as_f64().unwrap() - lamp["min"][i].as_f64().unwrap();
    assert!((size(0) - 20.0).abs() < 1e-6 && (size(1) - 16.0).abs() < 1e-6, "{lamp:#}");
    assert!(lamp["margin"].as_f64().is_none_or(|m| m >= 0.0), "{lamp:#}");
}

#[test]
fn fab_says_why_no_area_is_derived_when_the_other_connector_has_no_place() {
    let dir = project("system-areas-note", false);
    let run = netsmith(&dir, &["layout", "check", "--file", "src/kit.nsm", "--board", "Kit/power"]);
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    let run = netsmith(
        &dir,
        &["board", "fab", "src/kit.nsm", "--out", "out"],
    );
    let text = said(&run);
    assert!(
        text.contains(
            "Kit/power: note: the area `lamp` covers over `power_out` is not known before placing"
        ) && text.contains("`lamp.power_in` has no place until `lamp` is placed"),
        "{text}"
    );
}
