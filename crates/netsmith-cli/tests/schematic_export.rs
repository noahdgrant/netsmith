//! The schematic as a document (issue #1091): `netsmith view schematic` writes the editor's
//! drawing as a PDF (a page for each sheet), an SVG or a PNG, and `board fab` puts the PDF in the
//! package unless `--no-pdf` or `pdf = false` says not to. Run with KiCad switched off: none is
//! needed.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn netsmith_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn ok(run: &Output) {
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// The text each page of a PDF shows, its strings in the order they are drawn, from its
/// content streams (inflated) and their `(...) Tj` operators.
fn page_texts(pdf: &[u8]) -> Vec<Vec<String>> {
    const START: &[u8] = b"/FlateDecode >>\nstream\n";
    let mut pages = Vec::new();
    let mut rest = pdf;
    while let Some(at) = find(rest, START) {
        let start = at + START.len();
        let end = start + find(&rest[start..], b"\nendstream").expect("an end");
        let mut content = Vec::new();
        flate2::read::ZlibDecoder::new(&rest[start..end])
            .read_to_end(&mut content)
            .expect("inflates");
        pages.push(strings(&content));
        rest = &rest[end..];
    }
    pages
}

/// The strings of `(...) Tj`, with the escapes undone and each text object's pieces joined
/// (a letter of the Symbol font is a piece of its own).
fn strings(content: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(content);
    let mut out = Vec::new();
    for object in text.split("BT ").skip(1) {
        let object = object.split("ET").next().unwrap_or("");
        let mut joined = String::new();
        let mut rest = object;
        while let Some(open) = rest.find('(') {
            let mut s = String::new();
            let mut chars = rest[open + 1..].char_indices();
            let mut end = rest.len();
            while let Some((i, c)) = chars.next() {
                match c {
                    '\\' => {
                        if let Some((_, e)) = chars.next() {
                            s.push(e);
                        }
                    }
                    ')' => {
                        end = open + 1 + i + 1;
                        break;
                    }
                    c => s.push(c),
                }
            }
            joined.push_str(&s);
            rest = &rest[end..];
        }
        if !joined.is_empty() && !out.contains(&joined) {
            out.push(joined);
        }
    }
    out
}

/// The designators and the net labels the SVG draws, as its `<text>` of class `ref` and
/// `net-label` have them.
fn names_in(svg: &str) -> (Vec<String>, Vec<String>) {
    let doc = roxmltree::Document::parse(svg).expect("an SVG");
    let of = |class: &str| -> Vec<String> {
        let mut found: Vec<String> = doc
            .descendants()
            .filter(|n| {
                n.has_tag_name("text")
                    && n.attribute("class")
                        .is_some_and(|c| c.split_whitespace().any(|w| w == class))
            })
            .map(|n| {
                n.descendants()
                    .filter(|d| d.is_text() && d.parent().is_none_or(|p| !p.has_tag_name("title")))
                    .filter_map(|d| d.text())
                    .collect::<String>()
            })
            .collect();
        found.sort();
        found.dedup();
        found
    };
    (of("ref"), of("net-label"))
}

/// The sheets of the SVG: its module sheets, or the board alone.
fn sheet_count(svg: &str) -> usize {
    svg.matches("class=\"module-sheet\"").count().max(1)
}

#[test]
fn view_schematic_writes_a_page_for_each_sheet_with_every_designator_and_net_label() {
    let project = repo().join("examples/easyduino_esp32s3");
    let dir = netsmith_testkit::TempDir::new("schematic-pdf");
    let pdf = dir.join("a.pdf");
    let svg = dir.join("a.svg");
    ok(&netsmith_in(&project, &["view", "schematic", "--picture", pdf.to_str().unwrap()]));
    ok(&netsmith_in(
        &project,
        &[
            "view",
            "schematic",
            "--format",
            "svg",
            "--picture",
            svg.to_str().unwrap(),
        ],
    ));
    let bytes = std::fs::read(&pdf).unwrap();
    let svg = std::fs::read_to_string(&svg).unwrap();
    assert!(bytes.starts_with(b"%PDF-1.4"));
    let pages = page_texts(&bytes);
    let sheets = sheet_count(&svg);
    assert!(sheets > 1, "the example is drawn by module");
    assert_eq!(pages.len(), sheets);
    assert_eq!(
        String::from_utf8_lossy(&bytes)
            .matches("/Type /Page ")
            .count(),
        sheets
    );
    // Every designator and every net label is on a page, as text.
    let all: Vec<&String> = pages.iter().flatten().collect();
    let (refs, labels) = names_in(&svg);
    assert!(refs.len() > 10 && !labels.is_empty(), "{refs:?} {labels:?}");
    for name in refs.iter().chain(&labels) {
        assert!(
            all.contains(&name),
            "`{name}` is not in the PDF's text"
        );
    }
    // Each page's title block names its sheet of all of them.
    for (i, page) in pages.iter().enumerate() {
        let n = format!("Sheet {} of {sheets}", i + 1);
        assert!(page.contains(&n), "page {}: {page:?}", i + 1);
        assert!(page.iter().any(|t| t == "Project: easyduino_esp32s3"));
    }
    // The same source gives the same bytes.
    let again = dir.join("b.pdf");
    ok(&netsmith_in(&project, &["view", "schematic", "--picture", again.to_str().unwrap()]));
    assert_eq!(bytes, std::fs::read(&again).unwrap());
}

#[test]
fn view_schematic_draws_a_png_and_one_sheet_and_names_the_sheets_it_has() {
    let examples = repo().join("examples");
    let dir = netsmith_testkit::TempDir::new("schematic-png");
    let png = dir.join("led.png");
    let run = netsmith_in(
        &examples,
        &[
            "view",
            "schematic",
            "led.nsm",
            "--format",
            "png",
            "--picture",
            png.to_str().unwrap(),
        ],
    );
    ok(&run);
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).trim(),
        png.to_str().unwrap()
    );
    let bytes = std::fs::read(&png).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    // A board of a few parts is one sheet, named for the board: `--sheet` takes that name.
    let one = dir.join("one.pdf");
    ok(&netsmith_in(
        &examples,
        &[
            "view",
            "schematic",
            "led.nsm",
            "--sheet",
            "LedExample",
            "--picture",
            one.to_str().unwrap(),
        ],
    ));
    assert_eq!(page_texts(&std::fs::read(&one).unwrap()).len(), 1);
    let run = netsmith_in(
        &examples,
        &["view", "schematic", "led.nsm", "--sheet", "power"],
    );
    assert_eq!(run.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("no sheet `power`") && stderr.contains("`LedExample`"),
        "{stderr}"
    );
}

const NOTHING_TO_ROUTE: &str = "import std::parts::connector::Header1x2\nimport std::mfg::jlcpcb::TwoLayer\n\nboard Lonely {\n    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n    .h: Header1x2\n}\n";

#[test]
fn fab_writes_the_schematic_pdf_view_schematic_writes_and_the_switches_leave_it_out() {
    let dir = netsmith_testkit::TempDir::new("fab-schematic-pdf");
    std::fs::write(dir.join("a.nsm"), NOTHING_TO_ROUTE).unwrap();
    let pdf = dir.join("out/Lonely/Lonely-schematic.pdf");
    ok(&netsmith_in(&dir, &["board", "fab", "a.nsm", "--out", "out"]));
    let written = std::fs::read(&pdf).expect("fab writes <board>-schematic.pdf");
    let drawn = dir.join("drawn.pdf");
    ok(&netsmith_in(
        &dir,
        &["view", "schematic", "a.nsm", "--picture", drawn.to_str().unwrap()],
    ));
    assert_eq!(written, std::fs::read(&drawn).unwrap());
    // Not for the board house: the Gerber zip has no PDF.
    let zip = std::fs::File::open(dir.join("out/Lonely/Lonely-gerbers.zip")).unwrap();
    let archive = zip::ZipArchive::new(zip).unwrap();
    assert!(archive.file_names().all(|n| !n.ends_with(".pdf")));

    // `--no-pdf` leaves it out and removes the one of the earlier run.
    ok(&netsmith_in(
        &dir,
        &["board", "fab", "a.nsm", "--out", "out", "--no-pdf"],
    ));
    assert!(!pdf.exists());
    assert!(dir.join("out/Lonely/Lonely.step").is_file());

    // `pdf = false` in netsmith.toml does the same for the project, in `fab` and in an order.
    std::fs::create_dir(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/main.nsm"), NOTHING_TO_ROUTE).unwrap();
    std::fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"p\"\npdf = false\n\n[order.one.boards]\nLonely = 1\n",
    )
    .unwrap();
    ok(&netsmith_in(&dir, &["board", "fab", "--out", "kept"]));
    assert!(dir.join("kept/Lonely/Lonely.kicad_pcb").is_file());
    assert!(!dir.join("kept/Lonely/Lonely-schematic.pdf").exists());
    ok(&netsmith_in(&dir, &["project", "order", "one", "--out", "order"]));
    assert!(!walk(&dir.join("order")).iter().any(|f| f.ends_with(".pdf")));

    // Without the key the project's PDF names the project, and an order has it.
    std::fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"p\"\n\n[order.one.boards]\nLonely = 1\n",
    )
    .unwrap();
    ok(&netsmith_in(&dir, &["project", "order", "one", "--out", "order-on"]));
    let made = walk(&dir.join("order-on"));
    let pdf = made
        .iter()
        .find(|f| f.ends_with("Lonely-schematic.pdf"))
        .unwrap_or_else(|| panic!("{made:?}"));
    let pages = page_texts(&std::fs::read(pdf).unwrap());
    assert!(pages[0].iter().any(|t| t == "Project: p"), "{pages:?}");
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

#[test]
fn a_revision_in_the_manifest_is_in_the_title_block_the_fab_notes_and_the_order() {
    let dir = netsmith_testkit::TempDir::new("revision");
    std::fs::create_dir(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/main.nsm"), NOTHING_TO_ROUTE).unwrap();
    let manifest = |key: &str| {
        std::fs::write(
            dir.join("netsmith.toml"),
            format!("[project]\nname = \"p\"\n{key}\n[order.one.boards]\nLonely = 1\n"),
        )
        .unwrap();
    };
    let first_line = |file: PathBuf| {
        let text = std::fs::read_to_string(file).unwrap();
        text.lines().next().unwrap().to_string()
    };
    let shown = |pdf: PathBuf| -> Vec<String> {
        page_texts(&std::fs::read(pdf).unwrap())
            .into_iter()
            .flatten()
            .collect()
    };

    // With a revision: `Rev B` beside the sheet's name, "Drawn by netsmith" stays, and the notes
    // and the order begin with it; the files keep their names.
    manifest("revision = \"B\"\n");
    ok(&netsmith_in(&dir, &["board", "fab", "--out", "fab-b"]));
    let pdf = dir.join("fab-b/Lonely/Lonely-schematic.pdf");
    let texts = shown(pdf.clone());
    assert!(texts.iter().any(|t| t == "Rev B"), "{texts:?}");
    assert!(texts.iter().any(|t| t.starts_with("Drawn by netsmith ")));
    assert_eq!(
        first_line(dir.join("fab-b/Lonely/fab-notes.txt")),
        "fab-notes for Lonely, revision B"
    );
    for kept in ["bom.csv", "cpl.csv", "Lonely-gerbers.zip"] {
        assert!(dir.join("fab-b/Lonely").join(kept).is_file(), "{kept}");
    }
    // `view schematic` and the order say the same.
    let drawn = dir.join("drawn.pdf");
    ok(&netsmith_in(
        &dir,
        &["view", "schematic", "--picture", drawn.to_str().unwrap()],
    ));
    assert_eq!(std::fs::read(&pdf).unwrap(), std::fs::read(&drawn).unwrap());
    ok(&netsmith_in(&dir, &["project", "order", "one", "--out", "order-b"]));
    assert_eq!(
        first_line(dir.join("order-b/orders/one/order.txt")),
        "p, revision B"
    );
    let notes = walk(&dir.join("order-b"))
        .into_iter()
        .find(|f| f.ends_with("fab-notes.txt"))
        .expect("the order's package has notes");
    assert_eq!(first_line(notes), "fab-notes for Lonely, revision B");

    // Without one: no `Rev`, and the first lines are as they were.
    manifest("");
    ok(&netsmith_in(&dir, &["board", "fab", "--out", "fab-none"]));
    let texts = shown(dir.join("fab-none/Lonely/Lonely-schematic.pdf"));
    assert!(texts.iter().all(|t| !t.starts_with("Rev ")), "{texts:?}");
    assert_eq!(
        first_line(dir.join("fab-none/Lonely/fab-notes.txt")),
        "fab-notes for Lonely"
    );
    ok(&netsmith_in(&dir, &["project", "order", "one", "--out", "order-none"]));
    assert!(
        first_line(dir.join("order-none/orders/one/order.txt")).starts_with("order one: "),
    );

    // A revision that is over 32 characters, or not on one line, is refused with its line.
    for (key, why) in [
        (format!("revision = \"{}\"\n", "x".repeat(33)), "is 33 characters"),
        ("revision = \"A\\nB\"\n".to_string(), "one line"),
    ] {
        manifest(&key);
        let run = netsmith_in(&dir, &["board", "fab", "--out", "fab-bad"]);
        assert!(!run.status.success());
        let said = String::from_utf8_lossy(&run.stderr);
        assert!(
            said.contains("netsmith.toml:3: revision ") && said.contains(why),
            "{said}"
        );
    }
}
