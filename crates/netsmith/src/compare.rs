//! `netsmith compare`: whether two designs are the same circuit (ADR 0017).
//!
//! Each side is a `.kicad_sch` (exported to a netlist by `kicad-cli`), a `.kicad_pcb`, the
//! XML of a KiCad netlist, or a `.nsm` file, whose board is built as `netsmith build`
//! does. The comparison itself is in `netsmith-netlist`.

use std::fs;
use std::path::Path;

use netsmith_core::Design;
use netsmith_fab::KicadCli;
use netsmith_kicad::Mapping;
use netsmith_netlist::{Identity, Netlist, Options, Outcome, compare, report};

use crate::project::Target;
use crate::{outcome_with, render_problems};

/// What the command line asked for.
#[derive(Debug, Clone, Default)]
pub struct Request<'a> {
    pub a: &'a str,
    pub b: &'a str,
    pub board: Option<&'a str>,
    pub map: Option<&'a Path>,
    pub by_refdes: bool,
    pub ignore_unconnected: bool,
    pub loose_values: bool,
}

/// The answer, and what to tell the person.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Match,
    Different,
    /// The search was cut off before it knew.
    Undecided,
}

#[derive(Debug)]
pub struct Compared {
    pub verdict: Verdict,
    pub text: String,
    /// The same, as one JSON document for `--format json` (#455).
    pub json: String,
}

/// Read both sides and compare them. `kicad` is needed only for a `.kicad_sch`. The error
/// is a problem with the request or the files, not a difference.
pub fn compare_files(request: &Request, kicad: Option<&dyn KicadCli>) -> Result<Compared, String> {
    let a = load(request.a, request.board, kicad)?;
    let b = load(request.b, request.board, kicad)?;
    let identity = match (request.map, request.by_refdes) {
        (Some(map), _) => {
            let text = fs::read_to_string(map).map_err(|e| format!("{}: {e}", map.display()))?;
            Identity::Map(read_map(&text).map_err(|e| format!("{}: {e}", map.display()))?)
        }
        (None, true) => Identity::ByRefdes,
        (None, false) => Identity::Free,
    };
    let options = Options {
        identity,
        ignore_unconnected: request.ignore_unconnected,
        loose_values: request.loose_values,
        ..Options::default()
    };
    let outcome = compare(&a, &b, &options)?;
    let size = |name: &str, path: &str, n: &Netlist| {
        format!(
            "{name}: {path} ({} components, {} nets)\n",
            n.components.len(),
            n.nets.len()
        )
    };
    let text = format!(
        "{}{}{}",
        size("A", request.a, &a),
        size("B", request.b, &b),
        report(&outcome)
    );
    let verdict = match outcome {
        Outcome::Match(_) => Verdict::Match,
        Outcome::Different(_) => Verdict::Different,
        Outcome::Undecided(_) => Verdict::Undecided,
    };
    let side = |path: &str, n: &Netlist| serde_json::json!({ "path": path, "components": n.components.len(), "nets": n.nets.len() });
    let json = outcome_json(&outcome, side(request.a, &a), side(request.b, &b));
    Ok(Compared {
        verdict,
        text,
        json,
    })
}

/// The JSON document of a comparison: the two sides, the verdict, and the pairs of a match or
/// each difference, by kind.
fn outcome_json(outcome: &Outcome, a: serde_json::Value, b: serde_json::Value) -> String {
    use netsmith_netlist::Difference;
    use serde_json::json;
    let pin = |p: &netsmith_netlist::Pin| format!("{}.{}", p.refdes, p.pin);
    let mut doc = json!({ "command": "compare", "a": a, "b": b });
    match outcome {
        Outcome::Match(c) => {
            doc["verdict"] = json!("match");
            doc["components"] = json!(c.components);
            doc["nets"] = json!(c.nets);
            doc["interchangeable"] = json!(c.interchangeable);
        }
        Outcome::Undecided(why) => {
            doc["verdict"] = json!("undecided");
            doc["why"] = json!(why);
        }
        Outcome::Different(differences) => {
            doc["verdict"] = json!("different");
            let list: Vec<serde_json::Value> = differences
                .iter()
                .map(|d| match d {
                    Difference::ComponentOnly {
                        side,
                        refdes,
                        value,
                        footprint,
                        pins,
                    } => json!({
                        "kind": "component_only", "side": side.to_string(), "refdes": refdes,
                        "value": value, "footprint": footprint, "pins": pins,
                    }),
                    Difference::ComponentClass { a, b } => json!({
                        "kind": "component_class",
                        "a": { "refdes": a.0, "value": a.1, "footprint": a.2 },
                        "b": { "refdes": b.0, "value": b.1, "footprint": b.2 },
                    }),
                    Difference::NetPins {
                        a,
                        b,
                        only_a,
                        only_b,
                    } => json!({
                        "kind": "net_pins", "a": a, "b": b,
                        "only_a": only_a.iter().map(|(p, w)| json!([pin(p), w])).collect::<Vec<_>>(),
                        "only_b": only_b.iter().map(|(p, w)| json!([pin(p), w])).collect::<Vec<_>>(),
                    }),
                    Difference::NetOnly { side, name, pins } => json!({
                        "kind": "net_only", "side": side.to_string(), "name": name,
                        "pins": pins.iter().map(pin).collect::<Vec<_>>(),
                    }),
                    Difference::Unlocated(why) => json!({ "kind": "unlocated", "why": why }),
                })
                .collect();
            doc["differences"] = json!(list);
        }
    }
    let mut text = serde_json::to_string_pretty(&doc).expect("plain data");
    text.push('\n');
    text
}

/// The netlist of one argument, by its extension.
pub fn load(
    path: &str,
    board: Option<&str>,
    kicad: Option<&dyn KicadCli>,
) -> Result<Netlist, String> {
    let read = || fs::read_to_string(path).map_err(|e| format!("{path}: {e}"));
    let extension = Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase());
    match extension.as_deref() {
        Some("kicad_sch") => {
            if !Path::new(path).is_file() {
                return Err(format!("{path}: no such file"));
            }
            let kicad = kicad.ok_or_else(|| {
                match crate::fab::kicad_switched_off(&netsmith_kicad::install::Search::machine()) {
                    Some(off) => format!(
                        "{path}: reading a schematic needs KiCad's command-line tool, and {off}"
                    ),
                    None => format!(
                        "{path}: reading a schematic needs KiCad's command-line tool: set KICAD_CLI to its path"
                    ),
                }
            })?;
            Netlist::from_kicad_schematic(Path::new(path), kicad)
        }
        Some("kicad_pcb") => Netlist::from_kicad_pcb(&read()?).map_err(|e| format!("{path}: {e}")),
        Some("xml") => Netlist::from_kicad_xml(&read()?).map_err(|e| format!("{path}: {e}")),
        Some("nsm") => {
            let (design, mapping) = design_of(path, board)?;
            netlist_of_design(&design, &mapping).map_err(|e| format!("{path}: {e}"))
        }
        _ => Err(format!(
            "{path}: cannot tell what this is; compare takes a .kicad_sch, a .kicad_pcb, a KiCad netlist (.xml) or a .nsm file"
        )),
    }
}

/// Check, solve and design a `.nsm` file and pick its board: the one named, or the only one. The mapping is
/// the project's, for the footprint names.
fn design_of(path: &str, board: Option<&str>) -> Result<(Design, Mapping), String> {
    let target = Target::resolve(Some(path))?;
    let loader = target.loader()?;
    let catalogs = target.catalogs()?;
    let text = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let built = outcome_with(path, &text, &loader, &catalogs, None)
        .map_err(|problems| render_problems(&problems))?
        .boards;
    let mapping = Mapping::for_project(target.project.as_ref().map(|p| p.root.as_path()))
        .map_err(|e| e.to_string())?;
    let labels = || {
        built
            .iter()
            .map(|b| b.label())
            .collect::<Vec<_>>()
            .join(", ")
    };
    match board {
        Some(name) => built
            .iter()
            .position(|b| b.board == name || b.label() == name || b.design.name == name)
            .map(|i| (built[i].design.clone(), mapping))
            .ok_or_else(|| format!("{path} has no board `{name}`; its boards are {}", labels())),
        None if built.len() == 1 => Ok((built[0].design.clone(), mapping)),
        None => Err(format!(
            "{path} has {} boards ({}); name one with --board",
            built.len(),
            labels()
        )),
    }
}

/// The netlist of a built design, with the footprints the board would use.
pub fn netlist_of_design(design: &Design, mapping: &Mapping) -> Result<Netlist, String> {
    let mut components = Vec::new();
    // Silkscreen text is not in the circuit.
    for p in design.parts.iter().filter(|p| p.kind != "Label") {
        let mapped = mapping
            .map(&p.kind, p.package.as_deref())
            .map_err(|e| format!("{}: {e}", p.refdes))?;
        components.push((p.refdes.clone(), p.value.clone(), mapped.footprint));
    }
    let nets = design
        .nets
        .iter()
        .map(|n| {
            (
                n.name.clone(),
                n.pins
                    .iter()
                    .map(|p| (p.refdes.clone(), p.pin.clone()))
                    .collect(),
            )
        })
        .collect();
    Netlist::from_parts(components, nets).map_err(|e| e.to_string())
}

/// The pairs of a `--map` file: one `A_ref B_ref` per line; blank lines and `#` comments
/// are ignored.
pub fn read_map(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut pairs = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [a, b] = fields[..] else {
            return Err(format!(
                "line {}: expected two designators, `A_ref B_ref`, not `{line}`",
                i + 1
            ));
        };
        pairs.push((a.to_string(), b.to_string()));
    }
    Ok(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_file_has_a_pair_per_line() {
        let pairs = read_map("# comment\nR1 R7\n\nC2   C9 # trailing\n").unwrap();
        assert_eq!(
            pairs,
            [("R1".into(), "R7".into()), ("C2".into(), "C9".into())]
        );
        let e = read_map("R1 R2\nR3\n").unwrap_err();
        assert!(e.contains("line 2") && e.contains("`R3`"), "{e}");
        let e = read_map("R1 R2 R3\n").unwrap_err();
        assert!(e.contains("line 1"), "{e}");
    }

    #[test]
    fn an_unknown_kind_of_file_says_what_is_taken() {
        let e = load("design.net", None, None).unwrap_err();
        assert!(
            e.contains("design.net") && e.contains(".kicad_sch") && e.contains(".nsm"),
            "{e}"
        );
    }

    #[test]
    fn a_schematic_without_kicad_says_how_to_find_it() {
        let dir = netsmith_kicad::testlib::TempDir::new("compare");
        let sch = dir.join("x.kicad_sch");
        fs::write(&sch, "(kicad_sch)").unwrap();
        let e = load(sch.to_str().unwrap(), None, None).unwrap_err();
        assert!(e.contains("KICAD_CLI"), "{e}");
    }

    #[test]
    fn a_missing_file_is_named() {
        let e = load("/nonexistent/x.xml", None, None).unwrap_err();
        assert!(e.contains("/nonexistent/x.xml"), "{e}");
    }
}
