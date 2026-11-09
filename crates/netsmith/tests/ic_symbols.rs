//! The symbol of an IC is drawn by netsmith from its pin table, so a board with one is written
//! without the KiCad symbol library of the IC (ADR 0097, step 1).

use netsmith::{Libraries, build_with};
use netsmith_check::Loader;
use netsmith_kicad::testlib::{self, TempDir};
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_pcb::FootprintLibrary;
use netsmith_solve::Catalogs;

/// Our sample libraries without the symbol libraries of the ICs the standard mapping draws
/// itself: `Regulator_Linear` and `74xGxx` are not there, as on a machine with no KiCad.
fn libraries_without_ic_symbols(dir: &TempDir) -> Libraries {
    let symbols = dir.path().join("symbols");
    std::fs::create_dir_all(&symbols).unwrap();
    for entry in std::fs::read_dir(testlib::sample_symbols()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.starts_with("Regulator_Linear") || name.starts_with("74xGxx") {
            continue;
        }
        std::fs::copy(&path, symbols.join(&name)).unwrap();
    }
    assert!(!symbols.join("Regulator_Linear.kicad_sym").exists());
    Libraries::with_mapping(
        Mapping::standard(),
        SymbolLib::at(symbols),
        FootprintLibrary::at(testlib::sample_footprints()).with_generators(),
    )
}

fn regulator_board(libraries: &Libraries) -> netsmith::Built {
    let path = format!(
        "{}/../../examples/linear_regulator.nsm",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap();
    build_with(&path, &text, &Loader::std(), &Catalogs::jlcpcb(), libraries)
        .unwrap_or_else(|e| panic!("{e}"))
        .remove(0)
}

#[test]
fn a_board_with_an_ic_writes_its_schematic_with_no_symbol_library_of_the_ic() {
    let dir = TempDir::new("ic-symbols");
    let libraries = libraries_without_ic_symbols(&dir);
    let built = regulator_board(&libraries);

    // The regulator is drawn with a symbol of netsmith's own, which the schematic holds ...
    let ic = built
        .design
        .parts
        .iter()
        .find(|p| p.kind == "LinearRegulator")
        .unwrap();
    let name = format!(
        "netsmith:LinearRegulator_{}",
        ic.package.as_deref().unwrap()
    );
    assert!(
        built.schematic.contains(&format!("(lib_id \"{name}\")")),
        "no instance of {name}"
    );
    assert!(built.schematic.contains(&format!("(symbol \"{name}\"")));
    assert!(!built.schematic.contains("Regulator_Linear"));
    // ... with a pin for each pad of the table, named as the datasheet does.
    for pin in &ic.pins {
        assert!(
            built
                .schematic
                .contains(&format!("(name \"{}\"", pin.datasheet)),
            "no pin {}",
            pin.datasheet
        );
    }

    // The same bytes on a second run.
    assert_eq!(built.schematic, regulator_board(&libraries).schematic);

    // The library with the symbol is written beside the schematic and listed in the table,
    // and holds the symbol the schematic holds.
    let out = TempDir::new("ic-symbols-export");
    let folder = built.write(out.path()).unwrap();
    libraries.write_tables(&folder, &built.design).unwrap();
    let library = std::fs::read_to_string(folder.join("netsmith.kicad_sym")).unwrap();
    assert!(
        library.contains(&format!(
            "(symbol \"{}\"",
            name.strip_prefix("netsmith:").unwrap()
        )),
        "{library}"
    );
    let table = std::fs::read_to_string(folder.join("sym-lib-table")).unwrap();
    assert!(table.contains("${KIPRJMOD}/netsmith.kicad_sym"), "{table}");
}

#[test]
fn a_part_that_names_a_kicad_symbol_is_still_drawn_with_it() {
    let dir = TempDir::new("ic-symbols-named");
    let libraries = libraries_without_ic_symbols(&dir);
    let built = regulator_board(&libraries);
    assert!(built.schematic.contains("(lib_id \"Device:C\")"));
}
