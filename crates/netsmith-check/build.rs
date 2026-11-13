//! Lists the standard library's files for `Loader::std`: every `.nsm` file under the
//! workspace's `std/` folder, as a module path (`std::parts::passive`), a display name
//! (`std/parts/passive.nsm`) and the file's text. Adding a std file touches no list.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let std_dir = manifest.join("../../std");
    // Cargo rescans a folder named here for added, removed and changed files.
    println!("cargo:rerun-if-changed={}", std_dir.display());

    let mut files = Vec::new();
    collect(&std_dir, &mut files);
    files.sort();

    let mut out = String::from("&[\n");
    for file in &files {
        let relative = file
            .strip_prefix(&std_dir)
            .expect("under std/")
            .to_str()
            .expect("UTF-8 file name")
            .replace('\\', "/");
        let stem = relative.strip_suffix(".nsm").expect("a .nsm file");
        let module = format!("std::{}", stem.replace('/', "::"));
        let display = format!("std/{relative}");
        let absolute = file.canonicalize().expect("std file path");
        writeln!(
            out,
            "    ({module:?}, {display:?}, include_str!({:?})),",
            absolute.to_str().expect("UTF-8 path")
        )
        .unwrap();
    }
    out.push_str("]\n");

    let target = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR")).join("std_files.rs");
    std::fs::write(target, out).expect("write std_files.rs");
}

/// Every `.nsm` file under `dir`, at any depth.
fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read std/") {
        let path = entry.expect("std/ entry").path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "nsm") {
            files.push(path);
        }
    }
}
