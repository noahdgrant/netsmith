//! `netsmith catalog refresh` and `catalog age` (ADR 0039), against a fake server that answers
//! from responses saved from the real endpoints. No test reaches the network.

use crate::common;

use netsmith_testkit::TempDir;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::endpoints::{Fake, Reply};
use netsmith_fabricate::refresh::{self};

/// An empty directory of its own.
fn dir(name: &str) -> TempDir {
    TempDir::new(&format!("refresh-test-{name}"))
}

fn write(root: &Path, file: &str, text: &str) -> PathBuf {
    let path = root.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, text).unwrap();
    path
}

const HEADER: &str =
    "lcsc,mpn,name,label,resistance,package,library,price,stock,date,checked,source,notes\n";

/// A resistor catalog with rows for parts the saved responses know, and a `checked` column
/// that nothing has filled yet.
fn small_catalog() -> String {
    let mut text = String::from(HEADER);
    // C25104: Basic 0402 330R, a day old, a price and stock that have moved.
    text.push_str("C25104,0402WGF3300TCE,330 ohm,330,330ohm +/- 1%,0402,Basic,0.0011,2079055,2026-11-01,,\"jlcpcb-api:https://x ; lcsc-api:https://y\",\"a note, with a comma\"\n");
    // C25102: Extended, and JLCPCB flags it Preferred.
    text.push_str("C25102,0402WGF3000TCE,300 ohm,300,300ohm +/- 1%,0402,Extended,0.0019,679000,2026-10-31,,src,\n");
    // A made-up row has no lcsc number: nothing to look up.
    text.push_str(",,made up 1k,1k,1kohm,0402,,,,,,,\n");
    text
}

// ---- the network -----------------------------------------------------------------------

// ---- the files -------------------------------------------------------------------------

// ---- age -------------------------------------------------------------------------------

// ---- the command -----------------------------------------------------------------------

fn netsmith(cwd: &Path, fake: &Fake, args: &[&str]) -> std::process::Output {
    let ends = fake.endpoints();
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .current_dir(cwd)
        .env(refresh::LCSC_URL_VAR, &ends.lcsc)
        .env("NETSMITH_CACHE_DIR", cwd.join("cache"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn the_command_refreshes_the_projects_catalog_prints_the_summary_and_exits_1_on_a_finding() {
    let root = dir("command");
    write(&root, "netsmith.toml", "[project]\nname = \"acme\"\n");
    let path = write(&root, "catalogs/acme_res.csv", &small_catalog());
    let fake = Fake::fixtures();

    let dry = netsmith(
        &root,
        &fake,
        &["catalog", "refresh", "--dry-run", "--delay", "0"],
    );
    assert_eq!(
        dry.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&dry.stderr)
    );
    let out = String::from_utf8_lossy(&dry.stdout);
    assert!(out.contains("--- catalogs/acme_res.csv\n"), "{out}");
    // The command uses the real clock: the diff has the `checked` cell and no other.
    assert!(out.contains("-checked: \n+checked: 20"), "{out}");
    assert!(
        !out.contains("-stock") && !out.contains("-price") && !out.contains("-date"),
        "{out}"
    );
    assert!(out.contains("would change"), "{out}");
    assert_eq!(fs::read_to_string(&path).unwrap(), small_catalog());
    assert!(String::from_utf8_lossy(&dry.stderr).contains("acme_res 1/3: C25104"));
    // The command says plainly what it does not do, why, what it does, and what is unchecked.
    for words in [
        "JLCPCB data is not fetched",
        "This is a check against LCSC's product record",
        "Never written: price, stock, price_breaks, library, date.",
        "LCSC's terms of use have not been read",
        "Official APIs (JLCAPI, the LCSC API)",
        "future work",
        "acme_res.csv: 2 rows looked up, 2 verified, 0 conflict, 0 missing, 0 skipped; would change",
    ] {
        assert!(out.contains(words), "{out}\nlacks {words}");
    }
    // A leftover variable for JLCPCB's address is not read: nothing is sent there.
    let jlcpcb = Fake::start(|_| Reply::status(500, "never asked"));
    let again = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .current_dir(&root)
        .env(
            "NETSMITH_JLCPCB_URL",
            format!("http://127.0.0.1:{}/jlcpcb", jlcpcb.port),
        )
        .env(refresh::LCSC_URL_VAR, fake.endpoints().lcsc)
        .env("NETSMITH_CACHE_DIR", root.join("cache"))
        .args(["catalog", "refresh", "--dry-run", "--delay", "0"])
        .output()
        .unwrap();
    assert_eq!(again.status.code(), Some(0));
    assert!(jlcpcb.seen().is_empty());
    // An address of JLCPCB's for the LCSC variable stops the run (exit 2), and nothing is sent.
    let refused = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .current_dir(&root)
        .env(refresh::LCSC_URL_VAR, "https://jlcpcb.com/api/x")
        .args(["catalog", "refresh", "--dry-run", "--no-cache"])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(2));
    let err = String::from_utf8_lossy(&refused.stderr);
    assert!(
        err.contains("does not fetch JLCPCB data") && err.contains("JLCPCB data is not fetched"),
        "{err}"
    );

    let real = netsmith(&root, &fake, &["catalog", "refresh", "--delay", "0"]);
    assert_eq!(real.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&real.stdout).contains("catalogs/acme_res.csv: 2 rows looked up")
    );
    let written = fs::read_to_string(&path).unwrap();
    assert!(
        written.contains(",0.0011,2079055,2026-11-01,2026-") && !written.contains("1085700"),
        "{written}"
    );

    // The records no longer agree with the row about the manufacturer's number.
    fs::write(&path, small_catalog().replace("0402WGF3300TCE", "OTHER")).unwrap();
    let bad = netsmith(&root, &fake, &["catalog", "refresh", "--offline"]);
    // Offline, with what the earlier runs kept (the cache is in the test's directory).
    assert_eq!(
        bad.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&bad.stderr)
    );
    assert!(
        String::from_utf8_lossy(&bad.stdout).contains("CONFLICT catalogs/acme_res.csv:2 C25104:")
    );

    // An endpoint that refuses stops the run with exit code 2.
    let blocked = Fake::start(|_| Reply::status(403, "no"));
    let stopped = netsmith(
        &root,
        &blocked,
        &["catalog", "refresh", "--no-cache", "--delay", "0"],
    );
    assert_eq!(stopped.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&stopped.stderr).contains("error: LCSC: answered 403"));
}

#[test]
fn the_command_lists_old_rows() {
    let root = dir("command-age");
    let fake = Fake::fixtures();
    write(
        &root,
        "r.csv",
        "lcsc,price,date\nC1,1,2020-01-01\nC2,1,2999-01-01\nC3,,\n",
    );
    let out = netsmith(&root, &fake, &["catalog", "age", "r.csv", "--days", "30"]);
    assert_eq!(out.status.code(), Some(0));
    let out = String::from_utf8_lossy(&out.stdout);
    assert!(out.starts_with("r.csv:2 C1: read 2020-01-01, "), "{out}");
    assert!(
        out.contains("r.csv:4 C3: price and stock unknown (no supply provider"),
        "{out}"
    );
    assert!(
        out.contains("1 of 3 rows have a price and stock read more than 30 days before 20"),
        "{out}"
    );
    assert!(out.contains("1 of 3 have them unknown"), "{out}");
}

#[test]
fn the_help_of_the_catalog_commands_names_every_option_and_status() {
    let mut cli = crate::cli_help::model();
    let catalog = cli.find_subcommand_mut("catalog").expect("catalog");
    let mut help = |name: &str| {
        catalog
            .find_subcommand_mut(name)
            .expect(name)
            .render_long_help()
            .to_string()
    };
    let (refresh, age) = (help("refresh"), help("age"));
    for word in [
        "--dry-run",
        "--offline",
        "--only",
        "--lcsc",
        "--delay",
        "--cache-ttl",
        "--no-cache",
        "CONFLICT",
        "MISSING",
        "SKIPPED",
        "ADR 0039",
    ] {
        assert!(
            refresh.contains(word),
            "`catalog refresh --help` lacks `{word}`"
        );
    }
    for word in ["--days", "--only", "[default: 90]"] {
        assert!(age.contains(word), "`catalog age --help` lacks `{word}`");
    }
}
