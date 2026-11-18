//! `netsmith board nets` (issue #1223): a table of a routed board's nets, each net's routed and
//! straight length, its vias, how often it changes layer and its layers, and each diff pair's
//! lengths, skew, coupled length, gaps and vias side by side. The board is placed and has the
//! copper `view pcb` draws on it (`netsmith_fabricate::pcb_picture::put_copper`), routed first
//! with `--route`; the table is `netsmith_fab::NetTable`'s, which the editor's PCB summary tab
//! shows too.
//!
//! A sibling of `board fill` and not a flag of `board drc`: the check judges the board and fails
//! it, and the table judges nothing; and of `layout summary`, which reads the placement with no
//! copper. It reads the copper as `view pcb` draws it and takes its `--route`, as `board fill`
//! reads the pours.

use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_fabricate::pcb_picture;

use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`board nets` measures a board's routed copper and prints a table of it, so that a person or an agent reads each line's length, vias and skew with no KiCad and no picture. The board is placed (the layout file read, the placer run for the rest) and has the copper `view pcb` draws on it: the routes file beside the source, the router's copper the editor keeps (`.netsmith/routes/`) for every net whose pads have not moved since it was routed, and the teardrops. With `--route` the router runs first, from that copper, as `board fab --reuse-routes` does (`--route-timeout` as for `view pcb`). Nothing is written.\n\nFor each net that no zone pours and that has a track or a via (a poured net's tracks are no connection) it prints the routed length (all its tracks, an arc by its chords), the straight length (the shortest tree of straight lines between the centres of its pads), their ratio, its vias, its layer changes (the vias where its tracks meet on two layers or more) and the layers its tracks are on; the nets by name, or longest first with `--lengths`. Then for each diff pair of the board (two nets whose names differ by a suffix pair such as `dp`/`dm`, the nets of a `PairImpedance` or a `DiffPair`, the lines of a `Can`): both lengths, the skew and whether it is within the router's target (0.1 mm), the length of the first net that runs coupled (the other's track on its layer within 0.5 mm, centre to centre, sampled every 0.05 mm) and its share, the least and the largest gap edge to edge over that length (equal for a pair at one pitch), both nets' vias and how many stand side by side (copper no more than 0.5 mm apart). These are the numbers the router matches a pair by, and `board drc` notes a pair whose skew is over the target. Lengths are in mm.\n\nWith `--format json` one JSON document is printed on stdout: `board`, `nets` (each with `net`, `length`, `straight`, `ratio`, `vias`, `layer_changes`, `layers`) and `pairs` (each with `p`, `n`, `length_p`, `length_n`, `skew`, `limit`, `over`, `coupled`, `share`, `gaps` (the least and the largest, or null), `vias_p`, `vias_n`, `via_pairs`). The exit code is 0 when it is printed (a pair over its skew too: `board drc` is the check), 1 when the board does not build, place or route, and 2 when the command cannot run."
)]
pub struct Args {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The board, by name, when the source has more than one.
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// List the nets longest first instead of by name.
    #[arg(long)]
    pub lengths: bool,
    /// Route the board first, from the routes file's and the kept copper, as `board fab
    /// --reuse-routes` does.
    #[arg(long)]
    pub route: bool,
    /// Seconds the router may take with `--route`; 0 is no limit (default 300).
    #[arg(long, value_name = "SECS", requires = "route")]
    pub route_timeout: Option<u64>,
    #[command(flatten)]
    pub placing: common::Placing,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

pub fn run(args: Args) -> ExitCode {
    let route = super::pcb::route_of(args.route, args.route_timeout);
    let made = common::with_stage_events(|| {
        common::with_placed_board(
            args.file.as_deref(),
            args.board.as_deref(),
            Some(&args.placing),
            |placed| {
                placed.with_board(|built, board| {
                    let label = built.label();
                    let mut board = board.clone();
                    let say = |line: String| eprintln!("{label}: {line}");
                    pcb_picture::put_copper(&mut board, &placed.layout, route.as_ref(), &say)
                        .map_err(|e| format!("error: {label}: {e}\n"))?;
                    let mut table = netsmith_fabricate::net_table::of_board(&board)
                        .map_err(|e| format!("error: {label}: {e}\n"))?;
                    if args.lengths {
                        table.by_length();
                    }
                    table.board = label;
                    Ok(table)
                })?
            },
        )
    });
    match made {
        Ok(table) => {
            match args.format {
                common::Format::Json => {
                    println!("{}", serde_json::to_string(&table).expect("plain data"))
                }
                common::Format::Text => {
                    println!("{}:", table.board);
                    print!("{}", table.text());
                }
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::common;
    use crate::commands::common::testing::{Leaf, parse};

    #[test]
    fn a_file_a_board_the_order_routing_the_placer_and_a_format() {
        let Ok(Leaf::Nets(a)) = parse(&[
            "board",
            "nets",
            "a.nsm",
            "--board",
            "B",
            "--lengths",
            "--route",
            "--route-timeout",
            "30",
            "--place-seeds",
            "2",
            "--format",
            "json",
        ]) else {
            panic!("nets should parse");
        };
        assert_eq!(a.file.as_deref(), Some("a.nsm"));
        assert_eq!(a.board.as_deref(), Some("B"));
        assert!(a.lengths && a.route);
        assert_eq!(a.route_timeout, Some(30));
        assert_eq!(a.placing.place_seeds, Some(2));
        assert_eq!(a.format, common::Format::Json);
        let Ok(Leaf::Nets(a)) = parse(&["board", "nets"]) else {
            panic!("nets should parse");
        };
        assert_eq!(
            (a.file, a.board, a.lengths, a.route, a.format),
            (None, None, false, false, common::Format::Text)
        );
        // A timeout is for the router.
        assert!(parse(&["board", "nets", "--route-timeout", "30"]).is_err());
    }
}
