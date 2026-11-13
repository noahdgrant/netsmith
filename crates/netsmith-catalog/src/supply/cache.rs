//! The cache of a provider's answers (ADR 0047).
//!
//! One CSV file for each provider, `<dir>/<provider>.csv`, a row for each part and channel:
//!
//! ```csv
//! lcsc,mpn,channel,stock,price_breaks,currency,fetched
//! C900001,TESTRES-10K,jlcpcb-assembly,5000,1:0.0100 100:0.0050,USD,1793580000
//! ```
//!
//! `fetched` is Unix seconds; `price_breaks` is `from:price` pairs; an empty `stock` is
//! unknown. It is plain text so a person can read it and delete it. It is a cache, not
//! design data: a row that cannot be read is ignored, and a file that cannot be written is
//! not an error, since the answer in hand is still good.

use std::fs;
use std::path::PathBuf;

use netsmith_solve::parse_csv;

use super::{Channel, PartKey, PriceBreak, Quote};

const HEADER: &str = "lcsc,mpn,channel,stock,price_breaks,currency,fetched";

#[derive(Debug, Clone)]
pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    pub fn new(dir: PathBuf) -> Cache {
        Cache { dir }
    }

    /// The directory the cache is in by default: `supply/` in netsmith's cache directory
    /// (`$NETSMITH_CACHE_DIR`, else `$XDG_CACHE_HOME/netsmith`, else `~/.cache/netsmith`).
    pub fn default_dir() -> Option<PathBuf> {
        crate::refresh::cache_base().map(|b| b.join("supply"))
    }

    fn file(&self, provider: &str) -> PathBuf {
        self.dir.join(format!("{provider}.csv"))
    }

    /// Every row of the provider's file; rows that cannot be read are left out.
    fn rows(&self, provider: &str) -> Vec<(String, String, Quote)> {
        let Ok(text) = fs::read_to_string(self.file(provider)) else {
            return Vec::new();
        };
        let Ok(records) = parse_csv(&text) else {
            return Vec::new();
        };
        records
            .into_iter()
            .skip(1)
            .filter_map(|(_, f)| row(provider, &f))
            .collect()
    }

    /// What the cache holds for a part from this provider: a quote for each channel.
    pub fn get(&self, provider: &str, part: &PartKey) -> Vec<Quote> {
        self.rows(provider)
            .into_iter()
            .filter(|(lcsc, mpn, _)| part.matches(lcsc, mpn))
            .map(|(_, _, q)| q)
            .collect()
    }

    /// Replace what the cache holds for a part with `quotes`. A failure to write is ignored.
    pub fn put(&self, provider: &str, part: &PartKey, quotes: &[Quote]) {
        let mut rows: Vec<(String, String, Quote)> = self
            .rows(provider)
            .into_iter()
            .filter(|(lcsc, mpn, _)| !part.matches(lcsc, mpn))
            .collect();
        let lcsc = part.lcsc.clone().unwrap_or_default();
        let mpn = part.mpn.clone().unwrap_or_default();
        rows.extend(
            quotes
                .iter()
                .map(|q| (lcsc.clone(), mpn.clone(), q.clone())),
        );
        let mut text = format!("{HEADER}\n");
        for (lcsc, mpn, q) in &rows {
            let fields = [
                lcsc.clone(),
                mpn.clone(),
                q.channel.word().to_string(),
                q.stock.map(|s| s.to_string()).unwrap_or_default(),
                q.price_breaks
                    .iter()
                    .map(|b| format!("{}:{}", b.from, b.price))
                    .collect::<Vec<_>>()
                    .join(" "),
                q.currency.clone(),
                q.fetched.to_string(),
            ];
            let line: Vec<String> = fields.iter().map(|f| quote_field(f)).collect();
            text.push_str(&line.join(","));
            text.push('\n');
        }
        if fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let target = self.file(provider);
        let temp = self.dir.join(format!(".{provider}.csv.tmp"));
        if fs::write(&temp, text).is_ok() && fs::rename(&temp, &target).is_err() {
            let _ = fs::remove_file(&temp);
        }
    }
}

fn quote_field(field: &str) -> String {
    if field.contains([',', '"', '\n']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

fn row(provider: &str, f: &[String]) -> Option<(String, String, Quote)> {
    let [lcsc, mpn, channel, stock, breaks, currency, fetched] = f else {
        return None;
    };
    Some((
        lcsc.clone(),
        mpn.clone(),
        Quote {
            provider: provider.to_string(),
            channel: Channel::parse(channel)?,
            stock: if stock.trim().is_empty() {
                None
            } else {
                Some(stock.trim().parse().ok()?)
            },
            price_breaks: price_breaks(breaks)?,
            currency: currency.clone(),
            fetched: fetched.trim().parse().ok()?,
        },
    ))
}

/// `1:0.0100 100:0.0050`, rising; empty is no price.
pub(super) fn price_breaks(text: &str) -> Option<Vec<PriceBreak>> {
    let mut out: Vec<PriceBreak> = Vec::new();
    for pair in text.split_whitespace() {
        let (from, price) = pair.split_once(':')?;
        let b = PriceBreak {
            from: from.parse().ok()?,
            price: price
                .parse()
                .ok()
                .filter(|p: &f64| p.is_finite() && *p >= 0.0)?,
        };
        if out.last().is_some_and(|last| last.from >= b.from) {
            return None;
        }
        out.push(b);
    }
    Some(out)
}
