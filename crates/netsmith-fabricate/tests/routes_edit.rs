//! One edit of the routes file (`routes add-track`, `add-via`, `add-zone`, issue #1471) on a
//! dense board generated here: the grid of `routes_tidy` (two resistors and two or three nets a
//! cell, about 30 tracks and 2 vias a cell) on four layers, with a plane on each inner layer.
//! An edit is checked where it is: the items its check looks at are counted, and compared
//! between two sizes of board; they grow with the edit's neighbourhood, not with the board.
//! `routes join-pad` of a pad that only a via joins to its plane is made there too. `measured`
//! prints the time of each on a board of about 150 parts and 2,500 shapes.

use netsmith_fabricate::join_pad;
use netsmith_fabricate::routes::{self, Edit, Refusal};
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
    let small = checked(3, 2);
    let big = checked(6, 4);
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
        "join-pad: {} in {:.2?}, {} tries refused or not joining",
        if joined.join.is_some() { "joined" } else { "not joined" },
        began.elapsed(),
        joined.tried.len()
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


