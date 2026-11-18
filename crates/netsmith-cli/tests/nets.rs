//! `netsmith board nets` (issue #1223): the table of a board's nets and diff pairs, from the
//! copper `view pcb` draws, routed first with `--route`.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

/// Two headers joined pin to pin, the two lines stated a diff pair with a `DiffPair`.
const LINK: &str = r#"import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::DiffPair

/// Two lines of a link that the board routes as a pair, with no impedance stated.
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
"#;

fn netsmith(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("NETSMITH_NO_KICAD", "1")
        .output()
        .unwrap()
}

fn json(cwd: &Path, args: &[&str]) -> serde_json::Value {
    let run = netsmith(cwd, &[args, &["--format", "json"]].concat());
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    // One JSON document on stdout, and nothing else there.
    serde_json::from_slice(&run.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        )
    })
}

#[test]
fn the_routed_pair_has_both_lengths_and_its_skew_within_the_target() {
    let dir = TempDir::new("board-nets");
    fs::write(dir.join("link.nsm"), LINK).unwrap();
    // Nothing routed: no net has copper, so the table is empty.
    let bare = json(&dir, &["board", "nets", "link.nsm"]);
    assert_eq!(bare["board"], "Link");
    assert_eq!(bare["nets"].as_array().unwrap().len(), 0, "{bare}");
    assert_eq!(bare["pairs"].as_array().unwrap().len(), 0, "{bare}");
    let routed = json(&dir, &["board", "nets", "link.nsm", "--route"]);
    let nets = routed["nets"].as_array().unwrap();
    assert_eq!(nets.len(), 2, "{routed}");
    for n in nets {
        assert!(n["length"].as_f64().unwrap() > 0.0, "{n}");
        assert!(n["ratio"].as_f64().unwrap() >= 1.0 - 1e-9, "{n}");
        assert_eq!(n["layer_changes"].as_u64(), Some(0), "{n}");
    }
    let pairs = routed["pairs"].as_array().unwrap();
    assert_eq!(pairs.len(), 1, "{routed}");
    let pair = &pairs[0];
    println!("{pair}");
    let (lp, ln) = (
        pair["length_p"].as_f64().unwrap(),
        pair["length_n"].as_f64().unwrap(),
    );
    // The pair's two lengths are the two nets' rows, and its skew their difference.
    let length = |net: &serde_json::Value| -> f64 {
        nets.iter().find(|n| &n["net"] == net).unwrap()["length"]
            .as_f64()
            .unwrap()
    };
    assert!((length(&pair["p"]) - lp).abs() < 1e-9 && (length(&pair["n"]) - ln).abs() < 1e-9);
    assert!((pair["skew"].as_f64().unwrap() - (lp - ln).abs()).abs() < 1e-9);
    assert_eq!(pair["over"], false, "{pair}");
    assert_eq!(pair["limit"].as_f64(), Some(0.1));
    // The coupled length is a share of the first net's (the router's tests measure how much).
    let (coupled, share) = (
        pair["coupled"].as_f64().unwrap(),
        pair["share"].as_f64().unwrap(),
    );
    assert!((coupled - share * lp).abs() < 1e-6, "{pair}");
    // The text has a row for each net, then the pair.
    let text = netsmith(&dir, &["board", "nets", "link.nsm", "--route", "--lengths"]);
    let text = String::from_utf8_lossy(&text.stdout).into_owned();
    assert!(text.starts_with("Link:\nnet "), "{text}");
    assert!(text.contains("pair "), "{text}");
    assert!(text.contains("within the 0.1 mm limit"), "{text}");
}
