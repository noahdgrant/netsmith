//! `netsmith board resistance` (issue #1703): the DC resistance of a net's copper between two
//! pads or groups of pads, the voltage it drops at a current, the narrowest track and the vias the
//! current passes, and each piece's heat by the IPC-2221 rule of `track_current` and
//! `via_current`, the hottest first (`netsmith_fabricate::net_resistance`).
//!
//! A verb of its own and not a mode of `board nets`: `board nets` is a table of every net, and
//! this answers one question of one net between two places, with its own positionals (`NET FROM
//! TO`) and a current; it reads the copper as `board nets` does (`view pcb`'s, routed first with
//! `--route`) and its places are in the board file's frame, as `board nets`' are.

use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_fabricate::pcb_picture;

use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "`board resistance` measures the DC resistance of a net's copper between two ends, each a pad (`R1.1`) or a group of pads (`R1`, every pad of the part on the net; several, joined by commas), as the routed copper `board nets` reads stands (the routes file, the kept copper, and the router's with `--route`). The copper is a network of resistors solved as one, so a current shares between parallel ways as the copper shares it: each track at its drawn width in its layer's copper (`rho * length / (width * thickness)`, annealed copper at 20 °C, the track drop's figure; each leg into a pad measured to the pad's centre), and each via's barrel between the layers by its hole and the process's plating, the board's thickness between them. A pad joins what ends in it with no resistance, and a group of pads is one end at one voltage. A pour, a plane and a zone are not modelled (their copper is an area): the figure is of the tracks and vias alone, an upper bound where a pour also joins the ends, and the text says so. Nothing is written.\n\nIt prints the resistance, the voltage the current drops through it, the narrowest track the current passes and the vias it passes (a piece that carries 1 % of the current or more), and those pieces, the hottest first: each with its resistance, its share of the current, its current and drop, its temperature rise by IPC-2221 as `track_current` and `via_current` judge copper (a track at the narrowest finished width the tolerance allows), the width a track needs or the current a via carries at the board's allowed rise (`.process.max_temperature_rise`, 10 K unless stated), and `over` where it rises more. The current is `--amps`, or the net's one current (`board nets --currents`) when none is given, its heat then at the net's rms current where it states one (`heating` in the JSON); with neither, the resistance alone, the pieces by their share.\n\nWith `--format json`: `net`, `from` and `to` (the pads), `resistance` (ohms), `current` (A), `drop` (V), `allowed_rise` (K), `narrowest` (`width`, `layer`, `from`, `to`, `share`), `vias`, `segments` (each with `kind`, `track` or `via`, `layer`, `from`, `to`, `width` (a via's hole), `length`, `resistance`, `share`, `current`, `drop`, `rise`, `needs` or `carries`, `over`), `left_out` (the layers of each pour of the net not modelled), `iterations` (the solve's work) and `frame`. Places are in the board file's frame, as `board nets` has them. The exit code is 0 when it is printed, 1 when the board does not build or place, or the ends are no pads of the net or no copper joins them, and 2 when the command cannot run."
)]
pub struct Args {
    /// The net.
    #[arg(value_name = "NET")]
    pub net: String,
    /// One end: a pad (`R1.1`), a part's pads on the net (`R1`), or several, by commas.
    #[arg(
        value_name = "FROM",
        num_args = 1,
        value_delimiter = ',',
        required = true
    )]
    pub from: Vec<String>,
    /// The other end, as FROM.
    #[arg(
        value_name = "TO",
        num_args = 1,
        value_delimiter = ',',
        required = true
    )]
    pub to: Vec<String>,
    /// The current, in amperes (`2`, `500mA`); default the net's.
    #[arg(long, value_name = "A", value_parser = common::parse_amps)]
    pub amps: Option<f64>,
    #[command(flatten)]
    pub target: super::routes::Target,
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
    #[command(flatten)]
    pub framing: crate::frame::Framing,
}

pub fn run(args: Args) -> ExitCode {
    let route = super::pcb::route_of(args.route, args.route_timeout);
    let made = common::with_stage_events(|| {
        common::with_placed_board(
            args.target.file.as_deref(),
            args.target.board.as_deref(),
            Some(&args.placing),
            |placed| {
                placed.with_board(|built, board| {
                    let label = built.label();
                    let layout = placed.layout_of(built).file.clone();
                    let mut board = board.clone();
                    let say = |line: String| eprintln!("{label}: {line}");
                    pcb_picture::put_copper(&mut board, &layout, route.as_ref(), &say)
                        .map_err(|e| format!("error: {label}: {e}\n"))?;
                    Ok((
                        label.to_string(),
                        netsmith_fabricate::net_resistance::of_board(
                            &board, &args.net, &args.from, &args.to, args.amps,
                        ),
                    ))
                })?
            },
        )
    });
    let (label, measured) = match made {
        Ok(m) => m,
        Err(code) => return code,
    };
    let measured = match measured {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {label}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let own = crate::frame::Frame::BoardFile;
    let corner = Some(crate::frame::design_corner());
    match args.format {
        common::Format::Json => {
            if let Err(e) = args.framing.print(&measured, own, corner, true) {
                return crate::frame::refused(&e);
            }
        }
        common::Format::Text => {
            let (_, dx, dy) = match args.framing.moved(own, corner) {
                Ok(m) => m,
                Err(e) => return crate::frame::refused(&e),
            };
            println!("{label}:");
            print!("{}", measured.text_moved((dx, dy)));
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{Leaf, parse};

    #[test]
    fn the_ends_take_pads_parts_and_lists_and_the_current_takes_units() {
        let Ok(Leaf::Resistance(a)) = parse(&[
            "board",
            "resistance",
            "VIN",
            "J1.1",
            "U1,C3.2",
            "--amps",
            "500mA",
        ]) else {
            panic!("resistance should parse");
        };
        assert_eq!(a.net, "VIN");
        assert_eq!(a.from, ["J1.1"]);
        assert_eq!(a.to, ["U1", "C3.2"]);
        assert_eq!(a.amps, Some(0.5));
        assert!(parse(&["board", "resistance", "VIN", "J1.1", "U1", "--amps", "0"]).is_err());
    }
}
