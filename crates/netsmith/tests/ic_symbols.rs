//! The symbol of an IC is drawn by netsmith from its pin table, so a board with one is written
//! without a KiCad symbol of the IC (ADR 0097, step 1). The board is the LT8641 stage of
//! `lt8641.rs`, whose row in `std/kicad.toml` names no symbol.

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
    let name = "netsmith:Lt8641_UDC20_18";
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
    assert!(library.contains("(symbol \"Lt8641_UDC20_18\""), "{library}");
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
