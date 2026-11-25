//! `netsmith catalog add --lcsc C123 --to catalogs/x.csv` (#454): a row made from LCSC's record,
//! and its number an offer in `offers/lcsc.csv` (ADR 0143), against a fake server that answers from responses saved from the real endpoint. No test
//! reaches the network.

use crate::common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use common::endpoints::Fake;
use netsmith_fabricate::refresh::{AddError, add, render_added};
use netsmith_solve::parse_csv;
use netsmith_testkit::TempDir;

/// 2026-11-02 12:00 UTC.
const NOW: u64 = 1_793_620_800;

const HEADER: &str = "mpn,name,label,forward_voltage,color,package,price,stock,date,checked,source,notes\n";

/// An LED catalog of a project with one row of its own, and LCSC's offer of it.
fn catalog(root: &Path) -> (PathBuf, PathBuf) {
    let path = root.join("catalogs/zled.csv");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        format!("{HEADER}KT-0603W,white,W,2.8V..3.2V,white,0603,,,,,\"records, read by hand\",\n"),
    )
    .unwrap();
    let offers = root.join("offers/lcsc.csv");
    fs::create_dir_all(offers.parent().unwrap()).unwrap();
    fs::write(
        &offers,
        format!(
            "{}{}",
            netsmith_solve::migrate::OFFERS_HEADER,
            ",KT-0603W,C2290,,\"records, read by hand\",,\n"
        ),
    )
    .unwrap();
    (path, offers)
}

fn rows(text: &str) -> Vec<BTreeMap<String, String>> {
    let records = parse_csv(text).unwrap();
    let (_, header) = &records[0];
    records[1..]
        .iter()
        .map(|(_, f)| header.iter().cloned().zip(f.iter().cloned()).collect())
        .collect()
}

const OFFERS: &str = "offers/lcsc.csv";

#[test]
fn a_row_gets_the_records_number_package_and_source_and_its_offer_the_number() {
    let root = TempDir::new("catalog-add-row");
    let (path, offers) = catalog(&root);
    let (before, offers_before) = (
        fs::read_to_string(&path).unwrap(),
        fs::read_to_string(&offers).unwrap(),
    );
    let fake = Fake::fixtures();
    let added = add(
        "C2286",
        Some(((&path, "catalogs/zled.csv"), (&offers, OFFERS))),
        fake.network(None, NOW),
        &mut |_, _| Ok(Vec::new()),
    )
    .unwrap();
    let after = fs::read_to_string(&path).unwrap();
    // The rows that were there are byte for byte as they were, and the new one is last.
    assert!(after.starts_with(&before), "{after}");
    let row = rows(&after).pop().unwrap();
    assert!(!row.contains_key("lcsc"), "a row holds no supplier's number (ADR 0143)");
    assert_eq!(row["mpn"], "KT-0603R");
    assert_eq!(row["package"], "0603");
    assert!(
        row["source"].starts_with("LCSC product record for C2286 (the data behind https://www.lcsc.com/product-detail/C2286.html")
            && row["source"].contains("read 2026-11-02 (KT-0603R, 0603)"),
        "{}",
        row["source"]
    );
    for empty in [
        "name",
        "label",
        "forward_voltage",
        "color",
        "price",
        "stock",
        "date",
        "checked",
    ] {
        assert_eq!(row[empty], "", "{empty}");
    }
    // The number is an offer of LCSC, with the same source and the day it was read.
    let offers_after = fs::read_to_string(&offers).unwrap();
    assert!(offers_after.starts_with(&offers_before), "{offers_after}");
    let offer = rows(&offers_after).pop().unwrap();
    assert_eq!(
        (offer["mpn"].as_str(), offer["sku"].as_str(), offer["class"].as_str()),
        ("KT-0603R", "C2286", "")
    );
    assert_eq!(offer["source"], row["source"]);
    assert_eq!(offer["checked"], "2026-11-02");
    assert_eq!(added.written, Some(("catalogs/zled.csv".to_string(), 3)));
    assert_eq!(added.offer, Some((OFFERS.to_string(), 3)));
    assert_eq!(added.filled, ["mpn", "package", "source", "notes"]);
    // The columns a person fills: the ratings and the offer's class, not price, stock or name.
    assert_eq!(
        added.to_fill,
        [
            "forward_voltage",
            "color",
            "the class of the offer (offers/lcsc.csv, the library from JLCPCB's page) for a part JLCPCB assembles"
        ]
    );
    let said = render_added(&added);
    assert!(
        said.contains("added KT-0603R to catalogs/zled.csv:3")
            && said.contains("added the offer of C2286 to offers/lcsc.csv:3")
            && said.contains("fill from the datasheet (with its source) or JLCPCB's page: forward_voltage, color, the class of the offer"),
        "{said}"
    );
    // The offers read as a build reads them (the fixture's parts are standard ones, so with no
    // standard offers beside them).
    let mut catalogs = netsmith_solve::Catalogs::empty();
    catalogs
        .add_offers_file("lcsc", OFFERS, &offers_after)
        .unwrap();
    // LCSC was asked, and only LCSC.
    assert!(fake.seen().iter().all(|s| s.lcsc() == "C2286"));
}

#[test]
fn an_offers_file_that_is_not_there_is_made_with_its_header() {
    let root = TempDir::new("catalog-add-new-offers");
    let path = root.join("catalogs/zled.csv");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, HEADER).unwrap();
    let offers = root.join("offers/lcsc.csv");
    let fake = Fake::fixtures();
    add(
        "C2286",
        Some(((&path, "catalogs/zled.csv"), (&offers, OFFERS))),
        fake.network(None, NOW),
        &mut |_, _| Ok(Vec::new()),
    )
    .unwrap();
    let text = fs::read_to_string(&offers).unwrap();
    assert!(
        text.starts_with(netsmith_solve::migrate::OFFERS_HEADER),
        "{text}"
    );
    assert_eq!(rows(&text).len(), 1);
}

#[test]
fn a_file_with_a_manufacturer_column_gets_the_records_manufacturer_with_its_source() {
    let root = TempDir::new("catalog-add-manufacturer");
    let path = root.join("catalogs/zled.csv");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        "manufacturer,mpn,forward_voltage,color,package,source,notes\n",
    )
    .unwrap();
    let offers = root.join("offers/lcsc.csv");
    let fake = Fake::fixtures();
    let added = add(
        "C2286",
        Some(((&path, "catalogs/zled.csv"), (&offers, OFFERS))),
        fake.network(None, NOW),
        &mut |_, _| Ok(Vec::new()),
    )
    .unwrap();
    let row = rows(&fs::read_to_string(&path).unwrap()).pop().unwrap();
    // The record already fetched names the maker, as LCSC spells it (ADR 0143).
    assert_eq!(row["manufacturer"], "Hubei KENTO Elec");
    assert_eq!(row["mpn"], "KT-0603R");
    assert!(
        row["source"].ends_with(
            "; manufacturer: `Hubei KENTO Elec`, the record's `brandNameEn`, as LCSC spells it"
        ),
        "{}",
        row["source"]
    );
    assert!(
        row["notes"].ends_with("and the manufacturer's name against the datasheet's spelling"),
        "{}",
        row["notes"]
    );
    // The offer is of the row's manufacturer too.
    let offer = rows(&fs::read_to_string(&offers).unwrap()).pop().unwrap();
    assert_eq!(offer["manufacturer"], "Hubei KENTO Elec");
    assert_eq!(
        added.filled,
        ["manufacturer", "mpn", "package", "source", "notes"]
    );
    assert!(
        render_added(&added).contains("mpn KT-0603R, package 0603, manufacturer Hubei KENTO Elec"),
        "{}",
        render_added(&added)
    );
    // LCSC was asked once, for the number: nothing else was fetched for the maker.
    assert!(fake.seen().iter().all(|s| s.lcsc() == "C2286"));
}

#[test]
fn a_number_the_offers_have_a_part_the_file_has_or_one_lcsc_does_not_is_refused_and_nothing_is_written()
 {
    let root = TempDir::new("catalog-add-refused");
    let (path, offers) = catalog(&root);
    let (before, offers_before) = (
        fs::read_to_string(&path).unwrap(),
        fs::read_to_string(&offers).unwrap(),
    );
    let fake = Fake::fixtures();
    let files = Some(((path.as_path(), "catalogs/zled.csv"), (offers.as_path(), OFFERS)));
    match add("C2290", files, fake.network(None, NOW), &mut |_, _| Ok(Vec::new())) {
        Err(AddError::Refused(why)) => assert!(
            why.contains("offers/lcsc.csv:2: the file already has an offer of C2290"),
            "{why}"
        ),
        other => panic!("{other:?}"),
    }
    // The offers are read before LCSC is asked: nothing went out for a number they have.
    assert!(fake.seen().is_empty());
    match add("C999999999", files, fake.network(None, NOW), &mut |_, _| Ok(Vec::new())) {
        Err(AddError::Refused(why)) => {
            assert!(why.contains("LCSC has no record of C999999999"), "{why}")
        }
        other => panic!("{other:?}"),
    }
    match add("not-a-number", files, fake.network(None, NOW), &mut |_, _| Ok(Vec::new())) {
        Err(AddError::Stopped(why)) => assert!(why.contains("is not an LCSC part number"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), before);
    assert_eq!(fs::read_to_string(&offers).unwrap(), offers_before);
}

#[test]
fn a_catalog_with_the_old_columns_is_refused_with_the_migration() {
    let root = TempDir::new("catalog-add-old");
    let path = root.join("catalogs/zled.csv");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "lcsc,mpn,package,source\n").unwrap();
    let offers = root.join("offers/lcsc.csv");
    let fake = Fake::fixtures();
    match add(
        "C2286",
        Some(((&path, "catalogs/zled.csv"), (&offers, OFFERS))),
        fake.network(None, NOW),
        &mut |_, _| Ok(Vec::new()),
    ) {
        Err(AddError::Stopped(why)) => {
            assert!(why.contains("netsmith project migrate-offers"), "{why}")
        }
        other => panic!("{other:?}"),
    }
    assert!(fake.seen().is_empty());
}

#[test]
fn without_a_file_the_record_is_printed_and_nothing_is_written() {
    let fake = Fake::fixtures();
    let added = add("C2286", None, fake.network(None, NOW), &mut |_, _| Ok(Vec::new())).unwrap();
    assert_eq!(added.written, None);
    assert_eq!(added.offer, None);
    let said = render_added(&added);
    assert!(
        said.contains("LCSC's record of C2286 (read 2026-11-02): mpn KT-0603R, package 0603")
            && said.contains("nothing written: name a catalog file with --to"),
        "{said}"
    );
}

#[test]
fn offline_with_nothing_kept_says_so() {
    let fake = Fake::fixtures();
    let mut network = fake.network(None, NOW);
    network.offline = true;
    match add("C2286", None, network, &mut |_, _| Ok(Vec::new())) {
        Err(AddError::Stopped(why)) => assert!(why.contains("--offline"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert!(fake.seen().is_empty());
}
