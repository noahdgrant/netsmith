//! `netsmith catalog add --lcsc C123 --to catalogs/x.csv` (#454): a row made from LCSC's record,
//! against a fake server that answers from responses saved from the real endpoint. No test
//! reaches the network.

use crate::common;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use common::endpoints::Fake;
use netsmith_fabricate::refresh::{AddError, add, render_added};
use netsmith_solve::parse_csv;
use netsmith_testkit::TempDir;

/// 2026-11-02 12:00 UTC.
const NOW: u64 = 1_793_620_800;

const HEADER: &str = "lcsc,mpn,name,label,forward_voltage,color,package,library,price,stock,date,checked,source,notes\n";

/// An LED catalog of a project with one row of its own.
fn catalog(root: &Path) -> std::path::PathBuf {
    let path = root.join("catalogs/zled.csv");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        format!("{HEADER}C2290,KT-0603W,white,W,2.8V..3.2V,white,0603,Basic,,,,,\"records, read by hand\",\n"),
    )
    .unwrap();
    path
}

fn rows(text: &str) -> Vec<BTreeMap<String, String>> {
    let records = parse_csv(text).unwrap();
    let (_, header) = &records[0];
    records[1..]
        .iter()
        .map(|(_, f)| header.iter().cloned().zip(f.iter().cloned()).collect())
        .collect()
}

#[test]
fn a_row_gets_the_records_number_package_and_source_and_nothing_else() {
    let root = TempDir::new("catalog-add-row");
    let path = catalog(&root);
    let before = fs::read_to_string(&path).unwrap();
    let fake = Fake::fixtures();
    let added = add(
        "C2286",
        Some((&path, "catalogs/zled.csv")),
        fake.network(None, NOW),
    )
    .unwrap();
    let after = fs::read_to_string(&path).unwrap();
    // The rows that were there are byte for byte as they were, and the new one is last.
    assert!(after.starts_with(&before), "{after}");
    let row = rows(&after).pop().unwrap();
    assert_eq!(row["lcsc"], "C2286");
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
        "library",
        "price",
        "stock",
        "date",
        "checked",
    ] {
        assert_eq!(row[empty], "", "{empty}");
    }
    assert_eq!(added.written, Some(("catalogs/zled.csv".to_string(), 3)));
    assert_eq!(added.filled, ["lcsc", "mpn", "package", "source", "notes"]);
    // The columns a person fills: the ratings and the library type, not price, stock or name.
    assert_eq!(added.to_fill, ["forward_voltage", "color", "library"]);
    let said = render_added(&added);
    assert!(
        said.contains("added C2286 to catalogs/zled.csv:3")
            && said.contains("fill from the datasheet (with its source) or JLCPCB's page: forward_voltage, color, library"),
        "{said}"
    );
    // LCSC was asked, and only LCSC.
    assert!(fake.seen().iter().all(|s| s.lcsc() == "C2286"));
}

#[test]
fn a_number_the_file_has_or_lcsc_does_not_is_refused_and_nothing_is_written() {
    let root = TempDir::new("catalog-add-refused");
    let path = catalog(&root);
    let before = fs::read_to_string(&path).unwrap();
    let fake = Fake::fixtures();
    let shown = "catalogs/zled.csv";
    match add("C2290", Some((&path, shown)), fake.network(None, NOW)) {
        Err(AddError::Refused(why)) => assert!(
            why.contains("catalogs/zled.csv:2: the file already has a row for C2290"),
            "{why}"
        ),
        other => panic!("{other:?}"),
    }
    // The file is read before LCSC is asked: nothing went out for a number it has.
    assert!(fake.seen().is_empty());
    match add("C999999999", Some((&path, shown)), fake.network(None, NOW)) {
        Err(AddError::Refused(why)) => {
            assert!(why.contains("LCSC has no record of C999999999"), "{why}")
        }
        other => panic!("{other:?}"),
    }
    match add(
        "not-a-number",
        Some((&path, shown)),
        fake.network(None, NOW),
    ) {
        Err(AddError::Stopped(why)) => assert!(why.contains("is not an LCSC part number"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), before);
}

#[test]
fn without_a_file_the_record_is_printed_and_nothing_is_written() {
    let fake = Fake::fixtures();
    let added = add("C2286", None, fake.network(None, NOW)).unwrap();
    assert_eq!(added.written, None);
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
    match add("C2286", None, network) {
        Err(AddError::Stopped(why)) => assert!(why.contains("--offline"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert!(fake.seen().is_empty());
}
