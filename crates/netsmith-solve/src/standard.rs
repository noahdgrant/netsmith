//! What the solver knows about the catalogs of the standard generics that have synthesized
//! parts (ADR 0068 stage 3).
//!
//! `Resistor`, `Capacitor`, `Inductor` and `Led` are generics of the standard library that are
//! their own first provider (ADR 0068 section 7): each has the catalog its `catalog` statement
//! names, and, when the project states an E-series, a second provider that has no supplier, the
//! synthesized one of ADR 0047. The language cannot say how that provider makes its values
//! (ADR 0068 open question 10), so it is Rust, and this is the one place that names the four.
//! Everything else that used to be written four times is a field of an entry here:
//!
//! - the columns the synthesized provider can fill, and how it fills each of them, which is
//!   also the check that a part's `catalog` statement lists only columns it can fill;
//! - the kind of each column when a row is read again for the bill of materials;
//! - the column that holds the value a design is expected to choose or bound (ADR 0065);
//! - the columns a file of rows may leave out (ADR 0055, ADR 0058): they are the ones the
//!   synthesized provider leaves empty. A synthesized part has no `temperature_coefficient`,
//!   no `dielectric` and no `rms_current`, so a row of the catalog that is empty there must be
//!   offered to a design that does not ask for the column, or the synthesized part, which is
//!   empty there too, would fit no design at all.
//!
//! A catalog is a name that every part listing it shares (ADR 0022: `resistor.csv` is read by
//! every part with `catalog resistor`), so the entries are found by the catalog's name. The
//! `generic` of an entry is the standard generic whose own `catalog` statement it describes: a
//! test elaborates each of the four from the standard library and compares the columns, so an
//! entry cannot drift from the definition it belongs to.
//!
//! # Where Rust still names a standard generic (ADR 0068, stage 3)
//!
//! `grep -nE '"(resistor|capacitor|inductor|led)"|"Capacitor"' crates/netsmith-solve/src
//! crates/netsmith-check/src` cannot be empty: Rust that serves a generic names it once. What
//! stage 3 did was make this one table instead of seven lists (`generic.rs` had a `match` on
//! the four catalog names for the columns, the value column, the part word and the kinds of a
//! row; `table.rs` had `OPTIONAL` and `VALUE_COLUMNS`). The optional columns are derived, not
//! kept: they are the ones the synthesized provider leaves empty, and they must be the same, so
//! the list was one fact written twice. What stays, each with its reason:
//!
//! - `generic.rs`: `Spec`, `Need`, `Sense`, the package and tolerance tables of each series
//!   (`PACKAGES`, `CAPACITOR_TOLERANCES`, ...), `Series::for_reactive`, `with_needs`, `misses`
//!   and `nearest` are the synthesized provider itself, which stays Rust (the language cannot say
//!   how a value is generated). The entry is found by the catalog's name, not the generic's: a
//!   catalog is one name shared by every part that lists it, so the columns a file may leave
//!   out and the series belong to the catalog (ADR 0052 rejected the catalog's name for a
//!   bank, because a bank is a property of the part; these three are properties of the
//!   catalog).
//! - `netsmith-check/src/model.rs`, `BANKS` and `Model::bank`: the one list of generics that are
//!   banks (`CapacitorBank`, whose parts are `Capacitor`s, ADR 0123) and the one place that
//!   reads it, keyed by `Instance.standard`. The solver and the design stage ask `Model::bank`
//!   and name no generic. `CapacitorBank` has the catalog `capacitor` of `Capacitor`, so the
//!   `CAPACITOR` entry below serves both and a test compares the two `catalog` statements.
//! - `netsmith-view/src/layout.rs` `is_series_kind` (`Inductor`, `FerriteBead`, `Fuse`) and
//!   `netsmith-pcb/src/quality.rs` `is_capacitor_kind` read the kind and `Part.provides`: a
//!   project's own part called `Inductor` has been drawn in series by its kind since before
//!   generics, and the unit tests build a `Part` by hand with a kind and no `provides`. They
//!   are about different things (what a layout hangs in series, what the placer calls a
//!   decoupling capacitor).
//! - `netsmith-design/src/lib.rs` `default_value`: `"Led"` shows `LED` as the value of an LED the
//!   solver did not pick. It is a display default the language cannot declare, and reading
//!   `provides` would change that line of a bill of materials for a provider.
//! - `hand_soldered`, `is_connector`, `is_mechanical` and `Label` name parts, not generics (the
//!   connector and mechanical parts are parts); `Mating`, `Logic`, `Place`, `Outline`,
//!   `Cutout`, `Keepout` and `I2c` are interfaces of the standard library read by name.

use netsmith_check::FieldKind;
use netsmith_quantity::Dimension;

use crate::catalog::Candidate;
use crate::generic::{self, Sense, Series, Spec};

/// How a synthesized part fills one column of its catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fill {
    /// The marked value of the series, with the tolerance of the part: the number the part is
    /// chosen by.
    Marked,
    /// The power the part's package is usually rated for (a resistor's `max_power`).
    PackagePower,
    /// The marked value as a capacitance that derates with the bias, with no curve: a generic
    /// capacitor has none, and resolves only to a row that has one.
    NoCurve,
    /// A rating or a stated value the part cannot know, left open for the design to supply
    /// when the part is picked.
    Open(Sense),
    /// The name of the package.
    Package,
    /// Nothing: the part has no such value, and the cell is empty.
    Empty,
}

/// One column of a standard catalog.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Column {
    pub name: &'static str,
    /// What a cell is read as when a row is read again to resolve a generic part.
    pub kind: FieldKind,
    pub fill: Fill,
    /// Whether the column holds the part's value, the number a design is expected to choose or
    /// bound (ADR 0065): a pick of such a part that nothing limits is a guideline finding. A
    /// rating is not a value.
    pub value: bool,
}

/// Makes the synthesized parts of a series for the columns a part's catalog lists.
type Make = fn(&Standard, Series, &[(String, FieldKind)]) -> Option<Vec<Candidate>>;

/// A standard generic's catalog and its synthesized provider.
pub(crate) struct Standard {
    /// The standard generic that owns the catalog (`Resistor`). Read by the test that compares
    /// the entry with the standard library.
    #[cfg_attr(not(test), allow(dead_code))]
    pub generic: &'static str,
    /// The catalog it reads (`resistor`).
    pub catalog: &'static str,
    /// What a message calls one of its parts.
    pub word: &'static str,
    /// The columns of the generic's `catalog` statement, in its order.
    pub columns: &'static [Column],
    /// The synthesized parts of a series, in the order they are tried.
    make: Make,
}

const fn quantity(name: &'static str, dim: Dimension, fill: Fill, value: bool) -> Column {
    Column {
        name,
        kind: FieldKind::Quantity(dim),
        fill,
        value,
    }
}

const fn text(name: &'static str, fill: Fill) -> Column {
    Column {
        name,
        kind: FieldKind::Text,
        fill,
        value: false,
    }
}

/// `catalog resistor { .resistance, .max_power, .package, .temperature_coefficient }`.
static RESISTOR: Standard = Standard {
    generic: "Resistor",
    catalog: "resistor",
    word: "resistor",
    columns: &[
        quantity("resistance", Dimension::RESISTANCE, Fill::Marked, true),
        quantity("max_power", Dimension::POWER, Fill::PackagePower, false),
        text("package", Fill::Package),
        // Read with the others; a generic part has none and no row is matched on it.
        quantity(
            "temperature_coefficient",
            Dimension::NONE,
            Fill::Empty,
            false,
        ),
    ],
    make: generic::resistors,
};

/// `catalog capacitor { .capacitance, .effective_capacitance, .max_voltage, .package,
/// .dielectric }`.
static CAPACITOR: Standard = Standard {
    generic: "Capacitor",
    catalog: "capacitor",
    word: "capacitor",
    columns: &[
        quantity("capacitance", Dimension::CAPACITANCE, Fill::Marked, true),
        quantity(
            "effective_capacitance",
            Dimension::CAPACITANCE,
            Fill::NoCurve,
            true,
        ),
        quantity(
            "max_voltage",
            Dimension::VOLTAGE,
            Fill::Open(Sense::AtLeast),
            false,
        ),
        text("package", Fill::Package),
        // Read with the others; a generic part has none and no row is matched on it. It is an
        // enum in the part and text here.
        text("dielectric", Fill::Empty),
    ],
    make: generic::capacitors,
};

/// `catalog inductor { .inductance, .saturation_current, .dc_resistance, .package,
/// .rms_current }`.
static INDUCTOR: Standard = Standard {
    generic: "Inductor",
    catalog: "inductor",
    word: "inductor",
    columns: &[
        quantity("inductance", Dimension::INDUCTANCE, Fill::Marked, true),
        quantity(
            "saturation_current",
            Dimension::CURRENT,
            Fill::Open(Sense::AtLeast),
            false,
        ),
        quantity(
            "dc_resistance",
            Dimension::RESISTANCE,
            Fill::Open(Sense::AtMost),
            false,
        ),
        text("package", Fill::Package),
        quantity("rms_current", Dimension::CURRENT, Fill::Empty, false),
    ],
    make: generic::inductors,
};

/// `catalog led { .color, .forward_voltage, .package }`.
static LED: Standard = Standard {
    generic: "Led",
    catalog: "led",
    word: "LED",
    columns: &[
        text("color", Fill::Open(Sense::Stated)),
        quantity(
            "forward_voltage",
            Dimension::VOLTAGE,
            Fill::Open(Sense::Stated),
            false,
        ),
        text("package", Fill::Package),
    ],
    make: generic::leds,
};

/// The four, in the order of the standard library.
static STANDARD: [&Standard; 4] = [&RESISTOR, &CAPACITOR, &LED, &INDUCTOR];

impl Standard {
    /// The standard catalog of this name, if it has synthesized parts.
    pub(crate) fn of_catalog(name: &str) -> Option<&'static Standard> {
        STANDARD.iter().copied().find(|s| s.catalog == name)
    }

    /// The standard generic a synthesized part stands for.
    pub(crate) fn of_spec(spec: &Spec) -> &'static Standard {
        match spec {
            Spec::Resistor { .. } => &RESISTOR,
            Spec::Capacitor { .. } => &CAPACITOR,
            Spec::Inductor { .. } => &INDUCTOR,
            Spec::Led { .. } => &LED,
        }
    }

    pub(crate) fn column(&self, name: &str) -> Option<&'static Column> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// How a synthesized part fills the column, if it can.
    pub(crate) fn fill(&self, name: &str) -> Option<Fill> {
        self.column(name).map(|c| c.fill)
    }

    /// The column that holds the marked value the series is made of, for a kind that has one:
    /// an LED has none.
    pub(crate) fn marked(&self) -> Option<&'static str> {
        self.columns
            .iter()
            .find(|c| c.fill == Fill::Marked)
            .map(|c| c.name)
    }

    /// The columns that hold the part's value (ADR 0065).
    pub(crate) fn value_columns(&self) -> Vec<&'static str> {
        self.columns
            .iter()
            .filter(|c| c.value)
            .map(|c| c.name)
            .collect()
    }

    /// Whether a file of rows may leave the column out, and a row may be empty in it, without
    /// the row leaving every design that does not mention it (ADR 0055).
    pub(crate) fn is_optional(&self, column: &str) -> bool {
        self.fill(column) == Some(Fill::Empty)
    }

    /// The columns a row is read with when it is resolved for a generic part.
    pub(crate) fn kinds(&self) -> Vec<(String, FieldKind)> {
        self.columns
            .iter()
            .map(|c| (c.name.to_string(), c.kind))
            .collect()
    }

    /// The synthesized parts of `series` for a part whose catalog statement lists `columns`, in
    /// the order they are tried, if it lists only columns they can fill and the one the series
    /// is made of. `None` otherwise.
    pub(crate) fn candidates(
        &'static self,
        series: Series,
        columns: &[(String, FieldKind)],
    ) -> Option<Vec<Candidate>> {
        if !columns.iter().all(|(name, _)| self.column(name).is_some()) {
            return None;
        }
        if let Some(marked) = self.marked()
            && !columns.iter().any(|(name, _)| name == marked)
        {
            return None;
        }
        (self.make)(self, series, columns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netsmith_check::{Loader, check};

    /// The columns of the `catalog` statement of each standard generic, in the order it lists
    /// them, as the checker reads them from the standard library.
    fn declared(generic: &str, module: &str) -> (String, Vec<String>) {
        let (name, columns, _) = declared_with_optional(generic, module);
        (name, columns)
    }

    /// [`declared`], and the columns that the statement marks `optional` (ADR 0078).
    fn declared_with_optional(generic: &str, module: &str) -> (String, Vec<String>, Vec<String>) {
        let source =
            format!("import std::parts::{module}::{generic}\nboard B {{\n    .x: {generic}\n}}\n");
        let checked = check("main.nsm", &source, &Loader::std());
        assert!(checked.is_ok(), "{}", checked.render());
        let model = &checked.boards[0].model;
        let part = model.inst_at("x").expect("the part is placed");
        let catalog = model
            .instance(part)
            .catalog
            .as_ref()
            .expect("a standard generic has a catalog");
        let prefix = format!("{}.", model.path_of(part));
        let name_of = |&f: &_| {
            let path = model.path_of_field(f);
            path.strip_prefix(&prefix).unwrap_or(&path).to_string()
        };
        let columns = catalog.fields.iter().map(name_of).collect();
        let optional = catalog.optional.iter().map(name_of).collect();
        (catalog.name.clone(), columns, optional)
    }

    #[test]
    fn the_standard_catalog_statements_mark_optional_the_columns_a_synthesized_part_leaves_empty() {
        // ADR 0078: `optional` in the `catalog` statement and `Fill::Empty` here say the same
        // thing for the four standard kinds, so the marker in the library is not a second
        // truth that could drift from the rule the solver also keeps under the catalog's name.
        for standard in STANDARD {
            let (_, _, marked) = declared_with_optional(standard.generic, "passive");
            let ours: Vec<&str> = standard
                .columns
                .iter()
                .filter(|c| standard.is_optional(c.name))
                .map(|c| c.name)
                .collect();
            assert_eq!(marked, ours, "{}", standard.generic);
        }
    }

    #[test]
    fn each_entry_describes_the_catalog_statement_of_its_standard_generic() {
        for standard in STANDARD {
            let (catalog, columns) = declared(standard.generic, "passive");
            assert_eq!(catalog, standard.catalog, "{}", standard.generic);
            let ours: Vec<&str> = standard.columns.iter().map(|c| c.name).collect();
            assert_eq!(columns, ours, "{}", standard.generic);
        }
    }

    #[test]
    fn a_capacitor_bank_reads_the_catalog_of_a_capacitor_column_for_column() {
        // ADR 0123: the members of a bank are `Capacitor`s of one row, so the bank's `catalog`
        // statement and `Capacitor`'s are one, and the `CAPACITOR` entry serves both.
        assert_eq!(
            declared_with_optional("CapacitorBank", "passive"),
            declared_with_optional("Capacitor", "passive")
        );
    }

    #[test]
    fn each_entry_makes_parts_for_the_columns_of_its_generic() {
        for standard in STANDARD {
            let (_, columns) = declared(standard.generic, "passive");
            let kinds: Vec<(String, FieldKind)> = columns
                .iter()
                .map(|n| (n.clone(), standard.column(n).unwrap().kind))
                .collect();
            let made = standard
                .candidates(Series::E24, &kinds)
                .unwrap_or_else(|| panic!("{} makes no part", standard.generic));
            assert!(!made.is_empty(), "{}", standard.generic);
            // A synthesized part is empty exactly where a row may be.
            for part in &made {
                for column in standard.columns {
                    assert_eq!(
                        part.column(column.name).is_none(),
                        standard.is_optional(column.name),
                        "{} {}",
                        standard.generic,
                        column.name
                    );
                }
            }
        }
    }

    #[test]
    fn a_catalog_that_lists_a_column_the_part_cannot_fill_has_no_synthesized_parts() {
        let mut kinds = RESISTOR.kinds();
        kinds.push(("tolerance".into(), FieldKind::Text));
        assert!(RESISTOR.candidates(Series::E24, &kinds).is_none());
        kinds.truncate(RESISTOR.columns.len());
        kinds.remove(0);
        assert!(
            RESISTOR.candidates(Series::E24, &kinds).is_none(),
            "no value to build the series from"
        );
    }

    #[test]
    fn the_columns_a_file_may_leave_out_and_the_value_columns_are_the_ones_the_adrs_name() {
        // ADR 0055 and ADR 0058: the columns added after catalog files were written.
        let optional: Vec<(&str, &str)> = STANDARD
            .iter()
            .flat_map(|s| {
                s.columns
                    .iter()
                    .filter(|c| s.is_optional(c.name))
                    .map(|c| (s.catalog, c.name))
            })
            .collect();
        assert_eq!(
            optional,
            [
                ("resistor", "temperature_coefficient"),
                ("capacitor", "dielectric"),
                ("inductor", "rms_current"),
            ]
        );
        // ADR 0065: the number a design is expected to choose or bound.
        assert_eq!(RESISTOR.value_columns(), ["resistance"]);
        assert_eq!(
            CAPACITOR.value_columns(),
            ["capacitance", "effective_capacitance"]
        );
        assert_eq!(INDUCTOR.value_columns(), ["inductance"]);
        assert!(LED.value_columns().is_empty());
        assert_eq!(
            [
                RESISTOR.marked(),
                CAPACITOR.marked(),
                INDUCTOR.marked(),
                LED.marked()
            ],
            [
                Some("resistance"),
                Some("capacitance"),
                Some("inductance"),
                None
            ]
        );
    }

    #[test]
    fn only_the_four_have_an_entry_and_the_names_are_found_both_ways() {
        for standard in STANDARD {
            assert!(std::ptr::eq(
                Standard::of_catalog(standard.catalog).unwrap(),
                standard
            ));
        }
        assert!(Standard::of_catalog("ldo").is_none());
        assert!(Standard::of_catalog("Resistor").is_none());
    }
}
