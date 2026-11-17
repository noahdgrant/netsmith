//! The Gerber view of the editor's PCB area (issue #1107): `GET /api/gerber` reads the package
//! `fab` wrote for a board (here one an earlier run left in the fab folder) and `POST
//! /api/gerber` one the page sends, with the reader of `netsmith view gerber`, so the summary and
//! the SVG are the command's. A file the reader does not understand is listed with its line.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use netsmith_editor::Backend;
use netsmith_fab::viewer::{self, Show};
use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

const BOARD: &str = "TwoBoards/led";

fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../netsmith-fab/tests/data/gerber-view")
}

/// The files of a folder of the reader's test data, by name.
fn files_of(folder: &str) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(data().join(folder))
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(&p).unwrap(),
            )
        })
        .collect();
    files.sort();
    files
}

fn zip_of(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in files {
        out.start_file(name.as_str(), zip::write::SimpleFileOptions::default())
            .unwrap();
        out.write_all(bytes).unwrap();
    }
    out.finish().unwrap().into_inner()
}

fn project(name: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-gerber-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"gerber\"\n").unwrap();
    fs::write(
        root.join("src/main.nsm"),
        include_str!("../../../examples/two_boards.nsm"),
    )
    .unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

#[test]
fn the_package_in_the_fab_folder_is_drawn_as_the_command_draws_it() {
    let (root, editor) = project("fab");
    // No package yet: the page says to make one.
    assert_eq!(editor.gerber(BOARD, None).unwrap_err().status, 404);
    let files = files_of("tracespace");
    let dir = root.join("fab/TwoBoards/led");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("LedBoard-gerbers.zip"), zip_of(&files)).unwrap();
    fs::write(dir.join("bom.csv"), "Comment\n").unwrap();

    let view = editor.gerber(BOARD, None).unwrap();
    assert_eq!(view.source, "fab");
    assert_eq!(
        view.path.as_deref(),
        Some("fab/TwoBoards/led/LedBoard-gerbers.zip")
    );
    // The first of the reader's views, and its SVG byte for byte.
    assert_eq!(view.layer, view.views[0]);
    assert_eq!(view.views, ["top", "all"]);
    let pkg = viewer::open(&dir.join("LedBoard-gerbers.zip")).unwrap();
    let drawn = viewer::draw(&pkg, Show::Top, None).unwrap();
    assert_eq!(view.svg, drawn.svg);
    assert_eq!(view.region, drawn.region.to_vec());
    assert!(!view.mirrored);
    assert_eq!(view.drawn, drawn.drawn);
    let functions: Vec<&str> = view.files.iter().map(|f| f.function.as_str()).collect();
    assert_eq!(functions, ["drill", "outline", "copper", "silk", "mask"]);
    assert!(view.unread.is_empty() && !view.holes.is_empty());
    // Another view, and one the package has no file for.
    assert_eq!(editor.gerber(BOARD, Some("all")).unwrap().layer, "all");
    let refused = editor.gerber(BOARD, Some("bottom")).unwrap_err();
    assert_eq!(refused.status, 400);
    assert!(refused.message.contains("top, all"), "{}", refused.message);
    assert_eq!(editor.gerber(BOARD, Some("sideways")).unwrap_err().status, 400);
    assert_eq!(editor.gerber("Nope", None).unwrap_err().status, 404);
}

#[test]
fn a_package_the_page_opens_is_kept_and_what_is_not_understood_is_listed() {
    let (_root, editor) = project("opened");
    let mut files = files_of("tracespace");
    files.extend(files_of("tracespace-unsupported"));
    let bytes = zip_of(&files);
    let view = editor.gerber_open("other.zip", bytes, None).unwrap();
    assert_eq!(view.source, "opened");
    assert_eq!(view.package, "other.zip");
    assert!(view.path.is_none());
    let unread: Vec<&str> = view.unread.iter().map(|u| u.file.as_str()).collect();
    assert!(unread.contains(&"arc-route.drl"), "{unread:?}");
    assert!(view.unread.iter().all(|u| !view.drawn.contains(&u.file)));
    assert!(view.unread.iter().any(|u| u.line.is_some()));
    // Another view of it, without sending it again; a name it does not keep is gone.
    assert_eq!(
        editor.gerber_opened("other.zip", Some("all")).unwrap().layer,
        "all"
    );
    assert_eq!(
        editor.gerber_opened("else.zip", None).unwrap_err().status,
        404
    );
    // One file is a package of one; bytes that are not a package say so.
    let one = files_of("tracespace").remove(2);
    assert_eq!(one.0, "tracespace.gtl");
    let single = editor.gerber_open(&one.0, one.1, None).unwrap();
    assert_eq!(single.files.len(), 1);
    assert_eq!(
        editor
            .gerber_open("notes.txt", b"hello".to_vec(), None)
            .unwrap_err()
            .status,
        400
    );
}
