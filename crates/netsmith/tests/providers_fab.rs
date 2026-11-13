//! `netsmith fab` on a board that picks a provider of a generic (ADR 0068): the package is made
//! with the picked part's symbol and footprint, its bill of materials and placement name it, and
//! the provider that was passed over needs no mapping. The part and the numbers are invented.

use std::fs;

use netsmith::Libraries;
use netsmith::fab::{FabOptions, fab_with_options};
use netsmith_check::Loader;
use netsmith_kicad::install::skip_without_kicad_libraries;
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

const GENERIC: &str = "\
generic Tab {
    .designator &= \"U\"
    pins {
        .a passive
        .b passive
    }
    .span: Length
}
";

const TABS: &str = "\
import src::tab::Tab

/// An invented flat part, 30 x 26 mm, a pad at each end, soldered by hand.
part WideTab provides Tab {
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
    .span &= 24mm
    .assembly &= AssemblyMode::Hand
    .package: Package
}

/// A narrower one that the project has no symbol for.
part NarrowTab provides Tab {
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
    .span &= 10mm
    .assembly &= AssemblyMode::Hand
    .package: Package
}
";

const BOARD: &str = "\
import src::tab::Tab
import src::tabs::WideTab
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

board Widget {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .cell: Tab { .span >= 20mm }
    .row: Header1x2
    .row.p1 ~ .cell.a
    .row.p2 ~ .cell.b
}
";

const PHYSICAL: &str = "\
import std::pcb::Outline

physical for Widget {
    case {
        .outline: Outline {
            .width &= 60mm
            .height &= 40mm
        }
    }
    .row.place.x &= 30mm +/- 1mm
    .row.place.y &= 5mm +/- 1mm
    .row.place.side &= \"top\"
    .cell.place.side &= \"top\"
}
";

const KICAD_TOML: &str = "\
[[library]]
name = \"flat\"
footprints = \"libs/flat.pretty\"

[[part]]
kind = \"WideTab\"
package = \"Flat\"
default = true
symbol = \"Connector_Generic:Conn_01x02\"
footprint = \"flat:Flat\"

[part.pin_names]
A = \"Pin_1\"
B = \"Pin_2\"
";

const FOOTPRINT: &str = "(footprint \"Flat\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
     (property \"Reference\" \"REF**\" (at 0 -14 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
     (property \"Value\" \"Flat\" (at 0 14 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
     (attr smd) \
     (pad \"1\" smd rect (at -12 0) (size 2 2) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
     (pad \"2\" smd rect (at 12 0) (size 2 2) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
     (fp_rect (start -14.5 -12.5) (end 14.5 12.5) (stroke (width 0.1) (type solid)) (fill none) (layer \"F.Fab\")) \
     (fp_rect (start -15 -13) (end 15 13) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n";

#[test]
fn fab_makes_the_package_with_the_picked_provider_and_needs_no_mapping_for_the_other() {
    if skip_without_kicad_libraries() {
        return;
    }
    let dir = TempDir::unused("providers-fab");
    let project = dir.join("project");
    fs::create_dir_all(project.join("libs/flat.pretty")).unwrap();
    fs::write(project.join("kicad.toml"), KICAD_TOML).unwrap();
    fs::write(project.join("libs/flat.pretty/Flat.kicad_mod"), FOOTPRINT).unwrap();
    let libraries = Libraries::for_project(Some(&project)).unwrap_or_else(|e| panic!("{e}"));
    let mut loader = Loader::std().with_physical("widget.nsm", "widget.physical.nsm", PHYSICAL);
    loader.insert("src::tab", "src/tab.nsm", GENERIC);
    // The narrow provider is first by name in its file, so it is tried first and passed over.
    loader.insert("src::tabs", "src/tabs.nsm", TABS);
    let made = fab_with_options(
        "widget.nsm",
        BOARD,
        &loader,
        &dir.join("fab"),
        &Catalogs::jlcpcb(),
        &libraries,
        None,
        &FabOptions::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(made.len(), 1);
    let notes = &made[0].notes;
    assert!(notes.contains("design rules check: passed"), "{notes}");
    let out = dir.join("fab/Widget");
    let bom = fs::read_to_string(out.join("bom.csv")).unwrap_or_default();
    let positions = fs::read_dir(&out)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let board = fs::read_to_string(out.join("Widget.kicad_pcb")).unwrap();
    assert!(
        board.contains("flat:Flat") || board.contains("\"Flat\""),
        "{positions:?}"
    );
    assert!(board.contains("\"U1\""), "{positions:?}\n{bom}");
}

#[test]
fn a_provider_with_no_mapping_that_is_picked_fails_at_export_naming_it() {
    if skip_without_kicad_libraries() {
        return;
    }
    let dir = TempDir::unused("providers-fab-unmapped");
    let project = dir.join("project");
    fs::create_dir_all(&project).unwrap();
    let libraries = Libraries::for_project(Some(&project)).unwrap_or_else(|e| panic!("{e}"));
    let mut loader = Loader::std().with_physical("widget.nsm", "widget.physical.nsm", PHYSICAL);
    loader.insert("src::tab", "src/tab.nsm", GENERIC);
    loader.insert("src::tabs", "src/tabs.nsm", TABS);
    let e = fab_with_options(
        "widget.nsm",
        BOARD,
        &loader,
        &dir.join("fab"),
        &Catalogs::jlcpcb(),
        &libraries,
        None,
        &FabOptions::default(),
    )
    .expect_err("the picked part has no symbol");
    assert!(e.contains("WideTab"), "{e}");
}
