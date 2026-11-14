//! The pin file (ADR 0101), end to end: `build` and `fab` write it when a run picks a pin, a
//! second run leaves its bytes alone, `--locked` refuses a change, and a locked pin survives a
//! change of the design.

use std::fs;
use std::path::Path;

use netsmith_fabricate::pins::{pins_path, sync};
use netsmith_check::Loader;
use netsmith_testkit::TempDir;

const PART: &str = r#"
/// Zmcu4, an invented microcontroller.
part Zmcu4 {
    peripheral UARTE0 on GPIO { UARTE0_TX:output UARTE0_RX:input }
    pins {
        .p00 bidirectional "P0.00" "3" fn GPIO
        .p01 bidirectional "P0.01" "4" fn GPIO
        .p02 bidirectional "P0.02" "5" fn GPIO ANALOG
        .p03 bidirectional "P0.03" "6" fn GPIO ANALOG
    }
}
"#;

fn source(requests: &str) -> String {
    format!(
        "import std::parts::passive::Resistor\n{PART}\nboard Sensors {{\n    .u: Zmcu4\n    .load: Resistor\n{requests}\n}}\n"
    )
}

/// What `build` does with the pin file of `main.nsm` in `dir`: read it, check, write.
fn run(dir: &Path, text: &str, locked: bool) -> Result<Vec<String>, String> {
    let path = dir.join("main.nsm");
    fs::write(&path, text).unwrap();
    let mut loader = Loader::std();
    netsmith_fabricate::pins::attach(&mut loader, "main.nsm", &path, &Default::default()).unwrap();
    sync("main.nsm", &path, text, &loader, locked).map(|s| s.notes)
}

#[test]
fn a_run_writes_the_pins_and_the_next_run_leaves_the_file_alone() {
    let dir = TempDir::new("pins-write");
    let text = source("    .status: .u.GPIO\n    .sense: .u.ANALOG\n    .status ~ .load.a\n");
    let notes = run(dir.path(), &text, false).unwrap();
    assert_eq!(
        notes,
        [
            "wrote main.pins.toml",
            "  Sensors.sense: new, pin `p02` (ANALOG)",
            "  Sensors.status: new, pin `p00` (GPIO)"
        ]
    );
    let file = pins_path(&dir.path().join("main.nsm"));
    let first = fs::read_to_string(&file).unwrap();
    assert_eq!(
        first,
        "version = 1\n\n[Sensors.pins.sense]\npin = \"p02\"\nfunction = \"ANALOG\"\nsource = \"auto\"\n\n[Sensors.pins.status]\npin = \"p00\"\nfunction = \"GPIO\"\nsource = \"auto\"\n"
    );
    let stamp = fs::metadata(&file).unwrap().modified().unwrap();

    // Again: nothing to say, nothing written, the same bytes.
    assert_eq!(run(dir.path(), &text, false).unwrap(), Vec::<String>::new());
    assert_eq!(fs::read_to_string(&file).unwrap(), first);
    assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), stamp);
    // And with `--locked`, which has nothing to refuse.
    assert_eq!(run(dir.path(), &text, true).unwrap(), Vec::<String>::new());
}

#[test]
fn locked_refuses_a_change_and_writes_nothing() {
    let dir = TempDir::new("pins-locked");
    let one = source("    .status: .u.GPIO\n    .status ~ .load.a\n");
    // No file yet: a run with --locked does not make one.
    let e = run(dir.path(), &one, true).unwrap_err();
    assert!(e.starts_with("error: the pin file `main.pins.toml` is out of date, and --locked does not change it:\n  Sensors.status: new, pin `p00` (GPIO)\n"), "{e}");
    let file = pins_path(&dir.path().join("main.nsm"));
    assert!(!file.exists());

    run(dir.path(), &one, false).unwrap();
    let before = fs::read_to_string(&file).unwrap();
    // A new request is a change.
    let two = source("    .status: .u.GPIO\n    .vbat: .u.ANALOG\n    .status ~ .load.a\n");
    let e = run(dir.path(), &two, true).unwrap_err();
    assert!(e.contains("Sensors.vbat: new, pin `p02` (ANALOG)"), "{e}");
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
    // Without it, the file gains the entry and keeps the first.
    run(dir.path(), &two, false).unwrap();
    let after = fs::read_to_string(&file).unwrap();
    assert!(
        after.contains("[Sensors.pins.status]\npin = \"p00\""),
        "{after}"
    );
    assert!(
        after.contains("[Sensors.pins.vbat]\npin = \"p02\""),
        "{after}"
    );
}

#[test]
fn a_pin_that_a_person_locked_stays_when_the_design_changes() {
    let dir = TempDir::new("pins-keep");
    let text = source("    .status: .u.GPIO\n    .status ~ .load.a\n");
    run(dir.path(), &text, false).unwrap();
    let file = pins_path(&dir.path().join("main.nsm"));
    // A person moves the pin and locks it.
    fs::write(
        &file,
        "version = 1\n\n[Sensors.pins.status]\npin = \"p03\"\nfunction = \"GPIO\"\nlock = true\nsource = \"hand\"\n",
    )
    .unwrap();
    let before = fs::read_to_string(&file).unwrap();
    // A request that sorts first is added: it takes `p00`, and the locked pin does not move.
    let more =
        source("    .a: .u.GPIO\n    .status: .u.GPIO\n    .a ~ .load.b\n    .status ~ .load.a\n");
    let notes = run(dir.path(), &more, false).unwrap();
    assert_eq!(notes[0], "wrote main.pins.toml");
    let after = fs::read_to_string(&file).unwrap();
    assert!(after.contains("[Sensors.pins.a]\npin = \"p00\""), "{after}");
    assert!(
        after.contains("[Sensors.pins.status]\npin = \"p03\"\nfunction = \"GPIO\"\nlock = true\nsource = \"hand\"\n"),
        "{after}"
    );
    assert_ne!(after, before);
}

#[test]
fn a_stale_entry_is_said_and_dropped_and_a_locked_one_stops_the_build() {
    let dir = TempDir::new("pins-stale");
    let text = source("    .status: .u.GPIO\n    .status ~ .load.a\n");
    run(dir.path(), &text, false).unwrap();
    let file = pins_path(&dir.path().join("main.nsm"));
    let mut with_gone = fs::read_to_string(&file).unwrap();
    with_gone
        .push_str("\n[Sensors.pins.gone]\npin = \"p03\"\nfunction = \"GPIO\"\nsource = \"auto\"\n");
    fs::write(&file, &with_gone).unwrap();

    let e = run(dir.path(), &text, true).unwrap_err();
    assert!(e.contains("Sensors.gone: removed, it was pin `p03`"), "{e}");
    assert!(
        e.contains("main.pins.toml:8: the entry `gone` (pin `p03`) is stale"),
        "{e}"
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), with_gone);

    let notes = run(dir.path(), &text, false).unwrap();
    assert!(
        notes.iter().any(|n| n.contains("the entry `gone`")),
        "{notes:?}"
    );
    assert!(!fs::read_to_string(&file).unwrap().contains("gone"));

    // A locked stale entry is an error of the check, which `build` reports; `sync` leaves the
    // file alone.
    let locked = with_gone.replace("source = \"auto\"\n", "lock = true\nsource = \"hand\"\n");
    fs::write(&file, &locked).unwrap();
    assert_eq!(run(dir.path(), &text, false).unwrap(), Vec::<String>::new());
    assert_eq!(fs::read_to_string(&file).unwrap(), locked);
    let mut loader = Loader::std();
    netsmith_fabricate::pins::attach(
        &mut loader,
        "main.nsm",
        &dir.path().join("main.nsm"),
        &Default::default(),
    )
    .unwrap();
    let checked = netsmith_check::check("main.nsm", &text, &loader);
    assert_eq!(
        checked.render().lines().next().unwrap(),
        "error: `gone` is locked to pin `p03` in the table of `Sensors`, but `Sensors` has no request `gone`; delete the entry to let the file drop it"
    );
}

/// The blocks of the guide's section "A pin by capability": the part, the board and the file.
fn guide_blocks() -> (String, String, String) {
    let guide = netsmith_testkit::guide_text();
    let section = guide.split("### A pin by capability").nth(1).unwrap();
    let fenced = |fence: &str| -> String {
        let start = section.find(fence).unwrap() + fence.len();
        let end = section[start..].find("```").unwrap();
        section[start..start + end].to_string()
    };
    (
        fenced("```nsm module=src::zmcu4\n"),
        fenced("```nsm\n"),
        fenced("```toml\n"),
    )
}

#[test]
fn the_pin_file_in_the_guide_is_what_a_run_writes() {
    let (part, board, toml) = guide_blocks();
    let dir = TempDir::new("pins-guide");
    let text = board.replace("import src::zmcu4::Zmcu4\n", &format!("{part}\n"));
    run(dir.path(), &text, false).unwrap();
    let written = fs::read_to_string(pins_path(&dir.path().join("main.nsm"))).unwrap();
    assert_eq!(written, toml);
}

#[test]
fn a_board_with_requests_goes_through_the_solver_and_the_design_with_the_pins_chosen() {
    let text = source(
        "    .status: .u.GPIO\n    .status ~ .load.a\n    .load.resistance &= 1kohm +/- 5%\n",
    );
    let loader = Loader::std();
    let outcome = netsmith_pipeline::outcome_with(
        "main.nsm",
        &text,
        &loader,
        &netsmith_solve::Catalogs::jlcpcb(),
        None,
    );
    let outcome = match outcome {
        Ok(o) => o,
        Err(problems) => panic!("{}", netsmith_pipeline::render_problems(&problems)),
    };
    let design = &outcome.boards[0].design;
    let pin = design
        .parts
        .iter()
        .flat_map(|p| p.pins.iter())
        .find(|pin| pin.function.as_deref() == Some("GPIO"))
        .expect("a pin with the function GPIO");
    assert_eq!(pin.name, "p00");
}
