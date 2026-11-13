//! The STEP model of a part on disk (`.model`, ADR 0108): its file is found relative to the file
//! that states it and must stay inside the project; a file that is not there is a warning, and
//! the part keeps its generated shape. Nothing reads the file yet, and it never gives a height.

use std::{fs, path::Path};

use netsmith::models::{Places, locate};
use netsmith_check::Loader;
use netsmith_kicad::testlib::TempDir;
use netsmith_solve::Catalogs;

const LED: &str = include_str!("../../../examples/led.nsm");

/// A part of the project's own, in a folder of its own, that names its model.
const USB: &str = "\
/// An invented receptacle.
part Usb {
    .designator &= \"J\"
    .p1: Pin
    .model.file &= \"models/usb.step\"
    .model.rz &= 90
}
";

/// A project with the LED board in `src/main.nsm` and `physical` as its physical file.
fn project(label: &str, physical: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"led\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), LED).unwrap();
    fs::write(
        root.join("src/main.physical.nsm"),
        format!("physical for LedExample {{\n{physical}\n}}\n"),
    )
    .unwrap();
    root
}

/// The models of the LED board in a project at `root`, found on disk.
fn located(root: &Path) -> (netsmith_core::Design, netsmith::models::Located) {
    let mut loader = Loader::std();
    let physical = fs::read_to_string(root.join("src/main.physical.nsm")).unwrap();
    loader.attach_physical("src/main.nsm", "src/main.physical.nsm", &physical);
    let outcome = netsmith::outcome_with("src/main.nsm", LED, &loader, &Catalogs::jlcpcb(), None)
        .unwrap_or_else(|p| panic!("{}", netsmith::render_problems(&p)));
    let mut design = outcome.boards[0].design.clone();
    let found = locate(&mut design, &Places::under(root));
    (design, found)
}

#[test]
fn a_model_is_found_relative_to_the_file_that_states_it() {
    let root = project(
        "model-found",
        "    .power_in.model.file &= \"../models/header.step\"\n    .power_in.model.z &= 1mm",
    );
    fs::create_dir_all(root.join("models")).unwrap();
    fs::write(root.join("models/header.step"), "ISO-10303-21;\n").unwrap();
    let (design, found) = located(&root);
    assert_eq!(found, netsmith::models::Located::default());
    let models = &design.features.models;
    assert_eq!(models.len(), 1, "{models:?}");
    assert_eq!(models[0].path, "power_in");
    assert_eq!(models[0].offset, [0.0, 0.0, 1.0]);
    assert_eq!(models[0].source, "src/main.physical.nsm:2");
    assert_eq!(
        models[0].resolved.as_deref(),
        Some(
            std::path::absolute(root.join("models/header.step"))
                .unwrap()
                .as_path()
        )
    );
}

#[test]
fn a_model_that_is_not_there_is_a_warning_and_the_part_keeps_its_shape() {
    let root = project(
        "model-missing",
        "    .power_in.model.file &= \"../models/header.step\"",
    );
    let (design, found) = located(&root);
    assert!(found.errors.is_empty(), "{:?}", found.errors);
    assert_eq!(
        found.warnings,
        [
            "`power_in.model.file` names `models/header.step` (src/main.physical.nsm:2), and there is no such file, so `power_in` is drawn with its generated shape; add the file, or leave it out on a machine that may not have it"
        ]
    );
    assert_eq!(design.features.models[0].resolved, None);
}

#[test]
fn a_model_outside_the_project_is_refused() {
    let root = project(
        "model-outside",
        "    .power_in.model.file &= \"../../header.step\"",
    );
    let (_, found) = located(&root);
    assert_eq!(found.errors.len(), 1, "{:?}", found.errors);
    assert!(
        found.errors[0].starts_with(
            "`power_in.model.file` is `../../header.step` (src/main.physical.nsm:2), which is `"
        ) && found.errors[0].contains("outside the project"),
        "{}",
        found.errors[0]
    );
}

#[test]
fn an_absolute_model_path_is_refused() {
    let root = project(
        "model-absolute",
        "    .power_in.model.file &= \"/tmp/header.step\"",
    );
    let (_, found) = located(&root);
    assert_eq!(
        found.errors,
        [
            "`power_in.model.file` is `/tmp/header.step`, an absolute path (src/main.physical.nsm:2); name the STEP file relative to the file that states it, inside the project"
        ]
    );
}

#[test]
fn one_path_stated_in_two_folders_that_names_two_files_is_refused() {
    let root = TempDir::new("model-two-folders");
    let board = "import src::parts::usb::Usb\n\nboard B {\n    .usb: Usb\n    .usb.model.file &= \"models/usb.step\"\n}\n";
    let mut loader = Loader::std();
    loader.insert("src::parts::usb", "src/parts/usb.nsm", USB);
    let outcome = netsmith::outcome_with("src/main.nsm", board, &loader, &Catalogs::jlcpcb(), None)
        .unwrap_or_else(|p| panic!("{}", netsmith::render_problems(&p)));
    let mut design = outcome.boards[0].design.clone();
    let found = locate(&mut design, &Places::under(&root));
    assert_eq!(
        found.errors,
        [
            "`usb.model.file` is `models/usb.step` in each of src/parts/usb.nsm:5, src/main.nsm:5, and relative to each of those files it names a different file (`src/parts/models/usb.step`, `src/models/usb.step`): state it once, or with a path that names one file"
        ]
    );
}
