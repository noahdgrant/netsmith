//! The module layer of the physical file (ADR 0048 decision 2, issue #813): a module owns a
//! physical file of its own, which states how its parts stand to one another, and every board
//! that uses the module gets it.
//!
//! **What the owner decided** (ADR 0048, 2026-11-11, from `docs/research/physical-file-syntax-options.md`
//! section 8). The module names its file with a statement in its circuit source (option T3),
//! so a missing file is an error and not geometry lost without a message. A board's physical
//! file may only narrow what the module's states (option V1): a line that would loosen it, or
//! that disagrees with it, is an error that names both lines. A module may declare anchor
//! names (`anchor .dock = .row_a`), so that a rename inside it changes one alias (decision 4).
//!
//! **Readings** (the smallest choice consistent with the ADR where it leaves one open; listed
//! for the owner's review in the pull request of issue #813):
//! - The statement is `physical "radio_dock.physical.nsm"` in the module's body, as section 8
//!   writes it. The file it names is the one beside the module's source by stem, as a board's
//!   is: a source has one physical file, which may hold units for its boards and its modules.
//!   Another name is refused with the name it must be. A board names no file (its file is
//!   found by stem); a part, an interface or a `physical` statement in a physical file is
//!   refused.
//! - The module's unit is `physical for RadioDock { ... }`. A unit for a module that does not
//!   name its file is refused, and so is a module that names a file with no unit for it.
//! - A module's file states only what does not depend on where the module is on the board:
//!   a relation whose `.place` fields cancel along each axis (`.b.place.x - .a.place.x ==
//!   17.5mm +/- 0.05mm`, a `row`), and a part's `.height`, `.height_growth`, `.standoff`, `.pin_protrusion`,
//!   `.model` and `.silk`. A `.place` narrowed with `&=` or `?=` (a position, a turn or a
//!   face in the board's frame) is refused: the board's file places the module. It declares
//!   nothing (no `case { }`, no keepout: a region carried by a part, G2, is not built).
//! - Narrow only (V1) is checked for the offsets between two parts' places (`b - a` in a
//!   window, what the placer reads): a board's offset on the same pair must lie inside the
//!   module's window. A `&=` of a field such as `.height` narrows as every `&=` does, so a
//!   looser one changes nothing and one that disagrees is the empty-intersection error.
//! - The anchors are in the module's physical file, one name each (`anchor .dock = .row_a`);
//!   an anchor names a part or a module inside the module, may not shadow a member, and is
//!   read only by a physical file (the circuit source reaches the part by its path). A board's
//!   file declares none.
//! - A module's file reaches its own parts and, read-only, those of modules inside it, as a
//!   board's does. Whether it may state an overhang stays open (ADR 0045 section 10 (h)): it
//!   states no `.place` of its own, so it cannot today.

use netsmith_lang::Span;
use netsmith_lang::ast::{Anchor, BinOp, Definition, Ident, Item, Path, PhysicalFile};

use crate::checker::{Checker, Res};
use crate::diag::SourceId;
use crate::model::{FieldId, InstId, InstKind, RExpr};

pub(crate) const CASE_IN_MODULE: &str = "a `case { }` section belongs in a board's physical file; a module's states how its parts stand to one another (a relation, a `row`, an `anchor`)";

/// `anchor .dock = .row_a` of a module instance.
#[derive(Clone)]
pub(crate) struct AnchorName {
    pub name: String,
    pub target: InstId,
}

/// An offset between the places of two parts that a physical file states: `b - a` in
/// `lo..hi` (metres), with the module instance whose file states it (`None` for a board's).
#[derive(Clone)]
pub(crate) struct LayerOffset {
    pub a: FieldId,
    pub b: FieldId,
    pub lo: f64,
    pub hi: f64,
    pub layer: Option<InstId>,
    pub source: SourceId,
    pub span: Span,
}

/// The part of a relation's side that is linear in `.place` fields: the coefficient of each
/// and the constant, as a range. `None` for a side that is not.
struct Linear {
    terms: Vec<(FieldId, f64)>,
    k: (f64, f64),
}

impl Linear {
    fn plus(mut self, other: Linear, sign: f64) -> Linear {
        for (f, c) in other.terms {
            match self.terms.iter_mut().find(|t| t.0 == f) {
                Some(t) => t.1 += sign * c,
                None => self.terms.push((f, sign * c)),
            }
        }
        let (lo, hi) = if sign > 0.0 {
            other.k
        } else {
            (-other.k.1, -other.k.0)
        };
        self.k = (self.k.0 + lo, self.k.1 + hi);
        self
    }

    fn scaled(mut self, by: f64) -> Linear {
        for t in &mut self.terms {
            t.1 *= by;
        }
        let (a, b) = (self.k.0 * by, self.k.1 * by);
        self.k = (a.min(b), a.max(b));
        self
    }
}

/// The axis a `.place` field is along: x (`x`, `left`, `right`), y (`y`, `top`, `bottom`) or
/// the turn. `None` for the face, which no relation can make frame free.
fn axis(name: &str) -> Option<usize> {
    match name {
        "x" | "left" | "right" => Some(0),
        "y" | "top" | "bottom" => Some(1),
        "rotation" => Some(2),
        _ => None,
    }
}

/// The basename of a file name: `src/radio_dock.physical.nsm` is `radio_dock.physical.nsm`.
fn basename(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

impl Checker<'_> {
    fn linear(&self, e: &RExpr) -> Option<Linear> {
        let mut fields = Vec::new();
        e.fields(&mut fields);
        if !fields.iter().any(|&f| self.model.is_place_field(f)) {
            return match e {
                RExpr::Const(q) => Some(Linear {
                    terms: Vec::new(),
                    k: (q.lo(), q.hi()),
                }),
                // A constant this does not evaluate (another field) leaves the sum unknown,
                // which does not move the places: it is frame free all the same.
                _ => Some(Linear {
                    terms: Vec::new(),
                    k: (f64::NAN, f64::NAN),
                }),
            };
        }
        let scalar = |e: &RExpr| match e {
            RExpr::Const(q) if q.lo() == q.hi() => Some(q.lo()),
            _ => None,
        };
        match e {
            RExpr::Field(f) => Some(Linear {
                terms: vec![(*f, 1.0)],
                k: (0.0, 0.0),
            }),
            RExpr::Neg(inner) => Some(self.linear(inner)?.scaled(-1.0)),
            RExpr::Bin(BinOp::Add, a, b) => Some(self.linear(a)?.plus(self.linear(b)?, 1.0)),
            RExpr::Bin(BinOp::Sub, a, b) => Some(self.linear(a)?.plus(self.linear(b)?, -1.0)),
            RExpr::Bin(BinOp::Mul, a, b) => match (scalar(a), scalar(b)) {
                (Some(k), _) => Some(self.linear(b)?.scaled(k)),
                (_, Some(k)) => Some(self.linear(a)?.scaled(k)),
                _ => None,
            },
            RExpr::Bin(BinOp::Div, a, b) => match scalar(b) {
                Some(k) if k != 0.0 => Some(self.linear(a)?.scaled(1.0 / k)),
                _ => None,
            },
            _ => None,
        }
    }

    /// `lhs - rhs` as one linear form, or `None`.
    fn difference(&self, lhs: &RExpr, rhs: &RExpr) -> Option<Linear> {
        Some(self.linear(lhs)?.plus(self.linear(rhs)?, -1.0))
    }

    /// The module whose physical file is being read, if one is.
    fn module_layer(&self) -> Option<InstId> {
        self.phys.as_ref().and_then(|p| p.module)
    }

    fn frame_message(&self, field: FieldId) -> String {
        let Some(layer) = self.module_layer() else {
            return String::new();
        };
        // The path as the module's file writes it, so that each use of the module says the
        // same thing once.
        let f = self.model.field(field);
        let owner = self.model.path_below(f.owner, layer);
        let path = if owner.is_empty() {
            format!(".{}", f.name)
        } else {
            format!(".{owner}.{}", f.name)
        };
        let module = self.model.instance(layer).type_name.clone();
        format!(
            "`{path}` is where a part is in the board's frame, and the physical file of module `{module}` states only how its parts stand to one another, the same wherever the board puts the module (`.b.place.x - .a.place.x == 2.54mm`, a `row`); state the place in the physical file of the board that uses `{module}`"
        )
    }

    /// Whether a `&=` or `?=` in a module's physical file may state these fields: none of a
    /// `.place`, which is a position, a turn or a face in the board's frame. Reports it when
    /// not. Anything outside a module's file passes.
    pub(crate) fn module_frame_narrowing(&mut self, fields: &[FieldId], span: Span) -> bool {
        if self.module_layer().is_none() {
            return true;
        }
        if let Some(&f) = fields.iter().find(|&&f| self.model.is_place_field(f)) {
            let message = self.frame_message(f);
            self.error(span, message);
            return false;
        }
        true
    }

    /// Whether a relation in a module's physical file is free of the board's frame: its
    /// `.place` fields are linear and cancel along each axis, so moving or turning nothing
    /// but the whole module leaves it as true as it was. Reports it when not.
    pub(crate) fn module_frame_relation(&mut self, lhs: &RExpr, rhs: &RExpr, span: Span) -> bool {
        if self.module_layer().is_none() {
            return true;
        }
        let mut fields = Vec::new();
        lhs.fields(&mut fields);
        rhs.fields(&mut fields);
        let places: Vec<FieldId> = fields
            .into_iter()
            .filter(|&f| self.model.is_place_field(f))
            .collect();
        let Some(&first) = places.first() else {
            return true;
        };
        let free = self.difference(lhs, rhs).is_some_and(|d| {
            let mut sums = [0.0f64; 3];
            for (f, c) in &d.terms {
                match axis(&self.model.field(*f).name) {
                    Some(a) => sums[a] += c,
                    None => return false,
                }
            }
            sums.iter().all(|s| s.abs() < 1e-9)
        });
        if !free {
            let message = self.frame_message(first);
            self.error(span, message);
        }
        free
    }

    /// Remember a relation of a physical file that is an offset between two parts' places, and
    /// check that a board's (or an outer module's) offset only narrows the window a module's
    /// file states for the same pair (V1). Reports it when not.
    pub(crate) fn note_layer_offset(
        &mut self,
        lhs: &RExpr,
        rhs: &RExpr,
        op: netsmith_lang::ast::CmpOp,
        span: Span,
    ) {
        use netsmith_lang::ast::CmpOp;
        if self.phys.is_none() {
            return;
        }
        let Some(d) = self.difference(lhs, rhs) else {
            return;
        };
        let terms: Vec<(FieldId, f64)> =
            d.terms.into_iter().filter(|t| t.1.abs() > 1e-12).collect();
        let [(f1, c1), (f2, c2)] = terms.as_slice() else {
            return;
        };
        let (b, a) = match (*c1, *c2) {
            (p, m) if (p - 1.0).abs() < 1e-9 && (m + 1.0).abs() < 1e-9 => (*f1, *f2),
            (m, p) if (p - 1.0).abs() < 1e-9 && (m + 1.0).abs() < 1e-9 => (*f2, *f1),
            _ => return,
        };
        // `b - a + k op 0`: `b - a` is in `-k` with the operator's sense.
        let (lo, hi) = match op {
            CmpOp::EqEq => (-d.k.1, -d.k.0),
            CmpOp::Ge => (-d.k.0, f64::INFINITY),
            CmpOp::Le => (f64::NEG_INFINITY, -d.k.1),
            CmpOp::Narrow | CmpOp::Default => return,
        };
        let same_axis = axis(&self.model.field(a).name).is_some()
            && axis(&self.model.field(a).name) == axis(&self.model.field(b).name);
        if !same_axis || lo.is_nan() || hi.is_nan() {
            return;
        }
        let layer = self.module_layer();
        let source = self.cur_source();
        let inner: Vec<LayerOffset> = self
            .layer_offsets
            .iter()
            .filter(|o| o.layer.is_some() && o.layer != layer)
            .filter(|o| (o.a, o.b) == (a, b) || (o.a, o.b) == (b, a))
            .cloned()
            .collect();
        for o in inner {
            // The module's window for `b - a`.
            let (mlo, mhi) = if (o.a, o.b) == (a, b) {
                (o.lo, o.hi)
            } else {
                (-o.hi, -o.lo)
            };
            let tol = 1e-12;
            if lo >= mlo - tol && hi <= mhi + tol {
                continue;
            }
            let disjoint = hi < mlo - tol || lo > mhi + tol;
            let module = self
                .model
                .instance(o.layer.expect("an inner layer is a module's"))
                .type_name
                .clone();
            let (file, line) = self.line_in(o.source, o.span);
            let theirs = self.statement_text(o.source, o.span);
            let ours = self.statement_text(source, span);
            let field = |f: FieldId| self.model.path_of_field(f);
            let what = if disjoint {
                "disagrees with"
            } else {
                "would loosen"
            };
            let message = format!(
                "`{ours}` {what} the physical file of module `{module}`, which states `{theirs}` ({file}:{line}): it leaves `{} - {}` {}, and the module's file {}; a physical file may only narrow what a module's states (ADR 0048), so tighten the window to lie inside the module's, or change the module",
                field(b),
                field(a),
                window(lo, hi),
                window(mlo, mhi)
            );
            self.error(span, message);
            return;
        }
        self.layer_offsets.push(LayerOffset {
            a,
            b,
            lo,
            hi,
            layer,
            source,
            span,
        });
    }

    /// A statement's text with its white space collapsed, for a message.
    fn statement_text(&self, source: SourceId, span: Span) -> String {
        let text = self.sources.get(source).text.as_str();
        let end = netsmith_lang::floor_boundary(text, span.end);
        let start = netsmith_lang::floor_boundary(text, span.start).min(end);
        text[start..end]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Whether `module` names the physical file `file` (the file a unit `physical for` it is
    /// in). Reports it when not.
    pub(crate) fn module_names_file(
        &mut self,
        source: SourceId,
        module: &Definition,
        file: &str,
        at: Span,
    ) -> bool {
        let named = module
            .items
            .iter()
            .any(|i| matches!(i, Item::PhysicalFile(f) if f.file == basename(file)));
        if !named {
            self.report(
                source,
                at,
                format!(
                    "`physical for {name}` names a module, and `module {name}` does not name this file: add `physical \"{}\"` to it, so that every board that uses `{name}` gets the file and a missing file is an error (ADR 0048)",
                    basename(file),
                    name = module.name.name
                ),
            );
        }
        named
    }

    /// Check the `physical "..."` statements of the definitions of module `id`, read from the
    /// file `name`: only a module has one, it names the file beside its source, once, and that
    /// file exists and has a unit for the module.
    pub(crate) fn check_physical_statements(&mut self, id: usize, name: &str) {
        let source = self.modules[id].source;
        let defs = self.modules[id].defs.clone();
        let expected = crate::physical_file_name(name);
        let expected = basename(&expected).to_string();
        let loader = self.loader_ref();
        let has_file = loader.physical_of(name).is_some();
        for (index, def) in defs.iter().enumerate() {
            let statements: Vec<&PhysicalFile> = def
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::PhysicalFile(f) => Some(f),
                    _ => None,
                })
                .collect();
            for (k, f) in statements.iter().enumerate() {
                if def.kind != netsmith_lang::ast::DefKind::Module {
                    let what = if def.kind == netsmith_lang::ast::DefKind::Board {
                        format!(
                            "a board's physical file is found beside its source by stem, `{expected}`, and the board names none"
                        )
                    } else {
                        "only a module names its physical file".to_string()
                    };
                    self.report(
                        source,
                        f.span,
                        format!(
                            "`physical \"{}\"` is in {} `{}`: {what}; remove it",
                            f.file,
                            def.kind.keyword(),
                            def.name.name
                        ),
                    );
                    continue;
                }
                if k > 0 {
                    self.report(
                        source,
                        f.span,
                        format!(
                            "`module {}` names its physical file twice; keep one `physical \"{expected}\"`",
                            def.name.name
                        ),
                    );
                    continue;
                }
                if f.file != expected {
                    self.report(
                        source,
                        f.file_span,
                        format!(
                            "`physical \"{}\"`: a module's physical file is the one beside its source, named by its stem, `{expected}`; name that file",
                            f.file
                        ),
                    );
                    continue;
                }
                if !has_file {
                    self.report(
                        source,
                        f.span,
                        format!(
                            "`module {}` names its physical file `{expected}`, and there is no such file beside `{name}`; add the file with `physical for {} {{ ... }}`, or remove this line",
                            def.name.name, def.name.name
                        ),
                    );
                    continue;
                }
                if !self.has_attached((id, index)) {
                    self.report(
                        source,
                        f.span,
                        format!(
                            "`module {}` names its physical file `{expected}`, which has no `physical for {}` unit; add one",
                            def.name.name, def.name.name
                        ),
                    );
                }
            }
        }
    }

    /// A `physical "..."` statement met while statements are read: checked when the file was
    /// loaded, unless it stands in a physical file.
    pub(crate) fn physical_file_here(&mut self, f: &PhysicalFile) {
        if self.phys.is_some() {
            self.error(
                f.span,
                "`physical \"...\"` belongs in a module's circuit source, where it names the module's physical file; a physical file names no other",
            );
        }
    }

    /// An `anchor` met while statements are read: declared before (`declare_anchors`) in a
    /// module's physical file, refused anywhere else.
    pub(crate) fn anchor_here(&mut self, a: &Anchor) {
        if self.module_layer().is_some() {
            return;
        }
        let message = if self.phys.is_some() {
            "an `anchor` belongs in a module's physical file, where it names a part inside the module; a board's physical file reaches the part by its path or by the module's anchor"
        } else {
            "an `anchor` belongs in a module's physical file (`physical for Module { anchor .dock = .row_a }`, ADR 0048); the circuit source reaches a part by its path"
        };
        self.error(a.span, message);
    }

    /// Declare the anchors of a module's physical unit, before its statements are read.
    pub(crate) fn declare_anchors(&mut self, items: &[Item]) {
        let module = self.current();
        for item in items {
            let Item::Anchor(a) = item else { continue };
            let name = &a.name.name;
            if self.model.member(module, name).is_some() {
                self.error(
                    a.name.span,
                    format!(
                        "`anchor .{name}` would hide the member `.{name}` of `{}`; an anchor is a name of its own",
                        self.model.instance(module).type_name
                    ),
                );
                continue;
            }
            if self
                .anchors
                .get(&module)
                .is_some_and(|list| list.iter().any(|x| &x.name == name))
            {
                self.error(
                    a.name.span,
                    format!("`anchor .{name}` is declared twice; keep one"),
                );
                continue;
            }
            if !a.target.leading_dot {
                self.error(
                    a.target.span,
                    format!(
                        "an anchor names a part inside the module, with a leading dot: write `.{}`",
                        a.target.to_source()
                    ),
                );
                continue;
            }
            let target = match self.resolve_path(&a.target) {
                Res::Inst(i)
                    if matches!(
                        self.model.instance(i).kind,
                        InstKind::Part | InstKind::Module
                    ) && i != module =>
                {
                    i
                }
                Res::Inst(_) | Res::Field(_) => {
                    self.error(
                        a.target.span,
                        format!(
                            "`anchor .{name} = {}` names something that is not a part or a module inside `{}`; an anchor names a part or a module",
                            a.target.to_source(),
                            self.model.instance(module).type_name
                        ),
                    );
                    continue;
                }
                Res::Poison | Res::Bad => continue,
            };
            self.anchors.entry(module).or_default().push(AnchorName {
                name: name.clone(),
                target,
            });
        }
    }

    /// What anchor `name` of instance `owner` names, when a physical file is being read.
    pub(crate) fn anchor_target(&self, owner: InstId, name: &str) -> Option<InstId> {
        self.phys.as_ref()?;
        self.anchors
            .get(&owner)?
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.target)
    }

    /// The message for a path of a physical file that names no member of a module: a rename
    /// inside the module breaks it (ADR 0048, "The cost").
    pub(crate) fn broken_path(&self, module: InstId, seg: &Ident, path: &Path) -> String {
        let inst = self.model.instance(module);
        let ty = inst.type_name.clone();
        let mut names: Vec<String> = inst.children.iter().map(|(n, _)| n.clone()).collect();
        let anchors = self.anchors.get(&module).cloned().unwrap_or_default();
        names.extend(anchors.iter().map(|a| a.name.clone()));
        let near = crate::lower::closest(&seg.name, &names)
            .map(|n| format!("; did you mean `.{n}`?"))
            .unwrap_or_default();
        let anchor_note = if anchors.is_empty() {
            format!(
                "an `anchor .name = .part` line in the physical file of `{ty}` gives a part a name that a rename inside the module changes in one place"
            )
        } else {
            let list: Vec<String> = anchors.iter().map(|a| format!("`.{}`", a.name)).collect();
            format!("`{ty}` has the anchors {}", list.join(", "))
        };
        format!(
            "`{}` names nothing: module `{ty}` has no member `{}`. A physical file reaches inside a module by the path of the part, so a rename inside `{ty}` breaks this path; {anchor_note}{near}",
            path.to_source(),
            seg.name
        )
    }
}

/// A window for a message, in millimetres.
fn window(lo: f64, hi: f64) -> String {
    let mm = |v: f64| {
        let v = (v * 1e3 * 1e6).round() / 1e6;
        format!("{v}mm")
    };
    match (lo.is_finite(), hi.is_finite()) {
        (true, true) if (hi - lo).abs() < 1e-15 => format!("at {}", mm(lo)),
        (true, true) => format!("between {} and {}", mm(lo), mm(hi)),
        (true, false) => format!("at least {}", mm(lo)),
        (false, true) => format!("at most {}", mm(hi)),
        (false, false) => "anything".to_string(),
    }
}
