//! Parts that the board house is to assemble but that have no supplier part number (an `lcsc`),
//! as `fab` reports them: all together, before anything is placed or routed, each with its
//! reference designator, its path, what its catalog row says, and what to write, and where,
//! to say that the board house is not to assemble it (ADR 0024, `.assembly`).

use netsmith_check::Sources;
use netsmith_design::Sourced;

use netsmith_pipeline::Built;

/// One part to assemble with no supplier part number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unnumbered {
    pub refdes: String,
    /// Its path from the board: `first`, or `pair.a` inside a module.
    pub path: String,
    pub kind: String,
    pub value: String,
    pub mpn: Option<String>,
    /// Where the board declares the part, or, for a part inside a module, the board's member
    /// that holds it: `main.nsm:21`.
    pub declared: String,
    /// For a part inside a module: the module's member in the board, its definition, and
    /// where that is.
    pub module: Option<Module>,
}

/// The module that holds a part that the board cannot reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    /// The board's member that is an instance of it: `pair`.
    pub member: String,
    /// The definition's name: `Pair`.
    pub definition: String,
    /// The part's own name in its module, `a`, or its path inside it when the module holds
    /// the part through another module.
    pub inner: String,
    /// Where the definition of the module that holds the part directly is: `main.nsm:6`.
    pub defined: String,
    /// Whether the part is directly in the module the board's member is an instance of.
    pub direct: bool,
}

fn place(sources: &Sources, source: netsmith_check::SourceId, span: netsmith_lang::Span) -> String {
    let s = sources.get(source);
    let line = s.text[..span.start.min(s.text.len())].matches('\n').count() + 1;
    format!("{}:{line}", s.name)
}

/// The parts of `built` that the board house is to assemble and that have no `lcsc` in
/// `sourcing` (the sourcing after generic parts were resolved), in the design's order.
pub fn find(built: &Built, sourcing: &[Sourced]) -> Vec<Unnumbered> {
    let trace = &built.trace;
    let model = &trace.model;
    let mut found = Vec::new();
    for (i, s) in sourcing.iter().enumerate() {
        if !s.needs_part_number() || s.lcsc.is_some() || s.generic.is_some() {
            continue;
        }
        let (Some(part), Some(origin)) = (built.design.parts.get(i), trace.origins.get(i)) else {
            continue;
        };
        let inst = model.instance(*origin);
        let (source, span) = inst.at();
        // The board's member that holds the part, if the part is inside a module.
        let mut top = *origin;
        while let Some(parent) = model.instance(top).parent
            && !model.path_of(parent).is_empty()
        {
            top = parent;
        }
        let module = (top != *origin).then(|| {
            let holder = model.instance(inst.parent.unwrap_or(top));
            let member = model.instance(top);
            Module {
                member: member.name.clone(),
                definition: holder.type_name.clone(),
                inner: model.path_below(*origin, inst.parent.unwrap_or(top)),
                defined: place(&trace.sources, holder.source, holder.span),
                direct: model.instance(*origin).parent == Some(top),
            }
        });
        let declared = match &module {
            Some(_) => {
                let (source, span) = model.instance(top).at();
                place(&trace.sources, source, span)
            }
            None => place(&trace.sources, source, span),
        };
        found.push(Unnumbered {
            refdes: s.refdes.clone(),
            path: part.path.clone(),
            kind: s.kind.clone(),
            value: s.value.clone(),
            mpn: s.mpn.clone(),
            declared,
            module,
        });
    }
    found
}

/// What each part is, for a list: `R1 (first, Resistor 1.234k, ZZR-1234)`.
fn describe(u: &Unnumbered) -> String {
    let what = match &u.mpn {
        Some(mpn) if *mpn != u.value => format!("{} {} ({mpn})", u.kind, u.value),
        Some(mpn) => format!("{} {mpn}", u.kind),
        None => format!("{} {}, no manufacturer's number", u.kind, u.value),
    };
    let at = match u.module {
        Some(_) => "its module is at",
        None => "at",
    };
    format!("{} `{}`: {what}, {at} {}", u.refdes, u.path, u.declared)
}

/// The error `fab` prints for `parts`. It says what to write for each, and where.
pub fn message(board: &str, parts: &[Unnumbered]) -> String {
    let mut out = format!(
        "{} part(s) have no LCSC part number, so JLCPCB cannot assemble them, and `fab` does not package a board that says it will:\n",
        parts.len()
    );
    for u in parts {
        out.push_str(&format!("  {}\n", describe(u)));
    }
    out.push_str(
        "To get past it, say that the board house is not to assemble these parts (they stay out of bom.csv and cpl.csv, and `fab-notes.txt` lists them as `hand-solder:` and as having no part number), or give the row a supplier part number. `fab` has no flag for this; the design says it (`.assembly`, \"Who assembles a part\" in docs/LANGUAGE.md):\n",
    );
    let mut offered_row = false;
    for u in parts {
        match &u.module {
            None => out.push_str(&format!(
                "  {}: in the body of `board {board}` write `.{}.assembly &= AssemblyMode::Hand` (or `AssemblyMode::None` if nobody fits it, or `AssemblyMode::Consigned` if you ship the part to the board house, which then places it)\n",
                u.refdes, u.path
            )),
            Some(m) => {
                out.push_str(&format!(
                    "  {}: `{}` is inside `{}`, and a board cannot reach the parts of a module. ",
                    u.refdes, u.path, m.member
                ));
                out.push_str(&format!(
                    "In the definition of `{}` ({}) write `.{}.assembly &= AssemblyMode::Hand`",
                    m.definition, m.defined, m.inner
                ));
                if m.direct {
                    out.push_str(&format!(
                        "; or, so that each board decides, declare `.assembly_mode: AssemblyMode` there, write `.{}.assembly &= .assembly_mode` and, in the body of `board {board}`, `.{}.assembly_mode &= AssemblyMode::Hand`",
                        m.inner, m.member
                    ));
                }
                out.push('\n');
            }
        }
        if u.mpn.is_some() {
            offered_row = true;
        }
    }
    if offered_row {
        out.push_str(
            "For every part that a catalog row stands for, you can instead put `hand` in the row's `assembly` column in your own catalog file (ADR 0024), or fill its `lcsc` column with a number that has a recorded source (data/jlcpcb/README.md).\n",
        );
    }
    out.push_str(
        "The project-wide `assembly = \"hand\"` in netsmith.toml also works, but it is for every part of every board.",
    );
    out
}

/// The lines for `fab-notes.txt` about parts that have no supplier part number and are not
/// assembled by the board house (marked `hand` in the design or by a row) and whose catalog
/// row has a manufacturer's number only: the person ordering must supply them. Empty if there are none.
pub fn note(sourcing: &[Sourced]) -> Option<String> {
    let left: Vec<String> = sourcing
        .iter()
        .filter(|s| s.assembly == netsmith_design::Assembly::Hand)
        .filter(|s| s.lcsc.is_none() && s.mpn.is_some() && s.generic.is_none())
        .map(|s| s.refdes.clone())
        .collect();
    if left.is_empty() {
        return None;
    }
    Some(format!(
        "the board house does not assemble these (hand-solder, and no LCSC part number: they are not in bom.csv, so buy them yourself): {}",
        left.join(", ")
    ))
}
