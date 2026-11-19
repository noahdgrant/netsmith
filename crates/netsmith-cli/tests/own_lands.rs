//! `netsmith board build`, with no KiCad install, of a board with a tapped supercapacitor whose
//! `.mid` is joined and of a board with a coin cell with solder tabs (issue 1399): both packages
//! are mapped by the standard `kicad.toml` to netsmith's own symbol and generated land
//! (`Device:C_Polarized_Tap` on `netsmith:CP_Radial_D16.0mm_P7.50mm_Tap`, and
//! `Device:Battery_Cell` on `netsmith:BatteryCell_Coin_D20.0mm_Tabs_P15.00mm`), so they build and
//! export. The boards are the guide's (chapter 28).

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

const TAPPED: &str = r#"
import std::parts::passive::{Resistor, Supercap}
import std::sources::Rail

board Balanced {
    .rail: Rail { .voltage &= 5V +/- 5%; .pos.net &= "V5" }
    .hold: Supercap
    .rail.pos ~> .hold ~> .rail.neg
    .top: Resistor { .resistance &= 100kohm +/- 1% }
    .bottom: Resistor { .resistance &= 100kohm +/- 1% }
    .rail.pos ~> .top ~> .hold.mid
    .hold.mid ~> .bottom ~> .rail.neg
}
"#;

const SUPERCAP: &str = "mpn,capacitance,esr,dc_esr,leakage_current,max_voltage,cells,soldering,package,height_growth,library,source
ZZS-0F5-T,0.5F +/- 20%,120mohm,200mohm,6uA,5.5V,2,Reflow,Radial_D16.0mm_P7.50mm_Tap,1mm,Extended,invented for a test
";

const COIN: &str = r#"
import std::parts::passive::Resistor
import std::sources::Cell

board Coin {
    .bat: Cell
    .bat.terminals.voltage &= 3V..4.2V
    .bat.terminals.pos.net &= "VBAT"
    .load: Resistor { .resistance &= 10kohm +/- 1% }
    .bat.terminals.pos ~> .load ~> .bat.terminals.neg
}
"#;

const CELL: &str = "mpn,chemistry,capacity,internal_resistance,charge_temperature,discharge_temperature,float_voltage,max_charge_current,package,library,source
ZZC-40,LiIon,144C,1ohm,0degC..45degC,-20degC..60degC,4.2V +/- 50mV,40mA,Coin_D20.0mm_Tabs_P15.00mm,Extended,invented for a test
";

/// `netsmith board build` of `source` with the catalog file `catalog` (`<name>.csv`), with no
/// KiCad install to be found; what it printed, and whether it succeeded.
fn build(root: &Path, source: &str, catalog: (&str, &str)) -> (bool, String) {
    for dir in ["src", "catalogs"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"lands\"\nnetsmith = \"0.5.0\"\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), source).unwrap();
    fs::write(
        root.join(format!("catalogs/{}.csv", catalog.0)),
        catalog.1,
    )
    .unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["board", "build", "--out", "out"])
        .current_dir(root)
        .env_remove("KICAD_CLI")
        .env("NETSMITH_NO_KICAD", "1")
        .env("NETSMITH_KICAD_SYMBOLS", root.join("no-symbols"))
        .env("NETSMITH_KICAD_FOOTPRINTS", root.join("no-footprints"))
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    (run.status.success(), text)
}

/// Every file under `dir`, read as text where it is text, joined.
fn texts(dir: &Path) -> String {
    let mut out = String::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.push_str(&texts(&path));
        } else if let Ok(text) = fs::read_to_string(&path) {
            out.push_str(&text);
        }
    }
    out
}

#[test]
fn a_tapped_supercapacitor_with_its_mid_joined_builds_with_no_kicad() {
    let root = TempDir::new("own-lands-tap");
    let (ok, text) = build(&root, TAPPED, ("supercap", SUPERCAP));
    assert!(ok, "{text}");
    let out = root.join("out/Balanced");
    assert!(out.is_dir(), "{text}");
    let written = texts(&out);
    assert!(written.contains("C_Polarized_Tap"), "no tapped symbol in {out:?}");
    // The land is written into netsmith's own library beside the board.
    assert!(
        out.join("netsmith.pretty/CP_Radial_D16.0mm_P7.50mm_Tap.kicad_mod")
            .is_file(),
        "{text}"
    );
}

#[test]
fn a_coin_cell_with_tabs_builds_with_no_kicad() {
    let root = TempDir::new("own-lands-coin");
    let (ok, text) = build(&root, COIN, ("cell", CELL));
    assert!(ok, "{text}");
    let out = root.join("out/Coin");
    assert!(out.is_dir(), "{text}");
    assert!(texts(&out).contains("Battery_Cell"), "no cell symbol in {out:?}");
}
