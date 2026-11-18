//! Named corners of a board (issue #1256): each bound that reads a capacitor's effective
//! capacitance or ESR is judged again at every corner the board states.
//!
//! A board states its corners as members of the type `std::env::Corner`, an interface with one
//! temperature (`.temperature`) and a point of life (`.life`, `Life::New` or `Life::End`):
//!
//! ```text
//! .new: Corner { .temperature &= 25degC; .life &= Life::New }
//! .eol_cold: Corner { .temperature &= -20degC; .life &= Life::End }
//! ```
//!
//! # What a corner changes
//!
//! A part with a member `effective_capacitance` or `esr` (the standard `Capacitor` and
//! `CapacitorBank`, or a project's own part with the same members) has, at a corner:
//!
//! - **effective capacitance**: its value (the row's, after the DC bias and the tolerance), times
//!   its `capacitance_at_end_of_life` at `Life::End`. At `Life::New` it is as it is.
//! - **ESR**: the row's curve file over `temperature` (`curves/capacitor/<row>.csv`, issue #1260)
//!   read at the corner's temperature, times its `esr_growth` at `Life::End`. A row with no curve
//!   gives its single figure, which its datasheet states at 25 degC, so the figure stands at a
//!   corner of 25 degC only. An ESR the design states itself speaks for every temperature.
//!
//! Everything worked out from those values is worked out again from the corner's (an equation
//! `.f == expr`, a narrowing `.f &= expr` or a default the checker made one, and the joins
//! between fields): a bank's `.total_effective_capacitance`, a buck stage's `.output_esr` and
//! `.output_ripple`. Each bound (`<=`, `>=`) that reads any of them is then judged with the
//! corner's values in the worst case, as the check of the row's own figures is, and a failure
//! is an error that names the corner and the values it read. The check of the row's figures is
//! made as before; a corner is a check more, never one less.
//!
//! # What is not judged
//!
//! A value a corner needs that nothing states (a row with no `capacitance_at_end_of_life` at an
//! end-of-life corner, no `esr_growth`, or no curve over temperature at a corner away from
//! 25 degC) leaves every bound that reads it unjudged at that corner: the pick goes on, and a
//! note names the corner, the part, the value and the bounds (a check that could not be made is
//! never a pass). Equations whose value a corner moves are not judged against a window stated on
//! the same field: only bounds are.
//!
//! The pick takes the first row (and the fewest of a bank) for which the bounds hold at every
//! corner as well, because the trial of a candidate judges them the same way.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::Solver;
use super::Verdict;
use crate::curve::{self, Curve};
use crate::eval::{Val, eval_with, fields_in};
use netsmith_check::{
    Domain, FieldId, FieldKind, InstId, InstKind, Member, Model, RExpr, Relation, SourceId,
};
use netsmith_lang::Span;
use netsmith_lang::ast::CmpOp;
use netsmith_quantity::{Dimension, Quantity};

/// The temperature a datasheet states its single figure at, 25 degC, in kelvin.
const DATASHEET: f64 = netsmith_check::environment::DATASHEET_TEMPERATURE;

/// One corner a board states.
#[derive(Debug, Clone)]
pub(super) struct Corner {
    /// The member's name: `eol_cold`.
    pub name: String,
    pub board: InstId,
    /// Its temperature, in kelvin.
    pub kelvin: f64,
    /// Whether it is at the end of life (`Life::End`).
    pub end: bool,
}

impl Corner {
    /// `` `eol_cold` (253.15 K (-20degC), at the end of life) ``.
    pub fn describe(&self) -> String {
        format!(
            "`{}` ({}, {})",
            self.name,
            netsmith_check::environment::show(self.kelvin),
            if self.end {
                "at the end of life"
            } else {
                "new"
            }
        )
    }
}

/// Which value of a capacitor a field is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Base {
    Effective,
    Esr,
}

/// How a field that a corner moves is worked out from the others.
#[derive(Debug, Clone)]
enum Def {
    /// `.f == expr` or `.f &= expr`.
    Expr(RExpr),
    /// Joined to another field.
    Same(FieldId),
}

/// The corners of a design and what they move, made once for a solve.
#[derive(Debug, Default)]
pub(super) struct Corners {
    pub corners: Vec<Corner>,
    /// The capacitors' own values a corner sets, with their part.
    base: HashMap<FieldId, (InstId, Base)>,
    /// The fields worked out from them, each with how.
    defs: HashMap<FieldId, Vec<Def>>,
}

/// Why a value at a corner is not known.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Miss {
    /// The part states nothing for the member a corner needs (`capacitance_at_end_of_life`).
    Member { part: InstId, member: &'static str },
    /// The part's ESR is a single figure at 25 degC, and the corner is elsewhere.
    NoCurve { part: InstId },
    /// The part's curve does not reach the corner's temperature.
    Beyond { part: InstId, file: String },
    /// A value the bound reads has none at the corner (still unknown, or in a circle).
    Unknown,
}

/// What a bound says at a corner.
pub(super) enum AtCorner {
    Pass,
    Fail { message: String, margin: f64 },
    Unknown(Miss),
}

impl Corners {
    /// The corners `model` states and what they move, with the statements of a corner that are
    /// refused: one outside a board, one with no single temperature or no life, one outside the
    /// board's environment.
    pub fn of(model: &Model, relations: usize) -> (Self, Vec<(SourceId, Span, String)>) {
        let mut out = Corners::default();
        let mut refused = Vec::new();
        for (id, inst) in model.instances() {
            if inst.kind != InstKind::Interface || inst.type_name != "Corner" {
                continue;
            }
            let field = |name: &str| match model.member(id, name)? {
                Member::Field(f) => Some(f),
                _ => None,
            };
            let (Some(temperature), Some(life)) = (field("temperature"), field("life")) else {
                continue;
            };
            if model.field(temperature).kind != FieldKind::Quantity(Dimension::TEMPERATURE)
                || !matches!(model.field(life).kind, FieldKind::Enum(_))
            {
                continue;
            }
            let (source, span) = inst.at();
            let path = model.path_of(id);
            let board = match inst.parent {
                Some(p) if model.instance(p).kind == InstKind::Board => p,
                _ => {
                    refused.push((
                        source,
                        span,
                        format!(
                            "`{path}` is a corner (`Corner`), and only a board states corners: move it to the board that uses this"
                        ),
                    ));
                    continue;
                }
            };
            let kelvin = match &model.field(temperature).domain {
                Domain::Quantity(q) if q.lo() == q.hi() => q.lo(),
                Domain::Quantity(q) => {
                    refused.push((
                        source,
                        span,
                        format!(
                            "the corner `{path}` has the temperature {q}, a range: a corner is one temperature (`.{path}.temperature &= -20degC`), and a board states a corner for each end it means"
                        ),
                    ));
                    continue;
                }
                _ => {
                    refused.push((
                        source,
                        span,
                        format!(
                            "the corner `{path}` states no temperature: state one (`.{path}.temperature &= -20degC`)"
                        ),
                    ));
                    continue;
                }
            };
            let end = match &model.field(life).domain {
                Domain::Text(t) if t == "End" => true,
                Domain::Text(t) if t == "New" => false,
                _ => {
                    refused.push((
                        source,
                        span,
                        format!(
                            "the corner `{path}` states no life: state `.{path}.life &= Life::New` or `Life::End`"
                        ),
                    ));
                    continue;
                }
            };
            if let Some(Member::Inst(env)) = model.member(board, "environment")
                && let Some(Member::Field(t)) = model.member(env, "temperature")
                && let Domain::Quantity(range) = &model.field(t).domain
                && range.dim() == Dimension::TEMPERATURE
                && (kelvin < range.lo() - 1e-9 || kelvin > range.hi() + 1e-9)
            {
                refused.push((
                    source,
                    span,
                    format!(
                        "the corner `{path}` is at {}, outside the board's environment (`environment.temperature` {}): a corner lies inside the range the board works in, so move the corner or widen the environment",
                        netsmith_check::environment::show(kelvin),
                        netsmith_check::environment::show_range(range.lo(), range.hi())
                    ),
                ));
                continue;
            }
            out.corners.push(Corner {
                name: inst.name.clone(),
                board,
                kelvin,
                end,
            });
        }
        if out.corners.is_empty() {
            return (out, refused);
        }
        out.corners.sort_by(|a, b| a.name.cmp(&b.name));
        let boards: HashSet<InstId> = out.corners.iter().map(|c| c.board).collect();
        for (id, inst) in model.instances() {
            if inst.kind != InstKind::Part
                || !model.board_of(id).is_some_and(|b| boards.contains(&b))
            {
                continue;
            }
            for (name, base, dim) in [
                (
                    "effective_capacitance",
                    Base::Effective,
                    Dimension::CAPACITANCE,
                ),
                ("esr", Base::Esr, Dimension::RESISTANCE),
            ] {
                if let Some(Member::Field(f)) = model.member(id, name)
                    && model.field(f).kind == FieldKind::Quantity(dim)
                {
                    out.base.insert(f, (id, base));
                }
            }
        }
        // What is worked out from them: the design's own equations and narrowings, and the joins.
        let mut moved: HashSet<FieldId> = out.base.keys().copied().collect();
        loop {
            let before = moved.len();
            for r in &model.relations()[..relations] {
                let (CmpOp::EqEq | CmpOp::Narrow | CmpOp::Default, RExpr::Field(f)) =
                    (r.op, &r.lhs)
                else {
                    continue;
                };
                if out.base.contains_key(f) {
                    continue;
                }
                let mut reads = Vec::new();
                fields_in(&r.rhs, &mut reads);
                if reads.iter().any(|g| moved.contains(g)) {
                    let defs = out.defs.entry(*f).or_default();
                    if !defs
                        .iter()
                        .any(|d| matches!(d, Def::Expr(e) if *e == r.rhs))
                    {
                        defs.push(Def::Expr(r.rhs.clone()));
                    }
                    moved.insert(*f);
                }
            }
            for &(a, b) in model.equalities() {
                for (from, to) in [(a, b), (b, a)] {
                    if moved.contains(&from) && !out.base.contains_key(&to) {
                        let defs = out.defs.entry(to).or_default();
                        if !defs.iter().any(|d| matches!(d, Def::Same(g) if *g == from)) {
                            defs.push(Def::Same(from));
                        }
                        moved.insert(to);
                    }
                }
            }
            if moved.len() == before {
                break;
            }
        }
        (out, refused)
    }

    /// Whether a corner can move a field.
    fn moves(&self, f: FieldId) -> bool {
        self.base.contains_key(&f) || self.defs.contains_key(&f)
    }
}

/// The values worked out at one corner while one bound is judged.
struct At<'c> {
    corner: &'c Corner,
    memo: HashMap<FieldId, Result<Quantity, Miss>>,
    visiting: HashSet<FieldId>,
    /// The capacitors' values the corner set, in the order they were read.
    read: Vec<(FieldId, Quantity)>,
    /// Whether a value of a part on the corner's board was moved.
    touched: bool,
}

impl Solver<'_> {
    /// `verify`, and then each corner the board states: a bound that holds for the row's figures
    /// and fails at a corner fails, with a message that names the corner (issue #1256).
    pub(super) fn verify_cornered(&self, r: &Relation) -> Verdict {
        let verdict = self.verify(r);
        if !matches!(verdict, Verdict::Pass) || self.corners.corners.is_empty() {
            return verdict;
        }
        for corner in &self.corners.corners {
            if let AtCorner::Fail { message, margin } = self.at_corner(r, corner) {
                return Verdict::Fail { message, margin };
            }
        }
        verdict
    }

    /// What a bound says at one corner. A bound that reads nothing a corner moves, or that is
    /// not a bound, passes.
    pub(super) fn at_corner(&self, r: &Relation, corner: &Corner) -> AtCorner {
        if !matches!(r.op, CmpOp::Ge | CmpOp::Le) {
            return AtCorner::Pass;
        }
        let mut fields = Vec::new();
        fields_in(&r.lhs, &mut fields);
        fields_in(&r.rhs, &mut fields);
        if !fields.iter().any(|&f| self.corners.moves(f)) {
            return AtCorner::Pass;
        }
        let mut at = At {
            corner,
            memo: HashMap::new(),
            visiting: HashSet::new(),
            read: Vec::new(),
            touched: false,
        };
        let mut values = Vec::new();
        for f in fields {
            if self.corners.moves(f) && !values.iter().any(|(g, _)| *g == f) {
                match self.corner_value(f, &mut at) {
                    Ok(q) => values.push((f, q)),
                    Err(miss) => return AtCorner::Unknown(miss),
                }
            }
        }
        if !at.touched {
            return AtCorner::Pass;
        }
        let (Ok(Val::Q(l)), Ok(Val::Q(v))) = (
            eval_with(&self.model, &r.lhs, &values),
            eval_with(&self.model, &r.rhs, &values),
        ) else {
            return AtCorner::Unknown(Miss::Unknown);
        };
        let (ok, word, side, gap) = if r.op == CmpOp::Ge {
            (l.lo() >= v.hi(), "at least", "above", v.hi() - l.lo())
        } else {
            (l.hi() <= v.lo(), "at most", "below", l.hi() - v.lo())
        };
        if ok {
            return AtCorner::Pass;
        }
        let scale = [l.lo(), l.hi(), v.lo(), v.hi()]
            .iter()
            .fold(1e-30_f64, |m, x| m.max(x.abs()));
        let show = |e: &RExpr| self.model.show_expr(e);
        let there: Vec<String> = at
            .read
            .iter()
            .map(|(f, q)| format!("`{}` is {q}", self.model.path_of_field(*f)))
            .collect();
        AtCorner::Fail {
            message: format!(
                "at the corner {}, {} must always be {word} {}, but {l} is not always {side} {v} (there {})",
                corner.describe(),
                show(&r.lhs),
                show(&r.rhs),
                there.join(" and ")
            ),
            margin: gap / scale,
        }
    }

    /// The value of a field a corner moves, at the corner.
    fn corner_value(&self, f: FieldId, at: &mut At) -> Result<Quantity, Miss> {
        if let Some(known) = at.memo.get(&f) {
            return known.clone();
        }
        if !at.visiting.insert(f) {
            return Err(Miss::Unknown);
        }
        let value = if let Some(&(part, base)) = self.corners.base.get(&f) {
            let v = self.base_value(f, part, base, at);
            if let Ok(q) = &v
                && self.model.board_of(part) == Some(at.corner.board)
            {
                at.read.push((f, *q));
            }
            v
        } else {
            let mut found = Err(Miss::Unknown);
            for def in self.corners.defs.get(&f).into_iter().flatten() {
                found = match def {
                    Def::Same(g) => self.corner_value(*g, at),
                    Def::Expr(e) => {
                        let mut reads = Vec::new();
                        fields_in(e, &mut reads);
                        let mut values = Vec::new();
                        let mut miss = None;
                        for g in reads {
                            if self.corners.moves(g) && !values.iter().any(|(h, _)| *h == g) {
                                match self.corner_value(g, at) {
                                    Ok(q) => values.push((g, q)),
                                    Err(m) => {
                                        miss = Some(m);
                                        break;
                                    }
                                }
                            }
                        }
                        match miss {
                            Some(m) => Err(m),
                            None => match eval_with(&self.model, e, &values) {
                                Ok(Val::Q(q)) => Ok(q),
                                _ => Err(Miss::Unknown),
                            },
                        }
                    }
                };
                if found.is_ok() {
                    break;
                }
            }
            found
        };
        at.visiting.remove(&f);
        at.memo.insert(f, value.clone());
        value
    }

    /// A capacitor's own value at the corner: as it is on a board the corner is not of.
    fn base_value(
        &self,
        f: FieldId,
        part: InstId,
        base: Base,
        at: &mut At,
    ) -> Result<Quantity, Miss> {
        let Domain::Quantity(value) = self.model.field(f).domain else {
            return Err(Miss::Unknown);
        };
        if self.model.board_of(part) != Some(at.corner.board) {
            return Ok(value);
        }
        at.touched = true;
        let corner = at.corner;
        let member = |name: &'static str| -> Result<Quantity, Miss> {
            match self.model.member(part, name) {
                Some(Member::Field(g)) => match self.model.field(g).domain {
                    Domain::Quantity(q) => Ok(q),
                    _ => Err(Miss::Member { part, member: name }),
                },
                _ => Err(Miss::Member { part, member: name }),
            }
        };
        let value = match base {
            Base::Effective => value,
            Base::Esr => self.esr_at(f, part, value, corner.kelvin)?,
        };
        if !corner.end {
            return Ok(value);
        }
        let factor = match base {
            Base::Effective => member("capacitance_at_end_of_life")?,
            Base::Esr => member("esr_growth")?,
        };
        value.mul(&factor).map_err(|_| Miss::Unknown)
    }

    /// A capacitor's ESR at a temperature: off the curve of the row the trial or the pick gave
    /// it, the design's own figure, or the row's single figure at 25 degC.
    fn esr_at(
        &self,
        f: FieldId,
        part: InstId,
        value: Quantity,
        kelvin: f64,
    ) -> Result<Quantity, Miss> {
        if let Some((curve, domain)) = self.corner_curves.get(&f)
            && self.model.field(f).domain == *domain
        {
            return self
                .curve_at(part, curve, kelvin)
                .ok_or_else(|| Miss::Beyond {
                    part,
                    file: curve.file.clone(),
                });
        }
        if !self.model.field(f).narrowed_at.is_empty() || (kelvin - DATASHEET).abs() < 1e-6 {
            return Ok(value);
        }
        Err(Miss::NoCurve { part })
    }

    /// A curve read at one temperature, over the ranges the design gives its other axes.
    fn curve_at(&self, part: InstId, c: &Curve, kelvin: f64) -> Option<Quantity> {
        let mut ranges = Vec::with_capacity(c.axes.len());
        for (axis, dim) in &c.axes {
            if axis == curve::TEMPERATURE {
                ranges.push((kelvin, kelvin));
                continue;
            }
            let path = format!("{}.{axis}", self.model.path_of(part));
            match self
                .model
                .field_at(&path)
                .map(|g| &self.model.field(g).domain)
            {
                Some(Domain::Quantity(q)) if q.dim() == *dim => ranges.push((q.lo(), q.hi())),
                _ => return None,
            }
        }
        c.over(&ranges).ok()
    }

    /// The bounds that a corner could not judge, one note for each corner, part and missing
    /// value, naming the bounds (`relations` are the design's, by their place).
    pub(super) fn corner_notes(&self, relations: &[usize]) -> Vec<crate::levels::NotEvaluated> {
        if self.corners.corners.is_empty() {
            return Vec::new();
        }
        let mut grouped: Vec<((usize, Miss), Vec<String>)> = Vec::new();
        for &i in relations {
            let r = &self.model.relations()[i];
            for (k, corner) in self.corners.corners.iter().enumerate() {
                let AtCorner::Unknown(miss) = self.at_corner(r, corner) else {
                    continue;
                };
                if miss == Miss::Unknown {
                    continue;
                }
                let bound = format!(
                    "`{} {} {}`",
                    self.model.show_expr(&r.lhs),
                    if r.op == CmpOp::Ge { ">=" } else { "<=" },
                    self.model.show_expr(&r.rhs)
                );
                let key = (k, miss);
                match grouped.iter_mut().find(|(g, _)| *g == key) {
                    Some((_, bounds)) if !bounds.contains(&bound) => bounds.push(bound),
                    Some(_) => {}
                    None => grouped.push((key, vec![bound])),
                }
            }
        }
        grouped
            .into_iter()
            .map(|((k, miss), bounds)| {
                let corner = &self.corners.corners[k];
                let part = match &miss {
                    Miss::Member { part, .. } | Miss::NoCurve { part } | Miss::Beyond { part, .. } => {
                        *part
                    }
                    Miss::Unknown => unreachable!("an unknown value is not named"),
                };
                let path = self.model.path_of(part);
                let why = match &miss {
                    Miss::Member { member, .. } => format!(
                        "`{path}` states no `{member}` (its catalog row's column, from the maker's endurance test, or `.{path}.{member} &= ...`)"
                    ),
                    Miss::NoCurve { .. } => format!(
                        "the ESR of `{path}` is its row's single figure, which its datasheet states at 25degC, and no curve file over temperature gives it at {} (`curves/<catalog>/<row>.csv` with the axis `temperature`, or `.{path}.esr` stated for every temperature)",
                        netsmith_check::environment::show(corner.kelvin)
                    ),
                    Miss::Beyond { file, .. } => format!(
                        "the curve file {file} of `{path}` does not reach {}",
                        netsmith_check::environment::show(corner.kelvin)
                    ),
                    Miss::Unknown => unreachable!("an unknown value is not named"),
                };
                crate::levels::NotEvaluated {
                    at: part,
                    involves: vec![part],
                    message: format!(
                        "at the corner {}, {} {} not evaluated: {why}",
                        corner.describe(),
                        bounds.join(", "),
                        if bounds.len() == 1 { "is" } else { "are" }
                    ),
                    cause: netsmith_core::Cause::NotStated,
                }
            })
            .collect()
    }
}

/// The curve and the domain it gave a field when a candidate was tried, so that a corner reads
/// the curve at its own temperature (`Solver::corner_curves`).
pub(super) type CornerCurves = HashMap<FieldId, (Arc<Curve>, Domain)>;
