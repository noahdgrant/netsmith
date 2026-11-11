//! The `row` statement of a physical file (ADR 0048, "Syntax decisions" 10): a numbered run of
//! parts on one line, each neighbour at a pitch. It is lowered here to the relations the
//! equation form states, one pitch and one alignment for each pair of neighbours, each with
//! the span of the row statement, so the solver, the placer and the check meet nothing new.
//!
//! **Decisions** (ADR 0048 "Syntax decisions" 10 and 11, owner, 2026-11-04, issue #81). A
//! numbered run of parts on a pitch is one statement, spelled as a range of names: `row .pads.tp1
//! .. .pads.tp12 along x pitch 2.54mm +/- 0.05mm`. The first and the last name the run: the same
//! path prefix, a number at the end of the last segment, every number between them present as a
//! part. It puts the parts on one common line (the other axis equal) and each neighbour at the
//! pitch along `x` or `y`, within the window; a pitch without one is exact within the placer's
//! tolerance. It is physical file only. A number at the end of a path segment sorts numerically
//! wherever parts are numbered (`tp10` after `tp9`, so `.pads.tp2` has designator `TP2`); before,
//! designators followed the paths as text and the labels disagreed with the physical file.
//!
//! **Readings of the statement and the order**, all word for word from ADR 0048 (accepted as built
//! by the owner, 2026-11-11):
//! - "The words `along` and `pitch` are read as words only inside a `row` statement, and `row`
//!   begins one only when a path with a leading dot follows, so `row`, `along` and `pitch` stay
//!   usable as names."
//! - "Each part is tied to its neighbour (the pitch of tp(n+1) from tp(n), and the line of tp(n+1)
//!   the line of tp(n)), not to the first part, so a finding names the two neighbours; with a
//!   window the error may add up along a row of N parts (N-1 windows), as it does for the
//!   equations."
//! - "A negative pitch runs the row toward smaller coordinates."
//! - "A number written with leading zeros keeps the width of the first (`tp01 .. tp12`); a last
//!   number zero-padded when the first is not is refused. A run of one part is refused."
//! - "A part of a row must be a part (not a module, a pin or a value) and have a `.place` (every
//!   part has one, so the check cannot fail today)."
//! - "The checker reports the first part at fault, the two ends first, then the numbers between
//!   them in order, and one message for a pitch that is not a length."
//! - "The statement's text is the finding's source line for each of its relations; the
//!   relation's own name (`pads.tp7.place.x - pads.tp6.place.x`) is what a finding quotes."
//! - "Only a number at the end of a path segment counts as a number (`tp2a` is text), segments
//!   are compared in turn and a segment without a number sorts before one with it; two spellings
//!   of one number (`tp02`, `tp2`) are ordered by the number of digits so that the order is
//!   total."
//!
//! Not built, and not planned until a board needs it: a start offset (`from`), a row of a
//! module's parts addressed by the module, and a row whose parts are not one numbered run. The
//! editor's "pin here" does not write a row (issue #56).

use netsmith_lang::ast::{Axis, BinOp, CmpOp, Constraint, Expr, Ident, Path, Row};

use crate::checker::{Checker, Res};
use crate::lower::VTy;
use crate::model::{InstId, InstKind, Member};
use netsmith_quantity::Dimension;

/// A name that ends in a number, split: `tp12` is `("tp", "12")`.
fn split_number(name: &str) -> Option<(&str, &str)> {
    let stem_len = name.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    (stem_len < name.len()).then(|| name.split_at(stem_len))
}

/// The names of the run from `first` to `last` (digits as written), in order. The digits of
/// the first name give the width: `tp01 .. tp12` is `tp01`, `tp02`, ..., `tp12`.
fn run_names(stem: &str, first: &str, last: &str) -> Option<Vec<String>> {
    let (a, b): (u64, u64) = (first.parse().ok()?, last.parse().ok()?);
    let width = if first.starts_with('0') {
        first.len()
    } else {
        0
    };
    Some((a..=b).map(|n| format!("{stem}{n:0width$}")).collect())
}

impl Checker<'_> {
    /// Lower a `row` statement to relations. Every refusal names the statement's text and
    /// the part or number at fault.
    pub(crate) fn row(&mut self, r: &Row) {
        let text = format!("{} .. {}", r.first.to_source(), r.last.to_source());
        if self.phys.is_none() {
            self.error(
                r.span,
                "a `row` statement belongs in a physical file (`physical for Board { row ... }`, ADR 0048); the circuit source holds no placement",
            );
            return;
        }
        let Some((prefix, names)) = self.row_names(r, &text) else {
            return;
        };
        if self.row_parts(r, &text, &prefix, &names).is_none() {
            return;
        }
        // The pitch is lowered once, so a pitch that is not a length is one message.
        let Some(pitch) = self.lower(&r.pitch) else {
            return;
        };
        if pitch.ty != VTy::Q(Dimension::LENGTH) {
            self.error(
                r.pitch.span(),
                format!(
                    "the pitch of `row {text}` is a length such as `2.54mm`, and `{}` is not",
                    r.pitch.to_source()
                ),
            );
            return;
        }
        let (along, across) = match r.axis {
            Axis::X => ("x", "y"),
            Axis::Y => ("y", "x"),
        };
        for pair in names.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            let field =
                |name: &str, f: &str| Expr::Path(synthetic_path(r, &prefix, name, &["place", f]));
            // `b.place.x - a.place.x == pitch` and `b.place.y == a.place.y`.
            let pitch_line = Constraint {
                lhs: Expr::Binary {
                    op: BinOp::Sub,
                    lhs: Box::new(field(b, along)),
                    rhs: Box::new(field(a, along)),
                    span: r.span,
                },
                op: CmpOp::EqEq,
                rhs: r.pitch.clone(),
                at: None,
                span: r.span,
            };
            let line = Constraint {
                lhs: field(b, across),
                op: CmpOp::EqEq,
                rhs: field(a, across),
                at: None,
                span: r.span,
            };
            self.constraint(&pitch_line);
            self.constraint(&line);
        }
    }

    /// The prefix (the segments before the number's name) and the names of the run, or
    /// `None` after reporting why the two ends do not name one.
    fn row_names(&mut self, r: &Row, text: &str) -> Option<(Vec<Ident>, Vec<String>)> {
        for p in [&r.first, &r.last] {
            if !p.leading_dot {
                self.error(
                    p.span,
                    format!(
                        "`row {text}` names its parts from the board, with a leading dot: write `.{}`",
                        p.to_source()
                    ),
                );
                return None;
            }
        }
        let tail = |p: &Path| p.segments.last().expect("a path has a segment").clone();
        let (first, last) = (tail(&r.first), tail(&r.last));
        let (Some((stem_a, num_a)), Some((stem_b, num_b))) =
            (split_number(&first.name), split_number(&last.name))
        else {
            let (bad, p) = if split_number(&first.name).is_none() {
                (&first, &r.first)
            } else {
                (&last, &r.last)
            };
            self.error(
                p.span,
                format!(
                    "`row {text}` needs a numbered run of parts, and `{}` does not end in a number (`tp1 .. tp12`)",
                    bad.name
                ),
            );
            return None;
        };
        let prefix = |p: &Path| p.segments[..p.segments.len() - 1].to_vec();
        let (pa, pb) = (prefix(&r.first), prefix(&r.last));
        let path_of = |segs: &[Ident]| {
            segs.iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join(".")
        };
        if path_of(&pa) != path_of(&pb) {
            let shown = |segs: &[Ident]| {
                if segs.is_empty() {
                    "the board".to_string()
                } else {
                    format!("`{}`", path_of(segs))
                }
            };
            self.error(
                r.span,
                format!(
                    "`row {text}` joins parts of different modules: `{}` is in {} and `{}` is in {}; a row is the run of one module's parts",
                    first.name,
                    shown(&pa),
                    last.name,
                    shown(&pb)
                ),
            );
            return None;
        }
        if stem_a != stem_b {
            self.error(
                r.span,
                format!(
                    "`row {text}` joins `{}` and `{}`, which do not share a name before the number; a row is a run such as `tp1 .. tp12`",
                    first.name, last.name
                ),
            );
            return None;
        }
        let (Ok(a), Ok(b)) = (num_a.parse::<u64>(), num_b.parse::<u64>()) else {
            self.error(
                r.span,
                format!("`row {text}`: a number of the run is too large to count"),
            );
            return None;
        };
        if a > b {
            self.error(
                r.span,
                format!(
                    "`row {text}` starts at {a} and ends at {b}; write the lower number first (`{stem_a}{b} .. {stem_a}{a}`)"
                ),
            );
            return None;
        }
        if a == b {
            self.error(
                r.span,
                format!(
                    "`row {text}` is one part; a row is two or more parts, so a pitch has something to hold"
                ),
            );
            return None;
        }
        if num_b.starts_with('0') && !num_a.starts_with('0') {
            self.error(
                r.span,
                format!(
                    "`row {text}` zero-pads the last number and not the first; write both as `{stem_a}{num_a:0>w$}`, or neither",
                    w = num_b.len()
                ),
            );
            return None;
        }
        let names = run_names(stem_a, num_a, num_b).expect("both are numbers");
        Some((pa, names))
    }

    /// Find every part of the run, and say why one is no part to put in a row. Returns the
    /// parts, in order, once all of them are found and are parts with a `.place`.
    fn row_parts(
        &mut self,
        r: &Row,
        text: &str,
        prefix: &[Ident],
        names: &[String],
    ) -> Option<Vec<InstId>> {
        let parent = if prefix.is_empty() {
            self.current()
        } else {
            let path = Path {
                leading_dot: true,
                segments: prefix.to_vec(),
                span: r.first.span,
            };
            match self.resolve_path(&path) {
                Res::Inst(i) => i,
                // Reported by `resolve_path`, or an earlier error poisoned the name.
                _ => return None,
            }
        };
        let owner = if prefix.is_empty() {
            "the board".to_string()
        } else {
            format!(
                "`{}`",
                prefix
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(".")
            )
        };
        // The two ends the statement names are looked at first, then the numbers between
        // them in order; the first one at fault is the one reported.
        let last = names.len() - 1;
        let mut order: Vec<usize> = vec![0, last];
        order.extend(1..last);
        let mut found: Vec<Option<InstId>> = vec![None; names.len()];
        for index in order {
            let name = &names[index];
            let end = index == 0 || index == last;
            let at = match index {
                0 => r.first.span,
                i if i == last => r.last.span,
                _ => r.span,
            };
            match self.model.member(parent, name) {
                None => {
                    let what = if end {
                        format!(
                            "`row {text}` names `{name}`, and {owner} has no member of that name"
                        )
                    } else {
                        format!(
                            "`row {text}` has a gap: {owner} has no member `{name}`, and a row is every number from the first to the last"
                        )
                    };
                    self.error(at, what);
                    return None;
                }
                Some(Member::Inst(i)) => {
                    let kind = self.model.instance(i).kind;
                    if kind != InstKind::Part {
                        self.error(
                            at,
                            format!(
                                "`row {text}` includes `{name}`, which is a {}; a row is of parts (a relation is between the places of parts, not of a module's box)",
                                kind.word()
                            ),
                        );
                        return None;
                    }
                    if !matches!(self.model.member(i, "place"), Some(Member::Inst(_))) {
                        self.error(
                            at,
                            format!(
                                "`row {text}` includes `{name}`, which has no `.place` to hold"
                            ),
                        );
                        return None;
                    }
                    found[index] = Some(i);
                }
                Some(_) => {
                    self.error(
                        at,
                        format!("`row {text}` includes `{name}`, which is a value and not a part"),
                    );
                    return None;
                }
            }
        }
        found.into_iter().collect()
    }
}

/// `.<prefix>.<name>.<rest>` with the span of the whole row statement.
fn synthetic_path(r: &Row, prefix: &[Ident], name: &str, rest: &[&str]) -> Path {
    let at = |s: &str, quoted: bool| Ident {
        name: s.to_string(),
        quoted,
        span: r.span,
    };
    let mut segments: Vec<Ident> = prefix.iter().map(|s| at(&s.name, s.quoted)).collect();
    let quoted = r.last.segments.last().is_some_and(|s| s.quoted);
    segments.push(at(name, quoted));
    segments.extend(rest.iter().map(|s| at(s, false)));
    Path {
        leading_dot: true,
        segments,
        span: r.span,
    }
}
