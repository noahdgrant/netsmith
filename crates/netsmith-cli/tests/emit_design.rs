//! `netsmith build --emit-design <file>` writes the designs of the boards it builds (issue #609),
//! and nothing when a board fails.

use std::{fs, path::Path, process::Command};

use netsmith_core::design_file;
use netsmith_testkit::TempDir;

const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");

fn build(cwd: &Path, file: &str, emit: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["build", file, "--out", "out", "--emit-design"])
        .arg(emit)
        .current_dir(cwd)
        .env("NETSMITH_NO_KICAD", "1")
        .output()
        .unwrap()
}

#[test]
fn the_designs_of_every_board_of_the_file_are_written_in_order() {
    let dir = TempDir::new("emit-design");
    fs::write(dir.join("two_boards.nsm"), TWO_BOARDS).unwrap();
    let emit = dir.join("two_boards.design.json");
    let run = build(&dir, "two_boards.nsm", &emit);
    let said = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        said.contains(&format!("wrote {}", emit.display())),
        "{said}"
    );
    let designs = design_file::read(&emit).unwrap();
    let names: Vec<&str> = designs.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["PowerBoard", "LedBoard"]);
    assert!(
        designs
            .iter()
            .all(|d| !d.parts.is_empty() && !d.nets.is_empty())
    );
    // The same design is the same bytes.
    let first = fs::read_to_string(&emit).unwrap();
    assert!(build(&dir, "two_boards.nsm", &emit).status.success());
    assert_eq!(fs::read_to_string(&emit).unwrap(), first);
}

#[test]
fn nothing_is_written_when_a_board_fails() {
    let dir = TempDir::new("emit-design-fails");
    fs::write(
        dir.join("broken.nsm"),
        "board Broken {\n    .r1: NoSuchPart\n}\n",
    )
    .unwrap();
    let emit = dir.join("broken.design.json");
    let run = build(&dir, "broken.nsm", &emit);
    assert!(!run.status.success());
    assert!(!emit.exists());
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(said.contains("is not written, as a board failed"), "{said}");
}
