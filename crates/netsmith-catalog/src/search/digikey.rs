//! Digi-Key's own Product Information API (v4) as a [`SearchProvider`] (issue #1477).
//!
//! What this reads, and where it was read (2026-11-21; nothing here is from memory):
//! - The token: the two-legged OAuth flow, `POST https://api.digikey.com/v1/oauth2/token` with a
//!   form (`application/x-www-form-urlencoded`) of `client_id`, `client_secret` and `grant_type =
//!   client_credentials`, answered with `access_token`, `expires_in` (in seconds) and `token_type`
//!   (`Bearer`); a two-legged token lasts 10 minutes and has no refresh token, so a new one is
//!   asked for on a 401. Requests carry `Authorization: Bearer <token>` and `X-DIGIKEY-Client-Id`
//!   (Digi-Key's developer portal, _Documentation_, OAuth and shared concepts:
//!   <https://developer.digikey.com/documentation>).
//! - The rate limits of Product Information: 120 requests a minute and 1000 a day; a 429 says
//!   `Retry-After` in seconds, with a body whose `ErrorMessage` names the limit (same page).
//! - The endpoints, of the ProductSearch API's OpenAPI file (Swagger 2.0, host `api.digikey.com`,
//!   base path `/products/v4`; <https://developer.digikey.com/node/2357/oas-download>, linked from
//!   <https://developer.digikey.com/products/product-information-v4/productsearch/keywordsearch>
//!   and `.../productdetails`):
//!   - `POST /search/keyword` with a JSON body `KeywordRequest`: `Keywords` (up to 250
//!     characters), `Limit` (1 to 50), `Offset` and `FilterOptionsRequest`, whose `SearchOptions`
//!     list takes `InStock`. Answered with `KeywordResponse`: `Products`, `ProductsCount` and
//!     `ExactMatches` (both lists of `Product`) and `SearchLocaleUsed` (`Site`, `Language`,
//!     `Currency`).
//!   - `GET /search/{productNumber}/productdetails` for one part by a Digi-Key or a manufacturer's
//!     number, answered with `ProductDetails`: `Product` and `SearchLocaleUsed`. Its header
//!     `X-DIGIKEY-Account-Id` is, the file says, required with two-legged OAuth, so the record is
//!     asked this way only when the project names the variable of its account id
//!     (`account_id_env`); otherwise the record is a keyword search for the number, of which only
//!     the parts whose Digi-Key number or manufacturer's number is exactly it are taken
//!     ([`super::add::pick`]).
//!   - A `Product`: `ManufacturerProductNumber`, `Manufacturer { Name }`, `Description {
//!     ProductDescription, DetailedDescription }`, `ProductUrl`, `DatasheetUrl`, `Parameters` (each
//!     `ParameterId`, `ParameterText`, `ValueText`) and `ProductVariations`, each with
//!     `DigiKeyProductNumber`, `PackageType { Name }` (cut tape, reel), `StandardPricing` (each
//!     `BreakQuantity`, `UnitPrice`), `QuantityAvailableforPackageType` and
//!     `MinimumOrderQuantity`. Errors are `DKProblemDetails` (`title`, `detail`, `status`).
//!
//! Each product variation is one Digi-Key offer (a supplier with two packagings, which `catalog
//! add --from` lists and does not write unless its sku is named). The parameters are the found
//! part's values, by `ParameterText` and with the `ParameterId` as the short name. The file names
//! no parameter as the package, so a found part's `package` stays empty and `catalog add --from`
//! lists it to fill. Digi-Key says nothing of whether it is authorized, so `authorized` is false;
//! no other seller is in its answers. Parametric filters need Digi-Key's parameter and value ids,
//! which this does not pass on: `--filter` is refused. No locale header is sent, so Digi-Key's
//! defaults apply (site US, its currency), and the currency of the prices is the answer's
//! `SearchLocaleUsed.Currency`.
//!
//! Digi-Key's API terms of use (the API User Agreement, <https://developer.digikey.com/api-user-agreement>,
//! and the terms the OpenAPI file names, <https://www.digikey.com/en/terms-and-conditions>) have
//! not been read for netsmith (issue #10): a person must read them before the feature is
//! advertised.

use std::{path::PathBuf, time::Duration};

use serde_json::{Value as Json, json};

use super::http::{Cache, Client, Credentials, Failure, TOKEN_ENDPOINT, access_token, check, read};
use super::{Answer, FoundPart, PriceBreak, Query, SearchProvider, Stock, SupplierOffer, Value};
use crate::refresh::{cache_base, now_secs};
use crate::supply::SupplyError;

/// The name of the provider, as the manifest's table names it (`[supply.digikey]`), and the
/// supplier whose offers it has.
pub const NAME: &str = "digikey";

/// What messages call it.
pub const DESCRIBED: &str = "Digi-Key";

/// The base of the Product Information v4 endpoints.
pub const BASE_URL: &str = "https://api.digikey.com/products/v4";

/// The token endpoint of the two-legged flow.
pub const TOKEN_URL: &str = "https://api.digikey.com/v1/oauth2/token";

/// The environment variable that names another base, for a proxy of your own and for the tests,
/// which never reach the real one.
pub const URL_VAR: &str = "NETSMITH_DIGIKEY_URL";

/// The environment variable that names another token endpoint.
pub const TOKEN_URL_VAR: &str = "NETSMITH_DIGIKEY_TOKEN_URL";

/// The most parts one search asks for (the `Limit` of a keyword search).
pub const MAX_LIMIT: u32 = 50;

/// What a 400 of the token endpoint means.
const REFUSED: &str =
    "check the application at developer.digikey.com and the variables [supply.digikey] names";

/// The account id of `X-DIGIKEY-Account-Id`, read from the environment; never shown.
#[derive(Clone)]
pub struct AccountId(pub String);

impl std::fmt::Debug for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AccountId(..)")
    }
}

/// Where the provider asks, how politely, and what it keeps.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The base of the endpoints (`https://api.digikey.com/products/v4`).
    pub base: String,
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
    /// The real endpoints unless `NETSMITH_DIGIKEY_URL` and `NETSMITH_DIGIKEY_TOKEN_URL` say
    /// otherwise, a pause of 500 ms (under 120 a minute), three retries, and the cache in
    /// `search/digikey/` of netsmith's cache directory for 24 hours.
    pub fn live() -> Settings {
        let var = |name: &str, default: &str| {
            std::env::var(name)
                .ok()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| default.to_string())
        };
        Settings {
            base: var(URL_VAR, BASE_URL).trim_end_matches('/').to_string(),
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

/// `search/digikey/` in netsmith's cache directory.
pub fn default_cache_dir() -> Option<PathBuf> {
    Some(cache_base()?.join("search").join(NAME))
}

/// One request: a keyword search, or the details of one product.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Keyword(Json),
    Details(String),
}

impl Request {
    /// The text a kept answer is named by: the method, the path and the body; never a
    /// credential, and never the account id.
    fn text(&self) -> String {
        match self {
            Request::Keyword(body) => format!("POST /search/keyword {body}"),
            Request::Details(number) => format!("GET /search/{number}/productdetails"),
        }
    }
}

/// The body of a keyword search. A filter is refused: Digi-Key's parametric filters are by its
/// parameter and value ids.
pub fn keyword_request(query: &Query) -> Result<Json, String> {
    if !query.filters.is_empty() {
        return Err(
            "Digi-Key's parametric filters are by its own parameter and value ids, which netsmith does not pass on: put the values in the words, or use --provider nexar for --filter".to_string(),
        );
    }
    let words = query
        .mpn
        .as_deref()
        .or(query.keywords.as_deref())
        .unwrap_or("")
        .trim();
    if words.is_empty() {
        return Err("Digi-Key searches by words or a number: give one".to_string());
    }
    if words.chars().count() > 250 {
        return Err(
            "Digi-Key takes keywords of up to 250 characters: give fewer words".to_string(),
        );
    }
    let mut body = json!({
        "Keywords": words,
        "Limit": query.limit.clamp(1, MAX_LIMIT),
        "Offset": 0,
    });
    if query.in_stock {
        body["FilterOptionsRequest"] = json!({ "SearchOptions": ["InStock"] });
    }
    Ok(body)
}

/// A path segment: the number with every byte but letters, digits, `-`, `.`, `_` and `~` escaped.
fn segment(number: &str) -> String {
    let mut out = String::new();
    for b in number.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
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

/// A `Product` of an answer, with the currency the answer's prices are in.
pub fn parse_product(product: &Json, currency: Option<&str>) -> Result<FoundPart, String> {
    let mpn = text(product, "ManufacturerProductNumber")
        .ok_or_else(|| shape("Product.ManufacturerProductNumber"))?;
    let manufacturer = product
        .get("Manufacturer")
        .and_then(|m| text(m, "Name"))
        .ok_or_else(|| shape("Product.Manufacturer.Name"))?;
    let description = product
        .get("Description")
        .and_then(|d| text(d, "ProductDescription").or_else(|| text(d, "DetailedDescription")));
    let mut values = Vec::new();
    for parameter in product
        .get("Parameters")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        let (Some(name), Some(value)) = (
            text(parameter, "ParameterText"),
            text(parameter, "ValueText"),
        ) else {
            continue;
        };
        let shortname = match parameter.get("ParameterId") {
            Some(Json::Number(n)) => n.to_string(),
            Some(Json::String(s)) => s.clone(),
            _ => String::new(),
        };
        values.push(Value {
            name,
            shortname,
            value,
        });
    }
    let mut offers = Vec::new();
    for variation in product
        .get("ProductVariations")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        let Some(sku) = text(variation, "DigiKeyProductNumber") else {
            continue;
        };
        let stock = match variation
            .get("QuantityAvailableforPackageType")
            .and_then(Json::as_u64)
        {
            Some(n) => Stock::Count(n),
            None => Stock::Unknown,
        };
        let mut breaks: Vec<PriceBreak> = variation
            .get("StandardPricing")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(|b| {
                let from = u32::try_from(b.get("BreakQuantity")?.as_u64()?).ok()?;
                let price = b.get("UnitPrice")?.as_f64()?;
                (price.is_finite() && price >= 0.0).then_some(PriceBreak { from, price })
            })
            .collect();
        breaks.sort_by_key(|b| b.from);
        breaks.dedup_by_key(|b| b.from);
        offers.push(SupplierOffer {
            supplier: NAME.to_string(),
            seller: DESCRIBED.to_string(),
            authorized: false,
            sku,
            stock,
            packaging: variation.get("PackageType").and_then(|p| text(p, "Name")),
            moq: variation.get("MinimumOrderQuantity").and_then(Json::as_u64),
            currency: (!breaks.is_empty())
                .then(|| currency.map(str::to_string))
                .flatten(),
            price_breaks: breaks,
            updated: None,
        });
    }
    Ok(FoundPart {
        manufacturer,
        mpn,
        description,
        package: None,
        values,
        datasheet: text(product, "DatasheetUrl"),
        url: text(product, "ProductUrl"),
        offers,
        other_sellers: 0,
    })
}

fn currency_of(answer: &Json) -> Option<String> {
    answer
        .get("SearchLocaleUsed")
        .and_then(|l| text(l, "Currency"))
}

/// The parts and the count of a keyword search's answer: the exact matches first, then the other
/// products, each part once.
pub fn parse_keyword(answer: &Json) -> Result<(Option<u64>, Vec<FoundPart>), String> {
    let products = answer
        .get("Products")
        .and_then(Json::as_array)
        .ok_or_else(|| shape("Products"))?;
    let currency = currency_of(answer);
    let exact = answer
        .get("ExactMatches")
        .and_then(Json::as_array)
        .into_iter()
        .flatten();
    let mut parts: Vec<FoundPart> = Vec::new();
    for product in exact.chain(products) {
        let part = parse_product(product, currency.as_deref())?;
        if !parts
            .iter()
            .any(|p| p.mpn == part.mpn && p.manufacturer == part.manufacturer)
        {
            parts.push(part);
        }
    }
    Ok((answer.get("ProductsCount").and_then(Json::as_u64), parts))
}

/// The part of a product details answer.
pub fn parse_details(answer: &Json) -> Result<(Option<u64>, Vec<FoundPart>), String> {
    let product = answer
        .get("Product")
        .filter(|p| p.is_object())
        .ok_or_else(|| shape("Product"))?;
    let part = parse_product(product, currency_of(answer).as_deref())?;
    Ok((Some(1), vec![part]))
}

/// The Digi-Key provider: the settings, the credentials, the account id when the project names
/// one, and the token of this run (kept in memory only).
pub struct DigiKey {
    settings: Settings,
    credentials: Credentials,
    account: Option<AccountId>,
    client: Client,
    cache: Cache,
    token: Option<String>,
}

impl DigiKey {
    pub fn new(
        settings: Settings,
        credentials: Credentials,
        account: Option<AccountId>,
    ) -> DigiKey {
        let client = Client::new(settings.delay, settings.retries, settings.backoff);
        let cache = Cache {
            dir: settings.cache_dir.clone(),
        };
        DigiKey {
            settings,
            credentials,
            account,
            client,
            cache,
            token: None,
        }
    }

    /// Requests that went out, the tokens' included.
    pub fn requests(&self) -> u32 {
        self.client.requests
    }

    fn now(&self) -> u64 {
        self.settings.now.unwrap_or_else(now_secs)
    }

    /// The answer to a request: kept, or asked. `Ok((json, secs, cached))`.
    fn ask(&mut self, request: &Request) -> Result<(Json, u64, bool), SupplyError> {
        let key = Cache::key(&request.text());
        let ttl = if self.settings.offline {
            u64::MAX
        } else {
            self.settings.ttl.as_secs()
        };
        let parse = |body: &str| {
            serde_json::from_str::<Json>(body)
                .ok()
                .filter(Json::is_object)
        };
        if let Some((secs, kept)) = self.cache.kept(&key)
            && self.now().saturating_sub(secs) <= ttl
            && let Some(json) = parse(&kept)
        {
            return Ok((json, secs, true));
        }
        if self.settings.offline {
            return Err(SupplyError::Offline);
        }
        let unavailable = |why: String| SupplyError::Unavailable(format!("{DESCRIBED}: {why}"));
        check(&self.settings.base).map_err(unavailable)?;
        check(&self.settings.token).map_err(unavailable)?;
        let mut renewed = false;
        let answer = loop {
            if self.token.is_none() {
                self.token = Some(self.fetch_token().map_err(unavailable)?);
            }
            match self.send(request) {
                Ok(answer) => break answer,
                Err(Failure::Unauthorized) if !renewed => {
                    renewed = true;
                    self.token = None;
                }
                Err(Failure::Unauthorized) => {
                    return Err(unavailable(
                        "answered 401 with a new token: the application may not have the Product Information API, or the client id is not its own; see developer.digikey.com".to_string(),
                    ));
                }
                Err(Failure::No(why) | Failure::Later(why, _)) => return Err(unavailable(why)),
            }
        };
        let json = parse(&answer).ok_or_else(|| {
            unavailable(format!(
                "the answer is not a JSON object; it starts {:?}",
                answer.trim().chars().take(60).collect::<String>()
            ))
        })?;
        let secs = self.now();
        self.cache.keep(&key, secs, &answer);
        Ok((json, secs, false))
    }

    /// A token for this run. The secret goes in the form and nowhere else; neither it nor the
    /// token is in any message.
    fn fetch_token(&mut self) -> Result<String, String> {
        let url = self.settings.token.clone();
        let (id, secret) = (
            self.credentials.client_id.clone(),
            self.credentials.client_secret.clone(),
        );
        let body = self
            .client
            .with_retries(|agent| {
                let response = agent
                    .post(&url)
                    .header("Accept", "application/json")
                    .send_form([
                        ("client_id", id.as_str()),
                        ("client_secret", secret.as_str()),
                        ("grant_type", "client_credentials"),
                    ]);
                read(response, TOKEN_ENDPOINT, REFUSED)
            })
            .map_err(|f| match f {
                Failure::Unauthorized => format!(
                    "{TOKEN_ENDPOINT} refused the client id and secret (answered 401): {REFUSED}"
                ),
                Failure::No(why) | Failure::Later(why, _) => why,
            })?;
        access_token(&body)
    }

    fn send(&mut self, request: &Request) -> Result<String, Failure> {
        let token = self.token.clone().unwrap_or_default();
        let client_id = self.credentials.client_id.clone();
        let account = self.account.clone();
        let base = self.settings.base.clone();
        self.client.with_retries(|agent| match request {
            Request::Keyword(body) => {
                let response = agent
                    .post(format!("{base}/search/keyword"))
                    .header("Authorization", &format!("Bearer {token}"))
                    .header("X-DIGIKEY-Client-Id", &client_id)
                    .header("Content-Type", "application/json")
                    .header("Accept", "application/json")
                    .send(body.to_string());
                read(response, "the keyword search", REFUSED)
            }
            Request::Details(number) => {
                let mut call = agent
                    .get(format!("{base}/search/{}/productdetails", segment(number)))
                    .header("Authorization", &format!("Bearer {token}"))
                    .header("X-DIGIKEY-Client-Id", &client_id)
                    .header("Accept", "application/json");
                if let Some(AccountId(account)) = &account {
                    call = call.header("X-DIGIKEY-Account-Id", account);
                }
                read(call.call(), "the product details", REFUSED)
            }
        })
    }

    fn answer(
        &self,
        (hits, parts): (Option<u64>, Vec<FoundPart>),
        read: u64,
        cached: bool,
        requests: u32,
    ) -> Answer {
        Answer {
            provider: NAME.to_string(),
            described: DESCRIBED.to_string(),
            endpoint: self.settings.base.clone(),
            hits,
            parts,
            read,
            cached,
            // The parameters and the datasheet come with every answer.
            details: true,
            requests,
        }
    }
}

impl SearchProvider for DigiKey {
    fn name(&self) -> &str {
        NAME
    }

    fn describe(&self) -> &str {
        DESCRIBED
    }

    fn search(&mut self, query: &Query) -> Result<Answer, SupplyError> {
        let before = self.requests();
        let body = keyword_request(query)
            .map_err(|why| SupplyError::Unavailable(format!("{DESCRIBED}: {why}")))?;
        let (json, read, cached) = self.ask(&Request::Keyword(body))?;
        let found = parse_keyword(&json)
            .map_err(|why| SupplyError::Unavailable(format!("{DESCRIBED}: {why}")))?;
        Ok(self.answer(found, read, cached, self.requests() - before))
    }

    /// The product details of `number` when the project names its account id, else a keyword
    /// search for it. Only Digi-Key's own offers are in its answers.
    fn record(
        &mut self,
        supplier: &str,
        number: &str,
        _details: bool,
    ) -> Result<Answer, SupplyError> {
        if supplier != NAME {
            return Err(SupplyError::Unavailable(format!(
                "{DESCRIBED}'s API has only Digi-Key's own offers, not {supplier}'s: use --provider nexar for --from {supplier}"
            )));
        }
        let before = self.requests();
        let unavailable = |why: String| SupplyError::Unavailable(format!("{DESCRIBED}: {why}"));
        let found = if self.account.is_some() {
            let (json, read, cached) = self.ask(&Request::Details(number.trim().to_string()))?;
            (parse_details(&json).map_err(unavailable)?, read, cached)
        } else {
            let body = keyword_request(&Query {
                keywords: Some(number.trim().to_string()),
                limit: 10,
                ..Query::default()
            })
            .map_err(unavailable)?;
            let (json, read, cached) = self.ask(&Request::Keyword(body))?;
            (parse_keyword(&json).map_err(unavailable)?, read, cached)
        };
        let (parts, read, cached) = found;
        Ok(self.answer(parts, read, cached, self.requests() - before))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product() -> Json {
        json!({
            "ManufacturerProductNumber": "TESTCAP-100N",
            "Manufacturer": {"Id": 1, "Name": "Acme Passives"},
            "Description": {"ProductDescription": "CAP CER 100NF 16V 0402 (test)", "DetailedDescription": "longer"},
            "ProductUrl": "https://www.digikey.example/testcap",
            "DatasheetUrl": "https://example.com/testcap.pdf",
            "Parameters": [
                {"ParameterId": 2049, "ParameterText": "Capacitance", "ParameterType": "String", "ValueId": "1", "ValueText": "100 nF"},
                {"ParameterId": 7, "ParameterText": "No value"}
            ],
            "ProductVariations": [
                {"DigiKeyProductNumber": "TEST-DK-1", "PackageType": {"Id": 2, "Name": "Cut Tape (CT)"},
                 "StandardPricing": [{"BreakQuantity": 10, "UnitPrice": 0.05, "TotalPrice": 0.5},
                                     {"BreakQuantity": 1, "UnitPrice": 0.1, "TotalPrice": 0.1}],
                 "QuantityAvailableforPackageType": 1200, "MinimumOrderQuantity": 1},
                {"DigiKeyProductNumber": "TEST-DK-2", "PackageType": {"Id": 1, "Name": "Tape & Reel (TR)"},
                 "StandardPricing": []}
            ]
        })
    }

    #[test]
    fn a_product_is_a_part_with_an_offer_for_each_variation_and_no_guessed_package() {
        let part = parse_product(&product(), Some("USD")).unwrap();
        assert_eq!(
            (part.manufacturer.as_str(), part.mpn.as_str()),
            ("Acme Passives", "TESTCAP-100N")
        );
        assert_eq!(
            part.description.as_deref(),
            Some("CAP CER 100NF 16V 0402 (test)")
        );
        assert_eq!(part.package, None);
        assert_eq!(
            part.values,
            [Value {
                name: "Capacitance".into(),
                shortname: "2049".into(),
                value: "100 nF".into()
            }]
        );
        assert_eq!(part.offers.len(), 2);
        let ct = &part.offers[0];
        assert_eq!(
            (ct.supplier.as_str(), ct.sku.as_str(), ct.stock.clone()),
            ("digikey", "TEST-DK-1", Stock::Count(1200))
        );
        assert_eq!(ct.packaging.as_deref(), Some("Cut Tape (CT)"));
        assert_eq!(
            ct.price_breaks,
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
        assert_eq!(ct.currency.as_deref(), Some("USD"));
        // No quantity: unknown, not 0; no price: no currency.
        assert_eq!(part.offers[1].stock, Stock::Unknown);
        assert_eq!(part.offers[1].currency, None);
        let e = parse_product(&json!({"ManufacturerProductNumber": "X"}), None).unwrap_err();
        assert!(e.contains("Manufacturer.Name"), "{e}");
    }

    #[test]
    fn a_keyword_answer_lists_the_exact_matches_first_and_each_part_once() {
        let mut other = product();
        other["ManufacturerProductNumber"] = json!("TESTCAP-100N-B");
        let answer = json!({
            "Products": [other, product()],
            "ExactMatches": [product()],
            "ProductsCount": 41,
            "SearchLocaleUsed": {"Site": "US", "Language": "en", "Currency": "USD"}
        });
        let (hits, parts) = parse_keyword(&answer).unwrap();
        assert_eq!(hits, Some(41));
        let mpns: Vec<&str> = parts.iter().map(|p| p.mpn.as_str()).collect();
        assert_eq!(mpns, ["TESTCAP-100N", "TESTCAP-100N-B"]);
        assert!(parse_keyword(&json!({})).unwrap_err().contains("Products"));
        let (_, one) = parse_details(&json!({"Product": product()})).unwrap();
        assert_eq!(one[0].offers[0].currency, None);
    }

    #[test]
    fn a_keyword_request_holds_the_words_a_limit_digikey_allows_and_refuses_a_filter() {
        let body = keyword_request(&Query {
            keywords: Some("100nF 0402".into()),
            in_stock: true,
            limit: 100,
            ..Query::default()
        })
        .unwrap();
        assert_eq!(
            body,
            json!({"Keywords": "100nF 0402", "Limit": 50, "Offset": 0,
                   "FilterOptionsRequest": {"SearchOptions": ["InStock"]}})
        );
        let by_mpn = keyword_request(&Query {
            mpn: Some("LM358".into()),
            limit: 5,
            ..Query::default()
        })
        .unwrap();
        assert_eq!(by_mpn["Keywords"], "LM358");
        assert!(by_mpn.get("FilterOptionsRequest").is_none());
        let e = keyword_request(&Query {
            keywords: Some("x".into()),
            filters: vec![("case_package".into(), "0402".into())],
            limit: 5,
            ..Query::default()
        })
        .unwrap_err();
        assert!(e.contains("--provider nexar"), "{e}");
        assert_eq!(segment("TEST-DK 1/2"), "TEST-DK%201%2F2");
        assert!(
            !Request::Details("X".into()).text().contains("Bearer"),
            "a kept answer is named by the request alone"
        );
    }

    #[test]
    fn the_account_id_is_never_shown() {
        assert!(!format!("{:?}", AccountId("12345".into())).contains("12345"));
    }
}
