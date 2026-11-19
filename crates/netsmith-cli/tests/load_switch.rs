//! `netsmith board build` and `netsmith board power` on a board with two load switches (issue
//! 1269): an IC that provides `std::parts::switch::LoadSwitch`, which a host's pin turns on, and
//! the P-channel use, `PmosLoadSwitch`, whose gate a pull-up holds off and the host pulls low.
//! Each switched rail is supplied, each switch is a converter from the 3.3 V rail, and the 3.3 V
//! rail draws what the sensors behind the switches draw. With the IC's enable on the ground the
//! IC supplies nothing, and the note of its rail says why. `Zsw`, `Host` and `Sensor` are
//! invented; none is a real part.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

const SOURCE: &str = r#"import std::logic::Logic
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::parts::switch::{LoadSwitch, PmosLoadSwitch}
import std::mfg::jlcpcb::TwoLayer
import std::sources::Rail

/// Invented: a load switch IC, 2 A, 40 to 80 mohm.
part Zsw provides LoadSwitch {
    .on_resistance &= 40mohm..80mohm
    .max_current &= 2A
    .quiescent_current &= 0A..10uA
    pins {
        .in.pos  power "VIN"  "1"
        .in.neg  power "GND"  "2"
        .enable  input "ON"   "3"
        .out.pos power "VOUT" "4" "5"
    }
}

/// Invented: a host whose two outputs turn the switches on and off.
part Host {
    .vdd: Rail
    .vdd.current &= 1mA..20mA
    .en_a: Logic
    .en_a.reference ~ .vdd
    .en_a.reference.current &= 0A
    .en_a.voh &= .vdd.voltage - 0.4V at 4mA
    .en_a.vol &= 0.4V at 4mA
    .en_b: Logic
    .en_b.reference ~ .vdd
    .en_b.reference.current &= 0A
    .en_b.voh &= .vdd.voltage - 0.4V at 4mA
    .en_b.vol &= 0.4V at 4mA
    pins {
        .vdd.pos   power  "VDD" "1"
        .vdd.neg   power  "GND" "2" "5" "6"
        .en_a.line output "PA0" "3"
        .en_b.line output "PA1" "4"
    }
}

/// Invented: a sensor that draws from the rail a switch gives it.
part Sensor {
    .vdd: Rail
    .vdd.current &= 5mA..150mA
    pins {
        .vdd.pos power "VDD" "1"
        .vdd.neg power "GND" "2" "3"
    }
}

board Switched {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 3.3V +/- 3%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
    .host: Host
    .supply ~ .host.vdd

    // The IC: the host's pin turns it on.
    .sw: Zsw
    .supply ~ .sw.in
    .sw.enable ~ .host.en_a.line
    .sw.out.pos.net &= "3V3_A"
    .a: Sensor
    .sw.out ~ .a.vdd

    // The P-channel use: a pull-up holds it off, the host pulls the gate low.
    .psw: PmosLoadSwitch
    .supply ~ .psw.in
    .psw.out.pos.net &= "3V3_B"
    .gate_pull: Resistor { .resistance &= 100kohm +/- 5% }
    .supply.pos ~> .gate_pull ~> .psw.enable
    .psw.enable ~ .host.en_b.line
    .b: Sensor
    .psw.out ~ .b.vdd
}
"#;

const KICAD: &str = r#"[[part]]
kind = "Zsw"
package = "SOT-23-5"
default = true
footprint = "gen:SOT23(pins=5)"

[[part]]
kind = "Host"
package = "SOT-23-6"
default = true
footprint = "gen:SOT23(pins=6)"

[[part]]
kind = "Sensor"
package = "SOT-23"
default = true
footprint = "gen:SOT23(pins=3)"
"#;

fn project(root: &Path, source: &str) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"switched\"\nnetsmith = \"0.4.0\"\n",
    )
    .unwrap();
    fs::write(root.join("kicad.toml"), KICAD).unwrap();
    fs::write(root.join("src/main.nsm"), source).unwrap();
}

fn netsmith(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(root)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .output()
        .unwrap()
}

fn text(run: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    )
}

#[test]
fn both_load_switches_supply_their_rails_and_load_the_rail_in_front() {
    let root = TempDir::new("load-switch");
    project(&root, SOURCE);
    let build = netsmith(&root, &["board", "build", "--out", "out"]);
    assert!(build.status.success(), "{}", text(&build));
    let power = netsmith(&root, &["board", "power"]);
    let out = text(&power);
    assert!(power.status.success(), "{out}");
    for line in [
        "3V3 -> 3V3_A (converts: sw (Zsw))",
        "3V3 -> 3V3_B (converts: psw (PmosLoadSwitch))",
        "supplied by sw.out (rail): -150mA..-5mA",
        "supplied by psw.out (rail): -150mA..-5mA",
        "load sw.in (rail): 5mA..150.1mA",
        "load psw.in (rail): 5mA..150mA",
        "capacity 0A..2A (from sw (Zsw)): within budget",
        "No power issues.",
    ] {
        assert!(out.contains(line), "{line}\n{out}");
    }
    // The drop: the switched rails are below the 3.3 V rail's least by the on resistance times
    // the load.
    assert!(out.contains("3V3_A: 3.189V..3.399V"), "{out}");
}

#[test]
fn a_load_switch_whose_enable_is_grounded_leaves_its_rail_with_no_source() {
    let root = TempDir::new("load-switch-off");
    let source = SOURCE.replacen(".sw.enable ~ .host.en_a.line", ".sw.enable ~ .supply.neg", 1);
    assert_ne!(source, SOURCE);
    project(&root, &source);
    let power = netsmith(&root, &["board", "power"]);
    let out = text(&power);
    assert!(
        out.contains("3V3_A has no source: the load switch `sw` is off, since its enable `sw.enable` is held low"),
        "{out}"
    );
    assert!(!out.contains("3V3 -> 3V3_A"), "{out}");
}
