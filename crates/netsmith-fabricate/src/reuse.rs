//! Reusing the package a plain `fab` just made (issue #841).
//!
//! An order makes each board it names again: placed, routed, checked, written. When a plain `netsmith
//! fab` has made that board from the same inputs and nobody has touched its package since, the order
//! copies the package instead and runs none of those stages. The order shows it by a count: each
//! board's `Fabricated::stages_run`, which is 0 for a package that was reused, and a line of the
//! progress for each board that says reused or made again, and why.
//!
//! # How a package is known to be fresh
//!
//! A plain `fab` (`Reuse::Record`) writes `.fab-record.json` into the folder of each board it
//! makes, holding
//!
//! - the **key**, a SHA-256 over everything the package is made from ([`key`]): the netsmith
//!   executable itself (so a new build, with any change of the checker, solver, placer, router or
//!   the data embedded in it, never reuses what an older one made), the design the board was built
//!   to, the parts as sourced and as resolved against the catalogs, the schematic text, the
//!   footprints the parts load with, the layout file and the routes file as they are on disk,
//!   the STEP and case models the design names, the pin map for firmware, and the options of the
//!   run that change what is written (the router's time limit, the track margin, the placer's
//!   seeds and budget, the glb, STEP and PDF switches, `--strict`-ness of heights, `--allow-drc-violations`,
//!   `--cross-check`, `--offline`, `--save-placed`);
//! - the SHA-256 of every file of the package folder; and
//! - what the order reads of the package: its summary, notes, the files of the Gerber zip and the
//!   facts of the board (`BoardFacts`).
//!
//! An order ([`Reuse::From`](crate::fab::Reuse)) computes the key of each board from its own
//! inputs and reuses the package only when the record's key is the same **and** the folder holds
//! exactly the recorded files, byte for byte. A changed source, catalog, layout or option, a file
//! edited or removed or added by hand, a rebuilt netsmith, a record of another format, or no
//! record at all means the board is made again, and the progress says which. A stale package is
//! never used silently: when the key cannot be made (the executable cannot be read, a part
//! does not load), nothing is reused.
//!
//! What is not reused: a board of a system (its scene is made of its members as routed, which a
//! package does not keep) and a panel (it is made from its cell as routed) are always made again,
//! and a run whose pins were moved by `--pins auto` leaves no record (the package is of a pin file
//! that is no longer the one on disk).

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use netsmith_pcb::{Board, Layout, place_options};
use netsmith_pipeline::{Built, Libraries};
use netsmith_solve::Catalogs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::fab::{FabOptions, Fabricated};
use crate::order::BoardFacts;

/// The file of a package folder that says what the package was made from. It starts with a dot
/// so that it stays out of the listing a person reads, and it is not part of the package.
pub const RECORD: &str = ".fab-record.json";

/// The version of the record's layout: a record of another number is not read.
const FORMAT: u32 = 1;

/// What a plain `fab` leaves beside a package.
#[derive(Debug, Serialize, Deserialize)]
struct Record {
    format: u32,
    key: String,
    summary: String,
    notes: String,
    gerber_files: Vec<String>,
    facts: BoardFacts,
    /// Every file of the folder but the record: its path under the folder (with `/`) and the
    /// SHA-256 of its bytes.
    files: BTreeMap<String, String>,
}

/// What the key is made from, besides the executable.
pub struct Inputs<'a> {
    pub built: &'a Built,
    pub libraries: &'a Libraries,
    pub catalogs: &'a Catalogs,
    pub options: &'a FabOptions,
    /// The layout file as it was read (`None`: there is none).
    pub layout: Option<&'a Layout>,
    /// The pin map for firmware the board's package writes, as text.
    pub pin_map: String,
}

fn hex(bytes: &[u8]) -> String {
    crate::models::sha256(bytes)
}

/// The SHA-256 of the running executable, read once. `None` when it cannot be read (it was
/// replaced while the process runs, say): then no package is recorded or reused.
fn tool() -> Option<&'static str> {
    static TOOL: OnceLock<Option<String>> = OnceLock::new();
    TOOL.get_or_init(|| {
        let exe = std::env::current_exe().ok()?;
        let mut file = fs::File::open(exe).ok()?;
        let mut hash = Sha256::new();
        let mut chunk = vec![0u8; 1 << 20];
        loop {
            match file.read(&mut chunk).ok()? {
                0 => break,
                n => hash.update(&chunk[..n]),
            }
        }
        Some(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
    })
    .as_deref()
}

/// Adds one named part to the hash, so that two inputs cannot run into one another.
fn put(hash: &mut Sha256, name: &str, bytes: &[u8]) {
    hash.update((name.len() as u64).to_le_bytes());
    hash.update(name.as_bytes());
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

/// The bytes of a file the board reads beside the source, or a marker for one that is not there.
fn file_or_absent(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|_| b"(no such file)".to_vec())
}

/// The key of a board's package ([`Inputs`]); `None` when it cannot be made, and then nothing is
/// recorded or reused.
pub fn key(inputs: &Inputs) -> Option<String> {
    let Inputs {
        built,
        libraries,
        catalogs,
        options,
        layout,
        pin_map,
    } = inputs;
    let mut hash = Sha256::new();
    put(&mut hash, "tool", tool()?.as_bytes());
    put(
        &mut hash,
        "design",
        &serde_json::to_vec(&built.design).ok()?,
    );
    put(
        &mut hash,
        "sourcing",
        format!("{:?}", built.sourcing).as_bytes(),
    );
    let (resolved, _) = netsmith_pipeline::resolve::resolve_generics(&built.sourcing, catalogs);
    put(&mut hash, "resolved", format!("{resolved:?}").as_bytes());
    put(
        &mut hash,
        "catalog heights",
        format!("{:?}", built.catalog_heights).as_bytes(),
    );
    put(&mut hash, "schematic", built.schematic.as_bytes());
    put(
        &mut hash,
        "sheets",
        format!("{:?}", built.sheets).as_bytes(),
    );
    put(&mut hash, "project", built.project.as_bytes());
    // The footprints the parts load with, from the mapping and the libraries they are read from.
    let placement = place_options(&built.design, *layout);
    let loaded = Board::load(
        &built.design,
        &libraries.mapping,
        &libraries.footprints,
        &placement,
    )
    .ok()?;
    put(
        &mut hash,
        "footprints",
        &serde_json::to_vec(&loaded.parts).ok()?,
    );
    put(&mut hash, "pin map", pin_map.as_bytes());
    // The files beside the source, as they are now (a seed `fab` wrote is in the layout file).
    if let Some(file) = &options.layout {
        put(&mut hash, "layout", &file_or_absent(file));
        put(
            &mut hash,
            "routes",
            &file_or_absent(&crate::routes::beside_layout(file)),
        );
    }
    // The schematic layout file the schematic PDF is drawn with (issue #1091).
    if let Some(schematic) = options.schematic.as_ref().filter(|_| options.pdf) {
        put(
            &mut hash,
            "schematic layout",
            &file_or_absent(&schematic.layout),
        );
        put(
            &mut hash,
            "project",
            schematic.project.as_deref().unwrap_or("").as_bytes(),
        );
    }
    // The revision is the notes' first line, with or without the schematic PDF (issue #1134).
    put(
        &mut hash,
        "revision",
        options.revision.as_deref().unwrap_or("").as_bytes(),
    );
    // The STEP models and the case models, read from disk.
    for (name, file) in built
        .design
        .features
        .models
        .iter()
        .map(|m| (&m.path, &m.resolved))
        .chain(
            built
                .design
                .features
                .case_models
                .iter()
                .map(|m| (&m.name, &m.resolved)),
        )
    {
        if let Some(file) = file {
            put(&mut hash, name, &file_or_absent(file));
        }
    }
    // The options of the run that change what is written. `only`, `progress`, `lock`, `pins` and
    // `seed_writes` are left out: they do not change a package that is made (a lock and the pins
    // reach it through the design and the pin map, a seed through the layout file).
    put(
        &mut hash,
        "options",
        format!(
            "{:?}",
            (
                (
                    options.route_timeout,
                    options.allow_drc_violations,
                    options.cross_check,
                    options.strict_heights,
                    options.pin_map_source.as_deref(),
                    options.track_margin.to_bits(),
                    options.rip_rounds,
                    options.weights.map(|w| (
                        w.length.to_bits(),
                        w.via.to_bits(),
                        w.layer_change.to_bits()
                    )),
                ),
                (
                    options.glb,
                    options.save_placed,
                    options.step,
                    options.place_seeds,
                    options.place_budget,
                    options.offline,
                    catalogs.constraints_warn(),
                ),
                options.pdf,
            )
        )
        .as_bytes(),
    );
    Some(hex(&hash.finalize()))
}

/// Every file under `dir` but the record, by its path under `dir` with `/` and the SHA-256 of
/// its bytes.
fn files_of(dir: &Path) -> std::io::Result<BTreeMap<String, String>> {
    fn walk(root: &Path, dir: &Path, found: &mut BTreeMap<String, String>) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(root, &path, found)?;
            } else if path != root.join(RECORD) {
                let name = path
                    .strip_prefix(root)
                    .map_err(std::io::Error::other)?
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                found.insert(name, hex(&fs::read(&path)?));
            }
        }
        Ok(())
    }
    let mut found = BTreeMap::new();
    walk(dir, dir, &mut found)?;
    Ok(found)
}

/// Writes the record of a package a plain `fab` has just made into its folder (`made.dir`), under
/// `key`.
pub fn write_record(made: &Fabricated, key: &str) -> Result<(), String> {
    let record = Record {
        format: FORMAT,
        key: key.to_string(),
        summary: made.summary.clone(),
        notes: made.notes.clone(),
        gerber_files: made.gerber_files.clone(),
        facts: made.facts.clone(),
        files: files_of(&made.dir).map_err(|e| format!("{}: {e}", made.dir.display()))?,
    };
    let path = made.dir.join(RECORD);
    let text = serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?;
    fs::write(&path, text + "\n").map_err(|e| format!("{}: {e}", path.display()))
}

/// Copies the package of `label` from `from` (the folder a plain `fab` wrote it to) into `to`
/// when the record there is of `key` and the folder holds exactly the files it recorded. `Err`
/// says why not, for the progress line, and leaves `to` as it was.
pub fn take(from: &Path, to: &Path, key: &str, label: &str) -> Result<Fabricated, String> {
    let path = from.join(RECORD);
    let text = fs::read_to_string(&path)
        .map_err(|_| format!("no record of a plain fab at {}", path.display()))?;
    let record: Record = serde_json::from_str(&text)
        .map_err(|e| format!("{} cannot be read ({e})", path.display()))?;
    if record.format != FORMAT {
        return Err(format!("{} is of another format", path.display()));
    }
    if record.key != key {
        return Err(
            "the source, the data, the options or netsmith itself changed since that fab"
                .to_string(),
        );
    }
    let now = files_of(from).map_err(|e| format!("{}: {e}", from.display()))?;
    if now != record.files {
        let changed: Vec<&String> = record
            .files
            .keys()
            .chain(now.keys())
            .filter(|f| record.files.get(*f) != now.get(*f))
            .collect();
        let mut names: Vec<String> = changed.into_iter().cloned().collect();
        names.sort();
        names.dedup();
        return Err(format!(
            "the files of the package are not as fab wrote them ({})",
            names.join(", ")
        ));
    }
    let mut files: Vec<PathBuf> = Vec::new();
    for name in record.files.keys() {
        let target = to.join(name);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        fs::copy(from.join(name), &target).map_err(|e| format!("{}: {e}", target.display()))?;
        files.push(target);
    }
    Ok(Fabricated {
        board: label.to_string(),
        dir: to.to_path_buf(),
        gerber_files: record.gerber_files,
        summary: record.summary,
        notes: record.notes,
        files,
        facts: record.facts,
        origins: Vec::new(),
        requirements: Vec::new(),
        place_warnings: Vec::new(),
        routes: Default::default(),
        stages_run: 0,
        reused: true,
    })
}
