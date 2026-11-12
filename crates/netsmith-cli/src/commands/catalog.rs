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
#[command(
    after_long_help = "The one command that uses the network (ADR 0039). JLCPCB is not asked, and no price or stock is written. A row is VERIFIED, CONFLICT (the number or package is not the row's), MISSING (LCSC no longer lists it) or, with --offline, SKIPPED. The exit code is 0, 1 if any row is CONFLICT or MISSING, and 2 when the run had to stop."
)]
#[command(args_override_self = true)]
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
