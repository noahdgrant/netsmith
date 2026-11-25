//! `netsmith routes fanout` (issue #1640): the escape of a fine-pitch part fanned out as a group,
//! a via site assigned to each pad that needs one (negotiated across the group, or as a plan file
//! says) and each pad routed to its via, written to the routes file. A verb of `routes`, as
//! `sites` is; the fanout is `netsmith_fabricate::fanout`.

use std::process::ExitCode;

use clap::Args as ClapArgs;

use super::common;
use super::routes::Target;

#[derive(Debug, PartialEq, ClapArgs)]
pub struct Args {
    /// The part, by its reference (`U1`).
    #[arg(value_name = "REF")]
    pub part: String,
    #[command(flatten)]
    pub target: Target,
    /// Only the pads of these nets, `A,B`.
    #[arg(long, value_name = "NET,...", value_delimiter = ',')]
    pub net: Vec<String>,
    /// These pads instead of those that need to escape, `B2,C3` or `U1.B2` (the outer ring too).
    #[arg(
        long,
        value_name = "PAD,...",
        value_delimiter = ',',
        conflicts_with = "plan"
    )]
    pub pads: Vec<String>,
    /// Follow a plan: a file of `PAD = X,Y` lines, a via site for each pad (`#` starts a comment).
    ///
    /// Places are mm in the routes file's frame. Only the plan's pads are fanned out, each to its
    /// site; a site the design rules check finds an error with is refused, naming what blocks
    /// it, and nothing is written.
    #[arg(long, value_name = "FILE", conflicts_with = "sites")]
    pub plan: Option<String>,
    /// The sites the pads may take, `X,Y`, repeatable or `X,Y;X,Y` (default: the part's sites,
    /// as `routes sites` lists them).
    #[arg(long, value_name = "X,Y", value_delimiter = ';', value_parser = super::routes::parse_point, allow_hyphen_values = true, action = clap::ArgAction::Append)]
    pub sites: Vec<[f64; 2]>,
    /// Say what it would do, and write nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
    #[command(flatten)]
    pub framing: crate::frame::Framing,
}

pub const LONG_HELP: &str = "The pads that need to escape are the part's surface-mount pads on a net that leaves the part (a pad of another part is on it), not on the outer ring of its pads (the first or last of a row or a column, which reach the copper outside the part on its face with no via), that no copper of the routes file reaches already (a track of the net on the pad's face with an end in the pad, or a via of the net in it: said as joined already). `--net` keeps the pads of those nets; `--pads` names the pads instead, the outer ring too.\n\nThe sites are those `routes sites` lists between the part's pads, less those copper of another net holds, or with `--sites` the places given, each judged as `routes check-via` judges it (a place with an error is said with what blocks it and not used). A pad takes a site no further than 1.5 pitches from its centre, and two vias of different nets keep the board's clearance and hole to hole space. The pads and sites are matched together at the least total distance (the Hungarian method: the group at once, not one pad at a time), a pair of sites too near each other barred to the pad further from its site and matched again; then the group is routed in one run of the router, each pad to its via, the router's negotiation among their lines, every other net held as it is. A net whose pads did not reach their vias is matched again with its site barred, at most 3 rounds. With `--plan FILE` (lines `PAD = X,Y`, `REF.PAD` also taken, `#` for comments, places in the routes file's frame) the plan's pads are routed to its sites as they are: a planned site the check refuses, or one too near another planned site of another net, is refused with what blocks it, and nothing is written.\n\nEach pad that cannot escape is listed with what blocks it: no legal site within reach, sites held by another net's copper or given to other pads, what walls its line in (pads, vias, the copper of other nets, the edge), or the design rules check's refusal of its copper. The vias and tracks go to each net's record, unlocked, as one edit checked as `routes add-track` checks one: a result that adds an error is refused, and then each net is tried alone, so the nets that pass are written. `fab` routes the rest of each net from the vias as from any copper of the routes file. `--dry-run` says what it would do and writes nothing. With `--format json`: `board`, `part`, `via` (`diameter`, `drill`), `pitch`, `needed`, `escaped` (`pad`, `net`, `at`, `round`), `joined`, `left` (`pad`, `net`, `at`, `blocker`, `walls`), `sites`, `sites_refused` (`at`, `blocker`), `rounds`, `runs`, `checks`, `dry_run`, `written` and `file`. Places are in the outline's frame, as the routes file's; `--frame` moves those printed. The exit code is 0 when every pad that needs to escape is fanned out (or would be, with `--dry-run`), 1 when one is left (the others are written) or the ask is refused, and 2 when it could not run.";

pub fn run(args: &Args) -> ExitCode {
    use netsmith_fabricate::fanout;
    use netsmith_fabricate::routes as lib;
    let plan = match &args.plan {
        None => None,
        Some(path) => match std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read the plan {path}: {e}"))
            .and_then(|t| fanout::parse_plan(&t).map_err(|e| format!("the plan {path}: {e}")))
        {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        },
    };
    let ask = fanout::FanoutAsk {
        part: args.part.clone(),
        nets: args.net.clone(),
        pads: args.pads.clone(),
        plan,
        sites: args.sites.clone(),
        weights: None,
    };
    let done = common::with_placed_board(
        args.target.file.as_deref(),
        args.target.board.as_deref(),
        None,
        |placed| {
            let (board, layout) =
                placed.with_board(|built, board| (board.clone(), placed.layout_of(built).file))?;
            let file = lib::beside_layout(&layout);
            let routes = lib::load(&file).map_err(|e| format!("error: {e}\n"))?;
            let done = match fanout::fanout(&board, &routes, &ask) {
                Ok(d) => d,
                Err(lib::Refusal::Refused(e)) => return Ok(Err(e)),
                Err(lib::Refusal::Failed(e)) => return Err(format!("error: {e}\n")),
            };
            let written = match (&done.routes, args.dry_run) {
                (Some(r), false) => {
                    lib::save(&file, r).map_err(|e| format!("error: {e}\n"))?;
                    true
                }
                _ => false,
            };
            Ok(Ok((done, file, written)))
        },
    );
    let (done, file, written) = match done {
        Ok(Ok(done)) => done,
        Ok(Err(e)) => {
            eprintln!("error: {}", args.framing.routes_drc(&e));
            return ExitCode::FAILURE;
        }
        Err(code) => return code,
    };
    match args.format {
        common::Format::Json => {
            let mut doc = serde_json::to_value(&done).expect("plain data");
            doc["dry_run"] = args.dry_run.into();
            doc["written"] = written.into();
            doc["file"] = file.display().to_string().into();
            if let Err(e) = args.framing.print(
                &doc,
                crate::frame::Frame::Outline,
                Some(crate::frame::design_corner()),
                true,
            ) {
                return crate::frame::refused(&e);
            }
        }
        common::Format::Text => {
            let (by, says) = args.framing.text_move();
            print!("{}", done.text_moved(by, &says));
            if written {
                println!("wrote {}", file.display());
            } else if args.dry_run && done.routes.is_some() {
                println!("dry run: {} is not written", file.display());
            }
        }
    }
    if done.left.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
