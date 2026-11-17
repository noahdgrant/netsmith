//! `netsmith view pcb` (issue #1086) on the LED example, with KiCad switched off: the copper is the
//! routes file's and the copper `board fab --reuse-routes` keeps, and the router runs only with
//! `--route`. The pictures' own bytes are pinned in netsmith-fabricate's `tests/pcb_picture.rs`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn netsmith_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn ok(run: &Output) -> String {
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// A folder with the LED example in it.
fn led(name: &str) -> netsmith_testkit::TempDir {
    let dir = netsmith_testkit::TempDir::new(name);
    std::fs::copy(repo().join("examples/led.nsm"), dir.join("led.nsm")).unwrap();
    std::fs::copy(repo().join("examples/led.lock.toml"), dir.join("led.lock.toml")).unwrap();
    dir
}

fn open_lines(stdout: &str) -> Vec<&str> {
    stdout.lines().filter(|l| l.starts_with("open: ")).collect()
}

#[test]
fn with_no_copper_every_connection_is_open_and_the_same_board_gives_the_same_bytes() {
    let dir = led("view-pcb-open");
    let out = ok(&netsmith_in(&dir, &["view", "pcb", "led.nsm"]));
    assert_eq!(out.lines().next(), Some("LedExample-pcb.png"), "{out}");
    let png = std::fs::read(dir.join("LedExample-pcb.png")).unwrap();
    assert!(png.starts_with(b"\x89PNG"), "a PNG");
    assert_eq!(open_lines(&out).len(), 3, "{out}");
    let again = ok(&netsmith_in(
        &dir,
        &["view", "pcb", "led.nsm", "--picture", "again.png"],
    ));
    assert_eq!(open_lines(&again), open_lines(&out));
    assert_eq!(std::fs::read(dir.join("again.png")).unwrap(), png);

    let svg = ok(&netsmith_in(
        &dir,
        &[
            "view",
            "pcb",
            "led.nsm",
            "--layer",
            "bottom",
            "--format",
            "svg",
            "--only-open",
            "--designators",
        ],
    ));
    assert_eq!(svg.lines().next(), Some("LedExample-pcb-bottom.svg"), "{svg}");
    let text = std::fs::read_to_string(dir.join("LedExample-pcb-bottom.svg")).unwrap();
    assert!(text.starts_with("<svg "), "{text}");
    assert!(text.contains("seen from below"), "{text}");
}

#[test]
fn the_copper_fab_keeps_is_drawn_with_no_router_and_route_routes_first() {
    let dir = led("view-pcb-kept");
    let fab = netsmith_in(
        &dir,
        &[
            "board",
            "fab",
            "led.nsm",
            "--reuse-routes",
            "--no-glb",
            "--no-step",
            "--no-pdf",
            "--offline",
            "--out",
            "out",
        ],
    );
    ok(&fab);
    let run = netsmith_in(&dir, &["view", "pcb", "led.nsm", "--drc"]);
    let out = ok(&run);
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(said.contains("the router's copper of"), "{said}");
    assert!(open_lines(&out).is_empty(), "{out}");

    // Routed again from nothing, in another folder with no kept copper.
    let fresh = led("view-pcb-route");
    let out = ok(&netsmith_in(
        &fresh,
        &["view", "pcb", "led.nsm", "--route", "--format", "svg"],
    ));
    assert!(open_lines(&out).is_empty(), "{out}");
    assert!(
        !fresh.join(".netsmith").exists(),
        "`view pcb --route` writes nothing but the picture"
    );
}

#[test]
fn a_layer_or_an_id_the_board_has_not_exits_2_with_the_ones_it_has() {
    let dir = led("view-pcb-mistakes");
    let run = netsmith_in(&dir, &["view", "pcb", "led.nsm", "--highlight", "Q9"]);
    assert_eq!(run.status.code(), Some(2));
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(said.contains("`Q9`") && said.contains("net:VCC"), "{said}");
    let run = netsmith_in(&dir, &["view", "pcb", "led.nsm", "--layer", "inner1"]);
    assert_eq!(run.status.code(), Some(2));
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(said.contains("its layers are top, bottom"), "{said}");
    let run = netsmith_in(&dir, &[
            "view", "pcb", "led.nsm", "--region", "0,0,40,40", "--scale", "1000",
        ]);
    assert_eq!(run.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("more than 16384 a side"),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
}
