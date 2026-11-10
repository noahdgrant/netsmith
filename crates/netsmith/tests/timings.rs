//! `NETSMITH_TIMINGS=1` prints one line per stage to stderr, and with the variable unset a run
//! prints and writes exactly what it did without it.

use std::{fs, path::Path, process::Command};

use netsmith_kicad::testlib::TempDir;

const LED: &str = include_str!("../../../examples/led.nsm");

fn project(label: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"led\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), LED).unwrap();
    root
}

fn build(cwd: &Path, timings: Option<&str>) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_netsmith"));
    command
        .arg("build")
        .current_dir(cwd)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env(
            "NETSMITH_KICAD_SYMBOLS",
            netsmith_kicad::testlib::sample_symbols(),
        )
        .env(
            "NETSMITH_KICAD_FOOTPRINTS",
            netsmith_kicad::testlib::sample_footprints(),
        )
        .env_remove("NETSMITH_TIMINGS");
    if let Some(value) = timings {
        command.env("NETSMITH_TIMINGS", value);
    }
    command.output().unwrap()
}

#[test]
fn nothing_is_printed_without_the_variable_or_with_it_off() {
    let root = project("timings-off");
    for value in [None, Some(""), Some("0")] {
        let run = build(&root, value);
        assert!(
            run.status.success(),
            "{value:?}: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        let said = String::from_utf8_lossy(&run.stderr);
        assert!(!said.contains("timing: "), "{value:?}: {said}");
        assert!(
            !String::from_utf8_lossy(&run.stdout).contains("timing: "),
            "{value:?}"
        );
    }
}

#[test]
fn one_line_is_printed_for_each_stage_and_the_files_are_the_same() {
    let plain = project("timings-plain");
    let timed = project("timings-timed");
    let a = build(&plain, None);
    let b = build(&timed, Some("1"));
    assert!(a.status.success() && b.status.success());
    let said = String::from_utf8_lossy(&b.stderr);
    let lines: Vec<&str> = said.lines().filter(|l| l.starts_with("timing: ")).collect();
    for stage in [
        "timing: catalogs ",
        "timing: libraries ",
        "timing: check src/main.nsm ",
        "timing: solve LedExample ",
        "timing: solve.pick ",
        "timing: solve.dc ",
        "timing: design LedExample ",
        "timing: power LedExample ",
        "timing: export LedExample ",
        "timing: build.total ",
    ] {
        assert_eq!(
            lines.iter().filter(|l| l.starts_with(stage)).count(),
            1,
            "{stage}: {said}"
        );
    }
    assert!(
        lines
            .iter()
            .all(|l| l.ends_with(" ms") || l.ends_with(" s")),
        "{lines:?}"
    );
    // The same text on stdout and the same files as without it.
    let said = |root: &Path, run: &std::process::Output| {
        String::from_utf8_lossy(&run.stdout).replace(root.to_str().unwrap(), "<project>")
    };
    assert_eq!(said(&plain, &a), said(&timed, &b));
    for file in [
        "out/LedExample/LedExample.kicad_sch",
        "out/LedExample/LedExample.kicad_pro",
        "src/main.lock.toml",
    ] {
        assert_eq!(
            fs::read(plain.join(file)).unwrap(),
            fs::read(timed.join(file)).unwrap(),
            "{file}"
        );
    }
}
