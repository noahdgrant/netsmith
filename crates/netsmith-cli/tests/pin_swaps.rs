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

use netsmith::Libraries;
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_pcb::FootprintLibrary;
use netsmith_testkit::{TempDir, sample_footprints, sample_symbols};

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

/// A project of the swaps board in `root`: the source, its physical file, its libraries, and a pin
/// file that holds the requests on `p00` to `p09` in order, none locked (what a first assignment
/// gives, and what the optimizer would change on a board that does not route).
pub fn project(root: &Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    libraries(root);
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"swaps\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), circuit(&mirror())).unwrap();
    fs::write(
        root.join("src/main.physical.nsm"),
        physical(WIDTH, HEIGHT, MCU_Y),
    )
    .unwrap();
    let mut pins = String::from("version = 1\n");
    for k in 1..=10 {
        pins.push_str(&format!(
            "\n[Swaps.pins.g{k:02}]\npin = \"p{:02}\"\nfunction = \"GPIO\"\nsource = \"auto\"\n",
            k - 1
        ));
    }
    fs::write(root.join("src/main.pins.toml"), pins).unwrap();
}

pub fn netsmith(root: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(root)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env("NETSMITH_KICAD_SYMBOLS", netsmith_testkit::sample_symbols())
        .env(
            "NETSMITH_KICAD_FOOTPRINTS",
            netsmith_testkit::sample_footprints(),
        )
        .output()
        .unwrap()
}

/// What is in `dir` and below, by path and bytes, for "nothing was written".
fn tree(dir: &Path, base: &Path, into: &mut Vec<(String, Vec<u8>)>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            tree(&path, base, into);
        } else {
            let name = path.strip_prefix(base).unwrap().display().to_string();
            into.push((name, fs::read(&path).unwrap()));
        }
    }
}

#[test]
fn fab_locked_never_rewrites_the_pin_file_of_a_board_that_does_not_route() {
    let root = TempDir::new("pin-swaps-cli-locked");
    project(root.path());
    let pin_file = root.join("src/main.pins.toml");
    let before = fs::read(&pin_file).unwrap();
    // The first run makes the pick lock that `--locked` wants; `--pins locked` changes no pin.
    let first = netsmith(
        root.path(),
        &["fab", "--allow-drc-violations", "--pins", "locked"],
    );
    assert!(!first.status.success());
    assert_eq!(fs::read(&pin_file).unwrap(), before);
    let mut files = Vec::new();
    tree(&root.join("src"), root.path(), &mut files);

    // `--locked` alone: the board does not route, the optimizer finds a swap, and the run says
    // so and ends in an error, as `--pins locked` does. It writes no pin file and no pick lock.
    let run = netsmith(root.path(), &["fab", "--allow-drc-violations", "--locked"]);
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success(), "{err}");
    assert!(err.contains("9 connections could not be routed"), "{err}");
    // The person gave `--locked`, so the message says `--locked` and never advises dropping a
    // flag that was not passed.
    assert!(err.contains("--locked, so no pin moves"), "{err}");
    assert!(err.contains("or run without --locked"), "{err}");
    assert!(!err.contains("--pins locked"), "{err}");
    assert!(err.contains("pins.proposed.toml"), "{err}");
    assert!(!err.contains("pins: round"), "no round was made: {err}");
    assert_eq!(
        fs::read(&pin_file).unwrap(),
        before,
        "--locked must not change the pin file: {err}"
    );
    let mut after = Vec::new();
    tree(&root.join("src"), root.path(), &mut after);
    assert_eq!(after, files, "--locked changed a source file");
    assert!(root.join("fab/pins.proposed.toml").exists());

    // With `--pins auto` too, `--locked` wins: it is the flag that forbids a change.
    let both = netsmith(
        root.path(),
        &[
            "fab",
            "--allow-drc-violations",
            "--locked",
            "--pins",
            "auto",
        ],
    );
    let err = String::from_utf8_lossy(&both.stderr);
    assert!(!both.status.success(), "{err}");
    assert!(err.contains("--locked, so no pin moves"), "{err}");
    assert!(!err.contains("run without --pins locked"), "{err}");
    assert_eq!(fs::read(&pin_file).unwrap(), before, "{err}");

    // `--pins locked` alone names itself.
    let named = netsmith(
        root.path(),
        &["fab", "--allow-drc-violations", "--pins", "locked"],
    );
    let err = String::from_utf8_lossy(&named.stderr);
    assert!(err.contains("--pins locked, so no pin moves"), "{err}");
    assert!(err.contains("or run without --pins locked"), "{err}");

    // `pins = "locked"` in the manifest is neither flag: the message says where it comes from,
    // and the way out is the flag that wins over it.
    let manifest = root.join("netsmith.toml");
    let original = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        original.replace("[project]", "[project]\npins = \"locked\""),
    )
    .unwrap();
    let set = netsmith(root.path(), &["fab", "--allow-drc-violations"]);
    let err = String::from_utf8_lossy(&set.stderr);
    assert!(!set.status.success(), "{err}");
    assert!(
        err.contains("`pins = \"locked\"` in netsmith.toml, so no pin moves"),
        "{err}"
    );
    assert!(err.contains("or run with --pins auto"), "{err}");
    assert!(!err.contains("--pins locked, so"), "{err}");
    assert!(!err.contains("run without --locked"), "{err}");
    fs::write(&manifest, original).unwrap();

    // Without it the same board is made routable and the pin file changes: the contrast.
    let free = netsmith(root.path(), &["fab", "--allow-drc-violations"]);
    assert!(
        free.status.success(),
        "{}",
        String::from_utf8_lossy(&free.stderr)
    );
    assert_ne!(fs::read(&pin_file).unwrap(), before);
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
