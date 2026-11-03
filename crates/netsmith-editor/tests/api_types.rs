//! `web/src/api.ts` is generated from the Rust types and must be current.

use std::path::PathBuf;

#[test]
fn the_typescript_types_are_current() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web/src/api.ts");
    let wanted = netsmith_editor::api::typescript();
    if std::env::var_os("NETSMITH_WRITE_API_TS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &wanted).unwrap();
        return;
    }
    let have = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        have, wanted,
        "web/src/api.ts is out of date: run `NETSMITH_WRITE_API_TS=1 cargo test -p netsmith-editor` and commit it"
    );
}
