//! The examples' design files (`<stem>.design.json`, issue #609) and the loader the tests of the
//! later stages start from. scripts/check-example-designs.sh proves that each file is what the
//! board builds now; these tests prove that each one reads back.

use std::fs;
use std::path::{Path, PathBuf};

use netsmith_core::Design;
use netsmith_core::design_file::{read, to_json};
use netsmith_testkit::TempDir;

/// Every `*.design.json` under examples/.
fn design_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut dirs = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.to_string_lossy().ends_with(".design.json") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// Each file reads back into designs that write the same bytes, so a test that starts from one
/// gets exactly what `build --emit-design` saw.
#[test]
fn every_example_design_file_reads_back_to_the_same_bytes() {
    let files = design_files();
    assert!(!files.is_empty(), "no example has a design file");
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        let designs = read(&file).unwrap();
        assert!(!designs.is_empty(), "{}: holds no design", file.display());
        assert!(
            to_json(&designs) == text,
            "{}: reading and writing the file changes it",
            file.display()
        );
    }
}

#[test]
fn an_example_loads_as_its_board() {
    let led = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/led.design.json");
    let design = Design::from_json_file(&led).unwrap();
    assert_eq!(design.name, "LedExample");
    assert!(design.parts.iter().any(|p| p.refdes == "D1"));
    assert!(design.process.is_some());
}

#[test]
fn from_json_file_refuses_a_file_of_several_boards() {
    let two = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/two_boards.design.json");
    let e = Design::from_json_file(&two).unwrap_err();
    assert!(e.contains("holds 2 designs"), "{e}");
    assert_eq!(read(&two).unwrap().len(), 2);
}

#[test]
fn a_file_that_is_not_there_or_not_designs_is_named_in_the_error() {
    let dir = TempDir::new("design-file");
    let missing = dir.path().join("missing.design.json");
    let e = read(&missing).unwrap_err();
    assert!(e.starts_with(&missing.display().to_string()), "{e}");
    let bad = dir.path().join("bad.design.json");
    fs::write(&bad, "{}").unwrap();
    let e = Design::from_json_file(&bad).unwrap_err();
    assert!(e.starts_with(&bad.display().to_string()), "{e}");
}
