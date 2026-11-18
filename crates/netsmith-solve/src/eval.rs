//! Evaluating a checked expression over the current domains, in the worst case.

use crate::expand::tidy;
use netsmith_check::{Domain, FieldId, Model, RExpr, RangeEnd};
use netsmith_lang::ast::BinOp;
use netsmith_quantity::{Quantity, QuantityError};

#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Q(Quantity),
    T(String),
}

#[derive(Debug)]
pub enum EvalError {
    /// A field the expression needs has no value yet.
    Unknown(FieldId),
    Quantity(QuantityError),
    /// A range whose low end (its lowest value) is above its high end (its highest), with the
    /// two ends as evaluated (ADR 0066).
    EmptyRange(Quantity, Quantity),
    /// A point of a range that has none, because the range is open there: `max(.f)` of
    /// `1mm..` (the end read, the range as it stands).
    OpenEnd(RangeEnd, Quantity),
    /// Text where a number belongs, or the reverse.
    Type,
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown(_) => write!(f, "a value it needs is not known yet"),
            Self::Quantity(e) => write!(f, "{e}"),
            Self::EmptyRange(lo, hi) => write!(
                f,
                "the low end of the range is above the high end: {lo} against {hi}"
            ),
            Self::OpenEnd(end, range) => write!(
                f,
                "`{}` of {range} is no value: the range has an open end there",
                end.word()
            ),
            Self::Type => write!(f, "text and numbers are mixed"),
        }
    }
}

/// The most single evaluations one `eval` may spend tightening a range. Past it the answer
/// is the plain interval result, which is looser but still sound.
const BUDGET: usize = 50_000;

/// The least budget left with which a variable is still tightened: below it the rest is read
/// with plain interval arithmetic.
const MIN_BUDGET: usize = 32;

/// How many times in a row a range may be halved where a formula is not provably monotone in
/// it, so the piece that holds a turning point (zero for `x * x`) is at most `2^-MAX_SPLITS`
/// of the range wide. The halves that are monotone are read exactly at their ends; the last
/// piece that is not is read by the mean value form, whose excess shrinks with the square of
/// the piece's width, so at 2^-16 of the range what is left over is below a part in 10^9 of
/// the result for the smooth formulas circuits are made of.
const MAX_SPLITS: usize = 16;

/// Values fixed for the moment, in place of a field's domain.
type Env = Vec<(FieldId, Quantity)>;

/// The worst-case value of an expression over the current domains.
///
/// Interval arithmetic treats every appearance of a variable as independent, so a variable
/// that appears twice (`r_bottom` in a voltage divider's numerator and denominator) makes the
/// range wider than the true one. Here, a variable that appears more than once is handled
/// separately: if the formula is provably monotone in it over the whole range (its symbolic
/// derivative cannot change sign), the true range is spanned by the two end points, so each
/// is evaluated and the results joined; otherwise its range is halved and the halves' results
/// joined, each half judged the same way, so only the halves that hold a turning point are cut
/// again. A piece cut as far as it goes (`MAX_SPLITS`) is read by the mean value form, `f(m) +
/// f'(piece) * (piece - m)` with `m` its middle, where that is tighter than plain interval
/// arithmetic. Either way the answer contains every value the expression can take; for the
/// monotone formulas circuits are made of it is exact, and for one with a turning point (a
/// ripple that peaks at half the input voltage) it is exact but for a part in 10^9 or so.
pub fn eval(model: &Model, e: &RExpr) -> Result<Val, EvalError> {
    let mut budget = BUDGET;
    tight(model, e, &Env::new(), &mut budget, 0)
}

/// The worst-case value of an expression with some fields held to the values given, in place
/// of their domains: the values of a named corner of a board (issue #1256).
pub fn eval_with(
    model: &Model,
    e: &RExpr,
    values: &[(FieldId, Quantity)],
) -> Result<Val, EvalError> {
    let mut budget = BUDGET;
    tight(model, e, &values.to_vec(), &mut budget, 0)
}

/// Whether `e` has a range with a value at an end somewhere in it (ADR 0066), or reads the end
/// of a field's range (`min(.f)`, ADR 0098; also `max(.f)` and `nominal(.f)`): neither is a
/// smooth function of its fields.
fn has_range(e: &RExpr) -> bool {
    match e {
        RExpr::Range(..) | RExpr::End(..) => true,
        RExpr::Neg(x) => has_range(x),
        RExpr::Bin(_, a, b) => has_range(a) || has_range(b),
        RExpr::Const(_) | RExpr::Text(_) | RExpr::Field(_) => false,
    }
}

/// Whether `e` reads a point of a field's range (`min(.f)`, `max(.f)` or `nominal(.f)`, ADR 0098).
pub fn reads_end(e: &RExpr) -> bool {
    match e {
        RExpr::End(..) => true,
        RExpr::Neg(x) => reads_end(x),
        RExpr::Bin(_, a, b) => reads_end(a) || reads_end(b),
        RExpr::Range(a, b) => [a, b].into_iter().flatten().any(|e| reads_end(e)),
        RExpr::Const(_) | RExpr::Text(_) | RExpr::Field(_) => false,
    }
}

/// Whether `e` reads the centre of a field's range (`nominal(.f)`).
pub fn reads_nominal(e: &RExpr) -> bool {
    match e {
        RExpr::End(RangeEnd::Nominal, _) => true,
        RExpr::End(..) => false,
        RExpr::Neg(x) => reads_nominal(x),
        RExpr::Bin(_, a, b) => reads_nominal(a) || reads_nominal(b),
        RExpr::Range(a, b) => [a, b].into_iter().flatten().any(|e| reads_nominal(e)),
        RExpr::Const(_) | RExpr::Text(_) | RExpr::Field(_) => false,
    }
}

/// Whether the relation `r` reads a point of a field's range (`min`, `max` or `nominal`) on either side. Such a relation
/// is verified on the final values and nothing more: it is never used to refuse a block of
/// candidates (the hull's end is no enclosure of a row's own point) and never to take a bound for
/// another field (a range only narrows, so the end it read may be stale, ADR 0098).
pub fn relation_reads_end(r: &netsmith_check::Relation) -> bool {
    reads_end(&r.lhs) || reads_end(&r.rhs)
}

/// `splits` counts the cuts made on the way here.
fn tight(
    model: &Model,
    e: &RExpr,
    env: &Env,
    budget: &mut usize,
    splits: usize,
) -> Result<Val, EvalError> {
    // A range with a value at an end is not a smooth function of its fields (it is the hull of
    // two ends), so nothing is tightened and the plain interval result stands: wider at worst,
    // never narrower than the values it can take.
    if has_range(e) {
        return plain(model, e, env, budget);
    }
    let repeated = repeated_variables(model, e, env)?;
    let Some(&var) = repeated.first() else {
        return plain(model, e, env, budget);
    };
    if *budget < MIN_BUDGET {
        return plain(model, e, env, budget);
    }
    let range = value_of(model, var, env)?;

    let derivative = derivative(e, var).and_then(|d| tidy(&d));

    // Not a function of `var` at all (it cancels, as in `x - x`): any one value will do.
    let Some(derivative) = derivative else {
        let point = Quantity::point(range.dim(), range.lo()).map_err(EvalError::Quantity)?;
        return tight(model, e, &with(env, var, point), budget, splits);
    };

    // Monotone in `var` over the whole box: the end points span the range. The derivative
    // is itself tightened, since it repeats variables too (a divider's does).
    if let Ok(Val::Q(d)) = plain(model, &derivative, env, budget)
        && (d.lo() >= 0.0 || d.hi() <= 0.0)
    {
        let mut at = |value: f64| -> Result<Val, EvalError> {
            let point = Quantity::point(range.dim(), value).map_err(EvalError::Quantity)?;
            tight(model, e, &with(env, var, point), budget, splits)
        };
        let low = at(range.lo())?;
        let high = at(range.hi())?;
        return join(low, high);
    }

    // Not provably monotone: halve the range and join what each half gives, unless it has
    // been cut as far as it will be (or is too narrow for a double to have a middle).
    let middle = range.lo() + (range.hi() - range.lo()) / 2.0;
    if splits >= MAX_SPLITS || middle <= range.lo() || middle >= range.hi() {
        return last_piece(model, e, env, var, &range, budget, splits);
    }
    let mut joined: Option<Val> = None;
    for (lo, hi) in [(range.lo(), middle), (middle, range.hi())] {
        let piece = Quantity::interval(range.dim(), lo, hi).map_err(EvalError::Quantity)?;
        let value = tight(model, e, &with(env, var, piece), budget, splits + 1)?;
        joined = Some(match joined {
            None => value,
            Some(so_far) => join(so_far, value)?,
        });
    }
    Ok(joined.expect("there are two halves"))
}

/// A piece of `var`'s range that is cut no further: the values plain interval arithmetic gives,
/// narrowed to the mean value form's where that is tighter. For every value `x` of the piece
/// (and every value of the other fields), `f(x) = f(m) + f'(c) * (x - m)` for some `c` between
/// `m` and `x`, and the slope (the derivative read over the piece) holds every `f'(c)`, so
/// `f(m) + slope * (piece - m)` holds every `f(x)`: both enclosures are sound, and so is what
/// they share. The slope is read from the derivative as it was built, not from its tidied form,
/// which collects like terms in plain floating point: only outward-rounded interval arithmetic
/// goes into a bound. Where the form cannot be made (a slope that could not be worked out, a
/// dimension that does not match) the plain result stands.
fn last_piece(
    model: &Model,
    e: &RExpr,
    env: &Env,
    var: FieldId,
    range: &Quantity,
    budget: &mut usize,
    splits: usize,
) -> Result<Val, EvalError> {
    let plain_value = plain(model, e, env, budget)?;
    let Val::Q(wide) = plain_value else {
        return Ok(plain_value);
    };
    let slope = match derivative(e, var).map(|d| plain(model, &d, env, budget)) {
        Some(Ok(Val::Q(slope))) if slope.is_bounded() => slope,
        _ => return Ok(Val::Q(wide)),
    };
    let middle = Quantity::point(range.dim(), range.mid()).map_err(EvalError::Quantity)?;
    let form = match tight(model, e, &with(env, var, middle), budget, splits) {
        Ok(Val::Q(at_middle)) => range
            .sub(&middle)
            .and_then(|offset| slope.mul(&offset))
            .and_then(|spread| at_middle.add(&spread))
            .ok(),
        _ => None,
    };
    Ok(Val::Q(match form.map(|form| wide.intersect(&form)) {
        Some(Ok(Some(both))) => both,
        _ => wide,
    }))
}

fn with(env: &Env, var: FieldId, value: Quantity) -> Env {
    let mut next: Env = env.iter().filter(|(f, _)| *f != var).cloned().collect();
    next.push((var, value));
    next
}

/// The smallest range holding both.
fn join(a: Val, b: Val) -> Result<Val, EvalError> {
    match (a, b) {
        (Val::Q(x), Val::Q(y)) => {
            Quantity::interval(x.dim(), x.lo().min(y.lo()), x.hi().max(y.hi()))
                .map(Val::Q)
                .map_err(EvalError::Quantity)
        }
        _ => Err(EvalError::Type),
    }
}

/// A field's value right now: the one fixed for this evaluation, else its domain.
fn value_of(model: &Model, f: FieldId, env: &Env) -> Result<Quantity, EvalError> {
    if let Some((_, q)) = env.iter().find(|(g, _)| *g == f) {
        return Ok(*q);
    }
    match &model.field(f).domain {
        Domain::Unset => Err(EvalError::Unknown(f)),
        Domain::Quantity(q) => Ok(*q),
        Domain::Text(_) => Err(EvalError::Type),
    }
}

/// The centre of a range: the value a field is stated at when its range is a value with a
/// tolerance. A range is stored as its two ends, so this is their mean, and for `1kohm..3kohm`,
/// which states no centre, it is 2 kohm. A point is its own centre. The mean of ends that came
/// from `center +/- tol` is the center up to the last bit or two of a float (3.135 and 3.465 do
/// not average to exactly 3.3), which would fail `nominal(.v) >= 3.3V` by a rounding error, so
/// the result is rounded to 12 significant digits.
fn centre(range: &Quantity) -> f64 {
    let mid = range.mid();
    if mid == 0.0 || !mid.is_finite() {
        return mid;
    }
    let digits = 11 - mid.abs().log10().floor() as i32;
    let scale = 10f64.powi(digits);
    let rounded = (mid * scale).round() / scale;
    if rounded.is_finite() { rounded } else { mid }
}

/// Fields that appear more than once and are not a single value, in a fixed order.
fn repeated_variables(model: &Model, e: &RExpr, env: &Env) -> Result<Vec<FieldId>, EvalError> {
    let mut all = Vec::new();
    fields_in(e, &mut all);
    all.sort();
    let mut repeated = Vec::new();
    for window in all.windows(2) {
        if window[0] == window[1] && repeated.last() != Some(&window[0]) {
            let f = window[0];
            // Text fields and unknown fields are not tightened; plain evaluation reports them.
            match value_of(model, f, env) {
                // A field with an open end is not cut up (its pieces have no size): it is
                // read as it stands, which is wider at worst.
                Ok(q) if q.lo() != q.hi() && q.is_bounded() => repeated.push(f),
                _ => {}
            }
        }
    }
    Ok(repeated)
}

/// The derivative of `e` with respect to `var`, kept only well enough to tell its sign: the
/// factor of `var`'s own unit is dropped from every term alike. `None` is exactly zero.
fn derivative(e: &RExpr, var: FieldId) -> Option<RExpr> {
    use RExpr::{Bin, Const, Field, Neg};
    let one =
        || Const(Quantity::point(netsmith_quantity::Dimension::NONE, 1.0).expect("one is finite"));
    let bin = |op, a: RExpr, b: RExpr| Bin(op, Box::new(a), Box::new(b));
    match e {
        RExpr::Const(_) | RExpr::Text(_) => None,
        // Not reached (`tight` leaves a range alone); a derivative that spans zero says that
        // its sign is unknown.
        RExpr::Range(..) | RExpr::End(..) => {
            Quantity::interval(netsmith_quantity::Dimension::NONE, -1.0, 1.0)
                .ok()
                .map(Const)
        }
        Field(f) => (*f == var).then(one),
        Neg(x) => derivative(x, var).map(|d| Neg(Box::new(d))),
        Bin(op, a, b) => {
            let (da, db) = (derivative(a, var), derivative(b, var));
            let (a, b) = (a.as_ref().clone(), b.as_ref().clone());
            match op {
                BinOp::Add => match (da, db) {
                    (Some(x), Some(y)) => Some(bin(BinOp::Add, x, y)),
                    (x, y) => x.or(y),
                },
                BinOp::Sub => match (da, db) {
                    // `x - x`: the two sides change together, so nothing changes.
                    (Some(x), Some(y)) if x == y => None,
                    (Some(x), Some(y)) => Some(bin(BinOp::Sub, x, y)),
                    (Some(x), None) => Some(x),
                    (None, Some(y)) => Some(Neg(Box::new(y))),
                    (None, None) => None,
                },
                BinOp::Mul => match (
                    da.map(|x| bin(BinOp::Mul, x, b)),
                    db.map(|y| bin(BinOp::Mul, a, y)),
                ) {
                    (Some(x), Some(y)) => Some(bin(BinOp::Add, x, y)),
                    (x, y) => x.or(y),
                },
                BinOp::Div => {
                    let b_squared = bin(BinOp::Mul, b.clone(), b.clone());
                    match (da, db) {
                        (Some(x), None) => Some(bin(BinOp::Div, x, b)),
                        (None, Some(y)) => Some(Neg(Box::new(bin(
                            BinOp::Div,
                            bin(BinOp::Mul, a, y),
                            b_squared,
                        )))),
                        (Some(x), Some(y)) => Some(bin(
                            BinOp::Div,
                            bin(BinOp::Sub, bin(BinOp::Mul, x, b), bin(BinOp::Mul, a, y)),
                            b_squared,
                        )),
                        (None, None) => None,
                    }
                }
            }
        }
    }
}

/// Ordinary interval arithmetic: every appearance of a field counts as independent.
fn plain(model: &Model, e: &RExpr, env: &Env, budget: &mut usize) -> Result<Val, EvalError> {
    *budget = budget.saturating_sub(1);
    match e {
        RExpr::Const(q) => Ok(Val::Q(*q)),
        RExpr::Text(t) => Ok(Val::T(t.clone())),
        RExpr::Field(f) => {
            if let Some((_, q)) = env.iter().find(|(g, _)| g == f) {
                return Ok(Val::Q(*q));
            }
            match &model.field(*f).domain {
                Domain::Unset => Err(EvalError::Unknown(*f)),
                Domain::Quantity(q) => Ok(Val::Q(*q)),
                Domain::Text(t) => Ok(Val::T(t.clone())),
            }
        }
        // The end of a field's range is one value: a point at the least or the greatest value
        // the field can have now. An unset field has none, so the check waits. A point has no
        // spread, so it carries no correlation with other readings of the same field: the check
        // that compares it is made on two numbers.
        RExpr::End(end, f) => {
            let range = value_of(model, *f, env)?;
            let at = match end {
                RangeEnd::Min => range.lo(),
                RangeEnd::Max => range.hi(),
                RangeEnd::Nominal => centre(&range),
            };
            // The open end of a range is no value: `max(.f)` of `1mm..` has none, while its
            // `min(.f)` is the 1 mm.
            if !at.is_finite() {
                return Err(EvalError::OpenEnd(*end, range));
            }
            Quantity::point(range.dim(), at)
                .map(Val::Q)
                .map_err(EvalError::Quantity)
        }
        RExpr::Neg(inner) => match plain(model, inner, env, budget)? {
            Val::Q(q) => Ok(Val::Q(q.neg())),
            Val::T(_) => Err(EvalError::Type),
        },
        RExpr::Bin(op, a, b) => {
            let (x, y) = (plain(model, a, env, budget), plain(model, b, env, budget));
            let (Val::Q(x), Val::Q(y)) = (x?, y?) else {
                return Err(EvalError::Type);
            };
            let result = match op {
                BinOp::Add => x.add(&y),
                BinOp::Sub => x.sub(&y),
                BinOp::Mul => x.mul(&y),
                BinOp::Div => x.div(&y),
            };
            result.map(Val::Q).map_err(EvalError::Quantity)
        }
        // A range with a value at an end is a claim only when both ends are known: an end
        // with no value is `Unknown`, and nothing is narrowed or checked (ADR 0066). The
        // range holds the low end's lowest value and the high end's highest, as the written
        // `lo..hi` does, so an end that is itself a range is never trusted more than it says.
        RExpr::Range(lo, hi) => {
            let end = |e: &Option<Box<RExpr>>, budget: &mut usize| match e {
                Some(e) => plain(model, e, env, budget).map(Some),
                None => Ok(None),
            };
            let (x, y) = (end(lo, budget), end(hi, budget));
            let (x, y) = (x?, y?);
            let quantity = |v: Option<Val>| match v {
                Some(Val::Q(q)) => Ok(Some(q)),
                Some(Val::T(_)) => Err(EvalError::Type),
                None => Ok(None),
            };
            let (x, y) = (quantity(x)?, quantity(y)?);
            let range = match (x, y) {
                (Some(x), Some(y)) => {
                    if x.dim() == y.dim() && x.lo() > y.hi() {
                        return Err(EvalError::EmptyRange(x, y));
                    }
                    Quantity::range(&x, &y)
                }
                (Some(x), None) => Quantity::at_least(&x),
                (None, Some(y)) => Quantity::at_most(&y),
                (None, None) => unreachable!("a range has an end"),
            };
            range.map(Val::Q).map_err(EvalError::Quantity)
        }
    }
}

/// Every field an expression mentions.
pub fn fields_in(e: &RExpr, out: &mut Vec<FieldId>) {
    match e {
        RExpr::Field(f) | RExpr::End(_, f) => out.push(*f),
        RExpr::Neg(inner) => fields_in(inner, out),
        RExpr::Bin(_, a, b) => {
            fields_in(a, out);
            fields_in(b, out);
        }
        RExpr::Range(a, b) => {
            for end in [a, b].into_iter().flatten() {
                fields_in(end, out);
            }
        }
        RExpr::Const(_) | RExpr::Text(_) => {}
    }
}
