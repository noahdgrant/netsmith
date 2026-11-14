//! Write the footprint of one part of a KiCad board as a footprint file (`.kicad_mod`), as it was
//! in its library: the footprint a port draws a part with where the original's land pattern is
//! not netsmith's (issue #728). The ports' project libraries of the original footprints were
//! written with it.
//!
//! The footprint is the board's, item for item, with what the board adds taken off: its place,
//! the identifiers (`uuid`, `path`, the sheet), the nets and pin functions of its pads, every
//! property but the reference and the value (the reference becomes `REF**` and the value the
//! footprint's name), and the 3D models (netsmith reads none, and the original's paths are of
//! its own machine). A board stores the angle of a pad and of a text with the footprint's turn
//! added, so the turn is taken off them. A footprint on the back is refused: a board stores it
//! flipped, and no port has needed one yet.
//!
//! Usage: `cargo run -p netsmith-tools --example footprint_from_board -- BOARD.kicad_pcb
//! REFERENCE OUT_DIR [NAME]`; the file is `OUT_DIR/NAME.kicad_mod`, NAME by default the
//! footprint's name in its library (`C_0402` of `Lib:C_0402`).

use std::path::Path;

use netsmith_kicad::Sexp;

/// The children of a footprint that a board adds, or that a library footprint has no use for.
const DROPPED: [&str; 10] = [
    "uuid",
    "tstamp",
    "path",
    "sheetname",
    "sheetfile",
    "model",
    "net",
    "pinfunction",
    "pintype",
    "unlocked",
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, board, reference, out, rest @ ..] = args.as_slice() else {
        eprintln!("usage: footprint_from_board BOARD.kicad_pcb REFERENCE OUT_DIR [NAME]");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(board).unwrap_or_else(|e| panic!("{board}: {e}"));
    let root = Sexp::parse(&text).unwrap_or_else(|e| panic!("{board}: {e}"));
    let footprint = root
        .find_all("footprint")
        .find(|f| property(f, "Reference") == Some(reference.as_str()))
        .unwrap_or_else(|| panic!("{board} has no footprint {reference}"));
    let id = footprint.args().first().and_then(Sexp::text).unwrap_or("");
    let name = rest
        .first()
        .cloned()
        .unwrap_or_else(|| id.rsplit(':').next().unwrap_or(id).to_string());
    if footprint
        .find("layer")
        .and_then(|l| l.args().first()?.text())
        != Some("F.Cu")
    {
        panic!("{reference} is not on F.Cu: a footprint on the back is stored flipped");
    }
    let turn = footprint
        .find("at")
        .and_then(|at| at.args().get(2)?.text()?.parse::<f64>().ok())
        .unwrap_or(0.0);
    // The board's format when it is KiCad 8's or later; a KiCad 7 board's footprint is written
    // as KiCad 9 writes it, its reference and value as properties (below).
    let version = root
        .find("version")
        .and_then(|v| v.args().first()?.text())
        .filter(|v| *v >= "20240108")
        .unwrap_or("20241229");
    let mut items = vec![
        Sexp::string(name.clone()),
        Sexp::node("version", vec![Sexp::atom(version)]),
        Sexp::node("generator", vec![Sexp::string("netsmith")]),
    ];
    for item in footprint.args().iter().skip(1) {
        match item.head() {
            Some("at") => {}
            Some(h) if DROPPED.contains(&h) => {}
            Some("property") => {
                let value = match item.args().first().and_then(Sexp::text) {
                    Some("Reference") => "REF**".to_string(),
                    Some("Value") => name.clone(),
                    _ => continue,
                };
                let mut p = clean(item, turn);
                if let Sexp::List(parts) = &mut p {
                    parts[2] = Sexp::string(value);
                }
                items.push(p);
            }
            // KiCad 7 and earlier write the reference and the value as `fp_text reference` and
            // `fp_text value`, with a bare `hide`; KiCad 8 and later as properties.
            Some("fp_text") => {
                let (field, value) = match item.args().first().and_then(Sexp::text) {
                    Some("reference") => ("Reference", "REF**".to_string()),
                    Some("value") => ("Value", name.clone()),
                    _ => {
                        items.push(clean(item, turn));
                        continue;
                    }
                };
                let Sexp::List(parts) = clean(item, turn) else {
                    unreachable!("an fp_text is a list")
                };
                let mut p = vec![
                    Sexp::atom("property"),
                    Sexp::string(field),
                    Sexp::string(value),
                ];
                for part in parts.into_iter().skip(3) {
                    if part.text() == Some("hide") {
                        p.push(Sexp::node("hide", vec![Sexp::atom("yes")]));
                    } else {
                        p.push(part);
                    }
                }
                items.push(Sexp::List(p));
            }
            _ => items.push(clean(item, turn)),
        }
    }
    let file = Path::new(out).join(format!("{name}.kicad_mod"));
    std::fs::write(&file, Sexp::node("footprint", items).to_kicad_string())
        .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    println!("wrote {} from {reference} ({id})", file.display());
}

/// A footprint's property `name` (`Reference`), or its `fp_text` of that name in lower case, as
/// KiCad 7 and earlier write it.
fn property<'a>(footprint: &'a Sexp, name: &str) -> Option<&'a str> {
    let lower = name.to_lowercase();
    footprint
        .find_all("property")
        .find(|p| p.args().first().and_then(Sexp::text) == Some(name))
        .or_else(|| {
            footprint
                .find_all("fp_text")
                .find(|t| t.args().first().and_then(Sexp::text) == Some(lower.as_str()))
        })
        .and_then(|p| p.args().get(1)?.text())
}

/// `item` with the dropped children taken off, at every depth, and the footprint's `turn` taken
/// off the angle of a pad, a property or a text.
fn clean(item: &Sexp, turn: f64) -> Sexp {
    let Sexp::List(parts) = item else {
        return item.clone();
    };
    let turned = matches!(item.head(), Some("pad" | "property" | "fp_text"));
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        match part.head() {
            Some(h) if DROPPED.contains(&h) => {}
            Some("at") if turned => out.push(unturned(part, turn)),
            _ => out.push(clean(part, turn)),
        }
    }
    Sexp::List(out)
}

/// `(at x y angle)` with `turn` taken off the angle; an angle of 0 is left out.
fn unturned(at: &Sexp, turn: f64) -> Sexp {
    let numbers: Vec<f64> = at
        .args()
        .iter()
        .filter_map(|a| a.text()?.parse().ok())
        .collect();
    let (x, y) = (numbers[0], numbers[1]);
    let angle = (numbers.get(2).copied().unwrap_or(0.0) - turn).rem_euclid(360.0);
    let mut items = vec![Sexp::num(x), Sexp::num(y)];
    if angle.abs() > 1e-9 && (angle - 360.0).abs() > 1e-9 {
        items.push(Sexp::num(angle));
    }
    Sexp::node("at", items)
}
