//! `netsmith port tables` (#560) as a command: it writes `netlist.xml`, `placements.toml` and a
//! new `source.toml` into the reference folder, holds the original against an existing
//! `source.toml`, and with `--check` writes nothing and fails on a stale table. That the derived
//! netlist is a port's own is tested in the library (`crates/netsmith/tests/power_manifold.rs`).

use std::fs;
use std::process::{Command, Output};

use netsmith::port_tables::{Source, sha256_hex};
use netsmith_kicad::testlib::TempDir;
use netsmith_netlist::Netlist;

const BOARD: &str = r#"(kicad_pcb (version 20240108)
  (footprint "Resistor_SMD:R_0603_1608Metric" (layer "F.Cu") (at 10 10)
    (property "Reference" "R1") (property "Value" "4k7")
    (pad "1" smd rect (at -0.8 0) (size 0.8 0.9) (layers "F.Cu") (net 1 "VCC"))
    (pad "2" smd rect (at 0.8 0) (size 0.8 0.9) (layers "F.Cu") (net 2 "GND")))
  (footprint "Resistor_SMD:R_0603_1608Metric" (layer "F.Cu") (at 20 10)
    (property "Reference" "R2") (property "Value" "1k")
    (pad "1" smd rect (at -0.8 0) (size 0.8 0.9) (layers "F.Cu") (net 1 "VCC"))
    (pad "2" smd rect (at 0.8 0) (size 0.8 0.9) (layers "F.Cu") (net 2 "GND")))
  (gr_line (start 0 0) (end 30 0) (layer "Edge.Cuts"))
  (gr_line (start 30 0) (end 30 20) (layer "Edge.Cuts"))
  (gr_line (start 30 20) (end 0 20) (layer "Edge.Cuts"))
  (gr_line (start 0 20) (end 0 0) (layer "Edge.Cuts"))
)"#;

const URL: &str = "https://raw.githubusercontent.com/owner/repo/0123456789abcdef0123456789abcdef01234567/b.kicad_pcb";

fn run(dir: &TempDir, args: &[&str]) -> (Option<i32>, String) {
    let out: Output = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["port", "tables"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code(), text)
}

#[test]
fn the_command_writes_the_tables_and_the_source_and_holds_the_original_to_its_hash() {
    let dir = TempDir::new("port-tables-cli");
    fs::write(dir.join("b.kicad_pcb"), BOARD).unwrap();

    // A new source needs a URL, at a pinned commit.
    let (code, text) = run(&dir, &["b.kicad_pcb"]);
    assert_eq!(code, Some(2), "{text}");
    assert!(text.contains("--url"), "{text}");
    let branch = "https://raw.githubusercontent.com/owner/repo/main/b.kicad_pcb";
    let (code, text) = run(&dir, &["b.kicad_pcb", "--url", branch]);
    assert_eq!(code, Some(2), "{text}");
    assert!(text.contains("pinned commit"), "{text}");
    assert!(!dir.join("reference").exists(), "nothing is written");

    let (code, text) = run(&dir, &["b.kicad_pcb", "--url", URL]);
    assert_eq!(code, Some(0), "{text}");
    assert!(
        text.contains("2 components and 2 nets of b.kicad_pcb"),
        "{text}"
    );
    assert!(text.contains("reference/source.toml"), "{text}");
    let read = |f: &str| fs::read_to_string(dir.join("reference").join(f)).unwrap();
    let source = Source::parse(&read("source.toml"), "source.toml").unwrap();
    assert_eq!(
        source,
        Source {
            file: "b.kicad_pcb".into(),
            url: URL.into(),
            size: BOARD.len() as u64,
            sha256: sha256_hex(BOARD.as_bytes()),
        }
    );
    assert_eq!(
        Netlist::from_kicad_xml(&read("netlist.xml")).unwrap(),
        Netlist::from_kicad_pcb(BOARD).unwrap()
    );
    assert!(read("placements.toml").contains("file = \"b.kicad_pcb\""));

    // The committed tables check; a stale one does not.
    let (code, text) = run(&dir, &["b.kicad_pcb", "--check"]);
    assert_eq!(code, Some(0), "{text}");
    assert!(text.contains("are current"), "{text}");
    fs::write(dir.join("reference/netlist.xml"), "<export/>").unwrap();
    let (code, text) = run(&dir, &["b.kicad_pcb", "--check"]);
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("netlist.xml is not what"), "{text}");
    assert_eq!(read("netlist.xml"), "<export/>", "--check writes nothing");

    // Run again with the source there: no URL is needed and the tables are written again.
    let (code, text) = run(&dir, &["b.kicad_pcb"]);
    assert_eq!(code, Some(0), "{text}");
    assert!(!text.contains("source.toml"), "{text}");
    assert_eq!(run(&dir, &["b.kicad_pcb", "--check"]).0, Some(0));

    // Another URL, or another file, is not the original that source.toml names.
    let other = URL.replace("0123", "4567");
    let (code, text) = run(&dir, &["b.kicad_pcb", "--url", &other]);
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("is not the URL of"), "{text}");
    fs::write(dir.join("b.kicad_pcb"), BOARD.replace("4k7", "4k8")).unwrap();
    let (code, text) = run(&dir, &["b.kicad_pcb"]);
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("is not the original that"), "{text}");
    assert!(text.contains(&source.sha256), "{text}");
    assert_eq!(read("source.toml"), source.to_toml(), "source.toml is kept");
}
