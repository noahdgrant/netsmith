//! The placement files and the aperture macros a footprint rebuilt from Gerber files reads
//! (issue 1432, `netsmith_tools::gerber_footprint`); the command's tests are in `netsmith-cli`.

use netsmith_tools::gerber_footprint::{Placement, import, parse_placement};

#[test]
fn placement_files_are_read_by_their_column_names() {
    let jlc = "Designator,Mid X,Mid Y,Layer,Rotation\nU3,20mm,30mm,Bottom,90\n";
    assert_eq!(
        parse_placement(jlc),
        Some(vec![Placement {
            designator: "U3".into(),
            x: 20.0,
            y: 30.0,
            rotation: 90.0,
            bottom: true,
            package: None,
        }])
    );
    // KiCad's position file in columns, its header a comment.
    let pos = "### Footprint positions ###\n## Unit = mm\n# Ref     Val       Package          PosX      PosY       Rot  Side\nC1        100n      C_0402           1.5000   -2.0000   180.0000  top\n## End\n";
    let rows = parse_placement(pos).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].package.as_deref(), Some("C_0402"));
    assert_eq!((rows[0].x, rows[0].y, rows[0].rotation), (1.5, -2.0, 180.0));
    // Another tool's, in mils, its columns' units in brackets.
    let mils = "\"Designator\",\"Footprint\",\"Center-X(mil)\",\"Center-Y(mil)\",\"Layer\",\"Rotation\"\n\"J2\",\"HDR\",\"1000\",\"500\",\"TopLayer\",\"270\"\n";
    let rows = parse_placement(mils).unwrap();
    assert!((rows[0].x - 25.4).abs() < 1e-9 && (rows[0].y - 12.7).abs() < 1e-9);
    assert!(!rows[0].bottom);
    assert_eq!(rows[0].package.as_deref(), Some("HDR"));
    assert_eq!(parse_placement("Part,Value\nR1,10k\n"), None);
}

#[test]
fn kicads_rounded_and_turned_rectangles_and_an_outline_macro_become_their_pads() {
    // A part `U1` at the origin with three pads, each a macro KiCad writes: a rounded rectangle
    // (radius 0.1, 1.0 x 0.6), a rectangle 1.0 x 0.5 turned by 30 degrees, a triangle.
    let copper = "%FSLAX46Y46*%
%MOMM*%
%AMRoundRect*
0 Rectangle with rounded corners*
4,1,4,$2,$3,$4,$5,$6,$7,$8,$9,$2,$3,0*
1,1,$1+$1,$2,$3*
1,1,$1+$1,$4,$5*
1,1,$1+$1,$6,$7*
1,1,$1+$1,$8,$9*
20,1,$1+$1,$2,$3,$4,$5,0*
20,1,$1+$1,$4,$5,$6,$7,0*
20,1,$1+$1,$6,$7,$8,$9,0*
20,1,$1+$1,$8,$9,$2,$3,0*%
%AMRotRect*
21,1,$1,$2,0,0,$3*%
%AMTriangle*
4,1,3,0,0.5,-0.5,-0.5,0.5,-0.5,0,0.5,0*%
%ADD10RoundRect,0.100000X-0.400000X-0.200000X0.400000X-0.200000X0.400000X0.200000X-0.400000X0.200000*%
%ADD11RotRect,1.000000X0.500000X30.000000*%
%ADD12Triangle*%
%TO.P,U1,1*%
D10*
X-2000000Y0D03*
%TO.P,U1,2*%
D11*
X0Y0D03*
%TO.P,U1,3*%
D12*
X2000000Y0D03*
%TD*%
M02*
";
    let files = vec![
        ("u-F_Cu.gbr".to_string(), copper.as_bytes().to_vec()),
        (
            "pos.csv".to_string(),
            b"Designator,Mid X,Mid Y,Layer,Rotation\nU1,0,0,top,0\n".to_vec(),
        ),
    ];
    let got = import(files, "U1", Some("Mix")).unwrap();
    for line in [
        "(pad \"1\" smd roundrect (at -2 0) (size 1 0.6)",
        "(roundrect_rratio 0.1667)",
        "(pad \"2\" smd rect (at 0 0 30) (size 1 0.5)",
        "(pad \"3\" smd custom (at 2 0) (size 0.01 0.01)",
        "(xy -0.5 0.5)",
        "(xy 0.5 0.5)",
        "(xy 0 -0.5)",
    ] {
        assert!(got.text.contains(line), "{line}:\n{}", got.text);
    }
    netsmith_pcb::Footprint::parse("Mix", &got.text).unwrap();
}
