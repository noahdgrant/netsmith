//! Offers files (ADR 0143): the standard ones are the numbers of the standard rows, and a
//! project's give its rows their supplier number and class through the one function the solver
//! reads them by; a row holds no supplier's number. Every project part here is invented; `example.invalid` is not a real
//! address.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use netsmith_check::{FieldKind, Loader, check};
use netsmith_solve::{Catalogs, Channel, Library, Solution, parse_csv, solve};

fn data(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .join(path)
}

/// The rows of a catalog file, each with its line, as header name to cell.
fn rows(file: &str, text: &str) -> Vec<(usize, BTreeMap<String, String>)> {
    let records = parse_csv(text).unwrap_or_else(|e| panic!("{file}: {e}"));
    let (_, header) = &records[0];
    records[1..]
        .iter()
        .map(|(line, r)| {
            let cells = header
                .iter()
                .map(|h| h.trim().to_string())
                .zip(r.iter().map(|c| c.trim().to_string()))
                .collect();
            (*line, cells)
        })
        .collect()
}

/// The standard catalog files, as (shown name, text).
fn standard_files() -> Vec<(String, String)> {
    let dir = data("jlcpcb/catalog");
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "csv"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (
                format!("data/jlcpcb/catalog/{name}"),
                fs::read_to_string(&p).unwrap(),
            )
        })
        .collect()
}

/// The standard offers and the standard rows are one set of parts (ADR 0143): every offer is of
/// a row, and every row has one `lcsc` offer, with the number the row held in its `lcsc` column
/// before it went, and the `class` its `library` held. The catalogs say of that number what its
/// row says.
#[test]
fn the_standard_offers_and_the_standard_rows_are_the_same_parts() {
    let catalogs = Catalogs::jlcpcb();
    let offers = catalogs.offers();
    let mut found = Vec::new();
    let mut parts = 0;
    for (file, text) in standard_files() {
        for (line, row) in rows(&file, &text) {
            let at = format!("{file}:{line}");
            assert!(
                !row.contains_key("lcsc") && !row.contains_key("library"),
                "{at}: a standard catalog has no lcsc or library column (ADR 0143)"
            );
            let mpn = &row["mpn"];
            let manufacturer = row.get("manufacturer").filter(|m| !m.is_empty());
            parts += 1;
            match offers.of("lcsc", manufacturer.map(String::as_str), mpn) {
                Ok(Some(offer)) => {
                    let supplier = catalogs.supplier(&offer.sku).expect("a standard row");
                    assert_eq!(&supplier.mpn, mpn, "{at}");
                    assert_eq!(supplier.library, offer.class, "{at}");
                }
                Ok(None) => found.push(format!("{at}: `{mpn}` has no lcsc offer")),
                Err(e) => found.push(format!("{at}: {e}")),
            }
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
    // No standard offer is of a part that no standard row has, and every standard row is one
    // that JLCPCB's assembly library lists.
    assert_eq!(offers.iter().filter(|o| o.supplier == "lcsc").count(), parts);
    assert_eq!(offers.len(), parts);
    assert!(offers.iter().all(|o| o.class.is_some()));
    assert!(offers.iter().all(|o| o.built_in));
    assert!(catalogs.offer_notes().is_empty());
}

#[test]
fn a_catalog_with_an_lcsc_or_a_library_column_is_refused_with_how_to_move_them() {
    let e = Catalogs::jlcpcb()
        .add_file(
            "zz9",
            "catalogs/zz9.csv",
            "lcsc,mpn,package,library,source\nC1,ZZ9-33,SOT-23-3,Basic,x\n",
        )
        .unwrap_err();
    assert_eq!(
        e,
        "catalogs/zz9.csv:1: the columns `lcsc` and `library` have gone from catalog files (ADR 0143): a catalog row is the part, its `manufacturer` and `mpn`, and what a supplier calls it is an offer, a row of `offers/<supplier>.csv` (the `lcsc` number is the `sku` of the offer in `offers/lcsc.csv`, and the `library` its `class`); `netsmith project migrate-offers` moves them into the project's offers files and removes them here (docs/MIGRATING.md)"
    );
    let e = Catalogs::jlcpcb()
        .add_file("zz9", "catalogs/zz9.csv", "mpn,package,library,source\n")
        .unwrap_err();
    assert!(
        e.starts_with("catalogs/zz9.csv:1: the column `library` has gone from catalog files"),
        "{e}"
    );
}

/// A regulator of a project's catalog, keyed by its part number.
const BOARD: &str = r#"
import std::sources::Rail

part Zz9 {
    .vin:  Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    .vin.current == -.vout.current

    pins "SOT-23-3" {
        .vin.pos       power       "IN"     "1"
        .vin.neg       power       "GND"    "2"
        .vout.pos      power       "OUT"    "3"
    }

    .package: Package
    catalog zz9 by .mpn { .mpn, .package }
}

board B {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .u: Zz9 { .mpn &= "ZZ9-33" }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .input ~ .u.vin
    .u.vout ~ .output
}
"#;

const ROWS: &str = "\
mpn,package,source
ZZ9-33,SOT-23-3,https://example.invalid/zz9.pdf
ZZ9-50,SOT-23-3,https://example.invalid/zz9.pdf
";

const HEAD: &str = "manufacturer,mpn,sku,class,source,checked,notes\n";

fn project(offers: &[(&str, &str)]) -> Result<Catalogs, String> {
    let mut c = Catalogs::jlcpcb();
    c.add_file("zz9", "catalogs/zz9.csv", ROWS)?;
    for (supplier, rows) in offers {
        c.add_offers_file(
            supplier,
            &format!("offers/{supplier}.csv"),
            &format!("{HEAD}{rows}"),
        )?;
    }
    Ok(c)
}

fn solved(catalogs: &Catalogs) -> Solution {
    let c = check("test.nsm", BOARD, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    let s = solve(&c.boards[0], catalogs);
    let messages: Vec<&str> = s.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert!(messages.is_empty(), "{messages:#?}");
    s
}

#[test]
fn a_project_offer_gives_its_row_the_number_and_the_class() {
    let without = solved(&project(&[]).unwrap());
    assert_eq!(without.picks[0].lcsc, None);
    assert_eq!(without.picks[0].library, None);

    let with = project(&[("lcsc", ",ZZ9-33,C900001,Basic,https://example.invalid/jlc,,\n")])
        .unwrap();
    let s = solved(&with);
    assert_eq!(s.picks[0].mpn.as_deref(), Some("ZZ9-33"));
    assert_eq!(s.picks[0].lcsc.as_deref(), Some("C900001"));
    assert_eq!(s.picks[0].library, Some(Library::Basic));
    // What the catalogs say of the number is the offer's too.
    let supplier = with.supplier("C900001").expect("the offer's number");
    assert_eq!(supplier.library, Some(Library::Basic));
    assert!(with.offer_notes().is_empty());
}

#[test]
fn the_pick_records_the_skus_of_the_projects_channels_and_no_others() {
    let offers = [
        ("lcsc", ",ZZ9-33,C900001,Basic,https://example.invalid/lcsc,,\n"),
        ("digikey", ",ZZ9-33,ZZ9-33-ND,,https://example.invalid/digikey,,\n"),
    ];
    let skus = |c: Catalogs| solved(&c).picks[0].skus.clone();
    let word = |channel: &str, sku: &str| (channel.to_string(), sku.to_string());
    // With no list, the project's channels are the default two: a Digi-Key offer, which the
    // project does not buy from, moves nothing in its lock.
    assert_eq!(
        skus(project(&offers).unwrap()),
        [word("jlcpcb-assembly", "C900001"), word("lcsc-retail", "C900001")]
    );
    // Listing the channel adds its sku, and drops the one of a channel that is no longer listed.
    let listed = project(&offers)
        .unwrap()
        .with_channels(vec![Channel::DIGIKEY_RETAIL, Channel::JLCPCB_ASSEMBLY]);
    assert_eq!(
        skus(listed),
        [word("jlcpcb-assembly", "C900001"), word("digikey-retail", "ZZ9-33-ND")]
    );
}

#[test]
fn an_lcsc_offer_gives_the_number_and_no_class() {
    let c = project(&[("lcsc", ",ZZ9-50,C900002,,https://example.invalid/lcsc,,\n")]).unwrap();
    let columns = [
        ("mpn".to_string(), FieldKind::Text),
        ("package".to_string(), FieldKind::Text),
    ];
    let catalog = c.identity("zz9", &columns, &[]).unwrap().unwrap();
    let row = catalog
        .candidates
        .iter()
        .find(|r| r.mpn.as_deref() == Some("ZZ9-50"))
        .unwrap();
    assert_eq!(row.lcsc.as_deref(), Some("C900002"));
    assert_eq!(row.library, None);
}

#[test]
fn an_offer_of_a_part_in_no_catalog_is_a_note() {
    let c = project(&[("lcsc", ",ZZ9-12,C900003,,https://example.invalid/lcsc,,\n")]).unwrap();
    assert_eq!(
        c.offer_notes(),
        ["offers/lcsc.csv:2: the lcsc offer of `ZZ9-12` (sku `C900003`) is of a part that no catalog has a row of, so nothing reads it: add the part's row to a catalog file, or remove the offer"]
    );
}

#[test]
fn an_offer_with_no_source_is_refused() {
    let e = project(&[("lcsc", ",ZZ9-33,C900001,Basic,,,\n")]).unwrap_err();
    assert_eq!(
        e,
        "offers/lcsc.csv:2: the offer of `ZZ9-33` (sku `C900001`) has no source; every offer needs the URL or text its number was read from in its `source` column, as a catalog row does: no number comes from memory"
    );
}

#[test]
fn a_project_cannot_offer_a_standard_part_again() {
    let e = project(&[(
        "lcsc",
        ",0402WGF3300TCE,C900004,Extended,https://example.invalid/jlc,,\n",
    )])
    .unwrap_err();
    assert!(
        e.starts_with("offers/lcsc.csv:2: `0402WGF3300TCE` already has an offer of lcsc at data/offers/lcsc.csv:"),
        "{e}"
    );
    assert!(e.ends_with("; a supplier offers a part once"), "{e}");
    let e = project(&[("lcsc", ",ZZ9-33,C25104,,https://example.invalid/lcsc,,\n")]).unwrap_err();
    assert!(
        e.starts_with("offers/lcsc.csv:2: the sku `C25104` is already the offer at data/offers/lcsc.csv:"),
        "{e}"
    );
}

#[test]
fn a_row_with_no_manufacturer_between_two_offers_of_its_mpn_is_refused() {
    let c = project(&[(
        "lcsc",
        "Acme,ZZ9-33,C900005,Basic,https://example.invalid/a,,\nWidget Co,ZZ9-33,C900006,Basic,https://example.invalid/w,,\n",
    )])
    .unwrap();
    let columns = [
        ("mpn".to_string(), FieldKind::Text),
        ("package".to_string(), FieldKind::Text),
    ];
    let e = c.identity("zz9", &columns, &[]).unwrap().unwrap_err();
    assert_eq!(
        e,
        "catalogs/zz9.csv:2: the mpn `ZZ9-33` names no manufacturer, and lcsc offers two parts of it (`ZZ9-33` of `Acme` at offers/lcsc.csv:2 and `ZZ9-33` of `Widget Co` at offers/lcsc.csv:3): give the row its manufacturer, as the offer it is bought by has it"
    );
}
