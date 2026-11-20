//! Offers files (ADR 0143, stage 2): the standard ones hold the numbers of the standard rows,
//! and a project's give its rows their supplier number and class through the one function the
//! solver reads them by. Every project part here is invented; `example.invalid` is not a real
//! address.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use netsmith_check::{FieldKind, Loader, check};
use netsmith_solve::offers::Offers;
use netsmith_solve::{Catalogs, Library, Solution, parse_csv, solve};

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

/// What differs between the `lcsc` and `library` columns of the rows of the catalog file `file`
/// and the offers of their parts, one line each: a row with an `lcsc` must have a `jlcpcb` offer
/// of that `sku` and its `library` as the `class`, and an `lcsc` offer of that `sku`.
fn disagreements(file: &str, text: &str, offers: &Offers) -> Vec<String> {
    let mut out = Vec::new();
    for (line, row) in rows(file, text) {
        let cell = |name: &str| row.get(name).map(String::as_str).unwrap_or_default();
        let (lcsc, mpn) = (cell("lcsc"), cell("mpn"));
        let manufacturer = Some(cell("manufacturer")).filter(|m| !m.is_empty());
        if lcsc.is_empty() {
            continue;
        }
        let at = format!("{file}:{line}");
        for supplier in ["jlcpcb", "lcsc"] {
            let offer = match offers.of(supplier, manufacturer, mpn) {
                Ok(Some(offer)) => offer,
                Ok(None) => {
                    out.push(format!("{at}: `{mpn}` has no {supplier} offer"));
                    continue;
                }
                Err(e) => {
                    out.push(format!("{at}: {e}"));
                    continue;
                }
            };
            if offer.sku != lcsc {
                out.push(format!(
                    "{at}: lcsc `{lcsc}`, and the {supplier} offer at {} has the sku `{}`",
                    offer.at, offer.sku
                ));
            }
            let class = offer.class.map(|c| c.to_string()).unwrap_or_default();
            let library = if supplier == "jlcpcb" {
                cell("library")
            } else {
                ""
            };
            if class != library {
                out.push(format!(
                    "{at}: library `{library}`, and the {supplier} offer at {} has the class `{class}`",
                    offer.at
                ));
            }
        }
    }
    out
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

const REGENERATE: &str = "until stage 3 of ADR 0143 the rows hold the numbers: run `cargo run -p netsmith-solve --example offers_from_catalogs` after changing a row's lcsc, library or checked, and commit data/offers/";

#[test]
fn the_standard_offers_are_the_standard_rows_columns() {
    let catalogs = Catalogs::jlcpcb();
    let offers = catalogs.offers();
    let mut found = Vec::new();
    let mut numbered = 0;
    for (file, text) in standard_files() {
        found.extend(disagreements(&file, &text, offers));
        for (_, row) in rows(&file, &text) {
            let lcsc = &row["lcsc"];
            if lcsc.is_empty() {
                continue;
            }
            numbered += 1;
            // And the one function reads them: what the catalogs say of the number is the row's.
            let supplier = catalogs.supplier(lcsc).expect("a standard row");
            assert_eq!(supplier.mpn, row["mpn"], "{file}: {lcsc}");
            assert_eq!(
                supplier.library.map(|l| l.to_string()).unwrap_or_default(),
                row["library"],
                "{file}: {lcsc}"
            );
        }
    }
    assert!(found.is_empty(), "{}\n{REGENERATE}", found.join("\n"));
    // No standard offer is of a part that no standard row has.
    for supplier in ["jlcpcb", "lcsc"] {
        let count = offers.iter().filter(|o| o.supplier == supplier).count();
        assert_eq!(count, numbered, "{supplier}: {REGENERATE}");
    }
    assert!(offers.iter().all(|o| o.built_in));
    assert!(catalogs.offer_notes().is_empty());
}

#[test]
fn the_consistency_check_names_a_row_that_disagrees_with_its_offer() {
    let offers = Catalogs::jlcpcb().offers().clone();
    let file = "data/jlcpcb/catalog/resistor.csv";
    let text = fs::read_to_string(data("jlcpcb/catalog/resistor.csv")).unwrap();
    assert!(disagreements(file, &text, &offers).is_empty());
    // The 330 ohm row is Basic at C25104: say Extended, then another number.
    let line = text
        .lines()
        .position(|l| l.starts_with("C25104,"))
        .expect("the 330 ohm row")
        + 1;
    let changed = text.replacen(",Basic,", ",Extended,", 1);
    assert!(changed.lines().nth(line - 1).unwrap().contains(",Extended,"));
    assert_eq!(
        disagreements(file, &changed, &offers),
        [format!(
            "{file}:{line}: library `Extended`, and the jlcpcb offer at data/offers/jlcpcb.csv:{} has the class `Basic`",
            offers.of("jlcpcb", None, "0402WGF3300TCE").unwrap().unwrap().at.rsplit(':').next().unwrap()
        )]
    );
    let renumbered = text.replacen("C25104,", "C99999,", 1);
    let found = disagreements(file, &renumbered, &offers);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert!(found[0].contains("lcsc `C99999`, and the jlcpcb offer"), "{}", found[0]);
    assert!(found[1].contains("lcsc `C99999`, and the lcsc offer"), "{}", found[1]);
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
lcsc,mpn,package,library,source
,ZZ9-33,SOT-23-3,,https://example.invalid/zz9.pdf
,ZZ9-50,SOT-23-3,Extended,https://example.invalid/zz9.pdf
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

    let with = project(&[("jlcpcb", ",ZZ9-33,C900001,Basic,https://example.invalid/jlc,,\n")])
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
fn an_lcsc_offer_gives_the_number_and_the_row_keeps_its_class() {
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
    assert_eq!(row.library, Some(Library::Extended));
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
    let e = project(&[("jlcpcb", ",ZZ9-33,C900001,Basic,,,\n")]).unwrap_err();
    assert_eq!(
        e,
        "offers/jlcpcb.csv:2: the offer of `ZZ9-33` (sku `C900001`) has no source; every offer needs the URL or text its number was read from in its `source` column, as a catalog row does: no number comes from memory"
    );
}

#[test]
fn a_project_cannot_offer_a_standard_part_again() {
    let e = project(&[(
        "jlcpcb",
        ",0402WGF3300TCE,C900004,Extended,https://example.invalid/jlc,,\n",
    )])
    .unwrap_err();
    assert!(
        e.starts_with("offers/jlcpcb.csv:2: `0402WGF3300TCE` already has an offer of jlcpcb at data/offers/jlcpcb.csv:"),
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
        "jlcpcb",
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
        "catalogs/zz9.csv:2: the mpn `ZZ9-33` names no manufacturer, and jlcpcb offers two parts of it (`ZZ9-33` of `Acme` at offers/jlcpcb.csv:2 and `ZZ9-33` of `Widget Co` at offers/jlcpcb.csv:3): give the row its manufacturer, as the offer it is bought by has it"
    );
}
