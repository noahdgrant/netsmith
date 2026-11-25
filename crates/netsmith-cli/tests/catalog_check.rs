//! `netsmith catalog check` and the build's note on a pick whose package does not fit its
//! footprint (issue #1658), in a project whose `kicad.toml` gives a kind's `SOT-23` the SOT-323
//! footprint.

use netsmith_testkit::TempDir;
use std::fs;
use std::path::Path;
use std::process::Command;

fn netsmith(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap()
}

const SOURCE: &str = r#"part Ldo2 {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
        .c passive "C" "3"
    }
    .package: Package
    catalog ldo2 { .package }
}

board Demo {
    .u: Ldo2
}
"#;

/// A project with one catalog row of package `SOT-23`, and `kicad.toml` mapping the part's
/// `SOT-23` to `mapped`.
fn project(name: &str, mapped: &str) -> TempDir {
    let root = TempDir::new(name);
    let write = |file: &str, text: &str| {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    };
    write("netsmith.toml", "[project]\nname = \"demo\"\n");
    write(
        "kicad.toml",
        &format!(
            "[[part]]\nkind = \"Ldo2\"\npackage = \"SOT-23\"\nfootprint = \"{mapped}\"\n"
        ),
    );
    write("src/main.nsm", SOURCE);
    write("catalogs/ldo2.csv", "mpn,package,source\nX1,SOT-23,a datasheet\n");
    root
}

#[test]
fn a_sot_23_row_mapped_to_the_sot_323_footprint_fails_the_check_and_is_noted_by_the_build() {
    let root = project("catalog-check-wrong", "Package_TO_SOT_SMD:SOT-323_SC-70");
    let out = netsmith(&root, &["catalog", "check", "catalogs", "--format", "json"]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stderr));
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(doc["ok"], false);
    let wrong = &doc["mismatches"][0];
    assert_eq!((wrong["file"].as_str(), wrong["line"].as_u64()), (Some("catalogs/ldo2.csv"), Some(2)));
    let problem = wrong["problems"][0].as_str().unwrap();
    assert!(problem.contains("1.35 x 2.2 mm") && problem.contains("1.2-1.4 x 2.8-3 mm"), "{problem}");
    assert!(problem.contains("source: JSCJ 2N7002"), "{problem}");

    let built = netsmith(&root, &["board", "build", "src/main.nsm", "--out", "out"]);
    let text = String::from_utf8_lossy(&built.stdout).to_string() + &String::from_utf8_lossy(&built.stderr);
    assert!(
        text.contains("note: U1: the catalog row X1's package does not fit its footprint"),
        "{text}"
    );
}

#[test]
fn a_row_whose_footprint_is_its_package_passes_with_the_built_in_catalogs() {
    let root = project("catalog-check-right", "Package_TO_SOT_SMD:SOT-23");
    let out = netsmith(&root, &["catalog", "check"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains(", 0 that disagree"), "{text}");
    assert!(text.contains("not measured, netsmith knows no size of the package: 3215"), "{text}");

    let built = netsmith(&root, &["board", "build", "src/main.nsm", "--out", "out"]);
    let text = String::from_utf8_lossy(&built.stdout).to_string() + &String::from_utf8_lossy(&built.stderr);
    assert!(!text.contains("does not fit its footprint"), "{text}");
}
