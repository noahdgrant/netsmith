//! `board build --portable` and `board fab --portable` write a self-contained KiCad project
//! (issue #1198): the libraries and 3D models the KiCad files refer to are copied into the board's
//! folder, and the library tables and the footprints' `(model ...)` name them by paths inside it.
//! The tests read the tables and the footprints of the folder and check that each path resolves
//! relative to the folder and names a file that is there. They need no KiCad: the libraries are
//! the sample ones of `netsmith-testkit` (`NETSMITH_KICAD_SYMBOLS` and `_FOOTPRINTS`, with the
//! sample 3D models in the `3dmodels` folder beside them, where KiCad has its own).

use std::{fs, path::Path, process::Command};

use netsmith_testkit::{TempDir, assert_self_contained};

const LED: &str = include_str!("../../../examples/led.nsm");

/// A project with the LED board in `src/main.nsm`; with `model`, its header drawn from a STEP file
/// of the project.
fn project(label: &str, model: bool) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"led\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), LED).unwrap();
    if model {
        fs::create_dir_all(root.join("models")).unwrap();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../scripts/editor-verify-js/fixtures/model_3d.models/header.step"),
            root.join("models/header.step"),
        )
        .unwrap();
        fs::write(
            root.join("src/main.physical.nsm"),
            "physical for LedExample {\n    .power_in.model.file &= \"../models/header.step\"\n}\n",
        )
        .unwrap();
    }
    root
}

fn netsmith(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env("NETSMITH_KICAD_SYMBOLS", netsmith_testkit::sample_symbols())
        .env(
            "NETSMITH_KICAD_FOOTPRINTS",
            netsmith_testkit::sample_footprints(),
        )
        .env_remove("KICAD9_3DMODEL_DIR")
        .env_remove("KICAD8_3DMODEL_DIR")
        .env_remove("KICAD_3DMODEL_DIR")
        .output()
        .unwrap()
}

/// The absolute folders of the machine that no file of the project may name.
fn machine(root: &Path) -> Vec<String> {
    vec![
        root.display().to_string(),
        netsmith_testkit::sample_root().display().to_string(),
    ]
}

#[test]
fn build_portable_writes_a_folder_that_names_nothing_outside_it() {
    let root = project("portable-build", false);
    let run = netsmith(&root, &["board", "build", "--portable", "--out", "out"]);
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{err}");
    let folder = root.join("out/LedExample");
    let machine = machine(&root);
    let found = assert_self_contained(
        &folder,
        &machine.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    // The libraries the parts' symbols and footprints come from are in the folder, with tables.
    assert!(!found.libraries.is_empty(), "{found:?}");
    for (table, _, uri) in &found.libraries {
        assert!(uri.starts_with("${KIPRJMOD}/"), "{table}: {uri}");
    }
    // The footprints netsmith's generators make are its own, so there is no KiCad notice, and
    // their models, which are KiCad's, are not in the sample's `3dmodels`: a warning each.
    assert!(
        folder
            .join("libs/Resistor_SMD.pretty/R_0402_1005Metric.kicad_mod")
            .is_file()
    );
    assert!(!folder.join("LICENSES.md").exists());
    assert!(err.contains("warning: ") && err.contains("3D model"), "{err}");
    assert!(folder.join("LedExample.kicad_sch").is_file());
}

#[test]
fn a_portable_folder_can_be_moved_and_still_resolves() {
    let root = project("portable-moved", false);
    let run = netsmith(&root, &["board", "build", "--portable", "--out", "out"]);
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let elsewhere = TempDir::new("portable-moved-to");
    let moved = elsewhere.join("sent");
    fs::rename(root.join("out/LedExample"), &moved).unwrap();
    assert_self_contained(&moved, &[]);
}

#[test]
fn without_the_flag_the_folder_is_as_before() {
    let root = project("portable-off", false);
    let run = netsmith(&root, &["board", "build", "--out", "out"]);
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let folder = root.join("out/LedExample");
    assert!(!folder.join("libs").exists());
    assert!(!folder.join("models").exists());
    assert!(!folder.join("LICENSES.md").exists());
}

#[test]
fn fab_portable_gives_the_board_file_the_models_of_its_parts_inside_the_folder() {
    let root = project("portable-fab", true);
    let run = netsmith(&root, &["board", "fab", "--portable", "--out", "fab"]);
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{err}");
    let folder = root.join("fab/LedExample");
    let machine = machine(&root);
    let found = assert_self_contained(
        &folder,
        &machine.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    // The header is drawn from the project's STEP file, copied in; the board file names it.
    let pcb = fs::read_to_string(folder.join("LedExample.kicad_pcb")).unwrap();
    assert!(pcb.contains("${KIPRJMOD}/models/project/header.step"), "{pcb}");
    assert_eq!(
        fs::read(folder.join("models/project/header.step")).unwrap(),
        fs::read(root.join("models/header.step")).unwrap()
    );
    assert!(
        found
            .models
            .iter()
            .any(|(file, _)| file == "LedExample.kicad_pcb"),
        "{found:?}"
    );
    // A model that was not found is in the notes (the generated footprints' models are KiCad's,
    // and the sample has none), and the package is complete.
    let notes = fs::read_to_string(folder.join("fab-notes.txt")).unwrap();
    assert!(notes.contains("warning: ") && notes.contains("3D model"), "{notes}");
}

#[test]
fn project_order_does_not_take_the_flag() {
    let root = project("portable-order", false);
    let run = netsmith(&root, &["project", "order", "--portable"]);
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("--portable"));
}
