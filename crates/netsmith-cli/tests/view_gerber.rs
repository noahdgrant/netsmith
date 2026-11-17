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
    // The layers `--layer` takes by their words (issue #1143): the views, then each function.
    let names: Vec<&str> = json["layer_names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    assert_eq!(names[..3], ["top", "bottom", "all"]);
    for word in ["top-copper", "top-mask", "top-paste", "top-silk", "bottom-copper", "outline", "drill"] {
        assert!(names.contains(&word), "{word} in {names:?}");
    }
    assert_eq!(top["alone"], "top-copper");

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

    // A layer the package has no file for, and a word that is no layer, are mistakes that list
    // the ones the package has.
    let has = names.join(", ");
    let run = netsmith_in(&dir, &["view", "gerber", "unzipped", "--layer", "inner1"]);
    assert_eq!(run.status.code(), Some(2));
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(
        err.contains(&format!("no file for `inner1`; it has: {has}")),
        "{err}"
    );
    let run = netsmith_in(&dir, &["view", "gerber", "unzipped", "--layer", "top", "--layer", "middle"]);
    assert_eq!(run.status.code(), Some(2));
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(
        err.contains(&format!("`middle` is not a layer; the package has: {has}")),
        "{err}"
    );

    // One function alone, and a stack: the files drawn are those of the words asked for.
    let drawn = |layers: &[&str], picture: &str| -> String {
        let mut args = vec!["view", "gerber", "unzipped", "--format", "svg", "--picture", picture];
        for l in layers {
            args.extend(["--layer", l]);
        }
        ok(&netsmith_in(&dir, &args));
        std::fs::read_to_string(dir.join(picture)).unwrap()
    };
    let paste = drawn(&["top-paste"], "paste.svg");
    assert!(paste.contains("data-show=\"top-paste\""));
    assert!(paste.contains("data-function=\"paste\"") && !paste.contains("data-function=\"copper\""));
    let stack = drawn(&["bottom-copper", "drill"], "stack.svg");
    assert!(stack.contains("data-show=\"bottom-copper+drill\""));
    assert!(stack.contains("-B_Cu.gbr") && !stack.contains("-F_Cu.gbr"));
    // The default file of a stack is named for its layers.
    let run = netsmith_in(&dir, &["view", "gerber", "unzipped", "--layer", "top-paste", "--layer", "drill"]);
    ok(&run);
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "unzipped-top-paste+drill.png");

    // Golden pictures of the paste alone and the drill alone of this board.
    let mut pictures = Vec::new();
    for word in ["top-paste", "drill"] {
        let name = format!("{word}.png");
        ok(&netsmith_in(&dir, &["view", "gerber", "unzipped", "--layer", word, "--picture", &name]));
        pictures.push(format!("led-{word} {:016x}", fnv(&std::fs::read(dir.join(&name)).unwrap())));
    }
    golden(&pictures);
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Compares the pictures' hashes (FNV-1a 64 of the PNG bytes) with `tests/data/view-gerber.golden`,
/// or rewrites it with `NETSMITH_VIEW_GERBER_BLESS=1`; look at the pictures first (they are in the
/// test's folder, kept with `NETSMITH_KEEP_TEST_DIRS=1`).
fn golden(pictures: &[String]) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/view-gerber.golden");
    let now = pictures.join("\n") + "\n";
    if std::env::var_os("NETSMITH_VIEW_GERBER_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, now).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        now,
        want,
        "a picture changed: look at it, then rewrite {} with NETSMITH_VIEW_GERBER_BLESS=1",
        path.display()
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
