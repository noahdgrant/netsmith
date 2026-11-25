//! The packages netsmith knows the size of by name (issue #1658): how many pads a footprint of
//! the package has, and the size of its body, each from a maker's package drawing.
//!
//! A catalog row names its package (`SOT-23`), and the `kicad.toml` row of the part's kind and
//! that package names the footprint. Nothing else holds the two together: a row whose package is
//! `SOT-23` while the part is a SOT-323, or a mapping that gives `SOT-23` the SOT-323 footprint,
//! builds a board whose pads do not fit the part. `netsmith_pcb::check_package` measures a
//! footprint against the entry of the row's package here; [`disagreement_in`] compares the
//! package a supplier's description names with the row's.
//!
//! **Only a package with a recorded source is here.** Each entry's `source` names the drawing
//! (maker, document, revision or date, page) its numbers are read from; nothing is a guess or a
//! number from memory. The body is the molded body (not the leads), as its short side and its
//! long side, the least and the greatest of the drawings read; a drawing that prints one basic
//! length (`BSC`) gives that length for both. A package not here (crystals, inductors of one
//! maker's series, SOT-223, QFN and the other ICs, connectors) is not checked, and the check
//! says so.
//!
//! The names a supplier also writes for a package (`SC-70` for `SOT-323`, `DO-214AC` for `SMA`)
//! are its `aliases`, each from the same drawings or the drawings the footprint generators
//! record (`crates/netsmith-pcb/src/generate/`). `data/jlcpcb/README.md` lists the packages
//! and their sources too.

/// A size in mm: the least and the greatest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub min: f64,
    pub max: f64,
}

impl Span {
    const fn of(min: f64, max: f64) -> Span {
        Span { min, max }
    }

    /// Whether `measured` is this size, within [`SLACK`] of either end.
    pub fn holds(&self, measured: f64) -> bool {
        measured >= self.min - SLACK - 1e-9 && measured <= self.max + SLACK + 1e-9
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.min == self.max {
            write!(f, "{}", self.min)
        } else {
            write!(f, "{}-{}", self.min, self.max)
        }
    }
}

/// How far a footprint's body may be outside a package's range, in mm, each way. A footprint's
/// fabrication outline is often the greatest body rounded up: KiCad's SMA outline is 3.0 mm wide
/// where the drawings' widest is 2.92 mm. The packages here that share a pad count differ by
/// 0.4 mm or more (a SOT-23 is 2.8 mm long at least, a SOT-323 2.2 mm at most), so this slack
/// tells them apart.
pub const SLACK: f64 = 0.1;

/// A package of known size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KnownPackage {
    /// The name a catalog row writes, `SOT-23`.
    pub name: &'static str,
    /// Other names of the same package that a row or a supplier's description may write.
    pub aliases: &'static [&'static str],
    /// How many pads its footprint has, counted by number.
    pub pads: usize,
    /// The body's short side, then its long side.
    pub body: (Span, Span),
    /// The drawings the numbers are read from.
    pub source: &'static str,
}

/// A chip of two terminals.
const fn chip(
    name: &'static str,
    pads: usize,
    short: (f64, f64),
    long: (f64, f64),
    source: &'static str,
) -> KnownPackage {
    KnownPackage {
        name,
        aliases: &[],
        pads,
        body: (Span::of(short.0, short.1), Span::of(long.0, long.1)),
        source,
    }
}

/// Every package netsmith knows the size of.
pub const KNOWN: &[KnownPackage] = &[
    // Chips: the least and the greatest of Uniroyal's L and W and of Fenghua's widest tolerance
    // code of the size.
    chip("0201", 2, (0.21, 0.39), (0.51, 0.69), CHIPS),
    chip("0402", 2, (0.30, 0.70), (0.80, 1.20), CHIPS),
    chip("0603", 2, (0.60, 1.00), (1.40, 1.80), CHIPS),
    chip("0805", 2, (1.05, 1.45), (1.80, 2.20), CHIPS),
    chip("1206", 2, (1.30, 1.90), (2.90, 3.50), CHIPS),
    chip("1210", 2, (2.20, 2.80), (2.90, 3.50), CHIPS),
    KnownPackage {
        name: "SOD-123",
        aliases: &[],
        pads: 2,
        body: (Span::of(1.40, 1.80), Span::of(2.54, 2.85)),
        source: "Diodes Incorporated SOD123 Package Information (Rev. 2017-03-16, page 1: E 2.55 to 2.85, D 1.40 to 1.70) and onsemi SOD-123 case 425 issue H, 29 February 2024, 98ASB42927B (E 2.54 to 2.84, D 1.40 to 1.80), as recorded in crates/netsmith-pcb/src/generate/diode.rs",
    },
    KnownPackage {
        name: "SOD-323",
        aliases: &[],
        pads: 2,
        body: (Span::of(1.15, 1.40), Span::of(1.60, 1.80)),
        source: "Diodes Incorporated SOD323 Package Information (Rev. 2017-03-16, page 1: E 1.60 to 1.80, D 1.20 to 1.40) and onsemi SOD-323 case 477 issue K, 11 March 2024, 98ASB17533C (D 1.60 to 1.80, E 1.15 to 1.35), as recorded in crates/netsmith-pcb/src/generate/diode.rs",
    },
    KnownPackage {
        name: "SMA",
        aliases: &["DO-214AC"],
        pads: 2,
        body: (Span::of(2.29, 2.92), Span::of(4.00, 4.60)),
        source: "Diodes Incorporated SMA Package Information (Rev. 2017-03-14, page 1: B 4.00 to 4.60, A 2.29 to 2.92; DO-214AC), as recorded in crates/netsmith-pcb/src/generate/diode.rs",
    },
    KnownPackage {
        name: "SMB",
        aliases: &["DO-214AA"],
        pads: 2,
        body: (Span::of(3.30, 3.94), Span::of(4.06, 4.57)),
        source: "Diodes Incorporated SMB Package Information (Rev. 2019-08-30, page 1: B 4.06 to 4.57, A 3.30 to 3.94; DO-214AA), as recorded in crates/netsmith-pcb/src/generate/diode.rs",
    },
    KnownPackage {
        name: "SMC",
        aliases: &["DO-214AB"],
        pads: 2,
        body: (Span::of(5.59, 6.22), Span::of(6.60, 7.11)),
        source: "Diodes Incorporated SMC Package Information (Rev. 2017-03-14, page 1: E1 6.60 to 7.11, D 5.59 to 6.22; DO-214AB), as recorded in crates/netsmith-pcb/src/generate/diode.rs",
    },
    KnownPackage {
        name: "SOT-23",
        aliases: &["TO-236"],
        pads: 3,
        body: (Span::of(1.20, 1.40), Span::of(2.80, 3.00)),
        source: "JSCJ 2N7002 datasheet (Rev. J, September 2016, SHA-256 begins 7941fb423af7c6c6, the datasheet of the n_mosfet catalog's 2N7002 row), page 4, SOT-23 Package Outline Dimensions: D 2.800 to 3.000, E 1.200 to 1.400",
    },
    KnownPackage {
        name: "SOT-23-5",
        aliases: &[],
        pads: 5,
        body: (Span::of(1.45, 1.75), Span::of(2.75, 3.05)),
        source: "TI SN74LVC1G04 datasheet SCES214AF (revised October 2025), page 34, package outline DBV0005A (4214839/K 08/2024, reference JEDEC MO-178): body 1.45 to 1.75 by 2.75 to 3.05",
    },
    KnownPackage {
        name: "SOT-23-6",
        aliases: &[],
        pads: 6,
        body: (Span::of(1.45, 1.75), Span::of(2.75, 3.05)),
        source: "TI LMR16006 datasheet SNVSA24 (October 2014), page 23, package outline DDC0006A (4214841/A 08/2016, reference JEDEC MO-193, the package of the lmr16006y and tps54202 catalogs' rows): body 1.45 to 1.75 by 2.75 to 3.05",
    },
    KnownPackage {
        name: "TSOT-23-5",
        aliases: &[],
        pads: 5,
        body: (Span::of(1.397, 1.803), Span::of(2.692, 3.099)),
        source: "Richtek RT8059 datasheet DS8059-06 (May 2016), page 11, TSOT-23-5 Surface Mount Package: B 1.397 to 1.803, D 2.692 to 3.099",
    },
    KnownPackage {
        name: "SOT-323",
        aliases: &["SC-70", "SC-70-3"],
        pads: 3,
        body: (Span::of(1.15, 1.35), Span::of(1.80, 2.20)),
        source: "onsemi SC-70 case 419AB (issue A, 13 February 2023, 98AON34256E: D 1.80 to 2.20, E1 1.15 to 1.35) and Nexperia SOT323 package information (29 July 2019), as recorded in crates/netsmith-pcb/src/generate/sot.rs",
    },
    KnownPackage {
        name: "SOT-353",
        aliases: &["SC-70-5", "SC-88A", "SOT-353_SC-70-5"],
        pads: 5,
        body: (Span::of(1.15, 1.35), Span::of(1.80, 2.25)),
        source: "onsemi SC-88A case 419AC (issue A, 29 June 2010, 98AON34260E: D 1.80 to 2.20, E1 1.15 to 1.35) and Nexperia SOT353-1 package information (8 February 2016: 1.85 to 2.25, 1.15 to 1.35), as recorded in crates/netsmith-pcb/src/generate/sot.rs",
    },
    KnownPackage {
        name: "SOT-363",
        aliases: &["SC-70-6", "SC-88"],
        pads: 6,
        body: (Span::of(1.15, 1.35), Span::of(1.80, 2.20)),
        source: "onsemi SC-88 case 419AD (issue A, 7 July 2010, 98AON34266E: D 1.80 to 2.20, E1 1.15 to 1.35) and Nexperia SOT363 package information (9 May 2018), as recorded in crates/netsmith-pcb/src/generate/sot.rs",
    },
    KnownPackage {
        name: "SOIC-8",
        aliases: &[],
        pads: 8,
        body: (Span::of(3.90, 3.90), Span::of(4.90, 4.90)),
        source: "Microchip ATtiny13A datasheet DS40002307A, page 175, drawing C04-057-SN Rev F (8-Lead Plastic Small Outline, narrow 3.90 mm body): E1 3.90 BSC, D 4.90 BSC",
    },
];

const CHIPS: &str = "Uniroyal Thick Film Chip Resistors data sheet (01005 to 2512 series, Version 3, Feb. 12, 2019, 9 pages, SHA-256 begins 11cd644d5d8a34a6, the datasheet of the resistor catalog's rows), page 4, 5. Dimension, L and W; and Fenghua general type MLCC product specification (1005 to 1812 type, 30 pages, SHA-256 begins 705023d3be045414, a datasheet of the capacitor catalog's rows), PDF page 5, the dimension table, L and W of the size's widest tolerance code";

/// A package name as it is compared: upper case, with no `-`, `_`, `.` or space, and the
/// `L` of a lead count written after it (`SOT-23-5L` is `SOT235`) taken off.
fn normal(name: &str) -> String {
    let mut out: String = name
        .chars()
        .filter(|c| !matches!(c, '-' | '_' | '.' | ' '))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let bytes = out.as_bytes();
    if bytes.len() > 2
        && bytes[bytes.len() - 1] == b'L'
        && bytes[bytes.len() - 2].is_ascii_digit()
        && bytes[0].is_ascii_alphabetic()
    {
        out.pop();
    }
    out
}

/// The known package a row's package names, by its name or an alias.
pub fn known(package: &str) -> Option<&'static KnownPackage> {
    let wanted = normal(package);
    if wanted.is_empty() {
        return None;
    }
    KNOWN
        .iter()
        .find(|k| normal(k.name) == wanted || k.aliases.iter().any(|a| normal(a) == wanted))
}

/// The known packages that a supplier's text names, each once, in the order the text names
/// them. A word is a package's name or alias, written with or without its dashes and with a
/// lead count's `L` (`SOT323`, `SOT-23-3L`); the words are what lies between spaces, commas,
/// semicolons, slashes and brackets.
pub fn named_in(text: &str) -> Vec<&'static KnownPackage> {
    let mut found: Vec<&'static KnownPackage> = Vec::new();
    for word in text.split(|c: char| c.is_whitespace() || ",;/()[]".contains(c)) {
        if let Some(k) = known(word)
            && !found.iter().any(|f| f.name == k.name)
        {
            found.push(k);
        }
    }
    found
}

/// What a supplier's description says of the package that the row's does not.
#[derive(Debug, Clone, PartialEq)]
pub struct Disagreement {
    /// The row's package, as written.
    pub package: String,
    /// The packages the description names, none of them the row's.
    pub named: Vec<&'static str>,
    /// The description.
    pub description: String,
}

impl std::fmt::Display for Disagreement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the package is `{}`, but the supplier's description \"{}\" names {}",
            self.package,
            self.description,
            self.named
                .iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(" and ")
        )
    }
}

/// Whether a supplier's description names a known package and none that is the row's
/// `package`. Only a known `package` is compared: a row whose package is a maker's own name
/// (`FNR4020S`) may be described by another, and nothing here says they differ.
pub fn disagreement_in(package: &str, description: &str) -> Option<Disagreement> {
    let own = known(package)?;
    let named = named_in(description);
    if named.is_empty() || named.iter().any(|n| n.name == own.name) {
        return None;
    }
    Some(Disagreement {
        package: package.to_string(),
        named: named.iter().map(|n| n.name).collect(),
        description: description.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_known_with_or_without_its_dashes_and_by_an_alias() {
        assert_eq!(known("SOT-23").unwrap().name, "SOT-23");
        assert_eq!(known("sot23").unwrap().name, "SOT-23");
        assert_eq!(known("SOT323").unwrap().name, "SOT-323");
        assert_eq!(known("SC-70").unwrap().name, "SOT-323");
        assert_eq!(known("SOT-23-5L").unwrap().name, "SOT-23-5");
        assert_eq!(known("DO-214AC").unwrap().name, "SMA");
        assert_eq!(known("0402").unwrap().pads, 2);
        assert_eq!(known("FNR4020S"), None);
        assert_eq!(known(""), None);
    }

    #[test]
    fn every_package_has_a_source_and_sizes_in_order() {
        for k in KNOWN {
            assert!(!k.source.is_empty(), "{}", k.name);
            let (short, long) = k.body;
            assert!(short.min <= short.max && long.min <= long.max, "{}", k.name);
            assert!(short.max <= long.max, "{}: short side first", k.name);
            assert!(k.pads > 0, "{}", k.name);
        }
        // No two packages share a name or an alias.
        let mut names: Vec<String> = KNOWN
            .iter()
            .flat_map(|k| std::iter::once(k.name).chain(k.aliases.iter().copied()))
            .map(normal)
            .collect();
        let all = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), all);
    }

    #[test]
    fn a_description_that_names_another_package_disagrees() {
        let d = disagreement_in("SOT-23", "MOSFET N-CH 60V 115mA SOT323").unwrap();
        assert_eq!(d.named, vec!["SOT-323"]);
        assert!(d.to_string().contains("`SOT-23`"), "{d}");
        assert!(d.to_string().contains("`SOT-323`"), "{d}");
    }

    #[test]
    fn a_description_that_names_the_package_or_none_agrees() {
        assert_eq!(
            disagreement_in("SOT-23", "N-Channel 60V 115mA 0.225W Surface Mount SOT-23"),
            None
        );
        assert_eq!(disagreement_in("SMA", "Diode 40V 1A SMA(DO-214AC)"), None);
        assert_eq!(disagreement_in("SOT-23", "N-Channel 60V 115mA"), None);
        // An unknown package is not compared.
        assert_eq!(disagreement_in("FNR4020S", "4.7uH SMD 0402"), None);
    }
}
