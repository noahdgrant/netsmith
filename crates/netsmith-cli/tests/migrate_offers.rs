//! `netsmith project migrate-offers` (ADR 0143): a project written before a supplier's number
//! became an offer is moved, its catalogs' `lcsc` and `library` columns into its offers files and
//! its `.lcsc &=` hand picks to `.mpn &=`, everything or nothing. The parts are invented;
//! `example.invalid` is not a real address.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

const LED: &str = include_str!("../../../examples/led.nsm");

/// A project's own resistor, as a catalog file had it before ADR 0143: an Extended part with an
/// LCSC number, and a made-up row with neither.
const OLD: &str = "lcsc,mpn,name,resistance,max_power,package,library,source,notes\n\
C9900316,ZZR-316,ZZR-316,316ohm +/- 1%,62.5mW,0402,Extended,\"https://example.invalid/zzr, page 1\",\"a note, kept\"\n\
,,made up 1 ohm,1ohm,62.5mW,0402,,,made up\n";

fn project(label: &str, catalog: &str, source: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("catalogs")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"zz\"\n").unwrap();
    fs::write(root.join("catalogs/resistor.csv"), catalog).unwrap();
    fs::write(root.join("src/main.nsm"), source).unwrap();
    root
}

fn netsmith(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env("NETSMITH_KICAD_SYMBOLS", netsmith_testkit::sample_symbols())
        .env(
            "NETSMITH_KICAD_FOOTPRINTS",
            netsmith_testkit::sample_footprints(),
        )
        .output()
        .unwrap()
}

fn text(run: &std::process::Output) -> (String, String) {
    (
        String::from_utf8_lossy(&run.stdout).to_string(),
        String::from_utf8_lossy(&run.stderr).to_string(),
    )
}

fn read(root: &Path, file: &str) -> String {
    fs::read_to_string(root.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

/// The LED board with the project's resistor held to its row by its LCSC number.
fn source() -> String {
    LED.replace(
        ".r1: Resistor {}",
        ".r1: Resistor { .lcsc &= \"C9900316\" }",
    )
}

#[test]
fn the_columns_go_into_the_offers_files_and_the_hand_pick_becomes_an_mpn() {
    let root = project("migrate-offers", OLD, &source());
    // The old catalog and the old hand pick stop the build, each saying how to move it.
    let (_, err) = text(&netsmith(&root, &["board", "build"]));
    assert!(
        err.contains("catalogs/resistor.csv:1: the columns `lcsc` and `library` have gone from catalog files (ADR 0143)")
            && err.contains("`netsmith project migrate-offers`"),
        "{err}"
    );

    // `--check` says what it would change, writes nothing, and exits 1.
    let check = netsmith(&root, &["project", "migrate-offers", "--check"]);
    assert_eq!(check.status.code(), Some(1), "{}", text(&check).1);
    let (out, _) = text(&check);
    assert!(
        out.contains("catalogs/resistor.csv\n  removed the lcsc and library columns; 1 row has its number in the offers files now\n"),
        "{out}"
    );
    assert!(out.contains("offers/jlcpcb.csv\n  added 1 offer\n"), "{out}");
    assert!(out.contains("offers/lcsc.csv\n  added 1 offer\n"), "{out}");
    assert!(
        out.contains("src/main.nsm\n  line 14: `.lcsc &= \"C9900316\"` -> `.mpn &= \"ZZR-316\"`\n"),
        "{out}"
    );
    assert_eq!(read(&root, "catalogs/resistor.csv"), OLD);
    assert!(!root.join("offers").exists());

    // The run writes it all.
    let run = netsmith(&root, &["project", "migrate-offers"]);
    assert!(run.status.success(), "{}", text(&run).1);
    assert!(
        text(&run).0.contains("now run `netsmith project lock --update`"),
        "{}",
        text(&run).0
    );
    // Every other cell of the catalog as it was written, quotes and all.
    assert_eq!(
        read(&root, "catalogs/resistor.csv"),
        "mpn,name,resistance,max_power,package,source,notes\n\
ZZR-316,ZZR-316,316ohm +/- 1%,62.5mW,0402,\"https://example.invalid/zzr, page 1\",\"a note, kept\"\n\
,made up 1 ohm,1ohm,62.5mW,0402,,made up\n"
    );
    assert_eq!(
        read(&root, "offers/jlcpcb.csv"),
        "manufacturer,mpn,sku,class,source,checked,notes\n\
,ZZR-316,C9900316,Extended,\"https://example.invalid/zzr, page 1\",,moved from the lcsc and library columns of catalogs/resistor.csv:2 by netsmith project migrate-offers\n"
    );
    assert_eq!(
        read(&root, "offers/lcsc.csv"),
        "manufacturer,mpn,sku,class,source,checked,notes\n\
,ZZR-316,C9900316,,\"https://example.invalid/zzr, page 1\",,moved from the lcsc column of catalogs/resistor.csv:2 by netsmith project migrate-offers\n"
    );
    assert_eq!(
        read(&root, "src/main.nsm"),
        LED.replace(".r1: Resistor {}", ".r1: Resistor { .mpn &= \"ZZR-316\" }")
    );

    // The project builds with the part of its own, bought by the moved number.
    let build = netsmith(&root, &["board", "build"]);
    assert!(build.status.success(), "{}", text(&build).1);
    let lock = read(&root, "src/main.lock.toml");
    assert!(
        lock.contains(
            "mpn = \"ZZR-316\"\nsku = { jlcpcb-assembly = \"C9900316\", lcsc-retail = \"C9900316\" }\n"
        ),
        "{lock}"
    );

    // A second run has nothing to move.
    let again = netsmith(&root, &["project", "migrate-offers"]);
    assert!(again.status.success());
    assert!(text(&again).0.starts_with("nothing to move"), "{}", text(&again).0);
}

#[test]
fn what_cannot_be_moved_stops_it_with_the_place_and_nothing_is_written() {
    // A row with an LCSC number and no part number, and a hand pick of a number no offer has.
    let catalog = "lcsc,mpn,resistance,max_power,package,library,source\n\
C9900316,,316ohm +/- 1%,62.5mW,0402,Extended,https://example.invalid/zzr\n";
    let source = LED.replace(
        ".r1: Resistor {}",
        ".r1: Resistor { .lcsc &= \"C9999999\" }",
    );
    let root = project("migrate-offers-stops", catalog, &source);
    let run = netsmith(&root, &["project", "migrate-offers"]);
    assert_eq!(run.status.code(), Some(2));
    let (_, err) = text(&run);
    assert!(
        err.contains("catalogs/resistor.csv:2: the row has the lcsc `C9900316` and no `mpn`"),
        "{err}"
    );
    assert!(
        err.contains("src/main.nsm:14: `.lcsc &= \"C9999999\"`: no offer has the sku C9999999"),
        "{err}"
    );
    assert!(err.contains("migrate-offers: nothing was written"), "{err}");
    assert_eq!(read(&root, "catalogs/resistor.csv"), catalog);
    assert_eq!(read(&root, "src/main.nsm"), source);
    assert!(!root.join("offers").exists());
}

#[test]
fn outside_a_project_there_is_nothing_to_migrate() {
    let root = TempDir::new("migrate-offers-none");
    let run = netsmith(&root, &["project", "migrate-offers"]);
    assert_eq!(run.status.code(), Some(2));
    assert!(text(&run).1.contains("no netsmith.toml"), "{}", text(&run).1);
}
