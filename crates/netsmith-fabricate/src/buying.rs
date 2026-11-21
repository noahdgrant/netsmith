//! What a person buys for a board, by supply channel (ADR 0143, issue #1461).
//!
//! The board house buys the parts it assembles from its stock (`AssemblyMode::Fab`), by the LCSC
//! numbers of `bom.csv` ([`crate::fab::bom_entries`]). The other parts a board needs are the
//! owner's to buy: a consigned part (ADR 0084), which the owner ships to the board house to be
//! placed, and a part soldered by hand (ADR 0024). Each is bought in the supply channel that buys
//! it ([`netsmith_design::Sourced::channel`]: the first of the project's channels with an offer
//! of it, or the one `[supply.parts]` names), by that channel's sku. This module lists them, a
//! [`BuyLine`] for each part number of each channel, for `fab-notes.txt` ([`notes`]) and for
//! `order.txt` of an order, which adds the quantities of its boards and prices each line from
//! that channel's price ladder at the quantity of the whole order (issue #804).
//!
//! A part no channel buys has no line here: `fab-notes.txt` already names a hand-soldered one
//! that has a manufacturer's number and no offer ([`crate::no_supplier::note`]), and a consigned
//! one is in its `consigned:` line with its manufacturer's number. A part that is not fitted is
//! bought by nobody.
//!
//! No file a supplier's upload reads is written for these lines: the formats of Digi-Key's and
//! Mouser's bill of materials uploads could not be read from their own documentation, so no
//! column is written from memory. Issue #1498 tried on 2026-11-21: Mouser's BOM tool and its help
//! article refuse an automated reader, and Digi-Key's myLists and cart pages name no column,
//! header or limit. Issue #1563 has the pages and waits for a person to read each upload screen
//! after signing in.

use netsmith_design::{Assembly, Sourced};
use netsmith_solve::Channel;
use serde::{Deserialize, Serialize};

/// One part number the owner buys in one supply channel, for one board, and where it goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuyLine {
    /// The word of the supply channel that buys it: `digikey-retail`.
    pub channel: String,
    /// Its number in that channel.
    pub sku: String,
    pub manufacturer: Option<String>,
    pub mpn: Option<String>,
    /// The part's value, which names it when it has no manufacturer's number.
    pub value: String,
    /// The parts of the board it is, by designator, in order.
    pub designators: Vec<String>,
    /// The owner ships it to the board house, which places it (ADR 0084); otherwise it is
    /// soldered by hand.
    pub consigned: bool,
}

impl BuyLine {
    /// The channel, if netsmith knows its word (it always does for a line made here).
    pub fn channel(&self) -> Option<Channel> {
        Channel::parse(&self.channel)
    }

    /// What the part is, as a person asks a supplier for it: `Acme ZZR-1234`, the manufacturer's
    /// number alone where the row names no manufacturer, the value where it names neither.
    pub fn what(&self) -> String {
        match (&self.manufacturer, &self.mpn) {
            (Some(m), Some(mpn)) => format!("{m} {mpn}"),
            (None, Some(mpn)) => mpn.clone(),
            _ => self.value.clone(),
        }
    }

    /// `consigned` or `hand-solder`: the word of `fab-notes.txt` for who places it.
    pub fn placed_by(&self) -> &'static str {
        if self.consigned {
            "consigned"
        } else {
            "hand-solder"
        }
    }
}

/// The parts of `sourcing` (after generic parts were resolved) that the owner buys, a line for
/// each channel, sku and way of placing, by the order of [`Channel::ALL`] and then by first
/// designator.
pub fn lines(sourcing: &[Sourced]) -> Vec<BuyLine> {
    let mut lines: Vec<BuyLine> = Vec::new();
    for s in sourcing {
        if s.not_fitted.is_some() || !matches!(s.assembly, Assembly::Consigned | Assembly::Hand) {
            continue;
        }
        let (Some(channel), Some(sku)) = (s.channel, &s.sku) else {
            continue;
        };
        let consigned = s.assembly == Assembly::Consigned;
        match lines
            .iter_mut()
            .find(|l| l.channel == channel.word() && &l.sku == sku && l.consigned == consigned)
        {
            Some(line) => line.designators.push(s.refdes.clone()),
            None => lines.push(BuyLine {
                channel: channel.word().to_string(),
                sku: sku.clone(),
                manufacturer: s.manufacturer.clone(),
                mpn: s.mpn.clone(),
                value: s.value.clone(),
                designators: vec![s.refdes.clone()],
                consigned,
            }),
        }
    }
    for l in &mut lines {
        l.designators.sort_by_key(|d| designator_key(d));
    }
    lines.sort_by_key(|l| (channel_rank(&l.channel), designator_key(&l.designators[0])));
    lines
}

/// Where a channel's word stands in [`Channel::ALL`], for the order of the lists.
pub fn channel_rank(word: &str) -> usize {
    Channel::ALL
        .iter()
        .position(|c| c.word() == word)
        .unwrap_or(Channel::ALL.len())
}

/// The lines of `fab-notes.txt` for `lines`: one for each channel, in the order of
/// [`Channel::ALL`], `buy from digikey-retail (Digi-Key retail stock): 296-1-ND x2, Acme ZZR-1,
/// for R1, R2 (consigned); ...`. Empty when the owner buys nothing.
pub fn notes(lines: &[BuyLine]) -> Vec<String> {
    let mut out = Vec::new();
    let mut words: Vec<&str> = lines.iter().map(|l| l.channel.as_str()).collect();
    words.dedup();
    for word in words {
        let describe =
            Channel::parse(word).map_or(String::new(), |c| format!(" ({})", c.describe()));
        let items: Vec<String> = lines
            .iter()
            .filter(|l| l.channel == word)
            .map(|l| {
                format!(
                    "{} x{}, {}, for {} ({})",
                    l.sku,
                    l.designators.len(),
                    l.what(),
                    l.designators.join(", "),
                    l.placed_by()
                )
            })
            .collect();
        out.push(format!(
            "buy from {word}{describe}, for the parts you ship to the board house or solder yourself: {}",
            items.join("; ")
        ));
    }
    out
}

/// `C2` before `C10`: the letters, then the number.
fn designator_key(designator: &str) -> (String, u32) {
    let split = designator
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(designator.len());
    let (letters, digits) = designator.split_at(split);
    (letters.to_string(), digits.parse().unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(
        refdes: &str,
        assembly: Assembly,
        channel: Option<Channel>,
        sku: Option<&str>,
    ) -> Sourced {
        Sourced {
            refdes: refdes.into(),
            kind: "Resistor".into(),
            value: "1k".into(),
            package: Some("0402".into()),
            sku: sku.map(Into::into),
            channel,
            manufacturer: Some("Acme".into()),
            mpn: Some("ZZR-1".into()),
            library: None,
            assembly,
            assembly_stated: true,
            catalog_height: None,
            generic: None,
            resolved_row: None,
            not_fitted: None,
        }
    }

    #[test]
    fn the_owner_buys_the_consigned_and_hand_parts_by_channel_and_the_board_house_the_rest() {
        let dk = Some(Channel::DIGIKEY_RETAIL);
        let lcsc = Some(Channel::LCSC_RETAIL);
        let sourcing = [
            part("R10", Assembly::Consigned, dk, Some("296-1-ND")),
            part("R2", Assembly::Consigned, dk, Some("296-1-ND")),
            part("J1", Assembly::Hand, lcsc, Some("C42")),
            part(
                "R3",
                Assembly::Fab,
                Some(Channel::JLCPCB_ASSEMBLY),
                Some("C25"),
            ),
            part("R4", Assembly::Hand, None, None),
            part("R5", Assembly::None, dk, Some("296-1-ND")),
        ];
        let lines = lines(&sourcing);
        let got: Vec<(&str, &str, Vec<&str>, bool)> = lines
            .iter()
            .map(|l| {
                (
                    l.channel.as_str(),
                    l.sku.as_str(),
                    l.designators.iter().map(String::as_str).collect(),
                    l.consigned,
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                ("lcsc-retail", "C42", vec!["J1"], false),
                ("digikey-retail", "296-1-ND", vec!["R2", "R10"], true),
            ]
        );
        assert_eq!(
            notes(&lines),
            [
                "buy from lcsc-retail (LCSC retail stock), for the parts you ship to the board house or solder yourself: C42 x1, Acme ZZR-1, for J1 (hand-solder)",
                "buy from digikey-retail (Digi-Key retail stock), for the parts you ship to the board house or solder yourself: 296-1-ND x2, Acme ZZR-1, for R2, R10 (consigned)",
            ]
        );
    }
}
