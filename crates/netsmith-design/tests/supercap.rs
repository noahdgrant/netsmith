//! The supercapacitor (issue #1261) at the design stage: its swelling read from the catalog
//! row, and how it may be soldered against who solders
//! it. Every row here is invented: no real part has a recorded source yet
//! (`data/jlcpcb/README.md`).

use netsmith_check::{Loader, check};
use netsmith_design::{Designed, design_board};
use netsmith_solve::{Catalogs, solve};

const HEAD: &str = "mpn,capacitance,esr,dc_esr,leakage_current,max_voltage,cells,soldering,package,height_growth,assembly,source\n";

/// Two cells, 5.5 V to 65 degC and 4.2 V to 85 degC, soldered by hand only, and hand-assembled
/// by its row.
const HAND: &str = "ZZS-0F5,0.5F +/- 20%,120mohm,200mohm,6uA,\"5.5V to 65degC, 4.2V to 85degC\",2,Hand,Radial_D16.0mm_P7.50mm,1mm,hand,https://example.invalid/zzs-0f5.pdf\n";
/// One that allows reflow, and says nothing of who assembles it.
const REFLOW: &str = "ZZS-0F5-R,0.5F +/- 20%,120mohm,200mohm,6uA,5.5V,2,Reflow,Radial_D16.0mm_P7.50mm,1mm,,https://example.invalid/zzs-0f5-r.pdf\n";
/// One held in a holder, never soldered.
const HELD: &str = "ZZS-0F5-H,0.5F +/- 20%,120mohm,200mohm,6uA,5.5V,2,None,Radial_D16.0mm_P7.50mm,1mm,,https://example.invalid/zzs-0f5-h.pdf\n";

fn build(row: &str, environment: &str, lines: &str) -> Result<Designed, Vec<String>> {
    let mut catalogs = Catalogs::empty();
    netsmith_solve::migrate::add_to(&mut catalogs, "supercap", "catalogs/supercap.csv", &format!("{HEAD}{row}"))
        .unwrap();
    let src = format!(
        "import std::env::Environment
import std::parts::passive::Supercap
import std::sources::Rail

board B {{
    .environment: Environment {{ .temperature &= {environment} }}
    .supply: Rail {{ .voltage &= 5V +/- 5%; .pos.net &= \"VBAK\" }}
    .hold: Supercap
    .supply.pos ~> .hold ~> .supply.neg
{lines}
}}
"
    );
    let c = check("t.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &catalogs);
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    design_board(&c.boards[0], &s)
}

#[test]
fn the_rows_swelling_is_the_parts_height_growth() {
    let d = build(REFLOW, "-20degC..60degC", "").unwrap();
    let growth: Vec<_> = d
        .design
        .features
        .height_growths
        .iter()
        .map(|g| (g.path.as_str(), g.growth))
        .collect();
    assert_eq!(growth, [("hold", 1.0)]);
}

#[test]
fn a_part_soldered_by_hand_only_is_refused_where_the_board_house_solders_it() {
    // Its row says the hand assembles it: built.
    assert!(build(HAND, "-20degC..60degC", "").is_ok());
    let e = build(
        HAND,
        "-20degC..60degC",
        "    .hold.assembly &= AssemblyMode::Fab",
    )
    .unwrap_err();
    assert_eq!(e.len(), 1, "{e:#?}");
    assert!(
        e[0].starts_with("`hold` may be soldered by hand only (its catalog row ZZS-0F5 states `soldering` Hand), and its assembly is `AssemblyMode::Fab`"),
        "{}",
        e[0]
    );
    assert!(e[0].contains("`.hold.assembly &= AssemblyMode::Hand`"), "{}", e[0]);
}

#[test]
fn a_part_that_allows_reflow_may_be_soldered_any_way() {
    assert!(build(REFLOW, "-20degC..60degC", "").is_ok());
    assert!(
        build(
            REFLOW,
            "-20degC..60degC",
            "    .hold.assembly &= AssemblyMode::Hand"
        )
        .is_ok()
    );
}

#[test]
fn a_part_that_is_not_soldered_must_not_be_fitted() {
    let e = build(HELD, "-20degC..60degC", "").unwrap_err();
    assert_eq!(e.len(), 1, "{e:#?}");
    assert!(
        e[0].starts_with("`hold` is not soldered at all (its catalog row ZZS-0F5-H states `soldering` None), and its assembly is `AssemblyMode::Fab`"),
        "{}",
        e[0]
    );
    assert!(
        build(
            HELD,
            "-20degC..60degC",
            "    .hold.assembly &= AssemblyMode::None"
        )
        .is_ok()
    );
}
