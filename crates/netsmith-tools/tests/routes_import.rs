//! `netsmith routes import` of a blind and a buried via (issue #1015): KiCad's `micro` and
//! `blind` vias are kept with the two layers they end on, the upper first, as the routes file
//! writes them, and a through via with none.

use netsmith_check::Loader;
use netsmith_pcb::{CopperLayer, Pt, Via, ViaSpan, kicad_pcb};
use netsmith_pipeline::{Libraries, build_with};
use netsmith_solve::Catalogs;
use netsmith_tools::routes_import::{Options, ZoneReason, import_with};

/// The six-layer HDI board of the guide's chapter on the process, placed.
fn hdi_board() -> netsmith_pcb::Board {
    let source = include_str!("fixtures/hdi_dense.nsm");
    let built = build_with(
        "hdi.nsm",
        source,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let libraries = Libraries::sample();
    netsmith_placer::board_from_design(&built[0].design, &libraries.mapping, &libraries.footprints)
        .unwrap()
}

#[test]
fn a_blind_and_a_buried_via_are_imported_with_their_layers() {
    let mut board = hdi_board();
    let at = |x: f64| board.outline.min + Pt::new(x, 1.0);
    let via = |at: Pt, span: Option<ViaSpan>, drill: f64, diameter: f64| Via {
        net: "GND".into(),
        at,
        diameter,
        drill,
        span,
    };
    let blind = ViaSpan {
        from: CopperLayer::Inner(4),
        to: CopperLayer::Back,
    };
    let buried = ViaSpan {
        from: CopperLayer::Inner(1),
        to: CopperLayer::Inner(4),
    };
    let vias = [
        via(at(1.0), Some(blind), 0.1, 0.25),
        via(at(2.0), Some(buried), 0.2, 0.45),
        via(at(3.0), None, 0.2, 0.45),
    ];
    board.vias.extend(vias);
    let pcb = kicad_pcb(&board);
    let import = netsmith_tools::routes_import::import(&board, &pcb, None, false)
        .unwrap_or_else(|e| panic!("{e}"));
    let gnd = &import.nets["GND"];
    let layers: Vec<Vec<String>> = gnd.vias.iter().map(|v| v.layers.clone()).collect();
    assert_eq!(
        layers,
        [
            vec!["inner4".to_string(), "bottom".to_string()],
            vec!["inner1".to_string(), "inner4".to_string()],
            vec![],
        ],
        "{:?}",
        import.skipped
    );
    assert_eq!(gnd.vias[0].drill, 0.1);
}

/// The board with its process's plane nets, and an outer pour on top with settings of its own.
fn poured_board(clearance: f64) -> netsmith_pcb::Board {
    let mut board = hdi_board();
    board.pours.push(netsmith_pcb::BoardPour {
        layer: CopperLayer::Front,
        net: "GND".into(),
        stitch_pitch: None,
        thermal_gap: Some(0.4),
        spoke_width: None,
        clearance: Some(clearance),
        min_width: None,
        pads: Some(netsmith_pcb::ZonePads::Solid),
    });
    board
}

#[test]
fn the_processs_own_planes_and_pours_are_left_out_and_said() {
    let board = poured_board(0.3);
    let own = board.planes.len() + board.pours.len();
    assert!(own >= 2, "the fixture has planes and a pour: {own}");
    let pcb = kicad_pcb(&board);
    let import = netsmith_tools::routes_import::import(&board, &pcb, None, false)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(import.process_zones, own);
    assert_eq!(import.zones(), 0, "{:?}", import.nets);
    let said = import
        .skipped
        .iter()
        .filter(|s| s.contains("the process's own plane or pour"))
        .count();
    assert_eq!(said, own, "{:?}", import.skipped);
    assert!(
        import.nets.values().all(|n| n.zones.is_empty()),
        "{:?}",
        import.nets
    );
}

#[test]
fn a_pour_fab_wrote_and_changed_since_is_left_out_unless_asked_for() {
    // Issue #1609: the KiCad board has the pour `fab` wrote with a clearance of 0.3 mm, and the
    // process states 0.4 mm now (as if it was changed in KiCad): the zone's uuid says `fab` wrote
    // it for the process, so it is no hand zone.
    let pcb = kicad_pcb(&poured_board(0.3));
    let board = poured_board(0.4);
    let own = board.planes.len() + board.pours.len();
    let import = netsmith_tools::routes_import::import(&board, &pcb, None, false)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(import.process_zones, own);
    assert_eq!(import.zones(), 0, "{:?}", import.nets);
    let changed: Vec<_> = import
        .zones_left_out
        .iter()
        .filter(|z| z.reason == ZoneReason::ProcessChanged)
        .collect();
    assert_eq!(changed.len(), 1, "{:?}", import.zones_left_out);
    assert_eq!(changed[0].layers, ["F.Cu"]);
    assert!(changed[0].said.contains("--process-zones"), "{}", changed[0].said);
    // Asked for, every zone of the process is taken as a hand zone, the changed one as it is.
    let options = Options {
        lock: false,
        process_zones: true,
    };
    let taken = import_with(&board, &pcb, None, options).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(taken.zones(), own, "{:?}", taken.skipped);
    assert!(
        taken
            .zones_left_out
            .iter()
            .all(|z| !matches!(z.reason, ZoneReason::Process | ZoneReason::ProcessChanged)),
        "{:?}",
        taken.zones_left_out
    );
    let top = taken.nets["GND"]
        .zones
        .iter()
        .find(|z| z.layers == ["top"])
        .expect("the pour on top");
    assert_eq!(top.clearance, 0.3);
}

#[test]
fn a_zone_like_the_processs_pour_but_drawn_in_kicad_is_a_hand_zone() {
    // The same zone as `fab` writes for the pour, with settings of its own and a uuid KiCad gave
    // it: a hand zone.
    let fabbed = kicad_pcb(&poured_board(0.3));
    let board = poured_board(0.4);
    let uuid = netsmith_pcb::process_zone_uuid(&board, "F.Cu");
    assert!(fabbed.contains(&uuid), "fab writes the pour's uuid");
    let pcb = fabbed.replace(&uuid, "0f0e0d0c-0b0a-4909-8807-060504030201");
    let import = netsmith_tools::routes_import::import(&board, &pcb, None, false)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(import.zones(), 1, "{:?}", import.nets);
    let zone = &import.nets["GND"].zones[0];
    assert_eq!(zone.layers, ["top"]);
    assert_eq!(zone.clearance, 0.3);
}

/// A zone as KiCad 9 saves one that a person drew, on `layer` of `net`, its corners `corners`
/// (KiCad's places).
fn kicad_zone(net: &str, layer: &str, corners: &[Pt]) -> String {
    let pts: Vec<String> = corners
        .iter()
        .map(|p| format!("(xy {} {})", p.x, p.y))
        .collect();
    format!(
        r#"	(zone
		(net_name "{net}")
		(layer "{layer}")
		(uuid "3b9e4c1a-5d2f-4e8a-9c7b-1a2b3c4d5e6f")
		(name "hand")
		(hatch edge 0.5)
		(priority 2)
		(connect_pads yes
			(clearance 0.25)
		)
		(min_thickness 0.2)
		(filled_areas_thickness no)
		(fill
			(thermal_gap 0.4)
			(thermal_bridge_width 0.3)
			(island_removal_mode 1)
		)
		(polygon
			(pts
				{}
			)
		)
	)
	(zone
		(net 0)
		(net_name "")
		(layer "{layer}")
		(uuid "4c0f5d2b-6e3a-4f9b-8d8c-2b3c4d5e6f70")
		(name "keep clear")
		(hatch edge 0.5)
		(connect_pads
			(clearance 0)
		)
		(min_thickness 0.25)
		(filled_areas_thickness no)
		(keepout
			(tracks not_allowed)
			(vias not_allowed)
			(pads allowed)
			(copperpour not_allowed)
			(footprints allowed)
		)
		(fill
			(thermal_gap 0.5)
			(thermal_bridge_width 0.5)
		)
		(polygon
			(pts
				{}
			)
		)
	)
"#,
        pts.join(" "),
        pts.join(" ")
    )
}

/// `pcb` with `items` added before its last parenthesis.
fn with_items(pcb: &str, items: &str) -> String {
    let end = pcb.trim_end().rfind(')').expect("a board");
    format!("{}{items}{}", &pcb[..end], &pcb[end..])
}

#[test]
fn a_hand_zone_drawn_in_kicad_is_imported_and_the_rule_area_beside_it_said() {
    let board = poured_board(0.3);
    let other = board
        .planes
        .iter()
        .map(|p| p.net.clone())
        .find(|n| n != "GND")
        .expect("the fixture has a plane of a net other than GND");
    let p = |x: f64, y: f64| board.outline.min + Pt::new(x, y);
    let corners = [p(1.0, 1.0), p(5.0, 1.0), p(5.0, 3.0), p(1.0, 3.0)];
    let filled = netsmith_pcb::kicad_pcb_filled(&board).unwrap_or_else(|e| panic!("{e:?}"));
    let pcb = with_items(&filled.text, &kicad_zone(&other, "B.Cu", &corners));
    let import = netsmith_tools::routes_import::import(&board, &pcb, None, false)
        .unwrap_or_else(|e| panic!("{e}"));
    let zones = &import.nets[&other].zones;
    assert_eq!(zones.len(), 1, "{:?}", import.skipped);
    let zone = &zones[0];
    assert_eq!(zone.layers, ["bottom"]);
    assert_eq!(zone.outline, [[1.0, 1.0], [5.0, 1.0], [5.0, 3.0], [1.0, 3.0]]);
    assert_eq!(
        (zone.priority, zone.clearance, zone.min_width, zone.pads),
        (2, 0.25, 0.2, netsmith_pcb::ZonePads::Solid)
    );
    assert_eq!((zone.thermal_gap, zone.spoke_width), (0.4, 0.3));
    assert_eq!(zone.islands, netsmith_pcb::ZoneIslands::Keep);
    // The process's planes and pour, and the rule area, are left out, each with why.
    let own = board.planes.len() + board.pours.len();
    let count = |r: ZoneReason| {
        import
            .zones_left_out
            .iter()
            .filter(|z| z.reason == r)
            .count()
    };
    assert_eq!(count(ZoneReason::Process), own, "{:?}", import.zones_left_out);
    assert_eq!(count(ZoneReason::RuleArea), 1, "{:?}", import.zones_left_out);
}

#[test]
fn a_board_fab_wrote_imports_and_writes_again_with_the_same_zones() {
    // Issue #1609: the pour, the planes and a hand zone of another net over the pour, written as
    // `fab` writes them (filled), imported, and written again from the routes file.
    let mut board = poured_board(0.3);
    let other = board
        .planes
        .iter()
        .map(|p| p.net.clone())
        .find(|n| n != "GND")
        .expect("the fixture has a plane of a net other than GND");
    let p = |x: f64, y: f64| board.outline.min + Pt::new(x, y);
    board.zones = vec![netsmith_pcb::BoardZone {
        net: other.clone(),
        layers: vec!["F.Cu".into()],
        outline: vec![p(1.0, 1.0), p(4.0, 1.0), p(4.0, 3.0), p(1.0, 3.0)],
        holes: Vec::new(),
        priority: 1,
        clearance: 0.3,
        min_width: 0.25,
        pads: netsmith_pcb::ZonePads::Thermal,
        thermal_gap: 0.5,
        spoke_width: 0.5,
        islands: netsmith_pcb::ZoneIslands::Remove,
        island_min_area: None,
    }];
    let first = netsmith_pcb::kicad_pcb_filled(&board)
        .unwrap_or_else(|e| panic!("{e:?}"))
        .text;
    let import = netsmith_tools::routes_import::import(&poured_board(0.3), &first, None, false)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(import.zones(), 1, "the hand zone: {:?}", import.skipped);
    assert_eq!(
        import.process_zones,
        board.planes.len() + board.pours.len()
    );
    let mut again = poured_board(0.3);
    let mut routes = netsmith_pcb::Routes::default();
    routes
        .boards
        .insert(again.name.clone(), import.finish(&again));
    netsmith_pcb::apply_routes(&mut again, &routes, "board.routes.toml")
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(again.zones, board.zones);
    let second = netsmith_pcb::kicad_pcb_filled(&again)
        .unwrap_or_else(|e| panic!("{e:?}"))
        .text;
    let read = |pcb: &str| {
        let (zones, unread) =
            netsmith_tools::copper::read_zones(pcb, "board").unwrap_or_else(|e| panic!("{e}"));
        assert!(unread.is_empty(), "{unread:?}");
        zones
    };
    let (a, b) = (read(&first), read(&second));
    let diff = netsmith_tools::copper::compare_zones(&a, &b, 1e-6);
    assert!(
        diff.missing.is_empty() && diff.extra.is_empty() && diff.differing.is_empty(),
        "{diff:?}"
    );
    assert_eq!(diff.matched, a.len(), "{diff:?}");
}

#[test]
fn a_hand_zone_over_the_processs_pour_comes_back_with_its_own_priority() {
    // Issue #1314: `fab` writes the routes file's zones one priority above a pour drawn under
    // them; the import of that board takes it off again, so the routes file is as it was.
    let mut board = poured_board(0.3);
    let other = board
        .planes
        .iter()
        .map(|p| p.net.clone())
        .find(|n| n != "GND")
        .expect("the fixture has a plane of a net other than GND");
    let p = |x: f64, y: f64| board.outline.min + Pt::new(x, y);
    let zone = |priority: u32| netsmith_pcb::BoardZone {
        net: other.clone(),
        layers: vec!["F.Cu".into()],
        outline: vec![p(1.0, 1.0), p(4.0, 1.0), p(4.0, 3.0), p(1.0, 3.0)],
        holes: Vec::new(),
        priority,
        clearance: 0.3,
        min_width: 0.25,
        pads: netsmith_pcb::ZonePads::Thermal,
        thermal_gap: 0.5,
        spoke_width: 0.5,
        islands: netsmith_pcb::ZoneIslands::Remove,
        island_min_area: None,
    };
    for priority in [0, 2] {
        board.zones = vec![zone(priority)];
        assert_eq!(board.zone_lift(), 1);
        let pcb = kicad_pcb(&board);
        assert!(pcb.contains(&format!("(priority {})", priority + 1)), "{pcb}");
        let import = netsmith_tools::routes_import::import(&board, &pcb, None, false)
            .unwrap_or_else(|e| panic!("{e}"));
        // The GND pour on top is the process's, left out; the hand zone is taken.
        assert_eq!(import.process_zones, board.planes.len() + board.pours.len());
        let zones = &import.nets[&other].zones;
        assert_eq!(zones.len(), 1, "{:?}", import.skipped);
        assert_eq!(zones[0].priority, priority);
    }
}

/// The board of [`hdi_board`], 8 mm longer and wider, with a ground pour on top stitched every
/// 2 mm to its ground plane.
fn stitched_board() -> netsmith_pcb::Board {
    let mut board = hdi_board();
    board.outline.max = board.outline.max + Pt::new(8.0, 8.0);
    board.pours.push(netsmith_pcb::BoardPour {
        layer: CopperLayer::Front,
        net: "GND".into(),
        stitch_pitch: Some(2.0),
        thermal_gap: None,
        spoke_width: None,
        clearance: None,
        min_width: None,
        pads: None,
    });
    board
}

#[test]
fn stitching_vias_brought_back_by_an_import_are_not_laid_again() {
    // Issue #1483: fab, import, fab, import, fab of a board with a stitched pour. The import
    // leaves the stitching vias out, and `fab` lays the grid once.
    let mut counts = Vec::new();
    let mut routes = netsmith_pcb::Routes::default();
    for round in 0..3 {
        let mut board = stitched_board();
        let name = board.name.clone();
        netsmith_pcb::apply_routes(&mut board, &routes, "board.routes.toml")
            .unwrap_or_else(|e| panic!("{e:?}"));
        netsmith_router::route(&mut board);
        let gnd = board.vias.iter().filter(|v| v.net == "GND").count();
        counts.push((board.vias.len(), gnd));
        let pcb = kicad_pcb(&board);
        let import = netsmith_tools::routes_import::import(&stitched_board(), &pcb, None, false)
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(
            !import.stitching_vias.is_empty(),
            "round {round}: the stitching vias are known: {:?}",
            import.skipped
        );
        let imported = import.counts().2;
        assert_eq!(
            imported + import.stitching_vias.len(),
            board.vias.len(),
            "round {round}: every via is imported or known as stitching: {:?}",
            import.skipped
        );
        // Locked, the net gets no copper from `fab`, so the import keeps them.
        let locked = netsmith_tools::routes_import::import(&stitched_board(), &pcb, None, true)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            (locked.stitching_vias.len(), locked.counts().2),
            (0, board.vias.len()),
            "round {round}: {:?}",
            locked.skipped
        );
        routes.boards.insert(name, import.finish(&board));
    }
    assert!(counts[0].1 > 0, "the pour is stitched: {counts:?}");
    assert_eq!(counts[1], counts[0], "{counts:?}");
    assert_eq!(counts[2], counts[0], "{counts:?}");
}

/// The vias of `net` on `board` no track of the net ends on.
fn lone_vias(board: &netsmith_pcb::Board, net: &str) -> Vec<Via> {
    board
        .vias
        .iter()
        .filter(|v| {
            v.net == net
                && !board.tracks.iter().any(|t| {
                    t.net == v.net && (t.from.distance(v.at) < 1e-6 || t.to.distance(v.at) < 1e-6)
                })
        })
        .cloned()
        .collect()
}

#[test]
fn a_grid_point_with_a_via_of_the_pours_net_near_it_gets_no_other() {
    let mut board = stitched_board();
    netsmith_router::route(&mut board);
    // The stitching vias are those a board with no pitch lacks.
    let mut unstitched = stitched_board();
    unstitched.pours[0].stitch_pitch = None;
    netsmith_router::route(&mut unstitched);
    let stitched: Vec<Via> = lone_vias(&board, "GND")
        .into_iter()
        .filter(|v| !unstitched.vias.contains(v))
        .collect();
    assert!(stitched.len() > 4, "{stitched:?}");
    // The stitching vias, each moved 0.1 mm off where it was (still within a third of the pitch of its grid point),
    // in a routes file: routed again, the board gets no more.
    let mut again = stitched_board();
    let corner = again.outline.min;
    let record = netsmith_pcb::NetRoutes {
        pads: netsmith_pcb::pad_hash(&again, "GND").unwrap(),
        vias: stitched
            .iter()
            .map(|v| netsmith_pcb::RouteVia {
                at: [v.at.x - corner.x + 0.1, v.at.y - corner.y],
                diameter: v.diameter,
                drill: v.drill,
                layers: Vec::new(),
                lock: false,
            })
            .collect(),
        ..Default::default()
    };
    let mut routes = netsmith_pcb::Routes::default();
    routes
        .boards
        .entry(again.name.clone())
        .or_default()
        .nets
        .insert("GND".into(), record);
    netsmith_pcb::apply_routes(&mut again, &routes, "board.routes.toml").unwrap();
    netsmith_router::route(&mut again);
    let gnd = |b: &netsmith_pcb::Board| b.vias.iter().filter(|v| v.net == "GND").count();
    assert_eq!(gnd(&again), gnd(&board), "{:?}", again.vias);
}
