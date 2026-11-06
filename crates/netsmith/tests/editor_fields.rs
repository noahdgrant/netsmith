//! The fields the editor's page asked for in its UI rounds (ADR 0019, "Fields added for the
//! UI"). Every one is optional and left out of the JSON when empty.

use std::fs;

use netsmith::editor::Workspace;
use netsmith::project::Target;
use netsmith_editor::Backend;
use netsmith_kicad::testlib::TempDir;

const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");

/// A project: `two_boards.nsm` as its one top-level file.
fn project(name: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-fields-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), TWO_BOARDS).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

#[test]
fn the_project_says_which_netsmith_serves_it_and_where() {
    let (root, editor) = project("project");
    let tree = editor.project();
    assert_eq!(tree.version.as_deref(), Some(env!("CARGO_PKG_VERSION")));
    let shown = tree.root.expect("the project folder");
    assert!(std::path::Path::new(&shown).is_absolute(), "{shown}");
    assert_eq!(
        std::fs::canonicalize(&shown).unwrap(),
        std::fs::canonicalize(&*root).unwrap()
    );
    // Where kicad-cli was found is there exactly when it was found.
    assert_eq!(
        tree.kicad_path,
        netsmith_fab::KicadCliExe::locate().map(|k| k.exe().display().to_string())
    );
    assert_eq!(tree.kicad, tree.kicad_path.is_some());
    // The backend leaves the facts of the binary's page and checkout to the server.
    assert_eq!(tree.bundle_hash, None);
    assert_eq!(tree.git_describe, None);
}

#[test]
fn a_single_file_names_its_folder_as_the_root() {
    let dir = TempDir::new("editor-fields-single");
    fs::write(dir.join("one.nsm"), TWO_BOARDS).unwrap();
    let editor = Workspace::open(Target::resolve_from(&dir, dir.join("one.nsm").to_str()).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    let shown = editor.project().root.expect("a folder");
    assert_eq!(
        std::fs::canonicalize(&shown).unwrap(),
        std::fs::canonicalize(&*dir).unwrap()
    );
}

#[test]
fn the_new_project_fields_are_left_out_of_the_json_when_empty() {
    let project = netsmith_editor::api::Project {
        title: "t".into(),
        has_manifest: false,
        kicad: false,
        files: Vec::new(),
        folders: Vec::new(),
        version: None,
        git_describe: None,
        root: None,
        kicad_path: None,
        bundle_hash: None,
    };
    let json = serde_json::to_value(&project).unwrap();
    for key in ["version", "gitDescribe", "root", "kicadPath", "bundleHash"] {
        assert!(json.get(key).is_none(), "{key}: {json}");
    }
    let full = netsmith_editor::api::Project {
        kicad_path: Some("/usr/bin/kicad-cli".into()),
        bundle_hash: Some("8bfd81b2f58bf920".into()),
        git_describe: Some("v0.1.0-3-gabc".into()),
        ..project
    };
    let json = serde_json::to_value(&full).unwrap();
    assert_eq!(json["kicadPath"], "/usr/bin/kicad-cli");
    assert_eq!(json["bundleHash"], "8bfd81b2f58bf920");
    assert_eq!(json["gitDescribe"], "v0.1.0-3-gabc");
}
