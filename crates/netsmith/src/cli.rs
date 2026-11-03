//! The command line: `netsmith init`, `build`, `fab`, `editor` and `compare`.

use std::path::PathBuf;

pub const USAGE: &str = "usage: netsmith --version\n       netsmith init [--name <name>]\n       netsmith build [<file.nsm>] [--out <dir>] [--strict]\n       netsmith fab [<file.nsm> | --order [<name>]] [--out <dir>] [--route-timeout <secs>] [--allow-drc-violations] [--cross-check] [--strict] [--constraints warn|error]\n       netsmith editor [<file.nsm>] [--port <n>]\n       netsmith compare <a> <b> [--board <name>] [--map <file> | --by-refdes] [--ignore-unconnected] [--loose-values]\n       netsmith kicad-libs import <sym-lib-table> [<fp-lib-table>]\n       netsmith catalog refresh [<catalog.csv | dir>...] [--dry-run] [--offline] [--only <catalog>] [--lcsc <C123>] [--delay <ms>] [--cache-ttl <hours>] [--no-cache]\n       netsmith catalog age [<catalog.csv | dir>...] [--days <n>] [--only <catalog>]\n       netsmith dxf import <file.dxf> [--layer <name>]... [--holes-layer <name>]... [--units mm|cm|m|inch] [--board <name>] [--write <file.nsm>]\nwith no file, build, fab and editor act on the project's boards (see netsmith.toml); `fab --order <name>` makes one of its orders, and `fab --order` alone lists them; `dxf import` prints the board outline, cutouts and mounting holes of an enclosure's ASCII DXF as source to paste, and with `--write` puts their places in the layout file beside the source it names (ADR 0045); `catalog refresh` checks the manufacturer's number and package of the rows of the project's catalogs (or the standard ones, in the netsmith repository) against LCSC's record (never JLCPCB, so JLCPCB data is not fetched), writes only the day in a `checked` column, never a price or stock, and is never run by another command";

/// The port `netsmith editor` listens on unless told otherwise.
pub const DEFAULT_PORT: u16 = 8787;

/// What `netsmith catalog refresh` is asked to do (ADR 0039).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RefreshRequest {
    /// Catalog files or directories of them; none is the project's catalogs and, in the
    /// netsmith repository, the standard ones.
    pub paths: Vec<PathBuf>,
    pub dry_run: bool,
    pub offline: bool,
    pub only: Vec<String>,
    pub lcsc: Vec<String>,
    /// The pause between requests, in milliseconds, if not the default.
    pub delay_ms: Option<u64>,
    /// How long a kept answer is used, in hours, if not the default.
    pub cache_ttl_hours: Option<u64>,
    pub no_cache: bool,
}

/// What `netsmith fab --order` asks for (ADR 0035).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderRequest {
    /// `--order` with no name: list the project's orders.
    List,
    /// `--order <name>`: make that order.
    Make(String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// `--version`, `-V` or `version`: print the version this binary was built as.
    Version,
    /// Scaffold a project in the current directory, named `name` or after the directory.
    Init { name: Option<String> },
    /// `file` is `None` to act on the project's top-level files; `out` is `None` for the
    /// default directory.
    Build {
        file: Option<String>,
        out: Option<PathBuf>,
        /// `--strict`: a guideline finding stops the build for this run (ADR 0036).
        strict: bool,
    },
    Fab {
        file: Option<String>,
        out: Option<PathBuf>,
        /// Seconds the router may take for a board before `fab` stops and lists the
        /// connections left; `Some(0)` is no limit, `None` the default.
        route_timeout: Option<u64>,
        allow_drc_violations: bool,
        cross_check: bool,
        /// `--strict`: a guideline finding stops the run (ADR 0036).
        strict: bool,
        /// `--constraints warn` or `error` for this run, over the manifest's `constraints`
        /// (ADR 0045): `Some(true)` lists a requirement on where a part goes that the board
        /// does not meet instead of stopping the run, `Some(false)` stops even if the
        /// manifest says `warn`, `None` leaves it to the manifest.
        constraints_warn: Option<bool>,
        /// Make a named order of the project, or list them, instead of every board.
        order: Option<OrderRequest>,
    },
    /// The editor in a browser. `file` is `None` to act on the project's top-level files.
    Editor { file: Option<String>, port: u16 },
    /// Compare two designs by connectivity. Each is a `.kicad_sch`, `.kicad_pcb`, KiCad
    /// netlist XML or `.nsm` file.
    /// Check catalog rows against LCSC's record and record the day in `checked` (ADR 0039).
    /// JLCPCB is not asked, and no price or stock is written.
    CatalogRefresh(RefreshRequest),
    /// List the catalog rows whose date is older than `days` days.
    CatalogAge {
        paths: Vec<PathBuf>,
        only: Vec<String>,
        days: u32,
    },
    /// Print the outline, cutouts and holes of an enclosure's DXF file as source to paste
    /// (ADR 0045).
    DxfImport {
        file: PathBuf,
        layers: Vec<String>,
        holes_layers: Vec<String>,
        units: Option<netsmith_dxf::Units>,
        board: Option<String>,
        /// The source file whose layout file receives the places of the holes and cutouts.
        write: Option<PathBuf>,
    },
    /// Print the `[[library]]` stanzas for KiCad's library tables, to paste into
    /// `kicad.toml`.
    KicadLibsImport {
        sym_lib_table: String,
        fp_lib_table: Option<String>,
    },
    Compare {
        a: String,
        b: String,
        /// Which board of an `.nsm` file, if it has more than one.
        board: Option<String>,
        /// A file of `A_ref B_ref` lines that fixes pairs of designators.
        map: Option<PathBuf>,
        by_refdes: bool,
        ignore_unconnected: bool,
        loose_values: bool,
    },
}

/// Read the arguments after the program name. The error is what to tell the user.
pub fn parse_args<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter().peekable();
    let command = args.next().ok_or_else(|| USAGE.to_string())?;
    if ["--version", "-V", "version"].contains(&command.as_str()) {
        return match args.next() {
            None => Ok(Command::Version),
            Some(extra) => Err(format!(
                "`{command}` takes no argument, not `{extra}`\n{USAGE}"
            )),
        };
    }
    if command == "kicad-libs" {
        let rest: Vec<String> = args.collect();
        return match rest.as_slice() {
            [import, sym] if import == "import" => Ok(Command::KicadLibsImport {
                sym_lib_table: sym.clone(),
                fp_lib_table: None,
            }),
            [import, sym, fp] if import == "import" => Ok(Command::KicadLibsImport {
                sym_lib_table: sym.clone(),
                fp_lib_table: Some(fp.clone()),
            }),
            _ => Err(format!(
                "`kicad-libs` takes `import <sym-lib-table> [<fp-lib-table>]`\n{USAGE}"
            )),
        };
    }
    if command == "catalog" {
        return parse_catalog(args.collect());
    }
    if command == "dxf" {
        return parse_dxf(args.collect());
    }
    if !["init", "build", "fab", "editor", "compare"].contains(&command.as_str()) {
        return Err(format!("unknown command `{command}`\n{USAGE}"));
    }

    let mut files: Vec<String> = Vec::new();
    let mut board: Option<String> = None;
    let mut map: Option<PathBuf> = None;
    let (mut by_refdes, mut ignore_unconnected, mut loose_values) = (false, false, false);
    let mut name: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut route_timeout: Option<u64> = None;
    let (mut allow_drc_violations, mut cross_check, mut strict) = (false, false, false);
    let mut constraints_warn: Option<bool> = None;
    let mut order: Option<OrderRequest> = None;
    let mut port = DEFAULT_PORT;
    while let Some(arg) = args.next() {
        match (command.as_str(), arg.as_str()) {
            ("init", "--name") => {
                name = Some(args.next().ok_or("--name needs a project name")?);
            }
            ("build" | "fab", "--out") => {
                out = Some(PathBuf::from(args.next().ok_or("--out needs a directory")?));
            }
            ("fab", "--order") => {
                order = Some(match args.next_if(|next| !next.starts_with("--")) {
                    Some(name) => OrderRequest::Make(name),
                    None => OrderRequest::List,
                });
            }
            ("fab", "--route-timeout") => {
                let given = args
                    .next()
                    .ok_or("--route-timeout needs a number of seconds")?;
                route_timeout = Some(given.parse().map_err(|_| {
                    format!("--route-timeout needs a whole number of seconds (0 for no limit), not `{given}`")
                })?);
            }
            ("fab", "--allow-drc-violations") => allow_drc_violations = true,
            ("fab", "--cross-check") => cross_check = true,
            ("build" | "fab", "--strict") => strict = true,
            ("fab", "--constraints") => match args.next().as_deref() {
                Some("warn") => constraints_warn = Some(true),
                Some("error") => constraints_warn = Some(false),
                other => {
                    return Err(format!(
                        "--constraints takes `warn` (a requirement on where a part goes that is not met is listed instead of stopping `fab`) or `error` (it stops `fab`, whatever netsmith.toml says), not {}\n{USAGE}",
                        other.map_or("nothing".to_string(), |w| format!("`{w}`"))
                    ));
                }
            },
            ("editor", "--port") => {
                let given = args.next().ok_or("--port needs a number")?;
                port = given
                    .parse()
                    .map_err(|_| format!("--port needs a number from 0 to 65535, not `{given}`"))?;
            }
            ("compare", "--board") => {
                board = Some(args.next().ok_or("--board needs a board name")?);
            }
            ("compare", "--map") => {
                map = Some(PathBuf::from(args.next().ok_or("--map needs a file")?));
            }
            ("compare", "--by-refdes") => by_refdes = true,
            ("compare", "--ignore-unconnected") => ignore_unconnected = true,
            ("compare", "--loose-values") => loose_values = true,
            (_, flag) if flag.starts_with("--") => {
                return Err(format!("`{command}` has no option `{flag}`\n{USAGE}"));
            }
            ("init", extra) => return Err(format!("`init` takes no argument `{extra}`\n{USAGE}")),
            ("compare", path) if files.len() < 2 => files.push(path.to_string()),
            (c, path) if c != "compare" && files.is_empty() => files.push(path.to_string()),
            (_, extra) => return Err(format!("unexpected argument `{extra}`\n{USAGE}")),
        }
    }
    let file = files.first().cloned();
    if order.is_some() && file.is_some() {
        return Err(format!(
            "`fab --order` makes an order of the project and takes no file: an order names its boards in netsmith.toml\n{USAGE}"
        ));
    }
    if command == "compare" {
        let [a, b] = <[String; 2]>::try_from(files)
            .map_err(|_| format!("`compare` takes two files, A and B\n{USAGE}"))?;
        if map.is_some() && by_refdes {
            return Err(format!(
                "--map and --by-refdes both say which designators correspond; give one\n{USAGE}"
            ));
        }
        return Ok(Command::Compare {
            a,
            b,
            board,
            map,
            by_refdes,
            ignore_unconnected,
            loose_values,
        });
    }
    Ok(match command.as_str() {
        "init" => Command::Init { name },
        "build" => Command::Build { file, out, strict },
        "fab" => Command::Fab {
            file,
            out,
            route_timeout,
            allow_drc_violations,
            cross_check,
            strict,
            constraints_warn,
            order,
        },
        _ => Command::Editor { file, port },
    })
}

/// `dxf import`.
fn parse_dxf(args: Vec<String>) -> Result<Command, String> {
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some("import") => {}
        other => {
            return Err(format!(
                "`dxf` takes `import <file.dxf>`, not `{}`\n{USAGE}",
                other.unwrap_or("")
            ));
        }
    }
    let (mut file, mut layers, mut holes_layers) = (None, Vec::new(), Vec::new());
    let (mut units, mut board, mut write) = (None, None, None);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--layer" => layers.push(args.next().ok_or("--layer needs a layer name")?),
            "--holes-layer" => {
                holes_layers.push(args.next().ok_or("--holes-layer needs a layer name")?);
            }
            "--units" => {
                let given = args.next().ok_or("--units needs mm, cm, m or inch")?;
                units =
                    Some(netsmith_dxf::Units::parse(&given).ok_or_else(|| {
                        format!("--units needs mm, cm, m or inch, not `{given}`")
                    })?);
            }
            "--board" => board = Some(args.next().ok_or("--board needs a board name")?),
            "--write" => {
                write = Some(PathBuf::from(
                    args.next().ok_or("--write needs the board's .nsm file")?,
                ));
            }
            flag if flag.starts_with("--") => {
                return Err(format!("`dxf import` has no option `{flag}`\n{USAGE}"));
            }
            path if file.is_none() => file = Some(PathBuf::from(path)),
            extra => return Err(format!("unexpected argument `{extra}`\n{USAGE}")),
        }
    }
    let file = file.ok_or_else(|| format!("`dxf import` takes a .dxf file\n{USAGE}"))?;
    Ok(Command::DxfImport {
        file,
        layers,
        holes_layers,
        units,
        board,
        write,
    })
}

/// `catalog refresh` and `catalog age`.
fn parse_catalog(args: Vec<String>) -> Result<Command, String> {
    let mut args = args.into_iter();
    let sub = args
        .next()
        .ok_or_else(|| format!("`catalog` takes `refresh` or `age`\n{USAGE}"))?;
    if sub != "refresh" && sub != "age" {
        return Err(format!(
            "`catalog` takes `refresh` or `age`, not `{sub}`\n{USAGE}"
        ));
    }
    let mut request = RefreshRequest::default();
    let mut days: Option<u32> = None;
    let number = |args: &mut std::vec::IntoIter<String>, flag: &str, what: &str| {
        let given = args.next().ok_or(format!("{flag} needs {what}"))?;
        given
            .parse::<u64>()
            .map_err(|_| format!("{flag} needs {what}, not `{given}`"))
    };
    while let Some(arg) = args.next() {
        match (sub.as_str(), arg.as_str()) {
            ("refresh", "--dry-run") => request.dry_run = true,
            ("refresh", "--offline") => request.offline = true,
            ("refresh", "--no-cache") => request.no_cache = true,
            (_, "--only") => request
                .only
                .push(args.next().ok_or("--only needs a catalog name")?),
            ("refresh", "--lcsc") => request.lcsc.push(
                args.next()
                    .ok_or("--lcsc needs an LCSC number like C25104")?,
            ),
            ("refresh", "--delay") => {
                request.delay_ms = Some(number(&mut args, "--delay", "a number of milliseconds")?);
            }
            ("refresh", "--cache-ttl") => {
                request.cache_ttl_hours =
                    Some(number(&mut args, "--cache-ttl", "a number of hours")?);
            }
            ("age", "--days") => {
                days = Some(
                    u32::try_from(number(&mut args, "--days", "a whole number of days")?)
                        .map_err(|_| "--days is too large".to_string())?,
                );
            }
            (_, flag) if flag.starts_with("--") => {
                return Err(format!("`catalog {sub}` has no option `{flag}`\n{USAGE}"));
            }
            (_, path) => request.paths.push(PathBuf::from(path)),
        }
    }
    if sub == "age" {
        return Ok(Command::CatalogAge {
            paths: request.paths,
            only: request.only,
            days: days.unwrap_or(crate::refresh::STALE_DAYS),
        });
    }
    if request.offline && (request.delay_ms.is_some() || request.cache_ttl_hours.is_some()) {
        return Err(format!(
            "--offline asks nothing, so --delay and --cache-ttl mean nothing with it\n{USAGE}"
        ));
    }
    Ok(Command::CatalogRefresh(request))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, String> {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn the_version_is_asked_for_three_ways_and_takes_no_argument() {
        for flag in ["--version", "-V", "version"] {
            assert_eq!(parse(&[flag]), Ok(Command::Version));
        }
        let e = parse(&["--version", "build"]).unwrap_err();
        assert!(
            e.contains("`--version` takes no argument, not `build`"),
            "{e}"
        );
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
                "--holes-layer",
                "HOLES",
                "--units",
                "inch",
                "--board",
                "Case",
                "--write",
                "case.nsm"
            ]),
            Ok(Command::DxfImport {
                file: "case.dxf".into(),
                layers: vec!["EDGE".into()],
                holes_layers: vec!["HOLES".into()],
                units: Some(netsmith_dxf::Units::Inches),
                board: Some("Case".into()),
                write: Some("case.nsm".into()),
            })
        );
        let e = parse(&["dxf", "import"]).unwrap_err();
        assert!(e.contains("takes a .dxf file"), "{e}");
        let e = parse(&["dxf", "import", "a.dxf", "--units", "furlongs"]).unwrap_err();
        assert!(e.contains("not `furlongs`"), "{e}");
        assert!(parse(&["dxf", "export", "a.dxf"]).is_err());
        assert!(parse(&["dxf", "import", "a.dxf", "--bogus"]).is_err());
    }

    #[test]
    fn kicad_libs_import_takes_one_or_two_library_tables() {
        assert_eq!(
            parse(&["kicad-libs", "import", "sym-lib-table"]),
            Ok(Command::KicadLibsImport {
                sym_lib_table: "sym-lib-table".into(),
                fp_lib_table: None
            })
        );
        assert_eq!(
            parse(&["kicad-libs", "import", "s", "f"]),
            Ok(Command::KicadLibsImport {
                sym_lib_table: "s".into(),
                fp_lib_table: Some("f".into())
            })
        );
        let e = parse(&["kicad-libs"]).unwrap_err();
        assert!(e.contains("`kicad-libs` takes `import"), "{e}");
        assert!(parse(&["kicad-libs", "export", "s"]).is_err());
    }

    #[test]
    fn catalog_refresh_takes_paths_and_the_flags_of_a_polite_opt_in_run() {
        assert_eq!(
            parse(&["catalog", "refresh"]),
            Ok(Command::CatalogRefresh(RefreshRequest::default()))
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
            Ok(Command::CatalogRefresh(RefreshRequest {
                paths: vec!["catalogs/a.csv".into(), "data".into()],
                dry_run: true,
                offline: false,
                only: vec!["ldo".into(), "led".into()],
                lcsc: vec!["C5446".into()],
                delay_ms: Some(1500),
                cache_ttl_hours: Some(6),
                no_cache: true,
            }))
        );
        let Ok(Command::CatalogRefresh(r)) = parse(&["catalog", "refresh", "--offline"]) else {
            panic!()
        };
        assert!(r.offline);
        let e = parse(&["catalog", "refresh", "--delay", "soon"]).unwrap_err();
        assert!(
            e.contains("--delay needs a number of milliseconds, not `soon`"),
            "{e}"
        );
        let e = parse(&["catalog", "refresh", "--offline", "--delay", "9"]).unwrap_err();
        assert!(e.contains("--offline asks nothing"), "{e}");
        let e = parse(&["catalog", "refresh", "--days", "3"]).unwrap_err();
        assert!(
            e.contains("`catalog refresh` has no option `--days`"),
            "{e}"
        );
        assert!(parse(&["catalog", "refresh", "--lcsc"]).is_err());
    }

    #[test]
    fn catalog_age_takes_paths_a_catalog_and_the_days_with_90_as_the_default() {
        assert_eq!(
            parse(&["catalog", "age"]),
            Ok(Command::CatalogAge {
                paths: vec![],
                only: vec![],
                days: 90
            })
        );
        assert_eq!(
            parse(&["catalog", "age", "x.csv", "--days", "30", "--only", "ldo"]),
            Ok(Command::CatalogAge {
                paths: vec!["x.csv".into()],
                only: vec!["ldo".into()],
                days: 30
            })
        );
        let e = parse(&["catalog", "age", "--dry-run"]).unwrap_err();
        assert!(e.contains("`catalog age` has no option `--dry-run`"), "{e}");
        let e = parse(&["catalog"]).unwrap_err();
        assert!(e.contains("`catalog` takes `refresh` or `age`"), "{e}");
        let e = parse(&["catalog", "prices"]).unwrap_err();
        assert!(e.contains("not `prices`"), "{e}");
    }

    #[test]
    fn fab_takes_a_file_and_an_out_directory_like_build() {
        assert_eq!(
            parse(&["fab", "a.nsm", "--out", "x"]),
            Ok(Command::Fab {
                file: Some("a.nsm".into()),
                out: Some("x".into()),
                route_timeout: None,
                allow_drc_violations: false,
                cross_check: false,
                strict: false,
                constraints_warn: None,
                order: None,
            })
        );
        assert!(parse(&["fab", "a.nsm", "--port", "9"]).is_err());
    }

    #[test]
    fn fab_takes_the_flags_that_waive_the_design_rules_check_and_that_cross_check_with_kicad() {
        assert_eq!(
            parse(&["fab", "--allow-drc-violations", "--cross-check"]),
            Ok(Command::Fab {
                file: None,
                out: None,
                route_timeout: None,
                allow_drc_violations: true,
                cross_check: true,
                strict: false,
                constraints_warn: None,
                order: None,
            })
        );
        assert!(parse(&["build", "--cross-check"]).is_err());
    }

    #[test]
    fn fab_takes_a_route_timeout_in_whole_seconds() {
        assert_eq!(
            parse(&["fab", "--route-timeout", "30"]),
            Ok(Command::Fab {
                file: None,
                out: None,
                route_timeout: Some(30),
                allow_drc_violations: false,
                cross_check: false,
                strict: false,
                constraints_warn: None,
                order: None,
            })
        );
        let e = parse(&["fab", "--route-timeout", "soon"]).unwrap_err();
        assert!(
            e.contains("whole number of seconds") && e.contains("`soon`"),
            "{e}"
        );
        assert!(parse(&["fab", "--route-timeout"]).is_err());
        assert!(parse(&["build", "--route-timeout", "3"]).is_err());
    }

    #[test]
    fn fab_order_takes_a_name_or_lists_the_orders() {
        let fab = |order| {
            Ok(Command::Fab {
                file: None,
                out: None,
                route_timeout: None,
                allow_drc_violations: false,
                cross_check: false,
                strict: false,
                constraints_warn: None,
                order: Some(order),
            })
        };
        assert_eq!(
            parse(&["fab", "--order", "devkit"]),
            fab(OrderRequest::Make("devkit".into()))
        );
        assert_eq!(parse(&["fab", "--order"]), fab(OrderRequest::List));
        assert_eq!(
            parse(&["fab", "--order", "--out", "x"]),
            Ok(Command::Fab {
                file: None,
                out: Some("x".into()),
                route_timeout: None,
                allow_drc_violations: false,
                cross_check: false,
                strict: false,
                constraints_warn: None,
                order: Some(OrderRequest::List),
            })
        );
        assert_eq!(
            parse(&[
                "fab",
                "--out",
                "x",
                "--order",
                "d",
                "--route-timeout",
                "9",
                "--allow-drc-violations"
            ]),
            Ok(Command::Fab {
                file: None,
                out: Some("x".into()),
                route_timeout: Some(9),
                allow_drc_violations: true,
                cross_check: false,
                strict: false,
                constraints_warn: None,
                order: Some(OrderRequest::Make("d".into())),
            })
        );
        let e = parse(&["fab", "a.nsm", "--order", "d"]).unwrap_err();
        assert!(e.contains("takes no file") && e.contains("usage:"), "{e}");
        assert!(parse(&["build", "--order", "d"]).is_err());
    }

    #[test]
    fn build_and_fab_take_strict_for_one_run() {
        assert_eq!(
            parse(&["build", "--strict"]),
            Ok(Command::Build {
                file: None,
                out: None,
                strict: true,
            })
        );
        let Ok(Command::Fab { strict, .. }) = parse(&["fab", "a.nsm", "--strict"]) else {
            panic!("fab takes --strict");
        };
        assert!(strict);
        assert!(parse(&["editor", "--strict"]).is_err());
    }

    #[test]
    fn fab_takes_constraints_warn_for_one_run() {
        let Ok(Command::Fab {
            constraints_warn,
            strict,
            ..
        }) = parse(&["fab", "a.nsm", "--constraints", "warn"])
        else {
            panic!("fab takes --constraints warn");
        };
        assert!(constraints_warn == Some(true) && !strict);
        let Ok(Command::Fab {
            constraints_warn, ..
        }) = parse(&["fab", "a.nsm", "--constraints", "error"])
        else {
            panic!("fab takes --constraints error");
        };
        assert_eq!(constraints_warn, Some(false));
        let Ok(Command::Fab {
            constraints_warn, ..
        }) = parse(&["fab", "a.nsm"])
        else {
            panic!("fab");
        };
        assert_eq!(constraints_warn, None, "the manifest decides by default");
        let e = parse(&["fab", "--constraints", "off"]).unwrap_err();
        assert!(
            e.contains("takes `warn`") && e.contains("or `error`") && e.contains("`off`"),
            "{e}"
        );
        assert!(parse(&["fab", "--constraints"]).is_err());
        assert!(parse(&["build", "--constraints", "warn"]).is_err());
    }

    #[test]
    fn build_leaves_the_out_directory_to_the_target() {
        assert_eq!(
            parse(&["build", "a.nsm"]),
            Ok(Command::Build {
                file: Some("a.nsm".into()),
                out: None,
                strict: false,
            })
        );
    }

    #[test]
    fn build_takes_an_out_directory_before_or_after_the_file() {
        let expected = Ok(Command::Build {
            file: Some("a.nsm".into()),
            out: Some("x".into()),
            strict: false,
        });
        assert_eq!(parse(&["build", "a.nsm", "--out", "x"]), expected);
        assert_eq!(parse(&["build", "--out", "x", "a.nsm"]), expected);
    }

    #[test]
    fn the_editor_defaults_to_port_8787() {
        assert_eq!(
            parse(&["editor", "a.nsm"]),
            Ok(Command::Editor {
                file: Some("a.nsm".into()),
                port: 8787
            })
        );
    }

    #[test]
    fn the_editor_takes_a_port() {
        assert_eq!(
            parse(&["editor", "a.nsm", "--port", "9000"]),
            Ok(Command::Editor {
                file: Some("a.nsm".into()),
                port: 9000
            })
        );
    }

    #[test]
    fn a_bad_port_says_what_was_given() {
        let e = parse(&["editor", "a.nsm", "--port", "http"]).unwrap_err();
        assert!(e.contains("`http`"), "{e}");
        assert!(parse(&["editor", "a.nsm", "--port", "70000"]).is_err());
        assert!(parse(&["editor", "a.nsm", "--port"]).is_err());
    }

    #[test]
    fn options_belong_to_their_command() {
        assert!(parse(&["build", "a.nsm", "--port", "1"]).is_err());
        assert!(parse(&["editor", "a.nsm", "--out", "x"]).is_err());
    }

    #[test]
    fn mistakes_show_the_usage() {
        for args in [
            &[][..],
            &["frobnicate"],
            &["init", "dir"],
            &["init", "--out", "x"],
            &["editor", "a.nsm", "b.nsm"],
        ] {
            let e = parse(args).unwrap_err();
            assert!(e.contains("usage:"), "{args:?}: {e}");
        }
    }

    #[test]
    fn with_no_file_a_command_acts_on_the_project() {
        assert_eq!(
            parse(&["build"]),
            Ok(Command::Build {
                file: None,
                out: None,
                strict: false,
            })
        );
        assert_eq!(
            parse(&["fab", "--out", "x"]),
            Ok(Command::Fab {
                file: None,
                out: Some("x".into()),
                route_timeout: None,
                allow_drc_violations: false,
                cross_check: false,
                strict: false,
                constraints_warn: None,
                order: None,
            })
        );
        assert_eq!(
            parse(&["editor", "--port", "9"]),
            Ok(Command::Editor {
                file: None,
                port: 9
            })
        );
    }

    #[test]
    fn compare_takes_two_files_and_its_options() {
        assert_eq!(
            parse(&["compare", "a.kicad_sch", "b.nsm"]),
            Ok(Command::Compare {
                a: "a.kicad_sch".into(),
                b: "b.nsm".into(),
                board: None,
                map: None,
                by_refdes: false,
                ignore_unconnected: false,
                loose_values: false,
            })
        );
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
            Ok(Command::Compare {
                a: "a.xml".into(),
                b: "b.xml".into(),
                board: Some("Led".into()),
                map: Some("m.txt".into()),
                by_refdes: false,
                ignore_unconnected: true,
                loose_values: true,
            })
        );
    }

    #[test]
    fn compare_mistakes_show_the_usage() {
        for args in [
            &["compare"][..],
            &["compare", "a.nsm"],
            &["compare", "a", "b", "c"],
            &["compare", "a", "b", "--map"],
            &["compare", "a", "b", "--board"],
            &["compare", "a", "b", "--map", "m", "--by-refdes"],
            &["compare", "a", "b", "--port", "1"],
            &["build", "a.nsm", "--loose-values"],
        ] {
            let e = parse(args).unwrap_err();
            assert!(e.contains("usage:") || e.contains("needs"), "{args:?}: {e}");
        }
        let e = parse(&["compare", "a", "b", "--map", "m", "--by-refdes"]).unwrap_err();
        assert!(e.contains("--map") && e.contains("--by-refdes"), "{e}");
        let e = parse(&["compare", "a.nsm"]).unwrap_err();
        assert!(e.contains("two files"), "{e}");
    }

    #[test]
    fn init_takes_only_a_name() {
        assert_eq!(parse(&["init"]), Ok(Command::Init { name: None }));
        assert_eq!(
            parse(&["init", "--name", "demo"]),
            Ok(Command::Init {
                name: Some("demo".into())
            })
        );
        assert!(parse(&["init", "a.nsm"]).unwrap_err().contains("`a.nsm`"));
        assert!(parse(&["init", "--name"]).is_err());
    }
}
