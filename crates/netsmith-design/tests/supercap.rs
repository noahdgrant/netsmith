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

// ---- a storage element's discharge current on its terminals' nets (issue #1689) ----------

/// A supercapacitor across a 5 V rail with a Schottky diode in its return leg, whose row states
/// a discharge peak current of 3 A: the DC analysis sees the capacitor block, so without the
/// part's figure the node between the capacitor and the diode carries next to nothing.
fn hold_up_leg(row_current: &str) -> Designed {
    let mut catalogs = Catalogs::jlcpcb();
    let head = "mpn,capacitance,esr,dc_esr,leakage_current,max_voltage,cells,soldering,package,discharge_peak_current,source\n";
    let row = format!("ZZS-1F,1F +/- 20%,60mohm,100mohm,5uA,5.5V,2,Reflow,Radial_D16.0mm_P7.50mm,{row_current},https://example.invalid/zzs-1f.pdf\n");
    netsmith_solve::migrate::add_to(&mut catalogs, "supercap", "catalogs/supercap.csv", &format!("{head}{row}"))
        .unwrap();
    let src = "import std::mfg::jlcpcb::TwoLayer
import std::parts::diode::Schottky
import std::parts::passive::Supercap
import std::sources::Rail

board B {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail { .voltage &= 5V +/- 5%; .pos.net &= \"VBAK\"; .neg.net &= \"GND\" }
    .hold: Supercap
    .leg: Schottky { .current &= 0A..10mA }
    .supply.pos ~> .hold ~> .leg ~> .supply.neg
    .hold.n.net &= \"SC_RET\"
}
";
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &catalogs);
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    design_board(&c.boards[0], &s).unwrap()
}

#[test]
fn a_supercapacitors_discharge_peak_current_is_the_current_of_the_net_to_its_return_diode() {
    let d = hold_up_leg("3A");
    for c in &d.design.features.net_currents {
        eprintln!("{} {} {:?} {}", c.net, c.current, c.from, c.source);
    }
    let node = d
        .design
        .features
        .net_currents
        .iter()
        .find(|c| c.net == "SC_RET")
        .expect("the node between the capacitor and the diode has a current");
    assert_eq!(node.from, netsmith_core::CurrentFrom::Carrier, "{node:?}");
    assert!((node.current - 3.0).abs() < 1e-9, "{node:?}");
    assert!(
        node.source == "part `hold` states a `.discharge_peak_current` of 3 A, more than the 10 mA part `leg` carries",
        "{}",
        node.source
    );
    assert!(node.vias >= 2, "{node:?}");
    // The router draws the net for it, and the diode's pin is not held to the 10 mA it states.
    let width = d.design.features.net_widths.iter().find(|(n, _)| n == "SC_RET");
    assert!(width.is_some_and(|(_, w)| *w > 1.0), "{width:?}");
    assert!(
        d.design.features.pin_currents.iter().all(|p| p.net != "SC_RET"),
        "{:?}",
        d.design.features.pin_currents
    );
}

#[test]
fn a_supercapacitor_with_no_discharge_current_leaves_the_net_to_the_rest() {
    let d = hold_up_leg("");
    let node = d.design.features.net_currents.iter().find(|c| c.net == "SC_RET");
    assert!(
        node.is_none_or(|c| c.current < 1.0 && !c.source.contains("discharge")),
        "{node:?}"
    );
}

/// A cell (whose terminals are the rail it gives, not a bridge) that states a discharge peak
/// current of 2 A, feeding a 100 ohm load through a 1 ohm link: the DC analysis finds 50 mA.
#[test]
fn a_cells_discharge_peak_current_is_the_current_of_its_terminals_nets() {
    let mut catalogs = Catalogs::jlcpcb();
    let head = "mpn,chemistry,capacity,internal_resistance,charge_temperature,discharge_temperature,float_voltage,max_charge_current,discharge_peak_current,package,source\n";
    let row = "ZZC-1000,LiIon,3.6kC,150mohm,0degC..45degC,-20degC..60degC,4.2V +/- 50mV,500mA,2A,JST_PH_S2B-PH-K_1x02_P2.00mm_Horizontal,https://example.invalid/zzc-1000.pdf\n";
    netsmith_solve::migrate::add_to(
        &mut catalogs,
        "cell",
        "catalogs/cell.csv",
        &format!("{head}{row}"),
    )
    .unwrap();
    let src = "import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::sources::Cell

board B {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .bat: Cell { .terminals.voltage &= 3V..4.2V; .terminals.pos.net &= \"VBAT\"; .terminals.neg.net &= \"GND\" }
    .link: Resistor { .resistance &= 1ohm +/- 1% }
    .load: Resistor { .resistance &= 100ohm +/- 1% }
    .bat.terminals.pos ~> .link ~> .load ~> .bat.terminals.neg
}
";
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &catalogs);
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    let d = design_board(&c.boards[0], &s).unwrap();
    for c in &d.design.features.net_currents {
        eprintln!("{} {} {:?} {}", c.net, c.current, c.from, c.source);
    }
    for net in ["VBAT", "GND"] {
        let c = (d.design.features.net_currents.iter())
            .find(|c| c.net == net)
            .unwrap_or_else(|| panic!("no current for {net}"));
        assert!((c.current - 2.0).abs() < 1e-9, "{c:?}");
        assert!(
            c.source.starts_with("part `bat` states a `.discharge_peak_current` of 2 A"),
            "{}",
            c.source
        );
    }
    // The node between the link and the load is not a terminal's net: it keeps its own.
    let node = (d.design.features.net_currents.iter())
        .find(|c| c.net != "VBAT" && c.net != "GND")
        .expect("the node has a current");
    assert!(node.current < 0.1, "{node:?}");
}
