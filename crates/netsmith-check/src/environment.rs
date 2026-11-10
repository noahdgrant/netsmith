//! The comparison of a board's environment with the temperature range of a part (ADR 0099).
//!
//! One rule, used by the solver when it chooses a row and by the design stage when it judges
//! every fitted part: the hot end of the environment must be at or under the part's highest
//! temperature, and the cold end at or over its lowest. A part that states one end only is
//! judged on that end; an end that nothing states is not judged, and the caller names it.
//! Temperatures are kelvin everywhere and are shown with their Celsius value beside them,
//! because a datasheet prints Celsius.

/// What the comparison found for each end of the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fit {
    /// The cold end against the part's lowest temperature: `Some(true)` it is at or over it,
    /// `Some(false)` it is under it, `None` the part states none.
    pub cold: Option<bool>,
    /// The hot end against the part's highest temperature, the same way.
    pub hot: Option<bool>,
}

impl Fit {
    /// `environment` is the board's range, `(lowest, highest)` in kelvin; `min` and `max` the
    /// part's. Where the part states a range of its own (`.temp_max &= 350K..360K`), the caller
    /// passes the end that is worse for the design: the highest of the lowest temperature and the
    /// lowest of the highest.
    pub fn of(environment: (f64, f64), min: Option<f64>, max: Option<f64>) -> Self {
        const EPS: f64 = 1e-9;
        Fit {
            cold: min.map(|m| environment.0 >= m - EPS),
            hot: max.map(|m| environment.1 <= m + EPS),
        }
    }

    /// Whether the part's range does not cover the environment at a stated end.
    pub fn refused(&self) -> bool {
        self.cold == Some(false) || self.hot == Some(false)
    }

    /// Whether nothing is wrong at the ends the part states, and at least one end is not stated:
    /// the part is kept, and a finding says what was not judged.
    pub fn unjudged(&self) -> bool {
        !self.refused() && (self.cold.is_none() || self.hot.is_none())
    }

    /// The ends that nothing stated, as the members to state: `temp_min`, `temp_max`.
    pub fn missing(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.cold.is_none() {
            out.push("temp_min");
        }
        if self.hot.is_none() {
            out.push("temp_max");
        }
        out
    }
}

/// `358.15 K (85 C)`: a temperature as the messages show it.
pub fn show(kelvin: f64) -> String {
    format!("{} K ({} C)", trim(kelvin), trim(kelvin - 273.15))
}

/// `233.15 K to 398.15 K (-40 C to 125 C)`, or one temperature when the ends are equal.
pub fn show_range(lo: f64, hi: f64) -> String {
    if lo == hi {
        show(lo)
    } else {
        format!(
            "{} K to {} K ({} C to {} C)",
            trim(lo),
            trim(hi),
            trim(lo - 273.15),
            trim(hi - 273.15)
        )
    }
}

/// A number with at most three decimals and no trailing zeros: the float noise of
/// `358.15 - 273.15` is not shown.
fn trim(x: f64) -> String {
    let text = format!("{:.3}", (x * 1000.0).round() / 1000.0);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_string()
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_end_is_judged_on_its_own() {
        let env = (233.15, 398.15);
        let fit = Fit::of(env, Some(233.15), Some(398.15));
        assert_eq!((fit.refused(), fit.unjudged()), (false, false));
        assert!(Fit::of(env, Some(233.15), Some(358.15)).refused());
        assert!(Fit::of(env, Some(248.15), Some(398.15)).refused());
        let hot_only = Fit::of(env, None, Some(423.15));
        assert!(hot_only.unjudged());
        assert_eq!(hot_only.missing(), ["temp_min"]);
        assert_eq!(Fit::of(env, None, None).missing(), ["temp_min", "temp_max"]);
        // A refused end is refused whatever the other end says.
        assert!(Fit::of(env, None, Some(358.15)).refused());
    }

    #[test]
    fn messages_show_kelvin_with_celsius() {
        assert_eq!(show(358.15), "358.15 K (85 C)");
        assert_eq!(
            show_range(233.15, 398.15),
            "233.15 K to 398.15 K (-40 C to 125 C)"
        );
        assert_eq!(show(300.0), "300 K (26.85 C)");
    }
}
