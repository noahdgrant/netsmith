//! Generic passives (ADR 0047, provisional): a part the solver chooses by value alone, and
//! the step that later resolves it to a row of a catalog.
//!
//! A generic resistor is a value of an E-series (E12, E24 or E96) with a tolerance, a package
//! and a power rating, and no supplier. The solver sees it as one more candidate, so it is
//! never stuck on a value that no row of the catalog has. [`resolve`] runs afterwards, when
//! the bill of materials or the fab package is made: it finds the row of the catalog that is
//! at least as good (Basic first), or says which rows are nearest.
//!
//! Only resistors are generic so far. A capacitor also needs a bias curve (ADR 0013), which a
//! generic part has no source for.

use netsmith_check::FieldKind;
use netsmith_quantity::{Dimension, Quantity};

use crate::catalog::{Candidate, Catalogs, Library, Value};

/// An E-series: the values of one decade that a tolerance class is built on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Series {
    E12,
    E24,
    E96,
}

impl Series {
    /// `E12`, `E24` or `E96`, in any case.
    pub fn parse(word: &str) -> Option<Series> {
        match word.trim().to_ascii_uppercase().as_str() {
            "E12" => Some(Series::E12),
            "E24" => Some(Series::E24),
            "E96" => Some(Series::E96),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Series::E12 => "E12",
            Series::E24 => "E24",
            Series::E96 => "E96",
        }
    }

    /// The values of one decade, from 1.0 up to but not including 10, as the standard
    /// writes them (IEC 60063).
    fn decade(self) -> &'static [&'static str] {
        match self {
            Series::E12 => &[
                "1.0", "1.2", "1.5", "1.8", "2.2", "2.7", "3.3", "3.9", "4.7", "5.6", "6.8", "8.2",
            ],
            Series::E24 => &[
                "1.0", "1.1", "1.2", "1.3", "1.5", "1.6", "1.8", "2.0", "2.2", "2.4", "2.7", "3.0",
                "3.3", "3.6", "3.9", "4.3", "4.7", "5.1", "5.6", "6.2", "6.8", "7.5", "8.2", "9.1",
            ],
            Series::E96 => &[
                "1.00", "1.02", "1.05", "1.07", "1.10", "1.13", "1.15", "1.18", "1.21", "1.24",
                "1.27", "1.30", "1.33", "1.37", "1.40", "1.43", "1.47", "1.50", "1.54", "1.58",
                "1.62", "1.65", "1.69", "1.74", "1.78", "1.82", "1.87", "1.91", "1.96", "2.00",
                "2.05", "2.10", "2.15", "2.21", "2.26", "2.32", "2.37", "2.43", "2.49", "2.55",
                "2.61", "2.67", "2.74", "2.80", "2.87", "2.94", "3.01", "3.09", "3.16", "3.24",
                "3.32", "3.40", "3.48", "3.57", "3.65", "3.74", "3.83", "3.92", "4.02", "4.12",
                "4.22", "4.32", "4.42", "4.53", "4.64", "4.75", "4.87", "4.99", "5.11", "5.23",
                "5.36", "5.49", "5.62", "5.76", "5.90", "6.04", "6.19", "6.34", "6.49", "6.65",
                "6.81", "6.98", "7.15", "7.32", "7.50", "7.68", "7.87", "8.06", "8.25", "8.45",
                "8.66", "8.87", "9.09", "9.31", "9.53", "9.76",
            ],
        }
    }

    /// Every value of the series from 1 ohm up to 9.76 Mohm, rising. A value is the nearest
    /// double to its decimal digits, so `3.3` times 100 is exactly 330.
    pub fn resistances(self) -> Vec<f64> {
        (0..=6)
            .flat_map(|exp| {
                self.decade().iter().map(move |m| {
                    format!("{m}e{exp}")
                        .parse::<f64>()
                        .expect("a table entry is a number")
                })
            })
            .collect()
    }
}

/// The packages a generic resistor can be, in the order they are tried, with the power each
/// is rated for. These are the common ratings of the package sizes, not a supplier's data.
const PACKAGES: [(&str, f64); 3] = [("0402", 0.0625), ("0603", 0.1), ("0805", 0.125)];

/// The tolerance of a generic resistor, in percent. E24 and E96 values are made at 1%.
const TOLERANCE_PERCENT: f64 = 1.0;

/// What a generic part asks of the part that stands for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Generic {
    /// The catalog its row must be in (`resistor`).
    pub catalog: String,
    pub series: Series,
    /// The value, in ohms.
    pub nominal: f64,
    pub tolerance_percent: f64,
    pub package: String,
    /// The power it must be able to dissipate, in watts.
    pub max_power: f64,
}

impl Generic {
    /// `340 ohm 1% 0402 62.5 mW (E24)`.
    pub fn describe(&self) -> String {
        format!(
            "{} ohm {}% {} {} mW ({})",
            number(self.nominal),
            number(self.tolerance_percent),
            self.package,
            number(self.max_power * 1000.0),
            self.series.name()
        )
    }
}

/// A number the way a person writes it: `62.5`, `340`, with no trailing zeros.
fn number(x: f64) -> String {
    let text = format!("{x:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The schematic's value field for a resistance: `330`, `2.2k`, `1M`.
fn label(ohms: f64) -> String {
    let (scaled, suffix) = if ohms >= 1e6 {
        (ohms / 1e6, "M")
    } else if ohms >= 1e3 {
        (ohms / 1e3, "k")
    } else {
        (ohms, "")
    };
    format!("{}{suffix}", number(scaled))
}

/// The generic candidates of a catalog, in the order they are tried (smallest package first,
/// then rising value), if the catalog is one that has generic parts and the part lists only
/// columns they can fill. `None` otherwise.
pub fn candidates(
    series: Series,
    catalog: &str,
    columns: &[(String, FieldKind)],
) -> Option<Vec<Candidate>> {
    if catalog != "resistor" {
        return None;
    }
    let known = |name: &str| ["resistance", "max_power", "package"].contains(&name);
    if !columns.iter().all(|(name, _)| known(name))
        || !columns.iter().any(|(n, _)| n == "resistance")
    {
        return None;
    }
    let tolerance = Quantity::literal(TOLERANCE_PERCENT, Some("%"))
        .ok()?
        .quantity;
    let mut out = Vec::new();
    for (package, power) in PACKAGES {
        for ohms in series.resistances() {
            let center = Quantity::point(Dimension::RESISTANCE, ohms).ok()?;
            let generic = Generic {
                catalog: catalog.to_string(),
                series,
                nominal: ohms,
                tolerance_percent: TOLERANCE_PERCENT,
                package: package.to_string(),
                max_power: power,
            };
            let mut values = Vec::new();
            for (name, _) in columns {
                let value = match name.as_str() {
                    "resistance" => {
                        Value::Quantity(Quantity::tolerance(&center, &tolerance, true).ok()?)
                    }
                    "max_power" => Value::Quantity(Quantity::point(Dimension::POWER, power).ok()?),
                    _ => Value::Text(package.to_string()),
                };
                values.push((name.clone(), value));
            }
            out.push(Candidate {
                name: format!("generic {}", generic.describe()),
                label: label(ohms),
                columns: values,
                generic: Some(generic),
                ..Candidate::default()
            });
        }
    }
    Some(out)
}

/// The part a generic part resolved to.
#[derive(Debug, Clone, PartialEq)]
pub struct Orderable {
    pub lcsc: Option<String>,
    pub mpn: Option<String>,
    pub library: Option<Library>,
    /// What a message calls the row.
    pub name: String,
    /// Rows that fit but were passed over because a provider said they have no stock.
    pub passed_over: Vec<String>,
}

fn rank(library: Option<Library>) -> u8 {
    match library {
        Some(Library::Basic) => 0,
        Some(Library::Preferred) => 1,
        Some(Library::Extended) => 2,
        None => 3,
    }
}

/// The columns a generic resistor is matched on.
fn kinds() -> Vec<(String, FieldKind)> {
    vec![
        (
            "resistance".to_string(),
            FieldKind::Quantity(Dimension::RESISTANCE),
        ),
        (
            "max_power".to_string(),
            FieldKind::Quantity(Dimension::POWER),
        ),
        ("package".to_string(), FieldKind::Text),
    ]
}

fn quantity_of(c: &Candidate, key: &str) -> Option<Quantity> {
    match c.column(key) {
        Some(Value::Quantity(q)) => Some(*q),
        _ => None,
    }
}

/// Find the row of the catalog that stands for a generic part, or say why there is none.
///
/// A row fits when it has the package, the nominal value (to a millionth), a tolerance no
/// wider and a power rating no lower. Among the rows that fit the order is the
/// preferred-parts list: Basic, then Preferred, then Extended, and inside a class the order
/// of the file. `stock` is asked about each fitting row's LCSC number: a row it answers `0`
/// for is passed over, and a row it knows nothing about (`None`) is kept.
///
/// The error is the message for the part: what was asked for, and the nearest rows.
pub fn resolve(
    generic: &Generic,
    catalogs: &Catalogs,
    stock: &dyn Fn(&str) -> Option<u64>,
) -> Result<Orderable, String> {
    let Some(Ok(catalog)) = catalogs.identity(&generic.catalog, &kinds()) else {
        return Err(format!(
            "unresolved: there is no readable `{}` catalog to find {} in",
            generic.catalog,
            generic.describe()
        ));
    };
    let rows: Vec<&Candidate> = catalog
        .candidates
        .iter()
        .filter(|c| c.lcsc.is_some() || c.mpn.is_some())
        .collect();
    let want_lo = generic.nominal * (1.0 - generic.tolerance_percent / 100.0);
    let want_hi = generic.nominal * (1.0 + generic.tolerance_percent / 100.0);
    let slack = generic.nominal * 1e-9;
    let mut fits: Vec<&Candidate> = rows
        .iter()
        .copied()
        .filter(|c| {
            let Some(r) = quantity_of(c, "resistance") else {
                return false;
            };
            let power_ok = quantity_of(c, "max_power").is_some_and(|p| p.lo() >= generic.max_power);
            matches!(c.column("package"), Some(Value::Text(p)) if *p == generic.package)
                && (r.mid() - generic.nominal).abs() <= generic.nominal * 1e-6
                && r.lo() >= want_lo - slack
                && r.hi() <= want_hi + slack
                && power_ok
        })
        .collect();
    fits.sort_by_key(|c| rank(c.library));
    let mut passed_over = Vec::new();
    for c in fits.iter().copied() {
        let in_stock = c.lcsc.as_deref().and_then(stock);
        if in_stock == Some(0) {
            passed_over.push(format!("{} has no stock", c.name));
            continue;
        }
        return Ok(Orderable {
            lcsc: c.lcsc.clone(),
            mpn: c.mpn.clone(),
            library: c.library,
            name: c.name.clone(),
            passed_over,
        });
    }

    let mut message = format!(
        "unresolved: no row of the `{}` catalog can be ordered for {}.",
        generic.catalog,
        generic.describe()
    );
    if !passed_over.is_empty() {
        message.push_str(&format!(
            " Rows that fit were passed over: {}.",
            passed_over.join("; ")
        ));
    }
    let mut near: Vec<(&Candidate, f64)> = rows
        .iter()
        .copied()
        .filter_map(|c| {
            let r = quantity_of(c, "resistance")?.mid();
            (r > 0.0).then(|| (c, (r / generic.nominal).ln().abs()))
        })
        .filter(|(c, _)| !fits.iter().any(|f| std::ptr::eq(*f, *c)))
        .collect();
    let same_package = |c: &Candidate| matches!(c.column("package"), Some(Value::Text(p)) if *p == generic.package);
    near.sort_by(|a, b| {
        (!same_package(a.0))
            .cmp(&!same_package(b.0))
            .then(a.1.total_cmp(&b.1))
            .then(rank(a.0.library).cmp(&rank(b.0.library)))
    });
    let alternatives: Vec<String> = near
        .iter()
        .take(3)
        .map(|(c, _)| {
            let r = quantity_of(c, "resistance").map_or(0.0, |q| q.mid());
            let percent = (r - generic.nominal) / generic.nominal * 100.0;
            let library = c.library.map(|l| format!(", {l}")).unwrap_or_default();
            format!("{} ({percent:+.1}%{library})", c.name)
        })
        .collect();
    if alternatives.is_empty() {
        message.push_str(" The catalog has no resistor to offer instead.");
    } else {
        message.push_str(&format!(
            " Nearest in the catalog: {}.",
            alternatives.join(", ")
        ));
    }
    message.push_str(
        " Change the value in the design and build again: the solver then checks every relation again with it.",
    );
    Err(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_series_have_their_standard_number_of_values_in_each_decade() {
        assert_eq!(Series::E12.decade().len(), 12);
        assert_eq!(Series::E24.decade().len(), 24);
        assert_eq!(Series::E96.decade().len(), 96);
        let v = Series::E24.resistances();
        assert_eq!(v.len(), 24 * 7);
        assert!(v.windows(2).all(|w| w[0] < w[1]), "rising");
        assert!(v.contains(&330.0) && v.contains(&2200.0) && v.contains(&9.1e6));
    }

    #[test]
    fn a_value_is_written_the_way_the_catalog_rows_write_it() {
        assert_eq!(label(330.0), "330");
        assert_eq!(label(2200.0), "2.2k");
        assert_eq!(label(1e6), "1M");
        assert_eq!(label(4.7), "4.7");
    }

    #[test]
    fn series_are_read_in_any_case() {
        assert_eq!(Series::parse("e96"), Some(Series::E96));
        assert_eq!(Series::parse("E48"), None);
    }
}
