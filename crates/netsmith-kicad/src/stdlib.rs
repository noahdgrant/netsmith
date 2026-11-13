//! netsmith's own standard library of KiCad files: footprints (and the symbols that go with
//! them) of packages that KiCad's libraries do not have and the generators of ADR 0043 do not
//! make (ADR 0087).
//!
//! The files are in `std/libs/` as `.kicad_mod` and `.kicad_sym` text, compiled into the
//! binary like `std/kicad.toml`, and answer to the library name [`NAME`]: a row of
//! `std/kicad.toml` names `netsmith:<Footprint>` as it names `Package_SO:<Footprint>`. They are
//! found before anything is read from a KiCad install, on every machine, and a project cannot
//! list a library of the same name. A board that uses one gets the library written into its
//! output folder and listed in the project's library tables, as a project's own libraries are.

use std::sync::Arc;

use crate::sexp::Sexp;
use crate::symbols::Symbol;

/// The library name of the files in `std/libs/`: `netsmith:Footprint`, `netsmith:Symbol`.
pub const NAME: &str = "netsmith";

/// The symbol library, where it is in the repository (for messages).
pub(crate) const SYMBOL_FILE: &str = "std/libs/netsmith.kicad_sym";

/// The `.kicad_sym` of the library.
pub(crate) const SYMBOLS: &str = include_str!("../../../std/libs/netsmith.kicad_sym");

/// The folder that holds the footprints, in the output of a board and in the repository.
pub const FOOTPRINT_FOLDER: &str = "netsmith.pretty";

/// Every footprint: its name and the text of its `.kicad_mod`. A new file of
/// `std/libs/netsmith.pretty/` is listed here (a test fails until it is).
const FOOTPRINTS: &[(&str, &str)] = &[
    (
        "SMA_Amphenol_132134_Vertical",
        include_str!("../../../std/libs/netsmith.pretty/SMA_Amphenol_132134_Vertical.kicad_mod"),
    ),
    (
        "SW_SPST_CK_RS-282G05A3-SM",
        include_str!("../../../std/libs/netsmith.pretty/SW_SPST_CK_RS-282G05A3-SM.kicad_mod"),
    ),
    (
        "SW_SPST_Omron_B3U-1000P",
        include_str!("../../../std/libs/netsmith.pretty/SW_SPST_Omron_B3U-1000P.kicad_mod"),
    ),
    (
        "Tag-Connect_TC2030-IDC-NL_2x03_P1.27mm_Vertical",
        include_str!(
            "../../../std/libs/netsmith.pretty/Tag-Connect_TC2030-IDC-NL_2x03_P1.27mm_Vertical.kicad_mod"
        ),
    ),
    (
        "UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA",
        include_str!(
            "../../../std/libs/netsmith.pretty/UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA.kicad_mod"
        ),
    ),
    (
        "USB_C_Receptacle_GCT_USB4105_16P_TopMnt_Horizontal",
        include_str!(
            "../../../std/libs/netsmith.pretty/USB_C_Receptacle_GCT_USB4105_16P_TopMnt_Horizontal.kicad_mod"
        ),
    ),
    (
        "USB_C_Receptacle_GCT_USB4125_6P_TopMnt_Horizontal",
        include_str!(
            "../../../std/libs/netsmith.pretty/USB_C_Receptacle_GCT_USB4125_6P_TopMnt_Horizontal.kicad_mod"
        ),
    ),
    (
        "USB_C_Receptacle_HRO_TYPE-C-31-M-12",
        include_str!(
            "../../../std/libs/netsmith.pretty/USB_C_Receptacle_HRO_TYPE-C-31-M-12.kicad_mod"
        ),
    ),
];

/// The names of the symbols of the library's own file (`std/libs/netsmith.kicad_sym`), which a
/// board's library holds whatever its parts are.
pub(crate) fn file_symbol_names() -> &'static [String] {
    static NAMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        Sexp::parse(SYMBOLS)
            .expect("std/libs/netsmith.kicad_sym is valid (a test reads it)")
            .find_all("symbol")
            .filter_map(|s| s.args().first().and_then(Sexp::text))
            .map(|name| format!("{NAME}:{name}"))
            .collect()
    })
}

/// The names of the footprints, in a fixed order.
pub fn footprint_names() -> impl Iterator<Item = &'static str> {
    FOOTPRINTS.iter().map(|(name, _)| *name)
}

/// The text of the `.kicad_mod` named `name` (without the library).
pub fn footprint(name: &str) -> Option<&'static str> {
    FOOTPRINTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, text)| *text)
}

/// Whether the id `Library:Name` is in this library.
pub fn is_library_id(id: &str) -> bool {
    id.split_once(':')
        .is_some_and(|(library, _)| library == NAME)
}

/// The `.kicad_sym` of the library as a board's output folder holds it: the files' own
/// symbols and the `generated` ones, the symbols drawn from the pin tables of the board's
/// parts ([`crate::generate`], ADR 0097), in the order of their names. A symbol of the board
/// is the same in the schematic and here, so KiCad finds it in the library its table lists.
pub(crate) fn symbol_library(generated: &[Arc<Symbol>]) -> String {
    let mut own =
        Sexp::parse(SYMBOLS).expect("std/libs/netsmith.kicad_sym is valid (a test reads it)");
    if let Sexp::List(items) = &mut own {
        let mut drawn: Vec<&Arc<Symbol>> = generated.iter().collect();
        drawn.sort_by(|a, b| a.lib_id().cmp(b.lib_id()));
        drawn.dedup_by(|a, b| a.lib_id() == b.lib_id());
        items.extend(drawn.into_iter().map(|s| s.library_definition()));
    }
    own.to_kicad_string()
}

/// The files of the library as a board's output folder holds them: the path under the
/// folder and the text. The tables of [`crate::library_tables`] point at them. `generated`
/// is as for [`symbol_library`].
pub fn files(generated: &[Arc<Symbol>]) -> Vec<(String, String)> {
    let mut files = vec![(format!("{NAME}.kicad_sym"), symbol_library(generated))];
    files.extend(FOOTPRINTS.iter().map(|(name, text)| {
        (
            format!("{FOOTPRINT_FOLDER}/{name}.kicad_mod"),
            (*text).to_string(),
        )
    }));
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_of_the_folder_is_listed_and_every_listed_file_is_in_the_folder() {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../std/libs")
            .join(FOOTPRINT_FOLDER);
        let mut on_disk: Vec<String> = std::fs::read_dir(&folder)
            .unwrap_or_else(|e| panic!("{}: {e}", folder.display()))
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        on_disk.sort();
        let mut listed: Vec<String> = footprint_names()
            .map(|n| format!("{n}.kicad_mod"))
            .collect();
        listed.sort();
        assert_eq!(
            on_disk, listed,
            "std/libs/netsmith.pretty and the list in stdlib.rs differ"
        );
    }

    #[test]
    fn a_footprint_is_found_by_its_name_and_the_files_are_those_of_the_library() {
        let name = footprint_names().next().unwrap();
        assert!(
            footprint(name)
                .unwrap()
                .starts_with(&format!("(footprint \"{name}\""))
        );
        assert_eq!(footprint("Nothing"), None);
        assert!(is_library_id(&format!("{NAME}:{name}")));
        assert!(!is_library_id("Package_SO:SOIC-8_3.9x4.9mm_P1.27mm"));
        let files = files(&[]);
        assert_eq!(files[0].0, "netsmith.kicad_sym");
        assert!(files[1].0.starts_with("netsmith.pretty/"));
        assert_eq!(files.len(), 1 + footprint_names().count());
    }
}
