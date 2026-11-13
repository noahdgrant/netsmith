//! `netsmith port skeleton` (#452) as a command: it writes the source and the parts file, prints
//! the counts, the lines marked `TODO tolerance` and the gaps, and overwrites neither file. What
//! the skeleton holds is tested in the library (`crates/netsmith-tools/tests/port_skeleton.rs`).

use std::fs;
use std::process::Command;

use netsmith_tools::port::PortParts;
use netsmith_testkit::TempDir;

/// A board of a resistor and a part with a file of its own, and a part with none.
const SMALL: &str = r#"(kicad_pcb (version 20240108)
  (footprint "Resistor_SMD:R_0603_1608Metric"
    (property "Reference" "R1") (property "Value" "4k7")
    (pad "1" smd rect (net 1 "VCC")) (pad "2" smd rect (net 2 "Net-(R1-Pad2)")))
  (footprint "Package_TO_SOT_SMD:SOT-23"
    (property "Reference" "U1") (property "Value" "ABC1")
    (pad "1" smd rect (net 2 "Net-(R1-Pad2)")) (pad "2" smd rect (net 3 "GND")) (pad "3" smd rect (net 1 "VCC")))
  (footprint "Package_TO_SOT_SMD:SOT-23"
    (property "Reference" "U2") (property "Value" "XYZ-9")
    (pad "1" smd rect (net 3 "GND") (pinfunction "~{EN}"))
    (pad "2" smd rect (net 1 "VCC") (pinfunction "IN+"))
    (pad "3" smd rect (net 1 "VCC") (pinfunction "IN_POS"))
    (pad "4" smd rect (net 1 "VCC") (pinfunction "IN+"))
    (pad "5" smd rect (net 3 "GND") (pinfunction "IN+"))
    (pad "6" smd rect (net 3 "GND")))
)"#;

#[test]
fn the_command_writes_the_source_and_the_parts_file_and_overwrites_neither() {
    let dir = TempDir::new("port-skeleton-cli");
    fs::write(dir.join("small.kicad_pcb"), SMALL).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_netsmith"))
            .args([
                "port",
                "skeleton",
                "small.kicad_pcb",
                "--out",
                "src/main.nsm",
            ])
            .current_dir(&dir)
            .output()
            .unwrap()
    };
    let out = run();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("3 members: 1 standard parts, 2 gaps; 3 joins, 2 net names"),
        "{stdout}"
    );
    assert!(stdout.contains("U1, U2"), "{stdout}");
    assert!(stdout.contains("  src/main.nsm:22: R1\n"), "{stdout}");
    let source = fs::read_to_string(dir.join("src/main.nsm")).unwrap();
    assert!(source.contains("board Small {"), "{source}");
    let parts = PortParts::read(&dir.join("reference/parts.toml")).unwrap();
    assert_eq!(parts.reference_of("u2"), Some("U2"));

    let again = run();
    assert_eq!(again.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&again.stderr).contains("is there already"),
        "{}",
        String::from_utf8_lossy(&again.stderr)
    );
}
