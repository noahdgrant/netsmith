//! One file per command. A command's file holds its arguments and its `run`; its doc comments
//! are its help.

mod board3d;
mod build;
mod catalog;
mod check;
pub(crate) mod common;
mod compare;
mod dxf;
mod editor;
mod fab;
mod init;
mod kicad_libs;
mod lock;
mod parity;
mod port;
mod render;
mod route;
mod view;

use std::process::ExitCode;

use clap::Subcommand;

/// The exit code of every command is 0 when it did what it was asked, 1 when what it checked
/// failed, and 2 when it could not run (a usage mistake included).
#[derive(Debug, PartialEq, Subcommand)]
pub enum Command {
    /// Scaffold a project in the current folder.
    Init(init::Args),
    /// Check, solve and write a KiCad schematic of the boards.
    Build(build::Args),
    /// Check and solve the boards, write nothing, and print the findings.
    Check(check::Args),
    /// Make the fab package of the boards (Gerbers, drill, BOM, CPL, notes, 3D scene), or an order.
    Fab(fab::Args),
    /// Route a placed board that `fab --save-placed` wrote, alone.
    Route(route::Args),
    /// Refresh the pick lock, or check that it is current.
    Lock(lock::Args),
    /// Edit the source and see the boards in a browser.
    Editor(editor::Args),
    /// Print a board's 3D scene: heights, limits and overlaps.
    #[command(name = "board3d")]
    Board3d(board3d::Args),
    /// Draw a board in 3D to a PNG.
    Render(render::Args),
    /// Drive the 3D view of a running editor with `render`'s flags, and save its picture.
    View(view::Args),
    /// Say whether two designs are the same circuit.
    Compare(compare::Args),
    /// Compare a fab board with the board of a port's original, part by part.
    Parity(parity::Args),
    /// Import KiCad library tables.
    #[command(name = "kicad-libs")]
    KicadLibs(kicad_libs::Args),
    /// Check, list or add catalog rows against LCSC's record.
    Catalog(catalog::Args),
    /// Import an enclosure's DXF drawing.
    Dxf(dxf::Args),
    /// Start a port of a KiCad board: its first source and its parts file.
    Port(port::Args),
}

pub fn run(command: Command) -> ExitCode {
    match command {
        Command::Init(args) => init::run(args),
        Command::Build(args) => build::run(args),
        Command::Check(args) => check::run(args),
        Command::Fab(args) => fab::run(args),
        Command::Route(args) => route::run(args),
        Command::Lock(args) => lock::run(args),
        Command::Editor(args) => editor::run(args),
        Command::Board3d(args) => board3d::run(args),
        Command::Render(args) => render::run(args),
        Command::View(args) => view::run(args),
        Command::Compare(args) => compare::run(args),
        Command::Parity(args) => parity::run(args),
        Command::KicadLibs(args) => kicad_libs::run(args),
        Command::Catalog(args) => catalog::run(args),
        Command::Dxf(args) => dxf::run(args),
        Command::Port(args) => port::run(args),
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{Cli, kind, parse};
    use clap::CommandFactory;
    use clap::error::ErrorKind;

    #[test]
    fn the_model_is_sound() {
        Cli::command().debug_assert();
    }

    #[test]
    fn the_version_is_clap_s_flag() {
        assert_eq!(kind(&["--version"]), ErrorKind::DisplayVersion);
        assert_eq!(kind(&["-V"]), ErrorKind::DisplayVersion);
    }

    #[test]
    fn help_is_asked_for_by_flag_command_or_after_a_command() {
        for args in [
            &["--help"][..],
            &["-h"],
            &["help"],
            &["help", "fab"],
            &["build", "--help"],
            &["fab", "--order", "-h"],
            &["catalog", "refresh", "--help"],
            &["dxf", "-h"],
            &["kicad-libs", "--help"],
        ] {
            assert_eq!(kind(args), ErrorKind::DisplayHelp, "{args:?}");
        }
        // No command at all is the help, as a mistake (exit 2).
        assert_eq!(
            kind(&[]),
            ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }

    #[test]
    fn an_unknown_command_is_an_error() {
        assert_eq!(kind(&["frobnicate"]), ErrorKind::InvalidSubcommand);
        assert!(parse(&["frobnicate", "--help"]).is_err());
    }
}
