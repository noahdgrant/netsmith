//! The copper of every layer of an example, drawn by `netsmith view pcb` from the board model and
//! by `netsmith view gerber` from the zip `board fab` wrote, agrees on one grid (issue #1142): the
//! same `--layer` and `--scale`, the region of each by default, the bottom turned over the same
//! way. The comparison is `netsmith_fabricate::gerber_compare`'s (its header says how copper is
//! told from the rest and what the tolerance is); a failure lists where the two differ, as
//! `--region` boxes to draw. Run with KiCad switched off: none is needed.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use netsmith_fabricate::gerber_compare::{self, Limits};

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

fn ok(run: &Output) {
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

/// Fabs the example `stem` (its source and every file beside it) with the router's copper kept,
/// then compares the two pictures of each copper layer the package has.
fn agrees(stem: &str, board: &str, layers: &[&str]) {
    let dir = netsmith_testkit::TempDir::new(&format!("gerber-against-pcb-{stem}"));
    for entry in std::fs::read_dir(repo().join("examples")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.is_file() && name.starts_with(&format!("{stem}.")) {
            std::fs::copy(&path, dir.join(&name)).unwrap();
        }
    }
    let source = format!("{stem}.nsm");
    ok(&netsmith_in(
        &dir,
        &[
            "board",
            "fab",
            &source,
            "--reuse-routes",
            "--no-glb",
            "--no-step",
            "--no-pdf",
            "--offline",
            "--out",
            "out",
        ],
    ));
    let zip = dir.join(format!("out/{board}/{board}-gerbers.zip"));
    let zip = zip.to_str().unwrap();

    // The layers the package has are the board's.
    let run = netsmith_in(&dir, &["view", "gerber", zip, "--format", "json"]);
    ok(&run);
    let json: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let mut views: Vec<&str> = layers.to_vec();
    views.push("all");
    assert_eq!(json["views"], serde_json::json!(views), "{json}");

    let limits = Limits::default();
    let scale = limits.scale.to_string();
    let mut failed = Vec::new();
    for layer in layers {
        let (pcb, gerber) = (format!("pcb-{layer}.svg"), format!("gerber-{layer}.svg"));
        let common = ["--layer", layer, "--scale", &scale, "--format", "svg"];
        let mut args = vec!["view", "pcb", &source, "--picture", &pcb];
        args.extend(common);
        ok(&netsmith_in(&dir, &args));
        let mut args = vec!["view", "gerber", zip, "--picture", &gerber];
        args.extend(common);
        ok(&netsmith_in(&dir, &args));
        let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap();
        let agreement =
            gerber_compare::compare(&read(&pcb), &read(&gerber), *layer == "bottom", &limits)
                .unwrap_or_else(|e| panic!("{stem} {layer}: {e}"));
        println!("{stem} {layer}: {agreement}");
        assert!(agreement.model > 0, "{stem} {layer}: no copper drawn");
        if !agreement.ok(&limits) {
            failed.push(format!("{layer}: {agreement}"));
        }
    }
    assert!(
        failed.is_empty(),
        "the board model's copper and the Gerbers' differ (draw a place with `view pcb` and \
         `view gerber --region`):\n{}",
        failed.join("\n")
    );
}

#[test]
fn the_copper_of_the_led_example_agrees_with_its_gerbers() {
    agrees("led", "LedExample", &["top", "bottom"]);
}

#[test]
fn the_copper_of_four_layers_with_pours_and_planes_agrees_with_its_gerbers() {
    agrees(
        "outline_and_pours",
        "OutlineAndPours",
        &["top", "inner1", "inner2", "bottom"],
    );
}
