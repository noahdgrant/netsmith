//! The project's model table (`[models] folder` and `[[model]]` rows of `netsmith.toml`, issue
//! #763): a part that states no `.model.file` is drawn from the model of its footprint, a row
//! of the footprint's id first, else KiCad's layout of the models folder. A part's own
//! `.model.file` wins, and an empty one says it has none. Nothing here reads a model: the
//! files are found, not drawn.

use std::{fs, path::Path};

use netsmith_check::Loader;
use netsmith_core::Design;
use netsmith_fabricate::models::{Located, ModelRow, Places, locate};
use netsmith_kicad::Mapping;
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

const LED: &str = include_str!("../../../examples/led.nsm");

/// The LED board, with `physical` as its physical file.
fn design(physical: &str) -> Design {
    let mut loader = Loader::std();
    loader.attach_physical(
        "src/main.nsm",
        "src/main.physical.nsm",
        &format!("physical for LedExample {{\n{physical}\n}}\n"),
    );
    let outcome = netsmith_pipeline::outcome_with(
        "src/main.nsm",
        LED,
        &loader,
        &Catalogs::jlcpcb(),
        None,
    )
    .unwrap_or_else(|p| panic!("{}", netsmith_pipeline::render_problems(&p)));
    outcome.boards[0].design.clone()
}

/// The footprint id the standard mapping gives the part at `path` of `design`.
fn footprint(design: &Design, path: &str) -> String {
    let part = design.parts.iter().find(|p| p.path == path).unwrap();
    Mapping::standard()
        .map_part(&part.path, &part.kind, part.package.as_deref())
        .unwrap()
        .footprint
}

/// Write an empty STEP file at `root/rel`.
fn touch(root: &Path, rel: &str) {
    let file = root.join(rel);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, "ISO-10303-21;\n").unwrap();
}

fn row(footprint: &str, file: &str) -> ModelRow {
    ModelRow {
        footprint: footprint.to_string(),
        file: file.to_string(),
        offset: [0.0; 3],
        rotation: [0.0; 3],
    }
}

/// The models of `design` found with the table `folder` and `rows` of a project at `root`.
fn located(
    mut design: Design,
    root: &Path,
    folder: Option<&str>,
    rows: Vec<ModelRow>,
) -> (Design, Located) {
    let places = Places::under(root).with_table(folder, rows);
    let found = locate(&mut design, &places, Some(&Mapping::standard()));
    (design, found)
}

#[test]
fn a_row_gives_each_part_of_its_footprint_the_model() {
    let root = TempDir::new("model-table-row");
    let d = design("");
    let fp = footprint(&d, "r1");
    touch(&root, "models/r.step");
    let mut r = row(&fp, "models/r.step");
    r.rotation = [0.0, 0.0, 90.0];
    let (d, found) = located(d, &root, None, vec![r]);
    assert!(found.errors.is_empty() && found.warnings.is_empty(), "{found:?}");
    let m = d.features.models.iter().find(|m| m.path == "r1").unwrap();
    assert_eq!(m.file, "models/r.step");
    assert_eq!(m.rotation, [0.0, 0.0, 90.0]);
    assert_eq!(m.footprint.as_deref(), Some(fp.as_str()));
    assert_eq!(
        m.resolved.as_deref(),
        Some(std::path::absolute(root.join("models/r.step")).unwrap().as_path())
    );
    // The note counts the parts drawn and names the ones with no model.
    assert_eq!(found.notes.len(), 1, "{found:?}");
    let note = &found.notes[0];
    assert!(
        note.starts_with("the model table (netsmith.toml) draws 1 part from a STEP model; 2 parts have no model"),
        "{note}"
    );
    assert!(note.contains("`led` (") && note.contains("`power_in` ("), "{note}");
}

#[test]
fn the_folder_gives_a_footprint_its_file_in_kicads_layout() {
    let root = TempDir::new("model-table-folder");
    let d = design("");
    let fp = footprint(&d, "r1");
    let (lib, name) = fp.split_once(':').unwrap();
    touch(&root, &format!("lib3d/{lib}.3dshapes/{name}.step"));
    let (d, found) = located(d, &root, Some("lib3d"), Vec::new());
    assert!(found.errors.is_empty() && found.warnings.is_empty(), "{found:?}");
    let m = d.features.models.iter().find(|m| m.path == "r1").unwrap();
    assert_eq!(m.file, format!("lib3d/{lib}.3dshapes/{name}.step"));
    assert_eq!(m.source, format!("netsmith.toml, the models folder of `{fp}`"));
    // A footprint the folder has no file for is looked for, and is no warning.
    assert!(!d.features.models.iter().any(|m| m.path == "led"));
}

#[test]
fn a_part_that_states_its_model_wins_and_an_empty_file_says_none() {
    let root = TempDir::new("model-table-override");
    let base = design("");
    let (r1, led) = (footprint(&base, "r1"), footprint(&base, "led"));
    touch(&root, "models/r.step");
    touch(&root, "models/led.step");
    touch(&root, "src/own.step");
    let d = design("    .r1.model.file &= \"own.step\"\n    .led.model.file &= \"\"");
    let rows = vec![row(&r1, "models/r.step"), row(&led, "models/led.step")];
    let (d, found) = located(d, &root, None, rows);
    assert!(found.errors.is_empty() && found.warnings.is_empty(), "{found:?}");
    let of = |path: &str| -> Vec<&netsmith_core::StepModel> {
        d.features.models.iter().filter(|m| m.path == path).collect()
    };
    assert_eq!(of("r1").len(), 1);
    assert_eq!(of("r1")[0].file, "own.step");
    assert_eq!(of("r1")[0].footprint, None);
    assert_eq!(of("led").len(), 1);
    assert_eq!(of("led")[0].file, "");
    assert_eq!(of("led")[0].resolved, None);
    assert!(
        found.notes[0].contains("`led` (`.model.file` is empty)"),
        "{:?}",
        found.notes
    );
}

#[test]
fn a_placement_with_no_file_replaces_the_numbers_of_the_rows_model() {
    // The part states `.rz` and `.z` and no file: it keeps the row's file and the row's other
    // numbers, and the members it states take the place of the row's (issue #1005).
    let root = TempDir::new("model-table-placement");
    let base = design("");
    let fp = footprint(&base, "r1");
    touch(&root, "models/r.step");
    let mut r = row(&fp, "models/r.step");
    r.offset = [0.5, 0.25, 0.0];
    r.rotation = [90.0, 0.0, 180.0];
    let d = design("    .r1.model.rz &= 90\n    .r1.model.z &= 0.1mm");
    let (d, found) = located(d, &root, None, vec![r]);
    assert!(found.errors.is_empty() && found.warnings.is_empty(), "{found:?}");
    let models: Vec<_> = d.features.models.iter().filter(|m| m.path == "r1").collect();
    assert_eq!(models.len(), 1, "{:?}", d.features.models);
    let m = models[0];
    assert_eq!(m.file, "models/r.step");
    assert_eq!(m.offset, [0.5, 0.25, 0.1]);
    assert_eq!(m.rotation, [90.0, 0.0, 90.0]);
    assert_eq!(m.placement_only, None);
    assert_eq!(m.footprint.as_deref(), Some(fp.as_str()));
    assert!(m.resolved.is_some());
    assert!(m.source.contains("placed by"), "{}", m.source);
    // The note counts the part as drawn from the table.
    assert!(found.notes[0].starts_with("the model table (netsmith.toml) draws 1 part"));
}

#[test]
fn a_placement_with_no_file_over_the_folders_model_replaces_zero() {
    let root = TempDir::new("model-table-placement-folder");
    let d = design("    .r1.model.rz &= 90");
    let fp = footprint(&d, "r1");
    let (lib, name) = fp.split_once(':').unwrap();
    touch(&root, &format!("lib3d/{lib}.3dshapes/{name}.step"));
    let (d, found) = located(d, &root, Some("lib3d"), Vec::new());
    assert!(found.errors.is_empty() && found.warnings.is_empty(), "{found:?}");
    let m = d.features.models.iter().find(|m| m.path == "r1").unwrap();
    assert_eq!(m.rotation, [0.0, 0.0, 90.0]);
    assert_eq!(m.file, format!("lib3d/{lib}.3dshapes/{name}.step"));
}

#[test]
fn a_placement_with_no_file_and_no_model_in_the_table_is_an_error() {
    let root = TempDir::new("model-table-placement-none");
    // No table at all: the old message.
    let (_, found) = located(design("    .r1.model.rz &= 90"), &root, None, Vec::new());
    let mut places = Places::under(&root);
    let mut d = design("    .r1.model.rz &= 90");
    let alone = locate(&mut d, &places, Some(&Mapping::standard()));
    assert_eq!(
        alone.errors,
        ["`r1.model.rz` is stated, and `r1.model.file` is not, so there is no STEP model to place: name the file with `.r1.model.file &= \"models/part.step\"`, or remove the placement"]
    );
    // A table with nothing for the part says so.
    assert_eq!(found.errors.len(), 1, "{found:?}");
    assert!(
        found.errors[0].starts_with("`r1.model.rz` is stated, and `r1.model.file` is not, and the model table (netsmith.toml) has no model for the part's footprint, so there is no STEP model to place"),
        "{found:?}"
    );
    // No mapping, no table's model.
    places = places.with_table(None, vec![row("A:B", "x.step")]);
    let mut d = design("    .r1.model.x &= 1mm\n    .r1.model.y &= 2mm");
    let none = locate(&mut d, &places, None);
    assert!(
        none.errors[0].starts_with("`r1.model.x`, `r1.model.y` are stated"),
        "{none:?}"
    );
}

#[test]
fn a_row_whose_file_is_not_there_is_one_warning_and_the_parts_keep_their_shape() {
    let root = TempDir::new("model-table-missing");
    let d = design("");
    let fp = footprint(&d, "r1");
    let (d, found) = located(d, &root, None, vec![row(&fp, "models/r.step")]);
    assert!(found.errors.is_empty(), "{found:?}");
    assert_eq!(
        found.warnings,
        [format!(
            "the `[[model]]` row of `{fp}` (netsmith.toml) names `models/r.step`, and there is no such file, so `r1` is drawn with its generated shape; add the file, or leave it out on a machine that may not have it"
        )]
    );
    assert!(d.features.models.is_empty());
}

#[test]
fn a_row_outside_the_project_and_the_folder_is_an_error() {
    let root = TempDir::new("model-table-outside");
    let d = design("");
    let fp = footprint(&d, "r1");
    let (_, found) = located(d, &root, None, vec![row(&fp, "../elsewhere/r.step")]);
    assert_eq!(found.errors.len(), 1, "{found:?}");
    assert!(
        found.errors[0].contains("outside the project")
            && found.errors[0].contains("and the models folder"),
        "{found:?}"
    );
}

#[test]
fn a_folder_of_a_variable_that_is_not_set_is_one_warning() {
    let root = TempDir::new("model-table-variable");
    let var = "NETSMITH_TEST_MODEL_TABLE_UNSET_DIR";
    assert!(std::env::var_os(var).is_none());
    let (d, found) = located(design(""), &root, Some(&format!("${{{var}}}")), Vec::new());
    assert_eq!(
        found.warnings,
        [format!(
            "the models folder `${{{var}}}` (netsmith.toml `[models] folder`) names the environment variable `{var}`, which is not set, so no part is drawn from it; parts it would give a model keep their generated shape"
        )]
    );
    assert!(d.features.models.is_empty());
}

#[test]
fn a_design_file_leaves_the_tables_models_out() {
    let root = TempDir::new("model-table-design-file");
    let d = design("    .led.model.file &= \"\"");
    let fp = footprint(&d, "r1");
    touch(&root, "models/r.step");
    let (d, _) = located(d, &root, None, vec![row(&fp, "models/r.step")]);
    assert_eq!(d.features.models.len(), 2);
    let json = serde_json::to_value(&d).unwrap();
    let models = json["features"]["models"].as_array().unwrap();
    assert_eq!(models.len(), 1, "{models:?}");
    assert_eq!(models[0]["path"], "led");
}

#[test]
fn the_manifest_reads_and_writes_the_table() {
    use netsmith_fabricate::project::Manifest;
    let text = "[project]\nname = \"a\"\n\n[models]\nfolder = \"${KICAD9_3DMODEL_DIR}\"\n\n[[model]]\nfootprint = \"Package_SO:VSSOP-10_3x3mm_P0.5mm\"\nfile = \"${KICAD9_3DMODEL_DIR}/Package_SO.3dshapes/MSOP-10_3x3mm_P0.5mm.step\"\nrotation = [0, 0, 90]\n";
    let m = Manifest::parse(text).unwrap();
    assert_eq!(
        m.models.as_ref().and_then(|c| c.folder.as_deref()),
        Some("${KICAD9_3DMODEL_DIR}")
    );
    assert_eq!(m.model.len(), 1);
    assert_eq!(m.model[0].rotation, [0.0, 0.0, 90.0]);
    assert_eq!(Manifest::parse(&m.render()).unwrap(), m);
    let two = format!(
        "{text}\n[[model]]\nfootprint = \"Package_SO:VSSOP-10_3x3mm_P0.5mm\"\nfile = \"x.step\"\n"
    );
    assert_eq!(
        Manifest::parse(&two).unwrap_err(),
        "netsmith.toml: [[model]] footprint = \"Package_SO:VSSOP-10_3x3mm_P0.5mm\" has two rows: give each footprint one model"
    );
    assert_eq!(
        Manifest::parse("[project]\nname = \"a\"\n\n[[model]]\nfootprint = \"R_0402\"\nfile = \"r.step\"\n")
            .unwrap_err(),
        "netsmith.toml: [[model]] footprint = \"R_0402\" is not a footprint id: write it as `kicad.toml` maps a part, `Library:Name`"
    );
}
