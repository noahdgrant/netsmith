//! KiCad's design rule check of a board netsmith wrote, for the tests that lay a board out and
//! then ask KiCad (`kicad-cli`, found as `netsmith_fab::KicadCliExe::locate_runnable` does) what it
//! finds: skipped where it is not installed. Shared by this crate's `kicad_drc` and by the
//! netsmith crate's (`#[path]` module there), whose boards come from source text.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use netsmith_fab::KicadCliExe;
use netsmith_pcb::{Board, kicad_pcb, kicad_pro};
use netsmith_testkit::TempDir;

/// What a DRC run found: the type of each violation, and how many connections are missing.
pub struct Report {
    pub violations: Vec<String>,
    pub unconnected: usize,
}

impl Report {
    pub fn count(&self, kind: &str) -> usize {
        self.violations.iter().filter(|v| *v == kind).count()
    }
}

/// Run KiCad's DRC on a board and its project file (KiCad fills the zones first), and
/// netsmith's own on the same files, and require the two to agree on everything they both
/// check (ADR 0037). `None` if KiCad is not here.
pub fn drc(name: &str, pcb: &str, pro: &str) -> Option<Report> {
    use netsmith_fab::{KicadCli, drc::kicad};
    let cli = KicadCliExe::locate_runnable()?;
    // Tests run side by side, so each run gets a folder of its own.
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    let serial = RUNS.fetch_add(1, Ordering::SeqCst);
    let dir = TempDir::new(&format!("pcb-test-{serial}"));
    let path = dir.join(format!("{name}.kicad_pcb"));
    std::fs::write(&path, pcb).ok()?;
    std::fs::write(path.with_extension("kicad_pro"), pro).ok()?;
    let json = cli
        .drc_json(&path)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    let theirs = kicad::parse(&json).unwrap();

    let ours = netsmith_fab::drc::check(pcb, Some(pro), &Default::default())
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    let comparison = kicad::compare(&ours, &theirs, 0.1);
    let mut skipped: Vec<&str> = theirs
        .iter()
        .map(|f| f.kind.as_str())
        .filter(|k| kicad::OUT_OF_SCOPE.contains(k))
        .collect();
    skipped.sort_unstable();
    skipped.dedup();
    eprintln!(
        "drc cross-check {name}: {} agree, {} only netsmith's rules, {} of kinds not checked {skipped:?}",
        comparison.agreed, comparison.netsmith_only_rules, comparison.out_of_scope
    );
    assert!(
        comparison.agrees(),
        "{name}: netsmith's design rules check and KiCad's differ:\n{comparison}netsmith's:\n{ours}"
    );

    let mut report = Report {
        violations: Vec::new(),
        unconnected: 0,
    };
    for f in &theirs {
        match f.kind.as_str() {
            "unconnected_items" => report.unconnected += 1,
            // These tests lay boards out with our own small footprints, which are not the
            // ones in KiCad's library, so KiCad says they differ from it. That comparison is
            // `fab`'s, which uses the installed footprints.
            k if kicad::OUT_OF_SCOPE.contains(&k) => {}
            k => report.violations.push(k.to_string()),
        }
    }
    Some(report)
}

/// With `NETSMITH_KEEP_BOARDS` set to a folder, the board checked is kept there, to look at
/// the findings with `crates/netsmith-pcb/scripts/check-kicad.sh`.
pub fn keep(name: &str, b: &Board) {
    if let Some(dir) = std::env::var_os("NETSMITH_KEEP_BOARDS") {
        let dir = PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(format!("{name}.kicad_pcb")), kicad_pcb(b));
        let _ = std::fs::write(dir.join(format!("{name}.kicad_pro")), kicad_pro(b));
    }
}

pub fn check(name: &str, b: &Board) -> Option<Report> {
    keep(name, b);
    drc(name, &kicad_pcb(b), &kicad_pro(b))
}

/// KiCad's check of a four-layer board, with its planes filled by KiCad (a board netsmith
/// writes holds no fill, and KiCad's check needs one), and netsmith's own compared with it.
/// `None` if KiCad is not here.
pub fn plane_drc(name: &str, b: &Board) -> Option<netsmith_fab::drc::Counts> {
    keep(name, b);
    let r = drc(name, &kicad_pcb(b), &kicad_pro(b))?;
    Some(netsmith_fab::drc::Counts {
        violations: r.violations.len() as u32,
        unconnected: r.unconnected as u32,
        footprint_errors: 0,
    })
}

/// What KiCad found about copper and holes: the silkscreen of our invented footprints is
/// not what is tested, and KiCad has no library table for them.
pub fn copper_only(report: &Report) -> Vec<&String> {
    report
        .violations
        .iter()
        .filter(|v| !v.starts_with("silk") && !v.starts_with("lib_"))
        .collect()
}
