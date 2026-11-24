//! The switching regulators of a design and the parts round them (issue #1585), for the
//! power-stage report of `board nets --power-stage` and the `switch_node_copper` note of `board
//! drc`.
//!
//! **How a switching regulator is recognised.** A part that provides the standard library's
//! `BuckIc` (`std::parts::regulator`): every standard buck IC, and a project's own buck IC
//! that `Buck<Ic>` takes. Its roles are the pins the generic declares, which every provider
//! has by name: `vin.pos` is VIN, `vin.neg` its ground, `sw.pos` the switch node and `fb` the
//! feedback pin. A part that states `dc switching_regulator` without providing `BuckIc` (a
//! buck-boost IC that states the power balance on its own output rail) has no pin the model
//! calls its switch node, and is not recognised.
//!
//! The parts round it are found by the nets and by the kind (`#[kind(capacitor)]`, ADR 0125):
//! an **input capacitor** has a pad on the VIN net and one on the ground net; a **support
//! capacitor** has a pad on the net of one of the IC's other pins (a bootstrap pin, `BST`,
//! `BOOT`, `CB`; an internal supply, `INTVCC`, `VCC`; a soft start), and is a **boot**
//! capacitor when its other pad is on the switch node. A pin the IC ties to VIN, ground, the
//! switch node or the feedback net is no support pin.

use crate::{Design, PartKind};

/// The standard generic a switching regulator provides.
pub const BUCK_IC: &str = "BuckIc";

/// A switching regulator of a design: the IC, its pins by role, and the capacitors round it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SwitchingRegulator {
    pub refdes: String,
    /// The IC's path in the design (`reg.ic`).
    pub path: String,
    pub vin: RegulatorPin,
    pub gnd: RegulatorPin,
    /// The switch node: the IC's `sw.pos`.
    pub sw: RegulatorPin,
    /// The feedback pin; `None` for a provider whose pin has no pad.
    pub fb: Option<RegulatorPin>,
    /// The capacitors between the VIN net and the ground net, by reference designator.
    pub input_caps: Vec<String>,
    /// The capacitors on the IC's other pins.
    pub support: Vec<SupportCap>,
    /// The parts other than the IC with a pad on the switch node (the inductor, a catch diode,
    /// a boot capacitor): their nets are the power stage's own, never foreign copper.
    pub switch_parts: Vec<String>,
}

/// A role of a regulator's pin: the net it is on and the IC's pads of it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RegulatorPin {
    pub net: String,
    pub pads: Vec<String>,
}

/// A capacitor on a pin of the IC that is none of its roles.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SupportCap {
    /// The pin as its datasheet names it (`INTVCC`, `BOOT`).
    pub pin: String,
    /// The IC's pads of the pin, and their net.
    pub pads: Vec<String>,
    pub net: String,
    /// The capacitor's reference designator.
    pub cap: String,
    /// Whether its other pad is on the switch node: a bootstrap capacitor.
    pub boot: bool,
}

/// The switching regulators of `design`, in the order of its parts.
pub fn switching_regulators(design: &Design) -> Vec<SwitchingRegulator> {
    let net_of = |refdes: &str, pad: &str| -> Option<&str> {
        design
            .nets
            .iter()
            .find(|n| n.pins.iter().any(|p| p.refdes == refdes && p.pin == pad))
            .map(|n| n.name.as_str())
    };
    // The nets of each part's pads, by reference designator.
    let nets_of = |refdes: &str| -> Vec<&str> {
        design
            .nets
            .iter()
            .filter(|n| n.pins.iter().any(|p| p.refdes == refdes))
            .map(|n| n.name.as_str())
            .collect()
    };
    let capacitors: Vec<&str> = design
        .parts
        .iter()
        .filter(|p| p.declared_kind == Some(PartKind::Capacitor))
        .map(|p| p.refdes.as_str())
        .collect();
    let mut out = Vec::new();
    for part in design
        .parts
        .iter()
        .filter(|p| p.provides.iter().any(|g| g == BUCK_IC))
    {
        let role = |name: &str| -> Option<RegulatorPin> {
            let pin = part.pins.iter().find(|p| p.name == name)?;
            let net = pin.pads.iter().find_map(|pad| net_of(&part.refdes, pad))?;
            Some(RegulatorPin {
                net: net.to_string(),
                pads: pin.pads.clone(),
            })
        };
        let (Some(vin), Some(gnd), Some(sw)) = (role("vin.pos"), role("vin.neg"), role("sw.pos"))
        else {
            continue;
        };
        let fb = role("fb");
        // A pad of the IC on a role's net is that role's too (`VIN2`, `GND1`).
        let with_pads = |mut r: RegulatorPin| {
            for pin in &part.pins {
                for pad in &pin.pads {
                    if net_of(&part.refdes, pad) == Some(r.net.as_str()) && !r.pads.contains(pad) {
                        r.pads.push(pad.clone());
                    }
                }
            }
            r
        };
        let (vin, gnd, sw) = (with_pads(vin), with_pads(gnd), with_pads(sw));
        let fb = fb.map(with_pads);
        let own: Vec<&str> = [Some(&vin), Some(&gnd), Some(&sw), fb.as_ref()]
            .into_iter()
            .flatten()
            .map(|r| r.net.as_str())
            .collect();
        let input_caps: Vec<String> = capacitors
            .iter()
            .filter(|c| {
                let nets = nets_of(c);
                nets.contains(&vin.net.as_str()) && nets.contains(&gnd.net.as_str())
            })
            .map(|c| c.to_string())
            .collect();
        let mut support = Vec::new();
        for pin in &part.pins {
            let Some(net) = pin.pads.iter().find_map(|pad| net_of(&part.refdes, pad)) else {
                continue;
            };
            if own.contains(&net) || support.iter().any(|s: &SupportCap| s.net == net) {
                continue;
            }
            for c in capacitors.iter().filter(|c| nets_of(c).contains(&net)) {
                support.push(SupportCap {
                    pin: pin.datasheet.clone(),
                    pads: pin.pads.clone(),
                    net: net.to_string(),
                    cap: c.to_string(),
                    boot: nets_of(c).contains(&sw.net.as_str()),
                });
            }
        }
        let switch_parts = design
            .nets
            .iter()
            .filter(|n| n.name == sw.net)
            .flat_map(|n| n.pins.iter().map(|p| p.refdes.clone()))
            .filter(|r| *r != part.refdes)
            .fold(Vec::new(), |mut all: Vec<String>, r| {
                if !all.contains(&r) {
                    all.push(r);
                }
                all
            });
        out.push(SwitchingRegulator {
            refdes: part.refdes.clone(),
            path: part.path.clone(),
            vin,
            gnd,
            sw,
            fb,
            input_caps,
            support,
            switch_parts,
        });
    }
    out
}
