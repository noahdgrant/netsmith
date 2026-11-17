//! The current a track or a via carries, by IPC-2221 (ADR 0036): the formula the design stage
//! sizes each net's tracks with (`netsmith_design::guidelines`, whose header has the decisions)
//! and the design rules check holds the copper of a branch to (issue #1113). It is here, below
//! both, so the two size copper with one formula.

/// One mil, in mm.
const MIL: f64 = 0.0254;

// The formula and its constants (ADR 0036).
//
// IPC-2221B, "Generic Standard on Printed Board Design", section 6.2 "Conductor current
// carrying capacity" (the same fit is in IPC-2221A and, as the chart IPC-2221's Figure 6-4,
// in IPC-D-275): `I = k * dT^b * A^c`, with
//   I   the current in amperes,
//   dT  the temperature rise above ambient in degrees Celsius (= kelvin),
//   A   the conductor's cross-section in square mils (width times copper thickness),
//   b = 0.44, c = 0.725,
//   k = 0.048 for a conductor on an outer layer (in air) and 0.024 on an inner layer.
// The standard is paywalled and was not read here, so the constants are PROVISIONAL until a person
// compares them with it (issue #10); the formula, the constants and their meaning were taken from
// several agreeing published derivations (Altium's IPC-2221 calculator page, the `pcb-toolkit`
// crate's `current.rs`, circuit.rocks), and checked by hand in `tests/guidelines.rs`. It is an empirical fit to old measurements of tracks in
// air, and the calculators say it holds only inside the ranges of those measurements (which
// this code did not verify against the standard): outside them the result is extrapolated.
//
// Assumed, not from the standard (the tool's choices, accepted as built, ADR 0036): the copper
// weight is the process's `stackup.copper_weight` (1 oz, 0.035 mm, for JLCPCB's two-layer outer
// copper), and the rise is the board's `.process.max_temperature_rise` or, when it states none,
// `netsmith_design::guidelines::DEFAULT_TEMPERATURE_RISE`. A net's width is for the outer layers, so `K_EXTERNAL`; on a
// board with an inner signal layer the net also gets the width for that layer, `K_INTERNAL` in
// the inner copper (`stackup.inner_copper`), which the router draws its tracks there at (issue
// #833).

/// The IPC-2221 constant `k` for a track on an outer layer, in air.
const K_EXTERNAL: f64 = 0.048;
/// The IPC-2221 constant `k` for a track on an inner layer.
const K_INTERNAL: f64 = 0.024;

/// Where a track is: outer layers shed heat to the air, inner layers do not, so IPC-2221
/// allows an inner track half the current.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    External,
    Internal,
}

impl Layer {
    fn k(self) -> f64 {
        match self {
            Self::External => K_EXTERNAL,
            Self::Internal => K_INTERNAL,
        }
    }
}

/// The current a track of `width` mm in copper `copper` mm thick carries for a temperature
/// rise of `rise` kelvin, in amperes: IPC-2221's `I = k * dT^0.44 * A^0.725`, with the
/// cross-section `A` in square mils.
pub fn ipc2221_current(width: f64, copper: f64, rise: f64, layer: Layer) -> f64 {
    let area = (width / MIL) * (copper / MIL);
    layer.k() * rise.powf(0.44) * area.powf(0.725)
}

/// The narrowest track in mm, in copper `copper` mm thick, that carries `current` amperes
/// for a rise of `rise` kelvin: the formula of [`ipc2221_current`] solved for the area.
pub fn ipc2221_width(current: f64, copper: f64, rise: f64, layer: Layer) -> f64 {
    let area = (current / (layer.k() * rise.powf(0.44))).powf(1.0 / 0.725);
    area / (copper / MIL) * MIL
}

/// The current a via with a hole of `drill` mm, plated `plating` mm thick, carries for a rise of
/// `rise` kelvin (issue #431): IPC-2221's formula over the copper of the barrel, the ring of the
/// plating round the finished hole, with the inner-layer constant, since the barrel is
/// inside the board and sheds less heat than a track in air. Calculators differ here (some take
/// the outer constant, some IPC-2152's charts); the inner one is the lower of IPC-2221's two. A
/// 0.2 mm hole plated 0.018 mm carries 0.56 A at 10 K.
pub fn via_current(drill: f64, plating: f64, rise: f64) -> f64 {
    if drill <= 0.0 || plating <= 0.0 {
        return 0.0;
    }
    // A drill is the finished hole: the plating is round it.
    let inner = drill / 2.0;
    let outer = inner + plating;
    let area = std::f64::consts::PI * (outer * outer - inner * inner) / (MIL * MIL);
    Layer::Internal.k() * rise.powf(0.44) * area.powf(0.725)
}

/// How copper is sized for a current on one board (issue #1113): the process's copper, the
/// temperature rise the board allows and the fabricator's width tolerance, as the design stage
/// sizes a net's tracks with them (ADR 0036, amendment 5: a track is sized for the narrowest
/// finished track the tolerance allows), and the process's via.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackSizing {
    /// The outer copper's thickness, in mm.
    pub copper: f64,
    /// The copper of an inner signal layer, in mm; `None` when the board has none.
    pub inner_copper: Option<f64>,
    /// The temperature rise, in kelvin.
    pub rise: f64,
    /// The part of a track's width the finished track may lose (0.2 for 20 %).
    pub tolerance: f64,
    /// The process's via: its finished hole and its plating, in mm.
    pub via_drill: f64,
    pub via_plating: f64,
}

impl TrackSizing {
    /// The narrowest track on an outer layer that carries `current` amperes, in mm, before the
    /// process's least track.
    pub fn least_width(&self, current: f64) -> f64 {
        self.width(current, self.copper, Layer::External)
    }

    /// The same on an inner signal layer, in the inner copper; `None` when the board has none.
    pub fn inner_least_width(&self, current: f64) -> Option<f64> {
        self.inner_copper
            .filter(|&c| c > 0.0)
            .map(|c| self.width(current, c, Layer::Internal))
    }

    /// The least width on a layer: an inner one when `inner`, where the board has inner copper.
    pub fn least_width_on(&self, current: f64, inner: bool) -> f64 {
        match inner {
            true => self
                .inner_least_width(current)
                .unwrap_or_else(|| self.least_width(current)),
            false => self.least_width(current),
        }
    }

    /// How many of the process's vias, side by side, carry `current` amperes: at least one.
    pub fn vias(&self, current: f64) -> usize {
        let each = via_current(self.via_drill, self.via_plating, self.rise);
        if each > 0.0 && current > 0.0 {
            ((current / each) - 1e-9).ceil().max(1.0) as usize
        } else {
            1
        }
    }

    fn width(&self, current: f64, copper: f64, layer: Layer) -> f64 {
        if current <= 0.0 || copper <= 0.0 || self.rise <= 0.0 {
            return 0.0;
        }
        ipc2221_width(current, copper, self.rise, layer) / (1.0 - self.tolerance.clamp(0.0, 0.9))
    }
}
