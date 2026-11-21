//! Finding parts a catalog does not have yet: a **search provider** (ADR 0143, stage 5, issue
//! #1460), beside the supply provider of [`crate::supply`].
//!
//! A [`crate::supply::SupplyProvider`] is asked for the price and stock of a part a catalog
//! already has, by its offer's `sku`. A [`SearchProvider`] is asked what parts there are: by
//! keywords, by a manufacturer's number, and by parametric filters. What it answers is a list of
//! **found parts** ([`FoundPart`]): the manufacturer and the `mpn` as the provider spells them,
//! the package and the key values as the supplier states them, the datasheet's address, and for
//! each supplier netsmith has an offers file of (`lcsc`, `digikey`, `mouser`) the supplier's
//! number (`sku`), its stock and its price breaks. A found part is not a catalog row and not an
//! offer: `netsmith catalog add --from <supplier> <sku or mpn>` asks the provider for the record of
//! one part again, checks it is the part named, and writes the row and the offers with a `source`
//! that names the record and the day ([`add`]). Nothing here reads a number from anywhere but the
//! provider's answer: a cell the answer does not fill stays empty and is listed as needing the
//! datasheet.
//!
//! The rules are those of ADR 0047 for a provider that asks a supplier:
//! - *Off unless named.* A provider is used only when the project names it (`[supply.nexar]` or
//!   `[supply.digikey]` in `netsmith.toml`) and the environment variables that table names are set. The manifest holds
//!   the variables' *names*, never their values; the values are read from the environment when a
//!   request must go out and are kept in memory only: never in the manifest, a project file, the
//!   cache or a message. No test and no default path asks a supplier.
//! - *Unknown is not 0.* A stock the supplier gives no number for is [`Stock`] with the reason
//!   (`not stocked`, `in stock, no quantity`, `on request`, `unknown`), never a guessed count, and
//!   a price is the supplier's ladder or nothing. No currency is converted.
//! - *Errors* are [`SupplyError`]: `Offline` (`--offline` with no kept answer) or `Unavailable`
//!   with the reason (a refusal, a missing credential, an answer of another shape).
//! - *Polite.* One request at a time with a pause between two (`--delay`), a User-Agent that says
//!   what the program is, retries with a growing wait for an answer that means "later" (429, 5xx),
//!   a stop for one that means "no", and the answers kept in a cache (`search/<provider>/` in
//!   netsmith's cache directory) for the `[supply] cache_hours` (24): a search asked twice in a
//!   day asks once. `--offline` uses a kept answer of any age, and an answer older than 90 days is
//!   called stale wherever it is shown.
//! - *Terms of use.* A person must read the supplier's and the provider's terms of use before the
//!   feature is advertised (issue #10); the help of the commands says so.
//!
//! The providers are Nexar's (Octopart's) GraphQL API ([`nexar`]) and Digi-Key's own Product
//! Information API ([`digikey`], issue #1477), on what [`http`] shares.

pub mod add;
pub mod digikey;
pub mod http;
pub mod nexar;

use serde_json::{Value as Json, json};

pub use crate::supply::{PriceBreak, SupplyError};

/// The suppliers a found part's offers are kept for: those netsmith reads an offers file of
/// (ADR 0143), in their order.
pub use netsmith_solve::offers::SUPPLIERS;

/// The age in days after which a kept answer is called stale (ADR 0047).
pub const STALE_DAYS: u64 = 90;

/// What to look for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Query {
    /// Words matched against the parts' descriptions, categories and values.
    pub keywords: Option<String>,
    /// A manufacturer's number, or part of one.
    pub mpn: Option<String>,
    /// Parametric filters, each the short name of an attribute and a value, in the order given; a
    /// name given twice takes both values.
    pub filters: Vec<(String, String)>,
    /// Only parts some seller has in stock.
    pub in_stock: bool,
    /// How many parts to return.
    pub limit: u32,
    /// Also ask for the values, the package and the datasheet (on Nexar a plan that includes
    /// them; see [`nexar`]).
    pub details: bool,
}

impl Query {
    /// The query as a message names it: `"100nF 0402"`, `mpn "LM358"`, with the filters.
    pub fn describe(&self) -> String {
        let mut words = Vec::new();
        if let Some(k) = &self.keywords {
            words.push(format!("{k:?}"));
        }
        if let Some(m) = &self.mpn {
            words.push(format!("mpn {m:?}"));
        }
        for (name, value) in &self.filters {
            words.push(format!("{name}={value}"));
        }
        if self.in_stock {
            words.push("in stock".to_string());
        }
        words.join(", ")
    }
}

/// A supplier's stock of a part, as the supplier states it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stock {
    /// Pieces in stock.
    Count(u64),
    /// The supplier does not carry the part.
    NotStocked,
    /// In stock, and the supplier gives no number.
    InStock,
    /// Ask the supplier for a quote.
    OnRequest,
    /// No number, or not a number.
    Unknown,
}

impl Stock {
    /// The count, when the supplier gives one.
    pub fn count(&self) -> Option<u64> {
        match self {
            Stock::Count(n) => Some(*n),
            _ => None,
        }
    }

    /// What a message says for a stock with no count; `None` for a count.
    pub fn note(&self) -> Option<&'static str> {
        match self {
            Stock::Count(_) => None,
            Stock::NotStocked => Some("not stocked"),
            Stock::InStock => Some("in stock, no quantity given"),
            Stock::OnRequest => Some("on request"),
            Stock::Unknown => Some("unknown"),
        }
    }
}

/// What one supplier offers of a found part.
#[derive(Debug, Clone, PartialEq)]
pub struct SupplierOffer {
    /// The supplier, as its offers file is named: `lcsc`, `digikey`, `mouser`.
    pub supplier: String,
    /// The seller as the provider names it (`DigiKey`).
    pub seller: String,
    /// Whether the provider says the seller is authorized by the manufacturer.
    pub authorized: bool,
    /// The supplier's own number for the part.
    pub sku: String,
    pub stock: Stock,
    /// How the pieces are packed (cut tape, reel), as the supplier says.
    pub packaging: Option<String>,
    /// The fewest pieces the supplier sells.
    pub moq: Option<u64>,
    /// Rising by `from`; empty when the supplier gives no price.
    pub price_breaks: Vec<PriceBreak>,
    /// The currency of the prices (`USD`). Nothing converts it.
    pub currency: Option<String>,
    /// When the provider last heard from the supplier about this offer, as it writes the time.
    pub updated: Option<String>,
}

/// A value of a found part, as the supplier states it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    /// The attribute's name (`Capacitance`).
    pub name: String,
    /// The attribute's short name, which a filter uses (`capacitance`).
    pub shortname: String,
    /// The value as the provider shows it (`100nF`).
    pub value: String,
}

/// One part a search provider found.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundPart {
    /// The manufacturer as the provider spells it, which may not be the datasheet's.
    pub manufacturer: String,
    pub mpn: String,
    pub description: Option<String>,
    /// The package as the supplier states it; `None` when not asked ([`Query::details`]) or not
    /// stated.
    pub package: Option<String>,
    /// The key values; empty when not asked or not stated.
    pub values: Vec<Value>,
    /// The address of the datasheet; `None` when not asked or not given.
    pub datasheet: Option<String>,
    /// The provider's page of the part.
    pub url: Option<String>,
    /// The offers of the suppliers netsmith has offers files of, in the order of [`SUPPLIERS`].
    pub offers: Vec<SupplierOffer>,
    /// How many other sellers the provider lists, whose offers are not shown.
    pub other_sellers: usize,
}

impl FoundPart {
    /// The offers of one supplier.
    pub fn offers_of<'a>(&'a self, supplier: &'a str) -> impl Iterator<Item = &'a SupplierOffer> {
        self.offers.iter().filter(move |o| o.supplier == supplier)
    }
}

/// What a provider answered.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    /// The provider's name (`nexar`).
    pub provider: String,
    /// What a message calls it (`Nexar (Octopart)`).
    pub described: String,
    /// Where the answer came from: the provider's address, as a `source` names it.
    pub endpoint: String,
    /// How many parts match, when the provider says; it may be more than `parts`.
    pub hits: Option<u64>,
    pub parts: Vec<FoundPart>,
    /// Unix seconds: when the provider answered.
    pub read: u64,
    /// Whether the answer came from the cache.
    pub cached: bool,
    /// Whether the values, the package and the datasheet were asked for.
    pub details: bool,
    /// Requests that went out for it (0 from the cache).
    pub requests: u32,
}

/// A source of parts: what parts there are, and the record of one.
pub trait SearchProvider {
    /// The provider's name, as the manifest names it: `nexar`.
    fn name(&self) -> &str;

    /// What a message calls it: `Nexar (Octopart)`.
    fn describe(&self) -> &str;

    /// The parts that match `query`. An empty list is a search that found nothing; an error is
    /// not an answer.
    fn search(&mut self, query: &Query) -> Result<Answer, SupplyError>;

    /// The parts the provider has for `number`, a sku of `supplier` or a manufacturer's number,
    /// for [`add`] to check against what was named. With `details`, as for [`Query::details`].
    fn record(
        &mut self,
        supplier: &str,
        number: &str,
        details: bool,
    ) -> Result<Answer, SupplyError>;
}

/// What a message says of when an answer was read: `read 2026-11-20` and, from the cache, its
/// age and a stale mark past [`STALE_DAYS`].
pub fn read_note(answer: &Answer, now: u64) -> String {
    let day = crate::refresh::date_of(answer.read);
    if !answer.cached {
        return format!("read {day}");
    }
    let age = now.saturating_sub(answer.read);
    let days = age / 86_400;
    let how = if days == 0 {
        format!("{} hours ago", age / 3600)
    } else {
        format!("{days} days ago")
    };
    let stale = if days > STALE_DAYS { ", stale" } else { "" };
    format!("read {day}, kept from {how}{stale}")
}

fn price_text(offer: &SupplierOffer) -> String {
    if offer.price_breaks.is_empty() {
        return "no price".to_string();
    }
    let currency = offer.currency.as_deref().unwrap_or("");
    let breaks: Vec<String> = offer
        .price_breaks
        .iter()
        .take(4)
        .map(|b| format!("{}+ {}", b.from, b.price))
        .collect();
    let more = if offer.price_breaks.len() > 4 {
        ", ..."
    } else {
        ""
    };
    format!("{} {currency}{more}", breaks.join(", "))
}

fn stock_text(stock: &Stock) -> String {
    match stock {
        Stock::Count(n) => format!("stock {n}"),
        Stock::NotStocked => "not stocked".to_string(),
        Stock::InStock => "in stock, no quantity given".to_string(),
        Stock::OnRequest => "stock on request".to_string(),
        Stock::Unknown => "stock unknown".to_string(),
    }
}

/// What `catalog search` prints: a line about the answer, then each part with its offers and the
/// command that adds it.
pub fn render_answer(query: &Query, answer: &Answer, now: u64) -> String {
    let mut out = format!(
        "{} {}: {} part(s){} for {}\n",
        answer.described,
        read_note(answer, now),
        answer.parts.len(),
        answer.hits.map(|h| format!(" of {h}")).unwrap_or_default(),
        query.describe()
    );
    for (i, part) in answer.parts.iter().enumerate() {
        out.push_str(&format!(
            "\n{}. {} {}{}\n",
            i + 1,
            part.manufacturer,
            part.mpn,
            part.package
                .as_deref()
                .map(|p| format!(", package {p}"))
                .unwrap_or_default()
        ));
        if let Some(d) = &part.description {
            out.push_str(&format!("   {d}\n"));
        }
        if !part.values.is_empty() {
            let values: Vec<String> = part
                .values
                .iter()
                .take(8)
                .map(|v| format!("{} {}", v.name, v.value))
                .collect();
            out.push_str(&format!("   values: {}\n", values.join("; ")));
        }
        if let Some(d) = &part.datasheet {
            out.push_str(&format!("   datasheet: {d}\n"));
        }
        if let Some(u) = &part.url {
            out.push_str(&format!("   page: {u}\n"));
        }
        for offer in &part.offers {
            out.push_str(&format!(
                "   {:<8} {}  {}  {}{}\n",
                offer.supplier,
                offer.sku,
                stock_text(&offer.stock),
                price_text(offer),
                offer
                    .packaging
                    .as_deref()
                    .map(|p| format!("  ({p})"))
                    .unwrap_or_default()
            ));
        }
        if part.offers.is_empty() {
            out.push_str("   no offer of lcsc, digikey or mouser\n");
        }
        if part.other_sellers > 0 {
            out.push_str(&format!(
                "   and {} other seller(s), not shown\n",
                part.other_sellers
            ));
        }
        if let Some(offer) = part.offers.first() {
            out.push_str(&format!(
                "   add: netsmith catalog add --from {} {} --to catalogs/<file>.csv{}\n",
                offer.supplier,
                offer.sku,
                if answer.details { " --details" } else { "" }
            ));
        }
    }
    if !answer.details {
        out.push_str("\nthe package, the values and the datasheet were not asked for: --details asks (on Nexar, a plan that includes them)\n");
    }
    out
}

fn json_part(part: &FoundPart) -> Json {
    json!({
        "manufacturer": part.manufacturer,
        "mpn": part.mpn,
        "description": part.description,
        "package": part.package,
        "values": part.values.iter().map(|v| json!({
            "name": v.name,
            "shortname": v.shortname,
            "value": v.value,
        })).collect::<Vec<_>>(),
        "datasheet": part.datasheet,
        "url": part.url,
        "offers": part.offers.iter().map(|o| json!({
            "supplier": o.supplier,
            "seller": o.seller,
            "authorized": o.authorized,
            "sku": o.sku,
            "stock": o.stock.count(),
            "stock_note": o.stock.note(),
            "packaging": o.packaging,
            "moq": o.moq,
            "price_breaks": o.price_breaks.iter().map(|b| json!({"from": b.from, "price": b.price})).collect::<Vec<_>>(),
            "currency": o.currency,
            "updated": o.updated,
        })).collect::<Vec<_>>(),
        "other_sellers": part.other_sellers,
    })
}

/// What `catalog search --format json` prints: what [`render_answer`] says, as data.
pub fn json_answer(query: &Query, answer: &Answer, now: u64) -> Json {
    json!({
        "command": "catalog search",
        "ok": true,
        "provider": answer.provider,
        "query": {
            "keywords": query.keywords,
            "mpn": query.mpn,
            "filters": query.filters.iter().map(|(n, v)| json!({"name": n, "value": v})).collect::<Vec<_>>(),
            "in_stock": query.in_stock,
            "limit": query.limit,
            "details": query.details,
        },
        "read": crate::refresh::date_of(answer.read),
        "cached": answer.cached,
        "stale": answer.cached && now.saturating_sub(answer.read) / 86_400 > STALE_DAYS,
        "requests": answer.requests,
        "hits": answer.hits,
        "parts": answer.parts.iter().map(json_part).collect::<Vec<_>>(),
    })
}

/// A found part as data, for `catalog add --from --format json`.
pub fn json_found(part: &FoundPart) -> Json {
    json_part(part)
}
