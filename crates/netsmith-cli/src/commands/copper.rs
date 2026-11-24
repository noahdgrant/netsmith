//! `netsmith board copper` (issue #1085): which layers of a board carry a plane or a pour, on
//! which net and with which settings, how many zones the routes file holds by hand, and the
//! teardrop fills, from the source with no placing and no routing. The data and its text are
//! `netsmith_pcb::CopperAreas`; the editor's PCB summary tab shows the same.

use std::process::ExitCode;

use clap::Args as ClapArgs;

use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`board copper` checks and solves the board as `check` does, places nothing, routes nothing, writes nothing, and says what its copper areas are, from the source alone: every copper layer from the top (`top`, `inner1`, `bottom`) with its use (`plane`: an inner layer that is one sheet on a net; `pour`: an outer layer poured on a net by the process, `process.top.net`; `signal`: a layer for tracks, an inner signal layer or an outer layer with no pour), its net, and for a pour its fields as the design states them (`stitch_pitch`, `thermal_gap`, `spoke_width`, `clearance`, `min_width`, `pads`; a field it does not state is `null` and the text says `default`). The zones of the routes file beside the source (`routes add-zone`, or imported from a KiCad board) are counted apart as hand zones, each with its net, layers, settings and area; one of the net of the plane or pour the process states on its layer restates it, which is then not drawn (`replaced_by_zones`), unless it is a patch of it (`patch = true`, `routes add-zone --patch`: the plane or pour is drawn and the zone holds its settings inside its outline, `patched_by_zones`), and one of another net cuts it: the zone has the area it covers and the plane or pour is drawn round it. Teardrop fills are counted apart from both: the router makes them when it routes the board and `fab` writes each as a zone of the KiCad board that is no pour, so the source holds none and `teardrops.counted` is false.\n\nWith `--format json` one JSON document is printed on stdout: `board`, `layers` (`layer`, `kind`, `net`, `pour`, `zones`, `replaced_by_zones`, `patched_by_zones`), `zones` (`net`, `layers`, `priority`, `clearance`, `min_width`, `pads`, `thermal_gap`, `spoke_width`, `islands`, `area`, `locked`, `patch`) and `teardrops` (`counted`, `total`, `layers`). The exit code is 0 when it is printed, 1 when the board does not build or the routes file cannot be read, and 2 when the command cannot run."
)]
pub struct Args {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// Only the board of this name (`Board`, or `System/Board` for a board of a system).
    #[arg(long, value_name = "BOARD")]
    pub board: Option<String>,
    /// Text for a person, or one JSON document on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

pub fn run(args: Args) -> ExitCode {
    let made = common::with_placed_board(
        args.file.as_deref(),
        args.board.as_deref(),
        None,
        |placed| {
            placed
                .with_built(|built| {
                    netsmith_fabricate::routes::copper_areas(
                        &built.design,
                        &placed.layout_of(built).file,
                    )
                })
                .and_then(|areas| areas.map_err(|e| format!("error: {e}\n")))
        },
    );
    match made {
        Ok(areas) => {
            match args.format {
                common::Format::Json => {
                    println!("{}", serde_json::to_string(&areas).expect("plain data"))
                }
                common::Format::Text => print!("{}", areas.text()),
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
    fn a_file_a_board_and_a_format() {
        let Ok(Leaf::Copper(a)) = parse(&[
            "board", "copper", "a.nsm", "--board", "B", "--format", "json",
        ]) else {
            panic!("copper should parse");
        };
        assert_eq!(a.file.as_deref(), Some("a.nsm"));
        assert_eq!(a.board.as_deref(), Some("B"));
        assert_eq!(a.format, common::Format::Json);
        let Ok(Leaf::Copper(a)) = parse(&["board", "copper"]) else {
            panic!("copper should parse");
        };
        assert_eq!(
            (a.file, a.board, a.format),
            (None, None, common::Format::Text)
        );
    }
}
