//! The Gerber viewer (issue #1105, `netsmith_fab::viewer`): it reads the Gerbers and drill
//! files netsmith writes for a board and a package another tool wrote (the tracespace files in
//! `data/gerber-view`, their source in its README), draws them, and says which files it could
//! not read instead of drawing them wrong.
//!
//! The pictures are compared by the hash of their PNG bytes with `data/gerber-view/golden.txt`.
//! `NETSMITH_GERBER_VIEW_BLESS=1 cargo test -p netsmith-fab --test main gerber_view:: --
//! --test-threads 1` rewrites it from what the code draws now (one test at a time, as each
//! rewrites its own lines of the one file); `NETSMITH_GERBER_VIEW_OUT=<folder>` writes the
//! pictures there to look at first.

use std::path::{Path, PathBuf};

use netsmith_fab::viewer::{self, Face, Function, Layer, Package, Show};
use netsmith_fab::{Output, gerber_zip, render_gerber, RasterView};
use netsmith_geom::{Artwork, PlotRules, drill_files, plot_layer};

const BOARD: &str = include_str!("data/features.kicad_pcb");

fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/gerber-view")
}

/// The files netsmith's writers make for the board of every feature.
fn features() -> Vec<Output> {
    let art = Artwork::parse(BOARD).unwrap();
    let rules = PlotRules {
        edge_clearance: 0.2,
        hole_clearance: Some(0.1),
        classes: Vec::new(),
    };
    let mut files = plot_layer(&art, "features", &rules).unwrap().files;
    files.extend(drill_files(&art, "features"));
    files
}

fn package_of(files: &[Output]) -> Package {
    viewer::read(
        "features",
        files.iter().map(|f| (f.name.clone(), f.bytes.clone())).collect(),
    )
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Compares the pictures with the hashes of `golden.txt` under `key`, or rewrites them.
fn golden(key: &str, pictures: &[(String, Vec<u8>)]) {
    if let Some(dir) = std::env::var_os("NETSMITH_GERBER_VIEW_OUT") {
        std::fs::create_dir_all(&dir).unwrap();
        for (name, png) in pictures {
            std::fs::write(Path::new(&dir).join(format!("{key}-{name}.png")), png).unwrap();
        }
    }
    let path = data().join("golden.txt");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut now: Vec<String> = pictures
        .iter()
        .map(|(name, png)| format!("{key}-{name} {:016x}", fnv(png)))
        .collect();
    now.sort();
    if std::env::var_os("NETSMITH_GERBER_VIEW_BLESS").is_some() {
        // Keep the other test's lines; the file is rewritten by each test for its own key.
        let mut lines: Vec<String> = text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.starts_with(&format!("{key}-")))
            .map(str::to_string)
            .chain(now)
            .collect();
        lines.sort();
        let mut out = String::from(
            "# FNV-1a 64 of the PNG bytes of each picture of tests/gerber_view.rs.\n\
             # Rewrite with NETSMITH_GERBER_VIEW_BLESS=1 cargo test -p netsmith-fab --test main gerber_view:: -- --test-threads 1\n\
             # and look at the pictures first (NETSMITH_GERBER_VIEW_OUT=<folder>).\n",
        );
        for l in lines {
            out.push_str(&l);
            out.push('\n');
        }
        std::fs::write(&path, out).unwrap();
        return;
    }
    let want: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with(&format!("{key}-")))
        .collect();
    assert_eq!(
        now,
        want,
        "a picture changed: look at them (NETSMITH_GERBER_VIEW_OUT=<folder>), then rewrite {} \
         with NETSMITH_GERBER_VIEW_BLESS=1",
        path.display()
    );
}

fn picture(pkg: &Package, show: Show, region: Option<[f64; 4]>, scale: f64) -> Vec<u8> {
    let drawing = viewer::draw(pkg, show, region).unwrap();
    viewer::png(&drawing, Some(scale)).unwrap()
}

#[test]
fn the_gerbers_netsmith_writes_are_read_back_and_drawn() {
    let files = features();
    let pkg = package_of(&files);
    let summary = pkg.summary();
    assert!(summary.unread.is_empty(), "{:?}", summary.unread);
    assert!(summary.skipped.is_empty());
    // Every Gerber says what it is in its X2 attribute.
    for l in summary.layers.iter().filter(|l| l.format == "gerber") {
        assert_eq!(l.identified_by, Some("x2"), "{}", l.file);
    }
    let function = |suffix: &str| {
        pkg.files
            .iter()
            .find(|f| f.name.ends_with(suffix))
            .unwrap_or_else(|| panic!("no {suffix}"))
            .function
    };
    assert_eq!(function("-F_Cu.gbr"), Function::Copper(Layer::Top));
    assert_eq!(function("-In2_Cu.gbr"), Function::Copper(Layer::Inner(2)));
    assert_eq!(function("-B_Mask.gbr"), Function::Mask(Face::Bottom));
    assert_eq!(function("-F_Silkscreen.gbr"), Function::Silk(Face::Top));
    assert_eq!(function("-Edge_Cuts.gbr"), Function::Outline);
    assert_eq!(function("-NPTH.drl"), Function::Drill);
    assert_eq!(summary.views, ["top", "inner1", "inner2", "bottom", "all"]);
    // The frame is the outline's: its top left corner is the board's drill origin, and the
    // board is the outline's 40.5 by 27 mm.
    let board = summary.board.unwrap();
    assert_eq!((board.width, board.height), (40.5, 27.0));
    assert_eq!((summary.origin.x, summary.origin.y), (0.0, 0.0));
    // Three header pins (one a slot), five vias and two holes with no plating.
    let plated: usize = summary
        .holes
        .iter()
        .filter(|h| h.plated)
        .map(|h| h.holes + h.slots)
        .sum();
    let bare: usize = summary
        .holes
        .iter()
        .filter(|h| !h.plated)
        .map(|h| h.holes + h.slots)
        .sum();
    assert_eq!((plated, bare), (8, 2), "{:?}", summary.holes);
    // The pour is written with clear objects, and the reader takes them away: the copper's
    // area is the area the scanline rasterizer lights, to a percent.
    let poured = summary
        .layers
        .iter()
        .find(|l| l.file.ends_with("-B_Cu.gbr"))
        .unwrap();
    assert!(poured.clear.unwrap() > 0, "the pour has clearances");
    let text = std::str::from_utf8(
        &files
            .iter()
            .find(|f| f.name.ends_with("-B_Cu.gbr"))
            .unwrap()
            .bytes,
    )
    .unwrap();
    let view = RasterView {
        min: (-2.0, -29.0),
        max: (43.0, 2.0),
    };
    let ppm = 40.0;
    let lit = render_gerber(text, &view, ppm).unwrap().lit() as f64 / (ppm * ppm);
    let area = poured.area.unwrap();
    assert!((area - lit).abs() / lit < 0.01, "{area} mm² against {lit} mm²");

    let mut pictures = Vec::new();
    for show in [Show::Top, Show::Bottom, Show::Inner(1), Show::Inner(2), Show::All] {
        pictures.push((show.to_string(), picture(&pkg, show, None, 10.0)));
    }
    // A region of the top, larger, round the slot and the pour's corner.
    pictures.push((
        "top-region".to_string(),
        picture(&pkg, Show::Top, Some([2.0, 2.0, 12.0, 8.0]), 40.0),
    ));
    // The same files give the same bytes.
    assert_eq!(pictures[0].1, picture(&package_of(&features()), Show::Top, None, 10.0));
    golden("features", &pictures);
}

#[test]
fn the_bottom_is_drawn_turned_over() {
    let pkg = package_of(&features());
    let top = viewer::draw(&pkg, Show::Top, None).unwrap();
    let bottom = viewer::draw(&pkg, Show::Bottom, None).unwrap();
    // The region is the same rectangle of the frame; only what is in it is turned over.
    let first = |d: &viewer::Drawing| -> (f64, f64) {
        // The outline's first point, drawn in both.
        let at = d.svg.find("data-function=\"outline\"").unwrap();
        let path = &d.svg[at..];
        let m = path.find(" d=\"M").unwrap() + 5;
        let mut nums = path[m..].split([' ', 'Z']).map(|t| t.parse::<f64>().unwrap());
        (nums.next().unwrap(), nums.next().unwrap())
    };
    let ((tx, ty), (bx, by)) = (first(&top), first(&bottom));
    let [x0, _, w, _] = top.region;
    assert_eq!(top.region, bottom.region);
    assert!((tx - (2.0 * x0 + w - bx)).abs() < 1e-3 && (ty - by).abs() < 1e-9);
    assert!(bottom.drawn.iter().any(|f| f.ends_with("-B_Cu.gbr")));
    assert!(!bottom.drawn.iter().any(|f| f.ends_with("-F_Cu.gbr")));
}

#[test]
fn a_package_another_tool_wrote_is_read_and_drawn() {
    let folder = viewer::open(&data().join("tracespace")).unwrap();
    let s = folder.summary();
    assert!(s.unread.is_empty(), "{:?}", s.unread);
    let layer = |name: &str| s.layers.iter().find(|l| l.file == name).unwrap();
    let copper = layer("tracespace.gtl");
    assert_eq!((copper.function, copper.layer.as_deref()), ("copper", Some("top")));
    assert_eq!(copper.identified_by, Some("name"));
    // A square pad and a thermal flashed clear over it.
    assert_eq!((copper.apertures, copper.flashes, copper.clear), (Some(2), Some(2), Some(1)));
    // The thermal (a ring of 0.4 to 0.5 inches, cut by a cross 0.1 inches wide) is taken out
    // of the 1.1 inch square: about 45.6 mm² of ring less its four gaps.
    let square = 27.94 * 27.94;
    let cut = square - copper.area.unwrap();
    assert!(cut > 30.0 && cut < 45.6, "{cut} mm² cut");
    assert_eq!(layer("tracespace.gko").function, "outline");
    assert_eq!(layer("tracespace.gto").strokes, Some(4));
    let drill = layer("tracespace.drl");
    assert_eq!((drill.format, drill.holes, drill.slots), ("excellon", Some(0), Some(1)));
    // Half an inch, plated (the file does not say otherwise).
    assert_eq!(s.holes.len(), 1);
    assert_eq!((s.holes[0].diameter, s.holes[0].plated, s.holes[0].slots), (12.7, true, 1));
    // The region of arcs is the outline: from 0 to 1.5 inches.
    let board = s.board.unwrap();
    assert!((board.width - 38.1).abs() < 1e-3 && (board.height - 38.1).abs() < 1e-3);

    // The same files in a zip give the same picture.
    let files: Vec<Output> = folder
        .files
        .iter()
        .map(|f| Output {
            name: f.name.clone(),
            bytes: std::fs::read(data().join("tracespace").join(&f.name)).unwrap(),
        })
        .collect();
    let zip = netsmith_testkit::TempDir::new("gerber-view-zip");
    let path = zip.join("tracespace-gerbers.zip");
    std::fs::write(&path, gerber_zip(&files).unwrap()).unwrap();
    let zipped = viewer::open(&path).unwrap();
    assert_eq!(zipped.files[0].name, "gerber/tracespace.drl");
    let pictures = vec![
        ("top".to_string(), picture(&folder, Show::Top, None, 10.0)),
        ("all".to_string(), picture(&folder, Show::All, None, 10.0)),
    ];
    assert_eq!(pictures[0].1, picture(&zipped, Show::Top, None, 10.0));
    golden("tracespace", &pictures);
}

#[test]
fn a_file_with_a_construct_the_reader_does_not_support_is_said_and_not_drawn() {
    let pkg = viewer::open(&data().join("tracespace-unsupported")).unwrap();
    let s = pkg.summary();
    let unread = |name: &str| s.unread.iter().find(|u| u.file == name).unwrap();
    let sr = unread("one-polarity.gbr");
    assert_eq!(sr.line, Some(4));
    assert!(sr.message.contains("%SR"), "{}", sr.message);
    let arc = unread("arc-route.drl");
    assert!(arc.message.contains("G02"), "{}", arc.message);
    assert_eq!(s.unread.len(), 2);
    // The file it reads is drawn; the ones it does not are named, and not in the drawing.
    let all = viewer::draw(&pkg, Show::All, None).unwrap();
    assert_eq!(all.drawn, ["circle-with-hole.gtl"]);
    assert_eq!(all.unread, ["one-polarity.gbr", "arc-route.drl"]);
    assert!(!all.svg.contains("one-polarity"));
    // A side the package has no file for is refused with the ones it has.
    let none = viewer::draw(&pkg, Show::Bottom, None).unwrap_err();
    assert_eq!(
        none.to_string(),
        "the package has no file for `bottom`; it has: top, all"
    );
}
