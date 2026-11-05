//! Measure the schematic of the boards of a source file or a project, as the editor draws
//! it, and print one line per sheet: bends, crossings, wire length, satellite distances,
//! overlaps, shape (`netsmith_view::metrics`), and the count of rules the drawing breaks
//! (`netsmith_view::check`).
//!
//! `cargo run -p netsmith --example schematic_metrics -- <file.nsm | project folder>
//! [--plain] [--violations] [--save-inputs <file>] [--svg <file>]`. `--plain` prints every
//! number as `sheet.name = value`; `--violations` lists each broken rule. The exit status is 0
//! even when a rule is broken: this reports.
//!
//! A big project takes most of a minute to analyse, and the drawing a fraction of a second.
//! `--save-inputs <file>` writes what the drawing is made from (the design, its rails, its
//! must-connect pins, its module groups and the project folder the symbols come from) as JSON,
//! and `schematic_metrics --inputs <file>` draws from that file alone, so a change to the
//! drawing can be measured at once. `--svg <file>` writes the drawing as the editor serves it (a file of several boards
//! writes `<file>-<board>.svg` for each).

use std::{env, fs, path::PathBuf, process::ExitCode};

use netsmith::editor::{module_groups, must_connect, rails_of};
use netsmith::project::Target;
use netsmith::{Libraries, outcome_with};
use netsmith_core::{
    Design, InterfaceInstance, InterfaceMember, Net, Part, PartPin, PinKind, PinRef,
};
use netsmith_view::check::check;
use netsmith_view::metrics::measure;
use netsmith_view::{Group, Marks, Rails, layout_sheets_marked, render_svg_by_module_marked};
use serde_json::{Value, json};

/// Everything the drawing of one board is made from.
struct Inputs {
    root: Option<PathBuf>,
    design: Design,
    groups: Vec<Group>,
    rails: Rails,
    marks: Marks,
}

fn pin_kind(word: &str) -> PinKind {
    [
        PinKind::Power,
        PinKind::Input,
        PinKind::Output,
        PinKind::Bidirectional,
        PinKind::Passive,
        PinKind::NoConnect,
    ]
    .into_iter()
    .find(|k| k.word() == word)
    .unwrap_or(PinKind::Passive)
}

fn to_json(i: &Inputs) -> Value {
    json!({
        "root": i.root.as_ref().map(|r| r.to_string_lossy().into_owned()),
        "name": i.design.name,
        "parts": i.design.parts.iter().map(|p| json!({
            "refdes": p.refdes, "kind": p.kind, "value": p.value, "package": p.package,
            "path": p.path,
            "pins": p.pins.iter().map(|q| json!({
                "name": q.name, "datasheet": q.datasheet, "kind": q.kind.word(),
                "pads": q.pads, "function": q.function,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "nets": i.design.nets.iter().map(|n| json!({
            "name": n.name,
            "pins": n.pins.iter().map(|p| json!([p.refdes, p.pin])).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "interfaces": i.design.interfaces.iter().map(|f| json!({
            "path": f.path, "type": f.type_name,
            "members": f.members.iter().map(|m| json!([m.name, m.net])).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "positive": i.rails.positive,
        "ground": i.rails.ground,
        "must": i.marks.must.iter().map(|((r, p), m)| json!([r, p, m])).collect::<Vec<_>>(),
        "groups": i.groups.iter().map(|g| json!({
            "name": g.name, "detail": g.detail, "parts": g.parts,
        })).collect::<Vec<_>>(),
    })
}

fn from_json(v: &Value) -> Inputs {
    let s = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let list = |v: &Value| v.as_array().cloned().unwrap_or_default();
    let strs = |v: &Value| list(v).iter().map(s).collect::<Vec<_>>();
    let parts = list(&v["parts"])
        .iter()
        .map(|p| Part {
            refdes: s(&p["refdes"]),
            kind: s(&p["kind"]),
            value: s(&p["value"]),
            package: p["package"].as_str().map(String::from),
            path: s(&p["path"]),
            pins: list(&p["pins"])
                .iter()
                .map(|q| PartPin {
                    name: s(&q["name"]),
                    datasheet: s(&q["datasheet"]),
                    kind: pin_kind(&s(&q["kind"])),
                    pads: strs(&q["pads"]),
                    function: q["function"].as_str().map(String::from),
                })
                .collect(),
        })
        .collect();
    let nets = list(&v["nets"])
        .iter()
        .map(|n| Net {
            name: s(&n["name"]),
            pins: list(&n["pins"])
                .iter()
                .map(|p| PinRef {
                    refdes: s(&p[0]),
                    pin: s(&p[1]),
                })
                .collect(),
        })
        .collect();
    let mut rails = Rails::default();
    rails.positive.extend(strs(&v["positive"]));
    rails.ground.extend(strs(&v["ground"]));
    let mut marks = Marks::default();
    for m in list(&v["must"]) {
        marks.must.insert((s(&m[0]), s(&m[1])), s(&m[2]));
    }
    Inputs {
        root: v["root"].as_str().map(PathBuf::from),
        design: Design {
            name: s(&v["name"]),
            parts,
            nets,
            process: None,
            features: Default::default(),
            interfaces: list(&v["interfaces"])
                .iter()
                .map(|f| InterfaceInstance {
                    path: s(&f["path"]),
                    type_name: s(&f["type"]),
                    members: list(&f["members"])
                        .iter()
                        .map(|m| InterfaceMember {
                            name: s(&m[0]),
                            net: s(&m[1]),
                        })
                        .collect(),
                })
                .collect(),
        },
        groups: list(&v["groups"])
            .iter()
            .map(|g| Group {
                name: s(&g["name"]),
                detail: s(&g["detail"]),
                parts: list(&g["parts"])
                    .iter()
                    .filter_map(|x| x.as_u64().map(|x| x as usize))
                    .collect(),
            })
            .collect(),
        rails,
        marks,
    }
}

fn main() -> ExitCode {
    let mut path: Option<String> = None;
    let (mut plain, mut violations) = (false, false);
    let (mut save, mut load, mut svg): (Option<String>, Option<String>, Option<String>) =
        (None, None, None);
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--plain" => plain = true,
            "--violations" => violations = true,
            "--save-inputs" => save = args.next(),
            "--inputs" => load = args.next(),
            "--svg" => svg = args.next(),
            other => path = Some(other.to_string()),
        }
    }
    let fail = |e: String| {
        eprintln!("{e}");
        ExitCode::from(2)
    };
    let mut boards: Vec<(String, Inputs)> = Vec::new();
    if let Some(file) = &load {
        let text = match fs::read_to_string(file) {
            Ok(t) => t,
            Err(e) => return fail(format!("{file}: {e}")),
        };
        let value: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => return fail(format!("{file}: {e}")),
        };
        boards.push((file.clone(), from_json(&value)));
    } else {
        let Some(path) = path else {
            return fail(
                "usage: schematic_metrics <file.nsm | project folder> [--plain] [--violations] \
                 [--save-inputs <file>] [--svg <file>]  or  --inputs <file>"
                    .into(),
            );
        };
        let path = match PathBuf::from(&path).canonicalize() {
            Ok(p) => p,
            Err(e) => return fail(format!("{path}: {e}")),
        };
        let target = if path.is_dir() {
            Target::resolve_from(&path, None)
        } else {
            let parent = path.parent().unwrap_or(&path);
            Target::resolve_from(parent, Some(&path.to_string_lossy()))
        };
        let target = match target {
            Ok(t) => t,
            Err(e) => return fail(e),
        };
        let (loader, catalogs) = match (target.loader(), target.catalogs()) {
            (Ok(l), Ok(c)) => (l, c),
            (Err(e), _) | (_, Err(e)) => return fail(e),
        };
        let root = target.project.as_ref().map(|p| p.root.clone());
        for source in &target.files {
            let text = match fs::read_to_string(&source.path) {
                Ok(t) => t,
                Err(e) => return fail(format!("{}: {e}", source.name)),
            };
            let outcome = match outcome_with(&source.name, &text, &loader, &catalogs, None) {
                Ok(o) => o,
                Err(problems) => {
                    return fail(
                        problems
                            .iter()
                            .map(|p| p.rendered.clone())
                            .collect::<String>(),
                    );
                }
            };
            let file = source
                .path
                .file_name()
                .map_or_else(|| source.name.clone(), |n| n.to_string_lossy().into_owned());
            for built in &outcome.boards {
                boards.push((
                    file.clone(),
                    Inputs {
                        root: root.clone(),
                        design: built.design.clone(),
                        groups: module_groups(built),
                        rails: rails_of(built),
                        marks: must_connect(built),
                    },
                ));
            }
        }
    }
    for (file, inputs) in &boards {
        if let Some(out) = &save
            && let Err(e) = fs::write(out, to_json(inputs).to_string())
        {
            return fail(format!("{out}: {e}"));
        }
        let libraries = match Libraries::for_project(inputs.root.as_deref()) {
            Ok(l) => l,
            Err(e) => return fail(e),
        };
        let layout = match layout_sheets_marked(
            &inputs.design,
            &inputs.groups,
            &inputs.rails,
            &inputs.marks,
            &libraries.mapping,
            &libraries.symbols,
        ) {
            Ok(l) => l,
            Err(e) => return fail(format!("{}: {e}", inputs.design.name)),
        };
        if let Some(out) = &svg {
            // A file of several boards writes one picture for each, named after the board.
            let out = &if boards.len() > 1 {
                match out.rsplit_once('.') {
                    Some((stem, ext)) => format!("{stem}-{}.{ext}", inputs.design.name),
                    None => format!("{out}-{}", inputs.design.name),
                }
            } else {
                out.clone()
            };
            let drawn = render_svg_by_module_marked(
                &inputs.design,
                &inputs.groups,
                &inputs.rails,
                &inputs.marks,
                &libraries.mapping,
                &libraries.symbols,
            );
            match drawn {
                Ok(text) => {
                    if let Err(e) = fs::write(out, text) {
                        return fail(format!("{out}: {e}"));
                    }
                }
                Err(e) => return fail(e.to_string()),
            }
        }
        let metrics = measure(&layout);
        let broken = check(&inputs.design, &layout);
        println!(
            "# board {} ({}): {} parts, {} sheets",
            inputs.design.name,
            file,
            inputs.design.parts.len(),
            layout.sheets.len().max(1)
        );
        if plain {
            print!("{}", metrics.plain());
        } else {
            print!("{}", metrics.table());
        }
        let mut kinds: Vec<(&str, usize)> = Vec::new();
        for v in &broken {
            match kinds.iter_mut().find(|(k, _)| *k == v.kind()) {
                Some((_, n)) => *n += 1,
                None => kinds.push((v.kind(), 1)),
            }
        }
        kinds.sort();
        println!(
            "# rules broken: {}{}",
            broken.len(),
            kinds
                .iter()
                .map(|(k, n)| format!(", {k} {n}"))
                .collect::<String>()
        );
        if violations {
            for v in &broken {
                println!("#   {v}");
            }
        }
    }
    ExitCode::SUCCESS
}
