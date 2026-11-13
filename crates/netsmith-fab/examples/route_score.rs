//! Prints the route scorecard ([`netsmith_fab::Scorecard`]) of each board file given, with the
//! `.kicad_pro` beside it if there is one; `--json` prints them as one JSON object keyed by
//! the file name instead.
//!
//! `cargo run --release -p netsmith-fab --example route_score -- [--json] a.kicad_pcb b.kicad_pcb ...`

use netsmith_fab::Scorecard;
use netsmith_geom::drc_project_of;

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if files.is_empty() {
        eprintln!("usage: route_score [--json] <board.kicad_pcb>...");
        std::process::exit(2);
    }
    let mut all = serde_json::Map::new();
    for file in files {
        let path = Path::new(file);
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{file}: {e}"));
        let card = Scorecard::of(&text, drc_project_of(path).as_deref())
            .unwrap_or_else(|e| panic!("{file}: {e}"));
        if json {
            all.insert(
                file.clone(),
                serde_json::to_value(&card).expect("a scorecard is JSON"),
            );
        } else {
            println!("== {file}");
            print!("{}", card.text());
        }
    }
    if json {
        println!("{}", serde_json::Value::Object(all));
    }
}
