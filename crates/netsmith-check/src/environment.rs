//! The comparison of a board's environment with the temperature range of a part (ADR 0099).
//!
//! One rule, used by the solver when it chooses a row and by the design stage when it judges
//! every fitted part: the hot end of the environment must be at or under the part's highest
//! temperature, and the cold end at or over its lowest. A part that states one end only is
//! judged on that end; an end that nothing states is not judged, and the caller names it.
//! Temperatures are kelvin everywhere and are shown with their Celsius value beside them, in
//! the language's own spelling (`degC`, ADR 0103), because a datasheet prints Celsius.

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

/// The operating temperature range of a dielectric class of ceramic capacitor, for a catalog
/// row that names its class (`dielectric`) and states no `temp_min` or `temp_max` of its own.
///
/// A class range is a property of the class CODE, not of a particular part. It is the
/// operating temperature range that makers print for the code, the range over which the class
/// guarantees its limit on the change of capacitance (X7R: within +/-15 % from -55 to 125 degC;
/// C0G: within 0 +/-30 ppm/degC), so a part of the class may be rated narrower. A figure in a
/// row's own `temp_min` or `temp_max` (read from the datasheet) always wins, one end at a time,
/// and every message that rests on a class range says so ("from the class X7R, per ...").
///
/// Sources, each read on 2026-11-11 (the day is the owner's decision's, ENV2-C; the
/// documents' dates are their own). Three makers state the same three ranges, and none
/// disagrees:
///
/// - Murata, "Chip Multilayer Ceramic Capacitors for Automotive", catalog C03E (file
///   `C03E.pdf`, dated Nov. 22, 2019), PDF page 2 (printed page 5), "Temperature Characteristics"
///   table, column "Operating Temperature Range": `5C` C0G -55 to 125 degC (0 +/-30 ppm/degC,
///   reference 25 degC), `R6` X5R -55 to 85 degC (+/-15 %), `R7` X7R -55 to 125 degC (+/-15 %);
///   the table marks all three "EIA" in its code column. Read from the copy at
///   `https://static.chipdip.ru/lib/301/DOC041301196.pdf` (a mirror, because the catalog's own
///   address at murata.com could not be fetched).
/// - Samsung Electro-Mechanics, "MLCC" catalog of November 2015 (the PDF of
///   `https://datasheet.lcsc.com/datasheet/pdf/02336ea48ea44ca18c72517dd3cb7b47.pdf`), PDF
///   page 5 (printed 4 and 5), "Part Numbering System": Class I `C` C0G "Operation
///   Temperature Range" -55 to +125 degC, Class II `A` X5R -55 to +85, `B` X7R -55 to +125
///   (and `F` Y5V -30 to +85, `X` X6S -55 to +105, `Y` X7S and `Z` X7T -55 to +125, which are not
///   in this table because no catalog row has them).
/// - Yageo, "Surface-Mount Ceramic Multilayer Capacitors, General Purpose & High Capacitance,
///   Class 2, X7R, 6.3 V to 250 V", Product Specification Nov. 19, 2024 V.26 (the PDF of
///   `https://datasheet.lcsc.com/datasheet/pdf/23ccee80ee542e7cf156a772bb589942.pdf`), PDF page
///   14: "Operating temperature range: -55 degC to +125 degC" beside "Maximum capacitance change
///   as a function of temperature: +/-15 %". X7R only.
///
/// EIA-198, the standard that defines the codes, was NOT read (it is paywalled): the figures
/// are the makers' statements of the EIA codes, and KEMET's and TDK's documents could not be
/// fetched (KEMET's server had an expired certificate, TDK's refused the request). X5R and C0G
/// have two makers each (Murata and Samsung) and X7R three. A class that no source read here
/// states is not in the table, and a row of such a class stays unjudged and named (ADR 0099);
/// a class is added with the catalog rows that have it (ADR 0055) and its own citation.
#[derive(Debug, PartialEq)]
pub struct ClassRange {
    /// The class as the catalog's `dielectric` cell spells it (the variant of
    /// `std::parts::passive::Dielectric`): `X7R`.
    pub class: &'static str,
    /// The lowest and the highest operating temperature of the class, in kelvin.
    pub min: f64,
    pub max: f64,
    /// The documents the range was read from, as the messages quote them.
    pub source: &'static str,
}

/// The classes with a cited range (see [`ClassRange`]).
pub const CLASS_RANGES: [ClassRange; 3] = [
    ClassRange {
        class: "C0G",
        min: 218.15,
        max: 398.15,
        source: "Murata C03E (2019-11-22) p.2 and Samsung MLCC catalog (2015-11) p.5",
    },
    ClassRange {
        class: "X5R",
        min: 218.15,
        max: 358.15,
        source: "Murata C03E (2019-11-22) p.2 and Samsung MLCC catalog (2015-11) p.5",
    },
    ClassRange {
        class: "X7R",
        min: 218.15,
        max: 398.15,
        source: "Murata C03E (2019-11-22) p.2, Samsung MLCC catalog (2015-11) p.5 and Yageo X7R datasheet V.26 (2024-11-19) p.14",
    },
];

/// The range of a dielectric class by the name a catalog's `dielectric` cell gives it, or
/// `None` for a class with no cited range.
pub fn class_range(class: &str) -> Option<&'static ClassRange> {
    CLASS_RANGES.iter().find(|r| r.class == class)
}

impl ClassRange {
    /// What a message says of an end that this class states, in place of a datasheet's figure:
    /// the figure's origin is the class code and not the part.
    pub fn from_class(&self) -> String {
        format!(
            "from the class {}, per {}: a property of the class code and not of this part, which may be rated narrower, so a `temp_min` or `temp_max` of the catalog row or of the design overrides it",
            self.class, self.source
        )
    }
}

/// `358.15 K (85degC)`: a temperature as the messages show it.
pub fn show(kelvin: f64) -> String {
    format!("{} K ({}degC)", trim(kelvin), trim(kelvin - 273.15))
}

/// `233.15 K to 398.15 K (-40degC to 125degC)`, or one temperature when the ends are equal.
pub fn show_range(lo: f64, hi: f64) -> String {
    if lo == hi {
        show(lo)
    } else {
        format!(
            "{} K to {} K ({}degC to {}degC)",
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
    fn every_class_range_cites_its_sources() {
        for r in &CLASS_RANGES {
            assert!(!r.source.trim().is_empty(), "{} has no source", r.class);
            assert!(
                r.source.contains("p."),
                "{} cites no page: {}",
                r.class,
                r.source
            );
            assert!(r.min < r.max, "{}", r.class);
        }
        // Each class once, under the spelling of `std::parts::passive::Dielectric`.
        let mut names: Vec<_> = CLASS_RANGES.iter().map(|r| r.class).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names, ["C0G", "X5R", "X7R"]);
    }

    #[test]
    fn the_cited_class_limits_are_the_makers() {
        // Murata C03E p.2, Samsung's MLCC catalog p.5 and Yageo's X7R datasheet p.14.
        let range = |c| {
            let r = class_range(c).unwrap();
            (r.min, r.max)
        };
        assert_eq!(range("C0G"), (218.15, 398.15));
        assert_eq!(range("X5R"), (218.15, 358.15));
        assert_eq!(range("X7R"), (218.15, 398.15));
        // A class no source read here states is not guessed.
        assert!(class_range("Y5V").is_none());
        assert!(class_range("x7r").is_none());
    }

    #[test]
    fn a_class_end_says_it_is_the_classes_and_not_the_parts() {
        let said = class_range("X7R").unwrap().from_class();
        assert!(
            said.starts_with("from the class X7R, per Murata C03E"),
            "{said}"
        );
        assert!(said.contains("not of this part"), "{said}");
    }

    #[test]
    fn messages_show_kelvin_with_degc() {
        assert_eq!(show(358.15), "358.15 K (85degC)");
        assert_eq!(
            show_range(233.15, 398.15),
            "233.15 K to 398.15 K (-40degC to 125degC)"
        );
        assert_eq!(show(300.0), "300 K (26.85degC)");
    }
}
