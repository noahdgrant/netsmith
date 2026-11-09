//! Place and route the boards of a source file or a project and print how good the
//! placement is: wire length, ratsnest crossings, decoupling distances, and what the router
//! made of it. Writes each board's `.kicad_pcb` and `.kicad_pro` to `--out` for KiCad's check.
//!
//! `cargo run --release -p netsmith --example place_report -- [file.nsm] [--out dir] [--seed n]`
//! (run it from the project's directory for a project; ADR 0033 uses it for its numbers).

use std::{env, fs, path::PathBuf, process::ExitCode, time::Instant};

use netsmith::project::Target;
use netsmith::{Libraries, build_with};
use netsmith_pcb::{Board, Quality, RouteOptions, apply_layout, kicad_pcb, kicad_pro, route_with};

fn main() -> ExitCode {
    let mut file: Option<String> = None;
    let mut out = PathBuf::from("/tmp/place-report");
    let mut seed = 0u64;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = PathBuf::from(args.next().unwrap_or_default()),
            "--seed" => seed = args.next().and_then(|s| s.parse().ok()).unwrap_or(0),
            other => file = Some(other.to_string()),
        }
    }
    let fail = |e: String| {
        eprintln!("{e}");
        ExitCode::from(2)
    };
    let target = match Target::resolve(file.as_deref()) {
        Ok(t) => t,
        Err(e) => return fail(e),
    };
    let (loader, catalogs) = match (target.loader(), target.catalogs()) {
        (Ok(l), Ok(c)) => (l, c),
        (Err(e), _) | (_, Err(e)) => return fail(e),
    };
    let root = target.project.as_ref().map(|p| p.root.as_path());
    let libraries = match Libraries::for_project(root) {
        Ok(l) => l,
        Err(e) => return fail(e.to_string()),
    };
    for source in &target.files {
        let text = fs::read_to_string(&source.path).expect("the source is readable");
        let boards = match build_with(&source.name, &text, &loader, &catalogs, &libraries) {
            Ok(b) => b,
            Err(e) => return fail(e),
        };
        let layout = netsmith::layout::read(
            &netsmith::layout::layout_path(&source.path),
            &boards
                .iter()
                .map(|b| b.design.name.clone())
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        for built in &boards {
            let started = Instant::now();
            let mut options =
                netsmith_pcb::place_options(&built.design, layout.as_ref().map(|l| &l.0));
            if seed != 0 {
                options.seed = seed;
            }
            let mut board = Board::from_design_placed(
                &built.design,
                &libraries.mapping,
                &libraries.footprints,
                &options,
            )
            .unwrap_or_else(|e| panic!("{e}"));
            if let Some((layout, name)) = &layout {
                apply_layout(&mut board, layout, name).unwrap_or_else(|e| panic!("{e}"));
            }
            let placed = started.elapsed();
            let quality = Quality::of(&board);
            let problems = board.check_placement();
            println!(
                "{}: {} parts placed in {placed:?} on {} x {} mm ({} placement problems)",
                built.design.name,
                board.parts.len(),
                board.outline.width(),
                board.outline.height(),
                problems.len()
            );
            for p in &problems {
                println!("  problem: {p}");
            }
            println!(
                "  hpwl {:.1} mm, ratsnest {:.1} mm, crossings {}, decoupling caps {} (mean {:.2} mm, max {:.2} mm)",
                quality.hpwl,
                quality.ratsnest,
                quality.crossings,
                quality.decoupling.len(),
                quality.decoupling_mean(),
                quality.decoupling_max()
            );
            let routed = Instant::now();
            let routing = route_with(&mut board, RouteOptions::default());
            println!(
                "  routed in {:?}: {} connections left, {} tries, {} tracks, {} vias",
                routed.elapsed(),
                routing.left.len(),
                routing.tries,
                board.tracks.len(),
                board.vias.len()
            );
            let dir = out.join(&built.design.name);
            fs::create_dir_all(&dir).expect("the output folder can be made");
            let name = &built.design.name;
            fs::write(dir.join(format!("{name}.kicad_pcb")), kicad_pcb(&board)).unwrap();
            fs::write(dir.join(format!("{name}.kicad_pro")), kicad_pro(&board)).unwrap();
            libraries.write_tables(&dir, &built.design).unwrap();
        }
    }
    ExitCode::SUCCESS
}
