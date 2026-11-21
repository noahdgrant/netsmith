//! The outputs of `fab` and of an order by supply channel (ADR 0143, issue #1461): a board with
//! one part the board house buys in `jlcpcb-assembly`, one soldered by hand and bought in
//! `digikey-retail`, and one consigned and bought in `digikey-retail`. `bom.csv` is JLCPCB's file
//! as it was, with the consigned part's manufacturer's number as its comment and no LCSC number;
//! `fab-notes.txt` lists what the owner buys in each channel with its sku; `order.txt` lists them
//! by channel with the quantity of the whole order, each priced from its own channel's ladder.
//! Every part, number and price is invented; `example.invalid` is no real address. KiCad is not
//! asked for (ADR 0097).

use std::fs;
use std::path::Path;

use netsmith_fabricate::fab::FabOptions;
use netsmith_fabricate::order::{BOM_FILE, OrderLocks, SUMMARY_FILE, fab_order};
use netsmith_fabricate::project::Target;
use netsmith_pipeline::Libraries;
use netsmith_testkit::TempDir;

/// Three invented resistors of three values no standard row has.
const ROWS: &str = "\
mpn,resistance,max_power,package,source
ZZR-1,1.234kohm +/- 0.1%,100mW,0402,https://example.invalid/zzr-1
ZZR-2,2.345kohm +/- 0.1%,100mW,0402,https://example.invalid/zzr-2
ZZR-3,3.456kohm +/- 0.1%,100mW,0402,https://example.invalid/zzr-3
";

/// LCSC offers ZZR-1 in JLCPCB's library; Digi-Key offers the other two.
const LCSC: &str = "manufacturer,mpn,sku,class,source,checked,notes\n\
    ,ZZR-1,C9900011,Extended,https://example.invalid/lcsc,2026-11-20,\n";
const DIGIKEY: &str = "manufacturer,mpn,sku,class,source,checked,notes\n\
    ,ZZR-2,ZZR-2-ND,,https://example.invalid/digikey,2026-11-20,\n\
    ,ZZR-3,ZZR-3-ND,,https://example.invalid/digikey,2026-11-20,\n";

/// Invented quotes. ZZR-1 costs less in JLCPCB's assembly stock than at LCSC's retail stock, so
/// the price shown says which channel's ladder priced it; ZZR-2's second break is reached only
/// by the twelve boards of the order together.
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

[order.kit.boards]
Invented = 12
";

fn project() -> TempDir {
    let root = TempDir::new("channel-outputs");
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
    root
}

fn order(root: &Path) -> netsmith_fabricate::order::Made {
    let target = Target::resolve_from(root, None).unwrap();
    let o = target.project.as_ref().unwrap().manifest.orders[0].clone();
    fab_order(
        &o,
        &target,
        &target.loader().unwrap(),
        &target.catalogs().unwrap(),
        &Libraries::for_project(Some(root)).unwrap(),
        None,
        &FabOptions::default(),
        OrderLocks::Ignored,
        &target.fab_out,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn each_output_lists_its_lines_by_the_channel_that_buys_them() {
    let root = project();
    let made = order(&root);
    let board = made.dir.join("Invented");
    let notes = fs::read_to_string(board.join("fab-notes.txt")).unwrap();
    // The parts the board house places are numbered first, in the order the board declares
    // them, and the one soldered by hand after them.
    let (r1, r2, r3) = ("R1", "R3", "R2");

    // bom.csv is JLCPCB's: the part it buys by its LCSC number, the consigned one by its
    // manufacturer's number with the LCSC cell empty, and no line for the part soldered by hand.
    let bom = fs::read_to_string(board.join("bom.csv")).unwrap();
    let lines: Vec<&str> = bom.lines().collect();
    assert_eq!(lines[0], "Comment,Designator,Footprint,LCSC Part #", "{bom}");
    let line_of = |d: &str| lines.iter().find(|l| l.split(',').nth(1) == Some(d)).copied();
    let bought = line_of(r1).unwrap_or_else(|| panic!("no {r1} in\n{bom}"));
    assert!(bought.ends_with(",R_0402_1005Metric,C9900011"), "{bom}");
    assert_eq!(
        line_of(r3),
        Some(format!("ZZR-3,{r3},R_0402_1005Metric,").as_str()),
        "{bom}"
    );
    assert!(line_of(r2).is_none(), "the hand-soldered part is not in bom.csv:\n{bom}");
    assert!(!bom.contains("-ND"), "no Digi-Key number in JLCPCB's file:\n{bom}");
    // The placement file has the parts the board house places, and not the one soldered by hand.
    let cpl = fs::read_to_string(board.join("cpl.csv")).unwrap();
    let placed = |d: &str| cpl.lines().any(|l| l.split(',').next() == Some(d));
    assert!(placed(r1) && placed(r3) && !placed(r2), "{cpl}");

    // fab-notes.txt: who places each, and what the owner buys in each channel by its sku: the
    // header, which the standard offers sell in JLCPCB's assembly stock, and the two resistors in
    // Digi-Key's, by first designator.
    assert!(notes.contains(&format!("consigned: {r3} (ZZR-3, 1 part)")), "{notes}");
    let hand = notes
        .lines()
        .find(|l| l.starts_with("hand-solder: "))
        .unwrap_or_else(|| panic!("no hand-solder line in\n{notes}"));
    assert!(hand.split([',', ' ']).any(|d| d == r2), "{notes}");
    assert!(
        notes.contains("\nbuy from jlcpcb-assembly (JLCPCB assembly stock), for the parts you ship to the board house or solder yourself: C42431798 x1, PH2.54-1X2P-H25, for J1 (hand-solder)\n"),
        "{notes}"
    );
    assert!(
        notes.contains(&format!(
            "buy from digikey-retail (Digi-Key retail stock), for the parts you ship to the board house or solder yourself: ZZR-3-ND x1, ZZR-3, for {r3} (consigned); ZZR-2-ND x1, ZZR-2, for {r2} (hand-solder)\n"
        )),
        "{notes}"
    );

    // order.txt: the board house's bill as before, then what the owner buys by channel, each
    // with the quantity of the whole order (12 boards), and every line priced in its channel.
    let summary = fs::read_to_string(made.dir.join(SUMMARY_FILE)).unwrap();
    assert!(
        summary.contains("\nbill of materials (copies x parts on one board, no spares; also in order-bom.csv)\n  C9900011       12  "),
        "{summary}"
    );
    assert!(
        summary.contains(
            "\nparts you buy, by supply channel (consigned: you ship them to the board house, which places them; hand-solder: you solder them; copies x parts on one board, no spares)\n  jlcpcb-assembly (JLCPCB assembly stock)\n    C42431798      12  PH2.54-1X2P-H25, hand-solder (12 x 1 Invented)\n  digikey-retail (Digi-Key retail stock)\n    ZZR-2-ND       12  ZZR-2, hand-solder (12 x 1 Invented)\n    ZZR-3-ND       12  ZZR-3, consigned (12 x 1 Invented)\n"
        ),
        "{summary}"
    );
    for priced in [
        "  C9900011       12 x 0.0100 = 0.12 USD  (file, JLCPCB assembly stock, read 2026-11-19)\n",
        "  ZZR-2-ND       12 x 0.4000 = 4.80 USD  (file, Digi-Key retail stock, read 2026-11-19)\n",
        "  ZZR-3-ND       12 x 1.0000 = 12.00 USD  (file, Digi-Key retail stock, read 2026-11-19)\n",
    ] {
        assert!(summary.contains(priced), "missing {priced:?} in\n{summary}");
    }
    // order-bom.csv is what the board house buys: the one LCSC number.
    let csv = fs::read_to_string(made.dir.join(BOM_FILE)).unwrap();
    let rows: Vec<&str> = csv.lines().collect();
    assert_eq!(rows.len(), 2, "{csv}");
    assert!(rows[1].contains(",C9900011,12,12 x 1 Invented"), "{csv}");
}
