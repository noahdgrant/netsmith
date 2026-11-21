//! A part that names its own supply channel in `[supply.parts]` of `netsmith.toml` (ADR 0143
//! item 4, issue #1475): it is bought by that channel only, the rest of the board by the
//! project's list, and the pick lock records the named channel's sku. An unknown channel, a
//! part the board does not have and a part with no offer in its channel are refused with the
//! file and the key. Every part and number is invented; `example.invalid` is no real address.
//! Nothing here reads KiCad.

use std::fs;

use netsmith_fabricate::project::Project;
use netsmith_pipeline::{Outcome, Problem, outcome_with, render_problems};
use netsmith_testkit::TempDir;

/// One invented resistor, which both parts of the board are.
const ROWS: &str = "\
mpn,resistance,max_power,package,source
ZZR-1234,1.234kohm +/- 0.1%,100mW,0402,https://example.invalid/zzr-1234
";

/// Its offers: LCSC's (which JLCPCB's library lists) and Digi-Key's. Mouser has none.
const LCSC: &str = "manufacturer,mpn,sku,class,source,checked,notes\n\
    ,ZZR-1234,C900001,Extended,https://example.invalid/lcsc,2026-11-20,\n";
const DIGIKEY: &str = "manufacturer,mpn,sku,class,source,checked,notes\n\
    ,ZZR-1234,ZZR-1234-ND,,https://example.invalid/digikey,2026-11-20,\n";

/// Two resistors in a module, so that one has a path with a dot in it.
const BOARD: &str = r#"
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

module Pair {
    .supply: Rail
    .a: Resistor { .resistance &= 1.23kohm..1.24kohm }
    .b: Resistor { .resistance &= 1.23kohm..1.24kohm }
    .supply.pos ~> .a ~> .b ~> .supply.neg
}

board Invented {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .pair: Pair { }
    .supply ~ .pair.supply
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
}
"#;

/// A project with `supply` as its `[supply]` tables and the board above as `src/main.nsm`.
fn project(supply: &str) -> (TempDir, Result<Project, String>) {
    let root = TempDir::new("part-channels");
    let files = [
        (
            "netsmith.toml",
            format!("[project]\nname = \"invented\"\nseries = \"none\"\n\n{supply}"),
        ),
        ("src/main.nsm", BOARD.to_string()),
        ("catalogs/resistor.csv", ROWS.to_string()),
        ("offers/lcsc.csv", LCSC.to_string()),
        ("offers/digikey.csv", DIGIKEY.to_string()),
    ];
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let project = Project::load(root.path());
    (root, project)
}

/// The board of `project` built with no KiCad library.
fn build(project: &Project) -> Result<Outcome, Vec<Problem>> {
    let text = fs::read_to_string(project.root.join("src/main.nsm")).unwrap();
    outcome_with(
        "src/main.nsm",
        &text,
        &project.loader().unwrap(),
        &project.catalogs().unwrap(),
        None,
    )
}

/// The skus the lock entry of the part at `path` records.
fn locked(outcome: &Outcome, path: &str) -> Vec<(String, String)> {
    outcome
        .lock
        .rows("Invented")
        .iter()
        .find(|r| r.path == path)
        .unwrap_or_else(|| panic!("no lock entry for {path}"))
        .skus
        .clone()
}

/// The numbers the bill of materials buys the two resistors by, in designator order.
fn bom(outcome: &Outcome) -> Vec<Option<String>> {
    let mut lines: Vec<_> = outcome.boards[0]
        .sourcing
        .iter()
        .filter(|s| s.mpn.as_deref() == Some("ZZR-1234"))
        .map(|s| (s.refdes.clone(), s.sku.clone()))
        .collect();
    lines.sort();
    lines.into_iter().map(|(_, lcsc)| lcsc).collect()
}

fn sku(channel: &str, sku: &str) -> (String, String) {
    (channel.to_string(), sku.to_string())
}

/// The one problem of a build that failed, as it is printed.
fn refused(project: &Project) -> String {
    match build(project) {
        Ok(_) => panic!("the build succeeded"),
        Err(problems) => render_problems(&problems),
    }
}

#[test]
fn a_part_named_to_a_second_channel_is_bought_there_and_the_rest_follow_the_list() {
    let (_root, p) = project("[supply.parts.Invented]\n\"pair.b\" = \"digikey-retail\"\n");
    let outcome = build(&p.unwrap()).unwrap_or_else(|e| panic!("{}", render_problems(&e)));
    // The default list buys `pair.a` in JLCPCB's assembly, and its lock entry has the sku of
    // each channel of the list.
    assert_eq!(
        locked(&outcome, "pair.a"),
        [
            sku("jlcpcb-assembly", "C900001"),
            sku("lcsc-retail", "C900001")
        ]
    );
    // `pair.b` is bought from Digi-Key, which the project's list does not name, and its lock
    // entry records that channel's sku alone.
    assert_eq!(
        locked(&outcome, "pair.b"),
        [sku("digikey-retail", "ZZR-1234-ND")]
    );
    assert_eq!(
        bom(&outcome),
        [Some("C900001".to_string()), Some("ZZR-1234-ND".to_string())]
    );
}

#[test]
fn a_bare_path_names_a_part_of_the_projects_only_board() {
    let (_root, p) = project("[supply.parts]\n\"pair.a\" = \"lcsc-retail\"\n");
    let outcome = build(&p.unwrap()).unwrap_or_else(|e| panic!("{}", render_problems(&e)));
    assert_eq!(locked(&outcome, "pair.a"), [sku("lcsc-retail", "C900001")]);
    assert_eq!(locked(&outcome, "pair.b").len(), 2);
}

#[test]
fn the_table_sits_beside_the_rest_of_supply_and_renders_back() {
    let (_root, p) = project(
        "[supply]\nchannels = [\"jlcpcb-assembly\"]\n\n[supply.nexar]\nclient_id_env = \"NEXAR_CLIENT_ID\"\nclient_secret_env = \"NEXAR_CLIENT_SECRET\"\n\n[supply.parts]\n\"pair.a\" = \"lcsc-retail\"\n\n[supply.parts.Invented]\n\"pair.b\" = \"digikey-retail\"\n",
    );
    let p = p.unwrap();
    let rendered = p.manifest.render();
    assert!(rendered.contains("[supply.parts]"), "{rendered}");
    assert!(rendered.contains("[supply.parts.Invented]"), "{rendered}");
    let again = netsmith_fabricate::project::Manifest::parse(&rendered).unwrap();
    assert_eq!(again.supply, p.manifest.supply);
    let outcome = build(&p).unwrap_or_else(|e| panic!("{}", render_problems(&e)));
    assert_eq!(locked(&outcome, "pair.a"), [sku("lcsc-retail", "C900001")]);
    assert_eq!(
        locked(&outcome, "pair.b"),
        [sku("digikey-retail", "ZZR-1234-ND")]
    );
}

#[test]
fn an_unknown_channel_word_is_refused_with_the_file_and_the_key() {
    let (_root, p) = project("[supply.parts.Invented]\n\"pair.b\" = \"digikey\"\n");
    let e = p.unwrap_err();
    assert!(
        e.ends_with(": netsmith.toml: [supply.parts.Invented] \"pair.b\" = \"digikey\" is not a supply channel netsmith knows: `jlcpcb-assembly`, `lcsc-retail`, `digikey-retail` or `mouser-retail`"),
        "{e}"
    );
    let (_root, p) = project("[supply.parts.Invented]\npair.b = \"digikey-retail\"\n");
    let e = p.unwrap_err();
    assert!(
        e.contains("[supply.parts.Invented] \"pair\" holds a table, not a supply channel: a part's path with a dot in it is quoted"),
        "{e}"
    );
}

#[test]
fn a_part_the_board_does_not_have_is_refused_with_the_file_and_the_key() {
    let (_root, p) = project("[supply.parts.Invented]\n\"pair.c\" = \"digikey-retail\"\n");
    let e = refused(&p.unwrap());
    assert!(
        e.contains("netsmith.toml: [supply.parts.Invented] \"pair.c\": there is no part at the path `pair.c`: did you mean `pair."),
        "{e}"
    );
    let (_root, p) = project("[supply.parts.Invented]\n\"pair\" = \"digikey-retail\"\n");
    let e = refused(&p.unwrap());
    assert!(
        e.contains("netsmith.toml: [supply.parts.Invented] \"pair\": `pair` is a `Pair`, not a part"),
        "{e}"
    );
}

#[test]
fn a_board_the_project_does_not_have_is_refused_with_the_file_and_the_key() {
    let (_root, p) = project("[supply.parts.Invent]\n\"pair.b\" = \"digikey-retail\"\n");
    let e = refused(&p.unwrap());
    assert!(
        e.contains("netsmith.toml: [supply.parts.Invent] \"pair.b\" names the board or system `Invent` (`Invented`?), which no top-level file of the project declares; the project has Invented."),
        "{e}"
    );
    // A dotted key that is not quoted is a table in TOML: its first name is read as a board.
    let (_root, p) = project("[supply.parts]\npair.b = \"digikey-retail\"\n");
    let e = refused(&p.unwrap());
    assert!(
        e.contains("[supply.parts.pair] \"b\" names the board or system `pair`"),
        "{e}"
    );
    assert!(e.contains("A part's path with a dot in it is quoted"), "{e}");
}

#[test]
fn a_part_with_no_offer_in_its_channel_is_unresolved_by_name() {
    let (_root, p) = project("[supply.parts.Invented]\n\"pair.b\" = \"mouser-retail\"\n");
    let e = refused(&p.unwrap());
    assert!(
        e.contains("`pair.b` cannot be bought: no row of the `resistor` catalog it could be has an offer in `mouser-retail`, the supply channel netsmith.toml names for the part ([supply.parts.Invented] \"pair.b\"); add the offer of its row to the project's `offers/mouser.csv`"),
        "{e}"
    );
}

#[test]
fn a_part_named_twice_is_refused() {
    let (_root, p) = project(
        "[supply.parts]\n\"pair.b\" = \"lcsc-retail\"\n\n[supply.parts.Invented]\n\"pair.b\" = \"digikey-retail\"\n",
    );
    let e = refused(&p.unwrap());
    assert!(
        e.contains("names the part that [supply.parts] \"pair.b\" names too")
            || e.contains("names the part that [supply.parts.Invented] \"pair.b\" names too"),
        "{e}"
    );
}
