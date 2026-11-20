//! `netsmith board build` on a project with a hold-up (issue #1255): a supercapacitor charged to
//! 5 V behind a Schottky diode and a 3.3 V linear regulator, and a load of 0.15 W on the
//! regulator's output that must be held up for a while. The diode and the regulator are standard
//! rows (the 1N5819WS, 0.6 V; the XC6206P332, 0.68 V of dropout); the supercapacitor's row and its
//! curve of DC ESR over temperature are invented, since no real part has a recorded source yet
//! (`data/jlcpcb/README.md`). The board is the guide's (chapter 19, _Holding a rail up_).
//!
//! By hand: the storage starts at 4.9 V and keeps 0.8 F new, 0.56 F at the end of its life; the
//! rail's least is 3.135 V. A peak of 100 mA drops 0.68 V in the regulator and 0.6 V across the
//! diode, so the storage must keep 4.415 V plus the peak across its ESR: 250 mohm at -20 C
//! new, 500 mohm at the end of its life, 4.465 V and 0.435 V to give, 0.2436 C. The load draws
//! 47.85 mA at 3.135 V, and the storage leaks 5 uA: 0.2393 C over 5 s, 0.2871 C over 6 s.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

const SOURCE: &str = r#"
import std::checks::HoldUp
import std::env::{Corner, Environment, Life}
import std::parts::diode::Schottky
import std::parts::passive::Supercap
import std::parts::regulator::LinearRegulator
import std::sources::Rail

board Logger {
    .environment: Environment { .temperature &= -20degC..85degC }
    .new: Corner { .temperature &= 25degC; .life &= Life::New }
    .eol_cold: Corner { .temperature &= -20degC; .life &= Life::End }

    .backup: Rail { .voltage &= 5V +/- 2%; .pos.net &= "VB" }
    .sc: Supercap
    .backup.pos ~> .sc ~> .backup.neg

    .vin: Rail { .voltage &= 4.2V..4.6V; .pos.net &= "VIN" }
    .vin.neg ~ .backup.neg
    .d: Schottky { .current &= 0mA..100mA }
    .backup.pos ~> .d ~> .vin.pos

    .ldo: LinearRegulator { .vout.voltage &= 3.3V +/- 5% }
    .vin ~ .ldo.vin
    .out: Rail { .current &= 0mA..60mA; .pos.net &= "V3V3" }
    .ldo.vout ~ .out

    .hold: HoldUp { .average_power &= 0.15W; .peak_current &= 100mA; .duration &= 5s }
    .hold.storage ~ .sc.p
    .hold.rail ~ .out.pos
}
"#;

const SUPERCAP: &str = "mpn,capacitance,esr,dc_esr,leakage_current,max_voltage,cells,soldering,package,capacitance_at_end_of_life,esr_growth,source
ZZS-1F5,1F +/- 20%,60mohm,100mohm,5uA,5.5V,2,Reflow,Radial_D16.0mm_P7.50mm,0.7,2,https://example.invalid/zzs-1f5.pdf
";

const DC_ESR: &str = "temperature,dc_esr
-20degC,250mohm
25degC,100mohm
85degC,90mohm
";

/// `netsmith board build` on a project of one source, and what it printed.
fn build(root: &Path, source: &str) -> std::process::Output {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("catalogs/curves/supercap")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"logger\"\nnetsmith = \"0.5.0\"\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), source).unwrap();
    fs::write(root.join("catalogs/supercap.csv"), SUPERCAP).unwrap();
    fs::write(root.join("catalogs/curves/supercap/ZZS-1F5.csv"), DC_ESR).unwrap();
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["board", "build", "--out", "out"])
        .current_dir(root)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env("NETSMITH_NO_KICAD", "1")
        .output()
        .unwrap()
}

fn printed(run: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    )
}

#[test]
fn a_supercapacitor_that_holds_the_rail_up_at_every_corner_builds() {
    let root = TempDir::new("hold-up");
    let run = build(&root, SOURCE);
    let text = printed(&run);
    assert!(run.status.success(), "{text}");
    assert!(!text.contains("does not hold"), "{text}");
    assert!(!text.contains("hold-up `hold` is not judged"), "{text}");
    let lock = fs::read_to_string(root.join("src/main.lock.toml")).unwrap();
    assert!(lock.contains("ZZS-1F5"), "{lock}");
}

#[test]
fn a_hold_up_too_long_for_the_end_of_life_fails_and_names_the_corner() {
    let root = TempDir::new("hold-up-long");
    let run = build(&root, &SOURCE.replace(".duration &= 5s", ".duration &= 6s"));
    let text = printed(&run);
    assert_eq!(run.status.code(), Some(1), "{text}");
    let line = text
        .lines()
        .find(|l| l.contains("does not hold"))
        .unwrap_or_else(|| panic!("no hold-up error:\n{text}"));
    assert!(line.contains("`hold` does not hold `out` up at the corner `eol_cold`"), "{line}");
    assert!(line.contains("between its starting 4.9V and 4.465V"), "{line}");
    assert!(line.contains("560mF of effective capacitance"), "{line}");
    // Only the end of life fails: new, the storage holds the 6 s.
    assert_eq!(text.matches("does not hold").count(), 1, "{text}");
    // The guide quotes the message.
    assert!(GUIDE.contains(line), "the guide's message is not\n{line}");
}

const GUIDE: &str = include_str!("../../../docs/language/19-load-current.md");

#[test]
fn the_guide_shows_this_board() {
    assert!(GUIDE.contains(SOURCE.trim()), "chapter 19 has another board");
}
