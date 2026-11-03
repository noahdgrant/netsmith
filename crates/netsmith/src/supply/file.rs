//! The file provider (ADR 0047): prices and stock from a CSV file on this machine.
//!
//! ```csv
//! lcsc,mpn,channel,stock,price_breaks,currency,date
//! C900001,TESTRES-10K,jlcpcb-assembly,5000,1:0.0100 100:0.0050,USD,2026-11-01
//! ```
//!
//! A row is a part and a channel (`jlcpcb-assembly` or `lcsc-retail`). `lcsc` or `mpn` (one
//! is enough) names the part; `stock` empty is unknown; `price_breaks` is `from:price`
//! pairs, empty if the file has no price; `currency` defaults to `USD`; `date` is the day
//! the numbers were read and is required, because an answer with no date cannot be aged.
//! A person writes the file from a supplier's export or by hand, a company keeps its own
//! price list in one, and the tests use it in place of a network.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use netsmith_solve::parse_csv;

use super::cache::price_breaks;
use super::{Channel, PartKey, Quote, SupplyError, SupplyProvider};

#[derive(Debug)]
struct Row {
    lcsc: String,
    mpn: String,
    quote: Quote,
}

/// The rows of one file, read when the provider is made.
#[derive(Debug)]
pub struct FileProvider {
    rows: Vec<Row>,
}

impl FileProvider {
    /// Read a file. `shown` is what a message calls it. A mistake names the line.
    pub fn open(path: &Path, shown: &str) -> Result<FileProvider, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("{shown}: {e}"))?;
        FileProvider::parse(shown, &text)
    }

    pub fn parse(shown: &str, text: &str) -> Result<FileProvider, String> {
        let records = parse_csv(text).map_err(|e| format!("{shown}: {e}"))?;
        let Some(((hline, names), body)) = records.split_first() else {
            return Err(format!(
                "{shown}: the file is empty; the first line names the columns"
            ));
        };
        let names: Vec<&str> = names.iter().map(|n| n.trim()).collect();
        let column = |name: &str| names.iter().position(|n| *n == name);
        for needed in ["channel", "date"] {
            if column(needed).is_none() {
                return Err(format!("{shown}:{hline}: there is no `{needed}` column"));
            }
        }
        if column("lcsc").is_none() && column("mpn").is_none() {
            return Err(format!(
                "{shown}:{hline}: there is neither an `lcsc` nor an `mpn` column to name the parts"
            ));
        }
        for n in &names {
            if ![
                "lcsc",
                "mpn",
                "channel",
                "stock",
                "price_breaks",
                "currency",
                "date",
            ]
            .contains(n)
            {
                return Err(format!(
                    "{shown}:{hline}: `{n}` is not a column of a supply file (lcsc, mpn, channel, stock, price_breaks, currency, date)"
                ));
            }
        }
        let mut rows = Vec::new();
        let mut seen: BTreeSet<(String, String, &'static str)> = BTreeSet::new();
        for (line, fields) in body {
            let at = format!("{shown}:{line}");
            if fields.len() != names.len() {
                return Err(format!(
                    "{at}: the row has {} fields but the header (line {hline}) has {}",
                    fields.len(),
                    names.len()
                ));
            }
            let cell = |name: &str| {
                column(name)
                    .map(|i| fields[i].trim())
                    .filter(|c| !c.is_empty())
            };
            let (lcsc, mpn) = (cell("lcsc").unwrap_or(""), cell("mpn").unwrap_or(""));
            if lcsc.is_empty() && mpn.is_empty() {
                return Err(format!(
                    "{at}: the row names no part: give an lcsc or an mpn"
                ));
            }
            let word = cell("channel").unwrap_or("");
            let channel = Channel::parse(word).ok_or_else(|| {
                format!("{at}: channel `{word}` is not jlcpcb-assembly or lcsc-retail")
            })?;
            let stock = match cell("stock") {
                None => None,
                Some(s) => Some(
                    s.parse::<u64>()
                        .map_err(|_| format!("{at}: stock `{s}` is not a whole number"))?,
                ),
            };
            let breaks = cell("price_breaks").unwrap_or("");
            let price_breaks = price_breaks(breaks).ok_or_else(|| {
                format!(
                    "{at}: price_breaks `{breaks}` is not like `1:0.0100 100:0.0050` (pieces:price, rising)"
                )
            })?;
            let date = cell("date").unwrap_or("");
            let fetched = crate::refresh::days_of(date)
                .and_then(|d| u64::try_from(d).ok())
                .map(|d| d * 86_400)
                .ok_or_else(|| format!("{at}: date `{date}` is not like 2026-11-01"))?;
            if !seen.insert((lcsc.to_string(), mpn.to_string(), channel.word())) {
                return Err(format!(
                    "{at}: the part {} is there twice for {}",
                    if lcsc.is_empty() { mpn } else { lcsc },
                    channel.word()
                ));
            }
            rows.push(Row {
                lcsc: lcsc.to_string(),
                mpn: mpn.to_string(),
                quote: Quote {
                    provider: "file".to_string(),
                    channel,
                    stock,
                    price_breaks,
                    currency: cell("currency").unwrap_or("USD").to_string(),
                    fetched,
                },
            });
        }
        Ok(FileProvider { rows })
    }
}

impl SupplyProvider for FileProvider {
    fn name(&self) -> &str {
        "file"
    }

    fn cacheable(&self) -> bool {
        false
    }

    fn lookup(&self, part: &PartKey) -> Result<Vec<Quote>, SupplyError> {
        Ok(self
            .rows
            .iter()
            .filter(|r| part.matches(&r.lcsc, &r.mpn))
            .map(|r| r.quote.clone())
            .collect())
    }
}
