//! Does the part agree with its symbol and its footprint? (ADR 0016, decision 8.)
//!
//! A part's pinout says which pad is which pin; the symbol says which pin number is which
//! name and electrical type; the footprint says which pads exist. They are written by
//! three different people, and two pads swapped between them would build a board that
//! does not work. Every disagreement is reported, naming both sides.
//!
//! What is checked (ADR 0021, accepted by the owner on 2026-11-04): every pad of the pinout is a
//! numbered pad of the footprint and a pin of the symbol, and the other way round; the symbol pin
//! on a pad has the datasheet name after the mapping's `pin_names` (case does not matter, a symbol
//! pin named `~` is unnamed and accepted); the symbol pin's electrical type suits the kind. Every
//! message names the part, the package and both sides. `build` and `fab` run it for every part with
//! a pin table before the schematic is written, and stop with all the messages
//! (`crates/netsmith-pipeline/tests/pinout.rs`, with invented libraries). `crates/netsmith-pipeline/tests/kicad_rows.rs`
//! takes each row of `std/kicad.toml` and the pinout of its kind in that package from the standard
//! library's own source, and checks it against the installed libraries (skipped without KiCad) and,
//! for the kinds they hold, against the sample libraries.
//!
//! **A `passive` part pin suits any symbol type but a supply or a no-connect.** ADR 0016 allowed
//! `passive` or `unspecified` only. KiCad types a USB-C configuration pin `bidirectional` and a
//! mounting-hole pad `input`; a pin the part calls passive (neither takes nor gives) does not
//! contradict a symbol that says more. A `power_in`, `power_out` or `no_connect` symbol pin still
//! does. The other kinds follow ADR 0016.
//!
//! **A `power` pin that gives power suits an `output` symbol pin.** A provider's row has the kind
//! of its generic (`LinearRegulator` declares `.vout.pos power`), and many library symbols of a
//! regulator type the output `output` rather than `power_out`. When the solved design says the pin
//! gives power ([`PartPin::gives_power`]: the current of its rail is negative), the symbol's
//! `output` says the same thing, so the symbol is used as it is, with no copy to edit. A net with
//! no `power_out` on it gets a power flag in the schematic, so KiCad's electrical rules check sees
//! the rail driven. A power pin whose direction is not known, or one that takes power, is not an
//! output, and the message says what would make it one.

use netsmith_kicad::{Mapped, PartPin, PinKind, PinType, Pinout, Symbol};

use crate::footprint::Footprint;

/// The part being checked, for the start of each message.
#[derive(Debug, Clone, Copy)]
pub struct Subject<'a> {
    pub refdes: &'a str,
    pub kind: &'a str,
    pub package: Option<&'a str>,
}

impl std::fmt::Display for Subject<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({}", self.refdes, self.kind)?;
        if let Some(package) = self.package {
            write!(f, ", package \"{package}\"")?;
        }
        write!(f, ")")
    }
}

/// Whether a symbol pin of electrical type `symbol` can stand for the part pin `pin`.
/// A symbol pin typed `passive` is always accepted, because that is how stock symbols
/// hide stacked duplicates.
fn suits(pin: &PartPin, symbol: PinType) -> bool {
    use PinType::*;
    if symbol == Passive {
        return true;
    }
    match pin.kind {
        // A library regulator's output is often typed `output`: it suits a power pin that the
        // solved design says gives power.
        PinKind::Power => {
            matches!(symbol, PowerIn | PowerOut) || (symbol == Output && pin.gives_power())
        }
        PinKind::Input => symbol == Input,
        PinKind::Output | PinKind::TriState => matches!(symbol, Output | TriState | OpenCollector),
        // KiCad's stock symbols often type an open-drain output `output`.
        PinKind::OpenDrain => matches!(symbol, OpenCollector | Output),
        PinKind::Bidirectional => !matches!(symbol, PowerIn | PowerOut),
        PinKind::NoConnect => symbol == NoConnect,
        // A passive pin neither takes nor gives, so a symbol that says more about it
        // (KiCad types a USB-C configuration pin `bidirectional`, a mounting-hole pad
        // `input`) does not contradict it. Only a supply or no-connect type does.
        PinKind::Passive => !matches!(symbol, PowerIn | PowerOut | NoConnect),
    }
}

fn list(items: &[&str]) -> String {
    items.join(", ")
}

/// What the pinout says about a pad, for a message: `the no-connect pin `nc` (`NC`)`.
fn describe(pin: &PartPin) -> String {
    format!(
        "the {} pin `{}` (`{}`)",
        pin.kind.word(),
        pin.name,
        pin.datasheet
    )
}

/// Every disagreement between `pinout`, the symbol it is drawn with and the footprint it is
/// built on. Empty when they agree. `mapped` supplies the symbol's names for pins whose
/// datasheet name differs.
pub fn validate_pinout(
    subject: &Subject,
    pinout: &Pinout,
    mapped: &Mapped,
    symbol: &Symbol,
    footprint: &Footprint,
) -> Vec<String> {
    let mut problems = Vec::new();
    let mut say = |text: String| problems.push(format!("{subject}: {text}"));

    // Unnumbered pads (paste apertures) are not pins; several pads of one number are one.
    let pads: Vec<&str> = footprint
        .pad_numbers()
        .into_iter()
        .filter(|n| !n.is_empty())
        .collect();
    let mut symbol_pins: Vec<&str> = Vec::new();
    for pin in symbol.pins() {
        if !symbol_pins.contains(&pin.number.as_str()) {
            symbol_pins.push(&pin.number);
        }
    }
    let footprint_name = &mapped.footprint;
    // A generated footprint (`gen:`, ADR 0104) is written in a row of a `kicad.toml`: say which,
    // and how many pads it makes, so that the fix (the part's pins or the parameters) is plain.
    let from = netsmith_kicad::FootprintSpec::from_id(&mapped.footprint)
        .map(|spec| {
            format!(
                "; the footprint is `{}` of {}, and it makes {} pads",
                spec.spelling(),
                mapped.origin,
                pads.len()
            )
        })
        .unwrap_or_default();
    let lib_id = symbol.lib_id();

    let mut seen: Vec<&str> = Vec::new();
    for pad in pinout.pads() {
        if seen.contains(&pad) {
            continue;
        }
        seen.push(pad);
        let pin = pinout.pin_on(pad).expect("the pad is in the pinout");
        if !pads.contains(&pad) {
            say(format!(
                "the pinout has pad {pad} ({}) but the footprint `{footprint_name}` has pads {} only{from}",
                describe(pin),
                list(&pads)
            ));
        }
        let Some(sym) = symbol.pin(pad) else {
            // KiCad's symbols leave a no-connect pad out; the footprint has it and the part
            // says it must stay open, so the two do not disagree.
            if pin.kind == netsmith_core::PinKind::NoConnect && pads.contains(&pad) {
                continue;
            }
            say(format!(
                "the pinout has pad {pad} ({}) but the symbol `{lib_id}` has no pin {pad}; its pins are {}",
                describe(pin),
                list(&symbol_pins)
            ));
            continue;
        };
        let expected = mapped
            .pin_names
            .iter()
            .find(|(datasheet, _)| datasheet.eq_ignore_ascii_case(&pin.datasheet))
            .map_or(pin.datasheet.as_str(), |(_, symbol_name)| {
                symbol_name.as_str()
            });
        if !sym.is_unnamed() && !sym.name.eq_ignore_ascii_case(expected) {
            let renamed = if expected == pin.datasheet {
                String::new()
            } else {
                format!(" (`{}` is `{expected}` by pin_names)", pin.datasheet)
            };
            say(format!(
                "pad {pad} is {} in the part, but pin {pad} of the symbol `{lib_id}` is named `{}`{renamed}",
                describe(pin),
                sym.name
            ));
        }
        if !suits(pin, sym.kind) {
            // An `output` on a power pin is refused only because the design does not say the pin
            // gives power: say so, and what would.
            let why = if pin.kind == PinKind::Power && sym.kind == PinType::Output {
                "; a power pin suits `output` when the solved design says it gives power: the current of its rail is below 0 A"
            } else {
                ""
            };
            say(format!(
                "pad {pad} is {} in the part, but pin {pad} of the symbol `{lib_id}` has the electrical type `{}`, which does not suit a {} pin{why}",
                describe(pin),
                sym.kind.name(),
                pin.kind.word()
            ));
        }
    }
    for pad in &pads {
        if pinout.pin_on(pad).is_none() {
            say(format!(
                "the footprint `{footprint_name}` has pad {pad}, which no pin of the part has; give it to a pin in the pinout{from}"
            ));
        }
    }
    for sym in symbol.pins() {
        if pinout.pin_on(&sym.number).is_none()
            && symbol
                .pins()
                .iter()
                .find(|p| p.number == sym.number)
                .is_some_and(|first| std::ptr::eq(first, sym))
        {
            say(format!(
                "the symbol `{lib_id}` has pin {} (`{}`), which is on no pad of the part; give the pad to a pin in the pinout",
                sym.number, sym.name
            ));
        }
    }
    problems
}

/// The least distance between the centres of two pads of different numbers in one row or one
/// column of `footprint` (the same y, or the same x, to a micrometre), in mm: the footprint's
/// pitch as a table states it ([`netsmith_kicad::FootprintTable`]). `None` when no two pads line up.
pub fn footprint_pitch(footprint: &Footprint) -> Option<f64> {
    const ALIGNED: f64 = 0.001;
    let pads: Vec<_> = footprint
        .pads
        .iter()
        .filter(|p| !p.number.is_empty())
        .collect();
    let mut least: Option<f64> = None;
    for (i, a) in pads.iter().enumerate() {
        for b in &pads[i + 1..] {
            if a.number == b.number {
                continue;
            }
            let (dx, dy) = ((a.at.x - b.at.x).abs(), (a.at.y - b.at.y).abs());
            let d = if dy < ALIGNED {
                dx
            } else if dx < ALIGNED {
                dy
            } else {
                continue;
            };
            if d > ALIGNED && least.is_none_or(|l| d < l) {
                least = Some(d);
            }
        }
    }
    least
}

/// Round to a micrometre, for a message.
fn um(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// Every disagreement between `footprint` and the `table` of the `kicad.toml` row that gave it
/// (issue #1281): its count of pads by number, its pitch ([`footprint_pitch`]) and the size of the
/// body it draws on `F.Fab`, across and along. Empty when it agrees, or when the row states no
/// table. Each message names the part, the footprint, the row and both numbers.
pub fn validate_table(subject: &Subject, mapped: &Mapped, footprint: &Footprint) -> Vec<String> {
    let Some(table) = &mapped.table else {
        return Vec::new();
    };
    let mut problems = Vec::new();
    let name = &mapped.footprint;
    let row = format!("the table `{}` of {}", table.text, mapped.origin);
    if let Some(pads) = table.pads {
        let counted = footprint
            .pad_numbers()
            .into_iter()
            .filter(|n| !n.is_empty())
            .count();
        if counted != pads {
            problems.push(format!(
                "{subject}: the footprint `{name}` has {counted} pads, but {row} says {pads}; pads are counted by number, so an exposed pad drawn in pieces is one"
            ));
        }
    }
    if let Some(pitch) = table.pitch {
        match footprint_pitch(footprint) {
            None => problems.push(format!(
                "{subject}: the footprint `{name}` has no two pads in one row or column, so it has no pitch, but {row} says {pitch}mm"
            )),
            Some(measured) if (measured - pitch).abs() > netsmith_kicad::PITCH_TOLERANCE => {
                problems.push(format!(
                    "{subject}: the footprint `{name}` has its pads {}mm apart, but {row} says a pitch of {pitch}mm; the pitch is the least distance between the centres of two pads in one row or column",
                    um(measured)
                ))
            }
            Some(_) => {}
        }
    }
    if let Some((across, along)) = table.body {
        if footprint.body_is_courtyard {
            problems.push(format!(
                "{subject}: the footprint `{name}` draws no body on its fabrication layer (`F.Fab`), so its outline cannot be measured against {row}; draw the body on `F.Fab`, or leave `body` out of the table"
            ));
        } else {
            let (w, h) = (footprint.body.width(), footprint.body.height());
            if !across.holds(w) || !along.holds(h) {
                problems.push(format!(
                    "{subject}: the footprint `{name}` draws a body {}x{}mm on `F.Fab` (across, then along), but {row} says {across}x{along}mm",
                    um(w),
                    um(h)
                ));
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use netsmith_kicad::SymbolLib;
    use netsmith_testkit::TempDir;

    use super::*;
    use crate::footprint::FootprintLibrary;

    fn pin(path: &str, name: &str, kind: PinKind, pads: &[&str]) -> PartPin {
        PartPin {
            name: path.into(),
            datasheet: name.into(),
            kind,
            pads: pads.iter().map(|p| p.to_string()).collect(),
            function: None,
            power_flow: None,
        }
    }

    /// A two-pad chip footprint, an LDO symbol with a stacked output, and the pinout that
    /// agrees with both.
    struct Fixture {
        _dir: TempDir,
        symbols: SymbolLib,
        footprints: FootprintLibrary,
    }

    fn fixture() -> Fixture {
        let dir = TempDir::new("validate");
        let sym = dir.path().join("sym");
        let fp = dir.path().join("fp/proj.pretty");
        std::fs::create_dir_all(&sym).unwrap();
        std::fs::create_dir_all(&fp).unwrap();
        std::fs::write(
            sym.join("proj.kicad_sym"),
            "(kicad_symbol_lib (symbol \"Ldo\" (symbol \"Ldo_1_1\" \
             (pin power_in line (at -5 0 0) (length 1) (name \"IN\") (number \"1\")) \
             (pin power_out line (at 5 0 180) (length 1) (name \"OUT\") (number \"2\")) \
             (pin passive line (at 5 0 180) (hide yes) (length 1) (name \"OUT\") (number \"3\")) \
             (pin input line (at 0 5 270) (length 1) (name \"SNS\") (number \"4\")) \
             (pin no_connect line (at 0 -5 90) (length 1) (name \"NC\") (number \"5\")) \
             (pin power_in line (at 0 -5 90) (length 1) (name \"GND\") (number \"6\")))) \
             (symbol \"LdoBare\" (symbol \"LdoBare_1_1\" \
             (pin power_in line (at -5 0 0) (length 1) (name \"IN\") (number \"1\")) \
             (pin power_out line (at 5 0 180) (length 1) (name \"OUT\") (number \"2\")) \
             (pin passive line (at 5 0 180) (hide yes) (length 1) (name \"OUT\") (number \"3\")) \
             (pin input line (at 0 5 270) (length 1) (name \"SNS\") (number \"4\")) \
             (pin power_in line (at 0 -5 90) (length 1) (name \"GND\") (number \"6\"))))\
             )",
        )
        .unwrap();
        let pad =
            |n: &str| format!("(pad \"{n}\" smd rect (at 0 0) (size 1 1) (layers \"F.Cu\")) ");
        let mut pads: String = ["1", "2", "3", "4", "5", "6"]
            .iter()
            .map(|n| pad(n))
            .collect();
        // The exposed pad is pad 6 as well; the paste apertures have no number.
        pads += &pad("6");
        pads += &pad("");
        std::fs::write(
            fp.join("Dfn.kicad_mod"),
            format!("(footprint \"Dfn\" (attr smd) {pads})"),
        )
        .unwrap();
        Fixture {
            symbols: SymbolLib::at("/nonexistent").with_library("proj", sym.join("proj.kicad_sym")),
            footprints: FootprintLibrary::at(dir.path().join("fp")),
            _dir: dir,
        }
    }

    fn pinout() -> Pinout {
        Pinout {
            pins: vec![
                pin("vin.pos", "IN", PinKind::Power, &["1"]),
                pin("vout.pos", "OUT", PinKind::Power, &["2", "3"]),
                pin("sense", "SENSE", PinKind::Input, &["4"]),
                pin("nc", "NC", PinKind::NoConnect, &["5"]),
                pin("vin.neg", "GND", PinKind::Power, &["6"]),
            ],
        }
    }

    fn mapped() -> Mapped {
        Mapped {
            lib_id: Some("proj:Ldo".into()),
            footprint: "proj:Dfn".into(),
            origin: "kicad.toml line 3".into(),
            pin_names: BTreeMap::from([("SENSE".to_string(), "SNS".to_string())]),
            table: None,
        }
    }

    fn check(pinout: &Pinout, mapped: &Mapped) -> Vec<String> {
        let f = fixture();
        let symbol = f.symbols.symbol("proj:Ldo").unwrap();
        let footprint = f.footprints.load("proj:Dfn").unwrap();
        let subject = Subject {
            refdes: "U1",
            kind: "Ldo9",
            package: Some("DFN-8-EP"),
        };
        validate_pinout(&subject, pinout, mapped, &symbol, &footprint)
    }

    #[test]
    fn a_part_that_agrees_with_both_has_no_problems() {
        assert_eq!(check(&pinout(), &mapped()), Vec::<String>::new());
    }

    #[test]
    fn a_no_connect_pad_the_symbol_leaves_out_is_not_a_disagreement() {
        // KiCad's own symbols leave a no-connect pad out (the 74xGxx gates do).
        let f = fixture();
        let symbol = f.symbols.symbol("proj:LdoBare").unwrap();
        let footprint = f.footprints.load("proj:Dfn").unwrap();
        let subject = Subject {
            refdes: "U1",
            kind: "Ldo9",
            package: Some("DFN-8-EP"),
        };
        let m = Mapped {
            lib_id: Some("proj:LdoBare".into()),
            ..mapped()
        };
        assert_eq!(
            validate_pinout(&subject, &pinout(), &m, &symbol, &footprint),
            Vec::<String>::new()
        );
        // Any other kind of pin on that pad is still a disagreement.
        let mut p = pinout();
        p.pins[3].kind = PinKind::Passive;
        let problems = validate_pinout(&subject, &p, &m, &symbol, &footprint);
        assert!(
            problems
                .iter()
                .any(|m| m.contains("the symbol `proj:LdoBare` has no pin 5")),
            "{problems:#?}"
        );
    }

    #[test]
    fn a_pad_the_footprint_lacks_is_named_with_the_pads_it_has() {
        let mut p = pinout();
        p.pins[4].pads = vec!["9".into()];
        let problems = check(&p, &mapped());
        assert!(
            problems.contains(
                &"U1 (Ldo9, package \"DFN-8-EP\"): the pinout has pad 9 (the power pin `vin.neg` (`GND`)) \
                  but the footprint `proj:Dfn` has pads 1, 2, 3, 4, 5, 6 only"
                    .to_string()
            ),
            "{problems:#?}"
        );
    }

    #[test]
    fn a_footprint_pad_no_pin_has_is_named() {
        let mut p = pinout();
        p.pins[4].pads.clear();
        let problems = check(&p, &mapped());
        assert!(
            problems.iter().any(|m| m.contains(
                "the footprint `proj:Dfn` has pad 6, which no pin of the part has; give it to a pin in the pinout"
            )),
            "{problems:#?}"
        );
        // The symbol has the pin as well.
        assert!(
            problems
                .iter()
                .any(|m| m.contains("the symbol `proj:Ldo` has pin 6 (`GND`), which is on no pad")),
            "{problems:#?}"
        );
    }

    #[test]
    fn a_pad_the_symbol_lacks_names_the_pins_it_has() {
        let mut p = pinout();
        p.pins[2].pads = vec!["7".into()];
        let problems = check(&p, &mapped());
        assert!(
            problems.iter().any(|m| m.contains(
                "the pinout has pad 7 (the input pin `sense` (`SENSE`)) but the symbol `proj:Ldo` has no pin 7; its pins are 1, 2, 3, 4, 5, 6"
            )),
            "{problems:#?}"
        );
    }

    #[test]
    fn two_pads_swapped_between_the_part_and_the_symbol_name_both_sides() {
        let mut p = pinout();
        // The part says the no-connect is pad 4 and the sense is pad 5; the symbol says
        // the other way round.
        p.pins[2].pads = vec!["5".into()];
        p.pins[3].pads = vec!["4".into()];
        let problems = check(&p, &mapped());
        assert!(
            problems.contains(
                &"U1 (Ldo9, package \"DFN-8-EP\"): pad 4 is the no_connect pin `nc` (`NC`) in the part, \
                  but pin 4 of the symbol `proj:Ldo` is named `SNS`"
                    .to_string()
            ),
            "{problems:#?}"
        );
    }

    #[test]
    fn the_symbols_name_for_a_pin_comes_from_pin_names() {
        // With no pin_names the symbol's `SNS` is not the datasheet's `SENSE`.
        let mut m = mapped();
        m.pin_names.clear();
        let problems = check(&pinout(), &m);
        assert_eq!(problems.len(), 1, "{problems:#?}");
        assert!(problems[0].contains("pin 4 of the symbol `proj:Ldo` is named `SNS`"));
        // And when the names differ even after pin_names, it says what it expected.
        m.pin_names.insert("SENSE".into(), "SNX".into());
        let problems = check(&pinout(), &m);
        assert!(
            problems[0].contains("is named `SNS` (`SENSE` is `SNX` by pin_names)"),
            "{problems:#?}"
        );
    }

    #[test]
    fn names_compare_without_case() {
        let mut p = pinout();
        p.pins[0].datasheet = "in".into();
        assert!(check(&p, &mapped()).is_empty());
    }

    #[test]
    fn the_electrical_type_must_suit_the_kind() {
        let mut p = pinout();
        p.pins[2].kind = PinKind::Output; // pad 4 is an `input` in the symbol
        let problems = check(&p, &mapped());
        assert_eq!(problems.len(), 1, "{problems:#?}");
        assert!(problems[0].contains(
            "pad 4 is the output pin `sense` (`SENSE`) in the part, but pin 4 of the symbol `proj:Ldo` has the electrical type `input`, which does not suit a output pin"
        ), "{problems:#?}");
    }

    #[test]
    fn the_kinds_and_the_types_that_suit_them() {
        use PinKind::*;
        use PinType as T;
        for (kind, ok, bad) in [
            (
                Power,
                vec![T::PowerIn, T::PowerOut, T::Passive],
                vec![T::Input, T::Output],
            ),
            (
                Input,
                vec![T::Input, T::Passive],
                vec![T::Output, T::PowerIn],
            ),
            (
                Output,
                vec![T::Output, T::TriState, T::OpenCollector, T::Passive],
                vec![T::Input, T::PowerOut],
            ),
            (
                Bidirectional,
                vec![T::Bidirectional, T::Input, T::TriState, T::Unspecified],
                vec![T::PowerIn, T::PowerOut],
            ),
            (
                TriState,
                vec![T::TriState, T::Output, T::OpenCollector, T::Passive],
                vec![T::Input, T::PowerOut],
            ),
            (
                OpenDrain,
                vec![T::OpenCollector, T::Output, T::Passive],
                vec![T::Input, T::TriState, T::PowerOut],
            ),
            (NoConnect, vec![T::NoConnect, T::Passive], vec![T::Input]),
            (
                Passive,
                vec![T::Passive, T::Unspecified, T::Input, T::Bidirectional],
                vec![T::PowerIn, T::PowerOut, T::NoConnect],
            ),
        ] {
            let p = pin("p", "P", kind, &["1"]);
            for t in ok {
                assert!(suits(&p, t), "{kind:?} with {t:?}");
            }
            for t in bad {
                assert!(!suits(&p, t), "{kind:?} with {t:?}");
            }
        }
    }

    #[test]
    fn a_power_pin_that_gives_power_suits_an_output() {
        use netsmith_core::PowerFlow;
        let mut p = pin("vout.pos", "OUT", PinKind::Power, &["2"]);
        // Its direction not known, or taking power, it is no output.
        for flow in [None, Some(PowerFlow::Takes)] {
            p.power_flow = flow;
            assert!(!suits(&p, PinType::Output), "{flow:?}");
        }
        p.power_flow = Some(PowerFlow::Gives);
        assert!(suits(&p, PinType::Output));
        // Still no input, open collector or tri-state.
        for t in [PinType::Input, PinType::OpenCollector, PinType::TriState] {
            assert!(!suits(&p, t), "{t:?}");
        }
    }

    #[test]
    fn an_output_on_a_power_pin_whose_direction_is_not_known_says_what_would_suit() {
        // The symbol types pad 1 `power_in`; make it an `output`.
        let f = fixture();
        let text = std::fs::read_to_string(f._dir.path().join("sym/proj.kicad_sym"))
            .unwrap()
            .replacen(
                "(pin power_in line (at -5 0 0)",
                "(pin output line (at -5 0 0)",
                1,
            );
        let lib = f._dir.path().join("sym/out.kicad_sym");
        std::fs::write(&lib, text).unwrap();
        let symbols = SymbolLib::at("/nonexistent").with_library("proj", lib);
        let symbol = symbols.symbol("proj:Ldo").unwrap();
        let footprint = f.footprints.load("proj:Dfn").unwrap();
        let subject = Subject {
            refdes: "U1",
            kind: "Ldo9",
            package: Some("DFN-8-EP"),
        };
        let problems = validate_pinout(&subject, &pinout(), &mapped(), &symbol, &footprint);
        assert_eq!(
            problems,
            [
                "U1 (Ldo9, package \"DFN-8-EP\"): pad 1 is the power pin `vin.pos` (`IN`) in the part, \
              but pin 1 of the symbol `proj:Ldo` has the electrical type `output`, which does not suit \
              a power pin; a power pin suits `output` when the solved design says it gives power: \
              the current of its rail is below 0 A"
            ]
        );
        // The same pin, when the design says it gives power, is accepted.
        let mut given = pinout();
        given.pins[0].power_flow = Some(netsmith_core::PowerFlow::Gives);
        assert!(validate_pinout(&subject, &given, &mapped(), &symbol, &footprint).is_empty());
    }

    #[test]
    fn the_subject_reads_like_the_board() {
        let s = Subject {
            refdes: "R1",
            kind: "Resistor",
            package: None,
        };
        assert_eq!(s.to_string(), "R1 (Resistor)");
    }
}

#[cfg(test)]
mod table_tests {
    use super::*;
    use netsmith_kicad::FootprintTable;

    /// An SOIC-8 as a vendor draws it: two columns of four pads at 1.27 mm, 5.4 mm apart, and a
    /// body of 3.9 x 4.9 mm on `F.Fab`.
    fn soic8(pitch: f64, fab: bool) -> Footprint {
        let mut text = String::from("(footprint \"Soic8\" (attr smd) ");
        for i in 0..4 {
            let y = (i as f64 - 1.5) * pitch;
            text += &format!(
                "(pad \"{}\" smd rect (at -2.7 {y}) (size 1.5 0.6) (layers \"F.Cu\")) ",
                i + 1
            );
            text += &format!(
                "(pad \"{}\" smd rect (at 2.7 {}) (size 1.5 0.6) (layers \"F.Cu\")) ",
                8 - i,
                y
            );
        }
        if fab {
            text += "(fp_rect (start -1.95 -2.45) (end 1.95 2.45) (stroke (width 0.1) (type solid)) (fill none) (layer \"F.Fab\")) ";
        }
        text += ")";
        Footprint::parse("Soic8", &text).unwrap()
    }

    fn check(table: &str, footprint: &Footprint) -> Vec<String> {
        let mapped = Mapped {
            lib_id: None,
            footprint: "own:Soic8".into(),
            origin: "kicad.toml line 5".into(),
            pin_names: Default::default(),
            table: Some(FootprintTable::parse(table).unwrap()),
        };
        let subject = Subject {
            refdes: "U1",
            kind: "Driver",
            package: Some("SOIC-8"),
        };
        validate_table(&subject, &mapped, footprint)
    }

    #[test]
    fn a_footprint_that_agrees_with_its_table_has_no_problems() {
        let fp = soic8(1.27, true);
        assert_eq!(footprint_pitch(&fp), Some(1.27));
        assert_eq!(
            check("pads=8, pitch=1.27mm, body=3.9x4.9mm", &fp),
            Vec::<String>::new()
        );
        // A range is matched as it stands, one length within 0.05 mm.
        assert_eq!(check("body=3.8-4.0x4.87mm", &fp), Vec::<String>::new());
    }

    #[test]
    fn each_disagreement_names_the_part_the_row_and_both_numbers() {
        let fp = soic8(1.27, true);
        let problems = check("pads=9, pitch=1.25mm, body=3.9x5mm", &fp);
        assert_eq!(problems.len(), 3, "{problems:#?}");
        assert_eq!(
            problems[0],
            "U1 (Driver, package \"SOIC-8\"): the footprint `own:Soic8` has 8 pads, but the table \
             `pads=9, pitch=1.25mm, body=3.9x5mm` of kicad.toml line 5 says 9; pads are counted by \
             number, so an exposed pad drawn in pieces is one"
        );
        assert!(
            problems[1].contains("has its pads 1.27mm apart")
                && problems[1].contains("says a pitch of 1.25mm"),
            "{}",
            problems[1]
        );
        assert!(
            problems[2].contains("draws a body 3.9x4.9mm on `F.Fab`")
                && problems[2].contains("says 3.9x5mm"),
            "{}",
            problems[2]
        );
    }

    #[test]
    fn a_pitch_of_25_mil_is_not_one_of_065_mm() {
        let fp = soic8(0.635, true);
        assert_eq!(check("pitch=0.635mm", &fp), Vec::<String>::new());
        assert_eq!(check("pitch=0.65mm", &fp).len(), 1);
    }

    #[test]
    fn a_body_with_no_fabrication_drawing_cannot_be_measured() {
        let problems = check("body=3.9x4.9mm", &soic8(1.27, false));
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("draws no body on its fabrication layer"));
        // The other columns are still measured.
        assert_eq!(check("pads=8", &soic8(1.27, false)), Vec::<String>::new());
    }

    #[test]
    fn pads_of_one_number_are_one_and_have_no_pitch_between_them() {
        let fp = Footprint::parse(
            "Dfn",
            "(footprint \"Dfn\" (attr smd) \
             (pad \"1\" smd rect (at -1 0) (size 0.3 0.3) (layers \"F.Cu\")) \
             (pad \"2\" smd rect (at 1 0) (size 0.3 0.3) (layers \"F.Cu\")) \
             (pad \"3\" smd rect (at 0 0) (size 0.3 0.3) (layers \"F.Cu\")) \
             (pad \"3\" smd rect (at 0 0.4) (size 0.3 0.3) (layers \"F.Cu\")) \
             (pad \"\" smd rect (at 0 0.2) (size 0.2 0.2) (layers \"F.Paste\")))",
        )
        .unwrap();
        // Pad 3's two pieces are 0.4 mm apart, but they are one pad: the pitch is 1 mm.
        assert_eq!(footprint_pitch(&fp), Some(1.0));
        assert_eq!(check("pads=3, pitch=1mm", &fp), Vec::<String>::new());
    }
}
