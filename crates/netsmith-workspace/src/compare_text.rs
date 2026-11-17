//! A [`Compare`] as text for a person (`netsmith board compare --ref`, issue #1097): the
//! changes the editor's compare view lists, as lines, in the order of the answer. The JSON of
//! the command is the `Compare` itself, as `GET /api/compare` answers it, so the two cannot
//! differ; this is only how it reads on a terminal.
//!
//! Each board (and system) that changed is a heading with its lines, `+` for what was added,
//! `-` for what was removed and `~` for what changed in place; a board that did not change is
//! left out, as its count of unchanged parts and nets is. A ref that could not be read is a
//! sentence and the errors the compiler gave there, never an empty answer.

use std::fmt::Write;

use netsmith_editor::api::{
    BoardStatus, Budget, Compare, CompareBoard, CompareBomChange, CompareBomLine, CompareField,
    CompareInstance, CompareNet, ComparePin, CompareRail, CompareRailChange, CompareSystem,
    GitCommit, InstancesDiff, NamesDiff, PcbDiff, Problem, ProblemsDiff, Range, RangeChange,
    Severity, SourceStatus,
};

use super::diff::{amps, count};

/// What the comparison found, as lines: the header that names both sides, the one-line
/// summary, the source files that differ, then each board and system that changed.
pub fn render(c: &Compare) -> String {
    let mut out = String::new();
    let side = |commit: &GitCommit| format!("{} ({})", commit.short, commit.subject);
    let head = match (&c.against, &c.head.commit) {
        (Some(_), Some(commit)) => side(commit),
        _ if c.head.dirty => "the files on disk (uncommitted changes included)".to_string(),
        _ => "the files on disk".to_string(),
    };
    let _ = writeln!(out, "compare {} with {head}", side(&c.base));
    if let Some(error) = &c.error {
        let _ = writeln!(out, "cannot compare: {error}");
        problems_list(&mut out, "  ", &c.base_problems);
        return out;
    }
    let _ = writeln!(out, "{}", c.summary);
    if let Some(error) = &c.source.error {
        let _ = writeln!(out, "source files: {error}");
    } else if !c.source.files.is_empty() {
        let _ = writeln!(
            out,
            "source files: {} (+{} -{})",
            count(c.source.files_changed as usize, "changed", "changed"),
            c.source.added,
            c.source.removed
        );
        for f in &c.source.files {
            let status = match f.status {
                SourceStatus::Modified => "modified",
                SourceStatus::Added => "added",
                SourceStatus::Deleted => "deleted",
                SourceStatus::Untracked => "untracked",
            };
            let _ = writeln!(out, "  {} {status} (+{} -{})", f.path, f.added, f.removed);
        }
    }
    for b in c
        .boards
        .iter()
        .filter(|b| b.status != BoardStatus::Unchanged)
    {
        board(&mut out, b);
    }
    for s in c
        .systems
        .iter()
        .filter(|s| s.status != BoardStatus::Unchanged)
    {
        system(&mut out, s);
    }
    if !c.problems.new.is_empty() || !c.problems.resolved.is_empty() {
        let _ = writeln!(out, "\nproblems of no board");
        problems_diff(&mut out, &c.problems);
    }
    out
}

fn status(s: BoardStatus) -> &'static str {
    match s {
        BoardStatus::Added => "added",
        BoardStatus::Removed => "removed",
        BoardStatus::Changed => "changed",
        BoardStatus::Unchanged => "unchanged",
    }
}

fn fields(list: &[CompareField]) -> String {
    list.iter()
        .map(|f| {
            format!(
                "{} {} -> {}",
                f.field,
                f.old.as_deref().unwrap_or("none"),
                f.new.as_deref().unwrap_or("none")
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn pins(list: &[ComparePin]) -> String {
    list.iter()
        .map(|p| format!("{}.{}", p.designator, p.pin))
        .collect::<Vec<_>>()
        .join(" ")
}

fn range(r: Option<&Range>) -> &str {
    r.map_or("none", |r| r.text.as_str())
}

fn range_change(out: &mut String, what: &str, c: Option<&RangeChange>) {
    if let Some(c) = c {
        let _ = write!(
            out,
            ", {what} {} -> {}",
            range(c.base.as_ref()),
            range(c.head.as_ref())
        );
    }
}

fn budget(b: Budget) -> &'static str {
    match b {
        Budget::Within => "within budget",
        Budget::Over => "over budget",
        Budget::Unlimited => "unlimited",
        Budget::Unknown => "unknown",
    }
}

fn board(out: &mut String, b: &CompareBoard) {
    let _ = writeln!(
        out,
        "\nboard {} ({}): {}",
        b.name,
        status(b.status),
        b.summary
    );
    for p in &b.parts.added {
        let package = p
            .package
            .as_deref()
            .map_or(String::new(), |p| format!(" {p}"));
        let _ = writeln!(
            out,
            "  + part {} {} {}{package} ({})",
            p.designator, p.kind, p.value, p.path
        );
    }
    for p in &b.parts.removed {
        let _ = writeln!(
            out,
            "  - part {} {} {} ({})",
            p.designator, p.kind, p.value, p.path
        );
    }
    for p in &b.parts.changed {
        let _ = writeln!(
            out,
            "  ~ part {} ({}): {}",
            p.designator_head,
            p.path,
            fields(&p.fields)
        );
    }
    for p in &b.parts.renumbered {
        let _ = writeln!(
            out,
            "  ~ part {} is now {} ({})",
            p.designator_base, p.designator_head, p.path
        );
    }
    let net = |n: &CompareNet| {
        if n.pins.is_empty() {
            n.name.clone()
        } else {
            format!("{} [{}]", n.name, pins(&n.pins))
        }
    };
    for n in &b.nets.added {
        let _ = writeln!(out, "  + net {}", net(n));
    }
    for n in &b.nets.removed {
        let _ = writeln!(out, "  - net {}", net(n));
    }
    for n in &b.nets.renamed {
        let _ = writeln!(out, "  ~ net {} is now {}", n.base_name, n.name);
    }
    for n in &b.nets.changed {
        let mut line = format!("  ~ net {}:", n.name);
        if !n.joined.is_empty() {
            let _ = write!(line, " joined {}", pins(&n.joined));
        }
        if !n.left.is_empty() {
            let _ = write!(line, " left {}", pins(&n.left));
        }
        let _ = writeln!(out, "{line}");
    }
    rails(
        out,
        "  ",
        &b.rails.added,
        &b.rails.removed,
        &b.rails.changed,
    );
    instances(out, "module", &b.modules);
    instances(out, "interface", &b.interfaces);
    if let Some(bom) = &b.bom {
        for l in &bom.added {
            let _ = writeln!(out, "  + bom {}", bom_line(l));
        }
        for l in &bom.removed {
            let _ = writeln!(out, "  - bom {}", bom_line(l));
        }
        for l in &bom.changed {
            let _ = writeln!(out, "  ~ bom {}", bom_change(l));
        }
        if let Some(delta) = bom.delta_usd {
            let _ = writeln!(out, "  bill of materials: {delta:+.2} USD per board");
        }
    }
    if let Some(error) = &b.bom_error {
        let _ = writeln!(out, "  bill of materials not compared: {error}");
    }
    if let Some(pcb) = &b.pcb {
        pcb_diff(out, pcb);
    }
    if let Some(error) = &b.pcb_error {
        let _ = writeln!(out, "  PCB not compared: {error}");
    }
    problems_diff(out, &b.problems);
}

fn rails(
    out: &mut String,
    indent: &str,
    added: &[CompareRail],
    removed: &[CompareRail],
    changed: &[CompareRailChange],
) {
    for r in added {
        let _ = writeln!(
            out,
            "{indent}+ rail {} {} ({})",
            r.net,
            range(r.voltage.as_ref()),
            budget(r.budget)
        );
    }
    for r in removed {
        let _ = writeln!(
            out,
            "{indent}- rail {} {}",
            r.net,
            range(r.voltage.as_ref())
        );
    }
    for r in changed {
        let mut line = format!("{indent}~ rail {}", r.net);
        if let Some(was) = &r.base_net {
            let _ = write!(line, " (was {was})");
        }
        range_change(&mut line, "voltage", r.voltage.as_ref());
        range_change(&mut line, "demand", r.demand.as_ref());
        range_change(&mut line, "capacity", r.capacity.as_ref());
        if let Some(b) = &r.budget {
            let _ = write!(line, ", {} -> {}", budget(b.base), budget(b.head));
        }
        if let (Some(a), Some(b)) = (r.headroom_base, r.headroom_head) {
            let _ = write!(line, ", headroom {} -> {}", amps(a), amps(b));
        }
        let _ = writeln!(out, "{line}");
    }
}

fn instances(out: &mut String, what: &str, d: &InstancesDiff) {
    let one = |i: &CompareInstance| format!("{what} {} {}", i.path, i.type_name);
    for i in &d.added {
        let _ = writeln!(out, "  + {}", one(i));
    }
    for i in &d.removed {
        let _ = writeln!(out, "  - {}", one(i));
    }
    for i in &d.changed {
        let _ = writeln!(
            out,
            "  ~ {what} {}: {} -> {}",
            i.path, i.base_type, i.head_type
        );
    }
}

fn bom_line(l: &CompareBomLine) -> String {
    format!(
        "{} x {} {} ({})",
        l.quantity,
        l.kind,
        l.value,
        l.designators.join(" ")
    )
}

fn bom_change(l: &CompareBomChange) -> String {
    format!(
        "{} {}: {} -> {} ({} -> {})",
        l.kind,
        l.value,
        l.quantity_base,
        l.quantity_head,
        l.designators_base.join(" "),
        l.designators_head.join(" ")
    )
}

fn pcb_diff(out: &mut String, p: &PcbDiff) {
    if let Some(o) = &p.outline {
        let _ = writeln!(
            out,
            "  ~ pcb outline {} x {} mm -> {} x {} mm",
            o.base.width, o.base.height, o.head.width, o.head.height
        );
    }
    if let Some(l) = &p.layers {
        let _ = writeln!(
            out,
            "  ~ pcb layers {} -> {}",
            l.base.join(" "),
            l.head.join(" ")
        );
    }
    for m in &p.moved {
        let _ = writeln!(
            out,
            "  ~ pcb {} moved {:.2} mm ({})",
            m.designator_head,
            m.distance_mm,
            fields(&m.fields)
        );
    }
    for (what, areas) in [("keepout", &p.keepouts), ("cutout", &p.cutouts)] {
        for a in areas {
            let _ = writeln!(out, "  ~ pcb {what} {} {}", a.name, status(a.status));
        }
    }
}

fn problem_line(p: &Problem) -> String {
    let severity = match p.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
    };
    let at =
        p.at.as_ref()
            .map_or(String::new(), |l| format!(" ({}:{})", l.file, l.line));
    format!("{severity}: {}{at}", p.message)
}

fn problems_list(out: &mut String, indent: &str, list: &[Problem]) {
    for p in list {
        let _ = writeln!(out, "{indent}{}", problem_line(p));
    }
}

fn problems_diff(out: &mut String, d: &ProblemsDiff) {
    for p in &d.new {
        let _ = writeln!(out, "  + {}", problem_line(p));
    }
    for p in &d.resolved {
        let _ = writeln!(out, "  - {}", problem_line(p));
    }
}

fn names(out: &mut String, what: &str, d: &NamesDiff) {
    for n in &d.added {
        let _ = writeln!(out, "  + {what} {n}");
    }
    for n in &d.removed {
        let _ = writeln!(out, "  - {what} {n}");
    }
}

fn system(out: &mut String, s: &CompareSystem) {
    let _ = writeln!(
        out,
        "\nsystem {} ({}): {}",
        s.name,
        status(s.status),
        s.summary
    );
    instances(out, "board", &s.boards);
    let port = |p: &netsmith_editor::api::CompareSystemPort| format!("port {}.{}", p.board, p.name);
    for p in &s.ports.added {
        let _ = writeln!(out, "  + {}", port(p));
    }
    for p in &s.ports.removed {
        let _ = writeln!(out, "  - {}", port(p));
    }
    for p in &s.ports.changed {
        let _ = writeln!(
            out,
            "  ~ port {}.{}: {}",
            p.board,
            p.name,
            fields(&p.fields)
        );
    }
    names(out, "join", &s.joins);
    for m in &s.mates.added {
        let _ = writeln!(out, "  + mate {} - {}", m.a, m.b);
    }
    for m in &s.mates.removed {
        let _ = writeln!(out, "  - mate {} - {}", m.a, m.b);
    }
    for m in &s.mates.changed {
        let _ = writeln!(out, "  ~ mate {} - {}: {}", m.a, m.b, fields(&m.fields));
    }
    names(out, "ground group", &s.ground_groups);
    rails(
        out,
        "  ",
        &s.rails.added,
        &s.rails.removed,
        &s.rails.changed,
    );
}
