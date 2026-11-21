//! One edit of the routes file (`routes add-track`, `add-via`, `add-zone`, issue #1471) on a
//! dense board generated here: the grid of `routes_tidy` (two resistors and two or three nets a
//! cell, about 30 tracks and 2 vias a cell) on four layers, with a plane on each inner layer.
//! An edit is checked where it is: the items its check looks at are counted, and compared
//! between two sizes of board; they grow with the edit's neighbourhood, not with the board.
//! `routes join-pad` of a pad that only a via joins to its plane is made there too, and the
//! items its fills, connectivity and checks look at are compared so (issue #1474). `measured`
//! prints the time of each on a board of about 150 parts and 2,500 shapes. A batch of
//! `routes add` (issue #1472) is made there too: each shape's result and the findings of a
//! refusal, its document, and its work for each shape on two sizes of board; `measured_batch`
//! prints the time of 2,000 shapes.

use netsmith_fabricate::join_pad;
use netsmith_fabricate::routes::{self, Edit, Refusal, batch};
use netsmith_pcb::{
    Board, BoardNet, CopperLayer, NetRoutes, PadRef, Pt, RouteVia, Routes, ZonePads,
};

/// The four-layer fixture of `netsmith-pcb`: its process, rules and planes (GND on the first
/// inner layer, 3V3 on the second).
fn four_layer() -> Board {
    serde_json::from_str(include_str!(
        "../../netsmith-pcb/tests/fixtures/four_layer.board.json"
    ))
    .expect("a fixture is the JSON of a Board")
}

/// The LED board on the four-layer process with its planes.
fn led_on_four_layers() -> Board {
    let mut led: Board = serde_json::from_str(include_str!(
        "../../netsmith-pcb/tests/fixtures/led.board.json"
    ))
    .expect("a fixture is the JSON of a Board");
    let four = four_layer();
    led.process = four.process;
    led.rules = four.rules;
    led.planes = four.planes;
    led
}

/// A board of `columns` by `rows` cells on four layers and its routes file.
fn board(columns: usize, rows: usize) -> (Board, Routes) {
    let (mut board, routes) = super::routes_tidy::grid_on(led_on_four_layers(), columns, rows);
    for plane in board.planes.clone() {
        board.nets.push(BoardNet {
            name: plane.net,
            pads: Vec::new(),
        });
    }
    (board, routes)
}

/// [`board`] with a third resistor in the cell `n`, `RC<n>`, below its nets, both of whose pads
/// (on the top alone) are on GND, the net of the plane on the first inner layer, and joined to
/// nothing: `routes join-pad` joins pad 1 to the plane with a track and a via. A via of GND at
/// the cell's right edge keeps the plane's fill (a fill that touches no copper of its net is an
/// island, and removed).
fn lonely_pad(columns: usize, rows: usize, n: usize) -> (Board, Routes) {
    let (mut board, mut routes) = board(columns, rows);
    let [x, y] = cell(n, columns);
    let min = board.outline.min;
    let mut part = board.parts[0].clone();
    part.refdes = format!("RC{n}");
    part.path = part.refdes.to_lowercase();
    part.at = min + Pt::new(x + 5.0, y + 4.3);
    board.parts.push(part);
    let gnd = board
        .nets
        .iter_mut()
        .find(|net| net.name == "GND")
        .expect("the plane's net");
    for number in ["1", "2"] {
        gnd.pads.push(PadRef {
            refdes: format!("RC{n}"),
            number: number.to_string(),
        });
    }
    let pads = netsmith_pcb::pad_hash(&board, "GND").expect("a net of the board");
    routes
        .boards
        .get_mut(&board.name)
        .expect("the grid's record")
        .nets
        .insert(
            "GND".to_string(),
            NetRoutes {
                pads,
                vias: vec![RouteVia {
                    at: [x + 11.5, y + 4.5],
                    diameter: 0.45,
                    drill: 0.2,
                    layers: Vec::new(),
                    lock: false,
                }],
                ..NetRoutes::default()
            },
        );
    (board, routes)
}

/// The cell `n` of a board `columns` wide: its top left corner in the routes file's frame.
fn cell(n: usize, columns: usize) -> [f64; 2] {
    [1.0 + 12.0 * (n % columns) as f64, 0.5 + 5.0 * (n / columns) as f64]
}

/// The edits tried on the cell `n`: a track on the bottom under the top net's first track, a
/// via on that track, a zone of the net on the bottom round the via, all allowed, and a track
/// on the top across the top net of the cell's first corner, refused for a short.
fn edits(n: usize, columns: usize) -> Vec<(&'static str, Edit, bool)> {
    let [x, y] = cell(n, columns);
    let net = format!("s{n}");
    vec![
        (
            "add-track",
            Edit::AddTrack {
                net: net.clone(),
                layer: CopperLayer::Back,
                width: None,
                from: [x + 2.0, y + 1.0],
                mid: None,
                to: [x + 3.5, y + 1.0],
                lock: false,
            },
            true,
        ),
        (
            "add-via",
            Edit::AddVia {
                net: net.clone(),
                at: [x + 3.0, y + 1.0],
                diameter: None,
                drill: None,
                layers: None,
                lock: false,
            },
            true,
        ),
        (
            "add-zone",
            Edit::AddZone {
                net: net.clone(),
                layers: vec!["bottom".to_string()],
                outline: vec![
                    [x + 2.0, y + 0.3],
                    [x + 4.0, y + 0.3],
                    [x + 4.0, y + 1.7],
                    [x + 2.0, y + 1.7],
                ],
                clearance: None,
                min_width: None,
                thermal_gap: None,
                spoke_width: None,
                priority: 0,
                pads: ZonePads::Thermal,
                lock: false,
            },
            true,
        ),
        (
            "add-track (refused)",
            Edit::AddTrack {
                net: format!("z{n}"),
                layer: CopperLayer::Front,
                width: None,
                from: [x + 6.0, y + 0.5],
                mid: None,
                to: [x + 7.0, y + 1.5],
                lock: false,
            },
            false,
        ),
    ]
}

/// Each edit of [`edits`] made on the board of `columns` by `rows` cells, in the cell in the
/// middle, and the items its check looked at.
fn checked(columns: usize, rows: usize) -> Vec<(&'static str, usize)> {
    let (board, routes) = board(columns, rows);
    let n = columns * (rows / 2) + columns / 2;
    let mut out = Vec::new();
    for (what, edit, allowed) in edits(n, columns) {
        let mut next = routes.clone();
        let (result, counts) = routes::edit_all_counted(&board, &mut next, &[edit]);
        match (&result, allowed) {
            (Ok(_), true) | (Err(Refusal::Refused(_)), false) => {}
            _ => panic!("{what}: {result:?}"),
        }
        out.push((what, counts.items));
    }
    out
}

#[test]
fn the_check_of_an_edit_looks_at_its_neighbourhood_and_not_at_the_board() {
    // Four times the board: the same edits in the cell in the middle look at as many items.
    // Three rows, so the box round a via that the check of a cut piece of a fill looks in
    // (`pad_island_cut`, 5 mm round it, issue #1524) is inside the small board as well.
    let small = checked(3, 3);
    let big = checked(6, 6);
    for ((what, s), (_, b)) in small.iter().zip(&big) {
        eprintln!("{what}: {s} items on the small board, {b} on the big one");
        assert!(*s > 0, "{what}: the check looked at nothing");
        assert!(
            *b <= *s + *s / 4,
            "{what}: the items checked grow from {s} to {b} with four times the board"
        );
    }
}

/// Prints the time of each edit of [`edits`] on a board of 11 by 7 cells (154 parts, about
/// 2,300 tracks and 150 vias, four layers): `cargo test -p netsmith-fabricate --test main
/// routes_edit::measured -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement, not a check"]
fn measured() {
    let (columns, rows) = (11, 7);
    let (board, routes) = board(columns, rows);
    let shapes: usize = routes
        .boards
        .values()
        .flat_map(|t| t.nets.values())
        .map(|r| r.shapes())
        .sum();
    eprintln!(
        "{} parts, {} nets, {shapes} shapes, {} layers",
        board.parts.len(),
        board.nets.len(),
        board.process.stackup.layers
    );
    for (what, edit, _) in edits(columns * 3 + 5, columns) {
        let mut next = routes.clone();
        let began = std::time::Instant::now();
        let (result, counts) = routes::edit_all_counted(&board, &mut next, &[edit]);
        let time = began.elapsed();
        eprintln!(
            "{what}: {} in {time:.2?}, {} checks of {} items",
            match &result {
                Ok(_) => "made".to_string(),
                Err(e) => format!("refused ({e})"),
            },
            counts.checks,
            counts.items
        );
    }
    let n = columns * 3 + 5;
    let (board, routes) = lonely_pad(columns, rows, n);
    let began = std::time::Instant::now();
    let joined = join_pad::join_pad(&board, &routes, &format!("RC{n}.1")).unwrap();
    eprintln!(
        "join-pad: {} in {:.2?}, {} tries refused or not joining, {} fills, connectivity and checks of {} items",
        if joined.join.is_some() { "joined" } else { "not joined" },
        began.elapsed(),
        joined.tried.len(),
        joined.counts.checks,
        joined.counts.items
    );
}

#[test]
fn join_pad_joins_a_pad_to_its_plane_with_a_track_and_a_via() {
    let (board, routes) = lonely_pad(3, 2, 4);
    let joined = join_pad::join_pad(&board, &routes, "RC4.1").unwrap();
    let text = joined.text();
    let join = joined
        .join
        .as_ref()
        .unwrap_or_else(|| panic!("{text}{:?}", joined.refused));
    assert_eq!(join.fill_layer.as_deref(), Some("inner1"), "{text}");
    assert!(join.via.is_some(), "{text}");
}

#[test]
fn join_pad_looks_at_the_pads_neighbourhood_and_net_and_not_at_the_board() {
    // The pad in the cell in the middle of a board and of one four times its size, both large
    // enough that the box round the pad (more than 5 mm out) lies inside the board.
    let looked = |columns: usize, rows: usize| {
        let n = columns * (rows / 2) + columns / 2;
        let (board, routes) = lonely_pad(columns, rows, n);
        let joined = join_pad::join_pad(&board, &routes, &format!("RC{n}.1")).unwrap();
        assert!(joined.join.is_some(), "{}{:?}", joined.text(), joined.refused);
        let shapes: usize = board.parts.iter().map(|p| p.footprint.pads.len()).sum::<usize>()
            + routes
                .boards
                .values()
                .flat_map(|t| t.nets.values())
                .map(|r| r.shapes())
                .sum::<usize>();
        (joined.counts, shapes)
    };
    let (small, small_board) = looked(6, 4);
    let (big, big_board) = looked(12, 8);
    eprintln!(
        "join-pad: {} steps of {} items on the small board ({small_board} pads and shapes), {} steps of {} items on the big one ({big_board})",
        small.checks, small.items, big.checks, big.items
    );
    assert!(small.items > 0, "join-pad looked at nothing");
    assert_eq!(small.checks, big.checks, "the same tries on both boards");
    assert!(
        big.items <= small.items + small.items / 4,
        "the items join-pad looked at grow from {} to {} with four times the board",
        small.items,
        big.items
    );
}

/// The batch of `routes add` (issue #1472) for the cells `cells` of a board `columns` wide:
/// in each, `pieces` short tracks of its top net on the bottom under its first track, end to
/// end, a via on them and a zone of the net round the via, all allowed.
fn batch_of(cells: impl Iterator<Item = usize>, columns: usize, pieces: usize) -> batch::Document {
    let mut doc = batch::Document::default();
    for n in cells {
        let [x, y] = cell(n, columns);
        let net = format!("s{n}");
        let step = 1.5 / pieces as f64;
        for k in 0..pieces {
            doc.track.push(batch::TrackShape {
                net: net.clone(),
                layer: Some("bottom".to_string()),
                width: None,
                from: [x + 2.0 + step * k as f64, y + 1.0],
                to: [x + 2.0 + step * (k + 1) as f64, y + 1.0],
                mid: None,
                lock: false,
            });
        }
        doc.via.push(batch::ViaShape {
            net: net.clone(),
            at: batch::Places::One([x + 3.0, y + 1.0]),
            count: None,
            pitch: None,
            diameter: None,
            drill: None,
            layers: None,
            lock: false,
        });
        doc.zone.push(batch::ZoneShape {
            net,
            layer: Some(batch::Layers::One("bottom".to_string())),
            outline: None,
            rect: Some([x + 2.0, y + 0.3, 2.0, 1.4]),
            board_outline: false,
            clearance: None,
            min_width: None,
            thermal_gap: None,
            spoke_width: None,
            priority: 0,
            pads: None,
            lock: false,
        });
    }
    doc
}

#[test]
fn a_batch_makes_each_shape_skips_a_refused_one_and_says_why_with_its_place() {
    let (board, routes) = board(3, 2);
    let mut doc = batch_of([4].into_iter(), 3, 3);
    // Across the top net of the cell's first corner, on the top: a short.
    let [x, y] = cell(4, 3);
    doc.track.insert(
        1,
        batch::TrackShape {
            net: "z4".to_string(),
            layer: None,
            width: None,
            from: [x + 6.0, y + 0.5],
            to: [x + 7.0, y + 1.5],
            mid: None,
            lock: false,
        },
    );
    // A word that is not a layer is refused, not an error of the batch.
    doc.track.push(batch::TrackShape {
        layer: Some("middle".to_string()),
        ..doc.track[0].clone()
    });
    let mut next = routes.clone();
    let done = batch::add(&board, &mut next, &doc, false).unwrap();
    let results: Vec<(&str, usize, &str)> = done
        .shapes
        .iter()
        .map(|s| (s.kind, s.index, s.result))
        .collect();
    assert_eq!(
        results,
        [
            ("track", 1, "ok"),
            ("track", 2, "refused"),
            ("track", 3, "ok"),
            ("track", 4, "ok"),
            ("track", 5, "refused"),
            ("via", 1, "ok"),
            ("zone", 1, "ok"),
        ],
        "{}",
        done.text()
    );
    let short = &done.shapes[1];
    assert!(!short.findings.is_empty(), "{short:?}");
    let found = &short.findings[0];
    assert!(
        ["shorting_items", "tracks_crossing", "clearance"].contains(&found.rule.as_str()),
        "{found:?}"
    );
    // Its place is in the routes file's frame, near the track drawn.
    assert!(
        (x + 5.0..x + 8.0).contains(&found.at[0]) && (y - 0.5..y + 2.5).contains(&found.at[1]),
        "{found:?}"
    );
    assert!(done.shapes[4].said.contains("`middle`"), "{:?}", done.shapes[4]);
    assert_eq!((done.made, done.refused, done.not_tried), (5, 2, 0));
    let record = &next.boards[&board.name].nets["s4"];
    let before = &routes.boards[&board.name].nets["s4"];
    assert_eq!(record.tracks.len(), before.tracks.len() + 3);
    assert_eq!(record.vias.len(), before.vias.len() + 1);
    assert_eq!(record.zones.len(), before.zones.len() + 1);
    // The four tracks were checked together, refused, and halved until the short was found
    // alone: [1, 2] refused, [1] and [2] alone, then [3, 4]; the bad layer is no check; the
    // via and the zone are a group: 5 checks, each with and without its shapes.
    assert_eq!(done.checks, 2 * 6, "{}", done.text());

    // With `stop_on_refusal` the rest are not tried, and what was made before is kept.
    let mut next = routes.clone();
    let done = batch::add(&board, &mut next, &doc, true).unwrap();
    assert!(done.stopped);
    assert_eq!((done.made, done.refused, done.not_tried), (1, 1, 5));
    assert_eq!(
        next.boards[&board.name].nets["s4"].tracks.len(),
        before.tracks.len() + 1
    );
}

#[test]
fn a_batchs_document_is_toml_or_json_with_the_fields_of_the_verbs() {
    let toml = r#"
[[track]]
net = "s1"
layer = "bottom"
from = [1, 2]
to = [3, 2]

[[via]]
net = "s1"
at = [[1, 1], [1.5, 1]]

[[zone]]
net = "s1"
layer = ["top", "bottom"]
rect = [1, 1, 2, 2]
pads = "thru-hole-only"
"#;
    assert!(!batch::Document::is_json("shapes.toml", toml));
    let doc = batch::Document::parse(toml, "shapes.toml", false).unwrap();
    assert_eq!((doc.track.len(), doc.via.len(), doc.zone.len()), (1, 1, 1));
    let json = r#"{"track": [{"net": "s1", "layer": "bottom", "from": [1, 2], "to": [3, 2]}],
        "via": [{"net": "s1", "at": [[1, 1], [1.5, 1]]}],
        "zone": [{"net": "s1", "layer": ["top", "bottom"], "rect": [1, 1, 2, 2], "pads": "thru-hole-only"}]}"#;
    assert!(batch::Document::is_json("-", json));
    assert_eq!(
        batch::Document::parse(json, "stdin", true).unwrap(),
        doc
    );
    // A field no verb has, and a document with no shape, are errors naming the document.
    let e = batch::Document::parse("[[track]]\nnet = \"a\"\nfrom = [0, 0]\nto = [1, 0]\nwidht = 1\n", "s.toml", false)
        .unwrap_err();
    assert!(e.starts_with("s.toml:") && e.contains("widht"), "{e}");
    let e = batch::Document::parse("{}", "stdin", true).unwrap_err();
    assert!(e.contains("no shape"), "{e}");
}

/// The items the checks of a batch in the two by two cells at the top left corner of a board
/// `columns` by `rows` looked at, and the checks it made, for each shape.
fn batch_work_per_shape(columns: usize, rows: usize) -> (f64, f64) {
    let (board, mut routes) = board(columns, rows);
    let block = [0, 1, columns, columns + 1];
    let doc = batch_of(block.into_iter(), columns, 6);
    let done = batch::add(&board, &mut routes, &doc, false).unwrap();
    assert_eq!(done.refused, 0, "{}", done.text());
    let shapes = doc.shapes() as f64;
    (
        done.items_checked as f64 / shapes,
        done.checks as f64 / shapes,
    )
}

#[test]
fn a_batch_checks_its_shapes_in_their_neighbourhood_and_not_on_the_board() {
    // The same batch on four times the board: its checks look at about as many items for each
    // shape, so a batch costs its shapes' neighbourhoods; and its groups make fewer checks than
    // it has shapes.
    let (small, small_checks) = batch_work_per_shape(3, 2);
    let (big, big_checks) = batch_work_per_shape(6, 4);
    eprintln!(
        "{small:.1} items a shape on the small board ({small_checks:.2} checks a shape), {big:.1} on the big one ({big_checks:.2})"
    );
    assert!(small > 0.0);
    assert!(
        big <= small * 1.25,
        "the items checked for a shape grow from {small:.1} to {big:.1} with four times the board"
    );
    assert_eq!(big_checks, small_checks);
    assert!(big_checks < 1.0, "{big_checks} checks a shape");
}

/// Prints the time of a batch of about 2,000 shapes (26 in each cell) on the board of
/// [`measured`] (154 parts, four layers): `cargo test -p netsmith-fabricate --test main
/// routes_edit::measured_batch -- --ignored --nocapture`. The number to beat (issue #1472): a
/// minute.
#[test]
#[ignore = "a measurement, not a check"]
fn measured_batch() {
    let (columns, rows) = (11, 7);
    let (board, mut routes) = board(columns, rows);
    let doc = batch_of(0..columns * rows, columns, 24);
    let began = std::time::Instant::now();
    let done = batch::add(&board, &mut routes, &doc, false).unwrap();
    let time = began.elapsed();
    eprintln!(
        "{} parts, {} shapes: {} made, {} refused in {time:.2?} ({:.1?} a shape), {} checks of {} items",
        board.parts.len(),
        doc.shapes(),
        done.made,
        done.refused,
        time / doc.shapes() as u32,
        done.checks,
        done.items_checked
    );
}
