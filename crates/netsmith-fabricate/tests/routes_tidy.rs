//! `routes tidy` on a routed board generated here (issue #1225): a grid of cells, each two
//! resistors joined by two nets, one on the top with two right angles, a collinear pair and a
//! stub to tidy, one through two vias and a zigzag of 45 degree bends on the bottom with nothing
//! to tidy. In every third cell a track of a locked net shorts the inside of the first right
//! angle, so that each chamfer of it is refused, as a board imported with faults has some. The
//! work of a pass is counted, as the report counts it (the checks and the items they looked
//! at), and compared between two sizes: it grows with the changes and the board, not with their
//! product. `measured` prints the time of a board of about 2300 tracks.

use netsmith_fabricate::routes_tidy::{self, Tidied};
use netsmith_pcb::{
    Board, BoardNet, CopperLayer, NetRoutes, PadRef, Pt, Rect, RouteTrack, RouteVia, Routes,
};

/// The LED example placed, a fixture of `netsmith-pcb`: its resistor is the part of each cell.
fn led() -> Board {
    serde_json::from_str(include_str!(
        "../../netsmith-pcb/tests/fixtures/led.board.json"
    ))
    .expect("a fixture is the JSON of a Board")
}

const W: f64 = 0.15;
/// The size of a cell, in mm.
const CELL: [f64; 2] = [12.0, 5.0];
/// What each cell has to tidy: two corners, a collinear pair and a stub.
const CORNERS: usize = 2;
const MERGES: usize = 1;
const STUBS: usize = 1;
/// Every `SHORTED`th cell has its first corner shorted, and refused.
const SHORTED: usize = 3;

fn straight(layer: CopperLayer, points: &[[f64; 2]]) -> Vec<RouteTrack> {
    points
        .windows(2)
        .map(|p| RouteTrack {
            layer,
            width: W,
            from: p[0],
            mid: None,
            to: p[1],
            lock: false,
        })
        .collect()
}

fn via(at: [f64; 2]) -> RouteVia {
    RouteVia {
        at,
        diameter: 0.45,
        drill: 0.2,
        layers: Vec::new(),
        lock: false,
    }
}

/// A board of `columns` by `rows` cells and its routes file (places in the routes file's frame,
/// mm from the outline's top left corner).
fn grid(columns: usize, rows: usize) -> (Board, Routes) {
    grid_on(led(), columns, rows)
}

/// [`grid`] on `led`, the LED board or one like it (its process, rules and planes are the
/// grid's; its resistor `R1` is the part of each cell, its parts and nets are dropped): what
/// `routes_edit` builds its four-layer board on.
pub fn grid_on(led: Board, columns: usize, rows: usize) -> (Board, Routes) {
    let resistor = led
        .parts
        .iter()
        .find(|p| p.refdes == "R1")
        .expect("the LED board's resistor")
        .clone();
    let mut board = led.clone();
    board.name = "grid".to_string();
    board.parts.clear();
    board.nets.clear();
    let min = led.outline.min;
    board.outline = Rect::new(
        min,
        min + Pt::new(
            CELL[0] * columns as f64 + 1.0,
            CELL[1] * rows as f64 + 1.0,
        ),
    );
    let mut records: Vec<(String, Vec<RouteTrack>, Vec<RouteVia>, bool)> = Vec::new();
    for n in 0..columns * rows {
        let (x, y) = (
            1.0 + CELL[0] * (n % columns) as f64,
            0.5 + CELL[1] * (n / columns) as f64,
        );
        let (a, b) = (format!("RA{n}"), format!("RB{n}"));
        // Pads 1 and 2 of a resistor are 0.51 mm to the left and right of its place.
        for (refdes, at) in [(&a, [x + 1.0, y + 1.0]), (&b, [x + 10.0, y + 3.0])] {
            let mut part = resistor.clone();
            part.refdes = refdes.clone();
            part.path = refdes.to_lowercase();
            part.at = min + Pt::new(at[0], at[1]);
            board.parts.push(part);
        }
        let pad = |refdes: &str, number: &str| PadRef {
            refdes: refdes.to_string(),
            number: number.to_string(),
        };
        // The top net: two right angles, a pair joined at (x + 4, y + 1) and a stub up from the
        // middle of the second.
        let mut top = straight(
            CopperLayer::Front,
            &[
                [x + 1.51, y + 1.0],
                [x + 4.0, y + 1.0],
                [x + 6.5, y + 1.0],
                [x + 6.5, y + 3.0],
                [x + 9.49, y + 3.0],
            ],
        );
        top.extend(straight(
            CopperLayer::Front,
            &[[x + 5.0, y + 1.0], [x + 5.0, y + 0.4]],
        ));
        board.nets.push(BoardNet {
            name: format!("s{n}"),
            pads: vec![pad(&a, "2"), pad(&b, "1")],
        });
        records.push((format!("s{n}"), top, Vec::new(), false));
        if n % SHORTED == 0 {
            // A track of a locked net of no pads across the inside of the corner at (x + 6.5,
            // y + 1): every chamfer of it crosses the track.
            board.nets.push(BoardNet {
                name: format!("k{n}"),
                pads: Vec::new(),
            });
            let short = straight(
                CopperLayer::Front,
                &[[x + 6.25, y + 1.25], [x + 6.45, y + 1.05]],
            );
            records.push((format!("k{n}"), short, Vec::new(), true));
        }
        // The other net: down from pad 1 of the first to a via, a zigzag on the bottom that
        // bends by 45 degrees at each point, and up from a via to pad 2 of the second.
        let (start, end) = ([x + 0.49, y + 2.2], [x + 10.51, y + 3.8]);
        let mut points = vec![start, [x + 1.29, y + 3.0]];
        let mut at = x + 1.29;
        while at + 1.5 <= x + 9.71 {
            points.extend([
                [at + 0.4, y + 3.0],
                [at + 0.7, y + 2.7],
                [at + 1.1, y + 2.7],
                [at + 1.4, y + 3.0],
            ]);
            at += 1.4;
        }
        points.extend([[x + 9.71, y + 3.0], end]);
        let mut zig = straight(CopperLayer::Front, &[[x + 0.49, y + 1.0], start]);
        zig.extend(straight(CopperLayer::Back, &points));
        zig.extend(straight(CopperLayer::Front, &[end, [x + 10.51, y + 3.0]]));
        board.nets.push(BoardNet {
            name: format!("z{n}"),
            pads: vec![pad(&a, "1"), pad(&b, "2")],
        });
        records.push((format!("z{n}"), zig, vec![via(start), via(end)], false));
    }
    let mut routes = Routes::default();
    let table = routes.boards.entry(board.name.clone()).or_default();
    for (net, tracks, vias, lock) in records {
        table.nets.insert(
            net.clone(),
            NetRoutes {
                pads: netsmith_pcb::pad_hash(&board, &net).expect("a net of the board"),
                lock,
                tracks,
                vias,
                zones: Vec::new(),
            },
        );
    }
    (board, routes)
}

fn tracks(routes: &Routes) -> usize {
    routes
        .boards
        .values()
        .flat_map(|t| t.nets.values())
        .map(|r| r.tracks.len())
        .sum()
}

/// The pass over a board of `columns` by `rows` cells, checked as `checking` says, which makes
/// every change of every cell but the shorted corners, and refuses those.
fn tidied(columns: usize, rows: usize, checking: routes_tidy::Checking) -> (Tidied, usize) {
    let (board, routes) = grid(columns, rows);
    let tidied = routes_tidy::tidy_with(&board, &routes, None, checking).unwrap();
    let cells = columns * rows;
    let shorted = cells.div_ceil(SHORTED);
    let c = &tidied.counts;
    let said = tidied.text();
    assert_eq!(c.corners, CORNERS * cells - shorted, "{said}");
    assert_eq!(c.merges, MERGES * cells, "{said}");
    assert_eq!(c.stubs, STUBS * cells, "{said}");
    assert_eq!((c.vias, c.acid_traps), (0, 0), "{said}");
    assert_eq!(tidied.refused.len(), shorted, "{said}");
    assert!(
        tidied.refused.iter().all(|r| r.kind == routes_tidy::Kind::Corner
            && r.why.as_deref().is_some_and(|w| w.contains("new finding"))),
        "{said}"
    );
    assert!(tidied.findings_after < tidied.findings_before, "{said}");
    (tidied, tracks(&routes))
}

#[test]
fn the_work_of_a_pass_grows_with_the_changes_and_not_with_the_changes_times_the_board() {
    // Four times the cells: four times the changes and four times the board. Checking the
    // whole board for each group would look at sixteen times the items, as many more groups
    // over as much more board; the neighbourhoods look at four times, and the whole board once
    // a pass.
    use routes_tidy::Checking::{Near, Whole};
    let (small, small_tracks) = tidied(3, 2, Near);
    let (big, big_tracks) = tidied(6, 4, Near);
    assert_eq!(big_tracks, 4 * small_tracks);
    // The same changes and refusals as the check of the whole board for each group makes.
    let (whole, _) = tidied(3, 2, Whole);
    assert_eq!(small.changes, whole.changes);
    assert_eq!(small.refused, whole.refused);
    assert_eq!(small.routes, whole.routes);
    eprintln!(
        "{small_tracks} tracks, each group checked on the whole board: {} checks, {} items",
        whole.checks, whole.items_checked
    );
    let ratio = big.items_checked as f64 / small.items_checked as f64;
    eprintln!(
        "{small_tracks} tracks: {} checks, {} items; {big_tracks} tracks: {} checks, {} items; ratio {ratio:.2}",
        small.checks, small.items_checked, big.checks, big.items_checked
    );
    assert!(
        ratio < 6.0,
        "the items checked grow {ratio:.2} times for 4 times the board and the changes: {} then {}",
        small.items_checked,
        big.items_checked
    );
    // Each check of a neighbourhood looks at about as many items on either board.
    let per_check = |t: &Tidied| t.items_checked as f64 / t.checks as f64;
    assert!(
        per_check(&big) < 2.0 * per_check(&small),
        "{:.0} then {:.0} items a check",
        per_check(&small),
        per_check(&big)
    );
}

/// Prints the time of the pass over a board of about 2300 tracks with about 280 changes to make
/// and 26 to refuse, and its counts, checked by neighbourhoods and (with `NETSMITH_TIDY_WHOLE=1`,
/// which takes much longer) by the whole board for each group, and then the same with a zone of
/// a locked net over the whole bottom layer, whose fill each check makes: `cargo test --release
/// -p netsmith-fabricate --test main routes_tidy::measured -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement, not a check"]
fn measured() {
    let (board, routes) = grid(11, 7);
    let mut zoned = routes.clone();
    let [w, h] = [
        board.outline.width() - 0.5,
        board.outline.height() - 0.5,
    ];
    zoned
        .boards
        .get_mut(&board.name)
        .and_then(|t| t.nets.get_mut("k0"))
        .expect("cell 0 is shorted")
        .zones
        .push(netsmith_pcb::RouteZone {
            patch: false,
            layers: vec!["bottom".to_string()],
            priority: 0,
            clearance: 0.2,
            min_width: 0.25,
            pads: netsmith_pcb::ZonePads::Thermal,
            thermal_gap: 0.5,
            spoke_width: 0.5,
            islands: netsmith_pcb::ZoneIslands::Remove,
            island_min_area: None,
            outline: vec![[0.25, 0.25], [w, 0.25], [w, h], [0.25, h]],
            holes: Vec::new(),
            lock: true,
        });
    let mut ways = vec![routes_tidy::Checking::Near];
    if std::env::var_os("NETSMITH_TIDY_WHOLE").is_some() {
        ways.push(routes_tidy::Checking::Whole);
    }
    for (what, routes) in [("no zone", &routes), ("a zone", &zoned)] {
        for &checking in &ways {
            let began = std::time::Instant::now();
            let tidied = routes_tidy::tidy_with(&board, routes, None, checking).unwrap();
            let time = began.elapsed();
            eprintln!(
                "{what}, {checking:?}: {} tracks: {} change(s), {} refused, {} checks, {} items checked, {} pairs compared by the overlap pass, in {time:.1?}",
                tracks(routes),
                tidied.changes.len(),
                tidied.refused.len(),
                tidied.checks,
                tidied.items_checked,
                tidied.overlap_compared
            );
        }
    }
    // One net of thousands of tracks: the overlap pass's pairs (issue 1492).
    let (board, routes) = long(3000);
    let began = std::time::Instant::now();
    let tidied = routes_tidy::tidy(&board, &routes, None).unwrap();
    eprintln!(
        "one net of {} tracks: {} pairs compared by the overlap pass, in {:.1?}",
        tracks(&routes),
        tidied.overlap_compared,
        began.elapsed()
    );
}

/// A board for `--widen` (issue #1294): a net `p` that wants 0.5 mm, drawn from pad 2 of one
/// resistor to pad 1 of another as two straight tracks, 0.15 mm and 0.12 mm wide (two widths,
/// so that they are no collinear pair), and a track of a locked net of no pads 0.35 mm beside
/// the middle of the second. The first has room for the whole width; the second, 0.1 mm of
/// clearance from the locked track, has room for 0.33 mm.
fn narrow() -> (Board, Routes) {
    let led = led();
    let resistor = led
        .parts
        .iter()
        .find(|p| p.refdes == "R1")
        .expect("the LED board's resistor")
        .clone();
    let mut board = led.clone();
    board.name = "narrow".to_string();
    board.parts.clear();
    board.nets.clear();
    let min = led.outline.min;
    board.outline = Rect::new(min, min + Pt::new(15.0, 5.0));
    for (refdes, x) in [("RA", 2.0), ("RB", 12.0)] {
        let mut part = resistor.clone();
        part.refdes = refdes.to_string();
        part.path = refdes.to_lowercase();
        part.at = min + Pt::new(x, 2.0);
        board.parts.push(part);
    }
    let pad = |refdes: &str, number: &str| PadRef {
        refdes: refdes.to_string(),
        number: number.to_string(),
    };
    board.nets.push(BoardNet {
        name: "p".to_string(),
        pads: vec![pad("RA", "2"), pad("RB", "1")],
    });
    board.nets.push(BoardNet {
        name: "q".to_string(),
        pads: Vec::new(),
    });
    board.net_widths = vec![("p".to_string(), 0.5)];
    let mut p = straight(CopperLayer::Front, &[[2.51, 2.0], [7.0, 2.0]]);
    let mut second = straight(CopperLayer::Front, &[[7.0, 2.0], [11.49, 2.0]]);
    second[0].width = 0.12;
    p.extend(second);
    let q = straight(CopperLayer::Front, &[[8.0, 2.35], [10.0, 2.35]]);
    let mut routes = Routes::default();
    let table = routes.boards.entry(board.name.clone()).or_default();
    for (net, tracks, lock) in [("p", p, false), ("q", q, true)] {
        table.nets.insert(
            net.to_string(),
            NetRoutes {
                pads: netsmith_pcb::pad_hash(&board, net).expect("a net of the board"),
                lock,
                tracks,
                vias: Vec::new(),
                zones: Vec::new(),
            },
        );
    }
    (board, routes)
}

#[test]
fn widen_makes_each_narrow_track_as_wide_as_its_net_wants_or_its_neighbours_allow() {
    let (board, routes) = narrow();
    let widths = |t: &Tidied| -> Vec<f64> {
        t.routes.boards["narrow"].nets["p"]
            .tracks
            .iter()
            .map(|t| t.width)
            .collect()
    };
    // Without `--widen` nothing is widened.
    let plain =
        routes_tidy::tidy_passes(&board, &routes, None, routes_tidy::Checking::Near, routes_tidy::Widen::Off)
            .unwrap();
    assert_eq!(plain.counts.widened, 0, "{}", plain.text());
    assert_eq!(widths(&plain), vec![0.15, 0.12]);
    for checking in [routes_tidy::Checking::Near, routes_tidy::Checking::Whole] {
        let tidied = routes_tidy::tidy_passes(&board, &routes, None, checking, routes_tidy::Widen::ToNet).unwrap();
        let said = tidied.text();
        assert_eq!(tidied.counts.widened, 2, "{said}");
        assert!(tidied.refused.is_empty(), "{said}");
        // The first to the whole width; the second refused at the whole width and at three
        // quarters of the way (0.40 mm), and made half the way (0.31 mm).
        assert_eq!(widths(&tidied), vec![0.5, 0.31], "{said}");
        assert!(
            said.contains("to the 0.500 mm its net wants")
                && said.contains("to 0.310 mm, of the 0.500 mm its net wants"),
            "{said}"
        );
        assert!(said.contains("2 narrow track(s) widened"), "{said}");
        // The locked track is not touched.
        assert_eq!(tidied.routes.boards["narrow"].nets["q"], routes.boards["narrow"].nets["q"]);
        assert_eq!(tidied.findings_after, tidied.findings_before, "{said}");
    }
    assert_eq!(
        routes_tidy::wanted_width(&board, "p", CopperLayer::Front),
        0.5
    );
    assert_eq!(
        routes_tidy::wanted_width(&board, "q", CopperLayer::Front),
        board.rules.track_width
    );
}

/// One net of `n` straight tracks of 1 mm along one line (the widths alternate, so that no two
/// make a collinear pair) from a resistor to another, with a track of 0.5 mm inside the middle
/// one, which the overlap pass removes (issue 1492).
fn long(n: usize) -> (Board, Routes) {
    let led = led();
    let resistor = led
        .parts
        .iter()
        .find(|p| p.refdes == "R1")
        .expect("the LED board's resistor")
        .clone();
    let mut board = led.clone();
    board.name = "long".to_string();
    board.parts.clear();
    board.nets.clear();
    let min = led.outline.min;
    let end = n as f64 + 3.02;
    board.outline = Rect::new(min, min + Pt::new(end + 3.0, 5.0));
    for (refdes, x) in [("RA", 2.0), ("RB", end)] {
        let mut part = resistor.clone();
        part.refdes = refdes.to_string();
        part.path = refdes.to_lowercase();
        part.at = min + Pt::new(x, 2.0);
        board.parts.push(part);
    }
    let pad = |refdes: &str, number: &str| PadRef {
        refdes: refdes.to_string(),
        number: number.to_string(),
    };
    board.nets.push(BoardNet {
        name: "p".to_string(),
        pads: vec![pad("RA", "2"), pad("RB", "1")],
    });
    let mut tracks: Vec<RouteTrack> = (0..n)
        .map(|i| {
            let x = 2.51 + i as f64;
            RouteTrack {
                layer: CopperLayer::Front,
                width: if i % 2 == 0 { 0.15 } else { 0.16 },
                from: [x, 2.0],
                mid: None,
                to: [x + 1.0, 2.0],
                lock: false,
            }
        })
        .collect();
    let middle = 2.51 + (n / 2) as f64;
    tracks.extend(straight(
        CopperLayer::Front,
        &[[middle + 0.2, 2.0], [middle + 0.7, 2.0]],
    ));
    let mut routes = Routes::default();
    routes.boards.entry(board.name.clone()).or_default().nets.insert(
        "p".to_string(),
        NetRoutes {
            pads: netsmith_pcb::pad_hash(&board, "p").expect("a net of the board"),
            lock: false,
            tracks,
            vias: Vec::new(),
            zones: Vec::new(),
        },
    );
    (board, routes)
}

#[test]
fn the_overlap_pass_compares_a_track_with_its_neighbours_and_not_with_its_net() {
    // Four times the tracks of one net: the pairs of a net would be sixteen times as many,
    // the tracks near each track are as many on either board.
    let run = |n: usize| {
        let (board, routes) = long(n);
        let tidied = routes_tidy::tidy(&board, &routes, None).unwrap();
        let said = tidied.text();
        assert_eq!(tidied.counts.overlaps, 1, "{said}");
        assert_eq!(tidied.counts.merges, 0, "{said}");
        assert!(tidied.refused.is_empty(), "{said}");
        eprintln!("{n} tracks: {} pairs compared", tidied.overlap_compared);
        tidied.overlap_compared
    };
    let (small, big) = (run(100), run(400));
    assert!(small > 0, "the pass compared nothing");
    let ratio = big as f64 / small as f64;
    assert!(
        ratio < 6.0,
        "the pairs compared grow {ratio:.2} times for 4 times the tracks: {small} then {big}"
    );
    assert!(big < 20 * 400, "{big} pairs compared for 400 tracks");
}

/// A board for `--widen-to-guideline` (issue #1611): a net `p` of 1 A on a 1.2 V rail (a budget
/// of 12 mV) from pad 2 of one resistor to pad 1 of another as three straight tracks, 0.4 mm,
/// 0.09 mm (1 mm long) and 0.4 mm wide. The 0.4 mm tracks drop about 9.8 mV and the 0.09 mm
/// one 5.5 mV, so the path is over its budget and the guideline suggests 0.23 mm for the thin
/// track; the net wants 0.1 mm (the rules' track width). With `neighbour`, a track of a
/// locked net of no pads runs beside the thin one with 0.1025 mm of clearance, room for none
/// of the steps.
fn over_budget(neighbour: bool) -> (Board, Routes) {
    let (mut board, _) = narrow();
    board.net_widths.clear();
    board.net_currents = vec![netsmith_core::NetCurrent {
        net: "p".to_string(),
        current: 1.0,
        least_width: 0.1,
        vias: 1,
        ..Default::default()
    }];
    let mut p = Vec::new();
    for (from, to, width) in [(2.51, 6.5, 0.4), (6.5, 7.5, 0.09), (7.5, 11.49, 0.4)] {
        let mut t = straight(CopperLayer::Front, &[[from, 2.0], [to, 2.0]]);
        t[0].width = width;
        p.extend(t);
    }
    let mut q = straight(CopperLayer::Front, &[[6.3, 2.1975], [7.7, 2.1975]]);
    q[0].width = 0.1;
    let mut routes = Routes::default();
    let table = routes.boards.entry(board.name.clone()).or_default();
    let mut nets = vec![("p", p, false)];
    if neighbour {
        nets.push(("q", q, true));
    }
    for (net, tracks, lock) in nets {
        table.nets.insert(
            net.to_string(),
            NetRoutes {
                pads: netsmith_pcb::pad_hash(&board, net).expect("a net of the board"),
                lock,
                tracks,
                vias: Vec::new(),
                zones: Vec::new(),
            },
        );
    }
    (board, routes)
}

fn widths_of(t: &Tidied) -> Vec<f64> {
    t.routes.boards["narrow"].nets["p"]
        .tracks
        .iter()
        .map(|t| t.width)
        .collect()
}

fn to_guideline(volts: f64) -> routes_tidy::Widen {
    routes_tidy::Widen::ToGuideline(vec![("p".to_string(), volts)])
}

#[test]
fn widen_to_guideline_widens_the_thin_track_of_a_net_over_its_drop_budget_where_there_is_room() {
    let (board, routes) = over_budget(false);
    // `--widen` alone widens the thin track to the width its net wants, as before.
    let plain = routes_tidy::tidy_passes(
        &board,
        &routes,
        None,
        routes_tidy::Checking::Near,
        routes_tidy::Widen::ToNet,
    )
    .unwrap();
    assert_eq!(widths_of(&plain), vec![0.4, 0.1, 0.4], "{}", plain.text());
    assert!(plain.drops.is_empty() && !plain.widen_to_guideline);
    for checking in [routes_tidy::Checking::Near, routes_tidy::Checking::Whole] {
        let tidied =
            routes_tidy::tidy_passes(&board, &routes, None, checking, to_guideline(1.2)).unwrap();
        let said = tidied.text();
        assert_eq!(widths_of(&tidied), vec![0.4, 0.23, 0.4], "{said}");
        assert_eq!(tidied.counts.widened, 1, "{said}");
        assert!(tidied.refused.is_empty(), "{said}");
        let c = &tidied.changes[0];
        assert_eq!((c.width_before, c.width_after), (Some(0.09), Some(0.23)));
        assert!(
            said.contains("widened from 0.090 mm to the 0.230 mm the track-drop guideline suggests"),
            "{said}"
        );
        let [d] = tidied.drops.as_slice() else {
            panic!("one net over its budget: {said}")
        };
        assert_eq!(d.net, "p");
        assert!(d.before > 0.015 && d.after <= 0.012 && !d.over, "{d:?}");
        assert!(said.contains("net p: ") && said.contains(", within"), "{said}");
        // The thin track's own finding (narrower than the rules' least track) is gone.
        assert!(tidied.findings_after < tidied.findings_before, "{said}");
        let json = serde_json::to_value(&tidied).unwrap();
        assert_eq!(json["changes"][0]["width_after"], 0.23);
        assert_eq!(json["drops"][0]["net"], "p");
    }
}

#[test]
fn widen_to_guideline_leaves_a_thin_track_with_no_room_and_says_why() {
    let (board, routes) = over_budget(true);
    let tidied = routes_tidy::tidy_passes(
        &board,
        &routes,
        None,
        routes_tidy::Checking::Near,
        to_guideline(1.2),
    )
    .unwrap();
    let said = tidied.text();
    assert_eq!(widths_of(&tidied), vec![0.4, 0.09, 0.4], "{said}");
    assert_eq!(tidied.counts.widened, 0, "{said}");
    let [r] = tidied.refused.as_slice() else {
        panic!("the thin track refused: {said}")
    };
    assert_eq!(r.kind, routes_tidy::Kind::Widen);
    assert!(r.why.is_some(), "{said}");
    let [d] = tidied.drops.as_slice() else {
        panic!("one net over its budget: {said}")
    };
    assert!(d.over && (d.after - d.before).abs() < 1e-12, "{d:?}");
    assert!(said.contains(", still over"), "{said}");
    assert_eq!(tidied.routes.boards["narrow"].nets["q"], routes.boards["narrow"].nets["q"]);
}

#[test]
fn widen_to_guideline_leaves_a_net_within_its_budget_as_widen_does() {
    let (board, routes) = over_budget(false);
    // On a 3 V rail the budget is 30 mV: the net is within it.
    let tidied = routes_tidy::tidy_passes(
        &board,
        &routes,
        None,
        routes_tidy::Checking::Near,
        to_guideline(3.0),
    )
    .unwrap();
    let said = tidied.text();
    assert_eq!(widths_of(&tidied), vec![0.4, 0.1, 0.4], "{said}");
    assert!(tidied.drops.is_empty(), "{said}");
    assert!(said.contains("track drop: no net was over its budget"), "{said}");
}
