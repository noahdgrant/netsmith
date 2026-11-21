//! Nexar's (Octopart's) GraphQL API as a [`SearchProvider`].
//!
//! What this reads, and where it was read (2026-11-20; nothing here is from memory):
//! - The token: `POST https://identity.nexar.com/connect/token`, a form of `grant_type =
//!   client_credentials`, `client_id`, `client_secret` and `scope = supply.domain`, answered with
//!   `access_token`, `expires_in` (86400) and `token_type` (`Bearer`); no refresh token, so a new
//!   one is asked for on a 401 (Altium Developer Center, Octopart API, _Authorization_:
//!   <https://www.altium.com/documentation/altium-developer-center/octopart/api/authorization>).
//! - The queries: `POST https://api.nexar.com/graphql` with `Authorization: Bearer <token>` and a
//!   JSON body (`query`, `variables`); `supSearch` for keywords, `supSearchMpn` for a
//!   manufacturer's number, both with `q`, `limit` (default 10, at most 100), `filters` (a map of
//!   attribute short names to lists of values) and `inStockOnly`, answered with `hits` and
//!   `results { part }`; `supMultiMatch` with `queries: [{ mpnOrSku, limit }]` and `options: {
//!   filters: { distributor_id: [...] } }` for the record of one part, answered with `hits`,
//!   `error` and `parts` (_Search_, _Pagination_, _Inventory_ and _Example queries_ of the same
//!   guide, and the schema at
//!   <https://altiumdeveloper.github.io/octopart-api-docs/reference/supply/schema.graphql>).
//! - A part: `mpn`, `manufacturer { name }`, `shortDescription`, `octopartUrl`, `sellers(
//!   includeBrokers: false) { company { id name } isAuthorized offers { sku inventoryLevel
//!   packaging moq updated prices { quantity price currency } } }`; with `--details` also `specs {
//!   attribute { name shortname } displayValue }` and `bestDatasheet { url }`, which the schema
//!   says need a plan of the `PRO` tier with the `TECH_SPECS` and `DATASHEETS` features. A query
//!   that asks a field the plan lacks is answered with `errors`, which this reports.
//! - `inventoryLevel` is a count, or -1 not stocked, -2 in stock with no quantity, -3 unknown,
//!   -4 on request (_Inventory_).
//! - The package is the spec of the attribute `case_package` (`Case/Package`), and the sellers
//!   are found by their company `id`: LCSC 11779, DigiKey 459, Mouser 2401 (Octopart's list of
//!   attributes and sellers, <https://octopart.com/api/v4/values>).
//!
//! Nexar's plans limit how many parts an application may be answered with (_Plans &
//! Subscriptions_ of the guide; the numbers are in the portal of the account, and none is written
//! here): the cache, the default `--limit` of 5 and one request at a time keep the use small.
//!
//! Nexar's terms of use have not been read for netsmith (issue #10): a person must read them, and
//! those of the sellers, before the feature is advertised.

use std::{
    fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value as Json, json};

use super::{Answer, FoundPart, PriceBreak, Query, SearchProvider, Stock, SupplierOffer, Value};
use crate::refresh::{cache_base, check_endpoint, now_secs};
use crate::supply::SupplyError;

/// The name of the provider, as the manifest's table names it (`[supply.nexar]`).
pub const NAME: &str = "nexar";

/// What messages call it.
pub const DESCRIBED: &str = "Nexar (Octopart)";

/// The GraphQL endpoint.
pub const GRAPHQL_URL: &str = "https://api.nexar.com/graphql";

/// The token endpoint of Nexar's identity service.
pub const TOKEN_URL: &str = "https://identity.nexar.com/connect/token";

/// The scope of a token for the supply queries.
pub const SCOPE: &str = "supply.domain";

/// The environment variable that names another GraphQL endpoint, for a proxy of your own and for
/// the tests, which never reach the real one.
pub const URL_VAR: &str = "NETSMITH_NEXAR_URL";

/// The environment variable that names another token endpoint.
pub const TOKEN_URL_VAR: &str = "NETSMITH_NEXAR_TOKEN_URL";

/// The most parts one search asks for (Nexar's maximum `limit`).
pub const MAX_LIMIT: u32 = 100;

/// What netsmith calls itself to Nexar.
pub const USER_AGENT: &str = concat!(
    "netsmith-catalog-search/",
    env!("CARGO_PKG_VERSION"),
    " (opt-in part search for an electronics design tool; one request at a time)"
);

/// The sellers whose offers a found part keeps, by Octopart's company id, with the supplier whose
/// offers file holds their numbers (<https://octopart.com/api/v4/values>, read 2026-11-20).
pub const SELLERS: [(&str, &str); 3] = [("11779", "lcsc"), ("459", "digikey"), ("2401", "mouser")];

/// The Octopart company id of a supplier netsmith has offers of.
pub fn seller_id(supplier: &str) -> Option<&'static str> {
    SELLERS
        .iter()
        .find(|(_, s)| *s == supplier)
        .map(|(id, _)| *id)
}

/// A client id and secret, read from the environment. Never written anywhere, and never shown:
/// its `Debug` says only that it is there.
#[derive(Clone)]
pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credentials(..)")
    }
}

/// Where the provider asks, how politely, and what it keeps.
#[derive(Debug, Clone)]
pub struct Settings {
    pub graphql: String,
    pub token: String,
    /// The pause between two requests that go out.
    pub delay: Duration,
    /// How many times a request that failed for a reason that may pass is tried again.
    pub retries: u32,
    /// The wait before the first retry; it doubles (a `Retry-After` is honoured, up to a minute).
    pub backoff: Duration,
    /// Where answers are kept; `None` keeps none.
    pub cache_dir: Option<PathBuf>,
    /// How long a kept answer is used instead of asking.
    pub ttl: Duration,
    /// Ask nothing: use a kept answer of any age.
    pub offline: bool,
    /// The time to stamp and age answers by; `None` is now.
    pub now: Option<u64>,
}

impl Settings {
    /// The real endpoints unless `NETSMITH_NEXAR_URL` and `NETSMITH_NEXAR_TOKEN_URL` say
    /// otherwise, a pause of 500 ms, three retries, and the cache in `search/nexar/` of netsmith's
    /// cache directory for 24 hours.
    pub fn live() -> Settings {
        let var = |name: &str, default: &str| {
            std::env::var(name)
                .ok()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| default.to_string())
        };
        Settings {
            graphql: var(URL_VAR, GRAPHQL_URL),
            token: var(TOKEN_URL_VAR, TOKEN_URL),
            delay: Duration::from_millis(500),
            retries: 3,
            backoff: Duration::from_secs(2),
            cache_dir: default_cache_dir(),
            ttl: Duration::from_secs(24 * 3600),
            offline: false,
            now: None,
        }
    }
}

/// `search/nexar/` in netsmith's cache directory.
pub fn default_cache_dir() -> Option<PathBuf> {
    Some(cache_base()?.join("search").join(NAME))
}

/// Refuse an address a credential or a token must not go to: one of JLCPCB's or under `/api`
/// (as `catalog refresh` refuses, ADR 0039), and one that is not `https`, unless it is this
/// machine's (a proxy of your own, or a test's server).
pub fn check(url: &str) -> Result<(), String> {
    check_endpoint(url)?;
    let Some((scheme, rest)) = url.split_once("://") else {
        return Err(format!("`{url}` is not an address like https://host/path"));
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = if let Some(v6) = host.strip_prefix('[') {
        v6.split(']').next().unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    }
    .to_ascii_lowercase();
    let local = matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1");
    match scheme.to_ascii_lowercase().as_str() {
        "https" => Ok(()),
        "http" if local => Ok(()),
        _ => Err(format!(
            "`{url}` is not an https address: a client secret and a token are sent only over https, or to this machine"
        )),
    }
}

/// Why one attempt failed.
enum Failure {
    /// May pass: try again after at least this long.
    Later(String, Duration),
    /// The token was refused: ask for a new one, once.
    Unauthorized,
    /// Will not pass.
    No(String),
}

/// The Nexar provider: the settings, the credentials, and the token of this run (kept in memory
/// only).
pub struct Nexar {
    settings: Settings,
    credentials: Credentials,
    agent: ureq::Agent,
    token: Option<String>,
    last_request: Option<Instant>,
    /// Requests that went out, the token's included.
    pub requests: u32,
}

impl Nexar {
    pub fn new(settings: Settings, credentials: Credentials) -> Nexar {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .user_agent(USER_AGENT)
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into();
        Nexar {
            settings,
            credentials,
            agent,
            token: None,
            last_request: None,
            requests: 0,
        }
    }

    fn now(&self) -> u64 {
        self.settings.now.unwrap_or_else(now_secs)
    }

    /// The answer to a GraphQL request: kept, or asked. `Ok((data, secs, cached))`.
    fn ask(&mut self, body: &Json, details: bool) -> Result<(Json, u64, bool), SupplyError> {
        let text = body.to_string();
        let key = format!("{:016x}", fnv1a(text.as_bytes()));
        let ttl = if self.settings.offline {
            u64::MAX
        } else {
            self.settings.ttl.as_secs()
        };
        if let Some((secs, kept)) = self.kept(&key)
            && self.now().saturating_sub(secs) <= ttl
            && let Ok(data) = data_of(&kept, details)
        {
            return Ok((data, secs, true));
        }
        if self.settings.offline {
            return Err(SupplyError::Offline);
        }
        let unavailable = |why: String| SupplyError::Unavailable(format!("{DESCRIBED}: {why}"));
        check(&self.settings.graphql).map_err(unavailable)?;
        check(&self.settings.token).map_err(unavailable)?;
        let mut renewed = false;
        let answer = loop {
            if self.token.is_none() {
                self.token = Some(self.fetch_token().map_err(unavailable)?);
            }
            match self.post_query(&text) {
                Ok(answer) => break answer,
                Err(Failure::Unauthorized) if !renewed => {
                    renewed = true;
                    self.token = None;
                }
                Err(Failure::Unauthorized) => {
                    return Err(unavailable(
                        "answered 401 with a new token: the application may not have the supply scope, or its plan has ended; see portal.nexar.com".to_string(),
                    ));
                }
                Err(Failure::No(why) | Failure::Later(why, _)) => return Err(unavailable(why)),
            }
        };
        let data = data_of(&answer, details).map_err(unavailable)?;
        let secs = self.now();
        self.keep(&key, secs, &answer);
        Ok((data, secs, false))
    }

    fn cache_file(&self, key: &str) -> Option<PathBuf> {
        Some(
            self.settings
                .cache_dir
                .as_ref()?
                .join(format!("{key}.json")),
        )
    }

    /// A kept answer: the first line is the Unix time it was got.
    fn kept(&self, key: &str) -> Option<(u64, String)> {
        let text = fs::read_to_string(self.cache_file(key)?).ok()?;
        let (first, body) = text.split_once('\n')?;
        Some((first.trim().parse().ok()?, body.to_string()))
    }

    /// Keep the answer of a query. It holds what Nexar said of the parts and nothing of the
    /// credentials or the token.
    fn keep(&self, key: &str, secs: u64, body: &str) {
        let Some(path) = self.cache_file(key) else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(path, format!("{secs}\n{body}"));
    }

    fn pause(&self) {
        if let Some(last) = self.last_request {
            let since = last.elapsed();
            if since < self.settings.delay {
                thread::sleep(self.settings.delay - since);
            }
        }
    }

    /// One request with the pause before it and the retries after a failure that may pass.
    fn with_retries(
        &mut self,
        mut send: impl FnMut(&Nexar) -> Result<String, Failure>,
    ) -> Result<String, Failure> {
        let mut wait = self.settings.backoff;
        for attempt in 0..=self.settings.retries {
            self.pause();
            let outcome = send(self);
            self.last_request = Some(Instant::now());
            self.requests += 1;
            match outcome {
                Err(Failure::Later(why, asked)) => {
                    if attempt == self.settings.retries {
                        return Err(Failure::No(format!(
                            "{why}; gave up after {} tries",
                            attempt + 1
                        )));
                    }
                    thread::sleep(wait.max(asked.min(Duration::from_secs(60))));
                    wait *= 2;
                }
                other => return other,
            }
        }
        unreachable!("the last attempt returns")
    }

    /// A token for this run, from the identity service. The secret goes in the form and nowhere
    /// else; neither it nor the token is in any message.
    fn fetch_token(&mut self) -> Result<String, String> {
        let url = self.settings.token.clone();
        let (id, secret) = (
            self.credentials.client_id.clone(),
            self.credentials.client_secret.clone(),
        );
        let body = self
            .with_retries(|me| {
                let response = me
                    .agent
                    .post(&url)
                    .header("Accept", "application/json")
                    .send_form([
                        ("grant_type", "client_credentials"),
                        ("client_id", id.as_str()),
                        ("client_secret", secret.as_str()),
                        ("scope", SCOPE),
                    ]);
                read(response, "the identity service")
            })
            .map_err(|f| match f {
                Failure::Unauthorized => "the identity service refused the client id and secret (answered 401): check the application at portal.nexar.com and the variables [supply.nexar] names".to_string(),
                Failure::No(why) | Failure::Later(why, _) => why,
            })?;
        let json: Json = serde_json::from_str(&body)
            .map_err(|_| "the identity service answered something that is not JSON".to_string())?;
        json.get("access_token")
            .and_then(Json::as_str)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                "the identity service's answer has no `access_token` of the kind it had when netsmith was written".to_string()
            })
    }

    fn post_query(&mut self, text: &str) -> Result<String, Failure> {
        let url = self.settings.graphql.clone();
        let token = self.token.clone().unwrap_or_default();
        let text = text.to_string();
        self.with_retries(|me| {
            let response = me
                .agent
                .post(&url)
                .header("Authorization", &format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .header("Accept", "application/json")
                .send(text.as_str());
            read(response, "the GraphQL endpoint")
        })
    }
}

/// Read an answer: its body on 200, and what a status means otherwise.
fn read(
    response: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    what: &str,
) -> Result<String, Failure> {
    let mut response =
        response.map_err(|e| Failure::Later(format!("{what}: no answer ({e})"), Duration::ZERO))?;
    let status = response.status().as_u16();
    let asked = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(Duration::ZERO, Duration::from_secs);
    match status {
        200 => {
            let body = response.body_mut().read_to_string().map_err(|e| {
                Failure::Later(format!("{what}: the answer broke off ({e})"), asked)
            })?;
            if body.trim_start().starts_with('<') {
                return Err(Failure::No(format!("{what} answered a web page, not JSON")));
            }
            Ok(body)
        }
        401 => Err(Failure::Unauthorized),
        400 if what == "the identity service" => Err(Failure::No(format!(
            "{what} refused the client id and secret (answered 400): check the application at portal.nexar.com and the variables [supply.nexar] names"
        ))),
        429 | 500..=599 => Err(Failure::Later(format!("{what} answered {status}"), asked)),
        _ => Err(Failure::No(format!(
            "{what} answered {status}: it refuses this client, or the address has moved"
        ))),
    }
}

/// 64-bit FNV-1a: the name of a kept answer, from the request's text (which holds no credential).
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The `data` of a GraphQL answer; an answer with `errors` is not one (it may be a field the
/// plan does not include, asked with `--details`).
fn data_of(body: &str, details: bool) -> Result<Json, String> {
    let json: Json = serde_json::from_str(body).map_err(|_| {
        format!(
            "the answer is not JSON; it starts {:?}",
            body.trim().chars().take(60).collect::<String>()
        )
    })?;
    if let Some(errors) = json.get("errors").and_then(Json::as_array)
        && !errors.is_empty()
    {
        let messages: Vec<&str> = errors
            .iter()
            .filter_map(|e| e.get("message").and_then(Json::as_str))
            .take(3)
            .collect();
        let hint = if details {
            "; --details asks for the values, the package and the datasheet, which need a plan of the PRO tier with TECH_SPECS and DATASHEETS: try without it"
        } else {
            ""
        };
        return Err(format!(
            "answered with errors: {}{hint}",
            if messages.is_empty() {
                "(no message)".to_string()
            } else {
                messages.join("; ")
            }
        ));
    }
    json.get("data")
        .filter(|d| d.is_object())
        .cloned()
        .ok_or_else(|| "the answer has no `data`".to_string())
}

/// The fields asked of each part.
fn selection(details: bool) -> String {
    let mut s = String::from(
        "mpn manufacturer { name } shortDescription octopartUrl \
         sellers(includeBrokers: false) { company { id name } isAuthorized \
         offers { sku inventoryLevel packaging moq updated prices { quantity price currency } } }",
    );
    if details {
        s.push_str(" specs { attribute { name shortname } displayValue } bestDatasheet { url }");
    }
    s
}

/// The request of a search: `supSearch` for keywords, `supSearchMpn` for a manufacturer's number.
pub fn search_request(query: &Query) -> Json {
    let (operation, q) = match (&query.mpn, &query.keywords) {
        (Some(mpn), _) => ("supSearchMpn", Some(mpn.clone())),
        (None, keywords) => ("supSearch", keywords.clone()),
    };
    let mut filters = serde_json::Map::new();
    for (name, value) in &query.filters {
        let entry = filters
            .entry(name.clone())
            .or_insert_with(|| Json::Array(Vec::new()));
        if let Json::Array(values) = entry {
            values.push(Json::String(value.clone()));
        }
    }
    let document = format!(
        "query Search($q: String, $limit: Int, $filters: Map, $inStockOnly: Boolean) {{ \
         {operation}(q: $q, limit: $limit, filters: $filters, inStockOnly: $inStockOnly) {{ \
         hits results {{ part {{ {} }} }} }} }}",
        selection(query.details)
    );
    json!({
        "query": document,
        "variables": {
            "q": q,
            "limit": query.limit.clamp(1, MAX_LIMIT),
            "filters": if filters.is_empty() { Json::Null } else { Json::Object(filters) },
            "inStockOnly": if query.in_stock { Json::Bool(true) } else { Json::Null },
        },
    })
}

/// The request of the record of `number` at `supplier`: `supMultiMatch` by `mpnOrSku`, the offers
/// of that seller first.
pub fn record_request(supplier: &str, number: &str, details: bool) -> Json {
    let document = format!(
        "query Record($queries: [SupPartMatchQuery!]!, $options: SupPartMatchOptions) {{ \
         supMultiMatch(queries: $queries, options: $options) {{ hits error parts {{ {} }} }} }}",
        selection(details)
    );
    let options = match seller_id(supplier) {
        Some(id) => json!({ "filters": { "distributor_id": [id] } }),
        None => Json::Null,
    };
    json!({
        "query": document,
        "variables": {
            "queries": [{ "mpnOrSku": number, "limit": 5 }],
            "options": options,
        },
    })
}

fn shape(field: &str) -> String {
    format!("the answer has no `{field}` of the kind it had when netsmith was written")
}

fn text(json: &Json, field: &str) -> Option<String> {
    json.get(field)
        .and_then(Json::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// A part of an answer.
pub fn parse_part(part: &Json) -> Result<FoundPart, String> {
    let mpn = text(part, "mpn").ok_or_else(|| shape("part.mpn"))?;
    let manufacturer = part
        .get("manufacturer")
        .and_then(|m| text(m, "name"))
        .ok_or_else(|| shape("part.manufacturer.name"))?;
    let mut values = Vec::new();
    let mut package = None;
    for spec in part
        .get("specs")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        let attribute = spec.get("attribute");
        let (Some(name), Some(shortname), Some(value)) = (
            attribute.and_then(|a| text(a, "name")),
            attribute.and_then(|a| text(a, "shortname")),
            text(spec, "displayValue"),
        ) else {
            continue;
        };
        if shortname == "case_package" {
            package = Some(value.clone());
        }
        values.push(Value {
            name,
            shortname,
            value,
        });
    }
    let mut offers = Vec::new();
    let mut other_sellers = 0;
    for seller in part
        .get("sellers")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        let company = seller.get("company");
        let id = company.and_then(|c| match c.get("id") {
            Some(Json::String(s)) => Some(s.clone()),
            Some(Json::Number(n)) => Some(n.to_string()),
            _ => None,
        });
        let Some(supplier) = id
            .as_deref()
            .and_then(|id| SELLERS.iter().find(|(i, _)| *i == id))
            .map(|(_, s)| *s)
        else {
            other_sellers += 1;
            continue;
        };
        let name = company
            .and_then(|c| text(c, "name"))
            .unwrap_or_else(|| supplier.to_string());
        let authorized = seller
            .get("isAuthorized")
            .and_then(Json::as_bool)
            .unwrap_or(false);
        for offer in seller
            .get("offers")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
        {
            let Some(sku) = text(offer, "sku") else {
                continue;
            };
            let stock = match offer.get("inventoryLevel").and_then(Json::as_i64) {
                Some(n) if n >= 0 => Stock::Count(n as u64),
                Some(-1) => Stock::NotStocked,
                Some(-2) => Stock::InStock,
                Some(-4) => Stock::OnRequest,
                _ => Stock::Unknown,
            };
            let mut breaks: Vec<PriceBreak> = Vec::new();
            let mut currency = None;
            for price in offer
                .get("prices")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
            {
                let (Some(from), Some(p)) = (
                    price.get("quantity").and_then(Json::as_u64),
                    price.get("price").and_then(Json::as_f64),
                ) else {
                    continue;
                };
                let Ok(from) = u32::try_from(from) else {
                    continue;
                };
                if !p.is_finite() || p < 0.0 {
                    continue;
                }
                let c = text(price, "currency");
                if currency.is_none() {
                    currency = c.clone();
                } else if c.is_some() && c != currency {
                    // Prices in two currencies are not one ladder: keep the first currency's.
                    continue;
                }
                breaks.push(PriceBreak { from, price: p });
            }
            breaks.sort_by_key(|b| b.from);
            breaks.dedup_by_key(|b| b.from);
            offers.push(SupplierOffer {
                supplier: supplier.to_string(),
                seller: name.clone(),
                authorized,
                sku,
                stock,
                packaging: text(offer, "packaging"),
                moq: offer.get("moq").and_then(Json::as_u64),
                price_breaks: breaks,
                currency,
                updated: text(offer, "updated"),
            });
        }
    }
    offers.sort_by_key(|o| super::SUPPLIERS.iter().position(|s| *s == o.supplier));
    Ok(FoundPart {
        manufacturer,
        mpn,
        description: text(part, "shortDescription"),
        package,
        values,
        datasheet: part.get("bestDatasheet").and_then(|d| text(d, "url")),
        url: text(part, "octopartUrl"),
        offers,
        other_sellers,
    })
}

/// The parts and the hits of a search answer's `data`.
pub fn parse_search(data: &Json) -> Result<(Option<u64>, Vec<FoundPart>), String> {
    let set = ["supSearch", "supSearchMpn"]
        .iter()
        .find_map(|op| data.get(*op))
        .filter(|s| s.is_object())
        .ok_or_else(|| shape("supSearch"))?;
    let hits = set.get("hits").and_then(Json::as_u64);
    let mut parts = Vec::new();
    for result in set
        .get("results")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        let part = result.get("part").ok_or_else(|| shape("results.part"))?;
        parts.push(parse_part(part)?);
    }
    Ok((hits, parts))
}

/// The parts and the hits of a record answer's `data`.
pub fn parse_record(data: &Json) -> Result<(Option<u64>, Vec<FoundPart>), String> {
    let first = data
        .get("supMultiMatch")
        .and_then(Json::as_array)
        .and_then(|m| m.first())
        .ok_or_else(|| shape("supMultiMatch"))?;
    if let Some(error) = text(first, "error") {
        return Err(format!("the match answered the error {error:?}"));
    }
    let hits = first.get("hits").and_then(Json::as_u64);
    let parts = first
        .get("parts")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .map(parse_part)
        .collect::<Result<Vec<_>, _>>()?;
    Ok((hits, parts))
}

impl SearchProvider for Nexar {
    fn name(&self) -> &str {
        NAME
    }

    fn describe(&self) -> &str {
        DESCRIBED
    }

    fn search(&mut self, query: &Query) -> Result<Answer, SupplyError> {
        let before = self.requests;
        let (data, read, cached) = self.ask(&search_request(query), query.details)?;
        let (hits, parts) = parse_search(&data)
            .map_err(|why| SupplyError::Unavailable(format!("{DESCRIBED}: {why}")))?;
        Ok(Answer {
            provider: NAME.to_string(),
            described: DESCRIBED.to_string(),
            endpoint: self.settings.graphql.clone(),
            hits,
            parts,
            read,
            cached,
            details: query.details,
            requests: self.requests - before,
        })
    }

    fn record(
        &mut self,
        supplier: &str,
        number: &str,
        details: bool,
    ) -> Result<Answer, SupplyError> {
        let before = self.requests;
        let (data, read, cached) = self.ask(&record_request(supplier, number, details), details)?;
        let (hits, parts) = parse_record(&data)
            .map_err(|why| SupplyError::Unavailable(format!("{DESCRIBED}: {why}")))?;
        Ok(Answer {
            provider: NAME.to_string(),
            described: DESCRIBED.to_string(),
            endpoint: self.settings.graphql.clone(),
            hits,
            parts,
            read,
            cached,
            details,
            requests: self.requests - before,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_goes_only_over_https_or_to_this_machine_and_never_to_jlcpcb() {
        for ok in [
            "https://api.nexar.com/graphql",
            "https://identity.nexar.com/connect/token",
            "http://127.0.0.1:8080/graphql",
            "http://localhost/connect/token",
            "http://[::1]:9/graphql",
        ] {
            assert!(check(ok).is_ok(), "{ok}");
        }
        for (bad, says) in [
            ("http://api.nexar.com/graphql", "not an https address"),
            (
                "http://127.0.0.2.example.com/graphql",
                "not an https address",
            ),
            ("ftp://127.0.0.1/x", "not an https address"),
            ("https://jlcpcb.com/graphql", "does not fetch JLCPCB data"),
            ("https://example.com/api/graphql", "is under /api"),
            ("api.nexar.com/graphql", "not an address"),
        ] {
            let e = check(bad).unwrap_err();
            assert!(e.contains(says), "{bad}: {e}");
        }
    }

    #[test]
    fn the_credentials_are_never_shown() {
        let c = Credentials {
            client_id: "the-id".into(),
            client_secret: "the-secret".into(),
        };
        let shown = format!("{c:?}");
        assert!(
            !shown.contains("the-id") && !shown.contains("the-secret"),
            "{shown}"
        );
    }

    #[test]
    fn a_part_keeps_the_offers_of_the_suppliers_netsmith_reads_and_says_what_a_stock_code_means() {
        let part = json!({
            "mpn": "TESTCAP-100N",
            "manufacturer": {"name": "Acme Passives"},
            "shortDescription": "CAP CER 100NF 16V 0402 (test)",
            "octopartUrl": "https://octopart.com/testcap",
            "specs": [
                {"attribute": {"name": "Case/Package", "shortname": "case_package"}, "displayValue": "0402"},
                {"attribute": {"name": "Capacitance", "shortname": "capacitance"}, "displayValue": "100nF"}
            ],
            "bestDatasheet": {"url": "https://example.com/testcap.pdf"},
            "sellers": [
                {"company": {"id": "459", "name": "DigiKey"}, "isAuthorized": true, "offers": [
                    {"sku": "TEST-DK-1", "inventoryLevel": 1200, "packaging": "Cut Tape", "moq": 1,
                     "updated": "2026-11-19T10:00:00Z",
                     "prices": [{"quantity": 10, "price": 0.05, "currency": "USD"},
                                {"quantity": 1, "price": 0.1, "currency": "USD"},
                                {"quantity": 100, "price": 0.04, "currency": "EUR"}]},
                    {"sku": "TEST-DK-2", "inventoryLevel": -2, "prices": []}
                ]},
                {"company": {"id": "11779", "name": "LCSC"}, "isAuthorized": false, "offers": [
                    {"sku": "C9900101", "inventoryLevel": -1, "prices": []}
                ]},
                {"company": {"id": "1", "name": "Elsewhere"}, "isAuthorized": true, "offers": [
                    {"sku": "E-1", "inventoryLevel": 5, "prices": []}
                ]}
            ]
        });
        let found = parse_part(&part).unwrap();
        assert_eq!(found.manufacturer, "Acme Passives");
        assert_eq!(found.package.as_deref(), Some("0402"));
        assert_eq!(found.values.len(), 2);
        assert_eq!(
            found.datasheet.as_deref(),
            Some("https://example.com/testcap.pdf")
        );
        assert_eq!(found.other_sellers, 1);
        let suppliers: Vec<&str> = found.offers.iter().map(|o| o.supplier.as_str()).collect();
        assert_eq!(suppliers, ["lcsc", "digikey", "digikey"]);
        assert_eq!(found.offers[0].stock, Stock::NotStocked);
        let dk = &found.offers[1];
        assert_eq!(dk.stock, Stock::Count(1200));
        // Sorted, and the break in another currency left out: no currency is converted.
        assert_eq!(
            dk.price_breaks,
            [
                PriceBreak {
                    from: 1,
                    price: 0.1
                },
                PriceBreak {
                    from: 10,
                    price: 0.05
                }
            ]
        );
        assert_eq!(found.offers[2].stock, Stock::InStock);
        assert_eq!(found.offers[2].stock.count(), None);
        // A part with no manufacturer is an answer of another shape, not a part.
        let e = parse_part(&json!({"mpn": "X"})).unwrap_err();
        assert!(e.contains("manufacturer.name"), "{e}");
    }

    #[test]
    fn a_request_holds_the_query_and_never_a_credential() {
        let query = Query {
            keywords: Some("100nF 0402".into()),
            filters: vec![
                ("case_package".into(), "0402".into()),
                ("case_package".into(), "0603".into()),
            ],
            limit: 500,
            ..Query::default()
        };
        let request = search_request(&query);
        let document = request["query"].as_str().unwrap();
        assert!(document.contains("supSearch(q: $q"), "{document}");
        assert!(!document.contains("specs"), "{document}");
        assert_eq!(request["variables"]["limit"], 100);
        assert_eq!(
            request["variables"]["filters"],
            json!({"case_package": ["0402", "0603"]})
        );
        let by_mpn = search_request(&Query {
            mpn: Some("LM358".into()),
            details: true,
            limit: 5,
            ..Query::default()
        });
        assert!(by_mpn["query"].as_str().unwrap().contains("supSearchMpn("));
        assert!(by_mpn["query"].as_str().unwrap().contains("bestDatasheet"));
        let record = record_request("digikey", "TEST-DK-1", false);
        assert_eq!(
            record["variables"]["options"],
            json!({"filters": {"distributor_id": ["459"]}})
        );
        assert_eq!(record["variables"]["queries"][0]["mpnOrSku"], "TEST-DK-1");
    }

    #[test]
    fn an_answer_with_errors_is_not_an_answer_and_says_why() {
        let e = data_of(
            r#"{"data": null, "errors": [{"message": "plan does not include specs"}]}"#,
            true,
        )
        .unwrap_err();
        assert!(
            e.contains("plan does not include specs") && e.contains("try without it"),
            "{e}"
        );
        assert!(data_of("<html>", false).unwrap_err().contains("not JSON"));
        assert!(data_of(r#"{"data": {"supSearch": {}}}"#, false).is_ok());
    }
}
