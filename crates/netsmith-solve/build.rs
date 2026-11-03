//! Lists the catalog files under `data/` so that adding a row, or a whole catalog, is a
//! data change: every `*.csv` in `data/jlcpcb/catalog/` and `data/sample/` is embedded in
//! the binary, named by its file stem (ADR 0022).

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
    }
    let target = Path::new(&env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("catalogs.rs");
    fs::write(target, out).expect("OUT_DIR is writable");
}
