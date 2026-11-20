//! `netsmith catalog refresh` and `catalog age` (ADR 0039), against a fake server that answers
//! from responses saved from the real endpoints. No test reaches the network. A refresh checks
//! the offers of LCSC (`offers/lcsc.csv`, ADR 0143) and writes their `checked` cell; the catalogs
//! it reads for the package and the stock are never written. A test writes its rows with the
//! `lcsc` and `library` columns a row had before ADR 0143 and moves them with the migration
//! (`project`), so that the rows and their offers read as one table here.

use crate::common;

use netsmith_testkit::TempDir;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::endpoints::{Fake, Reply, Seen, fixture_reply};
use netsmith_fabricate::refresh::{
    self, Options, Status, Targets, age, refresh as run, render_age, render_diff, render_summary,
    resolve_targets,
};
use netsmith_pipeline::supply::Supply;
use netsmith_solve::parse_csv;

/// 2026-11-02 12:00 UTC.
const NOW: u64 = 1_793_620_800;
const TODAY: &str = "2026-11-02";

const STD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/jlcpcb/catalog");
const STD_OFFERS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/offers");

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

fn options(fake: &Fake, cache: Option<&Path>) -> Options {
    Options {
        network: fake.network(cache, NOW),
        ..Options::default()
    }
}

fn quiet(_: &str) {}

/// The cells of a file, by row, each row by column name.
fn rows(text: &str) -> Vec<BTreeMap<String, String>> {
    let records = parse_csv(text).unwrap();
    let (_, header) = &records[0];
    records[1..]
        .iter()
        .map(|(_, f)| header.iter().cloned().zip(f.iter().cloned()).collect())
        .collect()
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

/// Write the catalog `catalogs/<name>.csv` of `root` from `text`, a catalog with the old `lcsc`
/// and `library` columns, as the migration moves it: the file without the two columns, and its
/// offers in `offers/lcsc.csv` and `offers/jlcpcb.csv` (added to the ones there). The path of
/// the catalog file and of LCSC's offers file.
fn project(root: &Path, name: &str, text: &str) -> (PathBuf, PathBuf) {
    use netsmith_solve::migrate;
    let file = format!("catalogs/{name}.csv");
    let m = migrate::catalog(&file, text).unwrap().unwrap();
    let catalog = write(root, &file, &m.catalog);
    for supplier in ["jlcpcb", "lcsc"] {
        let path = root.join(format!("offers/{supplier}.csv"));
        let mut offers =
            fs::read_to_string(&path).unwrap_or_else(|_| migrate::OFFERS_HEADER.to_string());
        let mine: Vec<_> = m
            .offers
            .iter()
            .filter(|o| o.supplier == supplier)
            .cloned()
            .collect();
        offers.push_str(&migrate::offer_lines(&mine));
        write(root, &format!("offers/{supplier}.csv"), &offers);
    }
    (catalog, root.join("offers/lcsc.csv"))
}

/// The catalogs of `root` and the offers beside them.
fn target(root: &Path) -> Targets {
    resolve_targets(root, &[PathBuf::from("catalogs")]).unwrap()
}

#[test]
fn a_verified_offer_gets_the_day_in_checked_and_every_other_byte_is_as_it_was() {
    let root = dir("verify");
    let (catalog, path) = project(&root, "resistor", &small_catalog());
    let rows_before = fs::read_to_string(&catalog).unwrap();
    let old = fs::read_to_string(&path).unwrap();
    let fake = Fake::fixtures();
    let report = run(&target(&root), &options(&fake, None), &quiet).unwrap();
    let after = fs::read_to_string(&path).unwrap();

    let file = &report.files[0];
    assert_eq!(file.file, "offers/lcsc.csv");
    assert_eq!(file.rows.len(), 2);
    // The made-up row has no part number: no offer, nothing to ask.
    assert_eq!(file.without_lcsc, 0);
    assert!(file.written);
    for row in &file.rows {
        assert_eq!(row.status, Status::Verified, "{}", row.lcsc);
        let changes: Vec<(String, String, String)> = row
            .changes
            .iter()
            .map(|c| (c.column.clone(), c.from.clone(), c.to.clone()))
            .collect();
        assert_eq!(
            changes,
            [("checked".to_string(), String::new(), TODAY.to_string())],
            "{}: the only cell a refresh writes",
            row.lcsc
        );
        // LCSC lists other prices and stock than the rows have (C25104: 1,085,700 against
        // 2,079,055); those are not the board house's and are not even mentioned.
        assert!(row.notes.is_empty(), "{:?}", row.notes);
    }

    // Byte for byte: the old offers with the two empty `checked` cells filled, the quoting and
    // the comma in a source as they were. The catalog file is not written.
    let (old_rows, new_rows) = (rows(&old), rows(&after));
    assert_eq!(old_rows.len(), new_rows.len());
    for (a, b) in old_rows.iter().zip(&new_rows) {
        for (column, was) in a {
            if column != "checked" {
                assert_eq!(&b[column], was, "{column} of {}", a["sku"]);
            }
        }
        assert_eq!((a["checked"].as_str(), b["checked"].as_str()), ("", TODAY));
    }
    assert_eq!(old.lines().count(), after.lines().count());
    assert_eq!(new_rows[0]["source"], "jlcpcb-api:https://x ; lcsc-api:https://y");
    assert_eq!(fs::read_to_string(&catalog).unwrap(), rows_before);

    // The same day again: nothing to write.
    let again = run(&target(&root), &options(&fake, None), &quiet).unwrap();
    assert!(!again.files[0].written && again.files[0].rows.iter().all(|r| r.changes.is_empty()));
    assert_eq!(fs::read_to_string(&path).unwrap(), after);
}

#[test]
fn a_dry_run_prints_the_diff_and_writes_nothing() {
    let root = dir("dry");
    let (_, path) = project(&root, "resistor", &small_catalog());
    let old = fs::read_to_string(&path).unwrap();
    let fake = Fake::fixtures();
    let mut opts = options(&fake, None);
    opts.dry_run = true;
    let report = run(&target(&root), &opts, &quiet).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), old);
    assert!(!report.files[0].written);
    assert_eq!(
        render_diff(&report),
        "--- offers/lcsc.csv\n+++ offers/lcsc.csv (refreshed)\n\
         @@ line 2 C25104 @@\n-checked: \n+checked: 2026-11-02\n\
         @@ line 3 C25102 @@\n-checked: \n+checked: 2026-11-02\n"
    );
    let summary = render_summary(&report);
    assert!(summary.starts_with(refresh::NOTICE), "{summary}");
    for words in [
        "JLCPCB data is not fetched: netsmith asks LCSC only.",
        "This is a check of LCSC's offers (offers/lcsc.csv) against LCSC's product record",
        "gets today's date in its `checked` column",
        "Never written: price, stock, price_breaks, library, date.",
        "LCSC's price and stock differ from them",
        "LCSC's terms of use have not been read",
    ] {
        assert!(summary.contains(words), "{summary}\nlacks {words}");
    }
    assert!(
        summary.contains("offers/lcsc.csv: 2 offers looked up, 2 verified, 0 conflict, 0 missing, 0 skipped; would change"),
        "{summary}"
    );
    assert!(
        summary.contains("total: 2 verified, 0 conflict, 0 missing, 0 skipped; 2 requests to LCSC"),
        "{summary}"
    );
}

#[test]
fn a_part_that_is_not_the_offer_is_a_conflict_and_a_part_that_is_gone_is_missing_and_neither_changes_the_offer()
 {
    let root = dir("findings");
    let mut text = String::from(HEADER);
    // The records say the manufacturer's number is ...3000TCE.
    text.push_str("C25102,0402WGF3000TCX,300 ohm,300,300ohm +/- 1%,0402,Extended,0.0019,679000,2026-10-31,,src,\n");
    // The records say 0402.
    text.push_str(
        "C1549,0402CG180J500NT,18p,18p,18pF,0603,Basic,0.0018,1273978,2026-11-01,,src,\n",
    );
    // LCSC has no record of this one.
    text.push_str(
        "C22369487,RCA0223K7FLF,23.7k,23.7k,23.7kohm,0402,Extended,0.0009,17585,2026-11-01,,src,\n",
    );
    // LCSC has no record of this one either.
    text.push_str("C999999999,GONE,gone,gone,1ohm,0402,Basic,0.01,5,2026-11-01,,src,\n");
    let (_, path) = project(&root, "resistor", &text);
    let old = fs::read_to_string(&path).unwrap();
    let fake = Fake::fixtures();
    let report = run(&target(&root), &options(&fake, None), &quiet).unwrap();

    let file = &report.files[0];
    let why = |i: usize| file.rows[i].status.why().unwrap().to_string();
    assert!(
        matches!(file.rows[0].status, Status::Conflict(_)),
        "{:?}",
        file.rows[0]
    );
    assert_eq!(
        why(0),
        "LCSC lists the manufacturer's number 0402WGF3000TCE, the offer has 0402WGF3000TCX; the offer is as it was"
    );
    assert!(matches!(file.rows[1].status, Status::Conflict(_)));
    assert_eq!(
        why(1),
        "LCSC lists the package 0402, the row at catalogs/resistor.csv:3 has 0603; the offer is as it was"
    );
    assert!(matches!(file.rows[2].status, Status::Missing(_)));
    assert_eq!(
        why(2),
        "LCSC has no record of C22369487; the offer is as it was"
    );
    assert!(matches!(file.rows[3].status, Status::Missing(_)));
    assert!(file.rows.iter().all(|r| r.changes.is_empty()));
    assert!(report.has_findings());
    // Nothing changed, so nothing was written.
    assert!(!file.written);
    assert_eq!(fs::read_to_string(&path).unwrap(), old);
    let summary = render_summary(&report);
    assert!(
        summary.contains("CONFLICT offers/lcsc.csv:2 C25102: LCSC lists the manufacturer's number"),
        "{summary}"
    );
    assert!(
        summary.contains("MISSING offers/lcsc.csv:5 C999999999:"),
        "{summary}"
    );
    assert!(summary.contains("2 conflict, 2 missing"), "{summary}");
}

/// LCSC's answer for C25104 with `stockNumber` set to `stock` (none if `None`).
fn with_lcsc_stock(stock: Option<u64>) -> Fake {
    Fake::start(move |r: &Seen| {
        let mut json: serde_json::Value = serde_json::from_str(&fixture_reply(r).body).unwrap();
        if let Some(record) = json["result"].as_object_mut() {
            match stock {
                Some(n) => record.insert("stockNumber".to_string(), n.into()),
                None => record.remove("stockNumber"),
            };
        }
        Reply::ok(json.to_string())
    })
}

#[test]
fn lcsc_listing_no_stock_is_reported_as_information_and_the_row_keeps_its_stock() {
    let row = |stock: &str| {
        format!(
            "{HEADER}C25104,0402WGF3300TCE,330,330,330ohm,0402,Basic,0.0011,{stock},2026-11-01,,src,\n"
        )
    };
    let go = |name: &str, text: &str, fake: &Fake| {
        let root = dir(name);
        let (catalog, path) = project(&root, "resistor", text);
        let rows_before = fs::read_to_string(&catalog).unwrap();
        let report = run(&target(&root), &options(fake, None), &quiet).unwrap();
        // The stock cell, and every other cell of the row, is not touched.
        assert_eq!(fs::read_to_string(&catalog).unwrap(), rows_before);
        (report, fs::read_to_string(&path).unwrap())
    };
    // LCSC has none, the row has 5: said.
    let fake = with_lcsc_stock(Some(0));
    let (report, after) = go("sold-out-5", &row("5"), &fake);
    assert_eq!(report.files[0].rows[0].status, Status::Verified);
    assert_eq!(
        report.files[0].rows[0].notes,
        [
            "LCSC lists stock 0 where the row has stock 5: the row is not changed; check JLCPCB's parts page by hand"
        ]
    );
    assert_eq!(rows(&after)[0]["checked"], TODAY);
    let summary = render_summary(&report);
    assert!(
        summary.contains(
            "note offers/lcsc.csv:2 C25104: LCSC lists stock 0 where the row has stock 5"
        ),
        "{summary}"
    );
    assert!(!report.has_findings(), "a note is not a finding");

    // The row has no stock entered: said so.
    let (report, _) = go("sold-out-none", &row(""), &fake);
    assert_eq!(
        report.files[0].rows[0].notes,
        [
            "LCSC lists stock 0 where the row has no stock entered: the row is not changed; check JLCPCB's parts page by hand"
        ]
    );

    // The row has stock 0 and LCSC has some: said, not changed.
    let (report, _) = go("sold-out-0", &row("0"), &Fake::fixtures());
    assert_eq!(
        report.files[0].rows[0].notes,
        [
            "the row has stock 0 and LCSC lists stock 1085700: the row is not changed; check JLCPCB's parts page by hand"
        ]
    );

    // Both say 0: nothing to say. LCSC says nothing about stock: nothing to say.
    let (report, _) = go("sold-out-both", &row("0"), &with_lcsc_stock(Some(0)));
    assert!(report.files[0].rows[0].notes.is_empty());
    let (report, after) = go("sold-out-quiet", &row("5"), &with_lcsc_stock(None));
    let r = &report.files[0].rows[0];
    assert!(r.notes.is_empty(), "{:?}", r.notes);
    assert_eq!(r.status, Status::Verified);
    assert_eq!(rows(&after)[0]["checked"], TODAY);
}

#[test]
fn a_catalog_file_and_the_offers_of_jlcpcb_are_never_written() {
    let root = dir("never");
    // LCSC's record has a price ladder, a stock and no library; the row has other numbers in
    // every one of those cells, as a hand-entered JLCPCB snapshot would.
    let text = "lcsc,mpn,package,library,price,price_breaks,stock,date,checked,source\n\
                C5446,XC6206P332MR-G,SOT-23,Extended,0.1,1:0.1 100:0.08,7,2026-10-31,,s\n";
    let (catalog, path) = project(&root, "ldo", text);
    let (rows_before, jlcpcb) = (
        fs::read_to_string(&catalog).unwrap(),
        fs::read_to_string(root.join("offers/jlcpcb.csv")).unwrap(),
    );
    let fake = Fake::fixtures();
    let report = run(&target(&root), &options(&fake, None), &quiet).unwrap();
    assert_eq!(report.files.len(), 1, "only LCSC's offers are checked");
    assert_eq!(report.files[0].rows[0].changes.len(), 1);
    assert_eq!(fs::read_to_string(&catalog).unwrap(), rows_before);
    assert_eq!(
        fs::read_to_string(root.join("offers/jlcpcb.csv")).unwrap(),
        jlcpcb
    );
    assert_eq!(rows(&fs::read_to_string(&path).unwrap())[0]["checked"], TODAY);
}

#[test]
fn with_no_offers_file_of_lcsc_there_is_nothing_to_check() {
    let root = dir("no-offers");
    write(
        &root,
        "catalogs/ldo.csv",
        "mpn,package,source\nXC6206P332MR-G,SOT-23,s\n",
    );
    let fake = Fake::fixtures();
    let e = run(&target(&root), &options(&fake, None), &quiet).unwrap_err();
    assert!(
        e.starts_with("there is no offers file of LCSC (`offers/lcsc.csv`) to check"),
        "{e}"
    );
    assert!(fake.seen().is_empty());
}

#[test]
fn only_and_lcsc_choose_what_is_looked_up() {
    let root = dir("choose");
    project(&root, "resistor", &small_catalog());
    project(
        &root,
        "ldo",
        "lcsc,mpn,package,library,price,stock,date,source\nC5446,XC6206P332MR-G,SOT-23,Basic,0.1,1,2026-10-31,s\n",
    );
    let targets = target(&root);
    assert_eq!(targets.catalogs.len(), 2);
    assert_eq!(targets.offers.len(), 2);
    let fake = Fake::fixtures();

    let mut opts = options(&fake, None);
    opts.dry_run = true;
    opts.only = vec!["ldo".to_string()];
    let report = run(&targets, &opts, &quiet).unwrap();
    let looked: Vec<&str> = report.files[0].rows.iter().map(|r| r.lcsc.as_str()).collect();
    assert_eq!(looked, ["C5446"]);

    let mut opts = options(&fake, None);
    opts.dry_run = true;
    opts.lcsc = vec!["C25104".to_string(), "C1".to_string()];
    let report = run(&targets, &opts, &quiet).unwrap();
    let looked: Vec<&str> = report
        .files
        .iter()
        .flat_map(|f| &f.rows)
        .map(|r| r.lcsc.as_str())
        .collect();
    assert_eq!(looked, ["C25104"]);
    assert_eq!(report.unknown_lcsc, ["C1"]);
    assert!(render_summary(&report).contains("MISSING C1: no offer of LCSC has this number"));
}

// ---- the network -----------------------------------------------------------------------

#[test]
fn requests_are_one_at_a_time_with_a_pause_and_say_what_they_are() {
    let root = dir("polite");
    project(&root, "resistor", &small_catalog());
    let fake = Fake::fixtures();
    let mut opts = options(&fake, None);
    opts.dry_run = true;
    opts.network.delay = Duration::from_millis(40);
    run(&target(&root), &opts, &quiet).unwrap();
    let seen = fake.seen();
    assert_eq!(seen.len(), 2, "one LCSC request for each of two parts");
    for s in &seen {
        assert!(
            s.user_agent.starts_with("netsmith-catalog-refresh/"),
            "{}",
            s.user_agent
        );
        assert!(s.user_agent.contains("one request at a time"));
    }
    for pair in seen.windows(2) {
        assert!(
            pair[1].at.duration_since(pair[0].at) >= Duration::from_millis(35),
            "two requests {:?} apart",
            pair[1].at.duration_since(pair[0].at)
        );
    }
    assert_eq!(seen[0].method, "GET");
    assert_eq!(seen[0].target, "/lcsc?productCode=C25104");
    assert_eq!(seen[1].target, "/lcsc?productCode=C25102");
}

#[test]
fn a_cache_answers_again_within_its_time_and_not_after() {
    let root = dir("cache");
    project(&root, "resistor", &small_catalog());
    let cache = root.join("cache");
    let fake = Fake::fixtures();
    let mut opts = options(&fake, Some(&cache));
    opts.dry_run = true;
    run(&target(&root), &opts, &quiet).unwrap();
    assert_eq!(fake.seen().len(), 2);

    let again = run(&target(&root), &opts, &quiet).unwrap();
    assert_eq!(fake.seen().len(), 2, "the second run asked nothing");
    assert_eq!(again.requests, 0);

    // A day and a minute later the kept answers are too old.
    opts.network.now = Some(NOW + 24 * 3600 + 60);
    run(&target(&root), &opts, &quiet).unwrap();
    assert_eq!(fake.seen().len(), 4);
}

#[test]
fn offline_uses_the_cache_whatever_its_age_dates_the_row_by_when_it_was_got_and_skips_the_rest() {
    let root = dir("offline");
    let (_, path) = project(&root, "resistor", &small_catalog());
    let cache = root.join("cache");
    let fake = Fake::fixtures();
    // Fill the cache for C25104 only.
    let mut opts = options(&fake, Some(&cache));
    opts.dry_run = true;
    opts.lcsc = vec!["C25104".to_string()];
    run(&target(&root), &opts, &quiet).unwrap();
    let asked = fake.seen().len();

    // A month later, offline.
    let mut opts = options(&fake, Some(&cache));
    opts.network.offline = true;
    opts.network.now = Some(NOW + 30 * 86_400);
    let report = run(&target(&root), &opts, &quiet).unwrap();
    assert_eq!(fake.seen().len(), asked, "offline asks nothing");
    let looked = &report.files[0].rows;
    assert_eq!(looked[0].status, Status::Verified);
    assert_eq!(
        looked[1].status,
        Status::Skipped("offline and not in the cache".to_string())
    );
    let after = rows(&fs::read_to_string(&path).unwrap());
    assert_eq!(
        after[0]["checked"], "2026-11-02",
        "checked is the day the answer was got (not the day of the run)"
    );
    assert_eq!(after[1]["checked"], "", "the skipped offer is as it was");
}

#[test]
fn a_busy_server_is_asked_again_after_a_wait_and_given_up_on_after_a_few_tries() {
    let root = dir("retry");
    project(&root, "resistor", &small_catalog());
    let count = std::sync::atomic::AtomicU32::new(0);
    let fake = Fake::start(move |r: &Seen| {
        // The first two requests of the run are told to wait.
        match count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) {
            0 => Reply::status(503, "busy"),
            1 => Reply {
                headers: vec![("Retry-After".to_string(), "0".to_string())],
                ..Reply::status(429, "slow down")
            },
            _ => fixture_reply(r),
        }
    });
    let mut opts = options(&fake, None);
    opts.dry_run = true;
    opts.lcsc = vec!["C25104".to_string()];
    let report = run(&target(&root), &opts, &quiet).unwrap();
    assert_eq!(report.files[0].rows[0].status, Status::Verified);
    assert_eq!(fake.seen().len(), 3, "503, 429, then LCSC answered");

    let always = Fake::start(|_| Reply::status(503, "busy"));
    let mut opts = options(&always, None);
    opts.lcsc = vec!["C25104".to_string()];
    let e = run(&target(&root), &opts, &quiet).unwrap_err();
    assert!(
        e.contains("LCSC: answered 503; gave up on C25104 after 3 tries"),
        "{e}"
    );
    assert_eq!(always.seen().len(), 3);
}

#[test]
fn a_refusal_or_a_changed_endpoint_stops_the_run_with_a_message_and_writes_nothing() {
    let root = dir("blocked");
    let (_, path) = project(&root, "resistor", &small_catalog());
    let text = fs::read_to_string(&path).unwrap();
    let check = |fake: Fake, expect: &[&str]| {
        let e = run(&target(&root), &options(&fake, None), &quiet).unwrap_err();
        for part in expect {
            assert!(e.contains(part), "{e}\nlacks {part}");
        }
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            text,
            "nothing was written"
        );
    };
    // Blocked: no retry.
    let blocked = Fake::start(|_| Reply::status(403, "Forbidden"));
    let e = run(&target(&root), &options(&blocked, None), &quiet).unwrap_err();
    assert!(
        e.contains("answered 403: it refuses this client or the endpoint has moved"),
        "{e}"
    );
    assert_eq!(blocked.seen().len(), 1);
    // A web page instead of JSON (a captcha).
    check(
        Fake::start(|_| Reply::ok("<html><body>Please verify you are human</body></html>")),
        &["answered a web page, not JSON", "<html><body>Please verify"],
    );
    // JSON of another shape.
    check(
        Fake::start(|_| Reply::ok(r#"{"code":200,"data":{"results":[]}}"#)),
        &[
            "`result`",
            "may have changed or may be blocking this client; nothing was written",
            "ADR 0039",
        ],
    );
    // The first part answered, the second in a new shape: the first is not written either.
    check(
        Fake::start(|r: &Seen| {
            if r.lcsc() == "C25102" {
                Reply::ok(r#"{"code":200,"result":{"productCode":"C25102"}}"#)
            } else {
                fixture_reply(r)
            }
        }),
        &["`productModel`"],
    );
}

#[test]
fn an_lcsc_number_that_is_not_one_is_refused_and_never_sent() {
    let root = dir("badnumber");
    project(
        &root,
        "resistor",
        &format!("{HEADER}../../etc,X,x,x,1ohm,0402,Basic,1,1,2026-10-31,,s,\n"),
    );
    let fake = Fake::fixtures();
    let e = run(&target(&root), &options(&fake, None), &quiet).unwrap_err();
    assert_eq!(e, "`../../etc` is not an LCSC part number like C25104");
    assert!(fake.seen().is_empty());
}

// ---- the files -------------------------------------------------------------------------

/// A copy of the standard catalog files in `to/catalog` and of the standard offers in
/// `to/offers`; the names of the catalog files.
fn copy_std(to: &Path) -> Vec<String> {
    let mut names = Vec::new();
    for (from, into) in [(STD, "catalog"), (STD_OFFERS, "offers")] {
        let dir = to.join(into);
        fs::create_dir_all(&dir).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            fs::copy(&path, dir.join(&name)).unwrap();
            if into == "catalog" {
                names.push(name);
            }
        }
    }
    names.sort();
    names
}

/// The number of files in the standard catalog directory, so that a new catalog changes no count.
fn std_file_count() -> usize {
    fs::read_dir(STD).unwrap().count()
}

/// The text of every catalog file of `root/catalog`, by name.
fn catalog_texts(root: &Path, names: &[String]) -> Vec<String> {
    names
        .iter()
        .map(|n| fs::read_to_string(root.join("catalog").join(n)).unwrap())
        .collect()
}

#[test]
fn the_standard_offers_refresh_with_the_saved_responses_and_change_only_checked() {
    let root = dir("std");
    let names = copy_std(&root);
    assert_eq!(names.len(), std_file_count());
    let catalogs = catalog_texts(&root, &names);
    let offers = root.join("offers/lcsc.csv");
    let before = fs::read_to_string(&offers).unwrap();
    let fake = Fake::fixtures();
    let mut opts = options(&fake, None);
    // The ten parts the responses were saved for.
    opts.lcsc = "C5446 C25102 C25104 C2480 C8545 C2286 C1549 C113952 C14709 C20984"
        .split(' ')
        .map(str::to_string)
        .collect();
    let targets = resolve_targets(&root, &[PathBuf::from("catalog")]).unwrap();
    assert_eq!(targets.offers.len(), 2, "the offers beside the catalogs");
    let report = run(&targets, &opts, &quiet).unwrap();
    let looked: usize = report.files.iter().map(|f| f.rows.len()).sum();
    assert_eq!(looked, 10);
    assert!(!report.has_findings(), "{}", render_summary(&report));

    let after = fs::read_to_string(&offers).unwrap();
    let (old_rows, new_rows) = (rows(&before), rows(&after));
    assert_eq!(old_rows.len(), new_rows.len());
    let mut changed = 0;
    for (a, b) in old_rows.iter().zip(&new_rows) {
        assert_eq!(a["sku"], b["sku"], "the order of the offers");
        for (column, was) in a {
            if column != "checked" {
                assert_eq!(&b[column], was, "{}: {column}", a["sku"]);
            }
        }
        if a != b {
            changed += 1;
            assert_eq!((a["checked"].as_str(), b["checked"].as_str()), ("", TODAY));
        }
    }
    assert_eq!(changed, 10);
    // The catalogs, and JLCPCB's offers, are read and never written.
    assert_eq!(catalog_texts(&root, &names), catalogs);
    assert_eq!(
        fs::read_to_string(root.join("offers/jlcpcb.csv")).unwrap(),
        fs::read_to_string(Path::new(STD_OFFERS).join("jlcpcb.csv")).unwrap()
    );
}

#[test]
fn every_standard_catalog_has_a_checked_column_just_after_date() {
    let targets = resolve_targets(Path::new(STD), &[PathBuf::from(".")]).unwrap();
    assert_eq!(targets.catalogs.len(), std_file_count());
    for t in targets.catalogs {
        let text = fs::read_to_string(&t.path).unwrap();
        let header = parse_csv(&text).unwrap()[0].1.clone();
        let date = header.iter().position(|h| h == "date").unwrap();
        assert_eq!(header[date + 1], "checked", "{}", t.shown);
    }
}

#[test]
fn only_get_lcsc_product_code_is_ever_requested_never_a_jlcpcb_host_or_an_api_path() {
    // The fake server stands for every host: it records the method, path and query it is asked.
    // A refresh of every offer of the standard files asks it nothing else.
    let root = dir("only-lcsc");
    copy_std(&root);
    let fake = echo_of(&root);
    let targets = resolve_targets(&root, &[PathBuf::from("catalog")]).unwrap();
    let report = run(&targets, &options(&fake, None), &quiet).unwrap();
    let asked: usize = report.files.iter().map(|f| f.rows.len()).sum();
    let seen = fake.seen();
    assert!(
        asked > 300 && seen.len() == asked,
        "{asked} offers, {} requests",
        seen.len()
    );
    for s in &seen {
        assert_eq!(s.method, "GET", "{s:?}");
        assert_eq!(s.path(), "/lcsc", "{s:?}");
        assert!(s.target.starts_with("/lcsc?productCode=C"), "{s:?}");
        assert!(!s.target.contains("/api") && !s.target.to_lowercase().contains("jlcpcb"));
        assert!(s.body.is_empty(), "{s:?}");
    }
    // The saved responses are LCSC's only: no response of JLCPCB's endpoint is kept to be served.
    for entry in fs::read_dir(common::endpoints::fixtures_dir()).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        assert!(name == "README.md" || name.starts_with("lcsc-"), "{name}");
    }
}

#[test]
fn an_endpoint_at_a_jlcpcb_host_or_under_api_is_refused_before_anything_is_sent() {
    let root = dir("refused-endpoint");
    project(&root, "resistor", &small_catalog());
    let fake = Fake::fixtures();
    for url in [
        "https://jlcpcb.com/parts/list",
        "https://www.JLCPCB.com/parts",
        "https://wmsc.lcsc.com/api/wm/product/detail",
        &format!("http://127.0.0.1:{}/api/product", fake.port),
    ] {
        let mut opts = options(&fake, None);
        opts.network.endpoints.lcsc = url.to_string();
        let e = run(&target(&root), &opts, &quiet).unwrap_err();
        assert!(
            e.contains(url)
                && (e.contains("does not fetch JLCPCB data") || e.contains("is under /api")),
            "{e}"
        );
    }
    assert!(fake.seen().is_empty(), "{:?}", fake.seen());
    // What the command asks by default is allowed.
    assert!(refresh::check_endpoint("https://wmsc.lcsc.com/ftps/wm/product/detail").is_ok());
    assert!(refresh::check_endpoint(&refresh::Endpoints::live().lcsc).is_ok());
}

/// Responses made from the files themselves: each offer of `root/offers/lcsc.csv` is listed with
/// its `mpn` and the package of its row in `root/catalog`, with one more piece in stock.
fn echo_of(root: &Path) -> Fake {
    #[derive(Clone)]
    struct Part {
        mpn: String,
        package: String,
        price: f64,
        stock: u64,
    }
    let mut packages: BTreeMap<String, (String, f64, u64)> = BTreeMap::new();
    for entry in fs::read_dir(root.join("catalog")).unwrap() {
        let text = fs::read_to_string(entry.unwrap().path()).unwrap();
        for row in rows(&text) {
            packages.insert(
                row["mpn"].clone(),
                (
                    // The `header` catalog has no package column.
                    row.get("package").cloned().unwrap_or_default(),
                    // The standard rows ship no price or stock; any numbers will do.
                    row["price"].parse().unwrap_or(0.0123),
                    row["stock"].parse().unwrap_or(1000),
                ),
            );
        }
    }
    let mut parts: BTreeMap<String, Part> = BTreeMap::new();
    for offer in rows(&fs::read_to_string(root.join("offers/lcsc.csv")).unwrap()) {
        let (package, price, stock) = packages
            .get(&offer["mpn"])
            .cloned()
            .unwrap_or((String::new(), 0.0123, 1000));
        parts.insert(
            offer["sku"].clone(),
            Part {
                mpn: offer["mpn"].clone(),
                package,
                price,
                stock,
            },
        );
    }
    Fake::start(move |r: &Seen| {
        let code = r.lcsc();
        let Some(p) = parts.get(&code) else {
            return fixture_reply(r);
        };
        Reply::ok(
            serde_json::json!({"code": 200, "msg": null, "ok": true, "result": {
                "productCode": code, "productModel": p.mpn, "encapStandard": p.package,
                "stockNumber": p.stock + 1,
                "productPriceList": [
                    {"ladder": 1, "usdPrice": p.price},
                    {"ladder": 100, "usdPrice": p.price / 2.0},
                ],
            }})
            .to_string(),
        )
    })
}

#[test]
fn every_standard_offer_survives_a_refresh_with_only_its_checked_cell_changed() {
    let root = dir("roundtrip");
    let names = copy_std(&root);
    let catalogs = catalog_texts(&root, &names);
    let offers = root.join("offers/lcsc.csv");
    let old = fs::read_to_string(&offers).unwrap();
    let fake = echo_of(&root);
    let targets = resolve_targets(&root, &[PathBuf::from("catalog")]).unwrap();
    let report = run(&targets, &options(&fake, None), &quiet).unwrap();
    assert!(!report.has_findings(), "{}", render_summary(&report));
    let total = rows(&old).len();
    assert!(total > 300, "{total}");
    assert_eq!(
        report.files.iter().map(|f| f.rows.len()).sum::<usize>(),
        total
    );
    // Every standard row has an LCSC offer.
    assert_eq!(report.files[0].without_lcsc, 0);
    assert!(report.files[0].written);

    // Byte for byte: the old text with the empty `checked` cell of each offer filled. It is the
    // sixth of the seven columns in the standard file.
    let new = fs::read_to_string(&offers).unwrap();
    let columns = parse_csv(&old).unwrap()[0].1.clone();
    assert_eq!(columns[5], "checked");
    let mut expected: Vec<String> = old.lines().map(str::to_string).collect();
    for (line, fields) in parse_csv(&old).unwrap().into_iter().skip(1) {
        assert_eq!(fields[5], "", "offers/lcsc.csv:{line}");
        let tail = &fields[6];
        let notes = if tail.contains([',', '"']) {
            format!("\"{}\"", tail.replace('"', "\"\""))
        } else {
            tail.clone()
        };
        let at = &mut expected[line - 1];
        let was = format!(",,{notes}");
        assert!(at.ends_with(&was), "offers/lcsc.csv:{line}: {at}");
        let keep = at.len() - was.len();
        *at = format!("{},{TODAY},{notes}", &at[..keep]);
    }
    assert_eq!(new, expected.join("\n") + "\n");
    assert_eq!(catalog_texts(&root, &names), catalogs);

    // A second run the same day finds nothing to write.
    let again = run(&targets, &options(&fake, None), &quiet).unwrap();
    assert!(again.files.iter().all(|f| !f.written));
    assert!(
        render_summary(&again).contains("nothing to change"),
        "{}",
        render_summary(&again)
    );
}

#[test]
fn a_projects_own_offers_are_refreshed_with_no_argument_and_the_standard_ones_are_not_touched() {
    let root = dir("project");
    write(&root, "netsmith.toml", "[project]\nname = \"acme\"\n");
    write(&root, "src/main.nsm", "");
    let (catalog, path) = project(&root, "acme_res", &small_catalog());
    let targets = resolve_targets(&root, &[]).unwrap();
    assert_eq!(targets.catalogs.len(), 1);
    assert_eq!(targets.catalogs[0].shown, "catalogs/acme_res.csv");
    assert_eq!(targets.catalogs[0].catalog, "acme_res");
    let offers: Vec<&str> = targets.offers.iter().map(|t| t.shown.as_str()).collect();
    assert_eq!(offers, ["offers/jlcpcb.csv", "offers/lcsc.csv"]);
    // From a directory below the project root, too.
    let below = resolve_targets(&root.join("src"), &[]).unwrap();
    assert_eq!(below.catalogs[0].path, targets.catalogs[0].path);
    // A catalog file named alone takes the project's offers.
    let named = resolve_targets(&root, &[PathBuf::from("catalogs/acme_res.csv")]).unwrap();
    assert_eq!(named.offers, targets.offers);

    let fake = Fake::fixtures();
    let report = run(&targets, &options(&fake, None), &quiet).unwrap();
    assert!(report.files[0].written);
    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(rows(&after)[0]["checked"], TODAY);
    // The loader accepts the refreshed files (the fixture's parts are standard ones, so with no
    // standard catalogs beside them).
    let mut catalogs = netsmith_solve::Catalogs::empty();
    catalogs
        .add_file(
            "acme_res",
            "catalogs/acme_res.csv",
            &fs::read_to_string(&catalog).unwrap(),
        )
        .unwrap();
    catalogs
        .add_offers_file("lcsc", "offers/lcsc.csv", &after)
        .unwrap();
}

#[test]
fn with_no_argument_outside_a_project_and_the_repository_there_is_nothing_to_act_on() {
    let root = dir("nothing");
    let e = resolve_targets(&root, &[]).unwrap_err();
    assert!(
        e.contains("has no catalog files") && e.contains("data/jlcpcb/catalog/"),
        "{e}"
    );
    let e = resolve_targets(&root, &[PathBuf::from("nope.csv")]).unwrap_err();
    assert!(e.contains("no such file or directory"), "{e}");
    write(&root, "notes.txt", "x");
    let e = resolve_targets(&root, &[PathBuf::from("notes.txt")]).unwrap_err();
    assert!(e.contains("ends in .csv"), "{e}");
    // In the repository, the standard directories are the default.
    let repo = dir("repo");
    let mut names: Vec<String> = fs::read_dir(STD)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    for (from, into) in [(STD, "data/jlcpcb/catalog"), (STD_OFFERS, "data/offers")] {
        fs::create_dir_all(repo.join(into)).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let path = entry.unwrap().path();
            fs::copy(&path, repo.join(into).join(path.file_name().unwrap())).unwrap();
        }
    }
    let targets = resolve_targets(&repo, &[]).unwrap();
    assert_eq!(targets.catalogs.len(), std_file_count());
    // The files in the order of their names.
    assert_eq!(
        targets.catalogs[0].shown,
        format!("data/jlcpcb/catalog/{}", names[0])
    );
    let offers: Vec<&str> = targets.offers.iter().map(|t| t.shown.as_str()).collect();
    assert_eq!(offers, ["data/offers/jlcpcb.csv", "data/offers/lcsc.csv"]);
}

// ---- age -------------------------------------------------------------------------------

#[test]
fn age_lists_the_rows_older_than_the_days_and_the_rows_whose_price_is_unknown() {
    let root = dir("age");
    project(
        &root,
        "r",
        "lcsc,mpn,price,date,name,source\n\
         C1,A,1,2026-11-01,a,s\n\
         C2,B,1,2026-08-04,b,s\n\
         C3,C,1,2026-08-03,c,s\n\
         C4,D,1,,d,s\n\
         ,E,1,2020-01-01,no number,s\n\
         C5,F,1,soon,e,s\n",
    );
    let targets = target(&root);
    let none = Supply::none();
    let report = age(&targets, &[], 90, TODAY, &none).unwrap();
    assert_eq!(report.rows, 5);
    let old: Vec<(&str, Option<i64>)> = report
        .old
        .iter()
        .map(|a| (a.lcsc.as_str(), a.days))
        .collect();
    // 2026-08-04 is exactly 90 days before: not older than 90.
    assert_eq!(old, [("C3", Some(91)), ("C5", None)]);
    // No date and no provider: unknown, which is not old.
    let unknown: Vec<&str> = report.unknown.iter().map(|a| a.lcsc.as_str()).collect();
    assert_eq!(unknown, ["C4"]);
    assert_eq!(
        render_age(&report),
        "catalogs/r.csv:4 C3: read 2026-08-03, 91 days ago\n\
         catalogs/r.csv:7 C5: date `soon` is not like 2026-10-31\n\
         catalogs/r.csv:5 C4: price and stock unknown (no supply provider is configured ([supply] in netsmith.toml))\n\
         2 of 5 rows have a price and stock read more than 90 days before 2026-11-02; 1 of 5 have them unknown; configure a supply provider ([supply] in netsmith.toml, ADR 0047) or read the supplier's page by hand (`netsmith catalog refresh` does not write price or stock, ADR 0039)\n"
    );
    let report = age(&targets, &[], 400, TODAY, &none).unwrap();
    assert_eq!(report.old.len(), 1);
    let other = age(&targets, &["other".to_string()], 90, TODAY, &none).unwrap();
    assert!(other.old.is_empty() && other.unknown.is_empty());
    assert_eq!(
        render_age(&other),
        "0 of 0 rows have a price and stock read more than 90 days before 2026-11-02; 0 of 0 have them unknown\n"
    );
}

#[test]
fn a_provider_answer_ages_a_row_in_place_of_its_date_cell() {
    let root = dir("age-provider");
    project(
        &root,
        "r",
        "lcsc,mpn,price,date,source\nC900001,ZZ-1,1,2020-01-01,s\nC900002,ZZ-2,,,s\nC900003,ZZ-3,,,s\n",
    );
    let quotes = "lcsc,channel,stock,price_breaks,date\n\
                  C900001,jlcpcb-assembly,50,1:0.01,2026-11-01\n\
                  C900002,jlcpcb-assembly,,,2026-06-01\n";
    let supply = Supply::none().with_provider(Box::new(
        netsmith_pipeline::supply::FileProvider::parse("quotes.csv", quotes).unwrap(),
    ));
    let report = age(&target(&root), &[], 90, TODAY, &supply).unwrap();
    // C900001's old date cell is not what counts: the provider read it yesterday.
    let old: Vec<(&str, Option<i64>)> = report
        .old
        .iter()
        .map(|a| (a.lcsc.as_str(), a.days))
        .collect();
    assert_eq!(old, [("C900002", Some(154))]);
    assert_eq!(report.unknown.len(), 1);
    assert_eq!(report.unknown[0].lcsc, "C900003");
    assert_eq!(
        report.unknown[0].unknown.as_deref(),
        Some("file does not list the part")
    );
}

// ---- the command -----------------------------------------------------------------------

#[test]
fn the_standard_catalogs_ship_no_price_so_without_a_provider_every_row_is_unknown_not_old() {
    let root = dir("age-std");
    copy_std(&root);
    let targets = resolve_targets(&root, &[PathBuf::from("catalog")]).unwrap();
    let report = age(&targets, &[], u32::MAX, "2026-11-02", &Supply::none()).unwrap();
    assert!(report.rows > 300);
    assert!(report.old.is_empty(), "{:?}", report.old);
    assert_eq!(report.unknown.len(), report.rows);
    assert!(!render_age(&report).contains("undated"));
}
