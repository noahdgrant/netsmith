//! A `Logo` (issue #760): a drawing from an SVG or DXF file beside the source, on the silkscreen
//! or in the copper. The file is found relative to the file that states it, read at `fab` and
//! `build`, and plotted into the Gerbers; one that is missing or unreadable is an error naming it.

use std::{fs, path::Path};

use netsmith_check::Loader;
use netsmith_fabricate::fab::{FabOptions, fab_with_options};
use netsmith_fabricate::models::{Located, Places, locate};
use netsmith_kicad::skip_without_kicad_libraries;
use netsmith_pipeline::Libraries;
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

const LED: &str = include_str!("../../../examples/led.nsm");

const MARK: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 50\">\n  <path d=\"M0 0H100V50H0Z M20 10H80V40H20Z\"/>\n  <circle cx=\"50\" cy=\"25\" r=\"8\"/>\n</svg>\n";

/// A project with the LED board in `src/main.nsm` and `logo` in the `case { }` section of its
/// physical file.
fn project(label: &str, logo: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"led\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), LED).unwrap();
    fs::write(
        root.join("src/main.physical.nsm"),
        format!(
            "import std::parts::mechanical::Logo\n\nphysical for LedExample {{\n    case {{\n{logo}\n    }}\n}}\n"
        ),
    )
    .unwrap();
    root
}

fn loader(root: &Path) -> Loader {
    let mut loader = Loader::std();
    let physical = fs::read_to_string(root.join("src/main.physical.nsm")).unwrap();
    loader.attach_physical("src/main.nsm", "src/main.physical.nsm", &physical);
    loader
}

/// The LED board of a project at `root`, with its logos found on disk.
fn located(root: &Path) -> (netsmith_core::Design, Located) {
    let outcome = netsmith_pipeline::outcome_with(
        "src/main.nsm",
        LED,
        &loader(root),
        &Catalogs::jlcpcb(),
        None,
    )
    .unwrap_or_else(|p| panic!("{}", netsmith_pipeline::render_problems(&p)));
    let mut design = outcome.boards[0].design.clone();
    let found = locate(
        &mut design,
        &Places::under(root),
        Some(&netsmith_kicad::Mapping::standard()),
    );
    (design, found)
}

const SILK: &str = "        .mark: Logo {\n            .file &= \"../art/mark.svg\"\n            .size &= 10mm\n        }";

#[test]
fn a_logo_is_found_relative_to_the_file_that_states_it_and_read_at_its_size() {
    let root = project("logo-found", SILK);
    fs::create_dir_all(root.join("art")).unwrap();
    fs::write(root.join("art/mark.svg"), MARK).unwrap();
    let (design, found) = located(&root);
    assert_eq!(found, Located::default());
    let logos = &design.features.logos;
    assert_eq!(logos.len(), 1, "{logos:?}");
    let logo = &logos[0];
    assert_eq!(logo.path, "mark");
    assert_eq!(logo.layer, netsmith_core::LogoLayer::Silkscreen);
    assert_eq!(logo.size, 10.0);
    assert_eq!(logo.source, "src/main.physical.nsm:6");
    // The frame with its hole is one polygon (the cut joins them), and the circle is another.
    assert_eq!(logo.outlines.len(), 2);
    let xs: Vec<f64> = logo.outlines.iter().flatten().map(|p| p.0).collect();
    let ys: Vec<f64> = logo.outlines.iter().flatten().map(|p| p.1).collect();
    let span = |v: &[f64]| {
        v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min)
    };
    assert!((span(&xs) - 10.0).abs() < 1e-9 && (span(&ys) - 5.0).abs() < 1e-9);
    // Its part is the logo's file, fitted by nobody, with the designator `LOGO1`.
    let part = design.parts.iter().find(|p| p.path == "mark").unwrap();
    assert_eq!((part.refdes.as_str(), part.value.as_str()), ("LOGO1", "../art/mark.svg"));
}

#[test]
fn a_logo_file_that_is_not_there_is_an_error_naming_it() {
    let root = project("logo-missing", SILK);
    let (design, found) = located(&root);
    assert_eq!(
        found.errors,
        [
            "`mark.file` names `art/mark.svg` (src/main.physical.nsm:6), and there is no such file, so `mark` has nothing to print: add the SVG or DXF file, or take the logo out"
        ]
    );
    assert!(design.features.logos[0].outlines.is_empty());
}

#[test]
fn a_logo_file_that_cannot_be_read_is_an_error_naming_the_file_and_what_is_wrong() {
    let root = project("logo-bad", SILK);
    fs::create_dir_all(root.join("art")).unwrap();
    fs::write(
        root.join("art/mark.svg"),
        "<svg>\n<g transform=\"scale(2)\"><rect width=\"4\" height=\"4\"/></g></svg>",
    )
    .unwrap();
    let (_, found) = located(&root);
    assert_eq!(found.errors.len(), 1, "{:?}", found.errors);
    let error = &found.errors[0];
    assert!(
        error.starts_with("`mark.file` (src/main.physical.nsm:6), the logo's file `art/mark.svg`: `../art/mark.svg`: `<g>` at line 2 has a `transform`"),
        "{error}"
    );
}

#[test]
fn a_raster_image_is_refused_and_so_is_a_file_outside_the_project() {
    let root = project(
        "logo-png",
        "        .mark: Logo {\n            .file &= \"../art/mark.png\"\n            .size &= 10mm\n        }",
    );
    fs::create_dir_all(root.join("art")).unwrap();
    fs::write(root.join("art/mark.png"), "\u{89}PNG").unwrap();
    let (_, found) = located(&root);
    assert!(
        found.errors[0].contains("is neither an SVG nor a DXF file")
            && found.errors[0].contains("a raster image is not read"),
        "{:?}",
        found.errors
    );
    let root = project(
        "logo-outside",
        "        .mark: Logo {\n            .file &= \"../../mark.svg\"\n            .size &= 10mm\n        }",
    );
    let (_, found) = located(&root);
    assert!(
        found.errors[0].contains("outside the project") && found.errors[0].contains("SVG or DXF file"),
        "{:?}",
        found.errors
    );
}

fn problems(logo: &str) -> String {
    let root = project("logo-design", logo);
    match netsmith_pipeline::outcome_with(
        "src/main.nsm",
        LED,
        &loader(&root),
        &Catalogs::jlcpcb(),
        None,
    ) {
        Ok(_) => panic!("{logo} was accepted"),
        Err(p) => netsmith_pipeline::render_problems(&p),
    }
}

#[test]
fn a_logo_states_its_file_and_size_and_a_layer_that_is_known() {
    let e = problems("        .mark: Logo { .size &= 10mm }");
    assert!(e.contains("`mark` is a `Logo` and names no file"), "{e}");
    let e = problems("        .mark: Logo { .file &= \"m.svg\" }");
    assert!(e.contains("`mark` is a `Logo` and states no size"), "{e}");
    let e = problems("        .mark: Logo { .file &= \"m.svg\"; .size &= 10mm; .layer &= \"gold\" }");
    assert!(
        e.contains("`mark.layer` is \"gold\", which is not \"silkscreen\" or \"copper\""),
        "{e}"
    );
}

#[test]
fn a_logo_is_never_smaller_than_the_least_text_on_silk_or_the_least_track_in_copper() {
    let e = problems("        .mark: Logo { .file &= \"m.svg\"; .size &= 0.4mm }");
    assert!(
        e.contains("`mark.size` is 0.4 mm, under the process's least text height of"),
        "{e}"
    );
    let e = problems(
        "        .mark: Logo { .file &= \"m.svg\"; .size &= 0.04mm; .layer &= \"copper\" }",
    );
    assert!(
        e.contains("`mark.size` is 0.04 mm, under the process's least track of"),
        "{e}"
    );
}

/// The Gerber of a layer in a package.
fn gerber(dir: &Path, board: &str, file: &str) -> String {
    use std::io::Read as _;
    let zip = fs::File::open(dir.join(board).join(format!("{board}-gerbers.zip"))).unwrap();
    let mut archive = zip::ZipArchive::new(zip).unwrap();
    let mut text = String::new();
    archive
        .by_name(&format!("gerber/{file}"))
        .unwrap_or_else(|e| panic!("{file}: {e}"))
        .read_to_string(&mut text)
        .unwrap();
    text
}

/// How many filled regions a Gerber has.
fn regions(gerber: &str) -> usize {
    gerber.matches("G36*").count()
}

/// `fab` on the LED board with `logo` in its physical file, to a folder.
fn fabricated(label: &str, logo: &str) -> (TempDir, TempDir, Vec<netsmith_fabricate::fab::Fabricated>) {
    let root = project(label, logo);
    fs::create_dir_all(root.join("art")).unwrap();
    fs::write(root.join("art/mark.svg"), MARK).unwrap();
    let dir = TempDir::unused(&format!("{label}-out"));
    let options = FabOptions {
        models: Some(Places::under(&root)),
        ..FabOptions::default()
    };
    let made = fab_with_options(
        "src/main.nsm",
        LED,
        &loader(&root),
        &dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        None,
        &options,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    (root, dir, made)
}

const COPPER: &str = "        .mark: Logo {\n            .file &= \"../art/mark.svg\"\n            .size &= 10mm\n            .layer &= \"copper\"\n        }";

/// The regions of the front silkscreen and of the front copper of the LED board with `logo`.
fn regions_of(label: &str, logo: &str) -> (usize, usize, TempDir, String) {
    let (_root, dir, made) = fabricated(label, logo);
    let file = |layer: &str| {
        let name = made[0]
            .gerber_files
            .iter()
            .find(|f| f.contains(layer))
            .unwrap_or_else(|| panic!("no {layer} in {:?}", made[0].gerber_files));
        name.rsplit('/').next().unwrap().to_string()
    };
    let silk = regions(&gerber(&dir, "LedExample", &file("F_Silkscreen")));
    let copper = regions(&gerber(&dir, "LedExample", &file("F_Cu")));
    (silk, copper, dir, made[0].notes.clone())
}

#[test]
fn a_logo_is_plotted_on_the_layer_it_states_and_on_no_other() {
    if skip_without_kicad_libraries() {
        return;
    }
    let (silk_of_silk, copper_of_silk, dir, _) = regions_of("logo-silk", SILK);
    let (silk_of_copper, copper_of_copper, _dir, notes) = regions_of("logo-copper", COPPER);
    // The frame with its hole and the circle: two regions that are on one layer or the other.
    assert!(silk_of_silk >= silk_of_copper + 2, "{silk_of_silk} {silk_of_copper}");
    assert!(copper_of_copper >= copper_of_silk + 2, "{copper_of_copper} {copper_of_silk}");
    assert!(!notes.contains("design rules check: failed"), "{notes}");
    // It is fitted by nobody, so it is on no bill of materials, and the board file holds it.
    let bom = fs::read_to_string(dir.join("LedExample/bom.csv")).unwrap();
    assert!(!bom.contains("LOGO1"), "{bom}");
    let pcb = fs::read_to_string(dir.join("LedExample/LedExample.kicad_pcb")).unwrap();
    assert!(pcb.contains("(fp_poly") && pcb.contains("\"F.SilkS\""), "{pcb}");
}
