//! Writes the standard offers files, `data/offers/jlcpcb.csv` and `data/offers/lcsc.csv`
//! (ADR 0143, stage 2), from the `lcsc`, `library`, `source` and `checked` cells of the standard
//! catalogs, `data/jlcpcb/catalog/*.csv`. It asks no supplier anything: every number is the
//! row's, and so is its evidence.
//!
//! ```sh
//! cargo run -p netsmith-solve --example offers_from_catalogs
//! ```
//!
//! Run it after a row of a standard catalog is added or its `lcsc`, `library` or `checked`
//! changes: until stage 3 (issue #1458) the rows hold the numbers, and the test
//! `offers::the_standard_offers_are_the_standard_rows_columns` fails while the files differ.
//!
//! For each row with an `lcsc`, in the order of the files (by name) and of their rows:
//!
//! - `jlcpcb.csv`: `sku` is the row's `lcsc`, the number JLCPCB assembles the part by, and
//!   `class` its `library`;
//! - `lcsc.csv`: `sku` is the row's `lcsc` and `class` is empty.
//!
//! In both, `manufacturer` and `mpn` are the row's, `checked` is the row's, `source` is the first
//! part of the row's `source` (up to the first ` ; `, which in every standard row is the record
//! of JLCPCB and LCSC the numbers were read from, with its day), and `notes` names the file the
//! row is in, whose `source` and `notes` hold the rest of its evidence.

use std::fs;
use std::path::{Path, PathBuf};

use netsmith_solve::parse_csv;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let catalogs = root.join("data/jlcpcb/catalog");
    let mut files: Vec<PathBuf> = fs::read_dir(&catalogs)
        .unwrap_or_else(|e| panic!("{}: {e}", catalogs.display()))
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "csv"))
        .collect();
    files.sort();
    let head = "manufacturer,mpn,sku,class,source,checked,notes\n";
    let (mut jlcpcb, mut lcsc) = (head.to_string(), head.to_string());
    let mut count = 0;
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let shown = format!("data/jlcpcb/catalog/{name}");
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{shown}: {e}"));
        let records = parse_csv(&text).unwrap_or_else(|e| panic!("{shown}: {e}"));
        let Some(((_, header), rows)) = records.split_first() else {
            continue;
        };
        let column = |name: &str| header.iter().position(|c| c.trim() == name);
        for (line, row) in rows {
            let cell = |name: &str| {
                column(name)
                    .map(|i| row[i].trim().to_string())
                    .unwrap_or_default()
            };
            let number = cell("lcsc");
            if number.is_empty() {
                continue;
            }
            let mpn = cell("mpn");
            assert!(
                !mpn.is_empty(),
                "{shown}:{line}: the row of {number} has no mpn, and an offer names its part by it"
            );
            let source = cell("source");
            let evidence = source.split(" ; ").next().unwrap_or_default().trim();
            assert!(
                !evidence.is_empty(),
                "{shown}:{line}: the row of {number} has no source"
            );
            let (manufacturer, checked) = (cell("manufacturer"), cell("checked"));
            for (out, class, cells) in [
                (&mut jlcpcb, cell("library"), "`lcsc` and `library` cells"),
                (&mut lcsc, String::new(), "`lcsc` cell"),
            ] {
                let notes = format!(
                    "From the {cells} of the row of this part in {shown}, whose source and notes hold the rest of its evidence."
                );
                let fields = [
                    manufacturer.as_str(),
                    &mpn,
                    &number,
                    &class,
                    evidence,
                    &checked,
                    &notes,
                ];
                let quoted: Vec<String> = fields.iter().map(|f| quote(f)).collect();
                out.push_str(&quoted.join(","));
                out.push('\n');
            }
            count += 1;
        }
    }
    let offers = root.join("data/offers");
    fs::create_dir_all(&offers).expect("data/offers can be made");
    fs::write(offers.join("jlcpcb.csv"), jlcpcb).expect("data/offers/jlcpcb.csv is writable");
    fs::write(offers.join("lcsc.csv"), lcsc).expect("data/offers/lcsc.csv is writable");
    println!("wrote {count} offers to data/offers/jlcpcb.csv and data/offers/lcsc.csv");
}

/// A CSV field, quoted when it holds a comma, a quote, a line break or space at an end.
fn quote(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) || field.trim() != field {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}
