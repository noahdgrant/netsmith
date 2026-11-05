//! Making, moving and removing files from the editor: what is refused (a path that leaves the
//! project, a link that does, a name that is taken, a name a module cannot have) and what the
//! tree shows after each change.

use std::{fs, path::Path};

use netsmith::{editor::Workspace, project::Target};
use netsmith_editor::{Backend, api::FileRole};
use netsmith_kicad::testlib::TempDir;

const MAIN: &str = "//! The top level.\n\nimport std::mfg::jlcpcb::TwoLayer\n\nboard Top {\n    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n}\n";
const MODULE: &str = "import std::sources::Rail\n\n/// A module.\nmodule Power {\n    .power: Rail\n}\n";

/// A project with a top-level file `src/main.nsm`, a module `src/power.nsm` with a physical file
/// beside it, a folder `src/sheets` with one module, and a folder outside it that a test makes
/// links to.
fn project(name: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-files-{name}"));
    fs::create_dir_all(root.join("src/sheets")).unwrap();
    fs::create_dir_all(root.join("outside")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), MAIN).unwrap();
    fs::write(root.join("src/power.nsm"), MODULE).unwrap();
    fs::write(root.join("src/power.physical.nsm"), "//! physical\n").unwrap();
    fs::write(root.join("src/sheets/adc.nsm"), MODULE.replace("Power", "Adc")).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

fn paths(editor: &Workspace) -> Vec<String> {
    editor.project().files.into_iter().map(|f| f.path).collect()
}

fn listing(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

#[test]
fn a_new_file_is_in_the_tree_with_its_definitions_and_the_check_is_made_again() {
    let (root, editor) = project("create");
    let before = editor.analysis().version;
    let made = editor.create_file("src/sheets/ldo.nsm", &MODULE.replace("Power", "Ldo")).unwrap();
    assert_eq!(made.path, "src/sheets/ldo.nsm");
    assert_eq!(fs::read_to_string(root.join("src/sheets/ldo.nsm")).unwrap(), MODULE.replace("Power", "Ldo"));
    let file = editor
        .project()
        .files
        .into_iter()
        .find(|f| f.path == "src/sheets/ldo.nsm")
        .expect("in the tree");
    assert_eq!(file.role, FileRole::Module);
    assert_eq!(file.defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["Ldo"]);
    assert!(editor.analysis().version > before, "the files were read again");
}

#[test]
fn a_new_folder_is_in_the_tree_before_it_has_a_file() {
    let (root, editor) = project("folder");
    assert_eq!(editor.project().folders, ["src/sheets"]);
    editor.create_dir("src/proposed").unwrap();
    editor.create_dir("src/proposed/old").unwrap();
    assert!(root.join("src/proposed/old").is_dir());
    assert_eq!(editor.project().folders, ["src/proposed", "src/proposed/old", "src/sheets"]);
    // Its parent must be there: one folder at a time.
    assert_eq!(editor.create_dir("src/nope/deeper").unwrap_err().status, 404);
}

#[test]
fn a_physical_file_needs_its_source_beside_it() {
    let (root, editor) = project("physical");
    let e = editor.create_file("src/sheets/adc.physical.nsm", "physical for Adc {\n    case {\n    }\n}\n");
    assert!(e.is_ok(), "{e:?}");
    let e = editor.create_file("src/sheets/gone.physical.nsm", "").unwrap_err();
    assert_eq!(e.status, 400);
    assert!(e.message.contains("gone.nsm"), "{}", e.message);
    assert!(!root.join("src/sheets/gone.physical.nsm").exists());
}

#[test]
fn a_path_that_leaves_the_project_or_is_not_below_src_is_refused_and_nothing_is_written() {
    let (root, editor) = project("traversal");
    let top = listing(&root);
    let src = listing(&root.join("src"));
    for path in [
        "../escape.nsm",
        "src/../escape.nsm",
        "src/sheets/../../../escape.nsm",
        "/tmp/escape.nsm",
        "src\\escape.nsm",
        ".git/hooks/x.nsm",
        "target/x.nsm",
        "out/x.nsm",
        "fab/x.nsm",
        "src/.hidden/x.nsm",
        "outside/x.nsm",
        "x.nsm",
        "netsmith.toml",
    ] {
        let e = editor.create_file(path, "x").unwrap_err();
        assert!(e.status == 400 || e.status == 403, "{path}: {e:?}");
        let e = editor.create_dir(path).unwrap_err();
        assert!(e.status == 400 || e.status == 403, "{path}: {e:?}");
        let e = editor.rename_path("src/power.nsm", path).unwrap_err();
        assert!(e.status == 400 || e.status == 403, "{path}: {e:?}");
        let e = editor.rename_path(path, "src/other.nsm").unwrap_err();
        assert!(e.status >= 400, "{path}: {e:?}");
        let e = editor.delete_path(path, true).unwrap_err();
        assert!(e.status >= 400, "{path}: {e:?}");
    }
    assert_eq!(listing(&root), top);
    assert_eq!(listing(&root.join("src")), src);
    assert!(!root.parent().unwrap().join("escape.nsm").exists());
    assert_eq!(listing(&root.join("outside")), Vec::<String>::new());
}

#[test]
fn a_name_that_is_not_a_modules_is_refused() {
    let (root, editor) = project("names");
    for path in ["src/a-b.nsm", "src/1a.nsm", "src/a b.nsm", "src/power.txt", "src/power", "src/con.nsm", "src/x.y.nsm"] {
        assert_eq!(editor.create_file(path, "").unwrap_err().status, 400, "{path}");
    }
    for path in ["src/a-b", "src/1a", "src/a.b", "src/nul"] {
        assert_eq!(editor.create_dir(path).unwrap_err().status, 400, "{path}");
    }
    assert_eq!(editor.create_file("src/ok.nsm", &"x".repeat(1 << 21)).unwrap_err().status, 400);
    assert_eq!(editor.create_file("src/ok.nsm", "a\0b").unwrap_err().status, 400);
    assert_eq!(listing(&root.join("src")), ["main.nsm", "power.nsm", "power.physical.nsm", "sheets"]);
}

#[cfg(unix)]
#[test]
fn a_link_that_leaves_src_is_not_followed_to_make_move_or_remove_anything() {
    let (root, editor) = project("symlink");
    fs::write(root.join("outside/keep.nsm"), "keep").unwrap();
    std::os::unix::fs::symlink(root.join("outside"), root.join("src/link")).unwrap();
    std::os::unix::fs::symlink(root.join("outside/keep.nsm"), root.join("src/keep_link.nsm")).unwrap();
    // A link to a folder that is inside the project but not below src/ is no better.
    std::os::unix::fs::symlink(root.join("src/../outside"), root.join("src/sheets/up")).unwrap();
    for path in ["src/link/x.nsm", "src/sheets/up/x.nsm"] {
        let e = editor.create_file(path, "x").unwrap_err();
        assert_eq!(e.status, 403, "{path}: {e:?}");
    }
    assert_eq!(editor.create_dir("src/link/d").unwrap_err().status, 403);
    assert_eq!(editor.rename_path("src/power.nsm", "src/link/power.nsm").unwrap_err().status, 403);
    assert_eq!(editor.rename_path("src/link", "src/other").unwrap_err().status, 403);
    assert_eq!(editor.delete_path("src/link", true).unwrap_err().status, 403);
    assert_eq!(editor.delete_path("src/keep_link.nsm", true).unwrap_err().status, 404);
    assert!(root.join("outside/keep.nsm").is_file());
    assert_eq!(listing(&root.join("outside")), ["keep.nsm"]);
    assert!(root.join("src/power.nsm").is_file());
}

#[test]
fn a_path_that_is_taken_is_a_409_and_is_not_overwritten() {
    let (root, editor) = project("taken");
    let e = editor.create_file("src/power.nsm", "overwritten").unwrap_err();
    assert_eq!(e.status, 409);
    assert!(e.message.contains("already there"), "{}", e.message);
    assert_eq!(fs::read_to_string(root.join("src/power.nsm")).unwrap(), MODULE);
    // In another case of letters, which is the same path on some file systems.
    assert_eq!(editor.create_file("src/POWER.nsm", "x").unwrap_err().status, 409);
    assert_eq!(editor.create_dir("src/sheets").unwrap_err().status, 409);
    assert_eq!(editor.create_dir("src/Sheets").unwrap_err().status, 409);
    assert_eq!(editor.rename_path("src/sheets/adc.nsm", "src/power.nsm").unwrap_err().status, 409);
    assert_eq!(fs::read_to_string(root.join("src/power.nsm")).unwrap(), MODULE);
    assert!(root.join("src/sheets/adc.nsm").is_file());
    editor.create_dir("src/other").unwrap();
    assert_eq!(editor.rename_path("src/other", "src/sheets").unwrap_err().status, 409);
}

#[test]
fn a_file_is_renamed_in_place_or_into_another_folder_with_its_physical_file() {
    let (root, editor) = project("rename");
    let r = editor.rename_path("src/power.nsm", "src/sheets/supply.nsm").unwrap();
    assert_eq!(r.path, "src/sheets/supply.nsm");
    assert!(!root.join("src/power.nsm").exists() && !root.join("src/power.physical.nsm").exists());
    assert!(root.join("src/sheets/supply.nsm").is_file());
    assert!(root.join("src/sheets/supply.physical.nsm").is_file(), "its physical file goes with it");
    let files = paths(&editor);
    assert!(files.contains(&"src/sheets/supply.nsm".to_string()));
    assert!(files.contains(&"src/sheets/supply.physical.nsm".to_string()));
    assert!(!files.contains(&"src/power.nsm".to_string()));
    editor.rename_path("src/sheets/adc.nsm", "src/sheets/converter.nsm").unwrap();
    assert!(root.join("src/sheets/converter.nsm").is_file());
}

#[test]
fn a_folder_is_renamed_with_what_is_in_it() {
    let (root, editor) = project("rename-folder");
    editor.rename_path("src/sheets", "src/boards").unwrap();
    assert!(root.join("src/boards/adc.nsm").is_file() && !root.join("src/sheets").exists());
    assert_eq!(editor.project().folders, ["src/boards"]);
    editor.create_dir("src/archive").unwrap();
    editor.rename_path("src/boards", "src/archive/boards").unwrap();
    assert!(root.join("src/archive/boards/adc.nsm").is_file());
    // Into itself is no move.
    assert_eq!(editor.rename_path("src/archive", "src/archive/inner").unwrap_err().status, 400);
}

#[test]
fn the_manifest_the_library_and_the_top_level_files_are_not_renamed() {
    let (root, editor) = project("rename-fixed");
    let e = editor.rename_path("netsmith.toml", "src/other.nsm").unwrap_err();
    assert_eq!(e.status, 403, "{e:?}");
    let e = editor.rename_path("std/bus.nsm", "src/bus.nsm").unwrap_err();
    assert_eq!(e.status, 403, "{e:?}");
    let e = editor.rename_path("src/main.nsm", "src/primary.nsm").unwrap_err();
    assert_eq!(e.status, 400);
    assert!(e.message.contains("netsmith.toml"), "{}", e.message);
    // A physical file goes with its source, not by itself.
    assert_eq!(editor.rename_path("src/power.physical.nsm", "src/p.physical.nsm").unwrap_err().status, 400);
    // The module is not given a physical file's name.
    assert_eq!(editor.rename_path("src/power.nsm", "src/p.physical.nsm").unwrap_err().status, 400);
    assert!(root.join("src/main.nsm").is_file() && root.join("netsmith.toml").is_file());
    // A folder holding a top-level file is not moved.
    fs::create_dir_all(root.join("src/inner")).unwrap();
}

#[test]
fn a_folder_with_files_in_it_is_removed_only_with_a_yes() {
    let (root, editor) = project("delete-folder");
    let e = editor.delete_path("src/sheets", false).unwrap_err();
    assert_eq!(e.status, 428);
    assert!(e.message.contains("needs a yes"), "{}", e.message);
    assert!(root.join("src/sheets/adc.nsm").is_file());
    editor.delete_path("src/sheets", true).unwrap();
    assert!(!root.join("src/sheets").exists());
    assert!(editor.project().folders.is_empty());
    // An empty one needs none.
    editor.create_dir("src/empty").unwrap();
    editor.delete_path("src/empty", false).unwrap();
    assert!(!root.join("src/empty").exists());
    // `src` is where the modules are.
    assert_eq!(editor.delete_path("src", true).unwrap_err().status, 403);
    assert!(root.join("src").is_dir());
}

#[test]
fn a_file_is_removed_and_the_check_is_made_again() {
    let (root, editor) = project("delete-file");
    let before = editor.analysis().version;
    editor.delete_path("src/sheets/adc.nsm", false).unwrap();
    assert!(!root.join("src/sheets/adc.nsm").exists());
    assert!(!paths(&editor).contains(&"src/sheets/adc.nsm".to_string()));
    assert!(editor.analysis().version > before);
    // The physical file of a source goes with it, and a physical file alone is removed alone.
    editor.delete_path("src/power.nsm", false).unwrap();
    assert!(!root.join("src/power.physical.nsm").exists());
    assert_eq!(editor.delete_path("src/ghost.nsm", true).unwrap_err().status, 404);
}

#[test]
fn the_manifest_and_the_library_are_never_removed_and_a_top_level_file_needs_a_yes() {
    let (root, editor) = project("delete-fixed");
    for yes in [false, true] {
        assert_eq!(editor.delete_path("netsmith.toml", yes).unwrap_err().status, 403);
        assert_eq!(editor.delete_path("std/bus.nsm", yes).unwrap_err().status, 403);
    }
    assert!(root.join("netsmith.toml").is_file());
    let e = editor.delete_path("src/main.nsm", false).unwrap_err();
    assert_eq!(e.status, 428);
    assert!(root.join("src/main.nsm").is_file());
    editor.delete_path("src/main.nsm", true).unwrap();
    assert!(!root.join("src/main.nsm").exists());
}

#[test]
fn a_single_file_has_no_project_to_change() {
    let root = TempDir::new("editor-files-single");
    fs::write(root.join("only.nsm"), MAIN).unwrap();
    let target = Target::resolve_from(&root, Some("only.nsm")).unwrap();
    let editor = Workspace::open(target).unwrap();
    let e = editor.create_file("src/x.nsm", "").unwrap_err();
    assert_eq!(e.status, 400);
    assert!(e.message.contains("netsmith.toml"), "{}", e.message);
    assert_eq!(editor.create_dir("src/x").unwrap_err().status, 400);
    assert_eq!(editor.delete_path("only.nsm", true).unwrap_err().status, 400);
    assert!(editor.project().folders.is_empty());
}
