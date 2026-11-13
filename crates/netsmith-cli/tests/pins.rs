//! The pin file (ADR 0101), end to end: `build` and `fab` write it when a run picks a pin, a
//! second run leaves its bytes alone, `--locked` refuses a change, and a locked pin survives a
//! change of the design.

use std::fs;

use netsmith_testkit::TempDir;

const PART: &str = r#"
/// Zmcu4, an invented microcontroller.
part Zmcu4 {
    peripheral UARTE0 on GPIO { UARTE0_TX:output UARTE0_RX:input }
    pins {
        .p00 bidirectional "P0.00" "3" fn GPIO
        .p01 bidirectional "P0.01" "4" fn GPIO
        .p02 bidirectional "P0.02" "5" fn GPIO ANALOG
        .p03 bidirectional "P0.03" "6" fn GPIO ANALOG
    }
}
"#;

fn source(requests: &str) -> String {
    format!(
        "import std::parts::passive::Resistor\n{PART}\nboard Sensors {{\n    .u: Zmcu4\n    .load: Resistor\n{requests}\n}}\n"
    )
}

#[test]
fn the_binary_with_locked_stops_on_a_pin_file_that_would_change_before_it_writes_anything() {
    let dir = TempDir::new("pins-binary");
    let text = source("    .status: .u.GPIO\n    .status ~ .load.a\n");
    fs::write(dir.path().join("main.nsm"), &text).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .current_dir(dir.path())
        .args(["build", "main.nsm", "--locked"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "the pin file `main.pins.toml` is out of date, and --locked does not change it"
        ),
        "{err}"
    );
    // Nothing was written: no pin file, no pick lock, no output folder.
    let mut names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["main.nsm"]);
}
