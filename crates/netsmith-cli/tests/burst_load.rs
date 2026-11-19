//! `netsmith board build` on a project with a load that bursts (issue #1279): a radio behind a
//! 0.6 A buck that bursts 500 mA for 200 us while an MCU draws up to 150 mA. The bank across the
//! output holds the 50 mA the converter cannot give, so the board builds and the bill of
//! materials has the bank; without the bank the build fails and says how much capacitance holding
//! the burst takes. The board is the guide's (chapter 19, _A load that bursts_).

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

const SOURCE: &str = r#"
import std::parts::passive::CapacitorBank
import std::parts::regulator::{Buck, Lmr16006y}
import std::sources::Rail

board Radio {
    .input: Rail { .voltage &= 9V..16V; .peak &= 24V; .pos.net &= "VIN" }
    .reg: Buck<Lmr16006y> { .vout.pos.net &= "V3V3"; .vout.voltage &= 3.3V +/- 5% }
    .input ~ .reg.vin
    .mcu: Rail { .current &= 5mA..150mA }
    .reg.vout ~ .mcu
    .radio: Rail {
        .current &= 10mA..40mA
        .peak_current &= 500mA
        .burst &= 200us
        .duty &= 0.05
        .voltage >= 3V
    }
    .reg.vout ~ .radio
    .bulk: CapacitorBank
    .bulk.total_effective_capacitance >= 80uF
    .reg.vout.pos ~> .bulk ~> .reg.vout.neg
}
"#;

/// `netsmith board build` on a project of one source, and what it printed.
fn build(root: &Path, source: &str) -> std::process::Output {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"radio\"\nnetsmith = \"0.4.0\"\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), source).unwrap();
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
fn a_burst_held_by_the_bulk_capacitance_builds() {
    let root = TempDir::new("burst-load");
    let run = build(&root, SOURCE);
    let text = printed(&run);
    assert!(run.status.success(), "{text}");
    assert!(!text.contains("during a burst"), "{text}");
    // The bank and the converter's own capacitors are on the bill of materials.
    let lock = fs::read_to_string(root.join("src/main.lock.toml")).unwrap();
    assert!(lock.contains("bulk"), "{lock}");
}

#[test]
fn a_burst_with_no_bulk_capacitance_fails_and_says_what_holding_it_takes() {
    let root = TempDir::new("burst-load-bare");
    let source = SOURCE
        .replace("import std::parts::passive::CapacitorBank\n", "")
        .replace("    .bulk: CapacitorBank\n", "")
        .replace("    .bulk.total_effective_capacitance >= 80uF\n", "")
        .replace("    .reg.vout.pos ~> .bulk ~> .reg.vout.neg\n", "");
    assert!(!source.contains("bulk"), "{source}");
    let run = build(&root, &source);
    let text = printed(&run);
    assert_eq!(run.status.code(), Some(1), "{text}");
    assert!(
        text.contains("during a burst of `radio`") && text.contains("the source gives up to 650mA"),
        "{text}"
    );
    assert!(text.contains("holding the burst takes 74.07uF"), "{text}");
}

/// The same radio behind the buck on a port that gives at most 500 mA (issue #1340): the buck
/// draws the burst, scaled, from its input, and the capacitance across the input holds what the
/// port cannot give. The board is the guide's (chapter 19, _A load that bursts_).
const PORT: &str = r#"
import std::parts::passive::CapacitorBank
import std::parts::regulator::{Buck, Lmr16006y}
import std::sources::Rail

board Radio {
    .usb: Rail { .voltage &= 5V +/- 5%; .peak &= 5.5V; .pos.net &= "VBUS"; .max_peak_current &= 500mA }
    .reg: Buck<Lmr16006y> { .vout.pos.net &= "V3V3"; .vout.voltage &= 3.3V +/- 5% }
    .usb ~ .reg.vin
    .mcu: Rail { .current &= 5mA..150mA }
    .reg.vout ~ .mcu
    .radio: Rail { .current &= 10mA..40mA; .peak_current &= 500mA; .burst &= 200us; .voltage >= 3V }
    .reg.vout ~ .radio
    .bulk: CapacitorBank
    .bulk.total_effective_capacitance >= 80uF
    .reg.vout.pos ~> .bulk ~> .reg.vout.neg
    .input_bulk: CapacitorBank
    .input_bulk.total_effective_capacitance >= 40uF
    .usb.pos ~> .input_bulk ~> .usb.neg
}
"#;

#[test]
fn a_burst_held_by_the_input_capacitance_builds() {
    let root = TempDir::new("burst-load-port");
    let run = build(&root, PORT);
    let text = printed(&run);
    assert!(run.status.success(), "{text}");
    assert!(!text.contains("during a burst"), "{text}");
}

#[test]
fn a_burst_the_port_cannot_give_fails_and_says_what_the_input_needs() {
    let root = TempDir::new("burst-load-port-bare");
    let source = PORT
        .replace("    .input_bulk: CapacitorBank\n", "")
        .replace("    .input_bulk.total_effective_capacitance >= 40uF\n", "")
        .replace("    .usb.pos ~> .input_bulk ~> .usb.neg\n", "");
    assert!(!source.contains("input_bulk"), "{source}");
    let run = build(&root, &source);
    let text = printed(&run);
    assert_eq!(run.status.code(), Some(1), "{text}");
    assert!(
        text.contains("during a burst of `radio` (for 200us) `reg.ic`, which feeds it"),
        "{text}"
    );
    assert!(text.contains("of effective capacitance across the input"), "{text}");
}
