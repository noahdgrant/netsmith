//! The mated height of each mate of a system (ADR 0110).
//!
//! Each connector states its _Seat_ (`.mating.seat`, a `header` catalog column): how far above
//! the face of its board its partner rests. A mate's _Mated height_, the distance between the
//! two boards' faces, is the two seats added, with both tolerances: the std 2.54 mm header
//! (2.50 +/- 0.20 mm) in the std socket (8.50 +/- 0.15 mm) gives 10.65 mm to 11.35 mm. A mate
//! with a seat that nothing states has no mated height, never a default and never one from a
//! part's `.height` (a maximum, not a seat). The system scene reads these to place the boards
//! against each other; nothing here needs geometry.

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
        });
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}

/// A connector's `.mating.seat` in mm, if something states it.
fn seat(model: &Model, part: InstId, errors: &mut Vec<String>) -> Option<(f64, f64)> {
    let mating = model.mating_of(part)?;
    let Member::Field(field) = model.member(mating, "seat")? else {
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
        errors.push(format!(
            "`{}.mating.seat` is {q}, and a connector's partner does not rest below its board's face",
            model.path_of(part)
        ));
        return None;
    }
    Some((lo, hi))
}
