//! A self-contained KiCad project (issue #1198): the folder `board build --portable` writes holds
//! every library entry and 3D model its KiCad files refer to, with relative library tables, and
//! opens in KiCad wherever it is moved to. Each test reads the tables and the footprints of the
//! folder (`netsmith_testkit::assert_self_contained`) and needs no KiCad: the libraries are the
//! sample ones of `netsmith-testkit`, and the project's own library is a folder of the test.

use std::fs;
use std::path::Path;

use netsmith_check::Loader;
use netsmith_core::StepModel;
use netsmith_pipeline::{Libraries, build_with};
use netsmith_solve::Catalogs;
use netsmith_testkit::{TempDir, assert_self_contained};

use crate::common;

/// The project at `proj` ([`common::project_libraries`], whose library is `proj/libs/acme.*`),
/// with a 3D model in its footprint `acme:DIP-8`, built into `out`; the folder of the board, and
/// what the project_files says.
fn written(
    proj: &Path,
    out: &Path,
    models: Option<std::path::PathBuf>,
) -> (std::path::PathBuf, netsmith_pipeline::portable::Portable) {
    let libraries = common::project_libraries(proj);
    let mod_file = proj.join("libs/acme.pretty/DIP-8.kicad_mod");
    let text = fs::read_to_string(&mod_file).unwrap();
    let text = text.replacen(
        "(fp_rect",
        "(model \"${KIPRJMOD}/models/dip8.step\" (offset (xyz 0 0 0)) (scale (xyz 1 1 1)) (rotate (xyz 0 0 0)))\n    (fp_rect",
        1,
    );
    fs::write(&mod_file, text).unwrap();
    fs::create_dir_all(proj.join("models")).unwrap();
    fs::write(proj.join("models/dip8.step"), "ISO-10303-21; a dip\n").unwrap();
    let built = build_with(
        "t.nsm",
        common::DUAL_BOARD,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0);
    let folder = built.write(out).unwrap();
    let mut project_files = libraries.portable_with(&folder, models);
    project_files.libraries(&built.design).unwrap();
    (folder, project_files.finish().unwrap())
}

#[test]
fn the_folder_holds_its_libraries_and_models_with_relative_tables() {
    let proj = TempDir::new("portable-project");
    let out = TempDir::new("portable-out");
    let (folder, portable) = written(
        proj.path(),
        out.path(),
        Some(netsmith_testkit::sample_models()),
    );
    assert!(portable.warnings.is_empty(), "{:?}", portable.warnings);

    let found = assert_self_contained(
        &folder,
        &[
            &proj.path().display().to_string(),
            &netsmith_testkit::sample_root().display().to_string(),
        ],
    );
    let libraries: Vec<(&str, &str)> = found
        .libraries
        .iter()
        .map(|(table, name, _)| (table.as_str(), name.as_str()))
        .collect();
    // The project's own library, and the sample (KiCad's) ones the parts' symbols and footprints
    // come from, each with a file of its own in the folder.
    for expected in [
        ("sym-lib-table", "acme"),
        ("fp-lib-table", "acme"),
        ("sym-lib-table", "Connector_Generic"),
        ("fp-lib-table", "Connector_PinHeader_2.54mm"),
    ] {
        assert!(libraries.contains(&expected), "{expected:?} in {libraries:?}");
    }
    assert!(folder.join("libs/acme.kicad_sym").is_file());
    assert!(folder.join("libs/acme.pretty/DIP-8.kicad_mod").is_file());

    // The project's model is copied under `models/project/`, and the KiCad footprint's model
    // keeps its place under KiCad's model folder.
    assert!(
        found
            .models
            .iter()
            .any(|(file, path)| file.ends_with("DIP-8.kicad_mod")
                && path == "${KIPRJMOD}/models/project/dip8.step"),
        "{:?}",
        found.models
    );
    assert_eq!(
        fs::read(folder.join("models/project/dip8.step")).unwrap(),
        fs::read(proj.path().join("models/dip8.step")).unwrap()
    );
    let header = "models/PinHeader_1x02_P2.54mm_Vertical.wrl";
    assert_eq!(
        fs::read(folder.join(header)).unwrap(),
        fs::read(netsmith_testkit::sample_models().join("PinHeader_1x02_P2.54mm_Vertical.wrl"))
            .unwrap()
    );
}

#[test]
fn the_notice_lists_what_came_from_kicad_and_nothing_of_the_projects() {
    let proj = TempDir::new("portable-notice-project");
    let out = TempDir::new("portable-notice-out");
    let (folder, _) = written(
        proj.path(),
        out.path(),
        Some(netsmith_testkit::sample_models()),
    );
    let notice = fs::read_to_string(folder.join("LICENSES.md")).unwrap();
    assert!(notice.contains("CC-BY-SA 4.0"), "{notice}");
    assert!(notice.contains("design"), "{notice}");
    assert!(
        notice.contains("`Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical`"),
        "{notice}"
    );
    assert!(notice.contains("`PinHeader_1x02_P2.54mm_Vertical.wrl`"), "{notice}");
    assert!(!notice.contains("acme"), "{notice}");
    assert!(!notice.contains("dip8.step"), "{notice}");
}

#[test]
fn a_model_that_cannot_be_found_is_a_warning_and_the_footprint_has_no_model() {
    let proj = TempDir::new("portable-missing-project");
    let out = TempDir::new("portable-missing-out");
    // No KiCad model folder at all.
    let (folder, portable) = written(proj.path(), out.path(), None);
    assert!(
        portable
            .warnings
            .iter()
            .any(|w| w.contains("PinHeader_1x02_P2.54mm_Vertical")
                && w.contains("not in the project folder")),
        "{:?}",
        portable.warnings
    );
    let found = assert_self_contained(&folder, &[&proj.path().display().to_string()]);
    // The project's own model is still copied; the header's is not named any more.
    assert!(
        found
            .models
            .iter()
            .all(|(file, _)| !file.contains("PinHeader")),
        "{:?}",
        found.models
    );
    assert!(folder.join("models/project/dip8.step").is_file());
}

#[test]
fn a_part_with_a_step_model_of_its_own_gets_it_in_the_board_file() {
    let proj = TempDir::new("portable-pcb-project");
    let out = TempDir::new("portable-pcb-out");
    let libraries = Libraries::sample();
    let folder = out.path().join("Board");
    fs::create_dir_all(&folder).unwrap();
    let step = proj.path().join("usb.step");
    fs::write(&step, "ISO-10303-21; a usb\n").unwrap();
    let pcb = "(kicad_pcb (version 20241229)\n\
        (footprint \"Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical\" (layer \"F.Cu\") (at 1 2)\n\
          (property \"netsmith_path\" \"j\" (at 0 0 0) (layer \"F.Fab\") (hide yes))\n\
          (model \"${KICAD9_3DMODEL_DIR}/PinHeader_1x02_P2.54mm_Vertical.wrl\" (offset (xyz 0 0 0)) (scale (xyz 1 1 1)) (rotate (xyz 0 0 0))))\n\
        (footprint \"Package_TO_SOT_SMD:SOT-23\" (layer \"F.Cu\") (at 5 5)\n\
          (property \"netsmith_path\" \"q\" (at 0 0 0) (layer \"F.Fab\") (hide yes))\n\
          (model \"${KICAD9_3DMODEL_DIR}/SOT-23.wrl\" (offset (xyz 0 0 0)) (scale (xyz 1 1 1)) (rotate (xyz 0 0 0)))))\n";
    let model = |path: &str, file: &Path, offset, rotation| StepModel {
        path: path.to_string(),
        file: "usb.step".to_string(),
        offset,
        rotation,
        source: String::new(),
        stated_in: Vec::new(),
        resolved: Some(file.to_path_buf()),
        footprint: None,
        placement_only: None,
    };
    let mut project_files = libraries.portable_with(&folder, Some(netsmith_testkit::sample_models()));
    let text = project_files
        .pcb(pcb, &[model("j", &step, [0.5, 0.0, -0.2], [0.0, 0.0, 90.0])])
        .unwrap();
    let portable = project_files.finish().unwrap();
    assert!(portable.warnings.is_empty(), "{:?}", portable.warnings);
    fs::write(folder.join("Board.kicad_pcb"), &text).unwrap();

    let found = assert_self_contained(&folder, &[&proj.path().display().to_string()]);
    // The header has the part's model with the design's numbers, in place of the library's; the
    // transistor keeps the library's, copied in.
    assert!(text.contains("${KIPRJMOD}/models/project/usb.step"), "{text}");
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains("(offset (xyz 0.5 0 -0.2) )") && flat.contains("(rotate (xyz 0 0 90) )"),
        "{flat}"
    );
    assert!(!text.contains("PinHeader_1x02_P2.54mm_Vertical.wrl"), "{text}");
    assert!(text.contains("${KIPRJMOD}/models/SOT-23.wrl"), "{text}");
    assert_eq!(found.models.len(), 2, "{:?}", found.models);
    assert_eq!(
        fs::read(folder.join("models/project/usb.step")).unwrap(),
        fs::read(&step).unwrap()
    );
}

#[test]
fn two_different_files_of_one_name_are_both_kept() {
    let proj = TempDir::new("portable-same-name");
    let out = TempDir::new("portable-same-name-out");
    let libraries = Libraries::sample();
    let folder = out.path().join("Board");
    fs::create_dir_all(proj.path().join("a")).unwrap();
    fs::create_dir_all(proj.path().join("b")).unwrap();
    fs::write(proj.path().join("a/m.step"), "one").unwrap();
    fs::write(proj.path().join("b/m.step"), "two").unwrap();
    let footprint = |path: &str| {
        format!(
            "(footprint \"L:X\" (layer \"F.Cu\") (at 0 0)\n(property \"netsmith_path\" \"{path}\" (at 0 0 0) (layer \"F.Fab\") (hide yes)))\n"
        )
    };
    let pcb = format!("(kicad_pcb (version 20241229)\n{}{})\n", footprint("a"), footprint("b"));
    let model = |path: &str, file: std::path::PathBuf| StepModel {
        path: path.to_string(),
        file: "m.step".to_string(),
        offset: [0.0; 3],
        rotation: [0.0; 3],
        source: String::new(),
        stated_in: Vec::new(),
        resolved: Some(file),
        footprint: None,
        placement_only: None,
    };
    let mut project_files = libraries.portable_with(&folder, None);
    project_files
        .pcb(
            &pcb,
            &[
                model("a", proj.path().join("a/m.step")),
                model("b", proj.path().join("b/m.step")),
            ],
        )
        .unwrap();
    let portable = project_files.finish().unwrap();
    assert_eq!(
        portable.files,
        ["models/project/m-2.step", "models/project/m.step"]
    );
    assert_eq!(fs::read_to_string(folder.join("models/project/m.step")).unwrap(), "one");
    assert_eq!(fs::read_to_string(folder.join("models/project/m-2.step")).unwrap(), "two");
}

#[test]
fn a_board_built_with_the_installed_kicad_libraries_is_self_contained_too() {
    // Skips where KiCad's libraries are not installed (the test needs the real ones).
    if netsmith_kicad::skip_without_kicad_libraries() {
        return;
    }
    let libraries = Libraries::standard();
    let built = build_with(
        "led.nsm",
        include_str!("../../../examples/led.nsm"),
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0);
    let out = TempDir::new("portable-installed");
    let folder = built.write(out.path()).unwrap();
    let models = netsmith_pipeline::portable::kicad_models(libraries.footprints.root());
    let mut project_files = libraries.portable(&folder);
    project_files.libraries(&built.design).unwrap();
    let portable = project_files.finish().unwrap();
    let found = assert_self_contained(&folder, &[]);
    assert!(!found.libraries.is_empty());
    // KiCad's 3D models are a package of their own and are not always installed: without them
    // each model is a warning (with them, a footprint's model may still be one KiCad lacks).
    if models.is_none() {
        assert!(
            portable.warnings.iter().all(|w| w.contains("3D model")),
            "{:?}",
            portable.warnings
        );
    }
}
