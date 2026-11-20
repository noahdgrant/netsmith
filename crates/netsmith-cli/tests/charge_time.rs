//! `netsmith board build` on a project whose charger charges a backup supercapacitor (issue
//! #1264): a 5 V buck that gives at most 600 mA feeds an MCU and an invented linear charger, which
//! charges an invented 0.5 F supercapacitor within the connection's `.charge_time_limit`. The
//! board builds and writes its schematic; a larger programming resistor makes the charge too slow
//! and a smaller one takes more than the buck gives, and the build fails and says which. The
//! board is the guide's (chapter 28, _The time a charge takes, and what the charger takes from
//! its supply_).

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

const SOURCE: &str = r#"
import std::parts::passive::{Resistor, Supercap}
import std::parts::regulator::{Buck, Charger, Lmr16006y}
import std::sources::Rail

/// Invented: a charger IC whose fast charge current is 1000 V over the resistor on `PROG`.
part Zchg provides Charger {
    .package: Package
    .package &= "SOT-23-5"
    pins "SOT-23-5" {
        .nc      no_connect "NC" "1"
        .out.pos power "VBAT" "3"
        .vin.neg power "VSS"  "2"
        .vin.pos power "VDD"  "4"
        .prog    input "PROG" "5"
    }
    /// The resistor from `PROG` to `VSS`.
    .rprog: Resistance
    .charge_current == 1000V / .rprog
    .vin.current == .charge_current + 1mA
    .float_voltage &= 4.75V..4.85V
}

board Backup {
    .input: Rail { .voltage &= 9V..16V; .peak &= 24V; .pos.net &= "VIN" }
    .reg: Buck<Lmr16006y> { .vout.pos.net &= "V5"; .vout.voltage &= 5V +/- 5% }
    .input ~ .reg.vin
    .mcu: Rail { .current &= 5mA..150mA }
    .reg.vout ~ .mcu
    .charger: Zchg
    .reg.vout ~ .charger.vin
    .rprog: Resistor { .resistance &= 2.7kohm +/- 1% }
    .charger.rprog == .rprog.resistance
    .charger.prog ~> .rprog ~> .reg.vout.neg
    .charger.out.voltage &= 0V..4.8V
    .charger.out.pos.net &= "VCAP"
    .charger.out.charge_time_limit &= ..10s
    .hold: Supercap
    .charger.out.pos ~> .hold ~> .charger.out.neg
}
"#;

/// The project's supercapacitor: an invented row, as the standard catalog has none.
const SUPERCAP: &str = "mpn,capacitance,esr,dc_esr,leakage_current,max_voltage,cells,soldering,package,height_growth,source
ZZS-0F5,0.5F +/- 20%,120mohm,200mohm,6uA,5.5V,2,Reflow,Radial_D16.0mm_P7.50mm,1mm,invented for a test
";

/// The charger's footprint from the generator; the supercapacitor is in the standard mapping.
const KICAD: &str = r#"[[part]]
kind = "Zchg"
package = "SOT-23-5"
default = true
footprint = "gen:SOT23(pins=5)"
"#;

/// `netsmith board build` on the project with the programming resistor `rprog`, and what it
/// printed.
fn build(root: &Path, rprog: &str) -> std::process::Output {
    for dir in ["src", "catalogs"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"backup\"\nnetsmith = \"0.5.0\"\n",
    )
    .unwrap();
    fs::write(
        root.join("src/main.nsm"),
        SOURCE.replace("2.7kohm", rprog),
    )
    .unwrap();
    fs::write(root.join("catalogs/supercap.csv"), SUPERCAP).unwrap();
    fs::write(root.join("kicad.toml"), KICAD).unwrap();
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["board", "build", "--out", "out"])
        .current_dir(root)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
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
fn a_charge_within_its_limit_and_the_regulator_s_budget_builds() {
    let root = TempDir::new("charge-time");
    let run = build(&root, "2.7kohm");
    let text = printed(&run);
    assert!(run.status.success(), "{text}");
    assert!(!text.contains("to charge"), "{text}");
    assert!(!text.contains("while `charger` charges"), "{text}");
    assert!(root.join("out/Backup").is_dir(), "{text}");
}

#[test]
fn a_charge_that_is_too_slow_fails_and_says_the_current_it_needs() {
    let root = TempDir::new("charge-time-slow");
    let run = build(&root, "10kohm");
    let text = printed(&run);
    assert_eq!(run.status.code(), Some(1), "{text}");
    assert!(
        text.contains("`charger` takes up to 28.79s to charge 600mF across the rail (`hold`) from 0V to 4.75V"),
        "{text}"
    );
    assert!(
        text.contains("a charge in that time needs at least 285mA of charge current"),
        "{text}"
    );
}

#[test]
fn a_charge_current_above_the_regulator_s_budget_fails_and_names_the_charge() {
    let root = TempDir::new("charge-time-budget");
    let run = build(&root, "1.5kohm");
    let text = printed(&run);
    assert_eq!(run.status.code(), Some(1), "{text}");
    assert!(
        text.contains("while `charger` charges it takes up to 674.4mA from `charger.vin`"),
        "{text}"
    );
    assert!(
        text.contains("so the source gives up to 824.4mA, above the 600mA it can give"),
        "{text}"
    );
}
