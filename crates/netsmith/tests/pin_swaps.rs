//! The pin optimizer (ADR 0101, issue #109), measured and then end to end. The board is invented:
//! a 10-pin microcontroller whose pins all offer `GPIO` (no std part states `fn`), a 10-pin
//! connector joined to it in mirror order, every part held in place by the physical file, and
//! the back of the board closed to copper so that crossing lines cannot be resolved with vias.
//! The front is closed too, except for the strip between the rows and the pads' own reach: the
//! ends of the rows and the sides above and below them are keepouts that forbid tracks, so no
//! line can go round a row. That was needed once the router searched in eight directions and
//! routed even the first assignment round the ends of the rows (issue #415).
//! Only the pin map differs between runs. With the pins the first assignment gives (request `gK`
//! on `p0(K-1)`) the 10 nets cross each other 45 times; mirrored they cross none.

use std::fs;
use std::path::Path;
use std::time::Duration;

use std::sync::atomic::{AtomicU32, Ordering};

use netsmith::fab::{FabOptions, Fabricated, PinsMode, PinsUse, fab_with_options};
use netsmith::pin_swap::{Budget, Clock};

use netsmith::Libraries;
use netsmith_check::Loader;
use netsmith_kicad::testlib::{TempDir, sample_footprints, sample_symbols};
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_pcb::{Board, FootprintLibrary, Quality, place_options};
use netsmith_place_route::{RouteOptions, route_with};
use netsmith_solve::Catalogs;

pub const KICAD_TOML: &str = "\
[[library]]
name = \"swap\"
footprints = \"libs/swap.pretty\"

[[part]]
kind = \"Zmcu\"
package = \"Row\"
default = true
footprint = \"swap:Row\"

[[part]]
kind = \"Con10\"
package = \"Row\"
default = true
footprint = \"swap:Con\"
";

/// Ten pads of 0.4 by 1.4 mm, 0.8 mm apart.
pub fn mcu_footprint() -> String {
    let mut text = String::from(
        "(footprint \"Row\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
         (property \"Reference\" \"REF**\" (at 0 -3 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (property \"Value\" \"Row\" (at 0 3 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (attr smd)",
    );
    for i in 0..10 {
        let x = -3.6 + 0.8 * f64::from(i);
        text.push_str(&format!(
            " (pad \"{}\" smd rect (at {x:.2} 0) (size 0.4 1.4) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\"))",
            i + 1
        ));
    }
    text.push_str(
        " (fp_rect (start -4.5 -1.2) (end 4.5 1.2) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n",
    );
    text
}

/// Ten pads on the top face, 2.54 mm apart.
pub fn con_footprint() -> String {
    let mut text = String::from(
        "(footprint \"Con\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
         (property \"Reference\" \"REF**\" (at 0 -3 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (property \"Value\" \"Con\" (at 0 3 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (attr smd)",
    );
    for i in 0..10 {
        let x = -11.43 + 2.54 * f64::from(i);
        text.push_str(&format!(
            " (pad \"{}\" smd rect (at {x:.2} 0) (size 1.7 2.2) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\"))",
            i + 1
        ));
    }
    text.push_str(
        " (fp_rect (start -12.7 -1.5) (end 12.7 1.5) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n",
    );
    text
}

/// The parts of the board, `Zmcu` first, with `fn` lists as no std part has today.
pub const PARTS: &str = "\
/// An invented microcontroller: every pin offers `GPIO`.
part Zmcu {
    .designator &= \"U\"
    peripheral UARTE0 on GPIO { UARTE0_TX:output UARTE0_RX:input }
    pins {
        .p00 bidirectional \"P0.00\" \"1\" fn GPIO
        .p01 bidirectional \"P0.01\" \"2\" fn GPIO
        .p02 bidirectional \"P0.02\" \"3\" fn GPIO
        .p03 bidirectional \"P0.03\" \"4\" fn GPIO
        .p04 bidirectional \"P0.04\" \"5\" fn GPIO
        .p05 bidirectional \"P0.05\" \"6\" fn GPIO
        .p06 bidirectional \"P0.06\" \"7\" fn GPIO
        .p07 bidirectional \"P0.07\" \"8\" fn GPIO
        .p08 bidirectional \"P0.08\" \"9\" fn GPIO
        .p09 bidirectional \"P0.09\" \"10\" fn GPIO
    }
    .package: Package
}

/// An invented ten-pin connector.
part Con10 {
    .designator &= \"J\"
    pins {
        .p1 passive \"1\" \"1\"
        .p2 passive \"2\" \"2\"
        .p3 passive \"3\" \"3\"
        .p4 passive \"4\" \"4\"
        .p5 passive \"5\" \"5\"
        .p6 passive \"6\" \"6\"
        .p7 passive \"7\" \"7\"
        .p8 passive \"8\" \"8\"
        .p9 passive \"9\" \"9\"
        .p10 passive \"10\" \"10\"
    }
    .package: Package
}
";

/// The board: connector pin `k` is joined to the request `gKK`, in the order `join` says.
pub fn circuit(join: &[usize]) -> String {
    let mut text = format!(
        "import std::mfg::jlcpcb::TwoLayer\n\n{PARTS}\nboard Swaps {{\n    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}\n    .u: Zmcu {{ .assembly &= AssemblyMode::Hand }}\n    .j: Con10 {{ .assembly &= AssemblyMode::Hand }}\n"
    );
    for k in 1..=10 {
        text.push_str(&format!("    .g{k:02}: .u.GPIO\n"));
    }
    for (k, j) in join.iter().enumerate() {
        text.push_str(&format!("    .g{:02} ~ .j.p{j}\n", k + 1));
    }
    text.push_str("}\n");
    text
}

/// The physical file: the outline, and the two parts held where they are.
pub fn physical(width: f64, height: f64, mcu_y: f64) -> String {
    format!(
        "import std::pcb::{{Keepout, Outline}}

physical for Swaps {{
    case {{
        .outline: Outline {{
            .width &= {width}mm
            .height &= {height}mm
        }}
    }}
    // The back of the board is reserved: no track, via or copper on it.
    .back: Keepout {{
        .x &= 0mm
        .y &= 0mm
        .width &= {width}mm
        .height &= {height}mm
        .layers &= \"bottom\"
        .tracks &= \"forbid\"
        .vias &= \"forbid\"
        .copper &= \"forbid\"
        .parts &= \"allow\"
    }}
    // The front is closed round the rows (ends, above and below): no track goes round them.
    .left_end: Keepout {{
        .x &= 0mm
        .y &= 0mm
        .width &= {end}mm
        .height &= {height}mm
        .layers &= \"top\"
        .tracks &= \"forbid\"
        .parts &= \"allow\"
    }}
    .right_end: Keepout {{
        .x &= {right}mm
        .y &= 0mm
        .width &= {end}mm
        .height &= {height}mm
        .layers &= \"top\"
        .tracks &= \"forbid\"
        .parts &= \"allow\"
    }}
    .above: Keepout {{
        .x &= 0mm
        .y &= {above}mm
        .width &= {width}mm
        .height &= {above_h}mm
        .layers &= \"top\"
        .tracks &= \"forbid\"
        .parts &= \"allow\"
    }}
    .below: Keepout {{
        .x &= 0mm
        .y &= 0mm
        .width &= {width}mm
        .height &= 2.8mm
        .layers &= \"top\"
        .tracks &= \"forbid\"
        .parts &= \"allow\"
    }}
    .j.place.side &= \"top\"
    .j.place.rotation &= 0
    .j.place.x &= {cx}mm +/- 0.1mm
    .j.place.y &= 4mm +/- 0.1mm
    .u.place.side &= \"top\"
    .u.place.rotation &= 0
    .u.place.x &= {cx}mm +/- 0.1mm
    .u.place.y &= {mcu_y}mm +/- 0.1mm
}}
",
        cx = width / 2.0,
        above = mcu_y + 0.8,
        above_h = (height - mcu_y - 0.8).min(2.7),
        end = width / 2.0 - 12.7,
        right = width / 2.0 + 12.7
    )
}

/// A pin file that locks request `gKK` to pin `p{map[K-1]}`.
pub fn locked_pins(map: &[usize]) -> String {
    let mut text = String::from("version = 1\n");
    for (k, p) in map.iter().enumerate() {
        text.push_str(&format!(
            "\n[Swaps.pins.g{:02}]\npin = \"p{p:02}\"\nfunction = \"GPIO\"\nlock = true\nsource = \"hand\"\n",
            k + 1
        ));
    }
    text
}

/// The project libraries of the board in `dir`: the two footprints, and the sample libraries.
pub fn libraries(dir: &Path) -> Libraries {
    fs::create_dir_all(dir.join("libs/swap.pretty")).unwrap();
    fs::write(dir.join("kicad.toml"), KICAD_TOML).unwrap();
    fs::write(dir.join("libs/swap.pretty/Row.kicad_mod"), mcu_footprint()).unwrap();
    fs::write(dir.join("libs/swap.pretty/Con.kicad_mod"), con_footprint()).unwrap();
    Libraries::with_mapping(
        Mapping::for_project(Some(dir)).unwrap(),
        SymbolLib::at(sample_symbols()),
        FootprintLibrary::at(sample_footprints()),
    )
}

/// The size of the board the tests use: wide and short enough that the router, with no copper
/// on the back, cannot cross the lines the first assignment leaves.
const WIDTH: f64 = 28.0;
const HEIGHT: f64 = 11.0;
const MCU_Y: f64 = 7.5;

/// The connector in mirror order: connector pin `11 - k` is joined to request `gk`.
fn mirror() -> Vec<usize> {
    (1..=10).rev().collect()
}

/// What one pin map gives: the ratsnest before routing and the lines the router left.
#[derive(Debug, PartialEq)]
pub struct Measured {
    pub crossings: usize,
    pub left: usize,
}

/// Place and route the board with the pin file `pins`, as `fab` does, and measure it.
pub fn measure(libraries: &Libraries, join: &[usize], pins: &str) -> Measured {
    let outcome = build(libraries, join, pins);
    let built = &outcome.boards[0];
    let placement = place_options(&built.design, None);
    let mut board = Board::from_design_placed(
        &built.design,
        &libraries.mapping,
        &libraries.footprints,
        &placement,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let quality = Quality::of(&board);
    let routing = route_with(
        &mut board,
        RouteOptions {
            time_limit: Some(Duration::from_secs(120)),
            ..RouteOptions::default()
        },
    );
    Measured {
        crossings: quality.crossings,
        left: routing.left.len(),
    }
}

fn loader(pins: Option<&str>) -> Loader {
    let mut loader = Loader::std().with_physical(
        "swaps.nsm",
        "swaps.physical.nsm",
        &physical(WIDTH, HEIGHT, MCU_Y),
    );
    if let Some(text) = pins {
        loader.attach_pins("swaps.nsm", "swaps.pins.toml", text);
    }
    loader
}

fn build(libraries: &Libraries, join: &[usize], pins: &str) -> netsmith::Outcome {
    netsmith::outcome_with(
        "swaps.nsm",
        &circuit(join),
        &loader(Some(pins)),
        &Catalogs::jlcpcb(),
        Some(libraries),
    )
    .unwrap_or_else(|p| panic!("{p:?}"))
}

#[test]
fn swapping_pins_changes_the_crossings_and_what_the_router_leaves() {
    let dir = TempDir::new("pin-swaps-measure");
    let libs = libraries(dir.path());
    let identity: Vec<usize> = (0..10).collect();
    let mirrored: Vec<usize> = (0..10).rev().collect();
    // The measurement of the pull request: with the pins the first assignment gives, 45 pairs of
    // lines cross and the router leaves 9 of the 10 connections; mirrored, none cross and it routes.
    assert_eq!(
        measure(&libs, &mirror(), &locked_pins(&identity)),
        Measured {
            crossings: 45,
            left: 9
        }
    );
    assert_eq!(
        measure(&libs, &mirror(), &locked_pins(&mirrored)),
        Measured {
            crossings: 0,
            left: 0
        }
    );
}

/// `fab` on the board with `pins` as the pin file's text, if any, and what the run did.
struct Run {
    dir: TempDir,
    result: Result<Vec<Fabricated>, String>,
    pin_file: std::path::PathBuf,
}

fn fab(
    label: &str,
    libs: &Libraries,
    join: &[usize],
    pins: Option<&str>,
    mode: Option<PinsMode>,
) -> Run {
    let dir = TempDir::unused(&format!("pin-swaps-{label}"));
    let pin_file = dir.join("swaps.pins.toml");
    if let Some(text) = pins {
        fs::create_dir_all(dir.path()).unwrap();
        fs::write(&pin_file, text).unwrap();
    }
    let result = fab_with_options(
        "swaps.nsm",
        &circuit(join),
        &loader(pins),
        &dir.join("fab"),
        &Catalogs::jlcpcb(),
        libs,
        None,
        &FabOptions {
            route_timeout: Some(Duration::from_secs(120)),
            pins: mode.map(|mode| PinsUse::new(mode, pin_file.clone())),
            ..FabOptions::default()
        },
    );
    Run {
        dir,
        result,
        pin_file,
    }
}

fn pin_of(file: &str, request: &str) -> String {
    let table = format!("[Swaps.pins.{request}]\npin = \"");
    let at = file
        .find(&table)
        .unwrap_or_else(|| panic!("{request}: {file}"));
    file[at + table.len()..]
        .split('"')
        .next()
        .unwrap()
        .to_string()
}

#[test]
fn a_board_that_does_not_route_routes_after_a_swap_and_the_file_says_so() {
    let dir = TempDir::new("pin-swaps-libs");
    let libs = libraries(dir.path());
    // As before: without the optimizer the first assignment leaves a connection.
    let plain = fab("off", &libs, &mirror(), None, None);
    let e = plain
        .result
        .expect_err("the first assignment does not route");
    assert!(e.contains("9 connections could not be routed"), "{e}");
    assert!(!plain.pin_file.exists());

    let run = fab("auto", &libs, &mirror(), None, Some(PinsMode::Auto));
    let made = run.result.unwrap_or_else(|e| panic!("{e}"));
    let file = fs::read_to_string(&run.pin_file).expect("the pin file is written");
    // The requests were on p00..p09 in order; the optimizer moved them.
    assert!(
        (1..=10).any(|k| pin_of(&file, &format!("g{k:02}")) != format!("p{:02}", k - 1)),
        "{file}"
    );
    // The fab notes lead with the change, before every other note: a changed pin is a change
    // of the firmware.
    let notes = fs::read_to_string(run.dir.join("fab/Swaps/fab-notes.txt")).unwrap();
    let said = notes
        .find("pin map changed by this run, so the firmware's pin map changes: Swaps.g")
        .unwrap_or_else(|| panic!("{notes}"));
    assert!(said < notes.find("hand-solder:").unwrap(), "{notes}");
    assert!(made[0].notes.contains("pin map changed by this run"));
    // The next run starts from the file, routes, and leaves the file's bytes alone.
    let stamp = fs::metadata(&run.pin_file).unwrap().modified().unwrap();
    let again = fab("again", &libs, &mirror(), Some(&file), Some(PinsMode::Auto));
    again.result.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(fs::read_to_string(&again.pin_file).unwrap(), file);
    assert_eq!(
        fs::metadata(&run.pin_file).unwrap().modified().unwrap(),
        stamp
    );
    let notes = fs::read_to_string(again.dir.join("fab/Swaps/fab-notes.txt")).unwrap();
    assert!(!notes.contains("pin map changed"), "{notes}");
}

#[test]
fn the_same_board_gives_the_same_pin_file_every_time() {
    let dir = TempDir::new("pin-swaps-libs-det");
    let libs = libraries(dir.path());
    let a = fab("det-a", &libs, &mirror(), None, Some(PinsMode::Auto));
    let b = fab("det-b", &libs, &mirror(), None, Some(PinsMode::Auto));
    a.result.unwrap_or_else(|e| panic!("{e}"));
    b.result.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        fs::read(&a.pin_file).unwrap(),
        fs::read(&b.pin_file).unwrap()
    );
}

#[test]
fn a_locked_pin_stays_where_it_is() {
    let dir = TempDir::new("pin-swaps-libs-lock");
    let libs = libraries(dir.path());
    // `g05` and `g06` are locked on the pins that the first assignment gives them (and that the
    // mirror would swap); every other request is free.
    let mut pins = String::from("version = 1\n");
    for (k, p) in [(5, 4), (6, 5)] {
        pins.push_str(&format!(
            "\n[Swaps.pins.g{k:02}]\npin = \"p{p:02}\"\nfunction = \"GPIO\"\nlock = true\nsource = \"hand\"\n"
        ));
    }
    let run = fab("lock", &libs, &mirror(), Some(&pins), Some(PinsMode::Auto));
    // Two pins that cannot move cannot be mirrored, so the board may or may not route; the
    // locks hold either way, and so does the file's text for them.
    let _ = run.result;
    let file = fs::read_to_string(&run.pin_file).unwrap();
    assert_eq!(pin_of(&file, "g05"), "p04", "{file}");
    assert_eq!(pin_of(&file, "g06"), "p05", "{file}");
    assert!(file.contains("lock = true"), "{file}");
    // The other requests were picked by the run and none took a locked pin.
    for k in [1, 2, 3, 4, 7, 8, 9, 10] {
        let pin = pin_of(&file, &format!("g{k:02}"));
        assert!(pin != "p04" && pin != "p05", "g{k:02} is on {pin}: {file}");
    }
}

#[test]
fn locked_moves_no_pin_writes_the_proposal_and_fails_as_before() {
    let dir = TempDir::new("pin-swaps-libs-locked");
    let libs = libraries(dir.path());
    let run = fab("locked", &libs, &mirror(), None, Some(PinsMode::Locked));
    let e = run.result.expect_err("no pin moves, so it does not route");
    assert!(e.contains("9 connections could not be routed"), "{e}");
    assert!(e.contains("--pins locked, so no pin moves"), "{e}");
    assert!(e.contains("or run without --pins locked"), "{e}");
    assert!(!run.pin_file.exists(), "--pins locked writes no pin file");
    let proposed = run.dir.join("fab/pins.proposed.toml");
    let text = fs::read_to_string(&proposed).expect("the proposal is written");
    assert!(text.contains("version = 1"), "{text}");
    assert!(e.contains(&proposed.display().to_string()), "{e}");
    // Taking the proposal makes the board route.
    let body: String = text
        .lines()
        .filter(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let taken = fab(
        "taken",
        &libs,
        &mirror(),
        Some(&body),
        Some(PinsMode::Locked),
    );
    taken.result.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(fs::read_to_string(&taken.pin_file).unwrap(), body);
}

#[test]
fn a_board_with_no_requests_is_made_exactly_as_before() {
    // The same parts and nets with every pin named by the source: nothing to move, with or
    // without the optimizer, and the same failure.
    let dir = TempDir::new("pin-swaps-libs-plain");
    let libs = libraries(dir.path());
    let mut text = format!(
        "import std::mfg::jlcpcb::TwoLayer\n\n{PARTS}\nboard Swaps {{\n    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}\n    .u: Zmcu {{ .assembly &= AssemblyMode::Hand }}\n    .j: Con10 {{ .assembly &= AssemblyMode::Hand }}\n"
    );
    for k in 1..=10 {
        text.push_str(&format!("    .u.p{:02} ~ .j.p{}\n", k - 1, 11 - k));
    }
    text.push_str("}\n");
    let run = |mode: Option<PinsMode>| {
        let dir = TempDir::unused("pin-swaps-plain");
        let pin_file = dir.join("swaps.pins.toml");
        let result = fab_with_options(
            "swaps.nsm",
            &text,
            &loader(None),
            &dir.join("fab"),
            &Catalogs::jlcpcb(),
            &libs,
            None,
            &FabOptions {
                route_timeout: Some(Duration::from_secs(120)),
                pins: mode.map(|mode| PinsUse::new(mode, pin_file.clone())),
                ..FabOptions::default()
            },
        );
        assert!(!pin_file.exists());
        assert!(!dir.join("fab/pins.proposed.toml").exists());
        result.expect_err("with the pins on p00 to p09 in order the board does not route")
    };
    let off = run(None);
    assert_eq!(off, run(Some(PinsMode::Auto)));
    assert_eq!(off, run(Some(PinsMode::Locked)));
}

#[test]
fn a_board_that_cannot_route_whatever_the_pins_stops_after_at_most_three_rounds() {
    assert_eq!(netsmith::pin_swap::MAX_ROUNDS, 3);
    let dir = TempDir::new("pin-swaps-libs-bound");
    let libs = libraries(dir.path());
    // A wall across the board that no copper may cross, and a net that has to: no pin map
    // routes it.
    let text = circuit(&mirror()).replace(
        "    .j: Con10 { .assembly &= AssemblyMode::Hand }\n",
        "    .j: Con10 { .assembly &= AssemblyMode::Hand }\n    .k: Con10 { .assembly &= AssemblyMode::Hand }\n    .k.p1 ~ .j.p1\n",
    );
    let physical = physical(WIDTH, 16.0, MCU_Y).replace(
        "    .j.place.side",
        "    .wall: Keepout {\n        .x &= 0mm\n        .y &= 9.5mm\n        .width &= 28mm\n        .height &= 1mm\n        .parts &= \"allow\"\n    }\n    .k.place.side &= \"top\"\n    .k.place.rotation &= 0\n    .k.place.x &= 14mm +/- 0.1mm\n    .k.place.y &= 13mm +/- 0.1mm\n    .j.place.side",
    );
    let dir2 = TempDir::unused("pin-swaps-bound");
    let result = fab_with_options(
        "swaps.nsm",
        &text,
        &Loader::std().with_physical("swaps.nsm", "swaps.physical.nsm", &physical),
        &dir2.join("fab"),
        &Catalogs::jlcpcb(),
        &libs,
        None,
        &FabOptions {
            route_timeout: Some(Duration::from_secs(120)),
            pins: Some(PinsUse::new(PinsMode::Auto, dir2.join("swaps.pins.toml"))),
            ..FabOptions::default()
        },
    );
    let e = result.expect_err("a net the wall cuts cannot be routed");
    assert!(e.contains("connections could not be routed"), "{e}");
    let tried: usize = e
        .split("pins: tried ")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("the optimizer did not run: {e}"));
    assert!((1..=3).contains(&tried), "{e}");
}

/// The proposal of the search on the board as placed, for the pin file `pins`.
fn proposal(libs: &Libraries, pins: &str) -> Option<netsmith::pin_swap::Proposal> {
    searched(libs, pins, &Budget::unlimited()).0
}

/// The search under `budget`: its proposal and how many candidates it scored.
fn searched(
    libs: &Libraries,
    pins: &str,
    budget: &Budget,
) -> (Option<netsmith::pin_swap::Proposal>, usize) {
    let outcome = build(libs, &mirror(), pins);
    let built = &outcome.boards[0];
    let board = Board::from_design_placed(
        &built.design,
        &libs.mapping,
        &libs.footprints,
        &place_options(&built.design, None),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let mut scored = 0;
    let found = netsmith::pin_swap::improve_counting(
        &outcome.pins.spaces["Swaps"],
        &built.design,
        &board,
        budget,
        &mut scored,
    );
    (found, scored)
}

/// The board of `circuit` with four requests for `ANALOG`, which the first four pins of the part
/// list besides `GPIO`; connector pin `5 - k` is joined to request `aK`, so the first assignment
/// (`a1` on `p00`) crosses its lines.
fn analog_circuit() -> String {
    let mut text = circuit(&[]);
    for k in 0..4 {
        text = text.replace(
            &format!(".p0{k} bidirectional \"P0.0{k}\" \"{}\" fn GPIO", k + 1),
            &format!(
                ".p0{k} bidirectional \"P0.0{k}\" \"{}\" fn GPIO ANALOG",
                k + 1
            ),
        );
    }
    for k in (1..=10).rev() {
        text = text.replace(&format!("    .g{k:02}: .u.GPIO\n"), "");
    }
    let end = text.rfind('}').unwrap();
    text.truncate(end);
    for k in 1..=4 {
        text.push_str(&format!(
            "    .a{k}: .u.ANALOG\n    .a{k} ~ .j.p{}\n",
            5 - k
        ));
    }
    text.push_str("}\n");
    text
}

#[test]
fn the_search_gives_requests_for_a_shared_function_pins_of_their_own() {
    let dir = TempDir::new("pin-swaps-libs-analog");
    let libs = libraries(dir.path());
    let outcome = netsmith::outcome_with(
        "swaps.nsm",
        &analog_circuit(),
        &loader(Some("")),
        &Catalogs::jlcpcb(),
        Some(&libs),
    )
    .unwrap_or_else(|p| panic!("{p:?}"));
    let built = &outcome.boards[0];
    let space = &outcome.pins.spaces["Swaps"];
    let held: Vec<(&str, &str)> = space
        .iter()
        .flat_map(|s| &s.requests)
        .map(|r| (r.key.as_str(), r.pin.as_str()))
        .collect();
    // The check accepted the requests: four ANALOG pins for four requests, the first assignment.
    assert_eq!(
        held,
        [("a1", "p00"), ("a2", "p01"), ("a3", "p02"), ("a4", "p03")]
    );
    let board = Board::from_design_placed(
        &built.design,
        &libs.mapping,
        &libs.footprints,
        &place_options(&built.design, None),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let run = || netsmith::pin_swap::improve(space, &built.design, &board, &Budget::unlimited());
    let found = run().expect("a swap among the analog pins lowers the score");
    assert!(found.after < found.before, "{found:?}");
    // Every pick is an analog pin, and no two requests end on one pin.
    let mut ends: Vec<&str> = held
        .iter()
        .map(|(k, p)| found.picks.get(*k).map_or(*p, |s| s.as_str()))
        .collect();
    assert!(
        ends.iter()
            .all(|p| ["p00", "p01", "p02", "p03"].contains(p)),
        "{ends:?}"
    );
    ends.sort_unstable();
    ends.dedup();
    assert_eq!(ends.len(), 4, "{found:?}");
    assert_eq!(run(), Some(found));
}

/// A clock that moves by `tick` each time it is read, so that a run's time is the number of times
/// it asked, whatever the machine.
fn ticking(tick: Duration) -> Clock {
    let reads = AtomicU32::new(0);
    Clock::from_fn(move || tick * reads.fetch_add(1, Ordering::SeqCst))
}

#[test]
fn the_search_unwinds_the_crossings_the_same_way_every_time() {
    let dir = TempDir::new("pin-swaps-libs-search");
    let libs = libraries(dir.path());
    let first = proposal(&libs, "").expect("a swap lowers the score");
    assert_eq!(first.before.crossings, 45);
    assert_eq!(first.after.crossings, 0);
    assert!(
        first.after.ratsnest_um < first.before.ratsnest_um,
        "{first:?}"
    );
    // The mirror: every request but none of them stays where the first assignment put it.
    assert_eq!(first.picks.len(), 10, "{first:?}");
    assert_eq!(first.picks["g01"], "p09");
    assert_eq!(first.picks["g10"], "p00");
    assert_eq!(proposal(&libs, ""), Some(first));
}

#[test]
fn the_search_leaves_a_locked_entry_and_what_it_cannot_improve_alone() {
    let dir = TempDir::new("pin-swaps-libs-search-lock");
    let libs = libraries(dir.path());
    // `g01` and `g10` are locked where they are, and the other eight are free to move among the
    // eight pins left: none of the moves that mirror all ten is open, but the crossings among
    // the eight can still be cut.
    let mut pins = String::from("version = 1\n");
    for (k, p) in [(1, 0), (10, 9)] {
        pins.push_str(&format!(
            "\n[Swaps.pins.g{k:02}]\npin = \"p{p:02}\"\nfunction = \"GPIO\"\nlock = true\nsource = \"hand\"\n"
        ));
    }
    let found = proposal(&libs, &pins).expect("the eight free requests can still improve");
    assert!(
        !found.picks.contains_key("g01") && !found.picks.contains_key("g10"),
        "{found:?}"
    );
    assert!(found.after < found.before, "{found:?}");
    assert!(
        found.picks.values().all(|p| p != "p00" && p != "p09"),
        "{found:?}"
    );
    // Everything locked: nothing to propose.
    let mut all = String::from("version = 1\n");
    for k in 1..=10 {
        all.push_str(&format!(
            "\n[Swaps.pins.g{k:02}]\npin = \"p{:02}\"\nfunction = \"GPIO\"\nlock = true\nsource = \"hand\"\n",
            k - 1
        ));
    }
    assert_eq!(proposal(&libs, &all), None);
}

#[test]
fn the_search_asks_the_budget_before_every_candidate_it_scores() {
    let dir = TempDir::new("pin-swaps-libs-budget");
    let libs = libraries(dir.path());
    let (full, all) = searched(&libs, "", &Budget::unlimited());
    let full = full.expect("a swap lowers the score");
    // A round scores every move and swap of every request: far more than a handful.
    assert!(all > 20, "{all}");
    // The clock moves one second for each time it is read and the budget is 5 seconds: the start
    // reads it once, then the search reads it before each candidate, so it scores four candidates
    // and stops, though the round has many more. (With the deadline read once for the round the
    // whole round would be scored.)
    let budget = Budget::start(
        Some(Duration::from_secs(5)),
        &ticking(Duration::from_secs(1)),
    );
    let (cut, scored) = searched(&libs, "", &budget);
    assert_eq!(scored, 4);
    // What the four found is kept, and is no better than the full search found.
    let cut = cut.expect("one of the first four candidates lowers the score");
    assert!(cut.after < cut.before, "{cut:?}");
    assert!(cut.after >= full.after, "{cut:?} {full:?}");
    assert!(cut.moves.len() < full.moves.len(), "{cut:?}");
    // A budget that is spent before the search scores nothing and proposes nothing.
    let spent = Budget::start(Some(Duration::ZERO), &Clock::real());
    assert_eq!(searched(&libs, "", &spent), (None, 0));
}

/// The progress lines of a `fab` run, in order.
fn progress_of(lines: &std::sync::Mutex<Vec<String>>) -> Vec<String> {
    lines.lock().unwrap().clone()
}

static PROGRESS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

#[test]
fn the_search_time_is_taken_from_the_budget_before_a_round_routes() {
    let dir = TempDir::new("pin-swaps-libs-order");
    let libs = libraries(dir.path());
    let out = TempDir::unused("pin-swaps-order");
    let pin_file = out.join("swaps.pins.toml");
    PROGRESS.lock().unwrap().clear();
    // The budget is the route timeout, 120 s, and the clock moves 30 s each time it is read: the
    // start, the check before the round and two candidates use it up. The search finds a swap in
    // the candidates it had time for, and then there is no time left to route with it, so no
    // second pass is made. (A budget read before the search would hand the router the time the
    // search has already used, and a second pass would run.)
    let result = fab_with_options(
        "swaps.nsm",
        &circuit(&mirror()),
        &loader(None),
        &out.join("fab"),
        &Catalogs::jlcpcb(),
        &libs,
        None,
        &FabOptions {
            route_timeout: Some(Duration::from_secs(120)),
            progress: Some(|line| PROGRESS.lock().unwrap().push(line.to_string())),
            pins: Some(PinsUse {
                clock: ticking(Duration::from_secs(30)),
                ..PinsUse::new(PinsMode::Auto, pin_file.clone())
            }),
            ..FabOptions::default()
        },
    );
    let e = result.expect_err("the first pass does not route and no round was made");
    assert!(e.contains("9 connections could not be routed"), "{e}");
    let lines = progress_of(&PROGRESS);
    let passes = lines
        .iter()
        .filter(|l| l.contains(": placed on a "))
        .count();
    assert_eq!(passes, 1, "no second pass: {lines:#?}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains("the time budget of the route timeout is spent, no further round")),
        "{lines:#?}"
    );
    // Nothing was tried, so nothing is claimed and the pin file is as the run began.
    assert!(!e.contains("pins: tried"), "{e}");
    assert!(!pin_file.exists());
}
