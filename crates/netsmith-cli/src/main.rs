//! `netsmith --version`, `--help`, `init`, `build`, `fab`, `route`, `editor`, `board3d`, `render`, `view` and `compare`.

use std::{
    env, fs,
    net::{Ipv4Addr, TcpListener},
    path::Path,
    process::ExitCode,
    sync::Arc,
    time::Duration,
};

use netsmith::cli::{
    Board3dArgs, BuildArgs, CatalogAddArgs, CatalogAgeArgs, CatalogCommand, CheckArgs, Command,
    CompareArgs, DxfCommand, DxfImportArgs, EditorArgs, FabArgs, InitArgs, KicadLibsCommand,
    KicadLibsImportArgs, LockMode, OrderRequest, ParityArgs, RefreshRequest, RouteArgs, parse_args,
};
use netsmith::compare::{Request, Verdict, compare_files};
use netsmith::editor::Workspace;
use netsmith::fab::{FabOptions, fab_detailed};
use netsmith::findings::{BoardOut, Finding, Format, NotesMode, Report, Severity};
use netsmith::init::init;
use netsmith::order::OrderLocks;
use netsmith::project::{Project, Target};
use netsmith::supply::Supply;
use netsmith::{Libraries, outcome_with, render_problems};
use netsmith_check::Loader;
use netsmith_core::timings;
use netsmith_fab::{KicadCli, KicadCliExe};
use netsmith_solve::Catalogs;

fn main() -> ExitCode {
    // A mistake exits 2, netsmith's "cannot run"; help and the version exit 0.
    let command = parse_args(env::args_os().skip(1)).unwrap_or_else(|e| e.exit());
    match command {
        Command::Version => {
            println!("netsmith {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Command::Init(InitArgs { name }) => run_init(name.as_deref()),
        Command::Build(BuildArgs {
            file,
            out,
            strict,
            locked,
            format,
            notes,
        }) => timings::stage("build.total", "", || {
            // The pin file first: a request must have its pin before anything is built, and
            // `--locked` stops here before the pick lock or any file is written.
            if let Some(code) = sync_pins(file.as_deref(), locked) {
                return code;
            }
            run_build(&BuildRun {
                file: file.as_deref(),
                out: out.as_deref(),
                strict,
                locked,
                format,
                notes,
                write: true,
            })
        }),
        // `check` writes nothing, not even the pin file: a request without a pin is given one in
        // memory, as the editor does.
        Command::Check(CheckArgs {
            file,
            strict,
            format,
            notes,
        }) => run_build(&BuildRun {
            file: file.as_deref(),
            out: None,
            strict,
            locked: false,
            format,
            notes,
            write: false,
        }),
        Command::Parity(ParityArgs {
            board,
            table,
            map,
            svg,
            format,
        }) => run_parity(
            &board,
            table.as_deref(),
            map.as_deref(),
            svg.as_deref(),
            format,
        ),
        Command::Fab(args) => {
            let (constraints_warn, pins_locked, order) =
                (args.constraints_warn(), args.pins_locked(), args.order());
            let FabArgs {
                file,
                out,
                route_timeout,
                allow_drc_violations,
                cross_check,
                strict,
                locked,
                no_glb,
                save_placed,
                quiet,
                format,
                notes,
                ..
            } = args;
            if order.is_some() && format == Format::Json {
                eprintln!(
                    "error: `fab --order` prints text only; `--format json` is for `fab` of a file or of the project's boards"
                );
                return ExitCode::from(2);
            }
            if let Some(code) = sync_pins(file.as_deref(), locked) {
                return code;
            }
            match order {
                Some(request) => run_order(
                    &request,
                    out.as_deref(),
                    route_timeout,
                    allow_drc_violations,
                    cross_check,
                    strict,
                    locked,
                    constraints_warn,
                    no_glb,
                ),
                None => timings::stage("fab.total", "", || {
                    run_fab(
                        file.as_deref(),
                        out.as_deref(),
                        route_timeout,
                        allow_drc_violations,
                        cross_check,
                        strict,
                        locked,
                        constraints_warn,
                        pins_locked,
                        no_glb,
                        save_placed,
                        FabOutput {
                            quiet,
                            format,
                            notes,
                        },
                    )
                }),
            }
        }
        Command::Route(RouteArgs {
            file,
            out,
            route_timeout,
            margin,
        }) => run_route(&file, out.as_deref(), route_timeout, margin),
        Command::Lock(args) => run_lock(args.file.as_deref(), args.mode()),
        Command::Editor(EditorArgs { file, port }) => run_editor(file.as_deref(), port),
        Command::Board3d(Board3dArgs {
            file,
            board,
            json,
            glb,
            step,
        }) => run_board3d(
            file.as_deref(),
            board.as_deref(),
            json,
            glb.as_deref(),
            step.as_deref(),
        ),
        Command::Render(args) => run_render(
            args.file.as_deref(),
            args.board.as_deref(),
            args.out.as_deref(),
            &args.request(),
        ),
        Command::View(args) => match args.request() {
            Ok(request) => run_view(&request),
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::from(2)
            }
        },
        Command::Catalog(CatalogCommand::Refresh(request)) => run_catalog_refresh(&request),
        Command::Catalog(CatalogCommand::Age(CatalogAgeArgs { paths, only, days })) => {
            run_catalog_age(&paths, &only, days)
        }
        Command::Catalog(CatalogCommand::Add(CatalogAddArgs {
            lcsc,
            to,
            offline,
            no_cache,
        })) => run_catalog_add(&lcsc, to.as_deref(), offline, no_cache),
        Command::Dxf(DxfCommand::Import(DxfImportArgs {
            file,
            layers,
            holes_layers,
            units,
            board,
            write,
        })) => run_dxf_import(
            &file,
            &netsmith::dxf::Request {
                layers,
                holes_layers,
                units,
            },
            board.as_deref(),
            write.as_deref(),
        ),
        Command::KicadLibs(KicadLibsCommand::Import(KicadLibsImportArgs {
            sym_lib_table,
            fp_lib_table,
        })) => run_kicad_libs_import(&sym_lib_table, fp_lib_table.as_deref()),
        Command::Compare(CompareArgs {
            a,
            b,
            board,
            map,
            by_refdes,
            ignore_unconnected,
            loose_values,
            format,
        }) => run_compare(
            &Request {
                a: &a,
                b: &b,
                board: board.as_deref(),
                map: map.as_deref(),
                by_refdes,
                ignore_unconnected,
                loose_values,
            },
            format,
        ),
    }
}

