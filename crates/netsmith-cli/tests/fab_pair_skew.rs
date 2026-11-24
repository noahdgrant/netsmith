//! `board fab` judges a diff pair over its skew limit (issue #1597): the check `fab` runs has the
//! board's pairs, as `board drc` does, so a pair 1 mm out of skew is a `pair_skew` warning of
//! `fab-notes.txt` and an error that stops `fab --strict`, and a matched pair is none.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use netsmith_testkit::TempDir;

/// Two headers joined pin to pin, the two lines stated a diff pair with a `DiffPair`.
const LINK: &str = "\
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::DiffPair

board Link {
    .process: TwoLayer
    .process.stackup.thickness &= 1.6mm
    .near: Header1x2
    .far: Header1x2
    .lines: DiffPair
    .near.p1 ~ .far.p1
    .near.p2 ~ .far.p2
    .lines.p ~ .near.p1
    .lines.n ~ .near.p2
}
";

const PHYSICAL: &str = "\
import std::pcb::Outline

physical for Link {
    case {
        .outline: Outline {
            .width &= 60mm
            .height &= 20mm
        }
    }
}
";

const LAYOUT: &str = "\
version = 1

[Link.parts.near]
at = [10.0, 10.0]

[Link.parts.far]
at = [50.0, 10.0]
";

fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn said(run: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    )
}

fn project(name: &str) -> TempDir {
    let dir = TempDir::new(name);
    fs::write(dir.join("link.nsm"), LINK).unwrap();
    fs::write(dir.join("link.physical.nsm"), PHYSICAL).unwrap();
    fs::write(dir.join("link.pcb.toml"), LAYOUT).unwrap();
    dir
}

/// Copper in the routes file that leaves the pair about 1 mm out of skew: the second net a
/// straight 40 mm, the first the same with a trapezoid of 45 degree sides 1.2 mm high on the
/// way, which adds 0.99 mm. The router keeps both: it has no connection left to route.
fn skewed_project(name: &str) -> TempDir {
    let dir = project(name);
    let run = netsmith(
        &dir,
        &["routes", "route", "net-far.p2", "--file", "link.nsm"],
    );
    assert!(run.status.success(), "{}", said(&run));
    for (from, to) in [
        ("50,10", "46,10"),
        ("46,10", "44.8,8.8"),
        ("44.8,8.8", "41.2,8.8"),
        ("41.2,8.8", "40,10"),
        ("40,10", "10,10"),
    ] {
        let run = netsmith(
            &dir,
            &[
                "routes", "add-track", "--file", "link.nsm", "--net", "net-far.p1", "--from",
                from, "--to", to,
            ],
        );
        assert!(run.status.success(), "{}", said(&run));
    }
    dir
}

fn notes(dir: &Path, out: &str) -> String {
    fs::read_to_string(dir.join(out).join("Link/fab-notes.txt")).unwrap()
}

#[test]
fn a_pair_1_mm_out_of_skew_is_a_warning_of_the_notes_and_an_error_of_strict() {
    let dir = skewed_project("fab-pair-skew");
    let run = netsmith(&dir, &["board", "fab", "link.nsm", "--out", "out"]);
    let text = said(&run);
    assert!(run.status.success(), "{text}");
    let notes = notes(&dir, "out");
    let line = notes
        .lines()
        .find(|l| l.contains("[pair_skew]"))
        .unwrap_or_else(|| panic!("no pair_skew in:\n{notes}"));
    assert!(line.trim_start().starts_with("warning [pair_skew] diff pair net-far.p1/net-far.p2 is routed 40.99 and 40 mm, a skew of 0.99 mm over its limit of 0.1 mm"), "{line}");
    assert!(notes.contains("design rules warnings: 1"), "{notes}");

    // `--strict` makes it an error that stops `fab`, with no package.
    let run = netsmith(
        &dir,
        &["board", "fab", "link.nsm", "--out", "strict", "--strict"],
    );
    let text = said(&run);
    assert_eq!(run.status.code(), Some(1), "{text}");
    assert!(text.contains("error [pair_skew] diff pair net-far.p1/net-far.p2"), "{text}");
    assert!(text.contains("no package written"), "{text}");
    assert!(!dir.join("strict/Link/fab-notes.txt").exists());
}

#[test]
fn a_pair_the_router_matched_is_no_finding_with_or_without_strict() {
    let dir = project("fab-pair-matched");
    let run = netsmith(&dir, &["board", "fab", "link.nsm", "--out", "out", "--strict"]);
    let text = said(&run);
    assert!(run.status.success(), "{text}");
    let notes = notes(&dir, "out");
    assert!(!notes.contains("pair_skew") && !notes.contains("pair skew"), "{notes}");
}
