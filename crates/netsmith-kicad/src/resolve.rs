//! The symbol each part of a design is drawn with, settled for the design as a whole.
//!
//! [`SymbolLib::symbol_of`] answers for one part. Two things go wrong only when the parts are
//! seen together, and both are settled here, so that the schematic, the board's
//! `netsmith.kicad_sym` and the power-flag rule read one answer ([`part_symbols`]):
//!
//! - **One id, two drawings.** A symbol drawn from a pin table is named for the part's kind and
//!   package (`netsmith:Lt8641_UDC20_18`). Two parts of one kind and package can still differ
//!   in a pin's name or pads, and a schematic and a library file hold one symbol for an id, so
//!   such a pair would be a library mismatch in KiCad. Where two drawings meet at one id, each
//!   symbol that is drawn from a pin table takes the id with a short hash of its drawing
//!   appended ([`crate::generate::symbol_hashed`]), whatever the order of the parts. A design
//!   with no clash keeps the plain names, which tests and written files spell. The same goes for
//!   a name that `std/libs/netsmith.kicad_sym` already holds.
//! - **Two outputs on one net.** A power pin that the solved design says gives power is a
//!   `power_out` ([`crate::generate::pin_type`]), and KiCad's pin matrix reports an output
//!   driving an output. Where a net holds more than one, one stays the output and the others
//!   are drawn as pins of unknown direction, which are `power_in`: an input is satisfied by the
//!   output on its net, the net is driven, and it gets no power flag. The pin that stays is
//!   one that cannot be changed (a library symbol's, which netsmith does not redraw) or,
//!   failing that, the first by designator and pad, so the order of the parts does not choose.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use netsmith_core::{Design, Part};

use crate::generate;
use crate::mapping::{Mapped, Mapping};
use crate::schematic::EmitError;
use crate::sexp::Sexp;
use crate::stdlib;
use crate::symbols::{PinType, Symbol, SymbolLib};

/// What a part is drawn with: the row of the mapping and the symbol, or why it cannot be.
pub type PartSymbol = Result<(Mapped, Arc<Symbol>), EmitError>;

/// The symbol of every part of `design`, in the order of its parts, as the schematic draws it.
pub fn part_symbols(design: &Design, mapping: &Mapping, lib: &SymbolLib) -> Vec<PartSymbol> {
    settle(design, mapping, lib, stdlib::file_symbol_names())
}

/// [`part_symbols`], with `reserved` the ids that a drawing from a pin table may not take.
fn settle(
    design: &Design,
    mapping: &Mapping,
    lib: &SymbolLib,
    reserved: &[String],
) -> Vec<PartSymbol> {
    let mut drawn: Vec<PartSymbol> = design
        .parts
        .iter()
        .map(|part| {
            let mapped = mapping
                .map(&part.kind, part.package.as_deref())
                .map_err(|error| EmitError::Map {
                    refdes: part.refdes.clone(),
                    error,
                })?;
            let symbol = lib
                .symbol_of(part, &mapped)
                .map_err(|error| EmitError::Symbol {
                    refdes: part.refdes.clone(),
                    error,
                })?;
            Ok((mapped, symbol))
        })
        .collect();

    // The part each symbol was drawn from: the design's, or a copy whose giving pins were changed.
    let mut parts: Vec<Part> = design.parts.clone();
    for (index, pads) in outputs_to_give_up(design, &drawn) {
        let Ok((mapped, _)) = &drawn[index] else {
            continue;
        };
        let mapped = mapped.clone();
        for pin in &mut parts[index].pins {
            if pin.gives_power() && pin.pads.first().is_some_and(|pad| pads.contains(pad)) {
                pin.power_flow = None;
            }
        }
        if let Some(redrawn) = generate::symbol(&parts[index], &mapped.footprint) {
            drawn[index] = Ok((mapped, Arc::new(redrawn)));
        }
    }

    // Ids that two different drawings (or a drawing and the library file) meet at.
    let mut drawings: HashMap<&str, Vec<&Sexp>> = HashMap::new();
    for (_, symbol) in drawn.iter().flatten() {
        let seen = drawings.entry(symbol.lib_id()).or_default();
        if !seen.contains(&symbol.definition()) {
            seen.push(symbol.definition());
        }
    }
    let taken: HashSet<String> = drawings
        .iter()
        .filter(|(id, seen)| seen.len() > 1 || reserved.iter().any(|r| r == **id))
        .map(|(id, _)| (*id).to_string())
        .collect();
    if !taken.is_empty() {
        for (index, slot) in drawn.iter_mut().enumerate() {
            let Ok((mapped, symbol)) = slot else { continue };
            // Only a drawing from a pin table can be given another name: a row that names a
            // symbol names its id, and the library symbol is one drawing.
            if mapped.lib_id.is_none()
                && taken.contains(symbol.lib_id())
                && let Some(renamed) = generate::symbol_hashed(&parts[index], &mapped.footprint)
            {
                *symbol = Arc::new(renamed);
            }
        }
    }
    drawn
}

/// The pins that must stop being power outputs, as `(part, first pads)` for the parts to redraw:
/// on each net that holds more than one `power_out`, every one that can be redrawn but the
/// first.
fn outputs_to_give_up(design: &Design, drawn: &[PartSymbol]) -> Vec<(usize, Vec<String>)> {
    let index_of: HashMap<&str, usize> = design
        .parts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.refdes.as_str(), i))
        .collect();
    let mut give_up: Vec<(usize, Vec<String>)> = Vec::new();
    for net in &design.nets {
        // (refdes, pad, part, whether the symbol is fixed: not drawn from a pin table).
        let mut outputs: Vec<(&str, &str, usize, bool)> = Vec::new();
        for pin in &net.pins {
            let Some(&index) = index_of.get(pin.refdes.as_str()) else {
                continue;
            };
            let Ok((mapped, symbol)) = &drawn[index] else {
                continue;
            };
            if symbol.pin_type(&pin.pin) == Some(PinType::PowerOut) {
                outputs.push((&pin.refdes, &pin.pin, index, mapped.lib_id.is_some()));
            }
        }
        if outputs.len() < 2 {
            continue;
        }
        outputs.sort_unstable();
        let keeper = outputs.iter().position(|o| o.3).unwrap_or(0);
        for (k, &(_, pad, index, fixed)) in outputs.iter().enumerate() {
            if k == keeper || fixed {
                continue;
            }
            match give_up.iter_mut().find(|(i, _)| *i == index) {
                Some((_, pads)) => pads.push(pad.to_string()),
                None => give_up.push((index, vec![pad.to_string()])),
            }
        }
    }
    give_up
}

#[cfg(test)]
mod tests {
    use super::*;
    use netsmith_core::{Net, PartPin, PinKind, PinRef, PowerFlow};

    fn pin(name: &str, datasheet: &str, pad: &str, flow: Option<PowerFlow>) -> PartPin {
        PartPin {
            name: name.into(),
            datasheet: datasheet.into(),
            kind: PinKind::Power,
            pads: vec![pad.into()],
            function: None,
            power_flow: flow,
        }
    }

    fn part(refdes: &str, kind: &str, pins: Vec<PartPin>) -> Part {
        Part {
            // What the design stage records for a standard connector (ADR 0068).
            provides: if netsmith_testkit::std_kind(kind) == Some("connector") {
                vec!["Connector".to_string()]
            } else {
                Vec::new()
            },
            declared_kind: netsmith_testkit::std_kind(kind)
                .and_then(netsmith_core::PartKind::from_word),
            refdes: refdes.into(),
            kind: kind.into(),
            value: String::new(),
            package: Some("SOT-23".into()),
            path: String::new(),
            pins,
        }
    }

    fn design(parts: Vec<Part>, nets: Vec<(&str, Vec<(&str, &str)>)>) -> Design {
        Design {
            modules: Vec::new(),
            name: "d".into(),
            process: None,
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
            parts,
            nets: nets
                .into_iter()
                .map(|(name, pins)| Net {
                    name: name.into(),
                    pins: pins
                        .into_iter()
                        .map(|(refdes, pin)| PinRef {
                            refdes: refdes.into(),
                            pin: pin.into(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn mapping() -> Mapping {
        let row = "[[part]]\nkind = \"IcLdo\"\npackage = \"SOT-23\"\nfootprint = \"Package_TO_SOT_SMD:SOT-23\"\n";
        Mapping::standard()
            .merged(Mapping::parse(row, "test", std::path::Path::new(".")).unwrap())
            .unwrap()
    }

    fn ids(drawn: Vec<PartSymbol>) -> Vec<String> {
        drawn
            .into_iter()
            .map(|r| r.unwrap().1.lib_id().to_string())
            .collect()
    }

    fn ldo(refdes: &str, output: &str) -> Part {
        part(
            refdes,
            "IcLdo",
            vec![
                pin("vin.pos", "IN", "3", Some(PowerFlow::Takes)),
                pin("vout.pos", output, "2", Some(PowerFlow::Gives)),
            ],
        )
    }

    #[test]
    fn a_design_with_no_clash_keeps_the_plain_names_and_the_clash_is_settled_whatever_the_order() {
        let lib = SymbolLib::at(netsmith_testkit::sample_symbols());
        let nets = vec![("A", vec![("U1", "2")]), ("B", vec![("U2", "2")])];
        let alone = design(vec![ldo("U1", "OUT")], vec![("A", vec![("U1", "2")])]);
        assert_eq!(
            ids(part_symbols(&alone, &mapping(), &lib)),
            ["netsmith:IcLdo_SOT-23_out2"]
        );
        let twins = design(vec![ldo("U1", "OUT"), ldo("U2", "OUT")], nets.clone());
        let twin_ids = ids(part_symbols(&twins, &mapping(), &lib));
        assert_eq!(twin_ids[0], twin_ids[1]);
        assert_eq!(twin_ids[0], "netsmith:IcLdo_SOT-23_out2");

        let apart = design(vec![ldo("U1", "OUT"), ldo("U2", "VOUT")], nets.clone());
        let a = ids(part_symbols(&apart, &mapping(), &lib));
        assert_ne!(a[0], a[1]);
        for id in &a {
            // The plain name and eight hex digits.
            let hash = id.strip_prefix("netsmith:IcLdo_SOT-23_out2_").unwrap();
            assert_eq!(hash.len(), 8, "{id}");
            assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "{id}");
        }
        let swapped = design(vec![ldo("U2", "VOUT"), ldo("U1", "OUT")], nets);
        let b = ids(part_symbols(&swapped, &mapping(), &lib));
        assert_eq!([b[1].clone(), b[0].clone()], [a[0].clone(), a[1].clone()]);
    }

    #[test]
    fn a_drawing_may_not_take_a_name_that_the_library_file_holds() {
        let lib = SymbolLib::at(netsmith_testkit::sample_symbols());
        let d = design(vec![ldo("U1", "OUT")], vec![("A", vec![("U1", "2")])]);
        let drawn = settle(
            &d,
            &mapping(),
            &lib,
            &["netsmith:IcLdo_SOT-23_out2".to_string()],
        );
        let id = ids(drawn).remove(0);
        assert!(id.starts_with("netsmith:IcLdo_SOT-23_out2_"), "{id}");
        // The library file of the repository holds nothing under the names a board draws.
        assert!(!stdlib::file_symbol_names().contains(&"netsmith:IcLdo_SOT-23_out2".to_string()));
    }

    #[test]
    fn of_two_outputs_on_a_net_the_first_stays_and_the_design_is_not_changed() {
        let lib = SymbolLib::at(netsmith_testkit::sample_symbols());
        let d = design(
            vec![ldo("U2", "OUT"), ldo("U1", "OUT")],
            vec![("RAIL", vec![("U2", "2"), ("U1", "2")])],
        );
        let drawn = part_symbols(&d, &mapping(), &lib);
        let kind = |i: usize| drawn[i].as_ref().unwrap().1.pin_type("2");
        assert_eq!(kind(1), Some(PinType::PowerOut), "U1 is first");
        assert_eq!(kind(0), Some(PinType::PowerIn), "U2 gives way");
        assert!(d.parts[0].pins.iter().any(PartPin::gives_power));
    }

    #[test]
    fn an_output_alone_on_its_net_stays_an_output() {
        let lib = SymbolLib::at(netsmith_testkit::sample_symbols());
        let d = design(
            vec![ldo("U1", "OUT"), ldo("U2", "OUT")],
            vec![("A", vec![("U1", "2")]), ("B", vec![("U2", "2")])],
        );
        for (_, symbol) in part_symbols(&d, &mapping(), &lib).into_iter().flatten() {
            assert_eq!(symbol.pin_type("2"), Some(PinType::PowerOut));
        }
    }
}
