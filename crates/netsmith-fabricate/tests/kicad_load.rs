//! KiCad opens the board file `fab` writes (issue #1221): every example with planes or pours is
//! fabricated, and `kicad-cli` (found as `netsmith_fab::KicadCliExe::locate_runnable` does; the
//! test skips where KiCad is not installed) reads the `.kicad_pcb`, with its fill in each zone,
//! and exports its position file. KiCad 9.0.0 refused a whole board ("Failed to load board") for
//! a line longer than its reader takes, a plane's fill written on one line; the board that holds
//! that construct is netsmith-router's `kicad_drc.rs`
//! (`kicad_opens_the_board_fab_writes_when_a_planes_fill_has_more_points_than_a_line_holds`).
//!
//! Each example is copied to a folder of its own first, since `fab` writes the seed it placed
//! with to the layout file. The single-file examples are routed; the projects, which take the
//! router minutes, are written with the copper the router lays before its timeout of none (what
//! the zones are filled round is then the pads, the outline and the cutouts).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use netsmith_fab::{KicadCli as _, KicadCliExe};
use netsmith_fabricate::fab::{FabOptions, fab_with_options};
use netsmith_fabricate::project::Target;
use netsmith_kicad::Sexp;
use netsmith_pipeline::Libraries;
use netsmith_testkit::TempDir;

fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            copy_dir(&path, &to.join(entry.file_name()));
        } else {
            fs::copy(&path, to.join(entry.file_name())).unwrap();
        }
    }
}

/// The `.kicad_pcb` files `fab` writes under `dir`.
fn boards_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(boards_under(&path));
        } else if path.extension().is_some_and(|e| e == "kicad_pcb") {
            found.push(path);
        }
    }
    found
}

/// Fabricates the example `name` (a file of examples/, `four_layer`, or a project folder,
/// `orbit`) in a copy, checks that the board file holds a fill, and has KiCad open it.
fn kicad_opens(name: &str) {
    let dir = TempDir::new(&format!("kicad-opens-{name}"));
    let source = dir.join("source");
    let project = examples().join(name);
    let (target, route_timeout) = if project.is_dir() {
        copy_dir(&project, &source);
        (Target::resolve_from(&source, None), Some(Duration::ZERO))
    } else {
        fs::create_dir_all(&source).unwrap();
        for file in fs::read_dir(examples()).unwrap().flatten() {
            let file_name = file.file_name();
            let file_name = file_name.to_string_lossy();
            if file_name.split('.').next() == Some(name) && file.path().is_file() {
                fs::copy(file.path(), source.join(&*file_name)).unwrap();
            }
        }
        let file = format!("{name}.nsm");
        (
            Target::resolve_from(&source, Some(&file)),
            FabOptions::default().route_timeout,
        )
    };
    let target = target.unwrap_or_else(|e| panic!("{name}: {e}"));
    let loader = target.loader().unwrap_or_else(|e| panic!("{name}: {e}"));
    let catalogs = target.catalogs().unwrap_or_else(|e| panic!("{name}: {e}"));
    let root = target.project.as_ref().map(|_| source.as_path());
    let libraries = Libraries::for_project(root).unwrap_or_else(|e| panic!("{name}: {e}"));
    let out = dir.join("fab");
    for file in &target.files {
        let text = fs::read_to_string(&file.path).unwrap();
        // A board the router left open is written all the same (and the run fails): the
        // file is what is checked here.
        let _ = fab_with_options(
            &file.name,
            &text,
            &loader,
            &out,
            &catalogs,
            &libraries,
            None,
            &FabOptions {
                route_timeout,
                layout: Some(netsmith_fabricate::layout::layout_path(&file.path)),
                glb: false,
                step: false,
                pdf: false,
                ..FabOptions::default()
            },
        );
    }
    let boards = boards_under(&out);
    assert!(!boards.is_empty(), "{name}: fab wrote no .kicad_pcb");
    for pcb in &boards {
        let text = fs::read_to_string(pcb).unwrap();
        let tree = Sexp::parse(&text).unwrap();
        let filled = tree
            .find_all("zone")
            .filter(|z| z.find("filled_polygon").is_some())
            .count();
        assert!(filled > 0, "{}: no zone holds a fill", pcb.display());
        assert!(
            netsmith_kicad::longest_line(&text) <= netsmith_kicad::KICAD_LINE_MAX,
            "{}: a line longer than KiCad reads",
            pcb.display()
        );
    }
    let Some(cli) = KicadCliExe::locate_runnable() else {
        eprintln!("skipped: no KiCad");
        return;
    };
    for pcb in &boards {
        if let Err(e) = cli.pos(pcb) {
            panic!("KiCad cannot open {}: {e}", pcb.display());
        }
    }
}

/// The examples whose boards have planes or pours: those whose process names a net for a
/// layer, or that are made on more than two layers (which have planes by default).
const POURED: [&str; 8] = [
    "four_layer",
    "six_layer",
    "ten_layer",
    "outline_and_pours",
    "easyduino_esp32s3",
    "orbit",
    "power_manifold_charger",
    "rp2350_minimal",
];

#[test]
fn every_example_with_planes_or_pours_is_one_kicad_opens_here() {
    // A board's source that states a plane or a pour: a layer of its process given a net, or a
    // process of more than two layers. A new example with one is added to `POURED`.
    let states = |text: &str| {
        text.contains(".net &=")
            || ["FourLayer", "SixLayer", "TenLayer"]
                .iter()
                .any(|p| text.contains(&format!(".process: {p}")))
    };
    let mut found = Vec::new();
    for entry in fs::read_dir(examples()).unwrap().flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let sources: Vec<PathBuf> = if path.is_dir() {
            fs::read_dir(path.join("src"))
                .into_iter()
                .chain(fs::read_dir(&path))
                .flatten()
                .flatten()
                .map(|e| e.path())
                .collect()
        } else {
            vec![path.clone()]
        };
        let stated = sources.iter().any(|s| {
            s.extension().is_some_and(|e| e == "nsm")
                && fs::read_to_string(s).is_ok_and(|t| states(&t))
        });
        if stated {
            found.push(name.trim_end_matches(".nsm").to_string());
        }
    }
    found.sort();
    let mut listed: Vec<String> = POURED.iter().map(|s| s.to_string()).collect();
    listed.sort();
    assert_eq!(found, listed);
}

#[test]
fn kicad_opens_the_board_of_four_layer() {
    kicad_opens("four_layer");
}

#[test]
fn kicad_opens_the_board_of_six_layer() {
    kicad_opens("six_layer");
}

#[test]
fn kicad_opens_the_board_of_ten_layer() {
    kicad_opens("ten_layer");
}

#[test]
fn kicad_opens_the_board_of_outline_and_pours() {
    kicad_opens("outline_and_pours");
}

#[test]
fn kicad_opens_the_board_of_easyduino_esp32s3() {
    kicad_opens("easyduino_esp32s3");
}

#[test]
fn kicad_opens_the_board_of_orbit() {
    kicad_opens("orbit");
}

#[test]
fn kicad_opens_the_board_of_power_manifold_charger() {
    kicad_opens("power_manifold_charger");
}

#[test]
fn kicad_opens_the_board_of_rp2350_minimal() {
    kicad_opens("rp2350_minimal");
}
