//! A part of a project's own needs no KiCad install (ADR 0097, ADR 0104): its footprint is a file of
//! the project (`[[library]]`) or a generator's pads asked for by name and parameters (`gen:`), and
//! its symbol is drawn from its pin table. Each test builds and fabs a project folder holding only
//! its own files, on a machine with no KiCad (`Search::under` an empty folder), from the text of
//! the guide's section "A part of your own, with no KiCad installed", so the guide's examples are
//! the ones that run.

use std::fs;
use std::path::Path;

use netsmith_testkit::TempDir;

/// The fenced blocks of the guide's section, as (words after the fence, text).
fn section() -> Vec<(Vec<String>, String)> {
    let guide =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/LANGUAGE.md"))
            .expect("the guide exists");
    let heading = "### A part of your own, with no KiCad installed";
    let start = guide.find(heading).expect("the guide has the section");
    let body = &guide[start + heading.len()..];
    let body = &body[..body.find("\n## ").unwrap_or(body.len())];
    let mut blocks = Vec::new();
    let mut lines = body.lines();
    while let Some(line) = lines.next() {
        if let Some(info) = line.strip_prefix("```") {
            let mut text = Vec::new();
            for line in lines.by_ref() {
                if line.starts_with("```") {
                    break;
                }
                text.push(line);
            }
            blocks.push((
                info.split_whitespace().map(str::to_string).collect(),
                text.join("\n") + "\n",
            ));
        }
    }
    blocks
}

/// The block that starts with `needle`, of the kind `kind`.
fn block(kind: &str, needle: &str) -> String {
    section()
        .into_iter()
        .find(|(info, text)| info.first().is_some_and(|k| k == kind) && text.contains(needle))
        .unwrap_or_else(|| panic!("the guide has no `{kind}` block with {needle:?}"))
        .1
}

/// The `[[part]]` blocks of the guide's section that map `Sensor`.
fn sensor_row(footprint_starts: &str) -> String {
    block("toml", &format!("footprint = \"{footprint_starts}"))
}

/// The project: its circuit from the guide, and its `netsmith.toml`.
fn project(dir: &Path, kicad_toml: &str) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"probe\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/main.nsm"),
        block("nsm-fragment", "part Sensor"),
    )
    .unwrap();
    fs::write(dir.join("kicad.toml"), kicad_toml).unwrap();
}

/// A symbol `Sensor` of three passive pins, VDD, OUT and GND, numbered 1 to 3, in a library file.
const OWN_SYMBOLS: &str = r#"(kicad_symbol_lib (version 20241209) (generator "test") (generator_version "9.0")
  (symbol "Sensor" (exclude_from_sim no) (in_bom yes) (on_board yes)
    (property "Reference" "U" (at 0 5.08 0) (effects (font (size 1.27 1.27))))
    (property "Value" "Sensor" (at 0 -5.08 0) (effects (font (size 1.27 1.27))))
    (property "Footprint" "" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes)))
    (property "Datasheet" "~" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes)))
    (property "Description" "" (at 0 0 0) (effects (font (size 1.27 1.27)) (hide yes)))
    (symbol "Sensor_0_1"
      (rectangle (start -3.81 3.81) (end 3.81 -3.81)
        (stroke (width 0.254) (type default)) (fill (type background))))
    (symbol "Sensor_1_1"
      (pin passive line (at -6.35 2.54 0) (length 2.54)
        (name "VDD" (effects (font (size 1.27 1.27)))) (number "1" (effects (font (size 1.27 1.27)))))
      (pin passive line (at 6.35 0 180) (length 2.54)
        (name "OUT" (effects (font (size 1.27 1.27)))) (number "2" (effects (font (size 1.27 1.27)))))
      (pin passive line (at -6.35 -2.54 0) (length 2.54)
        (name "GND" (effects (font (size 1.27 1.27)))) (number "3" (effects (font (size 1.27 1.27))))))))
"#;

/// Run `netsmith <command>` in the project folder `dir` as the working directory, on a machine
/// with no KiCad (`NETSMITH_NO_KICAD=1`), and return its stderr or the failure.
fn run_in(dir: &Path, command: &str) -> Result<String, String> {
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .arg(command)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("NETSMITH_KICAD_SYMBOLS")
        .env_remove("NETSMITH_KICAD_FOOTPRINTS")
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&run.stderr).into_owned();
    if run.status.success() {
        Ok(err)
    } else {
        Err(err)
    }
}

#[test]
fn a_projects_own_libraries_beside_its_kicad_toml_are_not_installed_ones_with_no_kicad() {
    let dir = TempDir::new("own-part-beside");
    // The libraries are beside `kicad.toml`, and `netsmith` runs in that folder: with no KiCad
    // there is no installed library, so `./own.pretty` is the project's and nothing else's.
    let kicad_toml = block("toml", "[[library]]").replace(
        "footprints = \"libs/own.pretty\"",
        "symbols = \"own.kicad_sym\"\nfootprints = \"own.pretty\"",
    );
    assert!(kicad_toml.contains("own.pretty"), "{kicad_toml}");
    let kicad_toml = kicad_toml.replace(
        "footprint = \"own:Sensor\"",
        "symbol = \"own:Sensor\"\nfootprint = \"own:Sensor\"",
    );
    project(&dir, &kicad_toml);
    fs::create_dir_all(dir.join("own.pretty")).unwrap();
    fs::write(
        dir.join("own.pretty/Sensor.kicad_mod"),
        block("text", "(footprint \"Sensor\""),
    )
    .unwrap();
    fs::write(dir.join("own.kicad_sym"), OWN_SYMBOLS).unwrap();

    for command in ["build", "fab"] {
        let err = run_in(&dir, command).unwrap_or_else(|e| panic!("`netsmith {command}`: {e}"));
        assert!(!err.contains("both installed"), "{err}");
    }
    assert!(dir.join("out").is_dir() || dir.join("fab").is_dir());
}

#[test]
fn an_installed_id_with_no_kicad_is_never_read_from_the_working_directory() {
    let dir = TempDir::new("own-part-cwd");
    // `kicad.toml` names `Foo:Bar` for a library it does not list, so the id is KiCad's. With no
    // KiCad there is no installed library: a `Foo.pretty` and a `Foo.kicad_sym` that happen to be
    // in the working directory are not one, and a different folder must not give another board.
    let row = sensor_row("gen:SOT23(pins=3)").replace(
        "footprint = \"gen:SOT23(pins=3)\"",
        "symbol = \"Foo:Bar\"\nfootprint = \"Foo:Bar\"",
    );
    assert!(row.contains("Foo:Bar"), "{row}");
    project(&dir, &row);
    fs::create_dir_all(dir.join("Foo.pretty")).unwrap();
    fs::write(
        dir.join("Foo.pretty/Bar.kicad_mod"),
        block("text", "(footprint \"Sensor\"").replace("\"Sensor\"", "\"Bar\""),
    )
    .unwrap();
    fs::write(
        dir.join("Foo.kicad_sym"),
        OWN_SYMBOLS.replace("Sensor", "Bar"),
    )
    .unwrap();

    for command in ["build", "fab"] {
        let err = run_in(&dir, command)
            .err()
            .unwrap_or_else(|| panic!("`netsmith {command}` read the working directory's Foo"));
        assert!(err.contains("`Foo:Bar`"), "{err}");
        assert!(err.contains("NETSMITH_NO_KICAD"), "{err}");
        assert!(!err.contains("not found: looked for"), "{err}");
    }
    assert!(!dir.join("out").exists() && !dir.join("fab").exists());
}

/// Run `netsmith <args>` in `dir` with KiCad switched off and a `KICAD_CLI` that is no file, and
/// return the status and stderr.
fn switched_off(dir: &Path, args: &[&str]) -> (bool, String) {
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("NETSMITH_KICAD_SYMBOLS")
        .env_remove("NETSMITH_KICAD_FOOTPRINTS")
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .output()
        .unwrap();
    (
        run.status.success(),
        String::from_utf8_lossy(&run.stderr).into_owned(),
    )
}

#[test]
fn cross_check_with_kicad_switched_off_says_what_hides_kicad_cli() {
    let dir = TempDir::new("own-part-cross-check");
    project(&dir, &sensor_row("gen:SOT23(pins=3)"));
    let (ok, err) = switched_off(&dir, &["fab", "--cross-check"]);
    assert!(!ok, "{err}");
    assert!(err.contains("--cross-check"), "{err}");
    assert!(err.contains("kicad-cli"), "{err}");
    assert!(err.contains("NETSMITH_NO_KICAD is set"), "{err}");
    // `KICAD_CLI` changes nothing then: it is not the advice.
    assert!(!err.contains("set KICAD_CLI"), "{err}");
}

#[test]
fn fab_with_kicad_switched_off_says_it_is_switched_off_not_that_it_is_not_installed() {
    let dir = TempDir::new("own-part-fab-note");
    project(&dir, &sensor_row("gen:SOT23(pins=3)"));
    // The run may stop at a later stage: the note comes first.
    let (_, err) = switched_off(&dir, &["fab"]);
    assert!(
        err.contains("note: KiCad is switched off (NETSMITH_NO_KICAD is set)"),
        "{err}"
    );
    assert!(!err.contains("KiCad is not installed"), "{err}");
}

#[test]
fn compare_of_a_schematic_with_kicad_switched_off_says_what_hides_kicad_cli() {
    let dir = TempDir::new("own-part-compare");
    fs::write(dir.join("a.kicad_sch"), "").unwrap();
    fs::write(dir.join("b.kicad_sch"), "").unwrap();
    let (ok, err) = switched_off(&dir, &["compare", "a.kicad_sch", "b.kicad_sch"]);
    assert!(!ok, "{err}");
    assert!(
        err.contains("reading a schematic needs KiCad's command-line tool"),
        "{err}"
    );
    assert!(err.contains("NETSMITH_NO_KICAD is set"), "{err}");
    assert!(!err.contains("set KICAD_CLI"), "{err}");
}
