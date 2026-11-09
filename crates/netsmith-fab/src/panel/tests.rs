use super::*;
use crate::artwork::Artwork;

/// A board of 40 x 30 mm: a resistor, a pin header with a hole, a ground track on the front,
/// a supply track through a via on the back, and a plain outline.
const CELL: &str = r#"(kicad_pcb
	(version 20241229)
	(generator "netsmith")
	(general (thickness 1.6))
	(paper "A4")
	(title_block (title "Tile"))
	(layers
		(0 "F.Cu" signal)
		(2 "B.Cu" signal)
		(13 "F.Paste" user)
		(5 "F.SilkS" user "F.Silkscreen")
		(7 "B.SilkS" user "B.Silkscreen")
		(1 "F.Mask" user)
		(3 "B.Mask" user)
		(25 "Edge.Cuts" user)
		(35 "F.Fab" user)
	)
	(setup (aux_axis_origin 20 20))
	(net 0 "")
	(net 1 "GND")
	(net 2 "VCC")
	(footprint "Resistor_SMD:R_0603_1608Metric"
		(layer "F.Cu")
		(uuid "00000000-0000-0000-0000-000000000001")
		(at 30 30)
		(property "Reference" "R1" (at 0 -1.5 0) (layer "F.SilkS") (effects (font (size 1 1) (thickness 0.15))) (uuid "00000000-0000-0000-0000-000000000002"))
		(property "Value" "10k" (at 0 1.5 0) (layer "F.Fab") (effects (font (size 1 1) (thickness 0.15))) (uuid "00000000-0000-0000-0000-000000000003"))
		(attr smd)
		(pad "1" smd roundrect (at -0.8 0) (size 0.9 0.95) (layers "F.Cu" "F.Mask" "F.Paste") (roundrect_rratio 0.25) (net 1 "GND"))
		(pad "2" smd roundrect (at 0.8 0) (size 0.9 0.95) (layers "F.Cu" "F.Mask" "F.Paste") (roundrect_rratio 0.25) (net 2 "VCC"))
	)
	(footprint "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical"
		(layer "F.Cu")
		(uuid "00000000-0000-0000-0000-000000000004")
		(at 45 40)
		(property "Reference" "J1" (at 0 -2 0) (layer "F.SilkS") (effects (font (size 1 1) (thickness 0.15))) (uuid "00000000-0000-0000-0000-000000000005"))
		(property "Value" "Conn" (at 0 5 0) (layer "F.Fab") (effects (font (size 1 1) (thickness 0.15))) (uuid "00000000-0000-0000-0000-000000000006"))
		(attr through_hole)
		(pad "1" thru_hole rect (at 0 0) (size 1.7 1.7) (drill 1) (layers "*.Cu" "*.Mask") (net 1 "GND"))
		(pad "2" thru_hole oval (at 0 2.54) (size 1.7 1.7) (drill 1) (layers "*.Cu" "*.Mask") (net 2 "VCC"))
	)
	(segment (start 29.2 30) (end 26 30) (width 0.25) (layer "F.Cu") (net 1) (uuid "00000000-0000-0000-0000-000000000007"))
	(segment (start 26 30) (end 26 40) (width 0.25) (layer "F.Cu") (net 1) (uuid "00000000-0000-0000-0000-000000000008"))
	(segment (start 26 40) (end 45 40) (width 0.25) (layer "F.Cu") (net 1) (uuid "00000000-0000-0000-0000-000000000009"))
	(segment (start 30.8 30) (end 30.8 34) (width 0.25) (layer "F.Cu") (net 2) (uuid "00000000-0000-0000-0000-00000000000a"))
	(via (at 30.8 34) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 2) (uuid "00000000-0000-0000-0000-00000000000b"))
	(segment (start 30.8 34) (end 30.8 42.54) (width 0.25) (layer "B.Cu") (net 2) (uuid "00000000-0000-0000-0000-00000000000c"))
	(segment (start 30.8 42.54) (end 45 42.54) (width 0.25) (layer "B.Cu") (net 2) (uuid "00000000-0000-0000-0000-00000000000d"))
	(gr_rect (start 20 20) (end 60 50) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts") (uuid "00000000-0000-0000-0000-00000000000e"))
)"#;

/// Numbers in the shape of JLCPCB's, for the tests; the real ones are `std::mfg::jlcpcb`'s.
fn rules() -> Rules {
    Rules {
        fabricator: "JLCPCB".into(),
        source: "https://example.test/rules".into(),
        min_rail: 5.0,
        min_size: 70.0,
        max_size: 475.0,
        max_assembled_size: 250.0,
        max_cuts: 25,
        min_cut_spacing: 2.0,
        advised_cut_spacing: 3.0,
        min_edge_clearance: 0.4,
        advised_edge_clearance: 1.0,
        min_thickness: 0.6,
        tooling_hole: 2.0,
        fiducial: 1.0,
        fiducial_offset: 3.85,
    }
}

fn spec(columns: u32, rows: u32, rail: f64) -> Spec {
    Spec {
        name: "Panel".into(),
        columns,
        rows,
        rail,
        rules: rules(),
    }
}

fn facts() -> CellFacts {
    CellFacts {
        board: "Tile".into(),
        thickness: Some(1.6),
        edge_clearance: 0.5,
        assembled: true,
        bottom_assembled: false,
    }
}

fn entries() -> Vec<BomEntry> {
    vec![BomEntry {
        designators: vec!["R1".into()],
        value: "10k".into(),
        footprint: "R_0603_1608Metric".into(),
        lcsc: Some("C25804".into()),
    }]
}

fn build_2x3() -> PanelPackage {
    // Two columns and three rows of 40 x 30 mm boards, with rails: 90 x 100 mm.
    let entries = entries();
    let input = CellInput {
        pcb: CELL,
        pro: None,
        entries: &entries,
        facts: facts(),
    };
    build(&spec(2, 3, 5.0), &input, &Options::default()).unwrap_or_else(|e| panic!("{e}"))
}

fn messages(findings: &[Finding]) -> String {
    findings
        .iter()
        .filter(|f| f.error)
        .map(|f| f.message.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_cells_the_rails_and_the_cut_lines_of_a_grid() {
    let g = geometry(&spec(3, 2, 5.0), cell_outline(CELL).unwrap());
    assert_eq!((g.width, g.height), (130.0, 70.0));
    assert_eq!((g.cell_width, g.cell_height), (40.0, 30.0));
    // Four vertical lines (rail, two between boards, rail), three horizontal.
    assert_eq!(g.cuts_x, [25.0, 65.0, 105.0, 145.0]);
    assert_eq!(g.cuts_y, [25.0, 55.0, 85.0]);
    let offsets: Vec<(usize, f64, f64)> = g.cells.iter().map(|c| (c.index, c.dx, c.dy)).collect();
    assert_eq!(
        offsets,
        [
            (1, 5.0, 5.0),
            (2, 45.0, 5.0),
            (3, 85.0, 5.0),
            (4, 5.0, 35.0),
            (5, 45.0, 35.0),
            (6, 85.0, 35.0)
        ]
    );
    // Without rails the outer edge is the panel's edge and needs no line.
    let g = geometry(&spec(2, 2, 0.0), cell_outline(CELL).unwrap());
    assert_eq!(
        (g.cuts_x.clone(), g.cuts_y.clone()),
        (vec![60.0], vec![50.0])
    );
}

#[test]
fn a_board_with_rounded_corners_cannot_be_cut_by_a_v_cut() {
    let rounded = CELL.replace(
        r#"(gr_rect (start 20 20) (end 60 50) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts") (uuid "00000000-0000-0000-0000-00000000000e"))"#,
        r#"(gr_line (start 22 20) (end 58 20) (stroke (width 0.05) (type default)) (layer "Edge.Cuts"))
	(gr_arc (start 58 20) (mid 59.414 20.586) (end 60 22) (stroke (width 0.05) (type default)) (layer "Edge.Cuts"))
	(gr_line (start 60 22) (end 60 48) (stroke (width 0.05) (type default)) (layer "Edge.Cuts"))
	(gr_arc (start 60 48) (mid 59.414 49.414) (end 58 50) (stroke (width 0.05) (type default)) (layer "Edge.Cuts"))
	(gr_line (start 58 50) (end 22 50) (stroke (width 0.05) (type default)) (layer "Edge.Cuts"))
	(gr_arc (start 22 50) (mid 20.586 49.414) (end 20 48) (stroke (width 0.05) (type default)) (layer "Edge.Cuts"))
	(gr_line (start 20 48) (end 20 22) (stroke (width 0.05) (type default)) (layer "Edge.Cuts"))
	(gr_arc (start 20 22) (mid 20.586 20.586) (end 22 20) (stroke (width 0.05) (type default)) (layer "Edge.Cuts"))"#,
    );
    let e = cell_outline(&rounded).unwrap_err().to_string();
    assert!(
        e.contains("not a rectangle with square corners") && e.contains("corner radius to 0"),
        "{e}"
    );
}

#[test]
fn the_checks_name_the_number_and_the_rule() {
    let outline = cell_outline(CELL).unwrap();
    let run = |spec: Spec, facts: CellFacts| {
        let g = geometry(&spec, outline);
        messages(&check(&spec, &g, &facts))
    };
    // Too narrow a rail.
    let e = run(spec(3, 2, 3.0), facts());
    assert!(
        e.contains("the rail is 3 mm wide and a rail that carries tooling holes and fiducials is at least 5 mm")
            && e.contains("(JLCPCB, https://example.test/rules)"),
        "{e}"
    );
    // Too small: 2 x 1 boards with no rail are 80 x 30 mm.
    let mut bare = facts();
    bare.assembled = false;
    let e = run(spec(2, 1, 0.0), bare.clone());
    assert!(
        e.contains("the panel is 30 mm high and a V-cut panel is at least 70 mm each way"),
        "{e}"
    );
    // Too big for assembly, and for V-cut at all.
    let e = run(spec(7, 2, 5.0), facts());
    assert!(
        e.contains(
            "the panel is 290 mm wide and a panel that is assembled is at most 250 mm each way"
        ),
        "{e}"
    );
    assert!(run(spec(7, 2, 5.0), bare.clone()).is_empty());
    let e = run(spec(13, 2, 5.0), bare.clone());
    assert!(
        e.contains("the panel is 530 mm wide and a V-cut panel is at most 475 mm each way"),
        "{e}"
    );
    // More lines than JLCPCB cuts.
    let mut r = spec(3, 2, 5.0);
    r.rules.max_cuts = 3;
    let e = run(r, facts());
    assert!(
        e.contains("it needs 4 vertical V-cut lines and there may be 3 along one side"),
        "{e}"
    );
    // Two lines too close.
    let mut r = spec(3, 2, 5.0);
    r.rules.min_cut_spacing = 35.0;
    let e = run(r, facts());
    assert!(
        e.contains("the board height is 30 mm, which puts two V-cut lines 30 mm apart and they are at least 35 mm apart"),
        "{e}"
    );
    // Thickness and edge clearance of the board.
    let mut thin = facts();
    thin.thickness = Some(0.4);
    let e = run(spec(3, 2, 5.0), thin);
    assert!(
        e.contains("Tile is 0.4 mm thick and a V-cut board is at least 0.6 mm thick"),
        "{e}"
    );
    let mut open = facts();
    open.thickness = None;
    let e = run(spec(3, 2, 5.0), open);
    assert!(
        e.contains("Tile leaves its thickness open")
            && e.contains(".process.stackup.thickness &= 1.6mm"),
        "{e}"
    );
    let mut near = facts();
    near.edge_clearance = 0.2;
    let e = run(spec(3, 2, 5.0), near);
    assert!(
        e.contains(
            "Tile keeps copper 0.2 mm from its edge and copper is at least 0.4 mm from a V-cut edge"
        ) && e.contains(".process.min_edge_clearance &= 0.4mm..1m"),
        "{e}"
    );
    // An assembled panel needs its marks, which the rails carry.
    let mut big = spec(2, 3, 0.0);
    big.columns = 2;
    let e = run(big, facts());
    assert!(
        e.contains("has parts the board house assembles and a panel that is assembled needs tooling holes and fiducials on its rails: add `.rail &= 5mm`"),
        "{e}"
    );
    // Nothing to cut.
    let e = run(spec(1, 1, 0.0), bare);
    assert!(
        e.contains("one board and no rail has nothing to cut apart"),
        "{e}"
    );
}

#[test]
fn advice_is_a_warning_and_not_a_refusal() {
    let outline = cell_outline(CELL).unwrap();
    let s = spec(2, 3, 5.0);
    let g = geometry(&s, outline);
    let mut narrow = s.clone();
    narrow.rules.advised_cut_spacing = 45.0;
    let f = check(&narrow, &geometry(&narrow, outline), &facts());
    assert!(
        f.iter().any(|f| !f.error
            && f.message
                .contains("the board width is 40 mm and JLCPCB advises V-cut lines 45 mm apart")),
        "{f:?}"
    );
    assert!(f.iter().all(|f| !f.error), "{f:?}");
    let mut close = facts();
    close.edge_clearance = 0.45;
    let f = check(&s, &g, &close);
    assert!(
        f.iter().any(|f| !f.error
            && f.message.contains(
                "copper is 0.45 mm from the board edge and JLCPCB advises 1 mm from a V-cut"
            )),
        "{f:?}"
    );
}

#[test]
fn overlapping_cells_are_found() {
    let s = spec(2, 1, 5.0);
    let mut g = geometry(&s, cell_outline(CELL).unwrap());
    g.cells[1].dx -= 10.0;
    let e = messages(&check(&s, &g, &facts()));
    assert!(
        e.contains("cells 1 and 2 overlap (30 mm apart across, 0 mm down, boards 40 by 30 mm)"),
        "{e}"
    );
}

#[test]
fn each_cell_is_the_boards_items_moved_with_nets_and_designators_of_its_own() {
    let pkg = build_2x3();
    let root = sexp::parse(&pkg.pcb).unwrap();
    // 6 resistors and 6 headers, 4 tooling holes and 3 fiducials.
    assert_eq!(root.children("footprint").count(), 6 * 2 + 4 + 3);
    assert_eq!(root.children("via").count(), 6);
    assert_eq!(root.children("segment").count(), 6 * 6);
    let refs: Vec<String> = root
        .children("footprint")
        .filter_map(|f| {
            f.children("property")
                .find(|p| p.items()[1].text() == Some("Reference"))
                .and_then(|p| p.items()[2].text().map(str::to_string))
        })
        .collect();
    let mut unique = refs.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), refs.len(), "{refs:?}");
    for want in ["R1_1", "R1_6", "J1_3", "FID1", "TH4"] {
        assert!(refs.iter().any(|r| r == want), "{want} in {refs:?}");
    }
    // The nets of cell 3 are its own.
    let nets: Vec<String> = root
        .children("net")
        .filter_map(|n| n.items().get(2)?.text().map(str::to_string))
        .collect();
    assert_eq!(nets.len(), 1 + 6 * 2);
    assert!(nets.iter().any(|n| n == "cell3/GND") && nets.iter().any(|n| n == "cell6/VCC"));
    // Cell 1 is the board moved by the rail, cell 6 by one column and two rows more.
    let r1 = |k: usize| -> (f64, f64) {
        let f = root
            .children("footprint")
            .find(|f| {
                f.children("property")
                    .any(|p| p.items()[2].text() == Some(&format!("R1_{k}")))
            })
            .unwrap();
        let at = f.child("at").unwrap();
        (
            at.items()[1].text().unwrap().parse().unwrap(),
            at.items()[2].text().unwrap().parse().unwrap(),
        )
    };
    assert_eq!(r1(1), (35.0, 35.0));
    assert_eq!(r1(2), (75.0, 35.0));
    assert_eq!(r1(6), (75.0, 95.0));
    // The silkscreen keeps the board's own name: six plain texts "R1" and no other.
    let silk: Vec<&str> = root
        .children("footprint")
        .flat_map(|f| f.children("fp_text"))
        .filter_map(|t| t.items().get(2)?.text())
        .collect();
    assert_eq!(silk.iter().filter(|t| **t == "R1").count(), 6, "{silk:?}");
    // Every item has a name of its own.
    let text = &pkg.pcb;
    let mut ids: Vec<&str> = text
        .match_indices("(uuid \"")
        .map(|(i, _)| &text[i + 7..i + 43])
        .collect();
    let n = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), n, "a uuid is used twice");
}

#[test]
fn one_outline_with_the_cut_lines_and_their_marks_on_edge_cuts() {
    let pkg = build_2x3();
    let root = sexp::parse(&pkg.pcb).unwrap();
    let on_edge = |head: &str| -> usize {
        root.children(head)
            .filter(|e| layer_of(e) == Some("Edge.Cuts"))
            .count()
    };
    assert_eq!(on_edge("gr_rect"), 1, "the boards' own outlines are gone");
    // Rails and two columns: 3 vertical lines; rails and three rows: 4 horizontal.
    assert_eq!(on_edge("gr_line"), 3 + 4);
    assert_eq!(on_edge("gr_text"), 3 + 4);
    assert!(pkg.pcb.contains("\"V-CUT\""));
    let g = &pkg.geometry;
    assert_eq!((g.width, g.height), (90.0, 100.0));
    let art = Artwork::parse(&pkg.pcb).unwrap();
    let outline = edge::outline(&art).unwrap();
    assert_eq!(outline.outer.len(), 4);
    assert!(outline.cutouts.is_empty());
    assert_eq!(edge::cut_lines(&art).len(), 7);
    // The Gerber outline holds the rectangle (20, 20) to (110, 120) of the file, which is
    // (-5, 5) to (85, -95) from the drill and place origin (25, 25) with y up, the lines
    // across it and the marks beyond it.
    let gerber = String::from_utf8(gerber_file(&pkg, "Edge_Cuts.gbr")).unwrap();
    assert!(gerber.contains("X0Y5000000"), "{gerber}");
    assert!(gerber.contains("X0Y-95000000"), "{gerber}");
    assert!(gerber.contains("X-5000000Y-60000000"), "{gerber}");
    let view = crate::raster::gerber_extent(&gerber).unwrap();
    assert!(view.min.0 < -5.0 && view.max.0 > 85.0, "{view:?}");
    assert!(view.min.1 < -95.0 && view.max.1 > 5.0, "{view:?}");
}

fn gerber_file(pkg: &PanelPackage, suffix: &str) -> Vec<u8> {
    use std::io::Read as _;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(pkg.gerber_zip.clone())).unwrap();
    let mut out = Vec::new();
    zip.by_name(&format!("gerber/{}-{suffix}", pkg.name))
        .unwrap()
        .read_to_end(&mut out)
        .unwrap();
    out
}

#[test]
fn holes_bill_and_placement_are_six_times_the_boards() {
    let pkg = build_2x3();
    // The board has 2 header holes and a 0.3 mm via; the rails add 4 tooling holes.
    let pth = String::from_utf8(gerber_file(&pkg, "PTH.drl")).unwrap();
    let holes = pth.lines().filter(|l| l.starts_with('X')).count();
    assert_eq!(holes, 6 * 3, "{pth}");
    let npth = String::from_utf8(gerber_file(&pkg, "NPTH.drl")).unwrap();
    assert_eq!(
        npth.lines().filter(|l| l.starts_with('X')).count(),
        4,
        "{npth}"
    );
    // One BOM line with a designator for each cell; one placement each.
    assert_eq!(pkg.plan.bom.len(), 1);
    assert_eq!(pkg.plan.bom[0].designators.len(), 6);
    assert_eq!(pkg.plan.hand_solder.len(), 6);
    assert!(
        pkg.bom_csv.contains("\"R1_1,R1_2,R1_3,R1_4,R1_5,R1_6\""),
        "{}",
        pkg.bom_csv
    );
    let rows: Vec<&str> = pkg.cpl_csv.lines().skip(1).collect();
    assert_eq!(rows.len(), 6);
    // The board's R1 is 10 mm from its origin in both directions; the origin moves with
    // cell 1, and the cells are 40 mm across and 30 mm down.
    assert!(rows.contains(&"R1_1,10,-10,top,0"), "{rows:?}");
    assert!(rows.contains(&"R1_2,50,-10,top,0"), "{rows:?}");
    assert!(rows.contains(&"R1_6,50,-70,top,0"), "{rows:?}");
    // The board's checks ran on the whole panel.
    assert_eq!(pkg.drc, DrcOutcome::Passed, "{:?}", pkg.drc_report);
    assert!(pkg.notes.contains("design rules check: passed"));
}

#[test]
fn the_notes_say_what_the_panel_is_and_what_is_unverified() {
    let pkg = build_2x3();
    for want in [
        "panel: 2 columns x 3 rows of Tile (6 boards of 40 x 30 mm), 5 mm rails on all four sides, 90 x 100 mm in all",
        "V-cut: 3 vertical and 4 horizontal line(s)",
        "marked V-CUT",
        "x = 25, 65, 105 mm, y = 25, 55, 85, 115 mm",
        "4 tooling holes of 2 mm",
        "3 fiducials of 1 mm centred 3.85 mm from the panel edges",
        "V-cut is not available in economic assembly",
        "UNVERIFIED designators",
        "has not accepted a netsmith panel",
        "cell/Tile/",
    ] {
        assert!(
            pkg.notes.contains(want),
            "missing {want:?} in\n{}",
            pkg.notes
        );
    }
    assert!(pkg.svg.contains("Tile 6") && pkg.svg.contains("stroke-dasharray"));
}

#[test]
fn a_panel_with_no_rail_has_no_marks() {
    let mut bare = facts();
    bare.assembled = false;
    let input = CellInput {
        pcb: CELL,
        pro: None,
        entries: &[],
        facts: bare,
    };
    // 2 x 3 boards of 40 x 30 mm: 80 x 90 mm, nothing assembled, so no rail is needed.
    let options = Options {
        fit: [("R1".to_string(), Fit::NotFitted)].into(),
        ..Options::default()
    };
    let pkg = build(&spec(2, 3, 0.0), &input, &options).unwrap();
    let root = sexp::parse(&pkg.pcb).unwrap();
    assert_eq!(root.children("footprint").count(), 12);
    assert!(
        pkg.notes
            .contains("rails: none, so no tooling holes and no fiducials")
    );
}

#[test]
fn a_refused_panel_lists_every_reason() {
    let entries = entries();
    let mut f = facts();
    f.thickness = Some(0.4);
    f.edge_clearance = 0.2;
    let input = CellInput {
        pcb: CELL,
        pro: None,
        entries: &entries,
        facts: f,
    };
    let e = build(&spec(2, 1, 0.0), &input, &Options::default())
        .unwrap_err()
        .to_string();
    for want in [
        "is at least 70 mm each way",
        "0.4 mm thick",
        "0.2 mm from its edge",
        "tooling holes",
    ] {
        assert!(e.contains(want), "missing {want:?} in\n{e}");
    }
}

#[test]
fn the_panel_summary_counts_the_lines_of_bom_csv_the_consigned_ones_too() {
    // Every resistor is consigned: `bom.csv` has one line, and none is from stock.
    let entries = entries();
    let input = CellInput {
        pcb: CELL,
        pro: None,
        entries: &entries,
        facts: facts(),
    };
    let options = Options {
        fit: [("R1".to_string(), Fit::Consigned)].into(),
        ..Options::default()
    };
    let pkg = build(&spec(2, 3, 5.0), &input, &options).unwrap_or_else(|e| panic!("{e}"));
    let rows = pkg.bom_csv.lines().count() - 1;
    assert_eq!(rows, 1, "{}", pkg.bom_csv);
    assert!(
        pkg.summary()
            .contains(&format!("6 part(s) on {rows} BOM line(s)")),
        "{}",
        pkg.summary()
    );
}

#[test]
fn designators_carry_the_cell_and_lose_it_again() {
    assert_eq!(suffixed("R1", 3), "R1_3");
    assert_eq!(unsuffixed("R1_3"), "R1");
    assert_eq!(unsuffixed("R1"), "R1");
}

#[test]
fn a_length_reads_as_short_as_it_can() {
    assert_eq!(mm(50.0), "50");
    assert_eq!(mm(12.5), "12.5");
    assert_eq!(mm(-0.0), "0");
}
