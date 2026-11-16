//! `netsmith case import`: the outline, cutouts and holes of a case's STEP model as source (the
//! section's own tests are in `netsmith-3d` and `netsmith-tools`; this starts the binary).

use std::fs;

use netsmith_testkit::TempDir;

/// An invented case written for these tests (see `netsmith-tools/tests/case_import.rs`).
const CASE_BOX: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../netsmith-3d/tests/models/case_box.step"
);

fn netsmith(args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn case_import_prints_the_source_to_stdout() {
    let out = netsmith(&["case", "import", CASE_BOX, "--holes-at", "-4"]);
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("by `netsmith case import`.")
            && text.contains("polygon (60mm, 38mm)")
            && text.contains(".cutout2: Cutout {")
            && text.contains(".hole2: MountingHole { .diameter &= 3.2mm }"),
        "{text}"
    );
    assert!(out.stderr.is_empty(), "{out:?}");
}

#[test]
fn case_import_prints_the_height_limits_with_limits() {
    let lid = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../netsmith-3d/tests/models/case_lid.step"
    );
    let out = netsmith(&["case", "import", lid, "--limits"]);
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains(".limit1: Keepout {") && text.contains(".max_height &= 10mm"),
        "{text}"
    );
    let out = netsmith(&["case", "import", lid]);
    assert!(!String::from_utf8_lossy(&out.stdout).contains("Keepout"));
}

#[test]
fn case_import_limits_take_the_thickness_and_the_step() {
    let shaped = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../netsmith-3d/tests/models/case_shaped.step"
    );
    let out = netsmith(&["case", "import", shaped, "--limits", "--limit-step", "2"]);
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains(".layers &= \"bottom\"") && text.contains(".max_height &= 6.4mm"),
        "{text}"
    );
    let out = netsmith(&[
        "case", "import", shaped, "--limits", "--limit-step", "2", "--thickness", "1",
    ]);
    assert!(String::from_utf8_lossy(&out.stdout).contains(".max_height &= 7mm"));
    let out = netsmith(&["case", "import", shaped, "--limits", "--limit-step", "0"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("at least 0.05 mm"));
}

#[test]
fn case_import_takes_the_pose_and_the_plane() {
    // Turned half a turn about z and moved, the pocket's corner is where the pose puts it.
    let out = netsmith(&[
        "case", "import", CASE_BOX, "--rz", "180", "--x", "100", "--y", "50", "--at", "-0.8",
    ]);
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("pocket's bottom left corner is at (40, 10)"),
        "{text}"
    );
    // A plane above the whole model cuts nothing: an error, exit 1, nothing printed.
    let out = netsmith(&["case", "import", CASE_BOX, "--at", "20"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("cuts no pocket"));
}

#[test]
fn case_import_writes_the_physical_file_beside_the_source() {
    let dir = TempDir::new("case-import-write");
    let source = dir.join("boxed.nsm");
    fs::write(&source, "board Boxed {\n}\n").unwrap();
    let out = netsmith(&[
        "case",
        "import",
        CASE_BOX,
        "--write",
        source.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("physical file written; what changed:"), "{text}");
    let physical = fs::read_to_string(dir.join("boxed.physical.nsm")).unwrap();
    assert!(
        physical.contains("physical for Boxed {") && physical.contains(".outline: Outline {"),
        "{physical}"
    );
    assert_eq!(fs::read_to_string(&source).unwrap(), "board Boxed {\n}\n");
}

#[test]
fn case_import_of_a_missing_or_wrong_file_is_an_error() {
    let out = netsmith(&["case", "import", "/nonexistent/box.step"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(out.stdout.is_empty());
    let dir = TempDir::new("case-import-bad");
    let bad = dir.join("bad.step");
    fs::write(&bad, "not a model").unwrap();
    let out = netsmith(&["case", "import", bad.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("error: "));
}
