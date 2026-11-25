//! One file per command. A command's file holds its arguments and its `run`; its doc comments
//! are its help. The commands are grouped under nouns, each with its verbs (issue #1090): a
//! group is an enum here, and `netsmith <noun> <verb>` is a variant of it. `moved` names the
//! new spelling of a command or flag of an earlier release.

mod board3d;
mod bom;
mod build;
mod case;
mod catalog;
mod catalog_search;
mod check;
pub(crate) mod common;
mod compare;
mod copper;
mod drc;
mod dxf;
mod editor;
mod explain;
mod fab;
mod fill;
mod fmt;
mod footprint;
mod gerber;
mod init;
mod kicad_libs;
mod layout;
mod lint;
mod lock;
mod mcp;
mod migrate_offers;
pub mod moved;
mod nets;
mod order;
mod panel;
mod parity;
mod part;
mod pcb;
mod pins;
mod port;
mod power;
mod project_show;
mod render;
mod route;
mod routes;
mod routes_check_via;
mod routes_move_via;
mod routes_sites;
mod rules;
mod schematic;
mod schematic_layout;
mod show;
mod swap;
mod system;
mod view;
mod views;

use std::process::ExitCode;

use clap::Subcommand;

/// What `netsmith --help` ends with: the loop of a day's work in the order it is done, and where
/// the guide and the skills are (issue #1099; the skill `netsmith-use-netsmith` is the longer map).
pub const AFTER_HELP: &str = "A typical workflow:
  netsmith project init            make a project here (netsmith.toml, src/main.nsm)
  netsmith board check             check and solve the boards, writing nothing; fix the first error, run again
  netsmith board build             also write the schematic, the pick lock and the pin file
  netsmith view schematic          look: the schematic as a picture (also view 3d, view pcb, view gerber)
  netsmith view editor             edit the source and see the boards in a browser
  netsmith board fab               place, route, check the design rules and write the fab package
  netsmith project lock --check    in CI: the picks are the ones committed
Then read fab/<Board>/fab-notes.txt before you order.

In a project, `.agents/skills/` has the netsmith skills (start with `netsmith-use-netsmith`, the map) and
`docs/netsmith/LANGUAGE.md` is the language guide. `netsmith <noun> --help` lists the verbs of a noun.
Exit codes: 0 done, 1 what it checked failed, 2 could not run. `--format json` is output for a program.
NETSMITH_TIMINGS=1 prints the time of each stage of a run to stderr.";

/// The exit code of every command is 0 when it did what it was asked, 1 when what it checked
/// failed, and 2 when it could not run (a usage mistake included).
#[derive(Debug, PartialEq, Subcommand)]
pub enum Command {
    /// The project: make one, refresh its agent files, lock its picks, make an order.
    #[command(subcommand)]
    Project(Project),
    /// A board: check, build, fab, explain a pick, compare two designs.
    #[command(subcommand)]
    Board(Board),
    /// Show, set, move or check the entries of a board's layout file (where each part is).
    Layout(layout::Args),
    /// Fix copper of a board in its routes file: import it from KiCad, add, delete, lock, show.
    Routes(routes::Args),
    /// Show, set or check the entries of the schematic layout file (how the schematic is arranged), and arrange its sheets.
    Schematic(schematic_layout::Args),
    /// Look at a board: draw it in 3D, serve the editor, drive the editor's 3D view.
    #[command(subcommand)]
    View(View),
    /// Check, list, add or search for catalog rows against a supplier's record.
    Catalog(catalog::Args),
    /// Bring something from outside into a project: a drawing, KiCad's libraries, a part, a port, a case.
    #[command(subcommand)]
    Import(Import),
    /// Tools to develop netsmith with: route a saved board, a port's parity, the 3D scene's report.
    #[command(subcommand)]
    Dev(Dev),
    /// Serve these commands to an agent host over the Model Context Protocol, on stdin and stdout.
    Mcp(mcp::Args),
}

/// `netsmith project ...`.
#[derive(Debug, PartialEq, Subcommand)]
pub enum Project {
    /// Scaffold a project in the current folder.
    Init(init::Args),
    /// Bring the project's agent files (AGENTS.md, the skills, the language guide) up to this netsmith.
    Refresh(init::RefreshArgs),
    /// Refresh the pick lock, or check that it is current.
    Lock(lock::Args),
    /// Report what is worth changing in the sources: unused imports and definitions, redundant statements, names.
    Lint(lint::Args),
    /// Rewrite the sources in the one layout of the language, or check that they are in it.
    Fmt(fmt::Args),
    /// Move the catalogs' lcsc and library columns into offers files, and `.lcsc &=` hand picks to `.mpn &=` (ADR 0143).
    MigrateOffers(migrate_offers::Args),
    /// Make one of the project's orders, or list them with no name (ADR 0035).
    Order(order::Args),
    /// Print the project's boards, systems, panels, modules, files and orders as the editor's tree shows them.
    Show(project_show::Args),
}

/// `netsmith board ...`.
#[derive(Debug, PartialEq, Subcommand)]
pub enum Board {
    /// Check and solve the boards, write nothing, and print the findings.
    Check(check::Args),
    /// Check, solve and write a KiCad schematic of the boards.
    Build(build::Args),
    /// Make the fab package of the boards: Gerbers, drill, BOM, CPL, notes, 3D scene.
    Fab(fab::Args),
    /// Run the design rules check alone, on the placed board with its routes file or on a KiCad board.
    Drc(drc::Args),
    /// Print the rules the design rules check holds the board to: clearances, widths per net, mask, holes, vias, rule areas, classes, layers.
    Rules(rules::Args),
    /// Print what the solve left a part or member with, and why each part has its row.
    Explain(explain::Args),
    /// Say whether two designs are the same circuit.
    Compare(compare::Args),
    /// Show, lock, unlock and move the pins that the board's pin requests got, or apply a proposal.
    Pins(pins::Args),
    /// List the alternatives of a part no catalog row stands for, with their checks, or write one into the source.
    Swap(swap::Args),
    /// Print a board's parts and nets as the editor's inspector shows them: solved values, pins, pick lock.
    Show(show::Args),
    /// Print a board's bill of materials with price and stock.
    Bom(bom::Args),
    /// Print a board's power tree: rails, supplies, loads and what each leaves.
    Power(power::Args),
    /// Print a system's boards and mates, or a board's members and nets.
    System(system::Args),
    /// Print a panel: its grid, size and checks.
    Panel(panel::Args),
    /// Say which layers carry a plane or a pour, the hand zones of the routes file and the teardrop fills.
    Copper(copper::Args),
    /// Fill the pours as `fab` does and say how each pad of their nets is joined: solid, by how many thermal spokes, or not, and the copper islands.
    Fill(fill::Args),
    /// Print each net's routed and straight length, vias, layer changes and layers, and each diff pair's lengths, skew, coupled length, gaps and vias side by side.
    Nets(nets::Args),
}

/// `netsmith view ...`.
#[derive(Debug, PartialEq, Subcommand)]
pub enum View {
    /// Draw a board in 3D to a PNG.
    #[command(name = "3d")]
    ThreeD(render::Args),
    /// Edit the source and see the boards in a browser.
    Editor(editor::Args),
    /// Drive the 3D view of a running editor with `view 3d`'s flags, and save its picture.
    #[command(name = "editor-3d")]
    Editor3d(view::Args),
    /// Write a board's schematic as a PDF (a page for each sheet), an SVG or a PNG.
    Schematic(schematic::Args),
    /// Draw a board's copper in 2D, a layer or all, a region or the whole, with the open connections, to a PNG or an SVG.
    Pcb(pcb::Args),
    /// Draw the Gerber and drill files of a fab package to a PNG or an SVG, or sum them up.
    Gerber(gerber::Args),
}

/// `netsmith import ...`.
#[derive(Debug, PartialEq, Subcommand)]
pub enum Import {
    /// Print an enclosure's DXF drawing as the source of a board's outline, cutouts and holes.
    Dxf(dxf::ImportArgs),
    /// Print the stanzas of KiCad library tables for kicad.toml and netsmith.toml.
    #[command(name = "kicad-libs")]
    KicadLibs(kicad_libs::Args),
    /// Write a footprint from a KiCad board (a part's, by its reference) or footprint file into a library of the project.
    Footprint(footprint::Args),
    /// Draft a part file from a KiCad symbol's pins.
    Part(part::Args),
    /// Start a port of a KiCad board (its first source and its parts file), or derive its tables.
    Port(port::Args),
    /// A case model: the board it holds from its STEP model (`model`), its pose from a matrix (`place`).
    Case(case::Args),
}

/// `netsmith dev ...`.
#[derive(Debug, PartialEq, Subcommand)]
pub enum Dev {
    /// Route a placed board that `board fab --save-placed` wrote, alone, with the router's tuning flags.
    Route(route::Args),
    /// Compare a fab board with the board of a port's original, part by part.
    Parity(parity::Args),
    /// Print a board's 3D scene: heights, limits and overlaps.
    #[command(name = "board3d")]
    Board3d(board3d::Args),
}

pub fn run(command: Command) -> ExitCode {
    match command {
        Command::Project(command) => match command {
            Project::Init(args) => init::run(args),
            Project::Refresh(args) => init::run_refresh(args),
            Project::Lock(args) => lock::run(args),
            Project::Lint(args) => lint::run(args),
            Project::Fmt(args) => fmt::run(args),
            Project::MigrateOffers(args) => migrate_offers::run(args),
            Project::Order(args) => order::run(args),
            Project::Show(args) => project_show::run(args),
        },
        Command::Board(command) => match command {
            Board::Check(args) => check::run(args),
            Board::Build(args) => build::run(args),
            Board::Fab(args) => fab::run(args),
            Board::Drc(args) => drc::run(args),
            Board::Rules(args) => rules::run(args),
            Board::Explain(args) => explain::run(args),
            Board::Compare(args) => compare::run(args),
            Board::Pins(args) => pins::run(args),
            Board::Swap(args) => swap::run(args),
            Board::Show(args) => show::run(args),
            Board::Bom(args) => bom::run(args),
            Board::Power(args) => power::run(args),
            Board::System(args) => system::run(args),
            Board::Panel(args) => panel::run(args),
            Board::Copper(args) => copper::run(args),
            Board::Fill(args) => fill::run(args),
            Board::Nets(args) => nets::run(args),
        },
        Command::Layout(args) => layout::run(args),
        Command::Routes(args) => routes::run(args),
        Command::Schematic(args) => schematic_layout::run(args),
        Command::View(command) => match command {
            View::ThreeD(args) => render::run(args),
            View::Editor(args) => editor::run(args),
            View::Editor3d(args) => view::run(args),
            View::Schematic(args) => schematic::run(args),
            View::Pcb(args) => pcb::run(args),
            View::Gerber(args) => gerber::run(args),
        },
        Command::Catalog(args) => catalog::run(args),
        Command::Import(command) => match command {
            Import::Dxf(args) => dxf::run(args),
            Import::KicadLibs(args) => kicad_libs::run(args),
            Import::Footprint(args) => footprint::run(args),
            Import::Part(args) => part::run(args),
            Import::Port(args) => port::run(args),
            Import::Case(args) => case::run(args),
        },
        Command::Dev(command) => match command {
            Dev::Route(args) => route::run(args),
            Dev::Parity(args) => parity::run(args),
            Dev::Board3d(args) => board3d::run(args),
        },
        Command::Mcp(args) => mcp::run(args),
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{Cli, kind, parse_args};
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
            &["help", "board"],
            &["help", "board", "fab"],
            &["board", "--help"],
            &["board", "build", "--help"],
            &["project", "order", "-h"],
            &["catalog", "refresh", "--help"],
            &["import", "dxf", "-h"],
            &["import", "case", "place", "--help"],
            &["import", "case", "model", "--help"],
            &["import", "kicad-libs", "--help"],
            &["import", "footprint", "--help"],
            &["view", "3d", "--help"],
            &["dev", "--help"],
        ] {
            assert_eq!(kind(args), ErrorKind::DisplayHelp, "{args:?}");
        }
        // No command at all is the help, as a mistake (exit 2), and so is a noun with no verb.
        for args in [&[][..], &["board"], &["view"], &["project"]] {
            assert_eq!(
                kind(args),
                ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand,
                "{args:?}"
            );
        }
    }

    #[test]
    fn an_unknown_command_is_an_error() {
        assert_eq!(kind(&["frobnicate"]), ErrorKind::InvalidSubcommand);
        assert!(parse_args(&["frobnicate", "--help"]).is_err());
        assert_eq!(kind(&["board", "frobnicate"]), ErrorKind::InvalidSubcommand);
    }
}
