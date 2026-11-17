use netsmith_fabricate::fab::FabOptions;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::Args as ClapArgs;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`route` routes the board that `board fab --save-placed` saved again, alone, the way `fab` does (signals at the track margin first where there is room, then at the process's least track), with no checking, solving or placing, so an experiment on the router (a change to its code, a time limit, a track margin) takes the time of the routing and nothing else. It writes `<board>.kicad_pcb`, `.kicad_pro`, `.svg` and, when connections are left, `routing-failure.txt` and `routing-report.json` into `--out`, as `fab` does; it prints the connections left, the tracks, the vias and the time, as text or, with `--format json`, as one JSON document on stdout (the fields are under `--format`); the progress is on stderr either way. It is a router tool and not a fab package: it runs no design rules check and writes no Gerbers.\n\nThe exit code is 0 when every connection is routed, 1 when connections are left, and 2 when the file cannot be read: the file's layout changes with netsmith, so a file saved by another version is refused and is saved again with `board fab --save-placed`."
)]
pub struct Args {
    /// The placed board that `board fab --save-placed` wrote.
    #[arg(value_name = "BOARD.PLACED.JSON")]
    pub file: PathBuf,
    /// The folder to write the board files into (default `out/`).
    #[arg(long, value_name = "DIR")]
    pub out: Option<PathBuf>,
    /// Seconds the router may take; 0 is no limit.
    #[arg(long, value_name = "SECS")]
    pub route_timeout: Option<u64>,
    /// Rounds of rip-up and re-route of whole nets for connections the router leaves; 0 is none.
    ///
    /// As `board fab --rip-rounds`: when a try ends with connections open, each round routes every one
    /// of them through the copper of the nets in its way, removes those nets and routes them
    /// again after it. The default is the router's own, 6.
    #[arg(long, value_name = "N")]
    pub rip_rounds: Option<usize>,
    /// The track margin, from 1 (the process's least track) to 4, over the file's.
    ///
    /// It replaces the margin the file was saved with: how many times the least track a signal is
    /// drawn at where there is room.
    #[arg(long, value_name = "X", value_parser = parse_margin)]
    pub margin: Option<f64>,
    /// What a 45 degree bend costs the search, in tenths of a mm of track (the router's own is 25).
    ///
    /// For experiments on the router: a change that is only luck moves the result when the turn
    /// cost moves by one. The scorecard's robustness line routes at 24, 25, 26 and 27.
    #[arg(long, value_name = "N", value_parser = parse_turn)]
    pub turn: Option<i64>,
    /// Try at most this many orders of the lines, at least 1; each try routes the whole board.
    ///
    /// With `--route-timeout 0` and `--tries 1` a run is one try to its end, which is what the
    /// scorecard's robustness line measures.
    #[arg(long, value_name = "N", value_parser = parse_tries)]
    pub tries: Option<usize>,
    /// Draw the try that gave the result as PNG pictures an agent can open.
    ///
    /// `stage-1-first-pass.png`, `stage-2-rip-up.png`, `stage-3-negotiation.png` (when it ran) and
    /// `stage-4-final.png`: each the copper that stage left (front red, back blue), the pads and
    /// vias, and the lines still left bright magenta on top; and `crowding.png`, the crowding map
    /// over the board, yellow to red where the searches fought most. Pictures of an earlier run
    /// that this one does not draw are removed.
    #[arg(long)]
    pub stages: bool,
    /// Follow a coarse plan of the board: `off` (the router's own), `every` try, or `later`, from
    /// the second try on.
    ///
    /// For experiments on the router. The plan routes every line over tiles of 1 mm whose edges
    /// hold as many tracks as the pads and the edge leave room for, negotiating where they are
    /// full, and the search then pays for a step outside a line's tiles where the plan fills
    /// the tile. The header of the router's `plan.rs` has what it was measured to do.
    #[arg(long, value_name = "WHEN", value_parser = parse_plan)]
    pub plan: Option<netsmith_fabricate::placed::CoarsePlan>,
    /// Route the lines by class, the most important first, with the protection: `off`, `every`
    /// try, or `later`, from the second try on (the router's own).
    ///
    /// For experiments on the router. The classes, their order and which of them keep their
    /// copper against a later class are in the language guide's "How the router orders the
    /// nets".
    #[arg(long, value_name = "WHEN", value_parser = parse_classes)]
    pub classes: Option<netsmith_fabricate::placed::LineClasses>,
    /// The largest radius of the arc drawn at each bend, in widths of its track (the router's own
    /// is 4); 0 draws none, and every bend keeps its 45 degree corner.
    ///
    /// For experiments on the router. The arcs are drawn after the routing, where they keep the
    /// rules' space, and change nothing the search does.
    #[arg(long, value_name = "WIDTHS", value_parser = parse_arcs)]
    pub arcs: Option<f64>,
    /// Text for a person, or one JSON document on stdout.
    ///
    /// `json` prints one JSON document on stdout and nothing else there: `command` (`route`),
    /// `ok` (every connection is routed, as the exit code 0 says), the `board`, the `left`
    /// connections as the text lists them after `not routed:`, `connections_left` (their count), the
    /// `tracks` and `vias` of the result, the `seconds` it took, the `tries` it made, `stopped`
    /// (the router ran out of its route timeout) and the `written` files. A file that cannot be
    /// read or written is an error on stderr with exit code 2 and nothing on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: super::common::Format,
}

/// Routes the saved board again, alone, the way `fab` does, and writes its board files.
pub fn run(args: Args) -> ExitCode {
    let request = netsmith_fabricate::placed::RouteRequest {
        route_timeout: match args.route_timeout {
            Some(0) => None,
            Some(secs) => Some(Duration::from_secs(secs)),
            None => FabOptions::default().route_timeout,
        },
        margin: args.margin,
        progress: Some(|message| eprintln!("{message}")),
        turn: args.turn,
        tries: args.tries,
        stages: args.stages,
        plan: args.plan,
        classes: args.classes,
        arcs: args.arcs,
        rip_rounds: args.rip_rounds,
    };
    run_route(&args.file, args.out.as_deref(), request, args.format)
}

/// `netsmith dev route`: route the placed board that `board fab --save-placed` wrote, alone, and write
/// its board files into `out` (`out/` when `None`). Exits 1 when connections are left, 2 when
/// the file cannot be read or the files cannot be written.
fn run_route(
    file: &Path,
    out: Option<&Path>,
    request: netsmith_fabricate::placed::RouteRequest,
    format: super::common::Format,
) -> ExitCode {
    let placed = match fs::read_to_string(file)
        .map_err(|e| e.to_string())
        .and_then(|text| netsmith_fabricate::placed::from_json(&text))
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}: {e}", file.display());
            return ExitCode::from(2);
        }
    };
    let out = out.unwrap_or(Path::new("out"));
    let routed =
        match netsmith_fabricate::placed::route(placed, out, &request, &|m| eprintln!("{m}")) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        };
    if format == super::common::Format::Json {
        super::common::print_json(&serde_json::json!({
            "command": "route",
            "ok": routed.left.is_empty(),
            "board": routed.board.name,
            "left": routed.left,
            "connections_left": routed.left.len(),
            "tracks": routed.board.tracks.len(),
            "vias": routed.board.vias.len(),
            "seconds": routed.elapsed.as_secs_f64(),
            "tries": routed.tries,
            "stopped": routed.stopped,
            "written": routed.written.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        }));
    } else {
        for path in &routed.written {
            println!("wrote {}", path.display());
        }
        for line in &routed.left {
            println!("not routed: {line}");
        }
        println!(
            "{}: {} connections left, {} tracks, {} vias, routed in {:.1} s ({} tries{})",
            routed.board.name,
            routed.left.len(),
            routed.board.tracks.len(),
            routed.board.vias.len(),
            routed.elapsed.as_secs_f64(),
            routed.tries,
            if routed.stopped {
                ", stopped at the route timeout"
            } else {
                ""
            }
        );
    }
    if routed.left.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn parse_turn(given: &str) -> Result<i64, String> {
    given
        .parse::<i64>()
        .ok()
        .filter(|t| (0..=10_000).contains(t))
        .ok_or_else(|| {
            format!(
                "takes a whole number from 0 to 10000 (tenths of a mm of track; the router's own is 25), not `{given}`"
            )
        })
}

fn parse_plan(given: &str) -> Result<netsmith_fabricate::placed::CoarsePlan, String> {
    use netsmith_fabricate::placed::CoarsePlan;
    match given {
        "off" => Ok(CoarsePlan::Off),
        "every" => Ok(CoarsePlan::EveryTry),
        "later" => Ok(CoarsePlan::FromSecondTry),
        _ => Err(format!("takes `off`, `every` or `later`, not `{given}`")),
    }
}

fn parse_tries(given: &str) -> Result<usize, String> {
    given
        .parse::<usize>()
        .ok()
        .filter(|&n| n >= 1)
        .ok_or_else(|| format!("takes a whole number of at least 1, not `{given}`"))
}

fn parse_arcs(given: &str) -> Result<f64, String> {
    given
        .parse::<f64>()
        .ok()
        .filter(|w| (0.0..=20.0).contains(w))
        .ok_or_else(|| format!("takes a number of track widths from 0 to 20, not `{given}`"))
}

fn parse_margin(given: &str) -> Result<f64, String> {
    given
        .parse::<f64>()
        .ok()
        .filter(|m| (1.0..=4.0).contains(m))
        .ok_or_else(|| {
            format!("takes a number from 1 (the process's least track) to 4, not `{given}`")
        })
}

fn parse_classes(given: &str) -> Result<netsmith_fabricate::placed::LineClasses, String> {
    use netsmith_fabricate::placed::LineClasses;
    match given {
        "off" => Ok(LineClasses::Off),
        "every" => Ok(LineClasses::EveryTry),
        "later" => Ok(LineClasses::FromSecondTry),
        _ => Err(format!("takes `off`, `every` or `later`, not `{given}`")),
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::Leaf;
    use crate::commands::common::testing::{kind, parse};
    use crate::commands::route;
    use clap::error::ErrorKind;

    #[test]
    fn route_takes_a_placed_board_a_timeout_and_a_margin() {
        assert_eq!(
            parse(&[
                "dev",
                "route",
                "b.placed.json",
                "--out",
                "o",
                "--route-timeout",
                "0",
                "--margin",
                "2.5"
            ]),
            Ok(Leaf::Route(route::Args {
                file: "b.placed.json".into(),
                out: Some("o".into()),
                route_timeout: Some(0),
                rip_rounds: None,
                margin: Some(2.5),
                turn: None,
                tries: None,
                stages: false,
                plan: None,
                classes: None,
                arcs: None,
                format: crate::commands::common::Format::Text,
            }))
        );
        assert_eq!(kind(&["dev", "route"]), ErrorKind::MissingRequiredArgument);
        for bad in ["0.5", "5", "wide"] {
            let e = parse(&["dev", "route", "b.json", "--margin", bad]).unwrap_err();
            assert!(
                e.contains("1 (the process's least track) to 4"),
                "{bad}: {e}"
            );
        }
    }

    #[test]
    fn route_takes_the_rounds_of_rip_up() {
        let Ok(Leaf::Route(args)) = parse(&["dev", "route", "b.placed.json", "--rip-rounds", "0"])
        else {
            panic!("--rip-rounds 0 parses");
        };
        assert_eq!(args.rip_rounds, Some(0));
        assert!(parse(&["dev", "route", "b.placed.json", "--rip-rounds", "some"]).is_err());
    }

    #[test]
    fn route_takes_the_largest_arc_radius_in_track_widths() {
        let Ok(Leaf::Route(args)) = parse(&["dev", "route", "b.placed.json", "--arcs", "0"]) else {
            panic!("--arcs 0 parses");
        };
        assert_eq!(args.arcs, Some(0.0));
        for bad in ["-1", "21", "round"] {
            let given = format!("--arcs={bad}");
            let e = parse(&["dev", "route", "b.placed.json", &given]).unwrap_err();
            assert!(e.contains("track widths from 0 to 20"), "{bad}: {e}");
        }
    }

    #[test]
    fn route_takes_a_turn_cost_a_number_of_tries_and_stages_for_experiments() {
        assert_eq!(
            parse(&[
                "dev",
                "route",
                "b.placed.json",
                "--turn",
                "27",
                "--tries",
                "1",
                "--stages",
                "--route-timeout",
                "0",
            ]),
            Ok(Leaf::Route(route::Args {
                file: "b.placed.json".into(),
                out: None,
                route_timeout: Some(0),
                rip_rounds: None,
                margin: None,
                turn: Some(27),
                tries: Some(1),
                stages: true,
                plan: None,
                classes: None,
                arcs: None,
                format: crate::commands::common::Format::Text,
            }))
        );
        for bad in ["2.5", "steep", "10001"] {
            let e = parse(&["dev", "route", "b.placed.json", "--turn", bad]).unwrap_err();
            assert!(
                e.contains("takes a whole number from 0 to 10000"),
                "{bad}: {e}"
            );
        }
        for bad in ["0", "many"] {
            let e = parse(&["dev", "route", "b.placed.json", "--tries", bad]).unwrap_err();
            assert!(
                e.contains("takes a whole number of at least 1"),
                "{bad}: {e}"
            );
        }
        for (given, plan) in [
            ("off", netsmith_fabricate::placed::CoarsePlan::Off),
            ("every", netsmith_fabricate::placed::CoarsePlan::EveryTry),
            (
                "later",
                netsmith_fabricate::placed::CoarsePlan::FromSecondTry,
            ),
        ] {
            match parse(&["dev", "route", "b.placed.json", "--plan", given]) {
                Ok(Leaf::Route(args)) => assert_eq!(args.plan, Some(plan), "{given}"),
                other => panic!("{given}: {other:?}"),
            }
        }
        let e = parse(&["dev", "route", "b.placed.json", "--plan", "always"]).unwrap_err();
        assert!(e.contains("takes `off`, `every` or `later`"), "{e}");
        // clap takes a negative number for a flag of its own and refuses it before the parser.
        for flag in ["--turn", "--tries"] {
            assert!(
                parse(&["dev", "route", "b.placed.json", flag, "-1"]).is_err(),
                "{flag} -1"
            );
        }
    }
}
