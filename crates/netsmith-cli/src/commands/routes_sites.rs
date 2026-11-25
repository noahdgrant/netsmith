//! `netsmith routes sites` (issue #1628): the legal via sites between the pads of a part, and
//! which net's copper holds each, for the escape of a fine-pitch part. A verb of `routes`, as
//! `join-pad` is: it reads the placed board and the routes file and judges each site by the
//! check of `routes add-via`. The sites are `netsmith_fabricate::via_sites`.

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
    /// Judge the sites for a via of this net (default: a via of no net of the board).
    ///
    /// Its own pads and copper are then no other net's, and only its pads are sites in a pad.
    #[arg(long, value_name = "NET")]
    pub net: Option<String>,
    /// List apart the sites whose via would cut a new island with pads or vias off a pour or zone.
    ///
    /// The pour, plane or zone is named with the island, which holds pads or vias.
    #[arg(long)]
    pub zone_safe: bool,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
    #[command(flatten)]
    pub framing: crate::frame::Framing,
}

pub const LONG_HELP: &str = "The sites tried are the midpoints of the part's pairs of neighbouring pads, two pads no further apart than 1.5 times the part's pitch (its least distance between two pad centres): on a grid, the place between two pads of a row or column and the centre of each square of four. Each is judged for the process's through via by the design rules check that judges `routes add-via`, on the board near the part: a site is listed when the check finds no new error with the via on the placed board with no routes copper (clearance to pads of other nets, to the edge, hole to hole, drill and ring minimums, the mask, keepouts and rule areas, the process's limits), and the sites refused are counted by the first rule that refused them. A listed site is `held` when the check finds a new error with the via among the routes file's copper, and the tracks and vias it names hold it, each with its net, its place, the rule and, for a rule of a space, the space required, the space measured and the shortfall in mm, the figures `routes check-via` gives for the place; a pour, plane or zone holds none, as its fill is made round a via, and the router's copper of `fab` is not on the board. With no `--net` the via is of no net of the board, so a free site takes a via of any of its neighbours' nets; with `--net` its own pads and copper are not in its way. Each site has the pads whose centres are nearest it, with their nets and the distance from centre to centre.\n\nA site is marked `cuts a fill` when a via there would cut a pour, a plane or a zone, on any layer it passes, into a new island that holds pads or vias of the fill's net: each island with its layer, the fill, its net, a corner, its area and the pads and vias it would cut off. It is found on the board cut to a box 3 mm beyond the via's copper, filled with the via and without it as `board fill --islands` fills it, so a pocket that reaches further is not seen. It is a note: the check refuses none of it. With `--zone-safe` such sites are left out of the lists and listed apart, after them, with their islands.\n\nThe surface-mount pads of the part are listed apart, a via at each pad's centre of the pad's net (with `--net`, its pads alone): a via in a pad needs the process to fill and cap it (`.via_in_pad`); `filled` is false, with the `via_in_pad` finding, when the process fills none that wide, and the other rules judge it as they judge a site between pads.\n\nWith `--format json`: `board`, `part`, `net`, `via` (`diameter`, `drill`), `between` (each with `at`, `pads` of `pad`, `net` and `distance`, `free`, `held_by` (each with `net`, `what`, `track` or `via`, `item`, the check's name for it, `at`, `rule`, `required`, `measured` and `shortfall`, the largest shortfall first) and `pockets` (each with `layer`, `fill`, `plane`, `pour` or `zone`, `net`, `at`, `area`, `pads` and `vias` of `at`)), `in_pad` (each with `pad`, `net`, `at`, `filled`, `why`, `free`, `held_by`, `pockets`), `tried`, `refused` (`rule`, `sites`), `zone_safe`, `cuts_zone` and `cuts_zone_in_pad` (the sites `--zone-safe` set apart, as `between` and `in_pad` have them), `checks` and `fills`, the fills of the box round a site made. Places are in the outline's frame, as the routes file's. The exit code is 0 when the sites are listed, 1 when the board has no such part or net, and 2 when it could not run.";

pub fn run(args: &Args) -> ExitCode {
    use netsmith_fabricate::routes as lib;
    let done = common::with_placed_board(
        args.target.file.as_deref(),
        args.target.board.as_deref(),
        None,
        |placed| {
            let (board, layout) =
                placed.with_board(|built, board| (board.clone(), placed.layout_of(built).file))?;
            let routes =
                lib::load(&lib::beside_layout(&layout)).map_err(|e| format!("error: {e}\n"))?;
            match netsmith_fabricate::via_sites::sites(
                &board,
                &routes,
                &args.part,
                args.net.as_deref(),
            ) {
                Ok(s) => Ok(Ok(s)),
                Err(lib::Refusal::Refused(e)) => Ok(Err(e)),
                Err(lib::Refusal::Failed(e)) => Err(format!("error: {e}\n")),
            }
        },
    );
    let sites = match done {
        Ok(Ok(s)) if args.zone_safe => s.zone_safe(),
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
        Err(code) => return code,
    };
    let own = crate::frame::Frame::Outline;
    match args.format {
        common::Format::Json => {
            if let Err(e) =
                args.framing
                    .print(&sites, own, Some(crate::frame::design_corner()), true)
            {
                return crate::frame::refused(&e);
            }
        }
        common::Format::Text => {
            let (by, says) = args.framing.text_move();
            print!("{}", sites.text_moved(by, &says));
        }
    }
    ExitCode::SUCCESS
}
