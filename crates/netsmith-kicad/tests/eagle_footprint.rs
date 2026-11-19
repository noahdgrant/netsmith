//! A footprint taken from an Eagle library or board (issue #1430, `import_eagle_footprint`). The
//! files are small ones written for the tests.

use netsmith_kicad::{EagleSource, Sexp, import_eagle_footprint};

/// A library of one surface-mount package, drawn with Eagle's y up: three pads, the body on
/// tDocu, an outline and an arc on tPlace, a courtyard on a user layer of that name, the name
/// and value texts, and a keepout that a KiCad footprint has no layer for.
const LIBRARY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE eagle SYSTEM "eagle.dtd">
<eagle version="9.6.2">
<drawing>
<layers>
<layer number="1" name="Top" color="4" fill="1" visible="yes" active="yes"/>
<layer number="21" name="tPlace" color="7" fill="1" visible="yes" active="yes"/>
<layer number="25" name="tNames" color="7" fill="1" visible="yes" active="yes"/>
<layer number="27" name="tValues" color="7" fill="1" visible="yes" active="yes"/>
<layer number="39" name="tKeepout" color="4" fill="11" visible="yes" active="yes"/>
<layer number="51" name="tDocu" color="7" fill="1" visible="yes" active="yes"/>
<layer number="250" name="tCourtyard" color="3" fill="1" visible="yes" active="yes"/>
</layers>
<library name="probe-parts">
<packages>
<package name="SOT-3-SENSOR">
<description>A three-pin &lt;b&gt;sensor&lt;/b&gt;
 package.</description>
<smd name="1" x="-0.95" y="-1.1" dx="0.6" dy="0.9" layer="1"/>
<smd name="2" x="0.95" y="-1.1" dx="0.6" dy="0.9" layer="1" roundness="50"/>
<smd name="3" x="0" y="1.1" dx="0.6" dy="0.9" layer="1" rot="R90" cream="no"/>
<wire x1="-1.45" y1="-0.65" x2="1.45" y2="-0.65" width="0.1" layer="51"/>
<wire x1="1.45" y1="-0.65" x2="1.45" y2="0.65" width="0.1" layer="51"/>
<wire x1="1.45" y1="0.65" x2="-1.45" y2="0.65" width="0.1" layer="51"/>
<wire x1="-1.45" y1="0.65" x2="-1.45" y2="-0.65" width="0.1" layer="51"/>
<wire x1="-1" y1="0" x2="1" y2="0" width="0.15" layer="21" curve="-180"/>
<wire x1="-1.7" y1="-1.8" x2="1.7" y2="-1.8" width="0.05" layer="250"/>
<wire x1="1.7" y1="-1.8" x2="1.7" y2="1.8" width="0.05" layer="250"/>
<wire x1="1.7" y1="1.8" x2="-1.7" y2="1.8" width="0.05" layer="250"/>
<wire x1="-1.7" y1="1.8" x2="-1.7" y2="-1.8" width="0.05" layer="250"/>
<rectangle x1="-2" y1="-0.2" x2="2" y2="0.2" layer="39"/>
<text x="-1.5" y="2" size="0.8" layer="25">&gt;NAME</text>
<text x="-1.5" y="-2.8" size="0.8" layer="27">&gt;VALUE</text>
</package>
</packages>
</library>
</drawing>
</eagle>
"#;

/// A library of one through-hole package with each of Eagle's pad shapes, a hole and a circle.
const THROUGH_HOLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<eagle version="9.6.2">
<drawing>
<library>
<packages>
<package name="PINS">
<pad name="1" x="0" y="0" drill="1" shape="square"/>
<pad name="2" x="2.54" y="0" drill="1" diameter="1.8"/>
<pad name="3" x="5.08" y="0" drill="1" diameter="1.6" shape="long" rot="R90"/>
<pad name="4" x="7.62" y="0" drill="1" diameter="1.6" shape="offset"/>
<pad name="5" x="10.16" y="0" drill="0.4" shape="octagon" stop="no"/>
<hole x="5" y="3" drill="3.2"/>
<circle x="5" y="3" radius="2" width="0.2" layer="21"/>
<circle x="0" y="-2" radius="0.3" width="0" layer="21"/>
</package>
</packages>
</library>
</drawing>
</eagle>
"#;

/// A board with the library's package on the bottom face, turned, and a part of another library.
const BOARD: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<eagle version="9.6.2">
<drawing>
<board>
<libraries>
<library name="passives">
<packages>
<package name="R0402">
<smd name="1" x="-0.5" y="0" dx="0.5" dy="0.5" layer="1"/>
<smd name="2" x="0.5" y="0" dx="0.5" dy="0.5" layer="1"/>
</package>
</packages>
</library>
<library name="probe-parts">
<packages>
<package name="SOT-3-SENSOR">
<smd name="1" x="-0.95" y="-1.1" dx="0.6" dy="0.9" layer="1"/>
<smd name="2" x="0.95" y="-1.1" dx="0.6" dy="0.9" layer="1"/>
<smd name="3" x="0" y="1.1" dx="0.6" dy="0.9" layer="1"/>
</package>
</packages>
</library>
</libraries>
<elements>
<element name="R1" library="passives" package="R0402" value="10k" x="5" y="5"/>
<element name="U3" library="probe-parts" package="SOT-3-SENSOR" value="SENSOR-1" x="20" y="30" rot="MR90"/>
</elements>
</board>
</drawing>
</eagle>
"#;

/// The file on one line, as the tests write what they look for.
fn flat(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" )", ")")
}

#[test]
fn a_librarys_package_is_drawn_with_y_down() {
    let f = import_eagle_footprint(LIBRARY, EagleSource::Library { package: None }, Some("Sensor")).unwrap();
    assert_eq!(f.source_name, "SOT-3-SENSOR");
    assert_eq!(f.name, "Sensor");
    let parsed = Sexp::parse(&f.text).unwrap();
    assert_eq!(parsed.head(), Some("footprint"));
    let text = flat(&f.text);
    for want in [
        "(footprint \"Sensor\"",
        "(descr \"A three-pin <b>sensor</b> package.\")",
        "(attr smd)",
        "(pad \"1\" smd rect (at -0.95 1.1) (size 0.6 0.9) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\"))",
        "(pad \"2\" smd roundrect (at 0.95 1.1) (size 0.6 0.9) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\") (roundrect_rratio 0.25))",
        "(pad \"3\" smd rect (at 0 -1.1 90) (size 0.6 0.9) (layers \"F.Cu\" \"F.Mask\"))",
        "(fp_line (start -1.45 0.65) (end 1.45 0.65) (stroke (width 0.1) (type solid)) (layer \"F.Fab\"))",
        "(fp_line (start -1.7 1.8) (end 1.7 1.8) (stroke (width 0.05) (type solid)) (layer \"F.CrtYd\"))",
        // The arc sweeps clockwise from (-1, 0) to (1, 0) in Eagle's frame, over the top, so its
        // middle is at y 1 there and -1 here.
        "(fp_arc (start -1 0) (mid 0 -1) (end 1 0)",
        "(property \"Reference\" \"REF**\" (at -1.5 -2) (layer \"F.SilkS\")",
        "(property \"Value\" \"Sensor\" (at -1.5 2.8) (layer \"F.Fab\")",
    ] {
        assert!(text.contains(want), "{want}:\n{}", f.text);
    }
    assert!(!text.contains("NAME"), "{}", f.text);
    assert!(!text.contains("fp_poly"), "the keepout is left out:\n{}", f.text);
    assert_eq!(
        f.notes,
        ["left out 1 drawing on Eagle's layer tKeepout, which a KiCad footprint has no layer for"]
    );
    assert!(!f.from_bottom);
}

#[test]
fn each_through_hole_pad_shape_has_a_kicad_one() {
    let f = import_eagle_footprint(THROUGH_HOLE, EagleSource::Library { package: None }, None).unwrap();
    assert_eq!(f.name, "PINS");
    let text = flat(&f.text);
    for want in [
        "(attr through_hole)",
        // No diameter: Eagle's ring, a quarter of the drill held to at least 0.254 mm.
        "(pad \"1\" thru_hole rect (at 0 0) (size 1.508 1.508) (drill 1) (layers \"*.Cu\" \"*.Mask\"))",
        "(pad \"2\" thru_hole circle (at 2.54 0) (size 1.8 1.8) (drill 1)",
        "(pad \"3\" thru_hole oval (at 5.08 0 90) (size 3.2 1.6) (drill 1)",
        // The hole at the pad's end, the pad reaching out along x.
        "(pad \"4\" thru_hole oval (at 8.42 0) (size 3.2 1.6) (drill 1 (offset -0.8 0))",
        "(pad \"5\" thru_hole circle (at 10.16 0) (size 0.908 0.908) (drill 0.4) (layers \"*.Cu\"))",
        "(pad \"\" np_thru_hole circle (at 5 -3) (size 3.2 3.2) (drill 3.2)",
        "(fp_circle (center 5 -3) (end 7 -3) (stroke (width 0.2) (type solid)) (fill none) (layer \"F.SilkS\"))",
        "(fp_circle (center 0 2) (end 0.3 2) (stroke (width 0) (type solid)) (fill solid)",
    ] {
        assert!(text.contains(want), "{want}:\n{}", f.text);
    }
    assert_eq!(
        f.notes,
        ["1 octagonal pad written round: netsmith reads no octagonal pad"]
    );
}

#[test]
fn a_boards_part_gives_its_package_as_the_library_draws_it() {
    let f = import_eagle_footprint(BOARD, EagleSource::Board { reference: "U3" }, None).unwrap();
    assert_eq!(f.name, "SOT-3-SENSOR");
    assert_eq!(f.board_angle, 90.0);
    assert!(!f.from_bottom, "nothing was turned");
    assert!(f.text.contains("(at -0.95 1.1)"), "{}", f.text);
    assert!(f.text.contains("\"F.Cu\""), "{}", f.text);
    assert!(!f.text.contains("B.Cu"), "{}", f.text);
    assert!(f.notes[0].contains("bottom face"), "{:?}", f.notes);
}

#[test]
fn what_the_file_lacks_is_said() {
    let missing = import_eagle_footprint(BOARD, EagleSource::Board { reference: "U9" }, None);
    assert_eq!(
        missing.unwrap_err(),
        "the board has no part `U9`; its parts are R1, U3"
    );
    let two = LIBRARY.replace(
        "</packages>",
        "<package name=\"OTHER\"><smd name=\"1\" x=\"0\" y=\"0\" dx=\"1\" dy=\"1\" layer=\"1\"/></package></packages>",
    );
    let err = import_eagle_footprint(&two, EagleSource::Library { package: None }, None).unwrap_err();
    assert_eq!(
        err,
        "the library has 2 packages (SOT-3-SENSOR, OTHER): name the one to import with `--footprint NAME`"
    );
    let named = |package| EagleSource::Library {
        package: Some(package),
    };
    let other = import_eagle_footprint(&two, named("OTHER"), None).unwrap();
    assert_eq!(other.source_name, "OTHER");
    assert_eq!(other.name, "OTHER");
    let first = import_eagle_footprint(&two, named("SOT-3-SENSOR"), None).unwrap();
    assert_eq!(first.source_name, "SOT-3-SENSOR");
    let err = import_eagle_footprint(&two, named("NOPE"), None).unwrap_err();
    assert_eq!(
        err,
        "the library has no package `NOPE`; its packages are SOT-3-SENSOR, OTHER"
    );
    // A library of one package is asked by name too.
    let err = import_eagle_footprint(LIBRARY, named("OTHER"), None).unwrap_err();
    assert_eq!(
        err,
        "the library has no package `OTHER`; its packages are SOT-3-SENSOR"
    );
    let err = import_eagle_footprint(BOARD, EagleSource::Library { package: None }, None).unwrap_err();
    assert!(err.starts_with("not an Eagle library"), "{err}");
    let err = import_eagle_footprint("(kicad_pcb)", EagleSource::Library { package: None }, None).unwrap_err();
    assert!(err.starts_with("not an XML file"), "{err}");
    let err = import_eagle_footprint("<svg/>", EagleSource::Library { package: None }, None).unwrap_err();
    assert_eq!(err, "not an Eagle file: it starts with `<svg>`, not `<eagle>`");
}
