//! A project's offers files (ADR 0143, stage 2): `<supplier>.csv` in the folders of the
//! manifest's `offer_dirs` (default `["offers"]`), read after the standard offers, and those of
//! a dependency. Every part is invented; `example.invalid` is not a real address.

use std::fs;

use netsmith_fabricate::project::{Manifest, Project};
use netsmith_solve::Library;
use netsmith_testkit::TempDir;

/// A resistor of a project's catalog with no supplier number and no library of its own.
const ROWS: &str = "\
mpn,resistance,max_power,package,source
ZZR-316,31.6kohm +/- 1%,62.5mW,0402,https://example.invalid/zzr.pdf
";

const HEAD: &str = "manufacturer,mpn,sku,class,source,checked,notes\n";

fn offer(sku: &str, class: &str) -> String {
    format!("{HEAD},ZZR-316,{sku},{class},https://example.invalid/offer,2026-11-20,\n")
}

fn project(manifest: &str, files: &[(&str, &str)]) -> (TempDir, Result<Project, String>) {
    let root = TempDir::new("offer-files");
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"invented\"\n{manifest}"),
    )
    .unwrap();
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let project = Project::load(root.path());
    (root, project)
}

/// The supplier number and library the catalogs give the invented resistor's number.
fn bought(project: &Project, sku: &str) -> Option<Option<Library>> {
    let catalogs = project.catalogs().unwrap();
    catalogs.supplier(sku).map(|s| s.library)
}

#[test]
fn the_offers_folder_is_read_by_default() {
    let lcsc = offer("C900101", "Extended");
    let (_root, p) = project(
        "",
        &[
            ("catalogs/resistor.csv", ROWS),
            ("offers/lcsc.csv", &lcsc),
        ],
    );
    let p = p.unwrap();
    assert_eq!(p.manifest.offer_dirs, ["offers"]);
    let files = p.all_offer_files().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].supplier, "lcsc");
    assert_eq!(files[0].file, "offers/lcsc.csv");
    assert_eq!(bought(&p, "C900101"), Some(Some(Library::Extended)));
    assert!(p.catalogs().unwrap().offer_notes().is_empty());
}

#[test]
fn offer_dirs_names_other_folders_and_an_empty_list_reads_none() {
    let lcsc = offer("C900102", "");
    let files = [
        ("catalogs/resistor.csv", ROWS),
        ("supply/offers/lcsc.csv", lcsc.as_str()),
    ];
    let (_root, p) = project("offer_dirs = [\"supply/offers\"]\n", &files);
    let p = p.unwrap();
    assert_eq!(p.all_offer_files().unwrap()[0].file, "supply/offers/lcsc.csv");
    assert_eq!(bought(&p, "C900102"), Some(None));

    let (_root, p) = project("offer_dirs = []\n", &files);
    assert_eq!(bought(&p.unwrap(), "C900102"), None);
}

#[test]
fn a_file_not_named_for_a_supplier_is_refused() {
    let (_root, p) = project("", &[("offers/digikey.csv", HEAD)]);
    let e = p.unwrap().catalogs().unwrap_err();
    assert_eq!(
        e,
        "netsmith.toml: offers/digikey.csv in the offer_dirs folder \"offers\" is not an offers file: an offers file is named for its supplier, lcsc.csv. Rename it, or move it out of the folder (a folder's sub-folders are not read)"
    );
}

#[test]
fn a_jlcpcb_offers_file_is_refused_with_how_to_merge_it() {
    let (_root, p) = project("", &[("offers/jlcpcb.csv", HEAD)]);
    let e = p.unwrap().catalogs().unwrap_err();
    assert_eq!(
        e,
        "offers/jlcpcb.csv: JLCPCB's parts library uses LCSC's numbers, so its offers are the `lcsc` offers now (ADR 0143): merge the file into `lcsc.csv` beside it, the `class` of each of its offers into the `class` of the `lcsc` offer of that part (an `lcsc` offer whose `class` is empty is a part JLCPCB's library does not list), and remove it"
    );
}

#[test]
fn a_problem_in_an_offers_file_names_its_line() {
    let (_root, p) = project(
        "",
        &[(
            "offers/lcsc.csv",
            &format!("{HEAD},ZZR-316,C900103,Basic,,,\n"),
        )],
    );
    let e = p.unwrap().catalogs().unwrap_err();
    assert!(
        e.starts_with("offers/lcsc.csv:2: the offer of `ZZR-316` (sku `C900103`) has no source"),
        "{e}"
    );
}

#[test]
fn an_offer_of_a_part_in_no_catalog_is_a_note_not_an_error() {
    let (_root, p) = project("", &[("offers/lcsc.csv", &offer("C900104", ""))]);
    let notes = p.unwrap().catalogs().unwrap().offer_notes();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(
        notes[0].starts_with("offers/lcsc.csv:2: the lcsc offer of `ZZR-316` (sku `C900104`) is of a part that no catalog has a row of"),
        "{}",
        notes[0]
    );
}

#[test]
fn an_offer_dirs_entry_is_a_relative_folder_of_its_own() {
    let parse = |line: &str| Manifest::parse(&format!("[project]\nname = \"a\"\n{line}\n"));
    for line in ["offer_dirs = [\"/etc\"]", "offer_dirs = [\"\"]"] {
        let e = parse(line).unwrap_err();
        assert!(
            e.contains("offer_dirs") && e.contains("must be a folder relative to the project"),
            "{line}: {e}"
        );
    }
    let e = parse("offer_dirs = [\"offers\", \"./offers/\"]").unwrap_err();
    assert!(e.contains("offer_dirs lists the folder \"./offers/\" twice"), "{e}");
    let e = parse("offer_dirs = [\"catalogs\"]").unwrap_err();
    assert_eq!(
        e,
        "netsmith.toml: the folder \"catalogs\" is in offer_dirs and in catalog_dirs (\"catalogs\"); a folder holds catalogs or offers, not both: put the offers files in a folder of their own"
    );
    // The default is not written out, so a manifest that is written back gains no line.
    let m = parse("").unwrap();
    assert!(!m.render().contains("offer_dirs"), "{}", m.render());
    let other = parse("offer_dirs = [\"../shared/offers\"]").unwrap();
    assert_eq!(Manifest::parse(&other.render()).unwrap(), other);
}

#[test]
fn a_dependencys_offers_are_read_and_a_supplier_is_one_file() {
    let root = TempDir::new("offer-files-deps");
    let write = |path: &str, text: &str| {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    };
    write(
        "shared/netsmith.toml",
        "[project]\nname = \"shared\"\ntop_level = []\n",
    );
    write("shared/catalogs/resistor.csv", ROWS);
    write("shared/offers/lcsc.csv", &offer("C900105", "Basic"));
    write(
        "app/netsmith.toml",
        "[project]\nname = \"app\"\ntop_level = []\n\n[dependencies]\nshared = { path = \"../shared\" }\n",
    );
    let app = Project::load(&root.join("app")).unwrap();
    let files = app.all_offer_files().unwrap();
    assert_eq!(files[0].file, "../shared/offers/lcsc.csv");
    assert_eq!(bought(&app, "C900105"), Some(Some(Library::Basic)));

    write("app/offers/lcsc.csv", HEAD);
    let e = app.catalogs().unwrap_err();
    assert_eq!(
        e,
        "netsmith.toml: the offers of `lcsc` are in two files, offers/lcsc.csv and ../shared/offers/lcsc.csv; a supplier's offers are one file. Move the offers into one, and remove the other"
    );
}
