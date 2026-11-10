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
            Self::Type => write!(f, "text and numbers are mixed"),
        }
    }
}

/// The most single evaluations one `eval` may spend tightening a range. Past it the answer
/// is the plain interval result, which is looser but still sound.
const BUDGET: usize = 50_000;

/// How many pieces a range is cut into when a formula is not monotone in a variable.
const PIECES: usize = 8;

/// How many times in a row a range may be cut up, so the range is resolved to
/// `PIECES.pow(MAX_SPLITS)` parts at most. A piece that still straddles the awkward spot,
/// such as zero for `x * x`, is cut again until this runs out.
const MAX_SPLITS: usize = 4;

/// Values fixed for the moment, in place of a field's domain.
type Env = Vec<(FieldId, Quantity)>;

/// The worst-case value of an expression over the current domains.
///
/// Interval arithmetic treats every appearance of a variable as independent, so a variable
/// that appears twice (`r_bottom` in a voltage divider's numerator and denominator) makes the
/// range wider than the true one. Here, a variable that appears more than once is handled
/// separately: if the formula is provably monotone in it over the whole range (its symbolic
/// derivative cannot change sign), the true range is spanned by the two end points, so each
/// is evaluated and the results joined; otherwise its range is cut into pieces and the
/// pieces' results joined. Either way the answer contains every value the expression can
/// take, and for the monotone formulas circuits are made of, it is exact.
pub fn eval(model: &Model, e: &RExpr) -> Result<Val, EvalError> {
    let mut budget = BUDGET;
    tight(model, e, &Env::new(), &mut budget, 0)
}

/// Whether `e` has a range with a value at an end somewhere in it (ADR 0066), or reads the end
/// of a field's range (`min(.f)`, ADR 0098): neither is a smooth function of its fields.
fn has_range(e: &RExpr) -> bool {
    match e {
        RExpr::Range(..) | RExpr::End(..) => true,
        RExpr::Neg(x) => has_range(x),
        RExpr::Bin(_, a, b) => has_range(a) || has_range(b),
        RExpr::Const(_) | RExpr::Text(_) | RExpr::Field(_) => false,
    }
}

/// Whether `e` reads the end of a field's range (`min(.f)` or `max(.f)`, ADR 0098).
pub fn reads_end(e: &RExpr) -> bool {
    match e {
        RExpr::End(..) => true,
        RExpr::Neg(x) => reads_end(x),
        RExpr::Bin(_, a, b) | RExpr::Range(a, b) => reads_end(a) || reads_end(b),
        RExpr::Const(_) | RExpr::Text(_) | RExpr::Field(_) => false,
    }
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
    if *budget < 4 * PIECES {
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

    // Not provably monotone: cut the range up and join what each piece gives, unless it has
    // been cut as far as it will be.
    if splits >= MAX_SPLITS {
        return plain(model, e, env, budget);
    }
    let mut joined: Option<Val> = None;
    for i in 0..PIECES {
        let lo = if i == 0 {
            range.lo()
        } else {
            piece_edge(&range, i)
        };
        let hi = if i + 1 == PIECES {
            range.hi()
        } else {
            piece_edge(&range, i + 1)
        };
        let piece = Quantity::interval(range.dim(), lo, hi).map_err(EvalError::Quantity)?;
        let value = tight(model, e, &with(env, var, piece), budget, splits + 1)?;
        joined = Some(match joined {
            None => value,
            Some(so_far) => join(so_far, value)?,
        });
    }
    Ok(joined.expect("there is at least one piece"))
}

/// The `i`-th of `PIECES` equal cuts through `range`, computed the same way for both of the
/// pieces that meet there so that they share the edge exactly.
fn piece_edge(range: &Quantity, i: usize) -> f64 {
    range.lo() + (range.hi() - range.lo()) * (i as f64) / (PIECES as f64)
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
                Ok(q) if q.lo() != q.hi() => repeated.push(f),
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
            };
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
            let (x, y) = (plain(model, lo, env, budget), plain(model, hi, env, budget));
            let (Val::Q(x), Val::Q(y)) = (x?, y?) else {
                return Err(EvalError::Type);
            };
            if x.dim() == y.dim() && x.lo() > y.hi() {
                return Err(EvalError::EmptyRange(x, y));
            }
            Quantity::range(&x, &y)
                .map(Val::Q)
                .map_err(EvalError::Quantity)
        }
    }
}

/// Every field an expression mentions.
pub fn fields_in(e: &RExpr, out: &mut Vec<FieldId>) {
    match e {
        RExpr::Field(f) | RExpr::End(_, f) => out.push(*f),
        RExpr::Neg(inner) => fields_in(inner, out),
        RExpr::Bin(_, a, b) | RExpr::Range(a, b) => {
            fields_in(a, out);
            fields_in(b, out);
        }
        RExpr::Const(_) | RExpr::Text(_) => {}
    }
}
