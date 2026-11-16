//! Dependencies between projects (issue #68): a project's `[dependencies]` names another project
//! by its folder, the modules of that project's `src/` are imported under the entry's name
//! (`shared::rf`), an import of `src::` inside them is of their own project, and their catalogs
//! are read with the project's own. Every part and name is invented.

use std::fs;
use std::path::Path;

use netsmith_fabricate::project::{Project, Target};
use netsmith_pipeline::outcome_with;
use netsmith_testkit::TempDir;

const RF: &str = "\
generic CoaxReceptacle {
    .designator &= \"J\"
    pins {
        .signal passive
        .shield passive
    }
}
";

/// A file of the dependency that imports its own `src/` as `src::`.
const ACME: &str = "\
import src::rf::CoaxReceptacle

part AcmeMicroCoax provides CoaxReceptacle {
    pins {
        .signal passive \"S\" \"1\"
        .shield passive \"G\" \"2\"
    }
}
";

const MAIN: &str = "\
import shared::rf::CoaxReceptacle
import shared::acme::AcmeMicroCoax

board Feed {
    .rf: CoaxReceptacle
}
";

const ROWS: &str = "\
mpn,resistance,max_power,package,library,source
ACME-R316,31.6kohm +/- 1%,62.5mW,0402,Basic,https://example.invalid/acme.pdf
";

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// `root/shared`, a project of modules and a catalog, and `root/app`, which depends on it under
/// the name the manifest of `app` gives (`dependencies`).
fn workspace(dependencies: &str) -> TempDir {
    let root = TempDir::new("dependencies");
    write(
        &root.join("shared/netsmith.toml"),
        "[project]\nname = \"shared-lib\"\ntop_level = []\n",
    );
    write(&root.join("shared/src/rf.nsm"), RF);
    write(&root.join("shared/src/acme.nsm"), ACME);
    write(&root.join("shared/catalogs/acme_r.csv"), ROWS);
    write(
        &root.join("app/netsmith.toml"),
        &format!(
            "[project]\nname = \"app\"\ntop_level = [\"src/main.nsm\"]\n\n[dependencies]\n{dependencies}"
        ),
    );
    write(&root.join("app/src/main.nsm"), MAIN);
    root
}

fn picked(app: &Path) -> Result<String, Vec<String>> {
    let target = Target::resolve_from(app, None).map_err(|e| vec![e])?;
    let file = &target.files[0];
    let text = fs::read_to_string(&file.path).unwrap();
    let loader = target.loader().map_err(|e| vec![e])?;
    let catalogs = target.catalogs().map_err(|e| vec![e])?;
    let outcome = outcome_with(&file.name, &text, &loader, &catalogs, None)
        .map_err(|e| e.iter().map(|p| p.message.clone()).collect::<Vec<_>>())?;
    Ok(outcome.boards[0].design.parts[0].kind.clone())
}

#[test]
fn a_dependencys_modules_are_imported_under_its_name() {
    let root = workspace("shared = { path = \"../shared\" }\n");
    assert_eq!(picked(&root.join("app")).unwrap(), "AcmeMicroCoax");
}

#[test]
fn the_name_of_the_entry_and_not_of_the_project_is_the_first_step() {
    let root = workspace("lib = { path = \"../shared\" }\n");
    write(
        &root.join("app/src/main.nsm"),
        &MAIN.replace("shared::", "lib::"),
    );
    assert_eq!(picked(&root.join("app")).unwrap(), "AcmeMicroCoax");
    // The dependency's own `src::` is not the depending project's.
    let target = Target::resolve_from(&root.join("app"), None).unwrap();
    let loader = target.loader().unwrap();
    let keys: Vec<&str> = loader.modules().into_iter().map(|(k, _, _)| k).collect();
    assert!(keys.contains(&"lib::rf") && keys.contains(&"src::main"), "{keys:?}");
    assert!(!keys.contains(&"src::rf"), "{keys:?}");
    let shown: Vec<&str> = loader.modules().into_iter().map(|(_, f, _)| f).collect();
    assert!(shown.contains(&"../shared/src/rf.nsm"), "{shown:?}");
}

#[test]
fn a_module_of_a_project_with_no_such_dependency_is_unknown() {
    let root = workspace("");
    let errors = picked(&root.join("app")).unwrap_err();
    assert!(
        errors.iter().any(|e| e == "unknown module `shared::rf`"),
        "{errors:?}"
    );
}

#[test]
fn a_dependencys_catalogs_are_read_with_the_projects_own() {
    let root = workspace("shared = { path = \"../shared\" }\n");
    let project = Project::load(&root.join("app")).unwrap();
    let files: Vec<String> = project
        .all_catalog_files()
        .unwrap()
        .into_iter()
        .map(|f| f.file)
        .collect();
    assert_eq!(files, ["../shared/catalogs/acme_r.csv"]);
    // The project's own catalog files are still only its own.
    assert!(project.catalog_files().unwrap().is_empty());
    assert!(project.catalogs().unwrap().contains("acme_r"));
    // A dependency's catalog is watched, so the editor builds again when it changes.
    let target = Target::resolve_from(&root.join("app"), None).unwrap();
    let watched = target.watched();
    assert!(
        watched
            .iter()
            .any(|p| p.ends_with("shared/catalogs/acme_r.csv")),
        "{watched:?}"
    );
    assert!(
        watched.iter().any(|p| p.ends_with("shared/src/rf.nsm")),
        "{watched:?}"
    );
}

#[test]
fn a_catalog_in_the_project_and_in_a_dependency_is_refused_naming_both() {
    let root = workspace("shared = { path = \"../shared\" }\n");
    write(&root.join("app/catalogs/acme_r.csv"), ROWS);
    let project = Project::load(&root.join("app")).unwrap();
    assert_eq!(
        project.catalogs().unwrap_err(),
        "netsmith.toml: the catalog `acme_r` is in two files, catalogs/acme_r.csv and ../shared/catalogs/acme_r.csv; a catalog is one file. Rename or remove one"
    );
}

#[test]
fn a_dependency_of_a_dependency_is_read_too() {
    let root = workspace("shared = { path = \"../shared\" }\n");
    // `shared` moves its generic into a project of its own, `base`.
    write(
        &root.join("shared/netsmith.toml"),
        "[project]\nname = \"shared-lib\"\ntop_level = []\n\n[dependencies]\nbase = { path = \"../base\" }\n",
    );
    fs::remove_file(root.join("shared/src/rf.nsm")).unwrap();
    write(
        &root.join("base/netsmith.toml"),
        "[project]\nname = \"base\"\ntop_level = []\n",
    );
    write(&root.join("base/src/rf.nsm"), RF);
    write(
        &root.join("shared/src/acme.nsm"),
        &ACME.replace("src::rf", "base::rf"),
    );
    write(
        &root.join("app/src/main.nsm"),
        &MAIN.replace("shared::rf", "base::rf"),
    );
    let project = Project::load(&root.join("app")).unwrap();
    let found: Vec<(String, String)> = project
        .dependencies()
        .unwrap()
        .into_iter()
        .map(|d| (d.name, d.folder))
        .collect();
    assert_eq!(
        found,
        [
            ("shared".to_string(), "../shared".to_string()),
            ("base".to_string(), "../base".to_string())
        ]
    );
    assert_eq!(picked(&root.join("app")).unwrap(), "AcmeMicroCoax");
}

#[test]
fn a_folder_that_is_not_a_project_is_refused_naming_the_entry() {
    let root = workspace("shared = { path = \"../missing\" }\n");
    let e = Project::load(&root.join("app"))
        .unwrap()
        .dependencies()
        .unwrap_err();
    assert!(
        e.starts_with(
            "netsmith.toml: [dependencies] shared: ../missing is not a project: it has no netsmith.toml"
        ),
        "{e}"
    );
    // The commands that read modules stop with it.
    let target = Target::resolve_from(&root.join("app"), None).unwrap();
    assert_eq!(target.loader().unwrap_err(), e);
}

#[test]
fn a_project_that_depends_on_itself_is_refused_with_the_chain() {
    let root = workspace("shared = { path = \"../shared\" }\n");
    write(
        &root.join("shared/netsmith.toml"),
        "[project]\nname = \"shared-lib\"\ntop_level = []\n\n[dependencies]\napp = { path = \"../app\" }\n",
    );
    let e = Project::load(&root.join("app"))
        .unwrap()
        .dependencies()
        .unwrap_err();
    assert_eq!(
        e,
        "../shared/netsmith.toml: [dependencies] app: ../app depends on itself (app -> shared -> app); a project cannot depend on itself through others"
    );
}

#[test]
fn two_projects_under_one_name_are_refused() {
    let root = workspace("shared = { path = \"../shared\" }\nother = { path = \"../other\" }\n");
    write(
        &root.join("other/netsmith.toml"),
        "[project]\nname = \"other\"\ntop_level = []\n\n[dependencies]\nshared = { path = \"../elsewhere\" }\n",
    );
    write(
        &root.join("elsewhere/netsmith.toml"),
        "[project]\nname = \"elsewhere\"\ntop_level = []\n",
    );
    // The entries of a manifest are read in the order of their names: `other` (and its
    // `shared`) before this project's `shared`.
    let e = Project::load(&root.join("app"))
        .unwrap()
        .dependencies()
        .unwrap_err();
    assert_eq!(
        e,
        "netsmith.toml: [dependencies] shared: two projects are named `shared`, ../elsewhere and ../shared; the modules of a dependency are imported by its name, so the name is one project's"
    );
}
