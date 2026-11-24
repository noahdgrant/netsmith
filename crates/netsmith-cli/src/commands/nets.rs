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
//!
//! Its `quality` section (issue #1481) counts, for each net, the defects of routed copper that
//! the design rules check finds nothing of (`netsmith_fabricate::net_quality`), the worst nets
//! first; `--gate` makes the exit code 1 when a count is over its limit (`--max-<kind>`, 0 each
//! by default).

use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_fabricate::net_quality::{Counts, Quality};
use netsmith_fabricate::pcb_picture;

use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`board nets` measures a board's routed copper and prints a table of it, so that a person or an agent reads each line's length, vias and skew with no KiCad and no picture. The board is placed (the layout file read, the placer run for the rest) and has the copper `view pcb` draws on it: the routes file beside the source, the router's copper the editor keeps (`.netsmith/routes/`) for every net whose pads have not moved since it was routed, and the teardrops. With `--route` the router runs first, from that copper, as `board fab --reuse-routes` does (`--route-timeout` as for `view pcb`). Nothing is written.\n\nFor each net that no zone pours and that has a track or a via (a poured net's tracks are no connection) it prints the routed length (all its tracks, an arc by its chords), the straight length (the shortest tree of straight lines between the centres of its pads), their ratio, its vias, its layer changes (the vias where its tracks meet on two layers or more) and the layers its tracks are on; the nets by name, or longest first with `--lengths`. Then for each diff pair of the board (two nets whose names differ by a suffix pair such as `dp`/`dm`, the nets of a `PairImpedance` or a `DiffPair`, the lines of a `Can`): both lengths, the skew and whether it is within the pair's limit (its `.skew`, or the router's 0.1 mm), the length of the first net that runs coupled (the other's track on its layer within 0.5 mm, centre to centre, sampled every 0.05 mm) and its share, the least and the largest gap edge to edge over that length (equal for a pair at one pitch), both nets' vias and how many stand side by side (copper no more than 0.5 mm apart). Then for each group of nets matched in length: each net's length and the group's spread against its tolerance. These are the numbers the router matches a pair and a group by, and `board drc` notes a pair whose skew is over its limit and a group whose spread is over its tolerance. Lengths are in mm.\n\nWith `--format json` one JSON document is printed on stdout: `board`, `nets` (each with `net`, `length`, `straight`, `ratio`, `vias`, `layer_changes`, `layers`) and `pairs` (each with `p`, `n`, `length_p`, `length_n`, `skew`, `limit`, `over`, `coupled`, `share`, `gaps` (the least and the largest, or null), `vias_p`, `vias_n`, `via_pairs`) and `groups` (each with `path`, `nets` (each with `net`, `length`, `short_by`), `spread`, `tolerance`, `over`). Then the `track drop` section (issue #1540): for each power net whose rail states a voltage and whose current is known, of the nets no zone, plane or pour carries, the voltage its routed tracks drop, measured as `board fab`'s `track drop` guideline measures it (each run of the path at what the pins past it draw), in mV against its budget (1 % of the rail's voltage) and the share of the budget, `over` when it is more, the two pads of the path, its length, its narrowest track and the most current a run carries, the largest share first; it measures the copper the board has, routed or not, so a net left open drops what its copper does. In JSON it is `track_drops`, each with `net`, `volts`, `drop` and `allowed` (V), `share`, `over`, `current` (A), `whole_current` (every run at the net's current), `from`, `to`, `length`, `narrowest` and `thickness` (mm), and `width_for` (the `.min_track` in mm that brings the path within the budget, for a net over it). Then the `quality` section: the totals, the router's quality objective of the nets of the table (issue #1517: their routed length over their straight length, their vias and layer changes, and the nets routed over twice their straight length, the five longest ratios named; `objective` in the JSON, with `length`, `straight`, `detour`, `vias`, `layer_changes`, `over_twice` and `worst`, each with `net`, `ratio`, `vias`, `layer_changes`), and for each net with a defect, the most first, its own-net crossings (two straight tracks of the net on one layer whose centre lines cross away from their ends: a loop), its duplicate ways (a second way between the same two places, as `routes tidy` finds it), its unneeded copper (each track and via that ends in nothing, a dead stub or branch or a dangling via, a nudge from a pad's or a via's centre to a point still inside it, and a track inside or overlapping another of its net and layer), its short tracks (straight tracks shorter than two of their widths) and its vias side by side (two vias of the net whose copper is no more than the group gap apart, the process's least clearance or the board's `.process.via_group_gap`, each via taken once, a group no larger than the net's current needs left out), of the nets no zone pours; `board drc` notes the totals. In JSON it is `quality`, with `totals` and `nets` (each with `net`, `crossings`, `duplicates`, `unneeded`, `short`, `via_pairs`), the `limits` it is judged by and the kinds `over` them (each with `kind`, `count`, `limit`). The exit code is 0 when it is printed (a pair over its skew too: `board drc` is the check), 1 when the board does not build, place or route, or with `--gate` when a total is over its limit (0 of each, or the `--max-<kind>` flag's: `--max-crossings`, `--max-duplicates`, `--max-unneeded`, `--max-short`, `--max-via-pairs`), each kind over said on stderr, and 2 when the command cannot run."
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
    /// Fail (exit code 1) when a count of the quality section is over its limit.
    #[arg(long)]
    pub gate: bool,
    /// The most own-net crossings `--gate` allows (default 0).
    #[arg(long, value_name = "N", requires = "gate")]
    pub max_crossings: Option<usize>,
    /// The most duplicate ways `--gate` allows (default 0).
    #[arg(long, value_name = "N", requires = "gate")]
    pub max_duplicates: Option<usize>,
    /// The most unneeded tracks and vias `--gate` allows (default 0).
    #[arg(long, value_name = "N", requires = "gate")]
    pub max_unneeded: Option<usize>,
    /// The most short tracks `--gate` allows (default 0).
    #[arg(long, value_name = "N", requires = "gate")]
    pub max_short: Option<usize>,
    /// The most pairs of vias side by side `--gate` allows (default 0).
    #[arg(long, value_name = "N", requires = "gate")]
    pub max_via_pairs: Option<usize>,
    #[command(flatten)]
    pub placing: common::Placing,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
    #[command(flatten)]
    pub framing: crate::frame::Framing,
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
                    pcb_picture::put_copper(
                        &mut board,
                        &placed.layout_of(built).file,
                        route.as_ref(),
                        &say,
                    )
                    .map_err(|e| format!("error: {label}: {e}\n"))?;
                    let mut table = netsmith_fabricate::net_table::of_board(&board)
                        .map_err(|e| format!("error: {label}: {e}\n"))?;
                    if args.lengths {
                        table.by_length();
                    }
                    table.board = label;
                    let quality = Quality::of_board(&board).with_objective(&table);
                    // The track-drop guideline of `fab`, on the copper the board has (issue
                    // #1540).
                    let drops =
                        netsmith_fabricate::track_drop::drops_of(&board, &built.trace.model);
                    Ok((table, quality, drops))
                })?
            },
        )
    });
    let limits = Counts {
        crossings: args.max_crossings.unwrap_or(Counts::LIMITS.crossings),
        duplicates: args.max_duplicates.unwrap_or(Counts::LIMITS.duplicates),
        unneeded: args.max_unneeded.unwrap_or(Counts::LIMITS.unneeded),
        short: args.max_short.unwrap_or(Counts::LIMITS.short),
        via_pairs: args.max_via_pairs.unwrap_or(Counts::LIMITS.via_pairs),
    };
    match made {
        Ok((table, quality, drops)) => {
            let over = quality.over(&limits);
            match args.format {
                common::Format::Json => {
                    let mut out = serde_json::to_value(&table).expect("plain data");
                    let mut q = serde_json::to_value(&quality).expect("plain data");
                    q["limits"] = serde_json::to_value(limits).expect("plain data");
                    q["over"] = serde_json::to_value(&over).expect("plain data");
                    out["quality"] = q;
                    out["track_drops"] = serde_json::to_value(&drops).expect("plain data");
                    if let Err(e) = args.framing.print(
                        &out,
                        crate::frame::Frame::BoardFile,
                        Some(crate::frame::design_corner()),
                        false,
                    ) {
                        return crate::frame::refused(&e);
                    }
                }
                common::Format::Text => {
                    println!("{}:", table.board);
                    let moved = args.framing.moved(
                        crate::frame::Frame::BoardFile,
                        Some(crate::frame::design_corner()),
                    );
                    let (_, dx, dy) = match moved {
                        Ok(moved) => moved,
                        Err(e) => return crate::frame::refused(&e),
                    };
                    print!("{}", table.text_moved((dx, dy)));
                    print!("{}", netsmith_fabricate::track_drop::text(&drops));
                    print!("{}", quality.text());
                }
            }
            if args.gate && !over.is_empty() {
                for o in &over {
                    eprintln!(
                        "{}: quality gate: {} {} over the limit of {}",
                        table.board, o.count, o.kind, o.limit
                    );
                }
                return ExitCode::FAILURE;
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
        let Ok(Leaf::Nets(a)) = parse(&[
            "board",
            "nets",
            "--gate",
            "--max-crossings",
            "1",
            "--max-duplicates",
            "2",
            "--max-unneeded",
            "3",
            "--max-short",
            "4",
            "--max-via-pairs",
            "5",
        ]) else {
            panic!("nets should parse");
        };
        assert!(a.gate);
        assert_eq!(
            (
                a.max_crossings,
                a.max_duplicates,
                a.max_unneeded,
                a.max_short,
                a.max_via_pairs
            ),
            (Some(1), Some(2), Some(3), Some(4), Some(5))
        );
        // A limit is for the gate.
        assert!(parse(&["board", "nets", "--max-short", "3"]).is_err());
        // A timeout is for the router.
        assert!(parse(&["board", "nets", "--route-timeout", "30"]).is_err());
    }
}
