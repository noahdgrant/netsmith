//! `netsmith routes check-via` (issue #1643): whether a via fits at one place, and for each rule
//! it breaks, what is in its way and by how much. A verb of its own beside `add-via`, whose flags
//! it takes, rather than a mode of `routes sites`: `sites` answers for the many places of one
//! part, this for one place anywhere, with the numbers of each rule. The check is
//! `netsmith_fabricate::via_check`.

use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_pcb::CopperLayer;

use super::common;
use super::routes::{Target, parse_layer_pair, parse_point};

#[derive(Debug, PartialEq, ClapArgs)]
pub struct Args {
    #[command(flatten)]
    pub target: Target,
    /// Where the via would stand, `X,Y` in mm from the outline's top left corner.
    #[arg(long, value_name = "X,Y", value_parser = parse_point, allow_hyphen_values = true)]
    pub at: [f64; 2],
    /// The via's net (default: a net no pad is on).
    #[arg(long, value_name = "NET")]
    pub net: Option<String>,
    /// The diameter in mm (default: the process's via).
    #[arg(long, value_name = "MM")]
    pub diameter: Option<f64>,
    /// The drill in mm (default: the process's via drill).
    #[arg(long, value_name = "MM")]
    pub drill: Option<f64>,
    /// A blind or a buried via: the two layers it ends on, `top,inner1`, a via the board's HDI
    /// process makes; its size is then that via's (the laser's for a blind via). Default: a
    /// through via.
    #[arg(long, value_name = "LAYER,LAYER", value_parser = parse_layer_pair)]
    pub layers: Option<[CopperLayer; 2]>,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
    #[command(flatten)]
    pub framing: crate::frame::Framing,
}

pub const LONG_HELP: &str = "The via (the process's through via, or the one `--layers` names, with `--diameter` and `--drill` as `routes add-via` takes them) is judged by the design rules check that judges `routes add-via` and `routes sites`: the check of the placed board with the routes file's copper near the place, with the via and without it. A pour, plane or zone is left out, as its fill is made round a via, and the router's copper of `fab` is not on the board. With `--net` the via is of that net, so its own pads and copper are not in its way; with none it is of a net no pad is on, so it is judged against every net.\n\nEach error the via adds is a rule it breaks, listed with the thing in its way (`pad`, `track`, `via`, `hole`, `zone`, `outline`, `keep-out`, `mask`, `part`, or `process` for the via's own size against the process's limits), its net, and for a rule of a space the space required, the space measured and the shortfall, in mm: the figures of the check's own finding, not measured again. The check also reports the spaces the via keeps with no more room than the board's largest space: the smallest margins, the least room first, one for each rule and thing (the text lists the first 8). Warnings and notes the via adds (a via's hole or ring near a pad) are listed after them and do not make the place illegal; so is a note for each new island the via would cut a pour, a plane or a zone into, on a layer it passes, that holds pads or vias of the fill's net (with its layer, the fill, its net, a corner, its area and the pads and vias cut off), found on the board cut to a box 3 mm beyond the via's copper, filled with the via and without it as `board fill --islands` fills it.\n\nWith `--format json`: `board`, `at`, `net`, `via` (`diameter`, `drill`, `layers`), `legal`, `breaks` and `margins` (each with `rule`, `item`, `what`, `net`, `required`, `measured`, `shortfall` and `message`; a margin's shortfall is 0 or below, its room the negative), `notes`, `margin`, `pockets` (each with `layer`, `fill`, `plane`, `pour` or `zone`, `net`, `at`, `area`, `pads` and `vias` of `at`), `checks` and `fills`. Places are in the outline's frame, as the routes file's, and `--frame` moves the place printed (`--at` is always the outline's). The exit code is 0 when the via fits, 1 when a rule refuses it or the board has no such net, and 2 when it could not run.";

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
            match netsmith_fabricate::via_check::check_via(
                &board,
                &routes,
                args.at,
                args.net.as_deref(),
                args.diameter,
                args.drill,
                args.layers.as_ref(),
            ) {
                Ok(c) => Ok(Ok(c)),
                Err(lib::Refusal::Refused(e)) => Ok(Err(e)),
                Err(lib::Refusal::Failed(e)) => Err(format!("error: {e}\n")),
            }
        },
    );
    let checked = match done {
        Ok(Ok(c)) => c,
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
                    .print(&checked, own, Some(crate::frame::design_corner()), true)
            {
                return crate::frame::refused(&e);
            }
        }
        common::Format::Text => {
            let (by, says) = args.framing.text_move();
            print!("{}", checked.text_moved(by, &says));
        }
    }
    if checked.legal {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
