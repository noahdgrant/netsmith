//! The symbols of the passives and discrete parts are drawn by netsmith (ADR 0097, step 2):
//! a board of resistors, capacitors, inductors, LEDs, diodes, FETs, fuses, crystals, switches
//! and ferrite beads is written with no KiCad symbol library, and every symbol has the pins,
//! the pin positions and the body that KiCad's own symbol of the same id has. The comparison
//! with the installed KiCad runs where KiCad's libraries are installed and is skipped
//! where they are not; the written table of pin geometry below runs everywhere.

use netsmith_core::{Design, Net, PinRef};
use netsmith_kicad::sexp::Sexp;
use netsmith_kicad::testlib::TempDir;
use netsmith_kicad::{Mapping, PinType, Symbol, SymbolLib, emit_schematic, passives, stdlib};
use netsmith_pcb::{FootprintLibrary, Subject, validate_pinout};

use crate::kicad_rows::{part_of, pinout_of};
use netsmith::Libraries;

/// Libraries that read nothing from a folder: whatever they answer is made by netsmith.
fn bare() -> Libraries {
    Libraries::with_mapping(
        Mapping::standard(),
        SymbolLib::at("/nonexistent/symbols").with_generators(),
        FootprintLibrary::at("/nonexistent/footprints").with_generators(),
    )
}

/// The rows of the standard mapping that name a symbol netsmith draws: (kind, package, id).
fn generated_rows() -> Vec<(String, String, String)> {
    let ids: Vec<&str> = passives::ids().collect();
    Mapping::standard()
        .rows()
        .iter()
        .filter_map(|r| {
            let id = r.symbol.as_deref()?;
            ids.contains(&id)
                .then(|| (r.kind.clone(), r.package.clone(), id.to_string()))
        })
        .collect()
}

/// A pin of a KiCad symbol as it is measured: pad, name, type, x, y, direction, length.
type Pin = (&'static str, &'static str, &'static str, f64, f64, f64, f64);

/// The pins of each symbol, in the order KiCad 9 lists them, measured on its stock library
/// (`share/kicad/symbols`, 9.0): the geometry the layout engine and the exporter depend on.
/// `passives_agree_with_the_installed_kicad_symbols` proves the table against the install.
const KICAD_PINS: &[(&str, &[Pin])] = &[
    (
        "Device:R",
        &[
            ("1", "~", "passive", 0.0, 3.81, 270.0, 1.27),
            ("2", "~", "passive", 0.0, -3.81, 90.0, 1.27),
        ],
    ),
    (
        "Device:C",
        &[
            ("1", "~", "passive", 0.0, 3.81, 270.0, 2.794),
            ("2", "~", "passive", 0.0, -3.81, 90.0, 2.794),
        ],
    ),
    (
        "Device:L",
        &[
            ("1", "1", "passive", 0.0, 3.81, 270.0, 1.27),
            ("2", "2", "passive", 0.0, -3.81, 90.0, 1.27),
        ],
    ),
    (
        "Device:FerriteBead",
        &[
            ("1", "~", "passive", 0.0, 3.81, 270.0, 2.54),
            ("2", "~", "passive", 0.0, -3.81, 90.0, 2.54),
        ],
    ),
    (
        "Device:Fuse",
        &[
            ("1", "~", "passive", 0.0, 3.81, 270.0, 1.27),
            ("2", "~", "passive", 0.0, -3.81, 90.0, 1.27),
        ],
    ),
    (
        "Device:LED",
        &[
            ("1", "K", "passive", -3.81, 0.0, 0.0, 2.54),
            ("2", "A", "passive", 3.81, 0.0, 180.0, 2.54),
        ],
    ),
    (
        "Device:D",
        &[
            ("1", "K", "passive", -3.81, 0.0, 0.0, 2.54),
            ("2", "A", "passive", 3.81, 0.0, 180.0, 2.54),
        ],
    ),
    (
        "Device:D_Schottky",
        &[
            ("1", "K", "passive", -3.81, 0.0, 0.0, 2.54),
            ("2", "A", "passive", 3.81, 0.0, 180.0, 2.54),
        ],
    ),
    (
        "Device:D_Zener",
        &[
            ("1", "K", "passive", -3.81, 0.0, 0.0, 2.54),
            ("2", "A", "passive", 3.81, 0.0, 180.0, 2.54),
        ],
    ),
    (
        "Device:D_TVS",
        &[
            ("1", "A1", "passive", -3.81, 0.0, 0.0, 2.54),
            ("2", "A2", "passive", 3.81, 0.0, 180.0, 2.54),
        ],
    ),
    (
        "Device:Crystal",
        &[
            ("1", "1", "passive", -3.81, 0.0, 0.0, 1.27),
            ("2", "2", "passive", 3.81, 0.0, 180.0, 1.27),
        ],
    ),
    (
        "Switch:SW_SPST",
        &[
            ("1", "A", "passive", -5.08, 0.0, 0.0, 2.54),
            ("2", "B", "passive", 5.08, 0.0, 180.0, 2.54),
        ],
    ),
    (
        "Transistor_FET:Q_NMOS_GSD",
        &[
            ("1", "G", "input", -5.08, 0.0, 0.0, 2.54),
            ("3", "D", "passive", 2.54, 5.08, 270.0, 2.54),
            ("2", "S", "passive", 2.54, -5.08, 90.0, 2.54),
        ],
    ),
    (
        "Transistor_FET:Q_PMOS_GSD",
        &[
            ("1", "G", "input", -5.08, 0.0, 0.0, 2.54),
            ("3", "D", "passive", 2.54, 5.08, 270.0, 2.54),
            ("2", "S", "passive", 2.54, -5.08, 90.0, 2.54),
        ],
    ),
];

fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

/// The pins of `symbol` as the table writes them.
fn measured(symbol: &Symbol) -> Vec<(String, String, &'static str, f64, f64, f64, f64)> {
    symbol
        .pins()
        .iter()
        .map(|p| {
            (
                p.number.clone(),
                p.name.clone(),
                p.kind.name(),
                p.x,
                p.y,
                p.angle,
                p.length,
            )
        })
        .collect()
}

#[test]
fn every_generated_symbol_has_the_pin_geometry_written_down() {
    let lib = SymbolLib::at("/nonexistent/symbols").with_generators();
    assert_eq!(KICAD_PINS.len(), passives::ids().count());
    for id in passives::ids() {
        let (_, expected) = KICAD_PINS
            .iter()
            .find(|(i, _)| *i == id)
            .unwrap_or_else(|| panic!("{id} has no entry in KICAD_PINS"));
        let symbol = lib.symbol(id).unwrap();
        let got = measured(&symbol);
        assert_eq!(got.len(), expected.len(), "{id}: {got:?}");
        for (g, e) in got.iter().zip(*expected) {
            assert!(
                g.0 == e.0
                    && g.1 == e.1
                    && g.2 == e.2
                    && near(g.3, e.3, 1e-9)
                    && near(g.4, e.4, 1e-9)
                    && near(g.5, e.5, 1e-9)
                    && near(g.6, e.6, 1e-9),
                "{id}: pin {} is {g:?}, not {e:?}",
                e.0
            );
        }
        assert_eq!(symbol.units(), 1, "{id}");
    }
}

/// The extent of a symbol's drawing (the shapes, not the pins) as the view reads it:
/// `(min x, min y, max x, max y)`.
fn body(symbol: &Symbol) -> (f64, f64, f64, f64) {
    let mut out = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    let mut add = |x: f64, y: f64| {
        out = (out.0.min(x), out.1.min(y), out.2.max(x), out.3.max(y));
    };
    let num = |s: &Sexp, i: usize| s.args()[i].text().unwrap().parse::<f64>().unwrap();
    for unit in symbol.definition().find_all("symbol") {
        for item in unit.args() {
            match item.head() {
                Some("rectangle") => {
                    for end in ["start", "end"] {
                        let p = item.find(end).unwrap();
                        add(num(p, 0), num(p, 1));
                    }
                }
                Some("polyline") => {
                    for p in item.find("pts").unwrap().find_all("xy") {
                        add(num(p, 0), num(p, 1));
                    }
                }
                Some("circle") => {
                    let c = item.find("center").unwrap();
                    let r = num(item.find("radius").unwrap(), 0);
                    add(num(c, 0) - r, num(c, 1) - r);
                    add(num(c, 0) + r, num(c, 1) + r);
                }
                Some("arc") => {
                    for end in ["start", "mid", "end"] {
                        let p = item.find(end).unwrap();
                        add(num(p, 0), num(p, 1));
                    }
                }
                _ => {}
            }
        }
    }
    out
}

fn flags(symbol: &Symbol, list: &str) -> (bool, Option<f64>) {
    let def = symbol.definition();
    let hidden = def
        .find(list)
        .and_then(|l| l.find("hide"))
        .and_then(|h| h.args().first())
        .and_then(Sexp::text)
        == Some("yes");
    let offset = def
        .find(list)
        .and_then(|l| l.find("offset"))
        .and_then(|o| o.args().first())
        .and_then(Sexp::text)
        .and_then(|t| t.parse().ok());
    (hidden, offset)
}

/// How much the extent of the body may differ from KiCad's, in mm: the layout engine routes
/// round the body and writes the texts beside it, so the extents are kept, to the precision
/// of a coil drawn as true half circles.
const BODY_TOLERANCE: f64 = 0.01;

/// The ids whose body differs from KiCad's by more, and by how much (mm), with the reason: the
/// test fails if the difference is more than this, and if it is less than the tolerance above
/// (so the entry cannot go stale).
const BODY_DIFFERENCES: &[(&str, f64, &str)] = &[(
    "Device:FerriteBead",
    0.11,
    "KiCad's bead is a skewed shape whose lower edge is 0.1 mm short of its upper edge; a \
     rectangle turned 30 degrees about the origin is as far down as it is up",
)];

fn body_tolerance(id: &str) -> f64 {
    BODY_DIFFERENCES
        .iter()
        .find(|(i, _, _)| *i == id)
        .map_or(BODY_TOLERANCE, |(_, tolerance, _)| *tolerance)
}

#[test]
fn passives_agree_with_the_installed_kicad_symbols() {
    if netsmith_kicad::testlib::skip_without_kicad_libraries() {
        return;
    }
    let root = Libraries::standard().symbols.root().to_path_buf();
    let kicad = SymbolLib::at(&root);
    let ours = SymbolLib::at("/nonexistent/symbols").with_generators();
    for id in passives::ids() {
        let theirs = kicad.symbol(id).unwrap_or_else(|e| panic!("{id}: {e}"));
        let mine = ours.symbol(id).unwrap();
        // The pins: number, name, type, position, direction, length, in KiCad's order.
        let (a, b) = (measured(&theirs), measured(&mine));
        assert_eq!(a.len(), b.len(), "{id}: {b:?} against KiCad's {a:?}");
        for (t, m) in a.iter().zip(&b) {
            assert!(
                t.0 == m.0
                    && t.1 == m.1
                    && t.2 == m.2
                    && near(t.3, m.3, 1e-6)
                    && near(t.4, m.4, 1e-6)
                    && near(t.5, m.5, 1e-6)
                    && near(t.6, m.6, 1e-6),
                "{id}: pin {} is {m:?}, KiCad's is {t:?}",
                t.0
            );
        }
        assert_eq!(theirs.units(), mine.units(), "{id}");
        assert_eq!(
            theirs
                .pins()
                .iter()
                .map(|p| (p.unit, p.hidden))
                .collect::<Vec<_>>(),
            mine.pins()
                .iter()
                .map(|p| (p.unit, p.hidden))
                .collect::<Vec<_>>(),
            "{id}"
        );
        // The fields and how the pins' texts are shown.
        for field in ["Reference", "Value"] {
            assert_eq!(
                theirs.field_offset(field),
                mine.field_offset(field),
                "{id}: {field}"
            );
        }
        assert_eq!(
            flags(&theirs, "pin_numbers").0,
            flags(&mine, "pin_numbers").0,
            "{id}"
        );
        assert_eq!(
            flags(&theirs, "pin_names"),
            flags(&mine, "pin_names"),
            "{id}"
        );
        // The body's extent.
        let (t, m) = (body(&theirs), body(&mine));
        let tolerance = body_tolerance(id);
        assert!(
            near(t.0, m.0, tolerance)
                && near(t.1, m.1, tolerance)
                && near(t.2, m.2, tolerance)
                && near(t.3, m.3, tolerance),
            "{id}: the body is {m:?}, KiCad's is {t:?}"
        );
        if tolerance > BODY_TOLERANCE {
            assert!(
                !(near(t.0, m.0, BODY_TOLERANCE)
                    && near(t.1, m.1, BODY_TOLERANCE)
                    && near(t.2, m.2, BODY_TOLERANCE)
                    && near(t.3, m.3, BODY_TOLERANCE)),
                "{id} no longer differs from KiCad's body: drop it from BODY_DIFFERENCES"
            );
        }
    }
}

#[test]
fn a_symbol_is_the_same_bytes_every_time() {
    for id in passives::ids() {
        let first = SymbolLib::at("/nonexistent")
            .with_generators()
            .symbol(id)
            .unwrap();
        let second = passives::symbol(id).unwrap();
        assert_eq!(
            first.definition().to_kicad_string(),
            second.definition().to_kicad_string(),
            "{id}"
        );
    }
}

/// Every row that names a generated id, drawn with nothing read from a folder, agrees with the
/// pin table of its kind in pad numbers, names and electrical types, and with its footprint.
#[test]
fn every_row_with_a_generated_symbol_agrees_with_its_pin_table_with_no_kicad() {
    let libraries = bare();
    let rows = generated_rows();
    assert!(rows.len() >= 50, "only {} rows", rows.len());
    let mut checked = 0;
    for (kind, package, id) in &rows {
        let mapped = libraries.mapping.map(kind, Some(package)).unwrap();
        assert_eq!(mapped.lib_id.as_deref(), Some(id.as_str()));
        let symbol = libraries
            .symbols
            .symbol_of(&part_of(kind, package), &mapped)
            .unwrap_or_else(|e| panic!("{kind}/{package}: {e}"));
        assert!(
            stdlib::is_library_id(symbol.lib_id()),
            "{}",
            symbol.lib_id()
        );
        // A footprint nobody generates yet (a tactile switch's, ADR 0097 step 3) needs
        // KiCad's footprint library; its symbol still has to agree with the pin table.
        let Ok(footprint) = libraries.footprints.load(&mapped.footprint) else {
            assert_eq!(kind, "Switch", "{kind}/{package}: no generated footprint");
            let pinout = pinout_of(kind, package);
            let mut pads: Vec<&str> = pinout.pads().collect();
            pads.sort_unstable();
            pads.dedup();
            let mut pins: Vec<&str> = symbol.pins().iter().map(|p| p.number.as_str()).collect();
            pins.sort_unstable();
            assert_eq!(pads, pins, "{kind}/{package}");
            continue;
        };
        checked += 1;
        let subject = Subject {
            refdes: "U1",
            kind,
            package: Some(package),
        };
        let problems = validate_pinout(
            &subject,
            &pinout_of(kind, package),
            &mapped,
            &symbol,
            &footprint,
        );
        assert!(problems.is_empty(), "{kind}/{package}: {problems:#?}");
    }
    assert!(checked >= 45, "only {checked} rows were checked");
}

/// One part of each kind the task names, each pad on a net of its own.
fn every_kind() -> Design {
    let mut parts = Vec::new();
    let mut nets = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (i, (kind, package, id)) in generated_rows().into_iter().enumerate() {
        if seen.contains(&id) && !["Tvs", "TvsBidirectional"].contains(&kind.as_str()) {
            continue;
        }
        seen.push(id);
        let refdes = format!("X{i}");
        let mut part = part_of(&kind, &package);
        part.refdes = refdes.clone();
        part.path = refdes.to_lowercase();
        for pin in &part.pins {
            for pad in &pin.pads {
                nets.push(Net {
                    name: format!("{refdes}-{pad}"),
                    pins: vec![PinRef {
                        refdes: refdes.clone(),
                        pin: pad.clone(),
                    }],
                });
            }
        }
        part.package = Some(package);
        parts.push(part);
    }
    Design {
        name: "every_kind".into(),
        process: None,
        features: Default::default(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        parts,
        nets,
    }
}

#[test]
fn a_board_of_every_kind_is_written_with_no_kicad_symbol_library() {
    let libraries = bare();
    let design = every_kind();
    assert!(design.parts.len() >= 16, "{} parts", design.parts.len());
    let text = emit_schematic(&design, &libraries.mapping, &libraries.symbols)
        .unwrap_or_else(|e| panic!("{e}"));
    // `NETSMITH_WRITE_BOARDS=<folder>` keeps the sheet, to look at it (`scripts/check-kicad.sh`).
    if let Some(folder) = std::env::var_os("NETSMITH_WRITE_BOARDS") {
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            std::path::Path::new(&folder).join("every_kind.kicad_sch"),
            &text,
        )
        .unwrap();
    }
    let root = Sexp::parse(&text).unwrap();
    // Every symbol of the sheet is netsmith's own, embedded once.
    let embedded: Vec<&str> = root
        .find("lib_symbols")
        .unwrap()
        .find_all("symbol")
        .map(|s| s.args()[0].text().unwrap())
        .collect();
    assert!(
        embedded.iter().all(|n| n.starts_with("netsmith:")),
        "{embedded:?}"
    );
    for id in passives::ids() {
        let own = passives::answer_id(id).unwrap();
        assert!(embedded.contains(&own.as_str()), "{own} is not embedded");
        assert!(text.contains(&format!("(lib_id \"{own}\")")), "{own}");
    }
    assert!(!text.contains("(lib_id \"Device:"), "a KiCad id is left");
    // The same bytes on a second run.
    assert_eq!(
        text,
        emit_schematic(&design, &libraries.mapping, &libraries.symbols).unwrap()
    );

    // The board's own library holds the symbols, and its table lists it.
    let dir = TempDir::new("passives-tables");
    libraries.write_tables(dir.path(), &design).unwrap();
    let library = std::fs::read_to_string(dir.join("netsmith.kicad_sym")).unwrap();
    for id in passives::ids() {
        let name = id.split_once(':').unwrap().1;
        assert!(library.contains(&format!("(symbol \"{name}\"")), "{name}");
    }
    let table = std::fs::read_to_string(dir.join("sym-lib-table")).unwrap();
    assert!(table.contains("${KIPRJMOD}/netsmith.kicad_sym"), "{table}");
}

#[test]
fn kicads_erc_accepts_a_board_of_every_kind() {
    let libraries = bare();
    let design = every_kind();
    let text = emit_schematic(&design, &libraries.mapping, &libraries.symbols).unwrap();
    let Some(found) = crate::erc::erc("every_kind", &text) else {
        return;
    };
    // KiCad here has no table for the library `netsmith` (the files are beside a built
    // board), so it says it cannot compare the embedded symbols with their library; that is
    // about the library table, not the circuit.
    let found: Vec<String> = crate::erc::with_the_machines_libraries(found)
        .into_iter()
        .filter(|v| !v.starts_with("lib_symbol"))
        .collect();
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn the_types_of_the_pins_are_those_of_the_part() {
    let lib = SymbolLib::at("/nonexistent").with_generators();
    for id in ["Transistor_FET:Q_NMOS_GSD", "Transistor_FET:Q_PMOS_GSD"] {
        assert_eq!(lib.symbol(id).unwrap().pin_type("1"), Some(PinType::Input));
    }
}
