//! A board of a system reads its own layout file (issue #1327): a system whose boards are
//! defined in other files of the project places each of them with the layout file beside the
//! file it is defined in, the one it reads when it is built alone, and `netsmith layout` writes
//! there. A system's layout file written before, with a table for a member, is still read for
//! that member, and wins, with a note that the member's own file exists. The boards are invented:
//! a small regulator board and a small LED board, mated by their connectors.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use netsmith_testkit::TempDir;

const POWER: &str = "\
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::regulator::LinearRegulator
import std::sources::Rail

board PowerBoard {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .input: Rail { .voltage &= 5V +/- 5%; .pos.net &= \"VIN\" }
    .output: Rail { .pos.net &= \"3V3\"; .voltage &= 3.3V +/- 5%; .current &= 0mA..50mA }
    .regulator: LinearRegulator
    .input ~ .regulator.vin
    .regulator.vout ~ .output
    .power_in: Header1x2
    .power_out: Header1x2
    .input.pos ~ .power_in.p1
    .input.neg ~ .power_in.p2
    .output.pos ~ .power_out.p1
    .output.neg ~ .power_out.p2
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

const KIT: &str = "\
import src::power::PowerBoard
import src::lamp::LampBoard

system Kit {
    .power: PowerBoard
    .lamp: LampBoard
    .power.power_out.mating ~ .lamp.power_in.mating
}
";

/// A project with the two boards in files of their own and the system in a third.
fn project(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"kit\"\ntop_level = [\"src/kit.nsm\"]\n",
    )
    .unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/power.nsm"), POWER).unwrap();
    fs::write(dir.join("src/lamp.nsm"), LAMP).unwrap();
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

fn code(run: &Output) -> i32 {
    run.status.code().unwrap()
}

/// The place `layout parts` gives the part at `path` of the board `board` of the system: the
/// board placed as `dev board3d` places it.
fn placed_at(dir: &Path, board: &str, path: &str) -> (f64, f64) {
    let run = netsmith(
        dir,
        &[
            "layout", "parts", "--file", "src/kit.nsm", "--board", board, path, "--format", "json",
        ],
    );
    assert_eq!(code(&run), 0, "{}", said(&run));
    let json: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let at = &json["parts"][0]["frame"]["at"];
    (at[0].as_f64().unwrap(), at[1].as_f64().unwrap())
}

const REGULATOR_AT_10_8: &str = "\
version = 1

[PowerBoard.parts.regulator]
at = [10.0, 8.0]
rotation = 0
side = \"top\"
lock = true
source = \"hand\"
";

#[test]
fn a_member_is_placed_by_the_layout_file_beside_its_own_source() {
    let dir = project("system-layout-own");
    fs::write(dir.join("src/power.pcb.toml"), REGULATOR_AT_10_8).unwrap();
    assert_eq!(placed_at(&dir, "Kit/power", "regulator"), (10.0, 8.0));
    // `layout show` and `layout check` of the member read the same file.
    let run = netsmith(
        &dir,
        &["layout", "show", "--file", "src/kit.nsm", "--board", "Kit/power"],
    );
    assert_eq!(code(&run), 0, "{}", said(&run));
    let text = said(&run);
    assert!(text.contains("power.pcb.toml"), "{text}");
    assert!(text.contains("at 10, 8 mm"), "{text}");
    let run = netsmith(
        &dir,
        &["layout", "check", "--file", "src/kit.nsm", "--board", "Kit/power"],
    );
    assert_eq!(code(&run), 0, "{}", said(&run));
    // The system's layout file is not needed, and nothing made one.
    assert!(!dir.join("src/kit.pcb.toml").exists());
}

#[test]
fn layout_set_and_capture_of_a_member_write_its_own_file() {
    let dir = project("system-layout-write");
    let run = netsmith(
        &dir,
        &[
            "layout", "set", "--file", "src/kit.nsm", "--board", "Kit/lamp", "r1", "--at", "4,4",
        ],
    );
    assert_eq!(code(&run), 0, "{}", said(&run));
    let lamp = fs::read_to_string(dir.join("src/lamp.pcb.toml")).unwrap();
    assert!(lamp.contains("[LampBoard.parts.r1]"), "{lamp}");
    assert!(!dir.join("src/kit.pcb.toml").exists());

    let run = netsmith(
        &dir,
        &["layout", "capture", "--file", "src/kit.nsm", "--board", "Kit/power"],
    );
    assert_eq!(code(&run), 0, "{}", said(&run));
    let power = fs::read_to_string(dir.join("src/power.pcb.toml")).unwrap();
    assert!(power.contains("[PowerBoard.parts.regulator]"), "{power}");
    assert!(!dir.join("src/kit.pcb.toml").exists());

    // The board built alone reads what the system wrote.
    let run = netsmith(&dir, &["layout", "show", "--file", "src/lamp.nsm", "r1"]);
    assert_eq!(code(&run), 0, "{}", said(&run));
    assert!(said(&run).contains("at 4, 4 mm"), "{}", said(&run));
}

#[test]
fn a_table_in_the_systems_layout_file_still_wins_and_is_said() {
    let dir = project("system-layout-old");
    fs::write(dir.join("src/power.pcb.toml"), REGULATOR_AT_10_8).unwrap();
    fs::write(
        dir.join("src/kit.pcb.toml"),
        REGULATOR_AT_10_8.replace("[10.0, 8.0]", "[12.0, 6.0]"),
    )
    .unwrap();
    assert_eq!(placed_at(&dir, "Kit/power", "regulator"), (12.0, 6.0));
    let run = netsmith(
        &dir,
        &[
            "layout", "show", "--file", "src/kit.nsm", "--board", "Kit/power", "regulator",
        ],
    );
    assert_eq!(code(&run), 0, "{}", said(&run));
    let text = said(&run);
    assert!(text.contains("at 12, 6 mm"), "{text}");
    assert!(
        text.contains("note: src/kit.pcb.toml: the table `PowerBoard` is read for `Kit/power`")
            && text.contains("src/power.pcb.toml, is not"),
        "{text}"
    );
    // The other member, which has no table there, reads its own file.
    fs::write(
        dir.join("src/lamp.pcb.toml"),
        "version = 1\n\n[LampBoard.parts.r1]\nat = [4.0, 4.0]\nlock = true\n",
    )
    .unwrap();
    assert_eq!(placed_at(&dir, "Kit/lamp", "r1"), (4.0, 4.0));
}

#[test]
fn fab_of_the_system_reads_each_members_own_file() {
    let dir = project("system-layout-fab");
    // The power board's table is still in the system's file, as an older project has it.
    fs::write(dir.join("src/power.pcb.toml"), REGULATOR_AT_10_8).unwrap();
    fs::write(dir.join("src/kit.pcb.toml"), REGULATOR_AT_10_8).unwrap();
    // A wrong entry in the lamp board's own file stops `fab` of the system, naming that file:
    // it is the file `fab` reads for it.
    fs::write(
        dir.join("src/lamp.pcb.toml"),
        "version = 1\n\n[LampBoard.parts.nothing]\nat = [4.0, 4.0]\n",
    )
    .unwrap();
    let run = netsmith(&dir, &["board", "fab", "src/kit.nsm", "--out", "fab"]);
    assert_ne!(code(&run), 0, "{}", said(&run));
    let text = said(&run);
    assert!(
        text.contains("Kit/power: note: src/kit.pcb.toml: the table `PowerBoard` is read"),
        "{text}"
    );
    assert!(
        text.contains("error: Kit/lamp: src/lamp.pcb.toml: the board `LampBoard` has no part `nothing`"),
        "{text}"
    );
}
