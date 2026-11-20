//! Supply channels (ADR 0143, issue #1459) with invented parts, invented numbers and a fake
//! provider: no test reaches a supplier. A project lists its channels in order, the first with
//! an offer of a part buys it, and each channel has its own rule for the rows that fit a generic
//! part; `.assembly` is a separate thing.

use netsmith_catalog::resolve::resolve_generics;
use netsmith_catalog::supply::*;
use netsmith_design::{Assembly, Sourced};
use netsmith_solve::{Catalogs, Generic, Series, Spec, resolve};

/// 2026-11-02 00:00 UTC.
const NOW: u64 = 1_793_577_600;

/// Two invented 249 ohm resistors that both fit the generic part of [`generic`], in file order.
const ROWS: &str = "\
mpn,resistance,max_power,package,source
ZZR-249-A,249ohm +/- 1%,62.5mW,0402,https://example.invalid/zzr-249-a
ZZR-249-B,249ohm +/- 1%,62.5mW,0402,https://example.invalid/zzr-249-b
";

const HEAD: &str = "manufacturer,mpn,sku,class,source,checked,notes\n";

/// The catalogs of a project whose channels are `channels`, with `offers` as `(supplier, rows)`.
fn catalogs(channels: &[Channel], offers: &[(&str, &str)]) -> Catalogs {
    let mut c = Catalogs::empty();
    c.add_file("resistor", "catalogs/resistor.csv", ROWS).unwrap();
    for (supplier, rows) in offers {
        c.add_offers_file(
            supplier,
            &format!("offers/{supplier}.csv"),
            &format!("{HEAD}{rows}"),
        )
        .unwrap();
    }
    c.with_channels(channels.to_vec())
}

/// A generic 249 ohm 1% 0402 resistor, which both rows stand for.
fn generic() -> Generic {
    Generic {
        catalog: "resistor".into(),
        package: "0402".into(),
        spec: Spec::Resistor {
            series: Series::E96,
            nominal: 249.0,
            tolerance_percent: 1.0,
            max_power: 0.0625,
        },
    }
}

/// A provider that answers from a list of `(sku, channel, stock, price of one piece)`.
struct Fake(Vec<(&'static str, Channel, Option<u64>, f64)>);

impl SupplyProvider for Fake {
    fn name(&self) -> &str {
        "fake"
    }

    fn cacheable(&self) -> bool {
        false
    }

    fn lookup(&self, part: &PartKey) -> Result<Vec<Quote>, SupplyError> {
        Ok(self
            .0
            .iter()
            .filter(|(sku, ..)| part.lcsc.as_deref() == Some(*sku))
            .map(|&(_, channel, stock, price)| Quote {
                provider: "fake".into(),
                channel,
                stock,
                price_breaks: vec![PriceBreak { from: 1, price }],
                currency: "USD".into(),
                fetched: NOW,
            })
            .collect())
    }
}

/// `catalogs` with the fake provider's quotes as their stock and price.
fn quoted(catalogs: Catalogs, quotes: Vec<(&'static str, Channel, Option<u64>, f64)>) -> Catalogs {
    let channels = catalogs.channels().to_vec();
    let supply = Supply::none()
        .with_provider(Box::new(Fake(quotes)))
        .with_channels(channels)
        .with_now(NOW);
    catalogs.with_availability(supply.into_availability())
}

/// The row a generic part resolves to, its sku, and what was passed over.
fn resolved(c: &Catalogs) -> (String, Option<String>, Vec<String>) {
    let stock = |channel: Channel, sku: &str| c.availability().and_then(|a| a.stock(channel, sku));
    let got = resolve(&generic(), c, &stock).unwrap_or_else(|e| panic!("{e}"));
    (got.name, got.lcsc, got.passed_over)
}

#[test]
fn a_part_is_bought_from_the_second_channel_when_the_first_has_no_offer() {
    // Digi-Key, the first channel, offers B only; A has only an LCSC offer, with no class.
    let c = catalogs(
        &[Channel::DIGIKEY_RETAIL, Channel::LCSC_RETAIL],
        &[
            ("lcsc", ",ZZR-249-A,C9900249,,https://example.invalid/lcsc,,\n"),
            (
                "digikey",
                ",ZZR-249-B,ZZR-249-B-ND,,https://example.invalid/digikey,,\n",
            ),
        ],
    );
    let columns = [
        ("resistance".to_string(), netsmith_check::FieldKind::Text),
        ("max_power".to_string(), netsmith_check::FieldKind::Text),
        ("package".to_string(), netsmith_check::FieldKind::Text),
    ];
    let rows = c.identity("resistor", &columns, &[]).unwrap().unwrap();
    let bought: Vec<(Option<Channel>, Option<&str>)> = rows
        .candidates
        .iter()
        .map(|r| (r.channel, r.lcsc.as_deref()))
        .collect();
    assert_eq!(
        bought,
        [
            (Some(Channel::LCSC_RETAIL), Some("C9900249")),
            (Some(Channel::DIGIKEY_RETAIL), Some("ZZR-249-B-ND")),
        ]
    );

    // With the fake provider, the first channel's row is in stock: it is taken, though it is
    // the second row of the file.
    let in_stock = quoted(
        c.clone(),
        vec![
            ("ZZR-249-B-ND", Channel::DIGIKEY_RETAIL, Some(40), 0.12),
            ("C9900249", Channel::LCSC_RETAIL, Some(9000), 0.01),
        ],
    );
    assert_eq!(
        resolved(&in_stock),
        ("ZZR-249-B".into(), Some("ZZR-249-B-ND".into()), vec![])
    );

    // The first channel has none: the part is bought from the second, and the message says
    // which channel had no stock.
    let sold_out = quoted(
        c,
        vec![
            ("ZZR-249-B-ND", Channel::DIGIKEY_RETAIL, Some(0), 0.12),
            ("C9900249", Channel::LCSC_RETAIL, Some(9000), 0.01),
        ],
    );
    assert_eq!(
        resolved(&sold_out),
        (
            "ZZR-249-A".into(),
            Some("C9900249".into()),
            vec!["ZZR-249-B has no stock in digikey-retail".to_string()]
        )
    );
}

#[test]
fn a_retail_channel_goes_by_stock_then_price_and_jlcpcb_assembly_by_class() {
    let offers = ",ZZR-249-A,C9900248,Extended,https://example.invalid/a,,\n\
                  ,ZZR-249-B,C9900249,Basic,https://example.invalid/b,,\n";
    // JLCPCB's assembly: Basic before Extended, whatever the price.
    let assembly = quoted(
        catalogs(&[Channel::JLCPCB_ASSEMBLY], &[("lcsc", offers)]),
        vec![
            ("C9900248", Channel::JLCPCB_ASSEMBLY, Some(10), 0.001),
            ("C9900249", Channel::JLCPCB_ASSEMBLY, Some(10), 0.5),
        ],
    );
    assert_eq!(resolved(&assembly).0, "ZZR-249-B");

    // LCSC's retail, the same offers: the lower price, the class not read.
    let retail = |quotes| quoted(catalogs(&[Channel::LCSC_RETAIL], &[("lcsc", offers)]), quotes);
    let cheaper_a = retail(vec![
        ("C9900248", Channel::LCSC_RETAIL, Some(10), 0.001),
        ("C9900249", Channel::LCSC_RETAIL, Some(10), 0.5),
    ]);
    assert_eq!(resolved(&cheaper_a).0, "ZZR-249-A");
    // A row in stock comes before a cheaper one whose stock is unknown.
    let unknown_a = retail(vec![
        ("C9900248", Channel::LCSC_RETAIL, None, 0.001),
        ("C9900249", Channel::LCSC_RETAIL, Some(10), 0.5),
    ]);
    assert_eq!(resolved(&unknown_a).0, "ZZR-249-B");
    // With no quote at all, the file order decides, as with no provider.
    let none = catalogs(&[Channel::LCSC_RETAIL], &[("lcsc", offers)]);
    assert_eq!(resolved(&none).0, "ZZR-249-A");
    // The stock of another channel is not the stock of the channel that buys the part.
    let other = retail(vec![
        ("C9900248", Channel::JLCPCB_ASSEMBLY, Some(0), 0.001),
        ("C9900249", Channel::LCSC_RETAIL, Some(10), 0.5),
    ]);
    assert_eq!(resolved(&other).0, "ZZR-249-B");
}

#[test]
fn the_preferred_list_comes_before_every_channel() {
    let c = catalogs(
        &[Channel::JLCPCB_ASSEMBLY],
        &[(
            "lcsc",
            ",ZZR-249-A,C9900248,Basic,https://example.invalid/a,,\n\
             ,ZZR-249-B,C9900249,Extended,https://example.invalid/b,,\n",
        )],
    )
    .with_preferred(netsmith_solve::Preferred::parse("preferred.csv", "mpn\nZZR-249-B\n").unwrap());
    assert_eq!(resolved(&c).0, "ZZR-249-B");
}

#[test]
fn a_part_no_channel_has_an_offer_of_is_bought_by_nothing() {
    // JLCPCB's library does not list A (no class), and Mouser offers nothing: with only those
    // two channels, neither row is bought. A generic part still resolves to a row (the first of
    // the file), with no number, and `fab` names it with the channels it tried.
    let c = catalogs(
        &[Channel::JLCPCB_ASSEMBLY, Channel::MOUSER_RETAIL],
        &[("lcsc", ",ZZR-249-A,C9900248,,https://example.invalid/a,,\n")],
    );
    let (row, sku, _) = resolved(&c);
    assert_eq!((row.as_str(), sku), ("ZZR-249-A", None));
    assert_eq!(
        Channel::list(c.channels()),
        "jlcpcb-assembly, mouser-retail"
    );
}

/// A part of the board, with the generic resistor, assembled by `assembly`.
fn part(refdes: &str, assembly: Assembly) -> Sourced {
    Sourced {
        refdes: refdes.into(),
        kind: "Resistor".into(),
        value: "249".into(),
        package: Some("0402".into()),
        lcsc: None,
        mpn: None,
        library: None,
        assembly,
        assembly_stated: true,
        catalog_height: None,
        resolved_row: None,
        not_fitted: None,
        generic: Some(generic()),
    }
}

#[test]
fn assembly_is_independent_of_the_channel() {
    // LCSC's retail stock is the one channel; who places each part is `.assembly`.
    let c = quoted(
        catalogs(
            &[Channel::LCSC_RETAIL],
            &[(
                "lcsc",
                ",ZZR-249-A,C9900248,,https://example.invalid/a,,\n\
                 ,ZZR-249-B,C9900249,,https://example.invalid/b,,\n",
            )],
        ),
        vec![
            ("C9900248", Channel::LCSC_RETAIL, Some(0), 0.001),
            ("C9900249", Channel::LCSC_RETAIL, Some(10), 0.5),
        ],
    );
    let parts = [
        part("R1", Assembly::Fab),
        part("R2", Assembly::Hand),
        part("R3", Assembly::Consigned),
    ];
    let (sourcing, unresolved) = resolve_generics(&parts, &c);
    assert!(unresolved.is_empty(), "{unresolved:?}");
    // The board house's part and the one soldered by hand are both bought in the channel, by
    // its rule (A has no stock, so B); each keeps who places it.
    for s in &sourcing[..2] {
        assert_eq!(s.mpn.as_deref(), Some("ZZR-249-B"), "{}", s.refdes);
        assert_eq!(s.lcsc.as_deref(), Some("C9900249"), "{}", s.refdes);
    }
    assert_eq!(sourcing[0].assembly, Assembly::Fab);
    assert_eq!(sourcing[1].assembly, Assembly::Hand);
    // The owner supplies a consigned part: no channel buys it, and it stays as it was.
    assert_eq!(sourcing[2].assembly, Assembly::Consigned);
    assert_eq!((sourcing[2].lcsc.as_deref(), sourcing[2].mpn.as_deref()), (None, None));
    // The channels say nothing of assembly: the same parts with the default channels (no
    // offer is in JLCPCB's assembly library, so LCSC's retail buys) keep their modes too.
    let default = quoted(
        catalogs(
            &[],
            &[("lcsc", ",ZZR-249-B,C9900249,,https://example.invalid/b,,\n")],
        ),
        vec![],
    );
    let (sourcing, _) = resolve_generics(&parts, &default);
    let modes: Vec<Assembly> = sourcing.iter().map(|s| s.assembly).collect();
    assert_eq!(modes, [Assembly::Fab, Assembly::Hand, Assembly::Consigned]);
    assert_eq!(sourcing[0].lcsc.as_deref(), Some("C9900249"));
}
