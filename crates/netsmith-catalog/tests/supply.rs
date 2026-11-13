//! The supply layer with invented parts and files only: no test reaches a supplier.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use netsmith_testkit::TempDir;

use netsmith_catalog::supply::*;
use netsmith_solve::Availability;

/// 2026-11-02 00:00 UTC.
const NOW: u64 = 1_793_577_600;
const HOUR: u64 = 3600;
const DAY: u64 = 86_400;

fn quote(provider: &str, channel: Channel, stock: Option<u64>, fetched: u64) -> Quote {
    Quote {
        provider: provider.to_string(),
        channel,
        stock,
        price_breaks: vec![
            PriceBreak {
                from: 1,
                price: 0.01,
            },
            PriceBreak {
                from: 100,
                price: 0.005,
            },
        ],
        currency: "USD".to_string(),
        fetched,
    }
}

/// A provider that counts how often it is asked and answers from a list, or fails.
struct Fake {
    asked: Arc<AtomicUsize>,
    answer: Result<Vec<Quote>, SupplyError>,
}

impl SupplyProvider for Fake {
    fn name(&self) -> &str {
        "fake"
    }

    fn cacheable(&self) -> bool {
        true
    }

    fn lookup(&self, _: &PartKey) -> Result<Vec<Quote>, SupplyError> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        self.answer.clone()
    }
}

fn fake(answer: Result<Vec<Quote>, SupplyError>) -> (Box<Fake>, Arc<AtomicUsize>) {
    let asked = Arc::new(AtomicUsize::new(0));
    (
        Box::new(Fake {
            asked: asked.clone(),
            answer,
        }),
        asked,
    )
}

fn part() -> PartKey {
    PartKey::lcsc("C900001")
}

#[test]
fn the_price_is_the_highest_break_at_or_below_the_quantity() {
    let q = quote("t", Channel::JlcpcbAssembly, Some(5), NOW);
    assert_eq!(q.price_for(1), Some((0.01, None)));
    assert_eq!(q.price_for(99), Some((0.01, None)));
    assert_eq!(q.price_for(100), Some((0.005, None)));
    assert_eq!(q.price_for(5000), Some((0.005, None)));
    // Quantity 0 is a part that is not on the board: priced as one.
    assert_eq!(q.price_for(0), Some((0.01, None)));
}

#[test]
fn below_the_first_break_the_first_price_is_given_with_where_it_starts() {
    let mut q = quote("t", Channel::JlcpcbAssembly, None, NOW);
    q.price_breaks.remove(0);
    assert_eq!(q.price_for(3), Some((0.005, Some(100))));
    q.price_breaks.clear();
    assert_eq!(q.price_for(3), None);
}

#[test]
fn with_no_provider_everything_is_unknown_and_says_why() {
    let got = Supply::none().lookup(&part());
    match got {
        Lookup::Unknown(why) => assert!(why.contains("no supply provider is configured"), "{why}"),
        Lookup::Quoted(q) => panic!("{q:?}"),
    }
}

#[test]
fn a_part_with_no_number_has_nothing_to_look_up() {
    let (provider, asked) = fake(Ok(Vec::new()));
    let supply = Supply::none().with_provider(provider);
    assert!(matches!(
        supply.lookup(&PartKey::default()),
        Lookup::Unknown(_)
    ));
    assert_eq!(asked.load(Ordering::SeqCst), 0);
}

#[test]
fn a_part_the_provider_does_not_list_is_unknown_and_named() {
    let (provider, _) = fake(Ok(Vec::new()));
    let Lookup::Unknown(why) = Supply::none().with_provider(provider).lookup(&part()) else {
        panic!("a quote for a part nobody lists");
    };
    assert_eq!(why, "fake does not list the part");
}

#[test]
fn a_fresh_cached_answer_is_used_without_asking_again() {
    let dir = TempDir::new("supply-cache-fresh");
    let (provider, asked) = fake(Ok(vec![quote(
        "fake",
        Channel::JlcpcbAssembly,
        Some(7),
        NOW,
    )]));
    let supply = Supply::none()
        .with_provider(provider)
        .with_cache_dir(dir.path().to_path_buf())
        .with_now(NOW);
    for _ in 0..3 {
        let got = supply.lookup(&part());
        assert_eq!(got.quote().and_then(|q| q.stock), Some(7));
    }
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    // The cache is a plain CSV a person can read.
    let text = std::fs::read_to_string(dir.path().join("fake.csv")).unwrap();
    assert!(text.starts_with("lcsc,mpn,channel,stock,price_breaks,currency,fetched\n"));
    assert!(
        text.contains("C900001,,jlcpcb-assembly,7,1:0.01 100:0.005,USD,"),
        "{text}"
    );
}

#[test]
fn an_answer_older_than_the_cache_time_is_asked_for_again_and_replaced() {
    let dir = TempDir::new("supply-cache-old");
    let old = quote("fake", Channel::JlcpcbAssembly, Some(1), NOW - 25 * HOUR);
    let new = quote("fake", Channel::JlcpcbAssembly, Some(2), NOW);
    Cache::new(dir.path().to_path_buf()).put("fake", &part(), &[old]);
    let (provider, asked) = fake(Ok(vec![new]));
    let supply = Supply::none()
        .with_provider(provider)
        .with_cache_dir(dir.path().to_path_buf())
        .with_now(NOW);
    assert_eq!(
        supply.lookup(&part()).quote().and_then(|q| q.stock),
        Some(2)
    );
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    let kept = Cache::new(dir.path().to_path_buf()).get("fake", &part());
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].stock, Some(2));
}

#[test]
fn the_cache_time_is_the_manifests() {
    let dir = TempDir::new("supply-cache-hours");
    let kept = quote("fake", Channel::JlcpcbAssembly, Some(1), NOW - 3 * HOUR);
    Cache::new(dir.path().to_path_buf()).put("fake", &part(), &[kept]);
    let (provider, asked) = fake(Ok(vec![quote(
        "fake",
        Channel::JlcpcbAssembly,
        Some(2),
        NOW,
    )]));
    let supply = Supply::none()
        .with_provider(provider)
        .with_cache_dir(dir.path().to_path_buf())
        .with_cache_hours(2)
        .with_now(NOW);
    assert_eq!(
        supply.lookup(&part()).quote().and_then(|q| q.stock),
        Some(2)
    );
    assert_eq!(asked.load(Ordering::SeqCst), 1);
}

#[test]
fn offline_an_old_cached_answer_is_used_with_its_age_and_nothing_is_asked() {
    let dir = TempDir::new("supply-cache-offline");
    let old = quote("fake", Channel::JlcpcbAssembly, Some(4), NOW - 40 * DAY);
    Cache::new(dir.path().to_path_buf()).put("fake", &part(), &[old]);
    let (provider, asked) = fake(Err(SupplyError::Offline));
    let supply = Supply::none()
        .with_provider(provider)
        .with_cache_dir(dir.path().to_path_buf())
        .with_offline(true)
        .with_now(NOW);
    let got = supply.lookup(&part());
    let q = got.quote().expect("the kept answer");
    assert_eq!((q.stock, q.fetched), (Some(4), NOW - 40 * DAY));
    assert_eq!(asked.load(Ordering::SeqCst), 0);
}

#[test]
fn when_the_provider_fails_the_old_answer_is_used_and_with_none_the_part_is_unknown() {
    let dir = TempDir::new("supply-cache-fails");
    let old = quote("fake", Channel::JlcpcbAssembly, Some(4), NOW - 3 * DAY);
    Cache::new(dir.path().to_path_buf()).put("fake", &part(), &[old]);
    let make = |dir: &TempDir| {
        let (provider, _) = fake(Err(SupplyError::Unavailable("refused".into())));
        Supply::none()
            .with_provider(provider)
            .with_cache_dir(dir.path().to_path_buf())
            .with_now(NOW)
    };
    assert_eq!(
        make(&dir).lookup(&part()).quote().and_then(|q| q.stock),
        Some(4)
    );
    let empty = TempDir::new("supply-cache-nothing");
    let Lookup::Unknown(why) = make(&empty).lookup(&part()) else {
        panic!("a quote from nothing");
    };
    assert_eq!(why, "fake: refused");
}

#[test]
fn offline_with_nothing_kept_is_unknown_never_zero() {
    let dir = TempDir::new("supply-offline-nothing");
    let (provider, asked) = fake(Err(SupplyError::Offline));
    let supply = Supply::none()
        .with_provider(provider)
        .with_cache_dir(dir.path().to_path_buf())
        .with_offline(true)
        .with_now(NOW);
    let got = supply.lookup(&part());
    assert_eq!(got, Lookup::Unknown("fake: offline".to_string()));
    assert_eq!(got.quote().and_then(|q| q.stock), None);
    assert_eq!(asked.load(Ordering::SeqCst), 0);
    // Nothing was written for the failure.
    assert!(!dir.path().join("fake.csv").exists());
}

#[test]
fn an_unreadable_cache_row_is_ignored() {
    let dir = TempDir::new("supply-cache-bad");
    std::fs::write(
        dir.path().join("fake.csv"),
        "lcsc,mpn,channel,stock,price_breaks,currency,fetched\n\
         C900001,,warehouse,5,,USD,1\n\
         C900001,,jlcpcb-assembly,lots,,USD,1\n\
         short,row\n",
    )
    .unwrap();
    assert!(
        Cache::new(dir.path().to_path_buf())
            .get("fake", &part())
            .is_empty()
    );
}

#[test]
fn the_projects_channel_is_preferred_and_the_other_says_which_it_is() {
    let both = vec![
        quote("fake", Channel::LcscRetail, Some(900), NOW),
        quote("fake", Channel::JlcpcbAssembly, Some(100), NOW),
    ];
    let (provider, _) = fake(Ok(both.clone()));
    let got = Supply::none().with_provider(provider).lookup(&part());
    assert_eq!(
        got.quote().map(|q| (q.channel, q.stock)),
        Some((Channel::JlcpcbAssembly, Some(100)))
    );

    let (provider, _) = fake(Ok(both));
    let got = Supply::none()
        .with_provider(provider)
        .with_channel(Channel::LcscRetail)
        .lookup(&part());
    assert_eq!(got.quote().map(|q| q.stock), Some(Some(900)));

    // Only the other channel is known: it is the answer, labelled, and not the stock of
    // the channel the project means.
    let (provider, _) = fake(Ok(vec![quote("fake", Channel::LcscRetail, Some(900), NOW)]));
    let supply = Supply::none().with_provider(provider);
    let got = supply.lookup(&part());
    assert_eq!(got.quote().map(|q| q.channel), Some(Channel::LcscRetail));
    assert_eq!(StockOf(supply).stock("C900001"), None);
}

// ---- the file provider -----------------------------------------------------------------

const FILE: &str = "lcsc,mpn,channel,stock,price_breaks,currency,date\n\
                    C900001,TESTRES-10K,jlcpcb-assembly,5000,1:0.0100 100:0.0050,USD,2026-11-01\n\
                    C900001,TESTRES-10K,lcsc-retail,120,1:0.0200,USD,2026-11-01\n\
                    C900002,,jlcpcb-assembly,,,,2026-10-01\n";

#[test]
fn the_file_provider_answers_a_quote_for_each_channel_and_dates_it_from_the_file() {
    let file = FileProvider::parse("quotes.csv", FILE).unwrap();
    let quotes = file.lookup(&part()).unwrap();
    assert_eq!(quotes.len(), 2);
    assert_eq!(quotes[0].channel, Channel::JlcpcbAssembly);
    assert_eq!(quotes[0].stock, Some(5000));
    assert_eq!(quotes[0].fetched, 1_793_491_200);
    assert_eq!(quotes[1].channel, Channel::LcscRetail);
    // By manufacturer's number when the key has no LCSC number.
    let by_mpn = file
        .lookup(&PartKey {
            lcsc: None,
            mpn: Some("TESTRES-10K".into()),
        })
        .unwrap();
    assert_eq!(by_mpn.len(), 2);
    // An unlisted part is an empty answer, not an error.
    assert_eq!(file.lookup(&PartKey::lcsc("C900009")).unwrap(), []);
    assert!(!file.cacheable());
}

#[test]
fn a_blank_stock_and_a_blank_price_are_unknown_not_zero() {
    let file = FileProvider::parse("quotes.csv", FILE).unwrap();
    let q = &file.lookup(&PartKey::lcsc("C900002")).unwrap()[0];
    assert_eq!(q.stock, None);
    assert!(q.price_breaks.is_empty());
    assert_eq!(q.currency, "USD");
}

#[test]
fn a_mistake_in_a_supply_file_names_the_line() {
    let head = "lcsc,mpn,channel,stock,price_breaks,currency,date\n";
    for (row, expect) in [
        (
            "C1,,warehouse,5,,USD,2026-11-01\n",
            "quotes.csv:2: channel `warehouse`",
        ),
        (
            "C1,,lcsc-retail,many,,USD,2026-11-01\n",
            "quotes.csv:2: stock `many`",
        ),
        (
            "C1,,lcsc-retail,5,100:1 1:2,USD,2026-11-01\n",
            "quotes.csv:2: price_breaks",
        ),
        (
            "C1,,lcsc-retail,5,,USD,yesterday\n",
            "quotes.csv:2: date `yesterday`",
        ),
        (
            ",,lcsc-retail,5,,USD,2026-11-01\n",
            "quotes.csv:2: the row names no part",
        ),
        (
            "C1,,lcsc-retail,5,,USD\n",
            "quotes.csv:2: the row has 6 fields",
        ),
    ] {
        let e = FileProvider::parse("quotes.csv", &format!("{head}{row}")).unwrap_err();
        assert!(e.contains(expect), "{e}");
    }
    let twice =
        format!("{head}C1,,lcsc-retail,5,,USD,2026-11-01\nC1,,lcsc-retail,6,,USD,2026-11-01\n");
    assert!(
        FileProvider::parse("quotes.csv", &twice)
            .unwrap_err()
            .contains("quotes.csv:3: the part C1 is there twice for lcsc-retail")
    );
    let e = FileProvider::parse("quotes.csv", "lcsc,price\nC1,2\n").unwrap_err();
    assert!(e.contains("there is no `channel` column"), "{e}");
    let e = FileProvider::parse(
        "quotes.csv",
        "lcsc,channel,date,secret\nC1,lcsc-retail,2026-11-01,x\n",
    )
    .unwrap_err();
    assert!(
        e.contains("`secret` is not a column of a supply file"),
        "{e}"
    );
}

#[test]
fn a_file_provider_in_a_supply_is_not_cached() {
    let dir = TempDir::new("supply-file-uncached");
    let supply = Supply::none()
        .with_provider(Box::new(FileProvider::parse("quotes.csv", FILE).unwrap()))
        .with_cache_dir(dir.path().to_path_buf())
        .with_now(NOW);
    assert_eq!(
        supply.lookup(&part()).quote().and_then(|q| q.stock),
        Some(5000)
    );
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
}
