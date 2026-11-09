//! The crate's integration tests, one executable. Each file of this folder is a module of it
//! (a file per subject, as before); a new file needs a `mod` line here, or its tests do not run.
//! Run one file's tests with `cargo test -p netsmith-design --test main <file>::`, or with
//! `cargo nextest run -p netsmith-design <file>::`.

mod bank;
mod cancel;
mod design;
mod features;
mod guidelines;
mod ohm_bound;
mod place;

/// A file in this folder without a `mod` line above is not part of the executable: its tests would
/// never run, and nothing else would say so.
#[test]
fn every_test_file_is_declared_above() {
    let declared = include_str!("main.rs");
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(folder).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "rs")
            && let Some(name) = path.file_stem().and_then(|s| s.to_str())
            && name != "main"
            && !declared.lines().any(|l| l == format!("mod {name};"))
        {
            missing.push(name.to_string());
        }
    }
    assert!(
        missing.is_empty(),
        "add `mod` lines to main.rs for: {missing:?}"
    );
}
