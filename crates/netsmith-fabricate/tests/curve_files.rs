//! A project's curve files (issue #1260): `curves/<catalog>/<row>.csv` in a `catalog_dirs`
//! folder or beside a file `catalogs` lists, read with the catalogs. Every part is invented; its
//! 77 V rating is one no standard row has, so the board gets it.

use std::fs;

use netsmith_check::{Domain, Loader, check};
use netsmith_fabricate::project::Project;
use netsmith_testkit::TempDir;

const ROW: &str = "mpn,forward_voltage,max_reverse_voltage,max_forward_current,package,source
ZZD-CURVE,0.6V,77V,1A,SOD-323,https://example.invalid/zzd
";

const CURVE: &str = "current,forward_voltage
1mA,0.30V
100mA,0.42V
1A,0.55V
";

const BOARD: &str = "import std::parts::diode::Schottky

board B {
    .d: Schottky { .current &= 100mA; .max_reverse_voltage &= 77V }
}
";

fn project(manifest: &str, files: &[(&str, &str)]) -> (TempDir, Project) {
    let root = TempDir::new("curve-files");
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
    let project = Project::load(root.path()).unwrap();
    (root, project)
}

fn forward_voltage(project: &Project) -> (f64, f64) {
    let c = check("test.nsm", BOARD, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = netsmith_solve::solve(&c.boards[0], &project.catalogs().unwrap());
    let messages: Vec<&str> = s.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert!(messages.is_empty(), "{messages:?}");
    match s.model.domain_at("d.forward_voltage") {
        Some(Domain::Quantity(q)) => (q.lo(), q.hi()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_curve_in_a_catalog_folder_is_read_with_its_row() {
    let (_root, p) = project(
        "",
        &[
            ("catalogs/schottky.csv", ROW),
            ("catalogs/curves/schottky/ZZD-CURVE.csv", CURVE),
        ],
    );
    let files = p.all_curve_files().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].file, "catalogs/curves/schottky/ZZD-CURVE.csv");
    assert_eq!((files[0].catalog.as_str(), files[0].row.as_str()), ("schottky", "ZZD-CURVE"));
    let (lo, hi) = forward_voltage(&p);
    assert!((lo - 0.42).abs() < 1e-9 && (hi - 0.42).abs() < 1e-9, "{lo}..{hi}");
}

#[test]
fn a_curve_beside_a_listed_catalog_file_is_read() {
    let (_root, p) = project(
        "catalogs = [\"parts/schottky.csv\"]\n",
        &[
            ("parts/schottky.csv", ROW),
            ("parts/curves/schottky/ZZD-CURVE.csv", CURVE),
        ],
    );
    assert_eq!(
        p.all_curve_files().unwrap()[0].file,
        "parts/curves/schottky/ZZD-CURVE.csv"
    );
    let (lo, _) = forward_voltage(&p);
    assert!((lo - 0.42).abs() < 1e-9, "{lo}");
}

#[test]
fn a_misplaced_curve_file_is_an_error_that_names_it() {
    let (_root, p) = project(
        "",
        &[
            ("catalogs/schottky.csv", ROW),
            ("catalogs/curves/ZZD-CURVE.csv", CURVE),
        ],
    );
    let e = p.catalogs().unwrap_err();
    assert!(
        e.contains("catalogs/curves/ZZD-CURVE.csv is not in the folder of a catalog"),
        "{e}"
    );
    let (_root, p) = project(
        "",
        &[
            ("catalogs/schottky.csv", ROW),
            ("catalogs/curves/no-such/ZZD-CURVE.csv", CURVE),
        ],
    );
    let e = p.catalogs().unwrap_err();
    assert!(e.contains("catalogs/curves/no-such is not named after a catalog"), "{e}");
}
