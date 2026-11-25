//! `board fab --for-review` (issues #1189, #1212, #1213): a board that breaks its own height
//! limit and its requirements on where parts go is still written for a person to look at, with
//! every finding waived listed under `FOR REVIEW` at the top of the notes, and the pictures of its
//! copper that `view pcb` draws. Run with the sample libraries and no KiCad. The numbers are an
//! invented case drawing.

use netsmith_testkit::TempDir;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

/// A 40 by 24 mm board with a supply header, a resistor and an LED.
const BOARD: &str = "\
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Widget {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }
    .led: Led {
        .current &= 5mA..15mA
    }
    .r1: Resistor {}
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
";

/// The case drawing of `BOARD`: the header on the left edge, and a lid over the top right that
/// allows 3 mm on the top face, where the layout file moves the 9 mm header.
const PHYSICAL: &str = "\
import std::pcb::{Keepout, Outline}

physical for Widget {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 24mm
        }
    }

    .power_in.place.left &= 1.5mm +/- 0.1mm
    .power_in.place.rotation &= 90
    .power_in.height &= 9mm

    .lid: Keepout {
        .x &= 24mm
        .y &= 0mm
        .width &= 12mm
        .height &= 12mm
        .layers &= \"top\"
        .max_height &= 3mm
    }
}
";

/// A layout file that moves the header by hand, under the lid and out of its window.
const MOVED: &str = "version = 1\n[Widget.parts.power_in]\nat = [30.0, 5.0]\n";

/// The project of `BOARD`, with an order of the board.
fn widget(name: &str) -> TempDir {
    let root = TempDir::unused(&format!("for-review-test-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"widget\"\ntop_level = [\"src/main.nsm\"]\n\n[order.kit.boards]\nWidget = 2\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), BOARD).unwrap();
    fs::write(root.join("src/main.physical.nsm"), PHYSICAL).unwrap();
    fs::write(root.join("src/main.pcb.toml"), MOVED).unwrap();
    root
}

/// The editor recipes' lidded board (`scripts/editor-verify-js/fixtures/height_limits.nsm`): a
/// part 9 mm tall under a lid that allows 3 mm, no connections, and no design rules error.
fn lidded(name: &str) -> TempDir {
    let root = TempDir::unused(&format!("for-review-test-{name}"));
    fs::create_dir_all(&*root).unwrap();
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/editor-verify-js/fixtures");
    for file in [
        "height_limits.nsm",
        "height_limits.physical.nsm",
        "height_limits.pcb.toml",
    ] {
        fs::copy(fixtures.join(file), root.join(file)).unwrap();
    }
    root
}

fn netsmith_here(root: &Path, args: &[&str]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(root)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env("NETSMITH_KICAD_SYMBOLS", netsmith_testkit::sample_symbols())
        .env(
            "NETSMITH_KICAD_FOOTPRINTS",
            netsmith_testkit::sample_footprints(),
        )
        .output()
        .unwrap()
}

fn said(run: &Output) -> String {
    String::from_utf8_lossy(&run.stderr).to_string()
}

fn files_of(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|rd| {
            rd.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The heading of the notes and the findings listed under it: the line after the first, and the
/// lines after it indented by two spaces.
fn review_heading(notes: &str) -> (String, Vec<String>) {
    let mut lines = notes.lines().skip(1);
    let heading = lines.next().unwrap_or_default().to_string();
    let listed = lines
        .take_while(|l| l.starts_with("  "))
        .map(|l| l.trim_start().to_string())
        .collect();
    (heading, listed)
}

/// Issue #1212: a part over a keepout's `.max_height` stops a plain `fab` at placement and writes
/// nothing; `--for-review` writes the files to look at, lists that finding and the requirements
/// not met under `FOR REVIEW`, exits 1, and an order still refuses the board.
#[test]
fn a_part_over_a_height_limit_stops_fab_and_for_review_writes_the_package_with_every_finding_first()
{
    let root = widget("height");
    let board = root.join("fab/Widget");

    // A plain `fab` stops at placement with the height finding, and writes no package.
    let run = netsmith_here(&root, &["board", "fab", "--out", "fab"]);
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    assert!(
        said(&run).contains("`power_in` (J1) stands 9.00 mm tall on the top face, and the keepout `lid` allows 3.00 mm there"),
        "{}",
        said(&run)
    );
    let files = files_of(&board);
    assert!(
        !files
            .iter()
            .any(|f| f.ends_with(".zip") || f.ends_with(".csv") || f == "fab-notes.txt"),
        "{files:?}"
    );

    // `--for-review` writes the files to look at, and the run still fails.
    let run = netsmith_here(&root, &["board", "fab", "--out", "fab", "--for-review"]);
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    assert!(said(&run).contains("FOR REVIEW, NOT FOR ORDER"), "{}", said(&run));
    let files = files_of(&board);
    for file in [
        "Widget.kicad_pcb",
        "Widget.kicad_pro",
        "Widget.svg",
        "Widget-gerbers-REVIEW.zip",
        "Widget-top-REVIEW.png",
        "Widget-bottom-REVIEW.png",
        "fab-notes.txt",
    ] {
        assert!(files.contains(&file.to_string()), "{file} in {files:?}");
    }
    assert!(
        !files
            .iter()
            .any(|f| f == "bom.csv" || f == "cpl.csv" || f == "Widget-gerbers.zip"),
        "{files:?}"
    );

    // The notes begin with FOR REVIEW and list every finding waived, each with its rule, the
    // part, the numbers and the place: the height first, then the requirements not met.
    let notes = fs::read_to_string(board.join("fab-notes.txt")).unwrap();
    let (heading, listed) = review_heading(&notes);
    assert!(
        heading.starts_with("FOR REVIEW, NOT FOR ORDER: `--for-review` waived ")
            && heading.contains("project order")
            && heading.contains("Widget-top-REVIEW.png, Widget-bottom-REVIEW.png"),
        "{notes}"
    );
    assert!(
        heading.contains(&format!("waived {} findings", listed.len())),
        "{notes}"
    );
    assert!(
        listed[0].starts_with("error [too_tall] `power_in` (J1) stands 9.00 mm tall")
            && listed[0].contains("the keepout `lid` allows 3.00 mm there (6.00 mm too tall)")
            && listed[0].contains("around (30.0, "),
        "{notes}"
    );
    for requirement in ["power_in.place.left", "power_in.place.rotation"] {
        assert!(
            listed
                .iter()
                .any(|l| l.starts_with("error [constraint] ") && l.contains(requirement)),
            "{requirement} in {notes}"
        );
    }
    assert!(
        notes.contains("design rules check: NOT clean, FOR REVIEW, NOT FOR ORDER: "),
        "{notes}"
    );

    // As JSON each is a finding of code `fab` waived for review, with its data.
    let run = netsmith_here(
        &root,
        &["board", "fab", "--out", "fab", "--for-review", "--format", "json"],
    );
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(doc["ok"], false, "{doc}");
    let waived: Vec<&serde_json::Value> = doc["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["waived"] == "for-review")
        .collect();
    assert_eq!(waived.len(), listed.len(), "{doc}");
    assert!(
        waived
            .iter()
            .all(|f| f["code"] == "fab" && f["severity"] == "error" && f["board"] == "Widget"),
        "{doc}"
    );
    let tall = waived
        .iter()
        .find(|f| f["rule"] == "too_tall")
        .unwrap_or_else(|| panic!("{doc}"));
    assert_eq!(tall["parts"], serde_json::json!(["J1"]), "{tall}");
    let fault = &tall["placement"][0];
    assert!(
        fault["kind"] == "too_tall"
            && fault["keepout"] == "lid"
            && fault["height"] == 9.0
            && fault["limit"] == 3.0
            && fault["face"] == "top",
        "{tall}"
    );
    assert!(
        waived
            .iter()
            .any(|f| f["rule"] == "constraint" && f["place"]["x"].is_number()),
        "{doc}"
    );

    // An order does not take the folder: it makes the board again, and the height stops it.
    let run = netsmith_here(&root, &["project", "order", "kit", "--out", "fab"]);
    assert!(!run.status.success(), "{}", said(&run));
    assert!(!root.join("fab/order.txt").exists());
    assert!(
        !files_of(&board)
            .iter()
            .any(|f| f == "bom.csv" || f == "cpl.csv")
    );
}

/// Issue #1212: a board whose design rules check passes but that breaks its height limit is
/// packaged for review all the same (never to order from), and a plain `fab` of it once the
/// limit is met takes away what the review left.
#[test]
fn a_board_whose_only_finding_is_a_height_is_packaged_for_review_and_never_to_order() {
    let root = lidded("lidded");
    let board = root.join("fab/Lidded");
    let run = netsmith_here(
        &root,
        &["board", "fab", "height_limits.nsm", "--out", "fab", "--for-review"],
    );
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    let notes = fs::read_to_string(board.join("fab-notes.txt")).unwrap();
    let (heading, listed) = review_heading(&notes);
    assert!(heading.contains("waived 1 finding, "), "{notes}");
    assert!(
        listed.len() == 1 && listed[0].starts_with("error [too_tall] `tall` (J3) stands 9.00 mm"),
        "{notes}"
    );
    assert!(
        notes.contains(
            "design rules check: no errors, and the package is FOR REVIEW, NOT FOR ORDER"
        ),
        "{notes}"
    );
    let files = files_of(&board);
    for file in [
        "Lidded-gerbers-REVIEW.zip",
        "Lidded-top-REVIEW.png",
        "Lidded-bottom-REVIEW.png",
    ] {
        assert!(files.contains(&file.to_string()), "{file} in {files:?}");
    }
    assert!(
        !files.iter().any(|f| f == "bom.csv" || f == "cpl.csv"),
        "{files:?}"
    );

    // With the lid gone the board passes, and a plain `fab` writes the package to order from and
    // removes the review's zip and pictures.
    let physical = fs::read_to_string(root.join("height_limits.physical.nsm")).unwrap();
    let start = physical.find("    .lid: Keepout {").unwrap();
    let end = start + physical[start..].find("    }\n").unwrap() + "    }\n".len();
    fs::write(
        root.join("height_limits.physical.nsm"),
        format!("{}{}", &physical[..start], &physical[end..]),
    )
    .unwrap();
    let run = netsmith_here(&root, &["board", "fab", "height_limits.nsm", "--out", "fab"]);
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    let files = files_of(&board);
    assert!(
        files.contains(&"Lidded-gerbers.zip".to_string())
            && files.contains(&"bom.csv".to_string()),
        "{files:?}"
    );
    assert!(!files.iter().any(|f| f.contains("REVIEW")), "{files:?}");
    let notes = fs::read_to_string(board.join("fab-notes.txt")).unwrap();
    assert!(!notes.contains("FOR REVIEW"), "{notes}");
}

/// Issue #1213: the pictures of a review package are the bytes `view pcb --layer <layer> --drc
/// --islands` writes of the board `fab` routed (the router's copper kept with `--reuse-routes`, so
/// that `view pcb` draws the same copper), and `--no-pictures` leaves them out.
#[test]
fn the_pictures_of_a_review_package_are_the_bytes_view_pcb_draws_of_the_board_fab_routed() {
    let root = widget("pictures");
    let board = root.join("fab/Widget");
    let run = netsmith_here(
        &root,
        &[
            "board",
            "fab",
            "--out",
            "fab",
            "--for-review",
            "--reuse-routes",
        ],
    );
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    let pictures: Vec<String> = files_of(&board)
        .into_iter()
        .filter(|f| f.ends_with(".png"))
        .collect();
    assert_eq!(
        pictures,
        ["Widget-bottom-REVIEW.png", "Widget-top-REVIEW.png"],
        "a picture for each copper layer of a two-layer board"
    );
    for layer in ["top", "bottom"] {
        let picture = root.join(format!("view-{layer}.png"));
        let view = netsmith_here(
            &root,
            &[
                "view",
                "pcb",
                "--layer",
                layer,
                "--drc",
                "--islands",
                "--picture",
                picture.to_str().unwrap(),
            ],
        );
        assert_eq!(view.status.code(), Some(0), "{}", said(&view));
        let made = fs::read(board.join(format!("Widget-{layer}-REVIEW.png"))).unwrap();
        assert!(made.starts_with(b"\x89PNG"), "{layer}");
        assert!(
            made == fs::read(&picture).unwrap(),
            "the {layer} picture differs from `view pcb --layer {layer} --drc --islands`"
        );
    }

    // `--no-pictures` writes none, removes the earlier run's, and the notes name none.
    let run = netsmith_here(
        &root,
        &["board", "fab", "--out", "fab", "--for-review", "--no-pictures"],
    );
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    let files = files_of(&board);
    assert!(!files.iter().any(|f| f.ends_with(".png")), "{files:?}");
    assert!(files.contains(&"Widget-gerbers-REVIEW.zip".to_string()));
    let notes = fs::read_to_string(board.join("fab-notes.txt")).unwrap();
    assert!(!review_heading(&notes).0.contains("pictures"), "{notes}");
    // And it means nothing without `--for-review`.
    let run = netsmith_here(&root, &["board", "fab", "--no-pictures"]);
    assert_eq!(run.status.code(), Some(2), "{}", said(&run));
}

/// Issue #1707: `--all-refusals` lists every refusal a fab would make, here the part over the
/// lid's height and the errors of the design rules check (the requirements on where the header
/// goes), writes no package, and exits 1 as a refusing fab does; a plain `fab` still stops at the
/// first, the height, and says nothing of the check.
#[test]
fn all_refusals_lists_a_height_and_the_design_rules_errors_and_writes_nothing() {
    let root = widget("all-refusals");
    let board = root.join("fab/Widget");

    let run = netsmith_here(&root, &["board", "fab", "--out", "fab", "--all-refusals"]);
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    let text = said(&run);
    let heading = text
        .lines()
        .find(|l| l.starts_with("error: Widget: "))
        .unwrap_or_else(|| panic!("{text}"));
    assert!(
        heading.contains("refusals a fab would make (`--all-refusals`: no package was written)"),
        "{text}"
    );
    assert!(
        text.contains("  error [too_tall] `power_in` (J1) stands 9.00 mm tall"),
        "{text}"
    );
    assert!(text.contains("[constraint] ") && text.contains("power_in.place.left"), "{text}");
    assert!(!text.contains("not reached"), "{text}");
    assert!(!root.join("fab").exists(), "{:?}", files_of(&board));

    // As JSON each refusal is an error of code `fab` with its rule, none of them waived.
    let run = netsmith_here(
        &root,
        &["board", "fab", "--out", "fab", "--all-refusals", "--format", "json"],
    );
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(doc["ok"], false, "{doc}");
    let refusals: Vec<&serde_json::Value> = doc["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "fab" && f["severity"] == "error")
        .collect();
    for rule in ["too_tall", "constraint"] {
        assert!(
            refusals
                .iter()
                .any(|f| f["rule"] == rule && f["board"] == "Widget" && f["waived"].is_null()),
            "{rule} in {doc}"
        );
    }
    assert!(!root.join("fab").exists());

    // Without the flag `fab` stops at the height, as before, and says nothing of the check.
    let run = netsmith_here(&root, &["board", "fab", "--out", "fab"]);
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    assert!(said(&run).contains("stands 9.00 mm tall"), "{}", said(&run));
    assert!(!said(&run).contains("[constraint]"), "{}", said(&run));
    assert!(!said(&run).contains("--all-refusals"), "{}", said(&run));

    // The order lists the board's refusals, and writes nothing of the order.
    let run = netsmith_here(&root, &["project", "order", "kit", "--out", "fab", "--all-refusals"]);
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    assert!(said(&run).contains("[too_tall]"), "{}", said(&run));
    assert!(said(&run).contains("[constraint]"), "{}", said(&run));
    assert!(!root.join("fab/orders").exists());
}

/// Issue #1707: a refusal that leaves the stages after it nothing to check (the parts do not
/// fit the board) says which checks were not reached.
#[test]
fn all_refusals_says_which_checks_a_board_that_does_not_fit_did_not_reach() {
    let root = widget("all-refusals-not-reached");
    // The header moved off the board: the entries' check refuses it before placing.
    fs::write(
        root.join("src/main.pcb.toml"),
        "version = 1\n[Widget.parts.power_in]\nat = [60.0, 5.0]\nlock = true\n",
    )
    .unwrap();
    let run = netsmith_here(&root, &["board", "fab", "--out", "fab", "--all-refusals"]);
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    assert!(said(&run).contains("  error [place] the requirements on where parts go cannot all be met"), "{}", said(&run));
    assert!(
        said(&run).contains("  not reached: placing, the placement check after placing, routing, the design rules check"),
        "{}",
        said(&run)
    );
    assert!(!root.join("fab").exists());
}
