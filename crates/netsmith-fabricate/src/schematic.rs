//! The schematic of a built board as the editor draws it (`GET /api/schematic.svg`), for the
//! command line (`netsmith view schematic`) and the fab package (`<board>-schematic.pdf`),
//! issue #1091. The drawing is `netsmith-view`'s, with the inputs the editor gives it: the
//! module groups of the board, its rails, the pins that must be connected, and the schematic
//! layout file beside the source (`<stem>.sch.toml`, ADR 0061). What it is written as
//! (PDF, PNG, one sheet's SVG) is `netsmith_view::export`'s, a function of the SVG alone, so
//! the editor's export and these give the same bytes.

use std::fs;
use std::path::{Path, PathBuf};

use netsmith_pipeline::{Built, Libraries};

use crate::project::Project;
use netsmith_view::{HintFinding, LayoutHints};

pub use netsmith_view::export::{self, ExportError, Format, Heading};

/// The file the fab package holds the schematic in, beside the board's other files.
pub fn pdf_name(board: &str) -> String {
    format!("{board}-schematic.pdf")
}

/// What every title block of a project's schematic says besides the sheet and the board: the
/// project's name, its revision (`revision` in `netsmith.toml`, issue #1134) and the release of
/// netsmith that drew it.
pub fn heading(project: Option<&str>, revision: Option<&str>) -> Heading {
    Heading {
        project: project.map(str::to_string),
        revision: revision.map(str::to_string),
        release: format!("netsmith {}", env!("CARGO_PKG_VERSION")),
    }
}

/// What `fab` draws the schematic PDF with besides the board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inputs {
    /// The schematic layout file beside the source (`<stem>.sch.toml`); one that is not there
    /// changes nothing.
    pub layout: PathBuf,
    /// The project's name, for the title blocks.
    pub project: Option<String>,
    /// The project's revision, for the title blocks.
    pub revision: Option<String>,
}

impl Inputs {
    /// The inputs of a source file of `project` (none for a file outside any project).
    pub fn of(source: &Path, project: Option<&Project>) -> Inputs {
        Inputs {
            layout: netsmith_view::schematic_hints_path(source),
            project: project.map(|p| p.manifest.name.clone()),
            revision: project.and_then(|p| p.manifest.revision.clone()),
        }
    }

    /// The heading these inputs give a title block.
    pub fn heading(&self) -> Heading {
        heading(self.project.as_deref(), self.revision.as_deref())
    }
}

/// The schematic layout file beside `source`, as the drawing reads it: see [`read_hints_file`].
pub fn read_hints(source: &Path) -> (Option<LayoutHints>, Option<String>) {
    read_hints_file(&netsmith_view::schematic_hints_path(source))
}

/// The schematic layout file `path`, as the drawing reads it: its entries, none when there is
/// no file, and why it is not used when it cannot be read or does not parse (the drawing is
/// then the one without it, as in the editor).
pub fn read_hints_file(path: &Path) -> (Option<LayoutHints>, Option<String>) {
    let shown = path.display();
    match fs::read_to_string(path) {
        Ok(text) => match netsmith_view::hints::parse(&text) {
            Ok(hints) => (Some(hints), None),
            Err(errors) => (
                None,
                Some(format!(
                    "{shown}: {}; the schematic is drawn without the file",
                    errors
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("; ")
                )),
            ),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (None, None),
        Err(e) => (
            None,
            Some(format!(
                "{shown}: {e}; the schematic is drawn without the file"
            )),
        ),
    }
}

/// A finding of the schematic layout file as a line: `<file>:<line>: <entry>: <message>`.
pub fn finding_line(file: &Path, f: &HintFinding) -> String {
    if f.line == 0 {
        format!("{}: {}: {}", file.display(), f.entry, f.message)
    } else {
        format!("{}:{}: {}: {}", file.display(), f.line, f.entry, f.message)
    }
}

/// The PDF of the schematics of `boards` (one board, or a system's members one after the
/// other), titled `title`, as `fab` and `view schematic` write it, and what the drawing said of
/// the layout file (its entries it could not honour, a file it could not read).
pub fn pdf(
    title: &str,
    boards: &[&Built],
    libraries: &Libraries,
    inputs: Option<&Inputs>,
) -> Result<(Vec<u8>, Vec<String>), String> {
    let mut said = Vec::new();
    let hints = inputs.and_then(|i| {
        let (hints, why) = read_hints_file(&i.layout);
        said.extend(why);
        hints
    });
    let mut svgs = Vec::with_capacity(boards.len());
    for built in boards {
        let (svg, findings) = draw(built, libraries, hints.as_ref())
            .map_err(|why| format!("the schematic of {} cannot be drawn: {why}", built.label()))?;
        if let Some(i) = inputs {
            said.extend(findings.iter().map(|f| finding_line(&i.layout, f)));
        }
        svgs.push(svg);
    }
    let svgs: Vec<&str> = svgs.iter().map(String::as_str).collect();
    let heading = inputs.map_or_else(|| heading(None, None), Inputs::heading);
    let bytes = netsmith_view::export::pdf(title, &svgs, &heading).map_err(|e| e.to_string())?;
    Ok((bytes, said))
}

/// `<name>-schematic.pdf` in `dir`, of `boards` (or, with `write` false, the removal of one an
/// earlier run left): the file written and the notes for the person ordering when it could not
/// be. What the drawing said of the layout file is in the notes too, as `fab` says it.
pub fn write_pdf(
    dir: &Path,
    name: &str,
    boards: &[&Built],
    libraries: &Libraries,
    inputs: Option<&Inputs>,
    write: bool,
) -> (Option<PathBuf>, Vec<String>) {
    let file = dir.join(pdf_name(name));
    if !write {
        return match fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => (
                None,
                vec![format!(
                    "{}: {e}; an earlier run's schematic PDF stays in the package",
                    file.display()
                )],
            ),
            _ => (None, Vec::new()),
        };
    }
    let (bytes, said) = match pdf(&format!("{name} schematic"), boards, libraries, inputs) {
        Ok(done) => done,
        Err(why) => {
            return (
                None,
                vec![format!(
                    "{why}; the schematic PDF {} was not written, and the package is complete without it",
                    file.display()
                )],
            );
        }
    };
    match fs::write(&file, bytes) {
        Ok(()) => (Some(file), said),
        Err(e) => (
            None,
            vec![format!(
                "the schematic PDF {} was not written ({e}); the package is complete without it",
                file.display()
            )],
        ),
    }
}

/// The SVG of `built`'s schematic, as the editor draws it with `hints` (the schematic layout
/// file, if any), and the entries of the file the drawing could not honour.
pub fn draw(
    built: &Built,
    libraries: &Libraries,
    hints: Option<&LayoutHints>,
) -> Result<(String, Vec<HintFinding>), String> {
    let groups = module_groups(built);
    let (resolved, mut findings) = match hints {
        Some(h) => h.resolve(&built.design, &groups),
        None => Default::default(),
    };
    let (svg, unusable) = draw_resolved(built, libraries, &groups, &resolved)?;
    findings.extend(unusable);
    Ok((svg, findings))
}

/// [`draw`] with the module groups and the layout file's entries for the board already
/// worked out (the editor resolves them first, to list what does not match even when the
/// drawing fails): the SVG and the entries the drawing could not honour.
pub fn draw_resolved(
    built: &Built,
    libraries: &Libraries,
    groups: &[netsmith_view::Group],
    resolved: &netsmith_view::ResolvedHints,
) -> Result<(String, Vec<HintFinding>), String> {
    let (layout, unusable) = netsmith_view::layout_sheets_hinted_findings(
        &built.design,
        groups,
        &rails_of(built),
        &must_connect(built),
        resolved,
        &libraries.mapping,
        &libraries.symbols,
    )
    .map_err(|e| e.to_string())?;
    Ok((
        netsmith_view::render_layout(&built.design, &layout),
        unusable,
    ))
}

/// The layout the schematic view draws for `built`, with the inputs the editor gives it
/// (its rails, its must-connect pins, its module groups): what `render_svg_by_module_marked`
/// draws, before it is drawn. For tools that measure the drawing.
pub fn schematic_layout(
    built: &Built,
    libraries: &Libraries,
) -> Result<netsmith_view::Layout, String> {
    netsmith_view::layout_sheets_marked(
        &built.design,
        &module_groups(built),
        &rails_of(built),
        &must_connect(built),
        &libraries.mapping,
        &libraries.symbols,
    )
    .map_err(|e| e.to_string())
}

/// The power nets of a board, from the checker's model: a net a rail draws from is a supply,
/// a net the current returns on is a ground. Which nets are which is the design's to say
/// (a `Rail`'s `pos` and `neg`), not their names.
pub fn rails_of(built: &Built) -> netsmith_view::Rails {
    let mut rails = netsmith_view::Rails::default();
    for balance in built.trace.model.balances() {
        if balance.returns {
            rails.ground.insert(balance.net.clone());
        } else {
            rails.positive.insert(balance.net.clone());
        }
    }
    // A net both a return and a supply (a rail stacked on another) is drawn as a ground only
    // when it is never a supply.
    let both: Vec<String> = rails
        .positive
        .intersection(&rails.ground)
        .cloned()
        .collect();
    for net in both {
        rails.ground.remove(&net);
    }
    rails
}

/// The pins the checker says must be connected (a power or input pin that is not `optional`,
/// any pin that is `required`) that nothing else is on the net of, with the checker's message,
/// so the drawing can mark them. A board that has one does not build, so this is what a pin
/// table of a part under construction shows; it asks the checker's own question and changes
/// nothing in it.
pub fn must_connect(built: &Built) -> netsmith_view::Marks {
    use netsmith_check::{InstKind, PinKind, Pinout};
    let model = &built.trace.model;
    let refdes: std::collections::HashMap<&str, &str> = built
        .design
        .parts
        .iter()
        .map(|p| (p.path.as_str(), p.refdes.as_str()))
        .collect();
    let mut marks = netsmith_view::Marks::default();
    for (id, inst) in model.instances() {
        if inst.kind != InstKind::Part || model.board_of(id).is_none() {
            continue;
        }
        let Pinout::Table(table) = model.pinout(id) else {
            continue;
        };
        let path = model.path_of(id);
        let Some(&refdes) = refdes.get(path.as_str()) else {
            continue;
        };
        for row in &table.rows {
            let must = model.must_be_connected(row.pin, row);
            let company = model
                .net_of(row.pin)
                .is_some_and(|net| net.pins.iter().any(|&p| model.owner_of(p) != Some(id)));
            if !must || company {
                continue;
            }
            let kind = match row.kind {
                PinKind::Power => "a power pin",
                PinKind::Input => "an input",
                PinKind::Output => "an output",
                PinKind::Bidirectional => "a bidirectional pin",
                PinKind::Passive => "a passive pin",
                PinKind::NoConnect => "a no_connect pin",
                PinKind::TriState => "a tri_state output",
                PinKind::OpenDrain => "an open_drain output",
            };
            let pads = if row.pads.len() == 1 {
                format!("pad {}", row.pads[0])
            } else {
                format!("pads {}", row.pads.join(", "))
            };
            let message = format!(
                "`{}` is {kind} that must be connected ({pads}, `{}`), but nothing else is on its net",
                model.path_of(row.pin),
                row.name
            );
            for pad in &row.pads {
                marks
                    .must
                    .insert((refdes.to_string(), pad.clone()), message.clone());
            }
        }
    }
    marks
}

/// The parts of a board grouped by the module instance of the board each is inside (a part
/// declared on the board itself is in none), in the order the design lists the parts.
pub fn module_groups(built: &Built) -> Vec<netsmith_view::Group> {
    let model = &built.trace.model;
    let mut groups: Vec<netsmith_view::Group> = Vec::new();
    for (part, &inst) in built.trace.origins.iter().enumerate() {
        let mut top = inst;
        while let Some(parent) = model.instance(top).parent {
            if parent == model.root {
                break;
            }
            top = parent;
        }
        if top == inst || model.instance(top).parent.is_none() {
            continue;
        }
        let name = model.instance(top).name.clone();
        match groups.iter_mut().find(|g| g.name == name) {
            Some(g) => g.parts.push(part),
            None => groups.push(netsmith_view::Group {
                name,
                detail: model.instance(top).type_name.clone(),
                parts: vec![part],
            }),
        }
    }
    groups
}

/// The system's own sheet (issue #1091): a box for each member board, with the board it is and
/// its ports that join another board's, and under them the list of the joins and the mated
/// connectors, as the editor's system view lists them. An SVG in the schematic's style, with no
/// module sheets, so the PDF draws it as one page named for the system.
pub fn system_svg(system: &netsmith_pipeline::SolvedSystem) -> String {
    use std::fmt::Write as _;
    let model = &system.model;
    let member_of = |inst: netsmith_check::InstId| {
        let owner = model.board_of(inst)?;
        system
            .boards
            .iter()
            .find(|(_, _, board)| *board == owner)
            .map(|(member, _, _)| member.clone())
    };
    let joins: Vec<(Option<String>, String, Option<String>, String)> = model
        .joins()
        .iter()
        .map(|&(a, b)| {
            (
                member_of(a),
                model.path_of(a),
                member_of(b),
                model.path_of(b),
            )
        })
        .collect();
    let mates: Vec<String> = model
        .mates()
        .iter()
        .map(|m| format!("{} mates {}", model.path_of(m.a), model.path_of(m.b)))
        .collect();
    const BOX_W: f64 = 60.0;
    const GAP: f64 = 15.0;
    const LINE: f64 = 3.0;
    let ports_of = |member: &str| -> Vec<String> {
        let mut ports: Vec<String> = joins
            .iter()
            .flat_map(|(ma, pa, mb, pb)| {
                [
                    (ma.as_deref() == Some(member)).then(|| pa.clone()),
                    (mb.as_deref() == Some(member)).then(|| pb.clone()),
                ]
            })
            .flatten()
            .collect();
        let mut seen = std::collections::HashSet::new();
        ports.retain(|p| seen.insert(p.clone()));
        ports
    };
    let tallest = system
        .boards
        .iter()
        .map(|(m, _, _)| ports_of(m).len())
        .max()
        .unwrap_or(0);
    let box_h = 14.0 + LINE * tallest as f64;
    let n = system.boards.len().max(1) as f64;
    let list_top = 10.0 + box_h + 12.0;
    let rows = joins.len() + mates.len();
    let width = (n * BOX_W + (n - 1.0) * GAP + 20.0).max(120.0);
    let height = list_top + 6.0 + LINE * rows as f64 + 10.0;
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    let mut w = String::new();
    let _ = writeln!(
        w,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" class=\"schematic system\" viewBox=\"0 0 {width} {height}\" data-board=\"{}\">",
        esc(&system.name)
    );
    let _ = writeln!(
        w,
        "<rect x=\"0\" y=\"0\" width=\"{width}\" height=\"{height}\" fill=\"#fbfaf3\"/>"
    );
    let text = |w: &mut String, x: f64, y: f64, size: f64, bold: bool, s: &str| {
        let _ = writeln!(
            w,
            "<text x=\"{x}\" y=\"{y}\" font-size=\"{size}\" font-weight=\"{}\" fill=\"#1d4f5e\">{}</text>",
            if bold { 700 } else { 400 },
            esc(s)
        );
    };
    for (i, (member, type_name, _)) in system.boards.iter().enumerate() {
        let x = 10.0 + i as f64 * (BOX_W + GAP);
        let _ = writeln!(
            w,
            "<rect x=\"{x}\" y=\"10\" width=\"{BOX_W}\" height=\"{box_h}\" rx=\"1\" fill=\"#fff6bd\" stroke=\"#7b2020\" stroke-width=\"0.35\"/>"
        );
        text(&mut w, x + 3.0, 16.0, 3.6, true, member);
        text(&mut w, x + 3.0, 20.5, 2.2, false, type_name);
        for (k, port) in ports_of(member).iter().enumerate() {
            text(&mut w, x + 3.0, 26.0 + LINE * k as f64, 1.8, false, port);
        }
    }
    text(
        &mut w,
        10.0,
        list_top,
        3.0,
        true,
        "Joins between the boards",
    );
    for (k, line) in joins
        .iter()
        .map(|(_, a, _, b)| format!("{a} ~ {b}"))
        .chain(mates.iter().cloned())
        .enumerate()
    {
        text(
            &mut w,
            10.0,
            list_top + 6.0 + LINE * k as f64,
            2.0,
            false,
            &line,
        );
    }
    let _ = writeln!(w, "</svg>");
    w
}

/// [`write_pdf`] for a system: its own sheet ([`system_svg`]) and then each member's, as
/// `<out>/<System>/<System>-schematic.pdf`.
pub fn write_system_pdf(
    dir: &Path,
    system: &netsmith_pipeline::SolvedSystem,
    members: &[&Built],
    libraries: &Libraries,
    inputs: Option<&Inputs>,
    write: bool,
) -> (Option<PathBuf>, Vec<String>) {
    let file = dir.join(pdf_name(&system.name));
    if !write {
        return write_pdf(dir, &system.name, members, libraries, inputs, false);
    }
    let made = (|| -> Result<(Vec<u8>, Vec<String>), String> {
        let mut said = Vec::new();
        let hints = inputs.and_then(|i| {
            let (hints, why) = read_hints_file(&i.layout);
            said.extend(why);
            hints
        });
        let mut svgs = vec![system_svg(system)];
        for built in members {
            let (svg, _) = draw(built, libraries, hints.as_ref()).map_err(|why| {
                format!("the schematic of {} cannot be drawn: {why}", built.label())
            })?;
            svgs.push(svg);
        }
        let svgs: Vec<&str> = svgs.iter().map(String::as_str).collect();
        let heading = inputs.map_or_else(|| heading(None, None), Inputs::heading);
        let bytes =
            netsmith_view::export::pdf(&format!("{} schematic", system.name), &svgs, &heading)
                .map_err(|e| e.to_string())?;
        Ok((bytes, said))
    })();
    match made.and_then(|(bytes, said)| {
        fs::write(&file, bytes)
            .map(|()| said)
            .map_err(|e| e.to_string())
    }) {
        Ok(said) => (Some(file), said),
        Err(why) => (
            None,
            vec![format!(
                "the system's schematic PDF {} was not written ({why}); the boards' packages are complete without it",
                file.display()
            )],
        ),
    }
}
