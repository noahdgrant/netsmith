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
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::error::ErrorKind;
    use clap::{CommandFactory, Parser};

    use super::*;

    /// The command line as main.rs has it.
    #[derive(Debug, Parser)]
    #[command(name = "netsmith", version)]
    struct Cli {
        #[command(subcommand)]
        command: Command,
    }

    fn parse_args(args: &[&str]) -> Result<Command, clap::Error> {
        Cli::try_parse_from(std::iter::once(&"netsmith").chain(args)).map(|cli| cli.command)
    }

    fn parse(args: &[&str]) -> Result<Command, String> {
        parse_args(args).map_err(|e| e.to_string())
    }

    fn kind(args: &[&str]) -> ErrorKind {
        parse_args(args).unwrap_err().kind()
    }

    fn output() -> common::Output {
        common::Output {
            format: common::Format::Text,
            notes: common::Notes::All,
        }
    }

    /// The error of a `view` line: clap's, or the one its request gives.
    fn view_err(line: &[&str]) -> String {
        match parse(line) {
            Err(e) => e,
            Ok(Command::View(v)) => v.request().unwrap_err(),
            Ok(other) => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_model_is_sound() {
        Cli::command().debug_assert();
    }

    fn build(file: Option<&str>) -> build::Args {
        build::Args {
            file: file.map(str::to_string),
            out: None,
            strict: false,
            locked: false,
            output: output(),
        }
    }

    fn fab(args: &[&str]) -> fab::Args {
        let mut line = vec!["fab"];
        line.extend_from_slice(args);
        match parse(&line) {
            Ok(Command::Fab(fab)) => fab,
            other => panic!("{args:?}: {other:?}"),
        }
    }

    #[test]
    fn catalog_add_takes_a_number_and_a_file() {
        assert_eq!(
            parse(&[
                "catalog",
                "add",
                "--lcsc",
                "C2286",
                "--to",
                "catalogs/led.csv"
            ]),
            Ok(Command::Catalog(catalog::Args {
                command: catalog::Command::Add(catalog::AddArgs {
                    lcsc: "C2286".into(),
                    to: Some(PathBuf::from("catalogs/led.csv")),
                    offline: false,
                    no_cache: false,
                })
            }))
        );
        assert!(matches!(
            parse(&[
                "catalog",
                "add",
                "--lcsc",
                "C2286",
                "--offline",
                "--no-cache"
            ]),
            Ok(Command::Catalog(catalog::Args {
                command: catalog::Command::Add(catalog::AddArgs {
                    to: None,
                    offline: true,
                    no_cache: true,
                    ..
                })
            }))
        ));
        let e = parse(&["catalog", "add", "--to", "x.csv"]).unwrap_err();
        assert!(e.contains("--lcsc <C123>"), "{e}");
        assert_eq!(
            kind(&["catalog", "add", "--lcsc", "C1", "--dry-run"]),
            ErrorKind::UnknownArgument
        );
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

    #[test]
    fn dxf_import_takes_a_file_layers_units_a_board_and_a_source_to_write() {
        assert_eq!(
            parse(&[
                "dxf",
                "import",
                "case.dxf",
                "--layer",
                "EDGE",
                "--layer",
                "RIM",
                "--holes-layer",
                "HOLES",
                "--units",
                "inch",
                "--board",
                "Case",
                "--write",
                "case.nsm"
            ]),
            Ok(Command::Dxf(dxf::Args {
                command: dxf::Command::Import(dxf::ImportArgs {
                    file: "case.dxf".into(),
                    layers: vec!["EDGE".into(), "RIM".into()],
                    holes_layers: vec!["HOLES".into()],
                    units: Some(netsmith_dxf::Units::Inches),
                    board: Some("Case".into()),
                    write: Some("case.nsm".into()),
                })
            }))
        );
        assert_eq!(kind(&["dxf", "import"]), ErrorKind::MissingRequiredArgument);
        let e = parse(&["dxf", "import", "a.dxf", "--units", "furlongs"]).unwrap_err();
        assert!(e.contains("not `furlongs`"), "{e}");
        assert!(parse(&["dxf", "export", "a.dxf"]).is_err());
        assert!(parse(&["dxf", "import", "a.dxf", "--bogus"]).is_err());
    }

    #[test]
    fn kicad_libs_import_takes_one_or_two_library_tables() {
        assert_eq!(
            parse(&["kicad-libs", "import", "sym-lib-table"]),
            Ok(Command::KicadLibs(kicad_libs::Args {
                command: kicad_libs::Command::Import(kicad_libs::ImportArgs {
                    sym_lib_table: "sym-lib-table".into(),
                    fp_lib_table: None
                })
            }))
        );
        assert_eq!(
            parse(&["kicad-libs", "import", "s", "f"]),
            Ok(Command::KicadLibs(kicad_libs::Args {
                command: kicad_libs::Command::Import(kicad_libs::ImportArgs {
                    sym_lib_table: "s".into(),
                    fp_lib_table: Some("f".into())
                })
            }))
        );
        assert!(parse(&["kicad-libs"]).is_err());
        assert!(parse(&["kicad-libs", "export", "s"]).is_err());
        assert!(parse(&["kicad-libs", "import", "s", "f", "g"]).is_err());
    }

    #[test]
    fn catalog_refresh_takes_paths_and_the_flags_of_a_polite_opt_in_run() {
        assert_eq!(
            parse(&["catalog", "refresh"]),
            Ok(Command::Catalog(catalog::Args {
                command: catalog::Command::Refresh(catalog::RefreshArgs::default())
            }))
        );
        assert_eq!(
            parse(&[
                "catalog",
                "refresh",
                "catalogs/a.csv",
                "data",
                "--dry-run",
                "--only",
                "ldo",
                "--only",
                "led",
                "--lcsc",
                "C5446",
                "--delay",
                "1500",
                "--cache-ttl",
                "6",
                "--no-cache"
            ]),
            Ok(Command::Catalog(catalog::Args {
                command: catalog::Command::Refresh(catalog::RefreshArgs {
                    paths: vec!["catalogs/a.csv".into(), "data".into()],
                    dry_run: true,
                    offline: false,
                    only: vec!["ldo".into(), "led".into()],
                    lcsc: vec!["C5446".into()],
                    delay_ms: Some(1500),
                    cache_ttl_hours: Some(6),
                    no_cache: true,
                })
            }))
        );
        let Ok(Command::Catalog(catalog::Args {
            command: catalog::Command::Refresh(r),
        })) = parse(&["catalog", "refresh", "--offline"])
        else {
            panic!()
        };
        assert!(r.offline);
        let e = parse(&["catalog", "refresh", "--delay", "soon"]).unwrap_err();
        assert!(e.contains("--delay") && e.contains("soon"), "{e}");
        // --offline asks nothing, so the pace of the asking means nothing with it.
        for pace in ["--delay", "--cache-ttl"] {
            assert_eq!(
                kind(&["catalog", "refresh", "--offline", pace, "9"]),
                ErrorKind::ArgumentConflict
            );
        }
        assert_eq!(
            kind(&["catalog", "refresh", "--days", "3"]),
            ErrorKind::UnknownArgument
        );
        assert!(parse(&["catalog", "refresh", "--lcsc"]).is_err());
    }

    #[test]
    fn catalog_age_takes_paths_a_catalog_and_the_days_with_90_as_the_default() {
        assert_eq!(
            parse(&["catalog", "age"]),
            Ok(Command::Catalog(catalog::Args {
                command: catalog::Command::Age(catalog::AgeArgs {
                    paths: vec![],
                    only: vec![],
                    days: 90
                })
            }))
        );
        assert_eq!(
            parse(&["catalog", "age", "x.csv", "--days", "30", "--only", "ldo"]),
            Ok(Command::Catalog(catalog::Args {
                command: catalog::Command::Age(catalog::AgeArgs {
                    paths: vec!["x.csv".into()],
                    only: vec!["ldo".into()],
                    days: 30
                })
            }))
        );
        assert_eq!(
            kind(&["catalog", "age", "--dry-run"]),
            ErrorKind::UnknownArgument
        );
        assert!(parse(&["catalog"]).is_err());
        assert_eq!(kind(&["catalog", "prices"]), ErrorKind::InvalidSubcommand);
    }

    #[test]
    fn fab_has_its_defaults_and_takes_a_file_and_an_out_directory() {
        assert_eq!(
            fab(&["a.nsm", "--out", "x"]),
            fab::Args {
                file: Some("a.nsm".into()),
                out: Some("x".into()),
                route_timeout: None,
                allow_drc_violations: false,
                cross_check: false,
                strict: false,
                locked: false,
                constraints: None,
                pins: None,
                no_glb: false,
                save_placed: false,
                order: None,
                quiet: false,
                output: output(),
            }
        );
        assert!(parse(&["fab", "a.nsm", "--port", "9"]).is_err());
        let f = fab(&[
            "--allow-drc-violations",
            "--cross-check",
            "--quiet",
            "--no-glb",
        ]);
        assert!(f.allow_drc_violations && f.cross_check && f.quiet && f.no_glb);
        assert!(parse(&["build", "--cross-check"]).is_err());
        assert!(parse(&["build", "--no-glb"]).is_err());
    }

    #[test]
    fn fab_takes_a_route_timeout_in_whole_seconds() {
        assert_eq!(fab(&["--route-timeout", "30"]).route_timeout, Some(30));
        let e = parse(&["fab", "--route-timeout", "soon"]).unwrap_err();
        assert!(e.contains("--route-timeout") && e.contains("soon"), "{e}");
        assert!(parse(&["fab", "--route-timeout"]).is_err());
        assert!(parse(&["build", "--route-timeout", "3"]).is_err());
    }

    #[test]
    fn fab_order_takes_a_name_or_lists_the_orders_and_no_file() {
        assert_eq!(
            fab(&["--order", "devkit"]).order(),
            Some(fab::OrderRequest::Make("devkit".into()))
        );
        assert_eq!(fab(&["--order"]).order(), Some(fab::OrderRequest::List));
        assert_eq!(fab(&[]).order(), None);
        // A flag after `--order` is not its name.
        let f = fab(&["--order", "--out", "x"]);
        assert_eq!(
            (f.order(), f.out),
            (Some(fab::OrderRequest::List), Some("x".into()))
        );
        let f = fab(&["--out", "x", "--order", "d", "--route-timeout", "9"]);
        assert_eq!(f.order(), Some(fab::OrderRequest::Make("d".into())));
        assert_eq!(f.route_timeout, Some(9));
        assert_eq!(
            kind(&["fab", "a.nsm", "--order", "d"]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(
            kind(&["fab", "--order", "d", "--save-placed"]),
            ErrorKind::ArgumentConflict
        );
        assert!(fab(&["a.nsm", "--save-placed"]).save_placed);
        assert!(parse(&["build", "--order", "d"]).is_err());
    }

    #[test]
    fn build_and_fab_take_strict_and_locked_and_nothing_else_does() {
        let Ok(Command::Build(b)) = parse(&["build", "--strict", "--locked"]) else {
            panic!()
        };
        assert!(b.strict && b.locked);
        let f = fab(&["a.nsm", "--locked", "--strict"]);
        assert!(f.locked && f.strict);
        assert!(!fab(&[]).locked, "a run may change the lock by default");
        assert!(parse(&["editor", "--strict"]).is_err());
        assert!(parse(&["editor", "--locked"]).is_err());
        assert!(parse(&["compare", "a", "b", "--locked"]).is_err());
    }

    #[test]
    fn fab_takes_pins_and_constraints_for_one_run() {
        assert_eq!(fab(&["--pins", "locked"]).pins_locked(), Some(true));
        assert_eq!(fab(&["--pins", "auto"]).pins_locked(), Some(false));
        assert_eq!(fab(&[]).pins_locked(), None, "the manifest decides");
        assert_eq!(
            fab(&["--constraints", "warn"]).constraints_warn(),
            Some(true)
        );
        assert_eq!(
            fab(&["--constraints", "error"]).constraints_warn(),
            Some(false)
        );
        assert_eq!(fab(&[]).constraints_warn(), None, "the manifest decides");
        let e = parse(&["fab", "--pins", "swap"]).unwrap_err();
        assert!(
            e.contains("swap") && e.contains("auto") && e.contains("locked"),
            "{e}"
        );
        let e = parse(&["fab", "--constraints", "off"]).unwrap_err();
        assert!(
            e.contains("off") && e.contains("warn") && e.contains("error"),
            "{e}"
        );
        assert!(parse(&["fab", "--pins"]).is_err());
        assert!(parse(&["build", "--constraints", "warn"]).is_err());
    }

    #[test]
    fn format_and_notes_have_defaults_and_a_set_of_values() {
        let Ok(Command::Check(c)) = parse(&["check", "--format", "json", "--notes", "summary"])
        else {
            panic!()
        };
        assert_eq!(
            (c.output.format, c.output.notes),
            (common::Format::Json, common::Notes::Summary)
        );
        let Ok(Command::Check(c)) = parse(&["check"]) else {
            panic!()
        };
        assert_eq!(
            (c.output.format, c.output.notes),
            (common::Format::Text, common::Notes::All)
        );
        assert_eq!(kind(&["build", "--format", "xml"]), ErrorKind::InvalidValue);
        assert_eq!(kind(&["build", "--notes", "some"]), ErrorKind::InvalidValue);
        assert!(parse(&["editor", "--format", "json"]).is_err());
    }

    #[test]
    fn a_command_takes_one_nsm_file_before_or_after_its_flags() {
        assert_eq!(parse(&["build"]), Ok(Command::Build(build(None))));
        assert_eq!(
            parse(&["build", "a.nsm"]),
            Ok(Command::Build(build(Some("a.nsm"))))
        );
        let expected = Ok(Command::Build(build::Args {
            out: Some("x".into()),
            ..build(Some("a.nsm"))
        }));
        assert_eq!(parse(&["build", "a.nsm", "--out", "x"]), expected);
        assert_eq!(parse(&["build", "--out", "x", "a.nsm"]), expected);
        assert!(parse(&["editor", "a.nsm", "b.nsm"]).is_err());
        assert!(parse(&["init", "dir"]).is_err());
    }

    #[test]
    fn a_flag_given_twice_is_the_last_one() {
        let Ok(Command::Build(b)) =
            parse(&["build", "--out", "a", "--out", "b", "--strict", "--strict"])
        else {
            panic!()
        };
        assert_eq!(b.out, Some("b".into()));
        assert!(b.strict);
        let Ok(Command::Lock(l)) = parse(&["lock", "--check", "--check"]) else {
            panic!()
        };
        assert_eq!(l.mode(), lock::LockMode::Check);
    }

    #[test]
    fn lock_takes_update_or_check_and_not_both() {
        let mode = |args: &[&str]| match parse(args) {
            Ok(Command::Lock(l)) => (l.mode(), l.file),
            other => panic!("{args:?}: {other:?}"),
        };
        assert_eq!(mode(&["lock", "--update"]), (lock::LockMode::Update, None));
        assert_eq!(
            mode(&["lock", "--check", "a.nsm"]),
            (lock::LockMode::Check, Some("a.nsm".into()))
        );
        assert_eq!(kind(&["lock"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(
            kind(&["lock", "--update", "--check"]),
            ErrorKind::ArgumentConflict
        );
    }

    #[test]
    fn the_editor_takes_a_port_with_8787_as_the_default() {
        let port = |args: &[&str]| match parse(args) {
            Ok(Command::Editor(e)) => e.port,
            other => panic!("{args:?}: {other:?}"),
        };
        assert_eq!(port(&["editor", "a.nsm"]), 8787);
        assert_eq!(port(&["editor", "--port", "9000"]), 9000);
        assert!(
            parse(&["editor", "--port", "http"])
                .unwrap_err()
                .contains("http")
        );
        assert!(parse(&["editor", "--port", "70000"]).is_err());
        assert!(parse(&["editor", "--port"]).is_err());
        assert!(parse(&["editor", "--out", "x"]).is_err());
        assert!(parse(&["build", "--port", "1"]).is_err());
    }

    #[test]
    fn route_takes_a_placed_board_a_timeout_and_a_margin() {
        assert_eq!(
            parse(&[
                "route",
                "b.placed.json",
                "--out",
                "o",
                "--route-timeout",
                "0",
                "--margin",
                "2.5"
            ]),
            Ok(Command::Route(route::Args {
                file: "b.placed.json".into(),
                out: Some("o".into()),
                route_timeout: Some(0),
                margin: Some(2.5),
            }))
        );
        assert_eq!(kind(&["route"]), ErrorKind::MissingRequiredArgument);
        for bad in ["0.5", "5", "wide"] {
            let e = parse(&["route", "b.json", "--margin", bad]).unwrap_err();
            assert!(
                e.contains("1 (the process's least track) to 4"),
                "{bad}: {e}"
            );
        }
    }

    #[test]
    fn compare_takes_two_files_and_map_or_by_refdes() {
        assert_eq!(
            parse(&[
                "compare",
                "--board",
                "Led",
                "a.xml",
                "--map",
                "m.txt",
                "b.xml",
                "--ignore-unconnected",
                "--loose-values"
            ]),
            Ok(Command::Compare(compare::Args {
                a: "a.xml".into(),
                b: "b.xml".into(),
                board: Some("Led".into()),
                map: Some("m.txt".into()),
                by_refdes: false,
                ignore_unconnected: true,
                loose_values: true,
                format: common::Format::Text,
            }))
        );
        assert_eq!(
            kind(&["compare", "a.nsm"]),
            ErrorKind::MissingRequiredArgument
        );
        assert_eq!(
            kind(&["compare", "a", "b", "c"]),
            ErrorKind::UnknownArgument
        );
        assert_eq!(
            kind(&["compare", "a", "b", "--map", "m", "--by-refdes"]),
            ErrorKind::ArgumentConflict
        );
        assert!(parse(&["compare", "a", "b", "--map"]).is_err());
        assert!(parse(&["build", "a.nsm", "--loose-values"]).is_err());
    }

    #[test]
    fn parity_takes_the_board_and_its_files() {
        assert!(matches!(
            parse(&["parity", "b.kicad_pcb", "--format", "json"]),
            Ok(Command::Parity(parity::Args {
                table: None,
                map: None,
                svg: None,
                format: common::Format::Json,
                ..
            }))
        ));
        assert_eq!(kind(&["parity"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn init_takes_only_a_name() {
        assert_eq!(
            parse(&["init"]),
            Ok(Command::Init(init::Args { name: None }))
        );
        assert_eq!(
            parse(&["init", "--name", "demo"]),
            Ok(Command::Init(init::Args {
                name: Some("demo".into())
            }))
        );
        assert!(parse(&["init", "a.nsm"]).unwrap_err().contains("a.nsm"));
        assert!(parse(&["init", "--name"]).is_err());
    }

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
        assert_eq!(r.view, netsmith_pcb::raster3d::View::Top);
        assert_eq!(r.size, (64, 80));
        assert_eq!(r.section, Some((netsmith_pcb::mesh::Axis::Z, -0.8)));
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

    fn view_request(args: &[&str]) -> crate::view_client::Request {
        let mut line = vec!["view"];
        line.extend_from_slice(args);
        match parse(&line) {
            Ok(Command::View(v)) => v.request().unwrap(),
            other => panic!("{args:?}: {other:?}"),
        }
    }

    #[test]
    fn view_with_no_flag_reads_the_state_of_the_editor_on_the_default_port() {
        let request = view_request(&[]);
        assert_eq!(request.port, common::DEFAULT_PORT);
        assert!(!request.changes() && !request.json && request.picture.is_none());
        assert_eq!(request.change, netsmith_editor::api::ViewChange::default());
    }

    #[test]
    fn view_takes_the_flags_of_render_and_the_editors_own() {
        use netsmith_editor::api::{ColourBy, PreviewSetting, RenderView};
        let request = view_request(&[
            "--port",
            "9001",
            "--board",
            "Demo/main",
            "--view",
            "iso-bottom",
            "--section",
            "x=12.5",
            "--highlight",
            "J1,net:GND",
            "--highlight",
            "limit:lid",
            "--xray",
            "--perspective",
            "--colour-by",
            "margin",
            "--height-preview",
            "2.5",
            "--layers-apart",
            "--json",
            "--picture",
            "j1.png",
        ]);
        let change = &request.change;
        assert_eq!(request.port, 9001);
        assert_eq!(change.board.as_deref(), Some("Demo/main"));
        assert_eq!(change.view, Some(RenderView::IsoBottom));
        assert_eq!(change.section.as_deref(), Some("x=12.5"));
        assert_eq!(
            change.highlight,
            Some(vec!["J1".into(), "net:GND".into(), "limit:lid".into()])
        );
        assert_eq!(change.xray, Some(true));
        assert_eq!(change.ortho, Some(false));
        assert_eq!(change.colour_by, Some(ColourBy::Margin));
        assert_eq!(change.height_preview, Some(PreviewSetting::Mm(2.5)));
        assert_eq!(change.exploded, Some(true));
        assert!(request.json && request.changes());
        assert_eq!(request.picture, Some(PathBuf::from("j1.png")));
        // The reverse of each switch, and the other spelling of colour.
        let request = view_request(&[
            "--no-xray",
            "--ortho",
            "--no-layers-apart",
            "--color-by",
            "kind",
            "--height-preview",
            "off",
            "--section",
            "none",
            "--select",
            "R1",
        ]);
        let change = &request.change;
        assert_eq!(change.xray, Some(false));
        assert_eq!(change.ortho, Some(true));
        assert_eq!(change.exploded, Some(false));
        assert_eq!(change.colour_by, Some(ColourBy::Kind));
        assert_eq!(change.height_preview, Some(PreviewSetting::On(false)));
        assert_eq!(change.section.as_deref(), Some("none"));
        assert_eq!(change.select, Some(vec!["R1".into()]));
    }

    #[test]
    fn view_switches_and_their_reverses_are_the_last_one_given() {
        let change =
            view_request(&["--xray", "--no-xray", "--no-layers-apart", "--layers-apart"]).change;
        assert_eq!((change.xray, change.exploded), (Some(false), Some(true)));
        assert_eq!(
            view_request(&["--no-xray", "--xray"]).change.xray,
            Some(true)
        );
    }

    #[test]
    fn view_says_none_to_light_or_select_nothing() {
        assert_eq!(
            view_request(&["--highlight", "none"]).change.highlight,
            Some(Vec::new())
        );
        assert_eq!(
            view_request(&["--select", "none"]).change.select,
            Some(Vec::new())
        );
        for args in [
            &["view", "--highlight", "none,J1"][..],
            &["view", "--highlight", "none", "--highlight", "J1"],
            &["view", "--select", "J1", "--select", "none"],
        ] {
            let e = view_err(args);
            assert!(e.contains("clears it"), "{args:?}: {e}");
        }
    }

    #[test]
    fn a_picture_alone_still_changes_the_page_so_that_the_view_is_open() {
        let request = view_request(&["--picture", "a.png"]);
        assert!(request.changes());
        assert_eq!(request.change, netsmith_editor::api::ViewChange::default());
    }

    #[test]
    fn view_mistakes_say_what_is_wrong() {
        for (args, says) in [
            (&["--theme", "dark"][..], "person's own setting"),
            (&["--view", "sideways"], "iso-bottom"),
            (&["--view"], "--view"),
            (&["--section", "w=1"], "--section needs"),
            (&["--section", "x"], "--section needs"),
            (&["--ortho", "--perspective"], "cannot be used with"),
            (
                &["--highlight", "J1", "--select", "J1"],
                "cannot be used with",
            ),
            (&["--select", "limit:lid"], "--highlight"),
            (&["--colour-by", "shade"], "provenance"),
            (&["--height-preview", "0"], "0.1 to 50"),
            (&["--height-preview", "51"], "0.1 to 50"),
            (&["--height-preview", "tall"], "0.1 to 50"),
            (&["--port", "0"], "1..=65535"),
            (&["--port", "http"], "http"),
            (&["--picture"], "--picture"),
            (&["--out", "a.png"], "--picture"),
            (&["--size", "100x100"], "--size"),
            (&["examples/led.nsm"], "takes no file"),
        ] {
            let mut line = vec!["view"];
            line.extend_from_slice(args);
            let e = view_err(&line);
            assert!(e.contains(says), "{args:?}: {e}");
        }
    }
}
