//! Supply channels (ADR 0047, ADR 0143): where a project buys a part and how the stock and
//! price are read.
//!
//! A channel is named by who sells and how (`jlcpcb-assembly`, `lcsc-retail`,
//! `digikey-retail`, `mouser-retail`), reads the offers file of one supplier
//! ([`crate::offers`]) and has its own rule for choosing among the rows that fit a generic part
//! ([`Rule`]):
//!
//! | channel | offers it reads | rule |
//! |---|---|---|
//! | `jlcpcb-assembly` | `lcsc.csv`, only the offers that have a `class` (the parts JLCPCB's library lists) | [`Rule::Class`]: Basic, then Preferred, then Extended |
//! | `lcsc-retail` | `lcsc.csv`, every offer | [`Rule::StockThenPrice`] |
//! | `digikey-retail` | `digikey.csv` | [`Rule::StockThenPrice`] |
//! | `mouser-retail` | `mouser.csv` | [`Rule::StockThenPrice`] |
//!
//! A project lists its channels in order (`[supply] channels` of `netsmith.toml`, default
//! [`Channel::DEFAULT`]), and the first channel with an offer for a part buys it
//! ([`crate::Catalogs::channels`]). A channel is not `.assembly` (who places the part, ADRs
//! 0024 and 0084): the two are independent. The words are the ones a manifest, a lock and a
//! supply file write; a new channel is a row here, with the offers file it reads and its rule.

use std::fmt;

use crate::catalog::Library;
use crate::offers::Offer;

/// How a channel chooses among the rows that fit a generic part, after the project's
/// `preferred.csv` ([`crate::Preferred`]) and before the file order of the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rule {
    /// JLCPCB's sorting of its assembly library: Basic, then Preferred, then Extended (a part
    /// with no class last). A Basic part costs no feeder fee.
    Class,
    /// A row the provider says is in stock, then one whose stock is unknown; among those the
    /// lowest price of one piece, a row with no price after the priced ones. A row with no
    /// stock is passed over in every channel.
    StockThenPrice,
}

impl Rule {
    /// Where a class stands in [`Rule::Class`]: Basic 0, Preferred 1, Extended 2, and a part
    /// with no class 3.
    pub fn class_rank(library: Option<Library>) -> u8 {
        match library {
            Some(Library::Basic) => 0,
            Some(Library::Preferred) => 1,
            Some(Library::Extended) => 2,
            None => 3,
        }
    }
}

/// A supply channel: one row of [`Channel::ALL`].
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Channel {
    pub(crate) word: &'static str,
    supplier: &'static str,
    needs_class: bool,
    rule: Rule,
    describe: &'static str,
}

impl Channel {
    /// JLCPCB's assembly stock: the pieces it holds for the boards it assembles, bought by the
    /// LCSC numbers its parts library lists (the `lcsc` offers that have a `class`).
    pub const JLCPCB_ASSEMBLY: Channel = Channel {
        word: "jlcpcb-assembly",
        supplier: "lcsc",
        needs_class: true,
        rule: Rule::Class,
        describe: "JLCPCB assembly stock",
    };
    /// LCSC's retail stock: the pieces LCSC sells to a buyer.
    pub const LCSC_RETAIL: Channel = Channel {
        word: "lcsc-retail",
        supplier: "lcsc",
        needs_class: false,
        rule: Rule::StockThenPrice,
        describe: "LCSC retail stock",
    };
    /// Digi-Key's retail stock, bought by its numbers (`offers/digikey.csv`).
    pub const DIGIKEY_RETAIL: Channel = Channel {
        word: "digikey-retail",
        supplier: "digikey",
        needs_class: false,
        rule: Rule::StockThenPrice,
        describe: "Digi-Key retail stock",
    };
    /// Mouser's retail stock, bought by its numbers (`offers/mouser.csv`).
    pub const MOUSER_RETAIL: Channel = Channel {
        word: "mouser-retail",
        supplier: "mouser",
        needs_class: false,
        rule: Rule::StockThenPrice,
        describe: "Mouser retail stock",
    };

    /// Every channel netsmith knows, in the order a pick lock lists a part's skus.
    pub const ALL: [Channel; 4] = [
        Channel::JLCPCB_ASSEMBLY,
        Channel::LCSC_RETAIL,
        Channel::DIGIKEY_RETAIL,
        Channel::MOUSER_RETAIL,
    ];

    /// The channels of a project whose manifest names none: JLCPCB's assembly, then LCSC's
    /// retail stock for a part JLCPCB's library does not list.
    pub const DEFAULT: [Channel; 2] = [Channel::JLCPCB_ASSEMBLY, Channel::LCSC_RETAIL];

    /// The word a manifest, a lock and a supply file use: `jlcpcb-assembly`.
    pub fn word(self) -> &'static str {
        self.word
    }

    /// The channel a word names, if netsmith knows it.
    pub fn parse(word: &str) -> Option<Channel> {
        Channel::ALL.into_iter().find(|c| c.word == word.trim())
    }

    /// What a person reads: `JLCPCB assembly stock`.
    pub fn describe(self) -> &'static str {
        self.describe
    }

    /// The supplier whose offers file the channel reads (`lcsc`).
    pub fn supplier(self) -> &'static str {
        self.supplier
    }

    /// How the channel chooses among the rows that fit a generic part.
    pub fn rule(self) -> Rule {
        self.rule
    }

    /// Whether the channel buys by this offer: one of its supplier's, and, for
    /// `jlcpcb-assembly`, one that has a `class` (a part JLCPCB's library lists).
    pub fn takes(self, offer: &Offer) -> bool {
        offer.supplier == self.supplier && (!self.needs_class || offer.class.is_some())
    }

    /// The words of every channel, as a message lists them: `` `jlcpcb-assembly`,
    /// `lcsc-retail`, `digikey-retail` or `mouser-retail` ``.
    pub fn words() -> String {
        let all: Vec<String> = Channel::ALL
            .iter()
            .map(|c| format!("`{}`", c.word))
            .collect();
        let (last, rest) = all.split_last().expect("there are channels");
        format!("{} or {last}", rest.join(", "))
    }

    /// The words of `channels`, as a message lists the ones tried: `jlcpcb-assembly,
    /// lcsc-retail`.
    pub fn list(channels: &[Channel]) -> String {
        channels
            .iter()
            .map(|c| c.word)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The channel a part names for itself (`[supply.parts]` of `netsmith.toml`, ADR 0143 item 4),
/// with what a message calls the entry. The part is bought by that channel only: its offer
/// there gives the sku and the class, its rule orders the rows that fit a generic part, and a
/// part with no offer in it is unresolved by name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NamedChannel {
    pub channel: Channel,
    /// `netsmith.toml: [supply.parts.Board] "ldo.regulator"`: the file and the key.
    pub key: String,
}

impl NamedChannel {
    /// The clause a message adds: `` `digikey-retail`, the supply channel netsmith.toml names
    /// for the part ([supply.parts.Board] "u3") ``.
    pub fn named_by(&self) -> String {
        match self.key.split_once(": ") {
            Some((file, key)) => format!(
                "`{}`, the supply channel {file} names for the part ({key})",
                self.channel
            ),
            None => format!(
                "`{}`, the supply channel the manifest names for the part ({})",
                self.channel, self.key
            ),
        }
    }
}

/// One entry of `[supply.parts]`: the part at `path` of the board or system `unit` (the table
/// its pick lock entry is under) is bought by `named` only. `unit` is `None` for a bare path,
/// which names a part of the project's only board; the build refuses it when the project has
/// several.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartChannel {
    pub unit: Option<String>,
    /// The part's path in its board or system, as a hand pick and the pick lock name it:
    /// `ldo.regulator`, `decoupling[2]`, `power.stage.regulator` in a system.
    pub path: String,
    pub named: NamedChannel,
}

/// The `[supply.parts]` entries of a project, with the names of the boards and systems its
/// top-level files declare: what the build checks an entry's board against, since one file's
/// build sees only its own boards.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PartChannels {
    pub entries: Vec<PartChannel>,
    /// The boards and systems of every top-level file of the project, by name.
    pub declared: Vec<String>,
}

impl PartChannels {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The channel the part at `path` of `unit` names for itself, if any: an entry under
    /// `unit`, or a bare entry (which the build allows only when the project has one board).
    pub fn of(&self, unit: &str, path: &str) -> Option<&NamedChannel> {
        self.entries
            .iter()
            .filter(|e| e.path == path)
            .find(|e| e.unit.as_deref().is_none_or(|u| u == unit))
            .map(|e| &e.named)
    }
}

impl fmt::Debug for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word)
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_names_a_channel_and_an_unknown_word_none() {
        for c in Channel::ALL {
            assert_eq!(Channel::parse(c.word()), Some(c));
        }
        assert_eq!(Channel::parse(" lcsc-retail "), Some(Channel::LCSC_RETAIL));
        assert_eq!(Channel::parse("jlcpcb"), None);
        assert_eq!(
            Channel::words(),
            "`jlcpcb-assembly`, `lcsc-retail`, `digikey-retail` or `mouser-retail`"
        );
        assert_eq!(
            Channel::list(&Channel::DEFAULT),
            "jlcpcb-assembly, lcsc-retail"
        );
    }
}
