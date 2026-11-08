//! Price and stock from a provider, and generic resistors resolved to rows (ADR 0047), from
//! the `[supply]` table to the bill of materials, `fab` and `catalog age`. Every part, number
//! and file here is invented; no test reaches a supplier.

use netsmith_kicad::testlib::TempDir;
use std::fs;
use std::path::Path;

use netsmith::Libraries;
use netsmith::editor::Workspace;
use netsmith::fab::{FabOptions, fab_with_options};
use netsmith::project::{Manifest, Project, Target};
use netsmith::refresh::Network;
use netsmith_check::Loader;
use netsmith_editor::Backend;
use netsmith_editor::api::Bom;
use netsmith_fab::KicadCliExe;

/// 2026-11-02 12:00 UTC.
const NOW: u64 = 1_793_620_800;
const DAY: u64 = 86_400;

/// A resistor in series with an LED on 5 V: the resistor must be between 230 and 470 ohm.
const LED: &str = include_str!("../../../examples/led.nsm");

/// An invented resistor only this project has.
const OWN_CATALOG: &str = "lcsc,mpn,resistance,max_power,package,library,source\n\
     C9999999,ZZR-316,316ohm +/- 1%,250mW,0402,Extended,https://example.invalid/zzr\n";

/// Invented prices: 42 pieces at JLCPCB for assembly, 9000 at retail.
const QUOTES: &str = "lcsc,mpn,channel,stock,price_breaks,currency,date\n\
     C9999999,ZZR-316,jlcpcb-assembly,42,1:0.0123 100:0.0100,USD,2026-10-31\n\
     C9999999,ZZR-316,lcsc-retail,9000,1:0.0300,USD,2026-10-30\n";

struct Setup<'a> {
    name: &'a str,
    /// Extra keys of the `[project]` table.
    project: &'a str,
    /// The `[supply]` table, or none.
    supply: Option<&'a str>,
    quotes: Option<&'a str>,
    catalog: Option<&'a str>,
    resistance: &'a str,
}

fn open(s: &Setup) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("supply-test-{}", s.name));
    fs::create_dir_all(root.join("src")).unwrap();
    // The catalog file, if there is one, is in `catalogs/`, which the manifest reads by default.
    let mut manifest = format!("[project]\nname = \"own\"\n{}\n", s.project);
    if let Some(supply) = s.supply {
        manifest.push_str(&format!("[supply]\n{supply}\n"));
    }
    fs::write(root.join("netsmith.toml"), manifest).unwrap();
    if let Some(catalog) = s.catalog {
        fs::create_dir_all(root.join("catalogs")).unwrap();
        fs::write(root.join("catalogs/resistor.csv"), catalog).unwrap();
    }
    if let Some(quotes) = s.quotes {
        fs::create_dir_all(root.join("supply")).unwrap();
        fs::write(root.join("supply/quotes.csv"), quotes).unwrap();
    }
    let source = LED.replace(
        ".r1: Resistor {}",
        &format!(".r1: Resistor {{ .resistance &= {} }}", s.resistance),
    );
    fs::write(root.join("src/main.nsm"), source).unwrap();
    let editor = open_at(&root, NOW);
    (root, editor)
}

fn open_at(root: &Path, now: u64) -> Workspace {
    Workspace::open(Target::resolve_from(root, None).unwrap())
        .unwrap()
        .with_network(Network {
            now: Some(now),
            offline: true,
            cache_dir: None,
            ..Network::default()
        })
}

fn bom_of(editor: &Workspace) -> Bom {
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    editor.bom(&analysis.boards[0].name).unwrap()
}

const FILE_PROVIDER: &str = "provider = \"file\"\nfile = \"supply/quotes.csv\"";

fn priced(name: &str) -> Setup<'_> {
    Setup {
        name,
        project: "",
        supply: Some(FILE_PROVIDER),
        quotes: Some(QUOTES),
        catalog: Some(OWN_CATALOG),
        resistance: "316ohm +/- 1%",
    }
}

#[test]
fn the_bom_takes_price_stock_and_day_from_the_provider_and_says_which_stock_it_is() {
    let (_root, editor) = open(&priced("quoted"));
    let bom = bom_of(&editor);
    let r = bom.lines.iter().find(|l| l.kind == "Resistor").unwrap();
    assert_eq!(r.lcsc.as_deref(), Some("C9999999"));
    assert_eq!((r.unit_usd, r.stock), (Some(0.0123), Some(42)));
    assert_eq!(r.supply.as_deref(), Some("file, JLCPCB assembly stock"));
    assert_eq!(r.date.as_deref(), Some("2026-10-31"));
    assert_eq!((r.age_days, r.stale), (Some(2), false));
    assert_eq!(
        (r.unknown.as_deref(), r.unresolved.as_deref()),
        (None, None)
    );
    // The LED has a number the file does not list: unknown, not zero and not stale.
    let led = bom.lines.iter().find(|l| l.kind == "Led").unwrap();
    assert_eq!(
        (led.unit_usd, led.stock, led.date.as_deref()),
        (None, None, None)
    );
    assert_eq!(led.unknown.as_deref(), Some("file does not list the part"));
    assert!(!led.stale && led.supply.is_none());
    // The total is the priced lines only, and the note says one line is left out.
    assert_eq!(bom.total_usd, Some(0.0123));
    assert!(
        bom.note
            .contains("The total leaves out 1 line with no known price"),
        "{}",
        bom.note
    );
}

#[test]
fn the_channel_of_the_manifest_chooses_which_stock_is_shown() {
    let mut setup = priced("channel");
    setup.supply =
        Some("provider = \"file\"\nfile = \"supply/quotes.csv\"\nchannel = \"lcsc-retail\"");
    let (_root, editor) = open(&setup);
    let bom = bom_of(&editor);
    let r = bom.lines.iter().find(|l| l.kind == "Resistor").unwrap();
    assert_eq!((r.unit_usd, r.stock), (Some(0.03), Some(9000)));
    assert_eq!(r.supply.as_deref(), Some("file, LCSC retail stock"));
    assert_eq!(r.date.as_deref(), Some("2026-10-30"));
}

#[test]
fn a_price_is_for_the_quantity_the_line_needs() {
    // 100 pieces of the part on one board is not a board; use the price rule through the
    // quote itself: one resistor is priced at the one-piece break.
    let (_root, editor) = open(&priced("break"));
    let bom = bom_of(&editor);
    let r = bom.lines.iter().find(|l| l.kind == "Resistor").unwrap();
    assert_eq!(r.line_usd, Some(0.0123));
}

#[test]
fn an_old_answer_is_marked_stale_and_still_shown_with_its_age() {
    let (root, _) = open(&priced("stale"));
    let editor = open_at(&root, NOW + 91 * DAY);
    let bom = bom_of(&editor);
    let r = bom.lines.iter().find(|l| l.kind == "Resistor").unwrap();
    assert_eq!((r.age_days, r.stale), (Some(93), true));
    assert_eq!(r.unit_usd, Some(0.0123));
}

#[test]
fn with_no_provider_price_and_stock_are_unknown_and_nothing_is_called_undated() {
    let (_root, editor) = open(&Setup {
        name: "none",
        project: "",
        supply: None,
        quotes: None,
        catalog: Some(OWN_CATALOG),
        resistance: "316ohm +/- 1%",
    });
    let bom = bom_of(&editor);
    for line in bom.lines.iter().chain(&bom.hand) {
        assert_eq!(
            (line.unit_usd, line.stock, line.line_usd),
            (None, None, None)
        );
        assert!(!line.stale, "unknown is not stale: {line:?}");
        assert_eq!(line.date, None);
        assert!(
            line.unknown
                .as_deref()
                .is_some_and(|u| u.contains("no supply provider is configured")),
            "{line:?}"
        );
    }
    assert_eq!(bom.total_usd, None);
    assert!(!bom.note.contains("undated"));
}

#[test]
fn a_provider_answer_wins_over_the_row_cells_and_the_cells_are_the_last_resort() {
    let catalog = "lcsc,mpn,resistance,max_power,package,library,price,stock,date,source\n\
        C9999999,ZZR-316,316ohm +/- 1%,250mW,0402,Extended,0.5,1,2026-01-01,https://example.invalid/zzr\n";
    let mut with = priced("wins");
    with.catalog = Some(catalog);
    let (_root, editor) = open(&with);
    let r = bom_of(&editor)
        .lines
        .into_iter()
        .find(|l| l.kind == "Resistor")
        .unwrap();
    assert_eq!((r.unit_usd, r.stock), (Some(0.0123), Some(42)));

    // The file does not list the part: the row's own cells are used, and said to be.
    let mut without = priced("cells");
    without.catalog = Some(catalog);
    without.quotes = Some("lcsc,channel,date\nC9999998,jlcpcb-assembly,2026-10-31\n");
    let (_root, editor) = open(&without);
    let r = bom_of(&editor)
        .lines
        .into_iter()
        .find(|l| l.kind == "Resistor")
        .unwrap();
    assert_eq!((r.unit_usd, r.stock), (Some(0.5), Some(1)));
    assert_eq!(r.supply.as_deref(), Some("catalog file, entered by hand"));
    assert_eq!(r.date.as_deref(), Some("2026-01-01"));
}

// ---- generic resistors -------------------------------------------------------------

fn generic(name: &str) -> Setup<'_> {
    Setup {
        name,
        project: "series = \"E96\"",
        supply: Some(FILE_PROVIDER),
        quotes: Some(QUOTES),
        catalog: Some(OWN_CATALOG),
        // 249 ohm is on the E96 series, and no row of the standard resistor catalog has it.
        resistance: "249ohm +/- 1%",
    }
}

#[test]
fn a_generic_resistor_no_row_stands_for_is_unresolved_with_the_nearest_rows() {
    let (_root, editor) = open(&generic("unresolved"));
    let bom = bom_of(&editor);
    let r = bom.lines.iter().find(|l| l.kind == "Resistor").unwrap();
    assert_eq!(r.lcsc, None);
    assert_eq!(r.value, "249");
    let why = r.unresolved.as_deref().expect("an unresolved part");
    assert!(
        why.starts_with("unresolved: no row of the `resistor` catalog can be ordered for 249 ohm 1% 0402 62.5 mW (E96)."),
        "{why}"
    );
    // The E24 neighbours, 240 and 270 ohm, are among the nearest rows.
    assert!(why.contains("Nearest in the catalog: 240"), "{why}");
    assert!(
        why.contains("Change the value in the design and build again"),
        "{why}"
    );
    assert_eq!((r.unit_usd, r.unknown.as_deref()), (None, None));
    assert!(bom.note.contains("leaves out"), "{}", bom.note);
}

#[test]
fn a_row_added_to_the_catalog_resolves_the_generic_resistor_to_it() {
    let mut setup = generic("resolved");
    let catalog = "lcsc,mpn,resistance,max_power,package,library,source\n\
        C9999998,ZZR-249,249ohm +/- 1%,62.5mW,0402,Extended,https://example.invalid/zzr249\n";
    setup.catalog = Some(catalog);
    setup.quotes = Some(
        "lcsc,channel,stock,price_breaks,date\nC9999998,jlcpcb-assembly,10,1:0.002,2026-11-01\n",
    );
    let (_root, editor) = open(&setup);
    let bom = bom_of(&editor);
    let r = bom.lines.iter().find(|l| l.kind == "Resistor").unwrap();
    assert_eq!(r.lcsc.as_deref(), Some("C9999998"));
    assert_eq!(r.library.as_deref(), Some("Extended"));
    assert_eq!(r.unresolved, None);
    assert_eq!((r.unit_usd, r.stock), (Some(0.002), Some(10)));
}

#[test]
fn a_row_the_solver_picked_is_shown_with_its_stock_even_when_that_is_0() {
    // The solver does not read stock (ADR 0047): it takes the first row that works, and the
    // bill of materials says the provider has none of it. Nothing hides the row or turns the
    // 0 into unknown.
    let mut setup = generic("sold-out");
    setup.catalog = Some(
        "lcsc,mpn,resistance,max_power,package,library,source\n\
         C9999998,ZZR-249-A,249ohm +/- 1%,62.5mW,0402,Basic,https://example.invalid/a\n\
         C9999997,ZZR-249-B,249ohm +/- 1%,62.5mW,0402,Extended,https://example.invalid/b\n",
    );
    setup.quotes = Some(
        "lcsc,channel,stock,price_breaks,date\n\
         C9999998,jlcpcb-assembly,0,1:0.002,2026-11-01\n\
         C9999997,jlcpcb-assembly,5,1:0.004,2026-11-01\n",
    );
    let (_root, editor) = open(&setup);
    let r = bom_of(&editor)
        .lines
        .into_iter()
        .find(|l| l.kind == "Resistor")
        .unwrap();
    assert_eq!((r.lcsc.as_deref(), r.stock), (Some("C9999998"), Some(0)));
}

#[test]
fn a_value_a_row_has_is_picked_as_before_and_series_none_turns_generics_off() {
    // 316 ohm has a row (the project's), so no generic part is involved.
    let (_root, editor) = open(&priced("row-first"));
    assert_eq!(
        bom_of(&editor)
            .lines
            .iter()
            .find(|l| l.kind == "Resistor")
            .unwrap()
            .lcsc
            .as_deref(),
        Some("C9999999")
    );
    // With series "none" a value without a row is the solver's error again.
    let mut off = generic("off");
    off.project = "series = \"none\"";
    let (_root, editor) = open(&off);
    let analysis = editor.analysis();
    assert!(analysis.errors > 0, "{:?}", analysis.problems);
}

// ---- fab ---------------------------------------------------------------------------

fn have_libraries() -> bool {
    KicadCliExe::locate_runnable().is_some()
}

fn fab_of(root: &Path) -> Result<(), String> {
    let project = Project::load(root).unwrap();
    let source = fs::read_to_string(root.join("src/main.nsm")).unwrap();
    let catalogs = project.catalogs().unwrap();
    let out = TempDir::unused("supply-test-fab-out");
    let made = fab_with_options(
        "main.nsm",
        &source,
        &Loader::std(),
        &out.path().join("fab"),
        &catalogs,
        &Libraries::standard(),
        None,
        &FabOptions::default(),
    );
    let bom = made.map(|made| fs::read_to_string(made[0].dir.join("bom.csv")).unwrap_or_default());
    bom.map(|text| assert!(text.contains("C9999998"), "{text}"))
}

#[test]
fn fab_refuses_an_unresolved_part_by_name_and_orders_a_resolved_one() {
    if !have_libraries() {
        eprintln!("skipped: no KiCad to read the footprints from");
        return;
    }
    let (root, _) = open(&generic("fab-refused"));
    let e = fab_of(&root).unwrap_err();
    assert!(
        e.contains("R1") || e.contains("r1"),
        "the part is named: {e}"
    );
    assert!(
        e.contains("unresolved: no row of the `resistor` catalog"),
        "{e}"
    );
    assert!(e.contains("Nearest in the catalog"), "{e}");

    let mut ok = generic("fab-resolved");
    ok.catalog = Some(
        "lcsc,mpn,resistance,max_power,package,library,source\n\
         C9999998,ZZR-249,249ohm +/- 1%,62.5mW,0402,Extended,https://example.invalid/zzr249\n",
    );
    let (root, _) = open(&ok);
    fab_of(&root).unwrap();
}

// ---- the manifest ------------------------------------------------------------------

#[test]
fn the_manifest_has_no_field_for_a_credential_and_names_only_the_provider_that_is_built() {
    let parse =
        |supply: &str| Manifest::parse(&format!("[project]\nname = \"a\"\n[supply]\n{supply}\n"));
    let ok = parse("provider = \"file\"\nfile = \"supply/q.csv\"\ncache_hours = 6").unwrap();
    assert_eq!(ok.supply.as_ref().unwrap().cache_hours, Some(6));
    // A key, token or password is not a field; the parser refuses it with the key's name.
    for secret in ["key", "api_key", "token", "secret", "password"] {
        let e = parse(&format!(
            "provider = \"file\"\nfile = \"q.csv\"\n{secret} = \"x\""
        ))
        .unwrap_err();
        assert!(e.contains(&format!("unknown field `{secret}`")), "{e}");
    }
    let e = parse("provider = \"lcsc\"").unwrap_err();
    assert!(e.contains("only \"file\" is built"), "{e}");
    let e = parse("provider = \"file\"").unwrap_err();
    assert!(e.contains("needs `file ="), "{e}");
    let e = parse("provider = \"file\"\nfile = \"../q.csv\"").unwrap_err();
    assert!(e.contains("inside the project"), "{e}");
    let e = parse("provider = \"file\"\nfile = \"q.csv\"\nchannel = \"warehouse\"").unwrap_err();
    assert!(e.contains("jlcpcb-assembly"), "{e}");
    let e = Manifest::parse("[project]\nname = \"a\"\nseries = \"E48\"\n").unwrap_err();
    assert!(e.contains("series = \"E48\""), "{e}");
    // It is written back as it was read.
    let text = ok.render();
    assert!(
        text.contains("[supply]") && text.contains("cache_hours = 6"),
        "{text}"
    );
    assert_eq!(Manifest::parse(&text).unwrap(), ok);
}

#[test]
fn a_supply_file_that_is_wrong_stops_the_project_with_its_line() {
    let mut bad = priced("bad-file");
    bad.quotes = Some("lcsc,channel,stock,date\nC1,warehouse,5,2026-11-01\n");
    let root = TempDir::new("supply-test-bad-file");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("supply")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"own\"\n[supply]\n{FILE_PROVIDER}\n"),
    )
    .unwrap();
    fs::write(root.join("supply/quotes.csv"), bad.quotes.unwrap()).unwrap();
    let e = Project::load(&root).unwrap().catalogs().unwrap_err();
    assert!(
        e.contains("supply/quotes.csv:2: channel `warehouse`"),
        "{e}"
    );
}
