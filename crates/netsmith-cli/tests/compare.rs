//! `netsmith compare` on the example boards. The tests that read a schematic run real
//! KiCad (`kicad-cli sch export netlist`), so they skip when it is not installed, or is Windows KiCad that
//! this shell cannot start (no `cmd.exe`, as under `systemd-run --user`); the
//! rest need no KiCad.

use netsmith_testkit::TempDir;
use std::fs;
use std::path::Path;
use std::process::Command;

fn scratch(name: &str) -> TempDir {
    TempDir::new(&format!("compare-test-{name}"))
}

#[test]
fn the_command_exits_0_for_a_match_1_for_a_difference_and_2_for_a_mistake() {
    let dir = scratch("exit");
    let xml = |value: &str| {
        format!(
            r#"<export version="E"><components>
              <comp ref="R1"><value>{value}</value><footprint>Resistor_SMD:R_0402_1005Metric</footprint></comp>
              <comp ref="R2"><value>1k</value><footprint>Resistor_SMD:R_0402_1005Metric</footprint></comp>
            </components><nets>
              <net code="1" name="A"><node ref="R1" pin="1"/><node ref="R2" pin="1"/></net>
              <net code="2" name="unconnected-(R1-Pad2)"><node ref="R1" pin="2"/></net>
            </nets></export>"#
        )
    };
    let (a, b, c) = (dir.join("a.xml"), dir.join("b.xml"), dir.join("c.xml"));
    fs::write(&a, xml("4k7")).unwrap();
    fs::write(&b, xml("4k7")).unwrap();
    fs::write(&c, xml("4.7k")).unwrap();
    let run = |args: &[&Path]| {
        let out = Command::new(env!("CARGO_BIN_EXE_netsmith"))
            .arg("compare")
            .args(args)
            .output()
            .unwrap();
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    let (code, out, _) = run(&[&a, &b]);
    assert_eq!(code, Some(0), "{out}");
    assert!(
        out.contains("match: 2 components and 1 nets correspond"),
        "{out}"
    );

    let (code, out, _) = run(&[&a, &c]);
    assert_eq!(code, Some(1), "{out}");
    assert!(out.contains("different: 1 difference"), "{out}");
    let (code, _, _) = run(&[&a, &c, Path::new("--loose-values")]);
    assert_eq!(code, Some(0));

    let (code, _, err) = run(&[&a]);
    assert_eq!(code, Some(2));
    assert!(
        err.contains("required arguments were not provided") && err.contains("<B>"),
        "{err}"
    );
    let (code, _, err) = run(&[&a, &dir.join("missing.xml")]);
    assert_eq!(code, Some(2));
    assert!(err.contains("missing.xml"), "{err}");

    // --map names components that are not there.
    let map = dir.join("map.txt");
    fs::write(&map, "R9 R1\n").unwrap();
    let (code, _, err) = run(&[&a, &b, Path::new("--map"), &map]);
    assert_eq!(code, Some(2));
    assert!(err.contains("R9"), "{err}");
}

/// A resistor (`R_` footprint) whose two pads are the other way round in B matches, and the
/// swap is a note in the text and a `swapped` pair in the JSON; an LED turned the same way is
/// a difference (#67).
#[test]
fn a_resistor_turned_round_matches_with_a_note_and_an_led_does_not() {
    let dir = scratch("swapped");
    let xml = |r: (&str, &str), d: (&str, &str)| {
        format!(
            r#"<export version="E"><components>
              <comp ref="J1"><value>Conn</value><footprint>Connector:PinHeader_1x02</footprint></comp>
              <comp ref="R1"><value>330</value><footprint>Resistor_SMD:R_0402_1005Metric</footprint></comp>
              <comp ref="D1"><value>LED</value><footprint>LED_SMD:LED_0603_1608Metric</footprint></comp>
            </components><nets>
              <net code="1" name="VCC"><node ref="J1" pin="1"/><node ref="R1" pin="{}"/></net>
              <net code="2" name="X"><node ref="R1" pin="{}"/><node ref="D1" pin="{}"/></net>
              <net code="3" name="GND"><node ref="D1" pin="{}"/><node ref="J1" pin="2"/></net>
            </nets></export>"#,
            r.0, r.1, d.0, d.1
        )
    };
    let (a, b, c) = (dir.join("a.xml"), dir.join("b.xml"), dir.join("c.xml"));
    fs::write(&a, xml(("1", "2"), ("1", "2"))).unwrap();
    fs::write(&b, xml(("2", "1"), ("1", "2"))).unwrap();
    fs::write(&c, xml(("1", "2"), ("2", "1"))).unwrap();
    let run = |args: &[&Path]| {
        let out = Command::new(env!("CARGO_BIN_EXE_netsmith"))
            .arg("compare")
            .args(args)
            .output()
            .unwrap();
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        )
    };
    let (code, out) = run(&[&a, &b]);
    assert_eq!(code, Some(0), "{out}");
    assert!(
        out.contains("pads swapped") && out.contains("R1 -> R1"),
        "{out}"
    );
    let (code, out) = run(&[&a, &b, Path::new("--format"), Path::new("json")]);
    assert_eq!(code, Some(0), "{out}");
    let doc: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(doc["verdict"], "match");
    assert_eq!(doc["swapped"], serde_json::json!([["R1", "R1"]]));
    let (code, out) = run(&[&a, &c]);
    assert_eq!(code, Some(1), "{out}");
    assert!(out.contains("D1.1") && out.contains("D1.2"), "{out}");
}
