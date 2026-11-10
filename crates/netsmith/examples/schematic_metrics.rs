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
//! `--stability` does not print the drawing's metrics: it applies seven small edits to every
//! sheet (rename a net, add a decoupling capacitor, add a pull-up, delete a part, change a
//! value, add a part to another module, add a part on no net), draws before and after, and
//! prints what moved (`netsmith_view::stability`); `--sheets a,b` limits it to those sheets
//! and `--min-parts n` sets the size of the sheets its summary counts (default 8).
//! `--overlay <prefix> --edit <a..h> --sheets <name>` writes `<prefix>-before.svg` and `-after.svg`, one sheet drawn before the edit
//! and after it in one box, the same reference part at one point (laid over each other in two
//! colours by a script, so the
//! parts that did not move are dark and those that did show in two colours).
//!
//! A big project takes most of a minute to analyse, and the drawing a fraction of a second.
//! `--save-inputs <file>` writes what the drawing is made from (the design, its rails, its
//! must-connect pins, its module groups and the project folder the symbols come from) as JSON,
//! and `schematic_metrics --inputs <file>` draws from that file alone, so a change to the
//! drawing can be measured at once. `--svg <file>` writes the drawing as the editor serves it (a file of several boards
//! writes `<file>-<board>.svg` for each). `--hints <file.sch.toml>` draws with a schematic layout file (ADR 0061): its
//! errors stop the run, and what no longer matches the design and what the drawing could not honour are printed as
//! `# layout file:` lines.

use std::{env, fs, path::PathBuf, process::ExitCode};

use netsmith::editor::{module_groups, must_connect, rails_of};
use netsmith::project::Target;
use netsmith::{Libraries, outcome_with};
use netsmith_core::{
    Design, InterfaceInstance, InterfaceMember, Net, Note, NoteAt, NoteKind, Part, PartPin,
    PinKind, PinRef, PowerFlow,
};
use netsmith_view::check::check;
use netsmith_view::hints;
use netsmith_view::metrics::measure;
use netsmith_view::stability::{self, Board};
use netsmith_view::{
    Group, Marks, Rails, layout_sheets_hinted_findings, render_layout, render_svg_by_module_marked,
};
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
                "power_flow": q.power_flow.map(|f| f.word()),
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
        "notes": i.design.notes.iter().map(|n| json!({
            "path": n.path,
            "kind": match n.kind { NoteKind::Doc => "doc", NoteKind::Relation => "relation" },
            "text": n.text,
            "at": n.at.as_ref().map(|a| json!([a.file, a.line])),
            "anchors": n.anchors,
            "result": n.result,
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
            provides: strs(&p["provides"]),
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
                    power_flow: match q["power_flow"].as_str() {
                        Some("takes") => Some(PowerFlow::Takes),
                        Some("gives") => Some(PowerFlow::Gives),
                        _ => None,
                    },
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
            // An inputs file saved before notes existed has none.
            notes: list(&v["notes"])
                .iter()
                .map(|n| Note {
                    path: s(&n["path"]),
                    kind: if n["kind"] == "relation" {
                        NoteKind::Relation
                    } else {
                        NoteKind::Doc
                    },
                    text: s(&n["text"]),
                    at: n["at"].as_array().map(|a| NoteAt {
                        file: s(&a[0]),
                        line: a[1].as_u64().unwrap_or(0) as usize,
                    }),
                    anchors: strs(&n["anchors"]),
                    result: n["result"].as_str().map(String::from),
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

/// The sheet `sheet` drawn before `kind` of edit and after it, over each other.
fn write_overlay(
    out: &str,
    board: &Board,
    inputs: &Inputs,
    libraries: &Libraries,
    sheet: &str,
    kind: stability::Kind,
) -> Result<String, String> {
    let base = board.draw(&libraries.mapping, &libraries.symbols);
    let edit = stability::plan(board, &base, sheet, kind)
        .ok_or_else(|| format!("no {} for sheet {sheet}", kind.name()))?;
    let edited = stability::apply(board, &edit);
    let after = edited.board.draw(&libraries.mapping, &libraries.symbols);
    let m = stability::compare(
        (&base, &board.design),
        (&after, &edited.board.design),
        sheet,
        &edited.touched,
        &edited.renames,
    )
    .ok_or("the sheet is in neither drawing")?;
    let svg_of = |b: &Board| {
        render_svg_by_module_marked(
            &b.design,
            &b.groups,
            &b.rails,
            &inputs.marks,
            &libraries.mapping,
            &libraries.symbols,
        )
        .map_err(|e| e.to_string())
    };
    let view = |text: &str| -> Vec<f64> {
        text.split("viewBox=\"")
            .nth(1)
            .and_then(|r| r.split('"').next())
            .map(|v| v.split(' ').filter_map(|n| n.parse().ok()).collect())
            .unwrap_or_default()
    };
    let (svg_a, svg_b) = (svg_of(board)?, svg_of(&edited.board)?);
    // The box that holds both sheets with the reference part at the origin.
    let mut union = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for i in 0..2 {
        let (rx, ry) = m.reference_at[i];
        let f = m.frames[i];
        union = [
            union[0].min(f[0] - rx),
            union[1].min(f[1] - ry),
            union[2].max(f[2] - rx),
            union[3].max(f[3] - ry),
        ];
    }
    let (w, h) = (union[2] - union[0], union[3] - union[1]);
    for (i, (text, class)) in [(&svg_a, "before"), (&svg_b, "after")]
        .into_iter()
        .enumerate()
    {
        if view(text).len() != 4 {
            return Err("a drawing has no viewBox".into());
        }
        let (rx, ry) = m.reference_at[i];
        // The drawing as a nested picture whose point P lands at P minus the reference part, on
        // a sheet of paper that covers both: the two pictures have the same box.
        let end = text.find('>').ok_or("a drawing has no root")?;
        let picture = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {w} {h}\" width=\"{}\" height=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{w}\" height=\"{h}\" fill=\"#fff\"/><svg class=\"schematic\" x=\"{}\" y=\"{}\" width=\"{w}\" height=\"{h}\" viewBox=\"{} {} {w} {h}\" xmlns=\"http://www.w3.org/2000/svg\">{}",
            union[0],
            union[1],
            w * 5.0,
            h * 5.0,
            union[0],
            union[1],
            union[0],
            union[1],
            union[0] + rx,
            union[1] + ry,
            &text[end + 1..],
        );
        let file = format!("{out}-{class}.svg");
        fs::write(&file, format!("{picture}</svg>")).map_err(|e| format!("{file}: {e}"))?;
    }
    if std::env::var_os("NETSMITH_STABLE_TRACE").is_some() {
        for (k, d) in &m.moves {
            eprintln!("moved {k}: {:.2}, {:.2}", d.0, d.1);
        }
        eprintln!("reference {} rows {:?}", m.reference, m.rows);
    }
    Ok(format!(
        "{}: {} parts, {:.0}% of {} unmoved, {:.0}% of {} segments kept (anchor frame)",
        edit.label,
        m.size,
        100.0 * m.anchor.unmoved_fraction(),
        m.anchor.parts,
        100.0 * m.anchor.segments_fraction(),
        m.anchor.segments
    ))
}

fn main() -> ExitCode {
    let mut path: Option<String> = None;
    let (mut plain, mut violations, mut stable) = (false, false, false);
    let mut hints_file: Option<String> = None;
    let mut only: Vec<String> = Vec::new();
    let (mut overlay, mut edit): (Option<String>, Option<String>) = (None, None);
    let mut min_parts = 8usize;
    let (mut save, mut load, mut svg): (Option<String>, Option<String>, Option<String>) =
        (None, None, None);
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--plain" => plain = true,
            "--violations" => violations = true,
            "--stability" => stable = true,
            "--overlay" => overlay = args.next(),
            "--edit" => edit = args.next(),
            "--sheets" => {
                only = args
                    .next()
                    .unwrap_or_default()
                    .split(',')
                    .map(String::from)
                    .collect()
            }
            "--min-parts" => min_parts = args.next().and_then(|n| n.parse().ok()).unwrap_or(8),
            "--save-inputs" => save = args.next(),
            "--inputs" => load = args.next(),
            "--svg" => svg = args.next(),
            "--hints" => hints_file = args.next(),
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
    // The schematic layout file (ADR 0061), read once: its errors stop the run, what no longer
    // matches the design is a finding printed for each board.
    let layout_hints = match &hints_file {
        None => None,
        Some(path) => {
            let text = match fs::read_to_string(path) {
                Ok(t) => t,
                Err(e) => return fail(format!("{path}: {e}")),
            };
            match hints::parse(&text) {
                Ok(h) => {
                    let names: Vec<&str> =
                        boards.iter().map(|(_, i)| i.design.name.as_str()).collect();
                    for f in h.unknown_boards(&names) {
                        println!("# layout file: {}", f.render(path));
                    }
                    Some(h)
                }
                Err(errors) => {
                    for e in &errors {
                        eprintln!("{path}:{}: {}", e.line, e.message);
                    }
                    return ExitCode::from(2);
                }
            }
        }
    };
    let mut rows: Vec<stability::Row> = Vec::new();
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
        if let Some(out) = &overlay {
            let board = Board {
                design: inputs.design.clone(),
                groups: inputs.groups.clone(),
                rails: inputs.rails.clone(),
                marks: inputs.marks.clone(),
            };
            let letter = edit
                .as_deref()
                .and_then(|e| e.chars().next())
                .unwrap_or('b');
            let Some(kind) = stability::Kind::ALL
                .into_iter()
                .find(|k| k.letter() == letter)
            else {
                return fail(format!("no edit {letter}"));
            };
            let Some(sheet) = only.first() else {
                return fail("--overlay needs --sheets <name>".into());
            };
            return match write_overlay(out, &board, inputs, &libraries, sheet, kind) {
                Ok(label) => {
                    println!("{label}");
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            };
        }
        if stable {
            let board = Board {
                design: inputs.design.clone(),
                groups: inputs.groups.clone(),
                rails: inputs.rails.clone(),
                marks: inputs.marks.clone(),
            };
            rows.extend(stability::measure(
                &inputs.design.name,
                &board,
                &libraries.mapping,
                &libraries.symbols,
                &only,
            ));
            continue;
        }
        let (resolved, mut findings) = match &layout_hints {
            Some(h) => h.resolve(&inputs.design, &inputs.groups),
            None => Default::default(),
        };
        let (layout, engine_findings) = match layout_sheets_hinted_findings(
            &inputs.design,
            &inputs.groups,
            &inputs.rails,
            &inputs.marks,
            &resolved,
            &libraries.mapping,
            &libraries.symbols,
        ) {
            Ok(l) => l,
            Err(e) => return fail(format!("{}: {e}", inputs.design.name)),
        };
        findings.extend(engine_findings);
        if let Some(path) = &hints_file {
            for f in &findings {
                println!("# layout file: {}", f.render(path));
            }
        }
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
            if let Err(e) = fs::write(out, render_layout(&inputs.design, &layout)) {
                return fail(format!("{out}: {e}"));
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
    if stable {
        print!("{}", stability::table(&rows, min_parts));
    }
    ExitCode::SUCCESS
}
