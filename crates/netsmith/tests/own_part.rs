//! A part of a project's own needs no KiCad install (ADR 0097, ADR 0104): its footprint is a file of
//! the project (`[[library]]`) or a generator's pads asked for by name and parameters (`gen:`), and
//! its symbol is drawn from its pin table. Each test builds and fabs a project folder holding only
//! its own files, on a machine with no KiCad (`Search::under` an empty folder), from the text of
//! the guide's section "A part of your own, with no KiCad installed", so the guide's examples are
//! the ones that run.

use std::fs;
use std::path::Path;

use netsmith::Libraries;
use netsmith::fab::fab;
use netsmith_kicad::install::Search;
use netsmith_kicad::sexp::Sexp;
use netsmith_kicad::testlib::TempDir;
use netsmith_pcb::generate::generate;
use netsmith_solve::Catalogs;

/// The fenced blocks of the guide's section, as (words after the fence, text).
fn section() -> Vec<(Vec<String>, String)> {
    let guide =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/LANGUAGE.md"))
            .expect("the guide exists");
    let heading = "### A part of your own, with no KiCad installed";
    let start = guide.find(heading).expect("the guide has the section");
    let body = &guide[start + heading.len()..];
    let body = &body[..body.find("\n## ").unwrap_or(body.len())];
    let mut blocks = Vec::new();
    let mut lines = body.lines();
    while let Some(line) = lines.next() {
        if let Some(info) = line.strip_prefix("```") {
            let mut text = Vec::new();
            for line in lines.by_ref() {
                if line.starts_with("```") {
                    break;
                }
                text.push(line);
            }
            blocks.push((
                info.split_whitespace().map(str::to_string).collect(),
                text.join("\n") + "\n",
            ));
        }
    }
    blocks
}

/// The block that starts with `needle`, of the kind `kind`.
fn block(kind: &str, needle: &str) -> String {
    section()
        .into_iter()
        .find(|(info, text)| info.first().is_some_and(|k| k == kind) && text.contains(needle))
        .unwrap_or_else(|| panic!("the guide has no `{kind}` block with {needle:?}"))
        .1
}

/// The `[[part]]` blocks of the guide's section that map `Sensor`.
fn sensor_row(footprint_starts: &str) -> String {
    block("toml", &format!("footprint = \"{footprint_starts}"))
}

fn no_kicad() -> (TempDir, Search) {
    let empty = TempDir::new("own-part-no-kicad");
    // The search is switched off, as `NETSMITH_NO_KICAD=1` does: it finds nothing at all.
    let search = Search::under(empty.path()).with_no_kicad();
    (empty, search)
}

/// Build and fab `Probe` of the project in `dir`, with no KiCad. Returns the written board.
fn fab_probe(dir: &Path, search: &Search) -> Result<String, String> {
    let libraries = Libraries::for_project_by(Some(dir), search)?;
    // Nothing of KiCad's is anywhere: the libraries the build reads are the project's.
    assert!(!libraries.footprints.root().is_dir());
    let source = fs::read_to_string(dir.join("src/main.nsm")).unwrap();
    let out = TempDir::unused("own-part-fab");
    let made = fab(
        "main.nsm",
        &source,
        &out,
        &Catalogs::jlcpcb(),
        &libraries,
        None,
    )?;
    assert_eq!(made.len(), 1);
    assert_eq!(made[0].board, "Probe");
    let dir = out.join("Probe");
    let pcb = fs::read_to_string(dir.join("Probe.kicad_pcb")).unwrap();
    // The `fab` check ran and found the board clean.
    assert!(
        made[0].notes.contains("design rules check: passed"),
        "{}",
        made[0].notes
    );
    for file in [
        "Probe-gerbers.zip",
        "bom.csv",
        "cpl.csv",
        "Probe.kicad_pro",
        "fab-notes.txt",
    ] {
        assert!(dir.join(file).is_file(), "{file} was not written");
    }
    // The tables are written beside the board for the libraries it uses.
    let tables = fs::read_to_string(dir.join("fp-lib-table")).unwrap_or_default();
    Ok(format!("{pcb}\n;;; fp-lib-table\n{tables}"))
}

/// The project: its circuit from the guide, and its `netsmith.toml`.
fn project(dir: &Path, kicad_toml: &str) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"probe\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/main.nsm"),
        block("nsm-fragment", "part Sensor"),
    )
    .unwrap();
    fs::write(dir.join("kicad.toml"), kicad_toml).unwrap();
}

/// The sizes of the pads of the footprint `Sensor` placed on `pcb`, by number.
fn pads_of(pcb: &str, name: &str) -> Vec<(String, String, f64, f64)> {
    let root = Sexp::parse(pcb.split("\n;;; ").next().unwrap()).unwrap();
    let footprint = root
        .find_all("footprint")
        .find(|f| f.args().first().and_then(Sexp::text) == Some(name))
        .unwrap_or_else(|| panic!("the board has no footprint {name}"));
    let mut pads: Vec<_> = footprint
        .find_all("pad")
        .map(|p| {
            let size = p.find("size").unwrap();
            let number = |i: usize| size.args()[i].text().unwrap().parse::<f64>().unwrap();
            (
                p.args()[0].text().unwrap().to_string(),
                p.args()[1].text().unwrap().to_string(),
                number(0).min(number(1)),
                number(0).max(number(1)),
            )
        })
        .collect();
    pads.sort_by(|a, b| a.0.cmp(&b.0));
    pads
}

#[test]
fn a_project_with_a_gen_footprint_and_no_library_builds_and_fabs_with_no_kicad() {
    let dir = TempDir::new("own-part-gen");
    // Only the project's own files: a circuit and a `kicad.toml` with one row that asks for a
    // footprint by generator and names no symbol and no library.
    let row = sensor_row("gen:SOT23(pins=3)");
    assert!(
        !row.contains("[[library]]") && !row.contains("symbol"),
        "{row}"
    );
    project(&dir, &row);
    let (_empty, search) = no_kicad();

    let pcb = fab_probe(&dir, &search).unwrap_or_else(|e| panic!("{e}"));

    // The footprint is the generator's, under the name the parameters give, and nothing of a
    // library is listed for it.
    assert!(pcb.contains("(footprint \"gen:SOT23_pins3\""), "{pcb}");
    assert!(!pcb.contains("own:"), "{pcb}");
    let written = pads_of(&pcb, "gen:SOT23_pins3");
    // The pads are those of `Package_TO_SOT_SMD:SOT-23-3`: the same numbers, kind and sizes.
    let by_id = generate("Package_TO_SOT_SMD:SOT-23-3").unwrap();
    let mut expected: Vec<_> = by_id
        .pads
        .iter()
        .map(|p| {
            (
                p.number.clone(),
                "smd".to_string(),
                p.width.min(p.height),
                p.width.max(p.height),
            )
        })
        .collect();
    expected.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(written.len(), 3);
    for (got, want) in written.iter().zip(&expected) {
        assert_eq!((&got.0, &got.1), (&want.0, &want.1));
        assert!(
            (got.2 - want.2).abs() < 1e-3 && (got.3 - want.3).abs() < 1e-3,
            "{got:?} {want:?}"
        );
    }
}

#[test]
fn a_project_with_a_footprint_file_of_its_own_builds_and_fabs_with_no_kicad() {
    let dir = TempDir::new("own-part-files");
    // The `[[library]]` and the row of the guide, and the `.kicad_mod` it shows.
    let kicad_toml = block("toml", "[[library]]");
    assert!(
        kicad_toml.contains("footprint = \"own:Sensor\""),
        "{kicad_toml}"
    );
    project(&dir, &kicad_toml);
    fs::create_dir_all(dir.join("libs/own.pretty")).unwrap();
    fs::write(
        dir.join("libs/own.pretty/Sensor.kicad_mod"),
        block("text", "(footprint \"Sensor\""),
    )
    .unwrap();
    let (_empty, search) = no_kicad();

    let pcb = fab_probe(&dir, &search).unwrap_or_else(|e| panic!("{e}"));

    assert!(pcb.contains("(footprint \"own:Sensor\""), "{pcb}");
    let written = pads_of(&pcb, "own:Sensor");
    assert_eq!(
        written.iter().map(|p| p.0.as_str()).collect::<Vec<_>>(),
        ["1", "2", "3"]
    );
    assert!(
        written
            .iter()
            .all(|p| (p.2 - 0.7).abs() < 1e-3 && (p.3 - 1.2).abs() < 1e-3)
    );
    // The library is listed for KiCad beside the board.
    assert!(
        pcb.contains("(name \"own\")") && pcb.contains("libs/own.pretty"),
        "{pcb}"
    );
}

#[test]
fn the_two_rows_map_one_part_and_a_row_may_ask_for_a_generator_the_guide_lists() {
    // Both ways of the guide read as a mapping, and the QFN of the table is a footprint.
    for starts in ["gen:SOT23(pins=3)", "gen:QFN(pins=48"] {
        let row = sensor_row(starts);
        let m = netsmith_kicad::Mapping::parse(&row, "kicad.toml", Path::new("."))
            .unwrap_or_else(|e| panic!("{row}: {e}"));
        let mapped = m.map(&m.rows()[0].kind, None).unwrap();
        netsmith::libraries::check_generated_footprints(&m).unwrap_or_else(|e| panic!("{e}"));
        assert!(mapped.footprint.starts_with("gen:"), "{}", mapped.footprint);
    }
    let qfn = FootprintOf::of(&sensor_row("gen:QFN(pins=48"));
    // 48 leads and the exposed pad.
    assert_eq!(qfn.numbered, 49);
}

struct FootprintOf {
    numbered: usize,
}

impl FootprintOf {
    fn of(row: &str) -> FootprintOf {
        let m = netsmith_kicad::Mapping::parse(row, "kicad.toml", Path::new(".")).unwrap();
        let id = m.rows()[0].footprint.clone();
        let footprint = netsmith_pcb::FootprintLibrary::at("/nonexistent")
            .load(&id)
            .unwrap();
        FootprintOf {
            numbered: footprint
                .pad_numbers()
                .into_iter()
                .filter(|n| !n.is_empty())
                .count(),
        }
    }
}

#[test]
fn the_error_of_the_guide_is_the_one_the_tool_prints() {
    let row = block("toml", "footprint = \"gen:QFN(pins=16, pitch=0.5mm)\"");
    let m = netsmith_kicad::Mapping::parse(&row, "kicad.toml", Path::new(".")).unwrap();
    let e = netsmith::libraries::check_generated_footprints(&m).unwrap_err();
    let shown = block("text", "kicad.toml:5:");
    assert_eq!(e.trim_end(), shown.trim_end());
}

/// The part of a project, a `kicad.toml` and the pin table whose pad count is not the footprint's.
fn mismatch_message(kicad_toml: &str) -> String {
    let dir = TempDir::new("own-part-mismatch");
    project(&dir, kicad_toml);
    let (_empty, search) = no_kicad();
    let libraries = Libraries::for_project_by(Some(&dir), &search).unwrap();
    let source = fs::read_to_string(dir.join("src/main.nsm")).unwrap();
    let out = TempDir::unused("own-part-mismatch-fab");
    fab(
        "main.nsm",
        &source,
        &out,
        &Catalogs::jlcpcb(),
        &libraries,
        None,
    )
    .expect_err("the pads and the pins disagree")
}

#[test]
fn a_gen_footprint_with_more_pads_than_the_part_has_pins_names_the_row_and_its_line() {
    // Five pads for a part of three pins.
    let row = sensor_row("gen:SOT23(pins=3)").replace("pins=3", "pins=5");
    let e = mismatch_message(&row);
    assert!(
        e.contains("the footprint `gen:SOT23_pins5` has pad 4, which no pin of the part has"),
        "{e}"
    );
    assert!(
        e.contains("`gen:SOT23(pins=5)` of kicad.toml line 5, and it makes 5 pads"),
        "{e}"
    );
}

#[test]
fn a_gen_footprint_with_fewer_pads_than_the_part_has_pins_names_the_row_and_its_line() {
    // Three pads cannot be a part with a fourth pad in its table: pins 1 to 3 of five.
    let row = sensor_row("gen:SOT23(pins=3)");
    let dir = TempDir::new("own-part-fewer");
    project(&dir, &row);
    let source = block("nsm-fragment", "part Sensor").replace(
        "        .gnd passive \"GND\" \"3\"\n",
        "        .gnd passive \"GND\" \"3\"\n        .nc passive \"NC\" \"4\"\n",
    );
    fs::write(dir.join("src/main.nsm"), &source).unwrap();
    let (_empty, search) = no_kicad();
    let libraries = Libraries::for_project_by(Some(&dir), &search).unwrap();
    let out = TempDir::unused("own-part-fewer-fab");
    let e = fab(
        "main.nsm",
        &source,
        &out,
        &Catalogs::jlcpcb(),
        &libraries,
        None,
    )
    .expect_err("the pins have a pad the footprint lacks");
    assert!(
        e.contains("the pinout has pad 4") && e.contains("has pads 1, 2, 3 only"),
        "{e}"
    );
    assert!(
        e.contains("`gen:SOT23(pins=3)` of kicad.toml line 5, and it makes 3 pads"),
        "{e}"
    );
}
