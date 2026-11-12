use netsmith::project::Project;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, time::Duration};

use clap::{Args as ClapArgs, Subcommand};
use netsmith::supply::Supply;

#[derive(Debug, PartialEq, ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum Command {
    /// Check catalog rows against LCSC's record and write the day in `checked` (ADR 0039).
    Refresh(RefreshArgs),
    /// Append a row made from LCSC's record of a number to a catalog, or print the record.
    Add(AddArgs),
    /// List the catalog rows whose price and stock are older than `--days` days, and the unknown ones.
    Age(AgeArgs),
}

#[derive(Debug, Clone, PartialEq, Eq, Default, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`catalog refresh` is the one command that uses the network, and it is opt-in (ADR 0039): nothing else, and never the solver, asks anyone anything. JLCPCB data is not fetched: the command asks LCSC only. For each row with an `lcsc` number it reads LCSC's record of that part and checks the manufacturer's number and the package against the row. A row that agrees is `VERIFIED`, and if the file has a `checked` column it gets the day there: that is the only cell the command ever writes. It never writes `price`, `stock`, `price_breaks`, `library` or `date`: the board is assembled from JLCPCB's parts library, those cells are JLCPCB's numbers entered by hand, and LCSC's price (often at a first break above one piece) and stock differ from them. It may say, as a note and not a finding, that LCSC lists stock 0 where the row has some (or the reverse), so you read JLCPCB's parts page by hand; the age of a row's own numbers is still its `date`, so a refresh never makes a stale row fresh. It keeps the order, quoting and line endings of the file, so the result is a small diff.\n\nWith no path it acts on the project's catalog files (the `*.csv` of its `catalog_dirs` folders and the files under `catalogs`) and, in the netsmith repository, on `data/jlcpcb/catalog/`; a path is a catalog file or a directory of them (the standard catalogs inside the binary cannot be written, so they are checked in the repository and arrive with a release). A row is reported, not changed, when the records disagree with it: `CONFLICT` if the manufacturer's number or the package is not the row's, `MISSING` if LCSC no longer lists the number (a row is never deleted); `SKIPPED` is a row `--offline` has no kept answer for.\n\nLCSC's endpoint is unofficial and LCSC's terms of use have not been read: a person must read them before the command is advertised, and official APIs (JLCAPI, the LCSC API; they need an account) are future work. Meanwhile the command is polite: one request at a time, `--delay` apart, a User-Agent that says what it is, answers kept for `--cache-ttl` and retried with a growing wait.\n\nThe exit code is 0, 1 if any row is `CONFLICT` or `MISSING`, and 2 when the run had to stop (the endpoint refused, or answered in another shape: nothing is written then)."
)]
pub struct RefreshArgs {
    /// Catalog files or folders of them; none is the project's catalogs.
    #[arg(value_name = "CATALOG.CSV|DIR")]
    pub paths: Vec<PathBuf>,
    /// Print the diff, cell by cell, and write nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Ask nothing, and use the kept answers of any age.
    #[arg(long, conflicts_with_all = ["delay_ms", "cache_ttl_hours"])]
    pub offline: bool,
    /// Only this catalog (repeatable).
    #[arg(long, value_name = "CATALOG")]
    pub only: Vec<String>,
    /// Only the row of this LCSC number (repeatable).
    #[arg(long, value_name = "C123")]
    pub lcsc: Vec<String>,
    /// The pause between requests, in milliseconds (default 500).
    #[arg(long = "delay", value_name = "MS")]
    pub delay_ms: Option<u64>,
    /// How long a kept answer is used, in hours (default 24).
    #[arg(long = "cache-ttl", value_name = "HOURS")]
    pub cache_ttl_hours: Option<u64>,
    /// Keep no answer.
    #[arg(long)]
    pub no_cache: bool,
}

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "`catalog add --lcsc <C123> --to <catalog.csv>` asks LCSC's record of the number (never JLCPCB's) and appends a row with `lcsc`, `mpn`, `package` and a `source` that names the record and the day, leaving every other cell empty and listing the columns that need the datasheet or JLCPCB's page; without `--to` it prints the record and writes nothing."
)]
#[command(args_override_self = true)]
pub struct AddArgs {
    /// The LCSC number to add, such as C25104.
    #[arg(long, value_name = "C123")]
    pub lcsc: String,
    /// The catalog file to append the row to; without it the record is printed.
    #[arg(long, value_name = "CATALOG.CSV")]
    pub to: Option<PathBuf>,
    /// Ask nothing, and use a kept answer.
    #[arg(long)]
    pub offline: bool,
    /// Keep no answer.
    #[arg(long)]
    pub no_cache: bool,
}

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "`catalog age` prints the rows whose price and stock were read more than `--days` days ago (the supply provider's answer, else the row's `date`) and, apart, the rows whose price and stock are unknown because no provider has them and the row has no `date` (unknown is not old and not 0); it reads the `[supply]` table of the project that contains the directory."
)]
#[command(args_override_self = true)]
pub struct AgeArgs {
    /// Catalog files or folders of them, as for `refresh`.
    #[arg(value_name = "CATALOG.CSV|DIR")]
    pub paths: Vec<PathBuf>,
    /// The age in days from which a row is old.
    #[arg(long, value_name = "N", default_value_t = netsmith::refresh::STALE_DAYS)]
    pub days: u32,
    /// Only this catalog (repeatable).
    #[arg(long, value_name = "CATALOG")]
    pub only: Vec<String>,
}

pub fn run(args: Args) -> ExitCode {
    match args.command {
        Command::Refresh(a) => run_catalog_refresh(&a),
        Command::Add(a) => run_catalog_add(&a.lcsc, a.to.as_deref(), a.offline, a.no_cache),
        Command::Age(a) => run_catalog_age(&a.paths, &a.only, a.days),
    }
}

/// `netsmith catalog refresh`: the exit code is 1 if a row is a CONFLICT or MISSING, and 2 if
/// the run had to stop (an endpoint refused, a file could not be read) and wrote nothing.
/// `netsmith catalog add` (#454): exit 0 when the row is written (or the record printed), 1
/// when LCSC has no record or the file already has the number, 2 when the run could not be made.
fn run_catalog_add(lcsc: &str, to: Option<&Path>, offline: bool, no_cache: bool) -> ExitCode {
    use netsmith::refresh::{self, AddError, Network};
    let mut network = Network {
        offline,
        ..Network::default()
    };
    if no_cache {
        network.cache_dir = None;
    }
    let shown = to.map(|p| p.display().to_string());
    match refresh::add(lcsc, to.zip(shown.as_deref()), network) {
        Ok(added) => {
            print!("{}", refresh::render_added(&added));
            ExitCode::SUCCESS
        }
        Err(AddError::Refused(why)) => {
            eprintln!("error: {why}");
            ExitCode::FAILURE
        }
        Err(AddError::Stopped(why)) => {
            eprintln!("error: {why}");
            ExitCode::from(2)
        }
    }
}

fn run_catalog_refresh(request: &RefreshArgs) -> ExitCode {
    use netsmith::refresh::{self, Network};
    let fail = |e: String| {
        eprintln!("error: {e}");
        ExitCode::from(2)
    };
    let cwd = match env::current_dir() {
        Ok(d) => d,
        Err(e) => return fail(e.to_string()),
    };
    let targets = match refresh::resolve_targets(&cwd, &request.paths) {
        Ok(t) => t,
        Err(e) => return fail(e),
    };
    let mut network = Network {
        offline: request.offline,
        ..Network::default()
    };
    if let Some(ms) = request.delay_ms {
        network.delay = Duration::from_millis(ms);
    }
    if let Some(hours) = request.cache_ttl_hours {
        network.ttl = Duration::from_secs(hours.saturating_mul(3600));
    }
    if request.no_cache {
        network.cache_dir = None;
    }
    let options = refresh::Options {
        dry_run: request.dry_run,
        only: request.only.clone(),
        lcsc: request.lcsc.clone(),
        network,
    };
    // One line on stderr for each row that is looked up.
    let say = |m: &str| eprintln!("{m}");
    let report = match refresh::refresh(&targets, &options, &say) {
        Ok(r) => r,
        Err(e) => {
            eprint!("{}", refresh::NOTICE);
            return fail(e);
        }
    };
    print!("{}", refresh::render_diff(&report));
    print!("{}", refresh::render_summary(&report));
    if report.has_findings() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn run_catalog_age(paths: &[std::path::PathBuf], only: &[String], days: u32) -> ExitCode {
    use netsmith::refresh;
    let fail = |e: String| {
        eprintln!("error: {e}");
        ExitCode::from(2)
    };
    let cwd = match env::current_dir() {
        Ok(d) => d,
        Err(e) => return fail(e.to_string()),
    };
    let today = refresh::date_of(refresh::now_secs());
    // The supply provider of the project that contains the directory, if there is one.
    let supply =
        match Project::find(&cwd).and_then(|p| p.map_or(Ok(Supply::none()), |p| p.supply())) {
            Ok(supply) => supply,
            Err(e) => return fail(e),
        };
    match refresh::resolve_targets(&cwd, paths)
        .and_then(|t| refresh::age(&t, only, days, &today, &supply))
    {
        Ok(report) => {
            print!("{}", refresh::render_age(&report));
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{kind, parse};
    use crate::commands::{Command, catalog};
    use clap::error::ErrorKind;
    use std::path::PathBuf;

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
}
