//! A board's modules as the sheets of its KiCad schematic (issue #563), on a board of three
//! instances of one module (two the same circuit, one with another resistor) and a module of
//! two more inside a module. The files, the instance paths and the board's links to them are
//! read as text with no KiCad; the netlist and the electrical rules check run `kicad-cli` and
//! skip where it is not installed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use netsmith::{Built, Libraries, build_with};
use netsmith_check::Loader;
use netsmith_fab::{KicadCli, KicadCliExe};
use netsmith_kicad::sexp::Sexp;
use netsmith_netlist::{Identity, Netlist, Options, Outcome, compare};
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

const SHEETS: &str = include_str!("fixtures/module_sheets.nsm");

fn built() -> Built {
    build_with(
        "module_sheets.nsm",
        SHEETS,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0)
}

fn property(node: &Sexp, name: &str) -> String {
    node.find_all("property")
        .find(|p| p.args()[0].text() == Some(name))
        .and_then(|p| p.args()[1].text())
        .unwrap_or_else(|| panic!("no property {name}"))
        .to_string()
}

fn uuid(node: &Sexp) -> String {
    node.find("uuid").unwrap().args()[0]
        .text()
        .unwrap()
        .to_string()
}

/// Each sheet symbol of the root by its UUID: its name, its file and the names of its pins.
fn sheet_symbols(root: &Sexp) -> BTreeMap<String, (String, String, BTreeSet<String>)> {
    root.find_all("sheet")
        .map(|s| {
            let pins = s
                .find_all("pin")
                .map(|p| p.args()[0].text().unwrap().to_string())
                .collect();
            (
                uuid(s),
                (property(s, "Sheetname"), property(s, "Sheetfile"), pins),
            )
        })
        .collect()
}

/// Every placed symbol of a sheet as `(symbol uuid, instance path, reference)`, one for each
/// instance of the sheet.
fn placed(sheet: &Sexp) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for symbol in sheet.find_all("symbol") {
        let project = symbol.find("instances").unwrap().find("project").unwrap();
        for path in project.find_all("path") {
            out.push((
                uuid(symbol),
                path.args()[0].text().unwrap().to_string(),
                path.find("reference").unwrap().args()[0]
                    .text()
                    .unwrap()
                    .to_string(),
            ));
        }
    }
    out
}

#[test]
fn each_module_of_the_board_is_a_sheet_and_each_part_is_placed_once() {
    let b = built();
    let files: Vec<&str> = b.sheets.iter().map(|(n, _)| n.as_str()).collect();
    // `a` and `b` are one circuit, `c` has another resistor, and the pair's two lights are
    // drawn flat in its sheet.
    assert_eq!(
        files,
        ["Light.kicad_sch", "Light_2.kicad_sch", "Pair.kicad_sch"]
    );
    let root = Sexp::parse(&b.schematic).unwrap();
    let root_uuid = uuid(&root);
    let symbols = sheet_symbols(&root);
    let named: BTreeMap<&str, &str> = symbols
        .values()
        .map(|(name, file, _)| (name.as_str(), file.as_str()))
        .collect();
    assert_eq!(
        named,
        BTreeMap::from([
            ("a", "Light.kicad_sch"),
            ("b", "Light.kicad_sch"),
            ("c", "Light_2.kicad_sch"),
            ("pair", "Pair.kicad_sch"),
        ])
    );

    // Every part of the design is placed once, with its own reference, on the root (a part of
    // the board) or in the instance of the sheet of the module it is in.
    let path_of: BTreeMap<&str, &str> = b
        .design
        .parts
        .iter()
        .map(|p| (p.refdes.as_str(), p.path.as_str()))
        .collect();
    let mut seen: Vec<String> = Vec::new();
    for (_, at, reference) in placed(&root) {
        assert_eq!(at, format!("/{root_uuid}"));
        if !reference.starts_with('#') {
            assert!(!path_of[reference.as_str()].contains('.'), "{reference}");
            seen.push(reference);
        }
    }
    for (file, text) in &b.sheets {
        let sheet = Sexp::parse(text).unwrap();
        // The pins of each sheet symbol of the file are the file's hierarchical labels.
        let labels: BTreeSet<String> = sheet
            .find_all("hierarchical_label")
            .map(|l| l.args()[0].text().unwrap().to_string())
            .collect();
        for (sheet_uuid, (name, _, pins)) in symbols.iter().filter(|(_, s)| &s.1 == file) {
            assert_eq!(pins, &labels, "{file} as {name}");
            for (_, at, reference) in placed(&sheet)
                .into_iter()
                .filter(|(_, at, _)| *at == format!("/{root_uuid}/{sheet_uuid}"))
            {
                assert_eq!(at, format!("/{root_uuid}/{sheet_uuid}"));
                if !reference.starts_with('#') {
                    assert!(
                        path_of[reference.as_str()].starts_with(&format!("{name}.")),
                        "{reference} in {name}"
                    );
                    seen.push(reference);
                }
            }
        }
    }
    seen.sort();
    let mut all: Vec<String> = b.design.parts.iter().map(|p| p.refdes.clone()).collect();
    all.sort();
    assert_eq!(seen, all);
}

#[test]
fn each_footprint_names_its_symbol_and_the_sheet_it_is_on() {
    let b = built();
    let libraries = Libraries::sample();
    let board = netsmith_place_route::board_from_design(
        &b.design,
        &libraries.mapping,
        &libraries.footprints,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let pcb = Sexp::parse(&netsmith_pcb::kicad_pcb(&board)).unwrap();
    let root = Sexp::parse(&b.schematic).unwrap();
    let root_uuid = uuid(&root);
    let symbols = sheet_symbols(&root);
    let files: BTreeMap<&str, Sexp> = b
        .sheets
        .iter()
        .map(|(n, t)| (n.as_str(), Sexp::parse(t).unwrap()))
        .collect();
    let mut linked = 0;
    for footprint in pcb.find_all("footprint") {
        let reference = property(footprint, "Reference");
        let text = |head: &str| footprint.find(head).unwrap().args()[0].text().unwrap();
        let (path, sheet_name, sheet_file) = (text("path"), text("sheetname"), text("sheetfile"));
        let parts: Vec<&str> = path.split('/').collect();
        match parts[..] {
            ["", symbol] => {
                assert_eq!((sheet_name, sheet_file), ("/", "Sheets.kicad_sch"));
                assert!(
                    placed(&root).contains(&(
                        symbol.to_string(),
                        format!("/{root_uuid}"),
                        reference.clone()
                    )),
                    "{reference}: {path}"
                );
            }
            ["", sheet, symbol] => {
                let (name, file, _) = &symbols[sheet];
                assert_eq!(sheet_name, format!("/{name}/"));
                assert_eq!(sheet_file, file);
                assert!(
                    placed(&files[file.as_str()]).contains(&(
                        symbol.to_string(),
                        format!("/{root_uuid}/{sheet}"),
                        reference.clone()
                    )),
                    "{reference}: {path}"
                );
            }
            _ => panic!("{reference}: {path}"),
        }
        // The member path stays beside the link.
        assert!(!property(footprint, "netsmith_path").is_empty());
        linked += 1;
    }
    assert_eq!(linked, b.design.parts.len());
}

/// KiCad, or `None` (the test skips) when it is not installed or cannot be started here.
fn kicad() -> Option<KicadCliExe> {
    let cli = KicadCliExe::locate_runnable()?;
    match cli.interop() {
        Ok(()) => Some(cli),
        Err(e) => {
            eprintln!("skipping: {e}");
            None
        }
    }
}

fn write(dir: &Path, name: &str, root: &str, sheets: &[(String, String)]) -> std::path::PathBuf {
    let sch = dir.join(format!("{name}.kicad_sch"));
    std::fs::write(&sch, root).unwrap();
    for (file, text) in sheets {
        std::fs::write(dir.join(file), text).unwrap();
    }
    std::fs::write(
        dir.join(format!("{name}.kicad_pro")),
        netsmith_kicad::project_file(name),
    )
    .unwrap();
    sch
}

#[test]
fn kicads_netlist_of_the_sheets_is_the_netlist_of_one_sheet() {
    let b = built();
    let libraries = Libraries::sample();
    let Some(from_sheets) = sheets_are_one_sheet_to_kicad(&b, &libraries) else {
        return;
    };
    // And the design's own netlist.
    let from_design = netsmith::compare::netlist_of_design(&b.design, &libraries.mapping).unwrap();
    let options = Options {
        identity: Identity::ByRefdes,
        ..Options::default()
    };
    match compare(&from_sheets, &from_design, &options).unwrap() {
        Outcome::Match(_) => {}
        different => panic!("the sheets and the design differ: {different:?}"),
    }
}

/// KiCad's netlist of a built board's sheets, after checking that it is that of the same design
/// drawn as one sheet, part for part by designator; `None` where KiCad is not here.
pub(crate) fn sheets_are_one_sheet_to_kicad(b: &Built, libraries: &Libraries) -> Option<Netlist> {
    let kicad = kicad()?;
    assert!(
        !b.sheets.is_empty(),
        "{} has no module sheet",
        b.design.name
    );
    let hierarchy = TempDir::new("module-sheets-hierarchy");
    let sheets = write(&hierarchy, &b.design.name, &b.schematic, &b.sheets);
    // The same design with no module is drawn as one sheet.
    let mut flat_design = b.design.clone();
    flat_design.modules.clear();
    let flat_schematic =
        netsmith_kicad::emit_schematic(&flat_design, &libraries.mapping, &libraries.symbols)
            .unwrap();
    assert!(flat_schematic.sheets.is_empty());
    let flat = TempDir::new("module-sheets-flat");
    let one = write(&flat, &b.design.name, &flat_schematic.root, &[]);

    let from_sheets = Netlist::from_kicad_schematic(&sheets, |p| kicad.netlist(p)).unwrap();
    let from_one = Netlist::from_kicad_schematic(&one, |p| kicad.netlist(p)).unwrap();
    // The same designators: every part keeps its reference on its sheet.
    let options = Options {
        identity: Identity::ByRefdes,
        ..Options::default()
    };
    match compare(&from_sheets, &from_one, &options).unwrap() {
        Outcome::Match(_) => {}
        different => panic!("the sheets and one sheet differ: {different:?}"),
    }
    Some(from_sheets)
}

#[test]
fn kicads_erc_accepts_the_sheets() {
    let b = built();
    let Some(found) = crate::erc::erc_built(&b) else {
        return;
    };
    let found: Vec<String> = found
        .into_iter()
        .filter(|v| !v.starts_with("lib_symbol") && !v.starts_with("footprint_link"))
        .collect();
    assert_eq!(found, Vec::<String>::new());
}

/// The `.kicad_sch` files of a folder, by name.
fn sch_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".kicad_sch"))
        .collect();
    names.sort();
    names
}

/// Issue #575: a build removes the sheets an earlier build wrote there and no longer writes (a
/// module renamed), recognised by the `generator` netsmith writes in each sheet's header, and
/// leaves every other `.kicad_sch`, a person's own or one KiCad saved.
#[test]
fn a_build_removes_the_sheets_of_a_renamed_module_and_no_other_file() {
    let out = TempDir::new("sheets_renamed");
    let build_of = |src: &str| {
        build_with(
            "module_sheets.nsm",
            src,
            &Loader::std(),
            &Catalogs::jlcpcb(),
            &Libraries::sample(),
        )
        .unwrap_or_else(|e| panic!("{e}"))
        .remove(0)
    };
    let first = build_of(SHEETS);
    let dir = first.write(out.path()).unwrap();
    assert_eq!(
        sch_files(&dir),
        [
            "Light.kicad_sch",
            "Light_2.kicad_sch",
            "Pair.kicad_sch",
            "Sheets.kicad_sch"
        ]
    );

    // A person's sheet, one KiCad saved from a netsmith sheet, and one that is no text.
    std::fs::write(
        dir.join("Mine.kicad_sch"),
        "(kicad_sch (version 20250114) (generator \"someone\"))\n",
    )
    .unwrap();
    let saved = std::fs::read_to_string(dir.join("Pair.kicad_sch"))
        .unwrap()
        .replace("(generator \"netsmith\")", "(generator \"eeschema\")");
    std::fs::write(dir.join("Saved.kicad_sch"), saved).unwrap();
    std::fs::write(dir.join("Binary.kicad_sch"), [0xff, 0xfe, 0x00]).unwrap();
    std::fs::write(dir.join("notes.txt"), "keep").unwrap();

    let renamed = SHEETS
        .replace("module Light", "module Lamp")
        .replace(": Light", ": Lamp");
    let second = build_of(&renamed);
    assert_eq!(second.write(out.path()).unwrap(), dir);
    assert_eq!(
        sch_files(&dir),
        [
            "Binary.kicad_sch",
            "Lamp.kicad_sch",
            "Lamp_2.kicad_sch",
            "Mine.kicad_sch",
            "Pair.kicad_sch",
            "Saved.kicad_sch",
            "Sheets.kicad_sch"
        ]
    );
    assert!(dir.join("notes.txt").exists());
}
