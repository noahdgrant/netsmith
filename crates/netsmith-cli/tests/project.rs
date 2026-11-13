//! `netsmith init`, the `netsmith.toml` manifest, and projects of several files.

use crate::common;

use netsmith_testkit::{TempDir, TempFile};
use std::fs;
use std::path::Path;
use std::process::Command;

use netsmith::build_with;
use netsmith::editor::Workspace;
use netsmith::project::{Manifest, Project, Target};
use netsmith_editor::Backend;
use netsmith_solve::Catalogs;

const LED: &str = include_str!("../../../examples/led.nsm");

/// An empty directory of its own.
fn dir(name: &str) -> TempDir {
    TempDir::new(&format!("project-test-{name}"))
}

fn write(root: &Path, file: &str, text: &str) {
    let path = root.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn read(root: &Path, file: &str) -> String {
    fs::read_to_string(root.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

// ---- init ----------------------------------------------------------------------------

// ---- finding the project --------------------------------------------------------------

// ---- several files ----------------------------------------------------------------------

fn build_project(root: &Path) -> Result<Vec<netsmith::Built>, String> {
    let target = Target::resolve_from(root, None)?;
    let mut all = Vec::new();
    for f in &target.files {
        let text = fs::read_to_string(&f.path).unwrap();
        all.extend(build_with(
            &f.name,
            &text,
            &target.loader()?,
            &Catalogs::jlcpcb(),
            &netsmith::Libraries::sample(),
        )?);
    }
    Ok(all)
}

// ---- the program ------------------------------------------------------------------------

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

#[test]
fn init_then_build_from_a_subdirectory_through_the_program() {
    let root = TempFile::new("project-test-program", "blink");
    fs::create_dir_all(&root).unwrap();

    let init = netsmith(&root, &["init"]);
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let said = String::from_utf8_lossy(&init.stdout);
    assert!(
        said.contains("created netsmith.toml") && said.contains("netsmith build"),
        "{said}"
    );

    let again = netsmith(&root, &["init"]);
    assert!(!again.status.success());
    let e = String::from_utf8_lossy(&again.stderr);
    assert!(
        e.contains("not overwriting") && e.contains("netsmith.toml"),
        "{e}"
    );

    let build = netsmith(&root.join("src"), &["build"]);
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(
        root.join("out/Blink/Blink.kicad_sch").is_file(),
        "written under the project root, not the current directory"
    );
    assert!(!root.join("src/out").exists());

    // An explicit file still goes where it always did.
    let named = netsmith(&root, &["build", "src/main.nsm", "--out", "elsewhere"]);
    assert!(named.status.success());
    assert!(root.join("elsewhere/Blink/Blink.kicad_sch").is_file());

    // With no project and no file there is a message, not a crash.
    let lost = netsmith(&dir("lost"), &["build"]);
    assert_eq!(lost.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&lost.stderr).contains("netsmith init"));
}

/// A project that is only a library: `top_level = []`, and a module under `src/`.
fn library_project(name: &str) -> TempDir {
    let root = dir(name);
    write(
        &root,
        "netsmith.toml",
        "[project]\nname = \"lib\"\ntop_level = []\n",
    );
    write(
        &root,
        "src/parts.nsm",
        "import std::parts::passive::Resistor\n\n/// A resistor.\npart Pull {\n    .r: Resistor\n}\n",
    );
    root
}

#[test]
fn a_library_only_project_lists_no_top_level_file_and_its_commands_say_so() {
    let root = library_project("library");
    let target = Target::resolve_from(&root, None).unwrap_or_else(|e| panic!("{e}"));
    assert!(target.files.is_empty());
    assert_eq!(target.title, "lib");

    for command in ["build", "fab"] {
        let run = netsmith(&root, &[command]);
        assert_eq!(run.status.code(), Some(2), "{command}");
        let e = String::from_utf8_lossy(&run.stderr);
        assert_eq!(
            e,
            "error: this project has no boards: `top_level` in netsmith.toml is empty; name a .nsm file to build it\n",
            "{command}"
        );
    }
    // A named file still builds, with the project's modules in reach.
    write(
        &root,
        "src/main.nsm",
        "import src::parts::Pull\nboard B {\n    .p: Pull\n}\n",
    );
    let named = netsmith(&root, &["build", "src/main.nsm", "--out", "named"]);
    assert!(
        !String::from_utf8_lossy(&named.stderr).contains("no boards"),
        "{}",
        String::from_utf8_lossy(&named.stderr)
    );

    // The editor opens the project and has no board to show.
    let editor = Workspace::open(target).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(editor.analysis().boards.len(), 0);
}

// ---- a system in a project --------------------------------------------------------------

const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");

/// A project whose only top-level file holds a `system`.
fn system_project(name: &str) -> TempDir {
    let root = dir(name);
    write(
        &root,
        "netsmith.toml",
        "[project]\nname = \"pair\"\ntop_level = [\"src/pair.nsm\"]\n",
    );
    write(&root, "src/pair.nsm", TWO_BOARDS);
    root
}

#[test]
fn a_top_level_file_with_a_system_builds_into_a_directory_per_member() {
    let root = system_project("system-build");
    let built = build_project(&root).unwrap_or_else(|e| panic!("{e}"));
    let labels: Vec<_> = built.iter().map(|b| b.label()).collect();
    assert_eq!(labels, ["TwoBoards/power", "TwoBoards/led"]);

    // Through the program, from a subdirectory, with no file argument.
    let run = netsmith(&root.join("src"), &["build"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    for (member, board) in [("power", "PowerBoard"), ("led", "LedBoard")] {
        let dir = format!("out/TwoBoards/{member}");
        assert!(
            read(&root, &format!("{dir}/{board}.kicad_sch")).starts_with("(kicad_sch"),
            "{dir}"
        );
        assert!(root.join(&dir).join(format!("{board}.kicad_pro")).is_file());
    }
    // A board the system places is not also built on its own.
    assert!(!root.join("out/PowerBoard").exists() && !root.join("out/LedBoard").exists());
}

// ---- a project's own catalogs (ADR 0022) ---------------------------------------------

/// An invented regulator with a part number as its key, as a project module. Not a real
/// part; the catalog file below is invented too.
const ZZ9: &str = r#"import std::sources::Rail

/// An invented fixed-output regulator, picked by its part number.
part Zz9 {
    .vin:  Rail
    .vout: Rail
    .vin.neg ~ .vout.neg

    .dropout:     Voltage
    .max_current: Current
    .vin.voltage >= .vout.voltage + .dropout
    .vin.current == -.vout.current
    -.vout.current <= .max_current

    pins "SOT-23-5" {
        .vin.pos       power       "IN"     "1"
        .vin.neg       power       "GND"    "2"
        .en            input       "EN"     "3"
        .nc            no_connect  "NC"     "4"
        .vout.pos      power       "OUT"    "5"
    }

    .mpn: String
    .package: Package
    catalog zz9 by .mpn { .mpn, .vout.voltage, .dropout, .max_current, .package }
}
"#;

const ZZ9_BOARD: &str = r#"import std::sources::Rail
import src::ics::Zz9

board Zz9Board {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .u: Zz9 { .mpn &= "ZZ9-33-S" }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..50mA
    }
    .input ~ .u.vin
    .u.vout ~ .output
    .u.en ~ .u.vin.pos
}
"#;

const ZZ9_CSV: &str = "mpn,package,vout.voltage,dropout,max_current,library,price,stock,date,source\n\
ZZ9-33-S,SOT-23-5,3.3V +/- 2%,0.3V,0.5A,Basic,0.10,1000,2026-10-31,https://example.invalid/zz9-33-s.pdf\n";

/// A project whose `netsmith.toml` has the `manifest_lines` (about its catalogs), with the
/// regulator module and board, and `catalogs/zz9.csv`.
fn catalog_project(name: &str, manifest_lines: &str, csv: &str) -> TempDir {
    let root = dir(name);
    write(
        &root,
        "netsmith.toml",
        &format!("[project]\nname = \"zz\"\ntop_level = [\"src/main.nsm\"]\n{manifest_lines}\n"),
    );
    write(&root, "src/ics.nsm", ZZ9);
    write(&root, "src/main.nsm", ZZ9_BOARD);
    write(&root, "catalogs/zz9.csv", csv);
    root
}

#[test]
fn a_manifest_assembly_that_is_no_assembly_stops_every_command_naming_the_field() {
    let root = dir("asm-bad");
    write(
        &root,
        "netsmith.toml",
        "[project]\nname = \"asm\"\nassembly = \"solder\"\n",
    );
    write(&root, "src/main.nsm", LED);
    let run = netsmith(&root, &["build"]);
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success());
    assert!(
        said.contains(
            "assembly = \"solder\" is not one of \"fab\", \"hand\", \"none\" or \"consigned\""
        ),
        "{said}"
    );
}

#[test]
fn a_project_file_named_like_a_standard_catalog_adds_parts_to_it_through_the_program() {
    let root = dir("own-resistor");
    write(&root, "netsmith.toml", "[project]\nname = \"zr\"\n");
    write(
        &root,
        "catalogs/resistor.csv",
        "mpn,resistance,max_power,package,library,price,stock,date,source\n\
         ZZR-316,316ohm +/- 1%,250mW,0402,Basic,0.01,10,2026-10-31,https://example.invalid/zzr\n",
    );
    // The LED example with its resistor fixed to a value only the project has.
    let board = LED.replace(
        ".r1: Resistor {}",
        ".r1: Resistor { .resistance &= 316ohm +/- 1% }",
    );
    write(&root, "src/main.nsm", &board);
    let run = netsmith(&root, &["build"]);
    let said = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success(),
        "{}{said}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(said.contains("ZZR-316"), "{said}");
}

#[test]
fn a_mistake_in_a_project_catalog_file_names_the_file_and_line_through_the_program() {
    for (csv, expect) in [
        (
            // The second row has no source.
            format!("{ZZ9_CSV}ZZ9-33-D,SOT-23-5,3.3V,0.2V,1A,,,,,\n"),
            "catalogs/zz9.csv:3: the row for mpn ZZ9-33-D has no source",
        ),
        (
            ZZ9_CSV.replace("0.3V", "0.3A"),
            "catalogs/zz9.csv:2: `dropout` of `catalog zz9`",
        ),
        (
            ZZ9_CSV.replace("dropout", "droput"),
            "catalogs/zz9.csv:1: `droput` is not a column of `catalog zz9`",
        ),
        (
            format!(
                "{ZZ9_CSV}{}",
                ZZ9_CSV.lines().nth(1).unwrap().to_string() + "\n"
            ),
            "catalogs/zz9.csv:3: the mpn `ZZ9-33-S` is already a row at catalogs/zz9.csv:2",
        ),
    ] {
        let root = catalog_project("own-bad", "", &csv);
        let run = netsmith(&root, &["build"]);
        let said = String::from_utf8_lossy(&run.stderr);
        assert!(!run.status.success(), "{expect}");
        assert!(said.contains(expect), "wanted {expect}\n{said}");
    }
}

#[test]
fn a_project_file_that_repeats_a_standard_part_is_refused_naming_both_files() {
    let root = dir("own-dup");
    write(&root, "netsmith.toml", "[project]\nname = \"zd\"\n");
    write(
        &root,
        "catalogs/resistor.csv",
        "lcsc,mpn,resistance,max_power,package,source\nC25104,X,330ohm +/- 1%,62.5mW,0402,https://example.invalid/x\n",
    );
    write(&root, "src/main.nsm", LED);
    let run = netsmith(&root, &["build"]);
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success());
    assert!(
        said.contains("catalogs/resistor.csv:2: the lcsc `C25104` is already a row at data/jlcpcb/catalog/resistor.csv:"),
        "{said}"
    );
}

/// The catalog files of the project at `root`, as `catalog: file` pairs.
fn catalog_files(root: &Path) -> Result<Vec<String>, String> {
    Project::load(root)
        .and_then(|p| p.catalog_files())
        .map(|files| {
            files
                .into_iter()
                .map(|f| format!("{}: {}", f.name, f.file))
                .collect()
        })
}

#[test]
fn a_csv_in_a_catalog_folder_that_is_not_named_like_a_catalog_is_refused_naming_it() {
    let root = dir("folder-stray");
    write(&root, "netsmith.toml", "[project]\nname = \"p\"\n");
    write(&root, "catalogs/fine.csv", "mpn\n");
    for name in ["my-parts.csv", "3x.csv", "two words.csv", ".csv"] {
        write(&root, &format!("catalogs/{name}"), "x\n");
        let e = catalog_files(&root).unwrap_err();
        assert!(
            e.contains(&format!("catalogs/{name}"))
                && e.contains("is not a catalog")
                && e.contains("catalog_dirs"),
            "{name}: {e}"
        );
        fs::remove_file(root.join(format!("catalogs/{name}"))).unwrap();
    }
    // The program says the same, and does not build.
    write(&root, "catalogs/readings-2026.csv", "x\n");
    write(&root, "src/main.nsm", LED);
    let run = netsmith(&root, &["build"]);
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success());
    assert!(
        said.contains("catalogs/readings-2026.csv") && said.contains("is not a catalog"),
        "{said}"
    );
}

#[test]
fn a_catalog_in_two_files_or_a_file_named_twice_is_refused_naming_both() {
    let root = dir("folder-twice");
    // A folder file that the list names too.
    write(
        &root,
        "netsmith.toml",
        "[project]\nname = \"p\"\ncatalogs = [\"./catalogs/ldo.csv\"]\n",
    );
    write(&root, "catalogs/ldo.csv", "mpn\n");
    let e = catalog_files(&root).unwrap_err();
    assert!(
        e.contains("catalogs/ldo.csv is named twice")
            && e.contains("catalog_dirs")
            && e.contains("Remove it from `catalogs`"),
        "{e}"
    );
    // Two files of one stem: one in a folder, one from elsewhere.
    write(
        &root,
        "netsmith.toml",
        "[project]\nname = \"p\"\ncatalogs = [\"other/ldo.csv\"]\n",
    );
    write(&root, "other/ldo.csv", "mpn\n");
    let e = catalog_files(&root).unwrap_err();
    assert!(
        e.contains("the catalog `ldo` is in two files, catalogs/ldo.csv and other/ldo.csv"),
        "{e}"
    );
    // Two folders.
    write(
        &root,
        "netsmith.toml",
        "[project]\nname = \"p\"\ncatalog_dirs = [\"catalogs\", \"other\"]\n",
    );
    let e = catalog_files(&root).unwrap_err();
    assert!(
        e.contains("catalogs/ldo.csv") && e.contains("other/ldo.csv"),
        "{e}"
    );
    // The program refuses to build, with the message.
    write(&root, "src/main.nsm", LED);
    let run = netsmith(&root, &["build"]);
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success());
    assert!(
        said.contains("catalogs/ldo.csv") && said.contains("other/ldo.csv"),
        "{said}"
    );
    // Listing the file only (no folder) is the way it was.
    write(
        &root,
        "netsmith.toml",
        "[project]\nname = \"p\"\ncatalog_dirs = []\ncatalogs = [\"other/ldo.csv\"]\n",
    );
    assert_eq!(catalog_files(&root).unwrap(), ["ldo: other/ldo.csv"]);
}

#[test]
fn kicad_libs_import_prints_the_stanzas_to_paste_into_kicad_toml() {
    let root = dir("libs-import");
    fs::write(
        root.join("sym-lib-table"),
        "(sym_lib_table (version 7)\n  (lib (name \"acme\")(type \"KiCad\")(uri \"${KIPRJMOD}/libs/acme.kicad_sym\")(options \"\")(descr \"\")))\n",
    )
    .unwrap();
    fs::write(
        root.join("fp-lib-table"),
        "(fp_lib_table (version 7)\n  (lib (name \"acme\")(type \"KiCad\")(uri \"${KIPRJMOD}/libs/acme.pretty\")(options \"\")(descr \"\")))\n",
    )
    .unwrap();
    let out = netsmith(
        &root,
        &["kicad-libs", "import", "sym-lib-table", "fp-lib-table"],
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "[[library]]\nname = \"acme\"\nsymbols = \"libs/acme.kicad_sym\"\nfootprints = \"libs/acme.pretty\"\n\n"
    );
    // A table that is not there is a message.
    let lost = netsmith(&root, &["kicad-libs", "import", "nope"]);
    assert_eq!(lost.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&lost.stderr).contains("nope"));
}

#[test]
fn a_projects_kicad_toml_gives_its_own_libraries_to_build_and_the_tables_to_kicad() {
    let root = dir("own-libs");
    fs::write(root.join("netsmith.toml"), Manifest::new("own").render()).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.nsm"), common::DUAL_BOARD).unwrap();
    let _ = common::project_libraries(&root);
    let out = netsmith(&root, &["build"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    let board = root.join("out/B");
    let schematic = fs::read_to_string(board.join("B.kicad_sch")).unwrap();
    assert!(schematic.contains("(lib_id \"acme:Dual\")"));
    // KiCad is told where the project's libraries are, relative to the board's folder.
    let symbols = fs::read_to_string(board.join("sym-lib-table")).unwrap();
    assert!(
        symbols.contains("${KIPRJMOD}/../../libs/acme.kicad_sym"),
        "{symbols}"
    );
    let footprints = fs::read_to_string(board.join("fp-lib-table")).unwrap();
    assert!(
        footprints.contains("${KIPRJMOD}/../../libs/acme.pretty"),
        "{footprints}"
    );

    // A mistake in kicad.toml is one message with the file's name, not a crash.
    fs::write(root.join("kicad.toml"), "[[part]]\nknid = 1\n").unwrap();
    let bad = netsmith(&root, &["build"]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).starts_with("kicad.toml: "));
}
