//! Lists the chapters of the language guide, `docs/language/*.md`, for `init.rs` to
//! `include_str!`: a new chapter is a new file, with no list to edit (issue #615).

use std::{env, fs, path::Path};

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let chapters = Path::new(&manifest).join("../../docs/language");
    println!("cargo:rerun-if-changed={}", chapters.display());
    let mut names: Vec<String> = fs::read_dir(&chapters)
        .unwrap_or_else(|e| panic!("{}: {e}", chapters.display()))
        .map(|entry| {
            entry
                .expect("a folder entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".md"))
        .collect();
    names.sort();
    let mut code = format!(
        "const GUIDE_CHAPTERS: [(&str, &str); {}] = [\n",
        names.len()
    );
    for name in &names {
        let path = chapters.join(name);
        code.push_str(&format!(
            "    ({name:?}, include_str!({:?})),\n",
            path.display().to_string()
        ));
    }
    code.push_str("];\n");
    let out =
        Path::new(&env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("guide_chapters.rs");
    fs::write(out, code).expect("OUT_DIR can be written");
}
