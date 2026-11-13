//! `netsmith dxf import` (ADR 0045, C0 piece 3): an enclosure's DXF becomes an outline, a
//! cutout and mounting holes that the fab pipeline accepts. The DXF is written here by hand.

use netsmith_testkit::TempDir;
use std::fs;

fn pair(code: i32, value: impl std::fmt::Display) -> String {
    format!("{code}\n{value}\n")
}

fn line(layer: &str, a: (f64, f64), b: (f64, f64)) -> String {
    pair(0, "LINE")
        + &pair(8, layer)
        + &pair(10, a.0)
        + &pair(20, a.1)
        + &pair(11, b.0)
        + &pair(21, b.1)
}

fn arc(layer: &str, c: (f64, f64), r: f64, start: f64, end: f64) -> String {
    pair(0, "ARC")
        + &pair(8, layer)
        + &pair(10, c.0)
        + &pair(20, c.1)
        + &pair(40, r)
        + &pair(50, start)
        + &pair(51, end)
}

fn circle(layer: &str, c: (f64, f64), r: f64) -> String {
    pair(0, "CIRCLE") + &pair(8, layer) + &pair(10, c.0) + &pair(20, c.1) + &pair(40, r)
}

fn rounded(layer: &str, x0: f64, y0: f64, x1: f64, y1: f64, r: f64) -> String {
    line(layer, (x0 + r, y0), (x1 - r, y0))
        + &arc(layer, (x1 - r, y0 + r), r, 270.0, 360.0)
        + &line(layer, (x1, y0 + r), (x1, y1 - r))
        + &arc(layer, (x1 - r, y1 - r), r, 0.0, 90.0)
        + &line(layer, (x1 - r, y1), (x0 + r, y1))
        + &arc(layer, (x0 + r, y1 - r), r, 90.0, 180.0)
        + &line(layer, (x0, y1 - r), (x0, y0 + r))
        + &arc(layer, (x0 + r, y0 + r), r, 180.0, 270.0)
}

fn file(insunits: Option<i32>, entities: &str) -> String {
    let mut s = pair(0, "SECTION") + &pair(2, "HEADER");
    if let Some(u) = insunits {
        s += &(pair(9, "$INSUNITS") + &pair(70, u));
    }
    s + &pair(0, "ENDSEC")
        + &pair(0, "SECTION")
        + &pair(2, "ENTITIES")
        + entities
        + &pair(0, "ENDSEC")
        + &pair(0, "EOF")
}

/// An enclosure section 50 x 36 with rounded corners, a rounded slot and four holes, placed
/// away from the drawing's origin and with y up.
fn enclosure() -> String {
    let mut e = rounded("EDGE", 100.0, 200.0, 150.0, 236.0, 3.0);
    e += &rounded("EDGE", 106.0, 212.0, 109.0, 224.0, 1.0);
    for (x, y) in [
        (105.0, 230.5),
        (145.0, 230.5),
        (105.0, 205.5),
        (145.0, 205.5),
    ] {
        e += &circle("HOLES", (x, y), 1.6);
    }
    file(Some(4), &e)
}

fn temp(name: &str) -> TempDir {
    TempDir::new(&format!("dxf-test-{name}"))
}

fn netsmith(args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn the_command_prints_the_source_to_paste_and_writes_only_the_physical_file() {
    let dir = temp("cli");
    let dxf = dir.join("case.dxf");
    fs::write(&dxf, enclosure()).unwrap();
    let nsm = dir.join("case.nsm");
    fs::write(&nsm, "board Case {\n}\n").unwrap();
    let args = |extra: &[&str]| {
        let mut v = vec![
            "dxf",
            "import",
            dxf.to_str().unwrap(),
            "--layer",
            "EDGE",
            "--holes-layer",
            "HOLES",
        ];
        v.extend_from_slice(extra);
        netsmith(&v)
    };
    let printed = args(&[]);
    assert!(printed.status.success(), "{printed:?}");
    let text = String::from_utf8(printed.stdout).unwrap();
    assert!(
        text.contains(".outline: Outline {") && text.contains("physical for YourBoard {"),
        "{text}"
    );
    assert!(
        !dir.join("case.physical.nsm").exists(),
        "nothing is written without --write"
    );

    let written = args(&["--write", nsm.to_str().unwrap()]);
    assert!(written.status.success(), "{written:?}");
    let text = String::from_utf8(written.stdout).unwrap();
    assert!(text.contains("+        .hole1.place.x &= 5mm"), "{text}");
    let physical = fs::read_to_string(dir.join("case.physical.nsm")).unwrap();
    assert!(physical.contains(".cutout1: Cutout {"), "{physical}");
    assert!(!dir.join("case.pcb.toml").exists());
    assert_eq!(fs::read_to_string(&nsm).unwrap(), "board Case {\n}\n");

    // Without --layer the reader asks which layer: it lists them.
    let asked = netsmith(&["dxf", "import", dxf.to_str().unwrap()]);
    assert!(!asked.status.success());
    let err = String::from_utf8(asked.stderr).unwrap();
    assert!(
        err.contains("choose with --layer") && err.contains("`HOLES` (4 CIRCLE)"),
        "{err}"
    );

    // A binary file is refused with a message and exit code 1.
    fs::write(&dxf, b"AutoCAD Binary DXF\r\n\x1a\0rest").unwrap();
    let binary = netsmith(&["dxf", "import", dxf.to_str().unwrap()]);
    assert_eq!(binary.status.code(), Some(1));
    assert!(
        String::from_utf8(binary.stderr)
            .unwrap()
            .contains("binary DXF")
    );
}
