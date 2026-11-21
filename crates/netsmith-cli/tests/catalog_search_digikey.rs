//! `netsmith catalog search --provider digikey` and `catalog add --from digikey` (issue #1477)
//! against a fake Digi-Key on 127.0.0.1: the token endpoint and the Product Information v4
//! endpoints answer from the part below, which is made up (`TESTCAP-100N` of `Acme Passives`,
//! skus `TEST-DK-...`) and names no real part. No test reaches the network, and none reaches
//! Digi-Key.

use crate::common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::endpoints::{Fake, Reply, Seen};
use netsmith_testkit::TempDir;
use serde_json::{Value, json};

/// The values the fake Digi-Key wants; the test looks for the secret in every file afterwards.
const CLIENT_ID: &str = "test-dk-client-id";
const SECRET: &str = "Sekret-DigiKey-Value-For-The-Test-27182";
const ACCOUNT: &str = "8675309";
const TOKEN: &str = "test-dk-token-1";

const MANIFEST: &str = "[project]\nname = \"acme\"\n\n[supply.digikey]\nclient_id_env = \"TEST_DK_ID\"\nclient_secret_env = \"TEST_DK_SECRET\"\n";

const WITH_ACCOUNT: &str = "[project]\nname = \"acme\"\n\n[supply.digikey]\nclient_id_env = \"TEST_DK_ID\"\nclient_secret_env = \"TEST_DK_SECRET\"\naccount_id_env = \"TEST_DK_ACCOUNT\"\n";

const BOTH: &str = "[project]\nname = \"acme\"\n\n[supply.nexar]\nclient_id_env = \"TEST_NEXAR_ID\"\nclient_secret_env = \"TEST_NEXAR_SECRET\"\n\n[supply.digikey]\nclient_id_env = \"TEST_DK_ID\"\nclient_secret_env = \"TEST_DK_SECRET\"\n";

const CATALOG: &str = "manufacturer,mpn,name,label,capacitance,package,source,notes\n";

/// The made-up part, as Digi-Key's `Product` has it: two packagings.
fn product() -> Value {
    json!({
        "ManufacturerProductNumber": "TESTCAP-100N",
        "Manufacturer": {"Id": 1, "Name": "Acme Passives"},
        "Description": {"ProductDescription": "Test capacitor 100nF 16V 0402", "DetailedDescription": "Test capacitor, longer"},
        "ProductUrl": "https://www.digikey.example/testcap-100n",
        "DatasheetUrl": "https://example.com/testcap.pdf",
        "QuantityAvailable": 1200,
        "Parameters": [
            {"ParameterId": 2049, "ParameterText": "Capacitance", "ParameterType": "String", "ValueId": "1", "ValueText": "100 nF"}
        ],
        "ProductVariations": [
            {"DigiKeyProductNumber": "TEST-DK-1", "PackageType": {"Id": 2, "Name": "Cut Tape (CT)"},
             "StandardPricing": [{"BreakQuantity": 1, "UnitPrice": 0.1, "TotalPrice": 0.1},
                                 {"BreakQuantity": 100, "UnitPrice": 0.04, "TotalPrice": 4.0}],
             "QuantityAvailableforPackageType": 1200, "MinimumOrderQuantity": 1},
            {"DigiKeyProductNumber": "TEST-DK-2", "PackageType": {"Id": 1, "Name": "Tape & Reel (TR)"},
             "StandardPricing": [], "QuantityAvailableforPackageType": 0, "MinimumOrderQuantity": 10000}
        ]
    })
}

const LOCALE: fn() -> Value = || json!({"Site": "US", "Language": "en", "Currency": "USD"});

/// What the fake Digi-Key answers: a token for the right form, the part for a request with the
/// token and the client id.
fn digikey(request: &Seen) -> Reply {
    let authorized = request.authorization == format!("Bearer {TOKEN}")
        && request.header("X-DIGIKEY-Client-Id") == CLIENT_ID;
    match request.path() {
        "/v1/oauth2/token" => {
            let form = &request.body;
            let ok = request.method == "POST"
                && form.contains("grant_type=client_credentials")
                && form.contains(&format!("client_id={CLIENT_ID}"))
                && form.contains(&format!("client_secret={SECRET}"));
            if ok {
                Reply::ok(
                    json!({"access_token": TOKEN, "expires_in": 599, "token_type": "Bearer"})
                        .to_string(),
                )
            } else {
                Reply::status(400, r#"{"error":"invalid_client"}"#)
            }
        }
        "/products/v4/search/keyword" if authorized && request.method == "POST" => {
            let body: Value = serde_json::from_str(&request.body).unwrap();
            if body["Keywords"].as_str().unwrap_or("").is_empty() {
                return Reply::status(400, r#"{"title":"Bad Request","detail":"Keywords is required","status":400}"#);
            }
            Reply::ok(
                json!({
                    "Products": [product()],
                    "ProductsCount": 41,
                    "ExactMatches": [],
                    "SearchLocaleUsed": LOCALE(),
                })
                .to_string(),
            )
        }
        "/products/v4/search/TEST-DK-1/productdetails"
            if authorized && request.header("X-DIGIKEY-Account-Id") == ACCOUNT =>
        {
            Reply::ok(json!({"Product": product(), "SearchLocaleUsed": LOCALE()}).to_string())
        }
        path if path.starts_with("/products/v4/") && !authorized => Reply::status(401, ""),
        _ => Reply::status(404, ""),
    }
}

fn project(name: &str, manifest: &str) -> TempDir {
    let root = TempDir::new(&format!("catalog-search-digikey-{name}"));
    fs::write(root.join("netsmith.toml"), manifest).unwrap();
    fs::create_dir_all(root.join("catalogs")).unwrap();
    fs::write(root.join("catalogs/acme_cap.csv"), CATALOG).unwrap();
    root
}

fn netsmith(root: &Path, fake: &Fake, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .current_dir(root)
        .env(
            "NETSMITH_DIGIKEY_URL",
            format!("http://127.0.0.1:{}/products/v4", fake.port),
        )
        .env(
            "NETSMITH_DIGIKEY_TOKEN_URL",
            format!("http://127.0.0.1:{}/v1/oauth2/token", fake.port),
        )
        // Nexar, when a project names it too, is the same fake, which has no GraphQL endpoint.
        .env("NETSMITH_NEXAR_URL", format!("http://127.0.0.1:{}/graphql", fake.port))
        .env(
            "NETSMITH_NEXAR_TOKEN_URL",
            format!("http://127.0.0.1:{}/connect/token", fake.port),
        )
        .env("NETSMITH_CACHE_DIR", root.join("cache"))
        .env("TEST_DK_ID", CLIENT_ID)
        .env("TEST_DK_SECRET", SECRET)
        .env("TEST_DK_ACCOUNT", ACCOUNT)
        .env("TEST_NEXAR_ID", "nexar-id")
        .env("TEST_NEXAR_SECRET", "nexar-secret")
        .args(args)
        .output()
        .unwrap()
}

fn json_of(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn files(dir: &Path, all: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, all);
        } else {
            all.push(path);
        }
    }
}

/// No file under the project or the cache holds the secret, the token or the account id.
fn no_file_holds_a_credential(root: &Path) {
    let mut all = Vec::new();
    files(root, &mut all);
    assert!(all.iter().any(|p| p.starts_with(root.join("cache"))), "{all:?}");
    for path in all {
        let text = String::from_utf8_lossy(&fs::read(&path).unwrap()).into_owned();
        assert!(
            !text.contains(SECRET) && !text.contains(TOKEN) && !text.contains(ACCOUNT),
            "{} holds a credential",
            path.display()
        );
    }
}

fn ok(out: &Output) {
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_search_asks_digikey_once_with_the_token_and_the_client_id_and_keeps_no_secret() {
    let root = project("json", MANIFEST);
    let fake = Fake::start(digikey);
    let out = netsmith(
        &root,
        &fake,
        &["catalog", "search", "100nF", "0402", "--in-stock", "--limit", "80", "--format", "json", "--delay", "0"],
    );
    ok(&out);
    let answer = json_of(&out);
    assert_eq!(answer["provider"], "digikey");
    assert_eq!((answer["cached"].clone(), answer["requests"].clone()), (json!(false), json!(2)));
    assert_eq!(answer["hits"], 41);
    let found = &answer["parts"][0];
    assert_eq!(
        (found["manufacturer"].clone(), found["mpn"].clone()),
        (json!("Acme Passives"), json!("TESTCAP-100N"))
    );
    // Digi-Key names no package: nothing is guessed in its place.
    assert_eq!(found["package"], Value::Null);
    assert_eq!(
        found["values"],
        json!([{"name": "Capacitance", "shortname": "2049", "value": "100 nF"}])
    );
    assert_eq!(found["datasheet"], "https://example.com/testcap.pdf");
    let offers = found["offers"].as_array().unwrap();
    assert_eq!(offers.len(), 2);
    assert_eq!(
        (offers[0]["supplier"].clone(), offers[0]["sku"].clone(), offers[0]["stock"].clone()),
        (json!("digikey"), json!("TEST-DK-1"), json!(1200))
    );
    assert_eq!(offers[0]["packaging"], "Cut Tape (CT)");
    assert_eq!(offers[0]["currency"], "USD");
    assert_eq!(
        offers[0]["price_breaks"],
        json!([{"from": 1, "price": 0.1}, {"from": 100, "price": 0.04}])
    );
    assert_eq!((offers[1]["stock"].clone(), offers[1]["moq"].clone()), (json!(0), json!(10000)));

    // The token, then the keyword search with it and the client id; each says what the program is.
    let seen = fake.seen();
    let asked: Vec<(&str, &str)> = seen.iter().map(|s| (s.method.as_str(), s.path())).collect();
    assert_eq!(
        asked,
        [("POST", "/v1/oauth2/token"), ("POST", "/products/v4/search/keyword")]
    );
    assert!(
        seen.iter().all(|s| s.user_agent.starts_with("netsmith-catalog-search/")),
        "{seen:?}"
    );
    let body: Value = serde_json::from_str(&seen[1].body).unwrap();
    // Digi-Key's most is 50; in stock is one of its search options.
    assert_eq!(
        body,
        json!({"Keywords": "100nF 0402", "Limit": 50, "Offset": 0,
               "FilterOptionsRequest": {"SearchOptions": ["InStock"]}})
    );
    assert!(!seen[1].body.contains(SECRET));

    // Asked again within the day: from the cache, nothing sent.
    let again = json_of(&netsmith(
        &root,
        &fake,
        &["catalog", "search", "100nF", "0402", "--in-stock", "--limit", "80", "--format", "json"],
    ));
    assert_eq!((again["cached"].clone(), again["requests"].clone()), (json!(true), json!(0)));
    assert_eq!(fake.seen().len(), 2);

    // The text names the command that adds the part, by its Digi-Key number.
    let text = netsmith(&root, &fake, &["catalog", "search", "100nF", "0402", "--in-stock", "--limit", "80"]);
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(text.starts_with("Digi-Key read "), "{text}");
    assert!(text.contains("digikey  TEST-DK-1  stock 1200  1+ 0.1, 100+ 0.04 USD  (Cut Tape (CT))"), "{text}");
    assert!(text.contains("add: netsmith catalog add --from digikey TEST-DK-1"), "{text}");

    // A parametric filter is Nexar's: refused before anything is asked.
    let filtered = netsmith(&root, &fake, &["catalog", "search", "--filter", "case_package=0402"]);
    assert_eq!(filtered.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&filtered.stderr).contains("--provider nexar"));
    assert_eq!(fake.seen().len(), 2);
    no_file_holds_a_credential(&root);
}

#[test]
fn catalog_add_from_digikey_checks_digikeys_own_record_and_writes_only_its_offer() {
    let root = project("add", MANIFEST);
    let fake = Fake::start(digikey);
    let today = netsmith_fabricate::refresh::date_of(netsmith_fabricate::refresh::now_secs());
    let out = netsmith(
        &root,
        &fake,
        &["catalog", "add", "--from", "digikey", "TEST-DK-1", "--to", "catalogs/acme_cap.csv", "--delay", "0", "--format", "json"],
    );
    ok(&out);
    let added = json_of(&out);
    assert_eq!(added["provider"], "Digi-Key");
    assert_eq!(added["sku"], "TEST-DK-1");
    assert_eq!(added["written"], json!({"file": "catalogs/acme_cap.csv", "line": 2}));
    assert_eq!(
        added["offers"],
        json!([{"supplier": "digikey", "sku": "TEST-DK-1", "file": "offers/digikey.csv", "line": 2}])
    );
    // No package in Digi-Key's record: it is a cell to fill, not a guess.
    assert_eq!(added["package"], Value::Null);
    let to_fill = added["to_fill"].to_string();
    assert!(to_fill.contains("package") && to_fill.contains("capacitance"), "{to_fill}");
    // Without an account id the record is a keyword search for the number, taken only by an exact sku.
    let seen = fake.seen();
    let last = seen.last().unwrap();
    assert_eq!(last.path(), "/products/v4/search/keyword");
    assert!(last.body.contains("\"Keywords\":\"TEST-DK-1\""), "{}", last.body);
    assert_eq!(last.header("X-DIGIKEY-Account-Id"), "");

    let row = fs::read_to_string(root.join("catalogs/acme_cap.csv")).unwrap();
    let row = row.lines().nth(1).unwrap().to_string();
    assert!(row.starts_with("Acme Passives,TESTCAP-100N,"), "{row}");
    assert!(
        row.contains(&format!("Digi-Key record of TESTCAP-100N by Acme Passives (https://www.digikey.example/testcap-100n), read {today} from http://127.0.0.1:")),
        "{row}"
    );
    let offers = fs::read_to_string(root.join("offers/digikey.csv")).unwrap();
    let mut loaded = netsmith_solve::offers::Offers::empty();
    loaded.add_file("digikey", "offers/digikey.csv", &offers).unwrap();
    assert!(offers.lines().nth(1).unwrap().starts_with("Acme Passives,TESTCAP-100N,TEST-DK-1,,"), "{offers}");
    assert!(!root.join("offers/lcsc.csv").exists() && !root.join("offers/mouser.csv").exists());

    // By the mpn: two packagings, so the sku must say which (exit 1), nothing written.
    let two = netsmith(&root, &fake, &["catalog", "add", "--from", "digikey", "TESTCAP-100N", "--delay", "0"]);
    assert_eq!(two.status.code(), Some(1));
    let err = String::from_utf8_lossy(&two.stderr);
    assert!(err.contains("2 offers of digikey"), "{err}");

    // Another supplier's number is not in Digi-Key's API: refused before anything is asked.
    let asked = fake.seen().len();
    let other = netsmith(&root, &fake, &["catalog", "add", "--from", "lcsc", "C9900101"]);
    assert_eq!(other.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&other.stderr).contains("only Digi-Key's own offers"));
    assert_eq!(fake.seen().len(), asked);
    no_file_holds_a_credential(&root);
}

#[test]
fn with_an_account_id_the_record_is_digikeys_product_details_of_the_number() {
    let root = project("details", WITH_ACCOUNT);
    let fake = Fake::start(digikey);
    let out = netsmith(
        &root,
        &fake,
        &["catalog", "add", "--from", "digikey", "TEST-DK-1", "--to", "catalogs/acme_cap.csv", "--delay", "0", "--format", "json"],
    );
    ok(&out);
    assert_eq!(json_of(&out)["sku"], "TEST-DK-1");
    let seen = fake.seen();
    let last = seen.last().unwrap();
    assert_eq!(
        (last.method.as_str(), last.path()),
        ("GET", "/products/v4/search/TEST-DK-1/productdetails")
    );
    assert_eq!(last.header("X-DIGIKEY-Account-Id"), ACCOUNT);
    no_file_holds_a_credential(&root);
}

#[test]
fn with_both_providers_search_asks_nexar_unless_told_and_add_from_digikey_asks_digikey() {
    let root = project("both", BOTH);
    let fake = Fake::start(digikey);
    // The default is Nexar: the fake has no Nexar, so the error is Nexar's, and Digi-Key is not asked.
    let nexar = netsmith(&root, &fake, &["catalog", "search", "100nF", "--delay", "0", "--no-cache"]);
    assert_eq!(nexar.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&nexar.stderr).contains("Nexar (Octopart)"));
    assert!(fake.seen().iter().all(|s| !s.path().starts_with("/products/")));
    // --provider digikey asks Digi-Key.
    let told = netsmith(&root, &fake, &["catalog", "search", "100nF", "--provider", "digikey", "--delay", "0", "--format", "json"]);
    ok(&told);
    assert_eq!(json_of(&told)["provider"], "digikey");
    // `catalog add --from digikey` asks Digi-Key's own record without being told.
    let before = fake.seen().len();
    let add = netsmith(&root, &fake, &["catalog", "add", "--from", "digikey", "TEST-DK-1", "--delay", "0", "--format", "json"]);
    ok(&add);
    assert_eq!(json_of(&add)["provider"], "Digi-Key");
    assert!(fake.seen()[before..].iter().any(|s| s.path() == "/products/v4/search/keyword"));
    // A provider the project does not name is refused with how to set it up.
    let bare = project("bare", MANIFEST);
    let out = netsmith(&bare, &fake, &["catalog", "search", "x", "--provider", "nexar"]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--provider nexar, and netsmith.toml has no [supply.nexar] table"), "{err}");
}

#[test]
fn a_refused_token_is_asked_for_again_once_and_a_variable_not_set_asks_nothing() {
    let root = project("renew", MANIFEST);
    let searches = Arc::new(AtomicUsize::new(0));
    let fake = {
        let searches = Arc::clone(&searches);
        Fake::start(move |request| {
            if request.path() == "/products/v4/search/keyword"
                && searches.fetch_add(1, Ordering::SeqCst) == 0
            {
                return Reply::status(401, "");
            }
            digikey(request)
        })
    };
    let out = netsmith(&root, &fake, &["catalog", "search", "--mpn", "TESTCAP", "--delay", "0", "--format", "json"]);
    ok(&out);
    let paths: Vec<String> = fake.seen().iter().map(|s| s.path().to_string()).collect();
    assert_eq!(
        paths,
        [
            "/v1/oauth2/token",
            "/products/v4/search/keyword",
            "/v1/oauth2/token",
            "/products/v4/search/keyword"
        ]
    );
    assert!(fake.seen()[3].body.contains("\"Keywords\":\"TESTCAP\""));

    // A variable the table names that is not set: nothing is asked, and no value is shown.
    let asked = fake.seen().len();
    let out = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .current_dir(&root)
        .env("NETSMITH_DIGIKEY_URL", format!("http://127.0.0.1:{}/products/v4", fake.port))
        .env("NETSMITH_DIGIKEY_TOKEN_URL", format!("http://127.0.0.1:{}/v1/oauth2/token", fake.port))
        .env("TEST_DK_ID", CLIENT_ID)
        .env_remove("TEST_DK_SECRET")
        .args(["catalog", "search", "x"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("[supply.digikey] client_secret_env names the environment variable TEST_DK_SECRET, which is not set here")
            && err.contains("api-user-agreement"),
        "{err}"
    );
    assert!(!err.contains(CLIENT_ID), "{err}");
    assert_eq!(fake.seen().len(), asked);
}
