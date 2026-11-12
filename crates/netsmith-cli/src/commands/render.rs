use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args as ClapArgs, ValueEnum};

use super::common::{Ids, ViewName, parse_ids, with_placed_board};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`render` places a board as `board3d` does and draws it to a PNG with netsmith's own software rasterizer (ADR 0105): the same source and options give the same bytes, with no GPU, no browser and no KiCad. docs/BOARD3D.md, \"Pictures for agents\", has the frame and what is drawn."
)]
pub struct Args {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The board, when the file has several (`Name`, or `System/member`).
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// Where the camera looks from.
    #[arg(long, value_enum, default_value = "iso")]
    pub view: ViewName,
    /// The PNG to write (default `<board>-<view>.png`).
    ///
    /// The path written is printed.
    #[arg(long, value_name = "FILE.PNG")]
    pub out: Option<PathBuf>,
    /// The picture's size in pixels, 64 to 8192 a side.
    #[arg(long, value_name = "WxH", default_value = "1600x1000", value_parser = netsmith::render::parse_size)]
    pub size: (usize, usize),
    /// Cut the scene at a position in mm, such as `x=12.5`, and keep the far side.
    ///
    /// The position is in the scene's frame: y up from the outline's bottom-left corner, z up from
    /// the board's top face. The cut faces are filled as a section of the stackup, each layer at
    /// its true thickness in its own colour, unless `--no-caps` leaves them open.
    #[arg(long, value_name = "AXIS=MM", value_parser = netsmith::render::parse_section)]
    pub section: Option<(netsmith_3d::mesh::Axis, f64)>,
    /// Light a part, `net:<name>` or `limit:<name>` and dim the rest (comma-separated or repeated).
    ///
    /// A part is named by its designator or its path. An id the board does not have is an error
    /// that lists the ones it has.
    #[arg(long, value_name = "ID[,ID...]", value_parser = parse_ids)]
    pub highlight: Vec<Ids>,
    /// Draw everything see-through.
    #[arg(long)]
    pub xray: bool,
    /// Leave the cut faces of a section open.
    #[arg(long)]
    pub no_caps: bool,
    /// The picture's colours.
    #[arg(long, value_enum, default_value = "light")]
    pub theme: Theme,
    /// The parallel projection (the default).
    #[arg(long, conflicts_with = "perspective")]
    pub ortho: bool,
    /// The perspective projection.
    #[arg(long)]
    pub perspective: bool,
}

impl Args {
    pub fn request(&self) -> netsmith::render::Request {
        netsmith::render::Request {
            view: self.view.raster(),
            size: self.size,
            section: self.section,
            highlight: self
                .highlight
                .iter()
                .flat_map(|ids| ids.0.clone())
                .collect(),
            xray: self.xray,
            caps: !self.no_caps,
            dark: self.theme == Theme::Dark,
            perspective: self.perspective,
        }
    }
}

/// Draws one board in 3D and writes it as a PNG.
pub fn run(args: Args) -> ExitCode {
    run_render(
        args.file.as_deref(),
        args.board.as_deref(),
        args.out.as_deref(),
        &args.request(),
    )
}

/// `netsmith render`: one board placed as `board3d` places it, drawn by netsmith's own
/// rasterizer and written as a PNG (`<board>-<view>.png` unless `--out` says a file); the path
/// written is printed. Exit 0 when the picture is written, 1 when the board does not build or
/// place, 2 when the command cannot start, an id of `--highlight` is not the board's, or the file
/// cannot be written.
fn run_render(
    file: Option<&str>,
    board: Option<&str>,
    out: Option<&Path>,
    request: &netsmith::render::Request,
) -> ExitCode {
    let (scene, label) = match with_placed_board(file, board, |placed| placed.scene_and_label()) {
        Ok(made) => made,
        Err(code) => return code,
    };
    let png = match netsmith::render::draw(&scene, request) {
        Ok(png) => png,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let out = out.map_or_else(
        || netsmith::render::default_out(&label, request.view),
        Path::to_path_buf,
    );
    if let Err(e) = fs::write(&out, png) {
        eprintln!("error: {}: {e}", out.display());
        return ExitCode::from(2);
    }
    println!("{}", out.display());
    ExitCode::SUCCESS
}

/// `render --theme`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Theme {
    Light,
    Dark,
}

#[cfg(test)]
mod tests {
    use crate::commands::Command;
    use crate::commands::common::testing::{kind, parse};
    use clap::error::ErrorKind;

    fn render(args: &[&str]) -> netsmith::render::Request {
        let mut line = vec!["render"];
        line.extend_from_slice(args);
        match parse(&line) {
            Ok(Command::Render(r)) => r.request(),
            other => panic!("{args:?}: {other:?}"),
        }
    }

    #[test]
    fn render_has_the_defaults_of_a_request_and_takes_its_flags() {
        assert_eq!(render(&[]), netsmith::render::Request::default());
        let r = render(&[
            "--view",
            "top",
            "--size",
            "64x80",
            "--section",
            "z=-0.8",
            "--highlight",
            "J1,net:GND",
            "--highlight",
            "limit:lid",
            "--xray",
            "--no-caps",
            "--theme",
            "dark",
            "--perspective",
        ]);
        assert_eq!(r.view, netsmith_3d::raster3d::View::Top);
        assert_eq!(r.size, (64, 80));
        assert_eq!(r.section, Some((netsmith_3d::mesh::Axis::Z, -0.8)));
        assert_eq!(r.highlight, vec!["J1", "net:GND", "limit:lid"]);
        assert!(r.xray && !r.caps && r.dark && r.perspective);
        assert!(!render(&["--ortho"]).perspective);
        assert_eq!(
            kind(&["render", "--ortho", "--perspective"]),
            ErrorKind::ArgumentConflict
        );
        let e = parse(&["render", "--view", "sideways"]).unwrap_err();
        assert!(e.contains("iso-bottom"), "{e}");
        let e = parse(&["render", "--size", "10x10"]).unwrap_err();
        assert!(e.contains("--size"), "{e}");
    }
}
