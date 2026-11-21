//! `netsmith catalog search` and `catalog add --from` (issue #1460) against a fake Nexar on
//! 127.0.0.1: the token endpoint and the GraphQL endpoint answer from the parts below, which are
//! made up (`TESTCAP-100N` of `Acme Passives`, skus `TEST-...` and `C9900101`) and name no real
//! part. No test reaches the network, and none reaches Nexar.

use crate::common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::endpoints::{Fake, Reply, Seen};
use netsmith_testkit::TempDir;
use serde_json::{Value, json};

/// The values the fake Nexar wants; the test looks for the secret in every file afterwards.
const CLIENT_ID: &str = "test-client-id";
const SECRET: &str = "Sekret-Value-For-The-Test-31415";
const TOKEN: &str = "test-token-1";

const MANIFEST: &str = "[project]\nname = \"acme\"\n\n[supply.nexar]\nclient_id_env = \"TEST_NEXAR_ID\"\nclient_secret_env = \"TEST_NEXAR_SECRET\"\n";

const CATALOG: &str = "manufacturer,mpn,name,label,capacitance,package,source,notes\n";

/// The made-up part, as Nexar's GraphQL answer has it; `details` adds the specs and the datasheet.
fn part(details: bool) -> Value {
    let mut part = json!({
        "mpn": "TESTCAP-100N",
        "manufacturer": {"name": "Acme Passives"},
        "shortDescription": "Test capacitor 100nF 16V 0402",
        "octopartUrl": "https://octopart.example/testcap-100n",
        "sellers": [
            {"company": {"id": "459", "name": "DigiKey"}, "isAuthorized": true, "offers": [
                {"sku": "TEST-DK-1", "inventoryLevel": 1200, "packaging": "Cut Tape", "moq": 1,
                 "updated": "2026-11-19T10:00:00Z",
                 "prices": [{"quantity": 1, "price": 0.1, "currency": "USD"},
                            {"quantity": 100, "price": 0.04, "currency": "USD"}]}
            ]},
            {"company": {"id": "2401", "name": "Mouser"}, "isAuthorized": true, "offers": [
                {"sku": "TEST-MO-1", "inventoryLevel": 50, "packaging": "Cut Tape", "prices": []},
                {"sku": "TEST-MO-2", "inventoryLevel": 0, "packaging": "Reel", "prices": []}
            ]},
            {"company": {"id": "11779", "name": "LCSC"}, "isAuthorized": false, "offers": [
                {"sku": "C9900101", "inventoryLevel": -2, "prices": []}
            ]},
            {"company": {"id": "1", "name": "Elsewhere"}, "isAuthorized": true, "offers": [
                {"sku": "E-1", "inventoryLevel": 5, "prices": []}
            ]}
        ]
    });
    if details {
        part["specs"] = json!([
            {"attribute": {"name": "Case/Package", "shortname": "case_package"}, "displayValue": "0402"},
            {"attribute": {"name": "Capacitance", "shortname": "capacitance"}, "displayValue": "100nF"}
        ]);
        part["bestDatasheet"] = json!({"url": "https://example.com/testcap.pdf"});
    }
    part
}

/// What the fake Nexar answers: a token for the right form, the part for a query with the token.
fn nexar(request: &Seen) -> Reply {
    match request.path() {
        "/connect/token" => {
            let form = &request.body;
            let ok = request.method == "POST"
                && form.contains("grant_type=client_credentials")
                && form.contains(&format!("client_id={CLIENT_ID}"))
                && form.contains(&format!("client_secret={SECRET}"))
                && form.contains("scope=supply.domain");
            if ok {
                Reply::ok(
                    json!({"access_token": TOKEN, "expires_in": 86400, "token_type": "Bearer"})
                        .to_string(),
                )
            } else {
                Reply::status(400, r#"{"error":"invalid_client"}"#)
            }
        }
        "/graphql" if request.authorization == format!("Bearer {TOKEN}") => {
            let body: Value = serde_json::from_str(&request.body).unwrap();
            let query = body["query"].as_str().unwrap();
            let details = query.contains("specs");
            let data = if query.contains("supMultiMatch") {
                json!({"supMultiMatch": [{"hits": 1, "error": null, "parts": [part(details)]}]})
            } else {
                json!({"supSearch": {"hits": 41, "results": [{"part": part(details)}]}})
            };
            Reply::ok(json!({ "data": data }).to_string())
        }
        "/graphql" => Reply::status(401, ""),
        _ => Reply::status(404, ""),
    }
}

fn project(name: &str, manifest: &str) -> TempDir {
    let root = TempDir::new(&format!("catalog-search-{name}"));
    fs::write(root.join("netsmith.toml"), manifest).unwrap();
    fs::create_dir_all(root.join("catalogs")).unwrap();
    fs::write(root.join("catalogs/acme_cap.csv"), CATALOG).unwrap();
    root
}

fn netsmith(root: &Path, fake: &Fake, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .current_dir(root)
        .env("NETSMITH_NEXAR_URL", format!("http://127.0.0.1:{}/graphql", fake.port))
        .env(
            "NETSMITH_NEXAR_TOKEN_URL",
            format!("http://127.0.0.1:{}/connect/token", fake.port),
        )
        .env("NETSMITH_CACHE_DIR", root.join("cache"))
        .env("TEST_NEXAR_ID", CLIENT_ID)
        .env("TEST_NEXAR_SECRET", SECRET)
        .args(args)
        .output()
        .unwrap()
}

fn json_of(out: &Output) -> Value {
    assert!(
        out.status.code().is_some(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// Every file under `dir`, for the look for the secret.
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

fn no_file_holds_the_secret(root: &Path) {
    let mut all = Vec::new();
    files(root, &mut all);
    assert!(all.iter().any(|p| p.starts_with(root.join("cache"))), "{all:?}");
    for path in all {
        let text = String::from_utf8_lossy(&fs::read(&path).unwrap()).into_owned();
        assert!(
            !text.contains(SECRET) && !text.contains(TOKEN),
            "{} holds the secret or the token",
            path.display()
        );
    }
}

#[test]
fn a_search_asks_nexar_once_with_the_projects_credentials_and_keeps_no_secret() {
    let root = project("json", MANIFEST);
    let fake = Fake::start(nexar);
    let out = netsmith(
        &root,
        &fake,
        &["catalog", "search", "100nF", "0402", "--format", "json", "--delay", "0"],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let answer = json_of(&out);
    assert_eq!(answer["command"], "catalog search");
    assert_eq!(answer["provider"], "nexar");
    assert_eq!(answer["query"]["keywords"], "100nF 0402");
    assert_eq!(answer["query"]["limit"], 5);
    assert_eq!((answer["cached"].clone(), answer["requests"].clone()), (json!(false), json!(2)));
    assert_eq!(answer["hits"], 41);
    let found = &answer["parts"][0];
    assert_eq!(found["manufacturer"], "Acme Passives");
    assert_eq!(found["mpn"], "TESTCAP-100N");
    // Not asked for: no package, no values, no datasheet, and nothing guessed in their place.
    assert_eq!(found["package"], Value::Null);
    assert_eq!(found["values"], json!([]));
    assert_eq!(found["other_sellers"], 1);
    let offers: Vec<(String, String)> = found["offers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            (
                o["supplier"].as_str().unwrap().to_string(),
                o["sku"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        offers,
        [
            ("lcsc".into(), "C9900101".into()),
            ("digikey".into(), "TEST-DK-1".into()),
            ("mouser".into(), "TEST-MO-1".into()),
            ("mouser".into(), "TEST-MO-2".into()),
        ]
    );
    // LCSC says it has some and not how many: unknown is not 0.
    assert_eq!(found["offers"][0]["stock"], Value::Null);
    assert_eq!(found["offers"][0]["stock_note"], "in stock, no quantity given");
    assert_eq!(found["offers"][1]["stock"], 1200);
    assert_eq!(
        found["offers"][1]["price_breaks"],
        json!([{"from": 1, "price": 0.1}, {"from": 100, "price": 0.04}])
    );
    assert_eq!(found["offers"][3]["stock"], 0);

    // The token, then the query with it; one at a time, each saying what the program is.
    let seen = fake.seen();
    let asked: Vec<(&str, &str)> = seen
        .iter()
        .map(|s| (s.method.as_str(), s.path()))
        .collect();
    assert_eq!(asked, [("POST", "/connect/token"), ("POST", "/graphql")]);
    assert!(
        seen.iter()
            .all(|s| s.user_agent.starts_with("netsmith-catalog-search/")),
        "{seen:?}"
    );
    assert_eq!(seen[1].authorization, format!("Bearer {TOKEN}"));
    assert!(!seen[1].body.contains(SECRET) && seen[1].body.contains("supSearch("));

    // Asked again within the day: from the cache, nothing sent.
    let again = json_of(&netsmith(
        &root,
        &fake,
        &["catalog", "search", "100nF", "0402", "--format", "json"],
    ));
    assert_eq!((again["cached"].clone(), again["requests"].clone()), (json!(true), json!(0)));
    assert_eq!(again["parts"], answer["parts"]);
    assert_eq!(fake.seen().len(), 2);
    // --offline with nothing kept for another search asks nothing and says so.
    let offline = netsmith(&root, &fake, &["catalog", "search", "1uF", "--offline"]);
    assert_eq!(offline.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&offline.stderr).contains("no answer to this search is kept"));
    assert_eq!(fake.seen().len(), 2);

    // The text names the command that adds the part, by the sku of its first offer.
    let text = netsmith(&root, &fake, &["catalog", "search", "100nF", "0402"]);
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(text.contains("1. Acme Passives TESTCAP-100N"), "{text}");
    assert!(
        text.contains("lcsc     C9900101  in stock, no quantity given  no price"),
        "{text}"
    );
    assert!(
        text.contains("add: netsmith catalog add --from lcsc C9900101 --to catalogs/<file>.csv"),
        "{text}"
    );
    assert!(text.contains("--details asks"), "{text}");
    no_file_holds_the_secret(&root);
}

#[test]
fn with_no_provider_or_no_credential_nothing_is_asked_and_the_command_says_how_to_set_it_up() {
    let fake = Fake::start(nexar);
    let bare = project("bare", "[project]\nname = \"acme\"\n");
    let out = netsmith(&bare, &fake, &["catalog", "search", "100nF"]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    for words in [
        "names no search provider ([supply.nexar])",
        "read the terms of use of Nexar",
        "client_id_env = \"NEXAR_CLIENT_ID\"",
        "never in a file of the project",
    ] {
        assert!(err.contains(words), "{err}\nlacks {words}");
    }

    // The table, and a variable it names not set.
    let named = project("unset", MANIFEST);
    let out = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .current_dir(&named)
        .env("NETSMITH_NEXAR_URL", format!("http://127.0.0.1:{}/graphql", fake.port))
        .env(
            "NETSMITH_NEXAR_TOKEN_URL",
            format!("http://127.0.0.1:{}/connect/token", fake.port),
        )
        .env("TEST_NEXAR_ID", CLIENT_ID)
        .env_remove("TEST_NEXAR_SECRET")
        .args(["catalog", "add", "--from", "digikey", "TEST-DK-1"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("client_secret_env names the environment variable TEST_NEXAR_SECRET, which is not set here"),
        "{err}"
    );
    assert!(!err.contains(CLIENT_ID), "{err}");

    // A value written where the name goes is refused, and not repeated in the message.
    let leaked = project(
        "value",
        &format!(
            "[project]\nname = \"acme\"\n\n[supply.nexar]\nclient_id_env = \"TEST_NEXAR_ID\"\nclient_secret_env = \"{SECRET}\"\n"
        ),
    );
    let out = netsmith(&leaked, &fake, &["catalog", "search", "100nF"]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("client_secret_env is not the name of an environment variable")
            && err.contains("never its value"),
        "{err}"
    );
    assert!(!err.contains(SECRET), "{err}");
    // An unknown key of the table is refused: there is no field for a value.
    let keyed = project(
        "key",
        "[project]\nname = \"acme\"\n\n[supply.nexar]\nclient_id_env = \"A\"\nclient_secret_env = \"B\"\nclient_secret = \"x\"\n",
    );
    let out = netsmith(&keyed, &fake, &["catalog", "search", "100nF"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("client_secret"));
    assert!(fake.seen().is_empty(), "{:?}", fake.seen());
}

#[test]
fn catalog_add_from_writes_the_row_and_the_offers_the_record_names_and_nothing_else() {
    let root = project("add", MANIFEST);
    let fake = Fake::start(nexar);
    let today = netsmith_fabricate::refresh::date_of(netsmith_fabricate::refresh::now_secs());
    let out = netsmith(
        &root,
        &fake,
        &[
            "catalog",
            "add",
            "--from",
            "digikey",
            "TEST-DK-1",
            "--to",
            "catalogs/acme_cap.csv",
            "--details",
            "--delay",
            "0",
            "--format",
            "json",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let added = json_of(&out);
    assert_eq!(added["command"], "catalog add");
    assert_eq!(added["from"], json!({"supplier": "digikey", "number": "TEST-DK-1"}));
    assert_eq!(
        (added["manufacturer"].clone(), added["mpn"].clone(), added["package"].clone()),
        (json!("Acme Passives"), json!("TESTCAP-100N"), json!("0402"))
    );
    assert_eq!(added["written"], json!({"file": "catalogs/acme_cap.csv", "line": 2}));
    assert_eq!(
        added["offers"],
        json!([
            {"supplier": "lcsc", "sku": "C9900101", "file": "offers/lcsc.csv", "line": 2},
            {"supplier": "digikey", "sku": "TEST-DK-1", "file": "offers/digikey.csv", "line": 2},
        ])
    );
    // Two packagings at Mouser: which one the project buys is its choice, so neither is written.
    let not_written = added["not_written"].to_string();
    assert!(
        not_written.contains("2 offers of mouser (TEST-MO-1 (Cut Tape), TEST-MO-2 (Reel))"),
        "{not_written}"
    );
    // The record gives no capacitance a row can use as it is, nor JLCPCB's class.
    let to_fill = added["to_fill"].to_string();
    assert!(to_fill.contains("capacitance") && to_fill.contains("the class of the offer"), "{to_fill}");
    assert_eq!(added["filled"], json!(["manufacturer", "mpn", "package", "source", "notes"]));

    let catalog = fs::read_to_string(root.join("catalogs/acme_cap.csv")).unwrap();
    let row = catalog.lines().nth(1).unwrap();
    assert!(row.starts_with("Acme Passives,TESTCAP-100N,,,,0402,"), "{row}");
    assert!(
        row.contains(&format!("Nexar (Octopart) record of TESTCAP-100N by Acme Passives (https://octopart.example/testcap-100n), read {today} from http://127.0.0.1:")),
        "{row}"
    );
    assert!(row.contains("https://example.com/testcap.pdf"), "{row}");
    for (supplier, sku) in [("lcsc", "C9900101"), ("digikey", "TEST-DK-1")] {
        let text = fs::read_to_string(root.join(format!("offers/{supplier}.csv"))).unwrap();
        assert!(text.starts_with("manufacturer,mpn,sku,class,source,checked,notes\n"), "{text}");
        let offer = text.lines().nth(1).unwrap();
        assert!(
            offer.starts_with(&format!("Acme Passives,TESTCAP-100N,{sku},,")) && offer.contains(&today),
            "{offer}"
        );
        // The offers file is one the loader reads: a source, a day, no class but JLCPCB's.
        let mut offers = netsmith_solve::offers::Offers::empty();
        offers
            .add_file(supplier, &format!("offers/{supplier}.csv"), &text)
            .unwrap();
    }
    assert!(!root.join("offers/mouser.csv").exists());
    // The record was asked by the sku, at DigiKey, with the details.
    let query = fake.seen().last().unwrap().body.clone();
    assert!(
        query.contains("supMultiMatch(")
            && query.contains("\"mpnOrSku\":\"TEST-DK-1\"")
            && query.contains("\"distributor_id\":[\"459\"]")
            && query.contains("bestDatasheet"),
        "{query}"
    );

    // Again: the file has the part, so nothing is written (exit 1).
    let again = netsmith(
        &root,
        &fake,
        &["catalog", "add", "--from", "digikey", "TEST-DK-1", "--to", "catalogs/acme_cap.csv", "--details", "--format", "json"],
    );
    assert_eq!(again.status.code(), Some(1));
    let refused = json_of(&again);
    assert_eq!(refused["ok"], false);
    assert!(
        refused["findings"][0]["message"]
            .as_str()
            .unwrap()
            .contains("catalogs/acme_cap.csv:2: the file already has a row for TESTCAP-100N"),
        "{refused}"
    );
    assert_eq!(fs::read_to_string(root.join("catalogs/acme_cap.csv")).unwrap(), catalog);
    // By the mpn at Mouser, which has two offers of it: the sku must say which (exit 1).
    let two = netsmith(&root, &fake, &["catalog", "add", "--from", "mouser", "TESTCAP-100N"]);
    assert_eq!(two.status.code(), Some(1));
    let err = String::from_utf8_lossy(&two.stderr);
    assert!(err.contains("has 2 offers of mouser (TEST-MO-1 (Cut Tape), TEST-MO-2 (Reel))"), "{err}");
    // A supplier netsmith has no offers of is refused before anything is asked.
    let asked = fake.seen().len();
    let other = netsmith(&root, &fake, &["catalog", "add", "--from", "farnell", "X1"]);
    assert_eq!(other.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&other.stderr).contains("lcsc, digikey, mouser"));
    assert_eq!(fake.seen().len(), asked);
    no_file_holds_the_secret(&root);
}

#[test]
fn a_refused_token_is_asked_for_again_once_and_a_field_the_plan_lacks_is_an_error_that_says_so() {
    let root = project("plan", MANIFEST);
    // The first query is answered 401, as for a token that ended: a new token is asked for.
    let queries = Arc::new(AtomicUsize::new(0));
    let fake = {
        let queries = Arc::clone(&queries);
        Fake::start(move |request| {
            if request.path() == "/graphql" && queries.fetch_add(1, Ordering::SeqCst) == 0 {
                return Reply::status(401, "");
            }
            if request.path() == "/graphql" && request.body.contains("specs") {
                return Reply::ok(
                    json!({"data": null, "errors": [{"message": "The current plan does not include this field"}]})
                        .to_string(),
                );
            }
            nexar(request)
        })
    };
    let out = netsmith(
        &root,
        &fake,
        &["catalog", "search", "--mpn", "TESTCAP", "--delay", "0", "--format", "json"],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let paths: Vec<String> = fake.seen().iter().map(|s| s.path().to_string()).collect();
    assert_eq!(paths, ["/connect/token", "/graphql", "/connect/token", "/graphql"]);
    assert!(fake.seen()[3].body.contains("supSearchMpn("));

    let details = netsmith(
        &root,
        &fake,
        &["catalog", "search", "--mpn", "TESTCAP", "--details", "--delay", "0", "--format", "json"],
    );
    assert_eq!(details.status.code(), Some(2));
    assert!(details.stdout.is_empty());
    let err = String::from_utf8_lossy(&details.stderr);
    assert!(
        err.contains("The current plan does not include this field") && err.contains("try without it"),
        "{err}"
    );
    // An answer with errors is not kept.
    let again = netsmith(&root, &fake, &["catalog", "search", "--mpn", "TESTCAP", "--details", "--offline"]);
    assert_eq!(again.status.code(), Some(2));
    no_file_holds_the_secret(&root);
}
