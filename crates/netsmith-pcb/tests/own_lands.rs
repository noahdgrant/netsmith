//! netsmith's own lands (issue 1399): the footprints of `std/libs/netsmith.pretty/` that a
//! generator writes, the tapped supercapacitor can and the coin cell with tabs, which KiCad has
//! no footprint of. Each committed file is what its generator writes, so a change of the
//! generator changes the file in the same commit: `NETSMITH_UPDATE_FIXTURES=1 cargo test -p
//! netsmith-pcb --test main own_lands::` writes them.

use std::path::Path;

use netsmith_pcb::{FootprintLibrary, own_footprint_files};

#[test]
fn each_own_land_file_is_what_its_generator_writes() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std/libs/netsmith.pretty");
    let update = std::env::var_os("NETSMITH_UPDATE_FIXTURES").is_some();
    let files = own_footprint_files();
    assert_eq!(files.len(), 2);
    for (name, text) in files {
        let path = folder.join(format!("{name}.kicad_mod"));
        if update {
            std::fs::write(&path, &text).unwrap();
            continue;
        }
        let committed = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(
            committed, text,
            "{} is not what its generator writes: NETSMITH_UPDATE_FIXTURES=1 cargo test -p netsmith-pcb --test main own_lands::",
            path.display()
        );
    }
}

#[test]
fn the_own_lands_load_as_netsmith_footprints_with_no_kicad() {
    let library = FootprintLibrary::at("/nonexistent/footprints");
    let tap = library
        .load("netsmith:CP_Radial_D16.0mm_P7.50mm_Tap")
        .unwrap();
    assert_eq!(tap.pad_numbers(), ["1", "2", "3"]);
    let coin = library
        .load("netsmith:BatteryCell_Coin_D20.0mm_Tabs_P15.00mm")
        .unwrap();
    assert_eq!(coin.pad_numbers(), ["1", "2"]);
}
