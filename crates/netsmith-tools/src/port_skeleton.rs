//! `netsmith port skeleton` (#452): the first source of a port, written from the original's
//! `.kicad_pcb`, and the port's parts file, `reference/parts.toml` ([`crate::port::PortParts`]).
//!
//! The board file is read as it is (`Netlist::from_kicad_pcb`), with no KiCad. What is written:
//!
//! - **One member for each footprint**, named by the original's designator in lower case (`C5`
//!   is `.c5`, and a designator that is not a plain name is quoted, `.'u$1'`), after a comment
//!   with the designator, the value and the footprint (`// C5: 100nF, Capacitor_SMD:C_0402...`).
//! - **One join for each net** of two pins or more, after a comment with the original's net name,
//!   and on one pin of each net the original names itself, the name: `.u1.pa14_boot0.net &= "BOOT1"`.
//!   A net that KiCad named (`Net-(...)`, `unconnected-(...)`) gets no name line. A net of one
//!   pin is not a join; it gets its name line when it has a name of its own.
//! - **`reference/parts.toml`**: each member's path and the original's designator.
//!
//! **The standard parts it uses.** A footprint is a standard generic only when its designator,
//! its footprint and its value say so plainly; the mapping is this table and nothing else:
//!
//! | Designator | Footprint (KiCad's own names) | Value | Member |
//! |---|---|---|---|
//! | `R...` | `R_<size>_<metric>Metric...` | a resistance: `10k`, `4k7`, `R010`, `49.9`, `1Mohm` | `Resistor { .resistance &= <value> +/- 1%; .package &= "<size>" }` |
//! | `C...` | `C_<size>_<metric>Metric...` | a capacitance with a prefix: `100n`, `4.7uF`, `4u7` | `Capacitor { .capacitance &= <value> +/- 10%; ... }` |
//! | `L...` | `L_<size>_<metric>Metric...` | an inductance with a prefix: `1uH`, `470n` | `Inductor { .inductance &= <value> +/- 20%; ... }` |
//! | `D...`, `LED...` | `LED_<size>_<metric>Metric...` | `LED`, or a colour: `RED`, `GREEN`, `BLUE`, `YELLOW`, `WHITE` | `Led { .color &= Color::<Colour>; ... }` (`Color::Green`, with `Color` imported) |
//!
//! `<size>` is the imperial size (`0402`), which is the package the standard catalogs name. The
//! tolerances are the skeleton's, since a KiCad value states none: each such line ends in
//! `// TODO tolerance: the original states none`, and the command lists them, to check against
//! the original's bill of materials. A value of more than one word (`10uF 16V`), a value of zero, a
//! diode (whose value is a part number, which names no kind plainly) and every other footprint
//! are not standard parts.
//!
//! **Gaps.** Every other footprint is a member of a part kind named after its value
//! (`STM32G071C8T6` is `Stm32g071c8t6`; a value that does not start with a letter takes the
//! designator's letters, `C100nf`), imported from `src/<name>.nsm` beside the source written.
//! When that file is not there, the member is marked `GAP` and its import names the file to
//! write, so the source does not build until each has a part. The joins name a gap's pins by the
//! original board's pin names (each pad's `pinfunction`), made into paths by the rule of
//! `netsmith part from-symbol` ([`pin_path`]), so they are the pins of the part file it drafts
//! from the original's symbol (#453); a pad with no name is its quoted number (`.u1.'3'`). The
//! command guesses no pin kinds. When the file is there, its part's pin table gives the pins
//! their names; the first table that has every pad the board joins is used.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use netsmith_kicad::Sexp;
use netsmith_netlist::Netlist;

/// The tolerances the skeleton gives a standard part's value, since a KiCad value states none.
const RESISTOR_TOLERANCE: &str = "1%";
const CAPACITOR_TOLERANCE: &str = "10%";
const INDUCTOR_TOLERANCE: &str = "20%";
/// The colours the standard `led` catalog names.
const LED_COLOURS: &[&str] = &["RED", "GREEN", "BLUE", "YELLOW", "WHITE"];
/// The imperial sizes a standard part's package may be.
const SIZES: &[&str] = &[
    "01005", "0201", "0402", "0603", "0805", "1206", "1210", "1812", "2010", "2512",
];

/// What the skeleton wrote, and the counts a person (and the tests) read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skeleton {
    /// The `.nsm` source.
    pub source: String,
    /// `reference/parts.toml`.
    pub parts: String,
    /// One for each footprint of the original.
    pub members: usize,
    /// The members that are standard parts (the table of the module's docs).
    pub standard: usize,
    /// The designators of the members marked `GAP`, in the order written.
    pub gaps: Vec<String>,
    /// The lines that carry the skeleton's tolerance, marked `TODO tolerance`: `(line, the
    /// designator)`, the line counted from 1.
    pub tolerances: Vec<(usize, String)>,
    /// One for each net of two pins or more.
    pub joins: usize,
    /// The nets that got a name line (the original names them itself).
    pub named: usize,
    /// The pads of each pin the joins name: `(member, pin path)` to the original's pads.
    pub pads: BTreeMap<(String, String), Vec<String>>,
}

/// The skeleton of the board `pcb` (the text of a `.kicad_pcb`), named `board`. `file` is the
/// original's name, for the header comment, and `src` the folder the source goes in, where a
/// part file `<name>.nsm` that is there turns a gap into a member of that part.
pub fn skeleton(pcb: &str, file: &str, board: &str, src: &Path) -> Result<Skeleton, String> {
    let netlist = Netlist::from_kicad_pcb(pcb).map_err(|e| format!("{file}: {e}"))?;
    let mut order: Vec<usize> = (0..netlist.components.len()).collect();
    order.sort_by_key(|&i| designator_key(&netlist.components[i].refdes));
    let pin_names = pin_names(pcb);

    // The member of each footprint, and how each of its pads is named.
    let mut members: Vec<Member> = Vec::new();
    let mut names = BTreeSet::new();
    for &i in &order {
        let c = &netlist.components[i];
        let mut name = c.refdes.to_lowercase();
        while !names.insert(name.clone()) {
            name.push('_');
        }
        let pads: Vec<&str> = c.pins.iter().map(|(p, _)| p.as_str()).collect();
        let kind = standard_part(&c.refdes, &c.value, &c.footprint, &pads)
            .map(Kind::Standard)
            .unwrap_or_else(|| Kind::Own(own_part(&c.refdes, &c.value, &pads, src)));
        let pins = member_pins(&kind, &c.pins, pin_names.get(&c.refdes));
        members.push(Member {
            component: i,
            name,
            kind,
            pins,
        });
    }
    let member_of: BTreeMap<usize, usize> = members
        .iter()
        .enumerate()
        .map(|(m, member)| (member.component, m))
        .collect();

    let mut out = String::new();
    out.push_str(&format!(
        "//! The first source of the port of the KiCad board\n\
         //! `{file}`, written by `netsmith port skeleton`:\n\
         //! one member for each footprint of the original, named by its designator, and one join\n\
         //! for each net, after a comment with the original's net name.\n\
         //!\n\
         //! A member marked GAP has no part file yet, and its import names the file to write: the\n\
         //! source does not build until each has one. The joins name a gap's pins by the\n\
         //! original's pin names, as `netsmith part from-symbol` does, and a pin with no name by\n\
         //! its pad (`.u1.'3'`). The tolerance of each line marked `TODO tolerance` is the\n\
         //! skeleton's (resistors {RESISTOR_TOLERANCE}, capacitors {CAPACITOR_TOLERANCE}, \
         inductors {INDUCTOR_TOLERANCE}): check it against the\n\
         //! original's bill of materials. `reference/parts.toml` holds the original's designator\n\
         //! of each member.\n\n"
    ));

    // The imports: each own part's file, then the standard parts.
    let mut own_imports: BTreeMap<&str, (&str, bool)> = BTreeMap::new();
    let mut standard_kinds = BTreeSet::new();
    for m in &members {
        match &m.kind {
            Kind::Own(own) => {
                own_imports.insert(&own.ty, (&own.module, own.file.is_some()));
            }
            Kind::Standard(s) => {
                standard_kinds.insert(s.ty);
                // A colour is a variant of `Color`, written with the enum (ADR 0118).
                if s.fields.iter().any(|f| f.contains("Color::")) {
                    standard_kinds.insert("Color");
                }
            }
        }
    }
    for (ty, (module, there)) in &own_imports {
        out.push_str(&format!("import src::{module}::{ty}"));
        if !there {
            out.push_str(&format!("  // GAP: no part file, src/{module}.nsm"));
        }
        out.push('\n');
    }
    if !standard_kinds.is_empty() {
        let kinds: Vec<&str> = standard_kinds.iter().copied().collect();
        out.push_str(&if kinds.len() == 1 {
            format!("import std::parts::passive::{}\n", kinds[0])
        } else {
            format!("import std::parts::passive::{{{}}}\n", kinds.join(", "))
        });
    }
    out.push_str(&format!("\nboard {board} {{\n"));

    out.push_str("    // ---- The parts, one for each footprint of the original ----\n\n");
    let mut gaps = Vec::new();
    let mut tolerances = Vec::new();
    for m in &members {
        let c = &netlist.components[m.component];
        let value = if c.value.is_empty() {
            "no value"
        } else {
            &c.value
        };
        out.push_str(&format!(
            "    // {}: {}, {}\n",
            c.refdes,
            one_line(value),
            one_line(&c.footprint)
        ));
        match &m.kind {
            Kind::Standard(s) => {
                out.push_str(&format!(
                    "    .{}: {} {{ {} }}",
                    ident(&m.name),
                    s.ty,
                    s.fields.join("; ")
                ));
                if s.tolerance {
                    tolerances.push((out.lines().count(), c.refdes.clone()));
                    out.push_str("  // TODO tolerance: the original states none");
                }
                out.push('\n');
            }
            Kind::Own(own) => {
                match &own.file {
                    None => {
                        gaps.push(c.refdes.clone());
                        out.push_str(&format!(
                            "    // GAP: no part file; write `part {}` in src/{}.nsm\n",
                            own.ty, own.module
                        ));
                    }
                    Some(Err(why)) => {
                        out.push_str(&format!(
                            "    // {why}; the joins name its pins as the original does\n"
                        ));
                    }
                    Some(Ok(_)) => {}
                }
                out.push_str(&format!("    .{}: {}\n", ident(&m.name), own.ty));
            }
        }
    }

    out.push_str(
        "\n    // ---- The nets, one join for each, named as the original names them ----\n",
    );
    let (mut joins, mut named) = (0, 0);
    for net in &netlist.nets {
        // Pads that are one pin (a stacked pin of the original) are named once.
        let mut pins: Vec<String> = Vec::new();
        for (c, pad) in &net.pins {
            let m = &members[member_of[c]];
            let pin = format!(".{}.{}", ident(&m.name), m.pins[pad].1);
            if !pins.contains(&pin) {
                pins.push(pin);
            }
        }
        let own_name = !is_generated(&net.name);
        if pins.len() < 2 && !own_name {
            continue;
        }
        out.push_str(&format!("\n    // {}\n", one_line(&net.name)));
        if pins.len() >= 2 {
            joins += 1;
            out.push_str(&join_line(&pins));
        }
        if own_name {
            named += 1;
            out.push_str(&format!(
                "    {}.net &= \"{}\"\n",
                pins[0],
                net.name.replace('\\', "\\\\").replace('"', "\\\"")
            ));
        }
    }
    out.push_str("}\n");

    let mut parts = format!(
        "# The original's reference designator of each part of this port, by the part's member path\n\
         # (#456), written by `netsmith port skeleton` from\n\
         # `{file}`.\n\
         # The nets proof of the port's tests and `netsmith parity` read it. A member that is\n\
         # renamed in the source is renamed here.\n\n\
         [parts]\n"
    );
    for m in &members {
        parts.push_str(&format!(
            "{} = {}\n",
            toml_key(&m.name),
            toml::Value::String(netlist.components[m.component].refdes.clone())
        ));
    }

    Ok(Skeleton {
        source: out,
        parts,
        members: members.len(),
        standard: members
            .iter()
            .filter(|m| matches!(m.kind, Kind::Standard(_)))
            .count(),
        gaps,
        tolerances,
        joins,
        named,
        pads: members
            .iter()
            .flat_map(|m| {
                m.pins
                    .iter()
                    .map(|(pad, (path, _))| ((m.name.clone(), path.clone()), pad.clone()))
            })
            .fold(BTreeMap::new(), |mut map, (key, pad)| {
                map.entry(key).or_insert_with(Vec::new).push(pad);
                map
            }),
    })
}

/// `netsmith port skeleton <original> [--out <file.nsm>]`: write the skeleton of `original` to
/// `out` (default `src/main.nsm`) and its parts file to `reference/parts.toml` of the project
/// (the folder above the source's folder). Neither file may exist yet: the command never
/// overwrites a port's work. The board is named after the original's file.
pub fn write(original: &Path, out: Option<&Path>) -> Result<(Skeleton, PathBuf), String> {
    let out = out.map_or_else(|| PathBuf::from("src/main.nsm"), Path::to_path_buf);
    // An empty path is the current folder, and joins without a `./`.
    let src = out.parent().map_or_else(PathBuf::new, Path::to_path_buf);
    let project = src.parent().map_or_else(PathBuf::new, Path::to_path_buf);
    let parts_path = project.join("reference").join("parts.toml");
    for path in [&out, &parts_path] {
        if path.exists() {
            return Err(format!(
                "{} is there already; `port skeleton` writes the first source of a port and \
                 overwrites nothing (move it away, or give --out another file)",
                path.display()
            ));
        }
    }
    let file = original.display().to_string();
    let text = std::fs::read_to_string(original).map_err(|e| format!("{file}: {e}"))?;
    let name = original
        .file_name()
        .map_or(file.clone(), |n| n.to_string_lossy().into_owned());
    let stem = original
        .file_stem()
        .map_or(String::new(), |s| s.to_string_lossy().into_owned());
    let skeleton = skeleton(&text, &name, &type_name(&stem, "Board"), &src)?;
    for (path, text) in [(&out, &skeleton.source), (&parts_path, &skeleton.parts)] {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok((skeleton, parts_path))
}

struct Member {
    /// Its index in the netlist's components.
    component: usize,
    name: String,
    kind: Kind,
    /// [`member_pins`].
    pins: BTreeMap<String, (String, String)>,
}

enum Kind {
    Standard(Standard),
    Own(Own),
}

/// The pin on each pad of a member that the board joins: `(path, as source writes it)`. A
/// standard part's pins are its generic's; a part whose file is there, its pin table's; a gap's,
/// the original's pin names ([`pin_path`]), and a pad with no name is its quoted number (`'3'`,
/// `'A1'`, so that it never reads as a pin's name). Two pads whose names make one path get `_2`,
/// `_3`... in pad order, as `netsmith part from-symbol` does, unless they have one name and are
/// on one net: then they are one pin, as a stacked pin of the symbol is.
fn member_pins(
    kind: &Kind,
    pads: &[(String, usize)],
    names: Option<&BTreeMap<String, String>>,
) -> BTreeMap<String, (String, String)> {
    let source = |path: &str| path.split('.').map(ident).collect::<Vec<_>>().join(".");
    let quoted = |pad: &str| (pad.to_string(), format!("'{pad}'"));
    let mut pins = BTreeMap::new();
    let mut ordered: Vec<&(String, usize)> = pads.iter().collect();
    ordered.sort_by_key(|(pad, _)| pad_order(pad));
    // The pads given each path from a name so far: `(name, net, path)`.
    let mut by_path: BTreeMap<String, Vec<(&str, usize, String)>> = BTreeMap::new();
    for (pad, net) in ordered {
        let pin = match kind {
            Kind::Standard(s) => s
                .pins
                .iter()
                .find(|(p, _)| p == pad)
                .map(|p| (p.1.to_string(), p.1.to_string())),
            Kind::Own(Own {
                file: Some(Ok(table)),
                ..
            }) => table.get(pad).map(|path| (path.clone(), source(path))),
            Kind::Own(_) => names
                .and_then(|n| n.get(pad))
                .filter(|name| !name.is_empty() && name.as_str() != "~")
                .map(|name| {
                    let base = pin_path(name);
                    let taken = by_path.entry(base.clone()).or_default();
                    let path = match taken.iter().find(|(n, m, _)| n == name && m == net) {
                        Some((_, _, path)) => path.clone(),
                        None => {
                            let paths: BTreeSet<&String> = taken.iter().map(|t| &t.2).collect();
                            match paths.len() {
                                0 => base,
                                n => format!("{base}_{}", n + 1),
                            }
                        }
                    };
                    taken.push((name, *net, path.clone()));
                    (path.clone(), source(&path))
                }),
        };
        pins.insert(pad.clone(), pin.unwrap_or_else(|| quoted(pad)));
    }
    pins
}

/// A member path from a pin's name on the original board (`pinfunction`), made as
/// `netsmith part from-symbol` makes a pin's path from the symbol (`path_of` and `printed` in
/// `part_draft.rs`, #453), so that the joins name the pins of the part file it drafts. The two
/// must stay the same rule: lower case, a `_` between words, never starting with a digit (`3V3`
/// is `p3v3`), a trailing `+` or `-` is `_pos` or `_neg` (`INA-` is `ina_neg`), and an overbar
/// (`~{RESET}`) ends it in `_n`.
fn pin_path(name: &str) -> String {
    let low = name.contains("~{");
    let name = name.replace("~{", "").replace('}', "");
    let (name, sign) = match name.strip_suffix('+') {
        Some(rest) => (rest, "_pos"),
        None => match name.strip_suffix('-') {
            Some(rest) => (rest, "_neg"),
            None => (name.as_str(), ""),
        },
    };
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    let mut out = out.trim_end_matches('_').to_string();
    if out.is_empty() {
        out = "pin".to_string();
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, 'p');
    }
    out.push_str(sign);
    if low {
        out.push_str("_n");
    }
    out
}

/// Pads in the order of their numbers: `2` before `10`, names after numbers.
fn pad_order(pad: &str) -> (u64, String) {
    (pad.parse().unwrap_or(u64::MAX), pad.to_string())
}

/// The name of each pad of each footprint on the board (`(pinfunction "PA14-BOOT0")`), by
/// designator, read as `Netlist::from_kicad_pcb` reads the designators.
fn pin_names(pcb: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let Ok(root) = Sexp::parse(pcb) else {
        return out;
    };
    for fp in root.find_all("footprint") {
        let mut reference = None;
        for p in fp.find_all("fp_text").chain(fp.find_all("property")) {
            let text = |i: usize| p.items().get(i).and_then(Sexp::text).unwrap_or("");
            if matches!(text(1), "reference" | "Reference") {
                reference = Some(text(2).to_string());
            }
        }
        let Some(reference) = reference else {
            continue;
        };
        let names: &mut BTreeMap<String, String> = out.entry(reference).or_default();
        for pad in fp.find_all("pad") {
            let number = pad.items().get(1).and_then(Sexp::text).unwrap_or("");
            let name = pad
                .find("pinfunction")
                .and_then(|f| f.items().get(1))
                .and_then(Sexp::text);
            if let (false, Some(name)) = (number.is_empty(), name) {
                names.entry(number.to_string()).or_insert(name.to_string());
            }
        }
    }
    out
}

/// A standard generic, its fields, and the pin on each pad.
struct Standard {
    ty: &'static str,
    fields: Vec<String>,
    /// Its value carries the skeleton's tolerance, since the original states none.
    tolerance: bool,
    pins: &'static [(&'static str, &'static str)],
}

/// A part kind of the project's own, named after the value.
struct Own {
    ty: String,
    module: String,
    /// `None` when `src/<module>.nsm` is not there (a gap); else the pin on each pad, or why its
    /// pin table does not give them.
    file: Option<Result<BTreeMap<String, String>, String>>,
}

const TWO_PADS: &[(&str, &str)] = &[("1", "a"), ("2", "b")];
const LED_PADS: &[(&str, &str)] = &[("1", "cathode"), ("2", "anode")];

/// The standard generic the module's table makes of this footprint, if any.
fn standard_part(refdes: &str, value: &str, footprint: &str, pads: &[&str]) -> Option<Standard> {
    if pads.iter().any(|p| *p != "1" && *p != "2") {
        return None;
    }
    let letters: String = refdes
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect();
    let footprint = footprint.rsplit(':').next().unwrap_or(footprint);
    let size = |prefix: &str| -> Option<String> {
        let rest = footprint.strip_prefix(prefix)?.strip_prefix('_')?;
        let (size, rest) = rest.split_once('_')?;
        let metric = rest.split('_').next()?;
        (SIZES.contains(&size) && metric.ends_with("Metric")).then(|| size.to_string())
    };
    let value = value.trim();
    match letters.as_str() {
        "R" => {
            let package = size("R")?;
            let ohms = parse_value(value, &["", "ohm", "ohms", "\u{3a9}", "r"], true)?;
            Some(Standard {
                ty: "Resistor",
                fields: vec![
                    format!(
                        ".resistance &= {} +/- {RESISTOR_TOLERANCE}",
                        render(ohms, "ohm")
                    ),
                    format!(".package &= \"{package}\""),
                ],
                pins: TWO_PADS,
                tolerance: true,
            })
        }
        "C" => {
            let package = size("C")?;
            let farads = parse_value(value, &["", "f"], false)?;
            Some(Standard {
                ty: "Capacitor",
                fields: vec![
                    format!(
                        ".capacitance &= {} +/- {CAPACITOR_TOLERANCE}",
                        render(farads, "F")
                    ),
                    format!(".package &= \"{package}\""),
                ],
                pins: TWO_PADS,
                tolerance: true,
            })
        }
        "L" => {
            let package = size("L")?;
            let henries = parse_value(value, &["", "h"], false)?;
            Some(Standard {
                ty: "Inductor",
                fields: vec![
                    format!(
                        ".inductance &= {} +/- {INDUCTOR_TOLERANCE}",
                        render(henries, "H")
                    ),
                    format!(".package &= \"{package}\""),
                ],
                pins: TWO_PADS,
                tolerance: true,
            })
        }
        "D" | "LED" => {
            let package = size("LED")?;
            let colour = value.to_uppercase();
            let mut fields = Vec::new();
            if LED_COLOURS.contains(&colour.as_str()) {
                // The variant of `Color`: the word with its first letter capital
                // (`Color::Green`), and the file imports `Color`.
                let (first, rest) = colour.split_at(1);
                fields.push(format!(".color &= Color::{first}{}", rest.to_lowercase()));
            } else if colour != "LED" {
                return None;
            }
            fields.push(format!(".package &= \"{package}\""));
            Some(Standard {
                ty: "Led",
                fields,
                pins: LED_PADS,
                tolerance: false,
            })
        }
        _ => None,
    }
}

/// The part kind named after `value`, and whether `src` has its file.
fn own_part(refdes: &str, value: &str, pads: &[&str], src: &Path) -> Own {
    let letters: String = refdes
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect();
    let ty = if value.trim().starts_with(|c: char| c.is_ascii_alphabetic()) {
        type_name(value, "Part")
    } else if value.trim().is_empty() {
        type_name(&letters, "Part")
    } else {
        type_name(&format!("{letters}{value}"), "Part")
    };
    let module = ty.to_lowercase();
    let path = src.join(format!("{module}.nsm"));
    let file = path.is_file().then(|| pins_of(&path, &ty, pads));
    Own { ty, module, file }
}

/// The pin on each of `pads` in the part `ty` of the file at `path`: the first pin table that has
/// every one of them.
fn pins_of(path: &Path, ty: &str, pads: &[&str]) -> Result<BTreeMap<String, String>, String> {
    let shown = path.display();
    let text = std::fs::read_to_string(path).map_err(|e| format!("{shown}: {e}"))?;
    let file = netsmith_lang::parse(&text).map_err(|_| format!("{shown} does not parse"))?;
    let part = file
        .definitions
        .iter()
        .find(|d| d.kind == netsmith_lang::ast::DefKind::Part && d.name.name == ty)
        .ok_or_else(|| format!("{shown} has no `part {ty}`"))?;
    for item in &part.items {
        let netsmith_lang::ast::Item::Pins(table) = item else {
            continue;
        };
        let mut by_pad = BTreeMap::new();
        for row in &table.rows {
            let pin: Vec<&str> = row.pin.segments.iter().map(|s| s.name.as_str()).collect();
            for pad in &row.pads {
                by_pad.insert(pad.text.clone(), pin.join("."));
            }
        }
        if pads.iter().all(|p| by_pad.contains_key(*p)) {
            return Ok(by_pad);
        }
    }
    Err(format!(
        "no pin table of `{ty}` in {shown} has every pad the board joins"
    ))
}

/// `value` as a number of its unit, when it is one word: digits with an optional SI prefix
/// letter, which may stand for the decimal point (`4k7`; `R` too for a resistor, `4R7`,
/// `R010`), then one of `units` (lower case). `bare` allows a number with no prefix (`100` is
/// 100 ohm; a capacitor's `100` says nothing plainly). Zero is not a value.
fn parse_value(value: &str, units: &[&str], bare: bool) -> Option<f64> {
    if value.is_empty() || value.contains(char::is_whitespace) {
        return None;
    }
    let digits = |s: &str| s.chars().take_while(char::is_ascii_digit).count();
    // `R010` is 0.010 ohm.
    let value = match value.strip_prefix(['R', 'r']) {
        Some(rest) if bare && digits(rest) > 0 => format!("0R{rest}"),
        _ => value.to_string(),
    };
    let int = digits(&value);
    let mut number = value[..int].to_string();
    let mut rest = &value[int..];
    if let Some(after) = rest.strip_prefix('.') {
        let frac = digits(after);
        number.push('.');
        number.push_str(&after[..frac]);
        rest = &after[frac..];
    }
    if number.is_empty() || number == "." {
        return None;
    }
    let mut scale = None;
    if let Some(c) = rest.chars().next() {
        scale = match c {
            'p' => Some(1e-12),
            'n' => Some(1e-9),
            'u' | '\u{b5}' | '\u{3bc}' => Some(1e-6),
            'm' => Some(1e-3),
            'k' | 'K' => Some(1e3),
            'M' => Some(1e6),
            'R' | 'r' if bare => Some(1.0),
            _ => None,
        };
        if scale.is_some() {
            rest = &rest[c.len_utf8()..];
            let frac = digits(rest);
            if frac > 0 {
                if number.contains('.') {
                    return None;
                }
                number.push('.');
                number.push_str(&rest[..frac]);
                rest = &rest[frac..];
            }
        }
    }
    if scale.is_none() && !bare {
        return None;
    }
    if !units.contains(&rest.to_lowercase().as_str()) {
        return None;
    }
    let n = number.parse::<f64>().ok()? * scale.unwrap_or(1.0);
    (n > 0.0 && n.is_finite()).then_some(n)
}

/// `n` of `unit` with the SI prefix that puts it between 1 and 1000: `4.7kohm`, `100nF`.
fn render(n: f64, unit: &str) -> String {
    let prefixes = [
        (1e-12, "p"),
        (1e-9, "n"),
        (1e-6, "u"),
        (1e-3, "m"),
        (1.0, ""),
        (1e3, "k"),
        (1e6, "M"),
        (1e9, "G"),
    ];
    let (scale, prefix) = prefixes
        .iter()
        .rev()
        .find(|(s, _)| n >= s * (1.0 - 1e-9))
        .copied()
        .unwrap_or(prefixes[0]);
    // Twelve significant digits, so that 0.007 / 1e-3 is 7 and not 7.000000000000001.
    let mantissa = n / scale;
    let rounded: f64 = format!("{mantissa:.9}").parse().unwrap_or(mantissa);
    format!("{rounded}{prefix}{unit}")
}

/// A type name made of `text`: its words, each with a capital first letter (`charger-module`
/// is `ChargerModule`, `STM32G071C8T6` is `Stm32g071c8t6`), and `fallback` before it when it
/// does not start with a letter.
fn type_name(text: &str, fallback: &str) -> String {
    let mut name = String::new();
    for word in text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            name.push(first.to_ascii_uppercase());
            name.extend(chars.map(|c| c.to_ascii_lowercase()));
        }
    }
    if !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
        name.insert_str(0, fallback);
    }
    name
}

/// A name as source writes it: plain, or quoted when it is not a plain identifier (`'3'`).
fn ident(name: &str) -> String {
    let plain = name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if plain {
        name.to_string()
    } else {
        format!("'{name}'")
    }
}

/// A key of the parts file: bare when TOML allows it, else quoted.
fn toml_key(name: &str) -> String {
    if !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        name.to_string()
    } else {
        toml::Value::String(name.to_string()).to_string()
    }
}

/// A net name KiCad made up for a net the original does not name.
fn is_generated(name: &str) -> bool {
    name.starts_with("Net-(") || name.starts_with("unconnected-(") || name.is_empty()
}

/// Text for a `//` comment, on one line.
fn one_line(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
}

/// `.a ~ .b ~ .c`, broken after a `~` when it grows long.
fn join_line(pins: &[String]) -> String {
    let mut out = String::from("    ");
    let mut width = 4;
    for (i, pin) in pins.iter().enumerate() {
        if i > 0 {
            if width + pin.len() + 3 > 100 {
                out.push_str(" ~\n        ");
                width = 8;
            } else {
                out.push_str(" ~ ");
                width += 3;
            }
        }
        out.push_str(pin);
        width += pin.len();
    }
    out.push('\n');
    out
}

/// Designators in the order a person reads them: `C2` before `C10`.
fn designator_key(refdes: &str) -> (String, u64, String) {
    let letters: String = refdes.chars().take_while(|c| !c.is_ascii_digit()).collect();
    let rest = &refdes[letters.len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    (
        letters,
        digits.parse().unwrap_or(0),
        rest[digits.len()..].to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_read_only_when_they_are_plain() {
        let ohm = &["", "ohm", "ohms", "\u{3a9}", "r"];
        assert_eq!(
            parse_value("4k7", ohm, true).map(|v| render(v, "ohm")),
            Some("4.7kohm".into())
        );
        assert_eq!(
            parse_value("R010", ohm, true).map(|v| render(v, "ohm")),
            Some("10mohm".into())
        );
        assert_eq!(
            parse_value("100K", ohm, true).map(|v| render(v, "ohm")),
            Some("100kohm".into())
        );
        assert_eq!(
            parse_value("49.9", ohm, true).map(|v| render(v, "ohm")),
            Some("49.9ohm".into())
        );
        assert_eq!(
            parse_value("4R7", ohm, true).map(|v| render(v, "ohm")),
            Some("4.7ohm".into())
        );
        assert_eq!(parse_value("0R", ohm, true), None);
        let f = &["", "f"];
        assert_eq!(
            parse_value("100nF", f, false).map(|v| render(v, "F")),
            Some("100nF".into())
        );
        assert_eq!(
            parse_value("4u7", f, false).map(|v| render(v, "F")),
            Some("4.7uF".into())
        );
        assert_eq!(
            parse_value("30p", f, false).map(|v| render(v, "F")),
            Some("30pF".into())
        );
        assert_eq!(parse_value("100", f, false), None);
        assert_eq!(parse_value("10uF 16V", f, false), None);
        assert_eq!(parse_value("BLM18KG601SN1D", &["", "h"], false), None);
    }

    #[test]
    fn only_the_tables_footprints_are_standard_parts() {
        let r = standard_part("R1", "10k", "Resistor_SMD:R_0402_1005Metric", &["1", "2"]).unwrap();
        assert_eq!(r.ty, "Resistor");
        assert_eq!(
            r.fields,
            [".resistance &= 10kohm +/- 1%", ".package &= \"0402\""]
        );
        let led = standard_part("D1", "GREEN", "LED_SMD:LED_0603_1608Metric", &["1"]).unwrap();
        assert_eq!(
            led.fields,
            [".color &= Color::Green", ".package &= \"0603\""]
        );
        for (refdes, value, footprint) in [
            ("R1", "10k", "RPI_COMPILED:generic_resc1005x35"),
            ("C1", "100n", "Capacitor_SMD:CP_Elec_8x10"),
            ("FB1", "BLM18KG601SN1D", "Inductor_SMD:L_0603_1608Metric"),
            ("D2", "D_Schottky", "Diode_SMD:D_0402_1005Metric"),
            ("U1", "10k", "Resistor_SMD:R_0402_1005Metric"),
        ] {
            assert!(
                standard_part(refdes, value, footprint, &["1", "2"]).is_none(),
                "{refdes}"
            );
        }
        assert!(standard_part("R1", "10k", "Resistor_SMD:R_0402_1005Metric", &["3"]).is_none());
    }

    #[test]
    fn a_pin_path_is_made_as_part_from_symbol_makes_it() {
        // The cases of `part_draft.rs`'s own test, which the rule must keep matching.
        assert_eq!(pin_path("PA9"), "pa9");
        assert_eq!(pin_path("VDD/VDDA"), "vdd_vdda");
        assert_eq!(pin_path("3V3"), "p3v3");
        assert_eq!(pin_path("~{RESET}"), "reset_n");
        assert_eq!(pin_path("INA+"), "ina_pos");
        assert_eq!(pin_path("INA-"), "ina_neg");
        assert_eq!(pin_path("PA14-BOOT0"), "pa14_boot0");
    }

    #[test]
    fn names_are_plain_or_quoted() {
        assert_eq!(type_name("charger-module", "Board"), "ChargerModule");
        assert_eq!(type_name("STM32G071C8T6", "Part"), "Stm32g071c8t6");
        assert_eq!(type_name("100nF", "Part"), "Part100nf");
        assert_eq!(ident("u1"), "u1");
        assert_eq!(ident("3"), "'3'");
        assert_eq!(ident("u$1"), "'u$1'");
        assert!(designator_key("C2") < designator_key("C10"));
    }
}
