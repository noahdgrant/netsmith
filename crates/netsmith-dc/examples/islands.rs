//! Run islands (the text format of `src/text.rs`) and print what the DC analysis finds.
//!
//! `cargo run --release -p netsmith-dc --example islands -- [file.cir ...]`
//!
//! With no argument it runs every file of `tests/islands/`. A file's `# report:` line names the
//! quantities to print, its `# today:` line says what netsmith reports for the same part now.
//! For each island it prints the combinations of corners and states solved, the most iterations
//! of a regulator's fixed point, the time of one analysis (the mean of a few runs), and the
//! number of random points inside the ranges that fell outside the corner ranges.

use std::{env, fs, path::PathBuf, time::Instant};

use netsmith_dc::{analyze, check_interior, text::parse};

fn main() {
    let mut files: Vec<PathBuf> = env::args().skip(1).map(PathBuf::from).collect();
    if files.is_empty() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/islands");
        files = fs::read_dir(&dir)
            .expect("tests/islands")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "cir"))
            .collect();
        files.sort();
    }
    for path in files {
        let text = fs::read_to_string(&path).expect("read");
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let header = |key: &str| {
            text.lines()
                .find_map(|l| l.strip_prefix(&format!("# {key}:")))
                .map(str::trim)
                .unwrap_or("")
                .to_string()
        };
        let nl = match parse(&text) {
            Ok(nl) => nl,
            Err(e) => {
                println!("== {name}: {e}");
                continue;
            }
        };
        let runs = 5;
        let start = Instant::now();
        let mut report = None;
        for _ in 0..runs {
            report = Some(analyze(&nl));
        }
        let elapsed = start.elapsed() / runs;
        let report = match report.unwrap() {
            Ok(r) => r,
            Err(e) => {
                println!("== {name}: {e}");
                continue;
            }
        };
        let (outside, solved) = check_interior(&nl, &report, 300);
        println!("== {name}");
        println!(
            "   {} elements, {} nodes; {} combinations ({} consistent, {} refused, {} collapsed), fixed point <= {} iterations; {:.2?} per analysis; {} of {solved} interior points outside{}",
            nl.elements.len(),
            nl.node_count() - 1,
            report.combinations,
            report.solved,
            report.inconsistent,
            report.collapsed.len(),
            report.max_iterations,
            elapsed,
            outside.iter().map(|(_, n)| n).sum::<usize>(),
            if outside.is_empty() {
                String::new()
            } else {
                format!(
                    " ({})",
                    outside
                        .iter()
                        .map(|(l, n)| format!("{l}: {n}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        );
        for label in header("report").split_whitespace() {
            match report.get(label) {
                Some(r) => println!(
                    "   {label:<16} {r}   (high end: {})",
                    report.produced_by(label, true).unwrap_or_default()
                ),
                None => println!("   {label:<16} (no such quantity)"),
            }
        }
        if !report.below_min.is_empty() {
            println!(
                "   {} regulator input(s) below the minimum, first in: {}",
                report.below_min.len(),
                report.describe(report.below_min[0].1)
            );
        }
        println!("   today: {}", header("today"));
    }
}
