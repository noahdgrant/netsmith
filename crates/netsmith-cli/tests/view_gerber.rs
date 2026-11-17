//! `netsmith view gerber` (issue #1105): it draws the package `board fab` writes, the same from
//! the zip, from `fab`'s folder and from the files unzipped, sums it up as JSON, and names the
//! files it cannot read instead of drawing them. Run with KiCad switched off: none is needed.

use std::io::Read as _;
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

fn ok(run: &Output) {
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

/// A PNG's width and height, from its header.
fn size(png: &[u8]) -> (u32, u32) {
    let be = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
    (be(16), be(20))
}

#[test]
fn view_gerber_draws_the_package_fab_writes_from_the_zip_the_folder_and_the_files() {
    let dir = netsmith_testkit::TempDir::new("view-gerber");
    for f in ["led.nsm", "led.lock.toml"] {
        std::fs::copy(repo().join("examples").join(f), dir.join(f)).unwrap();
    }
    ok(&netsmith_in(&dir, &["board", "fab", "led.nsm", "--out", "out"]));
    let board = dir.join("out/LedExample");
    let zip = board.join("LedExample-gerbers.zip");
    assert!(zip.is_file());

    let run = netsmith_in(&dir, &["view", "gerber", zip.to_str().unwrap()]);
    ok(&run);
    // The default file is named for the package and the layer, and its path is printed.
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "LedExample-gerbers-top.png");
    let from_zip = std::fs::read(dir.join("LedExample-gerbers-top.png")).unwrap();
    assert!(from_zip.starts_with(b"\x89PNG"));

    // `fab`'s folder opens as its zip, and the files unzipped draw the same picture.
    ok(&netsmith_in(
        &dir,
        &["view", "gerber", board.to_str().unwrap(), "--picture", "folder.png"],
    ));
    let unzipped = dir.join("unzipped");
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip).unwrap()).unwrap();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let to = unzipped.join(entry.name());
        if entry.is_dir() {
            continue;
        }
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        std::fs::write(to, bytes).unwrap();
    }
    ok(&netsmith_in(
        &dir,
        &["view", "gerber", "unzipped", "--picture", "files.png"],
    ));
    assert_eq!(std::fs::read(dir.join("folder.png")).unwrap(), from_zip);
    assert_eq!(std::fs::read(dir.join("files.png")).unwrap(), from_zip);
    // The same package gives the same bytes.
    ok(&netsmith_in(
        &dir,
        &["view", "gerber", zip.to_str().unwrap(), "--picture", "again.png"],
    ));
    assert_eq!(std::fs::read(dir.join("again.png")).unwrap(), from_zip);

    // The summary: every file read, each by its X2 attribute, the board's size and its holes.
    let run = netsmith_in(&dir, &["view", "gerber", "unzipped", "--format", "json"]);
    ok(&run);
    let json: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(json["unread"], serde_json::json!([]));
    assert_eq!(json["origin"]["from"], "outline");
    assert!(json["board"]["width"].as_f64().unwrap() > 5.0, "{json}");
    let layers = json["layers"].as_array().unwrap();
    let top = layers
        .iter()
        .find(|l| l["file"] == "gerber/LedExample-F_Cu.gbr")
        .unwrap_or_else(|| panic!("{json}"));
    assert_eq!(top["function"], "copper");
    assert_eq!(top["layer"], "top");
    assert_eq!(top["identified_by"], "x2");
    assert!(top["apertures"].as_u64().unwrap() > 0);
    assert!(layers
        .iter()
        .filter(|l| l["format"] == "gerber")
        .all(|l| l["identified_by"] == "x2"));
    assert!(!json["holes"].as_array().unwrap().is_empty(), "{json}");
    assert_eq!(json["views"], serde_json::json!(["top", "bottom", "all"]));

    // The bottom, as an SVG, and a region of the top at a scale.
    ok(&netsmith_in(
        &dir,
        &[
            "view", "gerber", "unzipped", "--layer", "bottom", "--format", "svg", "--picture",
            "bottom.svg",
        ],
    ));
    let svg = std::fs::read_to_string(dir.join("bottom.svg")).unwrap();
    assert!(svg.contains("data-show=\"bottom\""));
    assert!(svg.contains("data-file=\"gerber/LedExample-B_Cu.gbr\""));
    ok(&netsmith_in(
        &dir,
        &[
            "view", "gerber", "unzipped", "--region", "0,0,5,4", "--scale", "40", "--picture",
            "region.png",
        ],
    ));
    assert_eq!(size(&std::fs::read(dir.join("region.png")).unwrap()), (200, 160));

    // A layer the package has no file for is a mistake that lists the ones it has.
    let run = netsmith_in(&dir, &["view", "gerber", "unzipped", "--layer", "inner1"]);
    assert_eq!(run.status.code(), Some(2));
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(
        err.contains("no file for `inner1`; it has: top, bottom, all"),
        "{err}"
    );
}

#[test]
fn view_gerber_names_the_files_it_cannot_read_and_draws_the_rest() {
    let dir = netsmith_testkit::TempDir::new("view-gerber-unread");
    let package = repo().join("crates/netsmith-fab/tests/data/gerber-view/tracespace-unsupported");
    let run = netsmith_in(
        &dir,
        &[
            "view",
            "gerber",
            package.to_str().unwrap(),
            "--layer",
            "all",
            "--picture",
            "all.png",
        ],
    );
    assert_eq!(run.status.code(), Some(1));
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(
        err.contains("error: one-polarity.gbr: line 4: unsupported command `%SRX3Y2I1J1`"),
        "{err}"
    );
    assert!(err.contains("arc-route.drl"), "{err}");
    assert!(dir.join("all.png").is_file(), "the rest is drawn");

    let run = netsmith_in(
        &dir,
        &["view", "gerber", package.to_str().unwrap(), "--format", "json"],
    );
    assert_eq!(run.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let unread = json["unread"].as_array().unwrap();
    assert_eq!(unread.len(), 2, "{json}");
    assert_eq!(unread[1]["file"], "one-polarity.gbr");
    assert_eq!(unread[1]["line"], 4);

    // No such package.
    let run = netsmith_in(&dir, &["view", "gerber", "nothing.zip"]);
    assert_eq!(run.status.code(), Some(2));
}
