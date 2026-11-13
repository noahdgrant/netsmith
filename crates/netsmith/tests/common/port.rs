//! The nets proof of a port of a KiCad board (#456), shared by the ports' tests: the design is
//! built, its parts are named as the original names them by the port's `reference/parts.toml`
//! (member path to the original's designator), and for every pad the set of the other pads on its
//! net must be the same on both sides. A port gives what is its own as data ([`Port`]): the
//! original's board file (or none, for a port that holds the derived `reference/netlist.xml`,
//! #560), the two-pad parts whose pads read the other way round, a pad the
//! original numbers otherwise, the parts with no nets (mounting holes) and the design's parts the
//! original does not have. What else a port proves (its values and packages, its open pins, ERC)
//! stays in its own file and uses the helpers here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use netsmith::compare::netlist_of_design;
use netsmith::port::PortParts;
use netsmith::port_tables::reference_netlist;
use netsmith_kicad::Mapping;
use netsmith_netlist::{Identity, Netlist, Options, Outcome, compare, report};

/// A pad: the original's designator of its part and its number.
pub type Pad = (String, String);

/// For each pad on a net with another pad, the other pads of that net.
pub type Neighbours = BTreeMap<Pad, BTreeSet<Pad>>;

/// What a port is and the differences from its original that it accepts.
pub struct Port {
    /// The folder of the example (`examples/<folder>`).
    pub folder: &'static str,
    /// The original's board file in `reference/`, or `None` when the port holds its derived
    /// tables instead (`reference/netlist.xml`, written by `netsmith port tables`, #560).
    pub board: Option<&'static str>,
    /// The name the messages give the original (`Easyduino has [...]`).
    pub original: &'static str,
    /// The original's two-pad parts whose pads 1 and 2 the design numbers the other way round: a
    /// resistor has no polarity, and the design enters it at pad 1.
    pub swapped: &'static [&'static str],
    /// A pad that the original's footprint numbers otherwise than the design's, as `(designator,
    /// the original's pad, the design's pad)`: the same pin on the same net.
    pub renumbered: &'static [(&'static str, &'static str, &'static str)],
    /// The original's parts with no nets (mounting holes), which the comparison of the wiring
    /// leaves out on both sides. One that `parts.toml` does not name is a part the design does
    /// not have.
    pub no_nets: &'static [&'static str],
    /// The member paths of the design's parts that the original does not have and `parts.toml`
    /// does not name (the physical file's holes, when the original's are left out).
    pub own: &'static [&'static str],
}

/// How the pads of a netlist are read: as they are, or with the port's differences applied (the
/// original's pads numbered as the design numbers them).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    AsIs,
    AsTheDesign,
}

impl Port {
    pub fn project(&self) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(self.folder)
    }

    /// `reference/parts.toml`.
    pub fn parts(&self) -> PortParts {
        PortParts::read(&self.project().join("reference/parts.toml"))
            .unwrap_or_else(|e| panic!("{e}"))
    }

    /// The original's netlist, read from its committed `.kicad_pcb` as a file
    /// (`Netlist::from_kicad_pcb`), or from `reference/netlist.xml` when the port names no board
    /// file, with no `kicad-cli`.
    pub fn reference(&self) -> Netlist {
        reference_netlist(&self.project().join("reference"), self.board)
            .unwrap_or_else(|e| panic!("{e}"))
    }

    /// The design's netlist, with the project's mapping of parts to footprints.
    pub fn design_netlist(&self, built: &netsmith::Built) -> Netlist {
        let mapping = Mapping::for_project(Some(&self.project())).unwrap_or_else(|e| panic!("{e}"));
        netlist_of_design(&built.design, &mapping).unwrap_or_else(|e| panic!("{e}"))
    }

    /// The original's designator of each part of `parts.toml`, by the design's designator. A path
    /// the design does not have is a panic that names it.
    pub fn designators(&self, built: &netsmith::Built) -> BTreeMap<String, String> {
        self.parts()
            .iter()
            .map(|(path, reference)| {
                let part = built
                    .design
                    .parts
                    .iter()
                    .find(|p| p.path == path)
                    .unwrap_or_else(|| panic!("{reference}: the board has no part `{path}`"));
                (part.refdes.clone(), reference.to_string())
            })
            .collect()
    }

    /// The number of pad `pad` of the original's part `reference` as `reading` says.
    fn pad(&self, reference: &str, pad: &str, reading: Reading) -> String {
        if reading == Reading::AsIs {
            return pad.to_string();
        }
        if self.swapped.contains(&reference) {
            match pad {
                "1" => return "2".to_string(),
                "2" => return "1".to_string(),
                _ => {}
            }
        }
        self.renumbered
            .iter()
            .find(|(r, theirs, _)| *r == reference && *theirs == pad)
            .map_or_else(|| pad.to_string(), |(_, _, ours)| (*ours).to_string())
    }

    /// For each pad that is on a net with another pad: the other pads of the net, in the
    /// original's designators (`to_reference` maps a designator of `netlist`; one it does not
    /// name keeps its name).
    pub fn neighbours(
        &self,
        netlist: &Netlist,
        to_reference: &BTreeMap<String, String>,
        reading: Reading,
    ) -> Neighbours {
        let mut out = BTreeMap::new();
        for net in &netlist.nets {
            let pads: BTreeSet<Pad> = net
                .pins
                .iter()
                .map(|(c, pin)| {
                    let refdes = &netlist.components[*c].refdes;
                    let refdes = to_reference.get(refdes).unwrap_or(refdes).clone();
                    let pin = self.pad(&refdes, pin, reading);
                    (refdes, pin)
                })
                .collect();
            let pads: Vec<Pad> = pads.into_iter().collect();
            if pads.len() < 2 {
                continue;
            }
            for pad in &pads {
                let others = pads.iter().filter(|p| *p != pad).cloned().collect();
                out.insert(pad.clone(), others);
            }
        }
        out
    }

    /// The design's pads on their nets, in the original's designators.
    pub fn ours(&self, built: &netsmith::Built) -> Neighbours {
        self.neighbours(
            &self.design_netlist(built),
            &self.designators(built),
            Reading::AsIs,
        )
    }

    /// The original's pads on their nets, numbered as the design numbers them.
    pub fn theirs(&self) -> Neighbours {
        self.theirs_of(&self.reference())
    }

    /// The pads on their nets of `reference`, a netlist of the original, as [`Port::theirs`]
    /// sees them.
    pub fn theirs_of(&self, reference: &Netlist) -> Neighbours {
        self.neighbours(reference, &BTreeMap::new(), Reading::AsTheDesign)
    }

    /// Every pad whose other pads on its net are not the same on both sides, one line each
    /// (empty when the two have the same connectivity), for the pads of either side.
    pub fn net_differences(&self, ours: &Neighbours, theirs: &Neighbours) -> Vec<String> {
        let mut differences = Vec::new();
        let pads: BTreeSet<&Pad> = ours.keys().chain(theirs.keys()).collect();
        let empty = BTreeSet::new();
        for pad in pads {
            let (a, b) = (
                ours.get(pad).unwrap_or(&empty),
                theirs.get(pad).unwrap_or(&empty),
            );
            if a != b {
                differences.push(format!(
                    "{}.{}: board has {:?}, {} has {:?}",
                    pad.0,
                    pad.1,
                    a.difference(b).collect::<Vec<_>>(),
                    self.original,
                    b.difference(a).collect::<Vec<_>>()
                ));
            }
        }
        differences
    }

    /// `netlist` with the value and footprint of every component blanked and the components of
    /// `drop` (by the designator of `netlist`) gone: only the wiring is left to compare.
    /// `to_reference` names the parts as the original does for `reading`.
    fn wiring_only(
        &self,
        netlist: &Netlist,
        to_reference: &BTreeMap<String, String>,
        drop: &BTreeSet<String>,
        reading: Reading,
    ) -> Netlist {
        let components = netlist
            .components
            .iter()
            .filter(|c| !drop.contains(&c.refdes))
            .map(|c| (c.refdes.clone(), String::new(), String::new()))
            .collect();
        let nets = netlist
            .nets
            .iter()
            .map(|n| {
                let pins = n
                    .pins
                    .iter()
                    .map(|(c, pin)| {
                        let refdes = netlist.components[*c].refdes.clone();
                        let renamed = to_reference.get(&refdes).unwrap_or(&refdes);
                        let pin = self.pad(renamed, pin, reading);
                        (refdes, pin)
                    })
                    .filter(|(refdes, _)| !drop.contains(refdes))
                    .collect();
                (n.name.clone(), pins)
            })
            .collect();
        Netlist::from_parts(components, nets).unwrap_or_else(|e| panic!("{e}"))
    }

    /// The design's wiring, without its parts that have no nets (those `parts.toml` names
    /// as a part of [`Port::no_nets`], and those of [`Port::own`]).
    pub fn wiring(&self, built: &netsmith::Built) -> Netlist {
        let to_reference = self.designators(built);
        let mut drop: BTreeSet<String> = to_reference
            .iter()
            .filter(|(_, reference)| self.no_nets.contains(&reference.as_str()))
            .map(|(refdes, _)| refdes.clone())
            .collect();
        drop.extend(
            built
                .design
                .parts
                .iter()
                .filter(|p| self.own.contains(&p.path.as_str()))
                .map(|p| p.refdes.clone()),
        );
        self.wiring_only(
            &self.design_netlist(built),
            &to_reference,
            &drop,
            Reading::AsIs,
        )
    }

    /// The original's wiring, numbered as the design numbers it, without its parts that have no
    /// nets.
    pub fn reference_wiring(&self) -> Netlist {
        let drop = self.no_nets.iter().map(|r| r.to_string()).collect();
        self.wiring_only(
            &self.reference(),
            &BTreeMap::new(),
            &drop,
            Reading::AsTheDesign,
        )
    }

    /// `netsmith-netlist`'s `compare` (what `netsmith compare` runs) of the design's wiring and
    /// the original's, with the parts paired by `parts.toml`.
    pub fn compare(&self, built: &netsmith::Built) -> Outcome {
        let pairs = self
            .designators(built)
            .into_iter()
            .filter(|(_, reference)| !self.no_nets.contains(&reference.as_str()))
            .collect();
        let options = Options {
            identity: Identity::Map(pairs),
            ..Options::default()
        };
        compare(&self.wiring(built), &self.reference_wiring(), &options)
            .unwrap_or_else(|e| panic!("{e}"))
    }

    /// The design has the parts of the original: every path of `parts.toml` (`parts` of them) is
    /// a part of the design, every part of the design is one of `parts.toml` or of
    /// [`Port::own`], and the original's `reference_parts` parts are those of `parts.toml` and
    /// those of [`Port::no_nets`] that it does not name, which have no pad on a net.
    pub fn assert_has_the_parts(
        &self,
        built: &netsmith::Built,
        parts: usize,
        reference_parts: usize,
    ) {
        let map = self.parts();
        assert_eq!(map.len(), parts, "the parts of reference/parts.toml");
        assert_eq!(self.designators(built).len(), parts);
        let own: Vec<&str> = built
            .design
            .parts
            .iter()
            .map(|p| p.path.as_str())
            .filter(|path| map.reference_of(path).is_none())
            .collect();
        assert_eq!(
            own, self.own,
            "a part that is not {}'s, and not one the port knows",
            self.original
        );
        let reference = self.reference();
        let theirs: BTreeSet<&str> = reference
            .components
            .iter()
            .map(|c| c.refdes.as_str())
            .collect();
        let ours: BTreeSet<&str> = map.iter().map(|(_, r)| r).collect();
        assert_eq!(theirs.len(), reference_parts, "the parts of the original");
        let unpaired: Vec<&str> = self
            .no_nets
            .iter()
            .copied()
            .filter(|r| map.path_of(r).is_none())
            .collect();
        assert_eq!(
            theirs.difference(&ours).copied().collect::<Vec<_>>(),
            unpaired,
            "the original's parts that the board does not have"
        );
        assert_eq!(
            ours.difference(&theirs).copied().collect::<Vec<_>>(),
            Vec::<&str>::new(),
            "parts.toml names a part the original does not have"
        );
        let theirs = self.theirs();
        for hole in self.no_nets {
            assert!(
                !theirs.keys().any(|(r, _)| r == hole),
                "{hole} is on a net of the original"
            );
        }
        assert!(built.schematic.starts_with("(kicad_sch"));
    }

    /// The proof: for every pad of every part of the original, the set of the other pads on its
    /// net is the same as on the design's, with the port's differences applied; both sides have
    /// more than `min_pads` pads on nets, the same number.
    pub fn assert_same_pads_on_nets(&self, built: &netsmith::Built, min_pads: usize) {
        let ours = self.ours(built);
        let theirs = self.theirs();
        let differences = self.net_differences(&ours, &theirs);
        assert!(differences.is_empty(), "{}", differences.join("\n"));
        assert!(
            theirs.len() > min_pads && ours.len() == theirs.len(),
            "{} {}",
            ours.len(),
            theirs.len()
        );
    }

    /// `compare` says the design's wiring and the original's are the same, the parts paired by
    /// `parts.toml`.
    pub fn assert_compare_matches(&self, built: &netsmith::Built) {
        let outcome = self.compare(built);
        assert!(matches!(outcome, Outcome::Match(_)), "{}", report(&outcome));
    }

    /// The proof sees a wrong net: the design's pad `pad` (the original's designator and pad
    /// number) said to be on the net of `other` is a difference at `pad`, and so is the pad
    /// taken off its net.
    pub fn assert_sees_a_wrong_net(
        &self,
        built: &netsmith::Built,
        pad: (&str, &str),
        other: (&str, &str),
    ) {
        let theirs = self.theirs();
        let mut ours = self.ours(built);
        assert_eq!(self.net_differences(&ours, &theirs), Vec::<String>::new());
        let key = (pad.0.to_string(), pad.1.to_string());
        let wrong = ours
            .get(&(other.0.to_string(), other.1.to_string()))
            .unwrap_or_else(|| panic!("{}.{} is on no net", other.0, other.1))
            .clone();
        assert_ne!(ours[&key], wrong, "the wrong net must be another one");
        ours.insert(key.clone(), wrong);
        let at = format!("{}.{}: board has", pad.0, pad.1);
        let differences = self.net_differences(&ours, &theirs);
        assert!(
            differences.iter().any(|d| d.starts_with(&at)),
            "{differences:?}"
        );
        // A pad that is on a net on one side only is a difference as well.
        let mut ours = self.ours(built);
        ours.remove(&key);
        let differences = self.net_differences(&ours, &theirs);
        assert!(
            differences
                .iter()
                .any(|d| d.starts_with(&format!("{at} []"))),
            "{differences:?}"
        );
    }
}
