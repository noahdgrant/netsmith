//! How the schematic of a design is split into sheets (issue #563): which part is drawn on
//! which sheet, the file each sheet is, the names its nets have inside it, and the path KiCad
//! gives each symbol. It reads the design alone (no symbol library), so the board's writer
//! links each footprint to its symbol with the same answer the schematic's writer draws.
//!
//! Each module instance of the board (`.stage: LdoStage`) is a sheet; a module inside a module
//! is drawn flat in the sheet of the board's module it is in. The board's own parts, the sheet
//! symbols and the power flags of the nets that reach them are the root sheet. Instances of one
//! module definition that are the same circuit (the same parts with the same values and pins,
//! joined the same way inside, with the same nets through the same members) share one file, as
//! KiCad's hierarchical sheets do: `LdoStage.kicad_sch`. One that differs (another value of a
//! resistor) has a file of its own, `LdoStage_2.kicad_sch`. A design whose parts are in no
//! module, or that lists no module (a netlist read from a file), is one sheet.
//!
//! Inside a sheet, a net that also reaches a part outside it is named after the first member of
//! the module it is on (`vin.pos`), which is the sheet pin's name and a hierarchical label's; a
//! net that reaches no member keeps its board name. A net only inside it keeps its name, with
//! the instance's path taken off an unnamed net's (`net-stage.regulator.adj` is
//! `net-regulator.adj`), so the sheet reads the same for every instance. A name that would meet
//! another (two nets of one name in a sheet, or names anywhere that differ only in case, which
//! KiCad's electrical rules check reports: `gnd` and `GND`) gives way to the board's name.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use netsmith_core::Design;
use uuid::Uuid;

/// Fixed namespace of the schematic's UUIDs (as in `schematic`).
const NAMESPACE: Uuid = Uuid::from_u128(0x6e65_7473_6d69_7468_0000_0000_0000_0001);

pub(crate) fn uuid_for(name: &str) -> String {
    Uuid::new_v5(&NAMESPACE, name.as_bytes()).to_string()
}

/// The kind of the parts that are silkscreen text, which have no symbol.
const LABEL_KIND: &str = "Label";

/// The sheets of a design's schematic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hierarchy {
    /// The root sheet's file: `<design>.kicad_sch`.
    pub root_file: String,
    /// The UUID of the root sheet.
    pub root_uuid: String,
    /// For each part of the design, the index in `instances` of the sheet it is drawn on, or
    /// `None` for the root sheet. A label (silkscreen text) is `None` and is not drawn.
    pub part_sheet: Vec<Option<usize>>,
    /// The module instances drawn as sheets, in the order the design declares them.
    pub instances: Vec<SheetInstance>,
    /// The files of the sheets, in the order of their first instance.
    pub files: Vec<SheetFile>,
}

/// A module instance drawn as a sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetInstance {
    /// The module instance's path, which is the sheet's name: `stage`.
    pub path: String,
    /// Its module definition: `LdoStage`.
    pub type_name: String,
    /// The index in [`Hierarchy::files`] of the file it is drawn from.
    pub file: usize,
    /// The UUID of its sheet symbol on the root sheet.
    pub uuid: String,
    /// Its page number: the root is 1.
    pub page: usize,
    /// Its parts, indices into the design's parts, in the design's order.
    pub parts: Vec<usize>,
    /// The nets that join a part of it and a part outside it, as `(net, name inside)`, in the
    /// order of the sheet pins: the order the module declares the members they are on.
    pub pins: Vec<(String, String)>,
    /// The name inside the sheet of every net on one of its parts.
    pub names: BTreeMap<String, String>,
}

/// One `.kicad_sch` of a module definition, drawn by one or more instances.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetFile {
    /// The file's name: `LdoStage.kicad_sch`.
    pub name: String,
    /// The indices in [`Hierarchy::instances`] of the instances drawn from it.
    pub instances: Vec<usize>,
}

/// Where a symbol is in the schematic, as a footprint of the board names it: KiCad's
/// `(path ...)`, `(sheetname ...)` and `(sheetfile ...)`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SymbolLink {
    /// `/<symbol uuid>` on the root sheet, `/<sheet uuid>/<symbol uuid>` on a module's sheet.
    pub path: String,
    /// `/` or `/stage/`.
    pub sheet_name: String,
    /// The file the symbol is in.
    pub sheet_file: String,
}

impl Hierarchy {
    /// The sheets of `design`.
    pub fn of(design: &Design) -> Self {
        let root_file = format!("{}.kicad_sch", design.name);
        let root_uuid = uuid_for(&design.name);
        let mut part_sheet: Vec<Option<usize>> = vec![None; design.parts.len()];
        let mut instances: Vec<SheetInstance> = Vec::new();
        // The board's own modules: a module inside a module is drawn in its sheet.
        for module in design.modules.iter().filter(|m| !m.path.contains('.')) {
            let prefix = format!("{}.", module.path);
            let parts: Vec<usize> = design
                .parts
                .iter()
                .enumerate()
                .filter(|(_, p)| p.kind != LABEL_KIND && p.path.starts_with(&prefix))
                .map(|(i, _)| i)
                .collect();
            if parts.is_empty() {
                continue;
            }
            for &p in &parts {
                part_sheet[p] = Some(instances.len());
            }
            instances.push(SheetInstance {
                path: module.path.clone(),
                type_name: module.type_name.clone(),
                file: 0,
                uuid: uuid_for(&format!("{}/sheet/{}", design.name, module.path)),
                page: instances.len() + 2,
                parts,
                pins: Vec::new(),
                names: BTreeMap::new(),
            });
        }

        // The names of the nets inside each sheet.
        let refdes_sheet: BTreeMap<&str, Option<usize>> = design
            .parts
            .iter()
            .zip(&part_sheet)
            .filter(|(p, _)| p.kind != LABEL_KIND)
            .map(|(p, s)| (p.refdes.as_str(), *s))
            .collect();
        // The board's names of the nets the root sheet labels: those on a part of the board or
        // on parts of more than one sheet.
        let mut root_names: HashSet<&str> = HashSet::new();
        let mut crossing: Vec<HashSet<&str>> = vec![HashSet::new(); instances.len()];
        for net in &design.nets {
            let sheets: BTreeSet<Option<usize>> = net
                .pins
                .iter()
                .filter_map(|p| refdes_sheet.get(p.refdes.as_str()).copied())
                .collect();
            if sheets.len() > 1 || sheets.contains(&None) {
                root_names.insert(&net.name);
            }
            if sheets.len() > 1 {
                for s in sheets.into_iter().flatten() {
                    crossing[s].insert(&net.name);
                }
            }
        }
        for (index, instance) in instances.iter_mut().enumerate() {
            let module = design
                .modules
                .iter()
                .find(|m| m.path == instance.path)
                .expect("the instance is one of the design's modules");
            let unnamed = format!("net-{}.", instance.path);
            for net in &design.nets {
                let on_it = net
                    .pins
                    .iter()
                    .any(|p| refdes_sheet.get(p.refdes.as_str()) == Some(&Some(index)));
                if !on_it {
                    continue;
                }
                let name = if crossing[index].contains(net.name.as_str()) {
                    module
                        .ports
                        .iter()
                        .find(|port| port.net == net.name)
                        .map_or_else(|| net.name.clone(), |port| port.name.clone())
                } else {
                    match net.name.strip_prefix(&unnamed) {
                        Some(rest) => format!("net-{rest}"),
                        None => net.name.clone(),
                    }
                };
                instance.names.insert(net.name.clone(), name);
            }
        }
        // A name inside a sheet gives way to the board's name of its net where it would meet
        // another: two nets of one name inside a sheet are one net in KiCad (the sheet then keeps
        // the board's names, which are distinct), and KiCad's electrical rules check reports two
        // labels anywhere in the schematic whose names differ only in case (`gnd` in a sheet and
        // the board's `GND`). The board's names are as the design has them.
        loop {
            let mut changed = false;
            for instance in instances.iter_mut() {
                let distinct: HashSet<&String> = instance.names.values().collect();
                if distinct.len() != instance.names.len() {
                    for (net, name) in instance.names.iter_mut() {
                        if name != net {
                            name.clone_from(net);
                            changed = true;
                        }
                    }
                }
            }
            let mut spellings: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
            for name in root_names.iter().copied().chain(
                instances
                    .iter()
                    .flat_map(|i| i.names.values().map(String::as_str)),
            ) {
                spellings
                    .entry(name.to_lowercase())
                    .or_default()
                    .insert(name);
            }
            let clashing: HashSet<String> = spellings
                .values()
                .filter(|s| s.len() > 1)
                .flat_map(|s| s.iter().map(|n| n.to_string()))
                .collect();
            for instance in instances.iter_mut() {
                for (net, name) in instance.names.iter_mut() {
                    if name != net && clashing.contains(name.as_str()) {
                        name.clone_from(net);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        // The pins in the order the module declares its members; a net on none after them.
        for (index, instance) in instances.iter_mut().enumerate() {
            let module = design
                .modules
                .iter()
                .find(|m| m.path == instance.path)
                .expect("the instance is one of the design's modules");
            let rank = |net: &str| {
                module
                    .ports
                    .iter()
                    .position(|port| port.net == net)
                    .unwrap_or(module.ports.len())
            };
            let mut pins: Vec<(String, String)> = design
                .nets
                .iter()
                .filter(|n| crossing[index].contains(n.name.as_str()))
                .map(|n| (n.name.clone(), instance.names[&n.name].clone()))
                .collect();
            pins.sort_by_key(|(net, _)| rank(net));
            instance.pins = pins;
        }

        // Instances of one definition that are the same circuit share a file.
        let mut files: Vec<SheetFile> = Vec::new();
        let mut signatures: Vec<(String, String)> = Vec::new();
        let mut taken: HashSet<String> = HashSet::from([root_file.to_lowercase()]);
        let mut per_type: BTreeMap<String, usize> = BTreeMap::new();
        for (index, instance) in instances.iter_mut().enumerate() {
            let key = (instance.type_name.clone(), signature(design, instance));
            let file = match signatures.iter().position(|s| *s == key) {
                Some(file) => file,
                None => {
                    let count = per_type.entry(key.0.clone()).or_insert(0);
                    let name = loop {
                        *count += 1;
                        let name = match *count {
                            1 => format!("{}.kicad_sch", key.0),
                            n => format!("{}_{n}.kicad_sch", key.0),
                        };
                        // Windows folds case, so two names that differ by it are one file.
                        if taken.insert(name.to_lowercase()) {
                            break name;
                        }
                    };
                    signatures.push(key);
                    files.push(SheetFile {
                        name,
                        instances: Vec::new(),
                    });
                    files.len() - 1
                }
            };
            files[file].instances.push(index);
            instance.file = file;
        }

        Self {
            root_file,
            root_uuid,
            part_sheet,
            instances,
            files,
        }
    }

    /// Whether the schematic is one sheet.
    pub fn is_flat(&self) -> bool {
        self.instances.is_empty()
    }

    /// A part's path inside the sheet it is drawn on: `regulator` for `stage.regulator`, the
    /// whole path on the root sheet.
    pub fn local_path<'a>(&self, design: &'a Design, part: usize) -> &'a str {
        let path = &design.parts[part].path;
        match self.part_sheet[part] {
            Some(s) => &path[self.instances[s].path.len() + 1..],
            None => path,
        }
    }

    /// The text every UUID of a part's symbol is made from: the design and the reference on
    /// the root sheet (as a one-sheet schematic has always had), the design, the file and the
    /// path inside it on a module's sheet, so that every instance of a file has the same.
    pub(crate) fn symbol_key(&self, design: &Design, part: usize) -> String {
        match self.part_sheet[part] {
            None => format!("{}/{}", design.name, design.parts[part].refdes),
            Some(s) => format!(
                "{}/{}/{}",
                design.name,
                self.files[self.instances[s].file].name,
                self.local_path(design, part)
            ),
        }
    }

    /// Where the first unit of each part's symbol is, for the board's footprints: `None` for a
    /// label, which has no symbol.
    pub fn links(&self, design: &Design) -> Vec<Option<SymbolLink>> {
        (0..design.parts.len())
            .map(|part| {
                if design.parts[part].kind == LABEL_KIND {
                    return None;
                }
                let symbol = uuid_for(&format!("{}/symbol", self.symbol_key(design, part)));
                Some(match self.part_sheet[part] {
                    None => SymbolLink {
                        path: format!("/{symbol}"),
                        sheet_name: "/".to_string(),
                        sheet_file: self.root_file.clone(),
                    },
                    Some(s) => {
                        let instance = &self.instances[s];
                        SymbolLink {
                            path: format!("/{}/{symbol}", instance.uuid),
                            sheet_name: format!("/{}/", instance.path),
                            sheet_file: self.files[instance.file].name.clone(),
                        }
                    }
                })
            })
            .collect()
    }
}

/// What an instance's sheet is drawn from, so that two instances with the same are one file:
/// each part by its path inside, what it is and the letters of its reference, and each net by
/// its name inside, whether it leaves the sheet, and the pads on it.
fn signature(design: &Design, instance: &SheetInstance) -> String {
    let local = |part: usize| &design.parts[part].path[instance.path.len() + 1..];
    let mut text = String::new();
    for &p in &instance.parts {
        let part = &design.parts[p];
        let letters = part.refdes.trim_end_matches(|c: char| c.is_ascii_digit());
        text.push_str(&format!(
            "part {} {letters} {:?} {:?} {:?} {:?} {:?}\n",
            local(p),
            part.kind,
            part.value,
            part.package,
            part.provides,
            part.pins
        ));
    }
    let pin_names: HashSet<&str> = instance.pins.iter().map(|(n, _)| n.as_str()).collect();
    let mut nets: Vec<String> = design
        .nets
        .iter()
        .filter_map(|net| {
            let name = instance.names.get(&net.name)?;
            let mut pads: Vec<String> = net
                .pins
                .iter()
                .filter_map(|pin| {
                    let p = instance
                        .parts
                        .iter()
                        .copied()
                        .find(|&p| design.parts[p].refdes == pin.refdes)?;
                    Some(format!("{}.{}", local(p), pin.pin))
                })
                .collect();
            pads.sort();
            Some(format!(
                "net {name} {} {}\n",
                pin_names.contains(net.name.as_str()),
                pads.join(" ")
            ))
        })
        .collect();
    nets.sort();
    for net in nets {
        text.push_str(&net);
    }
    text
}
