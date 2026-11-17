//! `netsmith layout regenerate` (issue #825): one module placed again with the rest held, the
//! proposal printed, and written to the layout file only with `--write`. The board is the
//! regulator example, whose `stage` module has three parts. With the router's copper kept on disk
//! (issue #1019), `fab --reuse-routes` starts from it and the proposal says what of it is kept.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use netsmith_testkit::TempDir;

const BOARD: &str = include_str!("../../../examples/linear_regulator.nsm");

fn project(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"regulator\"\ntop_level = [\"main.nsm\"]\n",
    )
    .unwrap();
    fs::write(dir.join("main.nsm"), BOARD).unwrap();
    dir
}

fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .arg("layout")
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The proposal of `seed` as JSON, after a run that must succeed.
fn proposal(dir: &Path, extra: &[&str]) -> serde_json::Value {
    let mut args = vec!["regenerate", "stage", "--format", "json"];
    args.extend_from_slice(extra);
    let run = netsmith(dir, &args);
    assert_eq!(run.status.code(), Some(0), "{}", text(&run.stderr));
    serde_json::from_str(&text(&run.stdout)).unwrap_or_else(|e| panic!("{e}: {}", text(&run.stdout)))
}

#[test]
fn regenerate_proposes_without_writing_and_writes_with_write() {
    let dir = project("layout-regenerate");
    // A seed that moves a part (a seed may find the arrangement that is there).
    let (seed, json) = (0..12)
        .map(|seed| (seed, proposal(&dir, &["--seed", &seed.to_string()])))
        .find(|(_, json)| {
            json["parts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["was"]["at"] != p["after"]["at"] || p["was"]["rotation"] != p["after"]["rotation"])
        })
        .expect("a seed moves a part");
    assert_eq!(json["module"], "stage");
    assert_eq!(json["written"], false);
    let parts: Vec<&str> = json["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["path"].as_str().unwrap())
        .collect();
    assert_eq!(parts.len(), 3, "{json}");
    assert!(parts.iter().all(|p| p.starts_with("stage.")), "{parts:?}");
    assert!(!json["nets"].as_array().unwrap().is_empty(), "{json}");
    assert!(json["comparison"]["before"]["hpwl"].as_f64().is_some(), "{json}");
    assert!(!dir.join("main.pcb.toml").exists(), "a proposal wrote the layout file");

    // The same seed again is the same proposal.
    let again = proposal(&dir, &["--seed", &seed.to_string()]);
    assert_eq!(again["parts"], json["parts"]);

    // Written: an entry for each part placed again, unlocked, `source = "regen"`.
    let written = proposal(&dir, &["--seed", &seed.to_string(), "--write"]);
    assert_eq!(written["written"], true);
    let file = fs::read_to_string(dir.join("main.pcb.toml")).unwrap();
    for part in &parts {
        assert!(file.contains(&format!("[RegulatorExample.parts.\"{part}\"]")) || file.contains(&format!("[RegulatorExample.parts.{part}]")), "{part} has no entry:\n{file}");
    }
    assert_eq!(file.matches("source = \"regen\"").count(), 3, "{file}");
    // The board placed with the file has the parts where the proposal put them.
    let show = netsmith(&dir, &["show", "stage", "--format", "json"]);
    assert_eq!(show.status.code(), Some(0), "{}", text(&show.stderr));
    let shown: serde_json::Value = serde_json::from_str(&text(&show.stdout)).unwrap();
    for p in json["parts"].as_array().unwrap() {
        let entry = shown["parts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["path"] == p["path"])
            .unwrap();
        assert_eq!(entry["entry"]["at"], p["after"]["at"], "{}", p["path"]);
    }
    // A second regeneration of the written board moves from those places.
    let next = proposal(&dir, &["--seed", &(seed + 1).to_string()]);
    for p in next["parts"].as_array().unwrap() {
        let first = json["parts"].as_array().unwrap().iter().find(|q| q["path"] == p["path"]).unwrap();
        assert_eq!(p["was"]["at"], first["after"]["at"], "{}", p["path"]);
    }
}

#[test]
fn regenerate_refuses_a_module_the_board_lacks_or_holds_whole() {
    let dir = project("layout-regenerate-refused");
    let run = netsmith(&dir, &["regenerate", "stag"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run.stderr));
    assert!(text(&run.stderr).contains("no part in `stag`; its modules include `stage`"), "{}", text(&run.stderr));
    assert!(!dir.join("main.pcb.toml").exists());

    // Every part of the module locked: nothing to place.
    for part in ["stage.input_cap", "stage.output_cap", "stage.regulator"] {
        let set = netsmith(&dir, &["set", part, "--at", "1,1", "--lock"]);
        assert_eq!(set.status.code(), Some(0), "{}", text(&set.stderr));
    }
    let before = fs::read_to_string(dir.join("main.pcb.toml")).unwrap();
    let run = netsmith(&dir, &["regenerate", "stage", "--write"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run.stderr));
    assert!(text(&run.stderr).contains("every part of `stage` is held"), "{}", text(&run.stderr));
    assert_eq!(fs::read_to_string(dir.join("main.pcb.toml")).unwrap(), before);
}

fn fab(dir: &Path, args: &[&str]) -> String {
    let run = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["fab", "--no-glb", "--no-step"])
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap();
    let said = format!("{}{}", text(&run.stdout), text(&run.stderr));
    assert_eq!(run.status.code(), Some(0), "{said}");
    said
}

/// The kept files of the project (issue #1019).
fn kept_files(dir: &Path) -> Vec<String> {
    let mut files: Vec<String> = fs::read_dir(dir.join(".netsmith/routes"))
        .map(|d| d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    files.sort();
    files
}

/// `(kept, routed again)` from fab's line "kept the router's copper of N net(s) from F, routed M
/// again".
fn carried(said: &str) -> Option<(usize, usize)> {
    let line = said.lines().find(|l| l.contains("kept the router's copper of "))?;
    let after = line.split("kept the router's copper of ").nth(1)?;
    let kept = after.split(' ').next()?.parse().ok()?;
    let again = line.split(", routed ").nth(1)?.split(' ').next()?.parse().ok()?;
    Some((kept, again))
}

/// Three modules of a resistor and an LED across one rail: regenerating `a` touches the rail
/// and its own middle net, not the middle nets of `b` and `c`.
const LIGHTS: &str = "\
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

/// A resistor and an LED across a rail.
module Indicator {
    .vin: Rail
    .led: Led {
        .current &= 5mA..15mA
    }
    .r: Resistor {}
    .vin.pos ~> .r ~> .led ~> .vin.neg
}

board Lights {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
    }
    .a: Indicator
    .b: Indicator
    .c: Indicator
    .supply ~ .a.vin
    .supply ~ .b.vin
    .supply ~ .c.vin
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
}
";

#[test]
fn fab_reuses_the_kept_copper_only_when_asked_and_regenerate_says_what_it_keeps() {
    let dir = TempDir::new("layout-regenerate-kept");
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"lights\"\ntop_level = [\"main.nsm\"]\n",
    )
    .unwrap();
    fs::write(dir.join("main.nsm"), LIGHTS).unwrap();
    // A plain fab keeps nothing and reads nothing.
    let plain = fab(&dir, &[]);
    assert!(kept_files(&dir).is_empty(), "{plain}");
    assert!(carried(&plain).is_none(), "{plain}");
    // The first run with --reuse-routes has nothing to start from, and keeps what it routed.
    let first = fab(&dir, &["--reuse-routes"]);
    assert!(carried(&first).is_none(), "{first}");
    let files = kept_files(&dir);
    assert_eq!(files.len(), 1, "{files:?}");
    assert!(files[0].starts_with("main."), "{files:?}");
    // The second keeps the copper of every net and routes none again: nothing moved.
    let second = fab(&dir, &["--reuse-routes"]);
    let (kept, again) = carried(&second).unwrap_or_else(|| panic!("{second}"));
    println!("kept {kept} net(s), routed {again} again");
    assert!(kept > 0, "{second}");
    assert_eq!(again, 0, "{second}");

    // A regeneration of `a` drops the copper of its nets and says what it keeps.
    let regenerate = |seed: u32, write: bool| {
        let seed = seed.to_string();
        let mut args = vec!["regenerate", "a", "--format", "json", "--seed", &seed];
        if write {
            args.push("--write");
        }
        let run = netsmith(&dir, &args);
        assert_eq!(run.status.code(), Some(0), "{}", text(&run.stderr));
        serde_json::from_str::<serde_json::Value>(&text(&run.stdout)).unwrap()
    };
    let (seed, json) = (0..12)
        .map(|seed| (seed, regenerate(seed, false)))
        .find(|(_, json)| {
            json["parts"].as_array().unwrap().iter().any(|p| p["was"]["at"] != p["after"]["at"])
        })
        .expect("a seed moves a part");
    let copper = &json["copper"];
    println!("{copper}");
    assert!(copper["unused"].is_null(), "{json}");
    assert!(copper["file"].as_str().unwrap().ends_with(&files[0]), "{json}");
    let kept_nets: Vec<&str> = copper["kept"].as_array().unwrap().iter().map(|n| n.as_str().unwrap()).collect();
    let again_nets: Vec<&str> = copper["again"].as_array().unwrap().iter().map(|n| n.as_str().unwrap()).collect();
    assert_eq!(kept_nets.len() + again_nets.len(), kept, "{json}");
    for net in ["net-b.led.anode", "net-c.led.anode"] {
        assert!(kept_nets.contains(&net), "{net} lost its copper: {json}");
    }
    for net in json["nets"].as_array().unwrap() {
        let net = net.as_str().unwrap();
        assert!(!kept_nets.contains(&net), "{net} was kept: {json}");
    }

    // Written, and made again: fab keeps the copper the proposal said it would.
    regenerate(seed, true);
    let third = fab(&dir, &["--reuse-routes"]);
    let (kept, again) = carried(&third).unwrap_or_else(|| panic!("{third}"));
    println!("after the regeneration: kept {kept} net(s), routed {again} again");
    assert_eq!(kept, kept_nets.len(), "{third}");
    assert_eq!(again, again_nets.len(), "{third}");
}
