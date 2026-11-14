//! The language guide and the agent skills cannot rot: every example in the guide is run.
//!
//! The guide is one file for each chapter, `docs/language/NN-topic.md`, read in file-name order
//! (issue #615); `docs/LANGUAGE.md` is its index. A message that names a block names the chapter
//! file and the line in it, `docs/language/02-quick-start.md:41`.
//!
//! Fenced blocks are marked by what they promise:
//!
//! - ` ```nsm ` is a complete file and must check.
//! - ` ```nsm solve ` must also solve, design and export (what `netsmith build` does).
//! - ` ```nsm solve report ` must build, and the next ` ```text ` block is the board report.
//! - ` ```nsm evaluate ` must check and solve with nothing reported, but is not exported: for
//!   a board of parts that have no KiCad symbol yet, whose ratings the solver still checks.
//! - ` ```nsm solve place ` must also place on the sample libraries with no finding of the
//!   placement check (`fab` would stop on one); ` ```nsm solve place refused ` must build and
//!   not place, and the next ` ```text ` block is the findings as `fab` prints them.
//! - ` ```nsm solve warns ` must build, and the next ` ```text ` block is the guideline
//!   warnings (ADR 0036), one per line, as the command line prints them after `warning: `.
//!   ` ```nsm evaluate warns ` is the same for a board of parts that have no KiCad symbol in the
//!   sample libraries: it is built with no library read, as the editor builds.
//! - ` generics ` on a ` ```nsm solve ` block offers generic parts after the catalogs' rows, as a
//!   project does by default (ADR 0047, `series = "E24"`).
//! - ` ```nsm solve notes ` must build, and the next ` ```text ` block is the notes that name a
//!   net with a current that is not known, whose tracks are a lower bound (ADR 0036), one per
//!   line, as the command line prints them after `note: `.
//! - ` ```nsm solve stock ` must build, and the next ` ```text ` block is the notes about a
//!   picked part that a file says is out of stock (issue #176), one per line, as the command
//!   line prints them after `note: `.
//! - ` ```nsm evaluate picks ` must build as `evaluate` does but through the whole pick of
//!   providers (ADR 0068, no KiCad needed), and the next ` ```text ` block is the line for each use
//!   of a generic, as the command line prints it after `note: `.
//! - ` ```nsm solve circular ` must build, and the next ` ```text ` block is the note that
//!   names values that depend on each other while none has a value, as the command line
//!   prints it after `note: `. With `warns` too (`solve warns circular`), the first
//!   ` ```text ` block is the warning and the second the note.
//! - ` ```nsm solve started ` must build, and the next ` ```text ` block is the note that says
//!   the solver started a circle of values itself (ADR 0053), as the command line prints it
//!   after `note: `.
//! - ` ```nsm fails ` must not build, and the next ` ```text ` block is the exact message.
//! - ` ```nsm-physical ` directly after an ` ```nsm ` block is that block's physical file
//!   (ADR 0048, `main.physical.nsm` beside `main.nsm`): the block is checked and built with
//!   it. Whatever the ` ```nsm ` block promises (a message in the next ` ```text ` block,
//!   which then follows the physical block) holds for the two files together.
//! - ` file=examples/led.nsm ` on any of them makes the block equal that file.
//! - ` ```csv file=catalogs/resistor.csv ` is a project's catalog file (ADR 0022): it adds its
//!   rows to the catalog named by the file, for the `nsm` blocks that use the same `set=`
//!   (default: none). ` ```csv fails ` must not load, and the next ` ```text ` block is the
//!   exact message. An `nsm` block names its set with ` catalogs=<set> `.
//! - ` module=src::power ` makes the block a project module that other blocks may import.
//! - ` ```nsm-fragment ` is a piece of a file and is not run.
//!
//! Run with `GUIDE_BLESS=1` to rewrite the message blocks from what the tool prints.
//!
//! An example that must fail may not depend on what the standard catalogs hold: a rating just
//! above the largest row, or a value no row has, stops failing when a row is added. Use a rating
//! or a value far beyond any real part, or a requirement that contradicts itself.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use netsmith_pipeline::{Libraries, build_with, outcome_with};
use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, solve, solve_system};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One fenced block of the guide.
struct Block {
    /// The words after the opening fence.
    info: Vec<String>,
    text: String,
    /// The chapter the block is in, as the path from the repository root.
    file: String,
    /// The 1-based line of the opening fence, in that file.
    line: usize,
    /// The range of lines (0-based, end exclusive) holding `text`.
    body: (usize, usize),
}

impl Block {
    /// Where the block is, for a message: `docs/language/02-quick-start.md:41`.
    fn at(&self) -> String {
        format!("{}:{}", self.file, self.line)
    }

    fn is(&self, word: &str) -> bool {
        self.info.first().is_some_and(|w| w == word)
    }

    fn has(&self, word: &str) -> bool {
        self.info.iter().any(|w| w == word)
    }

    fn value(&self, key: &str) -> Option<&str> {
        self.info
            .iter()
            .find_map(|w| w.strip_prefix(key).and_then(|v| v.strip_prefix('=')))
    }
}

fn blocks(file: &str, markdown: &str) -> Vec<Block> {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some(info) = lines[i].strip_prefix("```") {
            let start = i + 1;
            let mut end = start;
            while end < lines.len() && !lines[end].starts_with("```") {
                end += 1;
            }
            out.push(Block {
                info: info.split_whitespace().map(str::to_string).collect(),
                text: lines[start..end].join("\n"),
                file: file.to_string(),
                line: i + 1,
                body: (start, end),
            });
            i = end + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// The chapters of the guide, in order, with the path of each from the repository root.
fn chapters() -> Vec<(String, String)> {
    let chapters = netsmith_testkit::guide_chapters();
    assert!(
        chapters.len() >= 10,
        "docs/language/ holds only {} chapters",
        chapters.len()
    );
    chapters
}

/// The text of the whole guide, the chapters one after the other.
fn guide() -> String {
    netsmith_testkit::guide_text()
}

/// Every fenced block of the guide, the chapters in order. A block is read with the blocks
/// next to it (the message block that follows it), but only those of its own chapter.
fn guide_blocks() -> Vec<Block> {
    chapters()
        .iter()
        .flat_map(|(file, text)| blocks(file, text))
        .collect()
}

fn loader(blocks: &[Block]) -> Loader {
    let mut loader = Loader::std();
    for b in blocks.iter().filter(|b| b.is("nsm")) {
        if let Some(module) = b.value("module") {
            loader.insert(
                module,
                &format!("{}.nsm", module.replace("::", "/")),
                &b.text,
            );
        }
    }
    loader
}

/// The catalogs the `nsm` blocks of each `set=` use: the standard ones and the rows of the
/// set's `csv` blocks, which are files named by `file=` (`catalogs/resistor.csv` adds to
/// the `resistor` catalog). A set whose files do not load holds the problem.
fn catalog_sets(blocks: &[Block]) -> HashMap<String, Result<Catalogs, String>> {
    let mut sets: HashMap<String, Result<Catalogs, String>> = HashMap::new();
    for b in blocks.iter().filter(|b| b.is("csv") && !b.has("fails")) {
        let set = b.value("set").unwrap_or("").to_string();
        let file = b
            .value("file")
            .unwrap_or_else(|| panic!("{}: a csv block needs file=", b.at()));
        let name = Path::new(file).file_stem().unwrap().to_str().unwrap();
        let entry = sets.entry(set).or_insert_with(|| Ok(Catalogs::jlcpcb()));
        if let Ok(catalogs) = entry
            && let Err(e) = catalogs.add_file(name, file, &b.text)
        {
            *entry = Err(e);
        }
    }
    sets
}

/// What `fab` would stop on after placing `design` on `libraries`, one line each, as it
/// prints them: the placer's refusal, or the findings of the placement check.
fn placement_findings(design: &netsmith_core::Design, libraries: &Libraries) -> Vec<String> {
    let placed = netsmith_placer::board_from_design_placed(
        design,
        &libraries.mapping,
        &libraries.footprints,
        &netsmith_pcb::place_options(design, None),
    );
    match placed {
        Err(e) => vec![format!("error: {}: {e}", design.name)],
        Ok(board) => {
            let mut found: Vec<String> = board
                .check_placement()
                .iter()
                .map(|p| format!("error: {}: {p}", design.name))
                .collect();
            // The requirements on where parts go, measured on the board as `fab` does.
            if let Some(constraints) = netsmith_pipeline::constraints::of_design(design, false) {
                match netsmith_geom::Artwork::parse(&netsmith_pcb::kicad_pcb(&board)) {
                    Ok(art) => found.extend(
                        constraints
                            .findings(&art)
                            .iter()
                            .map(|f| format!("error: {}: {}", design.name, f.message)),
                    ),
                    Err(e) => found.push(format!("error: {}: {e}", design.name)),
                }
            }
            found
        }
    }
}

fn same(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    };
    norm(a) == norm(b)
}

#[test]
fn every_manifest_in_the_guide_is_a_manifest() {
    let mut found = 0;
    for b in guide_blocks()
        .iter()
        .filter(|b| b.is("toml") && b.text.starts_with("[project]"))
    {
        found += 1;
        netsmith_fabricate::project::Manifest::parse(&b.text)
            .unwrap_or_else(|e| panic!("{}: {e}", b.at()));
    }
    assert!(found >= 3, "found only {found} manifests");
}

/// The routes file the guide shows (issue #599) is one that netsmith reads, and writes back as
/// it is shown, one shape a line.
#[test]
fn the_routes_file_in_the_guide_is_a_routes_file() {
    let blocks = guide_blocks();
    let block = blocks
        .iter()
        .find(|b| b.is("toml") && b.text.contains(".nets."))
        .expect("the guide shows a routes file");
    let routes = netsmith_pcb::Routes::parse(&block.text, "the guide")
        .unwrap_or_else(|e| panic!("{}: {e}", block.at()));
    let shown: Vec<&str> = block
        .text
        .lines()
        .map(|l| l.split(" #").next().unwrap().trim_end())
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect();
    assert_eq!(
        routes.to_text().trim(),
        shown.join("\n").trim(),
        "{}",
        block.at()
    );
}

/// The orders of the guide's manifest are the ones the text describes, and the error it shows
/// for a board that is not there is what `resolve` says.
#[test]
fn the_orders_in_the_guide_are_read_and_their_error_is_the_real_one() {
    use netsmith_fabricate::order::resolve;
    let guide = guide();
    let blocks = guide_blocks();
    let manifest = blocks
        .iter()
        .find(|b| b.is("toml") && b.text.contains("[order.devkit.boards]"))
        .expect("the guide has a manifest with an order");
    let m = netsmith_fabricate::project::Manifest::parse(&manifest.text).unwrap();
    let names: Vec<&str> = m.orders.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["devkit", "spares"]);

    let boards: Vec<String> = ["DevKit/carrier", "DevKit/power_card", "Unit"]
        .map(String::from)
        .to_vec();
    let spares = resolve(&m.orders[1], &boards).unwrap();
    assert_eq!(
        spares
            .iter()
            .map(|s| (s.label.as_str(), s.quantity))
            .collect::<Vec<_>>(),
        [("DevKit/carrier", 5), ("DevKit/power_card", 5)]
    );
    // The guide's example with `Unitt` for `Unit`, on line 8 of the manifest.
    let typo =
        netsmith_fabricate::project::Manifest::parse(&manifest.text.replace("Unit =", "Unitt =")).unwrap();
    let e = resolve(&typo.orders[0], &boards).unwrap_err();
    let shown = guide
        .lines()
        .find(|l| l.starts_with("netsmith.toml:") && l.contains("Unitt"))
        .expect("the guide shows the error");
    assert_eq!(e.trim_end(), shown, "{e}");
}

#[test]
fn every_fence_says_what_it_is() {
    for b in guide_blocks() {
        let known = [
            "nsm",
            "nsm-physical",
            "nsm-fragment",
            "text",
            "sh",
            "toml",
            "csv",
        ];
        assert!(
            b.info.first().is_some_and(|w| known.contains(&w.as_str())),
            "{}: a fenced block must start with one of {known:?}, not {:?}",
            b.at(),
            b.info
        );
    }
}

#[test]
fn every_complete_example_checks_solves_and_fails_as_the_guide_says() {
    let blocks = guide_blocks();
    let loader = loader(&blocks);
    let sets = catalog_sets(&blocks);
    for (set, loaded) in &sets {
        if let Err(e) = loaded {
            panic!("the csv blocks of set `{set}` do not load: {e}");
        }
    }
    let standard = Catalogs::jlcpcb();
    let bless = std::env::var_os("GUIDE_BLESS").is_some();
    // (file, first line, last line, new text) of message blocks to rewrite.
    let mut rewrites: Vec<(String, usize, usize, String)> = Vec::new();
    let mut ran = 0;
    let mut with_physical = 0;

    for (n, b) in blocks.iter().enumerate() {
        if b.is("nsm-physical") {
            let at = b.at();
            assert!(
                n > 0 && blocks[n - 1].file == b.file && blocks[n - 1].is("nsm"),
                "{at}: an nsm-physical block is the physical file of the nsm block right before it"
            );
            if let Some(file) = b.value("file") {
                let real = fs::read_to_string(root().join(file)).expect("the named file exists");
                assert!(same(&b.text, &real), "{at}: the block differs from {file}");
            }
        }
        if b.is("csv") && b.has("fails") {
            let at = b.at();
            let file = b.value("file").expect("a csv block needs file=");
            let name = Path::new(file).file_stem().unwrap().to_str().unwrap();
            let message = match Catalogs::jlcpcb().add_file(name, file, &b.text) {
                Ok(()) => panic!("{at}: this file should not load and it does"),
                Err(message) => message,
            };
            let next = blocks
                .get(n + 1)
                .filter(|next| next.file == b.file && next.is("text"))
                .unwrap_or_else(|| panic!("{at}: the next block must be a ```text block"));
            if bless {
                rewrites.push((next.file.clone(), next.body.0, next.body.1, message.trim_end().to_string()));
            } else {
                assert!(
                    same(&next.text, &message),
                    "{at}: the message differs.\n--- guide\n{}\n--- tool\n{message}",
                    next.text
                );
            }
            continue;
        }
        if !b.is("nsm") {
            continue;
        }
        ran += 1;
        let at = b.at();
        if let Some(file) = b.value("file") {
            let real = fs::read_to_string(root().join(file)).expect("the named file exists");
            assert!(same(&b.text, &real), "{at}: the block differs from {file}");
        }
        // The physical file of this block, if the block after it is one; the message block
        // follows it.
        let physical = blocks
            .get(n + 1)
            .filter(|next| next.file == b.file && next.is("nsm-physical"));
        let after = n + 1 + usize::from(physical.is_some());
        let block_loader = match physical {
            Some(p) => {
                with_physical += 1;
                loader
                    .clone()
                    .with_physical("main.nsm", "main.physical.nsm", &p.text)
            }
            None => loader.clone(),
        };
        let loader = &block_loader;
        // A block with several checks (`warns circular`) has one ```text block for each, in the
        // order the checks are made.
        let taken = std::cell::Cell::new(0usize);
        let next_text = || {
            let k = taken.get();
            taken.set(k + 1);
            blocks
                .get(after + k)
                .filter(|next| next.file == b.file && next.is("text"))
                .unwrap_or_else(|| panic!("{at}: the next block must be a ```text block"))
        };
        let name = "main.nsm";
        let set = b.value("catalogs").unwrap_or("");
        let catalogs = match sets.get(set) {
            Some(Ok(c)) => c.clone(),
            Some(Err(e)) => panic!("{at}: the catalog files of set `{set}` do not load: {e}"),
            None if set.is_empty() => standard.clone(),
            None => panic!("{at}: there is no csv block in the set `{set}`"),
        };
        // A block that says `generics` is built as a project is by default (`series = "E24"`),
        // offering a generic part after the rows (ADR 0047).
        let catalogs = if b.has("generics") {
            catalogs.with_series(Some(netsmith_solve::Series::E24))
        } else {
            catalogs
        };

        if b.has("fails") {
            let message = match build_with(name, &b.text, loader, &catalogs, &Libraries::sample()) {
                Ok(_) => panic!("{at}: this block should fail and it builds"),
                Err(message) => message,
            };
            let next = next_text();
            if bless {
                rewrites.push((next.file.clone(), next.body.0, next.body.1, message.trim_end().to_string()));
            } else {
                assert!(
                    same(&next.text, &message),
                    "{at}: the message differs.\n--- guide\n{}\n--- tool\n{message}",
                    next.text
                );
            }
        } else if b.has("solve") {
            let built = build_with(name, &b.text, loader, &catalogs, &Libraries::sample())
                .unwrap_or_else(|e| panic!("{at}: should build:\n{e}"));
            // A block that does not say it warns states what its parts and values need (ADR
            // 0065): the two findings about a value that nobody stated are for a block that
            // shows them, and so is the one about a generic part that no row can be ordered for.
            if !b.has("warns") {
                let unstated: Vec<String> = built
                    .iter()
                    .flat_map(|x| x.guideline_warnings())
                    .filter(|w| {
                        w.contains(": unconstrained pick guideline:")
                            || w.contains(": unstated value guideline:")
                            || w.contains(": unresolved generic guideline:")
                    })
                    .collect();
                assert!(
                    unstated.is_empty(),
                    "{at}: this block should state what it needs and does not (mark it `warns` if it shows the finding):\n{}",
                    unstated.join("\n")
                );
            }
            if b.has("warns") {
                let warnings: String = built
                    .iter()
                    .flat_map(|x| x.guideline_warnings())
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    !warnings.is_empty(),
                    "{at}: this block should warn and does not"
                );
                let next = next_text();
                if bless {
                    rewrites.push((next.file.clone(), next.body.0, next.body.1, warnings));
                } else {
                    assert!(
                        same(&next.text, &warnings),
                        "{at}: the warnings differ.\n--- guide\n{}\n--- tool\n{warnings}",
                        next.text
                    );
                }
            }
            if b.has("notes") {
                let notes: String = built
                    .iter()
                    .flat_map(|x| x.notes.iter())
                    .filter(|n| n.contains("a lower bound"))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    !notes.is_empty(),
                    "{at}: this block should say a net is not sized and does not"
                );
                let next = next_text();
                if bless {
                    rewrites.push((next.file.clone(), next.body.0, next.body.1, notes));
                } else {
                    assert!(
                        same(&next.text, &notes),
                        "{at}: the notes differ.\n--- guide\n{}\n--- tool\n{notes}",
                        next.text
                    );
                }
            }
            if b.has("stock") {
                let notes: String = built
                    .iter()
                    .flat_map(|x| x.notes.iter())
                    .filter(|n| n.contains("the solver does not read stock"))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    !notes.is_empty(),
                    "{at}: this block should say a part is out of stock and does not"
                );
                let next = next_text();
                if bless {
                    rewrites.push((next.file.clone(), next.body.0, next.body.1, notes));
                } else {
                    assert!(
                        same(&next.text, &notes),
                        "{at}: the notes differ.\n--- guide\n{}\n--- tool\n{notes}",
                        next.text
                    );
                }
            }
            if b.has("circular") {
                let notes: String = built
                    .iter()
                    .flat_map(|x| x.notes.iter())
                    .filter(|n| n.contains("depend on each other"))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    !notes.is_empty(),
                    "{at}: this block should name values that wait on each other and does not"
                );
                let next = next_text();
                if bless {
                    rewrites.push((next.file.clone(), next.body.0, next.body.1, notes));
                } else {
                    assert!(
                        same(&next.text, &notes),
                        "{at}: the notes differ.\n--- guide\n{}\n--- tool\n{notes}",
                        next.text
                    );
                }
            }
            if b.has("started") {
                let notes: String = built
                    .iter()
                    .flat_map(|x| x.notes.iter())
                    .filter(|n| n.contains("depends on itself"))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    !notes.is_empty(),
                    "{at}: this block should say the solver started a circle of values and does not"
                );
                let next = next_text();
                if bless {
                    rewrites.push((next.file.clone(), next.body.0, next.body.1, notes));
                } else {
                    assert!(
                        same(&next.text, &notes),
                        "{at}: the notes differ.\n--- guide\n{}\n--- tool\n{notes}",
                        next.text
                    );
                }
            }
            if b.has("place") {
                let libraries = Libraries::sample();
                let findings: Vec<String> = built
                    .iter()
                    .flat_map(|x| placement_findings(&x.design, &libraries))
                    .collect();
                if b.has("refused") {
                    assert!(
                        !findings.is_empty(),
                        "{at}: this block should be refused by the placement check and is not"
                    );
                    let next = next_text();
                    let text = findings.join("\n");
                    if bless {
                        rewrites.push((next.file.clone(), next.body.0, next.body.1, text));
                    } else {
                        assert!(
                            same(&next.text, &text),
                            "{at}: the findings differ.\n--- guide\n{}\n--- tool\n{text}",
                            next.text
                        );
                    }
                } else {
                    assert!(
                        findings.is_empty(),
                        "{at}: this block should place cleanly:\n{}",
                        findings.join("\n")
                    );
                }
            }
            if b.has("report") {
                let report: String = built.iter().map(|x| x.report.clone()).collect();
                let next = next_text();
                if bless {
                    rewrites.push((next.file.clone(), next.body.0, next.body.1, report.trim_end().to_string()));
                } else {
                    assert!(
                        same(&next.text, &report),
                        "{at}: the report differs.\n--- guide\n{}\n--- tool\n{report}",
                        next.text
                    );
                }
            }
        } else if b.has("evaluate") && b.has("warns") {
            let outcome =
                outcome_with(name, &b.text, loader, &catalogs, None).unwrap_or_else(|e| {
                    panic!("{at}: should build:\n{}", netsmith_pipeline::render_problems(&e))
                });
            let warnings: String = outcome
                .boards
                .iter()
                .flat_map(|x| x.guideline_warnings())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                !warnings.is_empty(),
                "{at}: this block should warn and does not"
            );
            let next = next_text();
            if bless {
                rewrites.push((next.file.clone(), next.body.0, next.body.1, warnings));
            } else {
                assert!(
                    same(&next.text, &warnings),
                    "{at}: the warnings differ.\n--- guide\n{}\n--- tool\n{warnings}",
                    next.text
                );
            }
        } else if b.has("evaluate") && b.has("picks") {
            let outcome =
                outcome_with(name, &b.text, loader, &catalogs, None).unwrap_or_else(|e| {
                    panic!("{at}: should build:\n{}", netsmith_pipeline::render_problems(&e))
                });
            let lines: String = outcome
                .boards
                .iter()
                .flat_map(|x| x.notes.iter())
                .filter(|n| n.contains(": picked "))
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                !lines.is_empty(),
                "{at}: this block should pick a provider and does not"
            );
            let next = next_text();
            if bless {
                rewrites.push((next.file.clone(), next.body.0, next.body.1, lines));
            } else {
                assert!(
                    same(&next.text, &lines),
                    "{at}: the picks differ.\n--- guide\n{}\n--- tool\n{lines}",
                    next.text
                );
            }
        } else if b.has("evaluate") {
            let checked = check(name, &b.text, loader);
            assert!(checked.is_ok(), "{at}: should check:\n{}", checked.render());
            for board in checked.standalone_boards() {
                let solution = solve(board, &catalogs);
                assert!(
                    solution.diagnostics.is_empty(),
                    "{at}: should solve:\n{}",
                    solution
                        .diagnostics
                        .iter()
                        .map(|d| d.render(&checked.sources))
                        .collect::<String>()
                );
            }
            for system in &checked.systems {
                let solution = solve_system(system, &catalogs);
                assert!(
                    solution.diagnostics.is_empty(),
                    "{at}: should solve:\n{}",
                    solution
                        .diagnostics
                        .iter()
                        .map(|d| d.render(&checked.sources))
                        .collect::<String>()
                );
            }
        } else {
            let checked = check(name, &b.text, loader);
            assert!(checked.is_ok(), "{at}: should check:\n{}", checked.render());
        }
    }
    assert!(ran >= 20, "the guide runs only {ran} examples");
    assert!(
        with_physical >= 5,
        "the guide runs only {with_physical} examples with a physical file"
    );

    if bless && !rewrites.is_empty() {
        // Each chapter that has a message to rewrite is written back, the last block first so
        // that the line numbers of the ones before it hold.
        for (file, text) in chapters() {
            let mut mine: Vec<_> = rewrites.iter().filter(|r| r.0 == file).collect();
            if mine.is_empty() {
                continue;
            }
            let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
            mine.sort_by_key(|r| std::cmp::Reverse(r.1));
            for (_, start, end, new) in mine {
                lines.splice(*start..*end, new.lines().map(str::to_string));
            }
            fs::write(root().join(&file), lines.join("\n") + "\n").unwrap();
        }
    }
}

/// `docs/LANGUAGE.md` links every chapter, once, in the order of the files.
#[test]
fn the_index_links_every_chapter_in_order() {
    let index = fs::read_to_string(root().join("docs/LANGUAGE.md")).expect("docs/LANGUAGE.md exists");
    let linked: Vec<String> = index
        .lines()
        .filter_map(|l| l.split("](language/").nth(1))
        .filter_map(|rest| rest.strip_suffix(')'))
        .map(str::to_string)
        .collect();
    let files: Vec<String> = chapters()
        .iter()
        .map(|(path, _)| path.trim_start_matches("docs/language/").to_string())
        .collect();
    assert_eq!(
        linked, files,
        "docs/LANGUAGE.md must link each file of docs/language/ once, in the order of the file names"
    );
}

/// The anchors GitHub gives the headings of a Markdown file (outside fenced blocks).
fn anchors(markdown: &str) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut inside = false;
    let mut out = Vec::new();
    for line in markdown.lines() {
        if line.starts_with("```") {
            inside = !inside;
        } else if !inside
            && line.starts_with('#')
            && let Some(heading) = line.trim_start_matches('#').strip_prefix(' ')
        {
            let slug: String = heading
                .trim()
                .to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ' '))
                .map(|c| if c == ' ' { '-' } else { c })
                .collect();
            let n = seen.entry(slug.clone()).or_insert(0);
            out.push(if *n == 0 { slug } else { format!("{slug}-{n}") });
            *n += 1;
        }
    }
    out
}

/// A link of the guide that goes to a file or a heading lands: the guide is a file for each
/// chapter, so a link to a heading of another chapter names that chapter's file
/// (`04-members.md#heading`), and a bare `#heading` must be in the file it is written in.
#[test]
fn every_link_of_the_guide_lands() {
    let mut pages = chapters();
    pages.push((
        "docs/LANGUAGE.md".to_string(),
        fs::read_to_string(root().join("docs/LANGUAGE.md")).expect("docs/LANGUAGE.md exists"),
    ));
    for (file, text) in &pages {
        let dir = Path::new(file).parent().unwrap();
        let mut inside = false;
        for (n, line) in text.lines().enumerate() {
            if line.starts_with("```") {
                inside = !inside;
                continue;
            }
            if inside {
                continue;
            }
            for target in line.split("](").skip(1).filter_map(|r| r.split(')').next()) {
                if target.contains("://") || target.starts_with("mailto:") {
                    continue;
                }
                let at = format!("{file}:{}", n + 1);
                let (path, anchor) = match target.split_once('#') {
                    Some((path, anchor)) => (path, Some(anchor)),
                    None => (target, None),
                };
                let (to, to_text) = if path.is_empty() {
                    (file.clone(), text.clone())
                } else {
                    let to = dir.join(path);
                    let read = fs::read_to_string(root().join(&to))
                        .unwrap_or_else(|e| panic!("{at}: the link ({target}) goes nowhere: {e}"));
                    (to.display().to_string(), read)
                };
                if let Some(anchor) = anchor {
                    assert!(
                        anchors(&to_text).iter().any(|a| a == anchor),
                        "{at}: {to} has no heading #{anchor} (a heading of another chapter is linked as `NN-topic.md#heading`)"
                    );
                }
            }
        }
    }
}

/// The standard library definitions, as `std::parts::passive::Resistor`.
fn std_definitions() -> Vec<String> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "nsm") {
                out.push(path);
            }
        }
    }
    let std_dir = root().join("std");
    let mut files = Vec::new();
    walk(&std_dir, &mut files);
    let mut names = Vec::new();
    for file in files {
        let module = file
            .strip_prefix(&std_dir)
            .unwrap()
            .with_extension("")
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("::");
        for line in fs::read_to_string(&file).unwrap().lines() {
            for kind in [
                "part ",
                "interface ",
                "module ",
                "board ",
                "system ",
                "enum ",
                "generic ",
            ] {
                if let Some(rest) = line.strip_prefix(kind) {
                    let name: String = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    names.push(format!("std::{module}::{name}"));
                }
            }
        }
    }
    names.sort();
    names
}

#[test]
fn the_catalogue_lists_every_standard_library_definition_and_nothing_else() {
    let guide = guide();
    let real = std_definitions();
    assert!(real.len() >= 10, "found only {real:?}");
    for path in &real {
        assert!(
            guide.contains(&format!("`{path}`")),
            "docs/language/ does not list `{path}` in the catalogue"
        );
    }
    // Every `std::a::b::Name` the guide puts in backticks is a real definition.
    for span in guide.split('`').skip(1).step_by(2) {
        let upper = span
            .rsplit("::")
            .next()
            .and_then(|s| s.chars().next())
            .is_some_and(|c| c.is_uppercase());
        if span.starts_with("std::") && span.matches("::").count() >= 2 && upper {
            assert!(
                real.iter().any(|r| r == span),
                "docs/language/ mentions `{span}`, which is not in std/"
            );
        }
    }
}

// ---- skills ----------------------------------------------------------------------

fn skill_dirs() -> Vec<PathBuf> {
    let dir = root().join(".agents/skills");
    let mut dirs: Vec<PathBuf> = fs::read_dir(&dir)
        .expect(".agents/skills exists")
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}

#[test]
fn each_skill_is_a_well_formed_agent_skill() {
    let dirs = skill_dirs();
    assert!(dirs.len() >= 10, "found only {dirs:?}");
    for dir in dirs {
        let dir_name = dir.file_name().unwrap().to_string_lossy().to_string();
        let text = fs::read_to_string(dir.join("SKILL.md"))
            .unwrap_or_else(|e| panic!("{dir_name}/SKILL.md: {e}"));
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("---"), "{dir_name}: frontmatter first");
        let front: Vec<&str> = lines.by_ref().take_while(|l| *l != "---").collect();
        let field = |key: &str| {
            front
                .iter()
                .find_map(|l| l.strip_prefix(key).and_then(|v| v.strip_prefix(": ")))
                .unwrap_or_else(|| panic!("{dir_name}: no `{key}` in the frontmatter"))
        };
        assert_eq!(field("name"), dir_name, "the name must match its directory");
        assert!(
            dir_name.starts_with("netsmith-"),
            "{dir_name}: a skill is netsmith-* (for a project) or netsmith-dev-* (for netsmith)"
        );
        assert!(
            dir_name.len() <= 64
                && dir_name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{dir_name}: not a valid skill name"
        );
        let description = field("description");
        assert!(
            (20..=1024).contains(&description.len()),
            "{dir_name}: the description is {} characters",
            description.len()
        );
        assert!(
            description.contains("Use when") || description.contains("use when"),
            "{dir_name}: the description must say when to use the skill"
        );
    }
}

#[test]
fn claude_code_finds_the_skills_through_a_relative_symlink() {
    let link = root().join(".claude/skills");
    let target = fs::read_link(&link).expect(".claude/skills is a symlink");
    assert_eq!(target, Path::new("../.agents/skills"));
    assert!(link.join("netsmith-dev-verify/SKILL.md").exists());
}
