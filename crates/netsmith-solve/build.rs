//! Lists the catalog files under `data/` so that adding a row, or a whole catalog, is a
//! data change: every `*.csv` in `data/jlcpcb/catalog/` and `data/sample/` is embedded in
//! the binary, named by its file stem (ADR 0022), and so is every curve file of theirs,
//! `curves/<catalog>/<row>.csv` beside them (issue #1260; the folder may be missing). The
//! offers files, `data/offers/<supplier>.csv` (ADR 0143), are the constant `OFFERS`, named by
//! their supplier.

use std::{env, fs, path::Path};

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").expect("cargo sets the manifest directory");
    let data = Path::new(&manifest).join("../../data");
    let mut out = String::new();
    for (constant, dir) in [("JLCPCB", "jlcpcb/catalog"), ("SAMPLE", "sample")] {
        let dir = data.join(dir);
        println!("cargo:rerun-if-changed={}", dir.display());
        let mut files: Vec<_> = fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .map(|e| e.expect("a directory entry").path())
            .filter(|p| p.extension().is_some_and(|x| x == "csv"))
            .collect();
        files.sort();
        out.push_str(&format!(
            "pub(crate) const {constant}: &[(&str, &str, &str)] = &[\n"
        ));
        for file in files {
            let stem = file
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("a file name");
            let shown = format!(
                "data/{}/{stem}.csv",
                dir.strip_prefix(&data).expect("below data").display()
            );
            let path = fs::canonicalize(&file).expect("the file exists");
            println!("cargo:rerun-if-changed={}", path.display());
            out.push_str(&format!(
                "    ({stem:?}, {shown:?}, include_str!({:?})),\n",
                path.display().to_string()
            ));
        }
        out.push_str("];\n");
        out.push_str(&curves(&data, &dir, constant));
    }
    out.push_str(&offers(&data));
    let target = Path::new(&env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("catalogs.rs");
    fs::write(target, out).expect("OUT_DIR is writable");
}

/// The constant `<constant>_CURVES`: `(catalog, row, shown name, text)` for every
/// `curves/<catalog>/<row>.csv` under `dir`, in name order.
fn curves(data: &Path, dir: &Path, constant: &str) -> String {
    let root = dir.join("curves");
    // A path that does not exist is out of date to cargo on every build, so the folder is
    // watched only where it is; `dir` itself is watched, and a new `curves/` changes it
    // (issue #1352: watching a missing folder rebuilt this crate and all above it on every
    // `cargo` call).
    if root.is_dir() {
        println!("cargo:rerun-if-changed={}", root.display());
    }
    let mut found: Vec<(String, String, std::path::PathBuf)> = Vec::new();
    if let Ok(folders) = fs::read_dir(&root) {
        for folder in folders.map(|e| e.expect("a directory entry").path()) {
            if !folder.is_dir() {
                continue;
            }
            println!("cargo:rerun-if-changed={}", folder.display());
            let catalog = folder
                .file_name()
                .and_then(|s| s.to_str())
                .expect("a folder name")
                .to_string();
            for file in
                fs::read_dir(&folder).unwrap_or_else(|e| panic!("{}: {e}", folder.display()))
            {
                let file = file.expect("a directory entry").path();
                if file.extension().is_some_and(|x| x == "csv") {
                    let row = file
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .expect("a file name")
                        .to_string();
                    found.push((catalog.clone(), row, file));
                }
            }
        }
    }
    found.sort();
    let mut out = format!("pub(crate) const {constant}_CURVES: &[(&str, &str, &str, &str)] = &[\n");
    for (catalog, row, file) in found {
        let shown = format!(
            "data/{}",
            file.strip_prefix(data).expect("below data").display()
        );
        let path = fs::canonicalize(&file).expect("the file exists");
        println!("cargo:rerun-if-changed={}", path.display());
        out.push_str(&format!(
            "    ({catalog:?}, {row:?}, {shown:?}, include_str!({:?})),\n",
            path.display().to_string()
        ));
    }
    out.push_str("];\n");
    out
}

/// The constant `OFFERS`: `(supplier, shown name, text)` for every `data/offers/*.csv`, in name
/// order. Whether a file is named for a supplier is the loader's to say, with its file.
fn offers(data: &Path) -> String {
    let dir = data.join("offers");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "csv"))
        .collect();
    files.sort();
    let mut out = "pub(crate) const OFFERS: &[(&str, &str, &str)] = &[\n".to_string();
    for file in files {
        let stem = file
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("a file name");
        let path = fs::canonicalize(&file).expect("the file exists");
        println!("cargo:rerun-if-changed={}", path.display());
        out.push_str(&format!(
            "    ({stem:?}, \"data/offers/{stem}.csv\", include_str!({:?})),\n",
            path.display().to_string()
        ));
    }
    out.push_str("];\n");
    out
}
