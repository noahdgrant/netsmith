//! Build every board in a `.nsm` file and write its `.kicad_pcb`.
//!
//! `cargo run -p netsmith --example pcb -- examples/led.nsm --out out`

use std::{env, fs, path::PathBuf, process::ExitCode};

use netsmith_pcb::{FootprintLibrary, Ratsnest, kicad_pcb, kicad_pro, render_svg};
use netsmith_router::route;
use netsmith_solve::Catalogs;

const USAGE: &str = "usage: pcb <file.nsm> [--out <dir>] [--route]";

fn main() -> ExitCode {
    let mut file: Option<String> = None;
    let mut out = PathBuf::from("out");
    let mut routing = false;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--route" => routing = true,
            "--out" => match args.next() {
                Some(dir) => out = PathBuf::from(dir),
                None => {
                    eprintln!("{USAGE}");
                    return ExitCode::from(2);
                }
            },
            other if file.is_none() && !other.starts_with("--") => file = Some(other.to_string()),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(file) = file else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let text = match fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{file}: {e}");
            return ExitCode::from(2);
        }
    };
    let built = match netsmith::build(&file, &text, &Catalogs::sample()) {
        Ok(b) => b,
        Err(problems) => {
            eprint!("{problems}");
            return ExitCode::FAILURE;
        }
    };

    let library = FootprintLibrary::installed();
    let mut failed = false;
    for b in built {
        let mut board = match netsmith_placer::board_from_design(
            &b.design,
            &netsmith_kicad::Mapping::standard(),
            &library,
        ) {
            Ok(board) => board,
            Err(e) => {
                eprintln!("error: {}: {e}", b.label());
                return ExitCode::FAILURE;
            }
        };
        let problems = board.check_placement();
        for p in &problems {
            eprintln!("error: {}: {p}", b.label());
        }
        if !problems.is_empty() {
            return ExitCode::FAILURE;
        }
        let ratsnest = if routing {
            route(&mut board)
        } else {
            Ratsnest::of(&board)
        };

        let dir = b.dir(&out);
        let name = &b.design.name;
        let result = fs::create_dir_all(&dir)
            .and_then(|()| fs::write(dir.join(format!("{name}.kicad_pcb")), kicad_pcb(&board)))
            .and_then(|()| fs::write(dir.join(format!("{name}.kicad_pro")), kicad_pro(&board)))
            .and_then(|()| fs::write(dir.join(format!("{name}.svg")), render_svg(&board)));
        if let Err(e) = result {
            eprintln!("{}: {e}", dir.display());
            return ExitCode::from(2);
        }

        println!("board {}", board.name);
        println!("  parts:    {}", board.parts.len());
        println!(
            "  outline:  {} x {} mm",
            board.outline.width(),
            board.outline.height()
        );
        if routing {
            println!(
                "  routed:   {} tracks, {} vias",
                board.tracks.len(),
                board.vias.len()
            );
        }
        println!(
            "  {}: {} lines, {:.2} mm",
            if routing { "unrouted" } else { "ratsnest" },
            ratsnest.len(),
            ratsnest.total_length()
        );
        println!("wrote {}", dir.display());
        if routing && !ratsnest.is_empty() {
            eprintln!(
                "ERROR: {}: {} of the connections could not be routed:",
                board.name,
                ratsnest.len()
            );
            for line in &ratsnest.lines {
                eprintln!("  net {}: {} to {}", line.net, line.from, line.to);
            }
            failed = true;
        }
    }
    if failed {
        eprintln!("ERROR: the boards above are only partly routed");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
