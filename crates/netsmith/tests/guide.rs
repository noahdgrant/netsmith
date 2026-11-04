//! `docs/LANGUAGE.md` and the agent skills cannot rot: every example in the guide is run.
//!
//! Fenced blocks are marked by what they promise:
//!
//! - ` ```nsm ` is a complete file and must check.
//! - ` ```nsm solve ` must also solve, lower and export (what `netsmith build` does).
//! - ` ```nsm solve report ` must build, and the next ` ```text ` block is the board report.
//! - ` ```nsm evaluate ` must check and solve with nothing reported, but is not exported: for
//!   a board of parts that have no KiCad symbol yet, whose ratings the solver still checks.
//! - ` ```nsm solve place ` must also place on the sample libraries with no finding of the
//!   placement check (`fab` would stop on one); ` ```nsm solve place refused ` must build and
//!   not place, and the next ` ```text ` block is the findings as `fab` prints them.
//! - ` ```nsm solve warns ` must build, and the next ` ```text ` block is the guideline
//!   warnings (ADR 0036), one per line, as the command line prints them after `warning: `.
//! - ` ```nsm solve notes ` must build, and the next ` ```text ` block is the notes that name a
//!   net whose tracks were not sized from its current (ADR 0036), one per line, as the
//!   command line prints them after `note: `.
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

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use netsmith::{Libraries, build_with};
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
    /// The 1-based line of the opening fence.
    line: usize,
    /// The range of lines (0-based, end exclusive) holding `text`.
    body: (usize, usize),
}

impl Block {
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

fn blocks(markdown: &str) -> Vec<Block> {
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

fn guide() -> String {
    fs::read_to_string(root().join("docs/LANGUAGE.md")).expect("docs/LANGUAGE.md exists")
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
            .unwrap_or_else(|| panic!("docs/LANGUAGE.md:{}: a csv block needs file=", b.line));
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
    let placed = netsmith_pcb::Board::from_design_placed(
        design,
        &libraries.mapping,
        &libraries.footprints,
        &netsmith_pcb::place_options(design, None),
    );
    match placed {
        Err(e) => vec![format!("error: {}: {e}", design.name)],
        Ok(board) => board
            .check_placement()
            .iter()
            .map(|p| format!("error: {}: {p}", design.name))
            .collect(),
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
    let guide = guide();
    let mut found = 0;
    for b in blocks(&guide)
        .iter()
        .filter(|b| b.is("toml") && b.text.starts_with("[project]"))
    {
        found += 1;
        netsmith::project::Manifest::parse(&b.text)
            .unwrap_or_else(|e| panic!("docs/LANGUAGE.md:{}: {e}", b.line));
    }
    assert!(found >= 3, "found only {found} manifests");
}

/// The orders of the guide's manifest are the ones the text describes, and the error it shows
/// for a board that is not there is what `resolve` says.
#[test]
fn the_orders_in_the_guide_are_read_and_their_error_is_the_real_one() {
    use netsmith::order::resolve;
    let guide = guide();
    let blocks = blocks(&guide);
    let manifest = blocks
        .iter()
        .find(|b| b.is("toml") && b.text.contains("[order.devkit.boards]"))
        .expect("the guide has a manifest with an order");
    let m = netsmith::project::Manifest::parse(&manifest.text).unwrap();
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
        netsmith::project::Manifest::parse(&manifest.text.replace("Unit =", "Unitt =")).unwrap();
    let e = resolve(&typo.orders[0], &boards).unwrap_err();
    let shown = guide
        .lines()
        .find(|l| l.starts_with("netsmith.toml:") && l.contains("Unitt"))
        .expect("the guide shows the error");
    assert_eq!(e.trim_end(), shown, "{e}");
}

#[test]
fn every_fence_says_what_it_is() {
    let guide = guide();
    for b in blocks(&guide) {
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
            "docs/LANGUAGE.md:{}: a fenced block must start with one of {known:?}, not {:?}",
            b.line,
            b.info
        );
    }
}

#[test]
fn every_complete_example_checks_solves_and_fails_as_the_guide_says() {
    let markdown = guide();
    let blocks = blocks(&markdown);
    let loader = loader(&blocks);
    let sets = catalog_sets(&blocks);
    for (set, loaded) in &sets {
        if let Err(e) = loaded {
            panic!("the csv blocks of set `{set}` do not load: {e}");
        }
    }
    let standard = Catalogs::jlcpcb();
    let bless = std::env::var_os("GUIDE_BLESS").is_some();
    // (first line, last line, new text) of message blocks to rewrite.
    let mut rewrites: Vec<(usize, usize, String)> = Vec::new();
    let mut ran = 0;
    let mut with_physical = 0;

    for (n, b) in blocks.iter().enumerate() {
        if b.is("nsm-physical") {
            let at = format!("docs/LANGUAGE.md:{}", b.line);
            assert!(
                n > 0 && blocks[n - 1].is("nsm"),
                "{at}: an nsm-physical block is the physical file of the nsm block right before it"
            );
            if let Some(file) = b.value("file") {
                let real = fs::read_to_string(root().join(file)).expect("the named file exists");
                assert!(same(&b.text, &real), "{at}: the block differs from {file}");
            }
        }
        if b.is("csv") && b.has("fails") {
            let at = format!("docs/LANGUAGE.md:{}", b.line);
            let file = b.value("file").expect("a csv block needs file=");
            let name = Path::new(file).file_stem().unwrap().to_str().unwrap();
            let message = match Catalogs::jlcpcb().add_file(name, file, &b.text) {
                Ok(()) => panic!("{at}: this file should not load and it does"),
                Err(message) => message,
            };
            let next = blocks
                .get(n + 1)
                .filter(|next| next.is("text"))
                .unwrap_or_else(|| panic!("{at}: the next block must be a ```text block"));
            if bless {
                rewrites.push((next.body.0, next.body.1, message.trim_end().to_string()));
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
        let at = format!("docs/LANGUAGE.md:{}", b.line);
        if let Some(file) = b.value("file") {
            let real = fs::read_to_string(root().join(file)).expect("the named file exists");
            assert!(same(&b.text, &real), "{at}: the block differs from {file}");
        }
        // The physical file of this block, if the block after it is one; the message block
        // follows it.
        let physical = blocks.get(n + 1).filter(|next| next.is("nsm-physical"));
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
        let next_text = || {
            blocks
                .get(after)
                .filter(|next| next.is("text"))
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

        if b.has("fails") {
            let message = match build_with(name, &b.text, loader, &catalogs, &Libraries::sample())
            {
                Ok(_) => panic!("{at}: this block should fail and it builds"),
                Err(message) => message,
            };
            let next = next_text();
            if bless {
                rewrites.push((next.body.0, next.body.1, message.trim_end().to_string()));
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
                    rewrites.push((next.body.0, next.body.1, warnings));
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
                    .filter(|n| n.contains("are not sized from their current"))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    !notes.is_empty(),
                    "{at}: this block should say a net is not sized and does not"
                );
                let next = next_text();
                if bless {
                    rewrites.push((next.body.0, next.body.1, notes));
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
                        rewrites.push((next.body.0, next.body.1, text));
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
                    rewrites.push((next.body.0, next.body.1, report.trim_end().to_string()));
                } else {
                    assert!(
                        same(&next.text, &report),
                        "{at}: the report differs.\n--- guide\n{}\n--- tool\n{report}",
                        next.text
                    );
                }
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
        let mut lines: Vec<String> = markdown.lines().map(str::to_string).collect();
        rewrites.sort_by_key(|r| std::cmp::Reverse(r.0));
        for (start, end, text) in rewrites {
            lines.splice(start..end, text.lines().map(str::to_string));
        }
        fs::write(root().join("docs/LANGUAGE.md"), lines.join("\n") + "\n").unwrap();
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
            for kind in ["part ", "interface ", "module ", "board ", "system "] {
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
            "docs/LANGUAGE.md does not list `{path}` in the catalogue"
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
                "docs/LANGUAGE.md mentions `{span}`, which is not in std/"
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
        let body = lines.count();
        assert!(
            body <= 80,
            "{dir_name}: {body} body lines; keep a skill short"
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
