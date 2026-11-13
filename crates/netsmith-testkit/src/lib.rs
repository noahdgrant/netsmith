//! Helpers for tests across the workspace, in a crate that only `[dev-dependencies]` name, so
//! that a release binary carries none of it. It depends on no netsmith crate.
//!
//! - [`TempDir`] and [`TempFile`]: a folder or file in the temporary directory that is removed
//!   when the test ends, also on a panic.
//! - Small symbol and footprint libraries of our own ([`sample_symbols`],
//!   [`sample_footprints`]). They hold the five kinds the standard mapping (`std/kicad.toml`)
//!   names, with the geometry the stock KiCad libraries give those footprints, so that a test
//!   of the whole pipeline needs no KiCad, and the symbols of a hole, a test point and a
//!   fiducial (the footprints of those three are generated, `netsmith_pcb::generated_footprint`). They are
//!   written to a folder in the system's temporary directory the first time a process asks. A
//!   test that wants the stock libraries uses `netsmith_kicad::SymbolLib::installed` and skips
//!   itself without KiCad (`netsmith_kicad::skip_without_kicad_libraries`, behind
//!   netsmith-kicad's `test-support` feature).
//! - [`split_physical`]: a source written before the physical file, split into the two files.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The environment variable that keeps the folders of [`TempDir`] after a test, so that a
/// failure can be inspected: set `NETSMITH_KEEP_TEST_DIRS=1`.
pub const KEEP_VARIABLE: &str = "NETSMITH_KEEP_TEST_DIRS";

/// A folder in the temporary directory that is removed when it is dropped, also when the
/// test that holds it panics. The name is `netsmith-<label>-<process id>-<count>`, so that
/// a leftover folder says what made it. With `NETSMITH_KEEP_TEST_DIRS=1` in the
/// environment the folder is kept.
///
/// It dereferences to [`Path`], so `dir.join("x")` and `fs::write(&dir, ..)` read as they do
/// with a `PathBuf`. Keep it alive for as long as the test uses the folder: bind it to a
/// name (`_dir`, not `_`), because `_` drops it at once.
#[derive(Debug)]
pub struct TempDir(PathBuf);

impl TempDir {
    /// Makes the folder, empty.
    pub fn new(label: &str) -> TempDir {
        let dir = TempDir::unused(label);
        std::fs::create_dir_all(&dir.0).expect("the temporary directory can be written");
        dir
    }

    /// Makes the folder, empty, in `parent` instead of the system's temporary directory (for
    /// a program that cannot read from there, such as Windows KiCad).
    pub fn new_in(parent: &Path, label: &str) -> TempDir {
        let dir = TempDir::unused_in(parent, label);
        std::fs::create_dir_all(&dir.0).expect("the temporary directory can be written");
        dir
    }

    /// Picks a name and makes nothing: for a test that needs a folder that does not exist
    /// yet. What is made there is removed when the guard is dropped.
    pub fn unused(label: &str) -> TempDir {
        TempDir::unused_in(&std::env::temp_dir(), label)
    }

    fn unused_in(parent: &Path, label: &str) -> TempDir {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        TempDir(parent.join(format!(
            "netsmith-{label}-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::SeqCst)
        )))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl std::ops::Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if keeps(std::env::var_os(KEEP_VARIABLE).as_deref()) {
            return;
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A path to a file named `name` in a [`TempDir`] of its own; the file is not made. The folder,
/// and so the file, is removed when the guard is dropped.
#[derive(Debug)]
pub struct TempFile {
    _dir: TempDir,
    path: PathBuf,
}

impl TempFile {
    pub fn new(label: &str, name: &str) -> TempFile {
        let dir = TempDir::new(label);
        let path = dir.join(name);
        TempFile { _dir: dir, path }
    }
}

impl std::ops::Deref for TempFile {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for TempFile {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

/// Whether the value of `NETSMITH_KEEP_TEST_DIRS` asks to keep the folders.
fn keeps(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|v| !v.is_empty() && v != "0")
}

const FONT: &str = "(effects (font (size 1.27 1.27)))";

fn pin(
    kind: &str,
    extra: &str,
    at: (f64, f64, f64),
    length: f64,
    name: &str,
    number: &str,
) -> String {
    format!(
        "(pin {kind} line (at {} {} {}) {extra} (length {length}) \
         (name \"{name}\" {FONT}) (number \"{number}\" {FONT}))",
        at.0, at.1, at.2
    )
}

fn property(name: &str, value: &str, at: (f64, f64, f64)) -> String {
    format!(
        "(property \"{name}\" \"{value}\" (at {} {} {}) {FONT})",
        at.0, at.1, at.2
    )
}

fn rectangle(a: (f64, f64), b: (f64, f64)) -> String {
    drawn_rectangle(a, b, "none")
}

fn drawn_rectangle(a: (f64, f64), b: (f64, f64), fill: &str) -> String {
    format!(
        "(rectangle (start {} {}) (end {} {}) (stroke (width 0.254) (type default)) \
         (fill (type {fill})))",
        a.0, a.1, b.0, b.1
    )
}

/// A symbol of one unit: its properties, drawing and pins.
fn symbol(
    name: &str,
    fields: [(f64, f64, f64); 2],
    value: &str,
    drawing: &str,
    pins: &[String],
) -> String {
    format!(
        "(symbol \"{name}\" (pin_numbers (hide yes)) (pin_names (offset 0)) \
         (exclude_from_sim no) (in_bom yes) (on_board yes) {} {} {} {} {} \
         (symbol \"{name}_0_1\" {drawing}) (symbol \"{name}_1_1\" {}))",
        property("Reference", &name[..1], fields[0]),
        property("Value", value, fields[1]),
        property("Footprint", "", (0.0, 0.0, 0.0)),
        property("Datasheet", "~", (0.0, 0.0, 0.0)),
        property("Description", "", (0.0, 0.0, 0.0)),
        pins.join(" ")
    )
}

fn library(symbols: &[String]) -> String {
    format!(
        "(kicad_symbol_lib (version 20241209) (generator \"netsmith-test\") {})\n",
        symbols.join("\n")
    )
}

/// The stock-like `Device` library: `R`, `C` and `LED`.
pub fn device() -> String {
    let vertical = |kind: &str| {
        [
            pin(kind, "", (0.0, 3.81, 270.0), 1.27, "~", "1"),
            pin(kind, "", (0.0, -3.81, 90.0), 1.27, "~", "2"),
        ]
    };
    let r = symbol(
        "R",
        [(2.032, 0.0, 90.0), (0.0, 0.0, 90.0)],
        "R",
        &rectangle((-1.016, 2.54), (1.016, -2.54)),
        &vertical("passive"),
    );
    let c = symbol(
        "C",
        [(0.635, 2.54, 0.0), (0.635, -2.54, 0.0)],
        "C",
        &rectangle((-2.032, 0.762), (2.032, 0.508)),
        &vertical("passive"),
    );
    let led = symbol(
        "LED",
        [(0.0, 2.54, 0.0), (0.0, -2.54, 0.0)],
        "LED",
        "(polyline (pts (xy -1.27 1.27) (xy -1.27 -1.27) (xy 1.27 0) (xy -1.27 1.27)) \
         (stroke (width 0.254) (type default)) (fill (type none)))",
        &[
            pin("passive", "", (-3.81, 0.0, 0.0), 2.54, "K", "1"),
            pin("passive", "", (3.81, 0.0, 180.0), 2.54, "A", "2"),
        ],
    );
    library(&[r, c, led])
}

/// A single-row connector symbol of `n` pins, as KiCad draws them.
fn connector_symbol(name: &str, n: usize) -> String {
    let pins: Vec<String> = (0..n)
        .map(|i| {
            pin(
                "passive",
                "",
                (-5.08, -2.54 * i as f64, 0.0),
                3.81,
                &format!("Pin_{}", i + 1),
                &(i + 1).to_string(),
            )
        })
        .collect();
    symbol(
        name,
        [(0.0, 2.54, 0.0), (0.0, -2.54 * n as f64, 0.0)],
        name,
        &rectangle((-1.27, 1.27), (1.27, -2.54 * (n as f64 - 1.0) - 1.27)),
        &pins,
    )
}

/// Our own `Connector_Generic`, `Regulator_Linear` and `power` libraries.
fn connector() -> String {
    library(&[
        connector_symbol("Conn_01x02", 2),
        connector_symbol("Conn_01x04", 4),
    ])
}

/// Our own `Connector` library: the sockets that mate with `Conn_01x02` and `Conn_01x04`, and
/// the `TestPoint`.
fn socket() -> String {
    library(&[
        connector_symbol("Conn_01x02_Socket", 2),
        connector_symbol("Conn_01x04_Socket", 4),
        test_point(),
    ])
}

fn regulator() -> String {
    library(&[symbol(
        "AP1117-15",
        [(0.0, 3.81, 0.0), (0.0, 1.27, 0.0)],
        "AP1117-15",
        &drawn_rectangle((-5.08, 2.54), (5.08, -5.08), "background"),
        &[
            pin("power_in", "", (0.0, -7.62, 90.0), 2.54, "GND", "1"),
            pin("power_out", "", (7.62, 0.0, 180.0), 2.54, "VO", "2"),
            pin("power_in", "", (-7.62, 0.0, 0.0), 2.54, "VI", "3"),
        ],
    )])
}

fn power() -> String {
    library(&[symbol(
        "PWR_FLAG",
        [(0.0, 3.81, 0.0), (0.0, 1.905, 0.0)],
        "PWR_FLAG",
        "(polyline (pts (xy 0 0) (xy 0 1.27)) (stroke (width 0) (type default)) (fill (type none)))",
        &[pin("power_out", "", (0.0, 0.0, 90.0), 0.0, "pwr", "1")],
    )])
}

/// Our own `Mechanical` library, with the three symbols the standard mapping names (a hole,
/// a plated hole and a fiducial), and the `TestPoint` that goes into `Connector`.
fn mechanical() -> String {
    let one_pin = |kind: &str| [pin(kind, "", (0.0, -2.54, 90.0), 1.27, "1", "1")];
    library(&[
        symbol(
            "MountingHole",
            [(0.0, 3.81, 0.0), (0.0, -3.81, 0.0)],
            "MountingHole",
            "(circle (center 0 0) (radius 1.27) (stroke (width 0.254) (type default)) (fill (type none)))",
            &[],
        ),
        symbol(
            "MountingHole_Pad",
            [(0.0, 3.81, 0.0), (0.0, -3.81, 0.0)],
            "MountingHole_Pad",
            "(circle (center 0 0) (radius 1.27) (stroke (width 0.254) (type default)) (fill (type none)))",
            &one_pin("passive"),
        ),
        symbol(
            "Fiducial",
            [(0.0, 3.81, 0.0), (0.0, -3.81, 0.0)],
            "Fiducial",
            "(circle (center 0 0) (radius 1.27) (stroke (width 0.254) (type default)) (fill (type none)))",
            &[],
        ),
    ])
}

fn test_point() -> String {
    symbol(
        "TestPoint",
        [(0.0, 3.81, 0.0), (0.0, -3.81, 0.0)],
        "TestPoint",
        "(circle (center 0 1.27) (radius 0.762) (stroke (width 0) (type default)) (fill (type none)))",
        &[pin("passive", "", (0.0, 0.0, 90.0), 1.27, "1", "1")],
    )
}

/// A library for the symbol tests: `Reg`, `Reg33` that extends it, the three-unit `Dual`,
/// `Shared` with a pin in unit 0, `Stacked` with hidden duplicate pins, `Gate` with an
/// alternate body style, and `Types` with one pin of each electrical type.
pub fn acme() -> String {
    let reg = symbol(
        "Reg",
        [(0.0, 3.81, 0.0), (0.0, 1.27, 0.0)],
        "Reg",
        &rectangle((-5.08, 2.54), (5.08, -5.08)),
        &[
            pin("power_in", "", (0.0, -7.62, 90.0), 2.54, "GND", "1"),
            pin("power_out", "", (7.62, 0.0, 180.0), 2.54, "VO", "2"),
            pin("power_in", "", (-7.62, 0.0, 0.0), 2.54, "VI", "3"),
        ],
    );
    let reg33 = format!(
        "(symbol \"Reg33\" (extends \"Reg\") {})",
        property("Value", "Reg33", (0.0, 1.27, 0.0))
    );
    let unit = |name: &str, n: u32, pins: &[String]| {
        format!(
            "(symbol \"{name}_{n}_1\" {} {})",
            rectangle((-3.81, 3.81), (3.81, -3.81)),
            pins.join(" ")
        )
    };
    let dual = format!(
        "(symbol \"Dual\" (pin_names (offset 0)) (exclude_from_sim no) (in_bom yes) (on_board yes) \
         {} {} {} {} {} {} {} {})",
        property("Reference", "U", (0.0, 5.08, 0.0)),
        property("Value", "Dual", (0.0, -5.08, 0.0)),
        property("Footprint", "", (0.0, 0.0, 0.0)),
        property("Datasheet", "~", (0.0, 0.0, 0.0)),
        property("Description", "", (0.0, 0.0, 0.0)),
        unit(
            "Dual",
            1,
            &[
                pin("input", "", (-6.35, 1.27, 0.0), 2.54, "+", "3"),
                pin("input", "", (-6.35, -1.27, 0.0), 2.54, "-", "2"),
                pin("output", "", (6.35, 0.0, 180.0), 2.54, "OUT", "1"),
            ]
        ),
        unit(
            "Dual",
            2,
            &[
                pin("input", "", (-6.35, 1.27, 0.0), 2.54, "+", "5"),
                pin("input", "", (-6.35, -1.27, 0.0), 2.54, "-", "6"),
                pin("output", "", (6.35, 0.0, 180.0), 2.54, "OUT", "7"),
            ]
        ),
        unit(
            "Dual",
            3,
            &[
                pin("power_in", "", (0.0, 6.35, 270.0), 2.54, "V+", "8"),
                pin("power_in", "", (0.0, -6.35, 90.0), 2.54, "V-", "4"),
            ]
        ),
    );
    let shared = format!(
        "(symbol \"Shared\" (exclude_from_sim no) (in_bom yes) (on_board yes) {} {} {} {} {} {} {} {})",
        property("Reference", "U", (0.0, 5.08, 0.0)),
        property("Value", "Shared", (0.0, -5.08, 0.0)),
        property("Footprint", "", (0.0, 0.0, 0.0)),
        property("Datasheet", "~", (0.0, 0.0, 0.0)),
        property("Description", "", (0.0, 0.0, 0.0)),
        unit(
            "Shared",
            0,
            &[pin("power_in", "", (0.0, -6.35, 90.0), 2.54, "GND", "9")]
        ),
        unit(
            "Shared",
            1,
            &[
                pin("input", "", (-6.35, 0.0, 0.0), 2.54, "A", "1"),
                pin("output", "", (6.35, 0.0, 180.0), 2.54, "B", "2"),
            ]
        ),
        unit(
            "Shared",
            2,
            &[
                pin("input", "", (-6.35, 0.0, 0.0), 2.54, "C", "3"),
                pin("output", "", (6.35, 0.0, 180.0), 2.54, "D", "4"),
            ]
        ),
    );
    let stacked = symbol(
        "Stacked",
        [(0.0, 3.81, 0.0), (0.0, 1.27, 0.0)],
        "Stacked",
        &rectangle((-5.08, 2.54), (5.08, -5.08)),
        &[
            pin("power_out", "", (7.62, 2.54, 180.0), 2.54, "OUT", "1"),
            pin(
                "passive",
                "(hide yes)",
                (7.62, 2.54, 180.0),
                2.54,
                "OUT",
                "2",
            ),
            pin("passive", "hide", (7.62, 2.54, 180.0), 2.54, "OUT", "3"),
            pin("power_in", "", (-7.62, 0.0, 0.0), 2.54, "IN", "4"),
            pin("power_in", "", (0.0, -7.62, 90.0), 2.54, "GND", "5"),
        ],
    );
    let gate = format!(
        "(symbol \"Gate\" (exclude_from_sim no) (in_bom yes) (on_board yes) {} {} \
         (symbol \"Gate_1_1\" {} {} {} {}) (symbol \"Gate_1_2\" {} {} {})) ",
        property("Reference", "U", (0.0, 5.08, 0.0)),
        property("Value", "Gate", (0.0, -5.08, 0.0)),
        pin("input", "", (-6.35, 1.27, 0.0), 2.54, "A", "1"),
        pin("input", "", (-6.35, -1.27, 0.0), 2.54, "B", "2"),
        pin("output", "", (6.35, 0.0, 180.0), 2.54, "Y", "3"),
        rectangle((-3.81, 2.54), (3.81, -2.54)),
        pin("input", "", (-6.35, 2.54, 0.0), 2.54, "A", "1"),
        pin("input", "", (-6.35, -2.54, 0.0), 2.54, "B", "2"),
        pin("output", "", (6.35, 0.0, 180.0), 2.54, "Y", "3"),
    );
    let kinds = [
        "input",
        "output",
        "bidirectional",
        "tri_state",
        "passive",
        "free",
        "unspecified",
        "power_in",
        "power_out",
        "open_collector",
        "open_emitter",
        "no_connect",
    ];
    let types = symbol(
        "Types",
        [(0.0, 3.81, 0.0), (0.0, 1.27, 0.0)],
        "Types",
        &rectangle((-5.08, 2.54), (5.08, -30.0)),
        &kinds
            .iter()
            .enumerate()
            .map(|(i, k)| {
                pin(
                    k,
                    "",
                    (-7.62, -2.54 * i as f64, 0.0),
                    2.54,
                    &format!("P{}", i + 1),
                    &(i + 1).to_string(),
                )
            })
            .collect::<Vec<_>>(),
    );
    library(&[reg, reg33, dual, shared, stacked, gate, types])
}

fn footprint(
    name: &str,
    descr: &str,
    attr: &str,
    graphics: &str,
    pads: &str,
    label_y: f64,
) -> String {
    format!(
        "(footprint \"{name}\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
         (descr \"{descr}\") \
         (property \"Reference\" \"REF**\" (at 0 {label_y} 0) (layer \"F.SilkS\") \
         (effects (font (size 1 1) (thickness 0.15)))) \
         (property \"Value\" \"{name}\" (at 0 {} 0) (layer \"F.Fab\") \
         (effects (font (size 1 1) (thickness 0.15)))) \
         (attr {attr}) {graphics} {pads}\
         (model \"${{KICAD9_3DMODEL_DIR}}/{name}.wrl\" (offset (xyz 0 0 0)) (scale (xyz 1 1 1)) \
         (rotate (xyz 0 0 0))))\n",
        -label_y
    )
}

fn line(layer: &str, width: f64, a: (f64, f64), b: (f64, f64)) -> String {
    format!(
        "(fp_line (start {} {}) (end {} {}) (stroke (width {width}) (type solid)) (layer \"{layer}\")) ",
        a.0, a.1, b.0, b.1
    )
}

/// A closed rectangle of four lines, centred on the origin.
fn box_of(layer: &str, width: f64, half: (f64, f64)) -> String {
    rect_of(layer, width, (-half.0, -half.1), (half.0, half.1))
}

fn rect_of(layer: &str, width: f64, min: (f64, f64), max: (f64, f64)) -> String {
    [
        line(layer, width, (min.0, min.1), (max.0, min.1)),
        line(layer, width, (max.0, min.1), (max.0, max.1)),
        line(layer, width, (max.0, max.1), (min.0, max.1)),
        line(layer, width, (min.0, max.1), (min.0, min.1)),
    ]
    .concat()
}

fn smd_pad(number: &str, shape: &str, at: (f64, f64), size: (f64, f64)) -> String {
    format!(
        "(pad \"{number}\" smd {shape} (at {} {}) (size {} {}) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) ",
        at.0, at.1, size.0, size.1
    )
}

/// A two-pad chip footprint.
fn chip(name: &str, pad_x: f64, pad: (f64, f64), courtyard: (f64, f64), body_y: f64) -> String {
    let mut graphics = line("F.SilkS", 0.12, (-0.15, -body_y), (0.15, -body_y));
    graphics += &line("F.SilkS", 0.12, (-0.15, body_y), (0.15, body_y));
    graphics += &box_of("F.CrtYd", 0.05, courtyard);
    graphics += &box_of("F.Fab", 0.1, (pad_x, body_y - 0.08));
    let pads = smd_pad("1", "roundrect", (-pad_x, 0.0), pad)
        + &smd_pad("2", "roundrect", (pad_x, 0.0), pad);
    footprint(name, "chip", "smd", &graphics, &pads, -courtyard.1 - 0.7)
}

fn header(name: &str, n: usize) -> String {
    let last = 2.54 * (n as f64 - 1.0);
    let graphics = rect_of("F.CrtYd", 0.05, (-1.8, -1.8), (1.8, last + 1.81))
        + &line("F.SilkS", 0.12, (-1.33, -1.33), (1.33, -1.33));
    let pads: String = (0..n)
        .map(|i| {
            format!(
                "(pad \"{}\" thru_hole {} (at 0 {}) (size 1.7 1.7) (drill 1) (layers \"*.Cu\" \"*.Mask\")) ",
                i + 1,
                if i == 0 { "rect" } else { "oval" },
                2.54 * i as f64
            )
        })
        .collect();
    let pads = pads.as_str();
    footprint(name, "header", "through_hole", &graphics, pads, -2.5)
}

fn sot23() -> String {
    let graphics =
        box_of("F.CrtYd", 0.05, (1.92, 1.7)) + &line("F.SilkS", 0.12, (-0.9, -1.56), (0.9, -1.56));
    let size = (1.475, 0.6);
    let pads = smd_pad("1", "roundrect", (-0.9375, -0.95), size)
        + &smd_pad("2", "roundrect", (-0.9375, 0.95), size)
        + &smd_pad("3", "roundrect", (0.9375, 0.0), size);
    footprint("SOT-23", "SOT-23", "smd", &graphics, &pads, -2.4)
}

fn sot223() -> String {
    let graphics = box_of("F.CrtYd", 0.05, (4.4, 3.6))
        + &line("F.SilkS", 0.12, (-1.8, -3.36), (1.8, -3.36))
        // A marker beside pad 1.
        + "(fp_poly (pts (xy -4 -2.8) (xy -4.6 -2.5) (xy -4 -2.2)) (stroke (width 0.12) \
           (type solid)) (fill none) (layer \"F.SilkS\")) ";
    let pads = smd_pad("1", "roundrect", (-3.15, -2.3), (2.0, 1.5))
        + &smd_pad("2", "roundrect", (-3.15, 0.0), (2.0, 1.5))
        + &smd_pad("2", "roundrect", (3.15, 0.0), (2.0, 3.8))
        + &smd_pad("3", "roundrect", (-3.15, 2.3), (2.0, 1.5));
    footprint(
        "SOT-223-3_TabPin2",
        "SOT-223",
        "smd",
        &graphics,
        &pads,
        -4.3,
    )
}

/// Every footprint the standard mapping names, as `(library, name, text)`.
fn footprints() -> Vec<(&'static str, String, String)> {
    let mut all = Vec::new();
    for (library, prefix, suffix, sizes) in [
        (
            "Resistor_SMD",
            "R_",
            "Metric",
            [
                ("0402_1005", 0.51, (0.54, 0.64), (0.93, 0.47), 0.38),
                ("0603_1608", 0.825, (0.8, 0.95), (1.65, 0.73), 0.5),
                ("0805_2012", 0.9125, (1.025, 1.4), (1.68, 0.98), 0.73),
            ],
        ),
        (
            "Capacitor_SMD",
            "C_",
            "Metric",
            [
                ("0402_1005", 0.48, (0.56, 0.62), (0.91, 0.46), 0.38),
                ("0603_1608", 0.775, (0.75, 0.9), (1.6, 0.73), 0.5),
                ("0805_2012", 0.95, (1.0, 1.45), (1.7, 0.98), 0.73),
            ],
        ),
        (
            "LED_SMD",
            "LED_",
            "Metric",
            [
                ("0402_1005", 0.5, (0.5, 0.55), (0.93, 0.47), 0.38),
                ("0603_1608", 0.7875, (0.875, 0.95), (1.48, 0.73), 0.5),
                ("0805_2012", 1.0, (1.2, 1.2), (1.68, 0.98), 0.73),
            ],
        ),
    ] {
        for (size, pad_x, pad, courtyard, body_y) in sizes {
            let name = format!("{prefix}{size}{suffix}");
            let text = chip(&name, pad_x, pad, courtyard, body_y);
            all.push((library, name, text));
        }
    }
    for (library, kind) in [
        ("Connector_PinHeader_2.54mm", "PinHeader"),
        ("Connector_PinSocket_2.54mm", "PinSocket"),
    ] {
        for n in [2, 4] {
            let name = format!("{kind}_1x{n:02}_P2.54mm_Vertical");
            all.push((library, name.clone(), header(&name, n)));
        }
    }
    all.push(("Package_TO_SOT_SMD", "SOT-23".to_string(), sot23()));
    all.push((
        "Package_TO_SOT_SMD",
        "SOT-223-3_TabPin2".to_string(),
        sot223(),
    ));
    all
}

fn write_sample(dir: &Path) -> std::io::Result<()> {
    let symbols = dir.join("symbols");
    std::fs::create_dir_all(&symbols)?;
    for (name, text) in [
        ("Device", device()),
        ("Connector_Generic", connector()),
        ("Connector", socket()),
        ("Regulator_Linear", regulator()),
        ("power", power()),
        ("Mechanical", mechanical()),
    ] {
        std::fs::write(symbols.join(format!("{name}.kicad_sym")), text)?;
    }
    for (library, name, text) in footprints() {
        let folder = dir.join("footprints").join(format!("{library}.pretty"));
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join(format!("{name}.kicad_mod")), text)?;
    }
    Ok(())
}

/// A folder holding `symbols/` and `footprints/` with the libraries the standard mapping
/// needs. The same folder is reused by every test of every process, and is written once.
pub fn sample_root() -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    device().hash(&mut hasher);
    socket().hash(&mut hasher);
    mechanical().hash(&mut hasher);
    for (_, _, text) in footprints() {
        text.hash(&mut hasher);
    }
    let target =
        std::env::temp_dir().join(format!("netsmith-sample-libraries-{:x}", hasher.finish()));
    if !target.join("footprints").is_dir() || !target.join("symbols/Mechanical.kicad_sym").is_file()
    {
        // Written beside the target and moved in, so a process that reads the folder while
        // another writes it never sees half of it.
        let staging = TempDir::new("sample-staging");
        write_sample(staging.path()).expect("the temporary directory can be written");
        let finished = staging.path().to_path_buf();
        if std::fs::rename(&finished, &target).is_err() && !target.join("footprints").is_dir() {
            panic!("cannot write the sample libraries to {}", target.display());
        }
    }
    target
}

/// Our sample symbol libraries.
pub fn sample_symbols() -> PathBuf {
    sample_root().join("symbols")
}

/// Our sample footprint libraries.
pub fn sample_footprints() -> PathBuf {
    sample_root().join("footprints")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    /// The tests of removal mean nothing when the person running them asked to keep the
    /// folders.
    fn kept_on_request() -> bool {
        keeps(std::env::var_os(KEEP_VARIABLE).as_deref())
    }

    #[test]
    fn a_dropped_folder_is_removed_with_what_is_in_it() {
        if kept_on_request() {
            return;
        }
        let dir = TempDir::new("testlib-drop");
        let path = dir.path().to_path_buf();
        std::fs::create_dir_all(dir.join("deep")).unwrap();
        std::fs::write(dir.join("deep/file"), "x").unwrap();
        assert!(path.is_dir());
        drop(dir);
        assert!(!path.exists());
    }

    #[test]
    fn a_folder_is_removed_when_the_test_panics() {
        if kept_on_request() {
            return;
        }
        let seen = std::sync::Mutex::new(None);
        let result = std::panic::catch_unwind(|| {
            let dir = TempDir::new("testlib-panic");
            std::fs::write(dir.join("file"), "x").unwrap();
            *seen.lock().unwrap() = Some(dir.path().to_path_buf());
            panic!("the test fails");
        });
        assert!(result.is_err());
        let path = seen.lock().unwrap().take().unwrap();
        assert!(!path.exists(), "{} was left behind", path.display());
    }

    #[test]
    fn a_temp_file_is_a_path_in_a_folder_that_goes_with_it() {
        if kept_on_request() {
            return;
        }
        let file = TempFile::new("testlib-file", "a.txt");
        let folder = file.parent().unwrap().to_path_buf();
        assert!(folder.is_dir() && !file.exists());
        std::fs::write(&file, "x").unwrap();
        drop(file);
        assert!(!folder.exists());
    }

    #[test]
    fn an_unused_folder_is_not_made_but_what_is_made_there_is_removed() {
        if kept_on_request() {
            return;
        }
        let dir = TempDir::unused("testlib-unused");
        assert!(!dir.exists());
        std::fs::create_dir_all(dir.join("a")).unwrap();
        let path = dir.path().to_path_buf();
        drop(dir);
        assert!(!path.exists());
    }

    #[test]
    fn folders_have_distinct_names_that_say_what_made_them() {
        let (a, b) = (TempDir::new("testlib-names"), TempDir::new("testlib-names"));
        assert_ne!(a.path(), b.path());
        let name = a.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("netsmith-testlib-names-"), "{name}");
    }

    #[test]
    fn the_keep_variable_asks_to_keep_the_folders() {
        assert!(!keeps(None));
        assert!(!keeps(Some(OsStr::new(""))));
        assert!(!keeps(Some(OsStr::new("0"))));
        assert!(keeps(Some(OsStr::new("1"))));
        assert!(keeps(Some(OsStr::new("yes"))));
    }
}

// ---- sources written before the physical file ---------------------------------------

/// A source text that states a board's outline, cutouts, holes, keepouts and `.place`
/// requirements inside the board, as the language took them before ADR 0048, split into the
/// two files that the language takes now: the circuit source, and the text of the physical
/// file (none when the source had no physical statement).
///
/// This is for tests of a later stage (the design stage, the placer, the checks) that build many
/// boards from strings and care about what is stated, not which file it is in. It splits
/// each `board Name { ... }` at its top-level statements (four spaces in): a declaration of
/// an `Outline`, `Cutout`, `MountingHole`, `Fiducial` or `Label` goes in the `case { }`
/// section, a `Keepout` and a statement on a `.place` go in the unit `physical for Name`.
/// The tests of the pairing, the reach and the errors write real files.
pub fn split_physical(source: &str) -> (String, Option<String>) {
    const CASE_TYPES: [&str; 5] = ["Outline", "Cutout", "MountingHole", "Fiducial", "Label"];
    fn declared_type(line: &str) -> Option<&str> {
        let rest = line.strip_prefix("    .")?;
        let colon = rest.find(": ")?;
        let name = &rest[..colon];
        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        {
            return None;
        }
        let ty = rest[colon + 2..].trim_start();
        let end = ty
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(ty.len());
        Some(&ty[..end])
    }
    let mut circuit = String::new();
    let mut physical = String::new();
    let mut board: Option<String> = None;
    // The lines of the statement being collected, and where it goes: 0 circuit, 1 board
    // level of the physical file, 2 its case section.
    let mut chunk: Vec<&str> = Vec::new();
    let mut place = 0;
    let mut case = String::new();
    let mut loose = String::new();
    let flush = |chunk: &mut Vec<&str>,
                 place: usize,
                 circuit: &mut String,
                 case: &mut String,
                 loose: &mut String| {
        let text: String = chunk.iter().map(|l| format!("{l}\n")).collect();
        match place {
            0 => circuit.push_str(&text),
            1 => loose.push_str(&text),
            _ => case.push_str(&text),
        }
        chunk.clear();
    };
    for line in source.lines() {
        let Some(name) = &board else {
            if let Some(rest) = line.strip_prefix("board ")
                && line.trim_end().ends_with('{')
            {
                board = Some(
                    rest.chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect(),
                );
            }
            circuit.push_str(line);
            circuit.push('\n');
            continue;
        };
        if line == "}" {
            flush(&mut chunk, place, &mut circuit, &mut case, &mut loose);
            if !case.is_empty() || !loose.is_empty() {
                physical.push_str(&format!("physical for {name} {{\n"));
                if !case.is_empty() {
                    physical.push_str("    case {\n");
                    for l in case.lines() {
                        physical.push_str(&format!("    {l}\n"));
                    }
                    physical.push_str("    }\n");
                }
                physical.push_str(&loose);
                physical.push_str("}\n\n");
            }
            case.clear();
            loose.clear();
            board = None;
            circuit.push_str("}\n");
            continue;
        }
        let starts = line.starts_with("    ")
            && !line.starts_with("     ")
            && !line.trim_start().starts_with('}');
        let comment = line.trim_start().starts_with("//");
        if starts || (chunk.is_empty() && !line.trim().is_empty()) {
            // A new statement (or a comment above one) begins. Comments held so far belong
            // to the statement that follows them.
            let held_comments = chunk.iter().all(|l| l.trim_start().starts_with("//"));
            if !held_comments {
                flush(&mut chunk, place, &mut circuit, &mut case, &mut loose);
            }
            if !comment {
                place = match declared_type(line) {
                    Some(ty) if CASE_TYPES.contains(&ty) => 2,
                    Some("Keepout") => 1,
                    _ if line.contains(".place.") => 1,
                    _ => 0,
                };
            }
        }
        chunk.push(line);
    }
    let text = if physical.is_empty() {
        None
    } else {
        Some(
            format!(
                "import std::pcb::{{Cutout, Keepout, Outline}}\nimport std::parts::mechanical::{{Fiducial, Label, MountingHole}}\n\n{}",
                physical.trim_end()
            ) + "\n",
        )
    };
    (circuit, text)
}

#[cfg(test)]
mod split_tests {
    use super::split_physical;

    #[test]
    fn a_board_is_split_into_its_circuit_and_its_physical_statements() {
        let (circuit, physical) = split_physical(
            "import a::B\nboard X {\n    .a: R\n    // the case\n    .outline: Outline {\n        .width &= 4mm\n    }\n    .a.place.x &= 1mm\n    .zone: Keepout { .x &= 1mm }\n    .b.height &= 2mm\n}\n",
        );
        assert_eq!(
            circuit,
            "import a::B\nboard X {\n    .a: R\n    .b.height &= 2mm\n}\n"
        );
        let physical = physical.unwrap();
        assert!(
            physical.contains(
                "physical for X {\n    case {\n        // the case\n        .outline: Outline {\n            .width &= 4mm\n        }\n    }\n    .a.place.x &= 1mm\n    .zone: Keepout { .x &= 1mm }\n}"
            ),
            "{physical}"
        );
    }

    #[test]
    fn a_board_without_physical_statements_has_no_physical_file() {
        let (circuit, physical) = split_physical("board X {\n    .a: R\n}\n");
        assert_eq!(circuit, "board X {\n    .a: R\n}\n");
        assert_eq!(physical, None);
    }
}
