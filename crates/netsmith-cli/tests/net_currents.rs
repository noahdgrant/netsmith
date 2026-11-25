//! One current per net (issue #1584): `board nets --currents` lists each net's current with its
//! source, and `board rules`, which reads the rules with the function the design rules check
//! reads them with, holds the net's tracks (`track_current`) and its vias (`via_current`) to
//! that one number.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use netsmith_testkit::TempDir;

/// A 12 V supply through a 1 ohm link to a 10 kohm load, and a module whose clamp diode, from
/// ground to the node after the link, states that it carries up to 1 A: the DC analysis finds
/// none through it, as it blocks.
const CLAMP: &str = "\
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::diode::Diode
import std::parts::passive::Resistor
import std::sources::Rail

module Clamp {
    .vin: Rail
    .bypass: Diode { .current &= 0A..1A }
    .vin.neg ~> .bypass ~> .vin.pos
}

board B {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail { .voltage &= 12V; .pos.net &= \"VIN\"; .neg.net &= \"GND\" }
    .link: Resistor { .resistance &= 1ohm +/- 1% }
    .load: Resistor { .resistance &= 10kohm +/- 1% }
    .clamp: Clamp
    .supply.pos ~> .link ~> .load ~> .supply.neg
    .clamp.vin.pos ~ .link.b
    .clamp.vin.neg ~ .supply.neg
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
}
";

fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn json(run: &Output) -> serde_json::Value {
    assert!(
        run.status.success(),
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    serde_json::from_slice(&run.stdout).unwrap()
}

#[test]
fn a_stated_current_in_a_module_is_its_nets_one_current_in_the_listing_and_both_checks() {
    let dir = TempDir::new("net-currents");
    fs::write(dir.join("b.nsm"), CLAMP).unwrap();

    let listed = json(&netsmith(
        &dir,
        &["board", "nets", "b.nsm", "--currents", "--format", "json"],
    ));
    let currents = listed["currents"].as_array().expect("a currents list");
    let node = currents
        .iter()
        .find(|c| c["net"] != "GND" && c["net"] != "VIN")
        .unwrap_or_else(|| panic!("no current for the node after the link: {currents:?}"));
    println!("{node}");
    assert_eq!(node["from"], "carrier", "{node}");
    assert_eq!(node["lower_bound"], false, "{node}");
    assert!((node["current"].as_f64().unwrap() - 1.0).abs() < 1e-9, "{node}");
    let net = node["net"].as_str().unwrap();

    // The rules the check holds the net to are the listing's: one number for its tracks and
    // its vias.
    let rules = json(&netsmith(&dir, &["board", "rules", "b.nsm", "--format", "json"]));
    let held = rules["nets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["net"] == net)
        .unwrap_or_else(|| panic!("no rules for {net}"));
    println!("{held}");
    let close = |a: &serde_json::Value, b: &serde_json::Value| {
        assert!(
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-9,
            "{a} is not {b}"
        )
    };
    close(&held["least_width"], &node["least_width"]);
    assert_eq!(held["vias"], node["vias"], "{held} {node}");
    assert!(node["vias"].as_u64().unwrap() >= 2, "{node}");

    // The text says the same, with its source.
    let run = netsmith(&dir, &["board", "nets", "b.nsm", "--currents"]);
    let text = String::from_utf8_lossy(&run.stdout);
    assert!(run.status.success(), "{text}");
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with(&format!("{net}: ")))
        .unwrap_or_else(|| panic!("no line for {net} in:\n{text}"));
    assert!(
        line.contains(": 1 A (carrier: part `clamp.bypass` carries 1 A, more than the "),
        "{line}"
    );
}

/// A supercapacitor across a 5 V backup rail with a Schottky diode in its return leg (issue
/// #1689), whose invented row states a discharge peak current of 3 A: the DC analysis sees the
/// capacitor block, and the node between it and the diode is the 3 A net.
const HOLD_UP: &str = "\
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::diode::Schottky
import std::parts::passive::Supercap
import std::sources::Rail

board B {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .backup: Rail { .voltage &= 5V +/- 5%; .pos.net &= \"VBAK\"; .neg.net &= \"GND\" }
    .hold: Supercap
    .leg: Schottky { .current &= 0A..10mA }
    .backup.pos ~> .hold ~> .leg ~> .backup.neg
    .hold.n.net &= \"SC_RET\"
    .power_in: Header1x2
    .backup.pos ~ .power_in.p1
    .backup.neg ~ .power_in.p2
}
";

const SUPERCAP_ROW: &str = "mpn,capacitance,esr,dc_esr,leakage_current,max_voltage,cells,soldering,package,discharge_peak_current,source
ZZS-1F,1F +/- 20%,60mohm,100mohm,5uA,5.5V,2,Reflow,Radial_D16.0mm_P7.50mm,3A,https://example.invalid/zzs-1f.pdf
";

#[test]
fn a_storage_elements_discharge_peak_current_is_the_current_of_its_return_leg_in_the_listing_and_both_checks()
 {
    let dir = TempDir::new("net-currents-discharge");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("catalogs")).unwrap();
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"hold\"\nnetsmith = \"0.5.0\"\n",
    )
    .unwrap();
    fs::write(dir.join("src/main.nsm"), HOLD_UP).unwrap();
    fs::write(dir.join("catalogs/supercap.csv"), SUPERCAP_ROW).unwrap();

    let listed = json(&netsmith(
        &dir,
        &["board", "nets", "src/main.nsm", "--currents", "--format", "json"],
    ));
    let currents = listed["currents"].as_array().expect("a currents list");
    let node = currents
        .iter()
        .find(|c| c["net"] == "SC_RET")
        .unwrap_or_else(|| panic!("no current for the return leg: {currents:?}"));
    println!("{node}");
    assert_eq!(node["from"], "carrier", "{node}");
    assert!((node["current"].as_f64().unwrap() - 3.0).abs() < 1e-9, "{node}");
    assert!(node["vias"].as_u64().unwrap() >= 2, "{node}");

    // The rules the track and via checks hold the net to are the listing's.
    let rules = json(&netsmith(
        &dir,
        &["board", "rules", "src/main.nsm", "--format", "json"],
    ));
    let held = rules["nets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["net"] == "SC_RET")
        .unwrap_or_else(|| panic!("no rules for SC_RET"));
    println!("{held}");
    assert!(
        (held["least_width"].as_f64().unwrap() - node["least_width"].as_f64().unwrap()).abs()
            < 1e-9,
        "{held} {node}"
    );
    assert_eq!(held["vias"], node["vias"], "{held} {node}");

    // The text says it came from the part.
    let run = netsmith(&dir, &["board", "nets", "src/main.nsm", "--currents"]);
    let text = String::from_utf8_lossy(&run.stdout);
    assert!(run.status.success(), "{text}");
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with("SC_RET: "))
        .unwrap_or_else(|| panic!("no line for SC_RET in:\n{text}"));
    assert!(
        line.contains(
            ": 3 A (carrier: part `hold` states a `.discharge_peak_current` of 3 A, more than the 10 mA part `leg` carries)"
        ),
        "{line}"
    );
}
