//! A board's case model on disk (ADR 0109): its file is found relative to the physical file
//! that states it and must stay inside the project; a file that is not there leaves the fit
//! unchecked, a warning, and an error from `fab` when constraints are strict. Nothing reads
//! the file yet.

use std::{fs, path::Path};

use netsmith::models::{Located, Places, locate};
use netsmith_check::Loader;
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

const LED: &str = include_str!("../../../examples/led.nsm");

/// A project with the LED board in `src/main.nsm` and `case` in the `case { }` section of its
/// physical file.
fn project(label: &str, case: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"led\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), LED).unwrap();
    fs::write(
        root.join("src/main.physical.nsm"),
        format!("import std::pcb::CaseModel\n\nphysical for LedExample {{\n    case {{\n{case}\n    }}\n}}\n"),
    )
    .unwrap();
    root
}

/// The LED board of a project at `root`, with its case models found on disk.
fn located(root: &Path) -> (netsmith_core::Design, Located) {
    let mut loader = Loader::std();
    let physical = fs::read_to_string(root.join("src/main.physical.nsm")).unwrap();
    loader.attach_physical("src/main.nsm", "src/main.physical.nsm", &physical);
    let outcome = netsmith::outcome_with("src/main.nsm", LED, &loader, &Catalogs::jlcpcb(), None)
        .unwrap_or_else(|p| panic!("{}", netsmith::render_problems(&p)));
    let mut design = outcome.boards[0].design.clone();
    let found = locate(&mut design, &Places::under(root));
    (design, found)
}

const SHELL: &str = "        .shell: CaseModel {\n            .file &= \"../case/box.step\"\n            .z &= -1.6mm\n            .rx &= 180\n        }";

#[test]
fn a_case_model_is_found_relative_to_the_physical_file() {
    let root = project("case-found", SHELL);
    fs::create_dir_all(root.join("case")).unwrap();
    fs::write(root.join("case/box.step"), "ISO-10303-21;\n").unwrap();
    let (design, found) = located(&root);
    assert_eq!(found, Located::default());
    let cases = &design.features.case_models;
    assert_eq!(cases.len(), 1, "{cases:?}");
    assert_eq!(cases[0].name, "shell");
    assert_eq!(cases[0].offset, [0.0, 0.0, -1.6]);
    assert_eq!(cases[0].rotation, [180.0, 0.0, 0.0]);
    assert_eq!(cases[0].source, "src/main.physical.nsm:6");
    assert_eq!(
        cases[0].resolved.as_deref(),
        Some(
            std::path::absolute(root.join("case/box.step"))
                .unwrap()
                .as_path()
        )
    );
}

#[test]
fn a_case_model_that_is_not_there_leaves_the_fit_unchecked() {
    let root = project("case-missing", SHELL);
    let (design, found) = located(&root);
    assert!(found.errors.is_empty(), "{:?}", found.errors);
    assert!(found.warnings.is_empty(), "{:?}", found.warnings);
    assert_eq!(
        found.unchecked,
        [
            "`shell.file` names `case/box.step` (src/main.physical.nsm:6), and there is no such file, so the board's fit in its case model `shell` is not checked; add the file, or leave it out on a machine that may not have it"
        ]
    );
    assert_eq!(design.features.case_models[0].resolved, None);
}

#[test]
fn a_case_model_outside_the_project_is_refused() {
    let root = project(
        "case-outside",
        "        .shell: CaseModel {\n            .file &= \"../../box.step\"\n        }",
    );
    let (_, found) = located(&root);
    assert_eq!(found.errors.len(), 1, "{:?}", found.errors);
    assert!(
        found.errors[0]
            .starts_with("`shell.file` is `../../box.step` (src/main.physical.nsm:6), which is `")
            && found.errors[0].contains("outside the project"),
        "{}",
        found.errors[0]
    );
    let root = project(
        "case-absolute",
        "        .shell: CaseModel {\n            .file &= \"/tmp/box.step\"\n        }",
    );
    let (_, found) = located(&root);
    assert_eq!(
        found.errors,
        [
            "`shell.file` is `/tmp/box.step`, an absolute path (src/main.physical.nsm:6); name the STEP file relative to the file that states it, inside the project"
        ]
    );
}
