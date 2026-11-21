//! The editor's bill of materials by supply channel (ADR 0143, issue #1461): each line says the
//! channel that buys it and its sku there, and a consigned part says it is consigned. The board
//! is the one of `netsmith-fabricate`'s `channel_outputs` test: one part the board house buys in
//! `jlcpcb-assembly`, one consigned and one soldered by hand, both bought in `digikey-retail`.
//! Every part, number and price is invented; no test reaches a supplier.

use std::fs;

use netsmith_editor::Backend;
use netsmith_editor::api::BomLine;
use netsmith_fabricate::project::Target;
use netsmith_fabricate::refresh::Network;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

const ROWS: &str = "\
manufacturer,mpn,resistance,max_power,package,source
Acme,ZZR-1,1.234kohm +/- 0.1%,100mW,0402,https://example.invalid/zzr-1
,ZZR-2,2.345kohm +/- 0.1%,100mW,0402,https://example.invalid/zzr-2
,ZZR-3,3.456kohm +/- 0.1%,100mW,0402,https://example.invalid/zzr-3
";
const LCSC: &str = "manufacturer,mpn,sku,class,source,checked,notes\n\
    Acme,ZZR-1,C9900011,Extended,https://example.invalid/lcsc,2026-11-20,\n";
const DIGIKEY: &str = "manufacturer,mpn,sku,class,source,checked,notes\n\
    ,ZZR-2,ZZR-2-ND,,https://example.invalid/digikey,2026-11-20,\n\
    ,ZZR-3,ZZR-3-ND,,https://example.invalid/digikey,2026-11-20,\n";
/// ZZR-1 is cheaper in JLCPCB's assembly stock than at LCSC's retail stock.
const QUOTES: &str = "lcsc,mpn,channel,stock,price_breaks,currency,date\n\
    C9900011,,lcsc-retail,500,1:9.0000,USD,2026-11-19\n\
    C9900011,,jlcpcb-assembly,500,1:0.0100,USD,2026-11-19\n\
    ZZR-2-ND,,digikey-retail,500,1:0.5000 10:0.4000,USD,2026-11-19\n\
    ZZR-3-ND,,digikey-retail,500,1:1.0000,USD,2026-11-19\n";

const BOARD: &str = r#"
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Invented {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .bought: Resistor { .resistance &= 1.23kohm..1.24kohm }
    .soldered: Resistor {
        .resistance &= 2.34kohm..2.35kohm
        .assembly &= AssemblyMode::Hand
    }
    .shipped: Resistor {
        .resistance &= 3.45kohm..3.46kohm
        .assembly &= AssemblyMode::Consigned
    }
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
    .supply.pos ~> .bought ~> .soldered ~> .shipped ~> .supply.neg
}
"#;

const MANIFEST: &str = "\
[project]
name = \"invented\"
series = \"none\"
top_level = [\"src/main.nsm\"]

[supply]
channels = [\"jlcpcb-assembly\", \"digikey-retail\"]
provider = \"file\"
file = \"supply/quotes.csv\"
";

/// What a line says of where it is bought: `(designators, channel, sku, manufacturer, consigned,
/// hand-soldered, unit price, where the price is from)`.
type Said<'a> = (
    Vec<&'a str>,
    Option<&'a str>,
    Option<&'a str>,
    Option<&'a str>,
    bool,
    bool,
    Option<f64>,
    Option<&'a str>,
);

fn said(l: &BomLine) -> Said<'_> {
    (
        l.designators.iter().map(String::as_str).collect(),
        l.channel.as_deref(),
        l.sku.as_deref(),
        l.manufacturer.as_deref(),
        l.consigned,
        l.hand_solder,
        l.unit_usd,
        l.supply.as_deref(),
    )
}

#[test]
fn each_line_shows_its_channel_and_sku_and_a_consigned_part_says_so() {
    let root = TempDir::new("bom-channels");
    for (path, text) in [
        ("netsmith.toml", MANIFEST),
        ("src/main.nsm", BOARD),
        ("catalogs/resistor.csv", ROWS),
        ("offers/lcsc.csv", LCSC),
        ("offers/digikey.csv", DIGIKEY),
        ("supply/quotes.csv", QUOTES),
    ] {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap()
        .with_network(Network {
            offline: true,
            cache_dir: None,
            ..Network::default()
        });
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    let bom = editor.bom("Invented").unwrap();

    // The lines the board house places: the part it buys, priced from JLCPCB's assembly stock and
    // not LCSC's retail one, and the consigned part, with its Digi-Key number and price.
    let placed: Vec<Said> = bom.lines.iter().map(said).collect();
    assert_eq!(
        placed,
        [
            (
                vec!["R1"],
                Some("jlcpcb-assembly"),
                Some("C9900011"),
                Some("Acme"),
                false,
                false,
                Some(0.01),
                Some("file, JLCPCB assembly stock")
            ),
            (
                vec!["R2"],
                Some("digikey-retail"),
                Some("ZZR-3-ND"),
                None,
                true,
                false,
                Some(1.0),
                Some("file, Digi-Key retail stock")
            ),
        ]
    );
    // The part soldered by hand, with its channel and sku; one piece is at the first break.
    let resistor = bom
        .hand
        .iter()
        .find(|l| l.kind == "Resistor")
        .unwrap_or_else(|| panic!("{:?}", bom.hand));
    assert_eq!(
        said(resistor),
        (
            vec!["R3"],
            Some("digikey-retail"),
            Some("ZZR-2-ND"),
            None,
            false,
            true,
            Some(0.5),
            Some("file, Digi-Key retail stock")
        )
    );
}
