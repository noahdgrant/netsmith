//! The examples' design files (`<stem>.design.json`, issue #609), all of them. scripts/check-example-designs.sh
//! proves that each file is what the board builds now; this test proves that each one reads back.
//! (The loader's own tests, on named files, are netsmith-core's `design_file`.)

use std::fs;
use std::path::{Path, PathBuf};

use netsmith_core::design_file::{read, to_json};

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
