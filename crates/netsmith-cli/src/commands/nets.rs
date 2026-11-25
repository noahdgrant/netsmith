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
use netsmith_fabricate::net_quality::{Compared, Counts, Quality, Routing};
use netsmith_fabricate::pcb_picture;

use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`board nets` measures a board's routed copper and prints a table of it, so that a person or an agent reads each line's length, vias and skew with no KiCad and no picture. The board is placed (the layout file read, the placer run for the rest) and has the copper `view pcb` draws on it: the routes file beside the source, the router's copper the editor keeps (`.netsmith/routes/`) for every net whose pads have not moved since it was routed, and the teardrops. With `--route` the router runs first, from that copper, as `board fab --reuse-routes` does (`--route-timeout` as for `view pcb`). Nothing is written.\n\nFor each net that no zone pours and that has a track or a via (a poured net's tracks are no connection) it prints the routed length (all its tracks, an arc by its chords), the straight length (the shortest tree of straight lines between the centres of its pads), their ratio, its vias, its layer changes (the vias where its tracks meet on two layers or more) and the layers its tracks are on; the nets by name, or longest first with `--lengths`. Then for each diff pair of the board (two nets whose names differ by a suffix pair such as `dp`/`dm`, the nets of a `PairImpedance` or a `DiffPair`, the lines of a `Can`): both lengths, the skew and whether it is within the pair's limit (its `.skew`, or the router's 0.1 mm), the length of the first net that runs coupled (the other's track on its layer within 0.5 mm, centre to centre, sampled every 0.05 mm) and its share, the least and the largest gap edge to edge over that length (equal for a pair at one pitch), both nets' vias and how many stand side by side (copper no more than 0.5 mm apart). Then for each group of nets matched in length: each net's length and the group's spread against its tolerance. These are the numbers the router matches a pair and a group by, and `board drc` notes a pair whose skew is over its limit and a group whose spread is over its tolerance. Lengths are in mm.\n\nWith `--format json` one JSON document is printed on stdout: `board`, `nets` (each with `net`, `length`, `straight`, `ratio`, `vias`, `layer_changes`, `layers`) and `pairs` (each with `p`, `n`, `length_p`, `length_n`, `skew`, `limit`, `over`, `coupled`, `share`, `gaps` (the least and the largest, or null), `vias_p`, `vias_n`, `via_pairs`) and `groups` (each with `path`, `nets` (each with `net`, `length`, `short_by`), `spread`, `tolerance`, `over`). Then the `track drop` section (issue #1540): for each power net whose rail states a voltage and whose current is known, of the nets no zone, plane or pour carries, the voltage its routed tracks drop, measured as `board fab`'s `track drop` guideline measures it (each run of the path at what the pins past it draw), in mV against its budget (1 % of the rail's voltage) and the share of the budget, `over` when it is more, the two pads of the path, its length, its narrowest track and the most current a run carries, the largest share first; it measures the copper the board has, routed or not, so a net left open drops what its copper does. In JSON it is `track_drops`, each with `net`, `volts`, `drop` and `allowed` (V), `share`, `over`, `current` (A), `whole_current` (every run at the net's current), `from`, `to`, `length`, `narrowest` and `thickness` (mm), and `width_for` (the `.min_track` in mm that brings the path within the budget, for a net over it). With `--at-current NET=A`, once for each net (issue #1694), a `track drop at another current` section follows the list: for each net named, in the order given, its drop and share of the budget at the design's current (the net's one current, `--currents`) and at A, the budget and the two pads of the path; at A every run carries its design current times A over the net's, so the pins share it as in the design and the drop is in proportion. It is a report only: `track_drops`, the pieces, the `quality` rows, `--gate` and the exit code keep the design's current. In JSON it is `track_drops_at_current`, left out without the flag, each with `net`, `volts`, `allowed` (V), `from`, `to`, and `design` and `alternative`, each with `current` (the net's, A), `drop` (V), `share` and `over`; a net the board does not have, one with no track drop or one named twice is an error (exit code 2). Under a net at half its budget or more the pieces of its path are listed (issue #1610), the one that drops most first: each track (`#n track (x, y) to (x, y) on <layer>, <width> mm wide, <length> mm long, <current>: <mV> (<share> of the drop)`, then `, needs <w> mm` for a net over the budget where the track is narrower than the width it needs) and each via the path changes layer in; in JSON `path`, each with `step` (its place from the source, from 0), `track` (the index of the track in the board's copper) or `via` (its place), `layer`, `from`, `to`, `length`, `width` (drawn, mm), `current` (A), `drop` (V), `needs` (mm) and `unreachable` (no width is enough). With `--currents`, the `currents` section follows (issue #1584): for each net with a known current, the one current that the design rules check holds its tracks (`track_current`) and its vias (`via_current`) to and the track drop measures it at, the worst case in A, where it comes from (`sum`, every rail and part on the net with every load on at once; `analysis`, what the DC analysis finds into it over every corner of its ranges; `carrier`, one rail or part on the net that draws more than the analysis finds, such as a diode's stated `.current`), `at least` where a current on the net is not known, the words of the findings, and the least track on an outer and an inner layer and the vias side by side that it needs; copper that only feeds the pins of branches is held to what those pins carry, never more than this. In JSON it is `currents`, each with `net`, `current`, `from`, `lower_bound`, `source`, `least_width`, `inner_least_width` and `vias`. Then the `quality` section: the totals, the router's quality objective of the nets of the table (issue #1517: their routed length over their straight length, their vias and layer changes, and the nets routed over twice their straight length, the five longest ratios named; `objective` in the JSON, with `length`, `straight`, `detour`, `vias`, `layer_changes`, `over_twice` and `worst`, each with `net`, `ratio`, `vias`, `layer_changes`), and for each net with a defect, the most first, its own-net crossings (two straight tracks of the net on one layer whose centre lines cross away from their ends: a loop), its duplicate ways (a second way between the same two places, as `routes tidy` finds it), its unneeded copper (each track and via that ends in nothing, a dead stub or branch or a dangling via, a nudge from a pad's or a via's centre to a point still inside it, and a track inside or overlapping another of its net and layer), its short tracks (straight tracks shorter than two of their widths) and its vias side by side (two vias of the net whose copper is no more than the group gap apart, the process's least clearance or the board's `.process.via_group_gap`, each via taken once, a group no larger than the net's current needs left out), of the nets no zone pours; `board drc` notes the totals. In JSON it is `quality`, with `totals` and `nets` (each with `net`, `crossings`, `duplicates`, `unneeded`, `short`, `via_pairs`), the `limits` it is judged by and the kinds `over` them (each with `kind`, `count`, `limit`). After the objective, the `routing:` line (issue #1629): the tracks of the nets no zone pours, the connections left to route, the track drops over budget and the errors of the design rules check as `board drc` counts them, the connections left out (`tracks`, `connections_left`, `drops_over` and `findings` in `quality`). With `--compare FILE` each row a routing round is judged by (the signal length, short tracks, layer changes, via pairs, vias, tracks, connections left, track drops over budget and design rules errors, each read from the `quality` section, so no number is said twice) is printed before, after and with its change; the earlier state is the JSON of an earlier `board nets --format json` run (a `.json` file; a row it lacks is `-`) or a routes file, measured as the board is with it in place of the routes file beside the source (the kept copper as now, never routed). In JSON it is `compare`, with `against`, `tolerance` and `rows` (each with `row`, `before`, `after`, `change`, `worse`). With `--power-stage` (issue #1585) it prints, instead of all that, each switching regulator's power stage: a regulator is a part that provides the standard library's `BuckIc`, its VIN, ground, switch node and feedback the pins `vin.pos`, `vin.neg`, `sw.pos` and `fb`; each input capacitor (a capacitor between VIN and ground), nearest first, with the distance from its VIN pad and from its ground pad to the IC's nearest pad of each, centre to centre, and the routed length and vias of the shortest way through the tracks between them (`no track` where a pour or a plane carries the net, or it is open); each capacitor on another pin of the IC (a boot capacitor, whose other pad is on the switch node; an internal supply's) with the same from the pin; the feedback net's routed length and vias; and each via and track of another net within the process's `.switch_node_keepout` (1 mm unless stated) of the switch node or under the IC's body, with no plane between, which `board drc` notes as `switch_node_copper`. In JSON it is `board` and `power_stages`, each with `regulator`, `path`, `vin`, `gnd`, `sw`, `keepout`, `input_caps` (`cap`, `vin` and `gnd`, each with `from`, `to`, `distance`, `routed`, `vias`), `support` (`pin`, `net`, `cap`, `boot`, `leg`), `feedback` (`net`, `length`, `vias`) and `foreign` (`kind`, `net`, `layer`, `at`, `gap`, `under_body`). With `--decoupling` (issue #1730) it prints, instead of all that, each decoupling capacitor's loop, geometry only (no inductance): the capacitors of the placer's decoupling pairs (bulk capacitors of a rail too), the return the net the router takes for it (the one with a pour or a plane, else the one with more pads), the pin it serves the decoupled part's pin on the supply net nearest its supply pad and the return pin that part's pin on the return net nearest its other pad; for the supply leg and the return leg, the distance centre to centre and the routed length, narrowest track and vias of the shortest way through the net's tracks (`no track` where only a pour or a plane joins it, or it is open: a pour carries current but has no path, and the loop is then not complete); the loop, both added; and the return copper, the nearest copper layer with a zone of the return net over the capacitor's return pad and its height through the stackup (0 on the capacitor's own layer). In JSON it is `board` and `decoupling`, each with `cap`, `path`, `part`, `at`, `supply` and `return` (`net`, `from`, `to`, `distance`, `routed`, `narrowest`, `vias`), `loop` (`length`, `narrowest`, `vias`, `complete`) and `return_copper` (`net`, `layer`, `distance`); with `--compare FILE` (the JSON of an earlier `--decoupling` run, kept before a capacitor is moved, or a routes file) each capacitor's loop, vias, narrowest track and return copper before and after, `compare` with `against` and `loops` (`cap`, `before`, `after`, `length_change`, `vias_change`). The exit code is 0 when it is printed (a pair over its skew too: `board drc` is the check), 1 when the board does not build, place or route, or with `--gate` when a total is over its limit (0 of each, or the `--max-<kind>` flag's: `--max-crossings`, `--max-duplicates`, `--max-unneeded`, `--max-short`, `--max-via-pairs`), each kind over said on stderr, or with `--gate` and `--compare` when a row is more than before by over `--tolerance` percent of before (0 by default), each row worse said on stderr, and 2 when the command cannot run."
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
    /// Print each switching regulator's power stage instead of the table: loops, routes, near
    /// copper.
    ///
    /// Its input capacitors' loop, its support capacitors' routes, its feedback route and the
    /// copper of other nets near its switch node.
    #[arg(long, conflicts_with_all = ["lengths", "gate", "currents", "compare", "at_current"])]
    pub power_stage: bool,
    /// Print each decoupling capacitor's loop instead of the table: lengths, widths, vias, plane.
    ///
    /// Geometry only: the routed way to the pin it serves and back on the return net, its
    /// narrowest track, its vias and the nearest return copper. With `--compare`, before and
    /// after.
    #[arg(long, conflicts_with_all = ["lengths", "gate", "currents", "at_current", "power_stage", "tolerance"])]
    pub decoupling: bool,
    /// Route the board first, from the routes file's and the kept copper, as `board fab
    /// --reuse-routes` does.
    #[arg(long)]
    pub route: bool,
    /// Seconds the router may take with `--route`; 0 is no limit (default 300).
    #[arg(long, value_name = "SECS", requires = "route")]
    pub route_timeout: Option<u64>,
    /// Also list each net's one current and where it comes from.
    ///
    /// It is what the design rules check holds the net's tracks and vias to and the track drop
    /// measures it at.
    #[arg(long)]
    pub currents: bool,
    #[command(flatten)]
    pub at: common::AtCurrents,
    /// Fail (exit code 1) when a count of the quality section is over its limit.
    #[arg(long)]
    pub gate: bool,
    /// Compare the quality rows with an earlier state: a routes file, or a JSON run's output.
    #[arg(long, value_name = "FILE")]
    pub compare: Option<String>,
    /// How much worse than before, in percent, a compared row may be under `--gate` (default 0).
    #[arg(long, value_name = "PCT", requires = "compare")]
    pub tolerance: Option<f64>,
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
                    let layout = placed.layout_of(built).file.clone();
                    let placed_board = board;
                    let mut board = board.clone();
                    let say = |line: String| eprintln!("{label}: {line}");
                    pcb_picture::put_copper(&mut board, &layout, route.as_ref(), &say)
                        .map_err(|e| format!("error: {label}: {e}\n"))?;
                    // The table, the quality section with its rows and the track drops of a
                    // board's copper (issue #1629: the rows' design rules check is `board
                    // drc`'s).
                    let measure = |board: &netsmith_pcb::Board| {
                        let fail = |e: String| format!("error: {label}: {e}\n");
                        let table = netsmith_fabricate::net_table::of_board(board).map_err(fail)?;
                        let drops =
                            netsmith_fabricate::track_drop::drops_of(board, &built.trace.model);
                        let constraints = netsmith_pipeline::constraints::of_design(
                            &built.design,
                            placed.catalogs.constraints_warn(),
                        );
                        let checked = netsmith_fabricate::drc::of_board(
                            board,
                            constraints,
                            placed.catalogs.strict_guidelines(),
                        )
                        .map_err(fail)?;
                        let counts = checked.report.counts();
                        let routing = Routing::of_board(
                            board,
                            drops.iter().filter(|d| d.over).count(),
                            (counts.violations + counts.footprint_errors) as usize,
                        );
                        let quality = Quality::of_board(board)
                            .with_objective(&table)
                            .with_routing(routing);
                        Ok::<_, String>((table, quality, drops))
                    };
                    // The power stage of each switching regulator (issue #1585).
                    let stages = if args.power_stage {
                        netsmith_fabricate::power_stage::of_board(&board)
                            .map_err(|e| format!("error: {label}: {e}\n"))?
                    } else {
                        Vec::new()
                    };
                    // The decoupling loop of each capacitor (issue #1730), and the earlier
                    // state's with `--compare`: the JSON of an earlier run, or the board with
                    // another routes file.
                    let loops = if args.decoupling {
                        let now = netsmith_fabricate::decoupling_loop::of_board(&board)
                            .map_err(|e| format!("error: {label}: {e}\n"))?;
                        let before = match &args.compare {
                            None => None,
                            Some(file) => Some(loops_before(file, &|routes| {
                                let mut earlier = placed_board.clone();
                                let say = |line: String| eprintln!("{label}: {file}: {line}");
                                pcb_picture::put_copper_from(
                                    &mut earlier,
                                    &layout,
                                    routes,
                                    None,
                                    &say,
                                )
                                .map_err(|e| format!("error: {label}: {e}\n"))?;
                                netsmith_fabricate::decoupling_loop::of_board(&earlier)
                                    .map_err(|e| format!("error: {label}: {e}\n"))
                            })?),
                        };
                        Some((now, before))
                    } else {
                        None
                    };
                    if let Some(loops) = loops {
                        return Ok(Made::Loops(label.to_string(), loops));
                    }
                    // The track-drop guideline of `fab`, on the copper the board has (issue
                    // #1540).
                    let (mut table, quality, drops) = measure(&board)?;
                    // The earlier state's quality section (issue #1629): the JSON of an earlier
                    // run, or the board with another routes file, measured as it stands.
                    let before = match &args.compare {
                        None => None,
                        Some(file) => Some(before_of(file, &|routes| {
                            let mut earlier = placed_board.clone();
                            let say = |line: String| eprintln!("{label}: {file}: {line}");
                            pcb_picture::put_copper_from(&mut earlier, &layout, routes, None, &say)
                                .map_err(|e| format!("error: {label}: {e}\n"))?;
                            let (_, quality, _) = measure(&earlier)?;
                            Ok(serde_json::to_value(&quality).expect("plain data"))
                        })?),
                    };
                    if args.lengths {
                        table.by_length();
                    }
                    table.board = label.clone();
                    let currents = netsmith_fabricate::net_currents::of_board(&board);
                    // The drops at the currents asked for (issue #1694): a net that has none
                    // is the command's fault to say, not the board's.
                    let at = netsmith_fabricate::track_drop::drops_at(
                        &board,
                        &drops,
                        &args.at.at_current,
                    )
                    .map_err(|e| format!("error: {label}: {e}"));
                    Ok(Made::Table(Box::new((
                        table, quality, drops, at, currents, stages, before,
                    ))))
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
    let made = match made {
        Ok(Made::Loops(board, (loops, before))) => {
            return print_loops(&args, &board, &loops, before);
        }
        Ok(Made::Table(t)) => Ok(*t),
        Err(code) => Err(code),
    };
    match made {
        Ok((table, quality, drops, at, currents, stages, before)) => {
            let at = match at {
                Ok(at) => at,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(2);
                }
            };
            if args.power_stage {
                match args.format {
                    common::Format::Json => {
                        let out = serde_json::json!({
                            "board": table.board,
                            "power_stages": stages,
                        });
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
                        print!("{}", netsmith_fab::power_stage::text(&stages));
                    }
                }
                return ExitCode::SUCCESS;
            }
            let over = quality.over(&limits);
            let compared = before.map(|(file, before)| {
                let after = serde_json::to_value(&quality).expect("plain data");
                Compared::of(file, &before, &after, args.tolerance.unwrap_or(0.0))
            });
            match args.format {
                common::Format::Json => {
                    let mut out = serde_json::to_value(&table).expect("plain data");
                    let mut q = serde_json::to_value(&quality).expect("plain data");
                    q["limits"] = serde_json::to_value(limits).expect("plain data");
                    q["over"] = serde_json::to_value(&over).expect("plain data");
                    out["quality"] = q;
                    out["track_drops"] = serde_json::to_value(&drops).expect("plain data");
                    if !at.is_empty() {
                        out["track_drops_at_current"] =
                            serde_json::to_value(&at).expect("plain data");
                    }
                    if args.currents {
                        out["currents"] = serde_json::to_value(&currents).expect("plain data");
                    }
                    if let Some(c) = &compared {
                        out["compare"] = serde_json::to_value(c).expect("plain data");
                    }
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
                    print!("{}", netsmith_fabricate::track_drop::text(&drops, (dx, dy)));
                    print!("{}", netsmith_fabricate::track_drop::at_text(&at));
                    if args.currents {
                        print!("{}", netsmith_fabricate::net_currents::text(&currents));
                    }
                    print!("{}", quality.text());
                    if let Some(c) = &compared {
                        print!("{}", c.text());
                    }
                }
            }
            let worse: Vec<_> = compared.iter().flat_map(Compared::worse).collect();
            if args.gate && (!over.is_empty() || !worse.is_empty()) {
                for o in &over {
                    eprintln!(
                        "{}: quality gate: {} {} over the limit of {}",
                        table.board, o.count, o.kind, o.limit
                    );
                }
                if let Some(c) = &compared {
                    for w in &worse {
                        eprintln!("{}: quality gate: {}", table.board, c.worse_line(w));
                    }
                }
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(code) => code,
    }
}

/// What a board's run made: the table and its sections, or the decoupling loops (with the
/// earlier state's under `--compare`).
enum Made<T> {
    Table(Box<T>),
    Loops(String, Loops),
}

/// The decoupling loops, and the earlier state's: the file and each capacitor's figures.
type Loops = (
    Vec<netsmith_fab::decoupling_loop::DecouplingLoop>,
    Option<(String, Vec<(String, Figures)>)>,
);

use netsmith_fab::decoupling_loop::Figures;

/// Prints `board nets --decoupling` (issue #1730), with its comparison under `--compare`.
fn print_loops(
    args: &Args,
    board: &str,
    loops: &[netsmith_fab::decoupling_loop::DecouplingLoop],
    before: Option<(String, Vec<(String, Figures)>)>,
) -> ExitCode {
    let compared = before.map(|(file, before)| {
        let rows = netsmith_fab::decoupling_loop::compare(&before, loops);
        (file, rows)
    });
    let own = crate::frame::Frame::BoardFile;
    let corner = Some(crate::frame::design_corner());
    match args.format {
        common::Format::Json => {
            let mut out = serde_json::json!({ "board": board, "decoupling": loops });
            if let Some((file, rows)) = &compared {
                out["compare"] = serde_json::json!({ "against": file, "loops": rows });
            }
            if let Err(e) = args.framing.print(&out, own, corner, false) {
                return crate::frame::refused(&e);
            }
        }
        common::Format::Text => {
            let (_, dx, dy) = match args.framing.moved(own, corner) {
                Ok(moved) => moved,
                Err(e) => return crate::frame::refused(&e),
            };
            println!("{board}:");
            print!("{}", netsmith_fab::decoupling_loop::text(loops, (dx, dy)));
            if let Some((file, rows)) = &compared {
                print!(
                    "{}",
                    netsmith_fab::decoupling_loop::compare_text(file, rows)
                );
            }
        }
    }
    ExitCode::SUCCESS
}

/// The earlier state of `--decoupling --compare FILE`: each capacitor's figures, read from the
/// JSON of an earlier `board nets --decoupling --format json` run (a `.json` file), or else
/// measured by `measure` on the board with the routes file at `file`.
fn loops_before(
    file: &str,
    measure: &dyn Fn(
        &std::path::Path,
    ) -> Result<Vec<netsmith_fab::decoupling_loop::DecouplingLoop>, String>,
) -> Result<(String, Vec<(String, Figures)>), String> {
    let path = std::path::Path::new(file);
    if !path.is_file() {
        return Err(format!("error: --compare {file}: no such file\n"));
    }
    if path.extension().is_some_and(|e| e == "json") {
        let text = std::fs::read_to_string(path).map_err(|e| format!("error: {file}: {e}\n"))?;
        let doc: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("error: {file}: {e}\n"))?;
        let Some(loops) = doc.get("decoupling").and_then(|d| d.as_array()) else {
            return Err(format!(
                "error: {file}: no `decoupling` section: give the JSON of `board nets --decoupling --format json`, or a routes file\n"
            ));
        };
        let read = |l: &serde_json::Value| -> Option<(String, Figures)> {
            let t = l.get("loop")?;
            Some((
                l.get("cap")?.as_str()?.to_string(),
                Figures {
                    length: t.get("length")?.as_f64()?,
                    narrowest: t.get("narrowest").and_then(|w| w.as_f64()),
                    vias: t.get("vias")?.as_u64()? as usize,
                    complete: t.get("complete")?.as_bool()?,
                    return_copper: l
                        .get("return_copper")
                        .and_then(|p| p.get("distance"))
                        .and_then(|d| d.as_f64()),
                },
            ))
        };
        let figures: Option<Vec<_>> = loops.iter().map(read).collect();
        return figures
            .map(|f| (file.to_string(), f))
            .ok_or_else(|| format!("error: {file}: a loop of its `decoupling` section is not one `board nets --decoupling` prints\n"));
    }
    let loops = measure(path)?;
    Ok((
        file.to_string(),
        loops.iter().map(|l| (l.cap.clone(), l.figures())).collect(),
    ))
}

/// The earlier state of `--compare FILE`: the file's name and its `quality` section, read from
/// the JSON of an earlier `board nets --format json` run (a `.json` file), or else measured by
/// `measure` on the board with the routes file at `file`.
fn before_of(
    file: &str,
    measure: &dyn Fn(&std::path::Path) -> Result<serde_json::Value, String>,
) -> Result<(String, serde_json::Value), String> {
    let path = std::path::Path::new(file);
    if !path.is_file() {
        return Err(format!("error: --compare {file}: no such file\n"));
    }
    if path.extension().is_some_and(|e| e == "json") {
        let text = std::fs::read_to_string(path).map_err(|e| format!("error: {file}: {e}\n"))?;
        let doc: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("error: {file}: {e}\n"))?;
        return match doc.get("quality") {
            Some(q) => Ok((file.to_string(), q.clone())),
            None => Err(format!(
                "error: {file}: no `quality` section: give the JSON of `board nets --format json`, or a routes file\n"
            )),
        };
    }
    Ok((file.to_string(), measure(path)?))
}

#[cfg(test)]
mod tests {
    use crate::commands::common;
    use crate::commands::common::testing::{Leaf, parse};

    #[test]
    fn at_current_repeats_and_reads_amperes_and_milliamperes() {
        let Ok(Leaf::Nets(a)) = parse(&[
            "board",
            "nets",
            "--at-current",
            "3V3=0.5",
            "--at-current",
            "5V=250mA",
        ]) else {
            panic!("nets should parse");
        };
        let read: Vec<(&str, f64)> =
            a.at.at_current
                .iter()
                .map(|a| (a.net.as_str(), a.current))
                .collect();
        assert_eq!(read, [("3V3", 0.5), ("5V", 0.25)]);
        for bad in ["3V3", "=1", "3V3=", "3V3=0", "3V3=-1A", "3V3=2V", "3V3=inf"] {
            assert!(
                parse(&["board", "nets", "--at-current", bad]).is_err(),
                "{bad}"
            );
        }
        assert!(parse(&["board", "nets", "--power-stage", "--at-current", "5V=1"]).is_err());
        assert!(
            parse(&[
                "board",
                "drc",
                "--kicad-pcb",
                "b.kicad_pcb",
                "--at-current",
                "5V=1"
            ])
            .is_err()
        );
    }

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
        let Ok(Leaf::Nets(a)) = parse(&[
            "board",
            "nets",
            "--compare",
            "before.routes.toml",
            "--tolerance",
            "2.5",
        ]) else {
            panic!("nets should parse");
        };
        assert_eq!(
            (a.compare.as_deref(), a.tolerance),
            (Some("before.routes.toml"), Some(2.5))
        );
        // A tolerance is for the comparison, which is of the table's rows.
        assert!(parse(&["board", "nets", "--tolerance", "2"]).is_err());
        assert!(parse(&["board", "nets", "--compare", "a.json", "--power-stage"]).is_err());
        // A limit is for the gate.
        assert!(parse(&["board", "nets", "--max-short", "3"]).is_err());
        // A timeout is for the router.
        assert!(parse(&["board", "nets", "--route-timeout", "30"]).is_err());
    }
}
