//! `netsmith catalog refresh` and `catalog age` (ADR 0039), against a fake server that answers
//! from responses saved from the real endpoints. No test reaches the network. A refresh checks the
//! project's LCSC offers (`offers/lcsc.csv`, ADR 0143); a test writes its rows with the old `lcsc`
//! and `library` columns and moves them with the migration (`project`).

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

/// Write the catalog `catalogs/<name>.csv` of `root` from a text with the old columns, as the
/// migration moves it, with its offers in `offers/`; the path of the LCSC offers file.
fn project(root: &Path, name: &str, text: &str) -> PathBuf {
    netsmith_solve::migrate::write_to(root, &format!("catalogs/{name}.csv"), text).unwrap();
    root.join("offers/lcsc.csv")
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
    let path = project(&root, "acme_res", &small_catalog());
    let offers = fs::read_to_string(&path).unwrap();
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
    assert!(out.contains("--- offers/lcsc.csv\n"), "{out}");
    // The command uses the real clock: the diff has the `checked` cell and no other.
    assert!(out.contains("-checked: \n+checked: 20"), "{out}");
    assert!(
        !out.contains("-stock") && !out.contains("-price") && !out.contains("-date"),
        "{out}"
    );
    assert!(out.contains("would change"), "{out}");
    assert_eq!(fs::read_to_string(&path).unwrap(), offers);
    assert!(String::from_utf8_lossy(&dry.stderr).contains("acme_res 1/2: C25104"));
    // The command says plainly what it does not do, why, what it does, and what is unchecked.
    for words in [
        "JLCPCB data is not fetched",
        "This is a check of LCSC's offers (offers/lcsc.csv) against LCSC's product record",
        "Never written: price, stock, price_breaks, library, date.",
        "LCSC's terms of use have not been read",
        "Official APIs (JLCAPI, the LCSC API)",
        "future work",
        "offers/lcsc.csv: 2 offers looked up, 2 verified, 0 conflict, 0 missing, 0 skipped; would change",
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
        String::from_utf8_lossy(&real.stdout).contains("offers/lcsc.csv: 2 offers looked up")
    );
    let written = fs::read_to_string(&path).unwrap();
    assert!(
        written.contains("lcsc-api:https://y,20") && !written.contains("1085700"),
        "{written}"
    );

    // The records no longer agree with the offer about the manufacturer's number.
    fs::write(&path, offers.replace("0402WGF3300TCE", "OTHER")).unwrap();
    let bad = netsmith(&root, &fake, &["catalog", "refresh", "--offline"]);
    // Offline, with what the earlier runs kept (the cache is in the test's directory).
    assert_eq!(
        bad.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&bad.stderr)
    );
    assert!(
        String::from_utf8_lossy(&bad.stdout).contains("CONFLICT offers/lcsc.csv:2 C25104:")
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
    project(&root, "r", AGED);
    let out = netsmith(&root, &fake, &["catalog", "age", "catalogs/r.csv", "--days", "30"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let out = String::from_utf8_lossy(&out.stdout);
    assert!(out.starts_with("catalogs/r.csv:2 C1: read 2020-01-01, "), "{out}");
    assert!(
        out.contains("catalogs/r.csv:4 C3: price and stock unknown (no supply provider"),
        "{out}"
    );
    assert!(
        out.contains("1 of 3 rows have a price and stock read more than 30 days before 20"),
        "{out}"
    );
    assert!(out.contains("1 of 3 have them unknown"), "{out}");
}

/// Three rows of old prices, each with its LCSC number (moved into an offer).
const AGED: &str = "lcsc,mpn,price,date,source\nC1,ZZ-1,1,2020-01-01,s\nC2,ZZ-2,1,2999-01-01,s\nC3,ZZ-3,,,s\n";

fn json(run: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&run.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not one JSON document ({e}): {}",
            String::from_utf8_lossy(&run.stdout)
        )
    })
}

#[test]
fn refresh_prints_one_json_document_with_what_the_text_says_and_the_same_exit_codes() {
    let root = dir("command-json");
    write(&root, "netsmith.toml", "[project]\nname = \"acme\"\n");
    let path = project(&root, "acme_res", &small_catalog());
    let offers = fs::read_to_string(&path).unwrap();
    let fake = Fake::fixtures();

    let dry = netsmith(
        &root,
        &fake,
        &["catalog", "refresh", "--dry-run", "--delay", "0", "--format", "json"],
    );
    assert_eq!(dry.status.code(), Some(0));
    let doc = json(&dry);
    assert_eq!(doc["command"], "catalog refresh");
    assert_eq!(doc["ok"], true);
    assert_eq!(doc["dry_run"], true);
    let file = &doc["files"][0];
    assert_eq!(file["file"], "offers/lcsc.csv");
    assert_eq!(file["catalog"], "lcsc");
    // The text's "2 rows looked up, 2 verified, 0 conflict, 0 missing, 0 skipped".
    assert_eq!(
        (file["looked_up"].as_u64(), file["verified"].as_u64()),
        (Some(2), Some(2))
    );
    assert_eq!(
        (file["conflict"].as_u64(), file["missing"].as_u64(), file["skipped"].as_u64()),
        (Some(0), Some(0), Some(0))
    );
    assert_eq!(file["without_lcsc"], 0);
    assert_eq!(file["written"], false);
    // The diff of the text: the `checked` cell of the first row, and no other.
    let row = &file["rows"][0];
    assert_eq!((row["line"].as_u64(), row["lcsc"].as_str()), (Some(2), Some("C25104")));
    assert_eq!(row["status"], "verified");
    assert_eq!(row["changes"][0]["column"], "checked");
    assert_eq!(row["changes"][0]["from"], "");
    assert!(row["changes"][0]["to"].as_str().unwrap().starts_with("20"));
    assert_eq!(fs::read_to_string(&path).unwrap(), offers);

    // An offer the records no longer agree with is a finding: exit code 1, `ok` false, and its `why`.
    fs::write(&path, offers.replace("0402WGF3300TCE", "OTHER")).unwrap();
    let bad = netsmith(&root, &fake, &["catalog", "refresh", "--offline", "--format", "json"]);
    assert_eq!(bad.status.code(), Some(1));
    let doc = json(&bad);
    assert_eq!(doc["ok"], false);
    let conflict = doc["files"][0]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["status"] == "conflict")
        .unwrap_or_else(|| panic!("no conflict in {doc}"));
    assert_eq!(conflict["lcsc"], "C25104");
    assert!(conflict["why"].as_str().is_some_and(|w| !w.is_empty()));
}

#[test]
fn age_prints_the_old_and_the_unknown_rows_as_json() {
    let root = dir("command-age-json");
    let fake = Fake::fixtures();
    project(&root, "r", AGED);
    let out = netsmith(
        &root,
        &fake,
        &["catalog", "age", "catalogs/r.csv", "--days", "30", "--format", "json"],
    );
    assert_eq!(out.status.code(), Some(0));
    let doc = json(&out);
    assert_eq!((doc["command"].as_str(), doc["ok"].as_bool()), (Some("catalog age"), Some(true)));
    assert_eq!((doc["rows"].as_u64(), doc["days"].as_u64()), (Some(3), Some(30)));
    // `r.csv:2 C1: read 2020-01-01, N days ago`.
    let old = &doc["old"];
    assert_eq!(old.as_array().unwrap().len(), 1);
    assert_eq!(
        (old[0]["file"].as_str(), old[0]["line"].as_u64(), old[0]["lcsc"].as_str(), old[0]["date"].as_str()),
        (Some("catalogs/r.csv"), Some(2), Some("C1"), Some("2020-01-01"))
    );
    assert!(old[0]["days"].as_i64().unwrap() > 30);
    // `r.csv:4 C3: price and stock unknown (no supply provider ...)`.
    let unknown = &doc["unknown"];
    assert_eq!(unknown.as_array().unwrap().len(), 1);
    assert_eq!((unknown[0]["line"].as_u64(), unknown[0]["lcsc"].as_str()), (Some(4), Some("C3")));
    assert!(unknown[0]["why"].as_str().unwrap().starts_with("no supply provider"));
}

#[test]
fn add_prints_the_record_and_the_row_written_as_json() {
    let root = dir("command-add-json");
    let fake = Fake::fixtures();
    let record = netsmith(
        &root,
        &fake,
        &["catalog", "add", "--lcsc", "C25104", "--no-cache", "--format", "json"],
    );
    assert_eq!(
        record.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&record.stderr)
    );
    let doc = json(&record);
    assert_eq!((doc["command"].as_str(), doc["ok"].as_bool()), (Some("catalog add"), Some(true)));
    assert_eq!(doc["lcsc"], "C25104");
    assert_eq!(doc["mpn"], "0402WGF3300TCE");
    assert_eq!(doc["package"], "0402");
    // With no `--to`, nothing is written.
    assert!(doc["written"].is_null());

    let to = write(&root, "res.csv", "mpn,package,source\n");
    let added = netsmith(
        &root,
        &fake,
        &["catalog", "add", "--lcsc", "C25104", "--to", "res.csv", "--no-cache", "--format", "json"],
    );
    assert_eq!(added.status.code(), Some(0));
    let doc = json(&added);
    assert_eq!(doc["written"]["file"], "res.csv");
    assert_eq!(doc["written"]["line"], 2);
    assert!(doc["filled"].as_array().unwrap().iter().any(|c| c == "mpn"));
    assert!(
        doc["to_fill"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c.as_str().unwrap().starts_with("a JLCPCB offer"))
    );
    // The row names the part, and its number is an offer of LCSC's beside it (no project here,
    // so `offers/lcsc.csv` in the folder the command ran in).
    assert!(fs::read_to_string(&to).unwrap().contains("0402WGF3300TCE"));
    assert!(fs::read_to_string(&to).unwrap().starts_with("mpn,package,source\n"));
    assert_eq!(doc["offer"]["file"], "offers/lcsc.csv");
    assert!(fs::read_to_string(root.join("offers/lcsc.csv")).unwrap().contains(",0402WGF3300TCE,C25104,"));

    // The number is in the offers now, so a second add is refused: exit code 1 and a finding.
    let again = netsmith(
        &root,
        &fake,
        &["catalog", "add", "--lcsc", "C25104", "--to", "res.csv", "--no-cache", "--format", "json"],
    );
    assert_eq!(again.status.code(), Some(1));
    let doc = json(&again);
    assert_eq!(doc["ok"], false);
    assert_eq!(doc["findings"][0]["severity"], "error");
    assert_eq!(doc["findings"][0]["code"], "catalog");
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
