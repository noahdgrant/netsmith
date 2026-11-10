//! Price and stock from a provider (ADR 0047, provisional).
//!
//! A catalog row says what a part is. What a supplier says about it today (the pieces in
//! stock, the price at each quantity) comes from a [`SupplyProvider`], is asked for when the
//! bill of materials or `fab` is made, and is kept in a local cache with the time it was
//! read ([`cache`]). When no provider has an answer the price and stock are **unknown**:
//! never guessed from a neighbouring row and never 0.
//!
//! The one provider built reads a local file ([`file`]); it is how the tests work. No
//! provider here talks to a supplier: their terms of use are unread (ADR 0039, ADR 0047).
//! The solver, the checker and the language know nothing of this module.
//!
//! **The decision** (ADR 0047, provisional). A catalog row keeps what a part is (`lcsc`, `mpn`,
//! `library`, `package`, the electrical values, `source`, `checked`); price, stock and date are
//! a supplier's word about a day and go stale within days. A file that still has the columns is
//! read as the last resort of the chain, the `catalog` answer (the bill of materials and
//! `catalog age` fall back to the row's cells and say "catalog file, entered by hand"; it is not
//! a type here), and the shipped files leave them blank. The solver never reads them: the
//! stock-0 rule of ADR 0039 item 9 is withdrawn, and a row is offered whatever its `stock` says.
//! A sold-out part therefore no longer changes which part the solver picks; it changes what
//! resolution picks (`resolve.rs`), and says so.
//!
//! - *The answer says which stock it is.* JLCPCB assembly stock and LCSC retail stock differ for
//!   the same number (ADR 0039 recorded 1,085,700 against 2,059,008). A bill for a JLCPCB order
//!   shows `JlcpcbAssembly`; if a provider has only `LcscRetail` that is shown, labelled, and is
//!   never presented as assembly stock. The manifest's `channel` says which a project means.
//! - *A price is a ladder.* A line is priced at the highest break at or below the quantity it
//!   needs; below the first break the first break's price is shown, marked "from N pieces". No
//!   currency is converted. The quantity is for one board (ADR 0035's order quantity is not
//!   spread across boards).
//! - *Cache* (`$NETSMITH_CACHE_DIR`, else `$XDG_CACHE_HOME/netsmith`, else `~/.cache/netsmith`,
//!   in `supply/<provider>.csv`, the format in [`cache`]). An entry younger than `cache_hours`
//!   (24) is used without asking; an older one makes a request and a good answer replaces it; if
//!   the request fails or the run is offline the older entry is used and shown with its age
//!   ("read 12 days ago"), never silently refreshed and never dropped for being old; an entry
//!   older than 90 days is marked stale wherever it is shown; a part the provider does not list,
//!   and a failed request, are not cached. An unreadable row is ignored with a note. The file
//!   provider is not cached.
//! - *Unknown* is the state of a part with no quote: with no provider, with `--offline`, with
//!   the network down, and for a part the provider does not list ("not listed by <provider>").
//!   It is shown wherever a price or stock would be and is never 0; a total counts the priced
//!   lines and says how many have no price; nothing guesses a price from a neighbouring row. A
//!   made-up part (no `lcsc`) has nothing to look up. A line with a quote says "<provider>,
//!   <channel>, read <date>, <age>".
//! - *Configuration.* The `[supply]` table of `netsmith.toml` has `provider` (only `"file"` is
//!   built), `file` (relative to the project), `channel` (`jlcpcb-assembly` or `lcsc-retail`) and
//!   `cache_hours`. The manifest's parser refuses an unknown key, so a secret cannot be committed
//!   by mistake; a credential is given by the name of an environment variable and never by its
//!   value (owner, 2026-11-03), and the value is never stored in the manifest, a project file or
//!   the cache. The series of generic parts is the `series` key of `[project]` (owner,
//!   2026-11-04: not a statement of the language).
//! - *Terms of use.* Nothing here contacts a supplier. No provider that does is built until the
//!   maintainer has read the supplier's terms and the official API's licence: JLCPCB's
//!   `robots.txt` disallows `/api`, and ADR 0039 recorded that its terms could not be read. A
//!   real provider is a [`SupplyProvider`] that uses the supplier's official API with the user's
//!   own key and changes nothing else.
//!
//! Decided by the owner (2026-11-03 and 2026-11-04): the `[supply]` table; the `series` key; a
//! quantity below the first break shown at the first break's price with "from N pieces"; the
//! 24 hour cache and the 90 day stale mark.
//!
//! **Not built, and open for the maintainer** (ADR 0047, word for word where quoted):
//! - "Generic parts appended after the identity rows or instead of them? Appended keeps every
//!   design as it is; instead is the clean model and changes which part existing designs get."
//!   (Issue #33 records "appended, on by default" as decided on 2026-11-03; the ADR text does not.)
//! - "Is "JLCPCB Basic, then Preferred, then Extended, then file order" the preferred-parts list,
//!   or is it a file of its own (`preferred.csv`, a project's list that wins)?"
//! - "Which real providers, and in which order: the LCSC API, JLCAPI, an aggregator (the Nexar
//!   API, which Altium's bill of materials and KiCost use) or a community snapshot (the KiCad
//!   JLCPCB plugin downloads one daily). Each needs the maintainer to read its terms and to
//!   decide whose key a user brings."
//! - "Should the editor offer a swap with the checks run for each alternative, and should `fab`
//!   refuse an unresolved part (this ADR says it does) or build without it?" (The checks are
//!   built; the swap is not.)
//! - "The shipped `price`, `stock` and `date` columns: keep as the `catalog` provider's input
//!   (this ADR), or remove the columns from the format."
//! - The user-level credentials file (`$XDG_CONFIG_HOME/netsmith/supply.toml`) of item 5 was
//!   not part of the question the owner answered, and the spelling of the manifest field that
//!   names the variable (for example `key_env = "NETSMITH_LCSC_KEY"`) is not decided: neither
//!   is built.
//! - A `--offline` flag for the bill of materials and `fab` (`Supply::with_offline` exists and
//!   is tested), a named preferred-parts file, the quantity of an order across boards, and the
//!   editor's swap. Quantity breaks across several boards, currency conversion, taxes,
//!   shipping and JLCPCB's feeder fee for each Extended part are "not decided here".

pub mod cache;
pub mod file;

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use netsmith_solve::Availability;

pub use cache::Cache;
pub use file::FileProvider;

/// Which stock a quote is about. JLCPCB's assembly stock (the pieces it holds for its
/// assembly service, which a board ordered with assembly uses) is not LCSC's retail stock
/// (the pieces LCSC sells to a buyer): the same number has different counts in the two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    JlcpcbAssembly,
    LcscRetail,
}

impl Channel {
    /// The word a file and the manifest use.
    pub fn word(self) -> &'static str {
        match self {
            Channel::JlcpcbAssembly => "jlcpcb-assembly",
            Channel::LcscRetail => "lcsc-retail",
        }
    }

    pub fn parse(word: &str) -> Option<Channel> {
        match word.trim() {
            "jlcpcb-assembly" => Some(Channel::JlcpcbAssembly),
            "lcsc-retail" => Some(Channel::LcscRetail),
            _ => None,
        }
    }

    /// What a person reads: `JLCPCB assembly stock`.
    pub fn describe(self) -> &'static str {
        match self {
            Channel::JlcpcbAssembly => "JLCPCB assembly stock",
            Channel::LcscRetail => "LCSC retail stock",
        }
    }
}

/// How a part is asked for: by LCSC number, else by the manufacturer's.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PartKey {
    pub lcsc: Option<String>,
    pub mpn: Option<String>,
}

impl PartKey {
    pub fn lcsc(lcsc: &str) -> PartKey {
        PartKey {
            lcsc: Some(lcsc.to_string()),
            mpn: None,
        }
    }

    /// Whether the part has either number: a made-up part has nothing to look up.
    pub fn is_empty(&self) -> bool {
        self.lcsc.is_none() && self.mpn.is_none()
    }

    /// Whether a row that names `lcsc` and `mpn` is this part. The LCSC number decides if
    /// the key has one.
    pub fn matches(&self, lcsc: &str, mpn: &str) -> bool {
        match (&self.lcsc, &self.mpn) {
            (Some(k), _) => !lcsc.is_empty() && k == lcsc,
            (None, Some(k)) => !mpn.is_empty() && k == mpn,
            (None, None) => false,
        }
    }
}

/// The price of one piece from a quantity on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PriceBreak {
    pub from: u32,
    pub price: f64,
}

/// What a provider said about one part for one channel.
#[derive(Debug, Clone, PartialEq)]
pub struct Quote {
    /// The provider that answered (`file`).
    pub provider: String,
    pub channel: Channel,
    /// Pieces in stock, or `None` if the provider does not say. Not 0.
    pub stock: Option<u64>,
    /// Rising by `from`.
    pub price_breaks: Vec<PriceBreak>,
    /// `USD`. Nothing converts it.
    pub currency: String,
    /// Unix seconds: when the provider answered.
    pub fetched: u64,
}

impl Quote {
    /// The price of one piece when `quantity` pieces are needed: the highest break at or
    /// below the quantity. Below the first break the first break's price is given and the
    /// second value is the quantity it starts at (the price applies "from N pieces").
    /// `None` if the quote has no price.
    pub fn price_for(&self, quantity: u32) -> Option<(f64, Option<u32>)> {
        let first = self.price_breaks.first()?;
        match self
            .price_breaks
            .iter()
            .rev()
            .find(|b| b.from <= quantity.max(1))
        {
            Some(b) => Some((b.price, None)),
            None => Some((first.price, Some(first.from))),
        }
    }

    /// `jlcpcb-assembly` or `lcsc-retail`, with the provider: `file, JLCPCB assembly stock`.
    pub fn source(&self) -> String {
        format!("{}, {}", self.provider, self.channel.describe())
    }
}

/// Why a provider gave no answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupplyError {
    /// There is no network, or the run was told not to use it.
    Offline,
    /// The provider could not answer (a refusal, a missing credential, a bad reply).
    Unavailable(String),
}

impl fmt::Display for SupplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SupplyError::Offline => f.write_str("offline"),
            SupplyError::Unavailable(why) => f.write_str(why),
        }
    }
}

/// A source of price and stock for a part number.
pub trait SupplyProvider: Send + Sync {
    /// The provider's name: `file`, later `lcsc` or `jlcapi`.
    fn name(&self) -> &str;

    /// Whether answers are worth keeping in the cache. False for a provider that reads a
    /// local file: the file is already a local copy.
    fn cacheable(&self) -> bool;

    /// Every quote the provider has for the part, one for each channel. An empty list is a
    /// part the provider does not list; an error is not an answer.
    fn lookup(&self, part: &PartKey) -> Result<Vec<Quote>, SupplyError>;
}

/// What asking for a part came to.
#[derive(Debug, Clone, PartialEq)]
pub enum Lookup {
    /// A quote. It may be old: its `fetched` says when.
    Quoted(Quote),
    /// No provider has an answer; the text says why, for a message.
    Unknown(String),
}

impl Lookup {
    pub fn quote(&self) -> Option<&Quote> {
        match self {
            Lookup::Quoted(q) => Some(q),
            Lookup::Unknown(_) => None,
        }
    }
}

/// The providers a project configured, the cache in front of the cacheable ones, and the
/// channel whose stock the bill of materials shows.
pub struct Supply {
    providers: Vec<Box<dyn SupplyProvider>>,
    cache: Option<Cache>,
    channel: Channel,
    cache_secs: u64,
    offline: bool,
    now: u64,
}

impl fmt::Debug for Supply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Supply")
            .field(
                "providers",
                &self.providers.iter().map(|p| p.name()).collect::<Vec<_>>(),
            )
            .field("channel", &self.channel)
            .finish()
    }
}

/// How long an answer in the cache is used without asking again, unless the manifest says.
pub const CACHE_HOURS: u64 = 24;

impl Supply {
    /// No provider: every price and stock is unknown.
    pub fn none() -> Supply {
        Supply {
            providers: Vec::new(),
            cache: None,
            channel: Channel::JlcpcbAssembly,
            cache_secs: CACHE_HOURS * 3600,
            offline: false,
            now: crate::refresh::now_secs(),
        }
    }

    pub fn with_provider(mut self, provider: Box<dyn SupplyProvider>) -> Supply {
        self.providers.push(provider);
        self
    }

    /// Keep the answers of cacheable providers in this directory.
    pub fn with_cache_dir(mut self, dir: PathBuf) -> Supply {
        self.cache = Some(Cache::new(dir));
        self
    }

    pub fn with_channel(mut self, channel: Channel) -> Supply {
        self.channel = channel;
        self
    }

    pub fn with_cache_hours(mut self, hours: u64) -> Supply {
        self.cache_secs = hours.saturating_mul(3600);
        self
    }

    /// Ask no provider that would use the network; use kept answers of any age.
    pub fn with_offline(mut self, offline: bool) -> Supply {
        self.offline = offline;
        self
    }

    /// The time a cached answer's age is counted from, Unix seconds (a test sets it).
    pub fn with_now(mut self, now: u64) -> Supply {
        self.now = now;
        self
    }

    pub fn channel(&self) -> Channel {
        self.channel
    }

    /// Whether any provider is configured.
    pub fn is_configured(&self) -> bool {
        !self.providers.is_empty()
    }

    /// The quote for a part from the first provider that has one: fresh from the cache, else
    /// from the provider (which then fills the cache), else an older cached answer if the
    /// provider cannot answer or the run is offline. The quote of the project's channel is
    /// preferred; if the provider has only the other channel, that is the answer and it
    /// says which it is.
    pub fn lookup(&self, part: &PartKey) -> Lookup {
        if self.providers.is_empty() {
            return Lookup::Unknown(
                "no supply provider is configured ([supply] in netsmith.toml)".to_string(),
            );
        }
        if part.is_empty() {
            return Lookup::Unknown("the part has no LCSC or manufacturer's number".to_string());
        }
        let mut why: Vec<String> = Vec::new();
        for provider in &self.providers {
            let name = provider.name();
            let kept = match (&self.cache, provider.cacheable()) {
                (Some(cache), true) => cache.get(name, part),
                _ => Vec::new(),
            };
            let age = |quotes: &[Quote]| {
                quotes
                    .iter()
                    .map(|q| self.now.saturating_sub(q.fetched))
                    .max()
                    .unwrap_or(u64::MAX)
            };
            if !kept.is_empty() && age(&kept) <= self.cache_secs {
                return Lookup::Quoted(self.choose(kept));
            }
            let asked = if self.offline && provider.cacheable() {
                Err(SupplyError::Offline)
            } else {
                provider.lookup(part)
            };
            match asked {
                Ok(quotes) if !quotes.is_empty() => {
                    if let (Some(cache), true) = (&self.cache, provider.cacheable()) {
                        cache.put(name, part, &quotes);
                    }
                    return Lookup::Quoted(self.choose(quotes));
                }
                Ok(_) => why.push(format!("{name} does not list the part")),
                Err(e) => {
                    if kept.is_empty() {
                        why.push(format!("{name}: {e}"));
                    } else {
                        return Lookup::Quoted(self.choose(kept));
                    }
                }
            }
        }
        Lookup::Unknown(why.join("; "))
    }

    fn choose(&self, quotes: Vec<Quote>) -> Quote {
        let mut quotes = quotes;
        let i = quotes
            .iter()
            .position(|q| q.channel == self.channel)
            .unwrap_or(0);
        quotes.swap_remove(i)
    }
}

/// The stock of the project's channel, for the step that resolves generic parts to rows.
#[derive(Debug)]
pub struct StockOf(pub Supply);

impl Availability for StockOf {
    fn stock(&self, lcsc: &str) -> Option<u64> {
        self.0.lookup(&PartKey::lcsc(lcsc)).quote().and_then(|q| {
            // Only the stock a project means counts: the other channel's is not its stock.
            (q.channel == self.0.channel).then_some(q.stock).flatten()
        })
    }
}

impl Supply {
    /// This supply as the catalogs' source of stock.
    pub fn into_availability(self) -> Arc<dyn Availability> {
        Arc::new(StockOf(self))
    }
}

#[cfg(test)]
mod tests;
