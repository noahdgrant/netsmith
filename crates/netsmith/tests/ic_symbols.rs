//! The symbol of an IC is drawn by netsmith from its pin table, so a board with one is written
//! without a KiCad symbol of the IC (ADR 0097, step 1). The board is the LT8641 stage of
//! `lt8641.rs`, whose row in `std/kicad.toml` names no symbol.

use netsmith_core::PowerFlow;
use netsmith_kicad::sexp::Sexp;
use netsmith_kicad::testlib::TempDir;

use crate::lt8641::{built, libraries};

#[test]
fn a_board_with_an_ic_writes_its_schematic_with_no_symbol_library_of_the_ic() {
    let dir = TempDir::new("ic-symbols");
    // Our sample libraries: the passives' symbols and nothing of the IC. The folder has no
    // library for it, and the row names none.
    let libraries = libraries(&dir);
    assert!(!libraries.symbols.root().join("netsmith.kicad_sym").exists());
    let first = built(&libraries);

    let ic = first
        .design
        .parts
        .iter()
        .find(|p| p.kind == "Lt8641")
        .expect("the stage has the IC");
    // The converter's output (`buck.sw.pos`, whose first pad is 8) gives power: the name says so.
    let name = "netsmith:Lt8641_UDC20_18_out8";
    assert!(
        first.schematic.contains(&format!("(lib_id \"{name}\")")),
        "no instance of {name}"
    );
    assert!(first.schematic.contains(&format!("(symbol \"{name}\"")));
    // A pin for each pad of the pin table, named as the datasheet does, drawn once.
    for pin in &ic.pins {
        assert!(
            first
                .schematic
                .contains(&format!("(name \"{}\"", pin.datasheet)),
            "no pin {}",
            pin.datasheet
        );
        for pad in &pin.pads {
            assert!(
                first.schematic.contains(&format!("(number \"{pad}\"")),
                "no pad {pad}"
            );
        }
    }

    // The same bytes on a second run.
    assert_eq!(first.schematic, built(&libraries).schematic);

    // The library the schematic's symbol comes from is written beside it, and listed.
    let out = TempDir::new("ic-symbols-export");
    let folder = first.write(out.path()).unwrap();
    libraries.write_tables(&folder, &first.design).unwrap();
    let library = std::fs::read_to_string(folder.join("netsmith.kicad_sym")).unwrap();
    assert!(
        library.contains("(symbol \"Lt8641_UDC20_18_out8\""),
        "{library}"
    );
    let table = std::fs::read_to_string(folder.join("sym-lib-table")).unwrap();
    assert!(table.contains("${KIPRJMOD}/netsmith.kicad_sym"), "{table}");
}

#[test]
fn a_part_that_names_a_kicad_symbol_is_still_drawn_with_it() {
    let dir = TempDir::new("ic-symbols-named");
    let libraries = libraries(&dir);
    let built = built(&libraries);
    assert!(built.schematic.contains("(lib_id \"Device:C\")"));
}

/// The electrical type each pad of the embedded symbol `lib_id` has, by pad number.
fn pin_types(schematic: &str, lib_id: &str) -> std::collections::BTreeMap<String, String> {
    let root = Sexp::parse(schematic).unwrap();
    let symbol = root
        .find("lib_symbols")
        .unwrap()
        .find_all("symbol")
        .find(|s| s.args()[0].text() == Some(lib_id))
        .unwrap_or_else(|| panic!("no symbol {lib_id}"));
    symbol
        .find_all("symbol")
        .flat_map(|unit| unit.find_all("pin"))
        .map(|p| {
            (
                p.find("number").unwrap().args()[0]
                    .text()
                    .unwrap()
                    .to_string(),
                p.args()[0].text().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn the_converters_switch_pin_gives_power_and_the_other_power_pins_take_it() {
    let dir = TempDir::new("ic-symbols-direction");
    let libraries = libraries(&dir);
    let built = built(&libraries);
    let ic = built
        .design
        .parts
        .iter()
        .find(|p| p.kind == "Lt8641")
        .unwrap();
    let flow = |name: &str| ic.pins.iter().find(|p| p.name == name).unwrap().power_flow;
    // The load draws from the output, so the rail of the switch node has a negative current.
    assert_eq!(flow("buck.sw.pos"), Some(PowerFlow::Gives));
    assert_eq!(flow("vin.pos"), Some(PowerFlow::Takes));
    // `vin2` is joined to `vin.pos` inside the part, and the grounds are no rail's `pos`.
    assert_eq!(flow("vin2"), Some(PowerFlow::Takes));
    assert_eq!(flow("gnd1"), None);

    // The schematic types the switch pins `power_out` (pad 8 and the three stacked on its
    // point, which are `passive` as for any pin) and the rest of the power pins `power_in`.
    let types = pin_types(&built.schematic, "netsmith:Lt8641_UDC20_18_out8");
    assert_eq!(types["8"], "power_out");
    for pad in ["9", "21", "22"] {
        assert_eq!(types[pad], "passive", "pad {pad}");
    }
    for pad in ["4", "13", "6", "10"] {
        assert_eq!(types[pad], "power_in", "pad {pad}");
    }
    assert_eq!(types.values().filter(|t| *t == "power_out").count(), 1);

    // The net of the switch has a source on the board, so it needs no `PWR_FLAG`: the flags
    // are on the nets that take power and have no output (the input and the ground).
    let flagged: std::collections::BTreeSet<String> = {
        let root = Sexp::parse(&built.schematic).unwrap();
        let at = |n: &Sexp| {
            let a = n.find("at").unwrap().args();
            (
                a[0].text().unwrap().to_string(),
                a[1].text().unwrap().to_string(),
            )
        };
        root.find_all("symbol")
            .filter(|s| {
                s.find("lib_id").is_some_and(|l| {
                    l.args()[0]
                        .text()
                        .is_some_and(|id| id.ends_with(":PWR_FLAG"))
                })
            })
            .map(|s| {
                let label = root.find_all("label").find(|l| at(l) == at(s)).unwrap();
                label.args()[0].text().unwrap().to_string()
            })
            .collect()
    };
    let sw_net = built
        .design
        .nets
        .iter()
        .find(|n| n.pins.iter().any(|p| p.refdes == ic.refdes && p.pin == "8"))
        .expect("the switch pin is on a net");
    assert!(
        !flagged.contains(&sw_net.name),
        "the switch net `{}` is flagged: {flagged:?}",
        sw_net.name
    );
    assert!(flagged.contains("VIN"), "{flagged:?}");
}
