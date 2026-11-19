//! `netsmith import footprint` (issue #1424): a land pattern from a vendor's KiCad board or
//! footprint file or an Eagle library or board (issue #1430), written into a library of the
//! project, and a board built with it on a machine with no KiCad.

use std::fs;
use std::path::Path;
use std::process::Output;

use netsmith_testkit::TempDir;

/// The part and the board of the guide's _A part of your own, with no KiCad installed_.
const SOURCE: &str = r#"import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

part Sensor {
    pins {
        .vdd passive "VDD" "1"
        .out passive "OUT" "2"
        .gnd passive "GND" "3"
    }
}

board Probe {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .s: Sensor {
        .assembly &= AssemblyMode::Hand
    }
    .j: Header1x2
    .s.vdd ~ .j.p1
    .s.gnd ~ .j.p2
}
"#;

/// A vendor's evaluation board with the sensor `U3` on the bottom face, turned by 180 degrees, as
/// KiCad stores it there (every y of the footprint's frame negated, the layers on the other face,
/// the angles the board's), and a resistor `R1` on the top.
const VENDOR_BOARD: &str = r#"(kicad_pcb (version 20241229) (generator "pcbnew")
  (footprint "Vendor:R_0402" (layer "F.Cu") (uuid "b0") (at 5 5)
    (property "Reference" "R1" (at 0 -1 0) (layer "F.SilkS"))
    (pad "1" smd rect (at -0.5 0) (size 0.5 0.5) (layers "F.Cu" "F.Mask"))
    (pad "2" smd rect (at 0.5 0) (size 0.5 0.5) (layers "F.Cu" "F.Mask"))
  )
  (footprint "Vendor:SOT-23-3_Sensor" (layer "B.Cu") (uuid "b1") (at 20 30 180)
    (property "Reference" "U3" (at 0 2.5 180) (layer "B.SilkS") (effects (font (size 1 1)) (justify mirror)))
    (property "Value" "SENSOR-1" (at 0 -2.5 180) (layer "B.Fab") (effects (font (size 1 1)) (justify mirror)))
    (property "MPN" "SENSOR-1-TR" (at 0 0 180) (layer "B.Fab") (hide yes))
    (path "/0001") (sheetname "/") (sheetfile "eval.kicad_sch")
    (attr smd)
    (fp_rect (start -2.5 1.6) (end 2.5 -1.6) (stroke (width 0.05) (type solid)) (fill none) (layer "B.CrtYd"))
    (fp_rect (start -0.65 1.45) (end 0.65 -1.45) (stroke (width 0.1) (type solid)) (fill none) (layer "B.Fab"))
    (pad "1" smd rect (at -1.5 0.95 180) (size 1.2 0.7) (layers "B.Cu" "B.Mask" "B.Paste") (net 1 "VDD") (uuid "b2"))
    (pad "2" smd rect (at -1.5 -0.95 180) (size 1.2 0.7) (layers "B.Cu" "B.Mask" "B.Paste") (net 2 "OUT") (uuid "b3"))
    (pad "3" smd rect (at 1.5 0 180) (size 1.2 0.7) (layers "B.Cu" "B.Mask" "B.Paste") (net 3 "GND") (uuid "b4"))
  )
)
"#;

fn netsmith(dir: &Path, args: &[&str]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("NETSMITH_KICAD_SYMBOLS")
        .env_remove("NETSMITH_KICAD_FOOTPRINTS")
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A project with the probe board, a vendor board beside it, and the given `kicad.toml` (none
/// when empty).
fn project(dir: &Path, kicad_toml: &str) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("vendor")).unwrap();
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"probe\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(dir.join("src/main.nsm"), SOURCE).unwrap();
    fs::write(dir.join("vendor/eval.kicad_pcb"), VENDOR_BOARD).unwrap();
    if !kicad_toml.is_empty() {
        fs::write(dir.join("kicad.toml"), kicad_toml).unwrap();
    }
}

#[test]
fn a_vendor_boards_part_becomes_a_footprint_a_board_builds_with() {
    let dir = TempDir::new("import-footprint-board");
    let dir = dir.path();
    project(dir, "");
    let run = netsmith(
        dir,
        &[
            "import",
            "footprint",
            "vendor/eval.kicad_pcb",
            "--ref",
            "U3",
            "--name",
            "own:Sensor",
        ],
    );
    let out = text(&run.stdout);
    assert!(run.status.success(), "{}{out}", text(&run.stderr));
    assert!(
        out.starts_with(
            "wrote libs/own.pretty/Sensor.kicad_mod (3 pads, turned from the bottom face to the top)\n"
        ),
        "{out}"
    );
    assert!(
        out.contains("the file names it `SOT-23-3_Sensor`; it is `Sensor` here"),
        "{out}"
    );
    let stanza = "[[library]]\nname = \"own\"\nfootprints = \"libs/own.pretty\"\n";
    assert!(out.contains(stanza), "{out}");
    let row = "footprint = \"own:Sensor\"\ntable = \"pads=3, pitch=TODO, body=TODO\"\n";
    assert!(out.contains(row), "{out}");

    // The file is the footprint as the guide draws it, seen from the top.
    let written = fs::read_to_string(dir.join("libs/own.pretty/Sensor.kicad_mod")).unwrap();
    for line in [
        "(footprint \"Sensor\"",
        "(layer \"F.Cu\")",
        "(pad \"1\" smd rect\n\t\t(at -1.5 -0.95)",
        "(pad \"3\" smd rect\n\t\t(at 1.5 0)",
        "(layer \"F.CrtYd\")",
        "\"REF**\"",
    ] {
        assert!(written.contains(line), "{line}:\n{written}");
    }
    for gone in ["B.Cu", "(net", "uuid", "MPN", "eval.kicad_sch", "mirror"] {
        assert!(!written.contains(gone), "{gone}:\n{written}");
    }

    // The printed rows, with the datasheet's numbers in place of the TODOs, build the board, the
    // footprint measured against the table.
    let start = out.find("[[library]]").unwrap();
    let end = out.find("\nfill `table`").unwrap();
    let rows = out[start..end]
        .replace("kind = \"TODO\"", "kind = \"Sensor\"")
        .replace("package = \"TODO\"", "package = \"SOT-23\"")
        .replace(
            "pitch=TODO, body=TODO",
            "pitch=1.9mm, body=1.3x2.9mm",
        );
    fs::write(dir.join("kicad.toml"), &rows).unwrap();
    let build = netsmith(dir, &["board", "build", "--out", "out"]);
    assert!(build.status.success(), "{rows}\n{}", text(&build.stderr));

    // Once there, the file is not written over.
    let again = netsmith(
        dir,
        &[
            "import",
            "footprint",
            "vendor/eval.kicad_pcb",
            "--ref",
            "U3",
            "--name",
            "Sensor",
        ],
    );
    assert_eq!(again.status.code(), Some(2));
    assert_eq!(
        text(&again.stderr),
        "error: libs/own.pretty/Sensor.kicad_mod is there already: delete it to import the footprint again, or give another name with `--name`\n"
    );
}

#[test]
fn a_footprint_file_goes_into_the_one_library_kicad_toml_lists() {
    let dir = TempDir::new("import-footprint-file");
    let dir = dir.path();
    project(
        dir,
        "[[library]]\nname = \"parts\"\nfootprints = \"footprints/parts.pretty\"\n",
    );
    // A distributor's export: a `.pretty` folder with the one footprint.
    let board = netsmith(
        dir,
        &[
            "import",
            "footprint",
            "vendor/eval.kicad_pcb",
            "--ref",
            "U3",
            "--name",
            "tmp:Sensor",
        ],
    );
    assert!(board.status.success(), "{}", text(&board.stderr));
    fs::rename(dir.join("libs/tmp.pretty"), dir.join("vendor/dist.pretty")).unwrap();

    let run = netsmith(
        dir,
        &[
            "import",
            "footprint",
            "vendor/dist.pretty",
            "--name",
            "Probe-Sensor",
        ],
    );
    let out = text(&run.stdout);
    assert!(run.status.success(), "{}", text(&run.stderr));
    assert!(
        out.starts_with("wrote footprints/parts.pretty/Probe-Sensor.kicad_mod (3 pads)\n"),
        "{out}"
    );
    assert!(!out.contains("[[library]]"), "listed already:\n{out}");
    assert!(out.contains("footprint = \"parts:Probe-Sensor\""), "{out}");
    let written =
        fs::read_to_string(dir.join("footprints/parts.pretty/Probe-Sensor.kicad_mod")).unwrap();
    assert!(written.contains("(at -1.5 -0.95)"), "{written}");
}

#[test]
fn what_cannot_be_imported_says_why() {
    let dir = TempDir::new("import-footprint-errors");
    let dir = dir.path();
    project(dir, "");
    let cases: [(&[&str], &str); 5] = [
        (
            &["vendor/eval.kicad_pcb"],
            "error: vendor/eval.kicad_pcb: a board holds many footprints: name the part with `--ref REF`\n",
        ),
        (
            &["vendor/eval.kicad_pcb", "--ref", "U9", "--name", "own:X"],
            "error: vendor/eval.kicad_pcb: the board has no part `U9`; its parts are R1, U3\n",
        ),
        (
            &["vendor/eval.kicad_pcb", "--ref", "U3"],
            "error: kicad.toml lists no footprint library: name one with `--name LIB:NAME`\n",
        ),
        (
            &["vendor/eval.kicad_pcb", "--ref", "U3", "--name", "gen:X"],
            "error: `gen` is a library of netsmith's own (generated or standard footprints): give the project's library another name\n",
        ),
        (
            &["src/main.nsm"],
            "error: src/main.nsm: not a KiCad board (`.kicad_pcb`), footprint (`.kicad_mod`) or `.pretty` folder, nor an Eagle board (`.brd`) or library (`.lbr`), nor an Altium board (`.PcbDoc`) or library (`.PcbLib`), nor a folder or zip of Gerber files\n",
        ),
    ];
    for (args, want) in cases {
        let mut all = vec!["import", "footprint"];
        all.extend(args);
        let run = netsmith(dir, &all);
        assert_eq!(run.status.code(), Some(2), "{args:?}");
        assert_eq!(text(&run.stderr), want, "{args:?}");
    }
    assert!(!dir.join("libs").exists());

    // Outside a project there is no library to write into.
    let outside = TempDir::new("import-footprint-outside");
    fs::write(outside.path().join("eval.kicad_pcb"), VENDOR_BOARD).unwrap();
    let run = netsmith(
        outside.path(),
        &["import", "footprint", "eval.kicad_pcb", "--ref", "U3", "--name", "own:X"],
    );
    assert_eq!(run.status.code(), Some(2));
    assert!(
        text(&run.stderr).contains("no netsmith.toml here or in a folder above"),
        "{}",
        text(&run.stderr)
    );
}

/// An Eagle library of the sensor's one package (issue #1430), written for the test: Eagle's y
/// grows up, the body is on tDocu, the courtyard on a user layer of that name, and a keepout on a
/// layer a KiCad footprint has none for.
const EAGLE_LIBRARY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE eagle SYSTEM "eagle.dtd">
<eagle version="9.6.2">
<drawing>
<layers>
<layer number="1" name="Top" color="4" fill="1" visible="yes" active="yes"/>
<layer number="21" name="tPlace" color="7" fill="1" visible="yes" active="yes"/>
<layer number="39" name="tKeepout" color="4" fill="11" visible="yes" active="yes"/>
<layer number="51" name="tDocu" color="7" fill="1" visible="yes" active="yes"/>
<layer number="200" name="tCourtyard" color="3" fill="1" visible="yes" active="yes"/>
</layers>
<library name="probe-parts">
<packages>
<package name="SOT-3-SENSOR">
<smd name="1" x="-0.95" y="-1.1" dx="0.6" dy="0.9" layer="1"/>
<smd name="2" x="0.95" y="-1.1" dx="0.6" dy="0.9" layer="1"/>
<smd name="3" x="0" y="1.1" dx="0.6" dy="0.9" layer="1"/>
<wire x1="-1.45" y1="-0.65" x2="1.45" y2="-0.65" width="0.1" layer="51"/>
<wire x1="1.45" y1="-0.65" x2="1.45" y2="0.65" width="0.1" layer="51"/>
<wire x1="1.45" y1="0.65" x2="-1.45" y2="0.65" width="0.1" layer="51"/>
<wire x1="-1.45" y1="0.65" x2="-1.45" y2="-0.65" width="0.1" layer="51"/>
<wire x1="-1.6" y1="1.6" x2="-0.5" y2="1.6" width="0.15" layer="21"/>
<rectangle x1="-1.7" y1="-1.9" x2="1.7" y2="1.9" layer="200"/>
<rectangle x1="-2" y1="-0.2" x2="2" y2="0.2" layer="39"/>
<text x="-1.5" y="2" size="0.8" layer="25">&gt;NAME</text>
<text x="-1.5" y="-2.8" size="0.8" layer="27">&gt;VALUE</text>
</package>
</packages>
</library>
</drawing>
</eagle>
"#;

/// A vendor's Eagle board with the package's part `U3` on the bottom face, turned.
fn eagle_board() -> String {
    // The board lists the same layers, its courtyard layer among them.
    let layers = EAGLE_LIBRARY.find("<layers>").unwrap();
    let start = EAGLE_LIBRARY.find("<library").unwrap();
    let end = EAGLE_LIBRARY.find("</library>").unwrap() + "</library>".len();
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<eagle version=\"9.6.2\">\n<drawing>\n{}<board>\n<libraries>\n{}\n</libraries>\n<elements>\n<element name=\"U3\" library=\"probe-parts\" package=\"SOT-3-SENSOR\" value=\"SENSOR-1\" x=\"20\" y=\"30\" rot=\"MR90\"/>\n</elements>\n</board>\n</drawing>\n</eagle>\n",
        &EAGLE_LIBRARY[layers..start],
        &EAGLE_LIBRARY[start..end]
    )
}

#[test]
fn an_eagle_librarys_package_becomes_a_footprint_a_board_builds_with() {
    let dir = TempDir::new("import-footprint-eagle");
    let dir = dir.path();
    project(dir, "");
    fs::write(dir.join("vendor/sensor.lbr"), EAGLE_LIBRARY).unwrap();
    let run = netsmith(
        dir,
        &["import", "footprint", "vendor/sensor.lbr", "--name", "own:Sensor"],
    );
    let out = text(&run.stdout);
    assert!(run.status.success(), "{}{out}", text(&run.stderr));
    assert!(
        out.starts_with(
            "wrote libs/own.pretty/Sensor.kicad_mod (3 pads)\nthe file names it `SOT-3-SENSOR`; it is `Sensor` here\nnote: left out 1 drawing on Eagle's layer tKeepout, which a KiCad footprint has no layer for\n"
        ),
        "{out}"
    );
    let written = fs::read_to_string(dir.join("libs/own.pretty/Sensor.kicad_mod")).unwrap();
    for line in [
        "(footprint \"Sensor\"",
        "(pad \"1\" smd rect\n\t\t(at -0.95 1.1)",
        "(pad \"3\" smd rect\n\t\t(at 0 -1.1)",
        "(layer \"F.CrtYd\")",
        "(layer \"F.Fab\")",
        "(layer \"F.SilkS\")",
        "\"REF**\"",
    ] {
        assert!(written.contains(line), "{line}:\n{written}");
    }

    // The printed rows, with the datasheet's numbers, build the board, the footprint measured
    // against the table.
    let start = out.find("[[library]]").unwrap();
    let end = out.find("\nfill `table`").unwrap();
    let rows = out[start..end]
        .replace("kind = \"TODO\"", "kind = \"Sensor\"")
        .replace("package = \"TODO\"", "package = \"SOT-23\"")
        .replace("pitch=TODO, body=TODO", "pitch=1.9mm, body=2.9x1.3mm");
    fs::write(dir.join("kicad.toml"), &rows).unwrap();
    let build = netsmith(dir, &["board", "build", "--out", "out"]);
    assert!(build.status.success(), "{rows}\n{}", text(&build.stderr));

    // A board's part gives the same package, drawn from the top though the part is on the bottom.
    fs::write(dir.join("vendor/eval.brd"), eagle_board()).unwrap();
    let board = netsmith(
        dir,
        &["import", "footprint", "vendor/eval.brd", "--ref", "U3", "--name", "own:FromBoard"],
    );
    let out = text(&board.stdout);
    assert!(board.status.success(), "{}{out}", text(&board.stderr));
    assert!(
        out.contains("note: the part is on the bottom face of the board; Eagle keeps its package drawn from the top\n"),
        "{out}"
    );
    let from_board = fs::read_to_string(dir.join("libs/own.pretty/FromBoard.kicad_mod")).unwrap();
    assert_eq!(
        from_board.replace("FromBoard", "Sensor"),
        written,
        "the board's copy of the package is the library's"
    );

    let cases: [(&[&str], &str); 2] = [
        (
            &["vendor/eval.brd", "--name", "own:X"],
            "error: vendor/eval.brd: a board holds many footprints: name the part with `--ref REF`\n",
        ),
        (
            &["vendor/sensor.lbr", "--ref", "U3", "--name", "own:X"],
            "error: vendor/sensor.lbr: `--ref` names a part of a board, and a library's footprint is named with `--footprint`\n",
        ),
    ];
    for (args, want) in cases {
        let mut all = vec!["import", "footprint"];
        all.extend(args);
        let run = netsmith(dir, &all);
        assert_eq!(run.status.code(), Some(2), "{args:?}");
        assert_eq!(text(&run.stderr), want, "{args:?}");
    }
}

/// The sensor of the vendor board above as an Altium library holds it (mm, y up): three pads, the
/// body's outline on mechanical 13, the courtyard as a region on mechanical 15, and the
/// designator's text. `place` puts a point of the library on a board; `component` names the part.
fn altium_sensor(
    component: u16,
    place: &dyn Fn((f64, f64)) -> (f64, f64),
    angle: f64,
    bottom: bool,
) -> Vec<Vec<u8>> {
    use netsmith_kicad::altium_fixture::{self as fx, Pad, Shape, layer};
    let mut out = Vec::new();
    for (name, at) in [("1", (-1.5, 0.95)), ("2", (-1.5, -0.95)), ("3", (1.5, 0.0))] {
        let mut pad = Pad::smd(name, place(at), (1.2, 0.7), Shape::Rect);
        pad.component = component;
        pad.angle = angle;
        if bottom {
            pad.layer = layer::BOTTOM;
        }
        out.push(pad.record());
    }
    let body = [(-0.65, -1.45), (0.65, -1.45), (0.65, 1.45), (-0.65, 1.45)];
    for i in 0..4 {
        out.push(fx::track(
            layer::mechanical(13),
            component,
            place(body[i]),
            place(body[(i + 1) % 4]),
            0.1,
        ));
    }
    out.push(fx::region(
        layer::mechanical(15),
        component,
        &[(-2.5, -1.6), (2.5, -1.6), (2.5, 1.6), (-2.5, 1.6)].map(place),
    ));
    out.push(fx::text(
        if bottom { 34 } else { layer::TOP_OVERLAY },
        component,
        ".Designator",
    ));
    out
}

#[test]
fn an_altium_library_footprint_becomes_a_footprint_a_board_builds_with() {
    use netsmith_kicad::altium_fixture::{NO_COMPONENT, pcblib};
    let dir = TempDir::new("import-footprint-altium-lib");
    let dir = dir.path();
    project(dir, "");
    let file = pcblib(&[("SOT-23-3_Sensor", altium_sensor(NO_COMPONENT, &|p| p, 0.0, false))]);
    fs::write(dir.join("vendor/sensor.PcbLib"), file).unwrap();
    let run = netsmith(
        dir,
        &["import", "footprint", "vendor/sensor.PcbLib", "--name", "own:Sensor"],
    );
    let out = text(&run.stdout);
    assert!(run.status.success(), "{}{out}", text(&run.stderr));
    assert!(
        out.starts_with(
            "wrote libs/own.pretty/Sensor.kicad_mod (3 pads)\nthe file names it `SOT-23-3_Sensor`; it is `Sensor` here\nnote: 1 text(s) left out: the reference is `REF**` and the value the footprint's name\n"
        ),
        "{out}"
    );
    let written = fs::read_to_string(dir.join("libs/own.pretty/Sensor.kicad_mod")).unwrap();
    for line in [
        "(pad \"1\" smd rect\n\t\t(at -1.5 -0.95)",
        "(pad \"3\" smd rect\n\t\t(at 1.5 0)",
        "(layer \"F.CrtYd\")",
        "(layer \"F.Fab\")",
    ] {
        assert!(written.contains(line), "{line}:\n{written}");
    }

    // The printed rows, filled from the datasheet, build the board with no KiCad installed.
    let start = out.find("[[library]]").unwrap();
    let end = out.find("\nfill `table`").unwrap();
    let rows = out[start..end]
        .replace("kind = \"TODO\"", "kind = \"Sensor\"")
        .replace("package = \"TODO\"", "package = \"SOT-23\"")
        .replace("pitch=TODO, body=TODO", "pitch=1.9mm, body=1.3x2.9mm");
    fs::write(dir.join("kicad.toml"), &rows).unwrap();
    let build = netsmith(dir, &["board", "build", "--out", "out"]);
    assert!(build.status.success(), "{rows}\n{}", text(&build.stderr));
}

#[test]
fn an_altium_boards_part_on_the_bottom_comes_back_drawn_from_the_top() {
    use netsmith_kicad::altium_fixture::{Component, pcbdoc};
    let dir = TempDir::new("import-footprint-altium-board");
    let dir = dir.path();
    project(dir, "");
    // The part on the bottom face at (20, 30) mm, turned by 90 degrees: mirrored (x negated),
    // turned, moved.
    let place = |(x, y): (f64, f64)| (20.0 - y, 30.0 - x);
    let records = altium_sensor(0, &place, 90.0, true);
    let pick = |kind: u8| -> Vec<Vec<u8>> {
        records.iter().filter(|r| r[0] == kind).cloned().collect()
    };
    let board = pcbdoc(
        &[Component {
            designator: "U3",
            pattern: "SOT-23-3_Sensor",
            at: (20.0, 30.0),
            angle: 90.0,
            bottom: true,
        }],
        &[
            ("Pads6", pick(2)),
            ("Tracks6", pick(4)),
            ("Regions6", pick(11)),
            ("Texts6", pick(5)),
        ],
    );
    fs::write(dir.join("vendor/eval.PcbDoc"), board).unwrap();
    let run = netsmith(
        dir,
        &[
            "import",
            "footprint",
            "vendor/eval.PcbDoc",
            "--ref",
            "U3",
            "--name",
            "own:Sensor",
        ],
    );
    let out = text(&run.stdout);
    assert!(run.status.success(), "{}{out}", text(&run.stderr));
    assert!(
        out.starts_with(
            "wrote libs/own.pretty/Sensor.kicad_mod (3 pads, turned from the bottom face to the top)\n"
        ),
        "{out}"
    );
    let written = fs::read_to_string(dir.join("libs/own.pretty/Sensor.kicad_mod")).unwrap();
    for line in [
        "(pad \"1\" smd rect\n\t\t(at -1.5 -0.95)",
        "(pad \"2\" smd rect\n\t\t(at -1.5 0.95)",
        "(layers \"F.Cu\" \"F.Paste\" \"F.Mask\")",
    ] {
        assert!(written.contains(line), "{line}:\n{written}");
    }
    assert!(!written.contains("B.Cu"), "{written}");

    // A board needs `--ref`; a library has one footprint.
    let no_ref = netsmith(dir, &["import", "footprint", "vendor/eval.PcbDoc"]);
    assert_eq!(
        text(&no_ref.stderr),
        "error: vendor/eval.PcbDoc: a board holds many footprints: name the part with `--ref REF`\n"
    );
}

/// The sensor's library with a second package, `SOT-3-ALT`, whose pad 2 is on the other side.
fn eagle_library_of_two() -> String {
    EAGLE_LIBRARY.replace(
        "</packages>",
        "<package name=\"SOT-3-ALT\">\n<smd name=\"1\" x=\"-1\" y=\"0\" dx=\"0.6\" dy=\"0.9\" layer=\"1\"/>\n<smd name=\"2\" x=\"1\" y=\"0\" dx=\"0.6\" dy=\"0.9\" layer=\"1\"/>\n</package>\n</packages>",
    )
}

#[test]
fn a_library_of_several_packages_gives_the_one_footprint_names() {
    let dir = TempDir::new("import-footprint-eagle-several");
    let dir = dir.path();
    project(dir, "");
    fs::write(dir.join("vendor/parts.lbr"), eagle_library_of_two()).unwrap();

    let refused = netsmith(dir, &["import", "footprint", "vendor/parts.lbr", "--name", "own:X"]);
    assert_eq!(refused.status.code(), Some(2));
    assert_eq!(
        text(&refused.stderr),
        "error: vendor/parts.lbr: the library has 2 packages (SOT-3-SENSOR, SOT-3-ALT): name the one to import with `--footprint NAME`\n"
    );
    let missing = netsmith(
        dir,
        &["import", "footprint", "vendor/parts.lbr", "--footprint", "NOPE", "--name", "own:X"],
    );
    assert_eq!(missing.status.code(), Some(2));
    assert_eq!(
        text(&missing.stderr),
        "error: vendor/parts.lbr: the library has no package `NOPE`; its packages are SOT-3-SENSOR, SOT-3-ALT\n"
    );
    assert!(!dir.join("libs").exists());

    let run = netsmith(
        dir,
        &[
            "import", "footprint", "vendor/parts.lbr", "--footprint", "SOT-3-ALT", "--name",
            "own:Alt",
        ],
    );
    let out = text(&run.stdout);
    assert!(run.status.success(), "{}{out}", text(&run.stderr));
    assert!(
        out.starts_with(
            "wrote libs/own.pretty/Alt.kicad_mod (2 pads)\nthe file names it `SOT-3-ALT`; it is `Alt` here\n"
        ),
        "{out}"
    );
}

#[test]
fn a_pretty_folder_of_several_footprints_gives_the_one_footprint_names() {
    let dir = TempDir::new("import-footprint-pretty-several");
    let dir = dir.path();
    project(dir, "");
    // A distributor's folder of two footprints.
    for (name, reference) in [("Alpha", "U3"), ("Beta", "U3")] {
        let made = netsmith(
            dir,
            &[
                "import", "footprint", "vendor/eval.kicad_pcb", "--ref", reference, "--name",
                &format!("tmp:{name}"),
            ],
        );
        assert!(made.status.success(), "{}", text(&made.stderr));
    }
    fs::rename(dir.join("libs/tmp.pretty"), dir.join("vendor/dist.pretty")).unwrap();

    let refused = netsmith(dir, &["import", "footprint", "vendor/dist.pretty", "--name", "own:X"]);
    assert_eq!(refused.status.code(), Some(2));
    assert_eq!(
        text(&refused.stderr),
        "error: vendor/dist.pretty: the folder has 2 footprints (Alpha, Beta): name the one to import with `--footprint NAME`\n"
    );
    let missing = netsmith(
        dir,
        &["import", "footprint", "vendor/dist.pretty", "--footprint", "Gamma", "--name", "own:X"],
    );
    assert_eq!(
        text(&missing.stderr),
        "error: vendor/dist.pretty: the folder has no footprint `Gamma`; its footprints are Alpha, Beta\n"
    );
    let run = netsmith(
        dir,
        &["import", "footprint", "vendor/dist.pretty", "--footprint", "Beta", "--name", "own:Picked"],
    );
    let out = text(&run.stdout);
    assert!(run.status.success(), "{}{out}", text(&run.stderr));
    assert!(out.starts_with("wrote libs/own.pretty/Picked.kicad_mod (3 pads)\n"), "{out}");
    // The footprint file is the folder's `Beta`, whatever its name was.
    let kept = fs::read_to_string(dir.join("libs/own.pretty/Picked.kicad_mod")).unwrap();
    assert!(kept.contains("(footprint \"Picked\""), "{kept}");
}

#[test]
fn footprint_is_refused_for_a_board_and_a_footprint_file() {
    let dir = TempDir::new("import-footprint-flag-refused");
    let dir = dir.path();
    project(dir, "");
    fs::write(dir.join("vendor/eval.brd"), eagle_board()).unwrap();
    let cases: [(&[&str], &str); 2] = [
        (
            &["vendor/eval.kicad_pcb", "--ref", "U3", "--footprint", "X", "--name", "own:X"],
            "error: vendor/eval.kicad_pcb: `--footprint` picks one of the footprints of a `.pretty` folder, an Eagle library or an Altium library, and a board's part is named with `--ref`\n",
        ),
        (
            &["vendor/eval.brd", "--ref", "U3", "--footprint", "X", "--name", "own:X"],
            "error: vendor/eval.brd: `--footprint` picks one of the footprints of a `.pretty` folder, an Eagle library or an Altium library, and a board's part is named with `--ref`\n",
        ),
    ];
    for (args, want) in cases {
        let mut all = vec!["import", "footprint"];
        all.extend(args);
        let run = netsmith(dir, &all);
        assert_eq!(run.status.code(), Some(2), "{args:?}");
        assert_eq!(text(&run.stderr), want, "{args:?}");
    }
    assert!(!dir.join("libs").exists());
}

#[test]
fn an_altium_library_of_several_footprints_gives_the_one_footprint_names() {
    use netsmith_kicad::altium_fixture::{NO_COMPONENT, pcblib};
    let dir = TempDir::new("import-footprint-altium-several");
    let dir = dir.path();
    project(dir, "");
    let file = pcblib(&[
        ("SOT-23-3_Sensor", altium_sensor(NO_COMPONENT, &|p| p, 0.0, false)),
        ("OTHER", altium_sensor(NO_COMPONENT, &|p| p, 0.0, false)[..2].to_vec()),
    ]);
    fs::write(dir.join("vendor/parts.PcbLib"), file).unwrap();

    let refused = netsmith(dir, &["import", "footprint", "vendor/parts.PcbLib", "--name", "own:X"]);
    assert_eq!(refused.status.code(), Some(2));
    assert_eq!(
        text(&refused.stderr),
        "error: vendor/parts.PcbLib: the library holds 2 footprints (OTHER, SOT-23-3_Sensor): name the one to import with `--footprint NAME`\n"
    );
    let missing = netsmith(
        dir,
        &["import", "footprint", "vendor/parts.PcbLib", "--footprint", "NOPE", "--name", "own:X"],
    );
    assert_eq!(
        text(&missing.stderr),
        "error: vendor/parts.PcbLib: the library has no footprint `NOPE`; its footprints are OTHER, SOT-23-3_Sensor\n"
    );
    assert!(!dir.join("libs").exists());

    let run = netsmith(
        dir,
        &[
            "import", "footprint", "vendor/parts.PcbLib", "--footprint", "OTHER", "--name",
            "own:Other",
        ],
    );
    let out = text(&run.stdout);
    assert!(run.status.success(), "{}{out}", text(&run.stderr));
    assert!(
        out.starts_with(
            "wrote libs/own.pretty/Other.kicad_mod (2 pads)\nthe file names it `OTHER`; it is `Other` here\n"
        ),
        "{out}"
    );
}

/// An invented reference board's Gerber package with no X2 attributes (Protel's names): the
/// sensor `U3` of [`SOURCE`] on the top turned by 90 degrees at (20, 30), its pads `R` apertures
/// 0.7 x 1.2 mm in the board's frame; a resistor `R1` at (10, 10); a tented via at (15, 20) with a
/// track to `U3`'s pad 1.
const TOP_COPPER: &str = "G04 invented reference board, top copper*
%FSLAX46Y46*%
%MOMM*%
%ADD10R,0.700000X1.200000*%
%ADD11R,0.500000X0.500000*%
%ADD12C,0.600000*%
%ADD13C,0.200000*%
D10*
X19050000Y28500000D03*
X20950000Y28500000D03*
X20000000Y31500000D03*
D11*
X9500000Y10000000D03*
X10500000Y10000000D03*
D12*
X15000000Y20000000D03*
D13*
X15000000Y20000000D02*
X19050000Y28500000D01*
M02*
";

const TOP_MASK: &str = "G04 invented reference board, top mask*
%FSLAX46Y46*%
%MOMM*%
%ADD10R,0.800000X1.300000*%
%ADD11R,0.600000X0.600000*%
D10*
X19050000Y28500000D03*
X20950000Y28500000D03*
X20000000Y31500000D03*
D11*
X9500000Y10000000D03*
X10500000Y10000000D03*
M02*
";

/// Paste on two of the sensor's pads only, so the third has none.
const TOP_PASTE: &str = "G04 invented reference board, top paste*
%FSLAX46Y46*%
%MOMM*%
%ADD10R,0.700000X1.200000*%
D10*
X19050000Y28500000D03*
X20950000Y28500000D03*
M02*
";

const VIA_DRILL: &str = "M48
METRIC
T1C0.300
%
T1
X15.0Y20.0
M30
";

/// An assembler's placement file, its lengths with a unit.
const CPL: &str = "Designator,Mid X,Mid Y,Layer,Rotation
R1,10mm,10mm,Top,0
U3,20mm,30mm,Top,90
";

fn gerber_package(dir: &Path, files: &[(&str, &str)]) {
    fs::create_dir_all(dir).unwrap();
    for (name, text) in files {
        fs::write(dir.join(name), text).unwrap();
    }
}

#[test]
fn a_part_of_a_reference_boards_gerber_files_becomes_a_footprint_a_board_builds_with() {
    let dir = TempDir::new("import-footprint-gerber");
    let dir = dir.path();
    project(dir, "");
    gerber_package(
        &dir.join("vendor/gerbers"),
        &[
            ("ref.GTL", TOP_COPPER),
            ("ref.GTS", TOP_MASK),
            ("ref.GTP", TOP_PASTE),
            ("ref.drl", VIA_DRILL),
            ("cpl.csv", CPL),
        ],
    );
    let run = netsmith(
        dir,
        &[
            "import",
            "footprint",
            "vendor/gerbers",
            "--ref",
            "U3",
            "--name",
            "own:Sensor",
        ],
    );
    let out = text(&run.stdout);
    assert!(run.status.success(), "{}{out}", text(&run.stderr));
    assert!(
        out.starts_with("wrote libs/own.pretty/Sensor.kicad_mod (3 pads)\n"),
        "{out}"
    );
    for said in [
        "rebuilt from the Gerber files",
        "the place of U3 from cpl.csv: (20, 30) mm, 90°, top",
        "3 pads with their apertures' shapes and sizes",
        "the mask openings from the mask file",
        "guessed, to check against the datasheet:",
        "the pads of U3: the flashes of ref.GTL nearer its centre than any other part's of cpl.csv",
        "1 copper flash on a plated hole with no mask opening taken for via",
        "the pad numbers, counted counter-clockwise from the top left",
        "the footprint's angle: the placement file's 90° is taken off",
        "the courtyard: a rectangle 0.25 mm round the pads",
        "not rebuilt: the body on the fabrication layer, the silkscreen and a 3D model",
    ] {
        assert!(out.contains(said), "{said}:\n{out}");
    }
    let written = fs::read_to_string(dir.join("libs/own.pretty/Sensor.kicad_mod")).unwrap();
    for line in [
        "(footprint \"Sensor\"",
        "(pad \"1\" smd rect (at -1.5 -0.95) (size 1.2 0.7) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\"))",
        "(pad \"2\" smd rect (at -1.5 0.95) (size 1.2 0.7) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\"))",
        "(pad \"3\" smd rect (at 1.5 0) (size 1.2 0.7) (layers \"F.Cu\" \"F.Mask\"))",
        "(fp_rect (start -2.35 -1.55) (end 2.35 1.55)",
        "(layer \"F.CrtYd\")",
        "\"REF**\"",
    ] {
        assert!(written.contains(line), "{line}:\n{written}");
    }
    assert_eq!(written.matches("(pad ").count(), 3, "{written}");

    // The printed rows, with the datasheet's numbers, build the board with the footprint.
    let start = out.find("[[library]]").unwrap();
    let end = out.find("\nfill `table`").unwrap();
    let rows = out[start..end]
        .replace("kind = \"TODO\"", "kind = \"Sensor\"")
        .replace("package = \"TODO\"", "package = \"SOT-23\"")
        .replace("pitch=TODO", "pitch=1.9mm");
    assert!(rows.contains("table = \"pads=3, pitch=1.9mm\""), "{rows}");
    fs::write(dir.join("kicad.toml"), &rows).unwrap();
    let build = netsmith(dir, &["board", "build", "--out", "out"]);
    assert!(build.status.success(), "{rows}\n{}", text(&build.stderr));
}

/// An invented board's bottom copper with X2 attributes: the header `J1` on the bottom at
/// (40, 20), its pads named by `%TO.P`, pad 1 square.
const BOTTOM_COPPER: &str = "%TF.FileFunction,Copper,L2,Bot*%
%FSLAX46Y46*%
%MOMM*%
%TA.AperFunction,ComponentPad*%
%ADD10R,1.700000X1.700000*%
%ADD11C,1.700000*%
%TD*%
%TO.C,J1*%
%TO.P,J1,1,VCC*%
D10*
X40000000Y20000000D03*
%TO.P,J1,2,GND*%
D11*
X37460000Y20000000D03*
%TD*%
M02*
";

const PLATED_DRILL: &str = "M48
; #@! TF.FileFunction,Plated,1,2,PTH
METRIC
T1C1.000
%
T1
X40.0Y20.0
X37.46Y20.0
M30
";

const UNPLATED_DRILL: &str = "M48
; #@! TF.FileFunction,NonPlated,1,2,NPTH
METRIC
T1C1.200
%
T1
X38.73Y22.0
X70.0Y5.0
M30
";

/// KiCad's position file as CSV: its `Package` names the footprint.
const POS: &str = "Ref,Val,Package,PosX,PosY,Rot,Side
\"J1\",\"Conn\",\"PinHeader_1x02\",40.000000,20.000000,0.000000,bottom
\"R1\",\"10k\",\"R_0402\",10.000000,10.000000,0.000000,top
";

#[test]
fn x2_attributes_name_the_pads_and_a_bottom_part_is_seen_from_the_top() {
    let dir = TempDir::new("import-footprint-gerber-x2");
    let dir = dir.path();
    project(
        dir,
        "[[library]]\nname = \"parts\"\nfootprints = \"footprints/parts.pretty\"\n",
    );
    gerber_package(
        &dir.join("vendor/fab"),
        &[
            ("board-B_Cu.gbr", BOTTOM_COPPER),
            ("board-PTH.drl", PLATED_DRILL),
            ("board-NPTH.drl", UNPLATED_DRILL),
            ("board-pos.csv", POS),
        ],
    );
    let run = netsmith(dir, &["import", "footprint", "vendor/fab", "--ref", "J1"]);
    let out = text(&run.stdout);
    assert!(run.status.success(), "{}{out}", text(&run.stderr));
    assert!(
        out.starts_with(
            "wrote footprints/parts.pretty/PinHeader_1x02.kicad_mod (2 pads, turned from the bottom face to the top)\n"
        ),
        "{out}"
    );
    for said in [
        "the pads of J1: the flashes board-B_Cu.gbr names J1's by its X2 attributes",
        "2 pads with their apertures' shapes and sizes, 2 on a plated hole of the drill file",
        "1 unplated hole of the drill file within 2 mm of the pads",
        "the pad numbers from the copper file's X2 attributes",
        "no bottom mask file: every pad has a mask opening",
        "the bottom face: the pads are mirrored left to right",
    ] {
        assert!(out.contains(said), "{said}:\n{out}");
    }
    assert!(!out.contains("counter-clockwise"), "{out}");
    let written =
        fs::read_to_string(dir.join("footprints/parts.pretty/PinHeader_1x02.kicad_mod")).unwrap();
    for line in [
        "(attr through_hole)",
        "(pad \"1\" thru_hole rect (at 0 0) (size 1.7 1.7) (drill 1) (layers \"*.Cu\" \"*.Mask\"))",
        "(pad \"2\" thru_hole circle (at 2.54 0) (size 1.7 1.7) (drill 1) (layers \"*.Cu\" \"*.Mask\"))",
        "(pad \"\" np_thru_hole circle (at 1.27 -2) (size 1.2 1.2) (drill 1.2)",
    ] {
        assert!(written.contains(line), "{line}:\n{written}");
    }
    assert_eq!(written.matches("np_thru_hole").count(), 1, "{written}");
}

#[test]
fn what_a_gerber_package_lacks_is_said() {
    let dir = TempDir::new("import-footprint-gerber-errors");
    let dir = dir.path();
    project(dir, "");
    gerber_package(&dir.join("vendor/bare"), &[("ref.GTL", TOP_COPPER)]);
    gerber_package(
        &dir.join("vendor/full"),
        &[("ref.GTL", TOP_COPPER), ("cpl.csv", CPL)],
    );
    let cases: [(&[&str], &str); 4] = [
        (
            &["vendor/full", "--name", "own:X"],
            "error: vendor/full: Gerber files hold a whole board: name the part with `--ref REF`\n",
        ),
        (
            &["vendor/bare", "--ref", "U3", "--name", "own:X"],
            "error: vendor/bare: no placement file (a CSV of designators and their x, y and rotation) in it: add the reference board's placement file beside its Gerber files\n",
        ),
        (
            &["vendor/full", "--ref", "U9", "--name", "own:X"],
            "error: vendor/full: the placement file (cpl.csv) has no part `U9`\n",
        ),
        (
            &["vendor/full", "--ref", "U3", "--name", "own:X:"],
            "error: `X:` is not a footprint name: a name is a file's name, with no `/`, `\\` or `:`; give another with `--name`\n",
        ),
    ];
    for (args, want) in cases {
        let mut all = vec!["import", "footprint"];
        all.extend(args);
        let run = netsmith(dir, &all);
        assert_eq!(run.status.code(), Some(2), "{args:?}");
        assert_eq!(text(&run.stderr), want, "{args:?}");
    }
    assert!(!dir.join("libs").exists());
}
