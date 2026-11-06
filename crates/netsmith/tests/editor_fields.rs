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

fn line_of(text: &str, needle: &str) -> u32 {
    text.lines().position(|l| l.contains(needle)).unwrap() as u32 + 1
}

/// The line of the first line that is exactly `needle`, from line `from`.
fn line_after(text: &str, from: u32, needle: &str) -> u32 {
    text.lines()
        .enumerate()
        .skip(from as usize)
        .find(|(_, l)| *l == needle)
        .map(|(i, _)| i as u32 + 1)
        .unwrap()
}

#[test]
fn a_definition_says_where_it_ends_and_what_is_declared_in_it() {
    let (_root, editor) = project("outline");
    let tree = editor.project();
    let main = tree
        .files
        .iter()
        .find(|f| f.path == "src/main.nsm")
        .unwrap();
    let ldo = main.defs.iter().find(|d| d.name == "LdoStage").unwrap();
    let start = line_of(TWO_BOARDS, "module LdoStage");
    assert_eq!(ldo.line, start);
    assert_eq!(ldo.end_line, Some(line_after(TWO_BOARDS, start, "}")));
    let children: Vec<(&str, &str, Option<&str>)> = ldo
        .children
        .as_ref()
        .unwrap()
        .iter()
        .map(|c| (c.name.as_str(), c.type_name.as_str(), c.kind.as_deref()))
        .collect();
    // `Rail` is imported from the standard library: its kind is read from there.
    assert_eq!(children[0], ("vin", "Rail", Some("interface")));
    assert_eq!(children[1], ("vout", "Rail", Some("interface")));
    assert_eq!(children[2].0, "regulator");
    assert_eq!(children[2].1, "LinearRegulator");
    assert!(children[2].2.is_some(), "an imported definition has a kind");
    assert_eq!(
        children.iter().map(|c| c.0).collect::<Vec<_>>(),
        ["vin", "vout", "regulator", "input_cap", "output_cap"]
    );
    let vin = &ldo.children.as_ref().unwrap()[0];
    assert_eq!(vin.line, line_of(TWO_BOARDS, ".vin:  Rail"));
    assert_eq!(vin.end_line, vin.line);

    // A declaration of a definition of the same file takes its kind from there; a
    // declaration with a block ends on its closing line.
    let board = main.defs.iter().find(|d| d.name == "PowerBoard").unwrap();
    let kids = board.children.as_ref().unwrap();
    let stage = kids.iter().find(|c| c.name == "stage").unwrap();
    assert_eq!(
        (stage.type_name.as_str(), stage.kind.as_deref()),
        ("LdoStage", Some("module"))
    );
    let first = line_of(TWO_BOARDS, ".stage: LdoStage {");
    assert_eq!(stage.line, first);
    assert_eq!(stage.end_line, first + 2);
    // Only the declarations written directly in the body are children.
    assert!(kids.iter().all(|c| !c.name.contains("thickness")));
    // The system's members are boards.
    let system = main.defs.iter().find(|d| d.name == "TwoBoards").unwrap();
    assert_eq!(
        system.children.as_ref().unwrap()[0].kind.as_deref(),
        Some("board")
    );
}

#[test]
fn a_definition_with_no_declaration_has_no_children_key_and_the_new_keys_are_camel_case() {
    let (_root, editor) = project("outline-json");
    let tree = editor.project();
    let json = serde_json::to_value(&tree).unwrap();
    let defs = json["files"][0]["defs"].as_array().unwrap();
    assert!(defs.iter().all(|d| d.get("endLine").is_some()));
    let child = &defs[0]["children"][0];
    assert!(
        child.get("endLine").is_some() && child.get("typeName").is_some(),
        "{child}"
    );
    let bare = netsmith_editor::api::Def {
        kind: "module".into(),
        name: "M".into(),
        line: 1,
        end_line: Some(1),
        children: None,
    };
    assert!(
        serde_json::to_value(&bare)
            .unwrap()
            .get("children")
            .is_none()
    );
    // Standard library files are listed with their definitions too.
    let std_file = tree
        .files
        .iter()
        .find(|f| f.path == "std/sources.nsm")
        .unwrap();
    assert!(
        std_file
            .defs
            .iter()
            .all(|d| d.end_line.is_some_and(|e| e >= d.line))
    );
}
