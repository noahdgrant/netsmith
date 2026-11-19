//! The mated height of each mate of a system (ADR 0110).
//!
//! Each connector states its _Seat_ (`.mating.seat`, a `header` catalog column): how far above
//! the face of its board its partner rests. A mate's _Mated height_, the distance between the
//! two boards' faces, is the two seats added, with both tolerances: the std 2.54 mm header
//! (2.50 +/- 0.20 mm) in the std socket (8.50 +/- 0.15 mm) gives 10.65 mm to 11.35 mm. A mate
//! with a seat that nothing states has no mated height, never a default and never one from a
//! part's `.height` (a maximum, not a seat). The system scene reads these to place the boards
//! against each other; nothing here needs geometry.
//!
//! A connector's body is its seat, one surface with one tolerance, unless it states
//! `.mating.body_height` (issue 1324, decided by the owner): the system scene then keeps the
//! stated body, at its most, out of the other board at the least mated height.

use netsmith_check::{Domain, InstId, Member, Model, System};
use netsmith_solve::Solution;

/// One mate of a system, as the system scene needs it: the two connectors, their boards,
/// their seats and the mated height they give.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemMate {
    /// The two connectors, by path in the system (`power.power_out`), in the order the
    /// statement names them.
    pub a: String,
    pub b: String,
    /// The member of the system each connector is on (`power`).
    pub a_board: Option<String>,
    pub b_board: Option<String>,
    /// Each connector's seat, in mm: `(least, most)`. `None` when nothing states it.
    pub a_seat: Option<(f64, f64)>,
    pub b_seat: Option<(f64, f64)>,
    /// Each connector's `.mating.body_height` in mm, `(least, most)`: `None` when nothing
    /// states it, and the body is then the seat.
    pub a_body: Option<(f64, f64)>,
    pub b_body: Option<(f64, f64)>,
    /// The contacts the mate joins when a `.mating.contacts` subset says only some are:
    /// `(first of a, first of b, how many)`, pad `first_a + i` meeting pad `first_b + i`.
    /// `None` when it joins every contact, pad N to pad N.
    pub window: Option<(usize, usize, usize)>,
    /// Whether either connector says what keys its housing (`.mating.keyed` set to other than
    /// `"none"`), so that the mate goes together one way only. The system scene looks for pin 1
    /// on the pads of a mate that neither keys.
    pub keyed: bool,
}

impl SystemMate {
    /// The distance between the two boards' faces, in mm: `(least, most)`, the two seats'
    /// sum. `None` when a seat is unknown.
    pub fn mated_height(&self) -> Option<(f64, f64)> {
        let (a, b) = (self.a_seat?, self.b_seat?);
        Some((round(a.0 + b.0), round(a.1 + b.1)))
    }

    /// Why the mate has no mated height, naming each connector whose seat is unknown. `None`
    /// when it has one.
    pub fn unknown(&self) -> Option<String> {
        let missing: Vec<String> = [(&self.a, self.a_seat), (&self.b, self.b_seat)]
            .into_iter()
            .filter(|(_, seat)| seat.is_none())
            .map(|(path, _)| format!("`{path}.mating.seat`"))
            .collect();
        (!missing.is_empty()).then(|| {
            format!(
                "the mate of `{}` and `{}` has no mated height: nothing states {}",
                self.a,
                self.b,
                missing.join(" or ")
            )
        })
    }
}

/// A length in mm, with the rounding error of the sum taken off.
fn round(mm: f64) -> f64 {
    (mm * 1e9).round() / 1e9
}

/// The mates of a solved system, in the order the system states them, with their seats. A
/// seat below the board's face is an error, which names the field and the number.
pub fn system_mates(system: &System, solution: &Solution) -> Result<Vec<SystemMate>, Vec<String>> {
    let model = &solution.model;
    let board = |part: InstId| {
        let inst = model.board_of(part)?;
        system
            .boards
            .iter()
            .find(|b| b.inst == inst)
            .map(|b| b.name.clone())
    };
    let mut errors = Vec::new();
    let mut out = Vec::new();
    for mate in model.mates() {
        netsmith_core::job::checkpoint();
        out.push(SystemMate {
            a: model.path_of(mate.a),
            b: model.path_of(mate.b),
            a_board: board(mate.a),
            b_board: board(mate.b),
            a_seat: seat(model, mate.a, &mut errors),
            b_seat: seat(model, mate.b, &mut errors),
            a_body: length(model, mate.a, "body_height", &mut errors),
            b_body: length(model, mate.b, "body_height", &mut errors),
            window: (mate.a_first != 1 || mate.b_first != 1 || !mates_every_contact(model, mate))
                .then_some((mate.a_first, mate.b_first, mate.contacts.len())),
            keyed: keyed(model, mate.a) || keyed(model, mate.b),
        });
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}

/// Whether a mate joins every contact of both its connectors, as a mate with no
/// `.mating.contacts` subset does.
fn mates_every_contact(model: &Model, mate: &netsmith_check::Mate) -> bool {
    let positions = |part: InstId| {
        let mating = model.mating_of(part)?;
        let Member::Field(field) = model.member(mating, "positions")? else {
            return None;
        };
        match &model.field(field).domain {
            Domain::Quantity(q) if q.is_point() => Some(q.mid() as usize),
            _ => None,
        }
    };
    let n = mate.contacts.len();
    positions(mate.a) == Some(n) && positions(mate.b) == Some(n)
}

/// Whether a connector's `.mating.keyed` says something keys it: set, and not `"none"`, as the
/// checker's `mating` guideline reads it.
fn keyed(model: &Model, part: InstId) -> bool {
    let Some(mating) = model.mating_of(part) else {
        return false;
    };
    let Some(Member::Field(field)) = model.member(mating, "keyed") else {
        return false;
    };
    matches!(&model.field(field).domain, Domain::Text(k) if !k.is_empty() && k != "none")
}

/// A connector's `.mating.seat` in mm, if something states it.
fn seat(model: &Model, part: InstId, errors: &mut Vec<String>) -> Option<(f64, f64)> {
    length(model, part, "seat", errors)
}

/// A length of a connector's `.mating` (`seat`, `body_height`) in mm, if something states it.
/// One below the board's face is an error.
fn length(model: &Model, part: InstId, name: &str, errors: &mut Vec<String>) -> Option<(f64, f64)> {
    let mating = model.mating_of(part)?;
    let Member::Field(field) = model.member(mating, name)? else {
        return None;
    };
    let Domain::Quantity(q) = &model.field(field).domain else {
        return None;
    };
    if !(q.lo().is_finite() && q.hi().is_finite()) {
        return None;
    }
    let (lo, hi) = (round(q.lo() * 1e3), round(q.hi() * 1e3));
    if lo < 0.0 {
        let what = if name == "seat" {
            "a connector's partner does not rest below its board's face"
        } else {
            "a connector's body does not reach below its board's face"
        };
        errors.push(format!(
            "`{}.mating.{name}` is {q}, and {what}",
            model.path_of(part)
        ));
        return None;
    }
    Some((lo, hi))
}
