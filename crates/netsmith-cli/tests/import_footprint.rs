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
            "error: src/main.nsm: not a KiCad board (`.kicad_pcb`), footprint (`.kicad_mod`) or `.pretty` folder, nor an Eagle board (`.brd`) or library (`.lbr`)\n",
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
            "error: vendor/sensor.lbr: `--ref` names a part of a board, and a library the command reads has one footprint\n",
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
