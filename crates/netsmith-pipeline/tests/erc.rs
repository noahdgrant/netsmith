//! KiCad's own electrical rules check on the schematics netsmith writes. These run
//! `kicad-cli` (found as `KicadCliExe::locate` does: `$KICAD_CLI`, `PATH`, Windows KiCad
//! under WSL) and are skipped where it is not installed; the first one also needs KiCad's
//! symbol libraries.


use netsmith_pipeline::{Libraries, build_with};
use netsmith_check::Loader;
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

use crate::common;
use crate::common::erc::{erc, erc_built, erc_sheets, with_the_machines_libraries};

fn example(name: &str) -> (String, String) {
    let path = format!("{}/../../examples/{name}.nsm", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap();
    (path, text)
}

#[test]
fn every_example_passes_erc_with_kicads_own_symbols() {
    let libraries = Libraries::standard();
    if !libraries.symbols.root().is_dir() {
        eprintln!(
            "skipped: no KiCad symbols at {}",
            libraries.symbols.root().display()
        );
        return;
    }
    for name in [
        "led",
        "linear_regulator",
        "status_indicator",
        "four_layer",
        "two_boards",
    ] {
        let (path, text) = example(name);
        let built = build_with(
            &path,
            &text,
            &Loader::std(),
            &Catalogs::jlcpcb(),
            &libraries,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        for b in built {
            let Some(found) = erc_built(&b) else {
                return;
            };
            assert_eq!(
                with_the_machines_libraries(found),
                Vec::<String>::new(),
                "{name} / {}",
                b.label()
            );
        }
    }
}

#[test]
fn a_multi_unit_part_from_a_project_library_passes_erc() {
    for (name, source) in [
        ("dual", common::DUAL_BOARD),
        ("shared", common::SHARED_BOARD),
    ] {
        passes_erc(name, source);
    }
}

fn passes_erc(name: &str, source: &str) {
    let dir = TempDir::new("erc-project");
    let libraries = common::project_libraries(dir.path());
    let built = build_with(
        "t.nsm",
        source,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let Some(found) = erc_sheets(name, &built[0].schematic, &built[0].sheets) else {
        return;
    };
    // KiCad here has no library table for `acme`, so it says it cannot compare the embedded
    // symbols with their libraries. That is about the libraries, not the circuit.
    assert_eq!(electrical(found), Vec::<String>::new());
}

/// The violations of ERC that are about the circuit: the ones about library tables are left
/// out, because KiCad here has no table for the invented libraries or for our sample copies
/// of its own.
fn electrical(found: Vec<String>) -> Vec<String> {
    found
        .into_iter()
        .filter(|v| !v.starts_with("lib_symbol") && !v.starts_with("footprint_link"))
        .collect()
}

fn chip_passes_erc(name: &str, source: &str) {
    let dir = TempDir::new("erc-chip");
    let libraries = common::chip_libraries(
        dir.path(),
        &common::chip_symbol(&common::CHIP_PINS),
        &common::chip_footprint(&["1", "2", "3", "4", "5", "6"]),
    );
    let built = build_with(
        "t.nsm",
        &common::chip_source_of(source),
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let Some(found) = erc_sheets(name, &built[0].schematic, &built[0].sheets) else {
        return;
    };
    assert_eq!(electrical(found), Vec::<String>::new());
}

#[test]
fn pins_the_symbol_types_no_connect_pass_erc_without_a_flag_or_a_label() {
    // Two `NC` pins on a six-pin IC: neither is flagged (KiCad counts a flag on such a pin
    // as a connection) and neither is labelled.
    chip_passes_erc("chip-nc", common::CHIP_BOARD);
}

#[test]
fn a_net_with_a_single_pin_passes_erc_without_a_label() {
    // `VOUT` has the chip's output pin and nothing else on it.
    chip_passes_erc("chip-rails", common::CHIP_RAIL_BOARD);
}

/// The schematic of a board of one part `Probe`, drawn from the pin table `pins` (a body of
/// `.name kind "NAME" "pad"...` lines) in a `gen:SOT23(pins=<pad count>)` footprint, with no
/// KiCad library and every pin left on no net.
fn probe_schematic(pad_count: usize, pins: &str) -> String {
    use netsmith_kicad::{Search};

    let dir = TempDir::new("erc-probe");
    let empty = TempDir::new("erc-probe-no-kicad");
    std::fs::write(
        dir.join("kicad.toml"),
        format!(
            "[[part]]\nkind = \"Probe\"\npackage = \"SOT-23\"\ndefault = true\n\
             footprint = \"gen:SOT23(pins={pad_count})\"\n"
        ),
    )
    .unwrap();
    let source = format!(
        "part Probe {{\n    .package: Package\n    pins \"SOT-23\" {{\n{pins}\n    }}\n}}\n\nboard B {{\n    .u: Probe {{ .package &= \"SOT-23\" }}\n}}\n"
    );
    let libraries = Libraries::for_project_by(Some(dir.path()), &Search::under(empty.path()))
        .unwrap_or_else(|e| panic!("{e}"));
    build_with(
        "t.nsm",
        &source,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0)
    .schematic
}

/// The probe's schematic has `markers` no-connect markers, no two on one point, and passes
/// KiCad's ERC (where there is KiCad).
fn probe_passes_erc(name: &str, pad_count: usize, pins: &str, markers: usize) {
    let schematic = probe_schematic(pad_count, pins);
    let root = netsmith_kicad::Sexp::parse(&schematic).unwrap();
    let mut points: Vec<String> = root
        .find_all("no_connect")
        .map(|m| {
            let at = m.find("at").unwrap().args();
            format!("{} {}", at[0].text().unwrap(), at[1].text().unwrap())
        })
        .collect();
    points.sort();
    let all = points.len();
    points.dedup();
    assert_eq!(
        (all, points.len()),
        (markers, markers),
        "one marker for each point: {points:?}"
    );
    let Some(found) = erc(name, &schematic) else {
        return;
    };
    assert_eq!(electrical(found), Vec::<String>::new());
}

// KiCad's ERC says `no_connect_connected` of a `no_connect` pad that has another pad on its point,
// with a marker or without one, so the pads of a pin of several pads are one stack of `passive`
// pins and the point has one marker (a lone `no_connect` pad has none).

#[test]
fn a_part_that_is_one_no_connect_pin_of_several_pads_passes_erc() {
    probe_passes_erc(
        "probe-nc-only",
        3,
        "        .nc no_connect \"NC\" \"1\" \"2\" \"3\"",
        1,
    );
}

#[test]
fn a_no_connect_pin_of_several_pads_on_a_part_with_other_pins_passes_erc() {
    // The supply, the ground and the lone `no_connect` pad each have a marker or none.
    probe_passes_erc(
        "probe-nc-and-others",
        6,
        "        .vin power \"VIN\" \"1\" optional\n        .gnd power \"GND\" \"2\" optional\n        \
         .nc no_connect \"NC\" \"3\" \"4\" \"5\"\n        .spare no_connect \"NC\" \"6\"",
        3,
    );
}

#[test]
fn pins_of_several_pads_of_other_kinds_still_pass_erc() {
    // A ground and an unused output of two pads each: one stack, one marker each.
    probe_passes_erc(
        "probe-stacks",
        5,
        "        .vin power \"VIN\" \"1\" optional\n        .gnd power \"GND\" \"2\" \"3\" optional\n        \
         .out output \"OUT\" \"4\" \"5\"",
        3,
    );
}
