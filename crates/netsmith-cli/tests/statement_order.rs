//! Statement order never matters (docs/DESIGN-RULES.md), designators included (issue #634): an
//! example with its statements reversed in every block builds and fabs to the same bytes: the
//! design file (but for the source line of each note), the schematic, the board with its nets,
//! the bill of materials and the placement file.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use netsmith_testkit::TempDir;

/// Two headers of the board (`J1` and `J2`) and a module's parts between the board's: the order
/// of the board's statements and of the module's would show in the designators if it counted.
const EXAMPLE: &str = include_str!("../../../examples/status_indicator.nsm");
const LOCK: &str = include_str!("../../../examples/status_indicator.lock.toml");

/// The source with the statements of every `{ ... }` block at the top level (a module, a board)
/// in reverse order. A statement is a line at the block's first indent and the lines after it
/// until its braces close; comments and blank lines before it go with it.
fn reversed(source: &str) -> String {
    let mut out = String::new();
    let mut lines = source.lines().peekable();
    while let Some(line) = lines.next() {
        out.push_str(line);
        out.push('\n');
        if !(line.ends_with('{') && !line.starts_with(' ')) {
            continue;
        }
        let mut statements: Vec<Vec<&str>> = Vec::new();
        let mut current: Vec<&str> = Vec::new();
        let mut depth = 0i32;
        for line in lines.by_ref() {
            if depth == 0 && line == "}" {
                break;
            }
            current.push(line);
            depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
            let trimmed = line.trim();
            let is_comment = trimmed.is_empty() || trimmed.starts_with("//");
            if depth == 0 && !is_comment {
                statements.push(std::mem::take(&mut current));
            }
        }
        assert!(current.iter().all(|l| l.trim().is_empty()), "{current:?}");
        assert!(statements.len() > 2, "a block of {} statements", statements.len());
        for statement in statements.iter().rev() {
            for line in statement {
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push_str("}\n");
    }
    out
}

/// Every file under `dir`, by its path from `dir`.
fn files(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        for entry in fs::read_dir(&at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let key = path.strip_prefix(dir).unwrap().to_path_buf();
                found.insert(key, fs::read(&path).unwrap());
            }
        }
    }
    found
}

fn netsmith(cwd: &Path, args: &[&str]) {
    let run = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("NETSMITH_NO_KICAD", "1")
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "netsmith {args:?}:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

/// The example in a folder of its own, with `source` as its text: what `build --emit-design`
/// and `fab --locked` write there.
fn outputs(label: &str, source: &str) -> (TempDir, BTreeMap<PathBuf, Vec<u8>>) {
    let dir = TempDir::new(label);
    fs::write(dir.join("status_indicator.nsm"), source).unwrap();
    fs::write(dir.join("status_indicator.lock.toml"), LOCK).unwrap();
    netsmith(
        &dir,
        &[
            "build",
            "status_indicator.nsm",
            "--out",
            "build",
            "--locked",
            "--emit-design",
            "status_indicator.design.json",
        ],
    );
    netsmith(
        &dir,
        &[
            "fab",
            "status_indicator.nsm",
            "--out",
            "fab",
            "--locked",
            "--no-glb",
            "--no-step",
        ],
    );
    let written = files(&dir);
    (dir, written)
}

/// The designs of a design file without the source line of each note: a note says where its
/// doc comment or relation is written, which moves with the statement, as the line of an
/// error does. The notes themselves, and their order, are compared.
fn without_source_lines(bytes: &[u8]) -> serde_json::Value {
    let mut designs: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    for design in designs.as_array_mut().unwrap() {
        for note in design["notes"].as_array_mut().unwrap() {
            note["at"]["line"] = serde_json::Value::Null;
        }
    }
    designs
}

#[test]
fn reversing_the_statements_of_an_example_changes_no_output() {
    let shuffled = reversed(EXAMPLE);
    assert_ne!(shuffled, EXAMPLE);
    let (_dir, written) = outputs("order-as-written", EXAMPLE);
    let (_reversed_dir, reversed_written) = outputs("order-reversed", &shuffled);
    let names = |files: &BTreeMap<PathBuf, Vec<u8>>| files.keys().cloned().collect::<Vec<_>>();
    assert_eq!(names(&written), names(&reversed_written));
    // The board file carries the netlist: each pad's net.
    for kind in ["design.json", "kicad_sch", "kicad_pcb", "bom.csv", "cpl.csv"] {
        assert!(
            written
                .keys()
                .any(|p| p.to_string_lossy().ends_with(kind)),
            "no {kind} among {:?}",
            names(&written)
        );
    }
    for (path, bytes) in &written {
        if path.as_os_str() == "status_indicator.nsm" {
            continue;
        }
        if path.as_os_str() == "status_indicator.design.json" {
            assert_eq!(
                without_source_lines(bytes),
                without_source_lines(&reversed_written[path]),
                "the design differs when the statements are reversed"
            );
            continue;
        }
        assert!(
            bytes == &reversed_written[path],
            "{} differs when the statements are reversed",
            path.display()
        );
    }
}
